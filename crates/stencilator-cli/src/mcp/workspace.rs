//! The folder chosen when the server is started, not by the tools.

use std::ffi::OsStr;
use std::fs;
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Component, Path, PathBuf};

use cap_fs_ext::MetadataExt;
use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};
#[cfg(unix)]
use cap_std::fs::{OpenOptionsExt, Permissions, PermissionsExt};

pub(super) const MAX_INPUT_BYTES: u64 = 128 * 1024 * 1024;

pub(super) struct Workspace {
    path: PathBuf,
    root: Dir,
    #[cfg(test)]
    pub(super) pause_before_publish: Option<Box<dyn Fn() + Send + Sync>>,
}

pub(super) struct Destination<'a> {
    path: &'a Path,
    parent: PathBuf,
    name: &'a OsStr,
    directory: Dir,
}

impl Workspace {
    pub(super) fn new(path: &Path) -> Result<Self, String> {
        let failed = |error| format!("Workspace {}: {error}", path.display());
        let root = Dir::open_ambient_dir(path, ambient_authority()).map_err(failed)?;
        #[cfg(unix)]
        let path = fs::canonicalize(path).map_err(failed)?;
        #[cfg(windows)]
        let path = directory_path(&root).map_err(failed)?;
        Ok(Self {
            path,
            root,
            #[cfg(test)]
            pause_before_publish: None,
        })
    }

    fn resolve(&self, path: &Path) -> Result<PathBuf, String> {
        // Resolve absolute links to locations within the workspace too. The
        // result is only a relative name; actual access uses the open root.
        let resolved =
            fs::canonicalize(self.path.join(path)).map_err(|error| format!("{}: {error}", path.display()))?;
        let inside = resolved
            .strip_prefix(&self.path)
            .map_err(|_| format!("{} points outside the workspace.", path.display()))?;
        if inside.as_os_str().is_empty() {
            return Ok(PathBuf::from("."));
        }
        Ok(inside.to_owned())
    }

    pub(super) fn read(&self, path: &Path) -> Result<Vec<u8>, String> {
        file_name(path)?;
        let resolved = self.resolve(path)?;
        let failed = |error| format!("{}: {error}", path.display());
        let regular_file = || format!("{} must be a regular file.", path.display());
        if !self.root.metadata(&resolved).map_err(failed)?.is_file() {
            return Err(regular_file());
        }

        let mut options = OpenOptions::new();
        options.read(true);
        // A file replaced with a FIFO between the metadata check and open must
        // not block the server before it can check the actual file handle.
        #[cfg(unix)]
        options.custom_flags(rustix::fs::OFlags::NONBLOCK.bits().cast_signed());
        let file = self.root.open_with(resolved, &options).map_err(failed)?;
        let metadata = file.metadata().map_err(failed)?;
        if !metadata.is_file() {
            return Err(regular_file());
        }
        let too_large = || format!("{} is larger than the 128 MiB input limit.", path.display());
        if metadata.len() > MAX_INPUT_BYTES {
            return Err(too_large());
        }
        let mut bytes = Vec::new();
        file.take(MAX_INPUT_BYTES + 1).read_to_end(&mut bytes).map_err(failed)?;
        if bytes.len() as u64 > MAX_INPUT_BYTES {
            return Err(too_large());
        }
        Ok(bytes)
    }

    pub(super) fn destination<'a>(&self, path: &'a Path) -> Result<Destination<'a>, String> {
        let name = file_name(path)?;
        let parent_path = path.parent().unwrap_or_else(|| Path::new(""));
        let folder_error =
            |error| format!("Choose an existing folder in the workspace for {}: {error}", path.display());
        let parent = self.resolve(parent_path).map_err(folder_error)?;
        let directory = self.root.open_dir(&parent).map_err(|error| folder_error(error.to_string()))?;
        match directory.symlink_metadata(name) {
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Destination { path, parent, name, directory }),
            Err(error) => Err(format!("{}: {error}", path.display())),
            Ok(_) => Err(format!("{} already exists. Choose a new file name.", path.display())),
        }
    }

    /// Publishes a complete file without overwriting an existing directory entry, even
    /// if someone else creates it after the destination check.
    pub(super) fn write(
        &self,
        destination: &Destination<'_>,
        bytes: &[u8],
        before_publish: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        self.confirm_destination(destination)?;
        let publish = || {
            self.confirm_destination(destination)?;
            #[cfg(test)]
            if let Some(pause) = &self.pause_before_publish {
                pause();
            }
            before_publish()
        };
        let failed = |error: io::Error| match error.kind() {
            ErrorKind::AlreadyExists => {
                format!("{} already exists. Choose a new file name.", destination.path.display())
            }
            _ => format!("Could not write {}: {error}", destination.path.display()),
        };
        #[cfg(unix)]
        {
            let staged = cap_tempfile::TempDir::new_in(&destination.directory).map_err(failed)?;
            // Keep incomplete data private without losing group inheritance in
            // shared folders. The completed file can follow the umask.
            let group_inheritance = staged.dir_metadata().map_err(failed)?.permissions().mode() & 0o2000;
            staged.set_permissions(".", Permissions::from_mode(0o700 | group_inheritance)).map_err(failed)?;
            let mut file =
                staged.open_with("export", OpenOptions::new().write(true).create_new(true)).map_err(failed)?;
            file.write_all(bytes).and_then(|()| file.sync_all()).map_err(failed)?;
            publish()?;
            install_new(&staged, &destination.directory, destination.name).map_err(failed)?;
        }
        #[cfg(windows)]
        {
            // Derive the path from the checked handle. cap-std keeps it open
            // without FILE_SHARE_DELETE through the native no-replace move.
            let parent = directory_path(&destination.directory).map_err(failed)?;
            let mut staged = tempfile::NamedTempFile::new_in(&parent).map_err(failed)?;
            staged.write_all(bytes).and_then(|()| staged.as_file().sync_all()).map_err(failed)?;
            publish()?;
            staged.persist_noclobber(parent.join(destination.name)).map_err(|error| failed(error.error))?;
        }
        Ok(())
    }

    fn confirm_destination(&self, destination: &Destination<'_>) -> Result<(), String> {
        let current = self.destination(destination.path)?;
        let failed = |error| format!("{}: {error}", destination.path.display());
        let expected = destination.directory.dir_metadata().map_err(failed)?;
        let found = current.directory.dir_metadata().map_err(failed)?;
        if current.parent != destination.parent || (found.dev(), found.ino()) != (expected.dev(), expected.ino()) {
            return Err(format!("The folder for {} changed during the build. Try again.", destination.path.display()));
        }
        Ok(())
    }
}

#[cfg(unix)]
fn install_new(staged: &Dir, destination: &Dir, name: &OsStr) -> io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "redox", target_vendor = "apple"))]
    {
        use rustix::fs::{RenameFlags, renameat_with};
        use rustix::io::Errno;

        match renameat_with(staged, "export", destination, name, RenameFlags::NOREPLACE) {
            Ok(()) => return Ok(()),
            Err(Errno::NOSYS | Errno::INVAL | Errno::OPNOTSUPP) => {}
            #[cfg(target_vendor = "apple")]
            Err(Errno::NOTSUP) => {}
            Err(error) => return Err(error.into()),
        }
    }
    staged.hard_link("export", destination, name)
}

#[cfg(windows)]
fn directory_path(directory: &Dir) -> io::Result<PathBuf> {
    winx::file::get_file_path(&directory.try_clone()?.into_std_file())
}

fn file_name(path: &Path) -> Result<&OsStr, String> {
    // Path components hide trailing separators and "/.", but the request must name a file.
    path.file_name()
        .filter(|name| path.as_os_str().as_encoded_bytes().ends_with(name.as_encoded_bytes()))
        .filter(|_| {
            path.components().all(|part| match part {
                Component::CurDir => true,
                Component::Normal(name) => !name.to_string_lossy().contains(':'),
                _ => false,
            })
        })
        .ok_or_else(|| format!("Use a file path relative to the workspace, without '..': {}.", path.display()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "a failed setup should stop the test")]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_changed_export_folder_is_refused() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        let link = root.path().join("exports");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        symlink(&first, &link).unwrap();
        let workspace = Workspace::new(root.path()).unwrap();
        let path = Path::new("exports/set.zip");
        let destination = workspace.destination(path).unwrap();

        fs::remove_file(&link).unwrap();
        symlink(&second, &link).unwrap();
        assert!(workspace.write(&destination, b"not written", || Ok(())).is_err());
        assert!(fs::read_dir(first).unwrap().next().is_none());
        assert!(fs::read_dir(second).unwrap().next().is_none());
    }

    #[test]
    fn a_file_created_during_export_is_preserved() {
        let root = tempfile::tempdir().unwrap();
        let path = Path::new("set.zip");
        let workspace = Workspace::new(root.path()).unwrap();
        let destination = workspace.destination(path).unwrap();

        let output = root.path().join(path);
        let result = workspace.write(&destination, b"our file", || {
            fs::write(&output, b"their file").unwrap();
            Ok(())
        });
        assert!(result.unwrap_err().contains("already exists"));
        assert_eq!(fs::read(output).unwrap(), b"their file");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_cancelled_export_leaves_no_file_or_staging_entry() {
        let root = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(root.path()).unwrap();
        let destination = workspace.destination(Path::new("set.zip")).unwrap();
        let result = workspace.write(&destination, b"finished bytes", || Err("Cancelled".to_owned()));
        assert_eq!(result.unwrap_err(), "Cancelled");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
}
