use std::sync::Arc;

use super::*;
use crate::messaging::outbox::Drain;
use crate::protocol::{Event, OrchestrationRole};
use crate::state::WriteHold;
use crate::test_support::{app_state, PATIENCE};

/// A completion notification trims the list once the session is working: trimmed while it is
/// still idle, the record it publishes would show a plain idle between the finished work and
/// the turn it started.
#[tokio::test]
async fn a_completion_notification_does_not_publish_a_plain_idle_before_the_turn_starts() {
    let (state, dir) = app_state("server-notification-order");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    state
        .store
        .insert_session(&crate::test_support::console_session(
            "s",
            SessionStatus::Idle,
        ))
        .unwrap();
    let stop =
        br#"{"hook_event_name": "Stop", "background_tasks": [{"id": "b1", "type": "shell"}]}"#;
    apply_hook(&state, "s", stop);
    assert_eq!(state.background_tasks_of("s").len(), 1);
    let mut events = state.subscribe();

    let notified = br#"{"hook_event_name": "UserPromptSubmit", "prompt": "<task-notification><task-id>b1</task-id></task-notification>"}"#;
    apply_hook(&state, "s", notified);

    let mut published = Vec::new();
    while let Ok(Event::SessionUpserted { session }) = events.try_recv() {
        published.push((session.status, session.background_tasks.len()));
    }
    assert!(
        !published.contains(&(SessionStatus::Idle, 0)),
        "{published:?}"
    );
    assert_eq!(published.last(), Some(&(SessionStatus::Working, 0)));
    assert!(state.background_tasks_of("s").is_empty());
}

/// A Grok session that stopped on an error holds what is queued for it, and a prompt sent ends
/// the hold; the other agents' failures hold nothing.
#[tokio::test]
async fn only_a_grok_error_holds_writes_and_a_prompt_ends_the_hold() {
    for (agent, error, held) in [
        (Agent::Grok, r#"{"hookEventName": "StopFailure"}"#, true),
        (
            Agent::Grok,
            r#"{"hook_event_name": "Notification", "notification_type": "agent_error"}"#,
            true,
        ),
        (
            Agent::Grok,
            r#"{"hook_event_name": "Notification", "notification_type": "idle_prompt"}"#,
            false,
        ),
        (
            Agent::Claude,
            r#"{"hook_event_name": "StopFailure"}"#,
            false,
        ),
        (Agent::Codex, r#"{"hook_event_name": "StopFailure"}"#, false),
    ] {
        let (state, dir) = app_state("server-agent-error");
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        state
            .store
            .insert_session(&crate::test_support::console_session(
                "s",
                SessionStatus::Idle,
            ))
            .unwrap();
        let live = crate::test_support::fake_live_session("s", agent, 80, 24, "sleep 30");
        state.register_live(live.clone());

        apply_hook(&state, "s", error.as_bytes());
        let hold = state.write_hold("s");
        assert_eq!(
            hold,
            held.then_some(WriteHold::AgentError),
            "{agent:?} {error}"
        );
        // The error alone raises no hand; the session stays idle.
        assert_eq!(
            state.agent_status(&state.session_record("s").unwrap()),
            SessionStatus::Idle
        );
        if !held {
            continue;
        }

        state.queue_message("s", "go on");
        assert_eq!(state.flush_outbox("s", SessionStatus::Idle), Drain::Pending);
        assert_eq!(state.queued_count("s"), 1);

        apply_hook(&state, "s", br#"{"hook_event_name": "UserPromptSubmit"}"#);
        assert_eq!(state.write_hold("s"), None);
        let deadline = std::time::Instant::now() + PATIENCE;
        while state.queued_count("s") != 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "the prompt must release the queue"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}

/// An error that leaves no dialog on screen holds the queue only briefly: once the hold has run
/// out, the queue goes in with no prompt sent.
#[tokio::test]
async fn an_agent_error_hold_runs_out_and_releases_the_queue() {
    let (state, dir) = app_state("server-agent-error-settle");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    state
        .store
        .insert_session(&crate::test_support::console_session(
            "s",
            SessionStatus::Idle,
        ))
        .unwrap();
    let live = crate::test_support::fake_live_session("s", Agent::Grok, 80, 24, "sleep 30");
    state.register_live(live.clone());

    apply_hook(&state, "s", br#"{"hookEventName": "StopFailure"}"#);
    state.queue_message("s", "go on");
    assert_eq!(state.flush_outbox("s", SessionStatus::Idle), Drain::Pending);

    let deadline = std::time::Instant::now() + crate::session::AGENT_ERROR_SETTLE + PATIENCE;
    while state.queued_count("s") != 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "the queue must go in once the hold has run out"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(state.write_hold("s"), None);
}

/// A Grok Build prompt arms the title watch for its turn, so Ctrl-C before the first token,
/// which fires no hook, still puts the session back at its prompt.
#[tokio::test]
async fn a_grok_prompt_watches_the_title_for_a_cancel_no_hook_reports() {
    let (state, dir) = app_state("server-grok-title-watch");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    state
        .store
        .insert_session(&crate::test_support::console_session(
            "s",
            SessionStatus::Idle,
        ))
        .unwrap();
    let live = crate::test_support::fake_live_session("s", Agent::Grok, 80, 24, "sleep 30");
    state.register_live(live.clone());

    apply_hook(
        &state,
        "s",
        br#"{"hook_event_name": "UserPromptSubmit", "hookEventName": "user_prompt_submit"}"#,
    );
    assert_eq!(
        state.store.get_session("s").unwrap().unwrap().status,
        SessionStatus::Working
    );
    for byte in include_bytes!("../../../testdata/grok_title_cancel_before_output.bin") {
        live.on_output(std::slice::from_ref(byte));
    }
    let idle = {
        let state = state.clone();
        crate::test_support::within(std::time::Duration::from_secs(5), move || {
            state.store.get_session("s").unwrap().unwrap().status == SessionStatus::Idle
        })
        .await
    };
    assert!(
        idle,
        "the cancel the title showed puts the session at its prompt"
    );
}

/// What the title shows while a Grok Build card waits on the user is not counted against the
/// turn that carries on once the hand comes down: the watch is armed again from there.
#[tokio::test]
async fn a_grok_card_s_title_does_not_end_the_turn_that_carries_on_after_it() {
    let (state, dir) = app_state("server-grok-title-card");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    state
        .store
        .insert_session(&crate::test_support::console_session(
            "s",
            SessionStatus::Idle,
        ))
        .unwrap();
    let live = crate::test_support::fake_live_session("s", Agent::Grok, 80, 24, "sleep 30");
    state.register_live(live.clone());
    let status = |state: &Arc<AppState>| state.store.get_session("s").unwrap().unwrap().status;
    let running = "\x1b]0;\u{280B} - Working\u{2026} - grok\x07".as_bytes();
    let at_prompt = b"\x1b]0;grok\x07";

    apply_hook(&state, "s", br#"{"hook_event_name": "UserPromptSubmit"}"#);
    live.on_output(running);
    apply_hook(
        &state,
        "s",
        br#"{"hook_event_name": "PreToolUse", "toolName": "ask_user_question"}"#,
    );
    assert_eq!(status(&state), SessionStatus::WaitingUser);
    live.on_output(at_prompt);
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    apply_hook(
        &state,
        "s",
        br#"{"hook_event_name": "PostToolUse", "toolName": "ask_user_question"}"#,
    );
    assert_eq!(status(&state), SessionStatus::Working);
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    assert_eq!(
        status(&state),
        SessionStatus::Working,
        "the card's title ended the turn"
    );

    // The watch armed when the hand came down still sees a cancel.
    live.on_output(running);
    live.on_output(at_prompt);
    let idle = {
        let state = state.clone();
        crate::test_support::within(std::time::Duration::from_secs(5), move || {
            status(&state) == SessionStatus::Idle
        })
        .await
    };
    assert!(
        idle,
        "a cancel after the card still puts the session at its prompt"
    );
}

/// A Claude Code session at `status` with a live stand-in, and the path its transcript is at,
/// for the interrupt-watch tests below. The stand-in is returned to keep it alive.
fn claude_session_with_transcript(
    name: &str,
    status: SessionStatus,
) -> (
    Arc<AppState>,
    crate::test_support::ScratchDir,
    crate::test_support::StandIn,
    std::path::PathBuf,
) {
    let (state, dir) = app_state(name);
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    state
        .store
        .insert_session(&crate::test_support::console_session("s", status))
        .unwrap();
    let live = crate::test_support::idle_stand_in("s");
    state.register_live(live.clone());
    let path = dir.join("transcript.jsonl");
    std::fs::write(&path, "").unwrap();
    (state, dir, live, path)
}

fn claude_hook(event: &str, transcript: &std::path::Path) -> Vec<u8> {
    serde_json::json!({
        "hook_event_name": event,
        "prompt": "go",
        "tool_name": "Bash",
        "transcript_path": transcript,
    })
    .to_string()
    .into_bytes()
}

/// Whether the session reaches `status` within `timeout`.
async fn reaches_within(
    state: &Arc<AppState>,
    status: SessionStatus,
    timeout: std::time::Duration,
) -> bool {
    let state = state.clone();
    crate::test_support::within(timeout, move || {
        state.store.get_session("s").unwrap().unwrap().status == status
    })
    .await
}

async fn reaches(state: &Arc<AppState>, status: SessionStatus) -> bool {
    reaches_within(state, status, PATIENCE).await
}

/// An idle Claude Code session whose outbox checks that what it writes is sent, with its output
/// read so a test can see what reached it.
fn checked_claude_session(
    name: &str,
) -> (
    Arc<AppState>,
    crate::test_support::ScratchDir,
    crate::test_support::StandIn,
    std::path::PathBuf,
) {
    let (state, dir, live, path) = claude_session_with_transcript(name, SessionStatus::Idle);
    crate::session::spawn_reader_thread((*live).clone(), 8 * 1024);
    *state.send_wait.lock().unwrap() = Some(std::time::Duration::from_millis(300));
    (state, dir, live, path)
}

/// [`checked_claude_session`] with room for a slow machine to read the write and apply the
/// hook: a wait that succeeds ends as soon as the hook lands, so a long one costs nothing.
fn patient_claude_session(
    name: &str,
) -> (
    Arc<AppState>,
    crate::test_support::ScratchDir,
    crate::test_support::StandIn,
    std::path::PathBuf,
) {
    let session = checked_claude_session(name);
    *session.0.send_wait.lock().unwrap() = Some(PATIENCE);
    session
}

/// A message the agent does not send is reported unsent to its sender and announced to the
/// user, and one sent after it waits — unwritten — until the agent reports a prompt sent.
#[tokio::test]
async fn a_message_left_unsent_holds_what_follows_until_a_prompt_is_sent() {
    let (state, _dir, live, path) = checked_claude_session("server-unsent");
    let mut events = state.subscribe();

    let first = crate::messaging::delivery::write_message(
        &state,
        "s",
        "first message",
        crate::messaging::delivery::WhenBlocked::Queue,
    )
    .unwrap();
    assert_eq!(first, crate::messaging::delivery::Delivery::Unsent);
    let mut announced = false;
    while let Ok(event) = events.try_recv() {
        announced |= matches!(event, Event::SessionNotice { ref session, ref code, .. }
            if session == "s" && *code == crate::protocol::notice_code::MESSAGE_NOT_SENT);
    }
    assert!(announced);

    let second = crate::messaging::delivery::write_message(
        &state,
        "s",
        "second message",
        crate::messaging::delivery::WhenBlocked::Queue,
    )
    .unwrap();
    assert_eq!(second, crate::messaging::delivery::Delivery::Queued);
    assert_eq!(
        live.input_writes(),
        1,
        "nothing is written behind the unsent message"
    );

    apply_hook(&state, "s", &claude_hook("UserPromptSubmit", &path));
    let heard = crate::test_support::output_containing(&live, "second message");
    assert!(heard.contains("second message"), "{heard}");
}

/// A background task's completion arrives as a prompt the agent sends itself, which leaves
/// whatever waits in its input box there, so it does not lift the hold.
#[tokio::test]
async fn a_task_notification_does_not_lift_the_hold() {
    let (state, _dir, live, path) = checked_claude_session("server-unsent-notification");
    let first = crate::messaging::delivery::write_message(
        &state,
        "s",
        "first message",
        crate::messaging::delivery::WhenBlocked::Queue,
    )
    .unwrap();
    assert_eq!(first, crate::messaging::delivery::Delivery::Unsent);

    let notification = serde_json::json!({
        "hook_event_name": "UserPromptSubmit",
        "prompt": "<task-notification><task-id>b1</task-id></task-notification>",
        "transcript_path": path,
    });
    apply_hook(&state, "s", notification.to_string().as_bytes());
    let second = crate::messaging::delivery::write_message(
        &state,
        "s",
        "second message",
        crate::messaging::delivery::WhenBlocked::Queue,
    )
    .unwrap();
    assert_eq!(second, crate::messaging::delivery::Delivery::Queued);
    assert_eq!(live.input_writes(), 1);
}

/// A message the agent reports sending in time is delivered, as before.
#[tokio::test]
async fn a_message_the_agent_sends_is_delivered() {
    let (state, _dir, live, path) = patient_claude_session("server-sent");
    let hook_state = state.clone();
    let runtime = tokio::runtime::Handle::current();
    let sender = std::thread::spawn(move || {
        let _runtime = runtime.enter();
        crate::test_support::output_containing(&live, "a message");
        apply_hook(&hook_state, "s", &claude_hook("UserPromptSubmit", &path));
        live
    });
    let delivery = crate::messaging::delivery::write_message(
        &state,
        "s",
        "a message",
        crate::messaging::delivery::WhenBlocked::Queue,
    )
    .unwrap();
    let _live = sender.join().unwrap();
    assert_eq!(delivery, crate::messaging::delivery::Delivery::Written);
}

/// A write that lands mid-turn is queued by the agent and sent only when its turn ends, so it
/// is not waited on.
#[tokio::test]
async fn a_message_written_mid_turn_is_not_waited_on() {
    let (state, _dir, _live, _path) = checked_claude_session("server-mid-turn");
    state
        .apply_hook_status("s", SessionStatus::Working)
        .unwrap();
    let delivery = crate::messaging::delivery::write_message(
        &state,
        "s",
        "a message",
        crate::messaging::delivery::WhenBlocked::Queue,
    )
    .unwrap();
    assert_eq!(delivery, crate::messaging::delivery::Delivery::Written);
}

/// A session bound to an owner that raises the agent's hand at a permission prompt tells the
/// owner, once for the turn however often the hand goes up and down; an interrupt that puts it
/// back at its prompt says nothing, and a session that reaches its prompt unasked tells
/// nothing either.
#[tokio::test]
async fn an_owner_is_told_when_a_hook_puts_its_session_in_waiting_for_the_user() {
    let (state, _dir, owner, path) =
        claude_session_with_transcript("server-owner-waits", SessionStatus::Idle);
    crate::session::spawn_reader_thread((*owner).clone(), 8 * 1024);
    let worker = crate::test_support::bare_session("w", crate::protocol::Role::Project);
    state
        .store
        .insert_session(&crate::protocol::Session {
            bound_to: Some("s".to_string()),
            orchestration_role: OrchestrationRole::Worker,
            status: SessionStatus::Idle,
            ..worker
        })
        .unwrap();
    let heard = || String::from_utf8_lossy(&owner.recent_output(8 * 1024)).into_owned();
    let told = |count: usize| {
        let owner = (*owner).clone();
        crate::test_support::within(PATIENCE, move || {
            String::from_utf8_lossy(&owner.recent_output(8 * 1024))
                .matches("is waiting for the user")
                .count()
                == count
        })
    };

    apply_hook(&state, "w", &claude_hook("UserPromptSubmit", &path));
    apply_hook(&state, "w", &claude_hook("Stop", &path));
    apply_hook(&state, "w", &claude_hook("UserPromptSubmit", &path));
    apply_hook(&state, "w", &claude_hook("PermissionRequest", &path));
    assert!(told(1).await, "{}", heard());
    apply_hook(&state, "w", &claude_hook("PostToolUse", &path));
    apply_hook(&state, "w", &claude_hook("PermissionRequest", &path));
    apply_hook(&state, "w", &claude_hook("PostToolUse", &path));
    apply_hook(&state, "w", &claude_hook("Stop", &path));
    assert!(told(1).await, "{}", heard());
    // The owner is written to off the hook's path, so give a second notice time to arrive.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(heard().matches("is waiting for the user").count(), 1);

    apply_hook(&state, "w", &claude_hook("UserPromptSubmit", &path));
    apply_hook(&state, "w", &claude_hook("PermissionRequest", &path));
    assert!(told(2).await, "{}", heard());
}

/// A Claude Code turn the user interrupts fires no hook; its prompt arms the transcript watch
/// that puts the session back at its prompt.
#[tokio::test]
async fn an_interrupted_claude_turn_goes_back_to_its_prompt() {
    let (state, _dir, _live, path) =
        claude_session_with_transcript("server-interrupt", SessionStatus::Idle);
    apply_hook(&state, "s", &claude_hook("UserPromptSubmit", &path));
    apply_hook(&state, "s", &claude_hook("PreToolUse", &path));

    std::fs::write(&path, crate::test_support::fresh_interrupt()).unwrap();

    assert!(reaches(&state, SessionStatus::Idle).await);
}

/// The hand coming down on an answered prompt is a move back into work, which arms the watch
/// again: the one armed for the raised hand was looking for a decline.
#[tokio::test]
async fn a_claude_turn_interrupted_after_an_answered_prompt_goes_back_to_its_prompt() {
    let (state, _dir, _live, path) =
        claude_session_with_transcript("server-interrupt-after-hand", SessionStatus::Working);
    apply_hook(&state, "s", &claude_hook("PermissionRequest", &path));
    apply_hook(&state, "s", &claude_hook("PostToolUse", &path));

    std::fs::write(&path, crate::test_support::fresh_interrupt()).unwrap();

    assert!(reaches(&state, SessionStatus::Idle).await);
}

/// An interrupt the watch has not read yet when the next prompt arrives belongs to the turn
/// before, and must not end the new one; the new turn's own interrupt still does, though the
/// session never left `Working` between the two.
#[tokio::test]
async fn only_the_next_turns_own_interrupt_ends_it() {
    let (state, _dir, _live, path) =
        claude_session_with_transcript("server-interrupt-next-turn", SessionStatus::Idle);
    apply_hook(&state, "s", &claude_hook("UserPromptSubmit", &path));
    std::fs::write(&path, crate::test_support::fresh_interrupt()).unwrap();
    // Well inside the first watch's first poll, a second apart.
    apply_hook(&state, "s", &claude_hook("UserPromptSubmit", &path));

    assert!(
        !reaches_within(
            &state,
            SessionStatus::Idle,
            crate::records::transcript::TEST_TIMEOUT
        )
        .await,
        "the next turn must keep working"
    );

    std::fs::write(&path, crate::test_support::fresh_interrupt().repeat(2)).unwrap();
    assert!(reaches(&state, SessionStatus::Idle).await);
}

/// A helper agent's tool events carry an `agent_id` and no `agent_type`: they are not the
/// session's work, so an idle session stays idle.
#[tokio::test]
async fn a_helper_agents_tool_events_leave_an_idle_session_idle() {
    let (state, dir) = app_state("server-helper-agent");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    state
        .store
        .insert_session(&crate::test_support::console_session(
            "s",
            SessionStatus::Idle,
        ))
        .unwrap();
    for event in ["PreToolUse", "PostToolUse", "PostToolBatch"] {
        let body =
            format!(r#"{{"hook_event_name": "{event}", "agent_id": "h1", "tool_name": "Read"}}"#);
        apply_hook(&state, "s", body.as_bytes());
    }
    let session = state.store.get_session("s").unwrap().unwrap();
    assert_eq!(session.status, SessionStatus::Idle);
}

/// Subagent events keep the task list while the session stays idle; the handoff between two
/// subagents never publishes it empty, and the end of the last one does once it has stood.
#[tokio::test(start_paused = true)]
async fn subagent_events_keep_the_tasks_of_an_idle_session() {
    let (state, dir) = app_state("server-subagent-tasks");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    state
        .store
        .insert_session(&crate::test_support::console_session(
            "s",
            SessionStatus::Idle,
        ))
        .unwrap();
    let mut events = state.subscribe();
    let hook = |body: &str| apply_hook(&state, "s", body.as_bytes());
    let published = |events: &mut tokio::sync::broadcast::Receiver<Event>| {
        let mut out = Vec::new();
        while let Ok(Event::SessionUpserted { session }) = events.try_recv() {
            out.push((session.status, session.background_tasks.len()));
        }
        out
    };

    hook(
        r#"{"hook_event_name": "SubagentStart", "agent_id": "a", "agent_type": "general-purpose"}"#,
    );
    hook(
        r#"{"hook_event_name": "SubagentStart", "agent_id": "b", "agent_type": "general-purpose"}"#,
    );
    assert_eq!(
        published(&mut events),
        [(SessionStatus::Idle, 1), (SessionStatus::Idle, 2)]
    );
    // The helper agent is not work, and its stop says nothing.
    hook(r#"{"hook_event_name": "SubagentStop", "agent_id": "h", "background_tasks": []}"#);
    let stop_a = r#"{"hook_event_name": "SubagentStop", "agent_id": "a", "agent_type": "general-purpose", "background_tasks": [{"id": "a", "type": "subagent", "description": "probe A"}, {"id": "b", "type": "subagent", "description": "probe B"}]}"#;
    hook(stop_a);
    assert_eq!(published(&mut events), [(SessionStatus::Idle, 1)]);

    // b stops, a starts again in the same instant: no empty list in between.
    hook(
        r#"{"hook_event_name": "SubagentStop", "agent_id": "b", "agent_type": "general-purpose", "background_tasks": [{"id": "b", "type": "subagent"}]}"#,
    );
    hook(
        r#"{"hook_event_name": "SubagentStart", "agent_id": "a", "agent_type": "general-purpose"}"#,
    );
    tokio::time::sleep(crate::state::EMPTY_TASKS_DELAY * 2).await;
    let seen = published(&mut events);
    assert!(!seen.contains(&(SessionStatus::Idle, 0)), "{seen:?}");
    let tasks = state.background_tasks_of("s");
    // b had stopped with nothing else running, so only a is left.
    assert_eq!(tasks.len(), 1);
    // a was named by an earlier stop, and comes back with its description.
    let a = tasks.iter().find(|task| task.id == "a").unwrap();
    assert_eq!(a.description.as_deref(), Some("probe A"));

    // A `Stop` within the wait takes the emptying back: its own list is the truth.
    hook(
        r#"{"hook_event_name": "SubagentStop", "agent_id": "a", "agent_type": "general-purpose", "background_tasks": [{"id": "a", "type": "subagent"}]}"#,
    );
    hook(r#"{"hook_event_name": "Stop", "background_tasks": [{"id": "S", "type": "shell"}]}"#);
    tokio::time::sleep(crate::state::EMPTY_TASKS_DELAY * 2).await;
    let tasks = state.background_tasks_of("s");
    assert_eq!(
        tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        ["S"]
    );

    // The last subagent stops with nothing else running: the list is emptied once the stop has
    // stood.
    hook(r#"{"hook_event_name": "Stop", "background_tasks": []}"#);
    hook(
        r#"{"hook_event_name": "SubagentStart", "agent_id": "a", "agent_type": "general-purpose"}"#,
    );
    hook(
        r#"{"hook_event_name": "SubagentStop", "agent_id": "a", "agent_type": "general-purpose", "background_tasks": [{"id": "a", "type": "subagent"}]}"#,
    );
    assert!(!state.background_tasks_of("s").is_empty());
    tokio::time::sleep(crate::state::EMPTY_TASKS_DELAY * 2).await;
    assert!(state.background_tasks_of("s").is_empty());
    assert_eq!(
        published(&mut events).last(),
        Some(&(SessionStatus::Idle, 0))
    );
    assert_eq!(
        state.store.get_session("s").unwrap().unwrap().status,
        SessionStatus::Idle
    );
}

/// Inside a turn a subagent's start and stop leave the list alone: the turn's own `Stop` lists
/// everything, and a turn cancelled without one would leave a foreground subagent behind.
#[tokio::test(start_paused = true)]
async fn subagent_events_inside_a_turn_leave_the_tasks_alone() {
    let (state, dir) = app_state("server-subagent-in-turn");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    state
        .store
        .insert_session(&crate::test_support::console_session(
            "s",
            SessionStatus::Working,
        ))
        .unwrap();
    apply_hook(
        &state,
        "s",
        br#"{"hook_event_name": "SubagentStart", "agent_id": "a", "agent_type": "general-purpose"}"#,
    );
    assert!(state.background_tasks_of("s").is_empty());
    apply_hook(
        &state,
        "s",
        br#"{"hook_event_name": "SubagentStop", "agent_id": "a", "agent_type": "general-purpose", "background_tasks": [{"id": "a", "type": "subagent"}, {"id": "b", "type": "shell"}]}"#,
    );
    assert!(state.background_tasks_of("s").is_empty());
}

/// How each agent's turn ends sets and clears the paused flag across turns: an abnormal end sets
/// it (and publishes it, also for a session already idle), the next prompt clears it, and a
/// normal stop clears it and publishes that too.
#[tokio::test]
async fn an_abnormal_turn_end_pauses_the_session_until_the_next_turn_or_a_normal_stop() {
    for (agent, abnormal, normal, prompt) in [
        (
            Agent::Claude,
            r#"{"hook_event_name": "StopFailure"}"#,
            r#"{"hook_event_name": "Stop"}"#,
            r#"{"hook_event_name": "UserPromptSubmit"}"#,
        ),
        (
            Agent::Claude,
            r#"{"hook_event_name": "StopFailure"}"#,
            r#"{"hook_event_name": "Stop", "background_tasks": [{"id": "b1", "type": "shell"}]}"#,
            r#"{"hook_event_name": "UserPromptSubmit"}"#,
        ),
        (
            Agent::Codex,
            r#"{"hook_event_name": "Interrupt"}"#,
            r#"{"hook_event_name": "Stop"}"#,
            r#"{"hook_event_name": "UserPromptSubmit"}"#,
        ),
        (
            Agent::Grok,
            r#"{"hookEventName": "StopCancelled"}"#,
            r#"{"hookEventName": "Stop", "reason": "end_turn"}"#,
            r#"{"hookEventName": "UserPromptSubmit"}"#,
        ),
        (
            Agent::Grok,
            r#"{"hookEventName": "StopFailure"}"#,
            r#"{"hookEventName": "Stop", "reason": "end_turn"}"#,
            r#"{"hookEventName": "UserPromptSubmit"}"#,
        ),
    ] {
        let (state, dir) = app_state("hook-paused");
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        state
            .store
            .insert_session(&crate::test_support::console_session(
                "s",
                SessionStatus::Idle,
            ))
            .unwrap();
        let live = crate::test_support::fake_live_session("s", agent, 80, 24, "sleep 30");
        state.register_live(live.clone());
        let mut events = state.subscribe();
        let last_paused = |events: &mut tokio::sync::broadcast::Receiver<Event>| {
            let mut last = None;
            while let Ok(event) = events.try_recv() {
                if let Event::SessionUpserted { session } = event {
                    last = Some((session.status, session.paused));
                }
            }
            last
        };
        let label = format!("{agent:?} {abnormal}");

        // Ended abnormally while the session was working: the one publish carries the flag.
        apply_hook(&state, "s", prompt.as_bytes());
        assert_eq!(
            last_paused(&mut events),
            Some((SessionStatus::Working, false))
        );
        apply_hook(&state, "s", abnormal.as_bytes());
        assert_eq!(
            last_paused(&mut events),
            Some((SessionStatus::Idle, true)),
            "{label}"
        );

        // The next prompt ends it.
        apply_hook(&state, "s", prompt.as_bytes());
        assert_eq!(
            last_paused(&mut events),
            Some((SessionStatus::Working, false)),
            "{label}"
        );
        assert!(!state.last_turn_abnormal("s"));

        // Ended abnormally again, then normally: the second end is published although the status
        // stays idle.
        apply_hook(&state, "s", abnormal.as_bytes());
        assert_eq!(
            last_paused(&mut events),
            Some((SessionStatus::Idle, true)),
            "{label}"
        );
        apply_hook(&state, "s", normal.as_bytes());
        assert_eq!(
            last_paused(&mut events),
            Some((SessionStatus::Idle, false)),
            "{label}"
        );

        // An abnormal end of a session that is idle already is published too.
        apply_hook(&state, "s", abnormal.as_bytes());
        assert_eq!(
            last_paused(&mut events),
            Some((SessionStatus::Idle, true)),
            "{label}"
        );
        assert!(state.last_turn_abnormal("s"));
    }
}

/// An event that ends no turn leaves the flag as it is: Claude Code's `idle_prompt` follows a
/// failed stop, and Grok's backstop and teardown fire after turns that already ended.
#[tokio::test]
async fn events_that_end_no_turn_leave_the_paused_flag_alone() {
    for (agent, ends_abnormally, silent) in [
        (
            Agent::Claude,
            r#"{"hook_event_name": "StopFailure"}"#,
            r#"{"hook_event_name": "Notification", "notification_type": "idle_prompt"}"#,
        ),
        (
            Agent::Grok,
            r#"{"hookEventName": "StopCancelled"}"#,
            r#"{"hookEventName": "Notification", "notificationType": "idle_prompt"}"#,
        ),
        (
            Agent::Grok,
            r#"{"hookEventName": "StopCancelled"}"#,
            r#"{"hookEventName": "Stop", "reason": "shutdown"}"#,
        ),
    ] {
        let (state, dir) = app_state("hook-paused-silent");
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        state
            .store
            .insert_session(&crate::test_support::console_session(
                "s",
                SessionStatus::Working,
            ))
            .unwrap();
        let live = crate::test_support::fake_live_session("s", agent, 80, 24, "sleep 30");
        state.register_live(live.clone());
        apply_hook(&state, "s", ends_abnormally.as_bytes());
        assert!(state.last_turn_abnormal("s"));
        apply_hook(&state, "s", silent.as_bytes());
        assert!(state.last_turn_abnormal("s"), "{agent:?} {silent}");
        assert!(
            state
                .wire_session(state.session_record("s").unwrap())
                .paused
        );
    }
}

/// A Claude Code `SessionStart` that begins a new conversation (after `/clear`) has no stopped
/// turn, so it clears the paused flag and publishes that for a session that is idle already; one
/// that resumes or compacts the conversation leaves it.
#[tokio::test]
async fn a_new_claude_conversation_clears_the_paused_flag() {
    let (state, dir) = app_state("hook-paused-session-start");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    state
        .store
        .insert_session(&crate::test_support::console_session(
            "s",
            SessionStatus::Working,
        ))
        .unwrap();
    let live = crate::test_support::fake_live_session("s", Agent::Claude, 80, 24, "sleep 30");
    state.register_live(live.clone());
    apply_hook(&state, "s", br#"{"hook_event_name": "StopFailure"}"#);
    assert!(state.last_turn_abnormal("s"));

    // A resume or a compaction carries the conversation on, so the turn cut short stays the last.
    for source in ["resume", "compact"] {
        let start = format!(r#"{{"hook_event_name": "SessionStart", "source": "{source}"}}"#);
        apply_hook(&state, "s", start.as_bytes());
        assert!(state.last_turn_abnormal("s"), "{source}");
    }

    let mut events = state.subscribe();
    apply_hook(
        &state,
        "s",
        br#"{"hook_event_name": "SessionStart", "source": "clear"}"#,
    );
    assert!(!state.last_turn_abnormal("s"));
    let mut last = None;
    while let Ok(event) = events.try_recv() {
        if let Event::SessionUpserted { session } = event {
            last = Some((session.status, session.paused));
        }
    }
    assert_eq!(last, Some((SessionStatus::Idle, false)));
}
