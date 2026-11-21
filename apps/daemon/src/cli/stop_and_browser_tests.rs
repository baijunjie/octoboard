//! `stop`'s check for running work and the `browser` command, run as the commands do (see
//! `background_tests.rs`) against a stand-in daemon that records the port of a stub of the daemon's
//! own: one that answers `POST /stop-check`, and one that serves the control connection `browser`
//! asks. The stubs record what they were asked, which is what the commands are held to.

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;

use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::{self, Message};

use super::background::tests::{read_request, NOT_FOUND};
use super::background_tests::{command, finished, StandIn};
use crate::test_support::{ClosedPort, ScratchDir};

/// A listener on a port of its own that hands each connection to `serve` on a thread of its own,
/// and ends with the test: what it was asked arrives on [`Stub::asked`].
struct Stub {
    port: ClosedPort,
    asked: Receiver<String>,
    done: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Stub {
    fn start(serve: impl Fn(TcpStream, &Sender<String>) + Send + 'static) -> Self {
        let port = ClosedPort::new();
        let listener = TcpListener::bind(("127.0.0.1", port.port())).unwrap();
        let (sender, asked) = mpsc::channel();
        let done = Arc::new(AtomicBool::new(false));
        let ended = done.clone();
        let thread = std::thread::spawn(move || {
            while let Ok((stream, _)) = listener.accept() {
                if ended.load(Ordering::SeqCst) {
                    break;
                }
                serve(stream, &sender);
            }
        });
        Self {
            port,
            asked,
            done,
            thread: Some(thread),
        }
    }

    fn port(&self) -> u16 {
        self.port.port()
    }

    /// What it has been asked so far, once the command that asked has ended.
    fn asked(&self) -> Vec<String> {
        self.asked.try_iter().collect()
    }

    /// A stub of the daemon's `POST /stop-check`: answers `status` with `body`.
    fn stop_check(status: &'static str, body: &'static str) -> Self {
        Self::start(move |stream, asked| {
            stream
                .set_read_timeout(Some(crate::test_support::PATIENCE))
                .unwrap();
            let mut reader = std::io::BufReader::new(stream);
            let Some((request_line, _)) = read_request(&mut reader) else {
                return;
            };
            let answer = if request_line.starts_with("HEAD ") {
                NOT_FOUND.to_owned()
            } else {
                asked.send(request_line).ok();
                format!(
                    "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                )
            };
            reader.get_mut().write_all(answer.as_bytes()).ok();
        })
    }

    /// A stub of the daemon's control connection: sends a hello and a snapshot carrying
    /// `remote_access`, acknowledges every request, and reports each request's text.
    fn control(remote_access: Value) -> Self {
        Self::start(move |stream, asked| {
            let Ok(mut socket) = tungstenite::accept(stream) else {
                return;
            };
            let hello = json!({ "type": "hello" }).to_string();
            let snapshot =
                json!({ "type": "snapshot", "remote_access": remote_access }).to_string();
            if socket.send(Message::text(hello)).is_err()
                || socket.send(Message::text(snapshot)).is_err()
            {
                return;
            }
            while let Ok(Message::Text(text)) = socket.read() {
                let request: Value = serde_json::from_str(&text).unwrap();
                asked.send(text.to_string()).ok();
                let reply = json!({ "type": "ack", "id": request["id"] });
                if socket.send(Message::text(reply.to_string())).is_err() {
                    break;
                }
            }
        })
    }
}

impl Drop for Stub {
    fn drop(&mut self) {
        self.done.store(true, Ordering::SeqCst);
        // Wakes the accept the thread is blocked in.
        drop(TcpStream::connect(("127.0.0.1", self.port.port())));
        if let Some(thread) = self.thread.take() {
            thread.join().ok();
        }
    }
}

/// The snapshot's `remote_access` with browser access at `state`; remote access is off apart
/// from it.
fn remote_access(state: &str, port: Option<u16>, failure: Option<(&str, &str)>) -> Value {
    json!({
        "state": "off",
        "browser_state": state,
        "browser_port": port,
        "failure": null,
        "browser_failure": failure
            .map(|(reason, detail)| json!({ "reason": reason, "detail": detail })),
    })
}

const NOTHING_RUNS: &str = r#"{"sessions":[],"shell_terminals":[]}"#;
const WORK_RUNS: &str = r#"{"sessions":[{"id":"s1","title":"Fix the build","status":"working"},{"id":"s2","title":"","status":"waiting_user"}],"shell_terminals":[{"id":"t1","title":"zsh"}]}"#;
const OK: &str = "200 OK";

/// A daemon with sessions at work or a busy shell terminal is not stopped without `--force`: the
/// command names what runs, the daemon asked for it with the launch key, exits 1, and the daemon
/// runs on.
#[test]
fn stop_names_running_work_and_stops_nothing() {
    let home = ScratchDir::new("stop-check-refuses");
    let stub = Stub::stop_check(OK, WORK_RUNS);
    let mut stand_in = StandIn::start_listening_on(&home, stub.port(), false);
    let (code, stdout, stderr) = finished(command(&home, "stop"));
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert!(
        stderr.contains("session \"Fix the build\" (working)"),
        "{stderr}"
    );
    assert!(
        stderr.contains("session \"s2\" (waiting for you)"),
        "{stderr}"
    );
    assert!(
        stderr.contains("shell terminal \"zsh\" (running)"),
        "{stderr}"
    );
    assert!(stderr.contains("octoboardd stop --force"), "{stderr}");
    assert!(stand_in.running(), "the daemon was stopped");
    let asked = stub.asked();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert!(
        asked[0].starts_with(&format!(
            "POST /stop-check?key={} ",
            crate::test_support::TEST_KEY
        )),
        "{asked:?}"
    );
}

/// With nothing running that a stop would interrupt, `stop` goes ahead and ends the daemon.
#[test]
fn stop_goes_ahead_when_nothing_would_be_interrupted() {
    let home = ScratchDir::new("stop-check-clear");
    let stub = Stub::stop_check(OK, NOTHING_RUNS);
    let mut stand_in = StandIn::start_listening_on(&home, stub.port(), false);
    let (code, stdout, stderr) = finished(command(&home, "stop"));
    assert_eq!(code, 0, "{stdout}{stderr}");
    stand_in.end();
    assert_eq!(stub.asked().len(), 1);
}

/// `--force` stops a daemon that runs work, and does not ask what runs.
#[test]
fn stop_force_stops_regardless_of_running_work_without_asking() {
    let home = ScratchDir::new("stop-check-forced");
    let stub = Stub::stop_check(OK, WORK_RUNS);
    let mut stand_in = StandIn::start_listening_on(&home, stub.port(), false);
    let (code, stdout, stderr) = finished(command(&home, "stop --force"));
    assert_eq!(code, 0, "{stdout}{stderr}");
    stand_in.end();
    assert_eq!(stub.asked(), Vec::<String>::new());
}

/// A daemon that cannot be asked (one of an earlier build answers `404`) is taken as one that may
/// be running work: `stop` refuses, says how to stop it anyway and leaves it running.
#[test]
fn stop_refuses_a_daemon_that_cannot_be_asked_what_runs() {
    let home = ScratchDir::new("stop-check-unanswered");
    let stub = Stub::stop_check("404 Not Found", "");
    let mut stand_in = StandIn::start_listening_on(&home, stub.port(), false);
    let (code, stdout, stderr) = finished(command(&home, "stop"));
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert!(stderr.contains("could not be asked"), "{stderr}");
    assert!(stderr.contains("octoboardd stop --force"), "{stderr}");
    assert!(stand_in.running(), "the daemon was stopped");
}

/// The same check holds for the desktop application's daemon.
#[test]
fn stop_names_running_work_of_the_applications_daemon_too() {
    let home = ScratchDir::new("stop-check-application");
    let stub = Stub::stop_check(OK, WORK_RUNS);
    let mut stand_in = StandIn::start_listening_on(&home, stub.port(), true);
    let (code, stdout, stderr) = finished(command(&home, "stop"));
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert!(stderr.contains("session \"Fix the build\""), "{stderr}");
    assert!(stand_in.running(), "the daemon was stopped");
}

/// With no daemon running, `browser` says so, changes nothing and exits 3.
#[test]
fn browser_with_no_daemon_running_says_so_and_exits_3() {
    let home = ScratchDir::new("browser-not-running");
    for arguments in ["browser status", "browser on", "browser off"] {
        let (code, stdout, stderr) = finished(command(&home, arguments));
        assert_eq!(code, 3, "{arguments}: {stdout}{stderr}");
        assert!(stderr.contains("not running"), "{arguments}: {stderr}");
        assert!(
            stderr.contains("browser access was not changed"),
            "{arguments}: {stderr}"
        );
    }
}

/// A daemon that serves no browser has no setting: `browser` says it has none and exits 1.
#[test]
fn browser_against_a_daemon_serving_no_browser_says_it_has_none_and_exits_1() {
    let home = ScratchDir::new("browser-no-setting");
    let stub = Stub::control(Value::Null);
    let _stand_in = StandIn::start_listening_on(&home, stub.port(), false);
    for arguments in ["browser status", "browser off"] {
        let (code, stdout, stderr) = finished(command(&home, arguments));
        assert_eq!(code, 1, "{arguments}: {stdout}{stderr}");
        assert!(
            stderr.contains("serves no browser"),
            "{arguments}: {stderr}"
        );
    }
}

/// `browser on --port` sets the port and then turns browser access on, through the running daemon,
/// and says where the browser is served and what that costs.
#[test]
fn browser_on_sets_the_port_then_browser_access_and_states_the_cost() {
    let home = ScratchDir::new("browser-on");
    let stub = Stub::control(remote_access("serving", Some(4000), None));
    let _stand_in = StandIn::start_listening_on(&home, stub.port(), false);
    let (code, stdout, stderr) = finished(command(&home, "browser on --port 4000"));
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(
        stdout.contains("browser access is on, at port 4000"),
        "{stdout}"
    );
    assert!(stdout.contains("can take the browser port"), "{stdout}");
    let asked: Vec<Value> = stub
        .asked()
        .iter()
        .map(|text| serde_json::from_str(text).unwrap())
        .collect();
    let kinds: Vec<(&str, Option<u64>, Option<bool>)> = asked
        .iter()
        .map(|request| {
            (
                request["type"].as_str().unwrap(),
                request["port"].as_u64(),
                request["allowed"].as_bool(),
            )
        })
        .collect();
    assert_eq!(
        kinds,
        [
            ("set_remote_access_port", Some(4000), None),
            ("set_browser_access", None, Some(true))
        ]
    );
}

/// A port that is in use fails the command with a hint to choose another.
#[test]
fn browser_on_with_the_port_in_use_exits_1_and_asks_for_another_port() {
    let home = ScratchDir::new("browser-in-use");
    let stub = Stub::control(remote_access(
        "failed",
        Some(3369),
        Some(("port_in_use", "listening on 127.0.0.1:3369: in use")),
    ));
    let _stand_in = StandIn::start_listening_on(&home, stub.port(), false);
    let (code, stdout, stderr) = finished(command(&home, "browser on"));
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert!(stdout.contains("in use"), "{stdout}");
    assert!(stdout.contains("--port <port>"), "{stdout}");
}

/// `off` closes the port alone: on the desktop application's daemon it says Allow remote access
/// was not changed, on the server form that the socket keeps serving. Either sends the one
/// request, which is browser access's, never remote access's.
#[test]
fn browser_off_changes_browser_access_alone_and_says_what_it_left() {
    for (application, said) in [
        (true, "Allow remote access"),
        (false, "the browser socket keeps serving"),
    ] {
        let home = ScratchDir::new("browser-off");
        let stub = Stub::control(remote_access("off", Some(3369), None));
        let _stand_in = StandIn::start_listening_on(&home, stub.port(), application);
        let (code, stdout, stderr) = finished(command(&home, "browser off"));
        assert_eq!(code, 0, "{application}: {stdout}{stderr}");
        assert!(stdout.contains("browser access is off"), "{stdout}");
        assert!(stdout.contains(said), "{application}: {stdout}");
        let asked = stub.asked();
        assert_eq!(asked.len(), 1, "{asked:?}");
        let request: Value = serde_json::from_str(&asked[0]).unwrap();
        assert_eq!(
            (request["type"].as_str(), request["allowed"].as_bool()),
            (Some("set_browser_access"), Some(false))
        );
    }
}

/// `browser status` reports and sends nothing.
#[test]
fn browser_status_reports_and_changes_nothing() {
    let home = ScratchDir::new("browser-status");
    let stub = Stub::control(remote_access("serving", Some(3369), None));
    let _stand_in = StandIn::start_listening_on(&home, stub.port(), true);
    let (code, stdout, stderr) = finished(command(&home, "browser status"));
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(
        stdout.contains("browser access is on, at port 3369"),
        "{stdout}"
    );
    assert!(!stdout.contains("can take the browser port"), "{stdout}");
    assert_eq!(stub.asked(), Vec::<String>::new());
}

/// The daemon's own port is refused as the browser port by the command, naming both and sending
/// nothing.
#[test]
fn browser_on_refuses_the_daemons_own_port_before_sending_anything() {
    let home = ScratchDir::new("browser-own-port");
    let stub = Stub::control(remote_access("off", Some(3369), None));
    let own = stub.port();
    let _stand_in = StandIn::start_listening_on(&home, own, false);
    let (code, stdout, stderr) = finished(command(&home, &format!("browser on --port {own}")));
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert!(stderr.contains("browser port"), "{stderr}");
    assert!(stderr.contains("daemon's own port"), "{stderr}");
    assert!(stderr.contains(&own.to_string()), "{stderr}");
    assert_eq!(stub.asked(), Vec::<String>::new());
}
