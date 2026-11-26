//! The changes of sharing at work on the account's directory, which the daemon's stop waits for.
//! A change cut short there by the daemon's exit leaves the directory half-changed: an entry
//! neither linked nor restored, a move across volumes half done. Nothing finishes it but the
//! user asking again, which no one can do once Octoboard is uninstalled right after its
//! clean-up has stopped the daemon. So no change begins on disk once the daemon has begun to
//! stop, and its stop waits, with no deadline of its own, for one that began before.

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::watch;

use crate::state::{shutting_down_refusal, AppState};

/// Where the changes on disk stand.
#[derive(Debug, Default)]
struct Steps {
    /// The daemon has begun to stop: no change begins on disk from then on.
    closed: bool,
    /// The changes at work on disk now.
    running: usize,
}

/// The changes of sharing at work on disk, as [`super::Sharing`] holds them.
pub struct OnDisk(Arc<watch::Sender<Steps>>);

impl Default for OnDisk {
    fn default() -> Self {
        Self(Arc::new(watch::channel(Steps::default()).0))
    }
}

/// A change at work on disk, until dropped.
#[must_use = "the daemon's stop waits for the change only while this is held"]
pub(crate) struct OnDiskStep(Arc<watch::Sender<Steps>>);

impl Drop for OnDiskStep {
    fn drop(&mut self) {
        self.0
            .send_modify(|steps| steps.running = steps.running.saturating_sub(1));
    }
}

/// Lets a change of sharing begin on the account's directory, for as long as the answer is held;
/// refused with `daemon_shutting_down` once the daemon has been asked to stop or has begun to.
/// Taken before the change records anything, so that a refused change leaves the account as it
/// was.
pub(crate) fn begin_on_disk(state: &AppState) -> Result<OnDiskStep> {
    if state.stop_asked() {
        return Err(shutting_down_refusal());
    }
    let steps = &state.account_sharing.on_disk.0;
    let begun = steps.send_if_modified(|steps| {
        if steps.closed {
            return false;
        }
        steps.running += 1;
        true
    });
    if !begun {
        return Err(shutting_down_refusal());
    }
    Ok(OnDiskStep(steps.clone()))
}

/// Refuses every change of sharing that has not begun on disk yet, as the daemon begins to stop.
pub fn close_changes_on_disk(state: &AppState) {
    state
        .account_sharing
        .on_disk
        .0
        .send_modify(|steps| steps.closed = true);
}

/// Waits until no change of sharing is at work on disk, once [`close_changes_on_disk`] has
/// closed them: what the daemon's stop awaits before the daemon exits.
pub async fn changes_on_disk_ended(state: &AppState) {
    let mut steps = state.account_sharing.on_disk.0.subscribe();
    let running = steps.borrow().running;
    if running == 0 {
        return;
    }
    tracing::info!(
        running,
        "waiting for a change of sharing to finish on the account's directory before stopping"
    );
    let _ = steps.wait_for(|steps| steps.running == 0).await;
    tracing::info!("the changes of sharing have finished on disk");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::error_code;
    use crate::test_support::{app_state, code_of, tasks_idle};

    /// Once closed, no change begins on disk, refused as a stopping daemon refuses; the stop
    /// waits for the one at work and ends as soon as it is done.
    #[tokio::test(start_paused = true)]
    async fn the_stop_waits_for_a_change_at_work_and_admits_no_new_one() {
        let (state, _dir) = app_state("sharing-on-disk-wait");
        let step = begin_on_disk(&state).expect("a change begins before the stop");
        close_changes_on_disk(&state);
        let err = begin_on_disk(&state)
            .err()
            .expect("no change begins once closed");
        assert_eq!(code_of(&err), error_code::DAEMON_SHUTTING_DOWN);
        assert!(format!("{err:#}").contains("shutting down"), "{err:#}");

        let waiting = tokio::spawn({
            let state = state.clone();
            async move { changes_on_disk_ended(&state).await }
        });
        tasks_idle().await;
        assert!(!waiting.is_finished(), "the stop waits for the change");
        drop(step);
        waiting.await.unwrap();
    }

    /// A stop asked for refuses a change before main has closed them.
    #[tokio::test]
    async fn a_stop_asked_for_refuses_a_change() {
        let (state, _dir) = app_state("sharing-on-disk-asked");
        state.request_shutdown();
        let err = begin_on_disk(&state)
            .err()
            .expect("refused once a stop is asked");
        assert_eq!(code_of(&err), error_code::DAEMON_SHUTTING_DOWN);
    }
}
