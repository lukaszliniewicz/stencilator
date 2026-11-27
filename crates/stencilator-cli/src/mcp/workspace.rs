//! The folder chosen when the server is started, not by the tools.

use std::fs::{self, File};
use std::io::{ErrorKind, Read, Write};
use std::path::{Component, Path, PathBuf};

pub(super) const MAX_INPUT_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone)]
pub(super) struct Workspace(PathBuf);

impl Workspace {
    pub(super) fn new(path: &Path) -> Result<Self, String> {
        let root = fs::canonicalize(path).map_err(|error| format!("Workspace {}: {error}", path.display()))?;
        if !root.is_dir() {
            return Err(format!("The workspace must be a folder: {}.", path.display()));
        }
        Ok(Self(root))
    }

    fn resolve(&self, path: &Path) -> Result<PathBuf, String> {
        let resolved = fs::canonicalize(self.0.join(path)).map_err(|error| format!("{}: {error}", path.display()))?;
        if !resolved.starts_with(&self.0) {
            return Err(format!("{} points outside the workspace.", path.display()));
        }
        Ok(resolved)
    }

    pub(super) fn read(&self, path: &Path) -> Result<Vec<u8>, String> {
        relative(path)?;
        let resolved = self.resolve(path)?;
        if !resolved.is_file() {
            return Err(format!("{} must be a regular file.", path.display()));
        }
        let error = |error| format!("{}: {error}", path.display());
        let file = File::open(resolved).map_err(error)?;
        let mut bytes = Vec::new();
        file.take(MAX_INPUT_BYTES + 1).read_to_end(&mut bytes).map_err(error)?;
        if bytes.len() as u64 > MAX_INPUT_BYTES {
            return Err(format!("{} is larger than the 128 MiB input limit.", path.display()));
        }
        Ok(bytes)
    }

    pub(super) fn destination(&self, path: &Path) -> Result<PathBuf, String> {
        relative(path)?;
        let parent = self.resolve(path.parent().unwrap_or_else(|| Path::new("")))?;
        if !parent.is_dir() {
            return Err(format!("Choose an existing folder for {}.", path.display()));
        }
        let destination = parent.join(path.file_name().ok_or("Give a new file name.")?);
        match destination.symlink_metadata() {
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(destination),
            Err(error) => Err(format!("{}: {error}", path.display())),
            Ok(_) => Err(format!("{} already exists. Choose a new file name.", path.display())),
        }
    }

    /// Publishes a complete file without overwriting an existing directory entry, even
    /// if someone else creates it after the destination check.
    pub(super) fn write(&self, path: &Path, destination: &Path, bytes: &[u8]) -> Result<(), String> {
        if self.destination(path)? != destination {
            return Err(format!("The folder for {} changed during the build. Try again.", path.display()));
        }
        let parent = destination.parent().ok_or("Give a new file name.")?;
        let failed = |error| format!("Could not write {}: {error}", path.display());
        let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(failed)?;
        staged.write_all(bytes).map_err(failed)?;
        staged.as_file().sync_all().map_err(failed)?;
        staged.persist_noclobber(destination).map_err(|error| {
            if error.error.kind() == ErrorKind::AlreadyExists {
                format!("{} already exists. Choose a new file name.", path.display())
            } else {
                failed(error.error)
            }
        })?;
        Ok(())
    }
}

fn relative(path: &Path) -> Result<(), String> {
    // Path components hide trailing separators and "/.", but the request must name a file.
    let ends_in_file_name =
        path.file_name().is_some_and(|name| path.as_os_str().as_encoded_bytes().ends_with(name.as_encoded_bytes()));
    let safe = ends_in_file_name
        && path.components().all(|part| match part {
            Component::CurDir => true,
            Component::Normal(name) => !name.to_string_lossy().contains(':'),
            _ => false,
        });
    if !safe {
        return Err(format!("Use a file path relative to the workspace, without '..': {}.", path.display()));
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    #[allow(clippy::unwrap_used, reason = "a failed setup should stop the test")]
    fn a_changed_export_folder_is_refused() {
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
        assert!(workspace.write(path, &destination, b"not written").is_err());
        assert!(fs::read_dir(first).unwrap().next().is_none());
        assert!(fs::read_dir(second).unwrap().next().is_none());
    }
}
