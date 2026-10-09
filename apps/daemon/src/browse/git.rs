//! The one way browse reads run `git`: isolated from anything in the environment or the
//! configuration that could point it at another repository, worktree or index, or run a helper
//! program in the middle of a read, and bounded in time and output.
//!
//! The environment starts from the user's own shell environment (so it is their `git`, their
//! global configuration and their `safe.directory` list), but with every `GIT_*` variable removed
//! apart from the three that only say where the user's own configuration files are. That takes out
//! `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`, `GIT_OBJECT_DIRECTORY`, `GIT_COMMON_DIR`,
//! `GIT_CONFIG_PARAMETERS` and the rest of what git(1)'s "Environment Variables" lists as
//! redirecting a repository, wholesale rather than by a list that a later `git` release would
//! outgrow. What browse then sets on top of the shared non-interactive, read-only base is fixed:
//!
//! - `GIT_LITERAL_PATHSPECS=1`: a path is a path; `*`, `?`, `[` and `:(magic)` in a filename are
//!   never patterns. [`GitEnv::run_exact`] is the one exception: it spells its paths out with
//!   `:(literal)` magic itself, to keep what lies below them out.
//! - `GIT_NO_REPLACE_OBJECTS=1`: an object id names the object stored under it, not a replacement,
//!   so the id a reply reports is the content it carries.
//! - `GIT_NO_LAZY_FETCH=1`: a partial clone's missing object fails the read instead of reaching
//!   the network in the middle of it.
//! - `GIT_CEILING_DIRECTORIES` set to the home directory, so discovery never climbs into it: a
//!   home directory kept under version control is never taken for the repository of a project
//!   below it.
//!
//! That base is what leaves a command unable to prompt for anything nobody is there to answer,
//! and unable to write back to the index of the repository it reads and fail an agent's `git add`
//! on the lock it would hold.
//!
//! Every command also gets `--no-pager`, `-c core.fsmonitor=false` (a configured monitor is a
//! program `git` would run on a read) and `-c core.quotePath=false` (a patch's headers name a path
//! by its bytes, apart from the few characters `git` always escapes, whatever the user
//! configured), alongside the `-c` the base contributes — all of them before the subcommand, which
//! is where a `-c` has to stand. A diff-family command (`diff*`, `log`, `show`) gets
//! `--no-ext-diff --no-textconv --no-color` right after its name: an external diff or a textconv
//! filter is a configured program that would both run and rewrite the bytes the reply claims to
//! carry. Clean and smudge filters
//! are left alone — they define what a file's content is in the repository, which is not a
//! presentation choice. Output is read as bytes, never as text.

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

/// Pathspecs that match each of `paths` exactly: the path itself, literally, and nothing below
/// it (`<path>/**` excluded, the path's own glob characters escaped). A plain path would also match
/// everything under it when it is a directory on one side — a file replaced by a directory — and
/// excluding `<path>/` instead would exclude a submodule at the path itself, which a pathspec takes
/// for a directory. `git` applies an exclusion to every path of the command, so a path with another
/// of `paths` below it (a rename from `foo` to `foo/bar`) gets none: that would take the other path
/// out too. What then lies below it is matched as well, and the caller sorts it out.
fn exact_pathspecs(paths: &[&[u8]]) -> Vec<Vec<u8>> {
    let mut specs: Vec<Vec<u8>> = Vec::new();
    for path in paths {
        let exact = [b":(literal)".as_slice(), path].concat();
        if specs.contains(&exact) {
            continue;
        }
        specs.push(exact);
        if holds_another(path, paths) {
            continue;
        }
        let mut below = b":(exclude,glob)".to_vec();
        for &byte in *path {
            if matches!(byte, b'*' | b'?' | b'[' | b']' | b'\\') {
                below.push(b'\\');
            }
            below.push(byte);
        }
        below.extend_from_slice(b"/**");
        specs.push(below);
    }
    specs
}

/// Whether another of `paths` lies below `path`.
fn holds_another(path: &[u8], paths: &[&[u8]]) -> bool {
    paths.iter().any(|other| {
        other.len() > path.len() + 1 && other.starts_with(path) && other[path.len()] == b'/'
    })
}

/// Whether one of `paths` lies below another (a rename from `foo` to `foo/bar`): the one case
/// [`exact_pathspecs`] cannot keep what lies below a path out of a command.
pub(crate) fn paths_nest(paths: &[&[u8]]) -> bool {
    paths.iter().any(|path| holds_another(path, paths))
}

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
    /// The exit status, when `git` exited rather than being killed by a signal.
    pub code: Option<i32>,
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

        let kept = self.env.iter().filter(|(key, _)| {
            !key.starts_with("GIT_") || KEPT_GIT_VARIABLES.contains(&key.as_str())
        });
        let mut command = crate::git_env::non_interactive(&self.git, kept);
        command.env("GIT_LITERAL_PATHSPECS", "1");
        command.env("GIT_NO_REPLACE_OBJECTS", "1");
        command.env("GIT_NO_LAZY_FETCH", "1");
        if let Some(ceiling) = &self.ceiling {
            command.env("GIT_CEILING_DIRECTORIES", ceiling);
        }
        command.current_dir(dir);
        command.args([
            "--no-pager",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.quotePath=false",
        ]);
        crate::git_env::read_only(&mut command);
        command.arg(subcommand);
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
        self.run_command(
            &mut self.command(dir, subcommand, args),
            stdout_limit,
            cancel,
        )
    }

    /// Runs `command`, built by [`GitEnv::command`] and then adjusted, as [`GitEnv::run`] does.
    pub fn run_command(
        &self,
        command: &mut Command,
        stdout_limit: usize,
        cancel: &AtomicBool,
    ) -> Result<Vec<u8>, GitError> {
        let bounds = Bounds {
            timeout: budget::GIT_READ_TIMEOUT,
            stdout_limit,
            stderr_limit: budget::GIT_STDERR,
        };
        let output =
            subprocess::run_bounded(command, &bounds, cancel).map_err(GitError::Stopped)?;
        if !output.status.success() {
            return Err(GitError::Failed(GitFailed {
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
                code: output.status.code(),
            }));
        }
        Ok(output.stdout)
    }

    /// Runs `git <subcommand> <args> -- <paths>` with each of `paths` (relative to the repository's
    /// root) matched exactly and nothing below it, as [`exact_pathspecs`] writes them — except where
    /// `paths` nest ([`paths_nest`]): there everything below the outer path is matched too, and the
    /// caller keeps only what is its own (an index lookup by its exact match, a patch by its
    /// sections).
    pub fn run_exact(
        &self,
        dir: &Path,
        subcommand: &str,
        args: &[&[u8]],
        paths: &[&[u8]],
        stdout_limit: usize,
        cancel: &AtomicBool,
    ) -> Result<Vec<u8>, GitError> {
        let pathspecs = exact_pathspecs(paths);
        let mut all = args.to_vec();
        all.push(b"--");
        all.extend(pathspecs.iter().map(Vec::as_slice));
        let mut command = self.command(dir, subcommand, &all);
        // The magic below spells out literal matching itself; under `GIT_LITERAL_PATHSPECS` it
        // would be read as part of the name.
        command.env_remove("GIT_LITERAL_PATHSPECS");
        self.run_command(&mut command, stdout_limit, cancel)
    }

    #[cfg(test)]
    pub fn set_ceiling(&mut self, ceiling: Option<std::path::PathBuf>) {
        self.ceiling = ceiling;
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::test_support::{git, isolated_git_env, repo_with, SYSTEM_GIT};

    /// A `GitEnv` over [`isolated_git_env`] plus `extra`, running [`SYSTEM_GIT`].
    ///
    /// The two variables that keep the developer's own configuration out reach `git` only because
    /// [`KEPT_GIT_VARIABLES`] holds them; every other `GIT_*` of theirs is dropped on the way, so
    /// taking either off that list would quietly put their configuration back into every read a
    /// test makes.
    pub(in crate::browse) fn test_env(extra: &[(&str, &str)]) -> GitEnv {
        let mut env = isolated_git_env();
        for (key, value) in extra {
            env.insert(key.to_string(), value.to_string());
        }
        GitEnv::new(SYSTEM_GIT.to_string(), env)
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
        let redirected = Command::new(SYSTEM_GIT)
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
