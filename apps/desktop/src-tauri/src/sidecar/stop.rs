//! Stopping the `octoboardd` sidecar for a restart or a quit: the ordinary stop that interrupts the
//! sessions, and the planned one that hands them over to the next launch. Telling it the
//! application is quitting is `quit_note.rs`; what happens when it ends by itself is `recovery.rs`.

use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

use super::quit_note::{note_session_end, planned_stop_under_way};
use super::{
    current_address, daemon_stopped, mark_daemon_stopping, wait_for, wait_for_recoveries,
    DaemonAddress, DaemonState,
};
use crate::diagnostics::diagnostic;

/// How the daemon is asked to stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopKind {
    /// Every session's process is ended and the sessions are left interrupted: a quit.
    Ordinary,
    /// Every terminal is handed over to its process holder and nothing is interrupted: an update
    /// restart, whose next launch adopts the holders. Never asked by signal, which is an ordinary
    /// stop.
    Planned,
}

impl StopKind {
    /// The path of the daemon's request for this kind of stop.
    fn endpoint(self) -> &'static str {
        match self {
            Self::Ordinary => "shutdown",
            Self::Planned => "planned-stop",
        }
    }
}

/// How long a restart waits for the daemon to exit on its own after the UI asked it to stop, before
/// it sends its own request.
const DAEMON_EXIT_GRACE: Duration = Duration::from_secs(2);

/// How long the stop request itself may take, connecting, sending and reading the answer, all
/// together.
const STOP_REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

/// How long a planned stop waits after a request that could not be sent before it sends it once
/// more.
const PLANNED_RETRY_PAUSE: Duration = Duration::from_secs(1);

/// How long a planned stop waits for the daemon to exit after the request before it kills it. The
/// daemon answers the request at once and then saves the screens before it parts from the holders
/// (`coordinator/planned_stop.rs`). After the barriers of the planned restart handover its phases
/// are, nominally: the handover barrier waiting for the operations admitted (up to 5 seconds,
/// `HANDOVER_BARRIER_WAIT`), the hook events and status reports settling (0.5), a holder link being
/// reopened (up to 15, `Link::settled`: `CONNECT_DEADLINE` of 10 and 5 more), the holders' answer
/// to the handover (5, `HANDOVER_DEADLINE`), the readers reaching their marks (3, `READER_WAIT`)
/// and the parting, now from every holder at once (3, `PART_DEADLINE`): 31.5 seconds as a sum of
/// explicit deadlines, which no run is likely to use up in full and which is no measured maximum,
/// as scheduling, file writes and the account logins' stop add to it. The planned stop begins at
/// the UI's `planned_stop` request; `wait_for_recoveries` (up to 15 seconds) can precede the
/// escalation, whose own steps come before this wait (the exit grace of 2 seconds, then the
/// request, which may take 2, repeated after a pause of 1 when it could not be sent), so the kill
/// comes at least 32 seconds after the request (more where the recoveries are waited for) and at
/// most about 37 otherwise, roughly the nominal worst case, with no margin to speak of beyond it. A
/// kill before the screens are saved degrades to crash recovery: the holders keep running, as after
/// a crash. A longer wait would keep a restart that has nothing to show for it hanging.
const PLANNED_STOP_GRACE: Duration = Duration::from_secs(30);
const _: () = assert!(DAEMON_EXIT_GRACE.as_secs() + PLANNED_STOP_GRACE.as_secs() >= 32);
const _: () = assert!(
    DAEMON_EXIT_GRACE.as_secs()
        + 2 * STOP_REQUEST_TIMEOUT.as_secs()
        + PLANNED_RETRY_PAUSE.as_secs()
        + PLANNED_STOP_GRACE.as_secs()
        <= 37
);

/// How long the request of the stop at the end of the user's session (`stop_daemon_at_session_end`)
/// may take: the system gives the application only a few seconds in all.
const SESSION_END_REQUEST_TIMEOUT: Duration = Duration::from_secs(1);

/// How long the stop at the end of the user's session waits for the daemon to be gone after killing
/// it.
const SESSION_END_KILL_WAIT: Duration = Duration::from_millis(500);

/// How long a restart waits for the daemon to exit after the ordinary shutdown request, or SIGTERM
/// where the request could not be made, either of which has it stop its sessions' processes first,
/// before it kills it.
const DAEMON_TERM_GRACE: Duration = Duration::from_secs(5);

/// How long a restart waits for the daemon to be gone after killing it.
const DAEMON_KILL_WAIT: Duration = Duration::from_secs(2);

/// How long a restart waits for the recoveries under way to notice it and end the daemon they
/// started.
const RECOVERY_END_WAIT: Duration = Duration::from_secs(15);

/// Makes sure the daemon has exited before the application goes on: a restart launches a new
/// instance, whose own daemon refuses to start while this one still holds the instance lock
/// (`instance_lock` in the daemon), and a quit that installs an update replaces the daemon's
/// executable. The UI has asked it to stop before confirming, so this normally returns at once or
/// within moments. One that has not exited within `DAEMON_EXIT_GRACE` is sent the request of `kind`
/// (`POST /shutdown` or `POST /planned-stop`, which the daemon handles like the UI's).
///
/// For `StopKind::Ordinary` one that could not be sent it gets SIGTERM on macOS instead (Windows
/// has no such signal, so the request is the only ask), and one still running `DAEMON_TERM_GRACE`
/// after either is killed (`TerminateProcess` on Windows). For `StopKind::Planned` the request is
/// repeated once when it could not be sent, never replaced by a signal, and the daemon is killed
/// `PLANNED_STOP_GRACE` after it. Blocks the calling thread: call it off the main thread.
pub fn stop_daemon(app: &AppHandle, kind: StopKind) {
    mark_daemon_stopping(app);
    // A recovery under way sees the flag and ends the daemon it was starting; the daemon is not
    // stopped until it has.
    wait_for_recoveries(app, RECOVERY_END_WAIT);
    match kind {
        StopKind::Ordinary => escalate_stop(
            || daemon_stopped(app),
            || request_stop_of(app, kind, STOP_REQUEST_TIMEOUT),
            || terminate_child(app),
            || kill_child(app),
            [DAEMON_EXIT_GRACE, DAEMON_TERM_GRACE, DAEMON_KILL_WAIT],
        ),
        StopKind::Planned => escalate_planned_stop(
            || daemon_stopped(app),
            // A session end that began meanwhile has the daemon's planned stop made ordinary, or
            // ends it itself (`stop_daemon_at_session_end`): no planned request is sent after it,
            // or it would start a planned stop where the end has asked for none. Nothing else
            // changes: the wait goes on, and no signal or plain shutdown comes from here.
            || !planned_stop_under_way(app) || request_stop_of(app, kind, STOP_REQUEST_TIMEOUT),
            || kill_child(app),
            [
                DAEMON_EXIT_GRACE,
                PLANNED_RETRY_PAUSE,
                PLANNED_STOP_GRACE,
                DAEMON_KILL_WAIT,
            ],
        ),
    }
}

/// What the stop at the end of the user's session does about the daemon.
#[derive(Debug, PartialEq, Eq)]
enum SessionEndStop {
    /// The ordinary stop: the request, the signal where it was not accepted, then the kill.
    Ordinary,
    /// An update restart's planned stop is under way, or about to be: the UI's request for it may
    /// not have reached the daemon yet. The daemon has closed its HTTP listener once main has begun
    /// any stop, so `POST /shutdown` mostly fails fast, but where the planned request never arrived
    /// it is the ordinary stop. So the signal first (macOS; on Windows the quit note is the one
    /// notice, which the daemon answers by making a planned stop ordinary), then the request, then
    /// the kill at the end of the budget. A planned stop asked for after the note is made ordinary
    /// by the daemon too.
    PlannedUnderWay,
}

fn session_end_stop(planned_stop_under_way: bool) -> SessionEndStop {
    if planned_stop_under_way {
        SessionEndStop::PlannedUnderWay
    } else {
        SessionEndStop::Ordinary
    }
}

/// The stop of the daemon when the system ends the user's session (sign-out, restart or shutdown),
/// which gives the application no time to wait for the UI and asks nothing: the quit record and
/// note go out at once (`note_session_end`, also over an update restart's planned stop), and a
/// daemon still running is asked to stop and killed in the end (`SessionEndStop`); an update
/// restart's planned stop under way is ended, not waited for. Blocks the calling thread for at most
/// `wait` in all, the request, the wait for the daemon to exit and the wait after the kill included
/// (`session_end_budget`).
pub fn stop_daemon_at_session_end(app: &AppHandle, wait: Duration) {
    match session_end_stop(note_session_end(app)) {
        SessionEndStop::Ordinary => escalate_stop(
            || daemon_stopped(app),
            || request_stop_of(app, StopKind::Ordinary, SESSION_END_REQUEST_TIMEOUT),
            || terminate_child(app),
            || kill_child(app),
            session_end_budget(wait, SESSION_END_REQUEST_TIMEOUT),
        ),
        SessionEndStop::PlannedUnderWay => escalate_planned_session_end(
            || daemon_stopped(app),
            || terminate_child(app),
            || request_stop_of(app, StopKind::Ordinary, SESSION_END_REQUEST_TIMEOUT),
            || kill_child(app),
            session_end_budget(wait, SESSION_END_REQUEST_TIMEOUT),
        ),
    }
}

/// The waits of the stop at a session end, for `wait` in all: none before the request, which takes
/// up to `request_timeout`, then what is left of `wait` after it and the wait that follows the
/// kill, then that wait.
fn session_end_budget(wait: Duration, request_timeout: Duration) -> [Duration; 3] {
    [
        Duration::ZERO,
        wait.saturating_sub(request_timeout + SESSION_END_KILL_WAIT),
        SESSION_END_KILL_WAIT,
    ]
}

/// Sends the daemon at the address it is up at the request of `kind`, which may take `timeout`;
/// false when there is none or it did not accept.
fn request_stop_of(app: &AppHandle, kind: StopKind, timeout: Duration) -> bool {
    match current_address(app) {
        Some(address) => request_stop(&address, kind, timeout),
        None => false,
    }
}

fn terminate_child(app: &AppHandle) {
    if let Some(child) = app
        .state::<DaemonState>()
        .child
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .child
        .as_ref()
    {
        terminate(child.pid());
    }
}

/// Kills the running sidecar and lets go of it.
pub(super) fn kill_child(app: &AppHandle) {
    let child = app
        .state::<DaemonState>()
        .child
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .child
        .take();
    if let Some(child) = child {
        let _ = child.kill();
    }
}

/// The escalation of an ordinary `stop_daemon`: waits for `stopped` for `exit_grace`, then runs
/// `request`, which says whether the daemon accepted it; one it did not accept is followed by
/// `terminate`, which asks the same of the daemon by signal, so either way the daemon is asked
/// once. Then it waits `term_grace`, runs `kill` and waits `kill_wait`. Stops at the first wait
/// that sees `stopped`.
fn escalate_stop(
    stopped: impl Fn() -> bool,
    request: impl FnOnce() -> bool,
    terminate: impl FnOnce(),
    kill: impl FnOnce(),
    [exit_grace, term_grace, kill_wait]: [Duration; 3],
) {
    if wait_for(exit_grace, &stopped) {
        return;
    }
    if !request() {
        terminate();
    }
    if wait_for(term_grace, &stopped) {
        return;
    }
    kill();
    wait_for(kill_wait, &stopped);
}

/// The escalation of the stop at a session end while an update restart's planned stop is under way:
/// waits for `stopped` for `exit_grace`, then runs `terminate` and `request` (the answer does not
/// matter: it fails fast where the daemon no longer listens), waits `term_grace`, runs `kill` and
/// waits `kill_wait`. Stops at the first wait that sees `stopped`.
fn escalate_planned_session_end(
    stopped: impl Fn() -> bool,
    terminate: impl FnOnce(),
    request: impl FnOnce() -> bool,
    kill: impl FnOnce(),
    [exit_grace, term_grace, kill_wait]: [Duration; 3],
) {
    if wait_for(exit_grace, &stopped) {
        return;
    }
    terminate();
    request();
    if wait_for(term_grace, &stopped) {
        return;
    }
    kill();
    wait_for(kill_wait, &stopped);
}

/// The escalation of a planned `stop_daemon`: waits for `stopped` for `exit_grace`, then runs
/// `request`; one that was not accepted is run once more after `retry_pause`, and no signal ever
/// stands in for it, as a signal makes an ordinary stop. Then it waits `saved_grace` for the
/// screens to be saved and the daemon to exit, runs `kill` and waits `kill_wait`. Stops at the
/// first wait that sees `stopped`.
fn escalate_planned_stop(
    stopped: impl Fn() -> bool,
    mut request: impl FnMut() -> bool,
    kill: impl FnOnce(),
    [exit_grace, retry_pause, saved_grace, kill_wait]: [Duration; 4],
) {
    if wait_for(exit_grace, &stopped) {
        return;
    }
    if !request() {
        if wait_for(retry_pause, &stopped) {
            return;
        }
        request();
    }
    if wait_for(saved_grace, &stopped) {
        return;
    }
    kill();
    wait_for(kill_wait, &stopped);
}

/// Asks the daemon at `address` to stop with `POST /shutdown` or `POST /planned-stop`
/// (`StopKind::endpoint`), and says whether it answered that it will. The one HTTP request the
/// shell makes to the daemon, so it is written out over a socket rather than through a client the
/// rest of the shell has no use for. Each of its steps, connecting, sending and reading the answer,
/// gets what is left of `timeout`, so it cannot hold a restart up for longer than that.
fn request_stop(address: &DaemonAddress, kind: StopKind, timeout: Duration) -> bool {
    use std::io::{BufRead, BufReader, Write};
    use std::net::{Ipv4Addr, SocketAddr, TcpStream};

    let deadline = Instant::now() + timeout;
    // A socket timeout of zero is an error, so the last step still gets a moment.
    let left = || {
        deadline
            .saturating_duration_since(Instant::now())
            .max(Duration::from_millis(1))
    };
    let attempt = || -> std::io::Result<bool> {
        let target = SocketAddr::from((Ipv4Addr::LOCALHOST, address.port));
        let mut stream = TcpStream::connect_timeout(&target, left())?;
        stream.set_read_timeout(Some(left()))?;
        stream.set_write_timeout(Some(left()))?;
        write!(
            stream,
            "POST /{}?key={} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            kind.endpoint(),
            address.key,
            address.port
        )?;
        let mut status_line = String::new();
        BufReader::new(stream).read_line(&mut status_line)?;
        Ok(accepted(&status_line))
    };
    match attempt() {
        Ok(accepted) => accepted,
        Err(err) => {
            diagnostic!(
                "the {} request to the daemon failed: {err}",
                kind.endpoint()
            );
            false
        }
    }
}

/// Whether an HTTP status line is a success.
fn accepted(status_line: &str) -> bool {
    status_line
        .split_whitespace()
        .nth(1)
        .is_some_and(|status| status.starts_with('2'))
}

#[cfg(target_os = "macos")]
mod macos {
    /// Sends SIGTERM to `pid`, the signal the daemon's `platform::proc::shutdown_requested` waits
    /// for.
    pub fn terminate(pid: u32) {
        // SAFETY: `kill` only sends a signal; a pid that is no longer the daemon's fails with
        // `ESRCH`.
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
    }
}
#[cfg(target_os = "macos")]
use macos as platform;

/// Every target but macOS, Linux included.
#[cfg(not(target_os = "macos"))]
mod windows {
    /// There is no signal to send on Windows: the daemon's stop is the shutdown request, then the
    /// kill. Linux, which builds this half, stops its daemon the same way.
    pub fn terminate(_pid: u32) {}
}
#[cfg(not(target_os = "macos"))]
use windows as platform;

use platform::terminate;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    const SHORT: [Duration; 3] = [Duration::from_millis(30); 3];

    /// Which steps of `escalate_stop` ran, in order, for a daemon that is `stopped` once the step
    /// named `exits_after` has run (`None` for one that never is) and whose request is `accepted`
    /// or not.
    fn steps(exits_after: Option<&'static str>, accepted: bool) -> Vec<&'static str> {
        let order = Mutex::new(Vec::new());
        let stopped = || exits_after.is_some_and(|step| order.lock().unwrap().contains(&step));
        escalate_stop(
            stopped,
            || {
                order.lock().unwrap().push("request");
                accepted
            },
            || order.lock().unwrap().push("terminate"),
            || order.lock().unwrap().push("kill"),
            SHORT,
        );
        order.into_inner().unwrap()
    }

    #[test]
    fn a_daemon_that_exits_by_itself_is_not_signalled() {
        let signalled = AtomicUsize::new(0);
        escalate_stop(
            || true,
            || {
                signalled.fetch_add(1, Ordering::SeqCst);
                true
            },
            || {
                signalled.fetch_add(1, Ordering::SeqCst);
            },
            || {
                signalled.fetch_add(1, Ordering::SeqCst);
            },
            SHORT,
        );
        assert_eq!(signalled.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_daemon_that_exits_on_the_request_is_not_signalled() {
        assert_eq!(steps(Some("request"), true), ["request"]);
    }

    #[test]
    fn a_request_that_is_refused_falls_back_to_sigterm_at_once() {
        assert_eq!(steps(Some("terminate"), false), ["request", "terminate"]);
    }

    #[test]
    fn a_daemon_that_ignores_an_accepted_request_is_killed_without_sigterm() {
        assert_eq!(steps(None, true), ["request", "kill"]);
    }

    #[test]
    fn a_daemon_that_ignores_everything_is_killed_last() {
        assert_eq!(steps(None, false), ["request", "terminate", "kill"]);
    }

    /// Which steps of `escalate_planned_stop` ran, in order, for a daemon that is `stopped` once
    /// the step named `exits_after` has run (`None` for one that never is) and whose requests are
    /// accepted or not.
    fn planned_steps(exits_after: Option<&'static str>, accepted: bool) -> Vec<&'static str> {
        let order = Mutex::new(Vec::new());
        let stopped = || exits_after.is_some_and(|step| order.lock().unwrap().contains(&step));
        escalate_planned_stop(
            stopped,
            || {
                order.lock().unwrap().push("request");
                accepted
            },
            || order.lock().unwrap().push("kill"),
            [SHORT[0]; 4],
        );
        order.into_inner().unwrap()
    }

    #[test]
    fn a_daemon_that_exits_by_itself_after_a_planned_stop_is_not_asked_again() {
        let asked = AtomicUsize::new(0);
        escalate_planned_stop(
            || true,
            || {
                asked.fetch_add(1, Ordering::SeqCst);
                true
            },
            || {
                asked.fetch_add(1, Ordering::SeqCst);
            },
            [SHORT[0]; 4],
        );
        assert_eq!(asked.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn an_accepted_planned_request_is_sent_once_and_waited_for() {
        assert_eq!(planned_steps(Some("request"), true), ["request"]);
        assert_eq!(planned_steps(None, true), ["request", "kill"]);
    }

    #[test]
    fn a_planned_request_that_could_not_be_sent_is_repeated_once_and_never_signalled() {
        assert_eq!(planned_steps(None, false), ["request", "request", "kill"]);
    }

    #[test]
    fn the_stop_at_a_session_end_stays_within_the_wait_it_is_given() {
        for seconds in [4, 5] {
            let wait = Duration::from_secs(seconds);
            for request_timeout in [SESSION_END_REQUEST_TIMEOUT, Duration::ZERO] {
                let [before_request, after_request, after_kill] =
                    session_end_budget(wait, request_timeout);
                assert_eq!(before_request, Duration::ZERO);
                assert_eq!(
                    before_request + request_timeout + after_request + after_kill,
                    wait
                );
            }
        }
        // A wait too short for the request and the kill leaves no wait in between.
        assert_eq!(
            session_end_budget(Duration::from_millis(100), SESSION_END_REQUEST_TIMEOUT)[1],
            Duration::ZERO
        );
    }

    #[test]
    fn a_session_end_during_a_planned_stop_signals_then_requests_then_kills() {
        assert_eq!(session_end_stop(true), SessionEndStop::PlannedUnderWay);
        assert_eq!(session_end_stop(false), SessionEndStop::Ordinary);
        let run = |exits_after: Option<&'static str>| {
            let order = Mutex::new(Vec::new());
            let stopped = || exits_after.is_some_and(|step| order.lock().unwrap().contains(&step));
            escalate_planned_session_end(
                stopped,
                || order.lock().unwrap().push("terminate"),
                || {
                    order.lock().unwrap().push("request");
                    false
                },
                || order.lock().unwrap().push("kill"),
                SHORT,
            );
            order.into_inner().unwrap()
        };
        assert_eq!(run(None), ["terminate", "request", "kill"]);
        assert_eq!(run(Some("request")), ["terminate", "request"]);
    }

    #[test]
    fn only_a_success_status_accepts_the_shutdown_request() {
        assert!(accepted("HTTP/1.1 202 Accepted\r\n"));
        assert!(accepted("HTTP/1.1 200 OK\r\n"));
        assert!(!accepted("HTTP/1.1 401 Unauthorized\r\n"));
        assert!(!accepted("HTTP/1.1 404 Not Found\r\n"));
        assert!(!accepted(""));
    }

    #[test]
    fn each_kind_of_stop_goes_to_its_own_endpoint_and_a_refusal_is_not_an_acceptance() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for status in ["202 Accepted", "401 Unauthorized", "202 Accepted"] {
                let (mut stream, _) = listener.accept().unwrap();
                // The request may arrive in more than one read; its head ends at the blank line.
                let mut request = Vec::new();
                let mut buffer = [0u8; 1024];
                while !request.ends_with(b"\r\n\r\n") {
                    let read = stream.read(&mut buffer).unwrap();
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..read]);
                }
                requests.push(String::from_utf8_lossy(&request).into_owned());
                write!(stream, "HTTP/1.1 {status}\r\nContent-Length: 0\r\n\r\n").unwrap();
            }
            requests
        });
        let address = DaemonAddress {
            port,
            key: "ab12".to_string(),
        };
        assert!(request_stop(
            &address,
            StopKind::Ordinary,
            STOP_REQUEST_TIMEOUT
        ));
        assert!(!request_stop(
            &address,
            StopKind::Ordinary,
            STOP_REQUEST_TIMEOUT
        ));
        assert!(request_stop(
            &address,
            StopKind::Planned,
            STOP_REQUEST_TIMEOUT
        ));
        let requests = server.join().unwrap();
        assert!(requests[0].starts_with("POST /shutdown?key=ab12 HTTP/1.1\r\n"));
        assert!(requests[0].contains(&format!("Host: 127.0.0.1:{port}\r\n")));
        assert!(requests[2].starts_with("POST /planned-stop?key=ab12 HTTP/1.1\r\n"));
    }

    #[test]
    fn a_request_to_nothing_listening_is_not_an_acceptance() {
        // Port 1 (tcpmux) has no listener on a development machine or CI runner. A port freed by
        // binding and dropping a listener would not do: a test running alongside may be handed it.
        let address = DaemonAddress {
            port: 1,
            key: "ab12".to_string(),
        };
        assert!(!request_stop(
            &address,
            StopKind::Ordinary,
            STOP_REQUEST_TIMEOUT
        ));
        assert!(!request_stop(
            &address,
            StopKind::Planned,
            STOP_REQUEST_TIMEOUT
        ));
    }

    #[test]
    fn a_request_to_a_daemon_that_never_answers_ends_at_the_timeout() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = DaemonAddress {
            port: listener.local_addr().unwrap().port(),
            key: "ab12".to_string(),
        };
        let started = Instant::now();
        assert!(!request_stop(
            &address,
            StopKind::Ordinary,
            Duration::from_millis(200)
        ));
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
