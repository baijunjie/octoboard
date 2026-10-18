//! The `git` that reads and changes the repositories behind sessions' branches, and the worktree
//! reads built on it. Every command is built from `git_env`, with a deadline.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;

use crate::browse::git::GitEnv;
use crate::browse::source::parse_worktree_list;
use crate::env_shell;
use crate::git_env;

/// How long one `git` here may take. A forced removal deletes a whole worktree.
const GIT_TIMEOUT: Duration = Duration::from_secs(120);

/// The `git` the branch reads and the cleanup run, and the environment it runs it in.
pub struct Git {
    binary: PathBuf,
    env: HashMap<String, String>,
}

impl Git {
    /// The user's own `git`, as their login shell's `PATH` resolves it, in that shell's
    /// environment.
    pub(crate) fn of_user_shell() -> Result<Self> {
        let env = env_shell::cached_snapshot()?;
        let binary = env_shell::resolve_binary("git", &env)?.into();
        Ok(Self { binary, env })
    }

    #[cfg(test)]
    pub(crate) fn new(binary: impl Into<PathBuf>, env: HashMap<String, String>) -> Self {
        Self {
            binary: binary.into(),
            env,
        }
    }

    /// The same `git` and environment as the browse reads take them.
    pub(crate) fn browse_env(&self) -> GitEnv {
        GitEnv::new(self.binary.to_string_lossy().into_owned(), self.env.clone())
    }

    /// Runs `git <args>` in `dir` and returns its stdout; a read leaves the index alone. The error
    /// is git's own message, or why it could not run.
    fn run(&self, dir: &Path, args: &[&OsStr], write: bool) -> Result<Vec<u8>, String> {
        let mut command =
            git_env::non_interactive(&self.binary, git_env::user_configuration_only(&self.env));
        command.current_dir(dir);
        // A configured fsmonitor is a program `git` would start on every status read.
        command.args(["--no-pager", "-c", "core.fsmonitor=false"]);
        if !write {
            git_env::read_only(&mut command);
        }
        command.args(args);
        let output = crate::subprocess::run_with_timeout(&mut command, GIT_TIMEOUT)
            .map_err(|err| format!("running git: {err:#}"))?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    pub(super) fn read(&self, dir: &Path, args: &[&OsStr]) -> Result<Vec<u8>, String> {
        self.run(dir, args, false)
    }

    pub(super) fn write(&self, dir: &Path, args: &[&OsStr]) -> Result<(), String> {
        self.run(dir, args, true).map(|_| ())
    }
}

/// One worktree of the repository, as `git worktree list` gives it.
pub(super) struct Listed {
    pub(super) root: PathBuf,
    pub(super) branch: Option<Vec<u8>>,
    pub(super) main: bool,
    pub(super) locked: bool,
    /// Its directory is gone, and only git's record of it is left.
    pub(super) prunable: bool,
}

/// The repository's worktrees, the main one first.
pub(super) fn worktrees(git: &Git, repo: &Path) -> Result<Vec<Listed>, String> {
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
pub(super) fn changes(git: &Git, root: &Path) -> Result<bool, String> {
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
