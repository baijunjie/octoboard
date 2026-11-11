//! Watching a Codex session's rollout file for the one thing its hooks never report: a turn that
//! ended in an error. Codex's record store arms it (`super::records`), on the record watch its hook
//! interpreter asks for after a prompt.
//!
//! Such a turn fires `UserPromptSubmit` and then nothing — no `Stop`, no `Interrupt`, and Codex has
//! no notification event to fall back on — so the session would read *working* for as long as the
//! process lives, and its turn would stay open. Measured against Codex 0.160.0 with the account's
//! usage limit reached, three times out of three.
//!
//! What Codex does write is its rollout file, the payload's `transcript_path`: a
//! `{"type":"task_started","turn_id":…}` record when a turn begins, and a
//! `{"type":"task_complete","turn_id":…}` record when it ends, inside an `event_msg` entry's
//! `payload`. The `turn_id` is the one the turn's `UserPromptSubmit` carries. A turn that ended in
//! an error has an `error` object on that record (`message` is the user-facing text,
//! `codex_error_info` the cause), written about a second after the prompt and readable as soon as
//! it is; a turn that ended normally has `last_agent_message` and no `error` key at all. Codex
//! flushes per record. The file does not exist yet when the first prompt of a session is submitted
//! — it appears about a quarter of a second later — so a watch has to tolerate its absence.
//!
//! [`ending_in_line`] is the pure half: whether one line is the open turn's `task_complete`, and
//! how it says the turn ended. [`watch_for_failure`] arms `crate::records::watch` with it from the
//! turn's `UserPromptSubmit`, and turns a failure into a failed turn ending, the same as Claude
//! Code's `StopFailure`. A failure whose `codex_error_info` is `server_overloaded` (the selected
//! model is at capacity) is handed to `crate::messaging::capacity_retry` instead of being reported
//! at once.
//!
//! The rollout is Codex's own and is rewritten on upgrades; see `crate::records::watch` for how a
//! line this does not recognise is treated.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::hooks::TurnEnd;
use crate::messaging::capacity_retry::{self, Judgement};
use crate::protocol::SessionStatus;
use crate::records::watch::{self, Owed, Start, Watch};
use crate::state::{AppState, TurnClose};
use crate::text::trimmed_or_none;

/// How often the watch checks the rollout for new lines. A failed turn is over within about a
/// second of the prompt, and the session should come back to its prompt close to when Codex does.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// How the watched turn ended, as its `task_complete` record says.
#[derive(Debug, PartialEq, Eq)]
enum Ending {
    /// The turn ended without an error: a normal completion, which Codex follows with `Stop`, the
    /// hook that ends it for Octoboard. An interrupted turn has no `task_complete`: `Interrupt`
    /// ends it.
    Completed,
    /// The turn ended in an error, with the user-facing text where the record carries one, and
    /// whether the cause was the selected model being at capacity.
    Failed {
        message: Option<String>,
        at_capacity: bool,
    },
}

/// The `codex_error_info` of a turn that failed because the selected model was at capacity. The
/// field is a string for most failures but an object for some, so only this string counts.
const CAPACITY_ERROR: &str = "server_overloaded";

/// How one rollout line says turn `turn_id` ended, if it is that turn's `task_complete`. A line
/// that is not JSON, or is JSON of another shape, reads as `None`.
fn ending_in_line(line: &str, turn_id: &str) -> Option<Ending> {
    if !line.contains("\"task_complete\"") {
        return None;
    }
    let value = serde_json::from_str::<Value>(line).ok()?;
    let payload = value.get("payload")?;
    if payload.get("type").and_then(Value::as_str) != Some("task_complete")
        || payload.get("turn_id").and_then(Value::as_str) != Some(turn_id)
    {
        return None;
    }
    // Keyed on the `error` key alone: a normal completion has none, and `last_agent_message` is
    // null on a failed turn but also on a manual `/compact` turn.
    Some(
        match payload.get("error").filter(|error| !error.is_null()) {
            None => Ending::Completed,
            Some(error) => Ending::Failed {
                message: trimmed_or_none(error.get("message").and_then(Value::as_str)),
                at_capacity: error.get("codex_error_info").and_then(Value::as_str)
                    == Some(CAPACITY_ERROR),
            },
        },
    )
}

/// Starts watching `session_id`'s rollout for the end of its open turn `turn_id`. Call it from a
/// Codex `UserPromptSubmit`, where both are known.
///
/// The watch ends when it finds the turn's ending, when a newer watch for the session supersedes it
/// (`AppState::begin_record_watch`), when the next prompt's submission retires it
/// (`end_record_watch`), or when the process is gone.
///
/// It reads from where the session's previous watch of the same file got to, and from the start of
/// the file for the first ([`Start::Carried`]). Not from the file's end as it stands now: the
/// record it is after can already be written by the time this is called. The hook callback is a
/// fresh exec plus a request, and a turn that fails at once can have its record written before the
/// callback arrives, so starting at the end would miss the only notice the turn ever gives, which
/// is the whole defect. Nothing is misattributed by reading earlier records, since an ending counts
/// only for the exact open turn's id.
pub fn watch_for_failure(
    state: &Arc<AppState>,
    session_id: &str,
    transcript_path: Option<&str>,
    turn_id: Option<&str>,
) {
    let (Some(transcript_path), Some(turn_id)) = (transcript_path, turn_id) else {
        tracing::debug!(
            session = %session_id,
            "no rollout path or turn id on a prompt submission; the turn cannot be watched \
             for an error"
        );
        return;
    };
    let turn_id = turn_id.to_string();
    watch::spawn(
        state,
        session_id,
        PathBuf::from(transcript_path),
        Watch {
            interval: POLL_INTERVAL,
            start: Start::Carried,
        },
        |_, _| Owed::Yes,
        move |line| ending_in_line(line, &turn_id),
        |state, session_id, _, ending| match ending {
            // `Stop` is on its way and ends the turn in the usual way.
            Ending::Completed => {}
            Ending::Failed {
                message,
                at_capacity,
            } => end_failed_turn(state, session_id, message, at_capacity),
        },
    );
}

/// Ends the open turn as a failed one, as if the agent had fired a failure event for it: the turn
/// closes, a bound session's owner gets its report, and the session goes back to awaiting
/// instructions, paused.
///
/// A turn that failed because the model was at capacity is retried by `capacity_retry` while it
/// has retries left, and its owner is told of that in place of the report; with none left the
/// report says so.
pub(crate) fn end_failed_turn(
    state: &Arc<AppState>,
    session_id: &str,
    message: Option<String>,
    at_capacity: bool,
) {
    let judgement = at_capacity.then(|| state.capacity_retries.judge(session_id));
    let message = match judgement {
        Some(Judgement::Retry(plan)) => {
            // Closed here and not through `close_turn_and_report`, which would report the failure.
            let close = state.close_turn(session_id, false);
            if matches!(close, TurnClose::OwesReport | TurnClose::Reported) {
                crate::messaging::usage_limit::turn_failed_otherwise(state, session_id);
            }
            if !settle_failed_turn(state, session_id, close) {
                state.capacity_retries.forget(session_id);
            } else {
                capacity_retry::arm(
                    state,
                    session_id,
                    plan,
                    message,
                    close == TurnClose::OwesReport,
                );
            }
            return;
        }
        Some(Judgement::Exhausted) => Some(capacity_retry::failure_text(
            message,
            capacity_retry::Outcome::Exhausted,
        )),
        None => message,
    };
    let turn = TurnEnd {
        last_assistant_message: message,
        failed: true,
        backstop: false,
    };
    let close = crate::messaging::reporting::close_turn_and_report(state, session_id, turn);
    settle_failed_turn(state, session_id, close);
}

/// Puts the session back at its prompt, paused, after the turn it ended as failed was closed with
/// `close`. Whether it did: no turn open means a hook ended it first, and the session is already
/// where it belongs.
fn settle_failed_turn(state: &Arc<AppState>, session_id: &str, close: TurnClose) -> bool {
    match close {
        // `NotThisTurn` cannot come back for an end that is not clock-attributed; it is grouped
        // here only to keep the match exhaustive.
        TurnClose::NoTurn | TurnClose::NotThisTurn => {
            tracing::debug!(
                session = %session_id,
                "a failed turn was found in the rollout, but no turn was open"
            );
            false
        }
        TurnClose::OwesReport | TurnClose::Reported => {
            tracing::debug!(
                session = %session_id,
                "a turn ended in an error; the agent is back at its prompt"
            );
            // Before the status write, so the record it publishes carries the flag.
            state.set_paused(session_id, true);
            if let Err(err) = state.apply_hook_status(session_id, SessionStatus::Idle) {
                tracing::debug!(
                    session = %session_id, %err,
                    "applying the status after a failed turn failed"
                );
            }
            state.spawn_flush_outbox(session_id, SessionStatus::Idle);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;
    use std::path::Path;

    use super::*;
    use crate::protocol::{AgentId, Console, Session, SessionKind};
    use crate::test_support::{bare_session, idle_stand_in, ScratchDir, ScratchFile, StandIn};

    /// Two failed turns of one Codex 0.160.0 session whose account was at its usage limit, taken
    /// verbatim: each is `task_started`, `token_count`, then `task_complete` with an `error`
    /// object.
    const FAILED_TURNS: &str = include_str!("../../../testdata/rollout_failed_turns.jsonl");
    const FAILED_TURN: &str = "01a127d8-56fb-7442-b291-53bde00f15fe";
    const SECOND_FAILED_TURN: &str = "01a127d8-feef-7872-9890-c06ccd669d7d";

    /// The near-miss: a successful turn's records, from a Codex 0.160.0 session, whose
    /// `task_complete` carries `last_agent_message` and no `error` key.
    const COMPLETED: &str = include_str!("../../../testdata/rollout_completed_turn.jsonl");
    const COMPLETED_TURN: &str = "01a1009e-6e43-73d3-a46f-5bc648f49a7a";

    /// A manual `/compact` turn, from a Codex 0.160.0 session: `task_complete` has a null
    /// `last_agent_message` like a failed turn's, but no `error` key and no
    /// `time_to_first_token_ms`.
    const COMPACT: &str = include_str!("../../../testdata/rollout_compact_turn.jsonl");
    const COMPACT_TURN: &str = "01a1009f-e588-7f21-b87f-47a002c43820";

    /// The other near-miss: the failed capture cut off after `token_count`, a turn still running.
    const RUNNING: &str = include_str!("../../../testdata/rollout_running_turn.jsonl");

    /// A turn whose model was at capacity, the `task_started` and `task_complete` of one Codex
    /// 0.162 turn taken verbatim from a rollout: `error.codex_error_info` is `server_overloaded`.
    const CAPACITY_FAILURE: &str = include_str!("../../../testdata/rollout_capacity_failure.jsonl");
    const CAPACITY_TURN: &str = "01a182e0-5b29-71a3-90be-ab2f19c4fc55";

    /// The first `lines` lines of `text`.
    fn first_lines(text: &str, lines: usize) -> &str {
        let end = text.split_inclusive('\n').take(lines).map(str::len).sum();
        &text[..end]
    }

    /// The first of those turns alone: its three lines.
    fn failed_turn() -> &'static str {
        first_lines(FAILED_TURNS, 3)
    }

    fn rollout_file(name: &str) -> ScratchFile {
        ScratchFile::new(&format!("rollout-{name}"), "rollout.jsonl")
    }

    fn write_rollout(path: &Path, content: &str) {
        let mut file = File::create(path).expect("rollout file");
        file.write_all(content.as_bytes()).expect("write rollout");
    }

    fn ending_of(content: &str, turn_id: &str) -> Option<Ending> {
        let path = rollout_file("ending");
        write_rollout(&path, content);
        watch::scan_lines(&path, 0, |line| ending_in_line(line, turn_id))
            .expect("scan")
            .found
    }

    #[test]
    fn a_failed_turn_is_found_with_codexs_own_message() {
        let Some(Ending::Failed {
            message: Some(message),
            ..
        }) = ending_of(failed_turn(), FAILED_TURN)
        else {
            panic!("the failed turn was not recognised");
        };
        assert!(
            message.starts_with("You’ve hit your usage limit"),
            "{message}"
        );
    }

    #[test]
    fn a_turn_whose_model_was_at_capacity_is_recognised_as_that() {
        let Some(Ending::Failed {
            message: Some(message),
            at_capacity,
        }) = ending_of(CAPACITY_FAILURE, CAPACITY_TURN)
        else {
            panic!("the capacity failure was not recognised");
        };
        assert!(at_capacity);
        assert!(
            message.starts_with("Selected model is at capacity"),
            "{message}"
        );
    }

    /// The usage limit and every other error, whatever the shape of `codex_error_info`, are not a
    /// capacity failure.
    #[test]
    fn other_failures_are_not_a_capacity_failure() {
        assert!(matches!(
            ending_of(failed_turn(), FAILED_TURN),
            Some(Ending::Failed {
                at_capacity: false,
                ..
            })
        ));
        for info in [
            r#""codex_error_info":{"http_connection_failed":{"http_status_code":500}}"#,
            r#""codex_error_info":"other""#,
            r#""codex_error_info":null"#,
            r#""x":1"#,
        ] {
            let line = format!(
                r#"{{"type":"event_msg","payload":{{"type":"task_complete","turn_id":"t","error":{{"message":"m",{info}}}}}}}
"#
            );
            assert!(
                matches!(
                    ending_of(&line, "t"),
                    Some(Ending::Failed {
                        at_capacity: false,
                        ..
                    })
                ),
                "{info}"
            );
        }
    }

    #[test]
    fn a_turn_that_completed_normally_is_not_a_failure() {
        assert_eq!(
            ending_of(COMPLETED, COMPLETED_TURN),
            Some(Ending::Completed)
        );
    }

    /// Why the ending is keyed on the `error` key and not on a null message: a manual `/compact`
    /// turn has a null message too, and did not fail.
    #[test]
    fn a_compact_turn_is_not_a_failure() {
        assert_eq!(ending_of(COMPACT, COMPACT_TURN), Some(Ending::Completed));
    }

    #[test]
    fn a_turn_still_running_has_not_ended() {
        assert_eq!(ending_of(RUNNING, FAILED_TURN), None);
    }

    /// What reading the file from the start rests on: an earlier turn's ending, before the watched
    /// turn's `task_started`, is not attributed to it. The capture holds two failed turns of one
    /// Codex 0.160.0 session, the usual shape on an account at its limit; cut after the second
    /// turn's `task_started` it is that turn still running behind a failed one.
    #[test]
    fn an_earlier_turns_ending_is_not_the_watched_turns() {
        let two = FAILED_TURNS;
        let cut = first_lines(two, 4);
        let last = cut.lines().last().expect("the second turn's start");
        assert!(last.contains("task_started") && last.contains(SECOND_FAILED_TURN));

        assert!(matches!(
            ending_of(cut, FAILED_TURN),
            Some(Ending::Failed { .. })
        ));
        assert_eq!(ending_of(cut, SECOND_FAILED_TURN), None);
        assert!(matches!(
            ending_of(two, SECOND_FAILED_TURN),
            Some(Ending::Failed { .. })
        ));

        // A successful turn before a failed one: each id reads its own ending.
        let mixed = format!("{COMPLETED}{}", failed_turn());
        assert_eq!(ending_of(&mixed, COMPLETED_TURN), Some(Ending::Completed));
        assert!(matches!(
            ending_of(&mixed, FAILED_TURN),
            Some(Ending::Failed { .. })
        ));
    }

    /// Unrecognised and malformed lines are skipped, and the record after them is still read.
    #[test]
    fn unrecognised_lines_are_skipped_without_error() {
        let content = format!(
            "not json\n{{\"task_complete\":1}}\n\"task_complete\"\n{}",
            failed_turn()
        );
        assert!(matches!(
            ending_of(&content, FAILED_TURN),
            Some(Ending::Failed { .. })
        ));
    }

    fn codex_session(id: &str, status: SessionStatus) -> Session {
        Session {
            agent: AgentId::CODEX,
            status,
            has_conversation: true,
            ..bare_session(id, SessionKind::Project)
        }
    }

    /// A working Codex session with an open turn and a live process, and the rollout it will write.
    struct Fixture {
        _live: StandIn,
        state: Arc<AppState>,
        _dir: ScratchDir,
        path: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let (state, dir) = crate::test_support::app_state(&format!("rollout-{name}"));
            state
                .store
                .insert_console(&Console {
                    id: "console-1".to_string(),
                    name: "Console".to_string(),
                    workdir: dir.to_string_lossy().into_owned(),
                    default_agent: AgentId::CODEX,
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
                .insert_session(&codex_session("s", SessionStatus::Working))
                .expect("session");
            let live = idle_stand_in("s");
            state.register_live(live.clone());
            state.turn_started("s", None);
            let path = dir.join("rollout.jsonl");
            Self {
                _live: live,
                state,
                _dir: dir,
                path,
            }
        }

        fn watch(&self) {
            watch_for_failure(
                &self.state,
                "s",
                Some(&self.path.to_string_lossy()),
                Some(FAILED_TURN),
            );
        }

        async fn went_idle(&self, patience: Duration) -> bool {
            let state = self.state.clone();
            tokio::task::spawn_blocking(move || {
                crate::session::wait::wait_for(patience, || {
                    state
                        .store
                        .get_session("s")
                        .ok()
                        .flatten()
                        .map(|s| s.status)
                        == Some(SessionStatus::Idle)
                })
            })
            .await
            .unwrap()
        }
    }

    /// The whole path, with the rollout created after the watch began: the session returns to
    /// awaiting instructions and the turn is closed, so no later ending is read as another.
    #[tokio::test]
    async fn a_failed_turn_returns_the_session_to_awaiting_instructions() {
        let fixture = Fixture::new("failed");
        fixture.watch();
        write_rollout(&fixture.path, failed_turn());

        assert!(fixture.went_idle(crate::test_support::PATIENCE).await);
        assert_eq!(fixture.state.close_turn("s", false), TurnClose::NoTurn);
        let shown = fixture
            .state
            .wire_session(fixture.state.session_record("s").unwrap());
        assert!(shown.paused, "a failed turn leaves the session paused");
    }

    /// The race the watch's start offset exists for: Codex wrote the failed turn's `task_complete`
    /// before the hook callback armed the watch (0.25 s of margin on a session's first turn,
    /// against a fresh hook exec and an HTTP round trip). A watch started at the file's length
    /// would begin past the record and never see it. The case above only covers a file that does
    /// not exist yet, where any start offset reads from zero, so it cannot catch this.
    #[tokio::test]
    async fn a_failure_written_before_the_watch_was_armed_is_still_found() {
        let fixture = Fixture::new("written-first");
        write_rollout(&fixture.path, failed_turn());
        fixture.watch();

        assert!(fixture.went_idle(crate::test_support::PATIENCE).await);
        assert_eq!(fixture.state.close_turn("s", false), TurnClose::NoTurn);
    }

    /// A watch resumes from where the previous one got to, and that offset can be past the end of
    /// a file that has since shrunk. Skipping to the new end would skip the record the watch is
    /// after, so it reads again from the start.
    #[tokio::test]
    async fn a_shrunk_rollout_is_read_again_from_the_start() {
        let fixture = Fixture::new("shrunk");
        let past_the_end = failed_turn().len() as u64 + 1000;
        fixture
            .state
            .note_record_offset("s", &fixture.path, past_the_end);
        write_rollout(&fixture.path, failed_turn());
        fixture.watch();

        assert!(fixture.went_idle(crate::test_support::PATIENCE).await);
    }

    /// A turn that completed normally is `Stop`'s to end, not the watch's; one still running is not
    /// over. Neither may move the session.
    #[tokio::test]
    async fn a_turn_that_has_not_failed_leaves_the_session_working() {
        for (name, content) in [("running", RUNNING), ("completed", COMPLETED)] {
            let fixture = Fixture::new(name);
            fixture.watch();
            write_rollout(&fixture.path, content);
            assert!(
                !fixture.went_idle(POLL_INTERVAL * 6).await,
                "{name} must not move the session"
            );
        }
    }

    /// The next prompt's submission retires the watch, so a failure written afterwards moves
    /// nothing.
    #[tokio::test]
    async fn a_watch_retired_by_the_next_prompt_does_not_write() {
        let fixture = Fixture::new("retired");
        fixture.watch();
        fixture.state.end_record_watch("s");
        write_rollout(&fixture.path, failed_turn());
        assert!(!fixture.went_idle(POLL_INTERVAL * 6).await);
    }
}
