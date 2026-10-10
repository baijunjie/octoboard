//! Watching a Codex session's rollout file for the one thing its hooks never report: a turn that
//! ended in an error.
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
//! `codex_error_info` the cause), written about a second after the prompt and readable as soon as it is; a turn that
//! ended normally has `last_agent_message` and no `error` key at all. Codex flushes per record.
//! The file does not exist yet when the first prompt of a session is submitted — it appears about
//! a quarter of a second later — so a watch has to tolerate its absence.
//!
//! [`scan_for_ending`] is the pure half: given a path, an offset and the open turn's id, whether
//! that turn's `task_complete` was among the lines added. [`watch_for_failure`] is what calls it on
//! a timer from the turn's `UserPromptSubmit`, and turns a failure into a failed turn ending, the
//! same as Claude Code's `StopFailure`.
//!
//! The rollout is Codex's own and is rewritten on upgrades, so, as in `crate::transcript`, every
//! line is parsed on its own and one that fails to parse, or has a shape this does not recognise,
//! is skipped. Nothing here may abort the watch over a file it does not understand.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::hooks::TurnEnd;
use crate::protocol::SessionStatus;
use crate::state::{AppState, TurnClose};

/// How often the watch checks the rollout for new lines. A failed turn is over within about a
/// second of the prompt, and the session should come back to its prompt close to when Codex does.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// How the watched turn ended, as its `task_complete` record says.
#[derive(Debug, PartialEq, Eq)]
enum Ending {
    /// The turn ended without an error: it completed, which Codex follows with `Stop`, or it was
    /// interrupted, which Codex follows with `Interrupt`. Either hook ends it for Octoboard.
    Completed,
    /// The turn ended in an error, with the user-facing text where the record carries one.
    Failed(Option<String>),
}

/// What one scan of a rollout found.
struct Scan {
    /// The watched turn's `task_complete`, if it was among the lines read.
    ending: Option<Ending>,
    /// Where the next scan should start: the offset just past the last complete line read. A line
    /// still being written, with no trailing newline yet, is left for the next scan.
    offset: u64,
}

/// Reads whatever full lines were appended to the rollout at `path` since `offset`, and reports how
/// turn `turn_id` ended if one of them says so. An error is an I/O error opening or reading the
/// file, `NotFound` included, not a line that fails to parse.
fn scan_for_ending(path: &Path, offset: u64, turn_id: &str) -> io::Result<Scan> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut added = Vec::new();
    file.read_to_end(&mut added)?;

    let mut consumed = 0usize;
    let mut ending = None;
    while let Some(end) = added[consumed..].iter().position(|&byte| byte == b'\n') {
        let line = &added[consumed..consumed + end];
        consumed += end + 1;
        if let Some(found) = ending_in_line(line, turn_id) {
            ending = Some(found);
        }
    }
    Ok(Scan {
        ending,
        offset: offset + consumed as u64,
    })
}

/// How one rollout line says turn `turn_id` ended, if it is that turn's `task_complete`. A line
/// that is not JSON, or is JSON of another shape, reads as `None`.
///
/// Most lines never reach the JSON parser: a rollout line can run to hundreds of KB (a tool's
/// output), and a byte check for the record type rules nearly all of them out first.
fn ending_in_line(line: &[u8], turn_id: &str) -> Option<Ending> {
    if !std::str::from_utf8(line)
        .ok()?
        .contains("\"task_complete\"")
    {
        return None;
    }
    let value = serde_json::from_slice::<Value>(line).ok()?;
    let payload = value.get("payload")?;
    if payload.get("type").and_then(Value::as_str) != Some("task_complete")
        || payload.get("turn_id").and_then(Value::as_str) != Some(turn_id)
    {
        return None;
    }
    // Keyed on the `error` key alone: a normal completion has none, and `last_agent_message` is
    // null on a failed turn but also on an interrupted one.
    Some(
        match payload.get("error").filter(|error| !error.is_null()) {
            None => Ending::Completed,
            Some(error) => Ending::Failed(
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .map(str::to_string),
            ),
        },
    )
}

/// Starts watching `session_id`'s rollout for the end of its open turn `turn_id`. Call it from a
/// Codex `UserPromptSubmit`, where both are known.
///
/// Spawned, never awaited: the hook response has to return inside the adapters' few-second timeout.
/// The watch ends when it finds the turn's ending, when a newer watch for the session supersedes it
/// (`AppState::begin_transcript_watch`), when the next prompt's submission retires it
/// (`end_transcript_watch`), or when the process is gone.
///
/// It reads from where the session's previous watch of the same file got to, and from the start of
/// the file for the first. Not from the file's end as it stands now: the record it is after can
/// already be written by the time this is called. Which turn an ending belongs to is settled by the
/// turn's id, not by where the scan started.
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
    let path = PathBuf::from(transcript_path);
    // The hook callback is a fresh exec plus a request, and a turn that fails at once can have its
    // record written before the callback arrives — starting at the file's end would miss the only
    // notice the turn ever gives, which is the whole defect. Nothing is misattributed by reading
    // earlier records, since an ending counts only for the exact open turn's id.
    let mut offset = state.rollout_offset(session_id, &path);

    let generation = state.begin_transcript_watch(session_id);
    let state = state.clone();
    let session_id = session_id.to_string();
    let turn_id = turn_id.to_string();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;

            if !state.transcript_watch_current(&session_id, generation) {
                return; // A newer watch, or a hook ending the turn, has taken over.
            }
            if state.live_session(&session_id).is_none() {
                return; // The process is gone.
            }

            let len = match tokio::fs::metadata(&path).await {
                Ok(meta) => meta.len(),
                Err(_) => continue, // Not created yet.
            };
            if len < offset {
                // A shorter file than was read: rescan it from the start. That is safe because
                // matching is keyed on the open turn's id, and skipping to the new end could skip
                // the very record this watch is after. Not expected for a per-thread rollout.
                tracing::debug!(
                    session = %session_id, old_offset = offset, new_len = len,
                    "the rollout shrank; rescanning it from the start"
                );
                offset = 0;
            }
            if len == offset {
                continue;
            }

            // Blocking reads, and on a long session megabytes of them: off the runtime's threads.
            let scan = {
                let (path, turn_id) = (path.clone(), turn_id.clone());
                tokio::task::spawn_blocking(move || scan_for_ending(&path, offset, &turn_id)).await
            };
            let scan = match scan {
                Ok(Ok(scan)) => scan,
                Ok(Err(err)) if err.kind() == io::ErrorKind::NotFound => continue,
                Ok(Err(err)) => {
                    tracing::warn!(
                        session = %session_id, %err,
                        "reading the rollout failed while watching it for an error"
                    );
                    return;
                }
                Err(_) => return, // The scan panicked.
            };
            offset = scan.offset;
            state.note_rollout_offset(&session_id, &path, offset);
            match scan.ending {
                None => continue,
                // `Stop` is on its way and ends the turn in the usual way.
                Some(Ending::Completed) => return,
                Some(Ending::Failed(message)) => {
                    if state.transcript_watch_current(&session_id, generation) {
                        end_failed_turn(&state, &session_id, message);
                    }
                    return;
                }
            }
        }
    });
}

/// Ends the open turn as a failed one, as if the agent had fired a failure event for it: the turn
/// closes, a bound session's owner gets its report, and the session goes back to awaiting
/// instructions.
fn end_failed_turn(state: &Arc<AppState>, session_id: &str, message: Option<String>) {
    let turn = TurnEnd {
        last_assistant_message: message,
        failed: true,
        backstop: false,
    };
    match crate::reporting::close_turn_and_report(state, session_id, turn) {
        // No turn open means a hook ended it first, and the session is already where it belongs.
        // `NotThisTurn` cannot come back for an end that is not clock-attributed; it is grouped
        // here only to keep the match exhaustive.
        TurnClose::NoTurn | TurnClose::NotThisTurn => {
            tracing::debug!(
                session = %session_id,
                "a failed turn was found in the rollout, but no turn was open"
            );
        }
        TurnClose::OwesReport | TurnClose::Reported => {
            tracing::debug!(
                session = %session_id,
                "a turn ended in an error; the agent is back at its prompt"
            );
            if let Err(err) = state.apply_hook_status(session_id, SessionStatus::Idle) {
                tracing::debug!(
                    session = %session_id, %err,
                    "applying the status after a failed turn failed"
                );
            }
            state.spawn_flush_outbox(session_id, SessionStatus::Idle);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::protocol::{Agent, Console, Origin, Role, Session};
    use crate::test_support::{idle_stand_in, ScratchDir, ScratchFile, StandIn};

    /// A failed turn's records, taken verbatim from a Codex 0.160.0 session whose account was at
    /// its usage limit: `task_started`, `token_count`, then `task_complete` with an `error` object.
    const FAILED: &str = include_str!("../testdata/rollout_failed_turn.jsonl");
    const FAILED_TURN: &str = "01a127d8-56fb-7442-b291-53bde00f15fe";
    const SECOND_FAILED_TURN: &str = "01a127d8-feef-7872-9890-c06ccd669d7d";

    /// The near-miss: a successful turn's records, from a Codex 0.160.0 session, whose
    /// `task_complete` carries `last_agent_message` and no `error` key.
    const COMPLETED: &str = include_str!("../testdata/rollout_completed_turn.jsonl");
    const COMPLETED_TURN: &str = "01a1009e-6e43-73d3-a46f-5bc648f49a7a";

    /// The other near-miss: the failed capture cut off after `token_count`, a turn still running.
    const RUNNING: &str = include_str!("../testdata/rollout_running_turn.jsonl");

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
        scan_for_ending(&path, 0, turn_id).expect("scan").ending
    }

    #[test]
    fn a_failed_turn_is_found_with_codexs_own_message() {
        let Some(Ending::Failed(Some(message))) = ending_of(FAILED, FAILED_TURN) else {
            panic!("the failed turn was not recognised");
        };
        assert!(
            message.starts_with("You’ve hit your usage limit"),
            "{message}"
        );
    }

    #[test]
    fn a_turn_that_completed_normally_is_not_a_failure() {
        assert_eq!(
            ending_of(COMPLETED, COMPLETED_TURN),
            Some(Ending::Completed)
        );
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
        let two = include_str!("../testdata/rollout_failed_turns.jsonl");
        let second_started = two.lines().nth(3).expect("the second turn's start");
        assert!(second_started.contains(SECOND_FAILED_TURN));
        let cut = &two[..two.find(second_started).unwrap() + second_started.len() + 1];

        assert!(matches!(
            ending_of(cut, FAILED_TURN),
            Some(Ending::Failed(_))
        ));
        assert_eq!(ending_of(cut, SECOND_FAILED_TURN), None);
        assert!(matches!(
            ending_of(two, SECOND_FAILED_TURN),
            Some(Ending::Failed(_))
        ));

        // A successful turn before a failed one: each id reads its own ending.
        let mixed = format!("{COMPLETED}{FAILED}");
        assert_eq!(ending_of(&mixed, COMPLETED_TURN), Some(Ending::Completed));
        assert!(matches!(
            ending_of(&mixed, FAILED_TURN),
            Some(Ending::Failed(_))
        ));
    }

    /// Unrecognised and malformed lines are skipped, and the record after them is still read.
    #[test]
    fn unrecognised_lines_are_skipped_without_error() {
        let content = format!("not json\n{{\"task_complete\":1}}\n\"task_complete\"\n{FAILED}");
        assert!(matches!(
            ending_of(&content, FAILED_TURN),
            Some(Ending::Failed(_))
        ));
    }

    /// The production entry condition: the watch starts before Codex has created the file.
    #[test]
    fn a_file_that_does_not_exist_yet_reads_as_not_found() {
        let path = rollout_file("missing");
        std::fs::remove_file(&path).ok();
        let err = scan_for_ending(&path, 0, FAILED_TURN)
            .err()
            .expect("an error");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    /// The other: a record caught mid-write has no newline yet and is left for the next scan, which
    /// then reads it whole.
    #[test]
    fn a_line_truncated_mid_write_is_read_once_it_is_complete() {
        let last = FAILED.trim_end().rsplit('\n').next().unwrap();
        let before = &FAILED[..FAILED.len() - last.len() - 1];
        let cut = &last[..last.len() / 2];

        let path = rollout_file("truncated");
        write_rollout(&path, &format!("{before}{cut}"));
        let first = scan_for_ending(&path, 0, FAILED_TURN).expect("scan");
        assert_eq!(first.ending, None);
        assert_eq!(first.offset, before.len() as u64);

        write_rollout(&path, FAILED);
        let second = scan_for_ending(&path, first.offset, FAILED_TURN).expect("scan");
        assert!(matches!(second.ending, Some(Ending::Failed(_))));
    }

    fn codex_session(id: &str, status: SessionStatus) -> Session {
        Session {
            id: id.to_string(),
            agent: Agent::Codex,
            agent_session_id: None,
            console_id: "console-1".to_string(),
            project_id: None,
            host_id: crate::store::LOCAL_HOST_ID.to_string(),
            role: Role::Project,
            origin: Origin::User,
            title: "Session".to_string(),
            status,
            has_conversation: true,
            bound_to: None,
            lead: false,
            colour: None,
            ordinal: None,
            account_id: None,
            config_dir: None,
            pinned: false,
            started_at: 0,
            ended_at: None,
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
                    console_session_agent: Agent::Codex,
                    default_agent: Agent::Codex,
                    claude_account_id: None,
                    codex_account_id: None,
                    grok_account_id: None,
                    icon: None,
                    created_at: 0,
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
                crate::trust::wait_for(patience, || {
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
        write_rollout(&fixture.path, FAILED);

        assert!(fixture.went_idle(crate::test_support::PATIENCE).await);
        assert_eq!(fixture.state.close_turn("s", false), TurnClose::NoTurn);
    }

    /// The race the watch's start offset exists for: Codex wrote the failed turn's `task_complete`
    /// before the hook callback armed the watch (0.25 s of margin on a session's first turn,
    /// against a fresh hook exec and an HTTP round trip). A watch started at the file's length would begin
    /// past the record and never see it. The case above only covers a file that does not exist yet,
    /// where any start offset reads from zero, so it cannot catch this.
    #[tokio::test]
    async fn a_failure_written_before_the_watch_was_armed_is_still_found() {
        let fixture = Fixture::new("written-first");
        write_rollout(&fixture.path, FAILED);
        fixture.watch();

        assert!(fixture.went_idle(crate::test_support::PATIENCE).await);
        assert_eq!(fixture.state.close_turn("s", false), TurnClose::NoTurn);
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
        fixture.state.end_transcript_watch("s");
        write_rollout(&fixture.path, FAILED);
        assert!(!fixture.went_idle(POLL_INTERVAL * 6).await);
    }
}
