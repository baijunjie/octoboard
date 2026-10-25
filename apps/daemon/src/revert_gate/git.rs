//! The `git` the gate runs, the small readings of its output the rest of the gate shares, and the
//! queries about the repository's branches and remotes.

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::Duration;

use anyhow::{bail, Result};

use crate::{git_env, subprocess};

/// The deadline of each `git` call. The gate runs synchronously inside the user's `git`, so a
/// call that hangs holds that command up; this is long enough for a read of a large repository
/// and short enough that the user is not left waiting minutes on a check that fails open anyway.
const GIT_TIMEOUT: Duration = Duration::from_secs(30);

/// Where and how the gate runs `git`: the program, the directory it runs in and the environment
/// it gets. The hook's own repository comes from the environment git set for the hook
/// (`GIT_DIR` and the like), so nothing of it is filtered out.
pub(super) struct Git {
    program: OsString,
    dir: Option<PathBuf>,
    env: Vec<(OsString, OsString)>,
}

impl Git {
    /// The `git` on `PATH` — git puts its own exec path first for a hook it runs — in the hook's
    /// inherited environment and working directory.
    pub(super) fn hook() -> Self {
        Self {
            program: OsString::from("git"),
            dir: None,
            env: std::env::vars_os().collect(),
        }
    }

    #[cfg(test)]
    pub(super) fn with(program: &str, dir: PathBuf, env: Vec<(OsString, OsString)>) -> Self {
        Self {
            program: OsString::from(program),
            dir: Some(dir),
            env,
        }
    }

    /// The directory the commands run in, for resolving a relative path `git` prints.
    pub(super) fn dir(&self) -> Result<PathBuf> {
        match &self.dir {
            Some(dir) => Ok(dir.clone()),
            None => Ok(std::env::current_dir()?),
        }
    }

    fn command<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = git_env::non_interactive(&self.program, self.env.iter().cloned());
        git_env::read_only(&mut command);
        // A partial clone must not fetch from inside a hook; content that cannot be obtained
        // counts as unread.
        command.env("GIT_NO_LAZY_FETCH", "1");
        if let Some(dir) = &self.dir {
            command.current_dir(dir);
        }
        command.args(args);
        command
    }

    fn run<I, S>(&self, args: I, input: Option<Vec<u8>>) -> Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = self.command(args);
        match input {
            Some(input) => subprocess::run_with_input(&mut command, input, GIT_TIMEOUT),
            None => subprocess::run_with_timeout(&mut command, GIT_TIMEOUT),
        }
    }

    fn checked(&self, args: Vec<OsString>, input: Option<Vec<u8>>) -> Result<Vec<u8>> {
        let output = self.run(&args, input)?;
        if !output.status.success() {
            bail!(
                "git {} failed: {}",
                args.iter()
                    .map(|arg| arg.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(output.stdout)
    }

    /// What `git` printed; a failure of the command is an error.
    pub(super) fn read<I, S>(&self, args: I) -> Result<Vec<u8>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.checked(
            args.into_iter().map(|a| a.as_ref().to_owned()).collect(),
            None,
        )
    }

    /// [`Git::read`] with `input` on stdin.
    pub(super) fn read_with_input<I, S>(&self, args: I, input: Vec<u8>) -> Result<Vec<u8>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.checked(
            args.into_iter().map(|a| a.as_ref().to_owned()).collect(),
            Some(input),
        )
    }

    /// What `git` printed, whether or not it succeeded: for a command whose failure just means
    /// there is nothing to print (an unknown revision, no merge base).
    pub(super) fn read_lenient<I, S>(&self, args: I) -> Result<Vec<u8>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Ok(self.run(args, None)?.stdout)
    }

    /// Whether `git` exited with success; failing to run it at all is an error.
    pub(super) fn succeeds<I, S>(&self, args: I) -> Result<bool>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Ok(self.run(args, None)?.status.success())
    }

    /// The whitespace-trimmed text of [`Git::read_lenient`].
    pub(super) fn line_lenient<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Ok(lossy(&self.read_lenient(args)?).trim().to_string())
    }
}

pub(super) fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A full object id, SHA-1 or SHA-256.
pub(super) fn is_oid(text: &str) -> bool {
    matches!(text.len(), 40 | 64)
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Empty, or all zeros: the id git uses for "no object".
pub(super) fn is_null(oid: &str) -> bool {
    oid.bytes().all(|byte| byte == b'0')
}

/// Whitespace for the line comparison: Unicode `White_Space` plus the four information
/// separators, U+001C to U+001F.
pub(super) fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

/// The first ten characters of an object id, as the messages show it.
pub(super) fn short(oid: &str) -> &str {
    &oid[..oid.len().min(10)]
}

// -- the repository's branches ---------------------------------------------------

/// Local branch names pointing at `commit`. On a fast-forward merge `new` is the feature branch
/// tip, which is how its recorded fork point is found.
pub(super) fn branches_at(git: &Git, commit: &str) -> Result<Vec<String>> {
    let out = git.read([
        "for-each-ref",
        "--format=%(refname)",
        &format!("--points-at={commit}"),
        "refs/heads/",
    ])?;
    Ok(lossy(&out)
        .split_whitespace()
        .filter_map(|name| name.strip_prefix("refs/heads/"))
        .map(str::to_string)
        .collect())
}

pub(super) fn is_ancestor(git: &Git, a: &str, b: &str) -> Result<bool> {
    git.succeeds(["merge-base", "--is-ancestor", a, b])
}

pub(super) fn has_commit(git: &Git, oid: &str) -> Result<bool> {
    git.succeeds(["cat-file", "-e", &format!("{oid}^{{commit}}")])
}

pub(super) fn rev_count(git: &Git, args: &[&str]) -> Result<usize> {
    let out = git.read(
        ["rev-list", "--count"]
            .into_iter()
            .chain(args.iter().copied()),
    )?;
    Ok(lossy(&out).trim().parse().unwrap_or(0))
}

pub(super) fn remote_refs(git: &Git, branch: &str) -> Result<Vec<String>> {
    let out = git.read([
        "for-each-ref",
        "--format=%(refname)",
        &format!("refs/remotes/*/{branch}"),
    ])?;
    Ok(lossy(&out).split_whitespace().map(str::to_string).collect())
}

pub(super) fn remote_tips(git: &Git, branch: &str) -> Result<Vec<String>> {
    let out = git.read([
        "for-each-ref",
        "--format=%(objectname)",
        &format!("refs/remotes/*/{branch}"),
    ])?;
    Ok(lossy(&out).split_whitespace().map(str::to_string).collect())
}

/// The branch's upstream ref, and how to write it in a hint. With no upstream (or an upstream ref
/// that does not exist), the unique same-named branch on a remote is used instead; with no or
/// several same-named branches there is no ref and the hint is written `<remote>/<branch>`.
pub(super) fn upstream_of(git: &Git, branch: &str) -> Result<(Option<String>, String)> {
    let upstream = git.line_lenient([
        "for-each-ref",
        "--format=%(upstream)",
        &format!("refs/heads/{branch}"),
    ])?;
    if !upstream.is_empty() && git.succeeds(["rev-parse", "--verify", "-q", upstream.as_str()])? {
        return Ok((Some(upstream), format!("{branch}@{{u}}")));
    }
    let mut refs = remote_refs(git, branch)?;
    if refs.len() == 1 {
        let only = refs.remove(0);
        let shown = only
            .strip_prefix("refs/remotes/")
            .unwrap_or(&only)
            .to_string();
        return Ok((Some(only), shown));
    }
    Ok((None, format!("<remote>/{branch}")))
}

/// Names of branches checked out in any worktree, or currently being rebased. During a rebase
/// HEAD is detached, so the worktree listing does not show which branch it is on; read that from
/// each worktree's rebase state directory instead.
pub(super) fn busy_branches(git: &Git) -> Result<HashSet<String>> {
    let mut names = HashSet::new();
    let listing = git.read(["worktree", "list", "--porcelain"])?;
    for line in lossy(&listing).split('\n') {
        if let Some(name) = line.strip_prefix("branch refs/heads/") {
            names.insert(name.to_string());
        }
    }
    let common = git
        .dir()?
        .join(lossy(&git.read(["rev-parse", "--git-common-dir"])?).trim());
    let mut gitdirs = vec![common.clone()];
    if let Ok(linked) = std::fs::read_dir(common.join("worktrees")) {
        gitdirs.extend(linked.filter_map(Result::ok).map(|entry| entry.path()));
    }
    for gitdir in gitdirs {
        for state in ["rebase-merge", "rebase-apply"] {
            let Ok(head) = std::fs::read(gitdir.join(state).join("head-name")) else {
                continue;
            };
            if let Some(name) = lossy(&head).trim().strip_prefix("refs/heads/") {
                names.insert(name.to_string());
            }
        }
    }
    Ok(names)
}
