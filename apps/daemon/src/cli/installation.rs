//! The path the daemon launches its own successor, and `start` its daemon, from: the installation
//! entry point the daemon was invoked by, with its final symlink left unresolved. The running image
//! (`proc::running_image`) is what the operating system resolved that to when the process began,
//! so a symlink repointed at a newer build since, or a file replaced at the same pathname, would
//! still be launched as the old image if that were used.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::platform::{fs, proc};

/// The path to start this daemon's executable from, taken at start-up, before the working
/// directory could change. The path it was invoked by, where that is absolute, or made absolute,
/// or found through `PATH`, and names this very image; otherwise the running image's own path,
/// which is what a process started through a wrapper that sets its own `argv[0]` gets.
pub(crate) fn launch_path() -> Result<PathBuf> {
    let image = proc::running_image().context("finding this executable")?;
    let invoked = std::env::args_os().next().and_then(|argv0| {
        let cwd = std::env::current_dir().ok()?;
        invocation_path(&argv0, &cwd, std::env::var_os("PATH").as_deref())
    });
    Ok(invoked
        .filter(|path| names_image(path, &image))
        .unwrap_or(image))
}

/// The absolute path `argv0` names, final symlink unresolved: an absolute one as it stands, a
/// relative one (it has a separator) joined to `cwd`, a bare name looked up in `search_path` as a
/// shell does, the first executable file it finds. `None` when there is nothing to go by.
fn invocation_path(argv0: &OsStr, cwd: &Path, search_path: Option<&OsStr>) -> Option<PathBuf> {
    if argv0.is_empty() {
        return None;
    }
    let path = Path::new(argv0);
    if path.is_absolute() {
        return Some(path.to_path_buf());
    }
    if argv0.to_string_lossy().contains(std::path::is_separator) {
        return Some(cwd.join(path));
    }
    std::env::split_paths(search_path?)
        // An empty entry stands for the working directory.
        .map(|dir| cwd.join(dir).join(path))
        .find(|candidate| fs::is_executable(candidate))
}

/// Whether `path` leads to the same file as the running image `image`, symlinks followed (see
/// `fs::same_file`).
fn names_image(path: &Path, image: &Path) -> bool {
    fs::same_file(path, image)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::test_support::ScratchDir;

    #[test]
    fn an_absolute_path_is_kept_as_it_is() {
        let path = invocation_path(
            OsStr::new("/opt/octoboard/current/octoboardd"),
            Path::new("/work"),
            None,
        );
        assert_eq!(
            path,
            Some(PathBuf::from("/opt/octoboard/current/octoboardd"))
        );
    }

    #[test]
    fn a_relative_path_is_made_absolute_against_the_working_directory() {
        let cwd = Path::new("/work");
        assert_eq!(
            invocation_path(OsStr::new("./bin/octoboardd"), cwd, None),
            Some(PathBuf::from("/work/./bin/octoboardd"))
        );
        assert_eq!(
            invocation_path(OsStr::new("bin/octoboardd"), cwd, None),
            Some(PathBuf::from("/work/bin/octoboardd"))
        );
    }

    #[test]
    fn nothing_is_made_of_an_empty_name() {
        assert_eq!(
            invocation_path(OsStr::new(""), Path::new("/work"), None),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_bare_name_is_found_through_the_search_path_with_its_symlink_left_alone() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let dir = ScratchDir::new("installation-path");
        let (first, second) = (dir.join("first"), dir.join("second"));
        for sub in [&first, &second] {
            std::fs::create_dir_all(sub).unwrap();
        }
        let release = dir.join("release-b");
        std::fs::write(&release, "image").unwrap();
        std::fs::set_permissions(&release, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Not executable: skipped.
        std::fs::write(first.join("octoboardd"), "no").unwrap();
        std::fs::set_permissions(
            first.join("octoboardd"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        symlink(&release, second.join("octoboardd")).unwrap();

        let search = std::env::join_paths([&first, &second]).unwrap();
        let found = invocation_path(
            OsStr::new("octoboardd"),
            Path::new("/elsewhere"),
            Some(search.as_os_str()),
        );
        assert_eq!(found, Some(second.join("octoboardd")));
        assert_eq!(
            invocation_path(
                OsStr::new("missing"),
                Path::new("/elsewhere"),
                Some(search.as_os_str())
            ),
            None
        );
        assert_eq!(
            invocation_path(OsStr::new("octoboardd"), Path::new("/elsewhere"), None),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_relative_search_path_entry_is_taken_from_the_working_directory() {
        use std::os::unix::fs::PermissionsExt;

        let dir = ScratchDir::new("installation-relative-path");
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        let exe = dir.join("bin").join("octoboardd");
        std::fs::write(&exe, "image").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();

        let found = invocation_path(OsStr::new("octoboardd"), &dir, Some(OsStr::new("bin")));
        assert_eq!(found, Some(exe));
    }

    /// The launch path is the installation entry point, not the image it leads to: it names the
    /// new image once the link is repointed, and still names a file replaced at the same pathname.
    #[cfg(unix)]
    #[test]
    fn the_entry_point_is_the_image_while_it_leads_to_it() {
        use std::os::unix::fs::symlink;

        let dir = ScratchDir::new("installation-image");
        let (a, b) = (dir.join("a"), dir.join("b"));
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();
        let entry = dir.join("octoboardd");
        symlink(&a, &entry).unwrap();

        assert!(names_image(&entry, &a));
        assert!(!names_image(&entry, &b));
        std::fs::remove_file(&entry).unwrap();
        symlink(&b, &entry).unwrap();
        assert!(names_image(&entry, &b));
        // An entry point that is gone names nothing.
        std::fs::remove_file(&entry).unwrap();
        assert!(!names_image(&entry, &b));
    }
}
