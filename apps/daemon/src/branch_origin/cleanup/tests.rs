use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use serde_json::Map;

use super::*;
use crate::branch_origin::repository_of;
use crate::protocol::{error_code, now_millis, Agent, Event, Origin, RequestBody, Role};
use crate::store::LOCAL_HOST_ID;
use crate::test_support::{
    app_state, branch_made_by, console, fixture_git, git, host_tools, insert_project, repo_with,
    running, ScratchDir, PATIENCE,
};

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
        default_effort: None,
        context: None,
        observed_model: None,
        observed_effort: None,
        model: None,
        effort: None,
        has_conversation: true,
        bound_to: None,
        lead: false,
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

    fn inspect(&self, sessions: &[Session]) -> Vec<SessionBranchItem> {
        inspect(&self.state, sessions).unwrap()
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

// -- what goes and what stays ------------------------------------------------------

#[tokio::test]
async fn a_clean_worktree_and_its_merged_branch_go_and_another_sessions_stay() {
    let fixture = Fixture::new("cleanup-safe");
    let root = fixture.worktree("mine", "s1");
    branch_made_by(&fixture.state, &fixture.repo, "theirs", "s2");
    let archived = [session("s1", SessionStatus::Archived)];

    assert_eq!(
        summary(&fixture.inspect(&archived)),
        expect(&[
            (true, "mine", SessionBranchState::Removable),
            (false, "mine", SessionBranchState::Removable),
        ])
    );
    assert!(fixture.remove(&archived, &[]).await.is_empty());
    assert!(!root.exists());
    assert!(!fixture.has_branch("mine"));
    assert!(fixture.has_branch("theirs"));
}

#[tokio::test]
async fn uncommitted_changes_and_an_unmerged_branch_go_only_when_forced() {
    let fixture = Fixture::new("cleanup-forced");
    let root = fixture.worktree("work", "s1");
    commit_in(&root);
    std::fs::write(root.join("new.txt"), "untracked\n").unwrap();
    let archived = [session("s1", SessionStatus::Archived)];

    let shown = fixture.inspect(&archived);
    assert_eq!(
        summary(&shown),
        expect(&[
            (true, "work", SessionBranchState::UncommittedChanges),
            (false, "work", SessionBranchState::Unmerged),
        ])
    );
    assert_eq!(fixture.remove(&archived, &[]).await.len(), 2);
    assert!(root.join("new.txt").exists());
    assert!(fixture.has_branch("work"));

    let forced: Vec<SessionBranchTarget> = shown.iter().map(target).collect();
    assert!(fixture.remove(&archived, &forced).await.is_empty());
    assert!(!root.exists());
    assert!(!fixture.has_branch("work"));
}

#[tokio::test]
async fn what_git_must_keep_stays_even_when_forced_and_a_locked_worktree_goes_only_when_forced() {
    let fixture = Fixture::new("cleanup-kept");
    // Checked out in the main worktree.
    branch_made_by(&fixture.state, &fixture.repo, "in-main", "s1");
    git(&fixture.repo, &["switch", "-q", "in-main"]);
    let locked = fixture.worktree("locked", "s1");
    fixture.lock(&locked);
    // A worktree that is another project's folder, locked too.
    let folder = fixture.worktree("folder", "s1");
    fixture.lock(&folder);
    let mut other = fixture.state.store.get_project("p1").unwrap().unwrap();
    other.id = "p2".to_string();
    other.path = folder.to_string_lossy().into_owned();
    fixture.state.store.insert_project(&other).unwrap();
    let archived = [session("s1", SessionStatus::Archived)];

    let left = fixture.remove(&archived, &[]).await;
    assert_eq!(
        summary(&left),
        expect(&[
            (true, "folder", SessionBranchState::ProjectFolder),
            (true, "locked", SessionBranchState::Locked),
            (false, "folder", SessionBranchState::CheckedOut),
            (false, "in-main", SessionBranchState::CheckedOut),
            (false, "locked", SessionBranchState::CheckedOut),
        ])
    );
    assert!(locked.exists());

    let forced: Vec<SessionBranchTarget> = left.iter().map(target).collect();
    assert_eq!(fixture.remove(&archived, &forced).await.len(), 3);
    assert!(!locked.exists() && !fixture.has_branch("locked"));
    assert!(folder.exists() && fixture.has_branch("folder") && fixture.has_branch("in-main"));
}

#[tokio::test]
async fn a_force_goes_no_further_than_what_was_shown() {
    let fixture = Fixture::new("cleanup-consent");
    let changed = fixture.worktree("changed", "s1");
    std::fs::write(changed.join("new.txt"), "untracked\n").unwrap();
    let locked = fixture.worktree("locked", "s1");
    fixture.lock(&locked);
    let archived = [session("s1", SessionStatus::Archived)];
    let shown: Vec<SessionBranchTarget> = fixture
        .inspect(&archived)
        .iter()
        .filter(|item| item.kind == SessionBranchKind::Worktree)
        .map(target)
        .collect();
    assert!(shown.iter().all(
        |target| !target.uncommitted || target.state == SessionBranchState::UncommittedChanges
    ));

    // Since the confirmation: the one with changes was locked, the locked one gained changes.
    fixture.lock(&changed);
    std::fs::write(locked.join("new.txt"), "untracked\n").unwrap();
    let left = fixture.remove(&archived, &shown).await;
    assert!(changed.exists() && locked.exists());
    let worktrees: Vec<_> = left
        .iter()
        .filter(|item| item.kind == SessionBranchKind::Worktree)
        .collect();
    assert!(worktrees
        .iter()
        .all(|item| item.state == SessionBranchState::Locked && item.uncommitted));
    assert_eq!(to_report(left, &shown, &[]).len(), 2);
}

#[tokio::test]
async fn a_worktree_whose_directory_is_gone_goes_with_its_branch() {
    let fixture = Fixture::new("cleanup-prunable");
    let root = fixture.worktree("gone", "s1");
    std::fs::remove_dir_all(&root).unwrap();
    let archived = [session("s1", SessionStatus::Archived)];
    assert!(fixture.remove(&archived, &[]).await.is_empty());
    assert!(!fixture.has_branch("gone"));
}

#[tokio::test]
async fn a_worktree_with_submodules_goes_only_when_forced() {
    let fixture = Fixture::new("cleanup-submodules");
    let inner = repo_with("cleanup-submodules-inner", &[(b"b.txt", b"b\n")]);
    let root = fixture.worktree("subs", "s1");
    git(
        &root,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            inner.to_str().unwrap(),
            "inner",
        ],
    );
    git(&root, &["commit", "-qm", "inner"]);
    let archived = [session("s1", SessionStatus::Archived)];
    let shown = fixture.inspect(&archived);
    let worktree = shown
        .iter()
        .find(|item| item.kind == SessionBranchKind::Worktree)
        .unwrap();
    assert_eq!(worktree.state, SessionBranchState::Submodules);

    assert_eq!(fixture.remove(&archived, &[]).await.len(), 2);
    assert!(root.exists());
    let forced: Vec<SessionBranchTarget> = shown.iter().map(target).collect();
    assert!(fixture.remove(&archived, &forced).await.is_empty());
    assert!(!root.exists() && !fixture.has_branch("subs"));
}

#[test]
fn a_branch_counts_as_merged_into_its_upstream_where_it_has_one() {
    let fixture = Fixture::new("cleanup-upstream");
    let git_ = (fixture.state.host_tools.get().git)().unwrap();
    let repo = repository_of(&fixture.repo).unwrap();
    // `ahead` is ahead of `HEAD` but merged into its upstream, `up`.
    git(&fixture.repo, &["switch", "-q", "-c", "up"]);
    commit_in(&fixture.repo);
    git(&fixture.repo, &["branch", "ahead"]);
    git(&fixture.repo, &["switch", "-q", "main"]);
    git(&fixture.repo, &["branch", "--set-upstream-to=up", "ahead"]);
    // `behind` is merged into `HEAD` but not into its upstream, an unrelated branch.
    git(&fixture.repo, &["branch", "behind"]);
    git(&fixture.repo, &["switch", "-q", "--orphan", "elsewhere"]);
    git(
        &fixture.repo,
        &["commit", "-q", "--allow-empty", "-m", "orphan"],
    );
    git(&fixture.repo, &["switch", "-q", "main"]);
    git(
        &fixture.repo,
        &["branch", "--set-upstream-to=elsewhere", "behind"],
    );

    assert!(is_merged(&git_, &repo, b"ahead"));
    assert!(!is_merged(&git_, &repo, b"behind"));
    assert!(is_merged(&git_, &repo, b"main"));
}

#[tokio::test]
async fn a_git_that_cannot_run_leaves_every_bound_item_failed() {
    let fixture = Fixture::new("cleanup-no-git");
    branch_made_by(&fixture.state, &fixture.repo, "mine", "s1");
    fixture.state.host_tools.replace(crate::host_tools::Tools {
        git: || anyhow::bail!("git is not on the PATH"),
        ..host_tools()
    });
    let archived = [session("s1", SessionStatus::Archived)];
    assert!(inspect(&fixture.state, &archived).is_err());
    let left = fixture.remove(&archived, &[]).await;
    assert_eq!(
        summary(&left),
        expect(&[(false, "mine", SessionBranchState::Failed)])
    );
    assert!(fixture.has_branch("mine"));
}

#[tokio::test]
async fn removal_skips_a_session_no_longer_archived_and_takes_a_deleted_ones() {
    let fixture = Fixture::new("cleanup-resumed");
    let resumed = fixture.store("s1", SessionStatus::Idle);
    branch_made_by(&fixture.state, &fixture.repo, "resumed", "s1");
    branch_made_by(&fixture.state, &fixture.repo, "deleted", "s2");

    let archived = vec![
        Session {
            status: SessionStatus::Archived,
            ..resumed
        },
        session("s2", SessionStatus::Archived),
    ];
    assert!(fixture.remove(&archived, &[]).await.is_empty());
    assert!(fixture.has_branch("resumed"));
    assert!(!fixture.has_branch("deleted"));
}

/// A session resumed while its removal waits has a process again, which is no reason to wait on.
#[tokio::test]
async fn waiting_ends_for_a_session_resumed_meanwhile() {
    let fixture = Fixture::new("cleanup-wait");
    fixture.store("s1", SessionStatus::Idle);
    let _live = running(&fixture.state, "s1");
    let started = Instant::now();
    fixture
        .remove(&[session("s1", SessionStatus::Archived)], &[])
        .await;
    assert!(started.elapsed() < STOP_TIMEOUT / 2);
}

#[test]
fn the_user_is_told_of_what_was_not_kept_on_purpose() {
    let item = |state, uncommitted| SessionBranchItem {
        kind: SessionBranchKind::Worktree,
        repository: "/r/.git".to_string(),
        branch: Some("b".to_string()),
        path: Some("/w".to_string()),
        project: "p1".to_string(),
        session: Some("s1".to_string()),
        state,
        uncommitted,
        detail: None,
    };
    let kept = target(&item(SessionBranchState::UncommittedChanges, true));
    for (left, forced, keep, told) in [
        (
            item(SessionBranchState::Failed, false),
            vec![],
            vec![],
            true,
        ),
        (
            item(SessionBranchState::Locked, false),
            vec![kept.clone()],
            vec![],
            true,
        ),
        (
            item(SessionBranchState::UncommittedChanges, true),
            vec![],
            vec![],
            true,
        ),
        (
            item(SessionBranchState::UncommittedChanges, true),
            vec![],
            vec![kept.clone()],
            false,
        ),
        (
            item(SessionBranchState::Locked, true),
            vec![],
            vec![kept.clone()],
            true,
        ),
        (
            item(SessionBranchState::CheckedOut, false),
            vec![],
            vec![],
            false,
        ),
        (
            item(SessionBranchState::ProjectFolder, false),
            vec![],
            vec![],
            false,
        ),
    ] {
        let state = left.state;
        assert_eq!(
            !to_report(vec![left], &forced, &keep).is_empty(),
            told,
            "{state:?}"
        );
    }
}

#[test]
fn an_agent_is_told_each_item_left_in_plain_text_with_its_reason() {
    let item = SessionBranchItem {
        kind: SessionBranchKind::Worktree,
        repository: "/r/.git".to_string(),
        branch: Some("caf%E9".to_string()),
        path: Some("/r/wt%25".to_string()),
        project: "p1".to_string(),
        session: Some("s1".to_string()),
        state: SessionBranchState::Locked,
        uncommitted: true,
        detail: None,
    };
    assert_eq!(
        for_agent(&[item]),
        json!([{
            "kind": "worktree",
            "branch": "caf\u{FFFD}",
            "path": "/r/wt%",
            "reason": "the worktree is locked, with uncommitted changes",
        }])
    );
}

#[test]
fn a_client_is_told_each_item_left_as_structured_data() {
    let item = SessionBranchItem {
        kind: SessionBranchKind::Branch,
        repository: "/r/.git".to_string(),
        branch: Some("a,b".to_string()),
        path: None,
        project: "p1".to_string(),
        session: None,
        state: SessionBranchState::Failed,
        uncommitted: false,
        detail: Some("error: refused".to_string()),
    };
    let parsed: Value = serde_json::from_str(&for_client(&[item])).unwrap();
    assert_eq!(
        parsed,
        json!([{ "kind": "branch", "name": "a,b", "branch": "a,b", "state": "failed", "uncommitted": false, "detail": "error: refused" }])
    );
}

// -- the requests that remove them -----------------------------------------------------

/// Each of the user's requests that archives or deletes `s1`, with what it forces, and what it
/// leaves for the test to check.
async fn request_for(fixture: &Fixture, kind: &str, forced: Vec<SessionBranchTarget>) {
    let request = match kind {
        "archive" => RequestBody::ArchiveSession {
            session: "s1".to_string(),
            force_remove: forced,
            keep: Vec::new(),
        },
        "delete" => RequestBody::DeleteSession {
            session: "s1".to_string(),
            only_if_unused: false,
            trash_agent_records: false,
            force_remove: forced,
            keep: Vec::new(),
        },
        _ => RequestBody::DeleteArchivedSessions {
            console: "console-1".to_string(),
            project: Some("p1".to_string()),
            console_session: None,
            trash_agent_records: false,
            force_remove: forced,
            keep: Vec::new(),
        },
    };
    crate::coordinator::handle(&fixture.state, None, request)
        .await
        .unwrap();
}

#[tokio::test]
async fn the_users_requests_force_what_they_name_and_report_what_is_left() {
    for kind in ["archive", "delete", "delete-archived"] {
        let fixture = Fixture::new(&format!("cleanup-request-{kind}"));
        let status = if kind == "archive" {
            SessionStatus::Idle
        } else {
            SessionStatus::Archived
        };
        fixture.store("s1", status);
        let forced = fixture.worktree("forced", "s1");
        std::fs::write(forced.join("new.txt"), "untracked\n").unwrap();
        let left = fixture.worktree("left", "s1");
        std::fs::write(left.join("new.txt"), "untracked\n").unwrap();
        let shown = fixture.inspect(&[session("s1", status)]);
        let named: Vec<SessionBranchTarget> = shown
            .iter()
            .filter(|item| {
                item.path
                    .as_deref()
                    .is_some_and(|path| path.ends_with("/forced"))
            })
            .map(target)
            .collect();

        let mut events = fixture.state.subscribe();
        request_for(&fixture, kind, named).await;
        let told = reported(&mut events).await;
        assert!(!forced.exists(), "{kind}");
        assert!(left.exists(), "{kind}");
        let names: Vec<&str> = told
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap())
            .collect();
        assert_eq!(names.len(), 1, "{kind}: {told}");
        assert!(names[0].ends_with("/left"), "{kind}: {told}");
        assert_eq!(told[0]["state"], "uncommitted_changes");
        assert_eq!(told[0]["uncommitted"], true);
    }
}

#[tokio::test]
async fn deleting_an_unused_session_removes_what_can_go() {
    let fixture = Fixture::new("cleanup-unused");
    let unused = Session {
        has_conversation: false,
        ..session("s1", SessionStatus::Idle)
    };
    fixture.state.store.insert_session(&unused).unwrap();
    branch_made_by(&fixture.state, &fixture.repo, "early", "s1");
    crate::coordinator::handle(
        &fixture.state,
        None,
        RequestBody::DeleteSession {
            session: "s1".to_string(),
            only_if_unused: true,
            trash_agent_records: false,
            force_remove: Vec::new(),
            keep: Vec::new(),
        },
    )
    .await
    .unwrap();
    until(|| !fixture.has_branch("early")).await;
}

#[tokio::test]
async fn removing_a_project_takes_its_outside_worktrees_only_with_its_folder() {
    for trash in [false, true] {
        let fixture = Fixture::new(&format!("cleanup-remove-project-{trash}"));
        fixture.store("s1", SessionStatus::Archived);
        let outside = fixture.worktree("outside", "s1");
        crate::coordinator::handle(
            &fixture.state,
            None,
            RequestBody::DeleteProject {
                project: "p1".to_string(),
                stop_sessions: true,
                trash_directory: trash,
                force_remove: Vec::new(),
            },
        )
        .await
        .unwrap();
        assert!(fixture.state.store.get_project("p1").unwrap().is_none());
        if trash {
            until(|| !outside.exists()).await;
            assert!(!fixture.repo.exists());
        } else {
            // Nothing runs in the background to wait on: the request touches no worktree.
            assert!(outside.exists() && fixture.has_branch("outside"));
        }
    }
}

#[tokio::test]
async fn a_project_removal_that_cannot_find_out_what_goes_along_says_so_and_leaves_it() {
    let fixture = Fixture::new("cleanup-remove-project-unknown");
    fixture.store("s1", SessionStatus::Archived);
    let outside = fixture.worktree("outside", "s1");
    fixture.state.host_tools.replace(crate::host_tools::Tools {
        git: || anyhow::bail!("git is not on the PATH"),
        ..host_tools()
    });
    let mut events = fixture.state.subscribe();
    crate::coordinator::handle(
        &fixture.state,
        None,
        RequestBody::DeleteProject {
            project: "p1".to_string(),
            stop_sessions: true,
            trash_directory: true,
            force_remove: Vec::new(),
        },
    )
    .await
    .unwrap();
    assert!(fixture.state.store.get_project("p1").unwrap().is_none());
    let told = loop {
        match events.try_recv() {
            Ok(Event::Error { code, params, .. }) => break (code, params),
            Ok(_) => continue,
            Err(err) => panic!("not told: {err:?}"),
        }
    };
    assert_eq!(told.0, error_code::PROJECT_BRANCHES_NOT_DETERMINED);
    assert!(
        told.1["detail"].contains("git is not on the PATH"),
        "{:?}",
        told.1
    );
    assert!(outside.exists());
}

#[tokio::test]
async fn a_removal_that_would_be_refused_lists_nothing() {
    let fixture = Fixture::new("cleanup-refused");
    let mut inner = fixture.state.store.get_project("p1").unwrap().unwrap();
    inner.id = "p2".to_string();
    inner.path = fixture.repo.join("inner").to_string_lossy().into_owned();
    std::fs::create_dir(fixture.repo.join("inner")).unwrap();
    fixture.state.store.insert_project(&inner).unwrap();
    let err = crate::coordinator::handle(
        &fixture.state,
        None,
        RequestBody::InspectProjectRemoval {
            project: "p1".to_string(),
        },
    )
    .await
    .unwrap_err();
    let coded = err.downcast_ref::<crate::protocol::CodedError>().unwrap();
    assert_eq!(coded.code, error_code::PROJECT_DIRECTORY_SHARED);
}

// -- what an agent is told ---------------------------------------------------------------

/// `owner`, an unbound project session, with `worker` bound to it; `worker` made `branch` in a
/// worktree of its own with uncommitted changes.
fn agent_fixture(label: &str) -> (Fixture, PathBuf) {
    let fixture = Fixture::new(label);
    fixture.store("owner", SessionStatus::Idle);
    let worker = Session {
        bound_to: Some("owner".to_string()),
        ..session("worker", SessionStatus::Idle)
    };
    fixture.state.store.insert_session(&worker).unwrap();
    let root = fixture.worktree("work", "worker");
    std::fs::write(root.join("new.txt"), "untracked\n").unwrap();
    (fixture, root)
}

fn arguments(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[tokio::test]
async fn an_agents_archive_never_forces_and_names_what_it_left() {
    let (fixture, root) = agent_fixture("cleanup-agent-archive");
    let answer = crate::mcp::exec::call(
        &fixture.state,
        "owner",
        "archive_session",
        &arguments(json!({ "session": "worker" })),
    )
    .await
    .unwrap();
    assert!(root.exists() && fixture.has_branch("work"));
    let left = answer["branches_left"].as_array().unwrap();
    assert_eq!(left.len(), 2, "{answer}");
    assert_eq!(left[0]["reason"], "it has uncommitted changes");
    assert!(answer["note"]
        .as_str()
        .unwrap()
        .contains("git worktree remove"));
}

#[tokio::test]
async fn archiving_oneself_names_what_will_stay() {
    let (fixture, root) = agent_fixture("cleanup-agent-self");
    let owner_root = fixture.worktree("own", "owner");
    std::fs::write(owner_root.join("new.txt"), "untracked\n").unwrap();
    crate::coordinator::archive_session(&fixture.state, "worker").unwrap();
    let answer = crate::mcp::exec::call(&fixture.state, "owner", "archive_self", &Map::new())
        .await
        .unwrap();
    let left = answer["branches_left"].as_array().unwrap();
    assert!(left.iter().any(|item| item["branch"] == "own"), "{answer}");
    assert!(owner_root.exists() && root.exists());
}

#[tokio::test]
async fn a_done_report_names_what_will_stay() {
    let (fixture, root) = agent_fixture("cleanup-agent-report");
    let _owner = running(&fixture.state, "owner");
    let answer = crate::mcp::exec::call(
        &fixture.state,
        "worker",
        "report",
        &arguments(json!({ "summary": "Done.", "status": "done" })),
    )
    .await
    .unwrap();
    let note = answer["note"].as_str().unwrap();
    assert!(
        note.contains("will not be removed") && note.contains("uncommitted changes"),
        "{note}"
    );
    assert!(root.exists());
}

// -- a project's removal ---------------------------------------------------------------

fn project_p1(state: &AppState) -> crate::protocol::Project {
    state.store.get_project("p1").unwrap().unwrap()
}

fn listed_worktrees(repo: &Path) -> String {
    String::from_utf8(git(repo, &["worktree", "list", "--porcelain"])).unwrap()
}

fn carry_out_now(
    fixture: &Fixture,
    removal: project::Removal,
    forced: &[SessionBranchTarget],
) -> Vec<SessionBranchItem> {
    let tools = fixture.state.host_tools.get();
    project::carry_out(&(tools.git)().unwrap(), removal, forced, tools.trash)
}

#[test]
fn a_folder_holding_the_repository_takes_every_outside_worktree_but_another_projects() {
    let fixture = Fixture::new("cleanup-project-whole");
    fixture.store("s1", SessionStatus::Archived);
    let bound = fixture.worktree("bound", "s1");
    std::fs::write(bound.join("new.txt"), "untracked\n").unwrap();
    // Bound to no session, with a detached `HEAD`.
    let detached = fixture.worktrees.join("detached");
    git(
        &fixture.repo,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            detached.to_str().unwrap(),
        ],
    );
    let locked = fixture.worktrees.join("locked");
    git(
        &fixture.repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "locked",
            locked.to_str().unwrap(),
        ],
    );
    fixture.lock(&locked);
    std::fs::write(locked.join("new.txt"), "untracked\n").unwrap();
    // Another project's folder.
    let other = fixture.worktrees.join("other");
    git(
        &fixture.repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "other",
            other.to_str().unwrap(),
        ],
    );
    let mut theirs = project_p1(&fixture.state);
    theirs.id = "p2".to_string();
    theirs.path = other.to_string_lossy().into_owned();
    fixture.state.store.insert_project(&theirs).unwrap();
    branch_made_by(&fixture.state, &fixture.repo, "plain", "s1");

    let plan = |reads| {
        project::plan(&fixture.state, &project_p1(&fixture.state), reads)
            .unwrap()
            .unwrap()
    };
    let shown = plan(project::Reads::Inspection).items();
    assert_eq!(
        summary(&shown),
        expect(&[
            (true, "", SessionBranchState::Removable),
            (true, "bound", SessionBranchState::UncommittedChanges),
            (true, "locked", SessionBranchState::Locked),
        ])
    );
    assert!(shown
        .iter()
        .any(|item| item.state == SessionBranchState::Locked && item.uncommitted));
    let unbound = shown.iter().find(|item| item.branch.is_none()).unwrap();
    assert_eq!(
        (unbound.session.as_deref(), unbound.project.as_str()),
        (None, "p1")
    );

    // The removal's own plan reads no status: it does not know of the changes.
    assert!(carry_out_now(&fixture, plan(project::Reads::Removal), &[]).is_empty());
    assert!(!bound.exists() && !detached.exists() && locked.exists() && other.exists());

    // Forced as shown, with its changes, the locked one goes too.
    let forced: Vec<SessionBranchTarget> = shown.iter().map(target).collect();
    assert!(carry_out_now(&fixture, plan(project::Reads::Removal), &forced).is_empty());
    assert!(!locked.exists());
    assert!(fixture.has_branch("plain"));
}

#[test]
fn a_locked_worktree_that_gained_changes_since_it_was_shown_stays() {
    let fixture = Fixture::new("cleanup-project-consent");
    fixture.store("s1", SessionStatus::Archived);
    let locked = fixture.worktree("locked", "s1");
    fixture.lock(&locked);
    let plan = |reads| {
        project::plan(&fixture.state, &project_p1(&fixture.state), reads)
            .unwrap()
            .unwrap()
    };
    let forced: Vec<SessionBranchTarget> = plan(project::Reads::Inspection)
        .items()
        .iter()
        .map(target)
        .collect();
    std::fs::write(locked.join("new.txt"), "untracked\n").unwrap();
    let left = carry_out_now(&fixture, plan(project::Reads::Removal), &forced);
    assert_eq!(
        summary(&left),
        expect(&[(true, "locked", SessionBranchState::Locked)])
    );
    assert!(left[0].uncommitted && locked.exists());
}

#[test]
fn a_surviving_repository_loses_the_bound_branches_and_keeps_what_it_must() {
    let fixture = Fixture::without_project("cleanup-project-part");
    // The project is a linked worktree of the repository, so the repository outlives its folder.
    let folder = fixture.worktree("folder", "s1");
    insert_project(&fixture.state, &folder);
    fixture.store("s1", SessionStatus::Archived);
    let outside = fixture.worktree("outside", "s1");
    commit_in(&outside);
    let locked = fixture.worktree("locked", "s1");
    fixture.lock(&locked);
    let kept = fixture.worktree("kept", "s1");
    fixture.lock(&kept);
    // Bound to no session: the repository survives, so it is not the project's to take.
    let unbound = fixture.worktrees.join("unbound");
    git(
        &fixture.repo,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            unbound.to_str().unwrap(),
        ],
    );
    branch_made_by(&fixture.state, &fixture.repo, "in-main", "s1");
    git(&fixture.repo, &["switch", "-q", "in-main"]);

    let shown = project::plan(
        &fixture.state,
        &project_p1(&fixture.state),
        project::Reads::Inspection,
    )
    .unwrap()
    .unwrap()
    .items();
    assert_eq!(
        summary(&shown),
        expect(&[
            (true, "kept", SessionBranchState::Locked),
            (true, "locked", SessionBranchState::Locked),
            (true, "outside", SessionBranchState::Removable),
            (false, "folder", SessionBranchState::Removable),
            (false, "kept", SessionBranchState::Removable),
            (false, "locked", SessionBranchState::Removable),
            (false, "outside", SessionBranchState::Unmerged),
        ])
    );
    let forced: Vec<SessionBranchTarget> = shown
        .iter()
        .filter(|item| {
            item.path
                .as_deref()
                .is_some_and(|path| path.ends_with("/locked"))
        })
        .map(target)
        .collect();
    assert_eq!(forced.len(), 1);

    // The folder goes to the Trash after the plan and before the rest, as the removal runs.
    let removal = project::plan(
        &fixture.state,
        &project_p1(&fixture.state),
        project::Reads::Removal,
    )
    .unwrap()
    .unwrap();
    std::fs::remove_dir_all(&folder).unwrap();
    let left = carry_out_now(&fixture, removal, &forced);
    assert!(left.is_empty(), "{left:?}");
    assert!(!outside.exists() && !locked.exists());
    assert!(kept.exists() && unbound.exists());
    for gone in ["folder", "outside", "locked"] {
        assert!(!fixture.has_branch(gone), "{gone}");
    }
    assert!(fixture.has_branch("kept") && fixture.has_branch("in-main"));
    let listed = listed_worktrees(&fixture.repo);
    assert!(
        !listed.contains("/folder\n")
            && !listed.contains("/outside\n")
            && !listed.contains("/locked\n")
    );
    assert!(listed.contains("/kept\n"));
}

#[test]
fn a_locked_project_folder_keeps_its_branch_and_says_so() {
    let fixture = Fixture::without_project("cleanup-project-locked-folder");
    let folder = fixture.worktree("folder", "s1");
    insert_project(&fixture.state, &folder);
    fixture.lock(&folder);
    fixture.store("s1", SessionStatus::Archived);
    let removal = project::plan(
        &fixture.state,
        &project_p1(&fixture.state),
        project::Reads::Removal,
    )
    .unwrap()
    .unwrap();
    std::fs::remove_dir_all(&folder).unwrap();
    let left = carry_out_now(&fixture, removal, &[]);
    assert_eq!(
        summary(&left),
        expect(&[(false, "folder", SessionBranchState::CheckedOut)])
    );
    assert!(fixture.has_branch("folder"));
}

#[test]
fn a_locked_worktree_whose_move_to_the_trash_fails_keeps_its_lock() {
    let fixture = Fixture::without_project("cleanup-project-trash-fails");
    let folder = fixture.worktree("folder", "s1");
    insert_project(&fixture.state, &folder);
    fixture.store("s1", SessionStatus::Archived);
    let locked = fixture.worktree("pinned", "s1");
    fixture.lock(&locked);
    let plan = |reads| {
        project::plan(&fixture.state, &project_p1(&fixture.state), reads)
            .unwrap()
            .unwrap()
    };
    let forced: Vec<SessionBranchTarget> = plan(project::Reads::Inspection)
        .items()
        .iter()
        .filter(|item| item.state == SessionBranchState::Locked)
        .map(target)
        .collect();
    let removal = plan(project::Reads::Removal);
    std::fs::remove_dir_all(&folder).unwrap();
    let tools = fixture.state.host_tools.get();
    let left = project::carry_out(&(tools.git)().unwrap(), removal, &forced, |_| {
        anyhow::bail!("the Trash is full")
    });
    assert!(left
        .iter()
        .any(|item| item.state == SessionBranchState::Failed
            && item.kind == SessionBranchKind::Worktree));
    assert!(locked.exists());
    // git's own record of the lock, in the worktree's administrative directory.
    let admin = crate::browse::source::admin_dir(&locked).unwrap();
    assert!(admin.join("locked").is_file());
}
