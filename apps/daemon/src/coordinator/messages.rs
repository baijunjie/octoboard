//! The user's own write of a message into a session.

use std::sync::Arc;

use anyhow::Result;

use crate::messaging::delivery;
use crate::state::AppState;

/// The user's own write into a session. Refused rather than queued while the session is waiting for
/// them: they are the one who has to answer the prompt that is blocking it, and they can be told so.
/// Off the runtime, because the write blocks on the PTY and on the wait for the agent to send it.
pub(super) async fn send_message(state: &Arc<AppState>, id: &str, text: String) -> Result<()> {
    delivery::write_message_off_runtime(state, id, text, delivery::WhenBlocked::Refuse).await?;
    Ok(())
}
