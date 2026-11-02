//! Browsing directories: listing the directories under a path, finding the git repositories under a
//! parent, and detecting which agent a directory is set up for.

use std::path::{Path, PathBuf};

use anyhow::Result;

use super::{directory_unreadable, require_directory};
use crate::git::layout::is_git_repo;
use crate::protocol::{Agent, DirEntry};

/// Lists the directories under `path`, each flagged with whether it is a git repository.
///
/// Only directories, including dot-directories, because a project is a directory. A
/// failure here is reported rather than swallowed: on a packaged application a directory on another
/// volume raises a macOS file-access prompt, and the user may decline it or leave it unanswered —
/// a normal path, since projects are scattered across volumes.
pub fn list_dir(path: &Path) -> Result<Vec<DirEntry>> {
    require_directory(path)?;
    let read = std::fs::read_dir(path).map_err(|err| {
        directory_unreadable(
            path,
            &err,
            format!(
                "`{}` could not be read. On a volume the application has no file access to, macOS \
                 asks for permission per volume — grant it and try again: {err}",
                path.display()
            ),
        )
    })?;

    let mut entries = Vec::new();
    for entry in read {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                tracing::debug!(%err, "skipping an unreadable directory entry");
                continue;
            }
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }
        entries.push(DirEntry {
            name,
            path: entry_path.to_string_lossy().into_owned(),
            is_git_repo: is_git_repo(&entry_path),
        });
    }
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    Ok(entries)
}

/// The git repositories directly beneath `parent`, in name order. Only one level down: a nested
/// checkout belongs to the repository above it rather than being a project of its own.
pub fn discover_repos(parent: &Path) -> Result<Vec<PathBuf>> {
    let mut repos: Vec<PathBuf> = list_dir(parent)?
        .into_iter()
        .filter(|entry| entry.is_git_repo)
        .map(|entry| PathBuf::from(entry.path))
        .collect();
    repos.sort();
    Ok(repos)
}

/// The agent a project directory is set up for, read from marker files at its top level (see
/// [`detect_agent_from_names`]); `None` when the directory cannot be read.
pub fn detect_agent(dir: &Path) -> Option<Agent> {
    let names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
        .collect();
    detect_agent_from_names(names.iter().map(String::as_str))
}

/// The agent a repository is set up for, from the names at its top level, or `None` when they name
/// no agent or more than one — a repository carrying both `CLAUDE.md` and `AGENTS.md` is common,
/// and picking one of them would be a guess. `AGENTS.md` and `.agents` are generic, read by Grok
/// Build as well, so they point at Codex only when `.grok` is absent.
pub(crate) fn detect_agent_from_names<'a>(
    names: impl IntoIterator<Item = &'a str>,
) -> Option<Agent> {
    let (mut claude, mut grok, mut codex, mut generic) = (false, false, false, false);
    for name in names {
        match name {
            "CLAUDE.md" | ".claude" => claude = true,
            ".grok" => grok = true,
            ".codex" => codex = true,
            "AGENTS.md" | ".agents" => generic = true,
            _ => {}
        }
    }
    let codex = codex || (!grok && generic);
    match (claude, codex, grok) {
        (true, false, false) => Some(Agent::Claude),
        (false, true, false) => Some(Agent::Codex),
        (false, false, true) => Some(Agent::Grok),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{detect_agent, list_dir};
    use crate::protocol::Agent;
    use crate::test_support::ScratchDir;

    #[test]
    fn directory_listing_includes_hidden_directories_but_not_files() {
        let dir = ScratchDir::new("hostfs-hidden-directories");
        std::fs::create_dir_all(dir.join(".claude/.git")).expect("hidden repository");
        std::fs::create_dir(dir.join("Documents")).expect("visible directory");
        std::fs::write(dir.join(".hidden-file"), "").expect("hidden file");
        std::fs::write(dir.join("visible-file"), "").expect("visible file");

        let entries = list_dir(&dir).expect("listing");
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            [".claude", "Documents"]
        );
        assert_eq!(entries[0].path, dir.join(".claude").to_string_lossy());
        assert!(entries[0].is_git_repo);
        assert!(!entries[1].is_git_repo);
    }

    #[test]
    fn a_directory_that_cannot_be_read_is_reported_with_the_volume_hint() {
        use std::os::unix::fs::PermissionsExt;
        // SAFETY: a plain query of this process's effective user.
        if unsafe { libc::geteuid() } == 0 {
            // Permissions do not stop the superuser, so there is nothing to refuse.
            return;
        }
        let dir = ScratchDir::new("hostfs-list-locked");
        let locked = dir.join("locked");
        std::fs::create_dir(&locked).expect("directory");
        let chmod = |mode| {
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(mode)).unwrap()
        };
        chmod(0o000);
        let result = std::panic::catch_unwind(|| list_dir(&locked));
        chmod(0o755);
        let error = result.expect("listing panicked").unwrap_err();
        let coded = error.downcast_ref::<crate::protocol::CodedError>().unwrap();
        assert_eq!(
            coded.code,
            crate::protocol::error_code::DIRECTORY_UNREADABLE
        );
        assert!(coded.message.contains("macOS"), "{}", coded.message);
        assert_eq!(coded.params["path"], locked.to_string_lossy());
    }

    fn detected_with(label: &str, markers: &[&str]) -> Option<Agent> {
        let dir = ScratchDir::new(&format!("hostfs-detect-{label}"));
        for marker in markers {
            if marker.starts_with('.') {
                std::fs::create_dir(dir.join(marker)).expect("marker dir");
            } else {
                std::fs::write(dir.join(marker), "").expect("marker file");
            }
        }
        detect_agent(&dir)
    }

    #[test]
    fn detect_agent_reads_the_marker_files() {
        assert_eq!(detected_with("claude", &["CLAUDE.md"]), Some(Agent::Claude));
        assert_eq!(detected_with("codex", &["AGENTS.md"]), Some(Agent::Codex));
        // `AGENTS.md` is generic, so `.grok` claims it.
        assert_eq!(
            detected_with("grok", &[".grok", "AGENTS.md"]),
            Some(Agent::Grok)
        );
        assert_eq!(detected_with("both", &["CLAUDE.md", "AGENTS.md"]), None);
        assert_eq!(detected_with("none", &[]), None);
    }
}
