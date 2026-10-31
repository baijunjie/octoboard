//! Bounded reads of the files an adapter reads: configuration in the project's tree, through
//! `browse::live`, and the agent's own configuration, caches and manifests
//! (`crate::hostfs::read_regular_text`, re-exported here).

use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::browse::live;
pub(super) use crate::hostfs::read_regular_text;
use crate::wire_path::RelPath;

/// The most bytes read from a small configuration file (a JSON settings file, a TOML config).
pub(super) const SMALL_FILE_LIMIT: usize = 1024 * 1024;

/// The most bytes read from a configuration file that grows with use, such as Claude Code's global
/// config, which holds an entry for every project.
pub(crate) const LARGE_FILE_LIMIT: usize = 64 * 1024 * 1024;

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
    let key = |dir: &Path| crate::accounts::dirs::account_dir_key(&dir.to_string_lossy());
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
    crate::hostfs::utf8_text(bytes)
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
