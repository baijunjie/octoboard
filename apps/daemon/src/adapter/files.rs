//! Bounded reads of project configuration and agent-owned configuration, caches and manifests.

use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::browse::live;
use crate::browse::wire_path::RelPath;

/// The most bytes read from a small configuration file (a JSON settings file, a TOML config).
pub(super) const SMALL_FILE_LIMIT: usize = 1024 * 1024;

/// The most bytes read from a configuration file that grows with use, such as Claude Code's global
/// config, which holds an entry for every project.
pub(super) const LARGE_FILE_LIMIT: usize = 64 * 1024 * 1024;

/// `cwd` canonical, else as given: the `project` that `read_bounded_text` takes, so that a file
/// inside it is recognised as such.
pub(super) fn canonical_project(cwd: &Path) -> PathBuf {
    cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf())
}

/// Whether `dir`, a directory in the project's tree, is the agent configuration directory
/// `config_dir` in effect: a home directory opened as the project holds the agent's own, whose
/// files are the agent's and read with `read_regular_text`, not as the project's. Compared as
/// account directories are, with `config_dir`'s parent resolved, as `dir` lies under the canonical
/// project; `config_dir` itself is left unresolved, since a dotfiles manager often links it. A
/// `config_dir` spelt as the link's target, while the project's directory is the link, is not
/// matched, so its files are read as the project's: refused, the safe side for each reader.
pub(super) fn is_agent_config_dir(dir: &Path, config_dir: &Path) -> bool {
    let config_dir = crate::hostfs::lexically_normalise(config_dir);
    let resolved = match (config_dir.parent(), config_dir.file_name()) {
        (Some(parent), Some(name)) => parent
            .canonicalize()
            .unwrap_or_else(|_| parent.to_path_buf())
            .join(name),
        _ => config_dir,
    };
    let key = |dir: &Path| crate::account_dirs::account_dir_key(&dir.to_string_lossy());
    key(&crate::hostfs::lexically_normalise(dir)) == key(&resolved)
}

/// The text of a configuration file in the project's tree, read within `limit` bytes. Inside
/// `project` (canonical) it goes through `browse::live`, which refuses a link out of the project, a
/// FIFO and the like; anywhere else (a directory above the project) as `read_regular_text` does.
pub(super) fn read_bounded_text(
    path: &Path,
    project: &Path,
    limit: usize,
) -> std::io::Result<String> {
    use std::io::Error;

    let Some(rel) = path
        .strip_prefix(project)
        .ok()
        .and_then(|rel| RelPath::from_bytes(rel.as_os_str().as_bytes().to_vec()))
    else {
        return read_regular_text(path, limit);
    };
    // Absent is told from the metadata, since `live` reports it only as a failure.
    std::fs::symlink_metadata(path)?;
    let bytes = live::read_file(project, &rel, limit, &AtomicBool::new(false))
        .map_err(|err| Error::other(err.to_string()))?
        .bytes;
    utf8(bytes)
}

/// The text of a file the project does not own (an agent's configuration, cache or install,
/// the home directory, a system-wide directory), read within `limit` bytes. Links are followed, as
/// the agent follows them, even where the file lies under the project's directory, as a home
/// directory opened as a project holds the agent's own; the target must be a regular file, opened
/// without blocking so that a FIFO cannot hold the caller.
pub(crate) fn read_regular_text(path: &Path, limit: usize) -> std::io::Result<String> {
    use std::io::{Error, ErrorKind, Read};
    use std::os::unix::fs::OpenOptionsExt;

    let too_large = || Error::new(ErrorKind::InvalidData, "file too large");
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
    utf8(bytes)
}

fn utf8(bytes: Vec<u8>) -> std::io::Result<String> {
    String::from_utf8(bytes)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The configuration directory is often spelt through a link above it (a temporary directory
    /// under `/var` on macOS, a home reached through a mount link), while the project is canonical.
    #[test]
    fn an_agent_config_dir_is_recognised_under_any_spelling_of_its_parent() {
        let scratch = crate::test_support::ScratchDir::new("agent-config-dir");
        let home = canonical_project(&scratch);
        assert!(is_agent_config_dir(
            &home.join(".claude"),
            &scratch.join(".claude")
        ));
        assert!(is_agent_config_dir(
            &home.join(".claude"),
            &home.join("x/../.claude")
        ));
        assert!(!is_agent_config_dir(
            &home.join(".claude"),
            &scratch.join(".codex")
        ));
    }
}
