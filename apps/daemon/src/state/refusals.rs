//! The refusals every operation on a session shares: a locked session in its scope, and an archived
//! session.

use anyhow::Result;

use super::session_archived;
use crate::protocol::{error_code, CodedError, Session, SessionStatus};

/// Refuses an operation whose scope includes a locked session, naming the first.
pub(crate) fn ensure_unlocked(sessions: impl IntoIterator<Item = Session>) -> Result<()> {
    if let Some(session) = sessions.into_iter().find(|session| session.locked) {
        return Err(CodedError::raised(
            error_code::SESSION_LOCKED,
            "this operation includes a locked session; unlock it first",
            &[("session", &session.id)],
        ));
    }
    Ok(())
}

/// Refuses an archived session, which a switch is refused for, before the user is asked to approve
/// one.
pub(crate) fn refuse_if_archived(session: &Session) -> Result<()> {
    if session.status == SessionStatus::Archived {
        return Err(session_archived(&session.id));
    }
    Ok(())
}
