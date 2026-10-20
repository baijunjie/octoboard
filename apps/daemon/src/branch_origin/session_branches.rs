//! Where the branches a session is responsible for stand in git, read for a handoff package: the
//! worktree each is checked out in, its head, what it has committed against the branch it was cut
//! from, and whether its worktree holds uncommitted changes. Reading only; nothing is checked out,
//! fetched or written. The worktree of named branches can also be read alone ([`worktrees_of`]), as
//! a branch handover reports it.
//!
//! What a branch is compared with follows the project pane's Git mode ("Committed changes" in
//! `docs/product/project-pane-git-change-list.md`): any branch but the main working copy's has the
//! main working copy's branch as its base, and the main working copy's own branch has its remote
//! branch, which is the one it tracks when that is a remote-tracking branch, else the branch of
//! its name on a configured remote (`origin`'s preferred). A branch with no base has only its head
//! and worktree read, where the Git mode would list the main working copy's whole history.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::Result;

use super::bound::bound_in;
use super::git::{changes, worktrees, Git, Listed};
use crate::browse::{compare, wire_path};
use crate::protocol::{Session, UpstreamInfo};
use crate::state::AppState;

/// How many commits and changed files are listed; the totals are always given.
const LISTED_COMMITS: usize = 50;
const LISTED_FILES: usize = 200;

/// Where a branch is checked out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Worktree {
    /// No worktree has it checked out.
    #[default]
    NotCheckedOut,
    /// It could not be told, and why.
    Unknown(String),
    /// The worktree that has it checked out is gone from disk, and only git's record is left.
    Gone(PathBuf),
    CheckedOut {
        path: PathBuf,
        /// Whether it has modified or untracked files; `None` when that could not be read.
        uncommitted: Option<bool>,
    },
}

/// One branch a session is responsible for, as git has it now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchState {
    pub branch: String,
    pub worktree: Worktree,
    /// The full id of its tip.
    pub head: Option<String>,
    /// The branch it is compared with, and why there is none when it is `None`.
    pub base: Option<String>,
    pub base_note: Option<String>,
    /// Its commits not in the base, newest first, as `(abbreviated id, subject)`.
    pub commits: Vec<(String, String)>,
    pub commit_count: usize,
    /// Its changes since it forked from the base, as `(status letter, path)`.
    pub files: Vec<(char, String)>,
    pub file_count: usize,
    /// What could not be read.
    pub problems: Vec<String>,
}

/// The branches `session` is responsible for, in git's order. Blocking. Runs no `git` when it is
/// responsible for none, which is most sessions.
pub fn of_session(state: &AppState, session: &Session) -> Result<Vec<BranchState>> {
    let bound = bound_in(state, std::slice::from_ref(session))?;
    if bound.is_empty() {
        return Ok(Vec::new());
    }
    let git = (state.host_tools.get().git)()?;
    let mut states = Vec::new();
    for repo in &bound {
        let names: Vec<Vec<u8>> = repo
            .branches
            .iter()
            .map(|branch| wire_path::decode(&branch.branch).unwrap_or_default())
            .collect();
        states.extend(read_branches(&git, &repo.repo, &names));
    }
    Ok(states)
}

/// Where each of `branches` (wire paths) of the repository `repo` is checked out, and whether that
/// worktree holds uncommitted changes, in the order given: the worktrees are listed once and only
/// the checked-out ones are asked for their changes. Blocking. A failure to run `git` at all is the
/// error; one to list the worktrees leaves every branch [`Worktree::Unknown`].
pub fn worktrees_of(
    state: &AppState,
    repo: &Path,
    branches: &[String],
) -> Result<Vec<(String, Worktree)>> {
    let git = (state.host_tools.get().git)()?;
    let listed = worktrees(&git, repo);
    Ok(branches
        .iter()
        .map(|wire| {
            let name = wire_path::decode(wire).unwrap_or_default();
            let worktree = match &listed {
                Ok(listed) => worktree_of(&git, listed, &name).0,
                Err(detail) => {
                    Worktree::Unknown(format!("the worktrees could not be listed: {detail}"))
                }
            };
            (String::from_utf8_lossy(&name).into_owned(), worktree)
        })
        .collect())
}

/// Where the branch `name` is checked out among `listed`, and why its changes could not be read
/// when they could not.
fn worktree_of(git: &Git, listed: &[Listed], name: &[u8]) -> (Worktree, Option<String>) {
    let Some(worktree) = listed
        .iter()
        .find(|worktree| worktree.branch.as_deref() == Some(name))
    else {
        return (Worktree::NotCheckedOut, None);
    };
    if worktree.prunable {
        return (Worktree::Gone(worktree.root.clone()), None);
    }
    let (uncommitted, problem) = match changes(git, &worktree.root) {
        Ok(uncommitted) => (Some(uncommitted), None),
        Err(detail) => (
            None,
            Some(format!(
                "its worktree's changes could not be read: {detail}"
            )),
        ),
    };
    (
        Worktree::CheckedOut {
            path: worktree.root.clone(),
            uncommitted,
        },
        problem,
    )
}

/// Reads each of `names`, branches of `repo`, against one listing of its worktrees.
fn read_branches(git: &Git, repo: &Path, names: &[Vec<u8>]) -> Vec<BranchState> {
    let listed = worktrees(git, repo);
    let main_branch = listed
        .as_ref()
        .ok()
        .and_then(|listed| listed.iter().find(|worktree| worktree.main))
        .and_then(|main| main.branch.clone());
    // Only the main working copy's own branch needs its remote branch looked up.
    let upstream = match &main_branch {
        Some(main) if names.contains(main) => main_upstream(git, repo, main),
        _ => Ok(None),
    };
    names
        .iter()
        .map(|name| match &listed {
            Ok(listed) => read_branch(git, repo, listed, name, &upstream),
            Err(detail) => BranchState {
                branch: String::from_utf8_lossy(name).into_owned(),
                worktree: Worktree::Unknown(format!("the worktrees could not be listed: {detail}")),
                ..BranchState::default()
            },
        })
        .collect()
}

/// The remote branch the Git mode compares the main working copy's branch `main` with.
fn main_upstream(git: &Git, repo: &Path, main: &[u8]) -> Result<Option<UpstreamInfo>, String> {
    let list = compare::branches_in(&git.browse_env(), repo, repo, &AtomicBool::new(false))
        .map_err(|err| format!("{err:#}"))?;
    if !list.complete || !list.upstreams_complete {
        return Err("the repository's branch list was cut short".to_owned());
    }
    let wire = wire_path::encode(main);
    Ok(list
        .branches
        .into_iter()
        .find(|branch| branch.name == wire)
        .and_then(|branch| branch.upstream))
}

fn read_branch(
    git: &Git,
    repo: &Path,
    listed: &[Listed],
    name: &[u8],
    upstream: &Result<Option<UpstreamInfo>, String>,
) -> BranchState {
    let mut state = BranchState {
        branch: String::from_utf8_lossy(name).into_owned(),
        ..BranchState::default()
    };
    let reference = [b"refs/heads/", name].concat();
    let reference = OsStr::from_bytes(&reference);
    match text(
        git,
        repo,
        &[OsStr::new("rev-parse"), OsStr::new("--verify"), reference],
    ) {
        Ok(head) => state.head = Some(head),
        Err(detail) => {
            state.worktree = Worktree::Unknown(format!("its head could not be read: {detail}"));
            return state;
        }
    }
    let (worktree, problem) = worktree_of(git, listed, name);
    state.worktree = worktree;
    state.problems.extend(problem);

    let main_branch = listed
        .iter()
        .find(|worktree| worktree.main)
        .and_then(|main| main.branch.as_deref());
    let base = match (main_branch, upstream) {
        (Some(main), _) if main != name => Ok([b"refs/heads/", main].concat()),
        (Some(_), Ok(Some(upstream))) => Ok([
            b"refs/remotes/".as_slice(),
            &wire_path::decode(&upstream.name).unwrap_or_default(),
        ]
        .concat()),
        (Some(_), Ok(None)) => {
            Err("it is the main working copy's branch and has no remote branch".to_owned())
        }
        (Some(_), Err(detail)) => Err(format!(
            "its remote branch could not be looked up: {detail}"
        )),
        (None, _) => Err("the main working copy has no branch checked out".to_owned()),
    };
    let base = match base {
        Ok(base) => base,
        Err(note) => {
            state.base_note = Some(note);
            return state;
        }
    };
    state.base = Some(shortened(&base));
    compare(git, repo, &mut state, OsStr::from_bytes(&base), reference);
    state
}

/// A reference without its `refs/heads/` or `refs/remotes/` prefix.
fn shortened(reference: &[u8]) -> String {
    let name = ["refs/heads/", "refs/remotes/"]
        .iter()
        .find_map(|prefix| reference.strip_prefix(prefix.as_bytes()))
        .unwrap_or(reference);
    String::from_utf8_lossy(name).into_owned()
}

/// Fills in the commits and changed files of `branch` against `base`: what it has done since the
/// two forked, as `git diff base...branch` takes it.
fn compare(git: &Git, repo: &Path, state: &mut BranchState, base: &OsStr, branch: &OsStr) {
    let range = [base.as_bytes(), b"..", branch.as_bytes()].concat();
    let range = OsStr::from_bytes(&range);
    let count = text(
        git,
        repo,
        &[
            OsStr::new("rev-list"),
            OsStr::new("--count"),
            range,
            OsStr::new("--"),
        ],
    );
    match count.map(|count| count.parse::<usize>()) {
        Ok(Ok(count)) => state.commit_count = count,
        Ok(Err(_)) | Err(_) => {
            state
                .problems
                .push("its commits could not be counted".to_owned());
            return;
        }
    }
    let limit = format!("-n{LISTED_COMMITS}");
    match git.read(
        repo,
        &[
            OsStr::new("log"),
            OsStr::new("--no-show-signature"),
            OsStr::new("--format=%h%x09%s"),
            OsStr::new(&limit),
            range,
            OsStr::new("--"),
        ],
    ) {
        Ok(stdout) => {
            state.commits = String::from_utf8_lossy(&stdout)
                .lines()
                .filter_map(|line| line.split_once('\t'))
                .map(|(id, subject)| (id.to_owned(), subject.to_owned()))
                .collect();
        }
        Err(detail) => state
            .problems
            .push(format!("its commits could not be listed: {detail}")),
    }
    let triple = [base.as_bytes(), b"...", branch.as_bytes()].concat();
    match git.read(
        repo,
        &[
            OsStr::new("diff"),
            OsStr::new("--no-ext-diff"),
            OsStr::new("--no-renames"),
            OsStr::new("--ignore-submodules=all"),
            OsStr::new("--name-status"),
            OsStr::new("-z"),
            OsStr::from_bytes(&triple),
            OsStr::new("--"),
        ],
    ) {
        Ok(stdout) => {
            let mut fields = stdout
                .split(|&byte| byte == 0)
                .filter(|field| !field.is_empty());
            while let (Some(status), Some(path)) = (fields.next(), fields.next()) {
                state.file_count += 1;
                if state.files.len() < LISTED_FILES {
                    let letter = status.first().map_or('?', |&byte| char::from(byte));
                    state
                        .files
                        .push((letter, String::from_utf8_lossy(path).into_owned()));
                }
            }
        }
        Err(detail) => state
            .problems
            .push(format!("its changed files could not be listed: {detail}")),
    }
}

/// The trimmed text a read prints.
fn text(git: &Git, repo: &Path, args: &[&OsStr]) -> Result<String, String> {
    let stdout = git.read(repo, args)?;
    Ok(String::from_utf8_lossy(&stdout).trim().to_owned())
}
