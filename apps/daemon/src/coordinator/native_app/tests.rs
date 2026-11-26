use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use super::*;
use crate::coordinator::handle;
use crate::protocol::{Event, RequestBody, SessionStatus};
use crate::test_support::{app_state, bare_session, code_of, idle_stand_in, ScratchDir};

const CONVERSATION: &str = "0b8f1c52-3d1e-4a7b-9c41-5e6d7f8091a2";

/// A Claude session on `config_dir` (the default account when `None`) with a conversation.
fn claude_session(config_dir: Option<&Path>) -> Session {
    Session {
        has_conversation: true,
        agent_session_id: Some(CONVERSATION.to_string()),
        config_dir: config_dir.map(|dir| dir.to_string_lossy().into_owned()),
        ..bare_session("s", crate::protocol::SessionKind::Console)
    }
}

/// A locator whose default Claude account is `default_root`.
fn locator_for(default_root: &Path) -> Locator<'static> {
    Locator::with_shell(HashMap::from([(
        "CLAUDE_CONFIG_DIR".to_string(),
        default_root.to_string_lossy().into_owned(),
    )]))
}

/// Writes the conversation's transcript under account `root` with `len` bytes, last written
/// `age` ago.
fn record_in(root: &Path, len: usize, age: Duration) -> PathBuf {
    let dir = root.join("projects").join("project");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{CONVERSATION}.jsonl"));
    std::fs::write(&path, vec![b'x'; len]).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(SystemTime::now() - age)
        .unwrap();
    path
}

struct Accounts {
    _dir: ScratchDir,
    default: PathBuf,
    own: PathBuf,
}

impl Accounts {
    fn new(name: &str) -> Self {
        let dir = ScratchDir::new(&format!("native-app-{name}"));
        let default = dir.join("default");
        let own = dir.join("own");
        std::fs::create_dir_all(&default).unwrap();
        std::fs::create_dir_all(&own).unwrap();
        Self {
            _dir: dir,
            default,
            own,
        }
    }

    /// Whether the applications can see `session`'s conversation, with the default account here.
    fn seen(&self, session: &Session) -> bool {
        default_directory_holds_latest(&mut locator_for(&self.default), session)
    }
}

const NOW: Duration = Duration::from_secs(0);

/// The conversation is seen only where the default directory holds a record of it.
#[test]
fn a_conversation_the_default_directory_has_no_record_of_is_not_seen() {
    let accounts = Accounts::new("no-default-record");
    record_in(&accounts.own, 10, NOW);

    assert!(!accounts.seen(&claude_session(Some(&accounts.own))));
    assert!(!accounts.seen(&claude_session(None)));
}

/// A session with no conversation, or none the agent can resume, has nothing to be seen, whatever
/// the default directory holds.
#[test]
fn a_session_without_a_conversation_is_not_seen() {
    let accounts = Accounts::new("no-conversation");
    record_in(&accounts.default, 10, NOW);

    let mut session = claude_session(None);
    session.has_conversation = false;
    assert!(!accounts.seen(&session));
    let mut session = claude_session(None);
    session.agent_session_id = None;
    assert!(!accounts.seen(&session));
}

/// A session of the default account is seen once the default directory holds its record, and so is
/// one of another account whose own directory holds no record of its own.
#[test]
fn a_record_in_the_default_directory_is_seen_when_no_other_directory_holds_one() {
    let accounts = Accounts::new("default-only");
    record_in(&accounts.default, 10, NOW);

    assert!(accounts.seen(&claude_session(None)));
    assert!(accounts.seen(&claude_session(Some(&accounts.own))));
}

/// With a record in the account's own directory too, the default directory must hold a copy that is
/// neither shorter nor older: a shorter or an older one lags the conversation the agent kept.
#[test]
fn the_default_copy_must_be_neither_shorter_nor_older_than_the_accounts() {
    // The account's own copy is 10 bytes, last written a minute ago.
    for (default_len, default_age, seen, why) in [
        (10, Duration::from_secs(10), true, "as long and newer"),
        (20, Duration::from_secs(10), true, "longer and newer"),
        (5, Duration::from_secs(10), false, "shorter"),
        (20, Duration::from_secs(120), false, "longer but older"),
        (5, Duration::from_secs(120), false, "shorter and older"),
    ] {
        let accounts = Accounts::new("copies");
        record_in(&accounts.default, default_len, default_age);
        record_in(&accounts.own, 10, Duration::from_secs(60));
        assert_eq!(
            accounts.seen(&claude_session(Some(&accounts.own))),
            seen,
            "{why}"
        );
    }
}

/// The two paths that are one file, through a link, are no lag at all.
#[cfg(unix)]
#[test]
fn an_account_whose_directory_is_the_default_one_is_seen() {
    let accounts = Accounts::new("linked");
    record_in(&accounts.default, 10, NOW);
    let linked = accounts.own.with_file_name("linked");
    std::os::unix::fs::symlink(&accounts.default, &linked).unwrap();

    assert!(accounts.seen(&claude_session(Some(&linked))));
}

/// Opening is refused for an unknown session.
#[tokio::test]
async fn opening_an_unknown_session_in_its_app_is_refused() {
    let (state, _dir) = app_state("native-app-unknown");
    let err = handle(
        &state,
        None,
        RequestBody::OpenSessionInApp {
            session: "missing".to_string(),
            keep_process_ids: Vec::new(),
        },
    )
    .await
    .expect_err("refused");
    assert_eq!(code_of(&err), error_code::UNKNOWN_SESSION);
}

type Asker = Box<
    dyn FnOnce(&dyn NativeApp, &Session, &str, &Path, &Path, &HashMap<String, String>) -> Result<()>
        + Send,
>;

/// A fake host for [`open_with`]: the application is installed, and asking it only notes that it was.
fn asked_app() -> (Arc<AtomicBool>, Asker) {
    let asked = Arc::new(AtomicBool::new(false));
    let noted = Arc::clone(&asked);
    (
        asked,
        Box::new(move |_, _, _, _, _, _| {
            noted.store(true, Ordering::SeqCst);
            Ok(())
        }),
    )
}

fn shell() -> Result<HashMap<String, String>> {
    Ok(HashMap::new())
}

fn installed(_: &str) -> bool {
    true
}

fn not_installed(_: &str) -> bool {
    false
}

/// A Claude console session `id` with a conversation and the status `status`, and, for one with a
/// process, a live stand-in that the exit watcher watches.
fn with_process(
    state: &Arc<AppState>,
    dir: &Path,
    status: SessionStatus,
) -> crate::test_support::StandIn {
    state
        .store
        .insert_console(&crate::test_support::console(dir))
        .unwrap();
    state
        .store
        .insert_session(&Session {
            status,
            ..claude_session(None)
        })
        .unwrap();
    let live = idle_stand_in("s");
    state.register_live(live.clone());
    crate::coordinator::exit_watch::watch_exit(state, live.clone());
    live
}

/// A session at rest with a process (waiting for instructions) is archived, which ends its process,
/// and then its conversation is opened.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_at_rest_is_archived_and_then_opened() {
    let (state, dir) = app_state("native-app-at-rest");
    let live = with_process(&state, &dir, SessionStatus::Idle);
    let (asked, ask) = asked_app();

    open_with(&state, "s", Vec::new(), installed, shell, ask)
        .await
        .expect("opened");

    assert!(asked.load(Ordering::SeqCst));
    assert!(live.poll_exit(), "its process was ended");
    assert!(state.live_session("s").is_none());
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Archived
    );
    drop(state.begin_launch("s").expect("the claim is released"));
}

/// An interrupted session is archived and then opened, so a resume cannot start it again; an
/// archived one is opened as it is, with its archive time left alone.
#[tokio::test]
async fn an_interrupted_session_is_archived_and_an_archived_one_is_opened_as_it_is() {
    let (state, dir) = app_state("native-app-no-process");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    for status in [SessionStatus::Interrupted, SessionStatus::Archived] {
        let id = format!("{status:?}");
        state
            .store
            .insert_session(&Session {
                id: id.clone(),
                status,
                ended_at: (status == SessionStatus::Archived).then_some(5),
                ..claude_session(None)
            })
            .unwrap();
        let (asked, ask) = asked_app();
        open_with(
            &state,
            &id,
            vec!["ignored".to_string()],
            installed,
            shell,
            ask,
        )
        .await
        .expect("opened");
        assert!(asked.load(Ordering::SeqCst), "{status:?}");
        let after = state.session_record(&id).unwrap();
        assert_eq!(after.status, SessionStatus::Archived, "{status:?}");
        if status == SessionStatus::Archived {
            assert_eq!(after.ended_at, Some(5));
        }
    }
}

/// A session at work is refused with `session_working`, whatever it is doing, and is left running,
/// unarchived and unopened.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_at_work_is_not_opened_and_keeps_running() {
    for status in [
        SessionStatus::Starting,
        SessionStatus::Working,
        SessionStatus::WaitingUser,
        SessionStatus::Dormant,
    ] {
        let (state, dir) = app_state("native-app-at-work");
        let live = with_process(&state, &dir, status);
        let (asked, ask) = asked_app();

        let err = open_with(&state, "s", Vec::new(), installed, shell, ask)
            .await
            .expect_err("refused");

        assert_eq!(code_of(&err), error_code::SESSION_WORKING, "{status:?}");
        assert!(
            err.to_string()
                .contains("waiting for instructions or interrupted"),
            "{status:?}: {err}"
        );
        assert!(!asked.load(Ordering::SeqCst), "{status:?}");
        assert!(!live.poll_exit(), "{status:?}: its process was ended");
        assert_eq!(state.session_record("s").unwrap().status, status);
    }
}

/// A refusal for what does not depend on archiving (no conversation, an application that is not
/// installed, an agent without one) leaves a session at rest running and unarchived.
#[tokio::test(flavor = "multi_thread")]
async fn a_refusal_that_does_not_depend_on_the_process_leaves_it_running() {
    for case in ["not-installed", "no-conversation", "bad-id", "no-app"] {
        let (state, dir) = app_state("native-app-precondition");
        let live = with_process(&state, &dir, SessionStatus::Idle);
        let mut session = state.session_record("s").unwrap();
        match case {
            "no-conversation" => session.has_conversation = false,
            "bad-id" => session.agent_session_id = Some("not a conversation id".to_string()),
            "no-app" => session.agent = AgentId::GROK,
            _ => {}
        }
        state.store.update_session(&session).unwrap();
        let (asked, ask) = asked_app();

        let err = open_with(&state, "s", Vec::new(), not_installed, shell, ask)
            .await
            .expect_err("refused");

        let why = match case {
            "not-installed" => Some("application is not installed"),
            "no-conversation" => Some("this session has no conversation yet"),
            "bad-id" => Some("is not a conversation id"),
            _ => None,
        };
        match why {
            Some(why) => {
                assert_eq!(code_of(&err), error_code::NATIVE_APP_UNAVAILABLE, "{case}");
                assert!(
                    crate::test_support::param_of(&err, "detail").contains(why),
                    "{case}: {err}"
                );
            }
            None => assert_refused_as_no_app(&err, case),
        }
        assert!(!asked.load(Ordering::SeqCst), "{case}");
        assert!(!live.poll_exit(), "{case}: its process was ended");
        assert_eq!(
            state.session_record("s").unwrap().status,
            SessionStatus::Idle,
            "{case}"
        );
    }
}

/// The id of the tree of session `session` whose top is process `child`, once the ledger lists it.
#[cfg(unix)]
async fn listed_tree_of(state: &Arc<AppState>, session: &str, child: i32) -> String {
    let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
    loop {
        let trees = crate::coordinator::processes::list(
            state,
            crate::protocol::ProcessTarget::Session {
                session_id: session.to_string(),
            },
        )
        .await
        .unwrap();
        if let Some(tree) = trees
            .into_iter()
            .find(|tree| tree.id.starts_with(&format!("{child}:")))
        {
            return tree.id;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the tree never showed"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The trees the user chose to keep survive the archive's end of the session's process, which stops
/// the rest of what it started.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn the_trees_the_user_chose_to_keep_outlive_the_archive_an_open_makes() {
    // A project session: a console session keeps no trees.
    let (state, dir) = crate::test_support::console_with_project_and_accounts("native-app-keep");
    state
        .store
        .insert_session(&Session {
            has_conversation: true,
            agent_session_id: Some(CONVERSATION.to_string()),
            ..crate::test_support::project_session("s", AgentId::CLAUDE, SessionStatus::Idle)
        })
        .unwrap();
    let (live, child) = crate::test_support::running_leaving_one(&state, &dir, "s");
    let _guard = crate::test_support::KillOnDrop(vec![child]);
    let tree = listed_tree_of(&state, "s", child).await;
    let (asked, ask) = asked_app();

    open_with(&state, "s", vec![tree], installed, shell, ask)
        .await
        .expect("opened");

    assert!(asked.load(Ordering::SeqCst));
    assert!(live.poll_exit());
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Archived
    );
    assert!(
        crate::test_support::is_running(child),
        "the kept tree was stopped"
    );
}

/// A project session `id` with a conversation, in `status`, of the project
/// `console_with_project_and_accounts` stores.
fn project_session_with_conversation(id: &str, status: SessionStatus) -> Session {
    Session {
        has_conversation: true,
        agent_session_id: Some(CONVERSATION.to_string()),
        ..crate::test_support::project_session(id, AgentId::CLAUDE, status)
    }
}

/// Of the trees the session's process started, only the ones the user chose to keep outlive it: the
/// others are stopped with the process.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn the_trees_that_were_not_chosen_are_stopped_with_the_process() {
    let (state, dir) =
        crate::test_support::console_with_project_and_accounts("native-app-stop-rest");
    state
        .store
        .insert_session(&project_session_with_conversation("s", SessionStatus::Idle))
        .unwrap();
    let files = [dir.join("one"), dir.join("two")];
    let live = crate::test_support::stand_in_starting(
        "s",
        files
            .iter()
            .map(|file| crate::test_support::Background::sleeper(file))
            .collect(),
    );
    state.register_live(live.clone());
    crate::coordinator::exit_watch::watch_exit(&state, live.clone());
    let pids: Vec<i32> = files
        .iter()
        .map(|file| crate::test_support::wait_for_pid(file))
        .collect();
    let _guard = crate::test_support::KillOnDrop(pids.clone());
    let chosen = listed_tree_of(&state, "s", pids[0]).await;
    listed_tree_of(&state, "s", pids[1]).await;
    let (asked, ask) = asked_app();

    open_with(&state, "s", vec![chosen], installed, shell, ask)
        .await
        .expect("opened");

    assert!(asked.load(Ordering::SeqCst));
    assert!(live.poll_exit());
    crate::test_support::assert_stops_running(pids[1]);
    assert!(
        crate::test_support::is_running(pids[0]),
        "the chosen tree was stopped"
    );
}

/// An archived session is opened as it is: the trees named to keep are not taken, so the tree of
/// another session that the request names stays what it was.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn the_trees_named_for_an_archived_session_are_not_kept() {
    let (state, dir) =
        crate::test_support::console_with_project_and_accounts("native-app-archived-keep");
    for (id, status) in [
        ("s", SessionStatus::Archived),
        ("other", SessionStatus::Idle),
    ] {
        state
            .store
            .insert_session(&project_session_with_conversation(id, status))
            .unwrap();
    }
    let (_live, child) = crate::test_support::running_leaving_one(&state, &dir, "other");
    let _guard = crate::test_support::KillOnDrop(vec![child]);
    let tree = listed_tree_of(&state, "other", child).await;
    let (asked, ask) = asked_app();

    open_with(&state, "s", vec![tree.clone()], installed, shell, ask)
        .await
        .expect("opened");

    assert!(asked.load(Ordering::SeqCst));
    let trees = crate::coordinator::processes::list(
        &state,
        crate::protocol::ProcessTarget::Session {
            session_id: "other".to_string(),
        },
    )
    .await
    .unwrap();
    let still = trees
        .iter()
        .find(|listed| listed.id == tree)
        .expect("the tree is still its session's");
    assert!(!still.kept);
}

/// A session stored idle `id` of `agent` with a stand-in process whose screen shows `screen`, which
/// the stand-in itself draws nothing of, and that the exit watcher watches.
fn with_process_showing(
    state: &Arc<AppState>,
    dir: &Path,
    agent: AgentId,
    screen: &str,
) -> crate::test_support::StandIn {
    state
        .store
        .insert_console(&crate::test_support::console(dir))
        .unwrap();
    state
        .store
        .insert_session(&Session {
            status: SessionStatus::Idle,
            agent,
            ..claude_session(None)
        })
        .unwrap();
    let live = crate::test_support::idle_stand_in_of("s", agent);
    state.register_live(live.clone());
    live.on_output(screen.as_bytes());
    assert!(
        live.look_for_dialog().current.is_some(),
        "the screen shows a dialog"
    );
    crate::coordinator::exit_watch::watch_exit(state, live.clone());
    live
}

fn running_in_the_background() -> crate::hooks::BackgroundTasksChange {
    crate::hooks::BackgroundTasksChange::Replace(vec![crate::protocol::BackgroundTask {
        id: "dev-server".to_string(),
        kind: Some("shell".to_string()),
        description: None,
    }])
}

/// An idle session whose agent lists work still running in the background is at work and is
/// refused, left running; once that work is gone it is at rest and is opened.
#[tokio::test(flavor = "multi_thread")]
async fn an_idle_session_running_work_in_the_background_is_not_opened_until_it_is_done() {
    let (state, dir) = app_state("native-app-background");
    let live = with_process(&state, &dir, SessionStatus::Idle);
    state.apply_background_tasks_change("s", running_in_the_background());
    let (asked, ask) = asked_app();

    let err = open_with(&state, "s", Vec::new(), installed, shell, ask)
        .await
        .expect_err("refused");

    assert_eq!(code_of(&err), error_code::SESSION_WORKING);
    assert!(
        err.to_string().contains("running work in the background"),
        "{err}"
    );
    assert_eq!(crate::test_support::param_of(&err, "session"), "s");
    assert!(!asked.load(Ordering::SeqCst));
    assert!(!live.poll_exit(), "its process was ended");
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Idle
    );

    state.apply_background_tasks_change(
        "s",
        crate::hooks::BackgroundTasksChange::Replace(Vec::new()),
    );
    let (asked, ask) = asked_app();
    open_with(&state, "s", Vec::new(), installed, shell, ask)
        .await
        .expect("opened");
    assert!(asked.load(Ordering::SeqCst));
    assert!(live.poll_exit());
}

/// A session showing a view of its background agents is dormant, which is at work, whatever its
/// stored status says.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_showing_its_background_agents_is_not_opened() {
    let (state, dir) = app_state("native-app-dormant");
    let live = with_process_showing(
        &state,
        &dir,
        AgentId::CLAUDE,
        crate::test_support::SUBAGENT_VIEW,
    );
    let (asked, ask) = asked_app();

    let err = open_with(&state, "s", Vec::new(), installed, shell, ask)
        .await
        .expect_err("refused");

    assert_eq!(code_of(&err), error_code::SESSION_WORKING);
    assert!(
        err.to_string().contains("showing its background agents"),
        "{err}"
    );
    assert!(!asked.load(Ordering::SeqCst));
    assert!(!live.poll_exit());
}

/// A session whose turn stopped on its account's usage limit reads as waiting for the user, but is
/// at rest as for a handoff or a switch: it is archived and opened. Work it still has
/// running in the background keeps it at work.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_waiting_only_on_a_usage_limit_dialog_is_at_rest() {
    let (state, dir) = app_state("native-app-usage-limit");
    let live = with_process_showing(
        &state,
        &dir,
        AgentId::CODEX,
        crate::test_support::screens::CODEX_CREDIT_LIMIT,
    );
    assert_eq!(
        state
            .wire_session(state.session_record("s").unwrap())
            .status,
        SessionStatus::WaitingUser,
        "it is shown waiting for the user"
    );

    state.apply_background_tasks_change("s", running_in_the_background());
    let (asked, ask) = asked_app();
    let err = open_with(&state, "s", Vec::new(), installed, shell, ask)
        .await
        .expect_err("refused");
    assert_eq!(code_of(&err), error_code::SESSION_WORKING);
    assert!(
        err.to_string().contains("running work in the background"),
        "{err}"
    );
    assert!(!asked.load(Ordering::SeqCst));
    assert!(!live.poll_exit());

    state.apply_background_tasks_change(
        "s",
        crate::hooks::BackgroundTasksChange::Replace(Vec::new()),
    );
    let (asked, ask) = asked_app();
    open_with(&state, "s", Vec::new(), installed, shell, ask)
        .await
        .expect("opened");
    assert!(asked.load(Ordering::SeqCst));
    assert!(live.poll_exit(), "its process was ended");
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Archived
    );
}

/// A tree the user chose to keep that cannot be kept (it ends with its session whatever keeps it)
/// refuses the open before anything is ended: the session is left running and unopened.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_tree_that_cannot_be_kept_refuses_the_open_and_leaves_the_session_running() {
    struct Shared(crate::platform::proc::ProcessIdentity);
    impl Drop for Shared {
        fn drop(&mut self) {
            crate::processes::ledger::AGENT_CONSOLE_OVERRIDE
                .lock()
                .unwrap()
                .retain(|shared| *shared != self.0);
        }
    }

    let (state, dir) = crate::test_support::console_with_project_and_accounts("native-app-unkeep");
    state
        .store
        .insert_session(&Session {
            has_conversation: true,
            agent_session_id: Some(CONVERSATION.to_string()),
            ..crate::test_support::project_session("s", AgentId::CLAUDE, SessionStatus::Idle)
        })
        .unwrap();
    let (live, child) = crate::test_support::running_leaving_one(&state, &dir, "s");
    let _guard = crate::test_support::KillOnDrop(vec![child]);
    let tree = listed_tree_of(&state, "s", child).await;
    let identity = crate::platform::proc::ProcessIdentity::parse(&tree).unwrap();
    crate::processes::ledger::AGENT_CONSOLE_OVERRIDE
        .lock()
        .unwrap()
        .push(identity);
    let _shared = Shared(identity);
    let (asked, ask) = asked_app();

    let err = open_with(&state, "s", vec![tree], installed, shell, ask)
        .await
        .expect_err("refused");

    assert_eq!(code_of(&err), error_code::PROCESS_SHARES_AGENT_CONSOLE);
    assert!(
        err.to_string().contains("runs in the agent's own terminal"),
        "{err}"
    );
    assert!(!asked.load(Ordering::SeqCst));
    assert!(!live.poll_exit(), "its process was ended");
    assert!(state.live_session("s").is_some());
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Idle
    );
    assert!(crate::test_support::is_running(child));
    drop(state.begin_switch("s").expect("the claim is released"));
}

/// An application that fails to open after the archive leaves the session archived, with the
/// failure told and the claim let go.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_open_after_the_archive_leaves_the_session_archived() {
    let (state, dir) = app_state("native-app-open-fails");
    let live = with_process(&state, &dir, SessionStatus::Idle);
    let ask: Asker =
        Box::new(|_, _, _, _, _, _| Err(unavailable("the application would not start")));

    let err = open_with(&state, "s", Vec::new(), installed, shell, ask)
        .await
        .expect_err("failed");

    assert_eq!(code_of(&err), error_code::NATIVE_APP_UNAVAILABLE);
    assert!(live.poll_exit(), "its process was ended");
    assert!(state.live_session("s").is_none());
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Archived
    );
    drop(state.begin_launch("s").expect("the claim is released"));
}

/// The request reaches the open with its trees to keep: asked through the coordinator for a session
/// at work it is refused, with the process left running.
#[tokio::test(flavor = "multi_thread")]
async fn the_request_is_refused_for_a_session_at_work_with_its_process_running() {
    let (state, dir) = app_state("native-app-request");
    let live = with_process(&state, &dir, SessionStatus::Working);

    let err = handle(
        &state,
        None,
        RequestBody::OpenSessionInApp {
            session: "s".to_string(),
            keep_process_ids: vec!["1:1".to_string()],
        },
    )
    .await
    .expect_err("refused");

    assert_eq!(code_of(&err), error_code::SESSION_WORKING);
    assert!(
        err.to_string()
            .contains("waiting for instructions or interrupted"),
        "{err}"
    );
    assert!(!live.poll_exit());
}

/// A session whose status says it has no process but whose process is still registered is at
/// rest: it is archived, which ends its process, and opened.
#[tokio::test(flavor = "multi_thread")]
async fn an_interrupted_session_whose_process_is_still_up_is_archived_and_opened() {
    let (state, dir) = app_state("native-app-live");
    let live = with_process(&state, &dir, SessionStatus::Interrupted);
    let (asked, ask) = asked_app();

    open_with(&state, "s", Vec::new(), installed, shell, ask)
        .await
        .expect("opened");

    assert!(asked.load(Ordering::SeqCst));
    assert!(live.poll_exit());
    assert!(state.live_session("s").is_none());
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Archived
    );
}

/// What is bound to the session is archived with it, running sessions included and ended with
/// their processes; a session bound to another owner is left alone.
#[tokio::test(flavor = "multi_thread")]
async fn what_is_bound_to_the_session_is_archived_with_it() {
    let (state, dir) = app_state("native-app-descendants");
    let live = with_process(&state, &dir, SessionStatus::Idle);
    for (id, status, owner) in [
        ("worker", SessionStatus::Working, "s"),
        ("old", SessionStatus::Interrupted, "s"),
        ("elsewhere", SessionStatus::Working, "other"),
    ] {
        state
            .store
            .insert_session(&Session {
                id: id.to_string(),
                status,
                bound_to: Some(owner.to_string()),
                ..claude_session(None)
            })
            .unwrap();
    }
    let worker = idle_stand_in("worker");
    state.register_live(worker.clone());
    let elsewhere = idle_stand_in("elsewhere");
    state.register_live(elsewhere.clone());
    let (asked, ask) = asked_app();

    open_with(&state, "s", Vec::new(), installed, shell, ask)
        .await
        .expect("opened");

    assert!(asked.load(Ordering::SeqCst));
    assert!(live.poll_exit());
    for id in ["s", "worker", "old"] {
        assert_eq!(
            state.session_record(id).unwrap().status,
            SessionStatus::Archived,
            "{id}"
        );
    }
    crate::coordinator::test_support::wait_until(|| worker.poll_exit()).await;
    assert_eq!(
        state.session_record("elsewhere").unwrap().status,
        SessionStatus::Working
    );
    assert!(!elsewhere.poll_exit());
}

/// A locked session, or a locked session bound to it, refuses the open as an archive is refused,
/// with nothing archived or ended and the application not asked.
#[tokio::test(flavor = "multi_thread")]
async fn a_locked_session_or_descendant_refuses_the_open_and_nothing_changes() {
    for locked in ["s", "worker"] {
        let (state, dir) = app_state("native-app-locked");
        let live = with_process(&state, &dir, SessionStatus::Idle);
        state
            .store
            .insert_session(&Session {
                id: "worker".to_string(),
                status: SessionStatus::Interrupted,
                bound_to: Some("s".to_string()),
                ..claude_session(None)
            })
            .unwrap();
        state.store.set_session_locked(locked, true).unwrap();
        let (asked, ask) = asked_app();

        let err = open_with(&state, "s", Vec::new(), installed, shell, ask)
            .await
            .expect_err("refused");

        assert_eq!(code_of(&err), error_code::SESSION_LOCKED, "{locked}");
        assert!(
            err.to_string().contains("includes a locked session"),
            "{locked}: {err}"
        );
        assert_eq!(crate::test_support::param_of(&err, "session"), locked);
        assert!(!asked.load(Ordering::SeqCst), "{locked}");
        assert!(!live.poll_exit(), "{locked}");
        for id in ["s", "worker"] {
            assert_ne!(
                state.session_record(id).unwrap().status,
                SessionStatus::Archived,
                "{locked}: {id}"
            );
        }
    }
}

/// What an open is refused with for a provider that declares no desktop application.
fn assert_refused_as_no_app(err: &anyhow::Error, case: &str) {
    assert_eq!(code_of(err), error_code::PROVIDER_UNSUPPORTED, "{case}");
    assert_eq!(
        crate::test_support::param_of(err, "operation"),
        "native_app",
        "{case}"
    );
}

/// An agent whose provider declares no desktop application cannot be opened in one
/// (`provider_unsupported`), and nor can a session that has no conversation to show yet
/// (`native_app_unavailable`, naming a `detail`).
#[tokio::test]
async fn a_session_with_nothing_to_open_in_an_app_is_refused_as_unavailable() {
    let (state, dir) = app_state("native-app-unavailable");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    for (id, agent, status, conversation) in [
        ("grok", AgentId::GROK, SessionStatus::Interrupted, true),
        (
            "archived-grok",
            AgentId::GROK,
            SessionStatus::Archived,
            true,
        ),
        ("fresh", AgentId::CLAUDE, SessionStatus::Interrupted, false),
        (
            "fresh-codex",
            AgentId::CODEX,
            SessionStatus::Archived,
            false,
        ),
    ] {
        let session = Session {
            id: id.to_string(),
            agent,
            status,
            has_conversation: conversation,
            ..claude_session(None)
        };
        state.store.insert_session(&session).unwrap();
        let err = open_session_in_app(&state, id, Vec::new())
            .await
            .expect_err(id);
        if agent == AgentId::GROK {
            assert_refused_as_no_app(&err, id);
            continue;
        }
        assert_eq!(code_of(&err), error_code::NATIVE_APP_UNAVAILABLE, "{id}");
        assert!(
            crate::test_support::param_of(&err, "detail")
                .contains("this session has no conversation yet"),
            "{id}: {err}"
        );
    }
}

/// `probe_native_apps` echoes the request id, and names only agents that declare a desktop
/// application, each once; with no sessions asked about, none is reported seen.
#[tokio::test]
async fn probing_for_native_apps_names_only_agents_that_have_one() {
    let (state, _dir) = app_state("native-app-probe");
    let reply = handle(
        &state,
        Some("probe-1".to_string()),
        RequestBody::ProbeNativeApps {
            sessions: Vec::new(),
        },
    )
    .await
    .expect("answered");
    let Some(Event::NativeApps {
        id,
        agents,
        sessions_seen,
        ..
    }) = reply
    else {
        panic!("expected native_apps, got {reply:?}");
    };
    assert_eq!(id.as_deref(), Some("probe-1"));
    assert!(sessions_seen.is_empty());
    assert!(!agents.contains(&AgentId::GROK), "{agents:?}");
    let mut once = agents.clone();
    once.sort_by_key(|agent| agent.descriptor().id);
    once.dedup();
    assert_eq!(once.len(), agents.len(), "{agents:?}");
    assert_eq!(agents, installed_agents());
}

/// Opening holds the session's launch claim for as long as it takes, so a resume, a switch or a
/// launch begun meanwhile is refused, and a second open is refused while one is under way.
#[tokio::test]
async fn opening_is_refused_while_a_launch_or_a_switch_holds_the_session() {
    let (state, dir) = app_state("native-app-claimed");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    let session = Session {
        status: SessionStatus::Interrupted,
        ..claude_session(None)
    };
    state.store.insert_session(&session).unwrap();

    let launch = state.begin_launch("s").expect("claimed");
    let err = open_session_in_app(&state, "s", Vec::new())
        .await
        .expect_err("refused");
    assert_eq!(code_of(&err), error_code::SESSION_ALREADY_STARTING);
    assert_eq!(err.to_string(), "this session is already being started");
    drop(launch);

    let switch = state.begin_switch("s").expect("claimed");
    let err = open_session_in_app(&state, "s", Vec::new())
        .await
        .expect_err("refused");
    assert_eq!(code_of(&err), error_code::SESSION_ALREADY_STARTING);
    assert_eq!(err.to_string(), "this session is already being started");
    drop(switch);
}

/// A refused open lets go of the claim, so the session can be launched afterwards, whether its
/// provider declares no application or the application has nothing to open; an unknown session is
/// still reported as unknown.
#[tokio::test]
async fn a_refused_open_leaves_the_session_free() {
    let (state, dir) = app_state("native-app-claim-released");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    for (id, agent, conversation, code) in [
        (
            "no-app",
            AgentId::GROK,
            true,
            error_code::PROVIDER_UNSUPPORTED,
        ),
        (
            "no-conversation",
            AgentId::CLAUDE,
            false,
            error_code::NATIVE_APP_UNAVAILABLE,
        ),
    ] {
        let session = Session {
            id: id.to_string(),
            agent,
            status: SessionStatus::Interrupted,
            has_conversation: conversation,
            ..claude_session(None)
        };
        state.store.insert_session(&session).unwrap();

        let err = open_session_in_app(&state, id, Vec::new())
            .await
            .expect_err(id);
        assert_eq!(code_of(&err), code, "{id}");
        drop(state.begin_launch(id).expect("free again"));
    }

    let err = open_session_in_app(&state, "missing", Vec::new())
        .await
        .expect_err("refused");
    assert_eq!(code_of(&err), error_code::UNKNOWN_SESSION);
}

/// A conversation id the agent would not take is refused as `native_app_unavailable`, naming why,
/// before anything is asked of the host.
#[tokio::test]
async fn a_conversation_id_that_is_not_one_is_refused_as_unavailable() {
    let (state, dir) = app_state("native-app-bad-id");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    for agent in AgentId::ALL
        .into_iter()
        .filter(|agent| crate::adapter::provider(*agent).native_app().is_some())
    {
        let session = Session {
            id: format!("bad-{agent:?}"),
            agent,
            status: SessionStatus::Interrupted,
            agent_session_id: Some("not a conversation id".to_string()),
            ..claude_session(None)
        };
        state.store.insert_session(&session).unwrap();
        let err = open_session_in_app(&state, &session.id, Vec::new())
            .await
            .expect_err("refused");
        assert_eq!(
            code_of(&err),
            error_code::NATIVE_APP_UNAVAILABLE,
            "{agent:?}"
        );
        assert!(
            crate::test_support::param_of(&err, "detail").contains("not a conversation id"),
            "{agent:?}"
        );
    }
}

/// A probe names the installed agents and judges the asked sessions: those of an agent with an app,
/// each in exactly one of `seen` and `unseen`; unknown ids and agents with no app are not judged. The
/// default directories come from the login shell's environment, so where it cannot be captured
/// nothing is judged and the agents are still answered.
#[test]
fn a_probe_judges_only_sessions_of_agents_with_an_app() {
    let (state, dir) = app_state("native-app-probe-judging");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    for (id, agent) in [("claude", AgentId::CLAUDE), ("grok", AgentId::GROK)] {
        let session = Session {
            id: id.to_string(),
            agent,
            ..claude_session(None)
        };
        state.store.insert_session(&session).unwrap();
    }
    let asked: Vec<String> = ["claude", "grok", "missing"].map(String::from).into();

    let probe = probe(&state, &asked).expect("probed");

    assert_eq!(probe.agents, installed_agents());
    assert!(!probe
        .seen
        .iter()
        .chain(&probe.unseen)
        .any(|id| id != "claude"));
    if crate::env_shell::cached_snapshot().is_ok() {
        // A conversation no default directory holds a record of is judged, and judged unseen.
        assert_eq!(probe.unseen, ["claude"]);
        assert!(probe.seen.is_empty());
    } else {
        assert!(probe.seen.is_empty() && probe.unseen.is_empty());
    }
}

/// With no sessions asked about nothing is judged, and the shell is not needed.
#[test]
fn a_probe_of_no_sessions_judges_none() {
    let (state, _dir) = app_state("native-app-probe-empty");
    let probe = probe(&state, &[]).expect("probed");
    assert_eq!(probe.agents, installed_agents());
    assert!(probe.seen.is_empty() && probe.unseen.is_empty());
}

/// A probe judges no more than `PROBED_SESSIONS_LIMIT` sessions, the first ones asked.
#[test]
fn a_probe_judges_the_first_sessions_asked_up_to_its_limit() {
    let (state, dir) = app_state("native-app-probe-limit");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    let ids: Vec<String> = (0..PROBED_SESSIONS_LIMIT + 1)
        .map(|n| format!("s-{n:03}"))
        .collect();
    for id in &ids {
        let session = Session {
            id: id.clone(),
            ..claude_session(None)
        };
        state.store.insert_session(&session).unwrap();
    }

    let probe = probe(&state, &ids).expect("probed");

    let judged = probe.seen.len() + probe.unseen.len();
    if crate::env_shell::cached_snapshot().is_ok() {
        assert_eq!(judged, PROBED_SESSIONS_LIMIT);
        assert!(!probe
            .seen
            .iter()
            .chain(&probe.unseen)
            .any(|id| *id == ids[PROBED_SESSIONS_LIMIT]));
    } else {
        assert_eq!(judged, 0);
    }
}

/// An interrupted session `id` with a conversation, as `changes` leaves it, with a quota recovery
/// waiting to resume it.
fn parked(state: &Arc<AppState>, id: &str, changes: impl FnOnce(&mut Session)) {
    let mut session = Session {
        id: id.to_string(),
        status: SessionStatus::Interrupted,
        ..claude_session(None)
    };
    changes(&mut session);
    state.store.insert_session(&session).unwrap();
    // A recovery is parked for a process that is up; it outlives the process, which is gone by the
    // time the session is opened.
    let live = idle_stand_in(id);
    state.register_live(live.clone());
    state.quota_recovery.wait_for_test(state, id);
    assert!(state.quota_recovery.is_waiting_for_test(id), "{id} waits");
    assert!(state.remove_live(&live));
}

/// The claim an open takes leaves the session's pending recoveries alone, unlike a launch's, which
/// cancels them: the open may still be refused.
#[test]
fn only_a_launch_claim_cancels_the_pending_recoveries() {
    let (state, dir) = app_state("native-app-claims-recoveries");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    parked(&state, "s", |_| {});

    drop(state.begin_switch("s").expect("claimed"));
    assert!(state.quota_recovery.is_waiting_for_test("s"));

    drop(state.begin_launch("s").expect("claimed"));
    assert!(!state.quota_recovery.is_waiting_for_test("s"));
}

/// An open that is refused, whatever the reason, leaves the session's recovery waiting to resume it.
#[tokio::test]
async fn a_refused_open_leaves_a_pending_recovery_alone() {
    let (state, dir) = app_state("native-app-refused-recoveries");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    parked(&state, "running", |session| {
        session.status = SessionStatus::Working
    });
    parked(&state, "no-app", |session| session.agent = AgentId::GROK);
    parked(&state, "no-conversation", |session| {
        session.has_conversation = false
    });
    parked(&state, "bad-id", |session| {
        session.agent_session_id = Some("not a conversation id".to_string());
    });

    for (id, code, says) in [
        (
            "running",
            error_code::SESSION_WORKING,
            "in the middle of a turn",
        ),
        (
            "no-app",
            error_code::PROVIDER_UNSUPPORTED,
            "does not offer a desktop application",
        ),
        (
            "no-conversation",
            error_code::NATIVE_APP_UNAVAILABLE,
            "this session has no conversation yet",
        ),
        (
            "bad-id",
            error_code::NATIVE_APP_UNAVAILABLE,
            "is not a conversation id",
        ),
    ] {
        let err = open_session_in_app(&state, id, Vec::new())
            .await
            .expect_err(id);
        assert_eq!(code_of(&err), code, "{id}");
        assert!(err.to_string().contains(says), "{id}: {err}");
        assert!(state.quota_recovery.is_waiting_for_test(id), "{id}");
    }

    // Held by another operation: refused before anything is looked at, the recovery still waiting.
    let _claim = state.begin_switch("bad-id").expect("claimed");
    let err = open_session_in_app(&state, "bad-id", Vec::new())
        .await
        .expect_err("held");
    assert_eq!(code_of(&err), error_code::SESSION_ALREADY_STARTING);
    assert_eq!(err.to_string(), "this session is already being started");
    assert!(state.quota_recovery.is_waiting_for_test("bad-id"));
}

/// While a change of the session's account's sharing moves the account's directory, nothing is
/// opened and nothing is cancelled.
#[tokio::test]
async fn opening_is_refused_while_the_accounts_sharing_is_changing() {
    let (state, dir) = app_state("native-app-sharing-change");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    let account = crate::protocol::Account {
        id: "work".to_string(),
        agent: AgentId::CLAUDE,
        name: "Work".to_string(),
        config_dir: dir.join("work").to_string_lossy().into_owned(),
        share_default_config: false,
        share_default_config_incomplete: false,
        disabled: false,
        versions: Default::default(),
    };
    parked(&state, "s", |session| {
        session.account_id = Some("work".to_string());
        session.config_dir = Some(account.config_dir.clone());
    });

    let (_, lock) = state
        .lock_account_sharing(&account, |_, _| Ok(()))
        .expect("locked");
    let err = open_session_in_app(&state, "s", Vec::new())
        .await
        .expect_err("refused");
    assert_eq!(code_of(&err), error_code::ACCOUNT_SHARING_IN_PROGRESS);
    assert!(
        err.to_string()
            .contains("account `Work`'s sharing of the default configuration is being changed"),
        "{err}"
    );
    assert!(state.quota_recovery.is_waiting_for_test("s"));

    drop(lock);
    // Once it is done the open goes as far as its other checks take it: this session's conversation
    // id is not one, so it is refused for that, not for the sharing.
    state
        .store
        .update_session(&Session {
            agent_session_id: Some("not a conversation id".to_string()),
            ..state.session_record("s").unwrap()
        })
        .unwrap();
    let err = open_session_in_app(&state, "s", Vec::new())
        .await
        .expect_err("refused");
    assert_eq!(code_of(&err), error_code::NATIVE_APP_UNAVAILABLE);
    assert!(
        crate::test_support::param_of(&err, "detail").contains("is not a conversation id"),
        "{err}"
    );
}

/// Archiving for an open removes no branch or worktree the session made, where a user's archive does:
/// the application works in the session's directory.
#[tokio::test(flavor = "multi_thread")]
async fn an_open_keeps_the_branches_and_worktrees_the_session_made() {
    use crate::test_support::{branch_made_by, checked_project, fixture_git, git, repo_with};

    let (state, dir) = app_state("native-app-keeps-branches");
    let repo = repo_with("native-app-keeps-branches-repo", &[(b"a.txt", b"one\n")]);
    checked_project(&state, &dir, &repo);
    let worktrees = ScratchDir::new("native-app-keeps-branches-worktrees");
    let has_branch = |branch: &str| {
        fixture_git(&repo)
            .args([
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ])
            .status()
            .unwrap()
            .success()
    };
    for id in ["s", "control"] {
        state
            .store
            .insert_session(&Session {
                project_id: Some("p1".to_string()),
                ..project_session_with_conversation(id, SessionStatus::Interrupted)
            })
            .unwrap();
    }
    branch_made_by(&state, &repo, "plain-branch", "s");
    branch_made_by(&state, &repo, "tree-branch", "s");
    let tree = worktrees.join("tree-branch");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            tree.to_str().unwrap(),
            "tree-branch",
        ],
    );
    branch_made_by(&state, &repo, "control-branch", "control");
    let (asked, ask) = asked_app();

    open_with(&state, "s", Vec::new(), installed, shell, ask)
        .await
        .expect("opened");
    assert!(asked.load(Ordering::SeqCst));
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Archived
    );

    // A user's archive of another session is the control: once its branch is gone, the removal an
    // archive starts has run, and the open's archive started none.
    handle(
        &state,
        None,
        RequestBody::ArchiveSession {
            session: "control".to_string(),
            force_remove: Vec::new(),
            keep: Vec::new(),
            keep_process_ids: Vec::new(),
        },
    )
    .await
    .expect("archived");
    crate::coordinator::test_support::wait_until(|| !has_branch("control-branch")).await;

    assert!(has_branch("plain-branch"));
    assert!(has_branch("tree-branch"));
    assert!(tree.exists());
}

/// What the application was asked, and whether the session's recovery was still waiting when it was.
type Seen = (String, PathBuf, PathBuf, bool);

/// A stand-in application that notes what it is asked.
struct ShowingApp {
    state: Arc<AppState>,
    seen: std::sync::Mutex<Option<Seen>>,
}

impl NativeApp for ShowingApp {
    fn open(&self, open: &NativeAppOpen<'_>) -> Result<()> {
        *self.seen.lock().unwrap() = Some((
            open.conversation.to_string(),
            open.cwd.to_path_buf(),
            open.scratch.to_path_buf(),
            self.state
                .quota_recovery
                .is_waiting_for_test(&open.session.id),
        ));
        Ok(())
    }
}

/// The session's pending recoveries are cancelled once the application is about to be asked, so
/// that nothing resumes the conversation it now holds.
#[tokio::test(flavor = "multi_thread")]
async fn asking_the_app_cancels_the_pending_recoveries_first() {
    let (state, dir) = app_state("native-app-asks-cancels");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    parked(&state, "s", |_| {});
    let app = ShowingApp {
        state: Arc::clone(&state),
        seen: std::sync::Mutex::new(None),
    };
    let session = state.session_record("s").unwrap();
    let (cwd, scratch) = (dir.join("cwd"), dir.join("scratch"));

    ask_app(&state)(
        &app,
        &session,
        CONVERSATION,
        &cwd,
        &scratch,
        &HashMap::new(),
    )
    .expect("asked");

    let seen = app.seen.lock().unwrap().clone().expect("the app was asked");
    assert_eq!(seen, (CONVERSATION.to_string(), cwd, scratch, false));
    assert!(!state.quota_recovery.is_waiting_for_test("s"));
}

/// The archive an open makes cancels the session's pending recoveries, as any archive does: an archived
/// session is not brought back by an automatic account switch or a capacity retry, whatever becomes
/// of the open. They are gone before the application is asked.
#[tokio::test(flavor = "multi_thread")]
async fn the_archive_of_an_open_cancels_the_pending_recoveries() {
    let (state, dir) = app_state("native-app-archive-recoveries");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    parked(&state, "s", |_| {});
    let waiting_when_asked = Arc::new(AtomicBool::new(true));
    let (noted, watched) = (Arc::clone(&waiting_when_asked), Arc::clone(&state));
    let ask: Asker = Box::new(move |_, _, _, _, _, _| {
        noted.store(
            watched.quota_recovery.is_waiting_for_test("s"),
            Ordering::SeqCst,
        );
        Ok(())
    });

    open_with(&state, "s", Vec::new(), installed, shell, ask)
        .await
        .expect("opened");

    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Archived
    );
    assert!(!waiting_when_asked.load(Ordering::SeqCst));
}

/// A process that is not seen down in time refuses the open with `session_did_not_stop`: the session
/// stays archived, the application is not asked, its pending recovery is cancelled by the archive (an
/// archived session is not brought back by an automatic switch or retry) and the claim is let go.
#[tokio::test(flavor = "multi_thread")]
async fn a_process_that_does_not_stop_refuses_the_open_and_leaves_the_session_archived() {
    let (state, dir) = app_state("native-app-did-not-stop");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    state
        .store
        .insert_session(&Session {
            status: SessionStatus::Idle,
            ..claude_session(None)
        })
        .unwrap();
    // No exit watcher: nothing records the process down.
    let live = idle_stand_in("s");
    state.register_live(live.clone());
    state.quota_recovery.wait_for_test(&state, "s");
    assert!(state.quota_recovery.is_waiting_for_test("s"));
    *state.restart_stop_timeout.lock().unwrap() = Some(Duration::from_millis(200));
    let (asked, ask) = asked_app();

    let err = open_with(&state, "s", Vec::new(), installed, shell, ask)
        .await
        .expect_err("refused");

    assert_eq!(code_of(&err), error_code::SESSION_DID_NOT_STOP);
    assert_eq!(
        err.to_string(),
        "the session's process did not end in time, so its conversation was not opened in the app; \
         the session is archived"
    );
    assert_eq!(crate::test_support::param_of(&err, "cause"), "open_in_app");
    assert_eq!(crate::test_support::param_of(&err, "session"), "s");
    assert!(!asked.load(Ordering::SeqCst));
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Archived
    );
    assert!(
        !state.quota_recovery.is_waiting_for_test("s"),
        "an archived session is not resumed by its recovery"
    );
    drop(state.begin_switch("s").expect("the claim is released"));
}

/// A locked session, or a locked session bound to it, refuses an open that names trees to keep before
/// any of them is kept: the tree stays its session's, the session is neither archived nor ended.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_locked_session_or_descendant_keeps_nothing_when_it_refuses_the_open() {
    for locked in ["s", "worker"] {
        let (state, dir) =
            crate::test_support::console_with_project_and_accounts("native-app-locked-keep");
        for (id, status, owner) in [
            ("s", SessionStatus::Idle, None),
            ("worker", SessionStatus::Interrupted, Some("s")),
        ] {
            state
                .store
                .insert_session(&Session {
                    bound_to: owner.map(str::to_string),
                    ..project_session_with_conversation(id, status)
                })
                .unwrap();
        }
        state.store.set_session_locked(locked, true).unwrap();
        let (live, child) = crate::test_support::running_leaving_one(&state, &dir, "s");
        let _guard = crate::test_support::KillOnDrop(vec![child]);
        let tree = listed_tree_of(&state, "s", child).await;
        let (asked, ask) = asked_app();

        let err = open_with(&state, "s", vec![tree.clone()], installed, shell, ask)
            .await
            .expect_err("refused");

        assert_eq!(code_of(&err), error_code::SESSION_LOCKED, "{locked}");
        assert!(
            err.to_string().contains("includes a locked session"),
            "{locked}: {err}"
        );
        assert_eq!(crate::test_support::param_of(&err, "session"), locked);
        assert!(!asked.load(Ordering::SeqCst), "{locked}");
        assert!(!live.poll_exit(), "{locked}: its process was ended");
        for id in ["s", "worker"] {
            assert_ne!(
                state.session_record(id).unwrap().status,
                SessionStatus::Archived,
                "{locked}: {id}"
            );
        }
        let trees = crate::coordinator::processes::list(
            &state,
            crate::protocol::ProcessTarget::Session {
                session_id: "s".to_string(),
            },
        )
        .await
        .unwrap();
        let still = trees
            .iter()
            .find(|listed| listed.id == tree)
            .expect("the tree is still its session's");
        assert!(!still.kept, "{locked}");
        assert!(crate::test_support::is_running(child), "{locked}");
    }
}

fn uncapturable_shell() -> Result<HashMap<String, String>> {
    Err(anyhow::anyhow!("the login shell would not answer"))
}

fn marked_shell() -> Result<HashMap<String, String>> {
    Ok(HashMap::from([(
        "OCTOBOARD_TEST_SHELL".to_string(),
        "captured".to_string(),
    )]))
}

/// A shell environment that cannot be captured refuses the open before anything is archived or
/// ended, as the other refusals that do not depend on archiving do.
#[tokio::test(flavor = "multi_thread")]
async fn a_shell_environment_that_cannot_be_captured_refuses_the_open_before_the_archive() {
    for case in ["at-rest", "interrupted"] {
        let (state, dir) = app_state("native-app-no-shell");
        let (live, before) = if case == "at-rest" {
            (
                Some(with_process(&state, &dir, SessionStatus::Idle)),
                SessionStatus::Idle,
            )
        } else {
            state
                .store
                .insert_console(&crate::test_support::console(&dir))
                .unwrap();
            state
                .store
                .insert_session(&Session {
                    status: SessionStatus::Interrupted,
                    ..claude_session(None)
                })
                .unwrap();
            (None, SessionStatus::Interrupted)
        };
        let (asked, ask) = asked_app();

        let err = open_with(&state, "s", Vec::new(), installed, uncapturable_shell, ask)
            .await
            .expect_err("refused");

        assert_eq!(
            code_of(&err),
            error_code::SHELL_ENVIRONMENT_TIMEOUT,
            "{case}"
        );
        assert!(
            err.to_string()
                .contains("The shell environment could not be captured"),
            "{case}: {err}"
        );
        assert!(
            crate::test_support::param_of(&err, "detail")
                .contains("the login shell would not answer"),
            "{case}: {err}"
        );
        assert!(!asked.load(Ordering::SeqCst), "{case}");
        if let Some(live) = live {
            assert!(!live.poll_exit(), "{case}: its process was ended");
            assert!(state.live_session("s").is_some(), "{case}");
        }
        assert_eq!(state.session_record("s").unwrap().status, before, "{case}");
        drop(state.begin_switch("s").expect("the claim is released"));
    }
}

/// The environment the capture took is the one the application is asked with, for an archived
/// session as for one archived by the open.
#[tokio::test(flavor = "multi_thread")]
async fn the_captured_shell_environment_reaches_the_application() {
    let (state, dir) = app_state("native-app-shell-reaches");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    state
        .store
        .insert_session(&Session {
            status: SessionStatus::Archived,
            ..claude_session(None)
        })
        .unwrap();
    let seen = Arc::new(AtomicBool::new(false));
    let noted = Arc::clone(&seen);
    let ask: Asker = Box::new(move |_, _, _, _, _, shell| {
        noted.store(
            shell.get("OCTOBOARD_TEST_SHELL").map(String::as_str) == Some("captured"),
            Ordering::SeqCst,
        );
        Ok(())
    });

    open_with(&state, "s", Vec::new(), installed, marked_shell, ask)
        .await
        .expect("opened");

    assert!(seen.load(Ordering::SeqCst));
}
