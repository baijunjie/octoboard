//! Replacing a file whole, so that a reader sees the old content or the new and never part of
//! either, and a crash mid-write leaves the old file in place; and removing such a file. What
//! permissions the file gets is the platform layer's ([`Permissions`]).

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::platform::fs as platform;

pub use platform::Privacy as Permissions;

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
    let written = (|| {
        let mut file = platform::create_replacement(staged, target, permissions)?;
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

/// Removes the file at `target`, as a file replaced whole is removed once nothing is left to keep
/// in it. Nothing there is no error. Blocking.
pub fn remove_replaced(target: &Path) -> io::Result<()> {
    match std::fs::remove_file(target) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
        _ => Ok(()),
    }
}

fn staged_beside(target: &Path) -> Option<PathBuf> {
    let name = target.file_name()?.to_string_lossy();
    Some(target.with_file_name(format!("{}{}", staged_prefix(&name), std::process::id())))
}

/// What the name of a file staged beside the file `name` starts with, the staging process's pid
/// following it.
fn staged_prefix(name: &str) -> String {
    format!(".{name}.octoboard-")
}

/// Removes what a process that was cut short left staged beside `target` by
/// [`replace_beside`]: a file whose replacement was written but never renamed over it, which
/// nothing else removes and which would otherwise stay in a directory of the user's. Only a
/// regular file named exactly as [`replace_beside`] names one is looked at, and it is removed
/// only when it was last written longer ago than a replacement can take ([`STAGED_FOR_AT_MOST`])
/// and the process that staged it is gone: no running process holds the pid in its name,
/// or the one that does started well after the file was last written, so the pid has changed
/// hands.
/// This process's own is never touched, and neither is one of a process that still runs, which
/// may be another daemon replacing the file now. The age comes first because pids are this
/// host's alone: a daemon of the same user on another host that shares the directory, or in
/// another pid namespace, stages under a pid nobody holds here, and removing its file in the
/// middle of its replacement would fail its rename. Answers how many were removed; a directory that
/// cannot be listed has none. Blocking.
pub fn remove_stale_staged(target: &Path) -> usize {
    let (Some(dir), Some(name)) = (target.parent(), target.file_name()) else {
        return 0;
    };
    let prefix = staged_prefix(&name.to_string_lossy());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|staged| staged.strip_prefix(&prefix))
            .filter(|pid| !pid.is_empty() && pid.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|pid| pid.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if pid == std::process::id()
            || !metadata.is_file()
            || !older_than_a_replace_takes(&metadata)
            || !staged_by_a_gone_process(pid, &metadata)
        {
            continue;
        }
        match std::fs::remove_file(entry.path()) {
            Ok(()) => removed += 1,
            Err(err) => {
                tracing::warn!(path = %entry.path().display(), %err, "removing a file a cut replacement left staged failed")
            }
        }
    }
    removed
}

/// How long a file may stay staged while its replacement is still under way. A replacement
/// writes the file whole (the largest rewritten so is bounded at 64 MiB), flushes it to the disk
/// and renames it, with nothing waited for in between: seconds on a local disk, and still well
/// under this on a slow network file system. A staged file older than this was left by a
/// process that was cut off.
const STAGED_FOR_AT_MOST: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// Whether the file of `metadata` was last written longer ago than a replacement takes. One
/// whose time of writing cannot be read, or lies ahead of the clock, is not.
fn older_than_a_replace_takes(metadata: &std::fs::Metadata) -> bool {
    metadata
        .modified()
        .ok()
        .and_then(|written| written.elapsed().ok())
        .is_some_and(|age| age > STAGED_FOR_AT_MOST)
}

/// Whether the process `pid` that staged the file of `metadata` is gone. A file whose time of
/// writing cannot be read is not judged by it: only a pid nobody holds makes it stale.
fn staged_by_a_gone_process(pid: u32, metadata: &std::fs::Metadata) -> bool {
    let Some(holder_started) = crate::platform::proc::ProcessId(pid).holder_started_at_ms() else {
        return true;
    };
    metadata
        .modified()
        .ok()
        .and_then(|written| written.duration_since(std::time::UNIX_EPOCH).ok())
        .is_some_and(|written| {
            (written.as_millis() as i64) + STAGED_BEFORE_SLACK_MS < holder_started
        })
}

/// How long before a process started a staged file under its pid has to have been written for
/// the pid to count as having changed hands: more than any file system's coarsest time of
/// writing and any step of the clock between the two readings.
const STAGED_BEFORE_SLACK_MS: i64 = 60_000;

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::test_support::ScratchDir;

    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    #[cfg(unix)]
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

    /// A pid no process holds.
    const NO_SUCH_PID: u32 = 0x7fff_fff0;

    /// Makes `path` look last written an hour ago, longer than any replacement takes.
    fn aged(path: &Path) {
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(3600))
            .unwrap();
    }

    /// A staged file under a pid nobody holds here stays while it is fresh: it may be a
    /// replacement under way by a daemon on another host, or in another pid namespace, that
    /// shares the directory. Once it is older than a replacement takes, it goes.
    #[test]
    fn a_fresh_staged_file_stays_whatever_its_pid() {
        let dir = ScratchDir::new("replace-stale-staged-fresh");
        let target = dir.join("registry.json");
        let staged = dir.join(format!(".registry.json.octoboard-{NO_SUCH_PID}"));
        std::fs::write(&staged, "being written elsewhere").unwrap();
        assert_eq!(remove_stale_staged(&target), 0);
        assert!(staged.exists());

        // Just inside the bound.
        let file = std::fs::File::options().write(true).open(&staged).unwrap();
        file.set_modified(
            std::time::SystemTime::now() - STAGED_FOR_AT_MOST + std::time::Duration::from_secs(60),
        )
        .unwrap();
        drop(file);
        assert_eq!(remove_stale_staged(&target), 0);

        aged(&staged);
        assert_eq!(remove_stale_staged(&target), 1);
        assert!(!staged.exists());
    }

    /// What a cut replacement left staged is removed when its process is gone, and only then:
    /// this process's own, a running process's and anything not named exactly as a staged file
    /// stay.
    #[test]
    fn only_a_gone_processs_staged_file_is_removed() {
        let dir = ScratchDir::new("replace-stale-staged");
        let target = dir.join("registry.json");
        std::fs::write(&target, "kept").unwrap();
        let staged = |pid: &str| dir.join(format!(".registry.json.octoboard-{pid}"));
        let own = staged(&std::process::id().to_string());
        let stale = staged(&NO_SUCH_PID.to_string());
        let kept = [
            own.clone(),
            staged(""),
            staged("12x"),
            staged("-5"),
            staged(&format!("{NO_SUCH_PID}.bak")),
            dir.join(format!("registry.json.octoboard-{NO_SUCH_PID}")),
            dir.join(format!(".other.json.octoboard-{NO_SUCH_PID}")),
            dir.join(format!(".registry.json.octoboard{NO_SUCH_PID}")),
        ];
        for path in kept.iter().chain([&stale]) {
            std::fs::write(path, "staged").unwrap();
            aged(path);
        }
        // A directory of that very name is not a staged file.
        let directory = staged(&(NO_SUCH_PID - 1).to_string());
        std::fs::create_dir(&directory).unwrap();

        assert_eq!(remove_stale_staged(&target), 1);
        assert!(!stale.exists());
        for path in kept.iter().chain([&directory, &target]) {
            assert!(path.exists(), "{}", path.display());
        }
        assert_eq!(remove_stale_staged(&target), 0);
        assert_eq!(remove_stale_staged(Path::new("/")), 0);
    }

    /// A staged file under the pid of a process that still runs is that process's, which may be
    /// replacing the file now; one written before the process holding its pid started was staged
    /// by another process, long gone.
    #[test]
    fn a_running_processs_staged_file_stays_unless_the_pid_changed_hands() {
        let dir = ScratchDir::new("replace-stale-staged-running");
        let target = dir.join("registry.json");
        let mut other = crate::test_support::programs::sleep(30)
            .command()
            .spawn()
            .expect("a process that stays up");
        let staged = dir.join(format!(".registry.json.octoboard-{}", other.id()));
        std::fs::write(&staged, "staged").unwrap();
        assert_eq!(remove_stale_staged(&target), 0);
        assert!(staged.exists());

        // Written long before that process started: its pid was another process's then.
        let file = std::fs::File::options().write(true).open(&staged).unwrap();
        file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(86_400))
            .unwrap();
        drop(file);
        assert_eq!(remove_stale_staged(&target), 1);
        assert!(!staged.exists());
        other.kill().ok();
        other.wait().ok();
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
    #[cfg(unix)]
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

    #[cfg(unix)]
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
