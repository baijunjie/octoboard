//! Which route the two modes the dispatchers start take to the daemon ([`Routes::post`]): the
//! launch's, then the daemon's own through the owner and key records in the data directory, but
//! only where the launch's route is gone.
//!
//! Each case runs against stand-ins for the launch's port and the daemon's, which record the
//! request lines that reach them, so that what the daemon's key was sent to is read off what
//! arrived. The helper processes of the same routes, given an environment, are in
//! `cli::key_source_tests`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::owner_record::Owner;
use crate::session_git_hooks::route::{Deadlines, Outcome, Routes};
use crate::test_support::{ClosedPort, ScratchDir, PATIENCE, TEST_KEY};

/// The key of the launch's route, [`TEST_KEY`], and another, the daemon's.
const LAUNCH_KEY: &str = TEST_KEY;
const DAEMON_KEY: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

/// What a stand-in does with the request it gets.
#[derive(Clone, Copy)]
enum Answer {
    /// Replies with this status line and the body `answer`.
    Status(&'static str),
    /// Reads the request and never replies.
    Silent,
}

/// A loopback server that records the request line of every request it reads, and answers them all
/// the same way, until it is finished.
struct StandIn {
    port: u16,
    seen: Arc<Mutex<Vec<String>>>,
    release: Sender<()>,
    thread: JoinHandle<()>,
}

impl StandIn {
    fn start(answer: Answer) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (release, released) = channel::<()>();
        let record = seen.clone();
        let thread = std::thread::spawn(move || loop {
            let (stream, _) = listener.accept().unwrap();
            // A connection that sends nothing is the one that ends the server.
            let Some(line) = read_request(&stream) else {
                return;
            };
            record.lock().unwrap().push(line);
            match answer {
                Answer::Status(status) => {
                    let reply = format!(
                        "HTTP/1.1 {status}\r\nContent-Length: 6\r\nConnection: close\r\n\r\nanswer"
                    );
                    let _ = (&stream).write_all(reply.as_bytes());
                }
                // Held until the test is over: the caller's deadline is what ends its wait.
                Answer::Silent => {
                    let _ = released.recv();
                }
            }
        });
        Self {
            port,
            seen,
            release,
            thread,
        }
    }

    /// The request lines that reached it, once everything that was going to has.
    fn finish(self) -> Vec<String> {
        let _ = self.release.send(());
        drop(TcpStream::connect(("127.0.0.1", self.port)).unwrap());
        self.thread.join().unwrap();
        let seen = self.seen.lock().unwrap().clone();
        seen
    }
}

/// The request line of the request on `stream`, its headers and body read; `None` for a connection
/// closed without one.
fn read_request(stream: &TcpStream) -> Option<String> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok().filter(|read| *read > 0)?;
    let mut length = 0;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).ok()?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap();
            }
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(line.trim_end().to_owned())
}

/// What the data directory says of the daemon.
#[derive(Clone, Copy, Debug)]
enum Record {
    /// A daemon on the stand-in's port, running as this very process, with its key.
    Running,
    /// A daemon whose process is gone.
    Dead,
    /// The owner record of a running daemon, and no key record.
    NoKey,
    /// The owner record of a running daemon, and a key record that holds no key.
    UnreadableKey,
    /// Nothing.
    None,
}

/// A data directory under `dir` with `record` in it for a daemon on `port`.
fn data_with(dir: &ScratchDir, name: &str, record: Record, port: u16) -> std::path::PathBuf {
    let data = dir.join(name).join(".octoboard");
    let running = Owner::current(port, false, true).expect("this process's identity");
    match record {
        Record::Running => running.record_in(&data, DAEMON_KEY).unwrap(),
        Record::Dead => {
            let mut dead = running;
            dead.identity.start_micros = dead.identity.start_micros.wrapping_add(1);
            dead.record_in(&data, DAEMON_KEY).unwrap();
        }
        Record::NoKey => {
            running.record_in(&data, DAEMON_KEY).unwrap();
            std::fs::remove_file(data.join("daemon.key")).unwrap();
        }
        Record::UnreadableKey => {
            running.record_in(&data, DAEMON_KEY).unwrap();
            std::fs::write(data.join("daemon.key"), "not a key record\n").unwrap();
        }
        Record::None => {}
    }
    data
}

fn post(routes: &Routes, launch: Duration) -> Outcome {
    routes.post(
        "/git-ref/a",
        b"{}",
        Deadlines {
            launch,
            daemon: PATIENCE,
        },
    )
}

/// The key a request line carries.
fn key_of(line: &str) -> &str {
    line.split_once("?key=")
        .and_then(|(_, rest)| rest.split(' ').next())
        .unwrap()
}

/// A launch route that refuses the key (401 or 403) or has nothing listening is gone, and the
/// request goes to the daemon the owner record names, under the key in the key record: the launch's
/// port, which may be anyone's by now, is never sent the daemon's key.
#[test]
fn a_launch_route_that_refuses_the_key_or_is_not_listening_goes_on_to_the_daemons() {
    let dir = ScratchDir::new("route-gone");
    let launches = [
        Some(Answer::Status("401 Unauthorized")),
        Some(Answer::Status("403 Forbidden")),
        None,
    ];
    for (n, launch) in launches.into_iter().enumerate() {
        let daemon = StandIn::start(Answer::Status("200 OK"));
        let data = data_with(&dir, &format!("gone-{n}"), Record::Running, daemon.port);
        let stand_in = launch.map(StandIn::start);
        // A port below every system's ephemeral range, so no other test is handed it meanwhile.
        let closed = ClosedPort::new();
        let port = stand_in.as_ref().map_or(closed.port, |s| s.port);
        let routes = Routes {
            launch: Some((port, LAUNCH_KEY.to_string())),
            data,
        };

        let outcome = post(&routes, PATIENCE);

        assert!(
            matches!(&outcome, Outcome::Answered(body) if body == b"answer"),
            "case {n}: {outcome:?}"
        );
        let at_launch = stand_in.map(StandIn::finish).unwrap_or_default();
        let at_daemon = daemon.finish();
        assert_eq!(at_daemon.len(), 1, "case {n}");
        assert_eq!(key_of(&at_daemon[0]), DAEMON_KEY, "case {n}");
        for line in &at_launch {
            assert_eq!(key_of(line), LAUNCH_KEY, "case {n}: only its own key");
        }
        assert_eq!(at_launch.len(), usize::from(launch.is_some()), "case {n}");
    }
}

/// A launch route that answers is the route: the daemon's own is not asked as well.
#[test]
fn a_launch_route_that_answers_is_the_only_route_asked() {
    let dir = ScratchDir::new("route-answers");
    let launch = StandIn::start(Answer::Status("200 OK"));
    let daemon = StandIn::start(Answer::Status("200 OK"));
    let routes = Routes {
        launch: Some((launch.port, LAUNCH_KEY.to_string())),
        data: data_with(&dir, "home", Record::Running, daemon.port),
    };

    let outcome = post(&routes, PATIENCE);

    assert!(matches!(&outcome, Outcome::Answered(body) if body == b"answer"));
    assert_eq!(launch.finish().len(), 1);
    assert!(daemon.finish().is_empty());
}

/// A holder with no daemon to relay to (503) or one that lost its daemon (502), and a route that
/// does not answer within its deadline, are the daemon not being there, not the route being gone:
/// nothing is tried on the daemon's own route.
#[test]
fn a_launch_route_that_is_unavailable_or_silent_is_not_gone() {
    let dir = ScratchDir::new("route-not-gone");
    let cases = [
        (Answer::Status("503 Service Unavailable"), Some(503)),
        (Answer::Status("502 Bad Gateway"), Some(502)),
        // A deadline short enough to pass at once: the stand-in never answers, so this does not
        // race anything.
        (Answer::Silent, None),
    ];
    for (n, (answer, status)) in cases.into_iter().enumerate() {
        let launch = StandIn::start(answer);
        let daemon = StandIn::start(Answer::Status("200 OK"));
        let routes = Routes {
            launch: Some((launch.port, LAUNCH_KEY.to_string())),
            data: data_with(&dir, &format!("home-{n}"), Record::Running, daemon.port),
        };

        let outcome = post(&routes, Duration::from_millis(300));

        let Outcome::Failed(err) = &outcome else {
            panic!("case {n}: {outcome:?}");
        };
        assert_eq!(crate::loopback::error_status(err), status, "case {n}");
        assert_ne!(
            err.kind(),
            std::io::ErrorKind::ConnectionRefused,
            "case {n}"
        );
        assert_eq!(launch.finish().len(), 1, "case {n}");
        assert!(daemon.finish().is_empty(), "case {n}: {err}");
    }
}

/// With no launch route, or one that is gone, and no running daemon with a key to go to, nothing is
/// sent anywhere: a daemon's record that names a process that is gone, lacks its key or holds an
/// unreadable one is no route.
#[test]
fn with_no_route_at_all_nothing_is_sent() {
    let dir = ScratchDir::new("route-none");
    for (n, record) in [
        Record::None,
        Record::Dead,
        Record::NoKey,
        Record::UnreadableKey,
    ]
    .into_iter()
    .enumerate()
    {
        // Each launch case: none at all, one that refuses the key, and one that never answers.
        for launch in [
            None,
            Some(Answer::Status("401 Unauthorized")),
            Some(Answer::Silent),
        ] {
            let daemon = StandIn::start(Answer::Status("200 OK"));
            let data = data_with(&dir, &format!("home-{n}"), record, daemon.port);
            let stand_in = launch.map(StandIn::start);
            let routes = Routes {
                launch: stand_in
                    .as_ref()
                    .map(|stand_in| (stand_in.port, LAUNCH_KEY.to_string())),
                data,
            };
            // A silent launch route is not gone, so it is asked and fails; the others are
            // unrouted.
            let silent = matches!(launch, Some(Answer::Silent));
            let outcome = post(&routes, Duration::from_millis(300));
            let case = format!("{record:?}, launch route {}", launch.is_some());
            match (&outcome, silent) {
                (Outcome::Failed(_), true) | (Outcome::Unrouted, false) => {}
                _ => panic!("{case}: {outcome:?}"),
            }
            let at_launch = stand_in.map(StandIn::finish).unwrap_or_default();
            assert_eq!(at_launch.len(), usize::from(launch.is_some()), "{case}");
            assert!(daemon.finish().is_empty(), "{case}");
        }
    }
}
