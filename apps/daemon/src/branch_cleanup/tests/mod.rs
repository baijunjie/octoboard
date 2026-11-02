mod agent;
mod other_projects;
mod project_removal;
mod removal;
mod requests;

use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use super::project;
use super::remove::*;
use super::report::*;
use super::survey::*;
use crate::branch_origin::bound::bound_in;
use crate::git::repo_files::repository_of;
use crate::protocol::{
    error_code, Agent, Event, OrchestrationRole, Origin, RequestBody, Role, Session,
    SessionBranchCause, SessionBranchItem, SessionBranchKind, SessionBranchState,
    SessionBranchTarget, SessionStatus,
};
use crate::state::AppState;
use crate::store::LOCAL_HOST_ID;
use crate::test_support::{
    app_state, branch_made_by, console, fixture_git, git, host_tools, insert_project, repo_with,
    running, ScratchDir, PATIENCE,
};
use crate::time::now_millis;
use crate::wire_path;

/// A project session `id` of [`insert_project`]'s project, in `status`.
fn session(id: &str, status: SessionStatus) -> Session {
    Session {
        id: id.to_string(),
        agent: Agent::Claude,
        agent_session_id: None,
        console_id: "console-1".to_string(),
        project_id: Some("p1".to_string()),
        host_id: LOCAL_HOST_ID.to_string(),
        role: Role::Project,
        origin: Origin::User,
        title: format!("Session {id}"),
        status,
        terminal_title: None,
        record_titles: Default::default(),
        background_tasks: Vec::new(),
        switching_account: false,
        paused: false,
        prompt_on_launch: false,
        default_effort: None,
        orchestration_role: OrchestrationRole::Coordinator,
        context: None,
        observed_model: None,
        observed_effort: None,
        model: None,
        effort: None,
        has_conversation: true,
        bound_to: None,
        colour: None,
        ordinal: None,
        account_id: None,
        config_dir: None,
        pinned: false,
        locked: false,
        started_at: now_millis(),
        ended_at: None,
    }
}

/// A store holding the project `p1` over a fresh repository.
struct Fixture {
    state: Arc<AppState>,
    repo: ScratchDir,
    worktrees: ScratchDir,
    _dir: ScratchDir,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let fixture = Self::without_project(label);
        insert_project(&fixture.state, &fixture.repo);
        fixture
    }

    /// The store and the repository, with the project `p1` left for the test to place.
    fn without_project(label: &str) -> Self {
        let (state, dir) = app_state(label);
        state.store.insert_console(&console(&dir)).unwrap();
        let repo = repo_with(&format!("{label}-repo"), &[(b"a.txt", b"one\n")]);
        Self {
            state,
            repo,
            worktrees: ScratchDir::new(&format!("{label}-worktrees")),
            _dir: dir,
        }
    }

    /// Stores the project session `id` in `status`.
    fn store(&self, id: &str, status: SessionStatus) -> Session {
        let session = session(id, status);
        self.state.store.insert_session(&session).unwrap();
        session
    }

    /// A linked worktree with `branch`, made by `session`, checked out.
    fn worktree(&self, branch: &str, session: &str) -> PathBuf {
        branch_made_by(&self.state, &self.repo, branch, session);
        let root = self.worktrees.join(branch);
        git(
            &self.repo,
            &["worktree", "add", "-q", root.to_str().unwrap(), branch],
        );
        root
    }

    fn lock(&self, root: &Path) {
        git(&self.repo, &["worktree", "lock", root.to_str().unwrap()]);
    }

    fn has_branch(&self, branch: &str) -> bool {
        fixture_git(&self.repo)
            .args([
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ])
            .status()
            .unwrap()
            .success()
    }

    async fn inspect(&self, sessions: &[Session]) -> Vec<SessionBranchItem> {
        let (state, sessions) = (self.state.clone(), sessions.to_vec());
        tokio::task::spawn_blocking(move || inspect(&state, &sessions))
            .await
            .unwrap()
            .unwrap()
    }

    async fn remove(
        &self,
        sessions: &[Session],
        forced: &[SessionBranchTarget],
    ) -> Vec<SessionBranchItem> {
        remove_once_stopped(self.state.clone(), sessions.to_vec(), forced.to_vec()).await
    }
}

/// Each item as `(is a worktree, branch, state)`, sorted.
fn summary(items: &[SessionBranchItem]) -> Vec<(bool, String, SessionBranchState)> {
    let mut summary: Vec<_> = items
        .iter()
        .map(|item| {
            (
                item.kind == SessionBranchKind::Worktree,
                item.branch.clone().unwrap_or_default(),
                item.state,
            )
        })
        .collect();
    summary.sort_by(|a, b| (!a.0, &a.1).cmp(&(!b.0, &b.1)));
    summary
}

fn expect(items: &[(bool, &str, SessionBranchState)]) -> Vec<(bool, String, SessionBranchState)> {
    items
        .iter()
        .map(|(worktree, branch, state)| (*worktree, branch.to_string(), *state))
        .collect()
}

/// The item as a confirmation that showed it names it back.
fn target(item: &SessionBranchItem) -> SessionBranchTarget {
    SessionBranchTarget {
        kind: item.kind,
        repository: item.repository.clone(),
        branch: item.branch.clone(),
        path: item.path.clone(),
        state: item.state,
        uncommitted: item.uncommitted,
    }
}

/// Commits a change in `dir`, so its branch is ahead of `main`.
fn commit_in(dir: &Path) {
    std::fs::write(dir.join("a.txt"), "two\n").unwrap();
    git(dir, &["commit", "-qam", "two"]);
}

/// Waits for `seen` to say so, for up to [`PATIENCE`].
async fn until(mut seen: impl FnMut() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !seen() {
        assert!(Instant::now() < deadline, "not seen in time");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The items of the next `session_branches_not_removed` broadcast, for up to [`PATIENCE`].
async fn reported(events: &mut tokio::sync::broadcast::Receiver<Event>) -> Value {
    let deadline = tokio::time::Instant::now() + PATIENCE;
    loop {
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Ok(Ok(Event::Error { code, params, .. }))
                if code == error_code::SESSION_BRANCHES_NOT_REMOVED =>
            {
                return serde_json::from_str(&params["items"]).unwrap();
            }
            Ok(Ok(_)) => continue,
            other => panic!("no report: {other:?}"),
        }
    }
}

fn project_p1(state: &AppState) -> crate::protocol::Project {
    state.store.get_project("p1").unwrap().unwrap()
}

fn listed_worktrees(repo: &Path) -> String {
    String::from_utf8(git(repo, &["worktree", "list", "--porcelain"])).unwrap()
}

/// Runs `work`, a blocking function of the cleanup, as the daemon runs it: on a thread inside a
/// runtime, whose blocking threads the looks it waits for run on.
fn blocking<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let handle = runtime.handle().clone();
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let _entered = handle.enter();
                work()
            })
            .join()
            .unwrap()
    })
}

fn carry_out_now(
    fixture: &Fixture,
    removal: project::Removal,
    forced: &[SessionBranchTarget],
) -> Vec<SessionBranchItem> {
    let tools = fixture.state.host_tools.get();
    project::carry_out(
        &super::cleanup_git(&tools).unwrap(),
        removal,
        forced,
        tools.trash,
    )
}

/// Stores the project `p2` at `path`, in place of any `p2` stored before.
fn other_project_at(state: &AppState, path: &Path) {
    state.store.delete_project("p2").ok();
    let mut other = project_p1(state);
    other.id = "p2".to_string();
    other.path = path.to_string_lossy().into_owned();
    state.store.insert_project(&other).unwrap();
}
