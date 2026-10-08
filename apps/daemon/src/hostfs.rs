//! The host role's filesystem work: browsing directories, finding the git repositories under a
//! parent directory, and cloning a repository.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::protocol::{error_code, CodedError, DirEntry};

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

pub fn is_git_repo(path: &Path) -> bool {
    path.join(".git").exists()
}

pub fn not_a_directory(path: &Path) -> anyhow::Error {
    CodedError::raised(
        error_code::PATH_NOT_A_DIRECTORY,
        format!("`{}` is not a directory", path.display()),
        &[("path", &path.to_string_lossy())],
    )
}

/// Lists the directories under `path`, each flagged with whether it is a git repository.
///
/// Only directories, because a project is a directory; dot-directories are skipped as noise. A
/// failure here is reported rather than swallowed: on a packaged application a directory on another
/// volume raises a macOS file-access prompt, and the user may decline it or leave it unanswered —
/// a normal path, since projects are scattered across volumes.
pub fn list_dir(path: &Path) -> Result<Vec<DirEntry>> {
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
    let read = std::fs::read_dir(path).map_err(|err| {
        CodedError::raised(
            error_code::DIRECTORY_UNREADABLE,
            format!(
                "`{}` could not be read. On a volume the application has no file access to, macOS \
                 asks for permission per volume — grant it and try again: {err}",
                path.display()
            ),
            &[
                ("path", &path.to_string_lossy()),
                ("detail", &err.to_string()),
            ],
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

/// How long a clone may run before it is given up on. Generous — a large repository over a slow
/// connection legitimately takes minutes — but bounded, so a stalled transfer cannot hold a
/// blocking thread for the rest of the daemon's life.
const CLONE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// Clones `remote_url` into a new directory under `parent` and returns that directory. Blocking:
/// the caller runs it off the runtime.
///
/// `git` runs with the user's shell environment, the same one agents are launched with, rather than
/// the daemon's own: a daemon started from Finder has a minimal `PATH` with no `git` on it, and its
/// own environment may carry the agent-session markers that environment exists to filter out. It is
/// made non-interactive and given a deadline for the reasons in `git_status::run_git_output`; the
/// deadline is far longer here because a clone legitimately runs for minutes.
pub fn clone_repo(remote_url: &str, parent: &Path) -> Result<PathBuf> {
    let name = repo_name(remote_url).ok_or_else(|| {
        CodedError::raised(
            error_code::REPOSITORY_NAME_MISSING,
            format!("`{remote_url}` has no repository name in it"),
            &[("url", remote_url)],
        )
    })?;
    let target = parent.join(&name);
    if target.exists() {
        return Err(CodedError::raised(
            error_code::PATH_ALREADY_EXISTS,
            format!("`{}` already exists", target.display()),
            &[("path", &target.to_string_lossy())],
        ));
    }
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;

    let shell_env = crate::env_shell::snapshot().context("snapshotting the shell environment")?;
    let git = crate::env_shell::resolve_binary("git", &shell_env)?;
    let mut command = std::process::Command::new(git);
    command.env_clear();
    command.envs(&shell_env);
    command.env("GIT_TERMINAL_PROMPT", "0");
    command.env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes");
    command.env_remove("GIT_ASKPASS");
    command.env_remove("SSH_ASKPASS");
    command.arg("clone").arg(remote_url).arg(&target);
    let output = crate::env_shell::run_with_timeout(&mut command, CLONE_TIMEOUT)
        .context("running `git clone`")?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(CodedError::raised(
            error_code::GIT_CLONE_FAILED,
            format!("`git clone` failed: {detail}"),
            &[("detail", &detail)],
        ));
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
