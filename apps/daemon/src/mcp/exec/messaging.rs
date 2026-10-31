//! Talking to sessions: `instruct_session`, `send_message` and `report`, and how their delivery is
//! told to the caller.

use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use crate::mcp::args::{required_str, string_list};
use crate::messaging::delivery::{self, Delivery, WhenBlocked};
use crate::messaging::reporting::{self, ReportStatus};
use crate::messaging::sharing::{self, OwnerCopy};
use crate::protocol::{error_code, CodedError, Session, SessionStatus};
use crate::state::AppState;

use super::resolve::{resolve_owned_session, resolve_session};

pub(super) async fn instruct_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    let text = required_str(arguments, "text")?.to_string();
    let outcome = delivery::write_message_off_runtime(state, &target.id, text, WhenBlocked::Queue)
        .await
        .map_err(|err| point_to_reopen(err, &target))?;
    Ok(json!({
        "delivered": outcome == Delivery::Written,
        "note": delivery_note(outcome),
    }))
}

/// A refusal for a session with no process, said with what the caller can do about it: the
/// session is the caller's, so `reopen_session` relaunches it with the message. The code and the
/// params stay the refusal's own.
fn point_to_reopen(err: anyhow::Error, target: &Session) -> anyhow::Error {
    match err.downcast_ref::<CodedError>() {
        Some(coded)
            if coded.code == error_code::SESSION_NOT_RUNNING && target.status.is_dormant() =>
        {
            anyhow::Error::new(CodedError {
                code: coded.code,
                message: format!(
                    "{}: it is {}. `reopen_session` relaunches it, and its `text` delivers this \
                     message once it is running",
                    coded.message,
                    if target.status == SessionStatus::Archived {
                        "archived"
                    } else {
                        "interrupted"
                    },
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
fn delivery_note(outcome: Delivery) -> &'static str {
    match outcome {
        Delivery::Written => "Delivered.",
        Delivery::Queued => {
            "Queued: this session cannot take a message right now. It will be delivered as soon \
             as it can, so do not send it again."
        }
        Delivery::Unsent => {
            "Not sent: the message was written into this session's input, but its agent did not \
             send it to the conversation (it is in the input box, or went to a view of its \
             background agents), so the session has not read it. The user has been told to check \
             the session; do not send it again."
        }
        Delivery::Kept => {
            "Queued: this session is interrupted, with no process running. It is kept, and \
             delivered once the session is resumed, so do not send it again."
        }
    }
}

pub(super) async fn send_message(
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
    let mut note = delivery_note(outcome).to_string();
    match copy {
        OwnerCopy::NotNeeded => {}
        OwnerCopy::Sent(Delivery::Written) => note.push_str(" Its owner was sent a copy."),
        OwnerCopy::Sent(Delivery::Queued) => note.push_str(
            " Its owner's copy is queued, and will arrive as soon as the owner can take it.",
        ),
        OwnerCopy::Sent(Delivery::Kept) => {
            note.push_str(" Its owner's copy is kept, and will arrive once the owner is resumed.")
        }
        OwnerCopy::Sent(Delivery::Unsent) => note.push_str(
            " Its owner's copy was written into the owner's input but not sent to its \
             conversation; the user has been told.",
        ),
        OwnerCopy::Failed(reason) => note.push_str(&format!(
            " Its owner could not be sent a copy ({reason}); the information itself was not \
             affected."
        )),
    }
    Ok(json!({ "delivered": outcome == Delivery::Written, "note": note }))
}

pub(super) async fn report(
    state: &Arc<AppState>,
    session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let summary = required_str(arguments, "summary")?;
    let status_text = required_str(arguments, "status")?;
    let status = ReportStatus::parse(status_text).ok_or_else(|| {
        anyhow!("`{status_text}` is not one of `done`, `failed` or `needs_decision`")
    })?;
    let open_items = string_list(arguments, "open_items");

    let (note, _) =
        reporting::deliver_own_report(state, &session.id, summary.to_string(), status, open_items)
            .await?;
    Ok(json!({ "note": note }))
}

#[cfg(test)]
mod tests;
