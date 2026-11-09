use std::path::Path;
use std::sync::Mutex;

use serde_json::json;

use super::*;
use crate::branch_origin::repo_key;
use crate::git::repo_files::repository_of;
use crate::protocol::{Account, Availability, Role, SessionKind};
use crate::session_tools::Owner;
use crate::test_support::{
    app_state, branch_made_by, branch_made_from, console, heard_after_marker, insert_project,
    output_containing, repo_with, running, stand_in_launch_echoing, Launched, ScratchDir, StandIn,
};

/// A stored session of console `console-1`: a project session of project `p1` when `project`, else
/// a console session. `bound_to` is its parent, and `coordinator` says it keeps the coordinator
/// role under it; a session with a parent and no `coordinator` is a worker.
fn session(
    id: &str,
    project: bool,
    status: &str,
    bound_to: Option<&str>,
    coordinator: bool,
) -> Session {
    let role = if bound_to.is_none() || coordinator {
        Role::Coordinator
    } else {
        Role::Worker
    };
    let session: Session = serde_json::from_value(json!({
        "id": id, "agent": "claude", "agent_session_id": null, "console_id": "console-1",
        "project_id": project.then_some("p1"), "host_id": crate::store::LOCAL_HOST_ID,
        "kind": if project { "project" } else { "console" }, "origin": "user",
        "title": format!("Session {id}"), "status": status, "has_conversation": true,
        "pinned": false, "started_at": 0, "bound_to": bound_to,
        "role": role,
        "colour": (!project).then_some("pink"), "ordinal": (!project).then_some(1),
    }))
    .unwrap();
    session
}

/// What a handoff must leave alone when it is refused or fails: each session's status, owner and
/// role, who is responsible for the test branches, and the packages on disk.
type Snapshot = (
    Vec<(String, SessionStatus, Option<String>, Role)>,
    Vec<String>,
    Vec<std::path::PathBuf>,
);

struct Fixture {
    state: Arc<AppState>,
    dir: ScratchDir,
    repo: ScratchDir,
    /// Every stand-in process the launch below was asked for, kept alive for the test.
    started: Arc<Mutex<Vec<StandIn>>>,
    launches: Arc<Mutex<Vec<Launched>>>,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let (state, dir) = app_state(&format!("coordinator-handoff-{name}"));
        let console = console(&dir);
        std::fs::create_dir_all(&console.workdir).unwrap();
        state.store.insert_console(&console).unwrap();
        let repo = repo_with(
            &format!("coordinator-handoff-{name}-repo"),
            &[(b"a.txt", b"one\n")],
        );
        insert_project(&state, &repo);
        Self {
            state,
            dir,
            repo,
            started: Default::default(),
            launches: Default::default(),
        }
    }

    /// Stores `session` and gives it a process that reads what is written to it.
    fn run(&self, session: Session) -> StandIn {
        self.state.store.insert_session(&session).unwrap();
        running(&self.state, &session.id)
    }

    /// A launch that starts a stand-in process for the session asked for.
    fn launch(&self) -> impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static {
        self.launch_after(|| {})
    }

    /// [`Self::launch`], running `during` first: what happens while the agent is being started.
    fn launch_after(
        &self,
        during: impl FnOnce() + Send + 'static,
    ) -> impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static {
        // The combined prompt exceeds the cooked terminal's echo buffer; drain it like an agent.
        let launch =
            stand_in_launch_echoing(&self.started, &self.launches, "handoff-input-ready", during);
        move |request| {
            let launched = launch(request)?;
            let ready = output_containing(&launched.session, "handoff-input-ready");
            assert!(ready.contains("handoff-input-ready"), "{ready}");
            Ok(launched)
        }
    }

    async fn handoff(&self, session: &str) -> Result<Session> {
        self.handoff_to(session, AgentId::CODEX).await
    }

    async fn handoff_to(&self, session: &str, agent: AgentId) -> Result<Session> {
        self.handoff_launching(session, agent, self.launch()).await
    }

    async fn handoff_launching(
        &self,
        session: &str,
        agent: AgentId,
        launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
    ) -> Result<Session> {
        handoff_session_with(
            &self.state,
            Handoff {
                session: session.to_string(),
                agent,
                account: None,
                model: None,
                effort: None,
                title: None,
                note: None,
                asked_by: None,
                terminal_appearance: None,
            },
            launch,
        )
        .await
        .map(|moved| moved.receiving)
    }

    fn stored(&self, id: &str) -> Session {
        self.state.store.get_session(id).unwrap().unwrap()
    }

    fn responsible(&self, branch: &str) -> String {
        let key = repo_key(&repository_of(&self.repo).unwrap());
        self.state
            .store
            .branch_origins(&key)
            .unwrap()
            .into_iter()
            .find(|origin| origin.branch == branch)
            .unwrap()
            .responsible_session
    }

    fn output_of(&self, id: &str, needle: &str) -> String {
        let started = self.started.lock().unwrap();
        let live = started
            .iter()
            .find(|live| live.id == id)
            .expect("started by the handoff");
        output_containing(live, needle)
    }

    /// Everything a refused or failed handoff must leave alone.
    fn snapshot(&self) -> Snapshot {
        let mut sessions: Vec<_> = self
            .state
            .store
            .list_sessions()
            .unwrap()
            .into_iter()
            .map(|session| (session.id, session.status, session.bound_to, session.role))
            .collect();
        sessions.sort_by(|a, b| a.0.cmp(&b.0));
        let branches = ["one", "two"]
            .into_iter()
            .filter(|branch| {
                let key = repo_key(&repository_of(&self.repo).unwrap());
                self.state
                    .store
                    .branch_origins(&key)
                    .unwrap()
                    .iter()
                    .any(|origin| &origin.branch == branch)
            })
            .map(|branch| self.responsible(branch))
            .collect();
        let packages = std::fs::read_dir(self.state.handoff_dir())
            .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
            .unwrap_or_default();
        (sessions, branches, packages)
    }
}

/// A session handed to Codex on a non-default account starts keeping its MCP OAuth tokens in that
/// account's own home; one handed to the default account does not, as its home is the one Codex
/// itself uses.
#[tokio::test]
async fn a_handoff_to_codex_keeps_mcp_tokens_in_a_non_default_accounts_home_only() {
    for (case, account) in [("account", Some("codex-account")), ("default", None)] {
        let fixture = Fixture::new(&format!("codex-store-{case}"));
        let home = fixture.dir.join("codex-account");
        std::fs::create_dir_all(&home).unwrap();
        fixture
            .state
            .store
            .insert_account(&Account {
                id: "codex-account".to_string(),
                agent: AgentId::CODEX,
                name: "Work".to_string(),
                config_dir: home.to_string_lossy().into_owned(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
                versions: Default::default(),
            })
            .unwrap();
        fixture.run(session("original", true, "idle", None, false));
        let requested = Arc::new(Mutex::new(Vec::new()));
        let seen = requested.clone();
        let launch = fixture.launch();

        handoff_session_with(
            &fixture.state,
            Handoff {
                session: "original".to_string(),
                agent: AgentId::CODEX,
                account: Some(account.map(str::to_string)),
                model: None,
                effort: None,
                title: None,
                note: None,
                asked_by: None,
                terminal_appearance: None,
            },
            move |request: term::LaunchRequest| {
                seen.lock().unwrap().push((
                    request.agent,
                    request.config_dir.clone(),
                    crate::adapter::tests::codex_file_credential_overrides(
                        request.config_dir.as_deref(),
                        request.resume_agent_session_id.as_deref(),
                    ),
                ));
                launch(request)
            },
        )
        .await
        .unwrap();

        let expected = account.map(|_| home.clone());
        assert_eq!(
            *requested.lock().unwrap(),
            [(AgentId::CODEX, expected, usize::from(account.is_some()))],
            "{case}"
        );
    }
}

/// A receiver that exits before bootstrap delivery leaves a visible failure, not only a log.
#[tokio::test]
async fn a_rejected_first_work_prompt_is_announced_to_the_user() {
    let fixture = Fixture::new("bootstrap-rejected");
    let first = fixture.state.reserve_first_message("receiving").unwrap();
    fixture
        .state
        .store
        .insert_session(&session("receiving", true, "interrupted", None, false))
        .unwrap();
    let mut events = fixture.state.subscribe();
    start_work(
        &fixture.state,
        first,
        "Read the handoff package".to_string(),
    )
    .await;
    let mut notices = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let crate::protocol::Event::SessionNotice {
            session,
            code,
            message,
            ..
        } = event
        {
            notices.push((session, code, message));
        }
    }
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(notices[0].0, "receiving");
    assert_eq!(notices[0].1, notice_code::HANDOFF_MESSAGES_DROPPED);
    assert!(notices[0].2.contains("initial work prompt"), "{notices:?}");
    assert!(
        notices[0].2.contains("this session is not running"),
        "{notices:?}"
    );
}

/// Native queues receive one bootstrap after the original is archived and the place has moved.
#[tokio::test]
async fn the_first_native_submission_has_the_completed_handoff_context() {
    struct Capture {
        state: Arc<AppState>,
        repo: String,
        messages: Mutex<Vec<String>>,
    }
    impl term::MessageTransport for Capture {
        fn send(
            &self,
            text: &str,
            _: &str,
        ) -> std::result::Result<term::Sent, crate::platform::term::PartialWrite> {
            assert_eq!(
                self.state.session_record("original").unwrap().status,
                SessionStatus::Archived
            );
            let origins = self.state.store.branch_origins(&self.repo).unwrap();
            let receiving = &origins
                .iter()
                .find(|origin| origin.branch == "one")
                .unwrap()
                .responsible_session;
            let stored = self.state.session_record(receiving).unwrap();
            assert_eq!(stored.bound_to.as_deref(), Some("hub"));
            assert_eq!(stored.role, Role::Coordinator);
            assert_eq!(
                self.state
                    .session_record("worker")
                    .unwrap()
                    .bound_to
                    .as_deref(),
                Some(receiving.as_str())
            );
            self.messages.lock().unwrap().push(text.to_string());
            Ok(term::Sent::Queued)
        }
        fn attach(&self, _: term::TransportEvents) {}
        fn start_queue(&self) -> term::QueueStart {
            term::QueueStart::Nothing
        }
        fn stop(&self) {}
    }
    let fixture = Fixture::new("native-first-prompt");
    fixture.run(session("hub", false, "idle", None, false));
    fixture.run(session("original", true, "idle", Some("hub"), true));
    fixture.run(session("worker", true, "idle", Some("original"), false));
    branch_made_from(
        &fixture.state,
        &fixture.repo,
        "one",
        "original",
        Some("main"),
    );
    let capture = Arc::new(Capture {
        state: fixture.state.clone(),
        repo: repo_key(&repository_of(&fixture.repo).unwrap()),
        messages: Default::default(),
    });
    let (started, launches, transport) = (
        fixture.started.clone(),
        fixture.launches.clone(),
        capture.clone(),
    );
    let receiving = fixture
        .handoff_launching("original", AgentId::CODEX, move |request| {
            assert!(
                request.task.is_none(),
                "the process must start without a working turn"
            );
            let live = crate::test_support::tagged_idle_live_session(
                &request.session_id,
                request.agent,
                &request.process_tag,
            )
            .with_transport(transport);
            crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
            let process = (*live).clone();
            started.lock().unwrap().push(live);
            launches
                .lock()
                .unwrap()
                .push((request.session_id, request.owner, request.task));
            Ok(term::Launch {
                session: process,
                agent_session_id: None,
                notice: None,
            })
        })
        .await
        .unwrap();
    let observed = capture.clone();
    assert!(
        crate::test_support::within(crate::test_support::PATIENCE, move || !observed
            .messages
            .lock()
            .unwrap()
            .is_empty())
        .await
    );
    let messages = capture.messages.lock().unwrap();
    assert_eq!(messages.len(), 1, "{messages:?}");
    let prompt = &messages[0];
    assert!(
        prompt.starts_with(crate::messaging::texts::TAKEOVER_OPENING),
        "{prompt}"
    );
    assert!(prompt.contains(&format!("{}.md", receiving.id)), "{prompt}");
    for expected in [
        "Read that file first",
        "original is archived",
        "Your role is coordinator and your parent session",
        "`hub`",
        "`one` (target branch `main`)",
        "Your child session:",
        "`worker`",
    ] {
        assert!(prompt.contains(expected), "missing {expected}: {prompt}");
    }
}

/// A child can report after its parent changes while branch transfer is still blocked. Its real
/// report must wait behind the completed bootstrap, rather than become the successor's first turn.
#[tokio::test(flavor = "multi_thread")]
async fn a_report_during_branch_transfer_follows_the_completed_bootstrap_once() {
    let fixture = Fixture::new("report-during-transfer");
    fixture.run(session("original", true, "idle", None, false));
    let _worker = fixture.run(session("worker", true, "idle", Some("original"), false));
    branch_made_by(&fixture.state, &fixture.repo, "one", "original");
    let repo = repository_of(&fixture.repo).unwrap();
    let (release, released) = std::sync::mpsc::channel();
    let (ready, acquired) = std::sync::mpsc::channel();
    let state = fixture.state.clone();
    let gate_thread = Arc::new(Mutex::new(None));
    let gate_handle = gate_thread.clone();
    let during = fixture.launch_after(move || {
        let gate = std::thread::spawn(move || {
            state.branch_origins.with_repository_held(&repo, || {
                ready.send(()).unwrap();
                released
                    .recv_timeout(crate::test_support::PATIENCE)
                    .unwrap();
            });
        });
        *gate_handle.lock().unwrap() = Some(gate);
        acquired
            .recv_timeout(crate::test_support::PATIENCE)
            .unwrap();
    });
    let state = fixture.state.clone();
    let report = async move {
        let rebound = state.clone();
        assert!(
            crate::test_support::within(crate::test_support::PATIENCE, move || {
                rebound
                    .session_record("worker")
                    .unwrap()
                    .bound_to
                    .as_deref()
                    != Some("original")
            })
            .await,
            "the child was rebound while branch transfer waited"
        );
        let (_, delivered) = crate::messaging::reporting::deliver_own_report(
            &state,
            "worker",
            "report during the branch move".to_string(),
            crate::messaging::reporting::ReportStatus::NeedsDecision,
            vec!["wait for the next instruction".to_string()],
            Vec::new(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(delivered, delivery::Delivery::Queued);
        release.send(()).unwrap();
    };
    let (receiving, ()) = tokio::join!(
        fixture.handoff_launching("original", AgentId::CODEX, during),
        report
    );
    let receiving = receiving.unwrap();
    gate_thread.lock().unwrap().take().unwrap().join().unwrap();
    assert_eq!(fixture.stored("original").status, SessionStatus::Archived);
    assert_eq!(fixture.responsible("one"), receiving.id);
    let started = fixture.started.lock().unwrap();
    let live = started.iter().find(|live| live.id == receiving.id).unwrap();
    let output = heard_after_marker(&fixture.state, live);
    let bootstrap = output
        .find(crate::messaging::texts::TAKEOVER_OPENING)
        .expect("bootstrap arrived");
    let context = output
        .find("what it held is yours now")
        .expect("completed context arrived");
    let report = output
        .find("Report from your child session, the project session `worker`")
        .expect("the real child report arrived");
    assert!(bootstrap < context && context < report, "{output}");
    assert_eq!(
        output.matches("report during the branch move").count(),
        1,
        "{output}"
    );
    assert_eq!(
        output
            .matches(crate::messaging::texts::TAKEOVER_OPENING)
            .count(),
        1,
        "{output}"
    );
}

/// The receiving session gets one prompt with the package and responsibilities after the original
/// is archived; it takes the original's role, and nothing the original owned is archived.
#[tokio::test]
async fn an_unbound_project_session_hands_its_branches_and_sessions_to_the_receiving_session() {
    let fixture = Fixture::new("unbound");
    let mut pinned = session("original", true, "idle", None, false);
    pinned.pinned = true;
    pinned.terminal_title = Some("Fix the build".to_string());
    let original = fixture.run(pinned);
    let owned = fixture.run(session("owned", true, "idle", Some("original"), false));
    fixture.run(session(
        "owned-archived",
        true,
        "archived",
        Some("original"),
        false,
    ));
    branch_made_from(
        &fixture.state,
        &fixture.repo,
        "one",
        "original",
        Some("main"),
    );
    branch_made_by(&fixture.state, &fixture.repo, "two", "original");
    branch_made_by(&fixture.state, &fixture.repo, "theirs", "someone-else");

    let receiving = fixture.handoff("original").await.unwrap();

    let stored = fixture.stored(&receiving.id);
    assert_eq!(stored.agent, AgentId::CODEX);
    assert_eq!(stored.status, SessionStatus::Idle);
    assert!(stored.pinned, "the original's pinned mark is carried over");
    assert_eq!(
        stored.title, "Fix the build｜Session original",
        "named as the original is shown, task title included"
    );
    assert!(fixture.state.has_process(&receiving.id));
    assert_eq!(
        (
            stored.kind,
            stored.project_id.as_deref(),
            stored.bound_to.as_deref(),
            stored.role
        ),
        (SessionKind::Project, Some("p1"), None, Role::Coordinator)
    );
    let launches = fixture.launches.lock().unwrap();
    let (_, owner, task) = &launches[0];
    assert_eq!(*owner, None);
    assert!(
        task.is_none(),
        "handoff work must wait until the original is archived"
    );
    let task = fixture.output_of(&receiving.id, "what it held is yours now");
    let package = fixture
        .state
        .handoff_dir()
        .join(format!("{}.md", receiving.id));
    assert!(task.contains(&package.display().to_string()), "{task}");
    assert!(task.contains("Read that file first"), "{task}");
    assert!(std::fs::read_to_string(&package)
        .unwrap()
        .contains("Session original"));
    drop(launches);

    assert_eq!(fixture.responsible("one"), receiving.id);
    assert_eq!(fixture.responsible("two"), receiving.id);
    assert_eq!(fixture.responsible("theirs"), "someone-else");
    for id in ["owned", "owned-archived"] {
        assert_eq!(
            fixture.stored(id).bound_to.as_deref(),
            Some(receiving.id.as_str())
        );
    }
    assert_eq!(
        fixture.stored("owned-archived").status,
        SessionStatus::Archived
    );
    assert_eq!(fixture.stored("owned").status, SessionStatus::Idle);

    let told = output_containing(&owned, "your parent session has changed");
    assert!(told.contains(&receiving.id), "{told}");
    assert!(
        told.contains("replaces what you were told of your parent session"),
        "{told}"
    );
    let held = fixture.output_of(&receiving.id, "what it held is yours now");
    for expected in [
        "`one`",
        "`two`",
        "`owned`",
        "`owned-archived`",
        "- Your role is coordinator and your reporting relationship is independent: you answer \
         directly to the user and have no parent session.",
    ] {
        assert!(held.contains(expected), "lacks {expected}: {held}");
    }
    assert!(!held.contains("theirs"), "{held}");
    // The branches are named with the target branch each has recorded, where it has one.
    assert!(
        held.contains("the branches `one` (target branch `main`), `two`;"),
        "{held}"
    );
    assert_eq!(fixture.stored("original").status, SessionStatus::Archived);
    let started = fixture.started.lock().unwrap();
    let live = started.iter().find(|live| live.id == receiving.id).unwrap();
    let all = heard_after_marker(&fixture.state, live);
    assert_eq!(
        all.matches(crate::messaging::texts::TAKEOVER_OPENING)
            .count(),
        1,
        "{all}"
    );
    assert_eq!(all.matches("what it held is yours now").count(), 1, "{all}");
    assert!(
        !all.contains("Message from Octoboard about the handoff"),
        "{all}"
    );
    drop(original);
}

/// A bound coordinator keeps its binding to the console session as a bound coordinator, takes the
/// sessions it started, and its owner is told.
#[tokio::test]
async fn a_bound_coordinator_is_replaced_by_a_bound_coordinator_of_the_same_console_session() {
    let fixture = Fixture::new("coordinator");
    let hub = fixture.run(session("hub", false, "idle", None, false));
    fixture.run(session("original", true, "waiting_user", Some("hub"), true));
    fixture.run(session("worker", true, "idle", Some("original"), false));
    branch_made_by(&fixture.state, &fixture.repo, "one", "original");

    let receiving = fixture.handoff("original").await.unwrap();

    let stored = fixture.stored(&receiving.id);
    assert_eq!(stored.bound_to.as_deref(), Some("hub"));
    assert_eq!(stored.role, Role::Coordinator);
    assert_eq!(
        fixture.stored("worker").bound_to.as_deref(),
        Some(receiving.id.as_str())
    );
    assert!(fixture.stored("worker").is_worker());
    assert_eq!(fixture.responsible("one"), receiving.id);
    // It launched as an unbound session does, with that session's tools.
    assert_eq!(fixture.launches.lock().unwrap()[0].1, None);
    let told = output_containing(&hub, "was handed off");
    assert!(
        told.contains(&receiving.id) && told.contains("Session original"),
        "{told}"
    );
    let held = fixture.output_of(&receiving.id, "what it held is yours now");
    assert!(
        held.contains(
            "- Your role is coordinator and your parent session is the console session `hub` \
             (\"Session hub\"): take work only from it, or from the user typing in this \
             terminal, and report to it with `mcp__octoboard__report` once per round of work. \
             This replaces anything your role description says about being independent and not \
             calling `mcp__octoboard__report`. `done` with no open items archives you, and is \
             refused while a session you started is not archived; `needs_decision` or open items \
             leave you running. Items that are your parent session's to carry out rather than \
             yours go in `owner_items`, not `open_items`. You keep your child sessions and go on \
             driving them with `mcp__octoboard__open_session` and the tools that go with it."
        ),
        "{held}"
    );
    assert!(
        held.contains("replaces anything your role description says"),
        "{held}"
    );
    assert!(
        held.contains("`done` with no open items archives you"),
        "{held}"
    );
    assert_eq!(fixture.stored("original").status, SessionStatus::Archived);
}

/// A bound session reports to the same owner from its first launch; it owns nothing.
#[tokio::test]
async fn a_bound_session_is_replaced_by_a_session_bound_to_the_same_owner() {
    let fixture = Fixture::new("bound");
    let boss = fixture.run(session("boss", true, "idle", None, false));
    fixture.run(session("original", true, "idle", Some("boss"), false));
    branch_made_by(&fixture.state, &fixture.repo, "one", "original");

    let receiving = fixture.handoff("original").await.unwrap();

    let stored = fixture.stored(&receiving.id);
    assert_eq!(
        stored.title, "Session original",
        "with no task title, the original's name alone"
    );
    assert_eq!(
        (stored.bound_to.as_deref(), stored.role),
        (Some("boss"), Role::Worker)
    );
    assert_eq!(
        fixture.launches.lock().unwrap()[0].1,
        Some(Owner::Project {
            session_id: "boss".to_string()
        })
    );
    assert_eq!(fixture.responsible("one"), receiving.id);
    let told = output_containing(&boss, "was handed off");
    assert!(told.contains(&receiving.id), "{told}");
    assert_eq!(fixture.stored("original").status, SessionStatus::Archived);
}

/// An owner with no process because it is interrupted is not skipped: the notice of the handoff is
/// kept for its resume.
#[tokio::test]
async fn the_notice_of_a_handoff_is_kept_for_an_interrupted_owner() {
    let fixture = Fixture::new("interrupted-owner");
    fixture
        .state
        .store
        .insert_session(&session("boss", true, "interrupted", None, false))
        .unwrap();
    fixture.run(session("original", true, "idle", Some("boss"), false));

    let receiving = fixture.handoff("original").await.unwrap();

    let kept = fixture.state.store.kept_messages("boss").unwrap();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert!(kept[0].1.contains("was handed off"), "{kept:?}");
    assert!(kept[0].1.contains(&receiving.id), "{kept:?}");
    assert!(kept[0]
        .1
        .contains("Kept for you while you were interrupted"));
}

/// A console session has no branches and no project; the sessions it owns, its bound coordinators
/// among them, are the ones that move.
#[tokio::test]
async fn a_console_session_hands_the_sessions_it_owns_to_the_receiving_console_session() {
    let fixture = Fixture::new("console");
    fixture.run(session("original", false, "idle", None, false));
    let coordinator = fixture.run(session("coordinator", true, "idle", Some("original"), true));
    fixture.run(session("bound", true, "idle", Some("original"), false));

    let receiving = fixture.handoff("original").await.unwrap();

    let stored = fixture.stored(&receiving.id);
    assert_eq!(
        (
            stored.kind,
            stored.project_id.as_deref(),
            stored.bound_to.as_deref()
        ),
        (SessionKind::Console, None, None)
    );
    assert!(stored.colour.is_some() && stored.ordinal.is_some());
    for id in ["coordinator", "bound"] {
        assert_eq!(
            fixture.stored(id).bound_to.as_deref(),
            Some(receiving.id.as_str())
        );
    }
    assert_eq!(fixture.stored("coordinator").role, Role::Coordinator);
    let told = output_containing(&coordinator, "your parent session has changed");
    assert!(told.contains(&receiving.id), "{told}");
    assert_eq!(fixture.stored("original").status, SessionStatus::Archived);
}

/// Every refusal comes before anything changes: no session is recorded, no package written, no
/// binding, branch or process touched.
#[tokio::test]
async fn a_handoff_that_is_refused_changes_nothing() {
    let fixture = Fixture::new("refused");
    let boss = fixture.run(session("boss", true, "idle", None, false));
    fixture.run(session("idle", true, "idle", Some("boss"), false));
    fixture.run(session("working", true, "working", Some("boss"), false));
    fixture.run(session("starting", true, "starting", Some("boss"), false));
    fixture.run(session("archived", true, "archived", Some("boss"), false));
    let mut locked = session("locked", true, "idle", Some("boss"), false);
    locked.locked = true;
    fixture.run(locked);
    fixture.run(session("with-branch", true, "idle", None, false));
    branch_made_by(&fixture.state, &fixture.repo, "one", "with-branch");
    fixture
        .state
        .store
        .insert_account(&Account {
            id: "claude-account".to_string(),
            agent: AgentId::CLAUDE,
            name: "Work".to_string(),
            config_dir: fixture.dir.join("account").to_string_lossy().into_owned(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
            versions: Default::default(),
        })
        .unwrap();
    let before = fixture.snapshot();

    let refusal = |result: Result<Session>| {
        let err = result.expect_err("refused");
        err.downcast_ref::<CodedError>()
            .map_or_else(|| format!("{err:#}"), |coded| coded.code.to_string())
    };
    assert_eq!(
        refusal(fixture.handoff("missing").await),
        error_code::UNKNOWN_SESSION
    );
    assert_eq!(
        refusal(fixture.handoff("archived").await),
        error_code::SESSION_ARCHIVED
    );
    assert_eq!(
        refusal(fixture.handoff("working").await),
        error_code::SESSION_WORKING
    );
    assert_eq!(
        refusal(fixture.handoff("starting").await),
        error_code::SESSION_WORKING
    );
    assert_eq!(
        refusal(fixture.handoff("locked").await),
        error_code::SESSION_LOCKED
    );
    let same = fixture.handoff_to("idle", AgentId::CLAUDE).await;
    let same_text = format!("{:#}", same.as_ref().unwrap_err());
    assert_eq!(refusal(same), error_code::HANDOFF_TO_SAME_AGENT);
    assert!(same_text.contains("switch its account"), "{same_text}");

    fixture
        .state
        .set_agent_availability(vec![crate::protocol::AgentAvailability {
            agent: AgentId::CODEX,
            availability: Availability::Unavailable,
            default_account_dir: None,
        }]);
    assert_eq!(
        refusal(fixture.handoff("with-branch").await),
        error_code::AGENT_NOT_AVAILABLE
    );
    fixture
        .state
        .set_agent_availability(vec![crate::protocol::AgentAvailability {
            agent: AgentId::CODEX,
            availability: Availability::NotDetermined,
            default_account_dir: None,
        }]);

    // An account of another agent than the receiving one.
    let err = handoff_session_with(
        &fixture.state,
        Handoff {
            session: "with-branch".to_string(),
            agent: AgentId::CODEX,
            account: Some(Some("claude-account".to_string())),
            model: None,
            effort: None,
            title: None,
            note: None,
            asked_by: None,
            terminal_appearance: None,
        },
        fixture.launch(),
    )
    .await;
    assert_eq!(
        refusal(err.map(|moved| moved.receiving)),
        error_code::UNKNOWN_ACCOUNT
    );

    // A model the receiving agent does not offer.
    crate::test_support::offer_models(&fixture.state, AgentId::CODEX, &[("gpt-a", &["low"])]);
    for (model, effort) in [("gpt-z", None), ("gpt-a", Some("extreme"))] {
        let err = handoff_session_with(
            &fixture.state,
            Handoff {
                session: "with-branch".to_string(),
                agent: AgentId::CODEX,
                account: None,
                model: Some(Some(model.to_string())),
                effort: Some(effort.map(str::to_string)),
                title: None,
                note: None,
                asked_by: None,
                terminal_appearance: None,
            },
            fixture.launch(),
        )
        .await;
        assert_eq!(
            refusal(err.map(|moved| moved.receiving)),
            error_code::INVALID_LAUNCH_SETTING,
            "{model} {effort:?}"
        );
    }

    assert!(fixture.launches.lock().unwrap().is_empty());
    assert_eq!(fixture.snapshot(), before);
    drop(boss);
}

/// A handoff to an agent that is not added is refused with nothing changed.
#[tokio::test]
async fn a_handoff_to_an_agent_that_is_not_added_is_refused() {
    let fixture = Fixture::new("target-not-added");
    fixture.run(session("original", true, "idle", None, false));
    fixture
        .state
        .store
        .set_added_agents_if_undecided(&[AgentId::CLAUDE])
        .unwrap();
    let before = fixture.snapshot();

    let err = fixture
        .handoff_to("original", AgentId::CODEX)
        .await
        .expect_err("refused");
    let coded = err.downcast_ref::<CodedError>().expect("a coded error");
    assert_eq!(coded.code, error_code::AGENT_NOT_ADDED);
    assert_eq!(coded.params["agent"], AgentId::CODEX.label());
    assert!(fixture.launches.lock().unwrap().is_empty());
    assert_eq!(fixture.snapshot(), before);
}

/// An agent that cannot be launched leaves the original, its branches and its sessions as they were,
/// no receiving session and no package, and nobody is told.
#[tokio::test]
async fn a_start_that_fails_moves_nothing_and_removes_the_package() {
    let fixture = Fixture::new("start-fails");
    let original = fixture.run(session("original", true, "idle", None, false));
    let owned = fixture.run(session("owned", true, "idle", Some("original"), false));
    branch_made_by(&fixture.state, &fixture.repo, "one", "original");
    let before = fixture.snapshot();

    let err = handoff_session_with(
        &fixture.state,
        Handoff {
            session: "original".to_string(),
            agent: AgentId::CODEX,
            account: None,
            model: None,
            effort: None,
            title: None,
            note: None,
            asked_by: None,
            terminal_appearance: None,
        },
        |_| Err(anyhow!("the agent would not start")),
    )
    .await
    .expect_err("the start failed");

    assert!(format!("{err:#}").contains("would not start"), "{err:#}");
    assert_eq!(fixture.snapshot(), before);
    assert!(fixture.state.has_process("original") && fixture.state.has_process("owned"));
    for live in [&original, &owned] {
        let heard = heard_after_marker(&fixture.state, live);
        assert!(!heard.contains("Message from Octoboard"), "{heard}");
    }
}

/// A receiving session is told to follow the chain of packages back when the package names one.
#[test]
fn the_opening_prompt_tells_the_receiving_session_to_follow_earlier_packages() {
    let original = session("original", true, "idle", None, false);
    let task = takeover_prompt(&original, Path::new("/handoff/receiving.md"), None);
    assert!(
        task.starts_with(crate::messaging::texts::TAKEOVER_OPENING),
        "{task}"
    );
    assert!(
        task.contains("If the package names an earlier handoff package, read that one too"),
        "{task}"
    );
}

/// Deleting the receiving session removes its package.
#[tokio::test]
async fn deleting_the_receiving_session_removes_its_package() {
    let fixture = Fixture::new("package-removed");
    fixture.run(session("original", true, "idle", None, false));
    let receiving = fixture.handoff("original").await.unwrap();
    let package = fixture
        .state
        .handoff_dir()
        .join(format!("{}.md", receiving.id));
    assert!(package.is_file());

    crate::coordinator::lifecycle::archive_session(&fixture.state, &receiving.id).unwrap();
    assert!(fixture.state.delete_if_archived(&receiving.id).unwrap());

    assert!(!package.exists());
}

/// While the handoff is under way nothing is written to the original: a message sent meanwhile,
/// and one already in its outbox, reach the receiving session after what it took over, in order;
/// the original is not told.
#[tokio::test]
async fn messages_for_the_original_during_a_handoff_are_forwarded_to_the_receiving_session() {
    let fixture = Fixture::new("forwarded");
    let original = fixture.run(session("original", true, "idle", None, false));
    fixture
        .state
        .queue_message("original", "queued before the handoff");
    let state = fixture.state.clone();
    let during = fixture.launch_after(move || {
        let delivery = crate::messaging::delivery::write_message(
            &state,
            "original",
            "sent during the handoff",
            WhenBlocked::Queue,
        )
        .unwrap();
        assert_eq!(delivery, crate::messaging::delivery::Delivery::Queued);
    });

    let receiving = fixture
        .handoff_launching("original", AgentId::CODEX, during)
        .await
        .unwrap();

    let output = fixture.output_of(&receiving.id, "sent during the handoff");
    let held = output.find("what it held is yours now").unwrap();
    let note = output
        .find(
            "Any messages that were waiting for the original session follow this one, as they \
             were sent to it; their headers say who sent them.",
        )
        .expect("the prompt says the original's messages follow");
    let before = output.find("queued before the handoff").unwrap();
    let during = output.find("sent during the handoff").unwrap();
    assert!(held < note && note < before && before < during, "{output}");
    // The archived original has no process to take a marker; the receiving session's output above
    // is ordered after everything forwarded, and the original was written nothing before it ended.
    let heard = String::from_utf8_lossy(&original.recent_output(8 * 1024)).into_owned();
    assert!(
        !heard.contains("during the handoff") && !heard.contains("before the handoff"),
        "{heard}"
    );
}

/// Information kept for an interrupted original, which the archive that ends the handoff would
/// drop with the session, is forwarded to the receiving session and no longer kept.
#[tokio::test]
async fn information_kept_for_an_interrupted_original_goes_to_the_receiving_session() {
    let fixture = Fixture::new("kept-info");
    fixture
        .state
        .store
        .insert_session(&session("original", true, "interrupted", None, false))
        .unwrap();
    fixture
        .state
        .store
        .add_kept_message("original", "kept before the handoff")
        .unwrap();

    let receiving = fixture.handoff("original").await.unwrap();

    fixture.output_of(&receiving.id, "kept before the handoff");
    assert!(fixture
        .state
        .store
        .kept_messages("original")
        .unwrap()
        .is_empty());
    assert_eq!(fixture.stored("original").status, SessionStatus::Archived);
}

/// A kept message that a resume already put on the original's queue is forwarded once, in the
/// queue's order, and one that is only in the store follows it.
#[tokio::test]
async fn kept_information_that_is_also_queued_is_forwarded_once() {
    let fixture = Fixture::new("kept-and-queued");
    fixture.run(session("original", true, "idle", None, false));
    let queued = fixture
        .state
        .store
        .add_kept_message("original", "kept and queued")
        .unwrap();
    fixture
        .state
        .queue_kept_message("original", "kept and queued", queued);
    fixture
        .state
        .store
        .add_kept_message("original", "kept only")
        .unwrap();

    let receiving = fixture.handoff("original").await.unwrap();

    let output = fixture.output_of(&receiving.id, "kept only");
    assert_eq!(output.matches("kept and queued").count(), 1, "{output}");
    let bootstrap = output
        .find("what it held is yours now")
        .expect("bootstrap was delivered");
    let queued = output
        .find("kept and queued")
        .expect("queued information was delivered");
    let kept = output
        .find("kept only")
        .expect("kept information was delivered");
    assert!(bootstrap < queued && queued < kept, "{output}");
    assert!(fixture
        .state
        .store
        .kept_messages("original")
        .unwrap()
        .is_empty());
}

/// A handoff that fails before the receiving session has started releases the hold: what was
/// queued meanwhile is written to the original as usual.
#[tokio::test]
async fn messages_for_the_original_during_a_failed_handoff_reach_the_original() {
    let fixture = Fixture::new("failed-forward");
    let original = fixture.run(session("original", true, "idle", None, false));
    let state = fixture.state.clone();

    let err = handoff_session_with(
        &fixture.state,
        Handoff {
            session: "original".to_string(),
            agent: AgentId::CODEX,
            account: None,
            model: None,
            effort: None,
            title: None,
            note: None,
            asked_by: None,
            terminal_appearance: None,
        },
        move |_| {
            let delivery = crate::messaging::delivery::write_message(
                &state,
                "original",
                "sent during the handoff",
                WhenBlocked::Queue,
            )
            .unwrap();
            assert_eq!(delivery, crate::messaging::delivery::Delivery::Queued);
            Err(anyhow!("the agent would not start"))
        },
    )
    .await
    .expect_err("the start failed");

    assert!(format!("{err:#}").contains("would not start"), "{err:#}");
    let heard = output_containing(&original, "sent during the handoff");
    assert!(heard.contains("sent during the handoff"), "{heard}");
}

/// A failure after the place has moved leaves the receiving session running and its senders' sessions
/// bound to it; the queue held for the original goes to the receiving session, not back to it.
#[tokio::test]
async fn messages_held_for_the_original_go_to_the_receiving_session_when_the_handoff_fails_after_the_move(
) {
    let fixture = Fixture::new("failed-after-move");
    let original = fixture.run(session("original", true, "idle", None, false));
    fixture.run(session("owned", true, "idle", Some("original"), false));
    let state = fixture.state.clone();
    let during = fixture.launch_after(move || {
        // The user locks the original while the receiving session starts, so archiving it fails.
        state.store.set_session_locked("original", true).unwrap();
        crate::messaging::delivery::write_message(
            &state,
            "original",
            "sent during the handoff",
            WhenBlocked::Queue,
        )
        .unwrap();
    });

    let err = fixture
        .handoff_launching("original", AgentId::CODEX, during)
        .await
        .expect_err("archiving the original failed");

    assert!(
        format!("{err:#}").contains("archiving the original session failed"),
        "{err:#}"
    );
    let receiving = fixture.launches.lock().unwrap()[0].0.clone();
    assert_eq!(
        fixture.stored("owned").bound_to.as_deref(),
        Some(receiving.as_str())
    );
    assert_eq!(fixture.stored("original").status, SessionStatus::Idle);
    fixture.output_of(&receiving, "sent during the handoff");
    assert!(fixture.launches.lock().unwrap()[0].2.is_none());
    let started = fixture.started.lock().unwrap();
    let live = started.iter().find(|live| live.id == receiving).unwrap();
    let output = heard_after_marker(&fixture.state, live);
    assert!(
        !output.contains(crate::messaging::texts::TAKEOVER_OPENING),
        "{output}"
    );
    assert!(!output.contains("original is archived"), "{output}");
    let heard = heard_after_marker(&fixture.state, &original);
    assert!(!heard.contains("during the handoff"), "{heard}");
}

/// An owner that asked for a handoff and no longer owns the session by the time the operation
/// claims it (it was handed to another owner meanwhile) is refused, and nothing changes.
#[tokio::test]
async fn a_session_that_is_no_longer_the_askers_is_refused() {
    let fixture = Fixture::new("asker-lost-it");
    fixture.run(session("boss", true, "idle", None, false));
    fixture.run(session("other-boss", true, "idle", None, false));
    fixture.run(session("owned", true, "idle", Some("other-boss"), false));
    let before = fixture.snapshot();

    let err = handoff_session_with(
        &fixture.state,
        Handoff {
            session: "owned".to_string(),
            agent: AgentId::CODEX,
            account: None,
            model: None,
            effort: None,
            title: None,
            note: None,
            asked_by: Some("boss".to_string()),
            terminal_appearance: None,
        },
        fixture.launch(),
    )
    .await
    .expect_err("not the asker's any more");

    assert!(format!("{err:#}").contains("no longer yours"), "{err:#}");
    assert!(fixture.launches.lock().unwrap().is_empty());
    assert_eq!(fixture.snapshot(), before);
}

/// A stand-in process for the stored session `id` whose shell command leaves one process running
/// in a terminal session of its own, as an agent's dev server is; registered and watched as a
/// launched session's is. Answers that process's pid.
fn run_leaving_one(fixture: &Fixture, id: &str) -> (StandIn, i32) {
    let pid_file = fixture.dir.join(format!("{id}-child"));
    let live = crate::test_support::stand_in_starting(
        id,
        vec![crate::test_support::Background::sleeper(&pid_file)],
    );
    fixture.state.register_live(live.clone());
    crate::coordinator::exit_watch::watch_exit(&fixture.state, live.clone());
    let child = crate::test_support::wait_for_pid(&pid_file);
    (live, child)
}

/// Waits for `pid` to stop running, off the runtime, so the exit watchers that stop it run.
async fn stops_running(pid: i32) -> bool {
    crate::test_support::within(crate::test_support::PATIENCE, move || {
        !crate::test_support::is_running(pid)
    })
    .await
}

/// What the original's process started keeps running once the handoff's archive has ended that
/// process: the receiving session holds it, lists it, and stops it when it is archived in turn.
#[tokio::test(flavor = "multi_thread")]
async fn what_the_original_started_is_carried_over_to_the_receiving_session() {
    let fixture = Fixture::new("carry-over");
    fixture
        .state
        .store
        .insert_session(&session("original", true, "idle", None, false))
        .unwrap();
    let (_original, child) = run_leaving_one(&fixture, "original");
    let _guard = crate::test_support::KillOnDrop(vec![child]);

    let receiving = fixture.handoff("original").await.unwrap();
    let state = fixture.state.clone();
    assert!(
        crate::test_support::within(crate::test_support::PATIENCE, move || state
            .live_session("original")
            .is_none())
        .await,
        "the original's process was ended"
    );
    assert!(
        crate::test_support::is_running(child),
        "stopped with the original"
    );
    let trees = fixture.state.process_ledger.trees_of_sessions(
        std::slice::from_ref(&receiving.id),
        std::time::Duration::ZERO,
    );
    assert_eq!(trees.len(), 1, "{trees:?}");
    assert_eq!(trees[0].session_id.as_deref(), Some(receiving.id.as_str()));
    assert!(trees[0].id.starts_with(&format!("{child}:")));
    assert!(fixture
        .state
        .process_ledger
        .trees_of_sessions(&["original".to_string()], std::time::Duration::ZERO)
        .is_empty());

    crate::coordinator::lifecycle::archive_session(&fixture.state, &receiving.id).unwrap();
    assert!(
        stops_running(child).await,
        "kept running past the receiving session"
    );
}

/// A handoff whose receiving session does not start leaves the original running with what it
/// started, which is still stopped with it.
#[tokio::test(flavor = "multi_thread")]
async fn a_receiving_session_that_does_not_start_leaves_what_the_original_started_with_it() {
    let fixture = Fixture::new("carry-over-failed");
    fixture
        .state
        .store
        .insert_session(&session("original", true, "idle", None, false))
        .unwrap();
    let (original, child) = run_leaving_one(&fixture, "original");
    let _guard = crate::test_support::KillOnDrop(vec![child]);

    fixture
        .handoff_launching("original", AgentId::CODEX, |_| {
            Err(anyhow!("the agent could not be started"))
        })
        .await
        .expect_err("the receiving session did not start");
    assert!(crate::test_support::is_running(child));
    assert_eq!(fixture.stored("original").status, SessionStatus::Idle);

    crate::coordinator::lifecycle::archive_session(&fixture.state, &original.id).unwrap();
    assert!(stops_running(child).await, "kept running past the original");
}

/// A handoff whose archive of the original fails after what the original started was marked to be
/// carried over takes the mark back: the original stays running with it, and when it ends later
/// the processes are stopped with it, not carried to the receiving session left running.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_archive_takes_back_the_carry_over_of_what_the_original_started() {
    let fixture = Fixture::new("carry-over-archive-failed");
    fixture
        .state
        .store
        .insert_session(&session("original", true, "idle", None, false))
        .unwrap();
    let (original, child) = run_leaving_one(&fixture, "original");
    let _guard = crate::test_support::KillOnDrop(vec![child]);
    let state = fixture.state.clone();
    let during = fixture.launch_after(move || {
        // The user locks the original while the receiving session starts, so archiving it fails.
        state.store.set_session_locked("original", true).unwrap();
    });

    let err = fixture
        .handoff_launching("original", AgentId::CODEX, during)
        .await
        .expect_err("archiving the original failed");
    assert!(
        format!("{err:#}").contains("archiving the original session failed"),
        "{err:#}"
    );
    assert!(crate::test_support::is_running(child));

    fixture
        .state
        .store
        .set_session_locked("original", false)
        .unwrap();
    crate::coordinator::lifecycle::archive_session(&fixture.state, &original.id).unwrap();
    assert!(
        stops_running(child).await,
        "carried to the receiving session though the handoff failed"
    );
}

/// An archive of the original that fails after the work moved is answered with a
/// `handoff_archive_failed` that names the `receiving_session` and carries the archive's own
/// failure as `reason_code` and its params, so a client words the cause (a data folder it could
/// not write, say) and gives its remedy instead of reporting an internal error.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_archive_carries_the_failure_it_failed_with() {
    let fixture = Fixture::new("archive-failed-nests");
    fixture.run(session("original", true, "idle", None, false));
    let state = fixture.state.clone();
    let during = fixture.launch_after(move || {
        state.store.set_session_locked("original", true).unwrap();
    });

    let err = fixture
        .handoff_launching("original", AgentId::CODEX, during)
        .await
        .expect_err("archiving the original failed");
    let (code, params, message) = crate::protocol::error_parts(&err);
    assert_eq!(code, error_code::HANDOFF_ARCHIVE_FAILED);
    assert_eq!(params["reason_code"], "session_locked");
    assert_eq!(
        params["reason"],
        "this operation includes a locked session; unlock it first"
    );
    // The archive's own failure keeps naming the original; the receiving session rides apart.
    assert_eq!(params["session"], "original");
    assert_eq!(
        params["receiving_session"],
        fixture.launches.lock().unwrap()[0].0
    );
    assert!(
        message.contains("archiving the original session failed"),
        "{message}"
    );
}

/// The archive's failure to write Octoboard's database is nested with its cause, so the window
/// gives the data folder's remedy.
#[test]
fn a_failed_archive_keeps_the_cause_of_a_store_write_failure() {
    let inner = CodedError::raised(
        error_code::STORE_WRITE_FAILED,
        "Octoboard could not read or write its own data: disk full",
        &[("cause", "disk_full"), ("detail", "disk full")],
    );
    let (code, params, _) = crate::protocol::error_parts(&archive_failed("receiving-1", &inner));
    assert_eq!(code, error_code::HANDOFF_ARCHIVE_FAILED);
    assert_eq!(params["receiving_session"], "receiving-1");
    assert!(!params.contains_key("session"));
    assert_eq!(params["reason_code"], "store_write_failed");
    assert_eq!(params["cause"], "disk_full");
    assert_eq!(params["detail"], "disk full");
}

/// A failure with no code of its own is nested as the internal error it would be.
#[test]
fn a_failed_archive_of_an_uncoded_failure_nests_an_internal_error() {
    let (code, params, _) =
        crate::protocol::error_parts(&archive_failed("receiving-1", &anyhow!("disk exploded")));
    assert_eq!(code, error_code::HANDOFF_ARCHIVE_FAILED);
    assert_eq!(params["reason_code"], "internal_error");
    assert_eq!(params["reason"], "disk exploded");
}

/// A failure of the move of the place, once the receiving session is running, nests its cause
/// as a failed archive does, so a failure to write Octoboard's database gives the data folder's
/// remedy.
#[test]
fn a_failed_move_keeps_the_cause_of_a_store_write_failure() {
    let inner = CodedError::raised(
        error_code::STORE_WRITE_FAILED,
        "Octoboard could not read or write its own data: disk full",
        &[("cause", "disk_full"), ("detail", "disk full")],
    );
    let (code, params, message) = crate::protocol::error_parts(&move_failed("receiving-1", &inner));
    assert_eq!(code, error_code::HANDOFF_MOVE_FAILED);
    assert_eq!(params["receiving_session"], "receiving-1");
    assert!(!params.contains_key("session"));
    assert_eq!(params["reason_code"], "store_write_failed");
    assert_eq!(params["cause"], "disk_full");
    assert_eq!(params["detail"], "disk full");
    assert!(message.contains("is left running"), "{message}");
}

/// A move failure with no code of its own is nested as the internal error it would be.
#[test]
fn a_failed_move_of_an_uncoded_failure_nests_an_internal_error() {
    let (code, params, _) =
        crate::protocol::error_parts(&move_failed("receiving-1", &anyhow!("disk exploded")));
    assert_eq!(code, error_code::HANDOFF_MOVE_FAILED);
    assert_eq!(params["reason_code"], "internal_error");
    assert_eq!(params["reason"], "disk exploded");
}

/// A session the handoff moved to the receiving session without a process, archived or
/// interrupted, is left owed the notice of its new owner for its next launch.
#[tokio::test]
async fn a_moved_session_without_a_process_is_left_owed_the_notice_of_its_owner() {
    let (state, dir) = app_state("coordinator-handoff-owner-notice");
    state.store.insert_console(&console(&dir)).unwrap();
    let repo = repo_with(
        "coordinator-handoff-owner-notice-repo",
        &[(b"a.txt", b"one\n")],
    );
    insert_project(&state, &repo);
    let original = session("original", true, "idle", None, true);
    let receiving = Session {
        agent: AgentId::CODEX,
        ..session("receiving", true, "idle", None, true)
    };
    let asleep = session("asleep", true, "interrupted", Some("receiving"), false);
    let archived = session("archived", true, "archived", Some("receiving"), false);
    for stored in [&original, &receiving, &asleep, &archived] {
        state.store.insert_session(stored).unwrap();
    }
    let moved = Moved {
        receiving,
        owned: vec![asleep, archived],
        branches: Ok(Vec::new()),
    };

    tell(&state, &original, &moved, None).await;

    for id in ["asleep", "archived"] {
        assert_eq!(state.store.owner_notice_pending(id).unwrap(), 1, "{id}");
    }
}
