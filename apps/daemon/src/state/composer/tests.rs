//! The composer queue's state as delivery and the coordinator drive it: where a message goes
//! back to, what a late report does to it, and what the daemon's own dispatch may take.

use std::sync::Arc;

use super::*;
use crate::protocol::SessionStatus;
use crate::state::test_support::insert_record;
use crate::test_support::{app_state, code_of, ScratchDir};

fn state_with_session(label: &str) -> (Arc<AppState>, ScratchDir) {
    let (state, dir) = app_state(label);
    insert_record(&state, &dir, "s", SessionStatus::Interrupted);
    (state, dir)
}

fn message(id: &str, mode: QueuedMode) -> QueuedMessage {
    QueuedMessage {
        id: id.to_string(),
        text: id.to_string(),
        images: Vec::new(),
        files: Vec::new(),
        command: false,
        mode,
        state: QueuedMessageState::Waiting,
        failure: None,
        created_at: 1,
    }
}

fn add(state: &AppState, ids: &[&str], mode: QueuedMode) {
    for id in ids {
        state
            .composer_add("s", message(id, mode), Vec::new())
            .unwrap();
    }
}

fn ids(state: &AppState) -> Vec<String> {
    state
        .queued_messages("s")
        .unwrap()
        .into_iter()
        .map(|message| message.id)
        .collect()
}

fn lost() -> QueuedMessageFailure {
    failure_of_notice("message_not_taken", Default::default(), "not taken")
}

/// Hands `id` over as an idle dispatch and has delivery write it under `ticket`.
fn written(state: &AppState, id: &str, ticket: u64) {
    state
        .composer_begin_send("s", id, How::Idle, false)
        .unwrap();
    state.composer_note_ticket(id, ticket);
    state.composer_outcome("s", ticket, Outcome::Written);
}

/// A message written from the queue and reported not carried out comes back to its place among
/// the messages that were next to it: after the one that was before it, else before the one
/// after it, else where it stood; and it comes back once, whatever is reported again.
#[test]
fn a_message_not_carried_out_goes_back_next_to_where_it_stood() {
    let (state, _dir) = state_with_session("composer-state-place");
    add(&state, &["a", "b", "c", "d"], QueuedMode::Manual);
    state
        .composer_update("s", "b", |m| {
            m.mode = QueuedMode::WhenIdle;
            Ok(())
        })
        .unwrap();
    written(&state, "b", 1);
    assert_eq!(ids(&state), ["a", "c", "d"]);
    state.composer_withdraw("s", "c", |_| true).unwrap();
    state.composer_move("s", "d", 0).unwrap();
    // `a` was before it and is still there.
    state.composer_returned_late("s", 1, Some(lost()));
    state.composer_returned_late("s", 1, Some(lost()));
    assert_eq!(ids(&state), ["d", "a", "b"]);

    // With the message before it gone, it goes before the one that was after it.
    let (state, _dir) = state_with_session("composer-state-place-after");
    add(&state, &["a", "b", "c"], QueuedMode::Manual);
    state
        .composer_update("s", "b", |m| {
            m.mode = QueuedMode::WhenIdle;
            Ok(())
        })
        .unwrap();
    written(&state, "b", 2);
    state.composer_withdraw("s", "a", |_| true).unwrap();
    state.composer_returned_late("s", 2, Some(lost()));
    assert_eq!(ids(&state), ["b", "c"]);

    // With both gone, where it stood, as far as the queue reaches.
    let (state, _dir) = state_with_session("composer-state-place-alone");
    add(&state, &["a", "b", "c"], QueuedMode::Manual);
    written(&state, "c", 3);
    state.composer_withdraw("s", "b", |_| true).unwrap();
    state.composer_withdraw("s", "a", |_| true).unwrap();
    state.composer_returned_late("s", 3, Some(lost()));
    assert_eq!(ids(&state), ["c"]);
}

/// The daemon's own dispatch takes the first waiting `when_idle` message, one per idle; one that
/// went back from delivery is manual and is never taken again, while the message behind it may
/// go in the same idle.
#[test]
fn the_idle_dispatch_takes_one_message_per_idle_and_never_a_returned_one() {
    let (state, _dir) = state_with_session("composer-state-dispatch");
    add(&state, &["manual"], QueuedMode::Manual);
    add(&state, &["first", "second", "third"], QueuedMode::WhenIdle);

    let taken = state.composer_take_idle("s").expect("the first one");
    assert_eq!(taken.id, "first");
    assert_eq!(taken.state, QueuedMessageState::Sending);
    assert!(state.composer_take_idle("s").is_none(), "one at a time");
    state.composer_note_ticket("first", 1);
    state.composer_outcome("s", 1, Outcome::Written);
    assert!(state.composer_take_idle("s").is_none(), "one per idle");

    // Reported not carried out in the same idle: the next may go, the returned one may not.
    state.composer_returned_late("s", 1, Some(lost()));
    let returned = state
        .queued_messages("s")
        .unwrap()
        .into_iter()
        .find(|message| message.id == "first")
        .unwrap();
    assert_eq!(returned.mode, QueuedMode::Manual);
    assert_eq!(returned.state, QueuedMessageState::Waiting);
    assert_eq!(state.composer_take_idle("s").unwrap().id, "second");
    state.composer_back("s", "second", None, None);

    // A new idle: still the second, never a manual message.
    state.composer_idle_over("s");
    assert_eq!(state.composer_take_idle("s").unwrap().id, "second");
    state.composer_note_ticket("second", 2);
    state.composer_outcome("s", 2, Outcome::Written);
    state.composer_idle_over("s");
    assert_eq!(state.composer_take_idle("s").unwrap().id, "third");
    state.composer_note_ticket("third", 3);
    state.composer_outcome("s", 3, Outcome::Written);
    state.composer_idle_over("s");
    assert!(
        state.composer_take_idle("s").is_none(),
        "only manual ones are left"
    );
    assert!(!state.composer_has_work("s"));
    assert_eq!(ids(&state), ["manual", "first"]);
}

/// A report that the agent took the message after all takes back a message still as it came
/// back, and leaves one the user has changed since; a report of a ticket that is no queued
/// message's changes nothing.
#[test]
fn a_late_report_that_it_ran_takes_back_only_an_unchanged_message() {
    let (state, _dir) = state_with_session("composer-state-ran");
    add(&state, &["a", "b"], QueuedMode::WhenIdle);
    written(&state, "a", 1);
    state.composer_returned_late("s", 1, Some(lost()));
    assert_eq!(ids(&state), ["a", "b"]);
    state.composer_returned_late("s", 1, None);
    assert_eq!(ids(&state), ["b"], "unchanged, it leaves");
    state.composer_returned_late("s", 1, None);
    assert_eq!(ids(&state), ["b"]);

    state.composer_idle_over("s");
    written(&state, "b", 2);
    state.composer_returned_late("s", 2, Some(lost()));
    state
        .composer_update("s", "b", |m| {
            m.text = "changed".to_string();
            Ok(())
        })
        .unwrap();
    state.composer_returned_late("s", 2, None);
    assert_eq!(ids(&state), ["b"], "changed, it stays");

    // Nothing for tickets that no message owns.
    state.composer_outcome("s", 99, Outcome::Written);
    state.composer_outcome("s", 99, Outcome::Back(lost()));
    state.composer_returned_late("s", 99, Some(lost()));
    assert_eq!(ids(&state), ["b"]);
}

/// What delivery held and then lost is put back to waiting, with no failure; what it still
/// holds, and what only waits for a resumed session, is left.
#[test]
fn a_reconcile_puts_back_what_delivery_no_longer_holds() {
    let (state, _dir) = state_with_session("composer-state-reconcile");
    add(&state, &["held", "lost", "resuming"], QueuedMode::Manual);
    for (id, ticket) in [("held", 1), ("lost", 2)] {
        state.composer_begin_send("s", id, How::Now, false).unwrap();
        state.composer_note_ticket(id, ticket);
    }
    state
        .composer_begin_send("s", "resuming", How::Now, true)
        .unwrap();
    state.composer_reconcile("s", |ticket| ticket == 1);
    let by_id = |id: &str| {
        state
            .queued_messages("s")
            .unwrap()
            .into_iter()
            .find(|message| message.id == id)
            .unwrap()
    };
    assert_eq!(by_id("held").state, QueuedMessageState::Sending);
    assert_eq!(by_id("lost").state, QueuedMessageState::Waiting);
    assert_eq!(by_id("lost").failure, None);
    assert_eq!(by_id("resuming").state, QueuedMessageState::Sending);
    assert!(state.composer_has_work("s"), "the resume is waited for");
    let taken = state.composer_take_after_resume("s");
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].id, "resuming");
    assert!(!state.composer_has_work("s"));
}

/// A message being handed over cannot be changed, moved aside as waiting, or begun again; one
/// refused is told so by its code and the ids it names.
#[test]
fn a_message_being_handed_over_is_refused_every_change() {
    let (state, _dir) = state_with_session("composer-state-sending");
    add(&state, &["a"], QueuedMode::Manual);
    state
        .composer_begin_send("s", "a", How::Now, false)
        .unwrap();
    let refused = state
        .composer_begin_send("s", "a", How::Now, false)
        .unwrap_err();
    assert_eq!(code_of(&refused), error_code::QUEUED_MESSAGE_SENDING);
    let refused = state.composer_update("s", "a", |_| Ok(())).unwrap_err();
    assert_eq!(code_of(&refused), error_code::QUEUED_MESSAGE_SENDING);
    let refused = state.composer_move("s", "nope", 0).unwrap_err();
    assert_eq!(code_of(&refused), error_code::QUEUED_MESSAGE_NOT_FOUND);
    let refused = state
        .composer_begin_send("other", "a", How::Now, false)
        .unwrap_err();
    assert_eq!(code_of(&refused), error_code::QUEUED_MESSAGE_NOT_FOUND);
}

/// Each change is broadcast as the whole queue, one that changes nothing is not, and the
/// session's queue that has emptied is broadcast empty.
#[test]
fn every_change_to_a_queue_is_broadcast_whole() {
    let (state, _dir) = state_with_session("composer-state-broadcast");
    let mut events = state.subscribe();
    let mut seen = || {
        let mut queues = Vec::new();
        while let Ok(event) = events.try_recv() {
            if let Event::SessionMessageQueue { session, messages } = event {
                assert_eq!(session, "s");
                queues.push(messages.into_iter().map(|m| m.id).collect::<Vec<_>>());
            }
        }
        queues
    };
    add(&state, &["a", "b"], QueuedMode::Manual);
    assert_eq!(seen(), [vec!["a"], vec!["a", "b"]]);
    state.composer_move("s", "b", 0).unwrap();
    assert_eq!(seen(), [vec!["b", "a"]]);
    state.composer_move("s", "b", 0).unwrap();
    assert!(seen().is_empty(), "nothing changed, nothing broadcast");
    state.composer_withdraw("s", "b", |_| true).unwrap();
    state.composer_withdraw("s", "a", |_| true).unwrap();
    assert_eq!(seen(), [vec!["a"], Vec::<&str>::new()]);
}

/// What an archive hands back for a message: its text, then its images, then its files, one to a
/// line, with no empty line for a message of images or files alone.
#[test]
fn what_an_archive_hands_back_is_the_text_then_the_paths_a_line_each() {
    let with = |text: &str, images: &[&str], files: &[&str]| QueuedMessage {
        text: text.to_string(),
        images: images.iter().map(|s| s.to_string()).collect(),
        files: files.iter().map(|s| s.to_string()).collect(),
        ..message("m", QueuedMode::Manual)
    };
    assert_eq!(content_of(&with("t", &[], &[])), "t");
    assert_eq!(content_of(&with("t", &["/i.png"], &["f"])), "t\n/i.png\nf");
    assert_eq!(content_of(&with("", &["/i.png"], &[])), "/i.png");
    assert_eq!(content_of(&with("", &[], &["a", "b"])), "a\nb");
}

/// A session's queue and what is held for it go with the session, files and all.
#[test]
fn forgetting_a_session_drops_its_queue_and_its_directory() {
    let (state, _dir) = state_with_session("composer-state-forget");
    add(&state, &["a"], QueuedMode::Manual);
    let directory = state.composer_dir("s");
    std::fs::create_dir_all(directory.join("pasted")).unwrap();
    std::fs::write(directory.join("pasted/paste-1.png"), "x").unwrap();
    state.composer_forget("s");
    assert!(ids(&state).is_empty());
    assert!(!directory.exists());
    // A session that never had a directory is forgotten all the same.
    state.composer_forget("s");
}
