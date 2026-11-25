//! How GitHub Copilot CLI's hook events are read. The measurements are in
//! `docs/agent-cli-reference.md`, "GitHub Copilot CLI".
//!
//! Its native payloads name no event, so each hook entry passes its event's name beside the
//! payload; `notification` alone names itself, as `Notification`, and that name wins.
//!
//! Three things its hooks do not say, each read elsewhere:
//!
//! - **A turn the user cancels and a prompt the user declines run no hook.** The record gets an
//!   `abort` line, which the record watch reads (`super::record_watch`), as it does the line of a
//!   permission the user refused with what to do differently, after which the turn carries on.
//! - **A turn that ends in an error runs no `agentStop`.** `errorOccurred` runs for every failed
//!   model request, the retried ones too, always as recoverable, so it does not say the turn is
//!   over and is not read at all; the record's `session.error` is, by the same watch.
//! - **`agentStop` carries no final message.** The record's last assistant message is the
//!   turn's.
//!
//! **Background work** has no event of its own either. A shell command started in the background
//! and a background agent end their starting call at once, and the session's turn can end with
//! them running; each completion then comes back as a prompt of Copilot's own wrapped in
//! `<system_notification>`. So the work is read from the record (`super::record_lines::Work`), and a shell's
//! completion notice takes it off the list.
//!
//! **The record is read once at each `agentStop` of the session's**, from where the read before
//! it ended ([`Kept`], kept with the session's process): the lines since give the turn's message
//! and the work they started and ended, so a command started long ago stays listed however much
//! has been written since. The record is the session's own under its account's directory, as the
//! daemon resolved it, never the path the payload names. Copilot writes its record a moment after
//! the events it holds, so the read waits, half a second at most, until the turn's own
//! `agentStop` line is in.
//!
//! **A `permission_prompt` notification can arrive late**: Copilot does not wait for the hook, so
//! one can land after the prompt was answered and the turn has ended. The record says so, and
//! such a notification raises no hand.
//!
//! `permissionRequest` is not registered: it runs for a call an allow rule lets through with no
//! prompt as well, so it does not say the user is asked. The `permission_prompt` notification
//! does.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use super::record_lines::{
    asked_in, record_tail, record_under, shell_task_id, turn_line, Asked, TurnLine, Work,
    PROMPT_WINDOW, RECORD_SETTLE,
};
use crate::hooks::{
    BackgroundTasksChange, CallShape, HookInterpreter, HookSession, RecordWatch, ToolCallChange,
    TurnEnd, TurnOutcome,
};
use crate::protocol::{AgentId, BackgroundTask, SessionStatus};

pub(super) struct CopilotHooks;

/// The events Octoboard registers, by the names Copilot's hook files key them with.
pub(super) const SESSION_START: &str = "sessionStart";
pub(super) const PROMPT_SUBMITTED: &str = "userPromptSubmitted";
pub(super) const PRE_TOOL_USE: &str = "preToolUse";
pub(super) const POST_TOOL_USE: &str = "postToolUse";
pub(super) const POST_TOOL_USE_FAILURE: &str = "postToolUseFailure";
pub(super) use super::record_lines::AGENT_STOP;
pub(super) const SUBAGENT_START: &str = "subagentStart";
pub(super) const SUBAGENT_STOP: &str = "subagentStop";
pub(super) const NOTIFICATION: &str = "notification";

/// The name a `notification` payload gives itself, which is read in place of the entry's.
const NOTIFICATION_NAMED: &str = "Notification";

/// How far back from a record's end the first read of a session's process starts, and the most
/// of it that is held at once after that. Only that first read can miss work: what started before
/// it and is not listed already. Every later read goes on from where the one before it ended.
const RECORD_WINDOW: u64 = 4 * 1024 * 1024;

/// How long a read at a turn's end waits between two looks at a record that does not hold the
/// turn's `agentStop` yet, [`RECORD_SETTLE`] at most in all.
const SETTLE_STEP: Duration = Duration::from_millis(100);

impl HookInterpreter for CopilotHooks {
    /// No hook runs while the folder-trust prompt is up: the first comes with the first prompt.
    fn past_startup(&self, _event: &str, _payload: &Value) -> bool {
        true
    }

    fn starts_conversation(&self, event: &str, _payload: &Value) -> bool {
        event == PROMPT_SUBMITTED
    }

    /// The text as it was sent, a bracketed paste included.
    fn submitted_prompt<'a>(&self, payload: &'a Value) -> Option<&'a str> {
        payload.get("prompt")?.as_str()
    }

    /// A detached shell command's completion, which Copilot reports to the session as a prompt
    /// of its own wrapped in `<system_notification>`.
    fn is_completion_notice(&self, prompt: &str) -> bool {
        prompt.trim_start().starts_with("<system_notification>")
    }

    /// A subagent's own events and its start and stop carry `agentId`; the session's carry none.
    fn is_subagent(&self, payload: &Value) -> bool {
        subagent_id(payload).is_some()
    }

    fn agent_session_id(&self, payload: &Value) -> Option<String> {
        crate::hooks::agent_session_id(payload)
    }

    /// The files a file-writing call names. `create` and `edit`, which every model family but
    /// GPT is given, take an object whose `path` is the file; `apply_patch`, which the GPT models
    /// are given in their place, takes the patch itself as a string, each file named in a header
    /// line of it.
    fn edit_targets(&self, payload: &Value) -> (Vec<String>, Option<std::path::PathBuf>) {
        let cwd = payload
            .get("cwd")
            .and_then(Value::as_str)
            .map(std::path::PathBuf::from);
        let paths = match payload.get("toolArgs") {
            Some(Value::String(patch)) => crate::adapter::patch::patch_targets(patch),
            Some(args) => args
                .get("path")
                .and_then(Value::as_str)
                .filter(|path| !path.is_empty())
                .map(str::to_owned)
                .into_iter()
                .collect(),
            None => Vec::new(),
        };
        (paths, cwd)
    }

    fn deny_edit(&self, reason: &str) -> Option<Value> {
        Some(serde_json::json!({
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }))
    }

    /// `sessionStart` comes with the first prompt, after the folder-trust prompt, and is the first
    /// hook a session runs.
    fn announces_session_start(&self, event: &str, _payload: &Value) -> bool {
        event == SESSION_START
    }

    fn status(&self, event: &str, payload: &Value) -> Option<SessionStatus> {
        match event {
            PROMPT_SUBMITTED | POST_TOOL_USE | POST_TOOL_USE_FAILURE => {
                Some(SessionStatus::Working)
            }
            PRE_TOOL_USE if asks_the_user(payload) => Some(SessionStatus::WaitingUser),
            PRE_TOOL_USE => Some(SessionStatus::Working),
            AGENT_STOP => Some(SessionStatus::Idle),
            // Matched on the type, never on `message` or `title`, which are display text.
            NOTIFICATION | NOTIFICATION_NAMED => match notification_type(payload) {
                Some(PERMISSION_PROMPT) | Some("elicitation_dialog") => {
                    Some(SessionStatus::WaitingUser)
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// A `permission_prompt` notification whose prompt the record says was answered, with the
    /// turn over since, raises no hand: it arrived late.
    fn status_in(
        &self,
        session: &HookSession<'_>,
        event: &str,
        payload: &Value,
    ) -> Option<SessionStatus> {
        let status = self.status(event, payload)?;
        if matches!(event, NOTIFICATION | NOTIFICATION_NAMED)
            && notification_type(payload) == Some(PERMISSION_PROMPT)
            && answered_with_the_turn_over(session, payload, RECORD_SETTLE)
        {
            tracing::debug!(
                session = %session.id,
                "a prompt's notification came after its answer and its turn's end; it raises no hand"
            );
            return None;
        }
        Some(status)
    }

    fn turn_outcome(&self, event: &str, _payload: &Value) -> Option<TurnOutcome> {
        (event == AGENT_STOP).then_some(TurnOutcome::Normal)
    }

    /// The payload alone gives no message: [`Self::turn_end_in`] reads it.
    fn turn_end(&self, event: &str, _payload: &Value) -> Option<TurnEnd> {
        (event == AGENT_STOP).then_some(TurnEnd {
            last_assistant_message: None,
            failed: false,
            backstop: false,
        })
    }

    /// The turn's message is the last the session's record holds, read on from where the read
    /// before ended; the same read finds the background work the turn started and ended, which
    /// [`Self::background_tasks_changes_in`] answers with.
    fn turn_end_in(
        &self,
        session: &HookSession<'_>,
        event: &str,
        payload: &Value,
    ) -> Option<TurnEnd> {
        let mut end = self.turn_end(event, payload)?;
        end.last_assistant_message = read_turn_end(session, payload, RECORD_SETTLE);
        Some(end)
    }

    /// A subagent runs from its `subagentStart` to its `subagentStop`, both of which name it. A
    /// background agent that has gone idle and is written to again works with no start of its
    /// own, so a subagent's own prompt, which names it too, says the same as a start; its
    /// `subagentStop` runs again when it is done. A shell command's completion notice takes it
    /// off the list; a background agent's names no id, and its `subagentStop` has taken it off by
    /// then. What the record says started and ended is not the payload's to give
    /// ([`Self::background_tasks_changes_in`]).
    fn background_tasks_change(
        &self,
        event: &str,
        payload: &Value,
    ) -> Option<BackgroundTasksChange> {
        match (event, subagent_id(payload)) {
            (SUBAGENT_START | PROMPT_SUBMITTED, Some(id)) => {
                Some(BackgroundTasksChange::SubagentStarted(id.into()))
            }
            (SUBAGENT_STOP, Some(id)) => Some(BackgroundTasksChange::Finished(vec![id.into()])),
            (PROMPT_SUBMITTED, None) => {
                let ids = completed_shells(self.submitted_prompt(payload)?);
                (!ids.is_empty()).then_some(BackgroundTasksChange::Finished(ids))
            }
            _ => None,
        }
    }

    /// What the event itself says, then what the read at a turn's end found in the record
    /// ([`Self::turn_end_in`], asked before this for the same event): the shell commands and the
    /// background agents its new lines started, and the ones they ended, both from the one read.
    fn background_tasks_changes_in(
        &self,
        session: &HookSession<'_>,
        event: &str,
        payload: &Value,
    ) -> Vec<BackgroundTasksChange> {
        let mut changes: Vec<BackgroundTasksChange> = self
            .background_tasks_change(event, payload)
            .into_iter()
            .collect();
        changes.extend(session.carry(|kept: &mut Kept| std::mem::take(&mut kept.found)));
        changes
    }

    /// `preToolUse` and both post events name their call by `toolCallId`. Only the question's own
    /// call waits on the user from its start. A call a hook denies, or whose permission prompt the
    /// user declines, runs no post event; the turn's end and the next prompt leave none open.
    fn tool_call_change<'a>(&self, event: &str, payload: &'a Value) -> Option<ToolCallChange<'a>> {
        let id = || payload.get("toolCallId").and_then(Value::as_str);
        match event {
            PRE_TOOL_USE => Some(ToolCallChange::Started {
                id: id(),
                // No input: a permission prompt's notification names no call to match it to.
                call: CallShape {
                    tool: tool_name(payload),
                    input: None,
                },
                waits_on_user: asks_the_user(payload),
            }),
            POST_TOOL_USE | POST_TOOL_USE_FAILURE => Some(ToolCallChange::Ended { id: id() }),
            PROMPT_SUBMITTED | AGENT_STOP | SESSION_START => Some(ToolCallChange::AllEnded),
            _ => None,
        }
    }

    fn watch_reads_previous_status(&self) -> bool {
        true
    }

    /// The record is watched from the prompt, and again each time the hand comes down, for the
    /// cancel and the error no hook reports; while the hand is up, for the decline none reports.
    fn record_watch<'a>(
        &self,
        event: &str,
        payload: &'a Value,
        previous: Option<SessionStatus>,
        status: SessionStatus,
    ) -> Option<RecordWatch<'a>> {
        match status {
            SessionStatus::WaitingUser => Some(RecordWatch::Decline { record: None }),
            // A status that could not be read is taken as a move into working: a watch too many
            // costs a poll, one too few leaves a cancelled turn reading as working.
            SessionStatus::Working
                if self.starts_conversation(event, payload)
                    || previous != Some(SessionStatus::Working) =>
            {
                Some(RecordWatch::Interrupt {
                    record: None,
                    turn_id: None,
                })
            }
            _ => None,
        }
    }
}

fn tool_name(payload: &Value) -> Option<&str> {
    payload.get("toolName").and_then(Value::as_str)
}

fn asks_the_user(payload: &Value) -> bool {
    AgentId::COPILOT
        .descriptor()
        .is_ask_question_tool(tool_name(payload))
}

fn subagent_id(payload: &Value) -> Option<&str> {
    payload
        .get("agentId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
}

fn notification_type(payload: &Value) -> Option<&str> {
    payload.get("notification_type").and_then(Value::as_str)
}

/// The type of the notification a permission prompt raises.
const PERMISSION_PROMPT: &str = "permission_prompt";

/// Whether the record of the session a `permission_prompt` notification names says every prompt
/// of its turn was answered and the turn is over: the notification then arrived late. Copilot
/// writes its record a moment after the events it holds, so a record that says so is read once
/// more after `settle`: a prompt put up since (a background agent's, after the session's turn)
/// has its request in the file by then, and raises the hand. A record that says anything else,
/// or cannot be read, holds nothing back, and costs no wait.
fn answered_with_the_turn_over(
    session: &HookSession<'_>,
    payload: &Value,
    settle: Duration,
) -> bool {
    let Some(record) = payload
        .get("sessionId")
        .and_then(Value::as_str)
        .and_then(|id| record_under(session.config_dir, id))
    else {
        return false;
    };
    let over = || {
        record_tail(&record, PROMPT_WINDOW).map(|tail| asked_in(&tail))
            == Some(Asked::Answered { turn_over: true })
    };
    if !over() {
        return false;
    }
    between_reads(settle);
    over()
}

/// Waits `pause` between two reads of a record that Copilot may still be writing out, on the
/// thread that reads it (the session's hook queue). A test sets what happens there instead
/// (`tests::between_reads_do`): the record gains its line, with no time spent.
fn between_reads(pause: Duration) {
    #[cfg(test)]
    if tests::between_reads_hook() {
        return;
    }
    std::thread::sleep(pause);
}

/// What the interpreter keeps for one session ([`HookSession::carry`]): how far it has read into
/// the session's record, and what the lines read so far say. It goes with the session's process,
/// and the first read of the next starts [`RECORD_WINDOW`] before the record's end.
#[derive(Default)]
struct Kept {
    /// The record being read and the offset its next read starts at; `None` before the first.
    read: Option<(PathBuf, u64)>,
    /// Whether the first read started at the record's first byte, so that the work below is all
    /// the record says is running.
    whole: bool,
    /// The background work the lines read so far leave running.
    work: Work,
    /// The text of the last assistant message of the session's own since its last prompt.
    said: Option<String>,
    /// What the read at a turn's end found of the background work and no event has been
    /// answered with yet, in the order it is to be applied.
    found: Vec<BackgroundTasksChange>,
}

impl Kept {
    /// Reads the record on from where the last read ended, and says whether the lines read hold
    /// the session's `agentStop` with no prompt after it: the turn's last lines are then in.
    /// `None` when the record cannot be read. Only whole lines are taken, so one still being
    /// written is read next time. Another record than the one read before, or one shorter than
    /// what was read of it, is read from the start of its window with nothing kept.
    fn read_on(&mut self, record: &Path) -> Option<bool> {
        let mut file = crate::hostfs::open_regular(record).ok()?;
        let len = file.metadata().ok()?.len();
        let kept = self
            .read
            .as_ref()
            .filter(|(read, offset)| read.as_path() == record && *offset <= len)
            .map(|(_, offset)| *offset);
        let mut offset = match kept {
            Some(offset) => offset,
            None => {
                let from = len.saturating_sub(RECORD_WINDOW);
                *self = Self {
                    whole: from == 0,
                    ..Self::default()
                };
                from
            }
        };
        let mut turn_over = false;
        loop {
            file.seek(SeekFrom::Start(offset)).ok()?;
            let mut chunk = Vec::new();
            file.by_ref()
                .take(RECORD_WINDOW)
                .read_to_end(&mut chunk)
                .ok()?;
            let full = chunk.len() as u64 == RECORD_WINDOW;
            let whole = match chunk.iter().rposition(|byte| *byte == b'\n') {
                Some(end) => end + 1,
                // A line longer than one read is passed over, as one that does not parse is.
                None if full => chunk.len(),
                None => 0,
            };
            for line in String::from_utf8_lossy(&chunk[..whole]).lines() {
                self.work.read(line);
                match turn_line(line) {
                    Some(TurnLine::Prompt) => {
                        self.said = None;
                        turn_over = false;
                    }
                    Some(TurnLine::Notice) => turn_over = false,
                    Some(TurnLine::Message(text)) => self.said = Some(text),
                    Some(TurnLine::Stopped) => turn_over = true,
                    None => {}
                }
            }
            offset += whole as u64;
            self.read = Some((record.to_path_buf(), offset));
            if !full {
                break;
            }
        }
        Some(turn_over)
    }

    /// What the lines read since `before`, the ids then running, did to the background work.
    /// The work that started and the work that ended are two changes, the start first; where the
    /// record was read from its first byte, or a line ended everything (the session's shutdown,
    /// a resume), the list the record leaves is the whole truth and replaces what is listed.
    fn changes(&mut self, before: &[String]) -> Vec<BackgroundTasksChange> {
        let cleared = std::mem::take(&mut self.work.cleared);
        let ended = std::mem::take(&mut self.work.ended);
        let running = &self.work.running;
        let started: Vec<BackgroundTask> = running
            .iter()
            .filter(|task| !before.contains(&task.id))
            .cloned()
            .collect();
        let mut finished: Vec<String> = Vec::new();
        for id in ended {
            if !running.iter().any(|task| task.id == id) && !finished.contains(&id) {
                finished.push(id);
            }
        }
        if cleared || (self.whole && !started.is_empty() && !finished.is_empty()) {
            return vec![BackgroundTasksChange::Replace(running.clone())];
        }
        let mut found = Vec::new();
        if !started.is_empty() {
            found.push(BackgroundTasksChange::Started(started));
        }
        if !finished.is_empty() {
            found.push(BackgroundTasksChange::Finished(finished));
        }
        found
    }
}

/// Reads the record of the session an `agentStop` ends a turn of, on from where the read before
/// ended, and gives the turn's message; what the lines did to the background work is left for
/// [`CopilotHooks::background_tasks_changes_in`]. The record is the one the daemon places for the
/// payload's session id under the account's directory the launch runs on
/// ([`record_under`]), never the path the payload names, and is read only as a regular file
/// opened without blocking. While the lines read do not hold the turn's `agentStop`, which
/// Copilot writes out a moment after it runs the hook, the read is made again every
/// [`SETTLE_STEP`], for `settle` at most: the turn's last message and the work its last call
/// started are written with that line.
fn read_turn_end(session: &HookSession<'_>, payload: &Value, settle: Duration) -> Option<String> {
    let record = record_under(session.config_dir, payload.get("sessionId")?.as_str()?)?;
    session.carry(|kept: &mut Kept| {
        let before: Vec<String> = kept
            .work
            .running
            .iter()
            .map(|task| task.id.clone())
            .collect();
        let mut waited = Duration::ZERO;
        loop {
            match kept.read_on(&record)? {
                false if waited < settle => {
                    between_reads(SETTLE_STEP);
                    waited += SETTLE_STEP;
                }
                _ => break,
            }
        }
        let found = kept.changes(&before);
        kept.found.extend(found);
        kept.said.clone()
    })
}

/// The shell commands a completion notice says have ended, by the ids they are listed under:
/// `Shell command "<description>" (shellId: N) has completed successfully.` and `Detached shell
/// "<description>" (shellId: N) has completed.`, each wrapped in `<system_notification>`. A
/// description is the model's own text, so the id is the last `(shellId: N)` of its line. A
/// notice about an agent names no id and gives none.
fn completed_shells(prompt: &str) -> Vec<String> {
    const OPENS: &str = "(shellId: ";
    if !prompt.trim_start().starts_with("<system_notification>") {
        return Vec::new();
    }
    prompt
        .lines()
        .filter(|line| line.contains("has completed"))
        .filter_map(|line| {
            let (_, rest) = line.rsplit_once(OPENS)?;
            let (shell, _) = rest.split_once(')')?;
            (!shell.is_empty() && shell.chars().all(|c| c.is_ascii_alphanumeric()))
                .then(|| shell_task_id(shell))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::hooks::event_name;
    use crate::test_support::ScratchDir;

    thread_local! {
        /// What a test runs on this thread in place of the wait between two reads of a record.
        static BETWEEN_READS: RefCell<Option<Box<dyn FnMut()>>> = RefCell::new(None);
    }

    /// Runs `step` on this thread in place of each wait between two reads of a record, until
    /// the guard returned is dropped.
    pub(super) fn between_reads_do(step: impl FnMut() + 'static) -> BetweenReads {
        BETWEEN_READS.with(|hook| *hook.borrow_mut() = Some(Box::new(step)));
        BetweenReads
    }

    /// Takes the step [`between_reads_do`] set back when dropped.
    pub(super) struct BetweenReads;

    impl Drop for BetweenReads {
        fn drop(&mut self) {
            BETWEEN_READS.with(|hook| *hook.borrow_mut() = None);
        }
    }

    /// Runs the step a test set for this thread, if it set one, and says whether it did.
    pub(super) fn between_reads_hook() -> bool {
        let Some(mut step) = BETWEEN_READS.with(|hook| hook.borrow_mut().take()) else {
            return false;
        };
        step();
        BETWEEN_READS.with(|hook| {
            hook.borrow_mut().get_or_insert(step);
        });
        true
    }

    /// Real payloads of Copilot 1.0.114, one object a line: the scene (a session), the event the
    /// hook entry was registered under, and the payload (`testdata/copilot-captures.md`).
    const PAYLOADS: &str = include_str!("../../../testdata/copilot_hook_payloads.jsonl");
    /// Lines of real `events.jsonl` records, several sessions one after another.
    const RECORD: &str = include_str!("../../../testdata/copilot_events.jsonl");

    /// One hook run: its scene, the event as the daemon reads it (the name the payload gives
    /// itself, else the name the entry passed) and its payload.
    struct Run {
        scene: String,
        event: String,
        payload: Value,
    }

    fn runs() -> Vec<Run> {
        PAYLOADS
            .lines()
            .map(|line| {
                let line: Value = serde_json::from_str(line).expect("a payload line");
                let payload = line["payload"].clone();
                let passed = line["event"].as_str().expect("the entry's event");
                Run {
                    scene: line["scene"].as_str().expect("a scene").to_string(),
                    event: event_name(&payload).unwrap_or_else(|| passed.to_string()),
                    payload,
                }
            })
            .collect()
    }

    fn scene(name: &str) -> Vec<Run> {
        let found: Vec<Run> = runs().into_iter().filter(|run| run.scene == name).collect();
        assert!(!found.is_empty(), "no scene {name}");
        found
    }

    /// A subagent's own runs carry `agentId`; the session's carry none.
    fn is_a_subagents(run: &Run) -> bool {
        run.payload.get("agentId").is_some()
    }

    /// The first run of `event` in `scene` that is the session's own.
    fn own(scene_name: &str, event: &str) -> Run {
        scene(scene_name)
            .into_iter()
            .find(|run| run.event == event && !is_a_subagents(run))
            .unwrap_or_else(|| panic!("{scene_name} {event}"))
    }

    /// The statuses the session's own runs of `scene` report, in order.
    fn statuses(scene_name: &str) -> Vec<Option<SessionStatus>> {
        scene(scene_name)
            .iter()
            .filter(|run| !is_a_subagents(run))
            .map(|run| CopilotHooks.status(&run.event, &run.payload))
            .collect()
    }

    /// What each scene's own hooks say of the session's status, from the measured sequences. A
    /// subagent's runs are left out: the daemon reads only the background work off them.
    #[test]
    fn each_scene_reports_the_statuses_the_measurement_gives() {
        const W: Option<SessionStatus> = Some(SessionStatus::Working);
        const I: Option<SessionStatus> = Some(SessionStatus::Idle);
        const U: Option<SessionStatus> = Some(SessionStatus::WaitingUser);
        const N: Option<SessionStatus> = None;
        let approved = vec![N, W, W, N, U, W, I, N];
        for (name, expected) in [
            // sessionStart, prompt, agentStop, sessionEnd.
            ("prompt", vec![N, W, I, N]),
            // The permissionRequest says nothing; the notification is the waiting.
            ("shell-approved", approved.clone()),
            ("create-approved", approved.clone()),
            ("edit-approved", approved.clone()),
            ("apply-patch-approved", approved),
            // An allowed call raises no prompt, whatever its permissionRequest.
            ("mcp-call-allowed", vec![N, W, W, N, W, I, N]),
            // ask_user waits from its preToolUse, and its notification says the same.
            ("ask-user-answered", vec![N, W, U, U, W, I, N]),
            // The notification of the subagent's prompt carries no agentId: the session's own.
            ("subagent-shell-approved", vec![N, W, W, U, W, I, N]),
            // The completion's own prompt is a turn, then its stop.
            (
                "detached-shell-completes",
                vec![N, W, W, N, W, I, W, N, I, N],
            ),
            // errorOccurred is not read, and no agentStop follows.
            ("model-error", vec![N, W, N, N]),
            (
                "permission-refused-with-feedback",
                vec![N, W, W, N, U, I, N],
            ),
            ("two-calls-one-prompt", vec![N, W, W, W, W, N, U, W, I, N]),
            ("subagent-fails", vec![N, W, W, W, I, N]),
            // A cancelled turn and a declined prompt run no hook after the last of these.
            ("cancelled-esc-twice", vec![N, W, N]),
            ("permission-cancelled-esc", vec![N, W, W, N, U, N]),
            ("hosted-apply-patch-denied", vec![N, W, W, I, N]),
            // A message written while a shell command ran runs no hook of its own: the command
            // moves to the background and its call ends, the turn stops, and the command's
            // completion arrives as a turn of its own (its `shell_completed` says nothing more).
            ("message-during-shell", vec![N, W, W, N, W, I, W, N, I, N]),
            // The subagent's failed `ask_user` raises no form and no notification, and none of
            // its own events is read: nothing waits on the user.
            ("subagent-ask-user-unavailable", vec![N, W, W, W, I, N]),
        ] {
            assert_eq!(statuses(name), expected, "{name}");
        }
    }

    /// Every scene of the capture is covered above.
    #[test]
    fn every_scene_of_the_capture_is_one_the_statuses_are_checked_for() {
        let mut names: Vec<String> = runs().into_iter().map(|run| run.scene).collect();
        names.dedup();
        assert_eq!(names.len(), 18, "{names:?}");
    }

    /// A subagent has no `ask_user` tool: its call fails with no form and no notification, so no
    /// hand is raised, whatever the call's name says.
    #[test]
    fn a_subagents_failed_ask_user_raises_no_hand() {
        let runs = scene("subagent-ask-user-unavailable");
        let ask = runs
            .iter()
            .find(|run| run.event == PRE_TOOL_USE && run.payload["toolName"] == "ask_user")
            .expect("the subagent's call");
        assert!(CopilotHooks.is_subagent(&ask.payload));
        assert!(!runs.iter().any(|run| run.event == "Notification"));
        let failure = runs
            .iter()
            .find(|run| run.event == POST_TOOL_USE_FAILURE)
            .expect("the failed call");
        assert_eq!(failure.payload["error"], "Tool 'ask_user' does not exist.");
        assert!(CopilotHooks.is_subagent(&failure.payload));
    }

    /// A `notification` is the one payload that names its event, as `Notification`, and that name is
    /// what the daemon reads; it is told apart by its type, never by the text it displays.
    #[test]
    fn a_notification_names_itself_and_is_read_by_its_type() {
        let prompt = own("shell-approved", "Notification");
        assert_eq!(prompt.payload["hook_event_name"], "Notification");
        assert_eq!(prompt.payload["notification_type"], "permission_prompt");
        // Under the entry's own name too, for a payload that did not name itself.
        assert_eq!(
            CopilotHooks.status(NOTIFICATION, &prompt.payload),
            Some(SessionStatus::WaitingUser)
        );
        // Hand-built: no capture holds a notification of a type that is neither of the two.
        let mut other = prompt.payload.clone();
        other["notification_type"] = Value::from("something_else");
        assert_eq!(CopilotHooks.status("Notification", &other), None);
        other.as_object_mut().unwrap().remove("notification_type");
        assert_eq!(CopilotHooks.status("Notification", &other), None);
        // The display text says "Permission needed" and does not decide.
        assert_eq!(other["title"], "Permission needed");
        // The captured completion notification is of a type that says nothing of the user.
        let shell = own("detached-shell-completes", "Notification");
        assert_eq!(
            shell.payload["notification_type"],
            "shell_detached_completed"
        );
        assert_eq!(CopilotHooks.status("Notification", &shell.payload), None);
    }

    #[test]
    fn a_subagents_runs_are_told_from_the_sessions_by_their_agent_id() {
        for run in runs() {
            assert_eq!(
                CopilotHooks.is_subagent(&run.payload),
                is_a_subagents(&run),
                "{} {}",
                run.scene,
                run.event
            );
        }
        let subagent_runs: Vec<(String, bool)> = scene("subagent-shell-approved")
            .iter()
            .map(|run| (run.event.clone(), is_a_subagents(run)))
            .collect();
        // The subagent's start, its own events and its stop are its; the permission prompt's
        // notification, as measured, is not.
        assert_eq!(
            subagent_runs,
            [
                ("sessionStart", false),
                ("userPromptSubmitted", false),
                ("preToolUse", false),
                ("subagentStart", true),
                ("userPromptSubmitted", true),
                ("preToolUse", true),
                ("permissionRequest", true),
                ("Notification", false),
                ("postToolUse", true),
                ("agentStop", true),
                ("subagentStop", true),
                ("postToolUse", false),
                ("agentStop", false),
                ("sessionEnd", false),
            ]
            .map(|(event, sub)| (event.to_string(), sub))
        );
    }

    /// A subagent runs from its `subagentStart` to its `subagentStop`, and its own events change
    /// nothing of the background work.
    #[test]
    fn a_subagent_is_background_work_from_its_start_to_its_stop() {
        let id = "38f1f246-59de-4ee0-9093-8af2a733d2c1";
        let start = own_or_subagents("subagent-shell-approved", "subagentStart");
        assert_eq!(
            CopilotHooks.background_tasks_change(&start.event, &start.payload),
            Some(BackgroundTasksChange::SubagentStarted(id.to_string()))
        );
        let stop = own_or_subagents("subagent-shell-approved", "subagentStop");
        assert_eq!(
            CopilotHooks.background_tasks_change(&stop.event, &stop.payload),
            Some(BackgroundTasksChange::Finished(vec![id.to_string()]))
        );
        for run in runs() {
            // A shell command's completion notice is read too; the captures' records are not on
            // this machine, so no `agentStop` of theirs lists anything.
            let notice = run.event == PROMPT_SUBMITTED
                && run.payload["prompt"]
                    .as_str()
                    .is_some_and(|prompt| CopilotHooks.is_completion_notice(prompt));
            // A subagent's own prompt says it works, as its start does.
            if run.event == PROMPT_SUBMITTED && is_a_subagents(&run) {
                assert_eq!(
                    CopilotHooks.background_tasks_change(&run.event, &run.payload),
                    Some(BackgroundTasksChange::SubagentStarted(
                        run.payload["agentId"].as_str().unwrap().to_string()
                    )),
                    "{}",
                    run.scene
                );
                continue;
            }
            if notice || matches!(run.event.as_str(), SUBAGENT_START | SUBAGENT_STOP) {
                continue;
            }
            assert!(
                CopilotHooks
                    .background_tasks_change(&run.event, &run.payload)
                    .is_none(),
                "{} {}",
                run.scene,
                run.event
            );
        }
    }

    fn own_or_subagents(scene_name: &str, event: &str) -> Run {
        scene(scene_name)
            .into_iter()
            .find(|run| run.event == event)
            .unwrap_or_else(|| panic!("{scene_name} {event}"))
    }

    /// `sessionStart` comes with the first prompt, so the conversation starts at the prompt and the
    /// session's start is announced by `sessionStart` alone; turns begin at prompts and end at
    /// `agentStop`, and a cancelled turn, a model error and a declined prompt have no end of their
    /// own among the hooks.
    #[test]
    fn the_prompt_starts_the_conversation_and_turns_end_at_agent_stop() {
        for run in runs().iter().filter(|run| !is_a_notice(run)) {
            assert_eq!(
                CopilotHooks.starts_conversation(&run.event, &run.payload),
                run.event == PROMPT_SUBMITTED,
                "{} {}",
                run.scene,
                run.event
            );
            assert_eq!(
                CopilotHooks.announces_session_start(&run.event, &run.payload),
                run.event == SESSION_START,
                "{} {}",
                run.scene,
                run.event
            );
            assert_eq!(
                CopilotHooks.turn_end(&run.event, &run.payload).is_some(),
                run.event == AGENT_STOP,
                "{} {}",
                run.scene,
                run.event
            );
        }
        // Own prompts and own stops per scene: a turn that no stop closes is one a record line ends.
        for (name, prompts, stops) in [
            ("prompt", 1, 1),
            ("shell-approved", 1, 1),
            ("two-calls-one-prompt", 1, 1),
            ("detached-shell-completes", 2, 2),
            ("subagent-shell-approved", 1, 1),
            ("subagent-fails", 1, 1),
            ("model-error", 1, 0),
            ("cancelled-esc-twice", 1, 0),
            ("permission-cancelled-esc", 1, 0),
            // The message written during the shell command ran no `userPromptSubmitted`: the
            // prompts are the first one and the completion's notice.
            ("message-during-shell", 2, 2),
            ("subagent-ask-user-unavailable", 1, 1),
        ] {
            let own_runs: Vec<Run> = scene(name)
                .into_iter()
                .filter(|run| !is_a_subagents(run))
                .collect();
            let count = |event: &str| own_runs.iter().filter(|run| run.event == event).count();
            assert_eq!(count(PROMPT_SUBMITTED), prompts, "{name} prompts");
            assert_eq!(count(AGENT_STOP), stops, "{name} stops");
        }
        for run in runs().iter().filter(|run| run.event == AGENT_STOP) {
            assert_eq!(
                CopilotHooks.turn_outcome(&run.event, &run.payload),
                Some(TurnOutcome::Normal)
            );
        }
        assert_eq!(
            CopilotHooks.turn_outcome(PROMPT_SUBMITTED, &Value::Null),
            None
        );
    }

    /// A detached shell's completion reaches the session as a prompt wrapped in
    /// `<system_notification>`, which the agent sent itself; a prompt anyone typed is not one, even
    /// when it mentions the tag.
    #[test]
    fn a_detached_shells_completion_is_a_notice_and_a_typed_prompt_is_not() {
        let notice = runs().into_iter().find(is_a_notice).expect("a notice");
        assert_eq!(notice.scene, "detached-shell-completes");
        let prompt = CopilotHooks
            .submitted_prompt(&notice.payload)
            .expect("its prompt");
        assert!(prompt.contains("Detached shell"), "{prompt}");
        assert!(CopilotHooks.is_completion_notice(prompt));
        assert!(CopilotHooks.is_self_sent(prompt));
        // So is the completion of a shell command a message moved to the background.
        let moved = scene("message-during-shell")
            .into_iter()
            .find(is_a_notice)
            .expect("a notice");
        let prompt = CopilotHooks
            .submitted_prompt(&moved.payload)
            .expect("its prompt");
        assert!(prompt.contains("Shell command"), "{prompt}");
        assert!(CopilotHooks.is_completion_notice(prompt));
        assert!(CopilotHooks.is_self_sent(prompt));
        // Every other prompt of the capture was typed.
        for run in runs().iter().filter(|run| !is_a_notice(run)) {
            if let Some(prompt) = CopilotHooks.submitted_prompt(&run.payload) {
                assert!(!CopilotHooks.is_completion_notice(prompt), "{prompt}");
                assert!(!CopilotHooks.is_self_sent(prompt), "{prompt}");
            }
        }
        // Hand-built: a typed prompt that mentions the tag, and one that quotes the sentence.
        for typed in [
            "",
            "explain what a <system_notification> block is",
            "Detached shell \"nap\" (shellId: 0) has completed.",
        ] {
            assert!(!CopilotHooks.is_completion_notice(typed), "{typed}");
        }
    }

    fn is_a_notice(run: &Run) -> bool {
        run.event == PROMPT_SUBMITTED
            && run.payload["prompt"]
                .as_str()
                .is_some_and(|prompt| prompt.starts_with("<system_notification>"))
    }

    #[test]
    fn the_prompt_is_taken_as_it_was_sent_and_the_agents_session_id_from_the_payload() {
        let prompt = own("prompt", PROMPT_SUBMITTED);
        assert_eq!(
            CopilotHooks.submitted_prompt(&prompt.payload),
            Some("hello")
        );
        // The bracketed paste of two lines the reference describes is carried verbatim; no capture
        // holds one, so this prompt is built.
        let mut pasted = prompt.payload.clone();
        pasted["prompt"] = Value::from("first line\nsecond line");
        assert_eq!(
            CopilotHooks.submitted_prompt(&pasted),
            Some("first line\nsecond line")
        );
        assert_eq!(
            CopilotHooks.submitted_prompt(&own("prompt", AGENT_STOP).payload),
            None
        );
        for run in runs().iter().filter(|run| !is_a_subagents(run)) {
            assert_eq!(
                CopilotHooks.agent_session_id(&run.payload).as_deref(),
                run.payload["sessionId"].as_str(),
                "{} {}",
                run.scene,
                run.event
            );
        }
        assert_eq!(CopilotHooks.agent_session_id(&Value::Null), None);
    }

    /// A session whose launch runs on the account directory `root`, with what the interpreter
    /// keeps for it from one event to the next.
    struct Launch {
        root: PathBuf,
        carry: crate::hooks::HookCarry,
    }

    impl Launch {
        fn on(root: &Path) -> Self {
            Self {
                root: root.to_path_buf(),
                carry: Default::default(),
            }
        }

        fn session(&self) -> HookSession<'_> {
            HookSession::new("s", None, Some(&self.root), &self.carry)
        }

        /// The message of the turn `stop` ends, read at once.
        fn message_of(&self, stop: &Value) -> Option<String> {
            read_turn_end(&self.session(), stop, Duration::ZERO)
        }

        /// The ids of the work the lines read so far leave running.
        fn running(&self) -> Vec<String> {
            self.session().carry(|kept: &mut Kept| {
                kept.work
                    .running
                    .iter()
                    .map(|task| task.id.clone())
                    .collect()
            })
        }

        /// What the session's `agentStop` `stop` reads of the background work: the read at the
        /// turn's end, made at once, then the answers of the same event, in order.
        fn changes_at(&self, stop: &Value) -> Vec<BackgroundTasksChange> {
            let session = self.session();
            read_turn_end(&session, stop, Duration::ZERO);
            CopilotHooks.background_tasks_changes_in(&session, AGENT_STOP, stop)
        }

        /// [`Self::changes_at`] for a read that finds one change at most.
        fn work_at(&self, stop: &Value) -> Option<BackgroundTasksChange> {
            let mut changes = self.changes_at(stop);
            assert!(changes.len() <= 1, "more than one change: {changes:?}");
            changes.pop()
        }
    }

    /// Where the account directory `root` keeps the record of conversation `id`, its directory
    /// made.
    fn record_at(root: &Path, id: &str) -> PathBuf {
        let dir = root.join("session-state").join(id);
        std::fs::create_dir_all(&dir).expect("record directory");
        dir.join("events.jsonl")
    }

    fn append(record: &Path, lines: &str) {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(record)
            .expect("the record");
        file.write_all(lines.as_bytes()).expect("write the record");
    }

    /// The session's `agentStop` payload, as captured, and its record under `root` holding the
    /// first `lines` lines of the real record.
    fn stop_over(root: &Path, lines: usize) -> Value {
        let stop = own("prompt", AGENT_STOP).payload;
        let record = record_at(root, stop["sessionId"].as_str().unwrap());
        let text: String = RECORD
            .split_inclusive('\n')
            .take(lines)
            .collect::<Vec<_>>()
            .concat();
        std::fs::write(record, text).expect("record");
        stop
    }

    /// A turn ends at `agentStop` with the last text the session itself wrote after its last
    /// prompt, read from its record: the real records of each kind of turn, cut where each ends.
    #[test]
    fn the_final_message_is_the_sessions_last_text_after_its_last_prompt() {
        let dir = ScratchDir::new("copilot-turn-end");
        for (lines, expected, what) in [
            // Lines 1-11: a message that only requests a tool, then the turn's text.
            (11, Some("done"), "a tool-only message, then text"),
            // 1-33: the subagent's own messages sit among the session's.
            (33, Some("done"), "a subagent's messages between"),
            // 1-30: nothing of the session's own after its prompt but a tool-only message, though
            // the subagent said "done" in between.
            (30, None, "only a subagent's text"),
            (48, Some("done"), "a subagent that failed"),
            (85, Some("done"), "a prompt refused with feedback"),
            // 1-94: the last prompt's message is blank; the turn before had text.
            (94, None, "a blank final message"),
            // 1-103: a tool-only message and then a blank one.
            (103, None, "a tool-only message, then a blank one"),
        ] {
            let stop = stop_over(&dir, lines);
            assert_eq!(
                Launch::on(&dir).message_of(&stop).as_deref(),
                expected,
                "{what} (lines 1-{lines})"
            );
        }
        // The payload alone gives the end and no message.
        let end = CopilotHooks
            .turn_end(AGENT_STOP, &stop_over(&dir, 11))
            .expect("a turn end");
        assert!(!end.failed && !end.backstop && end.last_assistant_message.is_none());
    }

    /// The record read is the one the session's own account directory keeps for the payload's
    /// session, wherever the path the payload names points: a file planted at a path of the same
    /// shape is never opened.
    #[test]
    fn the_record_read_is_the_sessions_own_whatever_path_the_payload_names() {
        let dir = ScratchDir::new("copilot-turn-end-place");
        let account = dir.join("account");
        let mut stop = stop_over(&account, 11);
        let id = stop["sessionId"].as_str().unwrap().to_string();
        // As captured, the payload names a probe's home that is not here: the session's own
        // record is read all the same.
        assert_eq!(
            Launch::on(&account).message_of(&stop).as_deref(),
            Some("done")
        );

        // A record of the right shape elsewhere, saying something else, named by the payload.
        let elsewhere = dir.join("elsewhere");
        let planted = record_at(&elsewhere, &id);
        std::fs::write(
            &planted,
            "{\"type\":\"assistant.message\",\"data\":{\"content\":\"planted\"}}\n",
        )
        .expect("planted record");
        stop["transcriptPath"] = Value::from(planted.to_string_lossy().into_owned());
        assert_eq!(
            Launch::on(&account).message_of(&stop).as_deref(),
            Some("done")
        );
        // A launch on an account that keeps no record of the session reads nothing, the planted
        // file included.
        let other = dir.join("other-account");
        std::fs::create_dir_all(other.join("session-state")).expect("directory");
        assert_eq!(Launch::on(&other).message_of(&stop), None);

        // The record of another session of the same account, and an id that is no UUID.
        let mut another = stop.clone();
        another["sessionId"] = Value::from("00000000-0000-4000-8000-000000000000");
        assert_eq!(Launch::on(&account).message_of(&another), None);
        let mut named = stop.clone();
        std::fs::write(record_at(&account, "conv-1"), RECORD).expect("record");
        named["sessionId"] = Value::from("conv-1");
        assert_eq!(Launch::on(&account).message_of(&named), None);

        // No id.
        stop.as_object_mut().unwrap().remove("sessionId");
        assert_eq!(Launch::on(&account).message_of(&stop), None);
    }

    /// A record that is not a regular file gives no message, and is not waited on.
    #[cfg(unix)]
    #[test]
    fn a_record_that_is_a_fifo_gives_no_message() {
        let dir = ScratchDir::new("copilot-turn-end-fifo");
        let stop = stop_over(&dir, 11);
        let record = record_at(&dir, stop["sessionId"].as_str().unwrap());
        std::fs::remove_file(&record).expect("remove the record");
        crate::test_support::mkfifo(&record);
        assert_eq!(Launch::on(&dir).message_of(&stop), None);
    }

    /// A link in place of the records directory or of the conversation's own leads anywhere, and
    /// is not followed.
    #[cfg(unix)]
    #[test]
    fn a_linked_records_directory_gives_no_message() {
        let dir = ScratchDir::new("copilot-turn-end-linked");
        let real = dir.join("real");
        let stop = stop_over(&real, 11);
        let id = stop["sessionId"].as_str().unwrap().to_string();
        assert_eq!(Launch::on(&real).message_of(&stop).as_deref(), Some("done"));

        let linked_state = dir.join("linked-state");
        std::fs::create_dir_all(&linked_state).expect("directory");
        crate::test_support::symlink(
            real.join("session-state"),
            linked_state.join("session-state"),
        )
        .expect("link");
        assert_eq!(Launch::on(&linked_state).message_of(&stop), None);

        let linked_conversation = dir.join("linked-conversation");
        std::fs::create_dir_all(linked_conversation.join("session-state")).expect("directory");
        crate::test_support::symlink(
            real.join("session-state").join(&id),
            linked_conversation.join("session-state").join(&id),
        )
        .expect("link");
        assert_eq!(Launch::on(&linked_conversation).message_of(&stop), None);
    }

    /// One line of filler, of a kind no reader takes anything from.
    const FILLER: &str = "{\"type\":\"session.info\",\"data\":{\"pad\":\"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\"}}\n";

    /// Filler lines longer together than one read reaches back.
    fn past_the_window() -> String {
        FILLER.repeat(RECORD_WINDOW as usize / FILLER.len() + 2)
    }

    /// The turn's last message is found at the end of a record much longer than the first read
    /// reaches back over. Hand-built: no capture is that long.
    #[test]
    fn the_last_message_is_found_at_the_end_of_a_long_record() {
        let dir = ScratchDir::new("copilot-long-record");
        let stop = stop_over(&dir, 0);
        let record = record_at(&dir, stop["sessionId"].as_str().unwrap());
        let mut text = String::from("{\"type\":\"user.message\",\"data\":{\"content\":\"go\"}}\n");
        text.push_str(&past_the_window());
        text.push_str("{\"type\":\"assistant.message\",\"data\":{\"content\":\"the end\",\"toolRequests\":[]}}\n");
        std::fs::write(&record, text).expect("record");
        assert_eq!(
            Launch::on(&dir).message_of(&stop).as_deref(),
            Some("the end")
        );
    }

    /// Each read goes on from where the one before ended: a turn's read takes the lines written
    /// since, a line still being written is left for the next, and a record that is another one,
    /// or shorter than what was read of it, is read anew.
    #[test]
    fn a_read_goes_on_from_where_the_last_one_ended() {
        let dir = ScratchDir::new("copilot-read-on");
        let stop = stop_over(&dir, 0);
        let record = record_at(&dir, stop["sessionId"].as_str().unwrap());
        let launch = Launch::on(&dir);
        let offset = || {
            launch
                .session()
                .carry(|kept: &mut Kept| kept.read.as_ref().map(|(_, offset)| *offset))
        };
        let said = |text: &str| {
            format!("{{\"type\":\"assistant.message\",\"data\":{{\"content\":\"{text}\"}}}}\n")
        };

        append(
            &record,
            "{\"type\":\"user.message\",\"data\":{\"content\":\"go\"}}\n",
        );
        append(&record, &said("first"));
        assert_eq!(launch.message_of(&stop).as_deref(), Some("first"));
        let first = offset().expect("an offset");
        assert_eq!(first, std::fs::metadata(&record).unwrap().len());

        // Half a line: nothing new is taken, and the offset stays before it.
        let second = said("second");
        let (head, rest) = second.split_at(20);
        append(&record, head);
        assert_eq!(launch.message_of(&stop).as_deref(), Some("first"));
        assert_eq!(offset(), Some(first));
        append(&record, rest);
        assert_eq!(launch.message_of(&stop).as_deref(), Some("second"));

        // The record cut short: what was kept of the longer one is dropped.
        std::fs::write(&record, said("anew")).expect("record");
        assert_eq!(launch.message_of(&stop).as_deref(), Some("anew"));
    }

    /// The lines of a turn's end are told: the session's own `agentStop` hook line says the turn
    /// is over, which is what a read at a turn's end waits for, and a prompt after it that it is
    /// not (the real record of two detached shells: line 8 is the first turn's `agentStop`, line
    /// 9 the notification that starts the next).
    #[test]
    fn a_read_says_whether_the_turns_agent_stop_is_in() {
        let dir = ScratchDir::new("copilot-turn-over");
        let record = record_at(&dir, "f2414a15-b12d-42b5-a0fd-3f5ea5ca6c62");
        let mut kept = Kept::default();
        std::fs::write(&record, background_lines(1, 7) + "\n").expect("record");
        assert_eq!(kept.read_on(&record), Some(false));
        append(&record, &(background_lines(8, 8) + "\n"));
        assert_eq!(kept.read_on(&record), Some(true));
        // Nothing new: the lines read hold no `agentStop`.
        assert_eq!(kept.read_on(&record), Some(false));
        let mut anew = Kept::default();
        std::fs::write(&record, background_lines(1, 9) + "\n").expect("record");
        assert_eq!(anew.read_on(&record), Some(false));
        assert_eq!(Kept::default().read_on(&dir.join("absent")), None);
    }

    /// Calls open at `preToolUse` and close at a post event by their ids; only the question's own
    /// call waits on the user from its start; the prompt and the turn's end leave none open.
    #[test]
    fn a_call_opens_at_pre_tool_use_and_closes_at_a_post_event_by_its_id() {
        for run in runs().iter().filter(|run| !is_a_subagents(run)) {
            let id = run.payload["toolCallId"].as_str();
            let change = CopilotHooks.tool_call_change(&run.event, &run.payload);
            match run.event.as_str() {
                PRE_TOOL_USE => assert_eq!(
                    change,
                    Some(ToolCallChange::Started {
                        id,
                        call: CallShape {
                            tool: run.payload["toolName"].as_str(),
                            input: None
                        },
                        waits_on_user: run.payload["toolName"] == "ask_user",
                    }),
                    "{} {}",
                    run.scene,
                    run.payload["toolName"]
                ),
                POST_TOOL_USE | POST_TOOL_USE_FAILURE => {
                    assert_eq!(change, Some(ToolCallChange::Ended { id }), "{}", run.scene)
                }
                PROMPT_SUBMITTED | AGENT_STOP => {
                    assert_eq!(change, Some(ToolCallChange::AllEnded), "{}", run.scene)
                }
                "Notification" | "permissionRequest" | "errorOccurred" | "sessionEnd" => {
                    assert_eq!(change, None, "{} {}", run.scene, run.event)
                }
                _ => {}
            }
        }
        let ask = own("ask-user-answered", PRE_TOOL_USE);
        assert_eq!(ask.payload["toolName"], "ask_user");
    }

    /// The record is watched for a cancel and an error from the prompt on, for a decline while the
    /// hand is up, and for a cancel once the hand comes down or when the status before could not
    /// be read; more of the same arms nothing again.
    #[test]
    fn the_record_is_watched_from_the_prompt_and_while_the_hand_is_up() {
        assert!(CopilotHooks.watch_reads_previous_status());
        let interrupt = Some(RecordWatch::Interrupt {
            record: None,
            turn_id: None,
        });
        let prompt = own("prompt", PROMPT_SUBMITTED);
        for previous in [
            None,
            Some(SessionStatus::Idle),
            Some(SessionStatus::Working),
        ] {
            assert_eq!(watch(&prompt, previous, SessionStatus::Working), interrupt);
        }
        let waiting = own("shell-approved", "Notification");
        assert_eq!(
            watch(
                &waiting,
                Some(SessionStatus::Working),
                SessionStatus::WaitingUser
            ),
            Some(RecordWatch::Decline { record: None })
        );
        // The hand coming down, and a status that could not be read.
        let post = own("shell-approved", POST_TOOL_USE);
        assert_eq!(
            watch(
                &post,
                Some(SessionStatus::WaitingUser),
                SessionStatus::Working
            ),
            interrupt
        );
        assert_eq!(watch(&post, None, SessionStatus::Working), interrupt);
        // More of the same arms nothing again.
        assert_eq!(
            watch(&post, Some(SessionStatus::Working), SessionStatus::Working),
            None
        );
        let stop = own("shell-approved", AGENT_STOP);
        assert_eq!(
            watch(&stop, Some(SessionStatus::Working), SessionStatus::Idle),
            None
        );
    }

    fn watch<'a>(
        run: &'a Run,
        previous: Option<SessionStatus>,
        status: SessionStatus,
    ) -> Option<RecordWatch<'a>> {
        CopilotHooks.record_watch(&run.event, &run.payload, previous, status)
    }

    fn targets_of(scene_name: &str, tool: &str) -> (Vec<String>, Option<std::path::PathBuf>) {
        let run = scene(scene_name)
            .into_iter()
            .find(|run| run.event == PRE_TOOL_USE && run.payload["toolName"] == tool)
            .unwrap_or_else(|| panic!("{scene_name} {tool}"));
        CopilotHooks.edit_targets(&run.payload)
    }

    /// The files a file-writing call names, from the three tools' real calls: `create` and `edit`
    /// name theirs as the `path` of an object, `apply_patch` in the headers of a patch given as a
    /// string, a hosted model's included; a call of no such tool names none.
    #[test]
    fn a_file_writing_call_names_its_files_whichever_shape_its_arguments_have() {
        let work = Some(std::path::PathBuf::from("/tmp/probe/work"));
        let create = targets_of("create-approved", "create");
        assert_eq!(create.0.len(), 1);
        assert!(
            create.0[0].starts_with("/tmp/probe/work/"),
            "{:?}",
            create.0
        );
        assert_eq!(create.1, work);
        let edit = targets_of("edit-approved", "edit");
        assert_eq!(edit.0.len(), 1);
        assert!(edit.0[0].starts_with("/tmp/probe/work/"), "{:?}", edit.0);
        assert_eq!(
            targets_of("apply-patch-approved", "apply_patch"),
            (vec!["/tmp/probe/work/patched.txt".to_string()], work)
        );
        assert_eq!(
            targets_of("hosted-apply-patch-denied", "apply_patch"),
            (
                vec!["/tmp/copilot-probe/other/hosted.txt".to_string()],
                Some(std::path::PathBuf::from("/private/tmp/copilot-probe/own"))
            )
        );
        for (scene_name, tool) in [
            ("shell-approved", "bash"),
            ("mcp-call-allowed", "octoboard-report"),
        ] {
            assert!(targets_of(scene_name, tool).0.is_empty(), "{tool}");
        }
        assert!(CopilotHooks.edit_targets(&Value::Null).0.is_empty());
    }

    /// Hand-built, since no capture holds a patch that updates, moves or deletes: every header of a
    /// patch names a file, and a string that is no patch names none.
    #[test]
    fn every_header_of_a_patch_names_a_file() {
        let patch = |text: &str| {
            CopilotHooks
                .edit_targets(&serde_json::json!({ "toolArgs": text, "cwd": "/tmp/p" }))
                .0
        };
        assert_eq!(
            patch(
                "*** Begin Patch\n*** Add File: a.txt\n+x\n*** Update File: b/c.txt\n*** Move to: d/e.txt\n@@\n-x\n+y\n*** Delete File: f.txt\n*** End Patch\n"
            ),
            ["a.txt", "b/c.txt", "d/e.txt", "f.txt"]
        );
        for not_a_patch in [
            "ls -la",
            "",
            "*** Begin Patch\n*** End Patch\n",
            "*** Add File: \n",
        ] {
            assert!(patch(not_a_patch).is_empty(), "{not_a_patch:?}");
        }
        // An object without a path, or with an empty one, and arguments of another type.
        for args in [
            serde_json::json!({ "command": "ls" }),
            serde_json::json!({ "path": "" }),
            serde_json::json!(null),
            serde_json::json!(["a"]),
        ] {
            let payload = serde_json::json!({ "toolArgs": args });
            assert!(CopilotHooks.edit_targets(&payload).0.is_empty(), "{args}");
        }
    }

    /// A refusal is the decision the reference documents for a `preToolUse` command to print; the
    /// hosted run's denial is its effect.
    #[test]
    fn a_refusal_is_a_deny_decision() {
        assert_eq!(
            CopilotHooks.deny_edit("outside the project"),
            Some(serde_json::json!({
                "permissionDecision": "deny",
                "permissionDecisionReason": "outside the project",
            }))
        );
        // The denied call of the hosted run is followed by no post event, only the turn's stop.
        let events: Vec<String> = scene("hosted-apply-patch-denied")
            .iter()
            .map(|run| run.event.clone())
            .collect();
        assert!(!events.iter().any(|event| event.starts_with("postToolUse")));
        assert_eq!(
            events.iter().filter(|event| *event == "agentStop").count(),
            1
        );
    }

    /// Lines of the real records of the background scenes, one session after another, and the
    /// hooks' payloads of the same sessions (`testdata/copilot-captures.md`, "Background work").
    const BACKGROUND_RECORD: &str =
        include_str!("../../../testdata/copilot_background_events.jsonl");
    const BACKGROUND_PAYLOADS: &str =
        include_str!("../../../testdata/copilot_background_hook_payloads.jsonl");

    /// The lines `from..=to` of the background record, 1-based as the captures' notes give them.
    fn background_lines(from: usize, to: usize) -> String {
        BACKGROUND_RECORD
            .lines()
            .skip(from - 1)
            .take(to + 1 - from)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// What the record text leaves running, as `(id, kind, description)`.
    fn listed(record: &str) -> Vec<(String, String, String)> {
        let mut work = Work::default();
        for line in record.lines() {
            work.read(line);
        }
        work.running
            .into_iter()
            .map(|task| {
                (
                    task.id,
                    task.kind.unwrap_or_default(),
                    task.description.unwrap_or_default(),
                )
            })
            .collect()
    }

    fn shell(id: &str, description: &str) -> (String, String, String) {
        (id.to_string(), "shell".to_string(), description.to_string())
    }

    /// Two shell commands started detached: both are running when the turn that started them ends
    /// (line 8 is its `agentStop`), the longer one alone once the shorter one's completion has been
    /// taken (line 14), and none after the second (line 20).
    #[test]
    fn detached_shell_commands_are_listed_until_each_one_completes() {
        assert_eq!(
            listed(&background_lines(1, 8)),
            [
                shell("shell-0", "long sleep"),
                shell("shell-1", "short sleep")
            ]
        );
        assert_eq!(
            listed(&background_lines(1, 14)),
            [shell("shell-0", "long sleep")]
        );
        assert_eq!(listed(&background_lines(1, 20)), []);
    }

    /// A shell command started in the background and not detached is listed the same way, until
    /// its `shell_completed` notification (lines 22-34).
    #[test]
    fn a_background_shell_command_is_listed_until_it_completes() {
        assert_eq!(
            listed(&background_lines(22, 27)),
            [shell("shell-0", "background sleep")]
        );
        assert_eq!(listed(&background_lines(22, 33)), []);
    }

    /// A command whose end the model reads itself is off the list at that `read_bash` result,
    /// with no notification read: the real results of both forms (lines 31 and 12), each put
    /// right after the turn that started its command.
    #[test]
    fn a_command_the_model_reads_to_its_end_is_not_listed() {
        let read = format!("{}\n{}", background_lines(22, 27), background_lines(30, 31));
        assert_eq!(listed(&read), []);
        let detached = format!("{}\n{}", background_lines(1, 8), background_lines(11, 12));
        assert_eq!(listed(&detached), [shell("shell-0", "long sleep")]);
    }

    /// Only the tag Copilot closes a result with says where the command went. Hand-built from the
    /// real start and result of a background command (lines 24 and 25), since no capture holds a
    /// command that prints Copilot's own words: one that ran to its end and printed them is not
    /// listed, and one started in the background after printing a completion's words is.
    #[test]
    fn what_a_command_prints_is_not_read_as_where_it_went() {
        let with_content = |content: &str| {
            let mut result: Value =
                serde_json::from_str(&background_lines(25, 25)).expect("a record line");
            result["data"]["result"]["content"] = Value::from(content);
            listed(&format!("{}\n{result}", background_lines(24, 24)))
        };
        for printed in [
            "a.rs: \"moved to background\"\n<shellId: 0 completed with exit code 0>",
            "<command started in background with shellId: 0>\n<shellId: 0 completed with exit code 0>",
            "started in detached background with shellId: 0\n\n<shellId: 0 completed with exit code 1>\n",
            "moved to background with shellId: 0",
            "",
        ] {
            assert_eq!(with_content(printed), [], "{printed:?}");
        }
        for went in [
            "<command started in background with shellId: 0>",
            "<shellId: 9 completed with exit code 0>\n<command started in detached background with shellId: 0>",
            "output so far\n<command with shellId: 0 moved to background by the user. Read it later.>",
        ] {
            assert_eq!(
                with_content(went),
                [shell("shell-0", "background sleep")],
                "{went:?}"
            );
        }
    }

    /// A background agent is listed from its start to its completion, under its agent id and with
    /// the description its call gave; the shell command it ran itself, to its end, is never listed
    /// (lines 35-55).
    #[test]
    fn a_background_agent_is_listed_until_it_completes() {
        assert_eq!(
            listed(&background_lines(35, 41)),
            [(
                "a8bf4911-9df4-4a04-bda8-c1ae485c990d".to_string(),
                "general-purpose".to_string(),
                "background agent".to_string()
            )]
        );
        // Through the subagent's own shell command, which ran in the foreground.
        assert_eq!(listed(&background_lines(35, 47)).len(), 1);
        assert_eq!(listed(&background_lines(35, 54)), []);
    }

    /// A background agent that went idle and is written to again (lines 86-124) gets no second
    /// `subagent.started` and no `subagentStart`: it is listed again from the prompt it is sent
    /// (line 112, and its own `userPromptSubmitted`) until the notification that it is idle (118,
    /// and its `subagentStop`), under the description its first start gave.
    #[test]
    fn an_idle_agent_written_to_again_is_listed_while_it_works() {
        let id = "36077193-fd9f-4d2f-9a23-162ef7c9084c";
        let agent = [(
            id.to_string(),
            "general-purpose".to_string(),
            "background agent".to_string(),
        )];
        assert_eq!(listed(&background_lines(86, 93)), agent);
        assert_eq!(listed(&background_lines(86, 111)), []);
        assert_eq!(listed(&background_lines(86, 112)), agent);
        assert_eq!(listed(&background_lines(86, 118)), []);
        let changes: Vec<BackgroundTasksChange> = background_scene("idle-agent-written-to")
            .iter()
            .filter(|(_, payload)| payload.get("agentId").is_some())
            .filter_map(|(event, payload)| CopilotHooks.background_tasks_change(event, payload))
            .collect();
        let started = BackgroundTasksChange::SubagentStarted(id.to_string());
        let stopped = BackgroundTasksChange::Finished(vec![id.to_string()]);
        // Its start and first prompt, its stop, then the prompt written to it and its stop.
        assert_eq!(
            changes,
            [
                started.clone(),
                started.clone(),
                stopped.clone(),
                started,
                stopped
            ]
        );
    }

    /// The record of a process that was killed has no `session.shutdown`; the session resumed
    /// after it starts with a `session.resume` (line 133), and what the killed process left
    /// running (lines 125-132, one command detached and one not) is no longer listed: nothing
    /// will report its end.
    #[test]
    fn work_of_a_killed_process_is_not_listed_once_the_session_is_resumed() {
        assert_eq!(
            listed(&background_lines(125, 132)),
            [
                shell("shell-0", "background long"),
                shell("shell-1", "detached long")
            ]
        );
        assert_eq!(listed(&background_lines(125, 133)), []);
        assert_eq!(listed(&background_lines(125, 136)), []);
    }

    /// A detached shell command the model stops with `stop_bash` gets no completion notification
    /// and is off the list by the stop (lines 56-65).
    #[test]
    fn a_shell_command_the_model_stops_is_not_listed() {
        assert_eq!(
            listed(&background_lines(56, 59)),
            [shell("shell-0", "stopped sleep")]
        );
        assert_eq!(listed(&background_lines(56, 64)), []);
    }

    /// A command that ran to its end names a `shellId` too and is never listed (lines 66-85, two
    /// sessions of foreground commands), and a `session.shutdown` ends whatever ran before it: the
    /// first turn's two shells followed by the session's shutdown (line 21) leave nothing.
    #[test]
    fn foreground_commands_and_an_ended_session_leave_nothing_running() {
        assert_eq!(listed(&background_lines(66, 85)), []);
        let ended = format!("{}\n{}", background_lines(1, 8), background_lines(21, 21));
        assert_eq!(listed(&ended), []);
        assert_eq!(listed(BACKGROUND_RECORD), []);
        assert_eq!(listed(""), []);
        assert_eq!(
            listed("not a record\n{\"type\":\"tool.execution_complete\"}\n"),
            []
        );
    }

    /// The payloads of the background scene `name`, in the order the hooks ran.
    fn background_scene(name: &str) -> Vec<(String, Value)> {
        BACKGROUND_PAYLOADS
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("a payload line"))
            .filter(|line| line["scene"] == name)
            .map(|line| {
                let payload = line["payload"].clone();
                let passed = line["event"].as_str().expect("the entry's event");
                (
                    event_name(&payload).unwrap_or_else(|| passed.to_string()),
                    payload,
                )
            })
            .collect()
    }

    /// Each completion notice of a shell command takes that command off the list, by the id the
    /// record lists it under; a background agent's notice names no id and changes nothing, its
    /// `subagentStop` having taken it off.
    #[test]
    fn a_completion_notice_takes_its_shell_command_off_the_list() {
        let finished = |scene: &str| -> Vec<BackgroundTasksChange> {
            background_scene(scene)
                .iter()
                .filter(|(event, payload)| {
                    event == PROMPT_SUBMITTED && payload.get("agentId").is_none()
                })
                .filter_map(|(event, payload)| CopilotHooks.background_tasks_change(event, payload))
                .collect()
        };
        assert_eq!(
            finished("two-detached-shells"),
            [
                BackgroundTasksChange::Finished(vec!["shell-1".to_string()]),
                BackgroundTasksChange::Finished(vec!["shell-0".to_string()]),
            ]
        );
        assert_eq!(
            finished("background-shell-completes"),
            [BackgroundTasksChange::Finished(vec!["shell-0".to_string()])]
        );
        assert_eq!(finished("background-agent-idle"), []);
        let agent = "a8bf4911-9df4-4a04-bda8-c1ae485c990d";
        let stops: Vec<BackgroundTasksChange> = background_scene("background-agent-idle")
            .iter()
            .filter(|(event, _)| event == SUBAGENT_STOP)
            .filter_map(|(event, payload)| CopilotHooks.background_tasks_change(event, payload))
            .collect();
        assert_eq!(
            stops,
            [BackgroundTasksChange::Finished(vec![agent.to_string()])]
        );
    }

    /// Hand-built, since a description is the model's own text and no capture holds one that
    /// looks like an id: the id read is the notice's own, the last on its line, and a prompt that
    /// is not a notice names none.
    #[test]
    fn a_completion_notice_is_read_for_its_own_shell_id_alone() {
        let notice = |line: &str| format!("<system_notification>\n{line}\n</system_notification>");
        assert_eq!(
            completed_shells(&notice(
                "Shell command \"odd (shellId: 9) name\" (shellId: 2) has completed successfully."
            )),
            ["shell-2"]
        );
        assert_eq!(
            completed_shells(&notice(
                "Detached shell \"nap\" (shellId: 0) has completed."
            )),
            ["shell-0"]
        );
        for not_one in [
            "Shell command \"nap\" (shellId: 0) has completed successfully.".to_string(),
            notice("Agent \"bg-probe\" (general-purpose) has finished processing and is now idle."),
            notice("Shell command \"nap\" (shellId: ) has completed successfully."),
            notice("Shell command \"nap\" (shellId: 0/../1) has completed successfully."),
            String::new(),
        ] {
            assert_eq!(
                completed_shells(&not_one),
                Vec::<String>::new(),
                "{not_one}"
            );
        }
    }

    /// The background scene's `agentStop` payload of the session's own, and where the account
    /// directory `root` keeps that session's record.
    fn background_stop(root: &Path) -> (Value, PathBuf) {
        let (_, stop) = background_scene("two-detached-shells")
            .into_iter()
            .find(|(event, _)| event == AGENT_STOP)
            .expect("the turn's agentStop");
        let record = record_at(root, stop["sessionId"].as_str().unwrap());
        (stop, record)
    }

    fn ids(tasks: &[BackgroundTask]) -> Vec<&str> {
        tasks.iter().map(|task| task.id.as_str()).collect()
    }

    /// The session's own `agentStop` reads what its record's new lines started and ended (the
    /// real record of two detached shells): both commands at the turn that started them (lines
    /// 1-8), the shorter one's end at the next (9-14), the longer one's at the last (15-20), and
    /// nothing where the record cannot be read.
    #[test]
    fn the_sessions_agent_stop_reads_what_its_records_new_lines_started_and_ended() {
        let dir = ScratchDir::new("copilot-background");
        let (stop, record) = background_stop(&dir);
        let launch = Launch::on(&dir);
        assert_eq!(launch.work_at(&stop), None, "no record yet");

        append(&record, &(background_lines(1, 8) + "\n"));
        let Some(BackgroundTasksChange::Started(tasks)) = launch.work_at(&stop) else {
            panic!("the commands the turn started");
        };
        assert_eq!(ids(&tasks), ["shell-0", "shell-1"]);
        assert_eq!(tasks[0].description.as_deref(), Some("long sleep"));

        append(&record, &(background_lines(9, 14) + "\n"));
        assert_eq!(
            launch.work_at(&stop),
            Some(BackgroundTasksChange::Finished(vec!["shell-1".to_string()]))
        );
        append(&record, &(background_lines(15, 20) + "\n"));
        assert_eq!(
            launch.work_at(&stop),
            Some(BackgroundTasksChange::Finished(vec!["shell-0".to_string()]))
        );
        // A turn that started and ended nothing changes nothing.
        assert_eq!(launch.work_at(&stop), None);

        // The payload alone gives nothing.
        assert_eq!(
            CopilotHooks.background_tasks_change(AGENT_STOP, &stop),
            None
        );
    }

    /// The daemon's own two questions of one `agentStop`, in the order it asks them, are served
    /// by one read: the turn's end carries the record's last message, and the background work
    /// the same lines started is the event's change (lines 1-8, whose last is the `agentStop`
    /// line the read waits for).
    #[test]
    fn one_read_at_agent_stop_gives_the_turns_message_and_its_background_work() {
        let dir = ScratchDir::new("copilot-background-one-read");
        let (stop, record) = background_stop(&dir);
        append(&record, &(background_lines(1, 8) + "\n"));
        let launch = Launch::on(&dir);
        let session = launch.session();
        let end = CopilotHooks
            .turn_end_in(&session, AGENT_STOP, &stop)
            .expect("a turn end");
        assert_eq!(
            end.last_assistant_message,
            launch.session().carry(|kept: &mut Kept| kept.said.clone())
        );
        assert!(end.last_assistant_message.is_some());
        let read_to = launch
            .session()
            .carry(|kept: &mut Kept| kept.read.clone())
            .expect("a read");
        assert!(matches!(
            CopilotHooks
                .background_tasks_changes_in(&session, AGENT_STOP, &stop)
                .as_slice(),
            [BackgroundTasksChange::Started(_)]
        ));
        // The second answer read nothing more.
        assert_eq!(
            launch.session().carry(|kept: &mut Kept| kept.read.clone()),
            Some(read_to)
        );
    }

    /// A command started long ago stays listed however much the record has grown since: the
    /// read at a later turn's end goes on from the offset kept, so the shorter command's end
    /// (lines 9-14), written more than a window after both started, takes that one alone off.
    #[test]
    fn work_started_before_the_window_stays_listed_while_it_runs() {
        let dir = ScratchDir::new("copilot-background-window");
        let (stop, record) = background_stop(&dir);
        let launch = Launch::on(&dir);
        append(&record, &(background_lines(1, 8) + "\n"));
        assert!(matches!(
            launch.work_at(&stop),
            Some(BackgroundTasksChange::Started(_))
        ));

        append(&record, &past_the_window());
        append(&record, &(background_lines(9, 14) + "\n"));
        assert_eq!(
            launch.work_at(&stop),
            Some(BackgroundTasksChange::Finished(vec!["shell-1".to_string()]))
        );
        assert_eq!(launch.running(), ["shell-0"]);

        // A process that starts reading there (a relaunch, a daemon restarted) sees only the
        // end: it takes the shorter command off what is listed and adds nothing.
        let restarted = Launch::on(&dir);
        assert_eq!(
            restarted.work_at(&stop),
            Some(BackgroundTasksChange::Finished(vec!["shell-1".to_string()]))
        );
        assert!(restarted.running().is_empty());
    }

    /// A shell call's start and its result, as the record writes them. Hand-built: no capture
    /// ends one command and starts another within one turn.
    fn shell_started(call: &str, shell: &str) -> String {
        format!(
            "{{\"type\":\"tool.execution_start\",\"data\":{{\"toolCallId\":\"{call}\",\"toolName\":\"bash\",\"arguments\":{{\"description\":\"job {shell}\"}}}}}}\n\
             {{\"type\":\"tool.execution_complete\",\"data\":{{\"toolCallId\":\"{call}\",\"success\":true,\"result\":{{\"content\":\"<command started in detached background with shellId: {shell}>\",\"structuredContent\":{{\"shellId\":\"{shell}\"}}}}}}}}\n"
        )
    }

    fn shell_stopped(call: &str, shell: &str) -> String {
        format!(
            "{{\"type\":\"tool.execution_start\",\"data\":{{\"toolCallId\":\"{call}\",\"toolName\":\"stop_bash\"}}}}\n\
             {{\"type\":\"tool.execution_complete\",\"data\":{{\"toolCallId\":\"{call}\",\"success\":true,\"result\":{{\"content\":\"stopped\",\"structuredContent\":{{\"shellId\":\"{shell}\"}}}}}}}}\n"
        )
    }

    /// A turn that ends one command and starts another: where the record was read from its
    /// first byte the list it leaves replaces what is listed; where the first read started
    /// inside a longer record, which may hold work the reads never saw, the start and the end
    /// are both given at the turn's end, the start first, and nothing is left for the next event.
    #[test]
    fn a_turn_that_starts_and_ends_work_gives_both() {
        let dir = ScratchDir::new("copilot-background-both");
        let (stop, record) = background_stop(&dir);
        let turn = shell_stopped("c2", "0") + &shell_started("c3", "1");

        let whole = Launch::on(&dir);
        append(&record, &shell_started("c1", "0"));
        assert!(matches!(
            whole.work_at(&stop),
            Some(BackgroundTasksChange::Started(_))
        ));
        append(&record, &turn);
        let Some(BackgroundTasksChange::Replace(tasks)) = whole.work_at(&stop) else {
            panic!("the whole list");
        };
        assert_eq!(ids(&tasks), ["shell-1"]);

        // The same turn read by a process whose first read started past the record's start.
        std::fs::write(&record, past_the_window() + &shell_started("c1", "0")).expect("record");
        let inside = Launch::on(&dir);
        assert!(matches!(
            inside.work_at(&stop),
            Some(BackgroundTasksChange::Started(_))
        ));
        append(&record, &turn);
        let changes = inside.changes_at(&stop);
        let [BackgroundTasksChange::Started(tasks), BackgroundTasksChange::Finished(ended)] =
            changes.as_slice()
        else {
            panic!("the command the turn started, then the one it ended");
        };
        assert_eq!(ids(tasks), ["shell-1"]);
        assert_eq!(ended, &["shell-0".to_string()]);
        let prompt = serde_json::json!({"sessionId": stop["sessionId"], "prompt": "next"});
        assert_eq!(
            CopilotHooks.background_tasks_changes_in(&inside.session(), PROMPT_SUBMITTED, &prompt),
            Vec::new()
        );
    }

    /// A session's shutdown ends everything, whatever the reads saw start: the list the record
    /// leaves, an empty one, replaces what is listed (line 21 of the real record).
    #[test]
    fn a_shutdown_read_replaces_the_list() {
        let dir = ScratchDir::new("copilot-background-shutdown");
        let (stop, record) = background_stop(&dir);
        let launch = Launch::on(&dir);
        append(&record, &(background_lines(1, 8) + "\n"));
        launch.work_at(&stop);
        assert!(background_lines(21, 21).contains("\"session.shutdown\""));
        append(&record, &(background_lines(21, 21) + "\n"));
        assert_eq!(
            launch.work_at(&stop),
            Some(BackgroundTasksChange::Replace(Vec::new()))
        );
    }

    /// The `permission_prompt` notification of `scene`, and where the account directory `root`
    /// keeps its session's record.
    fn prompt_notification(root: &Path, scene: &str) -> (Value, PathBuf) {
        let (_, notification) = background_scene(scene)
            .into_iter()
            .find(|(event, _)| event == NOTIFICATION_NAMED)
            .expect("the prompt's notification");
        assert_eq!(notification["notification_type"], PERMISSION_PROMPT);
        let record = record_at(root, notification["sessionId"].as_str().unwrap());
        (notification, record)
    }

    /// A `permission_prompt` notification that lands after its prompt was answered and its turn
    /// ended raises no hand (the real hooks and record of `notification-after-turn-end`, lines
    /// 77-84); the same notification raises it while the record shows the prompt unanswered
    /// (77-80), answered with the turn still going (77-83), or nothing of the turn at all.
    #[test]
    fn a_notification_of_a_prompt_answered_with_its_turn_over_raises_no_hand() {
        let dir = ScratchDir::new("copilot-late-notification");
        let (notification, record) = prompt_notification(&dir, "notification-after-turn-end");
        let launch = Launch::on(&dir);
        let held_back = |lines: std::ops::RangeInclusive<usize>| {
            std::fs::write(
                &record,
                background_lines(*lines.start(), *lines.end()) + "\n",
            )
            .expect("record");
            answered_with_the_turn_over(&launch.session(), &notification, Duration::ZERO)
        };
        assert!(held_back(77..=84), "answered, and the turn is over");
        assert!(!held_back(77..=83), "answered, the turn still going");
        assert!(!held_back(77..=80), "the request alone");
        assert!(
            !held_back(77..=78),
            "no prompt of the turn in the record yet"
        );

        // Through the interpreter's own answer, which the daemon asks.
        std::fs::remove_file(&record).expect("remove the record");
        assert_eq!(
            CopilotHooks.status_in(&launch.session(), NOTIFICATION_NAMED, &notification),
            Some(SessionStatus::WaitingUser),
            "no record to say otherwise"
        );
        // A launch on another account does not read this one's record.
        std::fs::write(&record, background_lines(77, 84) + "\n").expect("record");
        let other = dir.join("other-account");
        std::fs::create_dir_all(&other).expect("directory");
        assert!(!answered_with_the_turn_over(
            &Launch::on(&other).session(),
            &notification,
            Duration::ZERO
        ));
    }

    /// The read at a turn's end looks again while the record does not hold the turn's own
    /// `agentStop` line: a record that gains the turn's last message and that line while the read
    /// waits gives that message, and the read stops looking once the line is in.
    #[test]
    fn the_read_at_a_turns_end_waits_for_the_turns_agent_stop_line() {
        let dir = ScratchDir::new("copilot-turn-end-settle");
        let (stop, record) = background_stop(&dir);
        // The shells started; the turn's last message and its `agentStop` (lines 7 and 8) not yet
        // written out.
        append(&record, &(background_lines(1, 6) + "\n"));
        let looks = std::rc::Rc::new(std::cell::Cell::new(0));
        let _written = between_reads_do({
            let (record, looks) = (record.clone(), looks.clone());
            move || {
                looks.set(looks.get() + 1);
                if looks.get() == 1 {
                    append(&record, &(background_lines(7, 8) + "\n"));
                }
            }
        });
        let launch = Launch::on(&dir);
        assert_eq!(
            read_turn_end(&launch.session(), &stop, RECORD_SETTLE).as_deref(),
            Some("done")
        );
        assert_eq!(
            looks.get(),
            1,
            "the read went on looking once the line was in"
        );
    }

    /// A notification whose record says its turn is over is checked once more after the settle
    /// wait: a prompt whose request the record gains meanwhile (a background agent's, put up
    /// right after the session's turn) still raises the hand.
    #[test]
    fn a_prompt_written_out_during_the_second_look_still_raises_the_hand() {
        let dir = ScratchDir::new("copilot-late-notification-settle");
        let (notification, record) = prompt_notification(&dir, "notification-after-turn-end");
        std::fs::write(&record, background_lines(77, 84) + "\n").expect("record");
        let launch = Launch::on(&dir);
        let _written = between_reads_do({
            let record = record.clone();
            move || {
                append(
                    &record,
                    "{\"type\":\"permission.requested\",\"agentId\":\"background-1\",\"data\":{\"requestId\":\"late-request\",\"permissionRequest\":{\"toolCallId\":\"background-call\"}}}\n",
                )
            }
        });
        assert!(!answered_with_the_turn_over(
            &launch.session(),
            &notification,
            RECORD_SETTLE
        ));
    }

    /// Only a permission prompt's notification is checked against the record: the turn's other
    /// events say what they said, whatever the record holds.
    #[test]
    fn the_record_holds_back_no_other_status() {
        let dir = ScratchDir::new("copilot-late-other");
        let (_, record) = prompt_notification(&dir, "notification-after-turn-end");
        std::fs::write(&record, background_lines(77, 84) + "\n").expect("record");
        let launch = Launch::on(&dir);
        for (event, payload) in background_scene("notification-after-turn-end") {
            if event == NOTIFICATION_NAMED {
                continue;
            }
            assert_eq!(
                CopilotHooks.status_in(&launch.session(), &event, &payload),
                CopilotHooks.status(&event, &payload),
                "{event}"
            );
        }
    }
}
