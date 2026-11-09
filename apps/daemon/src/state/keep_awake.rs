//! Whether the daemon is holding its host awake (Prevent sleep), which `crate::keep_awake`
//! decides and clients are told.

use std::sync::atomic::Ordering;

use super::AppState;
use crate::protocol::Event;

impl AppState {
    /// Whether the host is held awake right now; what the `snapshot` reports as `keep_awake_held`.
    pub fn keep_awake_held(&self) -> bool {
        self.keep_awake_held.load(Ordering::Acquire)
    }

    /// Records whether the host is held awake and tells every client when that changed.
    pub fn set_keep_awake_held(&self, held: bool) {
        if self.keep_awake_held.swap(held, Ordering::AcqRel) != held {
            self.broadcast(Event::KeepAwakeHeldUpdated {
                keep_awake_held: held,
            });
        }
    }
}
