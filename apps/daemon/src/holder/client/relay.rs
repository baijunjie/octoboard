//! The requests holders relay from their helpers: what the daemon is asked to handle, and the
//! calls in flight on one link.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use tokio::task::{AbortHandle, JoinHandle};

use super::link::locked;
use crate::holder::protocol::RelayRoute;

/// A helper's request, as the daemon's HTTP route of the same name takes it.
#[derive(Debug, Clone)]
pub struct RelayRequest {
    pub route: RelayRoute,
    /// The path's parameter: the session id, or for the MCP route its token.
    pub param: String,
    pub body: Vec<u8>,
}

/// What the HTTP route would have answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayReply {
    pub status: u16,
    pub body: String,
}

/// What handles the requests the holders relay. The holder has checked the launch's key, so a
/// request is trusted as coming from the session its parameter names.
pub trait RelayHandler: Send + Sync {
    /// Handles a report (`request`, which nobody waits on) before it returns, so that reports are
    /// handled in the order the holder sent them. It must not wait on anything, as it is called on
    /// the link's reader.
    fn report(&self, request: RelayRequest);

    /// Starts handling `request`, a question a helper waits on, on a runtime of the handler's own.
    /// Aborting the task stops the call, as the connection of an HTTP call closing does; dropping
    /// it lets the call run on.
    fn relay(&self, request: RelayRequest) -> JoinHandle<RelayReply>;
}

/// The calls of one link that are running and have a helper waiting on them.
#[derive(Default)]
pub(super) struct Relaying {
    running: Mutex<HashMap<u64, AbortHandle>>,
}

impl Relaying {
    fn running(&self) -> MutexGuard<'_, HashMap<u64, AbortHandle>> {
        locked(&self.running)
    }

    /// Registers the call `req`, so that it can be stopped.
    pub(super) fn begin(&self, req: u64, call: &JoinHandle<RelayReply>) {
        self.running().insert(req, call.abort_handle());
    }

    /// The call `req` has an answer to send, or none left to give.
    pub(super) fn end(&self, req: u64) {
        self.running().remove(&req);
    }

    /// The helper of the call `req` is gone.
    pub(super) fn cancel(&self, req: u64) {
        if let Some(call) = self.running().remove(&req) {
            call.abort();
        }
    }

    /// The link is lost: every call it carried is stopped, since its helper was failed.
    pub(super) fn cancel_all(&self) {
        for (_, call) in self.running().drain() {
            call.abort();
        }
    }
}
