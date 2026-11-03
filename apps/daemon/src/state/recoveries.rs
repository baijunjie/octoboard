//! Ending the waits a session can have for a recovery of its own: the automatic account switch's
//! wait for quota (`crate::coordinator::auto_switch`) and the wait to retry a turn that stopped at
//! its model's capacity (`crate::messaging::capacity_retry`). What invalidates one invalidates the
//! other, and they differ only in what the retry does for its owner when it is ended.

use super::AppState;
use crate::messaging::capacity_retry::Cause;

impl AppState {
    /// Ends the session `id`'s waits because of something done to its process, its record or its
    /// turn that opens no turn of its own (a launch, an account or model change, an archive, a
    /// delete, an interrupt). A retry after a capacity failure gives the owner the failed report it
    /// held back.
    pub(crate) fn cancel_recoveries(&self, id: &str) {
        self.capacity_retries.cancel(id, Cause::Report);
        self.quota_recovery.cancel(id);
    }

    /// [`Self::cancel_recoveries`] for a turn that starts, or a message queued for one: the retry's
    /// owner is not told anything, since the session is at work again and reports for itself.
    pub(crate) fn cancel_recoveries_silently(&self, id: &str) {
        self.capacity_retries.cancel(id, Cause::Silent);
        self.quota_recovery.cancel(id);
    }
}
