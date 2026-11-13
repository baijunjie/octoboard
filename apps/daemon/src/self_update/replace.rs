//! Putting a new executable where the running one is, and taking it back.
//!
//! The new file is written beside the old one under a name of its own (made new, never over what
//! is at that name), with the old one's permissions, and flushed; then the old file is kept as
//! `<name>.previous` (for `octoboardd update --rollback`) and the new one renamed over the path,
//! and the directory flushed. A process running the old file keeps its inode, so nothing
//! running is disturbed, and each rename is atomic: a path is never seen half written, and never
//! without a file, since the old file is kept by a link, not by a rename away from the path, and
//! the one rename at the path is the new file's.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use crate::platform::fs;

/// Why a file could not be replaced or put back.
#[derive(Debug, PartialEq, Eq)]
pub enum ReplaceError {
    /// The directory the file lives in is not one the daemon's user may change.
    NotWritable(PathBuf),
    /// The file `fill` was to write could not be completed: its own error.
    Filling(String),
    /// A step on the disk failed; the file at the path is as it was.
    Failed(String),
}

/// What the previous executable is kept as: the file's name and `.previous`, in its directory.
pub fn previous_path(target: &Path) -> Option<PathBuf> {
    let name = target.file_name()?.to_string_lossy();
    Some(target.with_file_name(format!("{name}.previous")))
}

/// A name beside `target` for a file of this operation: hidden, and carrying the process's id, so
/// that two processes never write one.
fn beside(target: &Path, purpose: &str) -> Option<PathBuf> {
    let name = target.file_name()?.to_string_lossy();
    Some(target.with_file_name(format!(".{name}.{purpose}-{}", std::process::id())))
}

fn failed(what: &str, err: io::Error) -> ReplaceError {
    ReplaceError::Failed(format!("{what}: {err}"))
}

/// Replaces the file `target` with what `fill` writes, keeping the old one as `<name>.previous`.
/// `target` is a file, not a link (the caller resolves it), in a directory the user may write to.
/// Blocking.
pub fn replace(
    target: &Path,
    fill: impl FnOnce(&mut File) -> Result<(), String>,
) -> Result<(), ReplaceError> {
    let (Some(dir), Some(staged), Some(previous)) = (
        target.parent(),
        beside(target, "update"),
        previous_path(target),
    ) else {
        return Err(ReplaceError::Failed(format!(
            "`{}` has no directory or file name",
            target.display()
        )));
    };
    if !fs::directory_writable(dir) {
        return Err(ReplaceError::NotWritable(target.to_path_buf()));
    }
    sweep_leftovers(target);
    // A file or link left at the name (by an earlier run of a process with this id, or planted) is
    // removed, and the new file made where nothing is, never through a link.
    match std::fs::remove_file(&staged) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => {
            return Err(failed("an earlier new file could not be removed", err));
        }
        _ => {}
    }
    let written = (|| {
        let mut file = fs::create_new_like(&staged, target)
            .map_err(|err| failed("the new file could not be created", err))?;
        fill(&mut file).map_err(ReplaceError::Filling)?;
        file.sync_all()
            .map_err(|err| failed("the new file could not be written to the disk", err))
    })();
    if let Err(err) = written {
        std::fs::remove_file(&staged).ok();
        return Err(err);
    }
    // The old file is kept under a second name first (a link to it, else a copy) and that name
    // renamed over `<name>.previous`; then the new file is renamed over the path in one step. The
    // path is never without a file, whatever stops the process between the steps.
    let Some(kept) = beside(target, "prev") else {
        std::fs::remove_file(&staged).ok();
        return Err(ReplaceError::Failed(
            "no name to keep the old file by".to_owned(),
        ));
    };
    let kept_as_previous = keep_beside(target, &kept)
        .map_err(|err| failed("the old file could not be kept", err))
        .and_then(|()| {
            std::fs::rename(&kept, &previous)
                .map_err(|err| failed("the old file could not be kept as the previous one", err))
        });
    if let Err(err) = kept_as_previous {
        std::fs::remove_file(&kept).ok();
        std::fs::remove_file(&staged).ok();
        return Err(err);
    }
    if let Err(err) = std::fs::rename(&staged, target) {
        std::fs::remove_file(&staged).ok();
        return Err(failed("the new file could not be put in place", err));
    }
    // The rename is done and visible; a directory that cannot be flushed only means a crash
    // soon after could undo it.
    if let Err(err) = fs::sync_directory(dir) {
        tracing::warn!(%err, dir = %dir.display(), "flushing the directory after the replacement failed");
    }
    Ok(())
}

/// Puts `<name>.previous` back at `target` and keeps what was there as `<name>.previous`, so that
/// a rollback can be rolled back. What is there is first kept under a second name (a link to it,
/// else a copy), then the previous file is renamed over the path in one step, then the second
/// name becomes `<name>.previous`: the path is never without a file. Blocking.
pub fn swap_with_previous(target: &Path) -> Result<(), ReplaceError> {
    let (Some(dir), Some(aside), Some(previous)) = (
        target.parent(),
        beside(target, "rollback"),
        previous_path(target),
    ) else {
        return Err(ReplaceError::Failed(format!(
            "`{}` has no directory or file name",
            target.display()
        )));
    };
    if !fs::directory_writable(dir) {
        return Err(ReplaceError::NotWritable(target.to_path_buf()));
    }
    sweep_leftovers(target);
    if !previous.is_file() {
        return Err(ReplaceError::Failed(format!(
            "there is no `{}` to go back to",
            previous.display()
        )));
    }
    keep_beside(target, &aside).map_err(|err| failed("the current file could not be kept", err))?;
    if let Err(err) = std::fs::rename(&previous, target) {
        std::fs::remove_file(&aside).ok();
        return Err(failed("the previous file could not be put back", err));
    }
    // The file that was current is what is kept now; if it cannot be, it stays beside as it is.
    if let Err(err) = std::fs::rename(&aside, &previous) {
        tracing::warn!(%err, file = %aside.display(), "the file that was replaced could not be kept as the previous one");
    }
    if let Err(err) = fs::sync_directory(dir) {
        tracing::warn!(%err, dir = %dir.display(), "flushing the directory after the rollback failed");
    }
    Ok(())
}

/// Makes `kept` another name for `target`'s content, a hard link or, where the file system makes
/// none, a copy; a file already at `kept` is replaced.
fn keep_beside(target: &Path, kept: &Path) -> io::Result<()> {
    match std::fs::remove_file(kept) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err),
        _ => {}
    }
    std::fs::hard_link(target, kept).or_else(|_| copy_new(target, kept))
}

/// Copies `target` to `kept`, which is made new, never through a link, with the target's
/// permissions.
fn copy_new(target: &Path, kept: &Path) -> io::Result<()> {
    let mut copy = fs::create_new_like(kept, target)?;
    io::copy(&mut File::open(target)?, &mut copy)?;
    copy.sync_all()
}

/// [`copy_new`], for the test of the fallback no file system of the tests' makes necessary.
#[cfg(test)]
pub(super) fn keep_copy_for_test(target: &Path, kept: &Path) -> io::Result<()> {
    copy_new(target, kept)
}

/// Removes what an earlier run left beside `target`: the files and links named `.<name>.update-<n>`,
/// `.<name>.prev-<n>` and `.<name>.rollback-<n>` (see [`beside`]), whatever process `<n>` was. Called
/// with the update lock held, so none is another update's in use. What cannot be removed stays.
fn sweep_leftovers(target: &Path) {
    let (Some(dir), Some(name)) = (target.parent(), target.file_name()) else {
        return;
    };
    let prefix = format!(".{}.", name.to_string_lossy());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let leftover = file_name.strip_prefix(&prefix).is_some_and(|rest| {
            ["update-", "prev-", "rollback-"].iter().any(|purpose| {
                rest.strip_prefix(purpose)
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|byte| byte.is_ascii_digit()))
            })
        });
        // Looked at without following: a link is removed itself, a directory never.
        let is_directory = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if leftover && !is_directory {
            std::fs::remove_file(entry.path()).ok();
        }
    }
}
