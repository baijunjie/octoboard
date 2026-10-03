//! The host role's filesystem work: browsing directories, finding the git repositories under a
//! parent directory, and cloning a repository.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::protocol::DirEntry;

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

pub fn is_git_repo(path: &Path) -> bool {
    path.join(".git").exists()
}

/// Lists the directories under `path`, each flagged with whether it is a git repository.
///
/// Only directories, because a project is a directory; dot-directories are skipped as noise. A
/// failure here is reported rather than swallowed: on a packaged application a directory on another
/// volume raises a macOS file-access prompt, and the user may decline it or leave it unanswered —
/// a normal path, since projects are scattered across volumes.
pub fn list_dir(path: &Path) -> Result<Vec<DirEntry>> {
    if !path.exists() {
        bail!("`{}` does not exist", path.display());
    }
    if !path.is_dir() {
        bail!("`{}` is not a directory", path.display());
    }
    let read = std::fs::read_dir(path).with_context(|| {
        format!(
            "`{}` could not be read. On a volume the application has no file access to, macOS asks \
             for permission per volume — grant it and try again.",
            path.display()
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
        if name.starts_with('.') {
            continue;
        }
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
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
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

/// Clones `remote_url` into a new directory under `parent` and returns that directory. Blocking:
/// the caller runs it off the runtime.
///
/// `git` runs with the user's shell environment, the same one agents are launched with, rather than
/// the daemon's own: a daemon started from Finder has a minimal `PATH` with no `git` on it, and its
/// own environment may carry the agent-session markers that environment exists to filter out.
pub fn clone_repo(remote_url: &str, parent: &Path) -> Result<PathBuf> {
    let name = repo_name(remote_url)
        .ok_or_else(|| anyhow::anyhow!("`{remote_url}` has no repository name in it"))?;
    let target = parent.join(&name);
    if target.exists() {
        bail!("`{}` already exists", target.display());
    }
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;

    let shell_env = crate::env_shell::snapshot().context("snapshotting the shell environment")?;
    let git = crate::env_shell::resolve_binary("git", &shell_env)?;
    let mut command = std::process::Command::new(git);
    command.env_clear();
    command.envs(&shell_env);
    let output = command
        .arg("clone")
        .arg(remote_url)
        .arg(&target)
        .output()
        .context("running `git clone`")?;
    if !output.status.success() {
        bail!(
            "`git clone` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(target)
}

/// The repository name from a clone URL, for both `https://host/owner/repo(.git)` and
/// `git@host:owner/repo(.git)`.
fn repo_name(remote_url: &str) -> Option<String> {
    let trimmed = remote_url.trim().trim_end_matches('/');
    let last = trimmed.rsplit(['/', ':']).next()?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::repo_name;

    #[test]
    fn repo_name_handles_both_url_shapes() {
        assert_eq!(
            repo_name("https://github.com/owner/repo.git").as_deref(),
            Some("repo")
        );
        assert_eq!(
            repo_name("https://github.com/owner/repo/").as_deref(),
            Some("repo")
        );
        assert_eq!(
            repo_name("git@github.com:owner/repo.git").as_deref(),
            Some("repo")
        );
        assert_eq!(repo_name("").as_deref(), None);
    }
}
