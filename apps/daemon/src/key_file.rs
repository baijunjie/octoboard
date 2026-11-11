//! The per-launch key file: where a helper process of a session (`octoboardd hook`, `mcp` and
//! `status-line`) reads the key every request to the daemon carries, so that the key is never in
//! arguments, which every user of the machine can list. The file is owner-only and lives in the
//! session's scratch directory. A file rather than the environment, because agents write the
//! environment they run in to files of their own (Codex snapshots every exported variable into
//! files other users can read) and filter what they pass to the helpers they run (Codex gives an
//! MCP server only the variables its `env_vars` names); a path on the command line is the one route
//! every agent keeps. The git hook helpers are given the path in the session's environment
//! instead, which git inherits from the agent.

use std::path::{Path, PathBuf};

use crate::hostfs::replace::{replace_beside, Permissions};

/// Where a session's key file is, or would be, in its scratch directory.
fn path(scratch: &Path) -> PathBuf {
    scratch.join("key")
}

/// Writes `key`, the key a session's launch is given, to the launch's key file and returns its
/// path. Owner-only from the first byte, like every file that holds the key; removed with the
/// session's scratch directory.
pub fn write(scratch: &Path, key: &str) -> std::io::Result<PathBuf> {
    let path = path(scratch);
    replace_beside(&path, key.as_bytes(), Permissions::Private)?;
    Ok(path)
}

/// The key in the file at `path`, without the whitespace after it; `None` when the file cannot be
/// read or holds no key. The helpers that take the path on their command line treat that as the
/// daemon refusing their requests; the git hook helpers go to the daemon's own route instead.
pub fn read(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let key = content.trim_end();
    (!key.is_empty()).then(|| key.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    #[test]
    fn the_key_file_holds_the_key_and_is_readable_by_its_owner_only() {
        let dir = ScratchDir::new("key-file-write");
        let path = write(&dir, "feedface").expect("key file");
        assert_eq!(path, dir.join("key"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "feedface");
        assert_eq!(read(&path).as_deref(), Some("feedface"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        // A later launch of the session replaces it whole.
        write(&dir, "cafe").expect("key file");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "cafe");
    }

    #[test]
    fn the_key_is_read_without_its_trailing_whitespace() {
        let dir = ScratchDir::new("key-file-read");
        let path = dir.join("key");
        for content in ["abc", "abc\n", "abc\r\n", "abc \n\n"] {
            std::fs::write(&path, content).unwrap();
            assert_eq!(read(&path).as_deref(), Some("abc"), "{content:?}");
        }
    }

    #[test]
    fn a_missing_or_empty_file_holds_no_key() {
        let dir = ScratchDir::new("key-file-absent");
        let path = dir.join("key");
        assert_eq!(read(&path), None);
        for content in ["", "\n", "  \r\n"] {
            std::fs::write(&path, content).unwrap();
            assert_eq!(read(&path), None, "{content:?}");
        }
    }

    #[test]
    fn a_path_that_cannot_be_read_as_a_file_holds_no_key() {
        let dir = ScratchDir::new("key-file-unreadable");
        assert_eq!(read(&dir), None);
        assert_eq!(read(Path::new("")), None);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = dir.join("key");
            std::fs::write(&path, "abc").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
            // The file stays readable for a process that bypasses permissions (root).
            let bypassed = std::fs::read(&path).is_ok();
            assert_eq!(read(&path).is_some(), bypassed);
        }
    }
}
