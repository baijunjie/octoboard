//! The host role's filesystem work that has no better home: expanding what the user typed into a
//! path, telling "nothing is there" from other errors, normalising a path's text, reading a regular
//! file without blocking (as bytes or as text), and refusing a path that is not a directory. The
//! rest lives beside it: `identity` and `place` tell directories apart whatever their paths look
//! like, `listing` browses directories, finds the git repositories under a parent and detects which
//! agent a directory is set up for, and `trash` moves a file or directory to the Trash.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::protocol::{error_code, CodedError};

pub mod identity;
pub mod listing;
pub mod place;
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
    use std::io::Read;

    let too_large = || std::io::Error::new(std::io::ErrorKind::InvalidData, "file too large");
    let file = open_regular(path)?;
    if file.metadata()?.len() > limit as u64 {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(too_large());
    }
    Ok(bytes)
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

/// Refuses a `path` that is not an existing directory, with `path_not_found` or
/// `path_not_a_directory`.
pub fn require_directory(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(CodedError::raised(
            error_code::PATH_NOT_FOUND,
            format!("`{}` does not exist", path.display()),
            &[("path", &path.to_string_lossy())],
        ));
    }
    if !path.is_dir() {
        return Err(not_a_directory(path));
    }
    Ok(())
}
