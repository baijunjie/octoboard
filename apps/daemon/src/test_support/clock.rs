//! Clock helpers for runtime progress and distinct wall-clock timestamps.

use std::time::Duration;

/// Returns once every task on the runtime has nothing left to do, for a check that something did
/// not happen, which has no event to wait for. Only on a paused clock
/// (`#[tokio::test(start_paused = true)]`): that clock moves on to the next timer only when the
/// runtime is idle, so this sleep ends after everything already able to run has run, however
/// slowly the machine runs it. A task in `spawn_blocking` holds the clock until it ends, so one
/// the test keeps from ending keeps this from returning; work on a thread of its own outside the
/// runtime, and a task waiting on I/O, are not waited for.
pub(crate) async fn tasks_idle() {
    tokio::time::sleep(Duration::from_millis(1)).await;
}

/// [`tasks_idle`] for a test that otherwise runs on the real clock: pauses the clock for the wait
/// and lets it run again after. Only a timer due within the wait fires early. For a
/// current-thread runtime (a plain `#[tokio::test]`); panics on a clock that is paused already.
pub(crate) async fn runtime_settled() {
    runtime_advanced(Duration::from_millis(1)).await;
}

/// Moves the clock of a test that otherwise runs on the real clock on by `by`, as
/// [`runtime_settled`] does: every timer due within it fires, and what it sets off runs until the
/// runtime is idle, before the clock moves on to the next one and before this returns.
pub(crate) async fn runtime_advanced(by: Duration) {
    tokio::time::pause();
    tokio::time::sleep(by).await;
    tokio::time::resume();
}

/// Waits until the wall-clock millisecond has moved past the one read at entry. Production stamps
/// records with wall-clock milliseconds, so tests that need distinct stamps must let that clock
/// move on; advancing tokio's clock cannot do so. This named real-time wait needs a running clock.
pub(crate) async fn next_millisecond() {
    let earlier = crate::time::now_millis();
    while crate::time::now_millis() <= earlier {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}
