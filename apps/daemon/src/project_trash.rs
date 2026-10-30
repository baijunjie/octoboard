//! Which folder removing a project may move to the Trash: never one whose loss would be drastic,
//! nor one that would take another project's files along, as `trash_safety` decides it: the
//! refusals' codes and wording are this module's. A project's folder that cannot be told apart
//! from another's counts as shared with it, so what an inspection could not resolve is kept
//! ([`UnresolvedRemovals`]) for the removal confirmed after it to repeat.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::Result;
use uuid::Uuid;

use crate::hostfs::place::Looking;
use crate::protocol::{error_code, Code, CodedError, Params, Project};
use crate::state::AppState;
use crate::trash_safety::{
    looked_at_for_trash, Blocked, Failure, Overlap, Unresolved, TRASH_LOOK_DEADLINE,
};

/// A refusal of a move to the Trash, kept so that it can be said again.
#[derive(Clone, Debug)]
struct Refusal {
    code: Code,
    message: String,
    params: Params,
    /// Whether a path that could not be resolved decided it, so that another look may decide
    /// otherwise.
    unresolved: bool,
}

impl Refusal {
    fn new(code: Code, message: String, pairs: &[(&str, &str)], unresolved: bool) -> Self {
        Self {
            code,
            message,
            params: pairs
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
            unresolved,
        }
    }

    fn error(&self) -> anyhow::Error {
        anyhow::Error::new(CodedError {
            code: self.code,
            message: self.message.clone(),
            params: self.params.clone(),
        })
    }
}

/// What removal inspections answered that a path that could not be resolved decided, each by the
/// token the answer carried (`inspection`), for the removal confirmed after that inspection to
/// repeat: a refusal, once ([`directory_to_trash`]), or the worktrees outside the folder it held
/// back because whether they are or hold another project's folder could not be told
/// (`crate::branch_origin::cleanup::project`), for every removal that sends the token until one
/// stands. Held in memory only. A token goes when a removal uses its refusal, when its project is
/// removed, or when its project holds [`KEPT_PER_PROJECT`] newer ones.
#[derive(Default)]
pub struct UnresolvedRemovals(Mutex<Kept>);

#[derive(Default)]
struct Kept {
    by_token: HashMap<String, KeptAnswer>,
    /// Numbers the answers in the order they were kept, so the oldest of a project goes first.
    issued: u64,
}

struct KeptAnswer {
    project: String,
    answer: Answer,
    issued: u64,
}

/// What an inspection answered, as kept for the removal after it.
enum Answer {
    Refused(Refusal),
    /// The roots of the worktrees held back, as `git worktree list` gives them.
    HeldBack(Vec<PathBuf>),
}

/// How many kept answers one project holds at most: one per window that inspected its removal
/// lately is plenty, and an inspection repeated without a removal must not grow the store.
const KEPT_PER_PROJECT: usize = 4;

impl UnresolvedRemovals {
    /// Drops what is kept for `project`, once it is removed.
    pub fn forget(&self, project: &str) {
        self.lock()
            .by_token
            .retain(|_, kept| kept.project != project);
    }

    /// Keeps the worktree roots `held_back` for `project`, as [`UnresolvedRemovals::keep`] does,
    /// for [`UnresolvedRemovals::held_back`] to read.
    pub(crate) fn keep_held_back(
        &self,
        state: &AppState,
        project: &str,
        held_back: Vec<PathBuf>,
    ) -> Option<String> {
        self.keep(state, project, Answer::HeldBack(held_back))
    }

    /// The worktree roots kept under `token` for `project`; none where the token is unknown,
    /// another project's, or a refusal's. They stay kept until the project is removed, so a
    /// removal that fails before it stands and is tried again holds them back too.
    pub(crate) fn held_back(&self, token: &str, project: &str) -> Vec<PathBuf> {
        match self.lock().by_token.get(token) {
            Some(KeptAnswer {
                project: of,
                answer: Answer::HeldBack(roots),
                ..
            }) if of == project => roots.clone(),
            _ => Vec::new(),
        }
    }

    /// Keeps `answer` for `project` under a new token, which it returns, unless `project` is no
    /// longer stored: an inspection that ends after its project was removed keeps nothing. The
    /// project is checked under this store's lock, which its removal takes after deleting it, so
    /// either the check sees it gone or the removal drops what was kept.
    fn keep(&self, state: &AppState, project: &str, answer: Answer) -> Option<String> {
        let mut kept = self.lock();
        if !matches!(state.store.get_project(project), Ok(Some(_))) {
            return None;
        }
        let of_project = |kept: &Kept| {
            kept.by_token
                .iter()
                .filter(|(_, entry)| entry.project == project)
                .map(|(token, entry)| (token.clone(), entry.issued))
                .collect::<Vec<_>>()
        };
        let mut held = of_project(&kept);
        held.sort_by_key(|(_, issued)| *issued);
        for (token, _) in held
            .iter()
            .take((held.len() + 1).saturating_sub(KEPT_PER_PROJECT))
        {
            kept.by_token.remove(token);
        }
        kept.issued += 1;
        let token = Uuid::new_v4().to_string();
        let issued = kept.issued;
        kept.by_token.insert(
            token.clone(),
            KeptAnswer {
                project: project.to_string(),
                answer,
                issued,
            },
        );
        Some(token)
    }

    /// The refusal kept under `token` for `project`, which it no longer keeps.
    fn take_refusal(&self, token: &str, project: &str) -> Option<Refusal> {
        let mut kept = self.lock();
        match kept.by_token.get(token) {
            Some(KeptAnswer {
                project: of,
                answer: Answer::Refused(_),
                ..
            }) if of == project => match kept.by_token.remove(token)?.answer {
                Answer::Refused(refusal) => Some(refusal),
                Answer::HeldBack(_) => None,
            },
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.lock().by_token.len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Kept> {
        self.0.lock().expect("unresolved removals lock poisoned")
    }
}

/// The directory to move to the Trash for `deleted`, as `inspect_project_removal` decides it
/// ([`judged`]). A refusal that a path that could not be resolved decided is kept, and carries the
/// token it is kept under as its `inspection` param, for the removal confirmed after this
/// inspection to repeat.
pub async fn inspected(
    state: &AppState,
    deleted: &Project,
    projects: &[Project],
) -> Result<Option<PathBuf>> {
    let home = crate::paths::known_home_dir();
    match judged(state, deleted, projects, home, crate::paths::data_dir()).await {
        Err(mut refusal) if refusal.unresolved => {
            let kept = Answer::Refused(refusal.clone());
            if let Some(token) = state.unresolved_removals.keep(state, &deleted.id, kept) {
                refusal.params.insert("inspection".to_string(), token);
            }
            Err(refusal.error())
        }
        judged => judged.map_err(|refusal| refusal.error()),
    }
}

/// The directory to move to the Trash for `deleted`, as `delete_project` decides it. With the
/// `inspection` token of an inspection's refusal that a path that could not be resolved decided,
/// that refusal, even once the path resolves: the person confirmed a removal that was shown to take
/// nothing along, and a look now could take along what was never shown. The token is then used up.
/// Otherwise, with no token, one no longer kept, or one that kept the worktrees an inspection held
/// back (left for [`UnresolvedRemovals::held_back`]), [`judged`].
pub async fn directory_to_trash(
    state: &AppState,
    deleted: &Project,
    projects: &[Project],
    inspection: Option<&str>,
) -> Result<Option<PathBuf>> {
    let kept =
        inspection.and_then(|token| state.unresolved_removals.take_refusal(token, &deleted.id));
    if let Some(refusal) = kept {
        return Err(refusal.error());
    }
    let home = crate::paths::known_home_dir();
    judged(state, deleted, projects, home, crate::paths::data_dir())
        .await
        .map_err(|refusal| refusal.error())
}

/// The directory to move to the Trash for `deleted`, or `None` when nothing is at its path, with
/// `home` and `data` the home directory and Octoboard's data directory. Refused for a directory
/// whose loss would be drastic (see [`PROJECT_DIRECTORY_PROTECTED`]), and while another project's
/// directory is the same, inside it or holds it, or another project's Git repository lies inside
/// it, as a linked worktree's does: trashing would take that project's files too ([`Overlap`]).
///
/// A path that cannot be resolved refuses the move as `trash_safety::looked_at_for_trash` says:
/// as shared with that project when it is another project's, and with
/// [`PROJECT_DIRECTORY_TRASH_FAILED`] when it is the folder itself, the home directory or
/// Octoboard's data directory, its `cause` `unanswered` where the look gave no answer in time and
/// `unreadable` where permission to read the folder itself was refused ([`Failure`]). Each look
/// is bounded by [`TRASH_LOOK_DEADLINE`], and one stuck from an earlier look is not waited for.
///
/// [`PROJECT_DIRECTORY_PROTECTED`]: error_code::PROJECT_DIRECTORY_PROTECTED
/// [`PROJECT_DIRECTORY_TRASH_FAILED`]: error_code::PROJECT_DIRECTORY_TRASH_FAILED
async fn judged(
    state: &AppState,
    deleted: &Project,
    projects: &[Project],
    home: Option<PathBuf>,
    data: PathBuf,
) -> Result<Option<PathBuf>, Refusal> {
    let others: Vec<&Project> = projects
        .iter()
        .filter(|other| other.id != deleted.id)
        .collect();
    let written = PathBuf::from(&deleted.path);
    let in_the_way: Vec<(PathBuf, Overlap)> = others
        .iter()
        .map(|other| (PathBuf::from(&other.path), Overlap::Whole))
        .collect();
    let unlooked = |what: Unresolved, failure: Failure| {
        let (failed, cause) = failure.described();
        let detail = match what {
            Unresolved::Folder => format!(
                "it {failed}, so whether another project's files are in it could not be told"
            ),
            Unresolved::Home => {
                format!("the home folder {failed}, so whether this is it could not be told")
            }
            Unresolved::Data => format!(
                "Octoboard's data folder {failed}, so whether it is in this folder could not be told"
            ),
        };
        let mut pairs = vec![("path", deleted.path.as_str()), ("detail", detail.as_str())];
        pairs.extend(cause.map(|cause| ("cause", cause)));
        Refusal::new(
            error_code::PROJECT_DIRECTORY_TRASH_FAILED,
            format!("could not move {} to the Trash: {detail}", deleted.path),
            &pairs,
            true,
        )
    };
    match looked_at_for_trash(
        state,
        Looking::within(TRASH_LOOK_DEADLINE),
        &written,
        &in_the_way,
        home,
        data,
    )
    .await
    {
        // A symlink is trashed as itself, not as what it points to.
        Ok(directory) => Ok(directory),
        Err(Blocked::Unlooked(what, failure)) => Err(unlooked(what, failure)),
        Err(Blocked::Protected) => Err(Refusal::new(
            error_code::PROJECT_DIRECTORY_PROTECTED,
            "this folder is too important to move to the Trash from here".to_string(),
            &[("path", &deleted.path)],
            false,
        )),
        Err(Blocked::InTheWay { index, unresolved }) => Err(Refusal::new(
            error_code::PROJECT_DIRECTORY_SHARED,
            format!(
                "the folder is shared with project \"{}\"; remove that project first",
                others[index].name
            ),
            &[("project", &others[index].name)],
            unresolved.is_some(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::path::Path;
    use std::sync::Arc;

    use super::*;
    use crate::hostfs::lexically_normalise;
    use crate::protocol::{ProjectAccount, ProjectSource};
    use crate::store::LOCAL_HOST_ID;
    use crate::test_support::{app_state, git, repo_with, second_real_path};

    /// A project named `id` stored at `path`.
    fn project_at(id: &str, path: &Path) -> Project {
        Project {
            id: id.to_string(),
            console_id: "console-1".to_string(),
            host_id: LOCAL_HOST_ID.to_string(),
            name: id.to_string(),
            path: path.to_string_lossy().into_owned(),
            default_agent: None,
            source: ProjectSource::Local,
            remote_url: None,
            claude_account_id: ProjectAccount::Inherit,
            codex_account_id: ProjectAccount::Inherit,
            grok_account_id: ProjectAccount::Inherit,
            claude_session_model: None,
            claude_session_effort: None,
            codex_session_model: None,
            codex_session_effort: None,
            grok_session_model: None,
            grok_session_effort: None,
            trust_consent: false,
            pinned: false,
            tags: Vec::new(),
            description: None,
            git_workflow: Default::default(),
        }
    }

    /// [`judged`] as an error a request answers with.
    async fn directory_to_trash_with(
        state: &AppState,
        deleted: &Project,
        projects: &[Project],
        home: Option<PathBuf>,
        data: PathBuf,
    ) -> Result<Option<PathBuf>> {
        judged(state, deleted, projects, home, data)
            .await
            .map_err(|refusal| refusal.error())
    }

    /// The refusal `result` holds.
    fn coded(result: Result<Option<PathBuf>>) -> CodedError {
        result.unwrap_err().downcast::<CodedError>().unwrap()
    }

    /// The refusal `result` holds: its code and its message.
    fn refusal(result: Result<Option<PathBuf>>) -> (Code, String) {
        let err = coded(result);
        (err.code, err.message)
    }

    /// The code `directory_to_trash` refuses `deleted` with, among `others`.
    async fn trash_refusal(state: &AppState, deleted: &Project, others: &[Project]) -> Code {
        refusal(directory_to_trash(state, deleted, others, None).await).0
    }

    /// Records a look at `path` as still stuck for as long as `stuck` lives, so the path counts as
    /// one whose look outran its deadline.
    fn stall(state: &AppState, path: &Path, stuck: &Arc<()>) {
        state
            .directory_looks
            .record(lexically_normalise(path), Arc::downgrade(stuck));
    }

    /// Trashing is refused for a protected directory, and over any other project's directory
    /// that is the same, inside or holding this one, however its path is spelled, whose
    /// repository is inside it, as a linked worktree's is, or that is a file inside it, and a file
    /// reached through a linked folder is the file another project is at; a sibling whose name
    /// merely starts the same, or another file, is not in the way, a directory already gone is
    /// skipped, and a folder reached through a link is trashed as the link.
    #[tokio::test]
    async fn trashing_a_directory_leaves_every_other_projects_files_alone() {
        let (state, dir) = app_state("project-trash-guard");
        let outer = dir.join("outer");
        let inner = outer.join("inner");
        let sibling = dir.join("outer-2");
        for path in [&inner, &sibling] {
            std::fs::create_dir_all(path).unwrap();
        }
        for file in [outer.join("file"), sibling.join("file")] {
            std::fs::write(file, b"").unwrap();
        }
        let linked = dir.join("linked");
        symlink(&outer, &linked).unwrap();
        let outer_project = project_at("outer", &outer);
        let inner_project = project_at("inner", &inner);
        for (deleted, other) in [
            (&outer_project, inner_project.clone()),
            (&inner_project, outer_project.clone()),
            (&outer_project, project_at("twin", &outer)),
            (&outer_project, project_at("linked", &linked)),
            (
                &outer_project,
                project_at("linked-inner", &linked.join("inner")),
            ),
            (&outer_project, project_at("file", &outer.join("file"))),
        ] {
            assert_eq!(
                trash_refusal(&state, deleted, &[deleted.clone(), other.clone()]).await,
                error_code::PROJECT_DIRECTORY_SHARED,
                "{}",
                other.path
            );
        }

        // A linked worktree elsewhere keeps its repository in the folder.
        let repo = repo_with("project-trash-repo", &[(b"a", b"a")]);
        let worktree = dir.join("worktree");
        git(
            &repo,
            &["worktree", "add", "-b", "other", worktree.to_str().unwrap()],
        );
        assert_eq!(
            trash_refusal(
                &state,
                &project_at("repo", &repo),
                &[project_at("worktree", &worktree)]
            )
            .await,
            error_code::PROJECT_DIRECTORY_SHARED
        );

        let home = crate::paths::home_dir();
        for protected in [Path::new("/"), home.as_path()] {
            assert_eq!(
                trash_refusal(&state, &project_at("protected", protected), &[]).await,
                error_code::PROJECT_DIRECTORY_PROTECTED
            );
        }

        // A file reached through a linked folder is the file another project is at.
        let through_link = project_at("through-link", &linked.join("file"));
        assert_eq!(
            trash_refusal(
                &state,
                &through_link,
                &[project_at("file", &outer.join("file"))]
            )
            .await,
            error_code::PROJECT_DIRECTORY_SHARED
        );

        let others = [
            project_at("sibling", &sibling),
            project_at("file-elsewhere", &sibling.join("file")),
        ];
        assert_eq!(
            directory_to_trash(&state, &through_link, &others, None)
                .await
                .unwrap(),
            Some(linked.join("file")),
            "another file is another entry"
        );
        assert_eq!(
            directory_to_trash(&state, &outer_project, &others, None)
                .await
                .unwrap(),
            Some(outer.clone())
        );
        assert_eq!(
            directory_to_trash(&state, &project_at("linked", &linked), &others, None)
                .await
                .unwrap(),
            Some(linked.clone()),
            "the link, not the folder it leads to"
        );
        let gone = project_at("gone", &dir.join("gone"));
        assert_eq!(
            directory_to_trash(&state, &gone, &others, None)
                .await
                .unwrap(),
            None
        );
    }

    /// One directory with two real paths is one directory: on macOS the data volume's firmlinks
    /// give everything under `/private` and `/Users` a second real path below
    /// `/System/Volumes/Data`, as a bind mount does on Linux. That holds for another project's
    /// folder, for the home directory and for Octoboard's data. Run on macOS only, where the
    /// scratch directory always has such a second path.
    #[tokio::test]
    async fn trashing_tells_a_directory_by_what_it_is_not_by_its_real_path() {
        let (state, dir) = app_state("project-trash-firmlink");
        let outer = std::fs::canonicalize(&*dir).unwrap().join("outer");
        let inner = outer.join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        let Some(second) = second_real_path(&outer) else {
            if cfg!(target_os = "macos") {
                panic!("{} has no second real path", outer.display());
            }
            return;
        };
        for (deleted, other) in [
            (&outer, second.join("inner")),
            (&inner, second.clone()),
            (&outer, second.clone()),
        ] {
            let deleted = project_at("deleted", deleted);
            assert_eq!(
                trash_refusal(&state, &deleted, &[project_at("other", &other)]).await,
                error_code::PROJECT_DIRECTORY_SHARED,
                "{} beside {}",
                deleted.path,
                other.display()
            );
        }

        // The home directory, and a folder holding Octoboard's data, given by their second paths.
        let elsewhere = dir.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let kept = project_at("kept", &outer);
        for (home, data) in [
            (second.clone(), elsewhere.clone()),
            (elsewhere.clone(), second.join("inner")),
        ] {
            let result = directory_to_trash_with(&state, &kept, &[], Some(home), data).await;
            assert_eq!(refusal(result).0, error_code::PROJECT_DIRECTORY_PROTECTED);
        }
        assert_eq!(
            directory_to_trash_with(
                &state,
                &kept,
                &[],
                Some(elsewhere.clone()),
                elsewhere.clone()
            )
            .await
            .unwrap(),
            Some(outer.clone()),
            "neither the home directory nor the data is in the folder"
        );
    }

    /// A path whose look outran its deadline counts as overlapping, so the move is refused: as
    /// shared with that project when it is another's, a project known to overlap being named
    /// first, and as a failed move when it is the folder itself, the home directory or the data
    /// directory, unless the folder is a disk's root, which is protected first. Another path with
    /// nothing at it is compared by its text instead.
    #[tokio::test]
    async fn trashing_is_refused_over_a_path_whose_look_outran_its_deadline() {
        let (state, dir) = app_state("project-trash-unresolved");
        let folder = dir.join("folder");
        let elsewhere = dir.join("elsewhere");
        let home = dir.join("home");
        let data = dir.join("data");
        for path in [&folder.join("inner"), &elsewhere, &home, &data] {
            std::fs::create_dir_all(path).unwrap();
        }
        let deleted = project_at("deleted", &folder);
        let with = |others: Vec<Project>, home: &Path, data: &Path| {
            let projects: Vec<Project> = [deleted.clone()].into_iter().chain(others).collect();
            let (home, data) = (home.to_path_buf(), data.to_path_buf());
            let (state, deleted) = (&state, &deleted);
            async move { directory_to_trash_with(state, deleted, &projects, Some(home), data).await }
        };

        let missing_elsewhere = project_at("missing", &dir.join("missing"));
        assert_eq!(
            with(vec![missing_elsewhere], &dir.join("no-home"), &data)
                .await
                .unwrap(),
            Some(folder.clone()),
            "nothing at another project's path or at the home directory, outside the folder"
        );
        let missing_inside = project_at("missing", &folder.join("missing"));
        assert_eq!(
            refusal(with(vec![missing_inside], &home, &data).await).0,
            error_code::PROJECT_DIRECTORY_SHARED,
            "nothing at another project's path, inside the folder by its text"
        );
        assert_eq!(
            refusal(with(vec![], &home, &folder.join("no-data")).await).0,
            error_code::PROJECT_DIRECTORY_PROTECTED,
            "nothing at the data directory, inside the folder by its text"
        );

        let stuck = Arc::new(());
        stall(&state, &elsewhere, &stuck);
        let others = vec![
            project_at("stuck", &elsewhere),
            project_at("inner", &folder.join("inner")),
        ];
        let (code, message) = refusal(with(others, &home, &data).await);
        assert_eq!(code, error_code::PROJECT_DIRECTORY_SHARED);
        assert!(message.contains("\"inner\""), "{message}");
        let (code, message) =
            refusal(with(vec![project_at("stuck", &elsewhere)], &home, &data).await);
        assert_eq!(code, error_code::PROJECT_DIRECTORY_SHARED);
        assert!(message.contains("\"stuck\""), "{message}");

        for (stalled, what) in [(&home, "home"), (&data, "data"), (&folder, "folder")] {
            // Each held alone: the stall ends with its guard.
            let stuck = Arc::new(());
            stall(&state, stalled, &stuck);
            let err = coded(with(vec![], &home, &data).await);
            assert_eq!(
                err.code,
                error_code::PROJECT_DIRECTORY_TRASH_FAILED,
                "{what}"
            );
            assert_eq!(
                err.params.get("cause").map(String::as_str),
                Some("unanswered")
            );
            assert!(
                err.params["detail"].contains("did not respond in time"),
                "{what}"
            );
        }
        // The root of a disk is protected for what it is, whatever cannot be looked at.
        let stuck = Arc::new(());
        stall(&state, &home, &stuck);
        stall(&state, &data, &stuck);
        let root = project_at("root", Path::new("/"));
        let result = directory_to_trash_with(&state, &root, &[], Some(home.clone()), data).await;
        assert_eq!(refusal(result).0, error_code::PROJECT_DIRECTORY_PROTECTED);
    }

    /// Another project at a path that cannot be read, as a folder whose permissions refuse its
    /// owner, or macOS's privacy controls, refuse it, is compared by its text, so it neither
    /// blocks every move nor is taken along when its text lies in the folder; the folder to be
    /// moved itself, when it cannot be read, refuses with the cause `unreadable`. Skipped as root,
    /// who can read it.
    #[tokio::test]
    async fn another_project_that_cannot_be_read_is_compared_by_its_text() {
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("skipped: root reads a folder whatever its permissions");
            return;
        }
        let (state, dir) = app_state("project-trash-unreadable");
        let folder = dir.join("folder");
        let locked = [folder.join("locked"), dir.join("locked")];
        for path in &locked {
            std::fs::create_dir_all(path.join("project")).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).unwrap();
        }
        let deleted = project_at("deleted", &folder);
        let inside = directory_to_trash(
            &state,
            &deleted,
            &[project_at("inside", &locked[0].join("project"))],
            None,
        )
        .await;
        // The folder itself cannot be read: refused, for that cause.
        let own = directory_to_trash(
            &state,
            &project_at("own", &locked[1].join("project")),
            &[],
            None,
        )
        .await;
        let outside = directory_to_trash(
            &state,
            &deleted,
            &[project_at("outside", &locked[1].join("project"))],
            None,
        )
        .await;
        for path in &locked {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(refusal(inside).0, error_code::PROJECT_DIRECTORY_SHARED);
        assert_eq!(outside.unwrap(), Some(folder.clone()));
        let own = coded(own);
        assert_eq!(own.code, error_code::PROJECT_DIRECTORY_TRASH_FAILED);
        assert_eq!(
            own.params.get("cause").map(String::as_str),
            Some("unreadable")
        );
        assert!(
            own.params["detail"].contains("could not be read"),
            "{:?}",
            own.params
        );
    }

    /// A removal given the token of an inspection's refusal over a path that could not be resolved
    /// repeats that refusal once the path resolves, and uses the token up; one with no token, or one
    /// no longer kept, looks for itself. A project keeps a few tokens at most, its removal takes them
    /// along, and an inspection that ends after its project is gone keeps none.
    #[tokio::test]
    async fn a_removal_repeats_the_refusal_its_unresolved_inspection_answered() {
        let (state, dir) = app_state("project-trash-kept");
        let folder = dir.join("folder");
        let elsewhere = dir.join("elsewhere");
        for path in [&folder, &elsewhere] {
            std::fs::create_dir_all(path).unwrap();
        }
        let deleted = project_at("deleted", &folder);
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        state.store.insert_project(&deleted).unwrap();
        let projects = [deleted.clone(), project_at("elsewhere", &elsewhere)];
        let removal = |token: Option<&str>| {
            let token = token.map(str::to_string);
            let (state, deleted, projects) = (&state, &deleted, &projects);
            async move { directory_to_trash(state, deleted, projects, token.as_deref()).await }
        };
        let inspect = || async {
            let stuck = Arc::new(());
            stall(&state, &elsewhere, &stuck);
            coded(inspected(&state, &deleted, &projects).await)
        };

        let inspection = inspect().await;
        assert_eq!(inspection.code, error_code::PROJECT_DIRECTORY_SHARED);
        let token = inspection.params["inspection"].as_str();
        assert_eq!(
            removal(None).await.unwrap(),
            Some(folder.clone()),
            "no token"
        );
        let repeated = coded(removal(Some(token)).await);
        assert_eq!(
            (repeated.code, &repeated.message),
            (inspection.code, &inspection.message)
        );
        assert_eq!(
            removal(Some(token)).await.unwrap(),
            Some(folder.clone()),
            "used up"
        );
        assert_eq!(
            removal(Some("unknown")).await.unwrap(),
            Some(folder.clone())
        );

        for _ in 0..KEPT_PER_PROJECT + 2 {
            inspect().await;
        }
        assert_eq!(state.unresolved_removals.len(), KEPT_PER_PROJECT);
        state.unresolved_removals.forget(&deleted.id);
        assert_eq!(state.unresolved_removals.len(), 0);

        state.store.delete_project(&deleted.id).unwrap();
        assert_eq!(inspect().await.params.get("inspection"), None);
        assert_eq!(state.unresolved_removals.len(), 0);
    }
}
