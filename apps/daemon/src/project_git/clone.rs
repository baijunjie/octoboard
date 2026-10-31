//! Cloning a repository for a new project, and probing a remote for the agent its top level is set
//! up for without a checkout.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::git::run::{self, Access, Limits, Policy, UserGit, WorkDir};
use crate::hostfs::listing::detect_agent_from_names;
use crate::processes::subprocess::BoundedFailure;
use crate::protocol::{error_code, Agent, CodedError};

/// How long a clone may run before it is given up on. Generous — a large repository over a slow
/// connection legitimately takes minutes — but bounded, so a stalled transfer cannot hold a
/// blocking thread for the rest of the daemon's life.
const CLONE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// A `git` command for the user's remote work, run with `shell_env`, the user's shell environment
/// that agents are launched with, rather than the daemon's own: a daemon started from Finder has a
/// minimal `PATH` with no `git` on it, and its own environment may carry the agent-session markers
/// that environment exists to filter out. Only the user's own git configuration is taken from it,
/// as for every `git` the daemon runs. Made non-interactive, so a missing credential or an unknown
/// host key fails at once instead of waiting on a terminal nobody is watching; the caller gives it
/// a deadline.
fn git_command(shell_env: &HashMap<String, String>) -> Result<std::process::Command> {
    let git = UserGit::from_env(shell_env.clone())?;
    Ok(git.command(&Policy::PLAIN, Access::Write, WorkDir::Inherited))
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

    let shell_env =
        crate::env_shell::launch_snapshot().context("snapshotting the shell environment")?;
    let mut command = clone_command(&shell_env, remote_url, &target)?;
    let output = run::run(&mut command, Limits::deadline(CLONE_TIMEOUT), None)
        .map_err(BoundedFailure::into_error)
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

/// The `git clone` [`clone_repo`] runs.
fn clone_command(
    shell_env: &HashMap<String, String>,
    remote_url: &str,
    target: &Path,
) -> Result<std::process::Command> {
    let mut command = git_command(shell_env)?;
    command.arg("clone").arg("--").arg(remote_url).arg(target);
    Ok(command)
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
    let mut clone = probe_clone_command(shell_env, remote_url, &scratch.0)?;
    let output = run::run(&mut clone, Limits::deadline(PROBE_TIMEOUT), None)
        .map_err(|stopped| unreachable(&stopped.to_string()))?;
    if !output.status.success() {
        return Err(unreachable(String::from_utf8_lossy(&output.stderr).trim()));
    }
    let mut list = probe_list_command(shell_env, &scratch.0)?;
    let listed = run::run(&mut list, Limits::deadline(LIST_TIMEOUT), None)
        .map_err(BoundedFailure::into_error)
        .context("running `git ls-tree`")?;
    // Any failure to list the tip answers "no agent": the clone has just succeeded, so the remote
    // is reachable, and the usual cause is an empty repository, whose `HEAD` is unborn.
    if !listed.status.success() {
        return Ok(None);
    }
    let listing = String::from_utf8_lossy(&listed.stdout);
    Ok(detect_agent_from_names(listing.lines()))
}

/// The fetch of [`probe_remote_agent`], into `scratch`.
fn probe_clone_command(
    shell_env: &HashMap<String, String>,
    remote_url: &str,
    scratch: &Path,
) -> Result<std::process::Command> {
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
        .arg(scratch);
    Ok(clone)
}

/// The listing of the tip [`probe_remote_agent`] fetched into `scratch`.
fn probe_list_command(
    shell_env: &HashMap<String, String>,
    scratch: &Path,
) -> Result<std::process::Command> {
    let mut list = git_command(shell_env)?;
    list.args(["ls-tree", "--name-only", "HEAD"])
        .current_dir(scratch);
    Ok(list)
}

/// The repository name from a clone URL, for both `https://host/owner/repo(.git)` and
/// `git@host:owner/repo(.git)`. `None` for a name that is no directory of its own (empty, `.`,
/// `..`), which a clone beneath a parent would otherwise land in or above.
pub(crate) fn repo_name(remote_url: &str) -> Option<String> {
    let trimmed = remote_url.trim().trim_end_matches('/');
    let last = trimmed.rsplit(['/', ':']).next()?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    if matches!(name, "" | "." | "..") {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

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

    /// A login shell exporting `GIT_DIR` and the like must not point the probe at another
    /// repository: here one set up for Codex, while the remote probed is set up for Claude.
    #[test]
    fn probe_ignores_git_variables_that_redirect_it() {
        let ours = crate::test_support::repo_with("hostfs-probe-ours", &[(b"CLAUDE.md", b"")]);
        let theirs = crate::test_support::repo_with("hostfs-probe-theirs", &[(b"AGENTS.md", b"")]);
        let mut env = crate::test_support::isolated_git_env();
        env.extend(
            crate::test_support::redirecting_overrides(&theirs)
                .into_iter()
                .map(|(key, value)| (key.to_string(), value)),
        );
        let url = format!("file://{}", ours.display());
        assert_eq!(probe_remote_agent(&url, &env).unwrap(), Some(Agent::Claude));
    }

    /// The command lines of a clone and of a probe's two steps, pinned whole: none gets an index
    /// guard or a leading option, the clones run wherever the daemon is and the listing in the
    /// probe's scratch directory.
    #[test]
    fn a_clone_and_a_probe_build_their_command_lines() {
        use crate::test_support::{pinned_shell_env, CommandLine};

        let env = pinned_shell_env();
        let git = crate::env_shell::resolve_binary("git", &env).expect("a git on the pinned PATH");
        let url = "https://example.com/r.git";
        let clone = super::clone_command(&env, url, Path::new("/parent/r")).unwrap();
        assert_eq!(
            CommandLine::of(&clone),
            CommandLine::pinned(&git, &["clone", "--", url, "/parent/r"], &[], None)
        );
        let probe = super::probe_clone_command(&env, url, Path::new("/scratch")).unwrap();
        assert_eq!(
            CommandLine::of(&probe),
            CommandLine::pinned(
                &git,
                &[
                    "clone",
                    "--depth",
                    "1",
                    "--filter=blob:none",
                    "--no-checkout",
                    "--quiet",
                    "--",
                    url,
                    "/scratch"
                ],
                &[],
                None
            )
        );
        let list = super::probe_list_command(&env, Path::new("/scratch")).unwrap();
        assert_eq!(
            CommandLine::of(&list),
            CommandLine::pinned(
                &git,
                &["ls-tree", "--name-only", "HEAD"],
                &[],
                Some("/scratch")
            )
        );
    }

    #[test]
    fn repo_name_reads_both_url_shapes_and_refuses_dot_names() {
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
        assert_eq!(repo_name("https://host/owner/..git").as_deref(), None);
        assert_eq!(repo_name("https://host/owner/...git").as_deref(), None);
    }
}
