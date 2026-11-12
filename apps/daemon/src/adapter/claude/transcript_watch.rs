//! Watching a Claude Code session's transcript for the two things its hooks never report: the user
//! rejecting a permission prompt or declining its own `AskUserQuestion`, and the user interrupting
//! a turn while the session is working. Claude Code's record store arms it (`super::records`), on
//! the record watch its hook interpreter asks for after an event.
//!
//! The permission-prompt path moves a session to `SessionStatus::WaitingUser` and then, on a
//! decline, falls silent — measured against the real CLI (2.1.274 and 2.1.286): no `Stop`, no
//! `Notification`, nothing. `PermissionDenied` exists in the binary's strings but never fires. The
//! only machine-readable trace is in the transcript JSONL at the payload's `transcript_path`: a
//! `tool_result` with `is_error: true` whose text begins "The user doesn't want to proceed with
//! this tool use", immediately followed by a user entry reading `[Request interrupted by user for
//! tool use]`. Either one alone says the pending decision was resolved by a rejection or an
//! interrupt — see `docs/agent-cli-reference.md`, section "## Claude Code", for the full
//! measurement. Cancelling an `AskUserQuestion` with Esc writes the same two records, field for
//! field — measured on 2.1.286 — and falls just as silent afterwards: no hook event either.
//!
//! On 2.1.289 the records and the recovery were confirmed in the running application rather than by
//! the same event-by-event measurement: a declined prompt wrote the record, and the session left
//! `WaitingUser` 484 ms later — a delay that fits this watch's poll and not a hook, which arrives at
//! once.
//!
//! The question list has no plain "No": its nearest exit, `4. Chat about this`, writes a
//! `tool_result` that starts with the same rejected-tool-use prefix but continues differently,
//! with no interrupt line after it, and the turn actually carries on — `PostToolBatch` and `Stop`
//! still fire. Matching on the prefix alone therefore catches that exit too, briefly reporting
//! `Idle` while the turn is in fact still running; the next `PostToolBatch` corrects it within one
//! step. That is an accepted trade: requiring the interrupt marker as well would narrow the match
//! and give up the redundancy that is this module's whole defence against the transcript format
//! changing under it.
//!
//! [`is_rejection_line`] is the pure half: whether one transcript line carries one of those two
//! markers. [`watch_for_rejection`] arms `crate::records::watch` with it, from the moment a hook
//! puts a Claude Code session into `WaitingUser`, until the rejection is found and applied or the
//! watch is no longer owed anything.
//!
//! An interrupt while the session is working — Esc pressed while the model is thinking, writing or
//! running a tool — is silent in the same way: the interrupted turn fires no hook at all, so the
//! session would read `Working` until its next prompt. What it leaves is a user entry whose text
//! block is exactly `[Request interrupted by user]` (an interrupt while the model thinks or writes,
//! seen in real transcripts of 2.1.296, 2.1.302 and 2.1.303; on 2.1.310 only once the model has
//! shown output) or `[Request interrupted by user for tool use]` (one while a tool runs, after the
//! rejection `tool_result` above; see the same section of `docs/agent-cli-reference.md`).
//! [`is_interrupt_entry`] recognises either, and [`watch_for_interrupt`] arms the watch with it
//! each time a hook moves a Claude Code session into `Working`, until the interrupt is found or the
//! session leaves `Working` some other way. Only an interrupt of the turn the hook named counts,
//! told by the `promptId` the entry carries. A cancel the agent carries straight on from, taking a
//! message the user left in its queue as the next turn, leaves the session working
//! ([`InterruptReading`]).
//!
//! An interrupt before the model has shown anything — while the request waits for its response or
//! the model is still thinking — leaves no marker at all on 2.1.310: the transcript is not written
//! to. That one is read off the terminal title instead, by `crate::session::title_interrupt`, armed
//! beside this watch and sharing its generation.
//!
//! The transcript is Claude Code's own and is rewritten on every upgrade; see
//! `crate::records::watch` for how a line this does not recognise is treated.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::protocol::SessionStatus;
use crate::records::watch::{self, Owed, Start, Then, Watch};
use crate::state::AppState;
use crate::time::now_millis;
use crate::time::parse_rfc3339_millis;

/// The start of the `tool_result` text Claude Code gives a rejected tool call.
const REJECTED_TOOL_USE_PREFIX: &str = "The user doesn't want to proceed with this tool use";

/// The user entry Claude Code writes immediately after the rejection `tool_result`, which is also
/// what an interrupt while a tool runs writes.
const INTERRUPTED_MARKER: &str = "[Request interrupted by user for tool use]";

/// The user entry Claude Code writes for an interrupt while the model is writing, and before
/// 2.1.310 while it is thinking too.
const INTERRUPTED_PLAIN_MARKER: &str = "[Request interrupted by user]";

/// What both interrupt markers start with, for ruling a line out before it is parsed.
const INTERRUPTED_MARKER_STEM: &str = "[Request interrupted by user";

/// What a `queue-operation` entry for a message taken from Claude Code's queue carries, for ruling
/// a line out before it is parsed.
const DEQUEUE_OPERATION: &str = "\"dequeue\"";

/// How far apart a dequeue and an interrupt may be stamped and still be one cancel that the agent
/// carried straight on from. Seen 4 to 11 ms apart (2.1.322), so a dequeue a second or more from
/// the interrupt is taken for something else.
const DEQUEUE_GAP: Duration = Duration::from_secs(1);

/// How long before the interrupt watch was armed an interrupt may be timestamped and still count.
/// The watch is armed when the daemon applies the hook event that moved the session into work,
/// which can be a while after Claude Code fired it. From 2.1.63 the hook is an HTTP post, answered
/// at once and applied afterwards from the session's queue of events, behind any still waiting
/// there; on an older Claude Code, or where an allowlist leaves the daemon's URL out, it is an
/// asynchronous command, which can take seconds to start while the machine is slow to start
/// processes. So an interrupt pressed right as a turn began can be stamped a little before the
/// watch existed. History copied from an earlier conversation is older than that.
const ARMING_SLACK: Duration = Duration::from_secs(5);

/// How often the watch checks the transcript for new lines. Fast enough that the status follows
/// close behind the person's answer or interrupt, cheap enough to keep polling for as long as they
/// take to decide or the turn runs.
const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Whether one transcript line is a rejection or interrupt record. Any line that is not valid JSON,
/// or valid JSON this does not recognise the shape of, reads as `false` rather than erroring —
/// that is what lets an unrelated line, or a future transcript format, pass through unremarked.
///
/// Most lines never reach the JSON parser at all: on the common path — the user answered `Yes`, and
/// the session sits in `WaitingUser` for the rest of the approved tool's run, per
/// `docs/agent-cli-reference.md`, section "## Claude Code" — the appended line is that tool's own
/// completed `tool_result`, which for a `Bash` with a lot of output or a `Read` of a big file runs
/// to hundreds of KB or more. A substring check for either marker rules out such a line before
/// paying for a DOM parse of it; only a line that actually contains one of the markers is parsed,
/// and the structural check below (`is_error: true`, `starts_with`, exact equality) requires the
/// full shape as well as the text, so a tool that merely prints one of these strings still cannot
/// match. The substring check assumes a marker appears in the serialized line exactly as it does
/// decoded — true today, since both markers are plain printable ASCII and Claude Code's
/// `JSON.stringify` escapes none of it, but a marker written out with a JSON escape would slip past
/// this check even though the structural check below, which matches the decoded string, would
/// still have caught it.
///
/// Only a user entry counts, for the reason [`is_interrupt_entry`] gives: the transcript also holds
/// logged requests whose `message.content` replays the conversation so far, earlier declines
/// included. Whether one is ever written while the hand is up was not measured; matching only user
/// entries makes that not matter.
fn is_rejection_line(line: &str) -> bool {
    if !line.contains(REJECTED_TOOL_USE_PREFIX) && !line.contains(INTERRUPTED_MARKER) {
        return false;
    }
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return false;
    };
    if value.get("type").and_then(Value::as_str) != Some("user") {
        return false;
    }
    let Some(content) = value.pointer("/message/content").and_then(Value::as_array) else {
        return false;
    };
    content.iter().any(is_rejection_marker)
}

/// Whether one block of a transcript entry's content is either marker.
fn is_rejection_marker(block: &Value) -> bool {
    match block.get("type").and_then(Value::as_str) {
        Some("tool_result") => {
            block.get("is_error").and_then(Value::as_bool) == Some(true)
                && tool_result_text(block)
                    .is_some_and(|text| text.starts_with(REJECTED_TOOL_USE_PREFIX))
        }
        Some("text") => block.get("text").and_then(Value::as_str) == Some(INTERRUPTED_MARKER),
        _ => false,
    }
}

/// What the interrupt watch found.
#[derive(Debug, PartialEq, Eq)]
enum Interrupt {
    /// The turn was cancelled, and the agent is back at its prompt.
    Cancelled,
    /// The turn was cancelled, and the agent went straight on with a message the user had left in
    /// its queue, as a turn of its own the watch now reads for.
    CarriedOn,
}

/// What one interrupt watch has read, across its scans: which turn it is reading for, and enough
/// to tell a cancel that leaves the agent at its prompt from one it carries straight on from.
///
/// Esc pressed while a message waits in Claude Code's queue cancels the turn and runs the message
/// as the next one, and no hook says so: the message's `UserPromptSubmit` fired when it was queued,
/// carrying the running turn's `prompt_id`, and none fires when it is taken; the next hook is that
/// turn's own `Stop` (2.1.322). What the transcript shows, all within some 25 ms, is the cancelled
/// turn's interrupt, a `queue-operation` entry whose `operation` is `dequeue` (written before the
/// interrupt in the file every time it was seen, though stamped a few milliseconds after it), and
/// the message as a user entry whose `parentUuid` is the interrupt's `uuid` and whose `promptId`
/// names a new turn. Either the dequeue or that entry, read with the interrupt, says the agent
/// carried on, and the watch goes on for the new turn: the turn the entry names, or any turn until
/// it is read, the cancelled turn's interrupt being behind it by then. A message taken into the
/// running turn instead, at a tool's end, is removed from the queue (`remove`), not dequeued, and
/// the turn's own interrupt then ends it as any other.
struct InterruptReading {
    /// How early an interrupt may be stamped and still count, in milliseconds since the epoch.
    not_before: i64,
    /// The turn being read for, as its `prompt_id` names it, where that is known.
    turn: Option<String>,
    /// The latest interrupt of the turn being read for, until the entry of a turn the agent
    /// carried on to from it has been read.
    interrupt: Option<InterruptEntry>,
    /// When the latest dequeue not yet taken for a carried-on cancel was stamped.
    dequeued_at: Option<i64>,
}

/// The parts of an interrupt entry the entries after it are told by.
struct InterruptEntry {
    uuid: Option<String>,
    turn: Option<String>,
    written: i64,
    /// The agent has been found to carry on from it.
    carried_on: bool,
}

impl InterruptReading {
    fn new(not_before: i64, turn: Option<String>) -> Self {
        Self {
            not_before,
            turn,
            interrupt: None,
            dequeued_at: None,
        }
    }

    /// What `line` makes of the turn being read for, or `None` where it changes nothing. A watch's
    /// scan acts on the last answer it got, so a line read after the interrupt that shows the agent
    /// carried on overrides the [`Interrupt::Cancelled`] the interrupt gave, and the next turn's
    /// own interrupt after that overrides it in turn.
    ///
    /// Unrecognised lines change nothing, as in [`is_rejection_line`], and a substring check keeps
    /// most lines away from the parser.
    fn read(&mut self, line: &str) -> Option<Interrupt> {
        let names_interrupt = self
            .interrupt
            .as_ref()
            .and_then(|interrupt| interrupt.uuid.as_deref())
            .is_some_and(|uuid| line.contains(uuid));
        if !line.contains(INTERRUPTED_MARKER_STEM)
            && !line.contains(DEQUEUE_OPERATION)
            && !names_interrupt
        {
            return None;
        }
        let value = serde_json::from_str::<Value>(line).ok()?;
        if value.get("type").and_then(Value::as_str) == Some("queue-operation") {
            if value.get("operation").and_then(Value::as_str) == Some("dequeue") {
                self.dequeued_at = written(&value);
                return self.carry_on_if_dequeued();
            }
            return None;
        }
        if is_interrupt_entry(&value, self.not_before, self.turn.as_deref()) {
            self.interrupt = Some(InterruptEntry {
                uuid: text_field(&value, "uuid"),
                turn: text_field(&value, "promptId"),
                written: written(&value)?,
                carried_on: false,
            });
            return Some(self.carry_on_if_dequeued().unwrap_or(Interrupt::Cancelled));
        }
        let interrupt = self.interrupt.as_ref()?;
        let next_turn = text_field(&value, "promptId")?;
        if value.get("type").and_then(Value::as_str) != Some("user")
            || interrupt.uuid.is_none()
            || value.get("parentUuid").and_then(Value::as_str) != interrupt.uuid.as_deref()
            || Some(&next_turn) == interrupt.turn.as_ref()
        {
            return None;
        }
        let found = (!interrupt.carried_on).then_some(Interrupt::CarriedOn);
        self.turn = Some(next_turn);
        self.interrupt = None;
        self.dequeued_at = None;
        found
    }

    /// Takes the latest interrupt as one the agent carried on from, if a dequeue was stamped close
    /// enough to it to be the agent taking the next message from its queue as the cancel ended the
    /// turn. The turn being read for is not known again until the next turn's entry is read.
    fn carry_on_if_dequeued(&mut self) -> Option<Interrupt> {
        let dequeued_at = self.dequeued_at?;
        let interrupt = self.interrupt.as_mut()?;
        if interrupt.carried_on
            || (dequeued_at - interrupt.written).abs() > DEQUEUE_GAP.as_millis() as i64
        {
            return None;
        }
        interrupt.carried_on = true;
        self.turn = None;
        self.dequeued_at = None;
        Some(Interrupt::CarriedOn)
    }
}

/// Whether one transcript entry is an interrupt record of the turn `turn` names: a user entry,
/// timestamped `not_before` (milliseconds since the epoch) or later, with a text block that is
/// exactly either interrupt marker.
///
/// An entry with no timestamp this can read is not counted: that leaves the session reading
/// `Working`, as it did before this watch existed, which is the safer of the two mistakes.
///
/// Claude Code stamps every user entry of a turn, its interrupt included, with the turn's
/// `promptId`, the `prompt_id` its hooks carry through the same turn, so an interrupt naming
/// another turn is not this one's, wherever it falls in the file. Where either side names no turn,
/// the entry is judged on its timestamp and place in the file alone.
///
/// The entry's own `type` is checked as well as its content, because the transcript also logs each
/// request sent to the model as an `api-request-blob` entry whose `message.content` replays the
/// conversation so far — interrupt markers from earlier turns included, seen with a trailing
/// newline in real transcripts but not guaranteed to keep one. Such an entry is written while the
/// session is working, which is exactly when this watch reads.
fn is_interrupt_entry(value: &Value, not_before: i64, turn: Option<&str>) -> bool {
    if value.get("type").and_then(Value::as_str) != Some("user") {
        return false;
    }
    let named = value.get("promptId").and_then(Value::as_str);
    if let (Some(turn), Some(named)) = (turn, named) {
        if named != turn {
            return false;
        }
    }
    if written(value).is_none_or(|written| written < not_before) {
        return false;
    }
    let Some(content) = value.pointer("/message/content").and_then(Value::as_array) else {
        return false;
    };
    content.iter().any(|block| {
        block.get("type").and_then(Value::as_str) == Some("text")
            && matches!(
                block.get("text").and_then(Value::as_str),
                Some(INTERRUPTED_MARKER | INTERRUPTED_PLAIN_MARKER)
            )
    })
}

/// When a transcript entry was written, in milliseconds since the epoch.
fn written(value: &Value) -> Option<i64> {
    value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_rfc3339_millis)
}

fn text_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// A `tool_result` block's text, however its `content` is shaped: a plain string in every
/// transcript this was measured against, but content blocks are Claude Code's general shape for
/// text elsewhere, so that form is read too rather than assumed away.
fn tool_result_text(block: &Value) -> Option<&str> {
    match block.get("content")? {
        Value::String(text) => Some(text.as_str()),
        Value::Array(blocks) => blocks
            .iter()
            .find_map(|block| block.get("text").and_then(Value::as_str)),
        _ => None,
    }
}

/// Starts watching `session_id`'s transcript for the rejection record that is the only trace left
/// of a declined permission prompt or a declined `AskUserQuestion` — both measured to write the
/// same record, see the module doc. Call this only once a hook has actually put the session into
/// `WaitingUser` for one of those two reasons — a session that resolves its own approvals never
/// reaches `WaitingUser` at all, and Codex and Grok report a decline through their own events and
/// must get no watcher.
///
/// Spawned, never awaited: the hook response has to return inside the adapters' few-second
/// timeout, and a person may sit at the prompt for far longer than that.
///
/// The transcript's current length is read before anything else ([`Start::AtEnd`]), so a rejection
/// left over from an earlier turn is never mistaken for this one's. A fresh generation is taken
/// from `state.begin_record_watch` before spawning, so a newer watch started for the same session
/// retires this one.
pub fn watch_for_rejection(state: &Arc<AppState>, session_id: &str, transcript_path: Option<&str>) {
    let Some(transcript_path) = transcript_path else {
        tracing::debug!(
            session = %session_id,
            "no transcript path on a pending decision; it cannot be watched for a rejection"
        );
        return;
    };
    watch::spawn(
        state,
        session_id,
        PathBuf::from(transcript_path),
        Watch {
            interval: POLL_INTERVAL,
            start: Start::AtEnd,
        },
        |state, session_id| still_at(state, session_id, SessionStatus::WaitingUser),
        |line| is_rejection_line(line).then_some(()),
        |state, session_id, generation, ()| {
            watch::back_at_prompt(
                state,
                session_id,
                generation,
                SessionStatus::WaitingUser,
                "transcript rejection",
            );
            Then::Stop
        },
    );
}

/// Starts watching `session_id`'s transcript for the record an interrupted turn leaves, the only
/// trace of the user cancelling it; Claude Code's hook interpreter says when
/// (`HookInterpreter::record_watch`), and names the turn as `turn`, the event's `prompt_id`.
/// Returns the watch's generation, or `None` when no watch was started.
///
/// The watch reads only what is written after it is armed ([`Start::AtEndOrNew`]), so an interrupt
/// left over from an earlier turn, or from a conversation resumed with its history, is never
/// mistaken for this one's. A transcript that is not there yet is read from its start once it
/// appears: a new conversation's first `UserPromptSubmit` runs before Claude Code has written the
/// file (measured on 2.1.303, in print mode). A file created then could still be seeded with older
/// lines, should Claude Code ever copy a forked or resumed conversation's history into it lazily,
/// so an interrupt also counts only if it is stamped no earlier than the watch was armed, less
/// [`ARMING_SLACK`], and only if it names the watch's turn ([`is_interrupt_entry`]). The next prompt
/// retires the watch before its turn opens (`end_record_watch`), and the write takes effect only
/// while the watch is still current, so an interrupt found late is never applied to the turn after
/// it. Spawned, never awaited, as [`watch_for_rejection`] is.
///
/// The turn is left open, as a declined prompt leaves it: no report is owed for a turn the user
/// cancelled, Claude Code has no clock-attributed turn end that could find it open, and the next
/// prompt's `turn_started` replaces it. Closing it here could instead close the next turn, were its
/// prompt to land between the status write and the close.
///
/// A cancel the agent carries straight on from, with a message from its queue, leaves the session
/// working, and the watch reading for the turn it carried on to ([`InterruptReading`],
/// [`carried_on`]).
pub fn watch_for_interrupt(
    state: &Arc<AppState>,
    session_id: &str,
    transcript_path: Option<&str>,
    turn: Option<&str>,
) -> Option<u64> {
    let Some(transcript_path) = transcript_path else {
        tracing::debug!(
            session = %session_id,
            "no transcript path on a working session's hook; it cannot be watched for an interrupt"
        );
        return None;
    };
    let reading = std::sync::Mutex::new(InterruptReading::new(
        now_millis() - ARMING_SLACK.as_millis() as i64,
        turn.map(str::to_string),
    ));
    watch::spawn(
        state,
        session_id,
        PathBuf::from(transcript_path),
        Watch {
            interval: POLL_INTERVAL,
            start: Start::AtEndOrNew,
        },
        |state, session_id| still_at(state, session_id, SessionStatus::Working),
        move |line| {
            reading
                .lock()
                .expect("interrupt reading poisoned")
                .read(line)
        },
        |state, session_id, generation, found| match found {
            Interrupt::Cancelled => {
                watch::back_at_prompt(
                    state,
                    session_id,
                    generation,
                    SessionStatus::Working,
                    "transcript interrupt",
                );
                Then::Stop
            }
            Interrupt::CarriedOn => carried_on(state, session_id, generation),
        },
    )
}

/// The turn watch `generation` was reading for was cancelled, and the agent went straight on with
/// a message from its queue as the next turn, which no hook announces ([`InterruptReading`]). The
/// session stays `Working`, and so does the turn the daemon holds open: the message joined it when
/// its `UserPromptSubmit` came at queueing, and the next turn's `Stop` closes it. The cancel still
/// counts as a turn ending (`note_turn_end`), and the watch, with the title watch sharing its
/// generation, goes on reading for the next turn, so a cancel of that one is caught too.
fn carried_on(state: &Arc<AppState>, session_id: &str, generation: u64) -> Then {
    let still_working = state.while_record_watch_current(session_id, generation, || {
        let working = still_at(state, session_id, SessionStatus::Working) == Owed::Yes;
        if working {
            state.note_turn_end(session_id);
        }
        working
    });
    if still_working == Some(true) {
        tracing::debug!(
            session = %session_id,
            "a transcript interrupt was found, and the agent went on with a queued message"
        );
        Then::KeepWatching
    } else {
        tracing::debug!(
            session = %session_id,
            "a transcript interrupt was found, but the session had already moved on"
        );
        Then::Stop
    }
}

/// Whether the agent is still at the `status` the watch was armed for.
fn still_at(state: &AppState, session_id: &str, status: SessionStatus) -> Owed {
    match state.store.get_session(session_id) {
        Ok(Some(session)) if state.agent_status(&session) == status => Owed::Yes,
        // A real hook event already moved the session on, or it is gone from the store
        // outright — either way, nothing here is still owed.
        Ok(_) => Owed::No,
        Err(err) => {
            // Transient, same as a metadata read failing — retry rather than abandoning the watch
            // and leaving the status stale with nothing in the log.
            tracing::debug!(
                session = %session_id, %err,
                "reading the session failed while watching its transcript"
            );
            Owed::Unknown
        }
    }
}

/// Lets a transcript watch poll [`TEST_POLLS`] times, for a test on tokio's paused clock
/// (`#[tokio::test(start_paused = true)]`) that then reads what the polls did. A poll's reads are
/// blocking tasks, which hold that clock until they end, so each poll it reaches has run to its
/// end when this returns, however slowly the machine ran it; the last one falls due as this
/// returns and may not have run yet. A test that waits on the watch this way must not hold the
/// clock with a blocking wait of its own (`test_support::within`).
#[cfg(test)]
pub(crate) async fn polls_ran() {
    tokio::time::sleep(POLL_INTERVAL * TEST_POLLS).await;
}

/// How many polls [`polls_ran`] lets a watch make: a few, so that a write the test made is read
/// by at least two.
#[cfg(test)]
const TEST_POLLS: u32 = 3;

#[cfg(test)]
mod tests {

    use super::*;
    use crate::protocol::{AgentId, Console, Session, SessionKind};
    use crate::test_support::{
        bare_session, captured_interrupt, fresh_interrupt, fresh_interrupt_of, idle_stand_in,
        queued_prompt_after_interrupt, ScratchDir, ScratchFile, StandIn, CAPTURED_TURN,
        QUEUED_TURN,
    };

    /// Whether `line`, read on its own by a watch for `turn` armed at `not_before`, is an interrupt
    /// of that turn.
    fn is_interrupt_line(line: &str, not_before: i64, turn: Option<&str>) -> bool {
        InterruptReading::new(not_before, turn.map(str::to_string)).read(line)
            == Some(Interrupt::Cancelled)
    }

    /// What a watch for `turn` makes of `lines` read in order, as the last answer a scan of them
    /// acts on, and the turn it reads for afterwards.
    fn read_through(lines: &[&str], turn: &str) -> (Option<Interrupt>, Option<String>) {
        let mut reading = InterruptReading::new(i64::MIN, Some(turn.to_string()));
        let found = lines.iter().filter_map(|line| reading.read(line)).last();
        (found, reading.turn)
    }

    /// The lines of [`queued_prompt_after_interrupt`]: the dequeue, the interrupt, the logged
    /// request replaying it and the queued message's prompt.
    fn queued_cancel_lines() -> Vec<String> {
        let lines: Vec<String> = queued_prompt_after_interrupt(QUEUED_TURN)
            .lines()
            .map(str::to_string)
            .collect();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].contains("\"dequeue\"") && lines[1].contains(INTERRUPTED_PLAIN_MARKER));
        lines
    }

    /// A real rejection record, trimmed to the two lines that carry the markers, captured from a
    /// permission-prompt decline on Claude Code 2.1.274; a capture of the same decline on 2.1.286
    /// was byte-for-byte the same shape, only the ids differing.
    const REJECTION: &str = include_str!("../../../testdata/transcript_rejection.jsonl");

    /// A normal completed tool call's `tool_result`, from the same capture, `is_error` absent: the
    /// ordinary case a rejection must not be confused with.
    const COMPLETED_TOOL_RESULT: &str =
        include_str!("../../../testdata/transcript_tool_result.jsonl");

    /// The same two records, from an `AskUserQuestion` cancelled with Esc on Claude Code 2.1.286 —
    /// field for field identical to a declined permission prompt's, down to `toolDenialKind`.
    const QUESTION_DECLINE: &str =
        include_str!("../../../testdata/transcript_question_decline.jsonl");

    /// The question list's own `4. Chat about this` exit, from the same capture: a `tool_result`
    /// that starts with the same rejected-tool-use prefix but continues differently, with no
    /// interrupt line following it — the shape the prefix match deliberately over-catches, see the
    /// module doc.
    const QUESTION_CHAT_EXIT: &str =
        include_str!("../../../testdata/transcript_question_chat_exit.jsonl");

    fn rejected(content: &str) -> bool {
        content.lines().any(is_rejection_line)
    }

    #[test]
    fn a_real_rejection_record_is_found() {
        assert!(rejected(REJECTION));
    }

    #[test]
    fn a_normal_completed_tool_result_does_not_match() {
        assert!(!rejected(COMPLETED_TOOL_RESULT));
    }

    /// The question-decline fixture is structurally identical to the permission-rejection one, so
    /// this is the regression anchor for the measured claim that the two paths write the same
    /// records — not independent branch coverage.
    #[test]
    fn an_ask_user_question_decline_is_found() {
        assert!(rejected(QUESTION_DECLINE));
    }

    /// The accepted over-match discussed in the module doc: the chat-exit record starts with the
    /// same prefix as a real rejection, so it is read as one too.
    #[test]
    fn the_chat_about_this_exit_also_matches_the_prefix() {
        // Pins the premise this test exists to check: if the fixture were ever re-captured with
        // an interrupt record too, it would stop testing the prefix branch and this must fail
        // loudly rather than silently pass for a different reason.
        assert!(!QUESTION_CHAT_EXIT.contains(INTERRUPTED_MARKER));

        assert!(rejected(QUESTION_CHAT_EXIT));
    }

    /// The realistic false positive: an agent reading this very file, or the transcript itself —
    /// which contains both marker strings verbatim — produces a completed, non-error `tool_result`
    /// whose text happens to start with the rejection prefix. The substring pre-filter alone would
    /// match it; the structural check (`is_error: true`) must not.
    #[test]
    fn a_tool_result_that_only_starts_with_the_prefix_but_is_not_an_error_does_not_match() {
        let line = format!(
            "{{\"type\":\"user\",\"message\":{{\"content\":[{{\"type\":\"tool_result\",\
             \"content\":\"{REJECTED_TOOL_USE_PREFIX} but this one is not an error\",\
             \"is_error\":false}}]}}}}\n"
        );
        assert!(!rejected(&line));
    }

    /// The substring pre-filter matches on raw bytes regardless of where they sit; the structural
    /// check that follows it must reject a marker sitting outside `/message/content` entirely.
    #[test]
    fn a_marker_string_outside_the_expected_shape_does_not_match() {
        let line = format!("{{\"type\":\"user\",\"note\":\"{REJECTED_TOOL_USE_PREFIX}\"}}\n");
        assert!(!rejected(&line));
    }

    #[test]
    fn a_transcript_with_no_rejection_at_all_is_not_reported_as_one() {
        assert!(!rejected("{\"type\":\"summary\"}\n"));
    }

    /// The transcript is rewritten on every Claude Code upgrade, so a line this does not recognise
    /// must not abort the scan — it is skipped, and whatever comes after it is still read.
    #[test]
    fn malformed_and_unknown_lines_are_skipped_without_error() {
        let path = ScratchFile::new("transcript-malformed", "transcript.jsonl");
        // The two cut-off lines carry a marker, so they reach the JSON parse, and fail it.
        let cut_rejection = format!("{{\"type\":\"user\",\"note\":\"{REJECTED_TOOL_USE_PREFIX}");
        let cut_interrupt = format!(
            "{{\"type\":\"user\",\"message\":{{\"content\":[{{\"type\":\"text\",\"text\":\"{INTERRUPTED_MARKER}"
        );
        std::fs::write(
            &path,
            format!(
                "not json at all\n{{\"no\":\"message field\"}}\n{cut_rejection}\n{cut_interrupt}\n{REJECTION}"
            ),
        )
        .expect("write transcript");
        let scan = watch::scan_lines(&path, 0, |line| is_rejection_line(line).then_some(()))
            .expect("scan");
        assert_eq!(scan.found, Some(()));
    }

    /// Both interrupt markers count: the plain one an interrupt while the model writes leaves, and
    /// the tool-use one in the rejection pair, which an interrupt while a tool runs also writes.
    /// Each is read as the watch of its own turn reads it.
    #[test]
    fn either_interrupt_record_is_found() {
        let tool_use_interrupt = REJECTION
            .lines()
            .nth(1)
            .expect("the rejection's interrupt line");
        for line in [captured_interrupt().trim_end(), tool_use_interrupt] {
            let entry: Value = serde_json::from_str(line).expect("entry");
            let turn = entry["promptId"].as_str();
            assert!(turn.is_some(), "{line}");
            assert!(is_interrupt_line(line, i64::MIN, turn), "{line}");
        }
    }

    /// An interrupt that names another turn is that turn's, however fresh.
    #[test]
    fn an_interrupt_naming_another_turn_is_not_this_ones() {
        let line = captured_interrupt().trim_end();
        assert!(!is_interrupt_line(line, i64::MIN, Some(QUEUED_TURN)));
        assert!(is_interrupt_line(line, i64::MIN, Some(CAPTURED_TURN)));
    }

    /// Where the watch was given no turn, or the entry names none, the interrupt is judged by its
    /// timestamp and its place in the file.
    #[test]
    fn an_interrupt_is_counted_when_either_side_names_no_turn() {
        let mut entry: Value = serde_json::from_str(captured_interrupt()).expect("entry");
        assert!(is_interrupt_line(&entry.to_string(), i64::MIN, None));
        entry.as_object_mut().expect("an object").remove("promptId");
        assert!(is_interrupt_line(
            &entry.to_string(),
            i64::MIN,
            Some(QUEUED_TURN)
        ));
    }

    /// The dequeue, written before the interrupt, says the agent carried on, and the watch then
    /// reads for any turn until the next turn's prompt names it.
    #[test]
    fn a_dequeue_beside_the_interrupt_says_the_agent_carried_on() {
        let lines = queued_cancel_lines();
        let (found, turn) = read_through(&[&lines[0], &lines[1]], CAPTURED_TURN);
        assert_eq!(found, Some(Interrupt::CarriedOn));
        assert_eq!(turn, None);

        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        let (found, turn) = read_through(&lines, CAPTURED_TURN);
        assert_eq!(found, Some(Interrupt::CarriedOn));
        assert_eq!(turn.as_deref(), Some(QUEUED_TURN));
    }

    /// The next turn's prompt under the interrupt says so too, with no dequeue read.
    #[test]
    fn the_next_turns_prompt_under_the_interrupt_says_the_agent_carried_on() {
        let lines = queued_cancel_lines();
        let (found, turn) = read_through(&[&lines[1], &lines[3]], CAPTURED_TURN);
        assert_eq!(found, Some(Interrupt::CarriedOn));
        assert_eq!(turn.as_deref(), Some(QUEUED_TURN));
    }

    /// A dequeue stamped well apart from the interrupt is not the agent taking the next message as
    /// the cancel ended the turn.
    #[test]
    fn a_dequeue_far_from_the_interrupt_is_not_the_agent_carrying_on() {
        let lines = queued_cancel_lines();
        let early = lines[0].replace("2100-01-01T00:00:00.006Z", "2099-12-31T23:59:50.000Z");
        let (found, turn) = read_through(&[&early, &lines[1]], CAPTURED_TURN);
        assert_eq!(found, Some(Interrupt::Cancelled));
        assert_eq!(turn.as_deref(), Some(CAPTURED_TURN));
    }

    /// A message taken into the running turn at a tool's end is removed from the queue, not
    /// dequeued, and the turn's interrupt then ends it (2.1.322).
    #[test]
    fn a_message_taken_into_the_running_turn_does_not_carry_the_cancel_on() {
        let lines = queued_cancel_lines();
        let removed = lines[0].replace("\"dequeue\"", "\"remove\"");
        let (found, _) = read_through(&[&removed, &lines[1]], CAPTURED_TURN);
        assert_eq!(found, Some(Interrupt::Cancelled));
    }

    /// An entry under the interrupt that names the same turn is not a next turn.
    #[test]
    fn an_entry_of_the_cancelled_turn_under_its_interrupt_is_not_a_next_turn() {
        let lines = queued_cancel_lines();
        let same_turn = lines[3].replace(QUEUED_TURN, CAPTURED_TURN);
        let (found, turn) = read_through(&[&lines[1], &same_turn], CAPTURED_TURN);
        assert_eq!(found, Some(Interrupt::Cancelled));
        assert_eq!(turn.as_deref(), Some(CAPTURED_TURN));
    }

    /// The rejection `tool_result` alone is not an interrupt: the `Chat about this` exit writes one
    /// and the turn carries on working.
    #[test]
    fn a_rejected_tool_result_alone_is_not_an_interrupt() {
        assert!(!QUESTION_CHAT_EXIT
            .lines()
            .any(|line| is_interrupt_line(line, i64::MIN, None)));
    }

    /// A logged request replays the conversation so far, an earlier turn's interrupt included, and
    /// is written while the session works; only a user entry of its own is an interrupt.
    #[test]
    fn an_interrupt_replayed_in_a_logged_request_is_not_one() {
        let line = format!(
            "{{\"type\":\"api-request-blob\",\"message\":{{\"role\":\"user\",\
             \"content\":[{{\"type\":\"text\",\"text\":\"{INTERRUPTED_PLAIN_MARKER}\"}}]}}}}"
        );
        assert!(!is_interrupt_line(&line, i64::MIN, None));
    }

    /// An interrupt whose time cannot be read is not counted: the watch cannot tell it from an
    /// earlier conversation's.
    #[test]
    fn an_interrupt_with_no_readable_timestamp_is_not_one() {
        let mut entry: Value = serde_json::from_str(captured_interrupt()).expect("entry");
        entry["timestamp"] = Value::from("not a time");
        assert!(!is_interrupt_line(&entry.to_string(), i64::MIN, None));
    }

    fn state_with_console(name: &str) -> (Arc<AppState>, ScratchDir) {
        let (state, dir) = crate::test_support::app_state(&format!("transcript-{name}"));
        state
            .store
            .insert_console(&Console {
                id: "console-1".to_string(),
                name: "Console".to_string(),
                workdir: dir.to_string_lossy().into_owned(),
                default_agent: AgentId::CLAUDE,
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
        (state, dir)
    }

    fn waiting_session(id: &str) -> Session {
        Session {
            status: SessionStatus::WaitingUser,
            has_conversation: true,
            ..bare_session(id, SessionKind::Project)
        }
    }

    fn working_session(id: &str) -> Session {
        Session {
            status: SessionStatus::Working,
            ..waiting_session(id)
        }
    }

    /// Starting a second watch for the same session must retire the first: entering `WaitingUser`
    /// twice in a row must not leave two watchers racing to write the session's status.
    #[test]
    fn a_newer_watch_supersedes_the_one_before_it() {
        let (state, _dir) = state_with_console("supersede-generations");
        let first = state.begin_record_watch("s");
        let second = state.begin_record_watch("s");
        assert_ne!(first, second);
        assert!(!state.record_watch_current("s", first));
        assert!(state.record_watch_current("s", second));
    }

    /// A watch's write is made only while the watch is current: once the next prompt has retired
    /// it, the write does not run at all.
    #[test]
    fn a_retired_watch_makes_no_write() {
        let (state, _dir) = state_with_console("retired-write");
        let generation = state.begin_record_watch("s");
        state.end_record_watch("s");
        assert_eq!(
            state.while_record_watch_current("s", generation, || ()),
            None
        );
    }

    fn status_of(state: &AppState, id: &str) -> Option<SessionStatus> {
        state.store.get_session(id).ok().flatten().map(|s| s.status)
    }

    /// Asserts that a watch was actually armed for `id`, as generation `0` — the first generation
    /// a watch can hand out on a freshly built `AppState`, which is what each test below builds
    /// through `Fixture::new`.
    fn assert_watch_armed(state: &AppState, id: &str) {
        assert!(state.record_watch_current(id, 0));
    }

    /// A stand-in session is registered and watched the same way in every test below; this bundles
    /// the common setup so each test only has to state what is different about it.
    struct Fixture {
        /// Kept so the stand-in registered for the session lives as long as the test.
        _live: StandIn,
        state: Arc<AppState>,
        _dir: ScratchDir,
        path: PathBuf,
    }

    impl Fixture {
        fn new(name: &str, session: Session) -> Self {
            let (state, dir) = state_with_console(name);
            state.store.insert_session(&session).expect("session");
            let live = idle_stand_in(&session.id);
            state.register_live(live.clone());

            let path = dir.join("transcript.jsonl");
            std::fs::write(&path, "").expect("empty transcript");
            Self {
                _live: live,
                state,
                _dir: dir,
                path,
            }
        }
    }

    /// The watcher must apply `Idle` only while the session is still the one it was started for:
    /// once a real hook event has moved the session on (here, straight to `Working`, standing in
    /// for a later prompt), a rejection record arriving afterwards must not overwrite it.
    ///
    /// This only exercises the pre-check at the top of each poll (the agent's status being
    /// `WaitingUser`), which returns before the scan ever runs — the session is moved on before
    /// the rejection is even written, so the watcher never reaches the scan or the write to find out
    /// about it. Two further guards sit right before the write, for a session that moved on after a
    /// scan did find a rejection: the generation check held across the write, and the conditional
    /// write that moves the session only while it is still waiting. Neither race is reachable
    /// deterministically from outside the watch loop — the generation check is covered by
    /// `a_retired_watch_makes_no_write`, the conditional write by the store's own test.
    #[tokio::test(start_paused = true)]
    async fn a_session_that_moved_on_is_not_scanned_at_all() {
        let mut session = waiting_session("s");
        let fixture = Fixture::new("moved-on", session.clone());
        watch_for_rejection(&fixture.state, "s", Some(&fixture.path.to_string_lossy()));
        assert_watch_armed(&fixture.state, "s");

        // A real hook event takes the session back to work before the rejection ever shows up.
        session.status = SessionStatus::Working;
        fixture
            .state
            .store
            .update_session(&session)
            .expect("update");

        std::fs::write(&fixture.path, REJECTION).expect("write rejection");

        // Nothing is expected to happen, so there is no event to wait for: the watch is given
        // its polls, and what they did is read after them.
        polls_ran().await;
        assert_eq!(
            status_of(&fixture.state, "s"),
            Some(SessionStatus::Working),
            "a session that moved on must not be scanned"
        );
    }

    /// A rejection that arrives while the session is still genuinely waiting is applied — the
    /// positive case the test above is the negative of.
    #[tokio::test(start_paused = true)]
    async fn a_rejection_found_while_still_waiting_is_applied() {
        let fixture = Fixture::new("real-write", waiting_session("s"));
        watch_for_rejection(&fixture.state, "s", Some(&fixture.path.to_string_lossy()));

        std::fs::write(&fixture.path, REJECTION).expect("write rejection");

        assert!(
            reaches(&fixture.state, SessionStatus::Idle).await,
            "the rejection must be applied"
        );
        assert!(
            fixture.state.last_turn_abnormal("s"),
            "a declined prompt leaves it paused"
        );
    }

    /// A watch follows the agent's own status, not the record's: with a session request
    /// holding the hand up, a decline found in the transcript moves what the agent is remembered
    /// at, and the record keeps showing the request's `WaitingUser`.
    #[tokio::test(start_paused = true)]
    async fn a_rejection_during_a_held_hand_moves_the_agent_and_not_the_record() {
        let fixture = Fixture::new("held-hand", waiting_session("s"));
        fixture.state.raise_request_hand("s").expect("raise");
        watch_for_rejection(&fixture.state, "s", Some(&fixture.path.to_string_lossy()));

        std::fs::write(&fixture.path, REJECTION).expect("write rejection");

        polls_ran().await;
        let record = fixture.state.session_record("s").expect("record");
        assert_eq!(
            fixture.state.agent_status(&record),
            SessionStatus::Idle,
            "the rejection must move the agent's own status"
        );
        assert_eq!(
            status_of(&fixture.state, "s"),
            Some(SessionStatus::WaitingUser)
        );
    }

    /// A conclusion about an agent that has since moved on is dropped, rather than falling through
    /// to the record: the record is held at `WaitingUser` by the request, so a write conditional on
    /// that would succeed and take the request's own hand down.
    #[test]
    fn a_rejection_is_dropped_once_the_agent_has_moved_on() {
        let fixture = Fixture::new("moved-on", waiting_session("s"));
        fixture.state.raise_request_hand("s").expect("raise");
        fixture
            .state
            .apply_hook_status("s", SessionStatus::Working)
            .expect("the agent reports a turn");

        let reported = fixture
            .state
            .report_paused_if("s", SessionStatus::WaitingUser)
            .expect("report");

        assert!(!reported, "the agent is no longer where the scan read it");
        let record = fixture.state.session_record("s").expect("record");
        assert_eq!(fixture.state.agent_status(&record), SessionStatus::Working);
        assert_eq!(
            status_of(&fixture.state, "s"),
            Some(SessionStatus::WaitingUser)
        );
    }

    /// A watch superseded by a newer one for the same session must not write either — the second
    /// guarantee `state.begin_record_watch` exists for, alongside retiring the stale watcher's
    /// polling loop.
    #[tokio::test(start_paused = true)]
    async fn a_superseded_watch_does_not_write() {
        let fixture = Fixture::new("superseded", waiting_session("s"));
        watch_for_rejection(&fixture.state, "s", Some(&fixture.path.to_string_lossy()));
        assert_watch_armed(&fixture.state, "s");
        // Superseding directly, rather than through a second `watch_for_rejection`, isolates the
        // generation check from the rest of the watch loop.
        fixture.state.begin_record_watch("s");

        std::fs::write(&fixture.path, REJECTION).expect("write rejection");

        polls_ran().await;
        assert_eq!(
            status_of(&fixture.state, "s"),
            Some(SessionStatus::WaitingUser),
            "a superseded watch must not write"
        );
    }

    /// A logged request replaying an earlier decline — both of its records, in the request's own
    /// `message.content` — is not a decline, and must not take the hand down.
    #[tokio::test(start_paused = true)]
    async fn a_decline_replayed_in_a_logged_request_does_not_lower_the_hand() {
        let fixture = Fixture::new("replayed-decline", waiting_session("s"));
        watch_for_rejection(&fixture.state, "s", Some(&fixture.path.to_string_lossy()));

        let blob = format!(
            "{{\"type\":\"api-request-blob\",\"message\":{{\"role\":\"user\",\"content\":[\
             {{\"type\":\"tool_result\",\"content\":\"{REJECTED_TOOL_USE_PREFIX}.\",\
             \"is_error\":true}},{{\"type\":\"text\",\"text\":\"{INTERRUPTED_MARKER}\"}}]}}}}\n"
        );
        std::fs::write(&fixture.path, blob).expect("write the logged request");

        assert!(
            !reaches(&fixture.state, SessionStatus::Idle).await,
            "a replayed decline must not lower the hand"
        );
    }

    /// A transcript that shrinks mid-watch — the offset left pointing past the new end-of-file —
    /// must recover rather than stall: a rejection appended after the shrink is still found.
    #[tokio::test(start_paused = true)]
    async fn a_shrunk_transcript_recovers_and_a_later_append_is_still_seen() {
        let fixture = Fixture::new("shrink-recovers", waiting_session("s"));
        watch_for_rejection(&fixture.state, "s", Some(&fixture.path.to_string_lossy()));

        // Grow the transcript past the watch's starting offset, then shrink it back to empty —
        // standing in for a rotated or truncated file — before the rejection is appended. The
        // filler is made longer than `REJECTION` itself, so that without the reset branch the
        // stale offset would still sit past the end of the file the rejection is written into —
        // `len < offset` stays true forever and the scan never runs again — rather than landing
        // inside `REJECTION`'s own bytes, where it could accidentally still find the marker.
        let filler_line = "{\"type\":\"summary\"}\n";
        let filler = filler_line.repeat(1 + REJECTION.len() / filler_line.len());
        std::fs::write(&fixture.path, &filler).expect("write filler");
        polls_ran().await;
        std::fs::write(&fixture.path, "").expect("shrink transcript");
        polls_ran().await;
        std::fs::write(&fixture.path, REJECTION).expect("write rejection");

        assert!(
            reaches(&fixture.state, SessionStatus::Idle).await,
            "a rejection appended after a shrink must still be found"
        );
    }

    /// A poll that finds the transcript exactly as long as it was last scanned to — nothing
    /// appended since — leaves the session's status unchanged, and growth after it is still
    /// detected normally.
    #[tokio::test(start_paused = true)]
    async fn an_unchanged_transcript_length_leaves_status_unchanged_and_later_growth_is_still_seen()
    {
        let fixture = Fixture::new("equal-length", waiting_session("s"));
        watch_for_rejection(&fixture.state, "s", Some(&fixture.path.to_string_lossy()));

        std::fs::write(&fixture.path, "{\"type\":\"summary\"}\n").expect("write filler");
        polls_ran().await;
        // Rewriting the same content leaves the length — and so the offset check — unchanged
        // across a poll.
        std::fs::write(&fixture.path, "{\"type\":\"summary\"}\n").expect("rewrite filler");
        polls_ran().await;
        assert_eq!(
            status_of(&fixture.state, "s"),
            Some(SessionStatus::WaitingUser)
        );

        std::fs::write(
            &fixture.path,
            format!("{{\"type\":\"summary\"}}\n{REJECTION}"),
        )
        .expect("append rejection");
        assert!(
            reaches(&fixture.state, SessionStatus::Idle).await,
            "growth after an unchanged poll must still be detected"
        );
    }

    /// Whether the session `s` is at `status` once the watch has had its polls.
    async fn reaches(state: &Arc<AppState>, status: SessionStatus) -> bool {
        polls_ran().await;
        status_of(state, "s") == Some(status)
    }

    /// An interrupt written while the session is working puts it back at its prompt.
    #[tokio::test(start_paused = true)]
    async fn an_interrupt_found_while_working_is_applied() {
        let fixture = Fixture::new("interrupt", working_session("s"));
        watch_for_interrupt(
            &fixture.state,
            "s",
            Some(&fixture.path.to_string_lossy()),
            Some(CAPTURED_TURN),
        );

        std::fs::write(&fixture.path, fresh_interrupt()).expect("write interrupt");

        assert!(
            reaches(&fixture.state, SessionStatus::Idle).await,
            "the interrupt must be applied"
        );
        assert!(
            fixture.state.last_turn_abnormal("s"),
            "an interrupt leaves it paused"
        );
    }

    /// An interrupt already in the transcript when the watch is armed belongs to an earlier turn,
    /// or to a resumed conversation's history, and must not end the turn now working.
    #[tokio::test(start_paused = true)]
    async fn an_interrupt_written_before_the_watch_is_not_applied() {
        let fixture = Fixture::new("old-interrupt", working_session("s"));
        // Stamped as if fresh, so only where it sits in the file rules it out.
        let interrupt = fresh_interrupt();
        std::fs::write(&fixture.path, &interrupt).expect("write the earlier interrupt");
        watch_for_interrupt(
            &fixture.state,
            "s",
            Some(&fixture.path.to_string_lossy()),
            Some(CAPTURED_TURN),
        );

        std::fs::write(
            &fixture.path,
            format!("{interrupt}{{\"type\":\"summary\"}}\n"),
        )
        .expect("append a line");

        assert!(
            !reaches(&fixture.state, SessionStatus::Idle).await,
            "an earlier interrupt must not be applied"
        );
    }

    /// Esc pressed while a message waits in Claude Code's queue cancels the turn and runs the
    /// message as the next one, with no hook: the watch armed when the message was queued, for
    /// the turn it was queued in, reads that turn's interrupt and then the agent carrying on. The
    /// session keeps working, the cancel counts as a turn ending, and the watch goes on.
    #[tokio::test(start_paused = true)]
    async fn a_cancel_the_agent_carries_on_from_with_a_queued_message_keeps_it_working() {
        let fixture = Fixture::new("queued-prompt", working_session("s"));
        watch_for_interrupt(
            &fixture.state,
            "s",
            Some(&fixture.path.to_string_lossy()),
            Some(CAPTURED_TURN),
        );
        assert_watch_armed(&fixture.state, "s");

        std::fs::write(&fixture.path, queued_prompt_after_interrupt(QUEUED_TURN))
            .expect("write the cancel and the queued prompt");

        assert!(
            reaches(&fixture.state, SessionStatus::Working).await,
            "the turn carried on to must keep the session working"
        );
        assert_eq!(fixture.state.turn_ends("s"), 1, "the cancel ends a turn");
        assert!(!fixture.state.last_turn_abnormal("s"));
        assert_watch_armed(&fixture.state, "s");
    }

    /// The turn the agent carried on to is still watched: its own interrupt ends it, whether read
    /// in a later scan or in the same one.
    #[tokio::test(start_paused = true)]
    async fn the_turn_carried_on_to_is_ended_by_its_own_interrupt() {
        for in_one_write in [false, true] {
            let fixture = Fixture::new("queued-own-interrupt", working_session("s"));
            watch_for_interrupt(
                &fixture.state,
                "s",
                Some(&fixture.path.to_string_lossy()),
                Some(CAPTURED_TURN),
            );
            let cancel = queued_prompt_after_interrupt(QUEUED_TURN);
            if !in_one_write {
                std::fs::write(&fixture.path, &cancel).expect("write the cancel");
                assert!(reaches(&fixture.state, SessionStatus::Working).await);
            }

            std::fs::write(&fixture.path, cancel + &fresh_interrupt_of(QUEUED_TURN))
                .expect("write the queued turn's interrupt");

            assert!(
                reaches(&fixture.state, SessionStatus::Idle).await,
                "the queued turn's own interrupt must be applied (in one write: {in_one_write})"
            );
            assert!(fixture.state.last_turn_abnormal("s"));
        }
    }

    /// A new conversation's transcript does not exist yet when its first prompt arms the watch;
    /// once it appears it is read from its start.
    #[tokio::test(start_paused = true)]
    async fn a_transcript_that_appears_after_the_watch_is_read_from_its_start() {
        let fixture = Fixture::new("new-transcript", working_session("s"));
        std::fs::remove_file(&fixture.path).expect("no transcript yet");
        watch_for_interrupt(
            &fixture.state,
            "s",
            Some(&fixture.path.to_string_lossy()),
            Some(CAPTURED_TURN),
        );
        assert_watch_armed(&fixture.state, "s");

        std::fs::write(&fixture.path, fresh_interrupt()).expect("write interrupt");

        assert!(
            reaches(&fixture.state, SessionStatus::Idle).await,
            "the new transcript's interrupt must be applied"
        );
    }

    /// A transcript that appears after the watch is armed can still open with older lines, a
    /// conversation's copied history for one; an interrupt stamped before the watch is not this
    /// turn's.
    #[tokio::test(start_paused = true)]
    async fn an_older_interrupt_in_a_transcript_that_appears_later_is_not_applied() {
        let fixture = Fixture::new("copied-history", working_session("s"));
        std::fs::remove_file(&fixture.path).expect("no transcript yet");
        watch_for_interrupt(
            &fixture.state,
            "s",
            Some(&fixture.path.to_string_lossy()),
            Some(CAPTURED_TURN),
        );
        assert_watch_armed(&fixture.state, "s");

        std::fs::write(&fixture.path, captured_interrupt()).expect("write copied history");

        assert!(
            !reaches(&fixture.state, SessionStatus::Idle).await,
            "an interrupt from before the watch must not be applied"
        );
    }
}
