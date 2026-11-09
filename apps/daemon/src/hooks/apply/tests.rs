use std::sync::Arc;

use super::*;
use crate::messaging::outbox::Drain;
use crate::protocol::{Event, Role};
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

/// Only Claude Code's `<task-notification>` prompt counts as a completion notice; the same prompt
/// to another agent's session counts for nothing.
#[tokio::test]
async fn only_claude_codes_task_notification_prompt_counts_as_a_completion_notice() {
    let prompt = br#"{"hook_event_name": "UserPromptSubmit", "prompt": "<task-notification><task-id>b1</task-id></task-notification>"}"#;
    for (agent, counted) in [
        (AgentId::CLAUDE, 1),
        (AgentId::CODEX, 0),
        (AgentId::TEST, 0),
    ] {
        let (state, dir) = app_state("server-completion-notice-count");
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        let mut session = crate::test_support::console_session("s", SessionStatus::Idle);
        session.agent = agent;
        state.store.insert_session(&session).unwrap();
        apply_hook(&state, "s", prompt);
        assert_eq!(state.completion_notices("s"), counted, "{agent:?}");
        // The prompt itself was processed as a conversation start for every agent.
        assert!(
            state
                .store
                .get_session("s")
                .unwrap()
                .unwrap()
                .has_conversation,
            "{agent:?}"
        );
    }
}

/// A Grok session that stopped on an error holds what is queued for it, and a prompt sent ends
/// the hold; the other agents' failures hold nothing.
#[tokio::test]
async fn only_a_grok_error_holds_writes_and_a_prompt_ends_the_hold() {
    for (agent, error, held) in [
        (AgentId::GROK, r#"{"hookEventName": "StopFailure"}"#, true),
        (
            AgentId::GROK,
            r#"{"hook_event_name": "Notification", "notification_type": "agent_error"}"#,
            true,
        ),
        (
            AgentId::GROK,
            r#"{"hook_event_name": "Notification", "notification_type": "idle_prompt"}"#,
            false,
        ),
        (
            AgentId::CLAUDE,
            r#"{"hook_event_name": "StopFailure"}"#,
            false,
        ),
        (
            AgentId::CODEX,
            r#"{"hook_event_name": "StopFailure"}"#,
            false,
        ),
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
    let live = crate::test_support::fake_live_session("s", AgentId::GROK, 80, 24, "sleep 30");
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
    let live = crate::test_support::fake_live_session("s", AgentId::GROK, 80, 24, "sleep 30");
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
        crate::test_support::within(PATIENCE, move || {
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
    let live = crate::test_support::fake_live_session("s", AgentId::GROK, 80, 24, "sleep 30");
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
        crate::test_support::within(PATIENCE, move || status(&state) == SessionStatus::Idle).await
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

/// Whether the session reaches `status` within [`PATIENCE`], polled from a blocking thread on the
/// real clock, so not from a test on tokio's paused clock.
async fn reaches_on_real_clock(state: &Arc<AppState>, status: SessionStatus) -> bool {
    let state = state.clone();
    crate::test_support::within(PATIENCE, move || {
        state.store.get_session("s").unwrap().unwrap().status == status
    })
    .await
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

/// A subagent resumed with `SendMessage` hands its report back as a prompt the agent sends
/// itself, the way Claude Code words it, which leaves whatever waits in its input box there as a
/// completion notice does, so it does not lift the hold either.
#[tokio::test]
async fn a_subagents_hand_back_does_not_lift_the_hold() {
    let (state, _dir, live, path) = checked_claude_session("server-unsent-hand-back");
    let first = crate::messaging::delivery::write_message(
        &state,
        "s",
        "first message",
        crate::messaging::delivery::WhenBlocked::Queue,
    )
    .unwrap();
    assert_eq!(first, crate::messaging::delivery::Delivery::Unsent);

    let hand_back = serde_json::json!({
        "hook_event_name": "UserPromptSubmit",
        "prompt": "Another Claude session sent a message:\n<agent-message from=\"afb2497118a985e98\">\n\
                   [Subagent hand-back] The report follows:\n  pass\n</agent-message>",
        "transcript_path": path,
    });
    apply_hook(&state, "s", hand_back.to_string().as_bytes());
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
    let worker = crate::test_support::bare_session("w", crate::protocol::SessionKind::Project);
    state
        .store
        .insert_session(&crate::protocol::Session {
            bound_to: Some("s".to_string()),
            role: Role::Worker,
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
    // The owner is written to off the hook's path, on blocking tasks this waits out; the marker
    // then shows whether one of them wrote a second notice.
    crate::test_support::runtime_settled().await;
    let behind_marker = {
        let (state, owner) = (state.clone(), (*owner).clone());
        tokio::task::spawn_blocking(move || crate::test_support::heard_after_marker(&state, &owner))
            .await
            .unwrap()
    };
    assert_eq!(behind_marker.matches("is waiting for the user").count(), 1);

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

    assert!(reaches_on_real_clock(&state, SessionStatus::Idle).await);
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

    assert!(reaches_on_real_clock(&state, SessionStatus::Idle).await);
}

/// An interrupt the watch has not read yet when the next prompt arrives belongs to the turn
/// before, and must not end the new one; the new turn's own interrupt still does, though the
/// session never left `Working` between the two.
#[tokio::test(start_paused = true)]
async fn only_the_next_turns_own_interrupt_ends_it() {
    let (state, _dir, _live, path) =
        claude_session_with_transcript("server-interrupt-next-turn", SessionStatus::Idle);
    let status = || state.store.get_session("s").unwrap().unwrap().status;
    apply_hook(&state, "s", &claude_hook("UserPromptSubmit", &path));
    std::fs::write(&path, crate::test_support::fresh_interrupt()).unwrap();
    // Before the first watch's first poll: the paused clock does not move until the test awaits.
    apply_hook(&state, "s", &claude_hook("UserPromptSubmit", &path));

    crate::adapter::claude::transcript_watch::polls_ran().await;
    assert_eq!(
        status(),
        SessionStatus::Working,
        "the next turn must keep working"
    );

    std::fs::write(&path, crate::test_support::fresh_interrupt().repeat(2)).unwrap();
    crate::adapter::claude::transcript_watch::polls_ran().await;
    assert_eq!(status(), SessionStatus::Idle);
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
            AgentId::CLAUDE,
            r#"{"hook_event_name": "StopFailure"}"#,
            r#"{"hook_event_name": "Stop"}"#,
            r#"{"hook_event_name": "UserPromptSubmit"}"#,
        ),
        (
            AgentId::CLAUDE,
            r#"{"hook_event_name": "StopFailure"}"#,
            r#"{"hook_event_name": "Stop", "background_tasks": [{"id": "b1", "type": "shell"}]}"#,
            r#"{"hook_event_name": "UserPromptSubmit"}"#,
        ),
        (
            AgentId::CODEX,
            r#"{"hook_event_name": "Interrupt"}"#,
            r#"{"hook_event_name": "Stop"}"#,
            r#"{"hook_event_name": "UserPromptSubmit"}"#,
        ),
        (
            AgentId::GROK,
            r#"{"hookEventName": "StopCancelled"}"#,
            r#"{"hookEventName": "Stop", "reason": "end_turn"}"#,
            r#"{"hookEventName": "UserPromptSubmit"}"#,
        ),
        (
            AgentId::GROK,
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
            AgentId::CLAUDE,
            r#"{"hook_event_name": "StopFailure"}"#,
            r#"{"hook_event_name": "Notification", "notification_type": "idle_prompt"}"#,
        ),
        (
            AgentId::GROK,
            r#"{"hookEventName": "StopCancelled"}"#,
            r#"{"hookEventName": "Notification", "notificationType": "idle_prompt"}"#,
        ),
        (
            AgentId::GROK,
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
    let live = crate::test_support::fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
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

/// A running session of `agent` in the middle of the turn `prompt` opens, as its hooks report
/// it, with its scratch directory and its stand-in process.
fn session_in_turn(
    label: &str,
    agent: AgentId,
    prompt: &str,
) -> (
    Arc<AppState>,
    crate::test_support::ScratchDir,
    crate::test_support::StandIn,
) {
    let (state, dir) = app_state(label);
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    state
        .store
        .insert_session(&crate::protocol::Session {
            agent,
            ..crate::test_support::console_session("s", SessionStatus::Idle)
        })
        .unwrap();
    let live = crate::test_support::fake_live_session("s", agent, 80, 24, "sleep 30");
    state.register_live(live.clone());
    apply_hook(&state, "s", prompt.as_bytes());
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Working
    );
    (state, dir, live)
}

/// The process ending as its exit watcher records it, cutting the turn short.
fn end_process(state: &Arc<AppState>) {
    exit_at(state, crate::time::now_millis());
    assert!(state.last_turn_abnormal("s"), "the end cut the turn short");
}

/// The exit watcher's record of the process ending at `ended_at`: the end written, then the live
/// entry gone.
fn exit_at(state: &Arc<AppState>, ended_at: i64) {
    state
        .end_process_record("s", |own| {
            state.store.record_session_exit("s", ended_at, own)
        })
        .unwrap()
        .expect("ended");
    crate::coordinator::exit_watch::end_process_for_test(state, "s");
}

/// Each agent's events of one turn, by the id it gives a turn: a prompt opening `t1`, a normal
/// end of `t1` and of `t0`, and an abnormal end of `t1` where the agent reports one.
fn late_turn_events(agent: AgentId) -> [String; 4] {
    match agent {
        AgentId::CLAUDE => [
            r#"{"hook_event_name": "UserPromptSubmit", "prompt_id": "t1"}"#.to_string(),
            r#"{"hook_event_name": "Stop", "prompt_id": "t1"}"#.to_string(),
            r#"{"hook_event_name": "Stop", "prompt_id": "t0"}"#.to_string(),
            r#"{"hook_event_name": "StopFailure", "prompt_id": "t1"}"#.to_string(),
        ],
        AgentId::CODEX => [
            r#"{"hook_event_name": "UserPromptSubmit", "turn_id": "t1"}"#.to_string(),
            r#"{"hook_event_name": "Stop", "turn_id": "t1"}"#.to_string(),
            r#"{"hook_event_name": "Stop", "turn_id": "t0"}"#.to_string(),
            r#"{"hook_event_name": "Interrupt", "turn_id": "t1"}"#.to_string(),
        ],
        AgentId::GROK => [
            r#"{"hookEventName": "UserPromptSubmit", "promptId": "t1"}"#.to_string(),
            r#"{"hookEventName": "Stop", "reason": "end_turn", "promptId": "t1"}"#.to_string(),
            r#"{"hookEventName": "Stop", "reason": "end_turn", "promptId": "t0"}"#.to_string(),
            r#"{"hookEventName": "StopCancelled", "promptId": "t1"}"#.to_string(),
        ],
        other => panic!("no fixture for {other}"),
    }
}

/// A normal end of the turn the process ended in, posted after the end, clears the flag the end
/// set; an end of another turn, or the turn's own abnormal end, leaves it.
#[tokio::test]
async fn a_late_normal_end_of_the_turn_cut_short_clears_the_paused_flag() {
    for agent in [AgentId::CLAUDE, AgentId::CODEX, AgentId::GROK] {
        let [prompt, stop, other_stop, abnormal] = late_turn_events(agent);

        let (state, _dir, _live) = session_in_turn("hook-late-stop", agent, &prompt);
        end_process(&state);
        apply_hook(&state, "s", stop.as_bytes());
        let record = state.session_record("s").unwrap();
        assert_eq!(record.status, SessionStatus::Interrupted, "{agent:?}");
        assert!(
            !record.paused,
            "{agent:?}: the turn ended normally after all"
        );

        let (state, _dir, _live) = session_in_turn("hook-late-other-stop", agent, &prompt);
        end_process(&state);
        apply_hook(&state, "s", other_stop.as_bytes());
        assert!(
            state.last_turn_abnormal("s"),
            "{agent:?}: another turn's end"
        );

        let (state, _dir, _live) = session_in_turn("hook-late-abnormal", agent, &prompt);
        end_process(&state);
        apply_hook(&state, "s", abnormal.as_bytes());
        assert!(state.last_turn_abnormal("s"), "{agent:?}: an abnormal end");
        // The abnormal end settled the turn, so nothing of it is taken as its end any more.
        apply_hook(&state, "s", stop.as_bytes());
        assert!(state.last_turn_abnormal("s"), "{agent:?}: settled already");
    }
}

/// A late end of the turn cut short that arrives once the session is running again belongs to
/// the process that ended: it neither clears the flag nor moves the new process, however often
/// it is posted.
#[tokio::test]
async fn a_late_end_of_the_turn_cut_short_leaves_a_resumed_session_alone() {
    for agent in [AgentId::CLAUDE, AgentId::CODEX, AgentId::GROK] {
        let [prompt, stop, ..] = late_turn_events(agent);
        let (state, _dir, _old) = session_in_turn("hook-late-stop-resumed", agent, &prompt);
        end_process(&state);
        state
            .store
            .update_session_lifecycle_if("s", SessionStatus::Interrupted, SessionStatus::Idle, None)
            .unwrap()
            .expect("relaunched");
        let live = crate::test_support::fake_live_session("s", agent, 80, 24, "sleep 30");
        state.register_live(live.clone());

        apply_hook(&state, "s", stop.as_bytes());
        let record = state.session_record("s").unwrap();
        assert_eq!(record.status, SessionStatus::Idle, "{agent:?}");
        assert!(
            record.paused,
            "{agent:?}: the resumed session is still paused"
        );

        // A new turn of the new process is not ended by the old turn's end either, one posted
        // again included.
        let next = prompt.replace("t1", "t2");
        apply_hook(&state, "s", next.as_bytes());
        apply_hook(&state, "s", stop.as_bytes());
        assert_eq!(
            state.session_record("s").unwrap().status,
            SessionStatus::Working,
            "{agent:?}"
        );
    }
}

/// An archive ends the turn before the process is gone, and the exit recorded afterwards is the
/// same end: a late normal end of the turn still clears the flag on the archived record.
#[tokio::test]
async fn a_late_normal_end_of_a_turn_an_archive_cut_short_clears_the_paused_flag() {
    let [prompt, stop, ..] = late_turn_events(AgentId::CODEX);
    let (state, _dir, _live) = session_in_turn("hook-late-stop-archived", AgentId::CODEX, &prompt);
    state.archive_session_record("s").unwrap();
    assert!(state.last_turn_abnormal("s"));
    // The exit recorded afterwards writes an `ended_at` of its own.
    let archived_at = state.session_record("s").unwrap().ended_at.unwrap();
    exit_at(&state, archived_at + 1_000);

    apply_hook(&state, "s", stop.as_bytes());
    let record = state.session_record("s").unwrap();
    assert_eq!(record.status, SessionStatus::Archived);
    assert_eq!(record.ended_at, Some(archived_at + 1_000));
    assert!(!record.paused);
}

/// A request's hand held over a working agent remembers it working, but the exit recorded after
/// an archive is the archive's own end: it does not set again the flag a late normal end of the
/// turn cleared.
#[tokio::test]
async fn the_exit_after_an_archive_does_not_set_again_a_flag_a_late_end_cleared() {
    let [prompt, stop, ..] = late_turn_events(AgentId::CODEX);
    let (state, _dir, _live) = session_in_turn("hook-late-stop-hand", AgentId::CODEX, &prompt);
    state.raise_request_hand("s").unwrap();
    state.archive_session_record("s").unwrap();
    assert!(state.last_turn_abnormal("s"));

    apply_hook(&state, "s", stop.as_bytes());
    assert!(!state.last_turn_abnormal("s"));
    let archived_at = state.session_record("s").unwrap().ended_at.unwrap();
    exit_at(&state, archived_at + 1_000);
    let record = state.session_record("s").unwrap();
    assert_eq!(record.status, SessionStatus::Archived);
    assert!(
        !record.paused,
        "the hand still says working, but the turn has ended"
    );
}

/// A server of the session's own that counts the starts of its queue.
struct CountingQueue(std::sync::atomic::AtomicUsize);

impl crate::session::term::MessageTransport for CountingQueue {
    fn send(
        &self,
        _: &str,
        _: &str,
    ) -> std::result::Result<crate::session::term::Sent, crate::platform::term::PartialWrite> {
        unreachable!("nothing is delivered here")
    }
    fn attach(&self, _: crate::session::term::TransportEvents) {}
    fn start_queue(&self) -> crate::session::term::QueueStart {
        self.0.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        crate::session::term::QueueStart::Started(1)
    }
    fn stop(&self) {}
}

/// A turn the user cancels in Codex's terminal leaves what Codex's own queue took of Octoboard's
/// waiting there, so the queue is started for it; a turn that ends normally runs it by itself.
#[tokio::test]
async fn a_cancelled_codex_turn_starts_what_octoboard_left_in_its_queue() {
    let (state, dir) = app_state("hook-cancel-queue");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    state
        .store
        .insert_session(&crate::protocol::Session {
            agent: AgentId::CODEX,
            ..crate::test_support::console_session("s", SessionStatus::Idle)
        })
        .unwrap();
    let queue = Arc::new(CountingQueue(Default::default()));
    let live =
        crate::test_support::idle_stand_in_of("s", AgentId::CODEX).with_transport(queue.clone());
    state.register_live(live.clone());
    apply_hook(&state, "s", br#"{"hook_event_name": "UserPromptSubmit"}"#);
    apply_hook(&state, "s", br#"{"hook_event_name": "Stop"}"#);
    apply_hook(&state, "s", br#"{"hook_event_name": "UserPromptSubmit"}"#);
    apply_hook(&state, "s", br#"{"hook_event_name": "Interrupt"}"#);
    let deadline = std::time::Instant::now() + PATIENCE;
    while queue.0.load(std::sync::atomic::Ordering::Acquire) == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "the queue was not started"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(queue.0.load(std::sync::atomic::Ordering::Acquire), 1);
}
