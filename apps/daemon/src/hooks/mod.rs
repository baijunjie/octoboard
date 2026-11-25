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

use std::any::Any;
use std::path::Path;
use std::sync::Mutex;

use serde_json::Value;

use crate::protocol::{BackgroundTask, SessionStatus};

/// The session a hook event is applied to, as far as reading the event may need it: handed to the
/// answers of a [`HookInterpreter`] that end in `_in`. An interpreter is one value for every
/// session of its provider, so whatever it has to know of one session, or keep for it from one
/// event to the next, is here.
pub struct HookSession<'a> {
    /// Octoboard's id of the session.
    pub id: &'a str,
    /// The directory the daemon made for the session's launch, where the launch has one and its
    /// process is there: what a path a payload names is checked against where the agent keeps the
    /// file under it. The daemon resolved it itself; nothing a payload says moves it.
    pub scratch: Option<&'a Path>,
    /// The agent's configuration directory the launch runs on, the account's; `None` for the
    /// agent's default account, whose directory the provider knows, and where the session has no
    /// process.
    pub config_dir: Option<&'a Path>,
    carry: &'a HookCarry,
}

impl<'a> HookSession<'a> {
    pub fn new(
        id: &'a str,
        scratch: Option<&'a Path>,
        config_dir: Option<&'a Path>,
        carry: &'a HookCarry,
    ) -> Self {
        Self {
            id,
            scratch,
            config_dir,
            carry,
        }
    }

    /// Runs `with` on what the interpreter keeps for this session from one event to the next (how
    /// far it has read into the agent's record, what one read found for a later answer of the same
    /// event), a `T::default()` where it has kept nothing yet. The interpreter's answers for one
    /// event are asked in a fixed order ([`HookInterpreter::turn_message_in`],
    /// [`HookInterpreter::turn_end_in`], [`HookInterpreter::background_tasks_changes_in`],
    /// [`HookInterpreter::status_in`]) and a session's events are applied one at a time, so what
    /// one answer leaves a later one of the same event finds, when that one is asked. Not every
    /// answer is asked for every event: a subagent's event is asked only
    /// [`HookInterpreter::background_tasks_changes_in`], one that reaches a session with no
    /// process is not asked [`HookInterpreter::turn_message_in`], and a turn end refused (not this
    /// turn's, or held for a later event) is asked neither of the last two. What one read found
    /// for a later answer therefore has to be kept until that answer is asked, at a later event if
    /// need be, not overwritten by the next read.
    ///
    /// It is kept with the session's process and no longer: a relaunch, the end of the process and
    /// a daemon that adopts the launch after a restart all start from `T::default()`, and so does
    /// an event that reaches a session with no process, whose carry is dropped with the event. An
    /// interpreter therefore keeps only what it can do without (an offset it can read again from
    /// a window), and keeps one type: asking for another drops what was kept.
    pub fn carry<T: Default + Send + 'static, R>(&self, with: impl FnOnce(&mut T) -> R) -> R {
        self.carry.with(with)
    }
}

/// What a provider's hook interpreter keeps for one session ([`HookSession::carry`]). It lives in
/// the session's process record (`crate::session::LiveSession`), so it goes when that does.
#[derive(Default)]
pub struct HookCarry(Mutex<Option<Box<dyn Any + Send>>>);

impl HookCarry {
    fn with<T: Default + Send + 'static, R>(&self, with: impl FnOnce(&mut T) -> R) -> R {
        // A panic in an interpreter leaves what it kept as it stood, which it can do without.
        let mut slot = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !slot.as_ref().is_some_and(|kept| kept.is::<T>()) {
            *slot = Some(Box::new(T::default()));
        }
        let kept = slot
            .as_mut()
            .and_then(|kept| kept.downcast_mut::<T>())
            .expect("the slot was just made to hold a T");
        with(kept)
    }

    /// Whether the interpreter has kept anything for the session.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_none()
    }
}

/// How one provider's hook events are read. Every answer has a default that reads nothing, so a
/// provider implements only what its events carry. A session's end is deliberately not taken from
/// a hook at all: the process exit is observed directly, and whether it means archived or
/// interrupted is not the agent's to say.
pub trait HookInterpreter: Send + Sync {
    /// A live context measurement from the main conversation of the launch that posted the hook.
    fn context_report(
        &self,
        _event: &str,
        _payload: &Value,
    ) -> Option<crate::records::context::ContextReport> {
        None
    }

    /// File paths this tool-call payload may edit, and the working directory for relative paths.
    /// An unrecognised payload touches nothing.
    fn edit_targets(&self, _payload: &Value) -> (Vec<String>, Option<std::path::PathBuf>) {
        (Vec::new(), None)
    }

    /// The provider's refusal response for an edit, or none when no response format is known.
    fn deny_edit(&self, _reason: &str) -> Option<Value> {
        None
    }

    /// What this event says about the session's status, or `None` when it says nothing Octoboard
    /// tracks.
    fn status(&self, _event: &str, _payload: &Value) -> Option<SessionStatus> {
        None
    }

    /// A program-status record delivered through an authenticated launch hook when its terminal
    /// cannot carry the report.
    fn program_status(
        &self,
        _event: &str,
        _payload: &Value,
    ) -> Option<crate::terminal::program_status::Record> {
        None
    }

    /// [`Self::status`] for an interpreter that has to know the session to say it: one whose agent
    /// posts an event late, after what it reports is over (a notification of a prompt that was
    /// answered already), and which can tell that from its record. Answering `None` holds the
    /// status back before anything of it is applied: nothing is published, the session's owner is
    /// told nothing, and the rest of what the status would set going is left out, as for an event
    /// that says nothing of the status. It is what the daemon asks; by default the payload's own
    /// answer. Asked on the session's hook queue, last of the answers for an event.
    fn status_in(
        &self,
        _session: &HookSession<'_>,
        event: &str,
        payload: &Value,
    ) -> Option<SessionStatus> {
        self.status(event, payload)
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
    /// first submission. No event starts a conversation unless the provider declares it. The
    /// payload is given for a provider that fires the same event for something that is no prompt.
    fn starts_conversation(&self, _event: &str, _payload: &Value) -> bool {
        false
    }

    /// The text the prompt submission ([`Self::starts_conversation`]) submitted, as the user or
    /// Octoboard wrote it: whatever the agent wraps a paste in taken off. None, by default.
    fn submitted_prompt<'a>(&self, _payload: &'a Value) -> Option<&'a str> {
        None
    }

    /// Whether the prompt that opens a turn can reach the daemon after the turn has started with
    /// none, so that the first prompt submitted after such a start is that turn's own and decides
    /// its credit (`AppState::turn_started_prompt_may_follow`): for a provider whose turns the
    /// program's status reports open while its prompts come by another path that nothing orders
    /// against them. Not by default: a prompt submitted while a turn is open joins it.
    fn opening_prompt_may_follow_start(&self) -> bool {
        false
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
    /// [`is_named_subagent`] for why that matters). No payload does, by default.
    fn is_subagent(&self, _payload: &Value) -> bool {
        false
    }

    /// The agent's own session id, if this payload carries it: what a resume needs. None, by
    /// default.
    fn agent_session_id(&self, _payload: &Value) -> Option<String> {
        None
    }

    /// Whether this event is the agent saying its session has started, past its start-up screens
    /// (a trust confirmation among them): what lets the session's input go to the agent once its
    /// input box is up. No event says so unless the provider declares it.
    fn announces_session_start(&self, _event: &str, _payload: &Value) -> bool {
        false
    }

    /// Whether this event shows the agent past its startup screens, its folder-trust confirmation
    /// among them: such an event ends the watch for that confirmation and lets through the
    /// messages held for it (`crate::session::trust::on_hook`). Every interpreter answers, with no
    /// default: a wrong "yes" lets a held message's Enter answer the confirmation. Every event
    /// does only for a provider measured to run no hook before its confirmation is answered, which
    /// is what each registered provider answers today, some by not registering the hook that would
    /// run early. It is here for a provider whose hooks can run before its startup screens are
    /// behind it and which has to register them: it says no for those events, and names what does
    /// show its startup behind it in its trust screen's start signal
    /// (`crate::session::trust::StartSignal`).
    fn past_startup(&self, event: &str, payload: &Value) -> bool;

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
    /// not reportable (a `Stop` that is only held on background work). Reading one from
    /// the other would make a phantom report out of each.
    fn turn_end(&self, _event: &str, _payload: &Value) -> Option<TurnEnd> {
        None
    }

    /// [`Self::turn_end`] for an interpreter that reads the turn's message from the agent's record
    /// and keeps how far it read ([`HookSession::carry`]). It is what the daemon asks; by default
    /// the payload's own answer.
    fn turn_end_in(
        &self,
        _session: &HookSession<'_>,
        event: &str,
        payload: &Value,
    ) -> Option<TurnEnd> {
        self.turn_end(event, payload)
    }

    /// Whether the provider gives its turns' messages through [`Self::turn_message`], so that a
    /// turn end the program's status report closes waits for one, up to a bound
    /// (`crate::session::program_status`). For a provider whose status comes from those reports
    /// and whose report carries the turn's message cut short, or not at all. None does, by
    /// default, and its turn ends wait for nothing.
    fn supplies_turn_message(&self) -> bool {
        false
    }

    /// Whether the `msg` of the program's `done` or `error` report is the turn's message, cut
    /// short or whole, so that it stands where the provider supplied none
    /// ([`Self::supplies_turn_message`]). It is, by default. A provider whose program reports
    /// something else there (the session's name, say) answers no: a turn its provider gave no
    /// message for, in time or at all, then ends with none, whatever its report's `msg` holds.
    fn report_msg_is_turn_message(&self) -> bool {
        true
    }

    /// The message of the session's current turn as this event gives it, for the turn end the
    /// program's status report closes, whose `msg` it replaces; `None` when the event gives none.
    /// It closes no turn: [`Self::turn_end`] is the turn end of a provider whose hooks give its
    /// status. Called on the session's hook queue, so a source that has to be opened is returned
    /// as [`TurnMessage::Read`] and read off it.
    fn turn_message(&self, _event: &str, _payload: &Value) -> Option<TurnMessage> {
        None
    }

    /// [`Self::turn_message`] for an interpreter that has to know the session to give it: one that
    /// opens a file the payload names only where it lies in the session's own directory
    /// ([`HookSession::scratch`], [`HookSession::config_dir`]). It is what the daemon asks; by
    /// default the payload's own answer.
    fn turn_message_in(
        &self,
        _session: &HookSession<'_>,
        event: &str,
        payload: &Value,
    ) -> Option<TurnMessage> {
        self.turn_message(event, payload)
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

    /// Every change an event makes to the session's background tasks, applied in the order given:
    /// for an interpreter whose agent has no event that lists its background work, and which reads
    /// it from the agent's record instead, where one read can find more than one kind of change
    /// (tasks started and tasks finished). It keeps how far it read ([`HookSession::carry`]) and
    /// answers with what the new lines started ([`BackgroundTasksChange::Started`]) and ended, so
    /// the list does not depend on how much of the record one read reaches back over. It is what
    /// the daemon asks; by default the payload's own answer ([`Self::background_tasks_change`]),
    /// or none.
    fn background_tasks_changes_in(
        &self,
        _session: &HookSession<'_>,
        event: &str,
        payload: &Value,
    ) -> Vec<BackgroundTasksChange> {
        self.background_tasks_change(event, payload)
            .into_iter()
            .collect()
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

impl HookInterpreter for NoHooks {
    /// A provider that registers no hook has none that runs before its startup screens; a callback
    /// that reaches its session all the same ends the watch, as one that names no event does.
    fn past_startup(&self, _event: &str, _payload: &Value) -> bool {
        true
    }
}

/// What a hook event leaves to be watched for, which the provider's hooks will not report. The
/// provider's record store decides how (`crate::records::RecordStore::arm_watch`): in its own
/// records, in what the agent shows, or both. `record` is the record path the event reported.
#[derive(Debug, PartialEq, Eq)]
pub enum RecordWatch<'a> {
    /// A prompt now waiting on the user may be declined without a hook saying so.
    Decline { record: Option<&'a str> },
    /// The turn just started may be interrupted without a hook saying so. `turn_id` is the turn the
    /// event belongs to, as the agent's record names it, so that an interrupt the record holds for
    /// another turn is not taken for this one's.
    Interrupt {
        record: Option<&'a str>,
        turn_id: Option<&'a str>,
    },
    /// The turn just started may end in an error without a hook saying so.
    Failure {
        record: Option<&'a str>,
        turn_id: Option<&'a str>,
    },
}

/// The agent's own session id, if this payload carries it under the `session_id` or `sessionId`
/// field: the reading a provider's [`HookInterpreter::agent_session_id`] may use.
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// These tasks started and run in the background, beside whatever is listed already: for an
    /// agent with no event that lists its background work, whose provider reads each start as it
    /// comes. Unlike a subagent's start it counts inside a turn too, the provider having told
    /// background work from a call the turn waits for. A task already listed under its id stays as
    /// it is.
    Started(Vec<BackgroundTask>),
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
    /// A started call is now waiting on the user, the event naming it by its id. A call not known
    /// as started waits all the same, under that id.
    WaitsOnUserById(&'a str),
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

/// Whether this payload names a subagent in a `subagentType`, `subagent_type` or `agent_type` field:
/// the reading a provider's [`HookInterpreter::is_subagent`] may use.
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

/// The event name as the payload itself reports it. Where it names none, the name the hook route
/// was given beside the body stands in (see `apply::apply_hook`); one the payload names wins.
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

/// The message of a turn as a provider gives it from a source of its own
/// ([`HookInterpreter::turn_message`]).
pub enum TurnMessage {
    /// The event carried it: the message, or `None` where the turn said nothing, which ends the
    /// turn end's wait all the same.
    Said(Option<String>),
    /// It has to be read from somewhere that may block, the agent's transcript say. Run on a
    /// blocking task; `None` where nothing could be read.
    Read(Box<dyn FnOnce() -> Option<String> + Send>),
}

/// How a turn ended, as far as whether the session is shown paused (see `AppState::set_paused`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnOutcome {
    /// Cancelled (Codex's `Interrupt`, Grok's `StopCancelled`), or failed (Claude Code's and
    /// Grok's `StopFailure`).
    Abnormal,
    /// An ordinary stop at the prompt, including a `Stop` that leaves background
    /// tasks running, and a Claude Code `SessionStart` that begins a fresh conversation.
    Normal,
}

/// A payload's text field, trimmed, or `None` when it is absent or blank.
pub(crate) fn text_field(payload: &Value, key: &str) -> Option<String> {
    crate::text::trimmed_or_none(payload.get(key).and_then(Value::as_str))
}

#[cfg(test)]
mod tests;
