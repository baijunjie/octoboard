//! The `git` that reads and changes the repositories behind sessions' branches, and the worktree
//! reads built on it. Every command is built and run through `crate::git::run`, with a deadline.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;

use crate::git::isolated::IsolatedGit;
use crate::git::run::{self, Access, GitError, Limits, Policy, UserGit, WorkDir};
use crate::git::worktree_list::parse_worktree_list;

/// How long one `git` here may take. A forced removal deletes a whole worktree.
const GIT_TIMEOUT: Duration = Duration::from_secs(120);

/// No pager, and no configured filesystem monitor, which is a program `git` would start on every
/// status read.
const UNMONITORED: Policy = Policy::new(&["--no-pager", "-c", "core.fsmonitor=false"], &[]);

/// The `git` the branch reads and the cleanup run: the user's own, under [`UNMONITORED`].
pub struct BranchGit(UserGit);

impl BranchGit {
    /// The user's own `git`, as their login shell's `PATH` resolves it, in that shell's
    /// environment.
    pub(crate) fn of_user_shell() -> Result<Self> {
        UserGit::cached_shell().map(Self)
    }

    #[cfg(test)]
    pub(crate) fn new(
        program: impl Into<String>,
        env: std::collections::HashMap<String, String>,
    ) -> Self {
        Self(UserGit::new(program, env))
    }

    /// The same `git` and environment as the browse reads take them.
    pub(crate) fn browse_env(&self) -> IsolatedGit {
        IsolatedGit::of(self.0.clone())
    }

    /// Runs `git <args>` in `dir` and returns its stdout; a read leaves the index alone. The error
    /// is git's own message, or why it could not run.
    fn run(&self, dir: &Path, args: &[&OsStr], access: Access) -> Result<Vec<u8>, String> {
        let mut command = self.command(dir, args, access);
        run::run_checked(&mut command, Limits::deadline(GIT_TIMEOUT), None)
            .map_err(GitError::into_message)
    }

    /// The `git <args>` command [`BranchGit::run`] runs in `dir`.
    fn command(&self, dir: &Path, args: &[&OsStr], access: Access) -> std::process::Command {
        let mut command = self.0.command(&UNMONITORED, access, WorkDir::At(dir));
        command.args(args);
        command
    }

    pub(crate) fn read(&self, dir: &Path, args: &[&OsStr]) -> Result<Vec<u8>, String> {
        self.run(dir, args, Access::Read)
    }

    pub(crate) fn write(&self, dir: &Path, args: &[&OsStr]) -> Result<(), String> {
        self.run(dir, args, Access::Write).map(|_| ())
    }
}

/// One worktree of the repository, as `git worktree list` gives it.
pub(crate) struct Listed {
    pub(crate) root: PathBuf,
    pub(crate) branch: Option<Vec<u8>>,
    pub(crate) main: bool,
    pub(crate) locked: bool,
    /// Its directory is gone, and only git's record of it is left.
    pub(crate) prunable: bool,
}

/// The repository's worktrees, the main one first.
pub(crate) fn worktrees(git: &BranchGit, repo: &Path) -> Result<Vec<Listed>, String> {
    let stdout = git.read(
        repo,
        &[
            OsStr::new("worktree"),
            OsStr::new("list"),
            OsStr::new("--porcelain"),
            OsStr::new("-z"),
        ],
    )?;
    Ok(parse_worktree_list(&stdout)
        .iter()
        .enumerate()
        .filter(|(_, record)| !record.bare)
        .map(|(at, record)| {
            let path = Path::new(OsStr::from_bytes(&record.path));
            Listed {
                root: std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
                branch: record.branch.clone(),
                main: at == 0,
                locked: record.locked,
                prunable: record.prunable,
            }
        })
        .collect())
}

/// Whether the worktree at `root` holds anything `git worktree remove` refuses to discard:
/// modified tracked files or untracked ones, the same `git status` it asks itself.
pub(crate) fn changes(git: &BranchGit, root: &Path) -> Result<bool, String> {
    let stdout = git.read(
        root,
        &[
            OsStr::new("status"),
            OsStr::new("--porcelain"),
            OsStr::new("-z"),
            OsStr::new("--ignore-submodules=none"),
            OsStr::new("--untracked-files=normal"),
        ],
    )?;
    Ok(!stdout.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{pinned_shell_env, CommandLine};

    /// The command line of a read and of a write, pinned whole: neither pages or runs a monitor,
    /// and only the read gets the index guard.
    #[test]
    fn a_read_and_a_write_build_their_command_lines() {
        let git = BranchGit::new("/opt/git", pinned_shell_env());
        let args = [OsStr::new("worktree"), OsStr::new("list")];
        assert_eq!(
            CommandLine::of(&git.command(Path::new("/repo"), &args, Access::Read)),
            CommandLine::pinned(
                "/opt/git",
                &[
                    "--no-pager",
                    "-c",
                    "core.fsmonitor=false",
                    "-c",
                    "diff.autoRefreshIndex=false",
                    "worktree",
                    "list"
                ],
                &[("GIT_OPTIONAL_LOCKS", "0")],
                Some("/repo")
            )
        );
        let args = [OsStr::new("worktree"), OsStr::new("prune")];
        assert_eq!(
            CommandLine::of(&git.command(Path::new("/repo"), &args, Access::Write)),
            CommandLine::pinned(
                "/opt/git",
                &[
                    "--no-pager",
                    "-c",
                    "core.fsmonitor=false",
                    "worktree",
                    "prune"
                ],
                &[],
                Some("/repo")
            )
        );
    }
}
