use std::time::{Duration, Instant};

use super::*;
use crate::messaging::texts::SenderNote;
use crate::protocol::{notice_code, AgentId, Event};
use crate::state::composer::{Outcome, COMMAND_SETTLE};
use crate::state::test_support::insert_record;
use crate::test_support::composer_agent::{agent, Agent};
use crate::test_support::{app_state, app_state_again, code_of, param_of, ScratchDir, PATIENCE};

/// A session `s` of the test provider at `status`, with no process.
fn bare(label: &str, status: SessionStatus) -> (Arc<AppState>, ScratchDir) {
    let (state, dir) = app_state(label);
    insert_record(&state, &dir, "s", status);
    let mut record = state.session_record("s").unwrap();
    record.agent = AgentId::TEST;
    state.save_session(&record).unwrap();
    (state, dir)
}

/// [`bare`] with an agent of its own drawing its input box.
fn running(label: &str, status: SessionStatus) -> (Arc<AppState>, Agent, ScratchDir) {
    let (state, dir) = bare(label, status);
    let agent = agent("s", "yes", "0", "");
    state.register_live(agent.live.clone());
    (state, agent, dir)
}

fn set_status(state: &Arc<AppState>, status: SessionStatus) {
    let mut record = state.session_record("s").unwrap();
    record.status = status;
    state.save_session(&record).unwrap();
}

fn text(text: &str) -> Content {
    Content {
        text: text.to_string(),
        images: Vec::new(),
        files: Vec::new(),
        command: false,
    }
}

async fn queue(state: &Arc<AppState>, content: Content, mode: SendMode) -> String {
    create(state, "s", content, Some(mode)).await.unwrap()
}

fn texts(state: &Arc<AppState>) -> Vec<String> {
    state
        .queued_messages("s")
        .unwrap()
        .into_iter()
        .map(|message| message.text)
        .collect()
}

fn message(state: &Arc<AppState>, id: &str) -> QueuedMessage {
    state
        .queued_messages("s")
        .unwrap()
        .into_iter()
        .find(|message| message.id == id)
        .expect("the message is queued")
}

/// Polls until `done` holds, for `PATIENCE` at most, returning as soon as it does.
async fn until(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting until {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Drains the session's delivery queue as a hook that reports `status` does, on the blocking
/// pool, and waits for it: what the composer queue does once the drain is over has been done
/// (or decided against) when this returns, though a hand-over it started is still under way.
async fn flushed(state: &Arc<AppState>, status: SessionStatus) {
    let flushing = state.clone();
    tokio::task::spawn_blocking(move || flushing.flush_outbox("s", status))
        .await
        .unwrap();
}

fn sent(agent: &Agent) -> Vec<String> {
    agent
        .logged()
        .into_iter()
        .filter(|line| line.starts_with("SENT "))
        .collect()
}

/// The last queue broadcast for `s` among `events`.
fn last_messages(
    events: &mut tokio::sync::broadcast::Receiver<Event>,
) -> Option<Vec<QueuedMessage>> {
    let mut last = None;
    while let Ok(event) = events.try_recv() {
        if let Event::SessionMessageQueue { session, messages } = event {
            if session == "s" {
                last = Some(messages);
            }
        }
    }
    last
}

/// The texts of [`last_messages`].
fn last_queue(events: &mut tokio::sync::broadcast::Receiver<Event>) -> Option<Vec<String>> {
    last_messages(events).map(|messages| messages.into_iter().map(|message| message.text).collect())
}

/// Adding, editing, moving, changing the mode of and withdrawing queued messages, each change
/// broadcast as the whole queue, for a session with no process.
#[tokio::test]
async fn messages_are_added_edited_moved_re_moded_and_withdrawn_with_each_change_broadcast() {
    let (state, _dir) = bare("composer-manage", SessionStatus::Interrupted);
    let mut events = state.subscribe();
    let a = queue(&state, text("a"), SendMode::Manual).await;
    let b = queue(&state, text("b"), SendMode::WhenIdle).await;
    let c = queue(&state, text("c"), SendMode::Manual).await;
    assert_eq!(last_queue(&mut events).unwrap(), ["a", "b", "c"]);
    assert_eq!(message(&state, &b).mode, QueuedMode::WhenIdle);

    update(&state, "s", &a, Some("a2".to_string()), None, None, None)
        .await
        .unwrap();
    assert_eq!(last_queue(&mut events).unwrap(), ["a2", "b", "c"]);

    move_to(&state, "s", &c, 0).unwrap();
    assert_eq!(last_queue(&mut events).unwrap(), ["c", "a2", "b"]);
    move_to(&state, "s", &c, 99).unwrap();
    assert_eq!(texts(&state), ["a2", "b", "c"]);

    set_message_mode(&state, "s", &b, QueuedMode::Manual).unwrap();
    assert_eq!(message(&state, &b).mode, QueuedMode::Manual);
    let broadcast = last_messages(&mut events).expect("the change is broadcast");
    assert_eq!(
        broadcast
            .iter()
            .map(|message| message.mode)
            .collect::<Vec<_>>(),
        [QueuedMode::Manual; 3]
    );

    withdraw(&state, "s", &a).unwrap();
    assert_eq!(last_queue(&mut events).unwrap(), ["b", "c"]);
    let refused = withdraw(&state, "s", &a).unwrap_err();
    assert_eq!(code_of(&refused), error_code::QUEUED_MESSAGE_NOT_FOUND);
}

/// Nothing to send, and an archived session, are refused; a full queue is refused at its bound.
#[tokio::test]
async fn blank_messages_archived_sessions_and_a_full_queue_are_refused() {
    let (state, _dir) = bare("composer-refusals", SessionStatus::Interrupted);
    let refused = create(&state, "s", text("  \u{200B} "), None)
        .await
        .unwrap_err();
    assert_eq!(code_of(&refused), error_code::FIELD_REQUIRED);
    assert_eq!(param_of(&refused, "field"), "text");
    for n in 0..crate::state::composer::MAX_QUEUED {
        queue(&state, text(&n.to_string()), SendMode::Manual).await;
    }
    let refused = create(&state, "s", text("one more"), None)
        .await
        .unwrap_err();
    assert_eq!(code_of(&refused), error_code::LIMIT_EXCEEDED);
    assert_eq!(param_of(&refused, "limit"), "queued_messages");
    assert_eq!(param_of(&refused, "max"), "100");
    assert_eq!(state.queued_messages("s").unwrap().len(), 100);

    let (state, _dir) = bare("composer-archived", SessionStatus::Archived);
    let refused = create(&state, "s", text("x"), None).await.unwrap_err();
    assert_eq!(code_of(&refused), error_code::SESSION_ARCHIVED);
    let refused = update(&state, "s", "m", Some("x".to_string()), None, None, None)
        .await
        .unwrap_err();
    assert_eq!(code_of(&refused), error_code::SESSION_ARCHIVED);
    let refused = send(&state, "s", "m").await.unwrap_err();
    assert_eq!(code_of(&refused), error_code::SESSION_ARCHIVED);
}

/// A message no queue holds, and a session that does not exist, are refused as such by every
/// request that names one.
#[tokio::test]
async fn unknown_messages_and_sessions_are_refused_as_such() {
    let (state, _dir) = bare("composer-unknown", SessionStatus::Interrupted);
    let refusals = [
        update(&state, "s", "nope", Some("x".to_string()), None, None, None)
            .await
            .unwrap_err(),
        set_message_mode(&state, "s", "nope", QueuedMode::Manual).unwrap_err(),
        move_to(&state, "s", "nope", 0).unwrap_err(),
        withdraw(&state, "s", "nope").unwrap_err(),
        send(&state, "s", "nope").await.unwrap_err(),
    ];
    for refused in refusals {
        assert_eq!(code_of(&refused), error_code::QUEUED_MESSAGE_NOT_FOUND);
        assert_eq!(param_of(&refused, "message"), "nope");
        assert_eq!(param_of(&refused, "session"), "s");
    }
    let refused = create(&state, "gone", text("x"), None).await.unwrap_err();
    assert_eq!(code_of(&refused), error_code::UNKNOWN_SESSION);
}

/// The session's default mode is stored with it, starts as `now`, and is what a message given no
/// mode is queued in.
#[tokio::test]
async fn the_session_default_mode_is_used_for_a_message_given_none() {
    let (state, _dir) = bare("composer-default-mode", SessionStatus::Interrupted);
    assert_eq!(state.session_record("s").unwrap().send_mode, SendMode::Now);
    let mut events = state.subscribe();
    set_send_mode(&state, "s", SendMode::WhenIdle).unwrap();
    let mut announced = None;
    while let Ok(event) = events.try_recv() {
        if let Event::SessionUpserted { session } = event {
            announced = Some(session.send_mode);
        }
    }
    assert_eq!(announced, Some(SendMode::WhenIdle), "every client is told");
    assert_eq!(
        state.session_record("s").unwrap().send_mode,
        SendMode::WhenIdle
    );
    let id = create(&state, "s", text("x"), None).await.unwrap();
    assert_eq!(message(&state, &id).mode, QueuedMode::WhenIdle);
    let refused = set_send_mode(&state, "gone", SendMode::Manual).unwrap_err();
    assert_eq!(code_of(&refused), error_code::UNKNOWN_SESSION);
}

/// Once the session is idle the first `when_idle` message goes, skipping the manual one before
/// it; the next waits for the next idle, after a turn.
#[tokio::test]
async fn idle_dispatch_sends_one_when_idle_message_per_idle_skipping_manual_ones() {
    let (state, agent, _dir) = running("composer-idle", SessionStatus::Working);
    queue(&state, text("manual"), SendMode::Manual).await;
    queue(&state, text("first"), SendMode::WhenIdle).await;
    queue(&state, text("second"), SendMode::WhenIdle).await;
    // Decided as each was queued: nothing was handed over while the session works.
    assert!(state
        .queued_messages("s")
        .unwrap()
        .iter()
        .all(|message| message.state == QueuedMessageState::Waiting));
    assert_eq!(state.outbox.len("s"), 0);

    set_status(&state, SessionStatus::Idle);
    flushed(&state, SessionStatus::Idle).await;
    until("the first is sent", || sent(&agent) == ["SENT first"]).await;
    until("it leaves the queue", || {
        texts(&state) == ["manual", "second"]
    })
    .await;
    // Another drain of the same idle hands over nothing more, so that one message went per idle.
    flushed(&state, SessionStatus::Idle).await;
    assert_eq!(message_texts_waiting(&state), ["manual", "second"]);
    assert_eq!(sent(&agent), ["SENT first"], "one per idle");

    // A turn, then the next idle.
    set_status(&state, SessionStatus::Working);
    flushed(&state, SessionStatus::Working).await;
    set_status(&state, SessionStatus::Idle);
    flushed(&state, SessionStatus::Idle).await;
    until("the second is sent", || {
        sent(&agent) == ["SENT first", "SENT second"]
    })
    .await;
    until("only the manual one is left", || {
        texts(&state) == ["manual"]
    })
    .await;
    // The manual message is never sent by itself, however many idles come.
    set_status(&state, SessionStatus::Working);
    flushed(&state, SessionStatus::Working).await;
    set_status(&state, SessionStatus::Idle);
    flushed(&state, SessionStatus::Idle).await;
    assert_eq!(message_texts_waiting(&state), ["manual"]);
    assert_eq!(sent(&agent).len(), 2);
}

/// The texts of the queue's messages that are waiting, none being handed over.
fn message_texts_waiting(state: &Arc<AppState>) -> Vec<String> {
    state
        .queued_messages("s")
        .unwrap()
        .into_iter()
        .inspect(|message| assert_eq!(message.state, QueuedMessageState::Waiting))
        .map(|message| message.text)
        .collect()
}

/// A `when_idle` message goes at once to a session that is idle already, paused included.
#[tokio::test]
async fn a_when_idle_message_goes_at_once_to_an_idle_or_paused_session() {
    let (state, agent, _dir) = running("composer-already-idle", SessionStatus::Idle);
    queue(&state, text("now"), SendMode::WhenIdle).await;
    until("it is sent", || sent(&agent) == ["SENT now"]).await;
    until("it leaves the queue", || texts(&state).is_empty()).await;

    let mut record = state.session_record("s").unwrap();
    record.paused = true;
    state.save_session(&record).unwrap();
    set_status(&state, SessionStatus::Working);
    flushed(&state, SessionStatus::Working).await;
    set_status(&state, SessionStatus::Idle);
    queue(&state, text("paused"), SendMode::WhenIdle).await;
    until("it is sent to the paused session", || {
        sent(&agent) == ["SENT now", "SENT paused"]
    })
    .await;
}

/// A session that waits on the user, one whose writes something holds, and one that has a
/// message queued in delivery are not idle for the composer: the message goes once none is true.
#[tokio::test]
async fn a_when_idle_message_waits_for_the_user_a_hold_and_delivery_to_be_done() {
    let (state, agent, _dir) = running("composer-conditions", SessionStatus::WaitingUser);
    queue(&state, text("patient"), SendMode::WhenIdle).await;
    flushed(&state, SessionStatus::WaitingUser).await;
    assert_eq!(
        message_texts_waiting(&state),
        ["patient"],
        "waiting on the user"
    );

    // Idle again, but delivery's queue is held (a handoff), with a message of another sender
    // in it.
    set_status(&state, SessionStatus::Idle);
    let hold = state.hold_outbox("s");
    state.queue_message("s", "from a peer");
    flushed(&state, SessionStatus::Idle).await;
    assert_eq!(
        message_texts_waiting(&state),
        ["patient"],
        "held and queued"
    );
    assert_eq!(state.outbox.len("s"), 1);
    assert!(sent(&agent).is_empty());

    drop(hold);
    until("both are sent, the peer's first", || {
        sent(&agent) == ["SENT from a peer", "SENT patient"]
    })
    .await;
    until("it leaves the queue", || texts(&state).is_empty()).await;
}

/// Send now hands a manual message over at once, whatever its mode, and it leaves the queue.
#[tokio::test]
async fn send_now_hands_a_message_over_at_once() {
    let (state, agent, _dir) = running("composer-send-now", SessionStatus::Working);
    let id = queue(&state, text("now please"), SendMode::Manual).await;
    send(&state, "s", &id).await.unwrap();
    assert_eq!(sent(&agent), ["SENT now please"]);
    until("it leaves the queue", || texts(&state).is_empty()).await;
}

/// A message queued in mode `now` goes at once too, its files after its text.
#[tokio::test]
async fn a_message_queued_now_goes_at_once_with_its_files_after_its_text() {
    let (state, agent, _dir) = running("composer-create-now", SessionStatus::Idle);
    let content = Content {
        files: vec!["src/main.rs".to_string(), "docs/a b.md".to_string()],
        ..text("look")
    };
    queue(&state, content, SendMode::Now).await;
    until("it is sent", || {
        sent(&agent) == ["SENT look\\nsrc/main.rs 'docs/a b.md'"]
    })
    .await;
    until("it leaves the queue", || texts(&state).is_empty()).await;
}

/// A send now refused (the session waits for the user) puts the message back in its place with
/// its mode, the refusal set as its failure.
#[tokio::test]
async fn a_refused_send_now_puts_the_message_back_with_its_failure() {
    let (state, agent, _dir) = running("composer-refused", SessionStatus::WaitingUser);
    let id = queue(&state, text("blocked"), SendMode::Manual).await;
    let refused = send(&state, "s", &id).await.unwrap_err();
    assert_eq!(code_of(&refused), error_code::SESSION_WAITING_FOR_USER);
    let back = message(&state, &id);
    assert_eq!(back.state, QueuedMessageState::Waiting);
    assert_eq!(back.mode, QueuedMode::Manual);
    let failure = back.failure.expect("a failure");
    assert_eq!(failure.kind, QueuedMessageFailureKind::Refused);
    assert_eq!(failure.code, "session_waiting_for_user");
    assert!(sent(&agent).is_empty());

    // Editing clears the failure.
    update(
        &state,
        "s",
        &id,
        Some("edited".to_string()),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(message(&state, &id).failure, None);
}

/// A message created with mode `now` that is refused stays queued as manual, so that it never
/// goes out later on its own, with the refusal as its failure.
#[tokio::test]
async fn a_message_created_now_and_refused_stays_as_manual_and_never_goes_by_itself() {
    let (state, agent, _dir) = running("composer-create-refused", SessionStatus::WaitingUser);
    let id = queue(&state, text("blocked"), SendMode::Now).await;
    until("the refusal is on the message", || {
        message(&state, &id).failure.is_some()
    })
    .await;
    let back = message(&state, &id);
    assert_eq!(back.mode, QueuedMode::Manual);
    assert_eq!(back.state, QueuedMessageState::Waiting);
    let failure = back.failure.unwrap();
    assert_eq!(failure.kind, QueuedMessageFailureKind::Refused);
    assert_eq!(failure.code, "session_waiting_for_user");

    set_status(&state, SessionStatus::Idle);
    flushed(&state, SessionStatus::Idle).await;
    assert_eq!(message_texts_waiting(&state), ["blocked"]);
    assert!(sent(&agent).is_empty());
}

/// The daemon's own dispatch at an idle leaves a message waiting, as it was, where the refusal
/// may pass; a refusal that will not pass switches it to manual with the refusal. A send now
/// that is refused for good keeps the mode.
#[tokio::test]
async fn a_refused_idle_dispatch_waits_where_it_may_pass_and_goes_manual_where_it_may_not() {
    let (state, agent, _dir) = running("composer-idle-refused", SessionStatus::WaitingUser);
    let id = queue(&state, text("patient"), SendMode::WhenIdle).await;
    let begun = state
        .composer_begin_send("s", &id, How::Idle, false)
        .unwrap();
    let refused = hand_over(&state, "s", begun, How::Idle).await.unwrap_err();
    assert_eq!(code_of(&refused), error_code::SESSION_WAITING_FOR_USER);
    let back = message(&state, &id);
    assert_eq!(back.state, QueuedMessageState::Waiting);
    assert_eq!(back.mode, QueuedMode::WhenIdle, "still to go at an idle");
    assert_eq!(back.failure, None);
    set_status(&state, SessionStatus::Idle);
    assert_eq!(
        state.composer_take_idle("s").map(|message| message.id),
        Some(id.clone()),
        "the next idle sends it, first in the queue"
    );
    state.composer_back("s", &id, None, None);
    set_status(&state, SessionStatus::WaitingUser);

    // A message with nothing to write is refused for good, wherever it was queued from.
    let hollow = QueuedMessage {
        text: String::new(),
        id: "hollow".to_string(),
        ..message(&state, &id)
    };
    state.composer_add("s", hollow, Vec::new()).unwrap();
    let begun = state
        .composer_begin_send("s", "hollow", How::Idle, false)
        .unwrap();
    let refused = hand_over(&state, "s", begun, How::Idle).await.unwrap_err();
    assert_eq!(code_of(&refused), error_code::FIELD_REQUIRED);
    let back = message(&state, "hollow");
    assert_eq!(back.mode, QueuedMode::Manual);
    let failure = back.failure.expect("the refusal");
    assert_eq!(failure.kind, QueuedMessageFailureKind::Refused);
    assert_eq!(failure.code, "field_required");

    set_message_mode(&state, "s", "hollow", QueuedMode::WhenIdle).unwrap();
    let refused = send(&state, "s", "hollow").await.unwrap_err();
    assert_eq!(code_of(&refused), error_code::FIELD_REQUIRED);
    assert_eq!(message(&state, "hollow").mode, QueuedMode::WhenIdle);
    assert!(sent(&agent).is_empty());
}

/// Send now on a session with no process resumes it, and hands the message over once the
/// resumed session can take one.
#[tokio::test]
async fn send_now_resumes_a_session_with_no_process_and_sends_once_it_can() {
    let (state, _dir) = bare("composer-resume", SessionStatus::Interrupted);
    let id = queue(&state, text("after resume"), SendMode::WhenIdle).await;
    let agent = agent("s", "yes", "0", "");
    let live = agent.live.clone();
    send_with(&state, "s", &id, |state, session| async move {
        set_status(&state, SessionStatus::Idle);
        state.register_live(live);
        state.spawn_flush_outbox(&session, SessionStatus::Idle);
        Ok(())
    })
    .await
    .unwrap();
    until("it is sent", || sent(&agent) == ["SENT after resume"]).await;
    until("it leaves the queue", || texts(&state).is_empty()).await;
}

/// A send now whose resume fails puts the message back waiting with the failure, its mode kept.
#[tokio::test]
async fn a_send_now_whose_resume_fails_puts_the_message_back_with_the_failure() {
    let (state, _dir) = bare("composer-resume-fails", SessionStatus::Interrupted);
    let id = queue(&state, text("stuck"), SendMode::WhenIdle).await;
    let refused = send_with(&state, "s", &id, |_, _| async {
        Err(anyhow::anyhow!("no account is available"))
    })
    .await
    .unwrap_err();
    assert!(refused.to_string().contains("no account"));
    let back = message(&state, &id);
    assert_eq!(back.state, QueuedMessageState::Waiting);
    assert_eq!(back.mode, QueuedMode::WhenIdle);
    let failure = back.failure.expect("the failure");
    assert_eq!(failure.kind, QueuedMessageFailureKind::Refused);
}

/// A `when_idle` message never resumes a session: it waits while there is no process, and is
/// sent once the session is resumed and idle.
#[tokio::test]
async fn a_when_idle_message_waits_while_the_session_has_no_process_and_goes_once_resumed() {
    let (state, _dir) = bare("composer-no-process", SessionStatus::Interrupted);
    let id = queue(&state, text("later"), SendMode::WhenIdle).await;
    // Decided as it was queued, with no process to look at.
    assert_eq!(message(&state, &id).state, QueuedMessageState::Waiting);
    assert!(state.live_session("s").is_none());

    let agent = agent("s", "yes", "0", "");
    set_status(&state, SessionStatus::Idle);
    state.register_live(agent.live.clone());
    flushed(&state, SessionStatus::Idle).await;
    until("it is sent", || sent(&agent) == ["SENT later"]).await;
    until("it leaves the queue", || texts(&state).is_empty()).await;
}

/// A message delivery wrote and then reported not carried out comes back to its place as manual,
/// marked not delivered; one the agent later reports taking after all leaves the queue again.
#[tokio::test]
async fn a_message_not_carried_out_comes_back_as_manual_and_leaves_again_if_it_ran() {
    let (state, _dir) = bare("composer-not-carried-out", SessionStatus::Interrupted);
    let before = queue(&state, text("before"), SendMode::Manual).await;
    let id = queue(&state, text("lost"), SendMode::WhenIdle).await;
    queue(&state, text("after"), SendMode::Manual).await;
    state
        .composer_begin_send("s", &id, How::Idle, false)
        .unwrap();
    state.composer_note_ticket(&id, 7);
    state.composer_outcome("s", 7, Outcome::Written);
    assert_eq!(texts(&state), ["before", "after"]);

    state.tell_senders_of(
        "s",
        AgentId::TEST,
        SenderNote::NotTaken,
        &[(Some("launch:7".to_string()), "lost".to_string())],
    );
    assert_eq!(texts(&state), ["before", "lost", "after"]);
    let back = message(&state, &id);
    assert_eq!(back.mode, QueuedMode::Manual);
    let failure = back.failure.expect("a failure");
    assert_eq!(failure.kind, QueuedMessageFailureKind::NotDelivered);
    assert_eq!(failure.code, "message_not_taken");

    state.tell_senders_of(
        "s",
        AgentId::TEST,
        SenderNote::RanAfterAll,
        &[(Some("launch:7".to_string()), "lost".to_string())],
    );
    assert_eq!(texts(&state), ["before", "after"]);
    // The message before it was never touched.
    assert_eq!(message(&state, &before).failure, None);
}

/// Each report of delivery that a message was not carried out puts it back as manual with that
/// notice's code, and a message put back is never sent by the daemon's own dispatch; the next
/// message may go in the same idle. Withdrawing the notice (the agent took it after all) takes
/// back a message still as it came back, and leaves one the user changed since.
#[tokio::test]
async fn every_report_of_a_message_not_carried_out_puts_it_back() {
    let (state, _dir) = bare("composer-notes", SessionStatus::Interrupted);
    let notes = [
        (SenderNote::NotTaken, "message_not_taken"),
        (
            SenderNote::MaybeDroppedAtCancel,
            "message_maybe_dropped_at_cancel",
        ),
        (SenderNote::DroppedAtCancel, "message_dropped_at_cancel"),
        (SenderNote::GivenBackUnsent, "message_given_back_unsent"),
        (SenderNote::NotDelivered, "message_not_delivered"),
    ];
    for (n, (note, code)) in notes.into_iter().enumerate() {
        let ticket = 100 + n as u64;
        let logical = vec![(Some(format!("launch:{ticket}")), "text".to_string())];
        let id = queue(&state, text(code), SendMode::WhenIdle).await;
        queue(&state, text("next"), SendMode::WhenIdle).await;
        let taken = state.composer_take_idle("s").expect("goes at the idle");
        assert_eq!(taken.id, id);
        state.composer_note_ticket(&id, ticket);
        state.composer_outcome("s", ticket, Outcome::Written);
        assert!(state.composer_take_idle("s").is_none(), "one per idle");

        state.tell_senders_of("s", AgentId::TEST, note, &logical);
        let back = message(&state, &id);
        assert_eq!(back.state, QueuedMessageState::Waiting, "{code}");
        assert_eq!(back.mode, QueuedMode::Manual, "{code}");
        let failure = back.failure.expect("a failure");
        assert_eq!(failure.kind, QueuedMessageFailureKind::NotDelivered);
        assert_eq!(failure.code, code);
        // The one it returned does not go again; the one behind it may, in the same idle.
        let after = state.composer_take_idle("s").expect("the next may go");
        assert_eq!(after.text, "next");
        state.composer_back("s", &after.id, None, None);
        withdraw(&state, "s", &after.id).unwrap();
        state.composer_idle_over("s");
        assert!(state.composer_take_idle("s").is_none(), "{code}: manual");

        // The agent took it after all: still as it came back, it leaves the queue again.
        state.tell_senders_of("s", AgentId::TEST, SenderNote::RanAfterAll, &logical);
        assert!(texts(&state).is_empty(), "{code}: {:?}", texts(&state));
    }

    // Changed by the user since it came back, it stays.
    let id = queue(&state, text("edited"), SendMode::WhenIdle).await;
    state.composer_take_idle("s").expect("goes");
    state.composer_note_ticket(&id, 200);
    state.composer_outcome("s", 200, Outcome::Written);
    let logical = vec![(Some("launch:200".to_string()), "edited".to_string())];
    state.tell_senders_of("s", AgentId::TEST, SenderNote::NotTaken, &logical);
    update(
        &state,
        "s",
        &id,
        Some("edited again".to_string()),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    state.tell_senders_of("s", AgentId::TEST, SenderNote::RanAfterAll, &logical);
    assert_eq!(texts(&state), ["edited again"]);
}

/// A message refused once queued in delivery comes back as manual with that refusal.
#[tokio::test]
async fn a_message_refused_once_queued_comes_back_as_manual() {
    let (state, _dir) = bare("composer-refused-queued", SessionStatus::Interrupted);
    let id = queue(&state, text("refused"), SendMode::WhenIdle).await;
    state
        .composer_begin_send("s", &id, How::Idle, false)
        .unwrap();
    state.composer_note_ticket(&id, 9);
    let failure = crate::state::composer::failure_of_notice(
        "command_not_found",
        Default::default(),
        "no such command",
    );
    state.composer_outcome("s", 9, Outcome::Back(failure));
    let back = message(&state, &id);
    assert_eq!(back.state, QueuedMessageState::Waiting);
    assert_eq!(back.mode, QueuedMode::Manual);
    assert_eq!(back.failure.unwrap().code, "command_not_found");
}

/// A message delivery holds unwritten reads as sending: it cannot be edited, but it can be
/// withdrawn, out of delivery too, the messages of other senders around it staying queued in
/// their order.
#[tokio::test]
async fn a_message_held_by_delivery_cannot_be_edited_and_can_be_withdrawn_alone() {
    let (state, agent, _dir) = running("composer-held", SessionStatus::Idle);
    let hold = state.hold_outbox("s");
    state.queue_message("s", "before");
    let id = queue(&state, text("held"), SendMode::Manual).await;
    send(&state, "s", &id).await.unwrap();
    state.queue_message("s", "after");
    assert_eq!(message(&state, &id).state, QueuedMessageState::Sending);
    assert_eq!(state.outbox.len("s"), 3);

    let refused = update(&state, "s", &id, Some("x".to_string()), None, None, None)
        .await
        .unwrap_err();
    assert_eq!(code_of(&refused), error_code::QUEUED_MESSAGE_SENDING);
    let refused = set_message_mode(&state, "s", &id, QueuedMode::WhenIdle).unwrap_err();
    assert_eq!(code_of(&refused), error_code::QUEUED_MESSAGE_SENDING);

    withdraw(&state, "s", &id).unwrap();
    assert!(texts(&state).is_empty());
    assert_eq!(state.outbox.len("s"), 2);
    drop(hold);
    // The withdrawn message would have gone between the two.
    until("the others are sent", || {
        sent(&agent) == ["SENT before", "SENT after"]
    })
    .await;
}

/// A message already taken by delivery to be written cannot be withdrawn: it is refused as
/// being sent, and stays as it was.
#[tokio::test]
async fn a_message_delivery_has_taken_to_write_cannot_be_withdrawn() {
    let (state, _dir) = bare("composer-written", SessionStatus::Interrupted);
    let id = queue(&state, text("going"), SendMode::Manual).await;
    state
        .composer_begin_send("s", &id, How::Now, false)
        .unwrap();
    // Delivery named a ticket for it that its queue no longer holds.
    state.composer_note_ticket(&id, 7);
    let refused = withdraw(&state, "s", &id).unwrap_err();
    assert_eq!(code_of(&refused), error_code::QUEUED_MESSAGE_SENDING);
    assert_eq!(message(&state, &id).state, QueuedMessageState::Sending);
}

/// What delivery held when the session's process ended goes with it; the message comes back to
/// waiting in its queue.
#[tokio::test]
async fn a_message_delivery_dropped_with_the_process_comes_back_to_waiting() {
    let (state, _agent, _dir) = running("composer-dropped", SessionStatus::Idle);
    let hold = state.hold_outbox("s");
    let id = queue(&state, text("held"), SendMode::Manual).await;
    send(&state, "s", &id).await.unwrap();
    assert_eq!(message(&state, &id).state, QueuedMessageState::Sending);
    state.discard_outbox("s");
    drop(hold);
    state.interrupt_session_record("s").unwrap();
    let back = message(&state, &id);
    assert_eq!(back.state, QueuedMessageState::Waiting);
    assert_eq!(back.failure, None);
}

/// The queue and the default mode survive a daemon restart, a message being handed over coming
/// back waiting.
#[tokio::test]
async fn the_queue_survives_a_restart() {
    let (state, mut dir) = bare("composer-restart", SessionStatus::Interrupted);
    set_send_mode(&state, "s", SendMode::Manual).unwrap();
    let a = queue(&state, text("a"), SendMode::WhenIdle).await;
    queue(&state, text("b"), SendMode::Manual).await;
    state.composer_begin_send("s", &a, How::Now, true).unwrap();

    let again = app_state_again(&mut dir);
    assert_eq!(texts(&again), ["a", "b"]);
    assert_eq!(message(&again, &a).state, QueuedMessageState::Waiting);
    assert_eq!(message(&again, &a).mode, QueuedMode::WhenIdle);
    assert_eq!(
        again.session_record("s").unwrap().send_mode,
        SendMode::Manual
    );
    let snapshot = again.message_queues();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].messages.len(), 2);
}

/// A planned stop saves what delivery holds for the next daemon, and leaves out the messages
/// of the composer's queue, which stay in it: the next daemon loads them there, waiting.
#[tokio::test]
async fn a_planned_stop_leaves_the_composers_messages_in_the_composer_queue() {
    let (state, _agent, mut dir) = running("composer-planned-stop", SessionStatus::Idle);
    let hold = state.hold_outbox("s");
    state.queue_message("s", "from a peer");
    let id = queue(&state, text("from the composer"), SendMode::Manual).await;
    send(&state, "s", &id).await.unwrap();
    assert_eq!(state.outbox.len("s"), 2);

    let path = dir.join("restart-outbox.json");
    assert_eq!(state.save_restart_outbox(&path), 1);
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("from a peer"), "{saved}");
    assert!(!saved.contains("from the composer"), "{saved}");
    drop(hold);

    let again = app_state_again(&mut dir);
    assert_eq!(texts(&again), ["from the composer"]);
    assert_eq!(message(&again, &id).state, QueuedMessageState::Waiting);
    assert_eq!(message(&again, &id).failure, None);
}

/// Archiving drops the queue and hands each message's content back in a notice of its own,
/// its text followed by the paths of its images and files, one to a line.
#[tokio::test]
async fn archiving_drops_the_queue_and_hands_each_message_back() {
    let (state, _dir) = bare("composer-archive", SessionStatus::Interrupted);
    let image = crate::session::pasted_image::save_bytes(&state, "s", b"\x89PNG\r\n\x1a\nimage")
        .unwrap()
        .to_string_lossy()
        .into_owned();
    queue(
        &state,
        Content {
            files: vec!["notes.md".to_string(), "docs/b.md".to_string()],
            images: vec![image.clone()],
            ..text("keep me")
        },
        SendMode::Manual,
    )
    .await;
    queue(
        &state,
        Content {
            files: vec!["only.md".to_string()],
            ..text("")
        },
        SendMode::WhenIdle,
    )
    .await;
    queue(&state, text("third"), SendMode::Manual).await;
    let mut events = state.subscribe();
    state.archive_session_record("s").unwrap();
    let mut handed_back = Vec::new();
    let mut final_queue = None;
    while let Ok(event) = events.try_recv() {
        match event {
            Event::SessionNotice {
                session,
                code,
                params,
                ..
            } if code == notice_code::QUEUED_MESSAGE_DISCARDED => {
                assert_eq!(session, "s");
                handed_back.push(params["detail"].clone());
            }
            Event::SessionMessageQueue { messages, .. } => final_queue = Some(messages),
            _ => {}
        }
    }
    assert_eq!(
        handed_back,
        [
            format!("keep me\n{image}\nnotes.md\ndocs/b.md"),
            "only.md".to_string(),
            "third".to_string()
        ]
    );
    assert_eq!(
        final_queue,
        Some(Vec::new()),
        "the queue is announced empty"
    );
    assert!(texts(&state).is_empty());
    assert!(state.message_queues().is_empty());
}

/// An image saved for a running session is moved into its composer directory when queued, a
/// session with no process saves straight there, and a withdrawn message's files are removed.
#[tokio::test]
async fn images_are_kept_in_the_composer_directory_and_removed_on_withdrawal() {
    let (state, _agent, _dir) = running("composer-images", SessionStatus::Working);
    let saved = crate::session::pasted_image::save_bytes(&state, "s", b"\x89PNG\r\n\x1a\nimage")
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let id = queue(
        &state,
        Content {
            images: vec![saved.clone()],
            ..text("see")
        },
        SendMode::Manual,
    )
    .await;
    let kept = message(&state, &id).images[0].clone();
    assert!(
        kept.starts_with(&*state.composer_dir("s").to_string_lossy()),
        "{kept}"
    );
    assert!(!std::path::Path::new(&saved).exists());
    assert!(std::path::Path::new(&kept).is_file());

    let refused = create(
        &state,
        "s",
        Content {
            images: vec!["/etc/passwd".to_string()],
            ..text("no")
        },
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(&refused), error_code::UNREADABLE_REQUEST);
    assert_eq!(param_of(&refused, "field"), "images");

    withdraw(&state, "s", &id).unwrap();
    assert!(!std::path::Path::new(&kept).exists());

    let (state, _dir) = bare("composer-images-no-process", SessionStatus::Interrupted);
    let saved =
        crate::session::pasted_image::save_bytes(&state, "s", b"\x89PNG\r\n\x1a\nimage").unwrap();
    assert!(saved.starts_with(state.composer_dir("s")));
    // Once the session is deleted its directory goes.
    state.store.delete_session("s").unwrap();
    state.forget_cascaded_sessions(&["s"]);
    assert!(!state.composer_dir("s").exists());
}

/// More than ten images are refused, and an image already in the composer directory (a message
/// edited again) keeps its path.
#[tokio::test]
async fn at_most_ten_images_a_message_and_a_kept_image_keeps_its_path_when_edited() {
    let (state, _dir) = bare("composer-ten-images", SessionStatus::Interrupted);
    let saved: Vec<String> = (0..11)
        .map(|n| {
            let bytes = [b"\x89PNG\r\n\x1a\n".as_slice(), &[n as u8]].concat();
            crate::session::pasted_image::save_bytes(&state, "s", &bytes)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let refused = create(
        &state,
        "s",
        Content {
            images: saved.clone(),
            ..text("too many")
        },
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(&refused), error_code::UNREADABLE_REQUEST);
    assert_eq!(param_of(&refused, "field"), "images");

    let id = create(
        &state,
        "s",
        Content {
            images: saved[..2].to_vec(),
            ..text("two")
        },
        Some(SendMode::Manual),
    )
    .await
    .unwrap();
    let kept = message(&state, &id).images;
    assert_eq!(kept, saved[..2]);
    update(
        &state,
        "s",
        &id,
        Some("edited".to_string()),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    update(&state, "s", &id, None, Some(kept[..1].to_vec()), None, None)
        .await
        .unwrap();
    let after = message(&state, &id);
    assert_eq!(after.images, kept[..1]);
    assert_eq!(after.text, "edited");
    assert!(std::path::Path::new(&kept[0]).is_file());
}

/// A message's files are written after its text on a line of their own, bare or, where a path
/// holds a space, in single quotes as a POSIX shell quotes it (a quote inside as `'\''`), joined
/// by spaces; a message of files alone is only that line.
#[tokio::test]
async fn files_are_written_after_the_text_quoted_as_dropped_paths_are() {
    let (state, agent, _dir) = running("composer-files", SessionStatus::Idle);
    let cases: [(&str, &[&str], &str); 3] = [
        ("see", &["a.rs"], "SENT see\\na.rs"),
        ("", &["a.rs", "b c.md"], "SENT a.rs 'b c.md'"),
        ("quote", &["it's a.md"], "SENT quote\\n'it'\\''s a.md'"),
    ];
    for (n, (body, files, expected)) in cases.into_iter().enumerate() {
        let content = Content {
            files: files.iter().map(|file| file.to_string()).collect(),
            ..text(body)
        };
        queue(&state, content, SendMode::Now).await;
        until("it is sent", || sent(&agent).len() == n + 1).await;
        assert_eq!(sent(&agent)[n], expected);
    }
}

/// A message marked as a command is sent as `send_command` sends one; one that also carries
/// images or files is refused as having no text of its own to send.
#[tokio::test]
async fn a_command_message_is_sent_as_a_command_and_takes_no_files() {
    let (state, agent, _dir) = running("composer-command", SessionStatus::Idle);
    let refused = create(
        &state,
        "s",
        Content {
            command: true,
            files: vec!["a.md".to_string()],
            ..text("/status")
        },
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(&refused), error_code::FIELD_REQUIRED);
    assert_eq!(param_of(&refused, "field"), "text");

    let refused = create(
        &state,
        "s",
        Content {
            command: true,
            ..text("status")
        },
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(&refused), error_code::UNREADABLE_REQUEST);
    assert!(texts(&state).is_empty());

    let id = queue(
        &state,
        Content {
            command: true,
            ..text("/status")
        },
        SendMode::Manual,
    )
    .await;
    assert!(message(&state, &id).command);
    send(&state, "s", &id).await.unwrap();
    assert_eq!(agent.logged(), ["PASTE /status", "SENT /status"]);
}

/// The files of a message that was sent are kept until the session is deleted, since the agent
/// reads one by its path whenever it gets to it.
#[tokio::test]
async fn the_images_of_a_sent_message_are_kept_until_the_session_is_deleted() {
    let (state, agent, _dir) = running("composer-kept-files", SessionStatus::Idle);
    let image = crate::session::pasted_image::save_bytes(&state, "s", b"\x89PNG\r\n\x1a\nimage")
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let id = queue(
        &state,
        Content {
            images: vec![image],
            ..text("look")
        },
        SendMode::Manual,
    )
    .await;
    let kept = message(&state, &id).images[0].clone();
    send(&state, "s", &id).await.unwrap();
    until("it is sent and leaves the queue", || {
        sent(&agent).len() == 1 && texts(&state).is_empty()
    })
    .await;
    assert!(std::path::Path::new(&kept).is_file());

    state.store.delete_session("s").unwrap();
    state.forget_cascaded_sessions(&["s"]);
    assert!(!std::path::Path::new(&kept).exists());
    assert!(!state.composer_dir("s").exists());
}

/// Withdrawing removes the files the message holds in the composer directory and nothing else:
/// a path it names outside it (a file given as context) is the user's own.
#[tokio::test]
async fn withdrawing_removes_only_files_in_the_composer_directory() {
    let (state, dir) = bare("composer-withdraw-files", SessionStatus::Interrupted);
    let context = dir.join("context.txt");
    std::fs::write(&context, "mine").unwrap();
    let id = queue(
        &state,
        Content {
            files: vec![context.to_string_lossy().into_owned()],
            ..text("see")
        },
        SendMode::Manual,
    )
    .await;
    withdraw(&state, "s", &id).unwrap();
    assert_eq!(std::fs::read_to_string(&context).unwrap(), "mine");

    // Not even by a path that starts in the directory and climbs out of it.
    let sneaky = format!("{}/../../context.txt", state.composer_dir("s").display());
    std::fs::create_dir_all(state.composer_dir("s")).unwrap();
    let id = queue(
        &state,
        Content {
            files: vec![sneaky],
            ..text("sneaky")
        },
        SendMode::Manual,
    )
    .await;
    withdraw(&state, "s", &id).unwrap();
    assert!(context.exists());
}

/// A message being handed over (begun, no ticket yet) cannot be withdrawn: the hand-over would
/// still write it. One a send now waits to hand over once its resumed session can take it can.
#[tokio::test]
async fn a_message_being_handed_over_cannot_be_withdrawn_but_one_waiting_on_a_resume_can() {
    let (state, _dir) = bare("composer-withdraw-in-flight", SessionStatus::Interrupted);
    let flying = queue(&state, text("in flight"), SendMode::WhenIdle).await;
    let resuming = queue(&state, text("after resume"), SendMode::Manual).await;
    state
        .composer_begin_send("s", &flying, How::Idle, false)
        .unwrap();
    state
        .composer_begin_send("s", &resuming, How::Now, true)
        .unwrap();
    let refused = withdraw(&state, "s", &flying).unwrap_err();
    assert_eq!(code_of(&refused), error_code::QUEUED_MESSAGE_SENDING);
    withdraw(&state, "s", &resuming).unwrap();
    assert_eq!(texts(&state), ["in flight"]);
    // The ticket delivery gives it later finds it no longer handed over.
    assert!(!state.composer_note_ticket(&resuming, 41));
}

/// A hand-over whose message was withdrawn before delivery queued it takes it back out of
/// delivery, so nothing is written.
#[tokio::test]
async fn a_hand_over_of_a_withdrawn_message_takes_it_back_out_of_delivery() {
    let (state, agent, _dir) = running("composer-note-withdrawn", SessionStatus::Idle);
    let _hold = state.hold_outbox("s");
    let refused = delivery::write_queued_off_runtime(
        &state,
        "s",
        "gone".to_string(),
        None,
        "withdrawn-meanwhile".to_string(),
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(&refused), error_code::QUEUED_MESSAGE_NOT_FOUND);
    assert_eq!(state.outbox.len("s"), 0);
    assert!(sent(&agent).is_empty());
}

/// What delivery reports of a ticket before the composer queue is given it is applied once it
/// is, rather than lost.
#[tokio::test]
async fn an_outcome_reported_before_the_ticket_is_noted_is_applied_when_it_is() {
    let (state, _dir) = bare("composer-early-outcome", SessionStatus::Interrupted);
    let id = queue(&state, text("early"), SendMode::WhenIdle).await;
    state
        .composer_begin_send("s", &id, How::Idle, false)
        .unwrap();
    state.composer_outcome("s", 5, Outcome::Written);
    assert_eq!(texts(&state), ["early"], "nothing placed yet");
    assert!(!state.composer_note_ticket(&id, 5), "placed as it is noted");
    assert!(texts(&state).is_empty());
}

/// A send now whose resume was made, on a session that ends again before it takes the message,
/// comes back refused (`session_not_running`) with its mode kept, whether the session had ended
/// by the time the resume returned or ends later.
#[tokio::test]
async fn a_send_now_whose_resumed_session_ends_comes_back_refused() {
    let (state, _dir) = bare("composer-resume-ended-at-once", SessionStatus::Interrupted);
    let id = queue(&state, text("lost resume"), SendMode::WhenIdle).await;
    send_with(&state, "s", &id, |_, _| async { Ok(()) })
        .await
        .unwrap();
    let back = message(&state, &id);
    assert_eq!(back.state, QueuedMessageState::Waiting);
    assert_eq!(back.mode, QueuedMode::WhenIdle);
    let failure = back.failure.expect("a failure");
    assert_eq!(failure.kind, QueuedMessageFailureKind::Refused);
    assert_eq!(failure.code, "session_not_running");

    let (state, _dir) = bare("composer-resume-ended-later", SessionStatus::Interrupted);
    let id = queue(&state, text("later"), SendMode::Manual).await;
    send_with(&state, "s", &id, |state, _| async move {
        set_status(&state, SessionStatus::Starting);
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(message(&state, &id).state, QueuedMessageState::Sending);
    state.interrupt_session_record("s").unwrap();
    let back = message(&state, &id);
    assert_eq!(back.state, QueuedMessageState::Waiting);
    assert_eq!(back.failure.unwrap().code, "session_not_running");
}

/// Send now on a session that reads as running but has no process registered (still starting)
/// does not resume it: delivery answers, and the message keeps its mode.
#[tokio::test]
async fn send_now_on_a_session_starting_hands_over_without_resuming() {
    let (state, _dir) = bare("composer-send-starting", SessionStatus::Working);
    let id = queue(&state, text("early bird"), SendMode::WhenIdle).await;
    let refused = send_with(&state, "s", &id, |_, _| async {
        panic!("a session that is not interrupted is not resumed")
    })
    .await
    .unwrap_err();
    assert_eq!(code_of(&refused), error_code::SESSION_NOT_RUNNING);
    let back = message(&state, &id);
    assert_eq!(back.mode, QueuedMode::WhenIdle);
    assert_eq!(
        back.failure.unwrap().kind,
        QueuedMessageFailureKind::Refused
    );
}

/// A slash command the daemon sent at an idle lets the next `when_idle` message go only once the
/// session still reads idle a settle period after it was written (a command may start a turn
/// whose report comes a moment late, or none at all), or once a turn has shown.
#[tokio::test]
async fn a_command_sent_at_an_idle_lets_the_next_message_go() {
    let (state, _dir) = bare("composer-command-idle", SessionStatus::Interrupted);
    let command = QueuedMessage {
        command: true,
        ..message(
            &state,
            &queue(&state, text("/status"), SendMode::WhenIdle).await,
        )
    };
    // Marked as a command directly: the test provider has no session to check it against.
    state
        .composer_update("s", &command.id, |message| {
            message.command = true;
            Ok(())
        })
        .unwrap();
    queue(&state, text("then this"), SendMode::WhenIdle).await;
    let taken = state.composer_take_idle("s").expect("the command goes");
    assert_eq!(taken.id, command.id);
    tokio::time::pause();
    state.composer_note_ticket(&taken.id, 11);
    state.composer_outcome("s", 11, Outcome::Written);
    assert!(
        state.composer_take_idle("s").is_none(),
        "not at once: a turn the command starts may not have shown yet"
    );
    assert!(!state.composer_could_dispatch("s", true));
    tokio::time::advance(COMMAND_SETTLE).await;
    assert!(state.composer_could_dispatch("s", true));
    assert!(
        !state.composer_could_dispatch("s", false),
        "only while idle"
    );
    let next = state
        .composer_take_idle("s")
        .expect("the next goes once settled");
    assert_eq!(next.text, "then this");
}

/// A slash command sent now holds a waiting `when_idle` message back as one sent at an idle does:
/// the drain that writes it finds the session still idle before any turn it starts has shown.
#[tokio::test]
async fn a_command_sent_now_holds_back_the_next_idle_message() {
    let (state, _dir) = bare("composer-command-now", SessionStatus::Interrupted);
    let id = queue(&state, text("/review"), SendMode::Manual).await;
    state
        .composer_update("s", &id, |message| {
            message.command = true;
            Ok(())
        })
        .unwrap();
    state
        .composer_begin_send("s", &id, How::Now, false)
        .unwrap();
    queue(&state, text("do X"), SendMode::WhenIdle).await;
    tokio::time::pause();
    state.composer_note_ticket(&id, 14);
    state.composer_outcome("s", 14, Outcome::Written);
    assert!(
        state.composer_take_idle("s").is_none(),
        "not into the turn the command may have started"
    );
    assert!(!state.composer_could_dispatch("s", true));
    tokio::time::advance(COMMAND_SETTLE).await;
    assert_eq!(state.composer_take_idle("s").unwrap().text, "do X");
}

/// A turn showing after a command ends its idle as it ends any other: the next idle may have the
/// next message without waiting out the settle period.
#[tokio::test]
async fn a_turn_after_a_command_lets_the_next_message_go_at_the_next_idle() {
    let (state, _dir) = bare("composer-command-turn", SessionStatus::Interrupted);
    let id = queue(&state, text("/review"), SendMode::WhenIdle).await;
    state
        .composer_update("s", &id, |message| {
            message.command = true;
            Ok(())
        })
        .unwrap();
    queue(&state, text("do X"), SendMode::WhenIdle).await;
    let taken = state.composer_take_idle("s").unwrap();
    state.composer_note_ticket(&taken.id, 12);
    state.composer_outcome("s", 12, Outcome::Written);
    assert!(state.composer_take_idle("s").is_none());
    state.composer_idle_over("s");
    assert_eq!(state.composer_take_idle("s").unwrap().text, "do X");
}

/// A retry drains a session for the composer queue only where a message could go now: not while
/// it works, not once its idle had its message, and while a send now waits on a resume.
#[tokio::test]
async fn the_composer_queue_asks_a_retry_to_drain_only_where_a_message_could_go() {
    let (state, _dir) = bare("composer-could-dispatch", SessionStatus::Interrupted);
    assert!(!state.composer_could_dispatch("s", true), "nothing queued");
    queue(&state, text("idle one"), SendMode::WhenIdle).await;
    let manual = queue(&state, text("manual"), SendMode::Manual).await;
    assert!(state.composer_could_dispatch("s", true));
    assert!(!state.composer_could_dispatch("s", false), "working");
    let taken = state.composer_take_idle("s").unwrap();
    assert!(
        !state.composer_could_dispatch("s", true),
        "being handed over"
    );
    state.composer_note_ticket(&taken.id, 13);
    state.composer_outcome("s", 13, Outcome::Written);
    queue(&state, text("next"), SendMode::WhenIdle).await;
    assert!(
        !state.composer_could_dispatch("s", true),
        "the idle had its message"
    );
    state
        .composer_begin_send("s", &manual, How::Now, true)
        .unwrap();
    assert!(
        state.composer_could_dispatch("s", false),
        "a send now waits on a resume"
    );
}

/// The files a message's own create moved in are known as its own across a restart, so
/// withdrawing it then still removes them.
#[tokio::test]
async fn a_messages_own_files_are_removed_on_withdrawal_after_a_restart() {
    let (state, _agent, mut dir) = running("composer-owned-restart", SessionStatus::Working);
    let saved = crate::session::pasted_image::save_bytes(&state, "s", b"\x89PNG\r\n\x1a\nown")
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let id = queue(
        &state,
        Content {
            images: vec![saved],
            ..text("mine")
        },
        SendMode::Manual,
    )
    .await;
    let moved = message(&state, &id).images[0].clone();
    assert!(
        std::path::Path::new(&moved).is_file(),
        "in place as the queue shows it"
    );
    let again = app_state_again(&mut dir);
    withdraw(&again, "s", &id).unwrap();
    assert!(!std::path::Path::new(&moved).exists());
}

/// A create the queue refuses (full) moves its files back where they were.
#[tokio::test]
async fn a_create_the_queue_refuses_moves_its_files_back() {
    let (state, _agent, _dir) = running("composer-full-moves-back", SessionStatus::Working);
    for n in 0..crate::state::composer::MAX_QUEUED {
        queue(&state, text(&n.to_string()), SendMode::Manual).await;
    }
    let saved = crate::session::pasted_image::save_bytes(&state, "s", b"\x89PNG\r\n\x1a\nback")
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let refused = create(
        &state,
        "s",
        Content {
            images: vec![saved.clone()],
            ..text("one too many")
        },
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(&refused), error_code::LIMIT_EXCEEDED);
    assert!(std::path::Path::new(&saved).is_file(), "moved back");
    let pasted = state.composer_dir("s").join("pasted");
    assert!(
        !pasted.exists() || std::fs::read_dir(&pasted).unwrap().next().is_none(),
        "nothing left in the composer directory"
    );
}

/// An edit writes only the fields it gives: one giving only files keeps the text and the command
/// flag the message has.
#[tokio::test]
async fn an_edit_writes_only_the_fields_it_gives() {
    let (state, _dir) = bare("composer-edit-given", SessionStatus::Interrupted);
    let id = queue(&state, text("first text"), SendMode::Manual).await;
    update(
        &state,
        "s",
        &id,
        None,
        None,
        Some(vec!["a.md".to_string()]),
        None,
    )
    .await
    .unwrap();
    let edited = message(&state, &id);
    assert_eq!(edited.text, "first text");
    assert_eq!(edited.files, ["a.md"]);
    assert!(!edited.command);
}

/// An edit that makes a message a command without giving its text writes the text as a command
/// is sent: trimmed, its control characters taken out.
#[tokio::test]
async fn an_edit_making_a_message_a_command_writes_its_text_normalised() {
    let (state, _agent, _dir) = running("composer-edit-command", SessionStatus::Idle);
    let id = queue(&state, text("  /review this\u{7} "), SendMode::Manual).await;
    update(&state, "s", &id, None, None, None, Some(true))
        .await
        .unwrap();
    let edited = message(&state, &id);
    assert!(edited.command);
    assert_eq!(edited.text, "/review this");
}

/// A message told as not carried out comes back with the very text of the notice the user is
/// told with.
#[tokio::test]
async fn a_message_not_carried_out_carries_the_notices_own_text() {
    let (state, _dir) = bare("composer-notice-text", SessionStatus::Interrupted);
    let id = queue(&state, text("lost"), SendMode::WhenIdle).await;
    state
        .composer_begin_send("s", &id, How::Idle, false)
        .unwrap();
    state.composer_note_ticket(&id, 31);
    state.composer_outcome("s", 31, Outcome::Written);
    state.tell_senders_of(
        "s",
        AgentId::TEST,
        SenderNote::NotTaken,
        &[(Some("launch:31".to_string()), "lost".to_string())],
    );
    let failure = message(&state, &id).failure.unwrap();
    assert_eq!(
        failure.message,
        crate::messaging::untaken_messages::not_taken_notice(AgentId::TEST, "lost").message
    );
}

/// Holds lifted with nothing in delivery's queue still have the composer queue looked at.
#[tokio::test]
async fn a_pending_flush_with_an_empty_outbox_still_dispatches_the_composer_queue() {
    let (state, agent, _dir) = running("composer-pending-flush", SessionStatus::Working);
    queue(&state, text("waited"), SendMode::WhenIdle).await;
    set_status(&state, SessionStatus::Idle);
    let flushing = state.clone();
    tokio::task::spawn_blocking(move || flushing.flush_pending_outbox("s"))
        .await
        .unwrap();
    until("it is sent", || sent(&agent) == ["SENT waited"]).await;
}

/// A refused create or update moves no file: every image is checked first, and files are moved
/// only once the queue has taken the change.
#[tokio::test]
async fn a_refused_create_or_update_moves_no_file() {
    let (state, _agent, _dir) = running("composer-refused-moves", SessionStatus::Working);
    let save = |n: u8| {
        crate::session::pasted_image::save_bytes(
            &state,
            "s",
            &[b"\x89PNG\r\n\x1a\n".as_slice(), &[n]].concat(),
        )
        .unwrap()
        .to_string_lossy()
        .into_owned()
    };
    let valid = save(1);
    let refused = create(
        &state,
        "s",
        Content {
            images: vec![valid.clone(), "/etc/passwd".to_string()],
            ..text("mixed")
        },
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(&refused), error_code::UNREADABLE_REQUEST);
    assert!(
        std::path::Path::new(&valid).is_file(),
        "the valid one stays"
    );

    let id = queue(&state, text("held"), SendMode::Manual).await;
    state.composer_begin_send("s", &id, How::Now, true).unwrap();
    let refused = update(
        &state,
        "s",
        &id,
        None,
        Some(vec![valid.clone()]),
        None,
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(&refused), error_code::QUEUED_MESSAGE_SENDING);
    assert!(std::path::Path::new(&valid).is_file());
    assert!(
        !state.composer_dir("s").join("pasted").exists()
            || std::fs::read_dir(state.composer_dir("s").join("pasted"))
                .unwrap()
                .next()
                .is_none(),
        "no claim is left behind"
    );
}

/// Withdrawing or editing removes only the files the message's own create or update moved in,
/// and none of a message delivery wrote and reported not carried out.
#[tokio::test]
async fn only_a_messages_own_files_are_removed_and_none_of_one_written() {
    let (state, _agent, _dir) = running("composer-own-files", SessionStatus::Working);
    let saved = crate::session::pasted_image::save_bytes(&state, "s", b"\x89PNG\r\n\x1a\nown")
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let first = queue(
        &state,
        Content {
            images: vec![saved],
            ..text("first")
        },
        SendMode::Manual,
    )
    .await;
    let moved = message(&state, &first).images[0].clone();
    // A second message naming the same file, as a client copying the path would.
    let second = queue(
        &state,
        Content {
            images: vec![moved.clone()],
            ..text("second")
        },
        SendMode::Manual,
    )
    .await;
    withdraw(&state, "s", &second).unwrap();
    assert!(
        std::path::Path::new(&moved).is_file(),
        "not the second's own"
    );

    // Written and reported not carried out: the agent may still read it.
    state
        .composer_begin_send("s", &first, How::Now, false)
        .unwrap();
    state.composer_note_ticket(&first, 21);
    state.composer_not_carried_out(
        "s",
        21,
        crate::state::composer::failure_of_notice("message_not_taken", Default::default(), "x"),
    );
    withdraw(&state, "s", &first).unwrap();
    assert!(std::path::Path::new(&moved).is_file(), "kept for the agent");
}

/// A file's path is quoted as the UI's `pastedPath` quotes a dropped one: an empty path, or one
/// with no whitespace, bare.
#[test]
fn paths_are_quoted_as_the_ui_pastes_them() {
    for (path, expected) in [
        ("", ""),
        ("a.rs", "a.rs"),
        ("a b.rs", "'a b.rs'"),
        ("it's x", "'it'\\''s x'"),
        ("tab\there", "'tab\there'"),
        ("it's", "it's"),
    ] {
        assert_eq!(quoted(path), expected, "{path:?}");
    }
}
