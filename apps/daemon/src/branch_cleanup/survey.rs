//! Surveying what is bound to sessions: where each branch and linked worktree stands, and the git
//! checks behind that.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use crate::branch_origin::bound::{bound_in, Bound, BoundBranch};
use crate::branch_origin::repo_key;
use crate::git::layout::admin_dir;
use crate::git::repo_files::uses_reftable;
use crate::git::run::Runner;
use crate::git::tree_entries::entries;
use crate::git::worktrees::{changes, worktrees, Listed};
use crate::host_tools::Pace;
use crate::hostfs::place::Looking;
use crate::platform::names;
use crate::protocol::{
    Session, SessionBranchCause, SessionBranchItem, SessionBranchKind, SessionBranchState,
};
use crate::removal_safety::{Blocked, Unresolved, Worktrees, TRASH_LOOK_DEADLINE};
use crate::state::AppState;
use crate::wire_path;

/// What is bound to `sessions`, and where each item stands now, with the user's `git` for `pace`.
/// Runs no `git` when none of the sessions is responsible for a branch, which is most sessions.
/// Blocking, waiting for its looks as [`WorktreeRemovability::look`] says.
pub fn inspect(
    state: &AppState,
    sessions: &[Session],
    pace: Pace,
) -> Result<Vec<SessionBranchItem>> {
    let bound = bound_in(state, sessions)?;
    if bound.is_empty() {
        return Ok(Vec::new());
    }
    let git = super::cleanup_git(&state.host_tools.get(), pace)?;
    let mut judge = WorktreeRemovability::of_projects(state, None)?;
    let mut items = Vec::new();
    for repo in &bound {
        match survey(state, &git, repo, &mut judge) {
            Ok(survey) => items.extend(survey.items()),
            Err(detail) => items.extend(repo.all_failed(&detail)),
        }
    }
    Ok(items)
}

/// Every item bound to `sessions` as an archive that forces nothing would find it: as [`inspect`]
/// finds it for a confirmation, but for an unmerged branch that a worktree git refuses has checked
/// out, which the confirmation offers to force along with that worktree. With nothing forced the
/// worktree stays, and the branch with it, as checked out there. Blocking.
pub fn inspect_unforced(state: &AppState, sessions: &[Session]) -> Result<Vec<SessionBranchItem>> {
    let mut items = inspect(state, sessions, Pace::Scheduled)?;
    let refused: Vec<(String, String)> = items
        .iter()
        .filter(|item| item.kind == SessionBranchKind::Worktree && refused_by_git(item.state))
        .filter_map(|item| Some((item.repository.clone(), item.branch.clone()?)))
        .collect();
    for item in &mut items {
        if item.kind == SessionBranchKind::Branch
            && item.state == SessionBranchState::Unmerged
            && item.branch.as_deref().is_some_and(|branch| {
                refused
                    .iter()
                    .any(|(repository, name)| *repository == item.repository && name == branch)
            })
        {
            item.state = SessionBranchState::CheckedOut;
        }
    }
    Ok(items)
}

/// Whether a worktree in `state` is one `git worktree remove` keeps unless forced.
fn refused_by_git(state: SessionBranchState) -> bool {
    matches!(
        state,
        SessionBranchState::UncommittedChanges
            | SessionBranchState::Submodules
            | SessionBranchState::Locked
    )
}

/// What an agent's own archive of `sessions` will leave, judged before anything is removed: every
/// bound item that cannot go without forcing. A failure is logged and judges nothing. Blocking.
pub fn will_be_left(state: &AppState, sessions: &[Session]) -> Vec<SessionBranchItem> {
    match inspect_unforced(state, sessions) {
        Ok(items) => items
            .into_iter()
            .filter(|item| item.state != SessionBranchState::Removable)
            .collect(),
        Err(err) => {
            tracing::warn!(%err, "inspecting the branches of sessions being archived failed");
            Vec::new()
        }
    }
}

/// [`will_be_left`] off the runtime's workers.
pub async fn left_by(state: &Arc<AppState>, sessions: Vec<Session>) -> Vec<SessionBranchItem> {
    let state = Arc::clone(state);
    tokio::task::spawn_blocking(move || will_be_left(&state, &sessions))
        .await
        .unwrap_or_else(|err| {
            tracing::warn!(%err, "inspecting the branches of sessions being archived did not finish");
            Vec::new()
        })
}

pub(super) fn failed(
    mut item: SessionBranchItem,
    detail: impl std::fmt::Display,
) -> SessionBranchItem {
    item.state = SessionBranchState::Failed;
    item.detail = Some(detail.to_string());
    item.cause = None;
    item
}

/// `item` as one kept because it is the home directory, the root of a disk or a directory holding
/// Octoboard's data.
fn protected(item: SessionBranchItem) -> SessionBranchItem {
    SessionBranchItem {
        cause: Some(SessionBranchCause::ProtectedFolder),
        ..failed(item, PROTECTED.to_string())
    }
}

/// `item` as one kept because whether it is or holds another project's folder, or what else it
/// must not take, could not be told, for the reason `detail`.
pub(super) fn untold(item: SessionBranchItem, detail: String) -> SessionBranchItem {
    SessionBranchItem {
        cause: Some(SessionBranchCause::ProjectFolderUnknown),
        ..failed(item, detail)
    }
}

// -- what is bound -------------------------------------------------------------

impl Bound {
    /// Every branch of the repository as failed, for a repository whose worktrees cannot be
    /// listed.
    pub(super) fn all_failed(&self, detail: &str) -> Vec<SessionBranchItem> {
        self.branches
            .iter()
            .map(|bound| {
                failed(
                    item_of(
                        &self.repo,
                        SessionBranchKind::Branch,
                        Some(bound.branch.clone()),
                        None,
                        &bound.project,
                        Some(&bound.session),
                        SessionBranchState::Failed,
                    ),
                    detail.to_string(),
                )
            })
            .collect()
    }
}

pub(super) fn item_of(
    repo: &Path,
    kind: SessionBranchKind,
    branch: Option<String>,
    path: Option<String>,
    project: &str,
    session: Option<&str>,
    state: SessionBranchState,
) -> SessionBranchItem {
    SessionBranchItem {
        kind,
        repository: repo_key(repo),
        branch,
        path,
        project: project.to_string(),
        session: session.map(str::to_string),
        state,
        uncommitted: false,
        tip: None,
        detail: None,
        cause: None,
    }
}

// -- what removing a worktree would take along ----------------------------------------

/// Why a worktree is kept when whether it is or holds another project's folder could not be told,
/// as a `failed` item's `detail`.
pub(super) const PROJECT_FOLDER_UNTOLD: &str =
    "whether it is or holds another project's folder could not be told";

/// Why a worktree whose loss would be drastic is kept, as a `failed` item's `detail`.
pub(super) const PROTECTED: &str =
    "it is the home folder, the root of a disk or a folder holding Octoboard's data";

/// Whether a worktree may be removed without taking what belongs to something else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Removability {
    Removable,
    /// It is another project's folder or holds one.
    ProjectFolder,
    /// It is the home directory, the root of a disk or a directory holding Octoboard's data.
    Protected,
    /// It could not be told, for the reason given as a `failed` item's `detail`. A miss would
    /// remove what it holds with the worktree, so the worktree is kept, as for `ProjectFolder`,
    /// though named as one that could not be told.
    Untold(String),
}

/// The worktrees a cleanup may remove, judged against every project's folder but one, the home
/// directory and Octoboard's data directory as `removal_safety` judges them ([`Worktrees`]).
///
/// The paths are read off the store's lock, each distinct one once per cleanup, a few at a time,
/// each bounded by [`TRASH_LOOK_DEADLINE`].
pub(super) struct WorktreeRemovability(Worktrees);

impl WorktreeRemovability {
    /// For the paths of every project but `except`, none of them looked at yet.
    pub(super) fn of_projects(state: &AppState, except: Option<&str>) -> Result<Self> {
        Self::of(
            state,
            except,
            crate::paths::known_home_dir(),
            crate::paths::data_dir(),
        )
    }

    /// [`WorktreeRemovability::of_projects`], with `home` and `data` the home directory and
    /// Octoboard's data directory.
    pub(super) fn of(
        state: &AppState,
        except: Option<&str>,
        home: Option<PathBuf>,
        data: PathBuf,
    ) -> Result<Self> {
        let others = state
            .store
            .list_projects()?
            .into_iter()
            .filter(|project| Some(project.id.as_str()) != except)
            .map(|project| PathBuf::from(project.path))
            .collect();
        Ok(Self(Worktrees::new(others, home, data)))
    }

    /// Looks at the worktree roots `roots` and at what they are judged against, those not looked
    /// at yet ([`Worktrees::look`]).
    ///
    /// Blocking: this thread waits for the looks, which the runtime runs on blocking threads of
    /// their own so each can be left behind at its deadline. So it runs on a blocking thread of
    /// the runtime (`spawn_blocking`), as every blocking function here does, never on one of its
    /// workers; off any runtime nothing is looked at, and every worktree counts as untold.
    pub(super) fn look(&mut self, state: &AppState, roots: impl IntoIterator<Item = PathBuf>) {
        let roots: Vec<PathBuf> = roots.into_iter().collect();
        if roots.is_empty() {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            tracing::warn!("no runtime to look at worktrees with; each counts as untold");
            return;
        };
        runtime.block_on(
            self.0
                .look(state, Looking::within(TRASH_LOOK_DEADLINE), roots),
        );
    }

    /// Whether the worktree at `root`, looked at through [`WorktreeRemovability::look`], may be
    /// removed.
    pub(super) fn removability(&self, root: &Path) -> Removability {
        match self.0.judged(root) {
            Ok(()) => Removability::Removable,
            Err(Blocked::Protected) => Removability::Protected,
            Err(Blocked::InTheWay {
                unresolved: None, ..
            }) => Removability::ProjectFolder,
            Err(Blocked::InTheWay { .. } | Blocked::Unlooked(Unresolved::Folder, _)) => {
                Removability::Untold(PROJECT_FOLDER_UNTOLD.to_string())
            }
            Err(Blocked::Unlooked(Unresolved::Home, failure)) => Removability::Untold(format!(
                "the home folder {}, so whether this worktree is it could not be told",
                failure.described().0
            )),
            Err(Blocked::Unlooked(Unresolved::Data, failure)) => Removability::Untold(format!(
                "Octoboard's data folder {}, so whether it is in this worktree could not be told",
                failure.described().0
            )),
        }
    }
}

/// The branches of `bound` that are still there, each with its name as bytes. A reftable
/// repository has no reflog files for `is_same_branch` to find gone, so there it is asked.
fn stored_names<'b>(
    git: &Runner,
    repo: &Path,
    bound: &'b [BoundBranch],
) -> Vec<(Vec<u8>, &'b BoundBranch)> {
    let reftable = uses_reftable(repo);
    bound
        .iter()
        .filter_map(|branch| {
            let name = wire_path::decode(&branch.branch)?;
            (!reftable || exists(git, repo, &name)).then_some((name, branch))
        })
        .collect()
}

/// A repository's worktrees as git lists them, and the bound branches that are still there: what
/// a survey and a project's removal both start from.
pub(super) struct BoundListing<'b> {
    pub(super) listed: Vec<Listed>,
    /// As [`stored_names`] returns them.
    pub(super) names: Vec<(Vec<u8>, &'b BoundBranch)>,
}

impl<'b> BoundListing<'b> {
    pub(super) fn of(git: &Runner, repo: &Path, bound: &'b [BoundBranch]) -> Result<Self> {
        Ok(Self {
            listed: worktrees(git, repo)?,
            names: stored_names(git, repo, bound),
        })
    }

    /// The bound branch `worktree` has checked out, if it has one.
    pub(super) fn bound_branch(&self, worktree: &Listed) -> Option<&'b BoundBranch> {
        self.names
            .iter()
            .find(|(name, _)| worktree.branch.as_deref() == Some(name.as_slice()))
            .map(|(_, branch)| *branch)
    }
}

// -- where it stands -----------------------------------------------------------

/// Where a bound linked worktree that is not, as far as can be told, another project's folder
/// and holds none stands, and whether it has uncommitted changes.
fn worktree_state(git: &Runner, worktree: &Listed) -> (SessionBranchState, bool, Option<String>) {
    // A status that cannot be read counts as changes, so a force never takes changes nobody saw.
    let uncommitted = || !worktree.prunable && changes(git, &worktree.root).unwrap_or(true);
    if worktree.locked {
        return (SessionBranchState::Locked, uncommitted(), None);
    }
    if worktree.prunable {
        return (SessionBranchState::Removable, false, None);
    }
    if has_submodules(git, &worktree.root) {
        return (SessionBranchState::Submodules, uncommitted(), None);
    }
    match changes(git, &worktree.root) {
        Ok(false) => (SessionBranchState::Removable, false, None),
        Ok(true) => (SessionBranchState::UncommittedChanges, true, None),
        Err(detail) => (SessionBranchState::Failed, false, Some(detail.to_string())),
    }
}

pub(super) struct WorktreeSurvey {
    pub(super) item: SessionBranchItem,
    pub(super) root: PathBuf,
}

pub(super) struct BranchSurvey {
    pub(super) item: SessionBranchItem,
    pub(super) name: Vec<u8>,
    pub(super) merged: bool,
}

/// Where every item bound in one repository stands, before anything is removed.
pub(super) struct Survey {
    pub(super) worktrees: Vec<WorktreeSurvey>,
    pub(super) branches: Vec<BranchSurvey>,
}

impl Survey {
    fn items(self) -> Vec<SessionBranchItem> {
        self.worktrees
            .into_iter()
            .map(|worktree| worktree.item)
            .chain(self.branches.into_iter().map(|branch| branch.item))
            .collect()
    }
}

pub(super) fn survey(
    state: &AppState,
    git: &Runner,
    bound: &Bound,
    judge: &mut WorktreeRemovability,
) -> Result<Survey, String> {
    let listing =
        BoundListing::of(git, &bound.repo, &bound.branches).map_err(|err| err.to_string())?;
    judge.look(
        state,
        listing
            .listed
            .iter()
            .filter(|worktree| !worktree.main && listing.bound_branch(worktree).is_some())
            .map(|worktree| worktree.root.clone()),
    );

    let mut worktrees = Vec::new();
    for worktree in &listing.listed {
        if worktree.main {
            continue;
        }
        let Some(branch) = listing.bound_branch(worktree) else {
            continue;
        };
        let item = |state| {
            item_of(
                &bound.repo,
                SessionBranchKind::Worktree,
                Some(branch.branch.clone()),
                Some(wire_path::encode(&names::name_bytes(
                    worktree.root.as_os_str(),
                ))),
                &branch.project,
                Some(&branch.session),
                state,
            )
        };
        let item = match judge.removability(&worktree.root) {
            Removability::ProjectFolder => item(SessionBranchState::ProjectFolder),
            Removability::Protected => protected(item(SessionBranchState::Failed)),
            Removability::Untold(detail) => untold(item(SessionBranchState::Failed), detail),
            Removability::Removable => {
                let (state, uncommitted, detail) = worktree_state(git, worktree);
                SessionBranchItem {
                    uncommitted,
                    detail,
                    ..item(state)
                }
            }
        };
        worktrees.push(WorktreeSurvey {
            item,
            root: worktree.root.clone(),
        });
    }

    let mut branches = Vec::new();
    for (name, branch) in listing.names {
        let holder_states: Vec<Option<SessionBranchState>> = listing
            .listed
            .iter()
            .filter(|worktree| worktree.branch.as_deref() == Some(name.as_slice()))
            .map(|holder| {
                worktrees
                    .iter()
                    .find(|worktree| worktree.root == holder.root)
                    .map(|worktree| worktree.item.state)
            })
            .collect();
        // A holder that is not a bound worktree this cleanup could remove keeps the branch
        // whatever is asked; one git refuses keeps it unless it is forced.
        let refused = |state: &Option<SessionBranchState>| state.is_some_and(refused_by_git);
        let stays = holder_states
            .iter()
            .any(|state| *state != Some(SessionBranchState::Removable) && !refused(state));
        let merged = is_merged(git, &bound.repo, &name);
        let state = if stays {
            SessionBranchState::CheckedOut
        } else if !merged {
            SessionBranchState::Unmerged
        } else if holder_states.iter().any(refused) {
            SessionBranchState::CheckedOut
        } else {
            SessionBranchState::Removable
        };
        branches.push(BranchSurvey {
            item: SessionBranchItem {
                tip: tip(git, &bound.repo, &name),
                ..item_of(
                    &bound.repo,
                    SessionBranchKind::Branch,
                    Some(branch.branch.clone()),
                    None,
                    &branch.project,
                    Some(&branch.session),
                    state,
                )
            },
            name,
            merged,
        });
    }
    Ok(Survey {
        worktrees,
        branches,
    })
}

/// The object id the branch `name` points at, `None` where it cannot be read.
pub(super) fn tip(git: &Runner, repo: &Path, name: &[u8]) -> Option<String> {
    let reference = names::name_from_bytes(&[b"refs/heads/".as_slice(), name].concat())?;
    let stdout = git
        .read(
            repo,
            &[
                OsStr::new("rev-parse"),
                OsStr::new("--verify"),
                OsStr::new("--quiet"),
                &reference,
            ],
        )
        .ok()?;
    let oid = String::from_utf8(stdout).ok()?.trim().to_string();
    (!oid.is_empty()).then_some(oid)
}

/// Whether the branch `name` exists.
pub(super) fn exists(git: &Runner, repo: &Path, name: &[u8]) -> bool {
    let Some(reference) = names::name_from_bytes(&[b"refs/heads/".as_slice(), name].concat())
    else {
        return false;
    };
    git.read(
        repo,
        &[
            OsStr::new("show-ref"),
            OsStr::new("--verify"),
            OsStr::new("--quiet"),
            &reference,
        ],
    )
    .is_ok()
}

/// Whether the worktree at `root` has initialized submodules, which `git worktree remove` refuses
/// without force: the same two signs it looks for, a `modules` directory in the worktree's own git
/// directory, or a gitlink in the index whose path holds a repository.
fn has_submodules(git: &Runner, root: &Path) -> bool {
    if admin_dir(root).is_some_and(|admin| admin.join("modules").is_dir()) {
        return true;
    }
    let Ok(stdout) = git.read(
        root,
        &[
            OsStr::new("ls-files"),
            OsStr::new("--stage"),
            OsStr::new("-z"),
        ],
    ) else {
        return false;
    };
    let mut gitlinks = entries(&stdout).filter(|entry| entry.is_gitlink);
    gitlinks.any(|entry| {
        names::path_from_name_bytes(entry.path)
            .is_some_and(|path| root.join(path).join(".git").exists())
    })
}

/// Whether `git branch -d` would take the branch `name` as merged: into its upstream when that
/// exists, into `HEAD` otherwise. Anything that cannot be told counts as not merged.
pub(super) fn is_merged(git: &Runner, repo: &Path, name: &[u8]) -> bool {
    let Some(reference) = names::name_from_bytes(&[b"refs/heads/".as_slice(), name].concat())
    else {
        return false;
    };
    let upstream = git
        .read(
            repo,
            &[
                OsStr::new("for-each-ref"),
                OsStr::new("--format=%(upstream)"),
                &reference,
            ],
        )
        .ok()
        .and_then(|stdout| {
            let first = stdout.split(|&b| b == b'\n').next()?.to_vec();
            (!first.is_empty()).then_some(first)
        })
        .filter(|upstream| {
            let Some(commit) =
                names::name_from_bytes(&[upstream.as_slice(), b"^{commit}"].concat())
            else {
                return false;
            };
            git.read(
                repo,
                &[
                    OsStr::new("rev-parse"),
                    OsStr::new("--verify"),
                    OsStr::new("--quiet"),
                    &commit,
                ],
            )
            .is_ok()
        });
    let against = upstream.unwrap_or_else(|| b"HEAD".to_vec());
    let Some(against) = names::name_from_bytes(&against) else {
        return false;
    };
    git.read(
        repo,
        &[
            OsStr::new("merge-base"),
            OsStr::new("--is-ancestor"),
            &reference,
            &against,
        ],
    )
    .is_ok()
}
