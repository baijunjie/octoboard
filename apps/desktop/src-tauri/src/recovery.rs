//! Bringing the daemon back after it ended without being asked to: the decisions, and the thread
//! that makes them.
//!
//! The decisions are small pure functions (`exit_action`, `RestartBudget`, `restart_backoff`,
//! `rebuild_query`, `budget_spent_detail`) so the rules can be tested without a process. A daemon
//! that keeps dying must not be restarted forever: at most `MAX_RESTARTS` relaunches in any
//! `RESTART_WINDOW`, after which the window is told, through the same `daemon-exited` event as any
//! daemon that ended for good, and the user restarts the application or reports the issue.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use crate::diagnostics::diagnostic;
use crate::sidecar::{
    daemon_params, mark_daemon_down, start_attempt, wait_for, DaemonAddress, DaemonState,
    DAEMON_PARAMS,
};
use crate::MAIN_WINDOW_LABEL;

/// How many times the daemon is relaunched within `RESTART_WINDOW` before it is left alone.
const MAX_RESTARTS: usize = 3;

/// The period `MAX_RESTARTS` is counted over.
const RESTART_WINDOW: Duration = Duration::from_secs(10 * 60);

/// What the shell does about a daemon that ended.
#[derive(Debug, PartialEq)]
pub enum ExitAction {
    /// The daemon died on its own: start it again.
    Recover,
    /// The daemon went down as asked, or as part of a restart or quit: say so and leave it.
    Stopped,
}

/// Exit code 0 is a requested shutdown (the UI's quit flow sends `shutdown` before it confirms the
/// quit, and the daemon exits 0 on SIGTERM as well), and nothing is started once a restart or quit
/// is under way. Every other end, a crash or a signal, is recoverable.
pub fn exit_action(stopping: bool, code: Option<i32>) -> ExitAction {
    if stopping || code == Some(0) {
        ExitAction::Stopped
    } else {
        ExitAction::Recover
    }
}

/// When the daemon was relaunched, to keep the relaunches within `MAX_RESTARTS` per
/// `RESTART_WINDOW`.
#[derive(Default)]
pub struct RestartBudget {
    restarts: Vec<Instant>,
}

impl RestartBudget {
    /// Counts a relaunch made at `now` and returns how many were already counted within the window
    /// before it. When the window is already full nothing is counted, and the number of
    /// relaunches in it comes back as the error.
    pub fn take(&mut self, now: Instant) -> Result<usize, usize> {
        self.restarts
            .retain(|at| now.saturating_duration_since(*at) < RESTART_WINDOW);
        let recent = self.restarts.len();
        if recent >= MAX_RESTARTS {
            return Err(recent);
        }
        self.restarts.push(now);
        Ok(recent)
    }
}

/// How long a relaunch waits first, given how many were made recently: 1 s, 2 s, 4 s.
pub fn restart_backoff(recent: usize) -> Duration {
    Duration::from_secs(1 << recent.min(4))
}

/// What the window is told once the daemon is no longer restarted. `ended` says how it ended.
pub fn budget_spent_detail(ended: &str, restarts: usize) -> String {
    format!(
        "{ended}. It was restarted {restarts} times in the last ten minutes and is not restarted again."
    )
}

/// The window's query once it is pointed at a restarted daemon: `daemon_params` in place of what
/// `current` held of the daemon's (its port and key, or the error a window opened without one got),
/// the rest of it (the languages) kept as it was, and `recovered=1` added so the page knows it was
/// reloaded rather than opened (`resolveRecovered` in `packages/ui/src/daemon.ts`).
pub fn rebuild_query(current: Option<&str>, daemon_params: &str) -> String {
    let kept = current
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter(|pair| {
            let name = pair.split_once('=').map_or(*pair, |(name, _)| name);
            !DAEMON_PARAMS.contains(&name) && name != "recovered"
        });
    std::iter::once(daemon_params)
        .chain(kept)
        .chain(std::iter::once("recovered=1"))
        .collect::<Vec<_>>()
        .join("&")
}

/// Brings the daemon back on a thread of its own, after it ended as described by `ended`. The
/// daemon is marked down first, so a quit in the meantime asks nothing about sessions that are gone
/// and a window opened meanwhile says why there is no daemon yet. The thread is counted before it
/// is spawned, so a restart that begins right after this returns waits for it.
pub fn recover(app: &AppHandle, ended: String) {
    mark_daemon_down(app, format!("{ended}. It is being restarted."));
    app.state::<DaemonState>()
        .recovering
        .fetch_add(1, Ordering::SeqCst);
    let app = app.clone();
    std::thread::spawn(move || {
        run(&app, ended);
        app.state::<DaemonState>()
            .recovering
            .fetch_sub(1, Ordering::SeqCst);
    });
}

fn run(app: &AppHandle, mut ended: String) {
    let state = app.state::<DaemonState>();
    let stopping = || state.stopping.load(Ordering::SeqCst);
    while !stopping() {
        let taken = state
            .restarts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take(Instant::now());
        let recent = match taken {
            Ok(recent) => recent,
            Err(restarts) => {
                let detail = budget_spent_detail(&ended, restarts);
                diagnostic!("giving up on the daemon: {detail}");
                mark_daemon_down(app, detail.clone());
                let _ = app.emit("daemon-exited", detail);
                return;
            }
        };
        let backoff = restart_backoff(recent);
        diagnostic!(
            "restarting the daemon in {} s ({} relaunches in the last ten minutes before this one)",
            backoff.as_secs(),
            recent
        );
        if wait_for(backoff, stopping) {
            return;
        }
        match start_attempt(app) {
            Ok(address) => {
                diagnostic!(
                    "the daemon was restarted and listens on port {}",
                    address.port
                );
                point_window_at(app, &address);
                return;
            }
            Err(failure) => {
                diagnostic!("restarting the daemon failed: {failure}");
                ended = format!("The daemon could not be restarted: {failure}");
            }
        }
    }
}

/// Navigates the main window to its current URL with its query rebuilt for the daemon at `address`.
/// With no window yet there is nothing to do: one opened later is built from the current address.
pub fn point_window_at(app: &AppHandle, address: &DaemonAddress) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        diagnostic!("there is no window yet to point at the restarted daemon");
        return;
    };
    let result = window.url().and_then(|mut url| {
        let query = rebuild_query(url.query(), &daemon_params(address));
        url.set_query(Some(&query));
        window.navigate(url)
    });
    if let Err(err) = result {
        diagnostic!("cannot point the window at the restarted daemon: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_unrequested_end_is_recovered() {
        assert_eq!(exit_action(false, Some(1)), ExitAction::Recover);
        assert_eq!(exit_action(false, None), ExitAction::Recover);
        assert_eq!(exit_action(false, Some(0)), ExitAction::Stopped);
        assert_eq!(exit_action(true, Some(1)), ExitAction::Stopped);
        assert_eq!(exit_action(true, None), ExitAction::Stopped);
    }

    #[test]
    fn the_budget_allows_three_relaunches_in_ten_minutes() {
        let start = Instant::now();
        let at = |seconds| start + Duration::from_secs(seconds);
        let mut budget = RestartBudget::default();
        assert_eq!(budget.take(at(0)), Ok(0));
        assert_eq!(budget.take(at(10)), Ok(1));
        assert_eq!(budget.take(at(20)), Ok(2));
        assert_eq!(budget.take(at(30)), Err(3));
        // A refused relaunch is not counted: the first one leaves the window at 600 s.
        assert_eq!(budget.take(at(599)), Err(3));
        assert_eq!(budget.take(at(600)), Ok(2));
        assert_eq!(budget.take(at(601)), Err(3));
    }

    #[test]
    fn the_backoff_grows_with_the_recent_relaunches() {
        let seconds = |recent| restart_backoff(recent).as_secs();
        assert_eq!([seconds(0), seconds(1), seconds(2)], [1, 2, 4]);
    }

    #[test]
    fn the_query_is_rebuilt_around_the_new_address() {
        assert_eq!(
            rebuild_query(Some("port=1&key=aa&languages=ja%2Cen"), "port=2&key=bb"),
            "port=2&key=bb&languages=ja%2Cen&recovered=1"
        );
        // A second recovery replaces the first one's marker rather than adding another.
        assert_eq!(
            rebuild_query(
                Some("languages=en&port=2&key=bb&recovered=1"),
                "port=3&key=cc"
            ),
            "port=3&key=cc&languages=en&recovered=1"
        );
        assert_eq!(
            rebuild_query(None, "port=3&key=cc"),
            "port=3&key=cc&recovered=1"
        );
        // A window opened while there was no daemon carries its error instead of an address.
        assert_eq!(
            rebuild_query(Some("error=gone&languages=en"), "port=4&key=dd"),
            "port=4&key=dd&languages=en&recovered=1"
        );
    }

    #[test]
    fn the_detail_says_how_often_the_daemon_was_restarted() {
        assert_eq!(
            budget_spent_detail("The daemon exited with exit code 1", 3),
            "The daemon exited with exit code 1. It was restarted 3 times in the last ten minutes and is not restarted again."
        );
    }
}
