//! Talking to sessions: `instruct_session`, `inform_session` and `report`, and how their delivery
//! is told to the caller.

use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use crate::mcp::lookup::{
    optional_string_list, required_str, resolve_owned_session, resolve_session,
};
use crate::messaging::delivery::{self, Delivery, SentBy, WhenBlocked};
use crate::messaging::reporting::{self, ReportStatus};
use crate::messaging::sharing::{self, OwnerCopy};
use crate::protocol::{error_code, Agent, CodedError, Session, SessionStatus};
use crate::session_tools::catalogue::qualified_tool_name;
use crate::state::AppState;

pub(in crate::mcp) async fn instruct_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    let text = required_str(arguments, "text")?.to_string();
    let outcome = delivery::write_message_off_runtime(state, &target.id, text, WhenBlocked::Queue)
        .await
        .map_err(|err| point_to_reopen(err, caller.agent, &target))?;
    Ok(json!({
        "delivered": outcome == Delivery::Written,
        "note": delivery_note(outcome),
    }))
}

/// A refusal for a session with no process, said with what the caller can do about it: the
/// session is the caller's, so `resume_session` relaunches it with the message. The code and the
/// params stay the refusal's own. `caller` is the agent of the session asking.
fn point_to_reopen(err: anyhow::Error, caller: Agent, target: &Session) -> anyhow::Error {
    match err.downcast_ref::<CodedError>() {
        Some(coded)
            if coded.code == error_code::SESSION_NOT_RUNNING && target.status.has_no_process() =>
        {
            anyhow::Error::new(CodedError {
                code: coded.code,
                message: format!(
                    "{}: it is {}. `{}` relaunches it, and its `text` delivers this message once \
                     it is running",
                    coded.message,
                    if target.status == SessionStatus::Archived {
                        "archived"
                    } else {
                        "interrupted"
                    },
                    qualified_tool_name(caller, "resume_session"),
                ),
                params: coded.params.clone(),
            })
        }
        _ => err,
    }
}

/// What the caller is told of a message for a session. A queued one is reported rather than
/// refused: queuing is the designed behaviour for a session that is waiting for the user, and
/// nagging it is exactly what the caller must not do.
fn delivery_note(outcome: Delivery) -> String {
    outcome.note("The message", "this session", SentBy::Reader)
}

pub(in crate::mcp) async fn inform_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let wanted = required_str(arguments, "session")?;
    let target = resolve_session(state, wanted)?;
    if target.id == caller.id {
        bail!("session `{wanted}` is your own session; there is nobody to share with");
    }
    let text = required_str(arguments, "text")?.to_string();
    let owned_state = state.clone();
    let (caller_id, target_id) = (caller.id.clone(), target.id.clone());
    // Delivery writes into the sessions' PTYs, which blocks.
    let (outcome, copy) = tokio::task::spawn_blocking(move || {
        sharing::deliver_info(&owned_state, &caller_id, &target_id, &text)
    })
    .await??;
    let note = format!("{}{}", delivery_note(outcome), owner_copy_note(&copy));
    Ok(json!({ "delivered": outcome == Delivery::Written, "note": note }))
}

/// What the result of `inform_session` adds of the copy sent to the receiver's owner.
fn owner_copy_note(copy: &OwnerCopy) -> String {
    match copy {
        OwnerCopy::NotNeeded => String::new(),
        OwnerCopy::Sent(delivery) => {
            format!(
                " {}",
                delivery.note("A copy", "its owner", SentBy::Octoboard)
            )
        }
        OwnerCopy::Failed(reason) => format!(
            " Its owner could not be sent a copy ({reason}); the information itself was not \
             affected."
        ),
    }
}

pub(in crate::mcp) async fn report(
    state: &Arc<AppState>,
    session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let summary = required_str(arguments, "summary")?;
    let status_text = required_str(arguments, "status")?;
    let status = ReportStatus::parse(status_text).ok_or_else(|| {
        anyhow!("`{status_text}` is not one of `done`, `failed` or `needs_decision`")
    })?;
    let open_items = optional_string_list(arguments, "open_items")?.unwrap_or_default();

    let (note, _) =
        reporting::deliver_own_report(state, &session.id, summary.to_string(), status, open_items)
            .await?;
    Ok(json!({ "note": note }))
}

#[cfg(test)]
mod tests;
