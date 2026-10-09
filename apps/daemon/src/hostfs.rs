//! The host role's filesystem work: browsing directories, finding the git repositories under a
//! parent directory, cloning a repository, and detecting which agent a directory or a remote
//! repository is set up for.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::protocol::{error_code, Agent, CodedError, DirEntry};

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
fn detect_agent_from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<Agent> {
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

/// Lists the directories under `path`, each flagged with whether it is a git repository.
///
/// Only directories, because a project is a directory; dot-directories are skipped as noise. A
/// failure here is reported rather than swallowed: on a packaged application a directory on another
/// volume raises a macOS file-access prompt, and the user may decline it or leave it unanswered —
/// a normal path, since projects are scattered across volumes.
pub fn list_dir(path: &Path) -> Result<Vec<DirEntry>> {
    require_directory(path)?;
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

/// A `git` command for the user's remote work, run with `shell_env`, the user's shell environment
/// that agents are launched with, rather than the daemon's own: a daemon started from Finder has a
/// minimal `PATH` with no `git` on it, and its own environment may carry the agent-session markers
/// that environment exists to filter out. Made non-interactive, so a missing credential or an
/// unknown host key fails at once instead of waiting on a terminal nobody is watching; the caller
/// gives it a deadline.
fn git_command(shell_env: &HashMap<String, String>) -> Result<std::process::Command> {
    let git = crate::env_shell::resolve_binary("git", shell_env)?;
    let mut command = std::process::Command::new(git);
    command.env_clear();
    command.envs(shell_env);
    command.env("GIT_TERMINAL_PROMPT", "0");
    command.env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes");
    command.env_remove("GIT_ASKPASS");
    command.env_remove("SSH_ASKPASS");
    Ok(command)
}

/// Clones `remote_url` into a new directory under `parent` and returns that directory. Blocking:
/// the caller runs it off the runtime.
///
/// Runs as [`git_command`] with a deadline far longer than a probe's, because a clone legitimately
/// runs for minutes.
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
    let mut command = git_command(&shell_env)?;
    command.arg("clone").arg("--").arg(remote_url).arg(&target);
    let output = crate::subprocess::run_with_timeout(&mut command, CLONE_TIMEOUT)
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

/// How long the fetch of a probe may run. Much shorter than a clone's: it fetches one commit and
/// its trees, no file contents, and the user is waiting on it with a form held shut.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// How long listing the fetched tip may run; it reads a local repository.
const LIST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// A scratch directory for one probe, removed when the probe ends.
struct ProbeDir(PathBuf);

impl Drop for ProbeDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// Checks that `remote_url` can be reached with the user's credentials and returns the agent its
/// top level is set up for (see [`detect_agent_from_names`]), running `git` as [`git_command`] does
/// with `shell_env`. Blocking: the caller runs it off the runtime.
///
/// A shallow, blobless, checkout-less clone into a scratch directory, whose top-level names are
/// then listed: that fetches the tip commit and its trees but no file, and works for any remote
/// (`git ls-remote` would show refs only). A remote that does not support filtering sends the
/// whole tip commit instead, which is still no history. A repository that cannot be read, or
/// whose fetch outlasts [`PROBE_TIMEOUT`], is `git_remote_unreachable`. An empty repository is
/// reachable and names no agent.
pub fn probe_remote_agent(
    remote_url: &str,
    shell_env: &HashMap<String, String>,
) -> Result<Option<Agent>> {
    let scratch = ProbeDir(std::env::temp_dir().join(format!(
        "octoboardd-probe-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    )));
    let unreachable = |detail: &str| {
        CodedError::raised(
            error_code::GIT_REMOTE_UNREACHABLE,
            format!("the repository could not be read: {detail}"),
            &[("detail", detail)],
        )
    };
    let mut clone = git_command(shell_env)?;
    clone
        .args([
            "clone",
            "--depth",
            "1",
            "--filter=blob:none",
            "--no-checkout",
            "--quiet",
        ])
        .arg("--")
        .arg(remote_url)
        .arg(&scratch.0);
    let output = crate::subprocess::run_with_timeout(&mut clone, PROBE_TIMEOUT)
        .map_err(|err| unreachable(&format!("{err:#}")))?;
    if !output.status.success() {
        return Err(unreachable(String::from_utf8_lossy(&output.stderr).trim()));
    }
    let mut list = git_command(shell_env)?;
    list.args(["ls-tree", "--name-only", "HEAD"])
        .current_dir(&scratch.0);
    let listed = crate::subprocess::run_with_timeout(&mut list, LIST_TIMEOUT)
        .context("running `git ls-tree`")?;
    // Any failure to list the tip answers "no agent": the clone has just succeeded, so the remote
    // is reachable, and the usual cause is an empty repository, whose `HEAD` is unborn.
    if !listed.status.success() {
        return Ok(None);
    }
    let listing = String::from_utf8_lossy(&listed.stdout);
    Ok(detect_agent_from_names(listing.lines()))
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
    use super::{detect_agent, probe_remote_agent, repo_name};
    use crate::protocol::Agent;
    use crate::test_support::ScratchDir;

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

    /// The probe reads a repository's top-level names without checking it out, and says why an
    /// unreadable remote failed.
    #[test]
    fn probe_reads_a_remote_without_a_checkout() {
        let dir = ScratchDir::new("hostfs-probe");
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(args)
                .current_dir(&*dir)
                .status()
                .expect("git");
            assert!(status.success(), "git {args:?}");
        };
        git(&["init", "--quiet", "--initial-branch=main", "work"]);
        std::fs::write(dir.join("work/CLAUDE.md"), "").expect("marker");
        git(&["-C", "work", "add", "."]);
        git(&["-C", "work", "commit", "--quiet", "-m", "init"]);
        git(&["init", "--quiet", "--bare", "empty.git"]);
        // The probe reads the process's own environment, not the user's login shell.
        let env: std::collections::HashMap<String, String> = std::env::vars().collect();
        let url = format!("file://{}", dir.join("work").display());
        assert_eq!(probe_remote_agent(&url, &env).unwrap(), Some(Agent::Claude));
        let empty = format!("file://{}", dir.join("empty.git").display());
        assert_eq!(probe_remote_agent(&empty, &env).unwrap(), None);

        let missing = format!("file://{}", dir.join("missing").display());
        let err = probe_remote_agent(&missing, &env).unwrap_err();
        let coded = err
            .downcast_ref::<crate::protocol::CodedError>()
            .expect("coded");
        assert_eq!(
            coded.code,
            crate::protocol::error_code::GIT_REMOTE_UNREACHABLE
        );
    }

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
