//! Cloning a repository for a new project, and probing a remote for the names at its top level
//! without a checkout.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::git::run::{Access, GitError, Limits, Policy, Runner, UserGit, WorkDir};
use crate::protocol::{error_code, CodedError};

/// How long a clone may run before it is given up on. Generous — a large repository over a slow
/// connection legitimately takes minutes — but bounded, so a stalled transfer cannot hold a
/// blocking thread for the rest of the daemon's life.
const CLONE_LIMITS: Limits = Limits::deadline(std::time::Duration::from_secs(30 * 60));

/// How long the fetch of a probe may run. Much shorter than a clone's: it fetches one commit and
/// its trees, no file contents, and the user is waiting on it with a form held shut.
const PROBE_LIMITS: Limits = Limits::deadline(std::time::Duration::from_secs(60));

/// How long listing the fetched tip may run; it reads a local repository.
const LIST_LIMITS: Limits = Limits::deadline(std::time::Duration::from_secs(10));

/// The `git` for the user's remote work, run with `shell_env`, the user's shell environment that
/// agents are launched with, rather than the daemon's own: a daemon started from Finder has a
/// minimal `PATH` with no `git` on it, and its own environment may carry the agent-session markers
/// that environment exists to filter out. Only the user's own git configuration is taken from it,
/// as for every `git` the daemon runs. Made non-interactive, so a missing credential or an unknown
/// host key fails at once instead of waiting on a terminal nobody is watching, under the plain
/// policy and `limits`.
fn remote_work_git(shell_env: &HashMap<String, String>, limits: Limits) -> Result<Runner> {
    Ok(Runner::new(
        UserGit::from_env(shell_env.clone())?,
        Policy::PLAIN,
        limits,
    ))
}

/// The `git clone <args>` that `git` runs, which reaches the remote with the user's own ssh
/// (`UserGit::remote_command`), in the daemon's own working directory, as it names every path it
/// works on. A clone reads no repository's configuration, only the system and global ones, so
/// `core.sshCommand` is read from those: by a `git config` run in `/`, outside any repository,
/// rather than wherever the daemon happens to be.
fn clone_with(git: &Runner, args: &[&OsStr]) -> std::process::Command {
    let ssh = git.git().ssh_configuration(Path::new("/"));
    let mut command = git.remote_command(
        Access::Write,
        WorkDir::Inherited,
        [OsStr::new("clone")],
        &ssh,
    );
    command.args(args);
    command
}

/// Clones `remote_url` into a new directory under `parent` and returns that directory. Blocking:
/// the caller runs it off the runtime.
///
/// Runs as [`remote_work_git`] with a deadline far longer than a probe's, because a clone
/// legitimately runs for minutes.
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
    let git = remote_work_git(&shell_env, CLONE_LIMITS)?;
    match git.run(&mut clone_command(&git, remote_url, &target)) {
        Ok(_) => Ok(target),
        Err(GitError::Exited { stderr, .. }) => Err(CodedError::raised(
            error_code::GIT_CLONE_FAILED,
            format!("`git clone` failed: {stderr}"),
            &[("detail", &stderr)],
        )),
        Err(GitError::Stopped(stopped)) => Err(stopped.into_error().context("running `git clone`")),
    }
}

/// The `git clone` [`clone_repo`] runs.
fn clone_command(git: &Runner, remote_url: &str, target: &Path) -> std::process::Command {
    clone_with(
        git,
        &[OsStr::new("--"), OsStr::new(remote_url), target.as_os_str()],
    )
}

/// A scratch directory for one probe, removed when the probe ends.
struct ProbeDir(PathBuf);

impl Drop for ProbeDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// Checks that `remote_url` can be reached with the user's credentials and returns the names at its
/// top level, running `git` as [`remote_work_git`] does with `shell_env`. Blocking: the caller runs
/// it off the runtime.
///
/// A shallow, blobless, checkout-less clone into a scratch directory, whose top-level names are
/// then listed: that fetches the tip commit and its trees but no file, and works for any remote
/// (`git ls-remote` would show refs only). A remote that does not support filtering sends the
/// whole tip commit instead, which is still no history. A repository that cannot be read, or
/// whose fetch outlasts [`PROBE_LIMITS`], is `git_remote_unreachable`. An empty repository is
/// reachable and has no names.
pub fn probe_remote_names(
    remote_url: &str,
    shell_env: &HashMap<String, String>,
) -> Result<Vec<String>> {
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
    let git = remote_work_git(shell_env, PROBE_LIMITS)?;
    git.run(&mut probe_clone_command(&git, remote_url, &scratch.0))
        .map_err(|err| unreachable(&err.to_string()))?;
    let list = git.with_limits(LIST_LIMITS);
    match list.run(&mut probe_list_command(&list, &scratch.0)) {
        Ok(stdout) => Ok(String::from_utf8_lossy(&stdout)
            .lines()
            .map(str::to_owned)
            .collect()),
        // Any failure to list the tip answers "no names": the clone has just succeeded, so the
        // remote is reachable, and the usual cause is an empty repository, whose `HEAD` is unborn.
        Err(GitError::Exited { .. }) => Ok(Vec::new()),
        Err(GitError::Stopped(stopped)) => {
            Err(stopped.into_error().context("running `git ls-tree`"))
        }
    }
}

/// The fetch of [`probe_remote_names`], into `scratch`.
fn probe_clone_command(git: &Runner, remote_url: &str, scratch: &Path) -> std::process::Command {
    clone_with(
        git,
        &[
            OsStr::new("--depth"),
            OsStr::new("1"),
            OsStr::new("--filter=blob:none"),
            OsStr::new("--no-checkout"),
            OsStr::new("--quiet"),
            OsStr::new("--"),
            OsStr::new(remote_url),
            scratch.as_os_str(),
        ],
    )
}

/// The listing of the tip [`probe_remote_names`] fetched into `scratch`, which reaches no remote.
fn probe_list_command(git: &Runner, scratch: &Path) -> std::process::Command {
    git.command(
        Access::Write,
        WorkDir::At(scratch),
        ["ls-tree", "--name-only", "HEAD"],
    )
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
        assert_eq!(probe_remote_names(&url, &env).unwrap(), ["CLAUDE.md"]);
        let empty = format!("file://{}", dir.join("empty.git").display());
        assert!(probe_remote_names(&empty, &env).unwrap().is_empty());

        let missing = format!("file://{}", dir.join("missing").display());
        let err = probe_remote_names(&missing, &env).unwrap_err();
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
        assert_eq!(probe_remote_names(&url, &env).unwrap(), ["CLAUDE.md"]);
    }

    /// The command lines of a clone and of a probe's two steps, pinned whole: none gets an index
    /// guard or a leading option beyond the base ones, the clones run wherever the daemon is and
    /// the listing in the probe's scratch directory.
    #[test]
    fn a_clone_and_a_probe_build_their_command_lines() {
        use crate::test_support::{pinned_shell_env, CommandLine};

        let env = pinned_shell_env();
        let git = crate::env_shell::resolve_binary("git", &env).expect("a git on the pinned PATH");
        let runner = remote_work_git(&env, CLONE_LIMITS).unwrap();
        let url = "https://example.com/r.git";
        let based = |args: &[&'static str]| {
            let mut all = crate::git::env::BASE_OPTIONS.to_vec();
            all.extend_from_slice(args);
            all
        };
        let clone = super::clone_command(&runner, url, Path::new("/parent/r"));
        assert_eq!(
            CommandLine::of(&clone),
            CommandLine::pinned(&git, &based(&["clone", "--", url, "/parent/r"]), &[], None)
        );
        let probe = super::probe_clone_command(&runner, url, Path::new("/scratch"));
        assert_eq!(
            CommandLine::of(&probe),
            CommandLine::pinned(
                &git,
                &based(&[
                    "clone",
                    "--depth",
                    "1",
                    "--filter=blob:none",
                    "--no-checkout",
                    "--quiet",
                    "--",
                    url,
                    "/scratch"
                ]),
                &[],
                None
            )
        );
        let list = super::probe_list_command(&runner, Path::new("/scratch"));
        assert_eq!(
            CommandLine::of(&list),
            CommandLine::pinned(
                &git,
                &based(&["ls-tree", "--name-only", "HEAD"]),
                &[],
                Some("/scratch")
            )
        );
    }

    /// A clone, with no repository of its own to read, reaches the remote with the ssh the user's
    /// global configuration sets, unable to prompt; the listing after it reaches no remote.
    #[test]
    fn a_clone_runs_the_ssh_command_of_the_users_global_configuration() {
        use crate::test_support::CommandLine;

        let dir = ScratchDir::new("clone-ssh");
        let global = dir.join("gitconfig");
        std::fs::write(&global, "[core]\n\tsshCommand = ssh -i /k\n").unwrap();
        let mut env = crate::test_support::isolated_git_env();
        env.insert(
            "GIT_CONFIG_GLOBAL".to_string(),
            global.to_string_lossy().into_owned(),
        );
        let url = "git@example.invalid:o/r.git";
        let ssh_command = |command: &std::process::Command| {
            CommandLine::of(command).env["GIT_SSH_COMMAND"].clone()
        };
        let expected = Some("ssh -oBatchMode=yes -i /k".to_string());
        let runner = remote_work_git(&env, CLONE_LIMITS).unwrap();
        let clone = super::clone_command(&runner, url, Path::new("/parent/r"));
        assert_eq!(ssh_command(&clone), expected);
        let probe = super::probe_clone_command(&runner, url, Path::new("/scratch"));
        assert_eq!(ssh_command(&probe), expected);
        let list = super::probe_list_command(&runner, Path::new("/scratch"));
        assert_eq!(ssh_command(&list), Some("ssh -oBatchMode=yes".to_string()));
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
