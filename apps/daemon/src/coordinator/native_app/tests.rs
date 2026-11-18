use std::collections::HashMap;
use std::path::{Path, PathBuf};
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
fn locator_for(default_root: &Path) -> Locator {
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
        },
    )
    .await
    .expect_err("refused");
    assert_eq!(code_of(&err), error_code::UNKNOWN_SESSION);
}

/// A session with a process is not opened in the application, whatever the process is doing: the
/// two would write to one conversation. Nothing is stopped for it.
#[tokio::test]
async fn a_session_with_a_process_is_not_opened_in_its_app() {
    for status in [
        SessionStatus::Starting,
        SessionStatus::Working,
        SessionStatus::WaitingUser,
        SessionStatus::Dormant,
        SessionStatus::Idle,
    ] {
        let (state, dir) = app_state("native-app-running");
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        let session = Session {
            status,
            ..claude_session(None)
        };
        state.store.insert_session(&session).unwrap();
        let err = open_session_in_app(&state, "s").await.expect_err("refused");
        assert_eq!(
            code_of(&err),
            error_code::SESSION_ALREADY_RUNNING,
            "{status:?}"
        );
    }
}

/// A session whose status says it has no process but whose process is still registered is running.
#[tokio::test]
async fn an_interrupted_session_whose_process_is_still_up_is_not_opened_in_its_app() {
    let (state, dir) = app_state("native-app-live");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    let session = Session {
        status: SessionStatus::Interrupted,
        ..claude_session(None)
    };
    state.store.insert_session(&session).unwrap();
    let live = idle_stand_in("s");
    state.register_live(live.clone());

    let err = open_session_in_app(&state, "s").await.expect_err("refused");
    assert_eq!(code_of(&err), error_code::SESSION_ALREADY_RUNNING);
}

/// An agent that has no desktop application cannot be opened in one, and nor can a session that has
/// no conversation to show yet; both come back as `native_app_unavailable` naming a `detail`.
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
        let err = open_session_in_app(&state, id).await.expect_err(id);
        assert_eq!(code_of(&err), error_code::NATIVE_APP_UNAVAILABLE, "{id}");
        assert!(
            !crate::test_support::param_of(&err, "detail").is_empty(),
            "{id}: the reason is given"
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
    let err = open_session_in_app(&state, "s").await.expect_err("refused");
    assert_eq!(code_of(&err), error_code::SESSION_ALREADY_STARTING);
    drop(launch);

    let switch = state.begin_switch("s").expect("claimed");
    let err = open_session_in_app(&state, "s").await.expect_err("refused");
    assert_eq!(code_of(&err), error_code::SESSION_ALREADY_STARTING);
    drop(switch);
}

/// A refused open lets go of the claim, so the session can be launched afterwards; an unknown session
/// is still reported as unknown.
#[tokio::test]
async fn a_refused_open_leaves_the_session_free() {
    let (state, dir) = app_state("native-app-claim-released");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    let session = Session {
        agent: AgentId::GROK,
        status: SessionStatus::Interrupted,
        ..claude_session(None)
    };
    state.store.insert_session(&session).unwrap();

    let err = open_session_in_app(&state, "s").await.expect_err("refused");
    assert_eq!(code_of(&err), error_code::NATIVE_APP_UNAVAILABLE);
    drop(state.begin_launch("s").expect("free again"));

    let err = open_session_in_app(&state, "missing")
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
        let err = open_session_in_app(&state, &session.id)
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

    drop(state.begin_launch_leaving_recoveries("s").expect("claimed"));
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
        session.status = SessionStatus::Idle
    });
    parked(&state, "no-app", |session| session.agent = AgentId::GROK);
    parked(&state, "no-conversation", |session| {
        session.has_conversation = false
    });
    parked(&state, "bad-id", |session| {
        session.agent_session_id = Some("not a conversation id".to_string());
    });

    for (id, code) in [
        ("running", error_code::SESSION_ALREADY_RUNNING),
        ("no-app", error_code::NATIVE_APP_UNAVAILABLE),
        ("no-conversation", error_code::NATIVE_APP_UNAVAILABLE),
        ("bad-id", error_code::NATIVE_APP_UNAVAILABLE),
    ] {
        let err = open_session_in_app(&state, id).await.expect_err(id);
        assert_eq!(code_of(&err), code, "{id}");
        assert!(state.quota_recovery.is_waiting_for_test(id), "{id}");
    }

    // Held by another operation: refused before anything is looked at, the recovery still waiting.
    let _claim = state
        .begin_launch_leaving_recoveries("bad-id")
        .expect("claimed");
    let err = open_session_in_app(&state, "bad-id")
        .await
        .expect_err("held");
    assert_eq!(code_of(&err), error_code::SESSION_ALREADY_STARTING);
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
    let err = open_session_in_app(&state, "s").await.expect_err("refused");
    assert_eq!(code_of(&err), error_code::ACCOUNT_SHARING_IN_PROGRESS);
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
    let err = open_session_in_app(&state, "s").await.expect_err("refused");
    assert_eq!(code_of(&err), error_code::NATIVE_APP_UNAVAILABLE);
}
