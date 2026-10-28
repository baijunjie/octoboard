//! Bounded reads of the configuration files an adapter looks at but does not own.

use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use crate::browse::live;
use crate::browse::wire_path::RelPath;

/// The most bytes read from a small configuration file (a JSON settings file, a TOML config).
pub(super) const SMALL_FILE_LIMIT: usize = 1024 * 1024;

/// The most bytes read from a configuration file that grows with use, such as Claude Code's global
/// config, which holds an entry for every project.
pub(super) const LARGE_FILE_LIMIT: usize = 64 * 1024 * 1024;

/// The text of a configuration file, read within `limit` bytes. Inside `project` (canonical) it
/// goes through `browse::live`, which refuses a link out of the project, a FIFO and the like;
/// anywhere else (a directory above the project, the home directory) the target must be a regular
/// file, opened without blocking so that a FIFO cannot hold the launch.
pub(super) fn read_bounded_text(
    path: &Path,
    project: &Path,
    limit: usize,
) -> std::io::Result<String> {
    use std::io::{Error, ErrorKind, Read};
    use std::os::unix::fs::OpenOptionsExt;

    let too_large = || Error::new(ErrorKind::InvalidData, "file too large");
    let bytes = match path
        .strip_prefix(project)
        .ok()
        .and_then(|rel| RelPath::from_bytes(rel.as_os_str().as_bytes().to_vec()))
    {
        Some(rel) => {
            // Absent is told from the metadata, since `live` reports it only as a failure.
            std::fs::symlink_metadata(path)?;
            live::read_file(project, &rel, limit, &AtomicBool::new(false))
                .map_err(|err| Error::other(err.to_string()))?
                .bytes
        }
        None => {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
                .open(path)?;
            let meta = file.metadata()?;
            if !meta.is_file() {
                return Err(Error::new(ErrorKind::InvalidInput, "not a regular file"));
            }
            if meta.len() > limit as u64 {
                return Err(too_large());
            }
            let mut bytes = Vec::new();
            file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > limit {
                return Err(too_large());
            }
            bytes
        }
    };
    String::from_utf8(bytes).map_err(|err| Error::new(ErrorKind::InvalidData, err))
}
