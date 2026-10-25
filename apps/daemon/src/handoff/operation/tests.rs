use std::sync::Mutex;

use serde_json::json;

use super::*;
use crate::branch_origin::{repo_key, repository_of};
use crate::mcp::Owner;
use crate::protocol::{Account, Availability, Role};
use crate::test_support::{
    app_state, branch_made_by, console, heard_after_marker, insert_project, output_containing,
    repo_with, running, stand_in_launch, Launched, ScratchDir, StandIn,
};

/// A stored session of console `console-1`: a project session of project `p1` when `project`, else
/// a console session. `bound_to` and `lead` are its binding.
fn session(id: &str, project: bool, status: &str, bound_to: Option<&str>, lead: bool) -> Session {
    let mut session: Session = serde_json::from_value(json!({
        "id": id, "agent": "claude", "agent_session_id": null, "console_id": "console-1",
        "project_id": project.then_some("p1"), "host_id": crate::store::LOCAL_HOST_ID,
        "role": if project { "project" } else { "console" }, "origin": "user",
        "title": format!("Session {id}"), "status": status, "has_conversation": true,
        "pinned": false, "started_at": 0, "bound_to": bound_to,
        "colour": (!project).then_some("pink"), "ordinal": (!project).then_some(1),
    }))
    .unwrap();
    session.lead = lead;
    session
}

/// What a handoff must leave alone when it is refused or fails: each session's status, owner and
/// lead mark, who is responsible for the test branches, and the packages on disk.
type Snapshot = (
    Vec<(String, SessionStatus, Option<String>, bool)>,
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
        let (state, dir) = app_state(&format!("handoff-operation-{name}"));
        let console = console(&dir);
        std::fs::create_dir_all(&console.workdir).unwrap();
        state.store.insert_console(&console).unwrap();
        let repo = repo_with(
            &format!("handoff-operation-{name}-repo"),
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
        stand_in_launch(&self.started, &self.launches, during)
    }

    async fn hand_off(&self, session: &str) -> Result<Session> {
        self.hand_off_to(session, Agent::Codex).await
    }

    async fn hand_off_to(&self, session: &str, agent: Agent) -> Result<Session> {
        self.hand_off_launching(session, agent, self.launch()).await
    }

    async fn hand_off_launching(
        &self,
        session: &str,
        agent: Agent,
        launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
    ) -> Result<Session> {
        hand_off_with(
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
            .map(|session| (session.id, session.status, session.bound_to, session.lead))
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

/// The receiving session runs with the package in its brief, in the original's place with the
/// original's role; the original is archived, and so is nothing it owned.
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
    branch_made_by(&fixture.state, &fixture.repo, "one", "original");
    branch_made_by(&fixture.state, &fixture.repo, "two", "original");
    branch_made_by(&fixture.state, &fixture.repo, "theirs", "someone-else");

    let receiving = fixture.hand_off("original").await.unwrap();

    let stored = fixture.stored(&receiving.id);
    assert_eq!(stored.agent, Agent::Codex);
    assert_eq!(stored.status, SessionStatus::Working);
    assert!(stored.pinned, "the original's pinned mark is carried over");
    assert_eq!(
        stored.title, "Fix the build｜Session original",
        "named as the original is shown, task title included"
    );
    assert!(fixture.state.has_process(&receiving.id));
    assert_eq!(
        (
            stored.role,
            stored.project_id.as_deref(),
            stored.bound_to.as_deref(),
            stored.lead
        ),
        (Role::Project, Some("p1"), None, false)
    );
    let launches = fixture.launches.lock().unwrap();
    let (_, owner, task) = &launches[0];
    assert_eq!(*owner, None);
    let task = task.as_deref().unwrap();
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

    let told = output_containing(&owned, "your owner has changed");
    assert!(told.contains(&receiving.id), "{told}");
    assert!(
        told.contains("replaces what you were told of your owner"),
        "{told}"
    );
    let held = fixture.output_of(&receiving.id, "what it held is yours now");
    for expected in ["`one`", "`two`", "`owned`", "`owned-archived`", "no owner"] {
        assert!(held.contains(expected), "lacks {expected}: {held}");
    }
    assert!(!held.contains("theirs"), "{held}");
    assert_eq!(fixture.stored("original").status, SessionStatus::Archived);
    drop(original);
}

/// A lead session keeps its binding to the console session as a lead session, takes the sessions it
/// started, and its owner is told.
#[tokio::test]
async fn a_lead_session_is_replaced_by_a_lead_session_of_the_same_console_session() {
    let fixture = Fixture::new("lead");
    let hub = fixture.run(session("hub", false, "idle", None, false));
    fixture.run(session("original", true, "waiting_user", Some("hub"), true));
    fixture.run(session("worker", true, "idle", Some("original"), false));
    branch_made_by(&fixture.state, &fixture.repo, "one", "original");

    let receiving = fixture.hand_off("original").await.unwrap();

    let stored = fixture.stored(&receiving.id);
    assert_eq!(stored.bound_to.as_deref(), Some("hub"));
    assert!(stored.lead && !stored.has_bound_role());
    assert_eq!(
        fixture.stored("worker").bound_to.as_deref(),
        Some(receiving.id.as_str())
    );
    assert!(!fixture.stored("worker").lead);
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
        held.contains("lead session of the console session `hub`"),
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

    let receiving = fixture.hand_off("original").await.unwrap();

    let stored = fixture.stored(&receiving.id);
    assert_eq!(
        stored.title, "Session original",
        "with no task title, the original's name alone"
    );
    assert_eq!(
        (stored.bound_to.as_deref(), stored.lead),
        (Some("boss"), false)
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

/// A console session has no branches and no project; the sessions it owns, its lead sessions
/// among them, are the ones that move.
#[tokio::test]
async fn a_console_session_hands_the_sessions_it_owns_to_the_receiving_console_session() {
    let fixture = Fixture::new("console");
    fixture.run(session("original", false, "idle", None, false));
    let lead = fixture.run(session("lead", true, "idle", Some("original"), true));
    fixture.run(session("bound", true, "idle", Some("original"), false));

    let receiving = fixture.hand_off("original").await.unwrap();

    let stored = fixture.stored(&receiving.id);
    assert_eq!(
        (
            stored.role,
            stored.project_id.as_deref(),
            stored.bound_to.as_deref()
        ),
        (Role::Console, None, None)
    );
    assert!(stored.colour.is_some() && stored.ordinal.is_some());
    for id in ["lead", "bound"] {
        assert_eq!(
            fixture.stored(id).bound_to.as_deref(),
            Some(receiving.id.as_str())
        );
    }
    assert!(fixture.stored("lead").lead);
    let told = output_containing(&lead, "your owner has changed");
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
            agent: Agent::Claude,
            name: "Work".to_string(),
            config_dir: fixture.dir.join("account").to_string_lossy().into_owned(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
        })
        .unwrap();
    let before = fixture.snapshot();

    let refusal = |result: Result<Session>| {
        let err = result.expect_err("refused");
        err.downcast_ref::<CodedError>()
            .map_or_else(|| format!("{err:#}"), |coded| coded.code.to_string())
    };
    assert_eq!(
        refusal(fixture.hand_off("missing").await),
        error_code::UNKNOWN_SESSION
    );
    assert_eq!(
        refusal(fixture.hand_off("archived").await),
        error_code::SESSION_ARCHIVED
    );
    assert_eq!(
        refusal(fixture.hand_off("working").await),
        error_code::SESSION_WORKING
    );
    assert_eq!(
        refusal(fixture.hand_off("starting").await),
        error_code::SESSION_WORKING
    );
    assert_eq!(
        refusal(fixture.hand_off("locked").await),
        error_code::SESSION_LOCKED
    );
    let same = fixture.hand_off_to("idle", Agent::Claude).await;
    let same_text = format!("{:#}", same.as_ref().unwrap_err());
    assert_eq!(refusal(same), error_code::HANDOFF_TO_SAME_AGENT);
    assert!(same_text.contains("switch its account"), "{same_text}");

    fixture
        .state
        .set_agent_availability(vec![crate::protocol::AgentAvailability {
            agent: Agent::Codex,
            availability: Availability::Unavailable,
            default_account_dir: None,
        }]);
    assert_eq!(
        refusal(fixture.hand_off("with-branch").await),
        error_code::AGENT_NOT_AVAILABLE
    );
    fixture
        .state
        .set_agent_availability(vec![crate::protocol::AgentAvailability {
            agent: Agent::Codex,
            availability: Availability::NotDetermined,
            default_account_dir: None,
        }]);

    // An account of another agent than the receiving one.
    let err = hand_off_with(
        &fixture.state,
        Handoff {
            session: "with-branch".to_string(),
            agent: Agent::Codex,
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
    crate::test_support::offer_models(&fixture.state, Agent::Codex, &[("gpt-a", &["low"])]);
    for (model, effort) in [("gpt-z", None), ("gpt-a", Some("extreme"))] {
        let err = hand_off_with(
            &fixture.state,
            Handoff {
                session: "with-branch".to_string(),
                agent: Agent::Codex,
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
        .set_added_agents_if_undecided(&[Agent::Claude])
        .unwrap();
    let before = fixture.snapshot();

    let err = fixture
        .hand_off_to("original", Agent::Codex)
        .await
        .expect_err("refused");
    let coded = err.downcast_ref::<CodedError>().expect("a coded error");
    assert_eq!(coded.code, error_code::AGENT_NOT_ADDED);
    assert_eq!(coded.params["agent"], Agent::Codex.label());
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

    let err = hand_off_with(
        &fixture.state,
        Handoff {
            session: "original".to_string(),
            agent: Agent::Codex,
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
    let task = brief(&original, Path::new("/handoff/receiving.md"), None);
    assert!(task.starts_with(TAKEOVER_OPENING), "{task}");
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
    let receiving = fixture.hand_off("original").await.unwrap();
    let package = fixture
        .state
        .handoff_dir()
        .join(format!("{}.md", receiving.id));
    assert!(package.is_file());

    crate::coordinator::archive_session(&fixture.state, &receiving.id).unwrap();
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
        let delivery = crate::reporting::write_message(
            &state,
            "original",
            "sent during the handoff",
            WhenBlocked::Queue,
        )
        .unwrap();
        assert_eq!(delivery, crate::reporting::Delivery::Queued);
    });

    let receiving = fixture
        .hand_off_launching("original", Agent::Codex, during)
        .await
        .unwrap();

    let output = fixture.output_of(&receiving.id, "sent during the handoff");
    let held = output.find("what it held is yours now").unwrap();
    let before = output.find("queued before the handoff").unwrap();
    let during = output.find("sent during the handoff").unwrap();
    assert!(held < before && before < during, "{output}");
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
        .add_pending_info("original", "kept before the handoff")
        .unwrap();

    let receiving = fixture.hand_off("original").await.unwrap();

    fixture.output_of(&receiving.id, "kept before the handoff");
    assert!(fixture
        .state
        .store
        .pending_info("original")
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
        .add_pending_info("original", "kept and queued")
        .unwrap();
    fixture
        .state
        .queue_kept_message("original", "kept and queued", queued);
    fixture
        .state
        .store
        .add_pending_info("original", "kept only")
        .unwrap();

    let receiving = fixture.hand_off("original").await.unwrap();

    let output = fixture.output_of(&receiving.id, "kept only");
    assert_eq!(output.matches("kept and queued").count(), 1, "{output}");
    assert!(
        output.find("kept and queued") < output.find("kept only"),
        "{output}"
    );
    assert!(fixture
        .state
        .store
        .pending_info("original")
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

    let err = hand_off_with(
        &fixture.state,
        Handoff {
            session: "original".to_string(),
            agent: Agent::Codex,
            account: None,
            model: None,
            effort: None,
            title: None,
            note: None,
            asked_by: None,
            terminal_appearance: None,
        },
        move |_| {
            let delivery = crate::reporting::write_message(
                &state,
                "original",
                "sent during the handoff",
                WhenBlocked::Queue,
            )
            .unwrap();
            assert_eq!(delivery, crate::reporting::Delivery::Queued);
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
        crate::reporting::write_message(
            &state,
            "original",
            "sent during the handoff",
            WhenBlocked::Queue,
        )
        .unwrap();
    });

    let err = fixture
        .hand_off_launching("original", Agent::Codex, during)
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

    let err = hand_off_with(
        &fixture.state,
        Handoff {
            session: "owned".to_string(),
            agent: Agent::Codex,
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
