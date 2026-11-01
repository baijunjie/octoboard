//! Replacing a file whole, so that a reader sees the old content or the new and never part of
//! either, and a crash mid-write leaves the old file in place.

use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// The permissions [`replace_file`] gives the file it writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permissions {
    /// The owner's alone (`0o600`), whatever the file had: for content that can hold secrets.
    Private,
    /// The permissions of the file being replaced, and the owner's alone for a new one.
    KeptOrPrivate,
    /// The permissions of the file being replaced, and for a new one what the process's umask
    /// leaves of `0o666`, as for any file a program creates. Unlike the others, a failure to set
    /// the kept mode past the umask is ignored: a file that cannot be given it is still written.
    KeptOrDefault,
}

/// Replaces the file at `target` with `bytes`: written whole to `staged` with the permissions
/// `permissions` says, flushed to the disk, then renamed over `target`. Without the flush a crash
/// soon after the rename could leave `target` empty or cut short on a file system that orders the
/// rename before the data. `staged` must lie in `target`'s directory, so the rename stays on one
/// file system, and is removed when any step fails; the caller keeps two writes through one
/// `staged` from running at once. Blocking.
pub fn replace_file(
    target: &Path,
    staged: &Path,
    bytes: &[u8],
    permissions: Permissions,
) -> io::Result<()> {
    let kept = || {
        std::fs::metadata(target)
            .ok()
            .map(|metadata| metadata.permissions().mode() & 0o7777)
    };
    // The mode an open applies is cut by the umask and ignored for a file already there, so a mode
    // that must hold exactly is set again once the file is open.
    let (create_mode, exact_mode) = match permissions {
        Permissions::Private => (0o600, Some(0o600)),
        Permissions::KeptOrPrivate => {
            let mode = kept().unwrap_or(0o600);
            (mode, Some(mode))
        }
        Permissions::KeptOrDefault => match kept() {
            Some(mode) => (mode, Some(mode)),
            None => (0o666, None),
        },
    };
    let written = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(create_mode)
            .open(staged)?;
        if let Some(mode) = exact_mode {
            let set = file.set_permissions(std::fs::Permissions::from_mode(mode));
            if permissions != Permissions::KeptOrDefault {
                set?;
            }
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(staged, target)
    })();
    if written.is_err() {
        std::fs::remove_file(staged).ok();
    }
    written
}

/// [`replace_file`] staging under a name beside `target`: hidden, so no listing takes it for a
/// file of its own, and carrying the daemon's pid, so two daemons never stage under one name. An
/// error for a path with no file name.
pub fn replace_beside(target: &Path, bytes: &[u8], permissions: Permissions) -> io::Result<()> {
    let staged = staged_beside(target)
        .ok_or_else(|| io::Error::other(format!("`{}` has no file name", target.display())))?;
    replace_file(target, &staged, bytes, permissions)
}

fn staged_beside(target: &Path) -> Option<PathBuf> {
    let name = target.file_name()?.to_string_lossy();
    Some(target.with_file_name(format!(".{name}.octoboard-{}", std::process::id())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    fn mode_of(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    fn set_mode(path: &Path, mode: u32) {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn the_file_is_replaced_and_nothing_is_left_beside_it() {
        let dir = ScratchDir::new("replace-file");
        let target = dir.join("file");
        std::fs::write(&target, "old").unwrap();
        let staged = staged_beside(&target).unwrap();
        replace_file(&target, &staged, b"new", Permissions::KeptOrDefault).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(dir.read_dir().unwrap().count(), 1);
    }

    #[test]
    fn a_file_is_replaced_beside_itself_and_a_path_with_no_name_is_refused() {
        let dir = ScratchDir::new("replace-beside");
        let target = dir.join("file");
        replace_beside(&target, b"new", Permissions::Private).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(dir.read_dir().unwrap().count(), 1);
        replace_beside(Path::new("/"), b"new", Permissions::Private).unwrap_err();
    }

    /// The owner's alone holds over the mode the file had, and over a staged file a crash left
    /// behind with a looser one, which the open alone would keep.
    #[test]
    fn a_private_file_is_the_owners_alone_whatever_was_there() {
        let dir = ScratchDir::new("replace-file-private");
        let target = dir.join("file");
        std::fs::write(&target, "old").unwrap();
        set_mode(&target, 0o644);
        let staged = dir.join(".partial");
        std::fs::write(&staged, "left by a crash").unwrap();
        set_mode(&staged, 0o666);
        replace_file(&target, &staged, b"new", Permissions::Private).unwrap();
        assert_eq!(mode_of(&target), 0o600);
    }

    #[test]
    fn a_kept_mode_is_the_replaced_files_and_a_new_file_gets_its_default() {
        let dir = ScratchDir::new("replace-file-kept");
        // What a program creating a file gets under this process's umask.
        let default = dir.join("default");
        std::fs::write(&default, "").unwrap();
        for (permissions, new_mode) in [
            (Permissions::KeptOrPrivate, 0o600),
            (Permissions::KeptOrDefault, mode_of(&default)),
        ] {
            let existing = dir.join(format!("existing-{permissions:?}"));
            std::fs::write(&existing, "old").unwrap();
            set_mode(&existing, 0o640);
            let staged = staged_beside(&existing).unwrap();
            replace_file(&existing, &staged, b"new", permissions).unwrap();
            assert_eq!(mode_of(&existing), 0o640, "{permissions:?}");

            let created = dir.join(format!("created-{permissions:?}"));
            let staged = staged_beside(&created).unwrap();
            replace_file(&created, &staged, b"new", permissions).unwrap();
            assert_eq!(mode_of(&created), new_mode, "{permissions:?}");
        }
    }

    /// A rename that fails, onto a directory here, leaves the target as it was and removes what was
    /// staged.
    #[test]
    fn a_failed_replace_leaves_the_target_and_removes_the_staged_file() {
        let dir = ScratchDir::new("replace-file-failed");
        let target = dir.join("directory");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("inside"), "kept").unwrap();
        let staged = staged_beside(&target).unwrap();
        replace_file(&target, &staged, b"new", Permissions::Private).unwrap_err();
        assert_eq!(
            std::fs::read_to_string(target.join("inside")).unwrap(),
            "kept"
        );
        assert!(std::fs::symlink_metadata(&staged).is_err());
    }
}
