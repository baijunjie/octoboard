//! The one way browse reads run `git`: isolated from anything in the environment or the
//! configuration that could point it at another repository, worktree or index, or run a helper
//! program in the middle of a read, and bounded in time and output.
//!
//! The environment starts from the user's own shell environment (so it is their `git`, their
//! global configuration and their `safe.directory` list), but with every `GIT_*` variable removed
//! apart from the few that say which of the user's own material to use (see
//! [`crate::git::env::KEPT_GIT_VARIABLES`]): their configuration files, and the certificates and
//! proxy helper a connection to a remote uses. Those reach `git` but do nothing here, since a
//! browse read never connects to a remote. That takes out
//! `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`, `GIT_OBJECT_DIRECTORY`, `GIT_COMMON_DIR`,
//! `GIT_CONFIG_PARAMETERS` and the rest of what git(1)'s "Environment Variables" lists as
//! redirecting a repository, wholesale rather than by a list that a later `git` release would
//! outgrow. What browse then sets on top of the shared non-interactive, read-only base is fixed:
//!
//! - `GIT_LITERAL_PATHSPECS=1`: a path is a path; `*`, `?`, `[` and `:(magic)` in a filename are
//!   never patterns. [`IsolatedGit::run_exact`] is the one exception: it spells its paths out with
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
//! carry. A `grep` gets the last two of those (`--no-textconv --no-color`), the only ones it takes.
//! Clean and smudge filters are left alone — they define what a file's content is in the
//! repository, which is not a presentation choice. Output is read as bytes, never as text.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use anyhow::Result;

use crate::browse::budget;
use crate::git::run::{self, Access, GitError, Limits, Policy, UserGit, WorkDir};

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
///
/// `grep` is one too (its `--textconv` option is what lets a driver change what it matches), but
/// it accepts only two of [`DIFF_SAFETY`]'s flags, not `--no-ext-diff`, so it cannot join this
/// list: [`IsolatedGit::command`] gives it [`GREP_SAFETY`] instead. A new subcommand of this kind
/// belongs in one of the two.
fn is_diff_family(subcommand: &str) -> bool {
    subcommand.starts_with("diff") || subcommand == "log" || subcommand == "show"
}

/// The flags that keep a diff-family command's output to what is stored: no external diff, no
/// textconv, no colour escapes.
const DIFF_SAFETY: &[&str] = &["--no-ext-diff", "--no-textconv", "--no-color"];

/// [`DIFF_SAFETY`] as far as `git grep` takes it. `--no-color` is the one that matters: with
/// `color.grep` set to `always`, the object id and the `:` before a count come wrapped in escape
/// sequences, which no parse of `<id>:<count>` survives. `--no-textconv` is `git grep`'s documented
/// default and only guards against that changing.
const GREP_SAFETY: &[&str] = &["--no-textconv", "--no-color"];

/// The fixed part of every isolated command: the leading options and variables the module
/// header describes. The ceiling depends on the home directory, and is set on its own.
const ISOLATED: Policy = Policy::new(
    &[
        "--no-pager",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.quotePath=false",
    ],
    &[
        ("GIT_LITERAL_PATHSPECS", "1"),
        ("GIT_NO_REPLACE_OBJECTS", "1"),
        ("GIT_NO_LAZY_FETCH", "1"),
    ],
);

/// The user's `git` a browse read runs, and the directory its discovery never climbs into.
#[derive(Debug, Clone)]
pub struct IsolatedGit {
    git: UserGit,
    ceiling: Option<std::path::PathBuf>,
}

impl IsolatedGit {
    /// The user's `git` and shell environment, from the cached login-shell snapshot every
    /// repeating `git` caller uses.
    pub fn from_shell() -> Result<Self> {
        UserGit::cached_shell().map(Self::of)
    }

    #[cfg(test)]
    pub fn new(git: String, env: std::collections::HashMap<String, String>) -> Self {
        Self::of(UserGit::new(git, env))
    }

    /// Browse reads over `git`.
    pub fn of(git: UserGit) -> Self {
        Self {
            git,
            ceiling: crate::git::layout::home_ceiling(),
        }
    }

    /// The command `git <subcommand> <args>` in `dir`, isolated as the module doc describes.
    pub fn command(&self, dir: &Path, subcommand: &str, args: &[&[u8]]) -> Command {
        self.command_configured(dir, &[], subcommand, args)
    }

    /// [`IsolatedGit::command`] with each of `config` (`key=value`) set by a `-c` of its own, for
    /// the one command that needs a setting the others must not have.
    pub fn command_configured(
        &self,
        dir: &Path,
        config: &[&str],
        subcommand: &str,
        args: &[&[u8]],
    ) -> Command {
        use std::os::unix::ffi::OsStrExt;

        let mut command = self.git.command(&ISOLATED, Access::Read, WorkDir::At(dir));
        if let Some(ceiling) = &self.ceiling {
            command.env("GIT_CEILING_DIRECTORIES", ceiling);
        }
        for item in config {
            command.args(["-c", item]);
        }
        command.arg(subcommand);
        if is_diff_family(subcommand) {
            command.args(DIFF_SAFETY);
        } else if subcommand == "grep" {
            command.args(GREP_SAFETY);
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
        self.run_within(
            dir,
            subcommand,
            args,
            stdout_limit,
            budget::GIT_READ_TIMEOUT,
            cancel,
        )
    }

    /// [`IsolatedGit::run`] with a deadline of `timeout` instead of the browse reads' own.
    pub fn run_within(
        &self,
        dir: &Path,
        subcommand: &str,
        args: &[&[u8]],
        stdout_limit: usize,
        timeout: Duration,
        cancel: &AtomicBool,
    ) -> Result<Vec<u8>, GitError> {
        self.run_command(
            &mut self.command(dir, subcommand, args),
            stdout_limit,
            timeout,
            cancel,
        )
    }

    /// Runs `command`, built by [`IsolatedGit::command`] and then adjusted, as [`IsolatedGit::run`]
    /// does, within `timeout`.
    pub fn run_command(
        &self,
        command: &mut Command,
        stdout_limit: usize,
        timeout: Duration,
        cancel: &AtomicBool,
    ) -> Result<Vec<u8>, GitError> {
        let limits = Limits {
            timeout,
            stdout: stdout_limit,
            stderr: budget::GIT_STDERR,
        };
        run::run_checked(command, limits, Some(cancel))
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
        let mut command = self.exact_command(dir, subcommand, args, paths);
        self.run_command(&mut command, stdout_limit, budget::GIT_READ_TIMEOUT, cancel)
    }

    /// The command [`IsolatedGit::run_exact`] runs.
    fn exact_command(
        &self,
        dir: &Path,
        subcommand: &str,
        args: &[&[u8]],
        paths: &[&[u8]],
    ) -> Command {
        let pathspecs = exact_pathspecs(paths);
        let mut all = args.to_vec();
        all.push(b"--");
        all.extend(pathspecs.iter().map(Vec::as_slice));
        let mut command = self.command(dir, subcommand, &all);
        // The magic below spells out literal matching itself; under `GIT_LITERAL_PATHSPECS` it
        // would be read as part of the name.
        command.env_remove("GIT_LITERAL_PATHSPECS");
        command
    }

    #[cfg(test)]
    pub fn set_ceiling(&mut self, ceiling: Option<std::path::PathBuf>) {
        self.ceiling = ceiling;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::test_support::{
        git, isolated_git_env, redirecting_overrides, repo_pair, repo_with, CommandLine, SYSTEM_GIT,
    };

    /// A `IsolatedGit` over [`isolated_git_env`] plus `extra`, running [`SYSTEM_GIT`].
    ///
    /// The two variables that keep the developer's own configuration out reach `git` only because
    /// [`crate::git::env::KEPT_GIT_VARIABLES`] holds them; every other `GIT_*` of theirs is dropped
    /// on the way, so taking either off that list would quietly put their configuration back into
    /// every read a test makes.
    pub(crate) fn test_env(extra: &[(&str, &str)]) -> IsolatedGit {
        let mut env = isolated_git_env();
        for (key, value) in extra {
            env.insert(key.to_string(), value.to_string());
        }
        IsolatedGit::new(SYSTEM_GIT.to_string(), env)
    }

    fn never() -> AtomicBool {
        AtomicBool::new(false)
    }

    /// The leading options every isolated command gets, the index guard's `-c` included.
    const LEADING: [&str; 7] = [
        "--no-pager",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.quotePath=false",
        "-c",
        "diff.autoRefreshIndex=false",
    ];

    /// What the isolation sets on top of the base environment, with the ceiling at `/home`.
    const ISOLATION: [(&str, &str); 5] = [
        ("GIT_LITERAL_PATHSPECS", "1"),
        ("GIT_NO_REPLACE_OBJECTS", "1"),
        ("GIT_NO_LAZY_FETCH", "1"),
        ("GIT_CEILING_DIRECTORIES", "/home"),
        ("GIT_OPTIONAL_LOCKS", "0"),
    ];

    fn pinned_env() -> IsolatedGit {
        let mut env = IsolatedGit::new(
            "/opt/git".to_string(),
            crate::test_support::pinned_shell_env(),
        );
        env.set_ceiling(Some("/home".into()));
        env
    }

    fn pinned(args: &[&str], tail: &[&str], without: &[&str]) -> CommandLine {
        let all: Vec<&str> = LEADING.iter().chain(args).chain(tail).copied().collect();
        let extra: Vec<(&str, &str)> = ISOLATION
            .into_iter()
            .filter(|(key, _)| !without.contains(key))
            .collect();
        CommandLine::pinned("/opt/git", &all, &extra, Some("/repo"))
    }

    /// The command line of each kind of isolated command, pinned whole: a diff-family command and
    /// a `grep` with their safety flags right after the subcommand, a plain one without, a
    /// configured one with its `-c` after the guard's, and an exact-path one out of the
    /// literal-pathspec mode.
    #[test]
    fn each_kind_of_command_builds_its_command_line() {
        let env = pinned_env();
        let dir = Path::new("/repo");
        let line = |command: &Command| CommandLine::of(command);
        assert_eq!(
            line(&env.command(dir, "diff", &[b"--cached", b"--", b"a"])),
            pinned(
                &["diff", "--no-ext-diff", "--no-textconv", "--no-color"],
                &["--cached", "--", "a"],
                &[]
            )
        );
        assert_eq!(
            line(&env.command(dir, "grep", &[b"-c", b"x"])),
            pinned(&["grep", "--no-textconv", "--no-color"], &["-c", "x"], &[])
        );
        assert_eq!(
            line(&env.command(dir, "ls-files", &[b"-z"])),
            pinned(&["ls-files"], &["-z"], &[])
        );
        let mut configured = env.command_configured(
            dir,
            &["core.precomposeUnicode=false"],
            "check-ignore",
            &[b"-q", b"--", b"./"],
        );
        configured.env_remove("GIT_LITERAL_PATHSPECS");
        assert_eq!(
            line(&configured),
            pinned(
                &["-c", "core.precomposeUnicode=false", "check-ignore"],
                &["-q", "--", "./"],
                &["GIT_LITERAL_PATHSPECS"]
            )
        );
        assert_eq!(
            line(&env.exact_command(dir, "diff", &[b"-z"], &[b"a*"])),
            pinned(
                &["diff", "--no-ext-diff", "--no-textconv", "--no-color"],
                &["-z", "--", ":(literal)a*", ":(exclude,glob)a\\*/**"],
                &["GIT_LITERAL_PATHSPECS"]
            )
        );
        let mut unbounded = pinned_env();
        unbounded.set_ceiling(None);
        assert_eq!(
            line(&unbounded.command(dir, "ls-files", &[])),
            pinned(&["ls-files"], &[], &["GIT_CEILING_DIRECTORIES"])
        );
    }

    #[test]
    fn environment_overrides_cannot_redirect_a_read_to_another_repository() {
        let (ours, theirs) = repo_pair("git");
        let overrides = redirecting_overrides(&theirs);
        let overrides: Vec<(&str, &str)> = overrides
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect();
        // The positive control: plain `git` with these variables reads the other repository.
        let redirected = Command::new(SYSTEM_GIT)
            .current_dir(&*ours)
            .envs(overrides.iter().copied())
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
