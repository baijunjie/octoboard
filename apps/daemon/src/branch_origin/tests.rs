use super::watch::FLOOR;
use super::*;
use crate::protocol::{
    Agent, Console, Origin, Project, ProjectAccount, ProjectSource, Role, SessionStatus,
};
use crate::store::LOCAL_HOST_ID;
use crate::test_support::{
    app_state, git, git_stdin_env, isolated_git_env, move_directory_time_back, repo_with,
    ScratchDir, PATIENCE, SYSTEM_GIT,
};
use serde_json::json;

/// A session of the fixture's project, with a lifetime in milliseconds since the epoch.
pub(super) fn session_record(id: &str, started_at: i64, ended_at: Option<i64>) -> Session {
    Session {
        id: id.to_string(),
        agent: Agent::Claude,
        agent_session_id: None,
        console_id: "console".to_string(),
        project_id: Some("project".to_string()),
        host_id: LOCAL_HOST_ID.to_string(),
        role: Role::Project,
        origin: Origin::User,
        title: format!("Session {id}"),
        status: SessionStatus::Working,
        terminal_title: None,
        usage: None,
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
        started_at,
        ended_at,
    }
}

struct Fixture {
    state: Arc<AppState>,
    repo: ScratchDir,
    common_dir: PathBuf,
    _dir: ScratchDir,
}

/// A project of the fixture's console at `path`.
fn project(id: &str, path: &Path) -> Project {
    Project {
        id: id.to_string(),
        console_id: "console".to_string(),
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
    }
}

impl Fixture {
    fn new() -> Self {
        let (state, dir) = app_state("branch-origin");
        let workdir = dir.join("workdir");
        std::fs::create_dir(&workdir).unwrap();
        state
            .store
            .insert_console(&Console {
                id: "console".to_string(),
                name: "Console".to_string(),
                workdir: workdir.to_string_lossy().into_owned(),
                console_session_agent: Agent::Claude,
                default_agent: Agent::Claude,
                claude_account_id: None,
                codex_account_id: None,
                grok_account_id: None,
                claude_console_session_model: None,
                claude_console_session_effort: None,
                codex_console_session_model: None,
                codex_console_session_effort: None,
                grok_console_session_model: None,
                grok_console_session_effort: None,
                icon: None,
                created_at: 0,
            })
            .unwrap();
        let repo = repo_with("branch-origin-repo", &[(b"a", b"a")]);
        state
            .store
            .insert_project(&project("project", &repo))
            .unwrap();
        let common_dir = repository_of(&repo).unwrap();
        Self {
            state,
            repo,
            common_dir,
            _dir: dir,
        }
    }

    /// A session of the project that started `started_secs` ago and is still running.
    fn session(&self, id: &str, started_secs: i64, status: SessionStatus) {
        self.session_between(id, now_millis() - started_secs * 1000, None, status);
    }

    /// A session of the project with a lifetime of its own, in milliseconds since the epoch.
    fn session_between(
        &self,
        id: &str,
        started_at: i64,
        ended_at: Option<i64>,
        status: SessionStatus,
    ) {
        let session = Session {
            status,
            ..session_record(id, started_at, ended_at)
        };
        self.state.store.insert_session(&session).unwrap();
    }

    fn env(&self) -> GitEnv {
        GitEnv::new(SYSTEM_GIT.to_string(), isolated_git_env())
    }

    fn list(&self) -> BranchList {
        compare::branches_in(&self.env(), &self.common_dir, &AtomicBool::new(false)).unwrap()
    }

    /// A hook of `session`'s arriving `ago_secs` ago, carrying `payload`.
    fn hook(&self, session: &str, ago_secs: i64, payload: Value) {
        self.state.branch_watch.activity().heard(
            session,
            Some(Agent::Claude),
            &payload,
            now_millis() - ago_secs * 1000,
        );
    }

    /// A tool call of `session`'s that started just now and is still running.
    fn calling(&self, session: &str) {
        let call = json!({"hook_event_name": "PreToolUse", "tool_use_id": session});
        self.hook(session, 0, call);
    }

    /// A look after a hook, with the previous one having found `known`, deciding late enough
    /// that every hook concerning a branch made just now has landed.
    fn hook_look(&self, known: &HashSet<String>) -> Vec<BranchOrigin> {
        self.hook_look_at(known, &HashMap::new(), now_millis() + 60_000)
            .rows
    }

    /// A look after a hook deciding at `now_ms`, with what an earlier one left `undecided`.
    fn hook_look_at(
        &self,
        known: &HashSet<String>,
        undecided: &HashMap<String, UndecidedBranch>,
        now_ms: i64,
    ) -> Reconciled {
        let fresh = HashSet::new();
        let window = Window {
            known: Some(known),
            fresh: &fresh,
            undecided,
            since_secs: Some(now_millis() / 1000 - 60),
            listed_ms: now_millis(),
            now_ms,
        };
        reconcile(&self.state, &self.common_dir, &self.list(), Some(&window)).unwrap()
    }

    /// The hook path's own look, driven through the watch so the window is the one the watch
    /// hands out rather than one the test wrote. Far enough past the floor to run at once.
    fn look_after_hook(&self) {
        self.run_claim(Caller::Hook);
    }

    /// The look the floor held back, as `on_hook` runs it once its wait is over.
    fn look_when_the_floor_is_over(&self) {
        self.run_claim(Caller::Trailing);
    }

    /// Decides as late as [`Fixture::hook_look`] does.
    fn run_claim(&self, caller: Caller) {
        let stamp = RefsStamp::read(&self.common_dir);
        let claim = self.state.branch_watch.claim(
            &self.common_dir,
            caller,
            &stamp,
            Instant::now() + FLOOR,
            now_millis(),
        );
        let Claim::Run(look) = claim else {
            panic!("the look is claimed");
        };
        look_with(&self.state, &self.env(), stamp, look, &|| {
            now_millis() + 60_000
        });
    }

    /// Moves `refs/heads` back in time, so a ref written after a look changes the stamp
    /// whatever the filesystem's time resolution is. That directory only: a branch whose name
    /// has a slash in it is written in one below it, which this leaves as it is.
    fn age_the_refs(&self) {
        move_directory_time_back(&self.common_dir.join("refs/heads"));
    }

    /// Branches at `HEAD`, each with the first reflog entry a branch's own creation writes,
    /// carrying `at_secs` as its time. One `git` for the lot, where a `git branch` each would
    /// be a thousand processes.
    fn branches_at(&self, names: &[String], at_secs: i64) {
        let head = String::from_utf8(git(&self.repo, &["rev-parse", "HEAD"])).unwrap();
        let refs: String = names
            .iter()
            .map(|name| format!("create refs/heads/{name} {}\n", head.trim()))
            .collect();
        git_stdin_env(
            &self.repo,
            &["update-ref", "-m", "branch: Created from HEAD", "--stdin"],
            &[("GIT_COMMITTER_DATE", &format!("{at_secs} +0000"))],
            refs.as_bytes(),
        );
    }

    fn names(&self) -> HashSet<String> {
        self.list().branches.into_iter().map(|b| b.name).collect()
    }

    fn rows(&self) -> Vec<BranchOrigin> {
        self.state
            .store
            .branch_origins(&repo_key(&self.common_dir))
            .unwrap()
    }

    fn row(&self, branch: &str) -> Option<BranchOrigin> {
        self.rows().into_iter().find(|row| row.branch == branch)
    }

    fn reconcile_full(&self) -> Vec<BranchOrigin> {
        let list = self.list();
        reconcile(&self.state, &self.common_dir, &list, None)
            .unwrap()
            .rows
    }
}

/// Parallel sessions are nearly always mid-turn, so being mid-turn cannot block a branch: only
/// a session running a tool when the branch was born could have made it.
#[test]
fn a_branch_belongs_to_the_only_session_with_a_tool_call_in_flight() {
    let fixture = Fixture::new();
    fixture.session("a", 600, SessionStatus::Working);
    fixture.session("b", 600, SessionStatus::Working);
    // `b` is mid-turn too, but its last call ended well before the branch, past every slack.
    let call = |event| json!({"hook_event_name": event, "tool_use_id": "b1"});
    fixture.hook("b", 60, call("PreToolUse"));
    fixture.hook("b", 30, call("PostToolUse"));
    fixture.calling("a");
    let known = fixture.names();
    git(&fixture.repo, &["branch", "feature"]);

    let rows = fixture.hook_look(&known);

    let row = rows.iter().find(|row| row.branch == "feature").unwrap();
    assert_eq!(row.source, BranchOriginSource::HookDiff);
    assert_eq!(row.session_id.as_deref(), Some("a"));
    assert_eq!(row.session_title.as_deref(), Some("Session a"));
    assert!(row.birth_commit.is_some(), "held against the reflog");
}

#[test]
fn tool_calls_of_two_sessions_in_flight_at_once_leave_the_branch_unknown() {
    let fixture = Fixture::new();
    fixture.session("a", 600, SessionStatus::Working);
    fixture.session("b", 600, SessionStatus::Working);
    fixture.calling("a");
    fixture.calling("b");
    let known = fixture.names();
    git(&fixture.repo, &["branch", "shared"]);

    fixture.hook_look(&known);

    let row = fixture.row("shared").unwrap();
    assert_eq!(
        (row.source, row.session_id),
        (BranchOriginSource::Unknown, None)
    );
}

/// The hooks are asynchronous, so the start of the call that made a branch can land after another
/// session's end has prompted the look. Decided on what had landed by then, the branch would go to
/// the other session for good.
#[test]
fn a_branch_waits_for_tool_call_starts_still_on_their_way() {
    let fixture = Fixture::new();
    fixture.session("a", 600, SessionStatus::Working);
    fixture.session("b", 600, SessionStatus::Working);
    let call = |event| json!({"hook_event_name": event, "tool_use_id": "b1"});
    fixture.hook("b", 1, call("PreToolUse"));
    fixture.hook("b", 0, call("PostToolUse"));
    let known = fixture.names();
    git(&fixture.repo, &["branch", "feature"]);

    let first = fixture.hook_look_at(&known, &HashMap::new(), now_millis());
    assert!(fixture.row("feature").is_none(), "nothing is decided yet");
    assert!(first.undecided.contains_key("feature"));

    fixture.calling("a");
    fixture.hook_look_at(&known, &first.undecided, now_millis() + 60_000);

    let row = fixture.row("feature").unwrap();
    assert_eq!(
        (row.source, row.session_id),
        (BranchOriginSource::Unknown, None),
        "the call that landed late overlapped the other one"
    );
}

/// The window begins at the previous look. With no look behind it, nothing says how long ago
/// what a listing handed it appeared, so it names nobody.
#[test]
fn a_window_with_no_look_behind_it_attributes_nothing() {
    let fixture = Fixture::new();
    fixture.session("a", 600, SessionStatus::Working);
    // A second session alive at the branch's birth, so the reflog cannot name one either and
    // the row left behind is the window's own answer.
    fixture.session("b", 600, SessionStatus::Idle);
    fixture.calling("a");
    git(&fixture.repo, &["branch", "loose"]);
    let known = fixture.names();
    let fresh = HashSet::from(["loose".to_string()]);
    let window = Window {
        known: Some(&known),
        fresh: &fresh,
        undecided: &HashMap::new(),
        since_secs: None,
        listed_ms: now_millis(),
        now_ms: now_millis() + 60_000,
    };

    reconcile(
        &fixture.state,
        &fixture.common_dir,
        &fixture.list(),
        Some(&window),
    )
    .unwrap();

    let row = fixture.row("loose").unwrap();
    assert_eq!(
        (row.source, row.session_id),
        (BranchOriginSource::Unknown, None)
    );
}

/// When no tool call names a session, the one session working in the branch's worktree does,
/// even from inside a worktree nested in the main one: that is the innermost holding it.
#[test]
fn a_branch_nobody_was_seen_making_belongs_to_the_only_session_in_its_worktree() {
    let fixture = Fixture::new();
    fixture.session("a", 600, SessionStatus::Working);
    fixture.session("b", 600, SessionStatus::Working);
    let linked = fixture.repo.join(".worktrees/wt");
    let path = linked.to_str().unwrap();
    git(&fixture.repo, &["worktree", "add", "-q", "-b", "wt", path]);
    std::fs::create_dir(linked.join("sub")).unwrap();
    let cwd = |dir: &Path| json!({"hook_event_name": "Stop", "cwd": dir});
    fixture.hook("a", 0, cwd(&linked.join("sub")));
    fixture.hook("b", 0, cwd(&fixture.repo));

    fixture.reconcile_full();

    let row = fixture.row("wt").unwrap();
    assert_eq!(
        (row.source, row.session_id.as_deref()),
        (BranchOriginSource::WorktreeCwd, Some("a"))
    );
    assert_eq!(
        fixture.row("main").unwrap().source,
        BranchOriginSource::Unknown,
        "a branch no creation is recorded for is nobody's however alone its worker is"
    );
}

#[test]
fn two_sessions_in_a_branch_worktree_leave_it_unknown() {
    let fixture = Fixture::new();
    fixture.session("a", 600, SessionStatus::Working);
    fixture.session("b", 600, SessionStatus::Working);
    let linked = fixture._dir.join("linked");
    let path = linked.to_str().unwrap();
    git(&fixture.repo, &["worktree", "add", "-q", "-b", "wt", path]);
    for session in ["a", "b"] {
        fixture.hook(
            session,
            0,
            json!({"hook_event_name": "Stop", "cwd": linked}),
        );
    }

    fixture.reconcile_full();

    assert_eq!(
        fixture.row("wt").unwrap().source,
        BranchOriginSource::Unknown
    );
}

/// One window writes no more rows than the cap. Past it the trim would drop most of them again
/// in the same transaction, and by the branches' birth times rather than by what this window
/// found.
#[test]
fn a_window_past_the_cap_writes_no_more_rows_than_it() {
    let fixture = Fixture::new();
    fixture.session("a", 600, SessionStatus::Working);
    fixture.calling("a");
    let names: Vec<String> = (0..budget::MAX_BRANCH_ORIGINS)
        .map(|n| format!("b{n:05}"))
        .collect();
    // Inside the window, and older than the branch made after them, which is what the trim
    // would keep over them.
    fixture.branches_at(&names, now_millis() / 1000 - 30);
    git(&fixture.repo, &["branch", "zz-last"]);

    fixture.hook_look(&HashSet::from(["main".to_string()]));

    assert_eq!(fixture.rows().len(), budget::MAX_BRANCH_ORIGINS);
    assert!(
        fixture.row("zz-last").is_none(),
        "the branches past the cap are the ones the listing reaches last"
    );
}

/// A read of the store that fails is not "no session of this repository": recorded that way,
/// the branch is stored as unattributable and the reflog is never consulted for it again.
#[test]
fn a_failed_read_of_the_sessions_fails_the_look() {
    let fixture = Fixture::new();
    fixture.session("a", 600, SessionStatus::Idle);
    git(&fixture.repo, &["branch", "earlier"]);
    let list = fixture.list();
    fixture
        .state
        .store
        .connection()
        .execute_batch("DROP TABLE sessions")
        .unwrap();

    assert!(reconcile(&fixture.state, &fixture.common_dir, &list, None).is_err());
    assert!(fixture.rows().is_empty(), "nothing is recorded from it");
}

#[test]
fn a_branch_the_panel_listed_first_is_still_the_next_look_to_attribute() {
    let fixture = Fixture::new();
    fixture.session("a", 600, SessionStatus::Working);
    fixture.session("b", 600, SessionStatus::Idle);
    fixture.age_the_refs();
    fixture.look_after_hook();
    fixture.calling("a");
    git(&fixture.repo, &["switch", "-q", "-c", "feature"]);

    // The panel polls while the hook's own look is still held back by the floor: two sessions
    // were alive at the branch's birth and both are in its worktree, so the listing can name
    // none, and it moves the baseline of known branches on to what it found.
    let mut list = fixture.list();
    annotate(&fixture.state, &fixture.common_dir, &mut list);
    assert_eq!(
        fixture.row("feature").unwrap().source,
        BranchOriginSource::Unknown
    );

    fixture.look_after_hook();

    let row = fixture.row("feature").unwrap();
    assert_eq!(
        (row.source, row.session_id.as_deref()),
        (BranchOriginSource::HookDiff, Some("a")),
        "what the listing could not tell, the window still can"
    );
}

#[test]
fn the_look_the_floor_held_back_attributes_what_appeared_in_its_window() {
    let fixture = Fixture::new();
    fixture.session("a", 600, SessionStatus::Working);
    fixture.age_the_refs();
    fixture.look_after_hook();
    fixture.calling("a");
    git(&fixture.repo, &["switch", "-q", "-c", "feature"]);

    let (repo, wait) =
        hook_arrived(&fixture.state, "a").expect("the look is held back, not dropped");
    assert_eq!(repo, fixture.common_dir);
    assert!(wait <= FLOOR);
    assert!(fixture.row("feature").is_none(), "nothing looked yet");

    fixture.look_when_the_floor_is_over();

    let row = fixture.row("feature").unwrap();
    assert_eq!(
        (row.source, row.session_id.as_deref()),
        (BranchOriginSource::HookDiff, Some("a"))
    );
}

#[test]
fn a_branch_with_no_session_to_name_is_not_sent() {
    let fixture = Fixture::new();
    git(&fixture.repo, &["branch", "mine"]);
    let mut list = fixture.list();

    annotate(&fixture.state, &fixture.common_dir, &mut list);

    assert!(list.branches.iter().all(|branch| branch.origin.is_none()));
    assert_eq!(
        fixture.row("mine").unwrap().source,
        BranchOriginSource::Unknown
    );
}

#[test]
fn a_branch_the_daemon_never_saw_appear_is_matched_by_its_reflog() {
    let fixture = Fixture::new();
    fixture.session("a", 600, SessionStatus::Idle);
    git(&fixture.repo, &["branch", "earlier"]);

    let mut list = fixture.list();
    annotate(&fixture.state, &fixture.common_dir, &mut list);

    let origin = list
        .branches
        .iter()
        .find(|branch| branch.name == "earlier")
        .and_then(|branch| branch.origin.clone())
        .unwrap();
    assert_eq!(
        (origin.source, origin.session.as_str()),
        (BranchOriginSource::ReflogMatch, "a")
    );

    // With a second session alive at that second it cannot be told.
    fixture.session("b", 600, SessionStatus::Idle);
    git(&fixture.repo, &["branch", "later"]);
    fixture.reconcile_full();
    assert_eq!(
        fixture.row("later").unwrap().source,
        BranchOriginSource::Unknown
    );
}

#[test]
fn a_session_that_ended_a_second_before_the_branch_still_counts_as_alive_for_it() {
    let fixture = Fixture::new();
    fixture.session("a", 600, SessionStatus::Idle);
    let ended = now_millis();
    fixture.session_between("b", ended - 600_000, Some(ended), SessionStatus::Idle);
    fixture.branches_at(&["close".to_string()], ended / 1000 + 1);

    fixture.reconcile_full();

    assert_eq!(
        fixture.row("close").unwrap().source,
        BranchOriginSource::Unknown,
        "two sessions are within the clock's slack of the branch"
    );
}

#[test]
fn a_branch_made_again_under_the_same_name_does_not_inherit_the_attribution() {
    let fixture = Fixture::new();
    fixture.session("a", 600, SessionStatus::Idle);
    fixture.calling("a");
    let known = fixture.names();
    git(&fixture.repo, &["branch", "topic"]);
    fixture.hook_look(&known);
    assert_eq!(
        fixture.row("topic").unwrap().session_id.as_deref(),
        Some("a")
    );

    // A different tip, so the new reflog's first entry cannot equal the old one even within
    // the same second.
    git(&fixture.repo, &["branch", "-q", "-D", "topic"]);
    git(
        &fixture.repo,
        &["commit", "-q", "--allow-empty", "-m", "next"],
    );
    git(&fixture.repo, &["branch", "topic"]);
    fixture.session("b", 600, SessionStatus::Idle);
    let rows = fixture.reconcile_full();

    let row = rows.iter().find(|row| row.branch == "topic").unwrap();
    assert_ne!(row.session_id.as_deref(), Some("a"), "the old row is void");
    assert_eq!(
        row.source,
        BranchOriginSource::Unknown,
        "two sessions were alive"
    );
}

#[test]
fn a_row_goes_with_its_branch_but_only_on_a_full_listing() {
    let fixture = Fixture::new();
    fixture.session("a", 600, SessionStatus::Idle);
    git(&fixture.repo, &["branch", "gone"]);
    fixture.reconcile_full();
    assert!(fixture.row("gone").is_some());
    git(&fixture.repo, &["branch", "-q", "-D", "gone"]);

    let cut = BranchList {
        complete: false,
        ..fixture.list()
    };
    reconcile(&fixture.state, &fixture.common_dir, &cut, None).unwrap();
    assert!(
        fixture.row("gone").is_some(),
        "a cut listing proves nothing"
    );

    fixture.reconcile_full();
    assert!(fixture.row("gone").is_none());
}

#[test]
fn a_listing_older_than_the_last_record_does_not_remove_it() {
    let fixture = Fixture::new();
    fixture.session("a", 600, SessionStatus::Working);
    // A reply listed before the branch was made, as a client's request that overlapped the
    // hook's look carries.
    let stale = fixture.list();
    let known = fixture.names();
    fixture.calling("a");
    git(&fixture.repo, &["switch", "-q", "-c", "feature"]);
    fixture.hook_look(&known);
    assert_eq!(
        fixture.row("feature").unwrap().source,
        BranchOriginSource::HookDiff
    );

    let mut list = stale;
    annotate(&fixture.state, &fixture.common_dir, &mut list);

    assert!(
        fixture.row("feature").is_some(),
        "the branch is missing from the listing because it was made after it"
    );
}

#[test]
fn deleting_a_project_leaves_the_origins_of_another_in_the_same_repository() {
    let fixture = Fixture::new();
    fixture.session("a", 600, SessionStatus::Idle);
    let inner = fixture.repo.join("inner");
    std::fs::create_dir(&inner).unwrap();
    fixture
        .state
        .store
        .insert_project(&project("other", &inner))
        .unwrap();
    git(&fixture.repo, &["branch", "topic"]);
    fixture.reconcile_full();
    assert!(fixture.row("topic").is_some());

    fixture.state.store.delete_project("project").unwrap();
    project_deleted(&fixture.state, &fixture.repo.to_string_lossy());
    assert!(
        fixture.row("topic").is_some(),
        "another project is still in the repository"
    );

    fixture.state.store.delete_project("other").unwrap();
    project_deleted(&fixture.state, &inner.to_string_lossy());
    assert!(fixture.rows().is_empty(), "the last project's deletion");
}

/// The rows of a repository are written under its gate, a project's deletion included: a
/// reconcile holding the gate has read the rows it is about to write, and would put back what
/// the deletion just removed — leaving a repository with rows, no project to clear them and
/// nothing that runs here for it again.
#[test]
fn deleting_a_project_waits_for_a_reconcile_holding_the_gate() {
    let fixture = Fixture::new();
    git(&fixture.repo, &["branch", "topic"]);
    fixture.reconcile_full();
    assert!(fixture.row("topic").is_some());

    let gate = fixture.state.branch_watch.gate(&fixture.common_dir);
    let held = gate.lock().unwrap();
    let (done, deleted) = std::sync::mpsc::channel();
    let state = Arc::clone(&fixture.state);
    let path = fixture.repo.to_string_lossy().into_owned();
    let worker = std::thread::spawn(move || {
        state.store.delete_project("project").unwrap();
        project_deleted(&state, &path);
        done.send(()).unwrap();
    });

    // The worker is at the gate once it is past the store's deletion; before that, nothing
    // has been waited for.
    let start = Instant::now();
    while fixture
        .state
        .store
        .get_project("project")
        .unwrap()
        .is_some()
    {
        assert!(start.elapsed() < PATIENCE, "the worker never started");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(fixture.row("topic").is_some());
    assert!(
        deleted.try_recv().is_err(),
        "the deletion waits for the gate"
    );
    drop(held);

    deleted
        .recv_timeout(PATIENCE)
        .expect("the deletion goes on");
    worker.join().unwrap();
    assert!(fixture.rows().is_empty());
}

/// A reconcile that read its project before a deletion finished and takes the gate after it
/// has nothing to name branches for, and nothing would clear what it wrote.
#[test]
fn a_reconcile_after_the_last_project_went_writes_nothing() {
    let fixture = Fixture::new();
    git(&fixture.repo, &["branch", "topic"]);
    let list = fixture.list();
    fixture.reconcile_full();
    fixture.state.store.delete_project("project").unwrap();
    project_deleted(&fixture.state, &fixture.repo.to_string_lossy());
    assert!(fixture.rows().is_empty());

    reconcile(&fixture.state, &fixture.common_dir, &list, None).unwrap();

    assert!(fixture.rows().is_empty());
}

/// The same late listing must not set the in-memory record of the repository up again either:
/// nothing is left to clear it, and it would hold every branch name until the daemon restarts.
#[test]
fn a_listing_after_the_last_project_went_leaves_no_record_of_the_repository() {
    let fixture = Fixture::new();
    let mut list = fixture.list();
    fixture.state.store.delete_project("project").unwrap();
    project_deleted(&fixture.state, &fixture.repo.to_string_lossy());
    assert!(fixture.state.branch_watch.holds_no_repository());

    annotate(&fixture.state, &fixture.common_dir, &mut list);

    assert!(fixture.state.branch_watch.holds_no_repository());
}

/// What the `list_project_branches` reply reserves room for (`budget::BRANCH_ORIGIN_BYTES`)
/// holds only because one listing attaches no more origins than the cap.
#[test]
fn a_listing_attaches_no_more_origins_than_the_cap() {
    let fixture = Fixture::new();
    fixture.session("a", 600, SessionStatus::Idle);
    let names: Vec<String> = (0..=budget::MAX_BRANCH_ORIGINS)
        .map(|n| format!("b{n:05}"))
        .collect();
    fixture.branches_at(&names, now_millis() / 1000);

    let mut list = fixture.list();
    annotate(&fixture.state, &fixture.common_dir, &mut list);

    assert!(list.complete, "the listing itself is not cut");
    assert_eq!(
        list.branches
            .iter()
            .filter(|branch| branch.origin.is_some())
            .count(),
        budget::MAX_BRANCH_ORIGINS
    );
}

#[test]
fn rows_past_the_cap_are_dropped_oldest_first() {
    let fixture = Fixture::new();
    let key = repo_key(&fixture.common_dir);
    let put: Vec<BranchOrigin> = (0..5)
        .map(|n| {
            let mut row = origin_of(
                &key,
                &format!("b{n}"),
                None,
                None,
                BranchOriginSource::Unknown,
            );
            row.created_at = n;
            row
        })
        .collect();
    fixture
        .state
        .store
        .apply_branch_origins(&key, &[], &put, 2)
        .unwrap();
    let kept: Vec<_> = fixture.rows().into_iter().map(|row| row.branch).collect();
    assert_eq!(kept, ["b3", "b4"]);
}
