//! What removing a project with its folder moved to the Trash takes along beyond the folder: the
//! linked worktrees outside it, and, when the repository survives the folder, the bound branches.
//!
//! Removing a project without moving its folder to the Trash takes nothing. With the folder, the
//! user asked for everything to go, so nothing here waits on git's judgement of what is safe: an
//! outside worktree is moved to the Trash whatever it holds, as recoverably as the folder itself.
//! Only a locked worktree waits for the user's own word (`force_remove`), given for it as it was
//! shown — one that has gained changes since stays — and a worktree that is another project's
//! folder, or holds one, is never touched. Nor is one of which that cannot be told
//! ([`Holders`]): it is listed as staying, and stays with the removal, which reports it as left.
//! An inspection that held one back keeps it under a token that the removal confirmed after it
//! sends back (`crate::project_trash::UnresolvedRemovals`), so that worktree stays even once it
//! can be told: the confirmation showed it staying. As the removal runs only once the project's
//! sessions have no process, it looks again just before it carries out: an outside worktree that
//! has come to be or hold another project's folder by then stays too, and one of which that can
//! no longer be told is held back.
//!
//! When the folder holds the repository — the common directory is inside it — every linked
//! worktree of the repository outside the folder goes, bound to a session or not, since none of
//! them works once the repository is gone; the branches go with the repository. Otherwise (the
//! folder is a part of a bigger repository, or a linked worktree of one) the repository stays:
//! only the worktrees bound to the project's sessions go, a locked one unlocked once it is in the
//! Trash, the records the moves left behind are pruned, and the bound branches are deleted with
//! `git branch -D`, except one still checked out in a worktree that stays, which is reported only
//! when that worktree was to go and did not (a locked one the user chose to keep was not). Every
//! command runs in the repository's common directory, which outlives the folder however the
//! repository is laid out. The main worktree is never touched.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use super::{
    exists, failed, is_merged, item_of, names, stored_names, wait_until_stopped, Holders, Holding,
};
use crate::branch_origin::bound::bound_in;
use crate::branch_origin::git::{changes, worktrees, Git, Listed};
use crate::branch_origin::repository_of;
use crate::browse::wire_path;
use crate::protocol::{
    Project, SessionBranchItem, SessionBranchKind, SessionBranchState, SessionBranchTarget,
};
use crate::state::AppState;

/// What a plan reads of where each item stands: everything, for a confirmation to show, or only
/// what the removal itself needs, so a removal's request runs no `git status` or merge check.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Reads {
    Inspection,
    Removal,
}

/// What the removal of one project, folder and all, takes along.
pub struct Removal {
    /// The project removed, whose own folder no worktree counts as holding.
    project: String,
    repo: PathBuf,
    /// Whether the repository outlives the project's folder.
    survives: bool,
    worktrees: Vec<(SessionBranchItem, PathBuf)>,
    branches: Vec<(SessionBranchItem, Vec<u8>)>,
    /// The outside worktrees it would take but for not being able to tell whether they are or hold
    /// another project's folder, as `failed` items with the cause `project_folder_unknown`, each
    /// with its root.
    held_back: Vec<(SessionBranchItem, PathBuf)>,
    /// The roots of the worktrees that go, or may: those inside the folder, which go with it, one
    /// whose record a prune takes, and the outside ones it takes.
    going: HashSet<PathBuf>,
}

impl Removal {
    /// Every item it may take along, as a confirmation lists them: a `locked` worktree goes only
    /// when forced, and a branch it has checked out only with it; and the worktrees it holds back,
    /// which stay.
    pub fn items(&self) -> Vec<SessionBranchItem> {
        self.worktrees
            .iter()
            .chain(&self.held_back)
            .map(|(item, _)| item.clone())
            .chain(self.branches.iter().map(|(item, _)| item.clone()))
            .collect()
    }

    /// Its items, apart: what it would carry out, and what it holds back.
    fn clone_items(&self) -> Items {
        Items {
            carried: self
                .worktrees
                .iter()
                .map(|(item, _)| item.clone())
                .chain(self.branches.iter().map(|(item, _)| item.clone()))
                .collect(),
            held_back: self
                .held_back
                .iter()
                .map(|(item, _)| item.clone())
                .collect(),
        }
    }

    /// The roots of the outside worktrees it holds back, as `git worktree list` gives them.
    pub fn held_back(&self) -> Vec<PathBuf> {
        self.held_back
            .iter()
            .map(|(_, root)| root.clone())
            .collect()
    }

    fn is_empty(&self) -> bool {
        self.worktrees.is_empty() && self.branches.is_empty() && self.held_back.is_empty()
    }

    /// Looks again at whether each outside worktree it takes is or holds another project's folder,
    /// as one may have come to since the plan: one that is stays, unreported, as one known to be so
    /// at the plan does, and one of which that cannot be told now is held back. Blocking, as
    /// [`Holders::look`] says.
    fn look_again(&mut self, state: &AppState) {
        let mut holders = Holders::of_projects(state, Some(&self.project));
        if let Ok(holders) = &mut holders {
            holders.look(state, self.worktrees.iter().map(|(_, root)| root.clone()));
        }
        for (item, root) in std::mem::take(&mut self.worktrees) {
            let holding = holders
                .as_ref()
                .map_or(Holding::Unknown, |holders| holders.holding(&root));
            match holding {
                Holding::No => self.worktrees.push((item, root)),
                Holding::Yes => {
                    self.going.remove(&root);
                }
                Holding::Unknown => {
                    self.going.remove(&root);
                    self.held_back.push((super::untold(item), root));
                }
            }
        }
    }
}

/// What removing `project` with its folder would take along now; `None` when nothing. Runs no
/// `git` for a project in no repository, nor where the repository survives and none of the
/// project's sessions is responsible for a branch. An outside worktree whose root is among
/// `held_back`, those its inspection held back, is held back again however it stands now.
/// Blocking, waiting for its looks as [`Holders::look`] says.
pub fn plan(
    state: &AppState,
    project: &Project,
    reads: Reads,
    held_back: &[PathBuf],
) -> Result<Option<Removal>> {
    let Some(repo) = repository_of(Path::new(&project.path)) else {
        return Ok(None);
    };
    let folder = std::fs::canonicalize(&project.path)?;
    let survives = !repo.starts_with(&folder);
    let sessions: Vec<_> = state
        .store
        .list_sessions()?
        .into_iter()
        .filter(|session| session.project_id.as_deref() == Some(project.id.as_str()))
        .collect();
    let bound = bound_in(state, &sessions)?
        .into_iter()
        .find(|bound| bound.repo == repo)
        .map(|bound| bound.branches)
        .unwrap_or_default();
    if survives && bound.is_empty() {
        return Ok(None);
    }
    let git = (state.host_tools.get().git)()?;
    let mut holders = Holders::of_projects(state, Some(&project.id))?;
    let listed = worktrees(&git, &repo).map_err(anyhow::Error::msg)?;
    let names = stored_names(&git, &repo, &bound);
    let full = reads == Reads::Inspection;
    let bound_branch = |worktree: &Listed| {
        names
            .iter()
            .find(|(name, _)| worktree.branch.as_deref() == Some(name.as_slice()))
            .map(|(_, branch)| *branch)
    };
    // The outside worktrees it would take, but for another project's folder.
    let outside = |worktree: &&Listed| {
        !worktree.main
            && !worktree.root.starts_with(&folder)
            && !worktree.prunable
            && !(survives && bound_branch(worktree).is_none())
    };
    holders.look(
        state,
        listed
            .iter()
            .filter(outside)
            .map(|worktree| worktree.root.clone()),
    );

    // The worktrees that go, or may: every one inside the folder, which goes with it, one whose
    // directory is gone and whose record a prune takes, and the outside ones taken, a locked one
    // among them only if it is forced.
    let mut going: HashSet<PathBuf> = HashSet::new();
    let mut taken = Vec::new();
    let mut untold = Vec::new();
    for worktree in &listed {
        if worktree.main {
            continue;
        }
        if worktree.root.starts_with(&folder) || (worktree.prunable && !worktree.locked) {
            going.insert(worktree.root.clone());
            continue;
        }
        if !outside(&worktree) {
            continue;
        }
        let bound_by = bound_branch(worktree);
        let item = |state| {
            item_of(
                &repo,
                SessionBranchKind::Worktree,
                worktree.branch.as_deref().map(wire_path::encode),
                Some(wire_path::encode(worktree.root.as_os_str().as_bytes())),
                bound_by.map_or(&project.id, |branch| &branch.project),
                bound_by.map(|branch| branch.session.as_str()),
                state,
            )
        };
        // One known to hold another project's folder never counts, whatever its inspection said.
        match holders.holding(&worktree.root) {
            Holding::Yes => continue,
            Holding::No if !held_back.contains(&worktree.root) => {}
            Holding::No | Holding::Unknown => {
                untold.push((
                    super::untold(item(SessionBranchState::Failed)),
                    worktree.root.clone(),
                ));
                continue;
            }
        }
        // A status that cannot be read counts as changes, so they are shown.
        let uncommitted = full && changes(&git, &worktree.root).unwrap_or(true);
        let state = if worktree.locked {
            SessionBranchState::Locked
        } else if uncommitted {
            SessionBranchState::UncommittedChanges
        } else {
            SessionBranchState::Removable
        };
        let mut item = item(state);
        item.uncommitted = uncommitted;
        going.insert(worktree.root.clone());
        taken.push((item, worktree.root.clone()));
    }

    let mut branches = Vec::new();
    if survives {
        for (name, branch) in names {
            let stays = listed.iter().any(|worktree| {
                worktree.branch.as_deref() == Some(name.as_slice())
                    && !going.contains(&worktree.root)
            });
            if stays {
                continue;
            }
            let state = if full && !is_merged(&git, &repo, &name) {
                SessionBranchState::Unmerged
            } else {
                SessionBranchState::Removable
            };
            let item = item_of(
                &repo,
                SessionBranchKind::Branch,
                Some(branch.branch.clone()),
                None,
                &branch.project,
                Some(&branch.session),
                state,
            );
            branches.push((item, name));
        }
    }
    let removal = Removal {
        project: project.id.clone(),
        repo,
        survives,
        worktrees: taken,
        branches,
        held_back: untold,
        going,
    };
    Ok((!removal.is_empty()).then_some(removal))
}

/// Carries `removal` out once the project's folder has gone and its sessions `ids` have no
/// process, a locked worktree only where `forced` names it as it stands. Returns what is left;
/// the project's removal stands either way.
pub async fn carry_out_once_stopped(
    state: Arc<AppState>,
    ids: Vec<String>,
    removal: Removal,
    forced: Vec<SessionBranchTarget>,
) -> Vec<SessionBranchItem> {
    wait_until_stopped(&state, &ids).await;
    let unfinished = removal.clone_items();
    let done = tokio::task::spawn_blocking(move || {
        let tools = state.host_tools.get();
        match (tools.git)() {
            Ok(git) => {
                let repo = removal.repo.clone();
                state.branch_cleanups.serialized(&repo, || {
                    let mut removal = removal;
                    removal.look_again(&state);
                    carry_out(&git, removal, &forced, tools.trash)
                })
            }
            Err(err) => removal.clone_items().all_failed(&format!("{err:#}")),
        }
    })
    .await;
    done.unwrap_or_else(|err| unfinished.all_failed(&format!("the removal did not finish: {err}")))
}

/// The items of a [`Removal`], apart: what it would carry out, and what it holds back.
struct Items {
    carried: Vec<SessionBranchItem>,
    held_back: Vec<SessionBranchItem>,
}

impl Items {
    /// Every item left, for a removal that could not be carried out at all: what it would carry
    /// out as `failed` with `detail`; what it holds back as it is, whose own reason still holds.
    fn all_failed(self, detail: &str) -> Vec<SessionBranchItem> {
        self.carried
            .into_iter()
            .map(|item| failed(item, detail.to_string()))
            .chain(self.held_back)
            .collect()
    }
}

/// The outside worktrees go to the Trash first, a locked one unlocked after where the repository
/// survives, so that the prune after can drop its record. Then, where the repository survives, the
/// records the moves left behind are pruned and every branch no worktree still has checked out is
/// deleted. What the removal holds back is left.
pub(crate) fn carry_out(
    git: &Git,
    removal: Removal,
    forced: &[SessionBranchTarget],
    trash: impl Fn(&Path) -> Result<()>,
) -> Vec<SessionBranchItem> {
    let mut left: Vec<SessionBranchItem> = removal
        .held_back
        .into_iter()
        .map(|(item, _)| item)
        .collect();
    let mut kept: HashSet<PathBuf> = HashSet::new();
    for (mut item, root) in removal.worktrees {
        if item.state == SessionBranchState::Locked {
            let Some(target) = forced.iter().find(|target| names(target, &item)) else {
                kept.insert(root);
                continue;
            };
            // Read now rather than at the plan: the user agreed to what they were shown. One that
            // cannot be read counts as changes, which a force without them does not take.
            item.uncommitted = changes(git, &root).unwrap_or(true);
            if target.state != SessionBranchState::Locked
                || (item.uncommitted && !target.uncommitted)
            {
                left.push(item);
                continue;
            }
        }
        if let Err(err) = trash(&root) {
            if root.exists() {
                left.push(failed(item, format!("{err:#}")));
            }
            continue;
        }
        // Unlocked only once its directory is in the Trash, so a move that fails leaves the lock;
        // the prune below then drops its record.
        if item.state == SessionBranchState::Locked && removal.survives {
            let unlock = [
                OsStr::new("worktree"),
                OsStr::new("unlock"),
                OsStr::new("--"),
                root.as_os_str(),
            ];
            if let Err(detail) = git.write(&removal.repo, &unlock) {
                left.push(failed(item, detail));
            }
        }
    }
    if !removal.survives {
        return left;
    }
    if let Err(detail) = git.write(
        &removal.repo,
        &[OsStr::new("worktree"), OsStr::new("prune")],
    ) {
        tracing::warn!(%detail, "pruning the worktree records of a removed project failed");
    }
    let listed = match worktrees(git, &removal.repo) {
        Ok(listed) => listed,
        Err(detail) => {
            left.extend(
                removal
                    .branches
                    .into_iter()
                    .map(|(item, _)| failed(item, detail.clone())),
            );
            return left;
        }
    };
    for (mut item, name) in removal.branches {
        if let Some(holder) = listed
            .iter()
            .find(|worktree| worktree.branch.as_deref() == Some(name.as_slice()))
        {
            // Only a holder that was to go and did not is reported. One a locked worktree the user
            // chose to keep has checked out stays with it, as they were shown; one any other
            // worktree has checked out, the removal never took or no longer takes.
            if removal.going.contains(&holder.root) && !kept.contains(&holder.root) {
                item.state = SessionBranchState::CheckedOut;
                left.push(item);
            }
            continue;
        }
        let args = [
            OsStr::new("branch"),
            OsStr::new("-D"),
            OsStr::new("--"),
            OsStr::from_bytes(&name),
        ];
        if let Err(detail) = git.write(&removal.repo, &args) {
            if exists(git, &removal.repo, &name) {
                left.push(failed(item, detail));
            }
        }
    }
    left
}
