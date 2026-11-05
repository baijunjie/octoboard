//! The host role's filesystem work that has no better home: expanding what the user typed into a
//! path, telling "nothing is there" from other errors, normalising a path's text, reading a source
//! within a bound, reading a regular file without blocking (as bytes or as text), appending to one,
//! comparing and removing entries, and refusing a path that is not a directory. The rest lives
//! beside it: `identity` and `place` tell directories apart whatever their paths look like,
//! `listing` browses directories and finds the git repositories under a parent, `replace` replaces
//! a file whole, and `trash` moves a file or directory to the Trash.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::Result;

use crate::platform::fs;
use crate::protocol::{error_code, CodedError};

pub mod identity;
pub mod listing;
pub mod place;
pub mod replace;
pub mod trash;

/// `path` made absolute with every link resolved, as the file system spells it; the one place a
/// path is resolved against the disk, so that every caller gets the host's canonical form.
pub fn canonical(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
    crate::platform::names::canonical(path.as_ref())
}

/// Expands a leading `~` and makes the path absolute. The UI sends whatever the user typed.
///
/// `~/<relative>` is the form a configuration document carries between machines, so its `/` reads
/// as the host's own separator; `~\` is taken too where the host writes paths with a backslash.
pub fn expand(path: &str) -> PathBuf {
    let trimmed = path.trim();
    if trimmed == "~" {
        return crate::paths::home_dir();
    }
    let windows = crate::platform::names::PATH_STYLE == crate::platform::names::PathStyle::Windows;
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return under_home(rest, windows);
    }
    if let Some(rest) = trimmed.strip_prefix("~\\").filter(|_| windows) {
        return under_home(rest, windows);
    }
    PathBuf::from(trimmed)
}

/// `relative` under the home directory; on a host with a backslash for separator, both
/// separators split it into components.
fn under_home(relative: &str, windows: bool) -> PathBuf {
    let home = crate::paths::home_dir();
    if !windows {
        return home.join(relative);
    }
    relative
        .split(['/', '\\'])
        .filter(|component| !component.is_empty())
        .fold(home, |path, component| path.join(component))
}

/// Whether `err` means nothing is at a path: none there, a link that leads nowhere or round in a
/// loop, or a file where a folder on the way should be.
pub fn is_not_found(err: &std::io::Error) -> bool {
    err.kind() == std::io::ErrorKind::NotFound
        || fs::is_not_a_directory(err)
        || fs::is_symlink_loop(err)
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

/// The key `path` is compared by where the question is whether two paths are the same one as
/// written: the path normalised ([`lexically_normalise`]) and spelled as the host reads one path
/// (`names::lexical_key`: on Windows `/` as `\` and case folded). Use it as the key of a set or a
/// map of paths, and in `==` or [`lexically_within`] between paths. Never display text.
pub fn lexical_key(path: &Path) -> PathBuf {
    PathBuf::from(crate::platform::names::lexical_key(&lexically_normalise(
        path,
    )))
}

/// Whether `path` is `root` or lies below it, as written: the path-wise `starts_with`, on the
/// [`lexical_key`] of each, so that spellings of one path the host reads alike match.
pub fn lexically_within(path: &Path, root: &Path) -> bool {
    lexical_key(path).starts_with(lexical_key(root))
}

/// Opens `path` for reading, following links, without blocking, and refuses anything but a regular
/// file: a FIFO's open would otherwise wait for a writer, and a device reads forever.
pub fn open_regular(path: &Path) -> std::io::Result<std::fs::File> {
    fs::open_regular(path)
}

/// Opens `path` to read and append to, without blocking, refusing anything but a regular file.
pub fn open_regular_to_append(path: &Path) -> std::io::Result<std::fs::File> {
    fs::open_regular_to_append(path)
}

/// Makes `at` an alias for `target`, a directory where `leads_to_dir` says it is one, a file
/// otherwise. How the alias was made is of no matter to the caller; where the host cannot make one
/// the error says so.
pub fn alias(target: &Path, at: &Path, leads_to_dir: bool) -> std::io::Result<()> {
    if leads_to_dir {
        fs::alias_dir(target, at)
    } else {
        fs::alias_file(target, at)
    }
    .map(drop)
}

/// When the file or directory `path` leads to was last modified, links followed: the epoch when
/// that cannot be read, so that it sorts before anything whose time can.
pub fn modified(path: &Path) -> std::time::SystemTime {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
}

/// Removes the entry at `path` itself, a directory with everything in it, never following a link:
/// a link is removed as a link. Nothing there is no error.
pub fn remove_any(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => fs::remove_file_or_link(path),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// Whether the two spellings resolve, links followed, to one file or directory. A path that cannot
/// be resolved (nothing there, unreadable) matches nothing. Blocking.
pub fn is_same_path(a: &Path, b: &Path) -> bool {
    match (canonical(a), canonical(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// The comparison key for a directory collision. The stored text stays as typed; only the key
/// folds case where the host's default file system is case-insensitive
/// ([`crate::platform::names::FOLDS_CASE`]: macOS), so `~/.Claude` and `~/.claude` read as one
/// directory there. Other hosts compare byte for byte. Unicode normalisation is not folded: no
/// crate for it is in the dependency tree.
pub fn dir_key(dir: &str) -> String {
    if crate::platform::names::FOLDS_CASE {
        dir.to_lowercase()
    } else {
        dir.to_string()
    }
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

/// Whether `path` is a readable directory or may be created beneath an existing directory.
/// A dangling link is an occupied entry, even though following it reports `NotFound`.
pub fn directory_exists_or_creatable(path: &Path) -> Result<bool> {
    let mut candidate = path;
    loop {
        match std::fs::symlink_metadata(candidate) {
            Ok(_) => {
                let metadata = std::fs::metadata(candidate).map_err(|err| {
                    directory_unreadable(
                        path,
                        &err,
                        format!("`{}` could not be looked at: {err}", path.display()),
                    )
                })?;
                if !metadata.is_dir() {
                    return Err(not_a_directory(candidate));
                }
                std::fs::read_dir(candidate).map_err(|err| {
                    directory_unreadable(
                        path,
                        &err,
                        format!("`{}` could not be read: {err}", path.display()),
                    )
                })?;
                return Ok(candidate == path);
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                candidate = candidate.parent().ok_or_else(|| not_a_directory(path))?;
            }
            Err(err) if fs::is_not_a_directory(&err) => {
                return Err(not_a_directory(path));
            }
            Err(err) => {
                return Err(directory_unreadable(
                    path,
                    &err,
                    format!("`{}` could not be looked at: {err}", path.display()),
                ))
            }
        }
    }
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
    fn a_tilde_path_is_taken_from_home_with_either_separator_where_the_host_has_both() {
        let home = crate::paths::home_dir();
        assert_eq!(expand("~"), home);
        assert_eq!(expand(" ~/a/b "), home.join("a").join("b"));
        assert_eq!(expand("/a/b"), PathBuf::from("/a/b"));
        if cfg!(windows) {
            assert_eq!(expand("~\\a\\b"), home.join("a").join("b"));
            assert_eq!(expand("~/a\\b"), home.join("a").join("b"));
        } else {
            // A backslash is an ordinary character of a Unix name.
            assert_eq!(expand("~\\a"), PathBuf::from("~\\a"));
            assert_eq!(expand("~/a\\b"), home.join("a\\b"));
        }
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
    #[cfg(unix)]
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
        let results = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            [
                require_directory(&dir.join("locked/inner")),
                directory_exists_or_creatable(&dir.join("locked/inner")).map(|_| ()),
                directory_exists_or_creatable(&dir.join("locked/missing")).map(|_| ()),
            ]
        }));
        chmod(0o755);
        for result in results.expect("looking at the path panicked") {
            assert_eq!(code_of(result), error_code::DIRECTORY_UNREADABLE);
        }
    }

    #[test]
    fn creatable_directories_exclude_occupied_and_unresolvable_paths() {
        use crate::test_support::symlink;

        let dir = ScratchDir::new("hostfs-creatable-directory");
        assert!(!directory_exists_or_creatable(&dir.join("missing/nested")).unwrap());
        assert!(!dir.join("missing").exists());
        assert!(directory_exists_or_creatable(&dir).unwrap());
        std::fs::write(dir.join("file"), "keep").unwrap();
        symlink(dir.join("absent"), dir.join("dangling")).unwrap();
        symlink(dir.join("loop"), dir.join("loop")).unwrap();
        for name in [
            "file",
            "file/child",
            "dangling",
            "dangling/child",
            "loop",
            "loop/child",
        ] {
            assert!(
                directory_exists_or_creatable(&dir.join(name)).is_err(),
                "{name}"
            );
        }
        symlink(&*dir, dir.join("valid")).unwrap();
        assert!(directory_exists_or_creatable(&dir.join("valid")).unwrap());
        assert!(!directory_exists_or_creatable(&dir.join("valid/new/nested")).unwrap());
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
    #[cfg(unix)]
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
