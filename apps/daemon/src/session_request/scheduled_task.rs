//! The request a session makes with a scheduled task tool that creates, changes, turns on or off,
//! deletes or runs a task while the user has not chosen to have scheduled task changes approved
//! without asking: the same wait, hand and answer as a request for a session (see `super`), with
//! the tool's own operation to approve, handed over as a [`TaskPerform`] and run once on an
//! approval. The dialog is told what the call asks for ([`ScheduledTaskAsked`]) and the text the
//! agent wrote ([`TaskChange::prompt`]); it has nothing for the user to edit. With the setting on,
//! the operation runs at once through `change::run_replacing`.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;

use crate::protocol::{AgentId, ScheduledTaskAsked, Session, SessionRequestAction};
use crate::state::AppState;

use super::{wait_for_answer, Ask, Change, Request, TaskPerform};

/// A change to a scheduled task, as the call asked for it and the dialog tells it.
pub(crate) struct TaskChange {
    /// The tool the change was asked with, one of the five that write.
    pub tool: &'static str,
    pub action: SessionRequestAction,
    pub asked: ScheduledTaskAsked,
    /// The agent-written text of the call: the task's prompt for a create, the new one for an
    /// update that changes it.
    pub prompt: Option<String>,
    pub perform: TaskPerform,
}

/// Where the task is after the change, and which agent the dialog names for it.
pub(crate) struct TaskPlace {
    pub agent: AgentId,
    pub console: String,
    pub project: Option<String>,
}

impl TaskChange {
    /// What names the change among the requester's: its tool and the task it acts on, as a switch
    /// or a handoff is named (`Change::slot_of`). A create names no task, so it has none, and
    /// replaces nothing.
    pub(super) fn slot(&self) -> Option<String> {
        self.asked
            .task
            .as_deref()
            .map(|task| Change::slot_of(self.tool, task))
    }

    /// What the change is called in what the call is told of how it ended.
    pub(super) fn what(&self) -> &'static str {
        match self.action {
            SessionRequestAction::CreateScheduledTask => "request to create a scheduled task",
            SessionRequestAction::UpdateScheduledTask => "request to change a scheduled task",
            SessionRequestAction::SetScheduledTaskEnabled => {
                "request to turn a scheduled task on or off"
            }
            SessionRequestAction::DeleteScheduledTask => "request to delete a scheduled task",
            _ => "request to run a scheduled task",
        }
    }

    /// What a request that came to nothing says was left undone.
    pub(super) fn untouched(&self) -> &'static str {
        match self.action {
            SessionRequestAction::RunScheduledTask => "the task was not run",
            _ => "nothing was changed",
        }
    }
}

/// The ask of the scheduled task tools that write: asks the user to approve `change` and waits up
/// to `limit` for the answer, as `ask_to_change` does. What it could never be carried out for is
/// refused by the caller before this. What the call returns is the result `change`'s
/// [`TaskPerform`] gives.
pub(crate) async fn ask_to_change_task(
    state: &Arc<AppState>,
    caller: &Session,
    place: TaskPlace,
    change: TaskChange,
    limit: Duration,
) -> Result<Value> {
    let request = Request::new(
        caller,
        place.agent,
        place.console,
        place.project,
        Ask::ScheduledTask(change),
        false,
    );
    wait_for_answer(state, caller, request, limit).await
}
