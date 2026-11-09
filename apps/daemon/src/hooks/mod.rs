//! Reading what one agent's hook event says: the session's status, where its turns begin and end,
//! the prompt a turn was started with, where its tool calls start and end, and its working
//! directory.
//!
//! Each provider names its events and shapes its payloads differently, and each has traps that make
//! a naive mapping wrong in a way nothing reports. How one provider's events are read is its
//! [`HookInterpreter`] (`crate::adapter::AgentProvider::hooks`), whose module records its traps;
//! what is common to every provider's payloads is here.
//!
//! Two gaps are accepted rather than worked around, both settled during validation: a turn that
//! ends with a plain-text question to the user is indistinguishable from a finished turn on every
//! agent; and Grok's bash mode (`!`) fires no tool or turn events (only its `idle_prompt` backstop
//! follows). Neither is recoverable by matching terminal text, which would need a VT emulator in
//! the daemon and differs between an agent's own renderers.
//!
//! What a provider's hooks never report — Claude Code's interrupts and declined prompts, a Codex
//! turn that ends in an error, a Grok turn cancelled before its first token — is recovered outside
//! this mapping, by the record watch the interpreter asks for after an event ([`RecordWatch`]).
//!
//! For the payload fields behind these decisions — what each event carries, what it can be
//! correlated on, and what never fires — see `docs/agent-cli-reference.md`.
//!
//! This file only reads a payload. What applying an event does to the session is [`apply`]; the
//! queue that keeps a session's events in the order they arrived is [`queue`].

pub mod apply;
pub mod queue;

use serde_json::Value;

use crate::protocol::{BackgroundTask, SessionStatus};

/// How one provider's hook events are read. Every answer has a default that reads nothing, so a
/// provider implements only what its events carry. A session's end is deliberately not taken from
/// a hook at all: the process exit is observed directly, and whether it means archived or
/// interrupted is not the agent's to say.
pub trait HookInterpreter: Send + Sync {
    /// What this event says about the session's status, or `None` when it says nothing Octoboard
    /// tracks.
    fn status(&self, _event: &str, _payload: &Value) -> Option<SessionStatus> {
        None
    }

    /// The id of the turn this event belongs to, where the agent gives one. Events outside a turn
    /// carry none, and neither does an empty id.
    fn turn_key(&self, _payload: &Value) -> Option<String> {
        None
    }

    /// Whether this event means the session now has a conversation the agent could resume. A
    /// session nobody has typed into has none, and resuming it by id fails rather than reopening
    /// anything.
    ///
    /// It is the prompt submission that creates the record, not the process starting — which is
    /// also where Codex's own session id comes from, since its thread is created lazily on that
    /// first submission. By default the event named `UserPromptSubmit`, which every measured
    /// provider fires for a submitted prompt.
    fn starts_conversation(&self, event: &str) -> bool {
        event == "UserPromptSubmit"
    }

    /// The text the prompt submission ([`Self::starts_conversation`]) submitted, as the user or
    /// Octoboard wrote it: whatever the agent wraps a paste in taken off. By default the payload's
    /// `prompt`.
    fn submitted_prompt<'a>(&self, payload: &'a Value) -> Option<&'a str> {
        payload.get("prompt")?.as_str()
    }

    /// Whether a submitted prompt is the agent reporting finished background work to itself rather
    /// than a prompt anyone sent it, which a turn's bookkeeping counts. None is, by default.
    fn is_completion_notice(&self, _prompt: &str) -> bool {
        false
    }

    /// Whether a submitted prompt is one the agent sent itself rather than one sent from its input
    /// box, which says nothing about what is in the input box: by default exactly a completion
    /// notice ([`Self::is_completion_notice`]).
    fn is_self_sent(&self, prompt: &str) -> bool {
        self.is_completion_notice(prompt)
    }

    /// Whether this payload belongs to a subagent rather than to the session itself (see
    /// [`is_named_subagent`] for why that matters).
    fn is_subagent(&self, payload: &Value) -> bool {
        is_named_subagent(payload)
    }

    /// Whether this event means the user cancelled the turn.
    ///
    /// Separate from [`Self::turn_end`] because the two differ in what they owe: a cancelled turn
    /// produces no report — the user did it and knows — but it still has to be *closed*, or a later
    /// clock-attributed end would find it open and invent a report for it.
    fn turn_cancelled(&self, _event: &str) -> bool {
        false
    }

    /// How the turn this event ends came to an end, or `None` when the event ends none: it is then
    /// no reason to set or clear the paused flag.
    fn turn_outcome(&self, _event: &str, _payload: &Value) -> Option<TurnOutcome> {
        None
    }

    /// Whether this event means a turn has ended, and what can be read out of it.
    ///
    /// Distinct from [`Self::status`] on purpose: several events put a session back to `idle`
    /// without a turn having ended (a `SessionStart`, a cancelled turn), and several turn ends are
    /// not reportable (a Claude Code `Stop` that is only held on background work). Reading one from
    /// the other would make a phantom report out of each.
    fn turn_end(&self, _event: &str, _payload: &Value) -> Option<TurnEnd> {
        None
    }

    /// The change an event makes to the session's background tasks, subagents' starts and stops
    /// included, or `None` when it makes none.
    fn background_tasks_change(
        &self,
        _event: &str,
        _payload: &Value,
    ) -> Option<BackgroundTasksChange> {
        None
    }

    /// Whether this event says the agent stopped on an error, and may now be showing a prompt only
    /// the user can answer, which the next paste and Enter would answer.
    fn stopped_on_agent_error(&self, _event: &str, _payload: &Value) -> bool {
        false
    }

    /// What this event says about the agent's tool calls, for keeping a hand raised for one call
    /// up while another call of the same turn ends (see [`ToolCallChange`]), or `None` when it says
    /// nothing about them.
    fn tool_call_change<'a>(
        &self,
        _event: &str,
        _payload: &'a Value,
    ) -> Option<ToolCallChange<'a>> {
        None
    }

    /// Whether [`Self::record_watch`] is to be given the status the agent was at before the event:
    /// reading it costs a store read before every status write, so only an interpreter that arms a
    /// watch on a move into a status, not on more of the same, asks for it.
    fn watch_reads_previous_status(&self) -> bool {
        false
    }

    /// The record watch to arm after this event, which reported `status` and found the agent at
    /// `previous` (`None` when it was not read or could not be), for what the provider's hooks will
    /// not report. Each watch is spawned, not awaited: it is expected to far outlive the hook's
    /// response, which the adapters' few-second hook timeout would not allow.
    fn record_watch<'a>(
        &self,
        _event: &str,
        _payload: &'a Value,
        _previous: Option<SessionStatus>,
        _status: SessionStatus,
    ) -> Option<RecordWatch<'a>> {
        None
    }
}

/// The interpreter of a provider that reads no hook events.
pub struct NoHooks;

impl HookInterpreter for NoHooks {}

/// What a hook event leaves to be watched for, which the provider's hooks will not report. The
/// provider's record store decides how (`crate::records::RecordStore::arm_watch`): in its own
/// records, in what the agent shows, or both. `record` is the record path the event reported.
#[derive(Debug, PartialEq, Eq)]
pub enum RecordWatch<'a> {
    /// A prompt now waiting on the user may be declined without a hook saying so.
    Decline { record: Option<&'a str> },
    /// The turn just started may be interrupted without a hook saying so.
    Interrupt { record: Option<&'a str> },
    /// The turn just started may end in an error without a hook saying so.
    Failure {
        record: Option<&'a str>,
        turn_id: Option<&'a str>,
    },
}

/// The agent's own session id, if this payload carries it. For Codex this is the only way the id
/// is ever learned.
pub fn agent_session_id(payload: &Value) -> Option<String> {
    for key in ["session_id", "sessionId"] {
        if let Some(id) = payload.get(key).and_then(Value::as_str) {
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }
    }
    None
}

/// What one hook event says about the background work a session has left running.
#[derive(Debug, PartialEq, Eq)]
pub enum BackgroundTasksChange {
    /// The complete list now running. A `Stop` always carries the whole list, so this also
    /// corrects anything missed since the last one; an empty list means nothing is running.
    Replace(Vec<BackgroundTask>),
    /// These tasks have finished: the agent reports each completion to itself as a turn of its
    /// own, whose prompt carries the task's id.
    Finished(Vec<String>),
    /// A named subagent stopped, with the session-wide registry its `SubagentStop` lists. That
    /// list always names the stopping subagent as running, so the subagent's own entry does not
    /// count; the rest replaces the current list. It may be momentarily empty between one
    /// subagent stopping and the next starting, so a change to empty is not final until it has
    /// stood for a while.
    SubagentStopped {
        id: String,
        tasks: Vec<BackgroundTask>,
    },
    /// A named subagent started, or resumed (`SubagentStart` fires again on every resume): it
    /// is running, whether or not a `Stop` has listed it yet.
    SubagentStarted(String),
}

/// What one hook event says about the agent's tool calls. An agent can run several calls of one
/// message at once, so the end of one call says nothing about another that waits on the user: the
/// session's hand comes down only once every call waiting on the user has ended
/// (`AppState::apply_tool_call_change`).
#[derive(Debug, PartialEq, Eq)]
pub enum ToolCallChange<'a> {
    /// A call started: its id where the event names one, and whether it waits on the user from
    /// the start (the agent's own ask-the-user tool).
    Started {
        id: Option<&'a str>,
        call: CallShape<'a>,
        waits_on_user: bool,
    },
    /// A started call is now waiting on the user, the event naming it only by its shape (a
    /// permission prompt).
    WaitsOnUser(CallShape<'a>),
    /// A call ended, by its id where the event names one.
    Ended { id: Option<&'a str> },
    /// Every call of the turn has ended, or the turn has.
    AllEnded,
}

/// What a call is, as far as telling it from the other calls of its turn without an id goes: its
/// tool and the input it was given.
#[derive(Debug, PartialEq, Eq)]
pub struct CallShape<'a> {
    pub tool: Option<&'a str>,
    pub input: Option<&'a Value>,
}

/// Whether this payload belongs to a named subagent, which carries an `agent_type` for every
/// agent. The reading for a session nothing is known of, and the default for every provider.
///
/// Subagent events reach Octoboard's hooks because its hooks are registered for the whole agent,
/// not per turn — and reading them as the session's own is destructive, not just noisy: a Grok
/// subagent's `SessionEnd` carries the *child's* session id, so storing it would overwrite the id
/// a resume needs and point the next resume at a conversation the user never had. A subagent's
/// `StopCancelled` would likewise report the session as finished while it is still working.
pub fn is_named_subagent(payload: &Value) -> bool {
    any_field_set(payload, &["subagentType", "subagent_type", "agent_type"])
}

/// Whether any of `keys` is present with a value: a non-empty string, or anything but null.
pub(crate) fn any_field_set(payload: &Value, keys: &[&str]) -> bool {
    keys.iter()
        .filter_map(|key| payload.get(*key))
        .any(|value| match value {
            Value::String(text) => !text.is_empty(),
            Value::Null => false,
            _ => true,
        })
}

/// The event name as the payload itself reports it. This is the only source: the hook script is
/// argument-free and the callback URL carries the session, not the event.
pub fn event_name(payload: &Value) -> Option<String> {
    for key in ["hook_event_name", "hookEventName"] {
        if let Some(name) = payload.get(key).and_then(Value::as_str) {
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// A turn that has just ended, as far as a report can be built from it.
pub struct TurnEnd {
    /// The turn's final assistant text, where the agent reports one. Absent on a turn-end signal
    /// that carries no payload of its own — Grok's `idle_prompt` backstop is the case that matters.
    pub last_assistant_message: Option<String>,
    /// The turn ended in an error rather than with a result. `last_assistant_message` is then the
    /// user-facing error text, not model output, which anything reading it as the turn's outcome
    /// has to account for.
    pub failed: bool,
    /// This end carries no turn id of its own and can only be attributed by session and clock —
    /// Grok's `idle_prompt` is the one such signal. It also fires after an ordinary `Stop`, so it
    /// may belong to a turn that has already been accounted for, or to one that ended before the
    /// turn now open began.
    pub backstop: bool,
}

/// How a turn ended, as far as whether the session is shown paused (see `AppState::set_paused`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnOutcome {
    /// Cancelled (Codex's `Interrupt`, Grok's `StopCancelled`), or failed (Claude Code's and
    /// Grok's `StopFailure`).
    Abnormal,
    /// An ordinary stop at the prompt, including a Claude Code `Stop` that leaves background
    /// tasks running, and a Claude Code `SessionStart` that begins a fresh conversation.
    Normal,
}

/// A payload's text field, trimmed, or `None` when it is absent or blank.
pub(crate) fn text_field(payload: &Value, key: &str) -> Option<String> {
    crate::text::trimmed_or_none(payload.get(key).and_then(Value::as_str))
}

#[cfg(test)]
mod tests;
