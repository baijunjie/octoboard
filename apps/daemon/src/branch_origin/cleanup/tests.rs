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

// -- what goes and what stays ------------------------------------------------------

#[tokio::test]
async fn a_clean_worktree_and_its_merged_branch_go_and_another_sessions_stay() {
    let fixture = Fixture::new("cleanup-safe");
    let root = fixture.worktree("mine", "s1");
    branch_made_by(&fixture.state, &fixture.repo, "theirs", "s2");
    let archived = [session("s1", SessionStatus::Archived)];

    assert_eq!(
        summary(&fixture.inspect(&archived).await),
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

    let shown = fixture.inspect(&archived).await;
    assert_eq!(
        summary(&shown),
        expect(&[
            (true, "work", SessionBranchState::UncommittedChanges),
            (false, "work", SessionBranchState::Unmerged),
        ])
    );
    // Kept for its changes, the worktree keeps the branch it has checked out, merged or not.
    let left = fixture.remove(&archived, &[]).await;
    assert_eq!(
        summary(&left),
        expect(&[
            (true, "work", SessionBranchState::UncommittedChanges),
            (false, "work", SessionBranchState::CheckedOut),
        ])
    );
    let told = to_report(left, &[], &[]);
    assert!(told.len() == 1 && told[0].kind == SessionBranchKind::Worktree);
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
async fn an_unmerged_branch_checked_out_in_a_worktree_that_stays_is_left_as_checked_out() {
    let fixture = Fixture::new("cleanup-held-unmerged");
    let folder = fixture.worktree("folder", "s1");
    commit_in(&folder);
    other_project_at(&fixture.state, &folder);
    let archived = [session("s1", SessionStatus::Archived)];

    let shown = fixture.inspect(&archived).await;
    assert_eq!(
        summary(&shown),
        expect(&[
            (true, "folder", SessionBranchState::ProjectFolder),
            (false, "folder", SessionBranchState::CheckedOut),
        ])
    );
    let left = fixture.remove(&archived, &[]).await;
    assert_eq!(summary(&left), summary(&shown));
    assert!(to_report(left, &[], &[]).is_empty());
    assert!(fixture.has_branch("folder"));
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
        .await
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
    let shown = fixture.inspect(&archived).await;
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
    let (state, sessions) = (fixture.state.clone(), archived.clone());
    let inspected =
        tokio::task::spawn_blocking(move || inspect(&state, &sessions).map(|_| ())).await;
    assert!(inspected.unwrap().is_err());
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
        cause: None,
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
        (
            item(SessionBranchState::ProjectFolder, false),
            vec![kept.clone()],
            vec![],
            true,
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
        cause: None,
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
        cause: None,
    };
    let parsed: Value = serde_json::from_str(&for_client(&[item])).unwrap();
    assert_eq!(
        parsed,
        json!([{ "kind": "branch", "name": "a,b", "branch": "a,b", "state": "failed", "uncommitted": false, "detail": "error: refused", "cause": null }])
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
        let shown = fixture.inspect(&[session("s1", status)]).await;
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
                inspection: None,
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
            inspection: None,
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

/// The removal confirmed after an inspection refused over another project's folder that did not
/// respond, given that inspection's token, is refused the same way once the folder responds; one
/// confirmed after an inspection that resolved, which carries no token, goes ahead. Removing the
/// project, or its console, drops what was kept for it.
#[tokio::test]
async fn a_removal_after_an_unresolved_inspection_repeats_its_refusal() {
    let fixture = Fixture::new("cleanup-refused-kept");
    let state = &fixture.state;
    let elsewhere = fixture.worktrees.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let mut other = state.store.get_project("p1").unwrap().unwrap();
    other.id = "p2".to_string();
    other.path = elsewhere.to_string_lossy().into_owned();
    state.store.insert_project(&other).unwrap();
    let elsewhere = &elsewhere;
    let inspect = |stalled: bool| async move {
        let stuck = Arc::new(());
        if stalled {
            state.directory_looks.record(
                crate::hostfs::lexically_normalise(elsewhere),
                Arc::downgrade(&stuck),
            );
        }
        crate::coordinator::handle(
            state,
            None,
            RequestBody::InspectProjectRemoval {
                project: "p1".to_string(),
            },
        )
        .await
    };
    let delete = |inspection: Option<String>| {
        crate::coordinator::handle(
            state,
            None,
            RequestBody::DeleteProject {
                project: "p1".to_string(),
                stop_sessions: true,
                trash_directory: true,
                force_remove: Vec::new(),
                inspection,
            },
        )
    };
    let coded = |err: anyhow::Error| err.downcast::<crate::protocol::CodedError>().unwrap();

    let inspection = coded(inspect(true).await.unwrap_err());
    assert_eq!(inspection.code, error_code::PROJECT_DIRECTORY_SHARED);
    let token = inspection.params["inspection"].clone();
    let repeated = coded(delete(Some(token)).await.unwrap_err());
    assert_eq!(
        (repeated.code, &repeated.message),
        (inspection.code, &inspection.message)
    );
    assert!(state.store.get_project("p1").unwrap().is_some());
    assert_eq!(state.unresolved_removals.len(), 0, "the token is used up");

    coded(inspect(true).await.unwrap_err());
    assert!(matches!(
        inspect(false).await.unwrap(),
        Some(Event::ProjectRemovalInspected { .. })
    ));
    delete(None).await.unwrap();
    assert!(state.store.get_project("p1").unwrap().is_none());
    assert_eq!(
        state.unresolved_removals.len(),
        0,
        "dropped with the project"
    );

    let again = fixture.worktrees.join("again");
    std::fs::create_dir_all(&again).unwrap();
    insert_project(state, &again);
    coded(inspect(true).await.unwrap_err());
    assert_eq!(state.unresolved_removals.len(), 1);
    crate::coordinator::handle(
        state,
        None,
        RequestBody::DeleteConsole {
            console: "console-1".to_string(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        state.unresolved_removals.len(),
        0,
        "dropped with the console"
    );
}

// -- another project's folder, told by what it is ------------------------------------------

/// Stores the project `p2` at `path`, in place of any `p2` stored before.
fn other_project_at(state: &AppState, path: &Path) {
    state.store.delete_project("p2").ok();
    let mut other = project_p1(state);
    other.id = "p2".to_string();
    other.path = path.to_string_lossy().into_owned();
    state.store.insert_project(&other).unwrap();
}

/// Records a look at `path` as still stuck for as long as `stuck` lives, so the path counts as one
/// whose look outran its deadline.
fn stall(state: &AppState, path: &Path, stuck: &Arc<()>) {
    state.directory_looks.record(
        crate::hostfs::lexically_normalise(path),
        Arc::downgrade(stuck),
    );
}

/// [`project::plan`] of `p1`'s removal, on a blocking thread as the daemon runs it.
async fn plan_now(
    state: &Arc<AppState>,
    reads: project::Reads,
    held_back: Vec<PathBuf>,
) -> Option<project::Removal> {
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        project::plan(&state, &project_p1(&state), reads, &held_back).unwrap()
    })
    .await
    .unwrap()
}

/// The state the inspection gives the worktree at `root`, with its detail.
async fn worktree_stands(
    fixture: &Fixture,
    sessions: &[Session],
    root: &Path,
) -> (SessionBranchState, Option<String>) {
    let path = wire_path::encode(root.as_os_str().as_bytes());
    let item = fixture
        .inspect(sessions)
        .await
        .into_iter()
        .find(|item| item.path.as_deref() == Some(path.as_str()))
        .expect("the worktree is listed");
    (item.state, item.detail)
}

/// How a project's removal with its folder lists the worktree at `root`: the item, `None` where it
/// is not listed.
async fn removal_lists(fixture: &Fixture, root: &Path) -> Option<SessionBranchItem> {
    let path = wire_path::encode(root.as_os_str().as_bytes());
    plan_now(&fixture.state, project::Reads::Inspection, Vec::new())
        .await?
        .items()
        .into_iter()
        .find(|item| item.path.as_deref() == Some(path.as_str()))
}

/// Whether `item` is one kept because whether it is or holds another project's folder could not be
/// told.
fn is_untold(item: &SessionBranchItem) -> bool {
    item.state == SessionBranchState::Failed
        && item.cause == Some(SessionBranchCause::ProjectFolderUnknown)
        && item.detail.as_deref() == Some(HOLDING_UNTOLD)
}

/// A worktree holding another project's folder is that project's however the folder's path is
/// spelled: through the second real path a macOS firmlink gives it (as a bind mount does on
/// Linux), through a link, or in another letter case on a file system that ignores case. It is
/// never removed, nor taken along with a project's folder. On macOS the scratch directory always
/// has a second real path, so its absence fails there.
#[tokio::test]
async fn a_worktree_holding_another_projects_folder_by_another_path_is_never_removed() {
    let fixture = Fixture::new("cleanup-holds-other");
    let archived = [fixture.store("s1", SessionStatus::Archived)];
    let root = std::fs::canonicalize(fixture.worktree("mine", "s1")).unwrap();
    std::fs::create_dir(root.join("inner")).unwrap();

    let mut spellings = Vec::new();
    match crate::test_support::second_real_path(&root) {
        Some(second) => spellings.push(("firmlink", second.join("inner"))),
        None if cfg!(target_os = "macos") => panic!("{} has no second real path", root.display()),
        None => {}
    }
    let link = fixture.worktrees.join("link");
    std::os::unix::fs::symlink(&root, &link).unwrap();
    spellings.push(("link", link.join("inner")));
    let name = root.file_name().unwrap().to_string_lossy().to_uppercase();
    let respelled = root.with_file_name(name);
    let identity = |path: &Path| {
        std::fs::metadata(path)
            .ok()
            .map(|m| crate::hostfs::identity(&m))
    };
    if identity(&respelled) == identity(&root) {
        spellings.push(("case", respelled.join("INNER")));
    } else {
        eprintln!("no case-only respelling: the file system tells case apart");
    }

    for (how, path) in &spellings {
        other_project_at(&fixture.state, path);
        assert_eq!(
            worktree_stands(&fixture, &archived, &root).await,
            (SessionBranchState::ProjectFolder, None),
            "{how}"
        );
        assert!(removal_lists(&fixture, &root).await.is_none(), "{how}");
    }
    let left = fixture.remove(&archived, &[]).await;
    assert!(root.exists());
    assert!(left
        .iter()
        .any(|item| item.state == SessionBranchState::ProjectFolder));
}

/// Where whether a worktree holds another project's folder cannot be told — that project's path
/// did not answer in time, or the worktree's own root did not — the worktree is kept as if it did:
/// left with a reason of its own, which the removal reports, and not listed as going along with a
/// project's folder but held back. Once both answer, it goes as before.
#[tokio::test]
async fn a_worktree_whose_holding_cannot_be_told_is_kept_and_reported() {
    let fixture = Fixture::new("cleanup-holds-untold");
    let archived = [fixture.store("s1", SessionStatus::Archived)];
    let root = std::fs::canonicalize(fixture.worktree("mine", "s1")).unwrap();
    let elsewhere = fixture.worktrees.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    other_project_at(&fixture.state, &elsewhere);
    let untold = (SessionBranchState::Failed, Some(HOLDING_UNTOLD.to_string()));

    for stalled in [&elsewhere, &root] {
        let stuck = Arc::new(());
        stall(&fixture.state, stalled, &stuck);
        assert_eq!(
            worktree_stands(&fixture, &archived, &root).await,
            untold,
            "{}",
            stalled.display()
        );
        let removal = plan_now(&fixture.state, project::Reads::Inspection, Vec::new())
            .await
            .unwrap();
        let listed: Vec<_> = removal
            .items()
            .into_iter()
            .filter(|item| item.path.is_some())
            .collect();
        assert!(listed.len() == 1 && is_untold(&listed[0]), "{listed:?}");
        assert_eq!(removal.held_back(), vec![root.clone()]);
    }

    let stuck = Arc::new(());
    stall(&fixture.state, &root, &stuck);
    let left = fixture.remove(&archived, &[]).await;
    assert!(root.exists() && fixture.has_branch("mine"));
    assert_eq!(
        summary(&left),
        expect(&[
            (true, "mine", SessionBranchState::Failed),
            (false, "mine", SessionBranchState::CheckedOut),
        ])
    );
    assert!(is_untold(&left[0]));
    let told: Value = serde_json::from_str(&for_client(&left)).unwrap();
    assert_eq!(told[0]["cause"], "project_folder_unknown");
    assert_eq!(told[1]["cause"], Value::Null);
    drop(stuck);

    assert_eq!(
        worktree_stands(&fixture, &archived, &root).await,
        (SessionBranchState::Removable, None)
    );
    assert_eq!(
        removal_lists(&fixture, &root).await.map(|item| item.state),
        Some(SessionBranchState::Removable)
    );
    // Held back by the inspection before it, it stays held back.
    let removal = plan_now(&fixture.state, project::Reads::Removal, vec![root.clone()])
        .await
        .unwrap();
    let left = carry_out_now(&fixture, removal, &[]);
    assert_eq!(
        summary(&left),
        expect(&[(true, "mine", SessionBranchState::Failed)])
    );
    assert!(root.exists());
}

/// Another project's path with nothing at it, or that cannot be read for want of permission, is
/// compared by its text, as written and as resolved: inside the worktree it keeps the worktree,
/// elsewhere it does not. A sibling whose name merely starts the same is not inside. Skipped as
/// root, who reads a folder whatever its permissions, for the unreadable ones.
#[tokio::test]
async fn another_project_with_nothing_there_or_unreadable_is_compared_by_its_text() {
    let fixture = Fixture::new("cleanup-holds-text");
    let archived = [fixture.store("s1", SessionStatus::Archived)];
    let root = std::fs::canonicalize(fixture.worktree("mine", "s1")).unwrap();
    let stands = |path: PathBuf| {
        let (fixture, archived, root) = (&fixture, &archived, &root);
        async move {
            other_project_at(&fixture.state, &path);
            worktree_stands(fixture, archived, root).await.0
        }
    };
    let held = SessionBranchState::ProjectFolder;
    let clear = SessionBranchState::Removable;
    assert_eq!(stands(root.join("missing")).await, held);
    assert_eq!(
        stands(fixture.worktrees.join("elsewhere/missing")).await,
        clear
    );

    if unsafe { libc::geteuid() } != 0 {
        use std::os::unix::fs::PermissionsExt;
        let elsewhere = std::fs::canonicalize(&*fixture.worktrees).unwrap();
        let locked = [root.join("locked"), elsewhere.join("locked")];
        for path in &locked {
            std::fs::create_dir_all(path.join("project")).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).unwrap();
        }
        let inside = stands(locked[0].join("project")).await;
        let outside = stands(locked[1].join("project")).await;
        for path in &locked {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
            std::fs::remove_dir_all(path).unwrap();
        }
        assert_eq!((inside, outside), (held, clear));
    } else {
        eprintln!("skipped the unreadable paths: root reads a folder whatever its permissions");
    }

    let sibling = fixture.worktrees.join("mine-2");
    std::fs::create_dir(&sibling).unwrap();
    assert_eq!(stands(sibling).await, clear);
    let left = fixture.remove(&archived, &[]).await;
    assert!(left.is_empty(), "{left:?}");
    assert!(!root.exists());
}

/// The removal confirmed after an inspection that held a worktree back, given that inspection's
/// token, leaves it and reports it, even once it can be told; without the token, one whose root
/// does not answer during the removal is left and reported all the same.
#[tokio::test]
async fn a_removal_after_an_inspection_that_held_a_worktree_back_leaves_it() {
    for with_token in [true, false] {
        let fixture = Fixture::new(&format!("cleanup-held-back-{with_token}"));
        let state = &fixture.state;
        fixture.store("s1", SessionStatus::Archived);
        let root = std::fs::canonicalize(fixture.worktree("mine", "s1")).unwrap();
        let elsewhere = fixture.worktrees.join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        other_project_at(state, &elsewhere);

        let mut stuck = Some(Arc::new(()));
        stall(state, &root, stuck.as_ref().unwrap());
        let inspected = crate::coordinator::handle(
            state,
            None,
            RequestBody::InspectProjectRemoval {
                project: "p1".to_string(),
            },
        )
        .await
        .unwrap();
        let Some(Event::ProjectRemovalInspected {
            items, inspection, ..
        }) = inspected
        else {
            panic!("not inspected: {inspected:?}");
        };
        let listed: Vec<_> = items.iter().filter(|item| item.path.is_some()).collect();
        assert!(listed.len() == 1 && is_untold(listed[0]), "{items:?}");
        assert!(inspection.is_some());
        // Without the token the root stays stuck through the removal.
        if with_token {
            stuck.take();
        }

        let mut events = state.subscribe();
        crate::coordinator::handle(
            state,
            None,
            RequestBody::DeleteProject {
                project: "p1".to_string(),
                stop_sessions: true,
                trash_directory: true,
                force_remove: Vec::new(),
                inspection: inspection.filter(|_| with_token),
            },
        )
        .await
        .unwrap();
        let told = reported(&mut events).await;
        assert_eq!(told[0]["state"], "failed", "{told}");
        assert_eq!(told[0]["cause"], "project_folder_unknown", "{told}");
        assert_eq!(told[0]["detail"], HOLDING_UNTOLD, "{told}");
        assert!(root.exists() && !fixture.repo.exists());
        assert_eq!(state.unresolved_removals.len(), 0);
    }
}

/// A removal that fails before it stands, such as one whose folder cannot be moved to the Trash,
/// leaves the token of the inspection that held a worktree back as it was, so the retry confirmed
/// with it holds that worktree back too.
#[tokio::test]
async fn a_retried_removal_still_holds_back_what_its_inspection_held_back() {
    let fixture = Fixture::new("cleanup-held-back-retry");
    let state = &fixture.state;
    fixture.store("s1", SessionStatus::Archived);
    let root = std::fs::canonicalize(fixture.worktree("mine", "s1")).unwrap();
    let elsewhere = fixture.worktrees.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    other_project_at(state, &elsewhere);
    let stuck = Arc::new(());
    stall(state, &root, &stuck);
    let Some(Event::ProjectRemovalInspected { inspection, .. }) = crate::coordinator::handle(
        state,
        None,
        RequestBody::InspectProjectRemoval {
            project: "p1".to_string(),
        },
    )
    .await
    .unwrap() else {
        panic!("not inspected");
    };
    drop(stuck);
    let delete = || {
        crate::coordinator::handle(
            state,
            None,
            RequestBody::DeleteProject {
                project: "p1".to_string(),
                stop_sessions: true,
                trash_directory: true,
                force_remove: Vec::new(),
                inspection: inspection.clone(),
            },
        )
    };

    state.host_tools.replace(crate::host_tools::Tools {
        trash: |_| anyhow::bail!("the Trash is full"),
        ..host_tools()
    });
    assert!(delete().await.is_err());
    assert!(state.store.get_project("p1").unwrap().is_some() && fixture.repo.exists());
    assert_eq!(
        state.unresolved_removals.len(),
        1,
        "the token is still kept"
    );

    state.host_tools.replace(host_tools());
    let mut events = state.subscribe();
    delete().await.unwrap();
    let told = reported(&mut events).await;
    assert_eq!(told[0]["cause"], "project_folder_unknown", "{told}");
    assert!(root.exists() && !fixture.repo.exists());
    assert_eq!(
        state.unresolved_removals.len(),
        0,
        "dropped with the project"
    );
}

/// A removal that cannot run git at all leaves everything it would carry out as failed for that
/// reason, while what it held back keeps its own reason, and so its group and note.
#[tokio::test]
async fn a_removal_that_cannot_run_git_keeps_why_it_held_a_worktree_back() {
    let fixture = Fixture::new("cleanup-held-back-no-git");
    let state = &fixture.state;
    fixture.store("s1", SessionStatus::Archived);
    let held = std::fs::canonicalize(fixture.worktree("mine", "s1")).unwrap();
    fixture.worktree("going", "s1");
    let elsewhere = fixture.worktrees.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    other_project_at(state, &elsewhere);
    let stuck = Arc::new(());
    stall(state, &held, &stuck);
    let removal = plan_now(state, project::Reads::Removal, Vec::new())
        .await
        .unwrap();
    drop(stuck);
    state.host_tools.replace(crate::host_tools::Tools {
        git: || anyhow::bail!("git is not on the PATH"),
        ..host_tools()
    });
    let left =
        project::carry_out_once_stopped(state.clone(), Vec::new(), removal, Vec::new()).await;
    let path = wire_path::encode(held.as_os_str().as_bytes());
    let (kept, others): (Vec<_>, Vec<_>) = left
        .iter()
        .partition(|item| item.path.as_deref() == Some(path.as_str()));
    assert!(kept.len() == 1 && is_untold(kept[0]), "{left:?}");
    assert!(!others.is_empty());
    assert!(others
        .iter()
        .all(|item| item.state == SessionBranchState::Failed
            && item.cause.is_none()
            && item.detail.as_deref() == Some("git is not on the PATH")));
}

/// A project's removal looks again when it carries out: an outside worktree that came to hold
/// another project's folder after the plan stays, unreported, and so does the branch it has
/// checked out.
#[tokio::test]
async fn a_removal_leaves_a_worktree_that_came_to_hold_a_project_folder_after_its_plan() {
    let fixture = Fixture::without_project("cleanup-project-look-again");
    let folder = fixture.worktree("folder", "s1");
    insert_project(&fixture.state, &folder);
    fixture.store("s1", SessionStatus::Archived);
    let outside = std::fs::canonicalize(fixture.worktree("outside", "s1")).unwrap();
    let removal = plan_now(&fixture.state, project::Reads::Removal, Vec::new())
        .await
        .unwrap();
    assert!(removal
        .items()
        .iter()
        .any(|item| item.kind == SessionBranchKind::Worktree
            && item.branch.as_deref() == Some("outside")));

    let inner = outside.join("inner");
    std::fs::create_dir(&inner).unwrap();
    other_project_at(&fixture.state, &inner);
    let left =
        project::carry_out_once_stopped(fixture.state.clone(), Vec::new(), removal, Vec::new())
            .await;
    assert!(inner.exists());
    assert!(fixture.has_branch("outside"));
    assert!(
        !left
            .iter()
            .any(|item| item.branch.as_deref() == Some("outside")),
        "{left:?}"
    );
}

/// A worktree of which it cannot be told, when the removal looks again, whether it holds another
/// project's folder is held back then: left and reported as such, with the branch it has checked
/// out staying, unreported.
#[tokio::test]
async fn a_removal_holds_back_a_worktree_it_can_no_longer_tell_about() {
    let fixture = Fixture::without_project("cleanup-project-look-again-untold");
    let folder = fixture.worktree("folder", "s1");
    insert_project(&fixture.state, &folder);
    fixture.store("s1", SessionStatus::Archived);
    let outside = std::fs::canonicalize(fixture.worktree("outside", "s1")).unwrap();
    let elsewhere = fixture.worktrees.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    other_project_at(&fixture.state, &elsewhere);
    let removal = plan_now(&fixture.state, project::Reads::Removal, Vec::new())
        .await
        .unwrap();

    let stuck = Arc::new(());
    stall(&fixture.state, &outside, &stuck);
    let left =
        project::carry_out_once_stopped(fixture.state.clone(), Vec::new(), removal, Vec::new())
            .await;
    drop(stuck);
    assert!(outside.exists());
    assert!(fixture.has_branch("outside"));
    let about: Vec<_> = left
        .iter()
        .filter(|item| item.branch.as_deref() == Some("outside"))
        .collect();
    assert!(
        about.len() == 1 && about[0].kind == SessionBranchKind::Worktree && is_untold(about[0]),
        "{left:?}"
    );
}

/// A worktree holding another project's repository — its main worktree, with the common directory
/// a linked worktree elsewhere that is that project's folder works from — would take that
/// project's history along, so it counts as holding that project's folder.
#[tokio::test]
async fn a_worktree_holding_another_projects_repository_is_never_removed() {
    let fixture = Fixture::new("cleanup-holds-repository");
    let archived = [fixture.store("s1", SessionStatus::Archived)];
    let root = std::fs::canonicalize(fixture.worktree("mine", "s1")).unwrap();
    let nested = root.join("nested");
    std::fs::create_dir(&nested).unwrap();
    git(&nested, &["init", "-q", "-b", "main"]);
    git(&nested, &["commit", "-q", "--allow-empty", "-m", "init"]);
    let linked = fixture.worktrees.join("linked");
    git(
        &nested,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "theirs",
            linked.to_str().unwrap(),
        ],
    );
    other_project_at(&fixture.state, &linked);
    assert_eq!(
        worktree_stands(&fixture, &archived, &root).await,
        (SessionBranchState::ProjectFolder, None)
    );
    assert!(removal_lists(&fixture, &root).await.is_none());
    fixture.remove(&archived, &[]).await;
    assert!(root.exists() && linked.exists());
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
        blocking(|| project::plan(&fixture.state, &project_p1(&fixture.state), reads, &[]))
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
        blocking(|| project::plan(&fixture.state, &project_p1(&fixture.state), reads, &[]))
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

    let shown = blocking(|| {
        project::plan(
            &fixture.state,
            &project_p1(&fixture.state),
            project::Reads::Inspection,
            &[],
        )
    })
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
    let removal = blocking(|| {
        project::plan(
            &fixture.state,
            &project_p1(&fixture.state),
            project::Reads::Removal,
            &[],
        )
    })
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
    let removal = blocking(|| {
        project::plan(
            &fixture.state,
            &project_p1(&fixture.state),
            project::Reads::Removal,
            &[],
        )
    })
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
        blocking(|| project::plan(&fixture.state, &project_p1(&fixture.state), reads, &[]))
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
