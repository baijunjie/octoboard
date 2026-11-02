//! Small file system steps shared by the account code and its coordinator callers. Each answers
//! what the file system said and leaves deciding what a failure means to its caller.

use std::io;
use std::path::Path;
use std::time::SystemTime;

/// When the file or directory `path` leads to was last modified, links followed: the epoch when
/// that cannot be read, so that it sorts before anything whose time can.
pub(crate) fn modified(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Removes the entry at `path` itself, a directory with everything in it, never following a link:
/// a link is removed as a link. Nothing there is no error.
pub(crate) fn remove_any(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// Whether the two spellings resolve, links followed, to one file or directory. A path that cannot
/// be resolved (nothing there, unreadable) matches nothing. Blocking.
pub(crate) fn is_same_path(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}
