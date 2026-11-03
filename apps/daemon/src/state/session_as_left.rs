//! A session as a failed turn left it, to tell later whether it is still as it was: the process,
//! how much was written into it, how many turns ended, and the record's account, model and
//! conversation. What a wait for a recovery of the session is allowed to act on.

use std::ops::Deref;
use std::sync::Arc;

use anyhow::Result;

use super::AppState;
use crate::protocol::Session;
use crate::session::LiveSession;

#[derive(Clone)]
pub(crate) struct SessionAsLeft {
    record: Session,
    live: Arc<LiveSession>,
    input_writes: u64,
    turn_ends: u64,
}

impl Deref for SessionAsLeft {
    type Target = Session;

    fn deref(&self) -> &Session {
        &self.record
    }
}

impl SessionAsLeft {
    /// The session `id` as it stands, or `None` when it has no process.
    pub(crate) fn capture(state: &AppState, id: &str) -> Result<Option<Self>> {
        let Some(live) = state.live_session(id) else {
            return Ok(None);
        };
        let record = state.session_record(id)?;
        Ok(Some(Self {
            input_writes: live.input_writes(),
            turn_ends: state.turn_ends(id),
            record,
            live,
        }))
    }

    /// The record as it stands now, when the session is still the same process with nothing
    /// written into it and no turn ended since, on the same account, model, effort and
    /// conversation. Its status is for the caller to judge.
    pub(crate) fn unchanged(&self, state: &AppState) -> Option<Session> {
        let id = &self.record.id;
        let same_process = state
            .live_session(id)
            .is_some_and(|live| Arc::ptr_eq(&live, &self.live))
            && !self.live.poll_exit()
            && self.live.input_writes() == self.input_writes
            && state.turn_ends(id) == self.turn_ends;
        if !same_process {
            return None;
        }
        let now = state.store.get_session(id).ok().flatten()?;
        (now.account_id == self.record.account_id
            && now.config_dir == self.record.config_dir
            && now.agent_session_id == self.record.agent_session_id
            && now.model == self.record.model
            && now.effort == self.record.effort)
            .then_some(now)
    }
}
