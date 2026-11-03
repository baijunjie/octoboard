//! The requests the daemon holds: which are waiting for the user's answer, how the last few that
//! stopped waiting ended, and which approvals are being carried out.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Result};
use serde_json::Value;
use tokio::sync::oneshot;

use crate::protocol::SessionRequestEnding;
use crate::state::AppState;

use super::request::Request;

/// How many ended requests are remembered, for telling a late answer what became of its request.
/// Far more than could be on screen at once; the rest are forgotten oldest first.
const REMEMBERED_ENDINGS: usize = 64;

/// What the waiting call is handed when its request is answered: the tool's result, or the reason
/// it failed, written for the model.
pub(super) type Outcome = std::result::Result<Value, String>;

/// The requests waiting for the user's answer, and how the last few that stopped waiting ended.
#[derive(Default)]
pub struct SessionRequests {
    inner: Mutex<Requests>,
}

#[derive(Default)]
pub(super) struct Requests {
    /// Oldest first.
    pub(super) waiting: Vec<Waiting>,
    ended: VecDeque<Ended>,
    /// The requests that have been approved and are being carried out, by `Request::approval_slot`.
    /// Such a request is not waiting, and asking again meanwhile would have it carried out twice.
    pub(super) approving: HashSet<String>,
}

pub(super) struct Waiting {
    pub(super) request: Request,
    /// Dropped without a value when the request is withdrawn, which is what wakes the call.
    pub(super) answer: oneshot::Sender<Outcome>,
}

struct Ended {
    request_id: String,
    session: String,
    how: SessionRequestEnding,
}

impl Requests {
    /// Makes room for `request`: refused while an approval of its earlier one is being carried
    /// out, else the request it takes the place of (`Request::replaces`) is withdrawn and
    /// returned, since a session asking again has stopped waiting for its earlier call.
    fn make_room_for(&mut self, request: &Request) -> Result<Vec<Waiting>> {
        if self.approving.contains(&request.approval_slot()) {
            bail!(
                "this session's earlier request has just been approved, and it is being carried \
                 out; its outcome arrives in this terminal as a message"
            );
        }
        Ok(self.take(
            |waiting| request.replaces(waiting),
            SessionRequestEnding::Withdrawn,
        ))
    }

    /// Marks `request` as being approved until the returned guard is dropped.
    fn start_approving(&mut self, state: &Arc<AppState>, request: &Request) -> Approving {
        let slot = request.approval_slot();
        self.approving.insert(slot.clone());
        Approving {
            state: state.clone(),
            slot,
        }
    }

    /// Takes the requests `matches` picks out of the waiting ones, recording how they ended.
    fn take(
        &mut self,
        matches: impl Fn(&Request) -> bool,
        how: SessionRequestEnding,
    ) -> Vec<Waiting> {
        let (taken, kept) = std::mem::take(&mut self.waiting)
            .into_iter()
            .partition(|waiting| matches(&waiting.request));
        self.waiting = kept;
        for waiting in &taken {
            if self.ended.len() == REMEMBERED_ENDINGS {
                self.ended.pop_front();
            }
            self.ended.push_back(Ended {
                request_id: waiting.request.id.clone(),
                session: waiting.request.session.clone(),
                how,
            });
        }
        taken
    }
}

impl SessionRequests {
    pub(super) fn lock(&self) -> std::sync::MutexGuard<'_, Requests> {
        self.inner.lock().expect("session requests lock poisoned")
    }

    /// Holds a new request, in place of the one it replaces, which is withdrawn and returned: a
    /// session asking again has stopped waiting for its earlier call, which its agent may have
    /// given up on by a time limit of its own without a word to Octoboard.
    /// Refused while an approval of its session's earlier request is being carried out.
    pub(super) fn hold(
        &self,
        request: Request,
        answer: oneshot::Sender<Outcome>,
    ) -> Result<Vec<Waiting>> {
        let mut requests = self.lock();
        let replaced = requests.make_room_for(&request)?;
        requests.waiting.push(Waiting { request, answer });
        Ok(replaced)
    }

    /// Marks a request approved without ever waiting: the session whose request it is takes the
    /// place of one it already has waiting, which is withdrawn and returned, and is marked as being
    /// approved until the returned guard is dropped. Refused as [`Self::hold`] refuses.
    pub(super) fn approve_directly(
        &self,
        state: &Arc<AppState>,
        request: &Request,
    ) -> Result<(Vec<Waiting>, Approving)> {
        let mut requests = self.lock();
        let replaced = requests.make_room_for(request)?;
        let approving = requests.start_approving(state, request);
        Ok((replaced, approving))
    }

    pub(super) fn take(
        &self,
        matches: impl Fn(&Request) -> bool,
        how: SessionRequestEnding,
    ) -> Vec<Waiting> {
        self.lock().take(matches, how)
    }

    /// Whether the session has a request waiting.
    pub fn is_waiting_for(&self, session_id: &str) -> bool {
        self.lock()
            .waiting
            .iter()
            .any(|waiting| waiting.request.session == session_id)
    }

    /// Takes one request for the user's answer. An approval also marks the request's session as
    /// being approved, in the same step, until the returned guard is dropped.
    pub(super) fn take_answered(
        &self,
        state: &Arc<AppState>,
        request_id: &str,
        how: SessionRequestEnding,
    ) -> Option<(Waiting, Option<Approving>)> {
        let mut requests = self.lock();
        let waiting = requests
            .take(|request| request.id == request_id, how)
            .pop()?;
        let approving = (how == SessionRequestEnding::Approved)
            .then(|| requests.start_approving(state, &waiting.request));
        Some((waiting, approving))
    }

    /// How a request that is no longer waiting ended and whose it was, if it is still remembered.
    pub(super) fn ending(&self, request_id: &str) -> Option<(String, SessionRequestEnding)> {
        self.lock()
            .ended
            .iter()
            .find(|ended| ended.request_id == request_id)
            .map(|ended| (ended.session.clone(), ended.how))
    }
}

/// An approval being carried out, released on drop however carrying it out ends.
pub(super) struct Approving {
    state: Arc<AppState>,
    slot: String,
}

impl Drop for Approving {
    fn drop(&mut self) {
        self.state
            .session_requests
            .lock()
            .approving
            .remove(&self.slot);
    }
}
