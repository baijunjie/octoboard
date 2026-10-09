//! The one way browse reads run `git`: isolated from anything in the environment or the
//! configuration that could point it at another repository, worktree or index, or run a helper
//! program in the middle of a read, and bounded in time and output.
//!
//! The environment starts from the user's own shell environment, as every other `git` the daemon
//! runs does (so it is their `git`, their global configuration and their `safe.directory` list),
//! but with every `GIT_*` variable removed apart from the three that only say where the user's own
//! configuration files are. That takes out `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`,
//! `GIT_OBJECT_DIRECTORY`, `GIT_COMMON_DIR`, `GIT_CONFIG_PARAMETERS` and the rest of what
//! git(1)'s "Environment Variables" lists as redirecting a repository, wholesale rather than by a
//! list that a later `git` release would outgrow. What is then set is fixed:
//!
//! - `GIT_LITERAL_PATHSPECS=1`: a path is a path; `*`, `?`, `[` and `:(magic)` in a filename are
//!   never patterns.
//! - `GIT_NO_REPLACE_OBJECTS=1`: an object id names the object stored under it, not a replacement,
//!   so the id a reply reports is the content it carries.
//! - `GIT_NO_LAZY_FETCH=1`: a partial clone's missing object fails the read instead of reaching
//!   the network in the middle of it.
//! - `GIT_OPTIONAL_LOCKS=0`: a read never takes the index lock, so it cannot get in the way of an
//!   agent writing to the same repository.
//! - `GIT_TERMINAL_PROMPT=0` and `GIT_SSH_COMMAND=ssh -oBatchMode=yes`, with the askpass helpers
//!   removed, for the reason every daemon `git` has them: nobody can answer a prompt.
//! - `GIT_CEILING_DIRECTORIES` set to the home directory, so discovery never climbs into it: a
//!   home directory kept under version control is never taken for the repository of a project
//!   below it.
//!
//! Every command also gets `--no-pager` and `-c core.fsmonitor=false` (a configured monitor is a
//! program `git` would run on a read), and a diff-family command (`diff*`, `log`, `show`) gets
//! `--no-ext-diff --no-textconv --no-color` right after its name: an external diff or a textconv
//! filter is a configured program that would both run and rewrite the bytes the reply claims to
//! carry. Clean and smudge filters are left alone — they define what a file's content is in the
//! repository, which is not a presentation choice. Output is read as bytes, never as text.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::AtomicBool;

use anyhow::Result;

use crate::browse::budget;
use crate::env_shell;
use crate::subprocess::{self, BoundedFailure, Bounds};

/// The `GIT_*` variables kept from the user's environment: they only say where the user's own
/// configuration lives, and dropping them would read some other configuration instead.
const KEPT_GIT_VARIABLES: &[&str] = &[
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_CONFIG_NOSYSTEM",
];

/// The subcommands whose output a configured diff driver can change.
fn is_diff_family(subcommand: &str) -> bool {
    subcommand.starts_with("diff") || subcommand == "log" || subcommand == "show"
}

/// The flags that keep a diff-family command's output to what is stored: no external diff, no
/// textconv, no colour escapes.
const DIFF_SAFETY: &[&str] = &["--no-ext-diff", "--no-textconv", "--no-color"];

/// The `git` binary and the environment a browse read runs it with, before isolation.
#[derive(Debug, Clone)]
pub struct GitEnv {
    git: String,
    env: HashMap<String, String>,
    ceiling: Option<std::path::PathBuf>,
}

/// A `git` run that ended with a non-zero status; `stderr` is its own message, verbatim.
#[derive(Debug)]
pub struct GitFailed {
    pub stderr: String,
}

/// How a bounded `git` run ended short of an output to use.
#[derive(Debug)]
pub enum GitError {
    /// `git` ran and failed.
    Failed(GitFailed),
    /// The run was stopped: a timeout, too much output, a cancel, or `git` never started.
    Stopped(BoundedFailure),
}

impl std::fmt::Display for GitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(failed) => formatter.write_str(&failed.stderr),
            Self::Stopped(stopped) => write!(formatter, "git {stopped}"),
        }
    }
}

impl GitEnv {
    /// The user's `git` and shell environment, from the cached login-shell snapshot every
    /// repeating `git` caller uses.
    pub fn from_shell() -> Result<Self> {
        let env = env_shell::cached_snapshot()?;
        let git = env_shell::resolve_binary("git", &env)?;
        Ok(Self::new(git, env))
    }

    pub fn new(git: String, env: HashMap<String, String>) -> Self {
        Self {
            git,
            env,
            ceiling: crate::browse::source::home_ceiling(),
        }
    }

    /// The command `git <subcommand> <args>` in `dir`, isolated as the module doc describes.
    pub fn command(&self, dir: &Path, subcommand: &str, args: &[&[u8]]) -> Command {
        use std::os::unix::ffi::OsStrExt;

        let mut command = Command::new(&self.git);
        command.env_clear();
        for (key, value) in &self.env {
            if key.starts_with("GIT_") && !KEPT_GIT_VARIABLES.contains(&key.as_str()) {
                continue;
            }
            if key == "SSH_ASKPASS" {
                continue;
            }
            command.env(key, value);
        }
        command.env("GIT_LITERAL_PATHSPECS", "1");
        command.env("GIT_NO_REPLACE_OBJECTS", "1");
        command.env("GIT_NO_LAZY_FETCH", "1");
        command.env("GIT_OPTIONAL_LOCKS", "0");
        command.env("GIT_TERMINAL_PROMPT", "0");
        command.env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes");
        if let Some(ceiling) = &self.ceiling {
            command.env("GIT_CEILING_DIRECTORIES", ceiling);
        }
        command.current_dir(dir);
        command.args(["--no-pager", "-c", "core.fsmonitor=false", subcommand]);
        if is_diff_family(subcommand) {
            command.args(DIFF_SAFETY);
        }
        for arg in args {
            command.arg(std::ffi::OsStr::from_bytes(arg));
        }
        command
    }

    /// Runs `git <subcommand> <args>` in `dir` to completion, its stdout bounded by
    /// `stdout_limit`, and returns that stdout as bytes.
    pub fn run(
        &self,
        dir: &Path,
        subcommand: &str,
        args: &[&[u8]],
        stdout_limit: usize,
        cancel: &AtomicBool,
    ) -> Result<Vec<u8>, GitError> {
        let mut command = self.command(dir, subcommand, args);
        let bounds = Bounds {
            timeout: budget::GIT_READ_TIMEOUT,
            stdout_limit,
            stderr_limit: budget::GIT_STDERR,
        };
        let output =
            subprocess::run_bounded(&mut command, &bounds, cancel).map_err(GitError::Stopped)?;
        if !output.status.success() {
            return Err(GitError::Failed(GitFailed {
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            }));
        }
        Ok(output.stdout)
    }

    #[cfg(test)]
    pub fn set_ceiling(&mut self, ceiling: Option<std::path::PathBuf>) {
        self.ceiling = ceiling;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    /// A `GitEnv` over this test process's own environment, plus `extra`, with `git` from
    /// `/usr/bin` — the system one, already exec'd countless times, so no fresh-executable cost.
    pub(crate) fn test_env(extra: &[(&str, &str)]) -> GitEnv {
        let mut env: HashMap<String, String> = std::env::vars().collect();
        env.retain(|key, _| !key.starts_with("GIT_"));
        for (key, value) in extra {
            env.insert(key.to_string(), value.to_string());
        }
        GitEnv::new("/usr/bin/git".to_string(), env)
    }

    /// Runs plain `git` in `dir` for fixture setup, failing the test on a non-zero exit. The user's
    /// own configuration is left out, so a hook or a signing setting of theirs cannot get in the
    /// way of a commit.
    pub(crate) fn git(dir: &Path, args: &[&str]) -> Vec<u8> {
        let args: Vec<&[u8]> = args.iter().map(|arg| arg.as_bytes()).collect();
        git_bytes(dir, &args)
    }

    /// [`git`] with arguments that need not be UTF-8.
    pub(crate) fn git_bytes(dir: &Path, args: &[&[u8]]) -> Vec<u8> {
        use std::os::unix::ffi::OsStrExt;
        let args: Vec<&std::ffi::OsStr> = args
            .iter()
            .map(|arg| std::ffi::OsStr::from_bytes(arg))
            .collect();
        let output = Command::new("/usr/bin/git")
            .current_dir(dir)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args([
                "-c",
                "init.defaultBranch=main",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(&args)
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }

    /// A repository in a scratch directory with one commit holding `files`.
    pub(crate) fn repo_with(label: &str, files: &[(&[u8], &[u8])]) -> ScratchDir {
        use std::os::unix::ffi::OsStrExt;

        let dir = ScratchDir::new(label);
        git(&dir, &["init", "-q"]);
        for (name, body) in files {
            let path = dir.join(std::ffi::OsStr::from_bytes(name));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", "fixture"]);
        dir
    }

    fn never() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn environment_overrides_cannot_redirect_a_read_to_another_repository() {
        let ours = repo_with("git-ours", &[(b"a.txt", b"ours\n")]);
        let theirs = repo_with("git-theirs", &[(b"a.txt", b"theirs\n")]);
        let theirs_git = theirs.join(".git");
        let theirs_git = theirs_git.to_str().unwrap();
        let theirs_index = theirs.join(".git/index");
        let overrides = [
            ("GIT_DIR", theirs_git),
            ("GIT_WORK_TREE", theirs.to_str().unwrap()),
            ("GIT_INDEX_FILE", theirs_index.to_str().unwrap()),
            ("GIT_COMMON_DIR", theirs_git),
            ("GIT_CONFIG_PARAMETERS", "'core.worktree'='/'"),
        ];
        // The positive control: plain `git` with these variables reads the other repository.
        let redirected = Command::new("/usr/bin/git")
            .current_dir(&*ours)
            .envs(overrides)
            .args(["cat-file", "blob", ":a.txt"])
            .output()
            .unwrap();
        assert_eq!(redirected.stdout, b"theirs\n");

        let env = test_env(&overrides);
        let read = env
            .run(&ours, "cat-file", &[b"blob", b":a.txt"], 1024, &never())
            .expect("isolated read");
        assert_eq!(read, b"ours\n");
    }

    #[test]
    fn configured_diff_helpers_neither_run_nor_rewrite_the_output() {
        let repo = repo_with(
            "git-helpers",
            &[(b"a.txt", b"one\n"), (b".gitattributes", b"* diff=probe\n")],
        );
        std::fs::write(repo.join("a.txt"), b"two\n").unwrap();
        let marker = |helper: &str| repo.join(format!("{helper}-ran"));
        let configure = |key: &str, helper: &str, then: &str| {
            let command = format!("touch '{}'; {then}", marker(helper).display());
            git(&repo, &["config", key, &command]);
        };
        configure("diff.external", "external", "echo rewritten");
        configure("diff.probe.textconv", "textconv", "cat");
        configure("core.fsmonitor", "fsmonitor", "echo");

        // The positive controls: unisolated, each helper runs — the external diff and the
        // monitor on a plain `git diff`, the textconv filter once the external diff is off.
        git(&repo, &["diff"]);
        git(&repo, &["diff", "--no-ext-diff"]);
        for helper in ["external", "textconv", "fsmonitor"] {
            assert!(marker(helper).exists(), "{helper} never ran unisolated");
            std::fs::remove_file(marker(helper)).unwrap();
        }

        let diff = test_env(&[])
            .run(&repo, "diff", &[b"--", b"a.txt"], 1024 * 1024, &never())
            .expect("isolated diff");
        for helper in ["external", "textconv", "fsmonitor"] {
            assert!(!marker(helper).exists(), "the configured {helper} ran");
        }
        let diff = String::from_utf8(diff).unwrap();
        assert!(diff.contains("-one\n+two\n"), "{diff}");
        assert!(
            !diff.contains("rewritten") && !diff.contains('\x1b'),
            "{diff}"
        );
    }

    #[test]
    fn a_filename_is_never_read_as_a_pathspec_pattern() {
        let repo = repo_with(
            "git-pathspec",
            &[
                (b"a*.txt", b"star\n"),
                (b"ab.txt", b"plain\n"),
                (b":(glob)x", b"magic\n"),
            ],
        );
        let env = test_env(&[]);
        let listed = env
            .run(
                &repo,
                "ls-files",
                &[b"-z", b"--", b"a*.txt"],
                1024,
                &never(),
            )
            .unwrap();
        assert_eq!(listed, b"a*.txt\0");
        let listed = env
            .run(
                &repo,
                "ls-files",
                &[b"-z", b"--", b":(glob)x"],
                1024,
                &never(),
            )
            .unwrap();
        assert_eq!(listed, b":(glob)x\0");
    }
}
