//! Watching a GitHub Copilot CLI session's record for what its hooks do not report
//! (`docs/agent-cli-reference.md`, "GitHub Copilot CLI"): the `abort` line a cancelled turn or a
//! declined prompt leaves, the `permission.completed` line of a prompt the user answered with
//! what to do differently, or of a subagent's prompt the user answered either way, after which
//! the turn carries on with no hook of the session's, and the `session.error` line of a turn
//! that ended in an error, after which no `agentStop` runs.
//!
//! No hook payload names the record before a turn's `agentStop`, so the record is found from the
//! session itself ([`record_of`]): `session-state/<agent session id>/events.jsonl` under its
//! account's directory.
//!
//! A raised hand is checked against what the record holds ([`asked_in`]), for two things the
//! hooks get wrong on their own. The record is read a moment after the hand went up
//! ([`RECORD_SETTLE`]), not at once: Copilot writes it out in batches, so the lines of the prompt a
//! notification is about are not in the file yet when the notification arrives. A `permission_prompt` notification names no call, so
//! the end of another call of the same message would lower the hand while the prompt is still up:
//! the call the record says is asked about is held as waiting by its id. And Copilot does not
//! wait for a notification hook, so one can land after the post event of its call, or after the
//! turn's end: a hand raised for a prompt the record says is answered is lowered again, unless a
//! call is still held as waiting, which says the record does not show every prompt yet.
//!
//! Each watch reads only what is written after it is armed, so an `abort` or an error of an
//! earlier turn, or of a conversation resumed with its history, is never taken for this turn's. A
//! cancel that lands before the watch is armed is missed, and the session reads as working until
//! its next prompt.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::record_lines::{
    asked_in, ending_in_line, record_of, record_tail, Asked, Ending, PROMPT_WINDOW, RECORD_SETTLE,
};
use crate::hooks::{RecordWatch, ToolCallChange};
use crate::protocol::SessionStatus;
use crate::records::watch::{self, Owed, Start, Then, Watch};
use crate::state::AppState;

const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Starts the watch `watch` asks for, over the record of `session_id`.
pub(super) fn arm(state: &Arc<AppState>, session_id: &str, watch: RecordWatch<'_>) {
    let expected = match watch {
        RecordWatch::Decline { .. } => SessionStatus::WaitingUser,
        RecordWatch::Interrupt { .. } => SessionStatus::Working,
        RecordWatch::Failure { .. } => return,
    };
    let Some(record) = record_of(state, session_id) else {
        tracing::debug!(
            session = %session_id,
            "the session's record could not be placed; it cannot be watched"
        );
        return;
    };
    let looked_back = (expected == SessionStatus::WaitingUser).then(|| record.clone());
    let generation = watch::spawn(
        state,
        session_id,
        record,
        Watch {
            interval: POLL_INTERVAL,
            start: Start::AtEndOrNew,
        },
        move |state, session_id| still_at(state, session_id, expected),
        ending_in_line,
        move |state, session_id, generation, ending| {
            match ending {
                Ending::Aborted => {
                    watch::back_at_prompt(state, session_id, generation, expected, "record abort")
                }
                Ending::Refused => {
                    // No post event runs for a call the user refused, so nothing else takes it
                    // off the calls held as waiting.
                    state.apply_tool_call_change(session_id, ToolCallChange::Ended { id: None });
                    working_again(state, session_id, generation)
                }
                Ending::Answered => working_again(state, session_id, generation),
                Ending::Failed(message) => {
                    watch::end_failed_turn(state, session_id, generation, message)
                }
            }
            Then::Stop
        },
    );
    if let (Some(record), Some(generation)) = (looked_back, generation) {
        look_back_once_written(state, session_id, record, generation);
    }
}

/// Reads what the record says of the prompt a hand was raised for, once the lines of that prompt
/// have had time to be written ([`RECORD_SETTLE`]), and acts on it while watch `generation`, armed
/// with the hand, is still the session's: a hook that moved the session on has replaced it.
fn look_back_once_written(
    state: &Arc<AppState>,
    session_id: &str,
    record: PathBuf,
    generation: u64,
) {
    let (state, session_id) = (state.clone(), session_id.to_string());
    tokio::spawn(async move {
        tokio::time::sleep(RECORD_SETTLE).await;
        if !state.record_watch_current(&session_id, generation) {
            return;
        }
        let asked = tokio::task::spawn_blocking(move || {
            record_tail(&record, PROMPT_WINDOW).map(|tail| asked_in(&tail))
        })
        .await
        .ok()
        .flatten();
        if let Some(asked) = asked {
            look_back(&state, &session_id, generation, asked);
        }
    });
}

fn look_back(state: &Arc<AppState>, session_id: &str, generation: u64, asked: Asked) {
    if !state.record_watch_current(session_id, generation)
        || still_at(state, session_id, SessionStatus::WaitingUser) != Owed::Yes
    {
        return;
    }
    match asked {
        Asked::Waiting(calls) => {
            for call in &calls {
                // Held by its id: the end of another call then leaves the hand up, and this
                // call's own post event lowers it.
                state.apply_tool_call_change(
                    session_id,
                    ToolCallChange::WaitsOnUserById(call.as_str()),
                );
            }
        }
        // A call still held as waiting says a prompt is up that the record does not show yet:
        // the hand stays.
        Asked::Answered { turn_over } if !state.holds_a_waiting_call(session_id) => {
            lower_late_hand(state, session_id, turn_over);
        }
        // The cancel or the error that took the prompt down ends the turn as the watch ends it
        // for one written after the hand went up; this one may have been written before.
        Asked::Aborted => watch::back_at_prompt(
            state,
            session_id,
            generation,
            SessionStatus::WaitingUser,
            "record abort",
        ),
        Asked::Failed(message) => watch::end_failed_turn(state, session_id, generation, message),
        Asked::Answered { .. } | Asked::Unknown => {}
    }
}

/// Lowers a hand a late event raised, for a prompt the record says was answered before the event
/// arrived: back to working while the turn carries on, whose cancel and error are then watched
/// for again, or to its prompt when the turn has ended since. Only while the session still waits
/// on the user, in one step, so that whatever moved it meanwhile stands, and beneath a session
/// request's hand where one is held (`AppState::report_idle_if_waiting`,
/// `AppState::report_working_if_waiting`): the board's hand then stays up for the request.
fn lower_late_hand(state: &Arc<AppState>, session_id: &str, turn_over: bool) {
    let (moved, status) = if turn_over {
        (
            state.report_idle_if_waiting(session_id),
            SessionStatus::Idle,
        )
    } else {
        (
            state.report_working_if_waiting(session_id),
            SessionStatus::Working,
        )
    };
    match moved {
        Ok(true) => {
            tracing::debug!(
                session = %session_id,
                "a prompt's notification came after its answer; the hand it raised is lowered"
            );
            state.spawn_flush_outbox(session_id, status);
            if !turn_over {
                arm(
                    state,
                    session_id,
                    RecordWatch::Interrupt {
                        record: None,
                        turn_id: None,
                    },
                );
            }
        }
        Ok(false) => {}
        Err(err) => tracing::debug!(
            session = %session_id, %err,
            "lowering the hand a late notification raised failed"
        ),
    }
}

/// Puts a session whose hand is up back to working, for a prompt answered in a way no hook of
/// the session's reports, while watch `generation` is still current. The agent's status moves
/// only if it is still waiting on the user, in one step, so that a turn's end that came
/// meanwhile is not undone, and beneath a session request's hand where one is held
/// (`AppState::report_working_if_waiting`). What was held for the session while its hand was up
/// may then go. The turn carries on from here with no hook to say so, so its cancel and its
/// error are watched for again.
fn working_again(state: &Arc<AppState>, session_id: &str, generation: u64) {
    let moved = state.while_record_watch_current(session_id, generation, || {
        state.report_working_if_waiting(session_id)
    });
    match moved {
        Some(Ok(true)) => {
            state.spawn_flush_outbox(session_id, SessionStatus::Working);
            // Armed outside the watch bookkeeping's lock, which starting a watch takes.
            arm(
                state,
                session_id,
                RecordWatch::Interrupt {
                    record: None,
                    turn_id: None,
                },
            );
        }
        Some(Ok(false)) | None => {}
        Some(Err(err)) => tracing::debug!(
            session = %session_id, %err,
            "returning the session to work after an answered permission failed"
        ),
    }
}

fn still_at(state: &AppState, session_id: &str, status: SessionStatus) -> Owed {
    match state.store.get_session(session_id) {
        Ok(Some(session)) if state.agent_status(&session) == status => Owed::Yes,
        // A hook has moved the session on, or it is gone.
        Ok(_) => Owed::No,
        Err(err) => {
            tracing::debug!(
                session = %session_id, %err,
                "reading the session failed while watching its record"
            );
            Owed::Unknown
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use serde_json::Value;

    use super::super::record_lines::record_in;
    use super::*;
    use crate::protocol::{AgentId, Console, Session, SessionKind};
    use crate::providers::copilot::COPILOT_EVENTS_FILE;
    use crate::state::TurnClose;
    use crate::test_support::{bare_session, idle_stand_in, idle_stand_in_of, ScratchDir, StandIn};

    /// Lines of real `events.jsonl` records of Copilot 1.0.114, several sessions one after another;
    /// the line numbers below are 1-based, as `testdata/copilot-captures.md` gives them.
    const RECORD: &str = include_str!("../../../testdata/copilot_events.jsonl");
    const CONVERSATION: &str = "0a6d3faa-bcc7-41aa-8108-05f3ff4b2528";

    /// Lines `from` to `to` of the record, each with its newline.
    fn lines(from: usize, to: usize) -> String {
        RECORD
            .split_inclusive('\n')
            .skip(from - 1)
            .take(to - from + 1)
            .collect()
    }

    fn line(number: usize) -> String {
        lines(number, number)
    }

    fn endings(from: usize, to: usize) -> Vec<Ending> {
        lines(from, to).lines().filter_map(ending_in_line).collect()
    }

    fn failed(message: &str) -> Ending {
        Ending::Failed(Some(message.to_string()))
    }

    /// What each real session's record says of how its turns ended: Esc twice and Ctrl-C and Esc at
    /// a prompt are an `abort`, a prompt refused with feedback is a `Refused` and the turn carries
    /// on, a model error and a model that answered with nothing are `session.error`s; and a
    /// permission approved, cancelled or a subagent's failure is no ending of the session's turn.
    #[test]
    fn each_real_session_ends_its_turn_as_its_record_says() {
        for (from, to, expected, what) in [
            (1, 11, vec![], "a permission approved"),
            (
                12,
                33,
                vec![Ending::Answered],
                "a subagent's permission prompt approved, its messages and turn end",
            ),
            (34, 48, vec![], "a subagent whose model request failed"),
            (
                49,
                58,
                vec![Ending::Aborted],
                "Esc twice with a subagent running",
            ),
            (59, 62, vec![Ending::Aborted], "Esc twice (user_abort)"),
            (63, 66, vec![Ending::Aborted], "Ctrl-C (user_initiated)"),
            (67, 74, vec![Ending::Aborted], "Esc at a permission prompt"),
            (75, 85, vec![Ending::Refused], "option 3 at a prompt"),
            (
                86,
                89,
                vec![failed("400 stand-in failure")],
                "a model error",
            ),
            (
                90,
                94,
                vec![failed(
                    "No response was returned. Send your message again to retry.",
                )],
                "a model answer with no text",
            ),
            (95, 103, vec![], "a tool-only message then a blank one"),
        ] {
            assert_eq!(endings(from, to), expected, "{what} (lines {from}-{to})");
        }
    }

    /// A subagent's lines carry `agentId` at the top level and never end the session's turn: its
    /// `session.error` (line 42) is one, and the same line without the id is the session's own.
    #[test]
    fn a_subagents_error_is_not_the_sessions_ending() {
        let subagents = line(42);
        assert!(subagents.contains("\"agentId\""));
        assert_eq!(ending_in_line(subagents.trim_end()), None);
        let value: Value = serde_json::from_str(&subagents).expect("a record line");
        let mut own = value.clone();
        own.as_object_mut().unwrap().remove("agentId");
        assert_eq!(
            ending_in_line(&own.to_string()),
            Some(failed("400 stand-in failure"))
        );
        // The same for a subagent's abort, built from the session's own (line 57).
        let mut abort: Value = serde_json::from_str(&line(57)).expect("a record line");
        assert_eq!(ending_in_line(&abort.to_string()), Some(Ending::Aborted));
        abort["agentId"] = value["agentId"].clone();
        assert_eq!(ending_in_line(&abort.to_string()), None);
    }

    /// The answer to a subagent's permission prompt is the one subagent line the session reads: an
    /// approval (line 22, as captured) and a refusal with feedback are `Answered`, a subagent's
    /// cancel is nothing, a session's own approval is nothing, and no other line of a subagent
    /// is read. The refusal and the cancel are line 22 with its `result.kind` changed.
    #[test]
    fn only_the_answer_to_a_subagents_permission_prompt_is_read_of_its_lines() {
        let approved = line(22);
        assert!(approved.contains("\"agentId\"") && approved.contains("\"approved\""));
        assert_eq!(ending_in_line(approved.trim_end()), Some(Ending::Answered));
        let with_kind = |kind: &str| {
            let mut value: Value = serde_json::from_str(&approved).expect("a record line");
            value["data"]["result"]["kind"] = Value::from(kind);
            value.to_string()
        };
        assert_eq!(
            ending_in_line(&with_kind("denied-interactively-by-user")),
            Some(Ending::Answered)
        );
        assert_eq!(ending_in_line(&with_kind("cancelled")), None);
        assert_eq!(ending_in_line(&with_kind("something-new")), None);
        // The session's own approval, as captured (line 6), is left to its call's post event.
        let own = line(6);
        assert!(!own.contains("\"agentId\""));
        assert_eq!(ending_in_line(own.trim_end()), None);
        // Every other line of the subagent in that session, its requests and messages, is set aside.
        for number in 16..=28 {
            if number == 22 {
                continue;
            }
            assert_eq!(
                ending_in_line(line(number).trim_end()),
                None,
                "line {number}"
            );
        }
    }

    /// Hand-built, since no capture holds them: a line that only mentions an ending, one cut short
    /// by a write in progress, a permission line with no result, and an error without its text.
    #[test]
    fn a_line_that_only_mentions_an_ending_is_not_one() {
        for line in [
            // The word is in the line, not as its type.
            "{\"type\":\"hook.start\",\"data\":{\"hookType\":\"abort\"}}",
            "{\"type\":\"user.message\",\"data\":{\"content\":\"abort\"}}",
            "{\"type\":\"user.message\",\"data\":{\"content\":\"denied-interactively-by-user\"}}",
            "{\"type\":\"assistant.message\",\"data\":{\"content\":\"a \\\"session.error\\\" is shown\"}}",
            "{\"type\":\"abort.requested\",\"data\":{}}",
            "{\"type\":\"permission.completed\"}",
            "{\"type\":\"permission.completed\",\"data\":{}}",
            "{\"type\":\"permission.completed\",\"data\":{\"result\":{}}}",
            "{\"type\":\"permission.completed\",\"data\":{\"result\":{\"kind\":\"approved\"}}}",
            "{\"type\":\"permission.requested\",\"data\":{\"kind\":\"denied-interactively-by-user\"}}",
            // A line that is not whole yet, as a write in progress leaves it.
            "{\"type\":\"abort\"",
            "\"abort\"",
            "abort",
            "",
        ] {
            assert_eq!(ending_in_line(line), None, "{line}");
        }
        for line in [
            "{\"type\":\"session.error\",\"data\":{\"statusCode\":500}}",
            "{\"type\":\"session.error\",\"data\":{\"message\":\"  \"}}",
            "{\"type\":\"session.error\"}",
        ] {
            assert_eq!(ending_in_line(line), Some(Ending::Failed(None)), "{line}");
        }
    }

    fn record_dirs(name: &str) -> (ScratchDir, PathBuf) {
        let dir = ScratchDir::new(name);
        let root = dir.join("account");
        std::fs::create_dir_all(root.join("session-state").join(CONVERSATION)).expect("dirs");
        (dir, root)
    }

    /// The record of a conversation is placed only for an id of the shape Copilot mints and only
    /// through real directories.
    #[test]
    fn the_record_is_placed_only_for_a_real_conversation_directory() {
        let (_dir, root) = record_dirs("copilot-record-in");
        let expected = root
            .join("session-state")
            .join(CONVERSATION)
            .join("events.jsonl");
        // The record itself need not exist yet.
        assert_eq!(record_in(&root, CONVERSATION), Some(expected));
        // Directories that are not there.
        let other = "00000000-0000-4000-8000-000000000000";
        assert_eq!(record_in(&root, other), None);
        assert_eq!(record_in(&root.join("nowhere"), CONVERSATION), None);
        // Ids that are no canonical UUID, a path among them.
        for id in [
            "",
            "conv-1",
            "../x",
            "..",
            "0a6d3faa-bcc7-41aa-8108-05f3ff4b252",
            "0a6d3faa-bcc7-41aa-8108-05f3ff4b25288",
            "0a6d3faabcc741aa810805f3ff4b2528",
            "0a6d3faa-bcc7-41aa-8108-05f3ff4b252g",
            "../0a6d3faa-bcc7-41aa-8108-05f3ff4b2528",
        ] {
            assert_eq!(record_in(&root, id), None, "{id:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_link_in_place_of_the_records_directory_or_the_conversation_is_not_followed() {
        let (dir, real) = record_dirs("copilot-record-linked");
        let linked_state = dir.join("linked-state");
        std::fs::create_dir_all(&linked_state).expect("directory");
        crate::test_support::symlink(
            real.join("session-state"),
            linked_state.join("session-state"),
        )
        .expect("link");
        assert_eq!(record_in(&linked_state, CONVERSATION), None);

        let linked_conversation = dir.join("linked-conversation");
        std::fs::create_dir_all(linked_conversation.join("session-state")).expect("directory");
        crate::test_support::symlink(
            real.join("session-state").join(CONVERSATION),
            linked_conversation.join("session-state").join(CONVERSATION),
        )
        .expect("link");
        assert_eq!(record_in(&linked_conversation, CONVERSATION), None);
        // A file where the directory should be is no directory either.
        let file = dir.join("file-state");
        std::fs::create_dir_all(&file).expect("directory");
        std::fs::write(file.join("session-state"), "x").expect("file");
        assert_eq!(record_in(&file, CONVERSATION), None);
    }

    /// A working Copilot session of an account directory of its own, with an open turn and a live
    /// process, and the record it will write.
    struct Fixture {
        _live: StandIn,
        state: Arc<AppState>,
        _dir: ScratchDir,
        record: PathBuf,
    }

    impl Fixture {
        fn new(name: &str, status: SessionStatus) -> Self {
            Self::of(name, status, AgentId::COPILOT)
        }

        /// A session of the test-only provider, whose stand-in takes a message as soon as it is
        /// written: for the cases that show a message going in. The watch reads Copilot's record
        /// whatever the session's own agent.
        fn taking_messages(name: &str, status: SessionStatus) -> Self {
            Self::of(name, status, AgentId::TEST)
        }

        fn of(name: &str, status: SessionStatus, agent: AgentId) -> Self {
            let (state, dir) = crate::test_support::app_state(&format!("copilot-watch-{name}"));
            let account = dir.join("account");
            let record_dir = account.join("session-state").join(CONVERSATION);
            std::fs::create_dir_all(&record_dir).expect("record directory");
            state
                .store
                .insert_console(&Console {
                    id: "console-1".to_string(),
                    name: "Console".to_string(),
                    workdir: dir.to_string_lossy().into_owned(),
                    default_agent: agent,
                    project_session_agent: None,
                    accounts: Default::default(),
                    launch: Default::default(),
                    project_session_launch: Default::default(),
                    icon: None,
                    console_session_prompt: None,
                    created_at: 0,
                    versions: Default::default(),
                })
                .expect("console");
            state
                .store
                .insert_session(&Session {
                    agent,
                    status,
                    has_conversation: true,
                    agent_session_id: Some(CONVERSATION.to_string()),
                    config_dir: Some(account.to_string_lossy().into_owned()),
                    ..bare_session("s", SessionKind::Project)
                })
                .expect("session");
            let live = if agent == AgentId::TEST {
                idle_stand_in_of("s", agent)
            } else {
                idle_stand_in("s")
            };
            state.register_live(live.clone());
            if agent == AgentId::TEST {
                // Its output read, so that what is written into it can be waited for.
                crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
            }
            state.turn_started("s", None);
            Self {
                _live: live,
                state,
                record: record_dir.join(COPILOT_EVENTS_FILE),
                _dir: dir,
            }
        }

        fn append(&self, text: &str) {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.record)
                .expect("record file")
                .write_all(text.as_bytes())
                .expect("write record");
        }

        fn arm_interrupt(&self) {
            arm(
                &self.state,
                "s",
                RecordWatch::Interrupt {
                    record: None,
                    turn_id: None,
                },
            );
        }

        /// Lets the watch make its polls (`watch::polls_ran`), on a paused clock.
        async fn polls_ran(&self) {
            watch::polls_ran(POLL_INTERVAL).await;
        }

        fn status(&self) -> SessionStatus {
            self.state.store.get_session("s").unwrap().unwrap().status
        }

        fn paused(&self) -> bool {
            self.state
                .wire_session(self.state.session_record("s").unwrap())
                .paused
        }
    }

    /// The record is the session's `events.jsonl` under its account's directory, named by the id the
    /// agent gave the conversation; with no such id, or one of another shape, it cannot be placed.
    #[test]
    fn the_record_is_the_conversations_events_file_under_its_account() {
        let fixture = Fixture::new("record-of", SessionStatus::Working);
        assert_eq!(record_of(&fixture.state, "s"), Some(fixture.record.clone()));
        for id in [None, Some("conv-1".to_string()), Some("../x".to_string())] {
            let mut session = fixture.state.session_record("s").unwrap();
            session.agent_session_id = id.clone();
            fixture
                .state
                .store
                .update_session(&session)
                .expect("session");
            assert_eq!(record_of(&fixture.state, "s"), None, "{id:?}");
        }
    }

    /// The whole path of a cancelled turn, on a paused clock so that the watch's poll interval
    /// costs no real time and the outcome does not depend on how fast the machine is: armed at the
    /// prompt, the session comes back to its prompt, paused, when the real `abort` line of Esc
    /// twice is written to the record.
    #[tokio::test(start_paused = true)]
    async fn a_cancel_written_after_the_prompt_returns_the_session_to_its_prompt() {
        let fixture = Fixture::new("cancel", SessionStatus::Working);
        fixture.append(&lines(49, 56));
        fixture.arm_interrupt();
        fixture.append(&line(57));
        fixture.polls_ran().await;
        assert_eq!(fixture.status(), SessionStatus::Idle);
        assert!(fixture.paused(), "a cancel leaves the session paused");
    }

    /// A turn that ends in an error, found by the watch armed at the prompt, ends as a failed one:
    /// closed, back at its prompt and paused.
    #[tokio::test(start_paused = true)]
    async fn an_error_written_after_the_prompt_ends_the_turn_as_failed() {
        let fixture = Fixture::new("error", SessionStatus::Working);
        fixture.append(&lines(86, 88));
        fixture.arm_interrupt();
        fixture.append(&line(89));
        fixture.polls_ran().await;
        assert_eq!(fixture.status(), SessionStatus::Idle);
        assert!(fixture.paused());
        assert_eq!(fixture.state.close_turn("s", false), TurnClose::NoTurn);
    }

    /// The watch reads only what is written after it is armed: over a record that already ends with
    /// the session's own `session.error`, a line that is no ending leaves the session working and
    /// not paused.
    #[tokio::test(start_paused = true)]
    async fn an_error_already_in_the_record_when_the_watch_is_armed_is_not_this_turns() {
        let fixture = Fixture::new("old-error", SessionStatus::Working);
        fixture.append(&lines(86, 89));
        fixture.arm_interrupt();
        // A harmless line of the next turn (a prompt, real).
        fixture.append(&line(90));
        fixture.polls_ran().await;
        assert_eq!(fixture.status(), SessionStatus::Working);
        assert!(!fixture.paused());
        assert_ne!(fixture.state.close_turn("s", false), TurnClose::NoTurn);
    }

    /// A watch the next prompt has retired ends nothing: an error written afterwards is the earlier
    /// turn's, and the turn now open is not the one that failed. The retired watch's end is the
    /// signal that it never will.
    #[tokio::test(start_paused = true)]
    async fn a_watch_retired_by_the_next_prompt_ends_nothing() {
        let fixture = Fixture::new("retired", SessionStatus::Working);
        fixture.arm_interrupt();
        fixture.state.end_record_watch("s");
        fixture.append(&line(89));
        fixture.append(&line(57));
        watch::last_ended().await;
        assert_eq!(fixture.status(), SessionStatus::Working);
        assert!(!fixture.paused());
        assert_ne!(fixture.state.close_turn("s", false), TurnClose::NoTurn);
    }

    /// A prompt waiting on the user that is cancelled with Esc leaves `permission.completed` with
    /// `cancelled` and an `abort` and no hook: the watch armed while the hand is up brings the
    /// session back to its prompt.
    #[tokio::test(start_paused = true)]
    async fn a_declined_prompt_returns_the_session_to_its_prompt() {
        let fixture = Fixture::new("decline", SessionStatus::WaitingUser);
        fixture.append(&lines(67, 71));
        arm(&fixture.state, "s", RecordWatch::Decline { record: None });
        fixture.append(&lines(72, 74));
        fixture.polls_ran().await;
        assert_eq!(fixture.status(), SessionStatus::Idle);
        assert!(fixture.paused());
    }

    /// A prompt answered "3. No, and tell Copilot what to do differently" runs no hook and the turn
    /// carries on: the session whose hand is up is working again, and not paused.
    #[tokio::test(start_paused = true)]
    async fn a_prompt_refused_with_feedback_puts_the_session_back_to_work() {
        let fixture = Fixture::new("refused", SessionStatus::WaitingUser);
        fixture.append(&lines(75, 79));
        arm(&fixture.state, "s", RecordWatch::Decline { record: None });
        fixture.append(&line(80));
        fixture.polls_ran().await;
        assert_eq!(fixture.status(), SessionStatus::Working);
        assert!(!fixture.paused());
    }

    /// Cancelled and approved permissions are no refusal: a session whose hand is up and whose
    /// record gets only an approval stays waiting for the user... until a hook moves it.
    #[tokio::test(start_paused = true)]
    async fn an_approved_permission_changes_nothing() {
        let fixture = Fixture::new("approved", SessionStatus::WaitingUser);
        arm(&fixture.state, "s", RecordWatch::Decline { record: None });
        // Written once the record was read for the hand: an answer found by that reading lowers
        // the hand, as the tests of the reading show.
        written().await;
        fixture.append(&lines(1, 7));
        fixture.polls_ran().await;
        assert_eq!(fixture.status(), SessionStatus::WaitingUser);
    }

    /// A refusal found when the session is no longer waiting for the user (a hook moved it on)
    /// changes nothing: the session is not put to work. The watch is owed nothing and stops, and
    /// its end is the signal that it never will.
    #[tokio::test(start_paused = true)]
    async fn a_refusal_found_after_the_hand_came_down_changes_nothing() {
        let fixture = Fixture::new("moved-on", SessionStatus::Idle);
        arm(&fixture.state, "s", RecordWatch::Decline { record: None });
        fixture.append(&line(80));
        watch::last_ended().await;
        assert_eq!(fixture.status(), SessionStatus::Idle);
    }

    /// A subagent's permission prompt raises the session's hand (its notification names no agent),
    /// and every hook that follows its answer names the subagent, so the real record lines of the
    /// prompt being approved (lines 12-22) are what lower it: the session goes from waiting for the
    /// user to working, and not paused.
    #[tokio::test(start_paused = true)]
    async fn a_subagents_prompt_approved_puts_the_session_back_to_work() {
        let fixture = Fixture::new("answered", SessionStatus::WaitingUser);
        arm(&fixture.state, "s", RecordWatch::Decline { record: None });
        fixture.append(&lines(12, 22));
        fixture.polls_ran().await;
        assert_eq!(fixture.status(), SessionStatus::Working);
        assert!(!fixture.paused());
    }

    /// `working_again` is one conditional write: a session whose hand a turn's end already lowered
    /// (`agentStop` wrote Idle first) is left at Idle, and no watch is armed after it.
    #[tokio::test]
    async fn working_again_leaves_a_session_that_is_no_longer_waiting() {
        let fixture = Fixture::new("again-idle", SessionStatus::WaitingUser);
        let generation = fixture.state.begin_record_watch("s");
        fixture
            .state
            .apply_hook_status("s", SessionStatus::Idle)
            .expect("the turn's end");
        working_again(&fixture.state, "s", generation);
        assert_eq!(fixture.status(), SessionStatus::Idle);
        // Still the current watch: none was armed to take its place.
        assert!(fixture.state.record_watch_current("s", generation));
    }

    /// A watch that is no longer the session's current one writes nothing.
    #[tokio::test]
    async fn working_again_writes_nothing_for_a_watch_that_is_not_current() {
        let fixture = Fixture::new("again-stale", SessionStatus::WaitingUser);
        let generation = fixture.state.begin_record_watch("s");
        fixture.state.end_record_watch("s");
        working_again(&fixture.state, "s", generation);
        assert_eq!(fixture.status(), SessionStatus::WaitingUser);
        assert!(!fixture.state.record_watch_current("s", generation));
    }

    /// When it moved the session, the turn goes on with no hook to say so, so its cancel is watched
    /// for again: a session-own `abort` written afterwards is caught by the watch it armed.
    #[tokio::test(start_paused = true)]
    async fn working_again_arms_a_watch_that_catches_the_turns_cancel() {
        let fixture = Fixture::new("again-moved", SessionStatus::WaitingUser);
        let generation = fixture.state.begin_record_watch("s");
        working_again(&fixture.state, "s", generation);
        assert_eq!(fixture.status(), SessionStatus::Working);
        // The new watch took the old one's place.
        assert!(!fixture.state.record_watch_current("s", generation));
        fixture.append(&line(57));
        fixture.polls_ran().await;
        assert_eq!(fixture.status(), SessionStatus::Idle);
        assert!(fixture.paused(), "a cancel leaves the session paused");
    }

    const HELD: &str = "held while the hand was up";

    /// A message held while the hand was up goes in once the answer to the subagent's prompt (the
    /// real record lines 12-22) puts the session back to working, with nothing left queued. The
    /// release goes through the watch's own poll and a write into a real stand-in, so it runs on
    /// the real clock and waits on the queue, bounded by `PATIENCE`, as soon as it is empty.
    #[tokio::test]
    async fn an_answered_prompt_releases_what_was_held_for_the_session() {
        let fixture = Fixture::taking_messages("answered-releases", SessionStatus::WaitingUser);
        fixture.state.queue_message("s", HELD);
        assert_eq!(fixture.state.queued_count("s"), 1);
        arm(&fixture.state, "s", RecordWatch::Decline { record: None });
        fixture.append(&lines(12, 22));
        let state = fixture.state.clone();
        let emptied = tokio::task::spawn_blocking(move || {
            crate::session::wait::wait_for(crate::test_support::PATIENCE, || {
                state.queued_count("s") == 0
            })
        })
        .await
        .unwrap();
        assert!(emptied, "the held message was never written");
        assert_eq!(fixture.status(), SessionStatus::Working);
    }

    /// The message that was held is written into the session: it is in what the session heard.
    #[cfg(unix)]
    #[tokio::test]
    async fn working_again_writes_the_message_that_was_held() {
        let fixture = Fixture::taking_messages("again-writes", SessionStatus::WaitingUser);
        fixture.state.queue_message("s", HELD);
        let generation = fixture.state.begin_record_watch("s");
        working_again(&fixture.state, "s", generation);
        assert_eq!(fixture.status(), SessionStatus::Working);
        let live = fixture.state.live_session("s").expect("running");
        let heard = tokio::task::spawn_blocking(move || {
            crate::test_support::output_containing(&live, HELD)
        })
        .await
        .unwrap();
        assert!(heard.contains(HELD), "{heard}");
        assert_eq!(fixture.state.queued_count("s"), 0);
    }

    /// With a session request's hand held the board keeps showing the request's hand: the record
    /// stays waiting for the user and nothing is published, while the agent reads working.
    #[tokio::test]
    async fn working_again_beneath_a_request_hand_leaves_the_boards_hand_up() {
        let fixture = Fixture::taking_messages("again-hand", SessionStatus::WaitingUser);
        fixture.state.raise_request_hand("s").expect("a hand");
        let mut events = fixture.state.subscribe();
        let generation = fixture.state.begin_record_watch("s");
        working_again(&fixture.state, "s", generation);
        let record = fixture.state.session_record("s").unwrap();
        assert_eq!(record.status, SessionStatus::WaitingUser);
        assert_eq!(fixture.state.agent_status(&record), SessionStatus::Working);
        assert_eq!(
            fixture.state.wire_session(record).status,
            SessionStatus::WaitingUser
        );
        let upserts = std::iter::from_fn(|| events.try_recv().ok())
            .filter(|event| matches!(event, crate::protocol::Event::SessionUpserted { .. }))
            .count();
        assert_eq!(upserts, 0);
    }

    /// Lines of the real records of the scenes with a prompt in them
    /// (`testdata/copilot-captures.md`, "Background work"), by their 1-based line numbers there.
    const PROMPTS: &str = include_str!("../../../testdata/copilot_background_events.jsonl");

    fn lines_of(record: &str, from: usize, to: usize) -> String {
        record
            .lines()
            .skip(from - 1)
            .take(to + 1 - from)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// One message that calls a command needing a prompt and a slow tool that needs none (lines
    /// 66-76): the command's call waits from its `permission.requested` (line 70), still does when
    /// the slow call has ended (71), and no longer once the permission is completed (72); the
    /// session's `agentStop` (75) then says the turn is over.
    #[test]
    fn the_call_a_prompt_is_about_waits_until_its_permission_is_completed() {
        let waiting = Asked::Waiting(vec!["call_3cb0785d".to_string()]);
        assert_eq!(asked_in(&lines_of(PROMPTS, 66, 69)), Asked::Unknown);
        assert_eq!(asked_in(&lines_of(PROMPTS, 66, 70)), waiting);
        assert_eq!(asked_in(&lines_of(PROMPTS, 66, 71)), waiting);
        assert_eq!(
            asked_in(&lines_of(PROMPTS, 66, 72)),
            Asked::Answered { turn_over: false }
        );
        assert_eq!(
            asked_in(&lines_of(PROMPTS, 66, 75)),
            Asked::Answered { turn_over: true }
        );
    }

    /// The record as a notification that lands after its turn finds it (lines 77-84): the prompt
    /// answered and the turn over. A turn with no prompt in it says nothing (lines 1-8), and a
    /// prompt of the session's own starts the count over (the next session's `user.message`).
    #[test]
    fn a_prompt_answered_before_its_notification_lands_is_told_from_the_record() {
        assert_eq!(
            asked_in(&lines_of(PROMPTS, 77, 84)),
            Asked::Answered { turn_over: true }
        );
        // A cancel or an error after the answer cuts the turn short (the real lines 57 and 89 of
        // the other capture), the error with its text, and a subagent's does not.
        let failed = Asked::Failed(Some("400 stand-in failure".to_string()));
        for (ending, cut) in [(line(57), Asked::Aborted), (line(89), failed)] {
            assert_eq!(
                asked_in(&format!("{}\n{ending}", lines_of(PROMPTS, 77, 81))),
                cut
            );
            assert_eq!(
                asked_in(&format!("{}\n{ending}", lines_of(PROMPTS, 77, 80))),
                cut,
                "a prompt the cancel took down is not waited at"
            );
        }
        assert_eq!(
            asked_in(&format!("{}\n{}", lines_of(PROMPTS, 77, 81), line(42))),
            Asked::Answered { turn_over: false }
        );
        assert_eq!(asked_in(&lines_of(PROMPTS, 1, 8)), Asked::Unknown);
        assert_eq!(asked_in(&lines_of(PROMPTS, 66, 77)), Asked::Unknown);
        assert_eq!(asked_in(""), Asked::Unknown);
        assert_eq!(
            asked_in("not a record\n{\"type\":\"permission.requested\"}\n"),
            Asked::Unknown
        );
    }

    /// A subagent's permission prompt is one the session waits at, but its call is not the
    /// session's to hold: lines 12-21 of the record capture end with the subagent's request open,
    /// line 22 completes it.
    #[test]
    fn a_subagents_prompt_waits_without_a_call_of_the_sessions() {
        assert_eq!(
            asked_in(&lines_of(RECORD, 12, 21)),
            Asked::Waiting(Vec::new())
        );
        assert_eq!(
            asked_in(&lines_of(RECORD, 12, 22)),
            Asked::Answered { turn_over: false }
        );
    }

    /// Hand-built in the shape of the captured lines, since no record capture holds an `ask_user`
    /// call: the form waits from the call's start to its end and names no call to hold, and a
    /// subagent's call of that name, which fails for want of the tool, is not a prompt.
    #[test]
    fn an_ask_user_form_waits_from_its_calls_start_to_its_end() {
        let start = r#"{"type":"tool.execution_start","data":{"toolCallId":"call_ask","toolName":"ask_user","arguments":{}}}"#;
        let end =
            r#"{"type":"tool.execution_complete","data":{"toolCallId":"call_ask","success":true}}"#;
        let subagents = r#"{"type":"tool.execution_start","agentId":"a1","data":{"toolCallId":"call_sub","toolName":"ask_user","arguments":{}}}"#;
        assert_eq!(asked_in(start), Asked::Waiting(Vec::new()));
        assert_eq!(
            asked_in(&format!("{start}\n{end}")),
            Asked::Answered { turn_over: false }
        );
        assert_eq!(asked_in(subagents), Asked::Unknown);
    }

    /// The hooks' payloads of the same scenes as [`PROMPTS`], and of the scenes of [`RECORD`], one
    /// object a line with the event its entry was registered under.
    const PROMPT_HOOKS: &str =
        include_str!("../../../testdata/copilot_background_hook_payloads.jsonl");
    const HOOKS: &str = include_str!("../../../testdata/copilot_hook_payloads.jsonl");

    /// Lines `from` to `to` of [`PROMPTS`] as a record holds them, the last one whole.
    fn prompt_lines(from: usize, to: usize) -> String {
        lines_of(PROMPTS, from, to) + "\n"
    }

    /// Applies what Copilot's interpreter reads of the calls in line `number` (1-based) of the
    /// hooks capture `capture`, and says whether a call still waits on the user: what keeps the
    /// hand up when the event's own status is working.
    fn hook_ran(fixture: &Fixture, capture: &str, number: usize) -> bool {
        use crate::hooks::HookInterpreter;
        let line: Value =
            serde_json::from_str(capture.lines().nth(number - 1).expect("a capture line"))
                .expect("a payload line");
        let event = line["event"].as_str().expect("the entry's event");
        let change = super::super::hooks::CopilotHooks
            .tool_call_change(event, &line["payload"])
            .expect("an event that changes the calls");
        fixture.state.apply_tool_call_change("s", change)
    }

    fn arm_decline(fixture: &Fixture) {
        arm(&fixture.state, "s", RecordWatch::Decline { record: None });
    }

    /// Lets the reading of the record that follows a raised hand run, on a paused clock: its
    /// read is a blocking task, which holds that clock until it ends.
    async fn written() {
        tokio::time::sleep(RECORD_SETTLE + Duration::from_millis(1)).await;
    }

    /// One message calls a command that needs a prompt and a slow tool that needs none (the real
    /// hooks and record of `slow-call-ends-at-prompt`). When the prompt's notification raises the
    /// hand, the record names the command's call as the one asked about (line 70) and it is held:
    /// the slow call's `postToolUse` leaves the hand up, and the command's own lowers it.
    #[tokio::test(start_paused = true)]
    async fn the_call_asked_about_is_held_until_its_own_post_event() {
        let fixture = Fixture::new("held", SessionStatus::WaitingUser);
        // Both calls' `preToolUse`: neither waits on the user by itself.
        assert!(!hook_ran(&fixture, PROMPT_HOOKS, 54));
        assert!(!hook_ran(&fixture, PROMPT_HOOKS, 55));
        fixture.append(&prompt_lines(66, 70));
        arm_decline(&fixture);
        written().await;
        assert!(
            hook_ran(&fixture, PROMPT_HOOKS, 59),
            "the slow call's end leaves the hand up"
        );
        assert!(
            !hook_ran(&fixture, PROMPT_HOOKS, 60),
            "the command's own end lowers it"
        );
    }

    /// A notification that lands after its prompt was answered, the turn still going (lines
    /// 66-72): the hand it raised is lowered to working, and the turn's cancel is watched for
    /// again (the real `abort` of the other capture, line 57).
    #[tokio::test(start_paused = true)]
    async fn a_late_notification_of_a_turn_still_going_is_lowered_to_working() {
        let fixture = Fixture::new("late-working", SessionStatus::WaitingUser);
        fixture.append(&prompt_lines(66, 72));
        arm_decline(&fixture);
        written().await;
        assert_eq!(fixture.status(), SessionStatus::Working);
        assert!(!fixture.paused());
        fixture.append(&line(57));
        fixture.polls_ran().await;
        assert_eq!(fixture.status(), SessionStatus::Idle);
        assert!(fixture.paused(), "a cancel leaves the session paused");
    }

    /// A notification that lands after its turn's `agentStop` (the real record of
    /// `notification-after-turn-end`, lines 77-84): the hand it raised is lowered to the prompt
    /// the turn ended at, which is no abnormal end.
    #[tokio::test(start_paused = true)]
    async fn a_late_notification_of_a_turn_that_is_over_is_lowered_to_idle() {
        let fixture = Fixture::new("late-idle", SessionStatus::WaitingUser);
        fixture.append(&prompt_lines(77, 84));
        arm_decline(&fixture);
        written().await;
        assert_eq!(fixture.status(), SessionStatus::Idle);
        assert!(!fixture.paused());
    }

    /// With a session request's hand held, a notification that lands after its turn's
    /// `agentStop` (lines 77-84) leaves the board's hand up: the record stays waiting for the
    /// user and nothing is published, while the agent reads idle.
    #[tokio::test(start_paused = true)]
    async fn a_late_notification_of_a_turn_that_is_over_leaves_a_request_hand_up() {
        let fixture = Fixture::new("late-idle-hand", SessionStatus::WaitingUser);
        fixture.state.raise_request_hand("s").expect("a hand");
        let mut events = fixture.state.subscribe();
        fixture.append(&prompt_lines(77, 84));
        arm_decline(&fixture);
        written().await;
        let record = fixture.state.session_record("s").unwrap();
        assert_eq!(record.status, SessionStatus::WaitingUser);
        assert_eq!(fixture.state.agent_status(&record), SessionStatus::Idle);
        assert_eq!(
            fixture.state.wire_session(record).status,
            SessionStatus::WaitingUser
        );
        let upserts = std::iter::from_fn(|| events.try_recv().ok())
            .filter(|event| matches!(event, crate::protocol::Event::SessionUpserted { .. }))
            .count();
        assert_eq!(upserts, 0);
    }

    /// A prompt refused with what to do differently runs no post event for its call, so the
    /// refusal's record line is what takes the call off those held: with the command's call held
    /// (lines 66-70) and a real refusal written (the other capture's line 80), the session is
    /// working and no call waits any more, so the next call's end would not keep the hand up.
    #[tokio::test(start_paused = true)]
    async fn a_refusal_clears_the_call_held_for_its_prompt() {
        let fixture = Fixture::new("refusal-clears", SessionStatus::WaitingUser);
        fixture.append(&prompt_lines(66, 70));
        arm_decline(&fixture);
        written().await;
        assert!(fixture.state.holds_a_waiting_call("s"));
        fixture.append(&line(80));
        fixture.polls_ran().await;
        assert_eq!(fixture.status(), SessionStatus::Working);
        assert!(!fixture.state.holds_a_waiting_call("s"));
    }

    /// A hand is not lowered while a call is held as waiting, whatever the record says: a form's
    /// call held from its `preToolUse` (the real one of `ask-user-answered`) over a record that
    /// does not show the form and says every prompt of the turn was answered (lines 66-72), as
    /// one whose latest line is not written out yet reads. The hand stays up and is watched as
    /// raised: a cancel written afterwards brings the session back to its prompt.
    #[tokio::test(start_paused = true)]
    async fn a_hand_is_not_lowered_while_a_call_is_held_as_waiting() {
        let fixture = Fixture::new("late-held", SessionStatus::WaitingUser);
        assert!(hook_ran(&fixture, HOOKS, 46), "the form's call waits");
        fixture.append(&prompt_lines(66, 72));
        arm_decline(&fixture);
        written().await;
        assert_eq!(fixture.status(), SessionStatus::WaitingUser);
        fixture.append(&line(57));
        fixture.polls_ran().await;
        assert_eq!(fixture.status(), SessionStatus::Idle);
        assert!(fixture.paused(), "a cancel leaves the session paused");
    }

    /// Copilot writes its record out in batches, so the request a notification is about is not
    /// in the file when the notification raises the hand (measured: 0.16 s after). The record
    /// is read for it only once it has had time to be written: the request's line (line 70)
    /// written half that time after the hand went up is still found, and its call held.
    #[tokio::test(start_paused = true)]
    async fn a_request_written_after_the_hand_went_up_is_still_found() {
        let fixture = Fixture::new("written-late", SessionStatus::WaitingUser);
        fixture.append(&prompt_lines(66, 69));
        arm_decline(&fixture);
        tokio::time::sleep(RECORD_SETTLE / 2).await;
        assert!(!fixture.state.holds_a_waiting_call("s"));
        fixture.append(&prompt_lines(70, 70));
        written().await;
        assert!(fixture.state.holds_a_waiting_call("s"));
        assert_eq!(fixture.status(), SessionStatus::WaitingUser);
    }

    /// The second prompt of a turn whose first was answered: when its notification raises the
    /// hand the file holds only the first prompt, answered (lines 66-72), and the second
    /// request is written a moment later (the real start and request of another command, lines
    /// 79-80). The hand is not lowered for the answered one, and the second call is held.
    #[tokio::test(start_paused = true)]
    async fn a_second_prompt_not_written_yet_is_not_taken_for_the_answered_first() {
        let fixture = Fixture::new("second-prompt", SessionStatus::WaitingUser);
        fixture.append(&prompt_lines(66, 72));
        arm_decline(&fixture);
        tokio::time::sleep(RECORD_SETTLE / 2).await;
        fixture.append(&prompt_lines(79, 80));
        written().await;
        assert_eq!(fixture.status(), SessionStatus::WaitingUser);
        assert!(fixture.state.holds_a_waiting_call("s"));
    }

    /// A notification that lands after its prompt was cancelled (lines 67-74 of the other
    /// capture, the `abort` already written when the hand goes up, so the watch armed with the
    /// hand never reads it): the session comes back to its prompt, paused.
    #[tokio::test(start_paused = true)]
    async fn a_late_notification_of_a_cancelled_prompt_returns_the_session_to_its_prompt() {
        let fixture = Fixture::new("late-cut", SessionStatus::WaitingUser);
        fixture.append(&lines(67, 74));
        arm_decline(&fixture);
        written().await;
        assert_eq!(fixture.status(), SessionStatus::Idle);
        assert!(fixture.paused(), "a cancel leaves the session paused");
    }

    /// A notification that lands after its prompt was taken down by an error (the prompt of lines
    /// 67-73, then the error of line 89, both written when the hand goes up): the turn ends as a
    /// failed one, closed and back at its prompt paused, as the watch ends a turn for an error
    /// written after it was armed.
    #[tokio::test(start_paused = true)]
    async fn a_late_notification_of_a_prompt_an_error_took_down_ends_the_turn_as_failed() {
        let fixture = Fixture::new("late-error", SessionStatus::WaitingUser);
        fixture.append(&(lines(67, 73) + &line(89)));
        arm_decline(&fixture);
        written().await;
        assert_eq!(fixture.status(), SessionStatus::Idle);
        assert!(fixture.paused());
        assert_eq!(fixture.state.close_turn("s", false), TurnClose::NoTurn);
    }
}
