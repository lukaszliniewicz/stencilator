//! Writing a set of files all or nothing.
//!
//! Every file is first written beside its destination under a temporary name. Only
//! when all of them are on disk do they move into place. Before a file is replaced, the
//! old one gets a second name (a hard link, or a copy where links are not possible), so
//! the original never disappears, and if a later move fails, every file replaced so far
//! comes back. Temporary names carry the process id, so they cannot clash with the
//! user's own files.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use crate::sources::Input;

fn sibling(path: &Path, tag: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".stencilator-{}.{tag}", std::process::id()));
    path.with_file_name(name)
}

/// The named directory entry: the real parent folder and the file name in lower case,
/// so differently cased names cannot collide on a case-insensitive file system.
fn identity(path: &Path) -> PathBuf {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let parent = fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
    parent.join(path.file_name().unwrap_or_default().to_string_lossy().to_lowercase())
}

/// Protect both a symbolic link's directory entry and the file it resolves to.
fn identities(path: &Path) -> impl Iterator<Item = PathBuf> {
    let named = identity(path);
    let target = fs::canonicalize(path).ok().map(|real| identity(&real)).filter(|real| *real != named);
    std::iter::once(named).chain(target)
}

/// Refuses two outputs with the same destination, and an output that would replace the
/// input, except a project saved over the project it came from. Each output is named for
/// the message, such as `--chart`.
pub(crate) fn check_destinations(outputs: &[(String, PathBuf)], input: &Input) -> Result<(), String> {
    let mut seen: BTreeMap<PathBuf, &str> = BTreeMap::new();
    let project = matches!(input, Input::Project(_));
    let input_name = if project { "the input project" } else { "the input file" };
    if let Some(path) = input.path() {
        seen.extend(identities(path).map(|key| (key, input_name)));
    }
    for (what, path) in outputs {
        for key in identities(path) {
            if let Some(other) = seen.insert(key, what) {
                if project && what == "--save" && other == input_name {
                    continue;
                }
                return Err(format!("{what} and {other} would both be {}; choose different names", path.display()));
            }
        }
    }
    Ok(())
}

/// Creates a folder and any missing parents, and returns those it created, deepest first.
pub(crate) fn create_dirs(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let missing: Vec<PathBuf> =
        dir.ancestors().take_while(|p| !p.as_os_str().is_empty() && !p.exists()).map(Path::to_path_buf).collect();
    fs::create_dir_all(dir).map_err(|e| {
        remove_dirs(&missing);
        format!("{}: {e}", dir.display())
    })?;
    Ok(missing)
}

/// Removes folders made by [`create_dirs`], if they are still empty.
pub(crate) fn remove_dirs(dirs: &[PathBuf]) {
    for dir in dirs {
        let _ = fs::remove_dir(dir);
    }
}

/// Writes every `(path, bytes)` pair, or none of them.
pub(crate) fn write_all(files: &[(PathBuf, Vec<u8>)]) -> Result<(), String> {
    let mut temporary: Vec<PathBuf> = Vec::new();
    let staged = files.iter().try_for_each(|(path, bytes)| {
        let part = sibling(path, "part");
        temporary.push(part.clone());
        fs::write(&part, bytes).map_err(|e| format!("{}: {e}", path.display()))
    });
    let result = staged.and_then(|()| replace_all(files.iter().map(|(p, _)| p).zip(&temporary)));
    if result.is_err() {
        for part in &temporary {
            let _ = fs::remove_file(part);
        }
    }
    result
}

/// Moves each temporary file over its final name. On any failure, puts back what was
/// there before and removes what was new.
fn replace_all<'a>(pairs: impl Iterator<Item = (&'a PathBuf, &'a PathBuf)>) -> Result<(), String> {
    // (final path, second name of the file it replaced, if there was one)
    let mut placed: Vec<(&PathBuf, Option<PathBuf>)> = Vec::new();
    let mut failure = None;
    for (final_path, part) in pairs {
        let backup = final_path.exists().then(|| sibling(final_path, "old"));
        if let Some(backup) = &backup {
            let kept = fs::hard_link(final_path, backup).or_else(|_| fs::copy(final_path, backup).map(|_| ()));
            if let Err(e) = kept {
                failure = Some(format!("{}: {e}", final_path.display()));
                break;
            }
        }
        if let Err(e) = fs::rename(part, final_path) {
            if let Some(backup) = &backup {
                let _ = fs::remove_file(backup);
            }
            failure = Some(format!("{}: {e}", final_path.display()));
            break;
        }
        placed.push((final_path, backup));
    }
    let Some(mut message) = failure else {
        for backup in placed.iter().filter_map(|(_, b)| b.as_ref()) {
            let _ = fs::remove_file(backup);
        }
        return Ok(());
    };
    for (final_path, backup) in placed.iter().rev() {
        match backup {
            Some(backup) => {
                if fs::rename(backup, final_path).is_err() {
                    let _ = write!(message, "; the previous {} is kept as {}", final_path.display(), backup.display());
                }
            }
            None => {
                let _ = fs::remove_file(final_path);
            }
        }
    }
    Err(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stencilator-files-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> =
            fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn writes_every_file_and_replaces_old_ones() {
        let dir = scratch("ok");
        fs::write(dir.join("a.txt"), b"old").unwrap();
        fs::write(dir.join("a.txt.bak"), b"mine").unwrap();
        write_all(&[(dir.join("a.txt"), b"new".to_vec()), (dir.join("b.txt"), b"b".to_vec())]).unwrap();
        assert_eq!(fs::read(dir.join("a.txt")).unwrap(), b"new");
        assert_eq!(fs::read(dir.join("b.txt")).unwrap(), b"b");
        assert_eq!(fs::read(dir.join("a.txt.bak")).unwrap(), b"mine");
        assert_eq!(names(&dir), ["a.txt", "a.txt.bak", "b.txt"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failure_leaves_everything_as_it_was() {
        let dir = scratch("fail");
        fs::write(dir.join("a.txt"), b"old").unwrap();
        let missing = dir.join("no such folder").join("b.txt");
        assert!(write_all(&[(dir.join("a.txt"), b"new".to_vec()), (missing, b"b".to_vec())]).is_err());
        assert_eq!(fs::read(dir.join("a.txt")).unwrap(), b"old");
        assert_eq!(names(&dir), ["a.txt"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_move_restores_earlier_files() {
        let dir = scratch("move");
        fs::write(dir.join("a.txt"), b"old a").unwrap();
        // A folder where the second file should go makes its final move fail.
        fs::create_dir(dir.join("b.txt")).unwrap();
        fs::write(dir.join("b.txt").join("inside"), b"x").unwrap();
        let result = write_all(&[(dir.join("a.txt"), b"new a".to_vec()), (dir.join("b.txt"), b"new b".to_vec())]);
        assert!(result.is_err());
        assert_eq!(fs::read(dir.join("a.txt")).unwrap(), b"old a");
        assert_eq!(names(&dir), ["a.txt", "b.txt"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn clashing_destinations_are_refused() {
        let dir = scratch("clash");
        let path = dir.join("photo.jpg");
        fs::write(&path, b"jpeg").unwrap();
        let input = Input::File(path.clone());
        let output = |flag: &str, path: PathBuf| (flag.to_owned(), path);
        let chart = output("--chart", dir.join("out.png"));
        let tiles = output("--tiles", dir.join("OUT.png"));
        assert_eq!(
            check_destinations(&[chart.clone(), tiles], &input).unwrap_err(),
            format!("--tiles and --chart would both be {}; choose different names", dir.join("OUT.png").display())
        );
        assert!(check_destinations(&[output("--chart", path.clone())], &input).is_err());
        assert!(check_destinations(&[output("--chart", dir.join(".").join("photo.jpg"))], &input).is_err());
        assert!(check_destinations(&[output("--save", path.clone())], &input).is_err());
        assert!(check_destinations(&[chart], &input).is_ok());
        // A project may be saved over itself, and nothing else may replace it.
        let project = Input::Project(dir.join("art.stencilator"));
        assert!(check_destinations(&[output("--save", dir.join("art.stencilator"))], &project).is_ok());
        assert_eq!(
            check_destinations(&[output("--3mf", dir.join("art.stencilator"))], &project).unwrap_err(),
            format!(
                "--3mf and the input project would both be {}; choose different names",
                dir.join("art.stencilator").display()
            )
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_input_and_its_target_are_protected() {
        let dir = scratch("symlink");
        let source = dir.join("source.stencilator");
        let alias = dir.join("alias.stencilator");
        let input = dir.join("input.stencilator");
        fs::write(&source, b"project").unwrap();
        std::os::unix::fs::symlink(&source, &alias).unwrap();
        std::os::unix::fs::symlink(&alias, &input).unwrap();
        let project = Input::Project(input.clone());
        for path in [input, alias, source.clone()] {
            let result = check_destinations(&[("--3mf".to_owned(), path.clone())], &project);
            assert!(result.is_err(), "{} must be protected", path.display());
        }
        assert!(check_destinations(&[("--save".to_owned(), source)], &project).is_ok());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_created_folders_are_removed() {
        let dir = scratch("dirs");
        let created = create_dirs(&dir.join("a").join("b")).unwrap();
        assert_eq!(created, [dir.join("a").join("b"), dir.join("a")]);
        remove_dirs(&created);
        assert!(!dir.join("a").exists() && dir.exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
