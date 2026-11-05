//! Waiting for a session or a condition from a blocking thread: polling a predicate, and waiting
//! for a session's output to go quiet.

use std::time::{Duration, Instant};

use crate::platform::term;
use crate::session::LiveSession;

/// How long a session's output has to stay silent to count as quiet: the span [`wait_quiet`]
/// watches for, which is what the terminal backend says it takes to have finished drawing.
pub(super) fn quiet() -> Duration {
    term::facts().settle_window
}

/// How often [`wait_for`] looks again.
pub(crate) const POLL: Duration = Duration::from_millis(25);

/// Waits until the session has printed nothing for [`quiet`], or `timeout` passes.
pub(crate) fn wait_quiet(live: &LiveSession, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        let before = live.output_total();
        std::thread::sleep(quiet());
        if live.output_total() == before {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
    }
}

/// Polls `done` until it holds or `timeout` passes.
pub(crate) fn wait_for(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(POLL);
    }
}
