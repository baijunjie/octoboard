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
    let _settle = crate::session::title_interrupt::without_settle();
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
    crate::session::title_interrupt::polls_settled().await;
    apply_hook(
        &state,
        "s",
        br#"{"hook_event_name": "PostToolUse", "toolName": "ask_user_question"}"#,
    );
    assert_eq!(status(&state), SessionStatus::Working);
    crate::session::title_interrupt::polls_settled().await;
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

/// [`claude_hook`] for the turn `prompt_id` names, as every turn-scoped Claude Code event carries.
fn claude_turn_hook(event: &str, transcript: &std::path::Path, prompt_id: &str) -> Vec<u8> {
    let mut hook: serde_json::Value =
        serde_json::from_slice(&claude_hook(event, transcript)).unwrap();
    hook["prompt_id"] = serde_json::json!(prompt_id);
    hook.to_string().into_bytes()
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

/// A subagent's hand-back (a background one's, or one resumed with `SendMessage`) is a completion
/// notice, the way Claude Code words it, which leaves whatever waits in its input box there, so it
/// does not lift the hold either.
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

/// Esc pressed while a message the user typed waits in Claude Code's queue cancels the turn and
/// runs the message as the next one (2.1.322). The message's `UserPromptSubmit` comes when it is
/// queued, naming the running turn; nothing comes when it is taken, and the cancelled turn's
/// interrupt names that same turn. The session keeps working through the next turn, and its `Stop`
/// leaves it idle, not paused.
#[tokio::test(start_paused = true)]
async fn a_cancel_claude_carries_on_from_with_a_queued_message_keeps_the_session_working() {
    use crate::test_support::{queued_prompt_after_interrupt, CAPTURED_TURN, QUEUED_TURN};
    let (state, _dir, _live, path) =
        claude_session_with_transcript("server-interrupt-queued", SessionStatus::Idle);
    let status = || state.store.get_session("s").unwrap().unwrap().status;
    for _ in 0..2 {
        apply_hook(
            &state,
            "s",
            &claude_turn_hook("UserPromptSubmit", &path, CAPTURED_TURN),
        );
    }
    std::fs::write(&path, queued_prompt_after_interrupt(QUEUED_TURN)).unwrap();

    crate::adapter::claude::transcript_watch::polls_ran().await;
    assert_eq!(
        status(),
        SessionStatus::Working,
        "the queued turn must keep working"
    );
    assert_eq!(state.turn_ends("s"), 1, "the cancelled turn ended");

    apply_hook(&state, "s", &claude_turn_hook("Stop", &path, QUEUED_TURN));
    assert_eq!(status(), SessionStatus::Idle);
    assert!(!state.last_turn_abnormal("s"));
}

/// Claude Code's hook events of a turn whose one message started a background `Agent` call and
/// then asked an `AskUserQuestion`, in auto mode, where the agent call is let through only once
/// the question is up, so its `PostToolUse` comes while the question waits (2.1.319; see
/// `testdata/claude-hook-captures.md`). Each line is given the transcript at `transcript`.
fn question_beside_late_call(transcript: &std::path::Path) -> Vec<Vec<u8>> {
    include_str!("../../../testdata/claude_hooks_question_beside_late_call.jsonl")
        .lines()
        .map(|line| {
            let mut event: serde_json::Value = serde_json::from_str(line).unwrap();
            event["transcript_path"] = serde_json::json!(transcript);
            event.to_string().into_bytes()
        })
        .collect()
}

/// The end of another call of the same message does not lower a hand raised for a question that
/// still waits; the question's own end does, and the turn then ends as usual.
#[tokio::test]
async fn a_question_waits_through_the_end_of_another_call() {
    let (state, _dir, _live, path) =
        claude_session_with_transcript("server-question-late-call", SessionStatus::Idle);
    let events = question_beside_late_call(&path);
    let answered = events
        .iter()
        .position(|event| {
            let event = String::from_utf8_lossy(event);
            event.contains(r#""hook_event_name":"PostToolUse""#)
                && event.contains(r#""tool_name":"AskUserQuestion""#)
        })
        .expect("the capture answers the question");
    for event in &events[..answered] {
        apply_hook(&state, "s", event);
    }
    let session = state.store.get_session("s").unwrap().unwrap();
    assert_eq!(session.status, SessionStatus::WaitingUser);

    for event in &events[answered..] {
        apply_hook(&state, "s", event);
    }
    let session = state.store.get_session("s").unwrap().unwrap();
    assert_eq!(session.status, SessionStatus::Idle);
    assert!(!session.paused);
}

/// A Grok Build session at `status` with a live stand-in, which is returned to keep it alive.
fn grok_session(
    name: &str,
    status: SessionStatus,
) -> (
    Arc<AppState>,
    crate::test_support::ScratchDir,
    crate::test_support::StandIn,
) {
    let (state, dir) = app_state(name);
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    let mut session = crate::test_support::console_session("s", status);
    session.agent = AgentId::GROK;
    state.store.insert_session(&session).unwrap();
    let live = crate::test_support::idle_stand_in_of("s", AgentId::GROK);
    state.register_live(live.clone());
    (state, dir, live)
}

/// Grok Build's hook events of turns whose one response ran another call beside an
/// `ask_user_question`, the other call ending while the question waited (1.0.77; see
/// `testdata/grok-hook-captures.md`).
const GROK_QUESTION_BESIDE_OTHER_CALL: [(&str, &str); 2] = [
    (
        "question first, then a shell command",
        include_str!("../../../testdata/grok_hooks_question_first_beside_shell.jsonl"),
    ),
    (
        "background subagent, then the question",
        include_str!("../../../testdata/grok_hooks_background_subagent_beside_question.jsonl"),
    ),
];

/// From the question's start to its own end, nothing another call of the same response does —
/// its start, its end, the subagent it started — lowers the hand; the question's end does, and
/// the turn then ends as usual.
#[tokio::test]
async fn a_grok_question_waits_through_the_other_calls_of_its_response() {
    let is = |line: &str, event: &str| {
        let payload: serde_json::Value = serde_json::from_str(line).unwrap();
        payload["hook_event_name"] == event && payload["tool_name"] == "ask_user_question"
    };
    for (capture, lines) in GROK_QUESTION_BESIDE_OTHER_CALL {
        let (state, _dir, _live) =
            grok_session("server-grok-question-other-call", SessionStatus::Idle);
        let lines: Vec<&str> = lines.lines().collect();
        let asked = lines
            .iter()
            .position(|line| is(line, "PreToolUse"))
            .expect("the capture asks a question");
        let answered = lines
            .iter()
            .position(|line| is(line, "PostToolUse"))
            .expect("the capture answers the question");
        for line in &lines[..asked] {
            apply_hook(&state, "s", line.as_bytes());
        }
        for line in &lines[asked..answered] {
            apply_hook(&state, "s", line.as_bytes());
            assert_eq!(
                status_of(&state),
                SessionStatus::WaitingUser,
                "{capture}: after {line}"
            );
        }
        for line in &lines[answered..] {
            apply_hook(&state, "s", line.as_bytes());
        }
        let session = state.store.get_session("s").unwrap().unwrap();
        assert_eq!(session.status, SessionStatus::Idle, "{capture}");
        assert!(!session.paused, "{capture}");
    }
}

/// A Grok Build hook event about the tool call `id` of `tool`.
fn grok_call_hook(event: &str, id: &str, tool: &str) -> Vec<u8> {
    serde_json::json!({
        "hook_event_name": event,
        "toolName": tool,
        "toolUseId": id,
    })
    .to_string()
    .into_bytes()
}

/// Whichever event ends a Grok turn, or denies the question's own call, a hand raised for a
/// question that never ended comes down with it, and a later call's end is then an ordinary
/// move to work.
#[tokio::test]
async fn a_grok_turn_end_or_denial_lowers_the_hand_of_a_question_still_waiting() {
    for (event, id, expected) in [
        ("UserPromptSubmit", "", SessionStatus::Working),
        ("StopCancelled", "", SessionStatus::Idle),
        ("StopFailure", "", SessionStatus::Idle),
        ("Stop", "", SessionStatus::Idle),
        ("PermissionDenied", "q", SessionStatus::Working),
    ] {
        let (state, _dir, _live) =
            grok_session("server-grok-question-ended-by", SessionStatus::Idle);
        apply_hook(&state, "s", br#"{"hook_event_name": "UserPromptSubmit"}"#);
        apply_hook(
            &state,
            "s",
            &grok_call_hook("PreToolUse", "q", "ask_user_question"),
        );
        assert_eq!(status_of(&state), SessionStatus::WaitingUser, "{event}");

        apply_hook(&state, "s", &grok_call_hook(event, id, "ask_user_question"));
        assert_eq!(status_of(&state), expected, "{event}");

        // Nothing is waiting any more, so another call's end is work again.
        apply_hook(
            &state,
            "s",
            &grok_call_hook("PostToolUse", "t2", "run_terminal_command"),
        );
        assert_eq!(status_of(&state), SessionStatus::Working, "{event}");
    }
}

/// A Grok Build permission prompt names no call and holds back the other calls of its response,
/// so the next call to end lowers it, as before.
#[tokio::test]
async fn a_grok_permission_prompt_comes_down_with_the_next_call_to_end() {
    let (state, _dir, _live) = grok_session("server-grok-prompt-next-end", SessionStatus::Idle);
    apply_hook(&state, "s", br#"{"hook_event_name": "UserPromptSubmit"}"#);
    apply_hook(
        &state,
        "s",
        &grok_call_hook("PreToolUse", "t1", "run_terminal_command"),
    );
    apply_hook(
        &state,
        "s",
        br#"{"hook_event_name": "Notification", "notificationType": "permission_prompt"}"#,
    );
    assert_eq!(status_of(&state), SessionStatus::WaitingUser);

    apply_hook(
        &state,
        "s",
        &grok_call_hook("PostToolUse", "t1", "run_terminal_command"),
    );
    assert_eq!(status_of(&state), SessionStatus::Working);
}

/// Whichever way another call of the question's response ends or is stopped — a failure, a denial
/// — and whichever of its own events arrives, a waiting question keeps its hand up, and its own
/// failure then lowers it.
#[tokio::test]
async fn a_grok_question_waits_through_the_failure_or_denial_of_another_call() {
    let (state, _dir, _live) =
        grok_session("server-grok-question-failed-call", SessionStatus::Idle);
    let hook = |event, id, tool| apply_hook(&state, "s", &grok_call_hook(event, id, tool));
    hook("UserPromptSubmit", "", "");
    hook("PreToolUse", "q", "ask_user_question");
    assert_eq!(status_of(&state), SessionStatus::WaitingUser);

    for (event, id) in [
        ("PreToolUse", "t1"),
        ("PostToolUseFailure", "t1"),
        ("PermissionDenied", "t2"),
        ("PostToolUse", "t3"),
    ] {
        hook(event, id, "run_terminal_command");
        assert_eq!(
            status_of(&state),
            SessionStatus::WaitingUser,
            "{event} {id}"
        );
    }

    hook("PostToolUseFailure", "q", "ask_user_question");
    assert_eq!(status_of(&state), SessionStatus::Working);
}

/// A question's own end does not lower the hand while another question of the response still
/// waits; the last one's end does.
#[tokio::test]
async fn a_grok_hand_stays_up_until_every_waiting_question_has_ended() {
    let (state, _dir, _live) = grok_session("server-grok-two-questions", SessionStatus::Idle);
    let hook = |event, id| apply_hook(&state, "s", &grok_call_hook(event, id, "ask_user_question"));
    hook("UserPromptSubmit", "");
    hook("PreToolUse", "q1");
    hook("PreToolUse", "q2");
    assert_eq!(status_of(&state), SessionStatus::WaitingUser);

    hook("PostToolUse", "q1");
    assert_eq!(status_of(&state), SessionStatus::WaitingUser);

    hook("PostToolUse", "q2");
    assert_eq!(status_of(&state), SessionStatus::Working);
}

/// A subagent's events are ignored whole: one that carries the question's own call id neither
/// ends the call nor moves the session.
#[tokio::test]
async fn a_grok_subagents_events_do_not_end_a_waiting_question() {
    let (state, _dir, _live) = grok_session("server-grok-question-subagent", SessionStatus::Idle);
    let hook = |event, id| apply_hook(&state, "s", &grok_call_hook(event, id, "ask_user_question"));
    hook("UserPromptSubmit", "");
    hook("PreToolUse", "q");
    assert_eq!(status_of(&state), SessionStatus::WaitingUser);

    for event in ["PostToolUse", "PostToolUseFailure", "PermissionDenied"] {
        let body = serde_json::json!({
            "hook_event_name": event,
            "toolName": "ask_user_question",
            "toolUseId": "q",
            "subagentType": "general-purpose",
        })
        .to_string();
        apply_hook(&state, "s", body.as_bytes());
        assert_eq!(status_of(&state), SessionStatus::WaitingUser, "{event}");
    }

    // The session's own end of the question still lowers the hand.
    hook("PostToolUse", "q");
    assert_eq!(status_of(&state), SessionStatus::Working);
}

/// `idle_prompt` ends no call: the other call's end that follows it still leaves the question's
/// hand up, until the question ends.
#[tokio::test]
async fn a_grok_idle_prompt_does_not_end_a_waiting_question() {
    let (state, _dir, _live) =
        grok_session("server-grok-question-idle-prompt", SessionStatus::Idle);
    let hook = |event, id, tool| apply_hook(&state, "s", &grok_call_hook(event, id, tool));
    hook("UserPromptSubmit", "", "");
    hook("PreToolUse", "q", "ask_user_question");
    apply_hook(
        &state,
        "s",
        br#"{"hookEventName": "Notification", "notificationType": "idle_prompt"}"#,
    );

    hook("PostToolUse", "t1", "run_terminal_command");
    assert_eq!(status_of(&state), SessionStatus::WaitingUser);

    hook("PostToolUse", "q", "ask_user_question");
    assert_eq!(status_of(&state), SessionStatus::Working);
}

/// Grok Build's `Stop` of a turn that left a shell command and a subagent running.
const GROK_STOP_WITH_WORK: &str = r#"{"hookEventName": "Stop", "reason": "end_turn", "backgroundTasks": [
    {"id": "sh1", "type": "shell", "status": "running", "command": "sleep 20"},
    {"id": "sub1", "type": "subagent", "status": "running", "description": "Sleep then reply OK", "agentType": "general-purpose"}]}"#;
const GROK_IDLE_PROMPT: &[u8] =
    br#"{"hookEventName": "Notification", "notificationType": "idle_prompt"}"#;

/// The ids of the work the session has listed, in order.
fn listed_task_ids(state: &Arc<AppState>) -> Vec<String> {
    state
        .background_tasks_of("s")
        .into_iter()
        .map(|task| task.id)
        .collect()
}

/// A Grok turn that ends with work still running is held on it: the session is idle with the work
/// listed and the turn open; the `idle_prompt` that follows does not close the turn or take the
/// list away; each finished piece wakes a turn of its own, whose `Stop` lists what is left, and
/// the one that lists nothing closes the turn and empties the list.
#[tokio::test]
async fn a_grok_turn_held_on_background_work_stays_open_until_the_work_is_done() {
    let (state, _dir, _live) = grok_session("server-grok-held-turn", SessionStatus::Idle);
    let hook = |body: &str| apply_hook(&state, "s", body.as_bytes());
    let prompt = |text: &str| {
        serde_json::json!({"hookEventName": "UserPromptSubmit", "prompt": text}).to_string()
    };

    hook(&prompt("run two things in the background"));
    assert_eq!(status_of(&state), SessionStatus::Working);
    assert!(state.turn_is_open("s"));

    // The turn ends with both running: idle, work listed, turn held open.
    hook(GROK_STOP_WITH_WORK);
    assert_eq!(status_of(&state), SessionStatus::Idle);
    assert_eq!(listed_task_ids(&state), ["sh1", "sub1"]);
    let listed = state.background_tasks_of("s");
    assert_eq!(listed[0].kind.as_deref(), Some("shell"));
    assert_eq!(listed[0].description.as_deref(), Some("sleep 20"));
    assert_eq!(listed[1].kind.as_deref(), Some("subagent"));
    assert_eq!(
        listed[1].description.as_deref(),
        Some("Sleep then reply OK")
    );
    assert!(state.turn_is_open("s"), "a held turn is not over");
    assert_eq!(
        state
            .wire_session(state.session_record("s").unwrap())
            .background_tasks
            .len(),
        2,
        "the record shows the session working in the background"
    );

    // The `idle_prompt` a minute later: neither the turn nor the list changes.
    state.quiet_past_backstop("s");
    apply_hook(&state, "s", GROK_IDLE_PROMPT);
    assert_eq!(status_of(&state), SessionStatus::Idle);
    assert_eq!(listed_task_ids(&state), ["sh1", "sub1"]);
    assert!(state.turn_is_open("s"));

    // The shell command finishes and wakes the session; its `Stop` lists the subagent.
    hook(&prompt(
        "<system-reminder>\nBackground task \"sh1\" completed (exit code: 0).\n</system-reminder>",
    ));
    assert_eq!(status_of(&state), SessionStatus::Working);
    assert_eq!(state.completion_notices("s"), 1);
    assert_eq!(listed_task_ids(&state), ["sub1"]);
    hook(
        r#"{"hookEventName": "Stop", "reason": "end_turn", "backgroundTasks": [{"id": "sub1", "type": "subagent", "description": "Sleep then reply OK"}]}"#,
    );
    assert_eq!(status_of(&state), SessionStatus::Idle);
    assert_eq!(listed_task_ids(&state), ["sub1"]);
    assert!(state.turn_is_open("s"));
    state.quiet_past_backstop("s");
    apply_hook(&state, "s", GROK_IDLE_PROMPT);
    assert!(state.turn_is_open("s"));

    // The subagent's own events never change the list.
    hook(
        r#"{"hookEventName": "SubagentStop", "subagentType": "general-purpose", "backgroundTasks": []}"#,
    );
    assert_eq!(listed_task_ids(&state), ["sub1"]);

    // The subagent finishes; the wake turn's `Stop` lists nothing and ends the turn.
    hook(&prompt(
        "<system-reminder>\nWhile you were idle, 1 background subagent completed:\n=== Task sub1 ===\nStatus: completed\n</system-reminder>",
    ));
    assert_eq!(status_of(&state), SessionStatus::Working);
    assert_eq!(state.completion_notices("s"), 2);
    hook(r#"{"hookEventName": "Stop", "reason": "end_turn", "backgroundTasks": []}"#);
    assert_eq!(status_of(&state), SessionStatus::Idle);
    assert!(state.background_tasks_of("s").is_empty());
    assert!(!state.turn_is_open("s"), "the turn ends with the last work");
}

/// The teardown `Stop` lists no work and leaves the list as the last per-turn `Stop` left it (the
/// process ending clears it).
#[tokio::test]
async fn a_grok_teardown_stop_leaves_the_listed_work_alone() {
    let (state, _dir, _live) = grok_session("server-grok-teardown-work", SessionStatus::Idle);
    apply_hook(&state, "s", GROK_STOP_WITH_WORK.as_bytes());
    assert_eq!(listed_task_ids(&state), ["sh1", "sub1"]);
    apply_hook(
        &state,
        "s",
        br#"{"hookEventName": "Stop", "reason": "shutdown"}"#,
    );
    assert_eq!(listed_task_ids(&state), ["sh1", "sub1"]);
}

/// A `Stop` that lists nothing after a held one takes the list away, and so does a failed turn.
#[tokio::test]
async fn a_grok_stop_without_work_or_a_failed_turn_empties_the_list() {
    for end in [
        r#"{"hookEventName": "Stop", "reason": "end_turn"}"#,
        r#"{"hookEventName": "StopFailure"}"#,
    ] {
        let (state, _dir, _live) = grok_session("server-grok-emptied-work", SessionStatus::Idle);
        apply_hook(&state, "s", GROK_STOP_WITH_WORK.as_bytes());
        assert_eq!(listed_task_ids(&state), ["sh1", "sub1"], "{end}");
        apply_hook(&state, "s", end.as_bytes());
        assert!(state.background_tasks_of("s").is_empty(), "{end}");
    }
}

/// A clock-attributed turn end is acted on only while no work is listed: with some listed and the
/// session at its prompt, `Stop` held the turn and the end is ignored (the caller reads nothing
/// else of the event), whether or not a turn is open and however long the turn has been quiet; with
/// none, it is read as before.
#[tokio::test]
async fn a_backstop_turn_end_is_ignored_while_background_work_is_listed() {
    let interpreter = crate::adapter::provider(AgentId::GROK).hooks();
    let backstop = serde_json::json!({"notificationType": "idle_prompt"});
    let read = |state: &Arc<AppState>| {
        read_turn_boundary(state, "s", interpreter, "Notification", &backstop)
    };

    // A turn open and held on work.
    let (state, _dir, _live) = grok_session("server-grok-backstop-held", SessionStatus::Idle);
    apply_hook(&state, "s", br#"{"hookEventName": "UserPromptSubmit"}"#);
    apply_hook(&state, "s", GROK_STOP_WITH_WORK.as_bytes());
    state.quiet_past_backstop("s");
    assert!(!read(&state));
    assert!(state.turn_is_open("s"));

    // The work listed and no turn open: still nothing to read in it.
    let (state, _dir, _live) = grok_session("server-grok-backstop-no-turn", SessionStatus::Idle);
    apply_hook(&state, "s", GROK_STOP_WITH_WORK.as_bytes());
    assert_eq!(status_of(&state), SessionStatus::Idle);
    assert!(!state.turn_is_open("s"));
    assert!(!read(&state));

    // With nothing listed, the backstop of a session with no turn open is believed, as before.
    let (state, _dir, _live) = grok_session("server-grok-backstop-free", SessionStatus::Idle);
    assert!(read(&state));
}

/// The quiet gate is what keeps a backstop from closing a turn that is still producing events; a
/// turn held on listed work has gone quiet past it, and the backstop still must not close it. The
/// control is the same turn with nothing listed, which the backstop does close: a closed turn is
/// the one that owes a report (`TurnClose::OwesReport`), and a turn left open owes none.
#[tokio::test]
async fn an_idle_prompt_after_the_quiet_gate_closes_a_turn_only_when_no_work_is_listed() {
    let prompt = br#"{"hookEventName": "UserPromptSubmit", "prompt": "go"}"#;

    // Held on work: the idle prompt arrives long after the `Stop`, and leaves everything as it was.
    let (state, _dir, _live) = grok_session("server-grok-aged-backstop-held", SessionStatus::Idle);
    apply_hook(&state, "s", prompt);
    apply_hook(&state, "s", GROK_STOP_WITH_WORK.as_bytes());
    let ends = state.turn_ends("s");
    state.quiet_past_backstop("s");
    apply_hook(&state, "s", GROK_IDLE_PROMPT);
    assert!(
        state.turn_is_open("s"),
        "the turn is still held on its work"
    );
    assert_eq!(state.turn_ends("s"), ends);
    assert_eq!(listed_task_ids(&state), ["sh1", "sub1"]);
    assert_eq!(status_of(&state), SessionStatus::Idle);
    // Its own end is still reportable: nothing closed it in between.
    assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);

    // Control: a turn left open with nothing listed, as long quiet, is closed by the same event.
    let (state, _dir, _live) = grok_session("server-grok-aged-backstop-free", SessionStatus::Idle);
    apply_hook(&state, "s", prompt);
    assert!(state.turn_is_open("s"));
    let ends = state.turn_ends("s");
    state.quiet_past_backstop("s");
    apply_hook(&state, "s", GROK_IDLE_PROMPT);
    assert!(!state.turn_is_open("s"), "the backstop closes a quiet turn");
    assert_eq!(state.turn_ends("s"), ends + 1);
    assert_eq!(status_of(&state), SessionStatus::Idle);
}

/// A prompt typed while the work is still listed joins the held turn, and when that turn reports
/// no `Stop` of its own the backstop is its only end: the session reads as working, not at its
/// prompt, so the listed work does not keep the backstop from closing the turn.
#[tokio::test]
async fn a_backstop_still_closes_a_turn_a_later_prompt_joined_while_work_is_listed() {
    let (state, _dir, _live) = grok_session("server-grok-backstop-joined", SessionStatus::Idle);
    apply_hook(
        &state,
        "s",
        br#"{"hookEventName": "UserPromptSubmit", "prompt": "go"}"#,
    );
    apply_hook(&state, "s", GROK_STOP_WITH_WORK.as_bytes());
    assert_eq!(status_of(&state), SessionStatus::Idle);
    apply_hook(
        &state,
        "s",
        br#"{"hookEventName": "UserPromptSubmit", "prompt": "and one more thing"}"#,
    );
    assert_eq!(status_of(&state), SessionStatus::Working);
    assert!(state.turn_is_open("s"));
    assert!(!state.background_tasks_of("s").is_empty());

    let ends = state.turn_ends("s");
    state.quiet_past_backstop("s");
    apply_hook(&state, "s", GROK_IDLE_PROMPT);
    assert!(!state.turn_is_open("s"), "the backstop closes the turn");
    assert_eq!(state.turn_ends("s"), ends + 1);
    assert_eq!(status_of(&state), SessionStatus::Idle);
}

/// Grok Build 1.0.77's hook events of a turn that started a shell command and a subagent and
/// ended, the work finishing one piece at a time (see `testdata/grok-hook-captures.md`).
const GROK_BACKGROUND_WORK_TURNS: &str =
    include_str!("../../../testdata/grok_hooks_background_work_turns.jsonl");

/// Replayed as captured: each per-turn `Stop` leaves the session idle with the work it lists, each
/// completion prompt wakes it, the subagent's own events change nothing, and the last `Stop` lists
/// nothing, which ends the turn with the list empty. The teardown `Stop` leaves that as it is.
#[tokio::test]
async fn a_captured_grok_run_with_background_work_lists_what_each_stop_says() {
    let (state, _dir, _live) = grok_session("server-grok-captured-work", SessionStatus::Idle);
    // The ids each of the session's own `Stop`s lists, in the order the capture has them.
    let mut stops = vec![
        vec![
            "ba739e73-d2e1-47cd-9e3c-b15e09208d66",
            "01a1d55a-d394-78a3-b398-801cdc722351",
        ],
        vec!["01a1d55a-d394-78a3-b398-801cdc722351"],
        vec![],
    ]
    .into_iter();
    let mut stops_seen = 0;
    for line in GROK_BACKGROUND_WORK_TURNS.lines() {
        let payload: serde_json::Value = serde_json::from_str(line).unwrap();
        let own = payload.get("subagentType").is_none();
        apply_hook(&state, "s", line.as_bytes());
        match payload["hook_event_name"].as_str().unwrap() {
            "UserPromptSubmit" if own => {
                assert_eq!(status_of(&state), SessionStatus::Working, "{line}");
            }
            "Stop" if payload["reason"] == "end_turn" => {
                stops_seen += 1;
                assert_eq!(status_of(&state), SessionStatus::Idle, "{line}");
                assert_eq!(listed_task_ids(&state), stops.next().unwrap(), "{line}");
                let held = !state.background_tasks_of("s").is_empty();
                assert_eq!(state.turn_is_open("s"), held, "{line}");
            }
            "SubagentStart" | "SubagentStop" | "UserPromptSubmit" | "Stop" => {}
            other => panic!("unexpected event {other}"),
        }
    }
    assert_eq!(stops_seen, 3);
    assert_eq!(status_of(&state), SessionStatus::Idle);
    assert!(state.background_tasks_of("s").is_empty());
    assert!(!state.turn_is_open("s"));
    assert_eq!(state.completion_notices("s"), 2);
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

/// A Claude Code hook event about the tool call `id` (none for a permission request, which names
/// no call), carrying `tool` and `input`.
fn claude_call_hook(
    event: &str,
    id: Option<&str>,
    tool: &str,
    input: serde_json::Value,
    transcript: &std::path::Path,
) -> Vec<u8> {
    let mut payload = serde_json::json!({
        "hook_event_name": event,
        "tool_name": tool,
        "tool_input": input,
        "transcript_path": transcript,
    });
    if let Some(id) = id {
        payload["tool_use_id"] = serde_json::json!(id);
    }
    payload.to_string().into_bytes()
}

fn status_of(state: &Arc<AppState>) -> SessionStatus {
    state.store.get_session("s").unwrap().unwrap().status
}

/// The end of another call does not take a session with a permission prompt up back to work; the
/// prompted call's own end does.
#[tokio::test]
async fn a_permission_prompt_waits_through_the_end_of_another_call() {
    let (state, _dir, _live, path) =
        claude_session_with_transcript("server-prompt-late-call", SessionStatus::Idle);
    let make = serde_json::json!({"command": "make"});
    let agent = serde_json::json!({"prompt": "go"});
    let hook = |event, id, tool, input| claude_call_hook(event, id, tool, input, &path);
    apply_hook(&state, "s", &claude_hook("UserPromptSubmit", &path));
    apply_hook(
        &state,
        "s",
        &hook("PreToolUse", Some("t1"), "Bash", make.clone()),
    );
    apply_hook(
        &state,
        "s",
        &hook("PreToolUse", Some("t2"), "Agent", agent.clone()),
    );
    apply_hook(
        &state,
        "s",
        &hook("PermissionRequest", None, "Bash", make.clone()),
    );
    assert_eq!(status_of(&state), SessionStatus::WaitingUser);

    apply_hook(
        &state,
        "s",
        &hook("PostToolUse", Some("t2"), "Agent", agent),
    );
    assert_eq!(status_of(&state), SessionStatus::WaitingUser);

    apply_hook(&state, "s", &hook("PostToolUse", Some("t1"), "Bash", make));
    assert_eq!(status_of(&state), SessionStatus::Working);
}

/// A failed call's end counts as an end as well.
#[tokio::test]
async fn a_failed_other_call_does_not_lower_a_waiting_questions_hand_either() {
    let (state, _dir, _live, path) =
        claude_session_with_transcript("server-question-failed-call", SessionStatus::Idle);
    let questions = serde_json::json!({"questions": []});
    let agent = serde_json::json!({"prompt": "go"});
    let hook = |event, id, tool, input| claude_call_hook(event, id, tool, input, &path);
    apply_hook(&state, "s", &claude_hook("UserPromptSubmit", &path));
    apply_hook(
        &state,
        "s",
        &hook("PreToolUse", Some("t1"), "Agent", agent.clone()),
    );
    apply_hook(
        &state,
        "s",
        &hook(
            "PreToolUse",
            Some("t2"),
            "AskUserQuestion",
            questions.clone(),
        ),
    );
    assert_eq!(status_of(&state), SessionStatus::WaitingUser);

    apply_hook(
        &state,
        "s",
        &hook("PostToolUseFailure", Some("t1"), "Agent", agent),
    );
    assert_eq!(status_of(&state), SessionStatus::WaitingUser);

    apply_hook(
        &state,
        "s",
        &hook(
            "PostToolUseFailure",
            Some("t2"),
            "AskUserQuestion",
            questions,
        ),
    );
    assert_eq!(status_of(&state), SessionStatus::Working);
}

/// The move to work an other call's end would bring is skipped whole: it arms no record watch,
/// where the one that moves a waiting session back to work arms the interrupt watch.
#[tokio::test]
async fn a_skipped_move_to_work_arms_no_watch() {
    let (state, _dir, _live, path) =
        claude_session_with_transcript("server-question-no-watch", SessionStatus::Idle);
    let questions = serde_json::json!({"questions": []});
    let agent = serde_json::json!({"prompt": "go"});
    let hook = |event, id, tool, input| claude_call_hook(event, id, tool, input, &path);
    apply_hook(&state, "s", &claude_hook("UserPromptSubmit", &path));
    apply_hook(
        &state,
        "s",
        &hook("PreToolUse", Some("t1"), "Agent", agent.clone()),
    );
    apply_hook(
        &state,
        "s",
        &hook(
            "PreToolUse",
            Some("t2"),
            "AskUserQuestion",
            questions.clone(),
        ),
    );
    // Retires the watch the question armed and stands in for the watch current now.
    let current = state.begin_record_watch("s");

    apply_hook(
        &state,
        "s",
        &hook("PostToolUse", Some("t1"), "Agent", agent),
    );
    assert_eq!(status_of(&state), SessionStatus::WaitingUser);
    assert!(state.record_watch_current("s", current));

    apply_hook(
        &state,
        "s",
        &hook("PostToolUse", Some("t2"), "AskUserQuestion", questions),
    );
    assert_eq!(status_of(&state), SessionStatus::Working);
    assert!(!state.record_watch_current("s", current));
}

/// Whichever event ends the turn's calls, a hand raised for a call that never ended comes down
/// with it, and a later call's end is then an ordinary move to work.
#[tokio::test]
async fn a_batch_prompt_or_turn_end_lowers_the_hand_of_a_call_still_waiting() {
    for (event, expected) in [
        ("PostToolBatch", SessionStatus::Working),
        ("UserPromptSubmit", SessionStatus::Working),
        ("Stop", SessionStatus::Idle),
        ("StopFailure", SessionStatus::Idle),
        ("SessionStart", SessionStatus::Idle),
    ] {
        let (state, _dir, _live, path) =
            claude_session_with_transcript("server-question-ended-by", SessionStatus::Idle);
        let questions = serde_json::json!({"questions": []});
        let hook = |event, id, tool, input| claude_call_hook(event, id, tool, input, &path);
        apply_hook(&state, "s", &claude_hook("UserPromptSubmit", &path));
        apply_hook(
            &state,
            "s",
            &hook("PreToolUse", Some("t1"), "AskUserQuestion", questions),
        );
        assert_eq!(status_of(&state), SessionStatus::WaitingUser, "{event}");

        apply_hook(&state, "s", &claude_hook(event, &path));
        assert_eq!(status_of(&state), expected, "{event}");

        // Nothing is waiting any more, so another call's end is work again.
        apply_hook(
            &state,
            "s",
            &hook("PostToolUse", Some("t2"), "Agent", serde_json::json!({})),
        );
        assert_eq!(status_of(&state), SessionStatus::Working, "{event}");
    }
}

/// A prompt for a call the daemon never saw start is lowered by the next call to end.
#[tokio::test]
async fn a_prompt_for_an_unseen_call_ends_with_the_next_call() {
    let (state, _dir, _live, path) =
        claude_session_with_transcript("server-prompt-unseen-call", SessionStatus::Working);
    let hook = |event, id, tool, input| claude_call_hook(event, id, tool, input, &path);
    apply_hook(
        &state,
        "s",
        &hook(
            "PermissionRequest",
            None,
            "Write",
            serde_json::json!({"path": "/x"}),
        ),
    );
    assert_eq!(status_of(&state), SessionStatus::WaitingUser);

    apply_hook(
        &state,
        "s",
        &hook("PostToolUse", Some("t9"), "Bash", serde_json::json!({})),
    );
    assert_eq!(status_of(&state), SessionStatus::Working);
}

/// Another agent's session does not tell its calls apart: a prompt is lowered by the end of any
/// call, the ids its events carry or not.
#[tokio::test]
async fn another_agents_prompt_is_lowered_by_the_end_of_any_call() {
    let (state, dir) = app_state("server-codex-prompt-any-end");
    state
        .store
        .insert_console(&crate::test_support::console(&dir))
        .unwrap();
    let mut session = crate::test_support::console_session("s", SessionStatus::Working);
    session.agent = AgentId::CODEX;
    state.store.insert_session(&session).unwrap();
    let hook = |event: &str, id: Option<&str>| {
        serde_json::json!({
            "hook_event_name": event,
            "tool_name": "Bash",
            "tool_input": {"command": "make"},
            "tool_use_id": id,
        })
        .to_string()
        .into_bytes()
    };
    apply_hook(&state, "s", &hook("PermissionRequest", None));
    assert_eq!(status_of(&state), SessionStatus::WaitingUser);

    apply_hook(&state, "s", &hook("PostToolUse", Some("other")));
    assert_eq!(status_of(&state), SessionStatus::Working);
}
