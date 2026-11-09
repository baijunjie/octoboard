//! How Claude Code's hook events are read.
//!
//! `Stop` and `StopFailure` are mutually exclusive, so both mean "turn over"; a `Stop` carrying a
//! non-empty `background_tasks` means held on background work, not finished. That is an idle
//! session with those tasks running (see [`HookInterpreter::background_tasks_change`]), not a
//! working one: `idle_prompt` fires 60 s after such a `Stop` for a shell, monitor or MCP task, and
//! only an agent task suppresses it, so a session read as working would flip to idle at an
//! arbitrary moment. `Notification` carries the state in `notification_type`, of which
//! `idle_prompt` is the only one worth mapping: its `permission_prompt` does fire, but a fixed six
//! seconds after the `PermissionRequest` that already raised the hand, so it says nothing new.
//! `idle_prompt` is no use as a general backstop either, being armed by `Stop`: a turn that fires
//! no `Stop` never produces one. The session's hooks also receive every subagent's events, at any
//! depth, each carrying an `agent_id`; none of them changes the session's status (see
//! [`HookInterpreter::is_subagent`]), but a named subagent's `SubagentStart` and `SubagentStop`
//! keep the task list current while the session is idle, because a `SubagentStop` carries the
//! session-wide registry and a subagent woken by its own background work runs with no `Stop` of
//! the session's.
//!
//! A post event says only that its own call ended, and one message's calls overlap: a background
//! `Agent` call held by auto mode's safety check can end while an `AskUserQuestion` from the same
//! message waits on the user. So a hand raised for a call stays up until that call ends, which
//! takes the calls told apart ([`HookInterpreter::tool_call_change`]).
//!
//! A user interrupt produces no event at all, and declining a permission prompt or its
//! `AskUserQuestion` is silent in exactly the same way. Neither is left as a gap: a working session
//! would read as working until its next `UserPromptSubmit`, and a waiting one would keep its raised
//! hand with nothing to lower it, so both are recovered from the agent's own transcript, by the
//! record watch asked for after an event (`crate::adapter::claude::transcript_watch`).

use serde_json::Value;

use crate::hooks::{
    any_field_set, text_field, BackgroundTasksChange, CallShape, HookInterpreter, RecordWatch,
    ToolCallChange, TurnEnd, TurnOutcome,
};
use crate::protocol::{AgentId, BackgroundTask, SessionStatus};

pub(super) struct ClaudeHooks;

/// Whether `tool` is Claude Code's own ask-the-user tool, a question waiting on a person.
fn is_question(tool: Option<&str>) -> bool {
    tool.is_some() && tool == AgentId::CLAUDE.descriptor().ask_question_tool
}

/// The ids of every `<task-id>…</task-id>` in a prompt.
fn task_ids(prompt: &str) -> Vec<String> {
    prompt
        .split("<task-id>")
        .skip(1)
        .filter_map(|rest| rest.split_once("</task-id>"))
        .map(|(id, _)| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect()
}

impl HookInterpreter for ClaudeHooks {
    fn status(&self, event: &str, payload: &Value) -> Option<SessionStatus> {
        match event {
            "SessionStart" => Some(SessionStatus::Idle),
            "UserPromptSubmit" | "PostToolUse" | "PostToolUseFailure" | "PostToolBatch" => {
                Some(SessionStatus::Working)
            }
            // The tool name is in the payload rather than the event: Claude Code's own
            // ask-the-user tool is a question waiting on a person, every other tool is work.
            "PreToolUse" => {
                if is_question(payload.get("tool_name").and_then(Value::as_str)) {
                    Some(SessionStatus::WaitingUser)
                } else {
                    Some(SessionStatus::Working)
                }
            }
            "PermissionRequest" => Some(SessionStatus::WaitingUser),
            "Notification" => match payload.get("notification_type").and_then(Value::as_str) {
                Some("idle_prompt") => Some(SessionStatus::Idle),
                _ => None,
            },
            // Idle even when held on background work: the tasks are tracked beside the status.
            "Stop" | "StopFailure" => Some(SessionStatus::Idle),
            _ => None,
        }
    }

    /// Its `prompt_id`, stable for the whole turn.
    fn turn_key(&self, payload: &Value) -> Option<String> {
        let key = payload.get("prompt_id").and_then(Value::as_str)?;
        (!key.is_empty()).then(|| key.to_string())
    }

    /// Octoboard writes every message as a paste, and Claude Code (2.1.295) hands a long one over
    /// as `<pasted_content id="…">` and the text on the lines after it.
    fn submitted_prompt<'a>(&self, payload: &'a Value) -> Option<&'a str> {
        Some(crate::text::without_paste_wrapper(
            payload.get("prompt")?.as_str()?,
        ))
    }

    /// A background task's or subagent's completion, which Claude Code reports to the session as a
    /// prompt of its own opening a `<task-notification>`.
    fn is_completion_notice(&self, prompt: &str) -> bool {
        prompt.contains("<task-notification>")
    }

    /// A completion notice, or the report a subagent resumed with `SendMessage` hands back, which
    /// Claude Code (2.1.316) submits as `Another Claude session sent a message:` followed by an
    /// `<agent-message from="…">` block.
    fn is_self_sent(&self, prompt: &str) -> bool {
        self.is_completion_notice(prompt) || prompt.contains("<agent-message from=")
    }

    /// Any non-empty `agent_id` marks a subagent too, at any depth: the internal helper agent that
    /// summarises a subagent's progress carries an `agent_id` but no `agent_type`, and its tool
    /// events would otherwise read as the session's own work.
    fn is_subagent(&self, payload: &Value) -> bool {
        any_field_set(
            payload,
            &["subagentType", "subagent_type", "agent_type", "agent_id"],
        )
    }

    // No `turn_cancelled`: Claude Code emits nothing at all when the user interrupts. The turn
    // stays open until the next `UserPromptSubmit` re-arms it, which is stale rather than wrong;
    // the status is recovered from the transcript instead.

    /// A `SessionStart` that begins a fresh conversation (`source` `startup`, or `clear` after
    /// `/clear`) is a normal outcome: a fresh conversation has no stopped turn. One that carries
    /// the conversation on (`resume`, `compact`) ends no turn, so a session resumed after its last
    /// turn was cut short comes back paused. The interrupt and declined prompts fire no event;
    /// those are recovered from the transcript.
    fn turn_outcome(&self, event: &str, payload: &Value) -> Option<TurnOutcome> {
        match event {
            "StopFailure" => Some(TurnOutcome::Abnormal),
            "Stop" => Some(TurnOutcome::Normal),
            "SessionStart"
                if matches!(
                    payload.get("source").and_then(Value::as_str),
                    Some("startup" | "clear")
                ) =>
            {
                Some(TurnOutcome::Normal)
            }
            _ => None,
        }
    }

    fn turn_end(&self, event: &str, payload: &Value) -> Option<TurnEnd> {
        match event {
            // A non-empty `background_tasks` means held on work still running, so the turn is
            // not over and synthesising here would report an unfinished turn as a result.
            "Stop" if has_background_work(payload) => None,
            "Stop" => Some(TurnEnd {
                last_assistant_message: text_field(payload, "last_assistant_message"),
                failed: false,
                backstop: false,
            }),
            "StopFailure" => Some(TurnEnd {
                last_assistant_message: text_field(payload, "last_assistant_message"),
                failed: true,
                backstop: false,
            }),
            _ => None,
        }
    }

    fn background_tasks_change(
        &self,
        event: &str,
        payload: &Value,
    ) -> Option<BackgroundTasksChange> {
        if self.is_subagent(payload) {
            // Only a named subagent counts: the internal helper agents fire these events too, with
            // no `agent_type`, and are not work the session has left running.
            let id = text_field(payload, "agent_id")?;
            text_field(payload, "agent_type")?;
            return match event {
                "SubagentStart" => Some(BackgroundTasksChange::SubagentStarted(id)),
                "SubagentStop" => Some(BackgroundTasksChange::SubagentStopped {
                    tasks: background_tasks(payload),
                    id,
                }),
                _ => None,
            };
        }
        match event {
            "Stop" => Some(BackgroundTasksChange::Replace(background_tasks(payload))),
            "StopFailure" => Some(BackgroundTasksChange::Replace(Vec::new())),
            "UserPromptSubmit" => {
                let prompt = self.submitted_prompt(payload)?;
                self.is_completion_notice(prompt)
                    .then(|| BackgroundTasksChange::Finished(task_ids(prompt)))
            }
            _ => None,
        }
    }

    /// `PreToolUse` and the two post events name their call by `tool_use_id`; `PermissionRequest`
    /// does not (2.1.274 and 2.1.319), so its call is told by its tool and input. One message's
    /// calls overlap: in auto mode (2.1.319) a call the safety check holds, an `Agent` say, is let
    /// through only once the rest of the message has streamed, so its `PostToolUse` can come while
    /// an `AskUserQuestion` from the same message waits on the user. A batch's end, a prompt and a
    /// turn's end leave no call open.
    fn tool_call_change<'a>(&self, event: &str, payload: &'a Value) -> Option<ToolCallChange<'a>> {
        let call = || CallShape {
            tool: payload.get("tool_name").and_then(Value::as_str),
            input: payload.get("tool_input"),
        };
        let id = || payload.get("tool_use_id").and_then(Value::as_str);
        match event {
            "PreToolUse" => {
                let call = call();
                let waits_on_user = is_question(call.tool);
                Some(ToolCallChange::Started {
                    id: id(),
                    call,
                    waits_on_user,
                })
            }
            "PermissionRequest" => Some(ToolCallChange::WaitsOnUser(call())),
            "PostToolUse" | "PostToolUseFailure" => Some(ToolCallChange::Ended { id: id() }),
            "PostToolBatch" | "UserPromptSubmit" | "Stop" | "StopFailure" | "SessionStart" => {
                Some(ToolCallChange::AllEnded)
            }
            _ => None,
        }
    }

    fn watch_reads_previous_status(&self) -> bool {
        true
    }

    /// A move into `WaitingUser` arms the rejection watch. A move into `Working` arms the interrupt
    /// watch: a new prompt always, its caller having just retired the previous turn's watch, and
    /// any other event that finds the agent not already working, such as the hand coming down on
    /// an answered prompt. An event reporting more of the same work arms nothing, since arming
    /// again would only retire a watch that is already reading.
    fn record_watch<'a>(
        &self,
        event: &str,
        payload: &'a Value,
        previous: Option<SessionStatus>,
        status: SessionStatus,
    ) -> Option<RecordWatch<'a>> {
        let transcript = payload.get("transcript_path").and_then(Value::as_str);
        match status {
            SessionStatus::WaitingUser => Some(RecordWatch::Decline { record: transcript }),
            SessionStatus::Working
                if self.starts_conversation(event) || previous != Some(SessionStatus::Working) =>
            {
                Some(RecordWatch::Interrupt { record: transcript })
            }
            _ => None,
        }
    }
}

/// The background tasks a `Stop` or `SubagentStop` lists; an entry without an id is not one.
fn background_tasks(payload: &Value) -> Vec<BackgroundTask> {
    payload
        .get("background_tasks")
        .and_then(Value::as_array)
        .map(|tasks| tasks.iter().filter_map(background_task).collect())
        .unwrap_or_default()
}

fn background_task(entry: &Value) -> Option<BackgroundTask> {
    Some(BackgroundTask {
        id: text_field(entry, "id")?,
        kind: text_field(entry, "type"),
        description: text_field(entry, "description"),
    })
}

/// Whether a `Stop` only holds the turn on background work: the same list
/// [`HookInterpreter::background_tasks_change`] records, so the two agree.
fn has_background_work(payload: &Value) -> bool {
    !background_tasks(payload).is_empty()
}
