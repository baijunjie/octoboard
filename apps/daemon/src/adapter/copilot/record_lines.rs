//! Where a GitHub Copilot CLI session's record is, and what its lines say: the readers the hook
//! interpreter (`super::hooks`) and the record watch (`super::record_watch`) share. The record is
//! `session-state/<conversation id>/events.jsonl` under the account's directory, one JSON object
//! a line; its facts are in `docs/agent-cli-reference.md`, "GitHub Copilot CLI".

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use crate::hooks::text_field;
use crate::protocol::{AgentId, BackgroundTask};
use crate::providers::copilot::{COPILOT_EVENTS_FILE, LAYOUT};
use crate::records::locate::{plausible_id, Locator};
use crate::state::AppState;

/// How much of the end of the record is read for the prompts of the turn in progress.
pub(super) const PROMPT_WINDOW: u64 = 1024 * 1024;

/// How long after a hand is raised the record is read for the prompt behind it. Copilot writes
/// its record out about a tenth of a second after the last event of a burst, and the events around
/// a prompt (the request, the hooks run for it) are one burst: the request's line was in the file
/// 0.16 s after its own time, a burst's first line up to 0.4 s after. A call that ends within this
/// time of another call's prompt still lowers the hand.
pub(super) const RECORD_SETTLE: Duration = Duration::from_millis(500);

/// The hook a turn's end runs, `agentStop`, which the record also names as a `hook.start` line's
/// `hookType` once Copilot has run it.
pub(super) const AGENT_STOP: &str = "agentStop";

/// The most calls remembered as open while their results are awaited: one that never gets a
/// result (a cancelled call) would otherwise be kept for the life of the process.
const OPEN_CALLS: usize = 1024;

/// The kind a shell command is listed under. A background agent is listed under Copilot's own
/// name for its kind (`agentName`, `general-purpose` say).
const SHELL_TASK: &str = "shell";

/// The record of the session's conversation, in the account its session is pinned to or the
/// default one; `None` while the agent's id of it, or the default account's place, is unknown, or
/// the record is not where Copilot puts one.
pub(super) fn record_of(state: &AppState, session_id: &str) -> Option<PathBuf> {
    let session = state.session_record(session_id).ok()?;
    let root = Locator::default().root(AgentId::COPILOT, session.config_dir.as_deref())?;
    record_in(&root, session.agent_session_id.as_deref()?)
}

/// The record of conversation `id` in the account directory a session's launch runs on,
/// `config_dir`, or in the default account's where the launch names none: the daemon's own
/// word of where the session keeps its record, whatever path a hook's payload names.
pub(super) fn record_under(config_dir: Option<&Path>, id: &str) -> Option<PathBuf> {
    let root = match config_dir {
        Some(dir) => dir.to_path_buf(),
        None => Locator::default().root(AgentId::COPILOT, None)?,
    };
    record_in(&root, id)
}

/// The record of conversation `id` under the account directory `root`, which the daemon resolved
/// itself. The id comes from a hook's payload, so it joins the path only in the shape of an id
/// Copilot mints, and the records directory and the conversation's own must both be real
/// directories: a link there could lead anywhere. The record itself need not exist yet.
pub(super) fn record_in(root: &Path, id: &str) -> Option<PathBuf> {
    if !plausible_id(id) {
        return None;
    }
    let records = root.join(LAYOUT.records_dir);
    let conversation = records.join(id);
    let real_dir = |dir: &Path| std::fs::symlink_metadata(dir).is_ok_and(|meta| meta.is_dir());
    (real_dir(&records) && real_dir(&conversation)).then(|| conversation.join(COPILOT_EVENTS_FILE))
}

/// What the end of a record says of the prompts the session's current turn has put up.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Asked {
    /// A prompt is still unanswered. The ids are the session's own calls whose permission is
    /// asked for; a subagent's permission and an `ask_user` form give none, the one because its
    /// post event is not the session's, the other because its own start already holds it.
    Waiting(Vec<String>),
    /// The turn put prompts up and every one has been answered; `turn_over` when the session's
    /// `agentStop` has run since.
    Answered { turn_over: bool },
    /// The turn put prompts up and was then cancelled.
    Aborted,
    /// The turn put prompts up and then ended in an error, with the text Copilot shows for it.
    Failed(Option<String>),
    /// The record holds no prompt of the turn: nothing can be said.
    Unknown,
}

/// Reads the prompts of the turn in progress off the end of a record: a `permission.requested`
/// until the `permission.completed` of its `requestId`, and the session's own `ask_user` call
/// from its `tool.execution_start` to its `tool.execution_complete`. A prompt of the session's
/// own (`user.message`, or a `system.notification`) starts a turn and the count over; the
/// record's `hook.start` of the session's `agentStop` says the turn is over, and the session's
/// own `abort` and `session.error` that it was cut short, after which no prompt of the turn is
/// still up. Copilot makes a request's line before it runs the notification hook, and a call's
/// start before its `preToolUse`, and writes both to the file later ([`RECORD_SETTLE`]).
pub(super) fn asked_in(record: &str) -> Asked {
    const READ: [&str; 8] = [
        "\"abort\"",
        "\"session.error\"",
        "\"permission.requested\"",
        "\"permission.completed\"",
        "\"tool.execution_",
        "\"user.message\"",
        "\"system.notification\"",
        "\"hook.start\"",
    ];
    // Each open prompt by what closes it, with the call to hold for it.
    let mut open: Vec<(String, Option<String>)> = Vec::new();
    let (mut asked, mut turn_over) = (false, false);
    // How the turn was cut short, by a cancel or by an error.
    let mut cut: Option<Asked> = None;
    for line in record.lines() {
        if !READ.iter().any(|kind| line.contains(kind)) {
            continue;
        }
        let Ok(line) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(kind) = line.get("type").and_then(Value::as_str) else {
            continue;
        };
        let data = line.get("data").unwrap_or(&Value::Null);
        let text =
            |value: &Value, key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
        let own = !of_subagent(&line);
        match kind {
            "user.message" | "system.notification" if own => {
                open.clear();
                (asked, turn_over, cut) = (false, false, None);
            }
            "permission.requested" => {
                let Some(request) = text(data, "requestId") else {
                    continue;
                };
                let call = data
                    .get("permissionRequest")
                    .and_then(|request| text(request, "toolCallId"))
                    .filter(|_| own);
                open.push((request, call));
                (asked, turn_over) = (true, false);
            }
            "permission.completed" => {
                if let Some(request) = text(data, "requestId") {
                    open.retain(|(closes, _)| *closes != request);
                }
            }
            "tool.execution_start" if own => {
                let tool = data.get("toolName").and_then(Value::as_str);
                if AgentId::COPILOT.descriptor().is_ask_question_tool(tool) {
                    if let Some(call) = text(data, "toolCallId") {
                        open.push((call, None));
                        (asked, turn_over) = (true, false);
                    }
                }
            }
            "tool.execution_complete" if own => {
                if let Some(call) = text(data, "toolCallId") {
                    open.retain(|(closes, _)| *closes != call);
                }
            }
            "hook.start"
                if own && data.get("hookType").and_then(Value::as_str) == Some(AGENT_STOP) =>
            {
                turn_over = true;
            }
            "abort" if own => {
                open.clear();
                cut = Some(Asked::Aborted);
            }
            "session.error" if own => {
                open.clear();
                cut = Some(Asked::Failed(text_field(data, "message")));
            }
            _ => {}
        }
    }
    if !open.is_empty() {
        Asked::Waiting(open.into_iter().filter_map(|(_, call)| call).collect())
    } else if let (true, Some(cut)) = (asked, cut) {
        cut
    } else if asked {
        Asked::Answered { turn_over }
    } else {
        Asked::Unknown
    }
}

/// The last `window` bytes of the record at `record`, as text. A window that starts inside a line
/// gives a first line that does not parse, which every reader of the tail passes over.
pub(super) fn record_tail(record: &Path, window: u64) -> Option<String> {
    let mut file = crate::hostfs::open_regular(record).ok()?;
    let len = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(window)))
        .ok()?;
    let mut tail = Vec::new();
    file.take(window).read_to_end(&mut tail).ok()?;
    Some(String::from_utf8_lossy(&tail).into_owned())
}

/// The id a shell command is listed under, from the `shellId` Copilot gives it: a number that
/// starts again at 0 in every session.
pub(super) fn shell_task_id(shell: &str) -> String {
    format!("shell-{shell}")
}

/// What an open tool call of the session's is to its background work.
enum Call {
    /// A shell command, with the description the model gave it.
    Shell(Option<String>),
    /// A `stop_bash`.
    Stop,
    /// A `read_bash` or a `write_bash`.
    Read,
}

/// The background work a session's record leaves running, read a line at a time
/// ([`Self::read`]) so that a record can be read on from where a read ended.
///
/// - A shell command is in the background once its call's result says so (started in the
///   background, started detached, or moved there by the user) and names its `shellId`, until a
///   `system.notification` names that shell as completed, a `stop_bash` call for it succeeds, or
///   a `read_bash` or `write_bash` call for it answers that it completed: the model that reads
///   the end itself is told there, whether or not a notification follows.
/// - A background agent runs from its `subagent.started` with `executionMode: "background"`
///   until its `subagent.completed` or `subagent.failed`, or a notification that names it. One
///   written to again once idle gets no second `subagent.started` and no `subagent.completed`:
///   it runs again from the prompt it is sent, a `user.message` of its own, until the
///   notification that it is idle.
///
/// A `session.shutdown` ends everything before it, and so does a `session.resume`, which is all
/// the record of a process that was killed shows of its end: the process that would have
/// reported a completion has gone. A subagent's own shell commands are not the session's and are
/// not read.
#[derive(Default)]
pub(super) struct Work {
    pub(super) running: Vec<BackgroundTask>,
    /// The session's calls whose results are awaited, by their ids.
    calls: HashMap<String, Call>,
    /// The background agents seen starting, for one that is written to again.
    agents: HashMap<String, BackgroundTask>,
    /// The ids of the work the lines read ended, until the interpreter (`Kept::changes`, `super::hooks`) takes them.
    pub(super) ended: Vec<String>,
    /// Whether a line read ended everything, until the interpreter (`Kept::changes`, `super::hooks`) takes it.
    pub(super) cleared: bool,
}

impl Work {
    fn end(&mut self, id: &str) {
        self.running.retain(|task| task.id != id);
        self.ended.push(id.to_string());
    }

    /// Takes one line of the record.
    pub(super) fn read(&mut self, line: &str) {
        const READ: [&str; 7] = [
            "\"user.message\"",
            "\"session.resume\"",
            "\"tool.execution_start\"",
            "\"tool.execution_complete\"",
            "\"subagent.",
            "\"system.notification\"",
            "\"session.shutdown\"",
        ];
        if !READ.iter().any(|kind| line.contains(kind)) {
            return;
        }
        let Ok(line) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let Some(kind) = line.get("type").and_then(Value::as_str) else {
            return;
        };
        let data = line.get("data").unwrap_or(&Value::Null);
        let text =
            |value: &Value, key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
        match kind {
            "session.shutdown" | "session.resume" => {
                self.running.clear();
                self.calls.clear();
                self.agents.clear();
                self.ended.clear();
                self.cleared = true;
            }
            "user.message" => {
                let again = text(&line, "agentId").and_then(|id| self.agents.get(&id).cloned());
                if let Some(agent) = again {
                    if !self.running.iter().any(|task| task.id == agent.id) {
                        self.running.push(agent);
                    }
                }
            }
            "subagent.started" => {
                let Some(id) = text(&line, "agentId") else {
                    return;
                };
                if data.get("executionMode").and_then(Value::as_str) == Some("background") {
                    self.running.retain(|task| task.id != id);
                    let agent = BackgroundTask {
                        id: id.clone(),
                        kind: text_field(data, "agentName"),
                        description: text_field(data, "agentDescription")
                            .or_else(|| text_field(data, "agentDisplayName")),
                    };
                    self.agents.insert(id, agent.clone());
                    self.running.push(agent);
                }
            }
            "subagent.completed" | "subagent.failed" => {
                if let Some(id) = text(&line, "agentId") {
                    self.end(&id);
                }
            }
            "system.notification" => {
                let Some(about) = data.get("kind") else {
                    return;
                };
                if let Some(shell) = text(about, "shellId") {
                    self.end(&shell_task_id(&shell));
                }
                if let Some(agent) = text(about, "agentId") {
                    self.end(&agent);
                }
            }
            _ if of_subagent(&line) => {}
            "tool.execution_start" => {
                let Some(call) = text(data, "toolCallId") else {
                    return;
                };
                let opened = match data.get("toolName").and_then(Value::as_str) {
                    Some("bash") => Call::Shell(
                        data.get("arguments")
                            .and_then(|arguments| text_field(arguments, "description")),
                    ),
                    Some("stop_bash") => Call::Stop,
                    Some("read_bash" | "write_bash") => Call::Read,
                    _ => return,
                };
                if self.calls.len() >= OPEN_CALLS {
                    self.calls.clear();
                }
                self.calls.insert(call, opened);
            }
            "tool.execution_complete" => {
                let Some(call) = text(data, "toolCallId").and_then(|call| self.calls.remove(&call))
                else {
                    return;
                };
                let result = data.get("result").unwrap_or(&Value::Null);
                let Some(shell) = result
                    .get("structuredContent")
                    .and_then(|content| text(content, "shellId"))
                else {
                    return;
                };
                let id = shell_task_id(&shell);
                let said = result.get("content").and_then(Value::as_str);
                match call {
                    Call::Shell(description) => {
                        if said.is_some_and(in_background) {
                            self.running.retain(|task| task.id != id);
                            self.running.push(BackgroundTask {
                                id,
                                kind: Some(SHELL_TASK.to_string()),
                                description,
                            });
                        }
                    }
                    Call::Stop => {
                        if data.get("success").and_then(Value::as_bool) == Some(true) {
                            self.end(&id);
                        }
                    }
                    Call::Read => {
                        if said.is_some_and(completed) {
                            self.end(&id);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// The tag Copilot closes a shell call's result with: what follows the last line break that a
/// `<` comes after, or the whole result when it opens with one. The command's own output comes
/// before it, so what the command printed is never read as Copilot's word, even where it prints
/// a line of the same form: Copilot's tag still comes last.
fn closing_tag(result: &str) -> Option<&str> {
    let result = result.trim_end();
    let tag = match result.rfind("\n<") {
        Some(at) => &result[at + 1..],
        None => result,
    };
    (tag.starts_with('<') && tag.ends_with('>')).then_some(tag)
}

/// Whether a shell call's result says its command went on in the background, by its closing tag:
/// `<command started in background with shellId: N>`, `<command started in detached background
/// with shellId: N>`, or `<command with shellId: N moved to background by the user. …>`. A
/// command that ran to its end closes with `<shellId: N completed with exit code …>`.
fn in_background(result: &str) -> bool {
    closing_tag(result).is_some_and(|tag| {
        (tag.starts_with("<command started in ") && tag.contains("background with shellId: "))
            || (tag.starts_with("<command with shellId: ") && tag.contains(" moved to background"))
    })
}

/// Whether a `read_bash` or `write_bash` result says the command it read has ended, by its
/// closing tag: `<shellId: N completed with exit code …>`, or `<detached command with shellId: N
/// completed with exit code …>`.
fn completed(result: &str) -> bool {
    closing_tag(result).is_some_and(|tag| {
        (tag.starts_with("<shellId: ") || tag.starts_with("<detached command with shellId: "))
            && tag.contains(" completed with exit code ")
    })
}

/// What a record line of the session's own is to the turn it is read for: a turn's last message
/// is the last text after its prompt, a message that only requests tools having none, so a turn
/// that ended without text does not answer with the text of the turn before it.
pub(super) enum TurnLine {
    /// A prompt, which starts a turn.
    Prompt,
    /// A notification of Copilot's own, which may start a turn of its own.
    Notice,
    /// An assistant message with text.
    Message(String),
    /// The `agentStop` hook being run: the turn is over.
    Stopped,
}

pub(super) fn turn_line(line: &str) -> Option<TurnLine> {
    const READ: [&str; 4] = [
        "\"assistant.message\"",
        "\"user.message\"",
        "\"system.notification\"",
        "\"hook.start\"",
    ];
    if !READ.iter().any(|kind| line.contains(kind)) {
        return None;
    }
    let line: Value = serde_json::from_str(line).ok()?;
    if of_subagent(&line) {
        return None;
    }
    match line.get("type")?.as_str()? {
        "user.message" => Some(TurnLine::Prompt),
        "system.notification" => Some(TurnLine::Notice),
        "assistant.message" => text_field(line.get("data")?, "content").map(TurnLine::Message),
        "hook.start" => (line.get("data")?.get("hookType")?.as_str()? == AGENT_STOP)
            .then_some(TurnLine::Stopped),
        _ => None,
    }
}

/// Whether a record line is a subagent's, which names its agent beside its `type`; the session's
/// own lines name none.
pub(super) fn of_subagent(line: &Value) -> bool {
    line.get("agentId").is_some_and(|id| !id.is_null())
}

/// What a record line says became of the turn.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Ending {
    /// The user cancelled the turn, or declined the prompt it waited at.
    Aborted,
    /// The user answered a permission prompt with what to do differently: the call is refused and
    /// the turn carries on.
    Refused,
    /// The user answered a subagent's permission prompt, allowing the call or refusing it with
    /// what to do differently: the subagent carries on, and with it the session's turn.
    Answered,
    /// The turn ended in an error, with the text Copilot shows for it.
    Failed(Option<String>),
}

/// What one record line says of the turn's end, when it is the session's own `abort`,
/// `session.error` or refused permission, or the answer to a subagent's permission prompt.
///
/// A subagent's lines are otherwise not the session's to read. The answer to its permission
/// prompt is: the prompt's notification names no agent, so it raises the session's hand, and the
/// events that follow the answer (the subagent's own post event, and this line) all name the
/// subagent, so no hook of the session's lowers it. The session's own approvals are left to the
/// post event of their call.
pub(super) fn ending_in_line(line: &str) -> Option<Ending> {
    if !line.contains("\"abort\"")
        && !line.contains("\"session.error\"")
        && !line.contains("\"permission.completed\"")
    {
        return None;
    }
    let line: Value = serde_json::from_str(line).ok()?;
    let data = line.get("data");
    let kind = line.get("type")?.as_str()?;
    if kind == "permission.completed" {
        let answer = data?.get("result")?.get("kind")?.as_str()?;
        return match (of_subagent(&line), answer) {
            (false, REFUSED_BY_USER) => Some(Ending::Refused),
            (true, REFUSED_BY_USER | APPROVED) => Some(Ending::Answered),
            _ => None,
        };
    }
    if of_subagent(&line) {
        return None;
    }
    match kind {
        "abort" => Some(Ending::Aborted),
        "session.error" => Some(Ending::Failed(
            data.and_then(|data| text_field(data, "message")),
        )),
        _ => None,
    }
}

/// The `result.kind` of a permission the user answered with "No, and tell Copilot what to do
/// differently". One the user cancelled with Esc is `cancelled` and is followed by an `abort`.
const REFUSED_BY_USER: &str = "denied-interactively-by-user";

/// The `result.kind` of a permission the user allowed.
const APPROVED: &str = "approved";
