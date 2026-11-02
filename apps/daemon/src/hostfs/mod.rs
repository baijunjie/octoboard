//! The host role's filesystem work that has no better home: expanding what the user typed into a
//! path, telling "nothing is there" from other errors, normalising a path's text, reading a source
//! within a bound, reading a regular file without blocking (as bytes or as text), and refusing a
//! path that is not a directory. The rest lives beside it: `identity` and `place` tell directories
//! apart whatever their paths look like, `listing` browses directories, finds the git repositories
//! under a parent and detects which agent a directory is set up for, `replace` replaces a file
//! whole, and `trash` moves a file or directory to the Trash.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::Result;

use crate::protocol::{error_code, CodedError};

pub mod identity;
pub mod listing;
pub mod place;
pub mod replace;
pub mod trash;

/// Expands a leading `~` and makes the path absolute. The UI sends whatever the user typed.
pub fn expand(path: &str) -> PathBuf {
    let trimmed = path.trim();
    if trimmed == "~" {
        return crate::paths::home_dir();
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return crate::paths::home_dir().join(rest);
    }
    PathBuf::from(trimmed)
}

/// Whether `err` means nothing is at a path: none there, a link that leads nowhere or round in a
/// loop, or a file where a folder on the way should be.
pub fn is_not_found(err: &std::io::Error) -> bool {
    err.kind() == std::io::ErrorKind::NotFound
        || matches!(err.raw_os_error(), Some(libc::ENOTDIR | libc::ELOOP))
}

/// A path with its `.` components dropped, each `..` folded into the component before it, and no
/// trailing separator. Lexical only: no symlink is resolved and nothing is read from disk, so the
/// result is the path as the user chose it, written one way.
pub fn lexically_normalise(path: &Path) -> PathBuf {
    let mut normalised = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                normalised.pop();
            }
            std::path::Component::CurDir => {}
            other => normalised.push(other),
        }
    }
    normalised
}

/// Opens `path` for reading, following links, without blocking, and refuses anything but a regular
/// file: a FIFO's open would otherwise wait for a writer, and a device reads forever.
pub fn open_regular(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    Ok(file)
}

/// The bytes of the regular file at `path` (see [`open_regular`]), refused when it holds more than
/// `limit`.
pub fn read_regular(path: &Path, limit: usize) -> std::io::Result<Vec<u8>> {
    let too_large = || std::io::Error::new(std::io::ErrorKind::InvalidData, "file too large");
    let file = open_regular(path)?;
    let len = file.metadata()?.len();
    if len > limit as u64 {
        return Err(too_large());
    }
    read_bounded(file, limit, len, None).map_err(|err| match err {
        ReadFailure::Exceeded => too_large(),
        ReadFailure::Io(err) => err,
        ReadFailure::Cancelled => std::io::Error::from(std::io::ErrorKind::Interrupted),
    })
}

/// Why [`read_bounded`] read nothing.
#[derive(Debug)]
pub enum ReadFailure {
    Exceeded,
    Cancelled,
    Io(std::io::Error),
}

/// The size of each read, and how often a cancel is noticed.
const READ_CHUNK: usize = 64 * 1024;

/// Reads `source` to its end, holding at most `limit` bytes: one byte more and the read stops
/// there, whatever the source would still give. A file that grows while it is read is therefore
/// bounded by the read, not by the size it had when it was opened. The buffer is sized from
/// `expected` and grown at most once, straight to the limit, so it never holds more than that.
/// `cancel`, when there is one, is checked before each chunk; `None` is a read that cannot be
/// cancelled.
pub fn read_bounded(
    mut source: impl std::io::Read,
    limit: usize,
    expected: u64,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<u8>, ReadFailure> {
    let mut bytes = Vec::with_capacity(expected.min(limit as u64) as usize + 1);
    let mut chunk = vec![0u8; READ_CHUNK];
    loop {
        if cancel.is_some_and(|cancel| cancel.load(std::sync::atomic::Ordering::Relaxed)) {
            return Err(ReadFailure::Cancelled);
        }
        let room = (limit - bytes.len() + 1).min(READ_CHUNK);
        match source.read(&mut chunk[..room]) {
            Ok(0) => return Ok(bytes),
            Ok(n) => {
                if bytes.len() + n > bytes.capacity() {
                    bytes.reserve_exact(limit + 1 - bytes.len());
                }
                bytes.extend_from_slice(&chunk[..n]);
                if bytes.len() > limit {
                    return Err(ReadFailure::Exceeded);
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
            Err(err) => return Err(ReadFailure::Io(err)),
        }
    }
}

/// The text of a file the project does not own (an agent's configuration, cache or install,
/// the home directory, a system-wide directory), read within `limit` bytes. Links are followed, as
/// the agent follows them, even where the file lies under the project's directory, as a home
/// directory opened as a project holds the agent's own; the target must be a regular file, opened
/// without blocking so that a FIFO cannot hold the caller.
pub(crate) fn read_regular_text(path: &Path, limit: usize) -> std::io::Result<String> {
    utf8_text(read_regular(path, limit)?)
}

/// `bytes` as text, refused as `InvalidData` when they are not UTF-8.
pub(crate) fn utf8_text(bytes: Vec<u8>) -> std::io::Result<String> {
    String::from_utf8(bytes)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))
}

pub fn not_a_directory(path: &Path) -> anyhow::Error {
    CodedError::raised(
        error_code::PATH_NOT_A_DIRECTORY,
        format!("`{}` is not a directory", path.display()),
        &[("path", &path.to_string_lossy())],
    )
}

/// The `directory_unreadable` error for `path` failing with `err`, saying `message`.
pub fn directory_unreadable(path: &Path, err: &std::io::Error, message: String) -> anyhow::Error {
    CodedError::raised(
        error_code::DIRECTORY_UNREADABLE,
        message,
        &[
            ("path", &path.to_string_lossy()),
            ("detail", &err.to_string()),
        ],
    )
}

/// Refuses a `path` that is not an existing directory, with `path_not_found` for one that is not
/// there, `path_not_a_directory` for one that is something else, and `directory_unreadable` when it
/// cannot be looked at (no access, or any other I/O failure): only a missing path is absent.
pub fn require_directory(path: &Path) -> Result<()> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(not_a_directory(path)),
        Err(err) if is_not_found(&err) => Err(CodedError::raised(
            error_code::PATH_NOT_FOUND,
            format!("`{}` does not exist", path.display()),
            &[("path", &path.to_string_lossy())],
        )),
        Err(err) => Err(directory_unreadable(
            path,
            &err,
            format!("`{}` could not be looked at: {err}", path.display()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    fn code_of(result: Result<()>) -> crate::protocol::Code {
        let error = result.unwrap_err();
        error.downcast_ref::<CodedError>().unwrap().code
    }

    #[test]
    fn a_directory_is_required_to_exist_and_to_be_one() {
        let dir = ScratchDir::new("hostfs-require-directory");
        std::fs::write(dir.join("file"), "x").unwrap();
        assert!(require_directory(&dir).is_ok());
        assert_eq!(
            code_of(require_directory(&dir.join("missing"))),
            error_code::PATH_NOT_FOUND
        );
        assert_eq!(
            code_of(require_directory(&dir.join("file"))),
            error_code::PATH_NOT_A_DIRECTORY
        );
    }

    /// A path that cannot be looked at is not a path that is absent.
    #[test]
    fn a_directory_that_cannot_be_looked_at_is_unreadable_not_missing() {
        use std::os::unix::fs::PermissionsExt;
        // SAFETY: a plain query of this process's effective user.
        if unsafe { libc::geteuid() } == 0 {
            // Permissions do not stop the superuser, so there is nothing to refuse.
            return;
        }
        let dir = ScratchDir::new("hostfs-require-directory-locked");
        std::fs::create_dir_all(dir.join("locked/inner")).unwrap();
        let chmod = |mode| {
            std::fs::set_permissions(dir.join("locked"), std::fs::Permissions::from_mode(mode))
                .unwrap()
        };
        chmod(0o000);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_directory(&dir.join("locked/inner"))
        }));
        chmod(0o755);
        assert_eq!(
            code_of(result.expect("looking at the path panicked")),
            error_code::DIRECTORY_UNREADABLE
        );
    }

    #[test]
    fn a_regular_file_is_read_up_to_its_limit_and_refused_past_it() {
        let dir = ScratchDir::new("hostfs-read-regular");
        let path = dir.join("file");
        std::fs::write(&path, "0123456789").unwrap();
        assert_eq!(read_regular(&path, 10).unwrap(), b"0123456789");
        let err = read_regular(&path, 9).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(err.to_string(), "file too large");
    }

    /// A source that gives more than the limit is stopped one byte past it, whatever length it was
    /// expected to have, and a cancel is noticed before the next chunk.
    #[test]
    fn a_bounded_read_stops_past_its_limit_and_at_a_cancel() {
        let outcome = read_bounded(std::io::repeat(b'x'), 1000, 3, None);
        assert!(matches!(outcome, Err(ReadFailure::Exceeded)));
        let bytes = read_bounded(&b"0123456789"[..], 10, 0, None).unwrap();
        assert_eq!(bytes, b"0123456789");
        let cancel = AtomicBool::new(true);
        let outcome = read_bounded(&b"0123456789"[..], 10, 10, Some(&cancel));
        assert!(matches!(outcome, Err(ReadFailure::Cancelled)));
    }

    /// A FIFO is refused at once rather than waited on for a writer, and a directory is refused.
    #[test]
    fn anything_but_a_regular_file_is_refused() {
        let dir = ScratchDir::new("hostfs-read-regular-fifo");
        let fifo = dir.join("fifo");
        let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: `name` is a valid NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        for path in [fifo.as_path(), &dir] {
            let err = read_regular(path, 10).unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput, "{path:?}");
        }
    }
}
