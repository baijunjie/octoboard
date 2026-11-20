//! `restart` against a stand-in daemon: the check it asks before a restart
//! ([`super::restart_check`]), what it requests or not after it, and a daemon that ends while
//! the user is asked. The command runs as a process of its own, as in `background_tests.rs`,
//! whose stand-in and command helpers these use, against a loopback server recording what it is
//! asked ([`HttpDaemon`]); Unix only, for the same reason.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::background::tests::{answer_once, probe, read_request, NOT_FOUND};
use super::background::Proceed;
use super::background_tests::{command, finished, in_home, said, Renewal, StandIn};
use crate::owner_record::{Owner, Predecessor};
use crate::test_support::{ScratchDir, PATIENCE, TEST_KEY};

/// `restart` asks a daemon whose key is recorded over loopback, at the port it records, so a
/// refusal comes back with its reason at once: the command says it on stderr and exits 1 without
/// waiting for a restart, and the daemon, which was asked rather than signalled, still runs.
#[test]
fn restart_reports_a_refusal_from_a_daemon_that_records_its_key_and_leaves_it_running() {
    let home = ScratchDir::new("restart-refused");
    let text = "a holder is in service";
    let body = format!(r#"{{"reason":"holders_in_service","message":"{text}"}}"#);
    let (port, served) = answer_once(format!(
        "HTTP/1.1 409 Conflict\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    ));
    let mut stand_in = StandIn::start_on(&home, port);
    let mut restart = command(&home, "restart");
    // Well inside the command's own wait of ninety seconds for a restart that never came.
    assert!(restart.ends(), "it went on waiting after the refusal");
    let (code, stdout, stderr) = finished(restart);
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert_eq!(
        said(&stderr),
        format!("{} refused the restart: {text}", crate::APP_NAME),
        "{stdout}"
    );
    assert_eq!(
        served.join().unwrap(),
        format!("POST /restart?key={TEST_KEY} HTTP/1.1\r\n")
    );
    assert!(stand_in.running(), "the refused daemon was ended");
}

/// What the tests below start as `octoboardd restart` whose question to the user is answered by a
/// line on its standard input: it says `asking`, waits for the line, and goes on as the command
/// does once the user has answered yes, the exit status being the process's. The user's answer
/// is what the real command takes from the terminal ([`super::restart_check::restart_into`]);
/// what the command does after it is the real `background::restart`.
#[test]
#[ignore = "started by the tests below as the command"]
fn restart_asked_child() {
    let status = super::background::restart(&mut |_, _| {
        println!("asking");
        std::io::stdout().flush().unwrap();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
        Ok(Proceed::Request)
    })
    .unwrap();
    std::process::exit(status);
}

/// Whether `request_line` is one of the calls `restart` makes of the daemon: a `POST` to
/// `/restart-check` or `/restart`. Anything else that reaches a stand-in, such as the `HEAD /` a
/// running Octoboard daemon probes a session's ports with (`background::tests::is_probe`), is
/// not one.
fn is_restart_call(request_line: &str) -> bool {
    let mut parts = request_line.split_whitespace();
    let (Some("POST"), Some(target)) = (parts.next(), parts.next()) else {
        return false;
    };
    matches!(
        target.split('?').next(),
        Some("/restart-check" | "/restart")
    )
}

/// A loopback server standing in for the daemon's port: it answers a call `restart` makes
/// ([`is_restart_call`]) by its path from `routes` (`404` for any other) and records it, as its
/// request line and body, in the order they came; anything else is answered with a plain `404`
/// and not recorded, so that what reaches the port from elsewhere changes no assertion. It ends
/// when dropped.
struct HttpDaemon {
    port: u16,
    requests: Arc<Mutex<Vec<(String, String)>>>,
    done: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl HttpDaemon {
    /// `routes` are the paths to answer and the status line and body each is answered with.
    fn start(routes: &[(&str, &str, String)]) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().unwrap().port();
        let routes: Vec<(String, String, String)> = routes
            .iter()
            .map(|(path, status, body)| (path.to_string(), status.to_string(), body.clone()))
            .collect();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let done = Arc::new(AtomicBool::new(false));
        let thread = {
            let (requests, done) = (requests.clone(), done.clone());
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if done.load(Ordering::SeqCst) {
                        return;
                    }
                    let Ok(stream) = stream else { continue };
                    stream.set_read_timeout(Some(PATIENCE)).ok();
                    let mut reader = BufReader::new(stream);
                    let Some((request_line, body)) = read_request(&mut reader) else {
                        continue;
                    };
                    if !is_restart_call(&request_line) {
                        // A probe of the port (`is_probe`), or anything else the command does
                        // not send: answered plainly, and not counted as asked of the daemon.
                        reader.get_mut().write_all(NOT_FOUND.as_bytes()).ok();
                        continue;
                    }
                    let path = request_line
                        .split_whitespace()
                        .nth(1)
                        .and_then(|target| target.split('?').next())
                        .unwrap_or_default()
                        .to_owned();
                    let (status, answer) = routes
                        .iter()
                        .find(|(route, _, _)| *route == path)
                        .map_or(("404 Not Found", String::new()), |(_, status, body)| {
                            (status.as_str(), body.clone())
                        });
                    requests.lock().unwrap().push((
                        request_line.trim_end().to_owned(),
                        String::from_utf8_lossy(&body).into_owned(),
                    ));
                    let response = format!(
                        "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Type: application/json\r\n\
                         Content-Length: {}\r\n\r\n{answer}",
                        answer.len()
                    );
                    reader.get_mut().write_all(response.as_bytes()).ok();
                }
            })
        };
        Self {
            port,
            requests,
            done,
            thread: Some(thread),
        }
    }

    /// The paths asked of it so far, in order.
    fn paths(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|(line, _)| {
                let target = line.split_whitespace().nth(1).unwrap_or_default();
                target.split('?').next().unwrap_or_default().to_owned()
            })
            .collect()
    }
}

impl Drop for HttpDaemon {
    fn drop(&mut self) {
        self.done.store(true, Ordering::SeqCst);
        // The accept the thread is blocked in returns for this connection.
        std::net::TcpStream::connect(("127.0.0.1", self.port)).ok();
        if let Some(thread) = self.thread.take() {
            thread.join().ok();
        }
    }
}

/// The answer of `POST /restart-check` for `verdict`, listing a session and a shell terminal where
/// it would or may interrupt, and carrying `refusal` (reason and message) where given.
fn check_answer(
    verdict: crate::coordinator::handover_check::RestartVerdict,
    refusal: Option<(&str, &str)>,
) -> (&'static str, &'static str, String) {
    use crate::coordinator::handover_check::{
        ImpactRefusal, ImpactSession, ImpactTerminal, RestartImpact, RestartVerdict,
    };
    let interrupting = verdict != RestartVerdict::Safe;
    let impact = RestartImpact {
        verdict,
        known: verdict != RestartVerdict::MayInterrupt,
        sessions: if interrupting {
            vec![ImpactSession {
                id: "s1".into(),
                title: "Fix the build".into(),
                status: "working".into(),
            }]
        } else {
            Vec::new()
        },
        shell_terminals: if interrupting {
            vec![ImpactTerminal {
                id: "t1".into(),
                title: "zsh".into(),
            }]
        } else {
            Vec::new()
        },
        unreached_shell_terminals: 0,
        refusal: refusal.map(|(reason, message)| ImpactRefusal {
            reason: reason.into(),
            message: message.into(),
        }),
    };
    (
        "/restart-check",
        "200 OK",
        serde_json::to_string(&impact).unwrap(),
    )
}

/// The refusal `POST /restart` answers with, which ends the command at once where a restart that
/// is accepted would have it wait for a successor.
fn restart_refused() -> (&'static str, &'static str, String) {
    (
        "/restart",
        "409 Conflict",
        r#"{"reason":"agent_update_running","message":"an agent CLI update is running"}"#
            .to_owned(),
    )
}

/// What reaches the stand-in's port from elsewhere, such as a running Octoboard daemon's probe of
/// a session's ports, is answered and not counted as asked of the daemon.
#[test]
fn the_stand_in_counts_only_the_calls_restart_makes() {
    let daemon = HttpDaemon::start(&[restart_refused()]);
    assert_eq!(probe(daemon.port), "HTTP/1.1 404 Not Found");
    assert!(daemon.paths().is_empty(), "{:?}", daemon.paths());
    assert!(is_restart_call(&format!(
        "POST /restart-check?key={TEST_KEY} HTTP/1.1"
    )));
    assert!(is_restart_call("POST /restart HTTP/1.1"));
    assert!(!is_restart_call("HEAD / HTTP/1.1"));
    assert!(!is_restart_call("GET /restart HTTP/1.1"));
    assert!(!is_restart_call("POST /shutdown HTTP/1.1"));
}

/// Runs `octoboardd restart <arguments>` against a stand-in daemon recording `daemon`'s port, with
/// no terminal on its standard input, and gives its exit status and output.
fn restart_against(
    label: &str,
    arguments: &str,
    routes: &[(&str, &str, String)],
) -> (i32, String, String, Vec<String>) {
    let home = ScratchDir::new(label);
    let daemon = HttpDaemon::start(routes);
    let stand_in = StandIn::start_on(&home, daemon.port);
    let restart = command(&home, arguments);
    let (code, stdout, stderr) = finished(restart);
    let paths = daemon.paths();
    drop(stand_in);
    (code, stdout, stderr, paths)
}

/// With no terminal, `restart --if-safe` asks the daemon what the restart would interrupt, with the
/// key, naming this build as the one it restarts into; where nothing would be, it requests the
/// restart, after the check.
#[test]
fn restart_if_safe_checks_then_restarts_where_nothing_would_be_interrupted() {
    use crate::coordinator::handover_check::RestartVerdict;
    let home = ScratchDir::new("restart-if-safe-safe");
    let daemon = HttpDaemon::start(&[check_answer(RestartVerdict::Safe, None), restart_refused()]);
    let stand_in = StandIn::start_on(&home, daemon.port);
    let (code, stdout, stderr) = finished(command(&home, "restart --if-safe"));
    // The daemon refused the request, which is what ends the command here: it was made.
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert!(stderr.contains("refused the restart"), "{stderr}");
    assert_eq!(daemon.paths(), ["/restart-check", "/restart"]);
    assert!(stdout.contains("every session keeps running"), "{stdout}");
    let requests = daemon.requests.lock().unwrap();
    assert_eq!(
        requests[0].0,
        format!("POST /restart-check?key={TEST_KEY} HTTP/1.1")
    );
    let asked: serde_json::Value = serde_json::from_str(&requests[0].1).unwrap();
    assert_eq!(
        asked,
        serde_json::json!({
            "holder_floor": crate::holder::compat::FLOOR,
            "target_version": crate::self_update::RUNNING,
        })
    );
    drop(stand_in);
}

/// Where the restart would or may interrupt work, `--if-safe` says which, does not request the
/// restart, and exits with the status for a decline.
#[test]
fn restart_if_safe_declines_what_would_or_may_interrupt_without_requesting_it() {
    use crate::coordinator::handover_check::RestartVerdict;
    for (verdict, word) in [
        (RestartVerdict::Interrupts, "would"),
        (RestartVerdict::MayInterrupt, "may"),
    ] {
        let (code, stdout, stderr, paths) = restart_against(
            "restart-if-safe-declined",
            "restart --if-safe",
            &[check_answer(verdict, None), restart_refused()],
        );
        assert_eq!(code, super::restart_check::DECLINED, "{stdout}{stderr}");
        assert_eq!(paths, ["/restart-check"], "{verdict:?}");
        assert!(
            stdout.contains("restarting into") && stdout.contains(&format!("{word} interrupt")),
            "{stdout}"
        );
        assert!(
            stdout.contains(&format!(
                "not restarting, as the restart {word} interrupt the work above"
            )),
            "{stdout}"
        );
        assert!(
            stdout.contains("session \"Fix the build\" (s1)"),
            "{stdout}"
        );
    }
}

/// A restart the daemon foresees refusing, and that would or may interrupt work, is neither asked
/// about nor requested: the refusal is said and the command fails (and does not exit as a decline).
#[test]
fn restart_if_safe_does_not_request_a_restart_the_daemon_foresees_refusing() {
    use crate::coordinator::handover_check::RestartVerdict;
    for verdict in [RestartVerdict::Interrupts, RestartVerdict::MayInterrupt] {
        let (code, stdout, stderr, paths) = restart_against(
            "restart-if-safe-refused",
            "restart --if-safe",
            &[
                check_answer(
                    verdict,
                    Some(("agent_update_running", "an agent CLI update is running")),
                ),
                restart_refused(),
            ],
        );
        assert_eq!(code, 1, "{verdict:?}: {stdout}{stderr}");
        assert_eq!(paths, ["/restart-check"], "{verdict:?}");
        assert!(
            stdout.contains(&format!(
                "{} would refuse the restart: an agent CLI update is running",
                crate::APP_NAME
            )),
            "{stdout}"
        );
    }
}

/// A restart that interrupts nothing is requested even where the daemon foresees a refusal: the
/// request says the daemon's reason, or restarts where the refusal has cleared meanwhile.
#[test]
fn restart_if_safe_requests_a_restart_that_interrupts_nothing_whatever_the_daemon_foresees() {
    use crate::coordinator::handover_check::RestartVerdict;
    let (code, stdout, stderr, paths) = restart_against(
        "restart-if-safe-safe-refusal",
        "restart --if-safe",
        &[
            check_answer(
                RestartVerdict::Safe,
                Some(("agent_update_running", "an agent CLI update is running")),
            ),
            restart_refused(),
        ],
    );
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert_eq!(paths, ["/restart-check", "/restart"]);
    assert!(stderr.contains("refused the restart"), "{stderr}");
}

/// A stop or restart under way is said and followed rather than added to: no request is made, and
/// the command waits for the daemon that comes up in its place, here none, so it fails.
#[test]
fn restart_follows_a_stop_under_way_without_requesting_a_restart() {
    use crate::coordinator::handover_check::RestartVerdict;
    let home = ScratchDir::new("restart-stop-under-way");
    let daemon = HttpDaemon::start(&[
        check_answer(
            RestartVerdict::Interrupts,
            Some(("stop_in_progress", "the daemon is already stopping")),
        ),
        restart_refused(),
    ]);
    let mut stand_in = StandIn::start_on(&home, daemon.port);
    let restart = command(&home, "restart --if-safe");
    assert!(
        crate::session::wait::wait_for(PATIENCE, || !daemon.paths().is_empty()),
        "the check was not asked"
    );
    // The stop under way ends the daemon, with none in its place.
    stand_in.end();
    let (code, stdout, stderr) = finished(restart);
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert_eq!(daemon.paths(), ["/restart-check"]);
    assert!(
        stdout.contains("already stopping or restarting; following that"),
        "{stdout}"
    );
    assert!(
        stderr.contains("was already stopping and has stopped; no daemon runs in its place"),
        "{stderr}"
    );
    // It describes the stop it followed, not a restart it never asked for.
    assert!(!stderr.contains("to restart"), "{stderr}");
    assert!(!stderr.contains("cut the restart short"), "{stderr}");
    assert!(
        stderr.contains("start it with `octoboardd start`"),
        "{stderr}"
    );
}

/// `restart --yes`, and `restart` with no terminal, restart regardless: the daemon is asked for
/// the restart at once and not what it would interrupt, however it would answer.
#[test]
fn restart_regardless_sends_no_check() {
    use crate::coordinator::handover_check::RestartVerdict;
    for arguments in ["restart --yes", "restart"] {
        let (code, stdout, stderr, paths) = restart_against(
            "restart-regardless",
            arguments,
            &[
                check_answer(
                    RestartVerdict::Interrupts,
                    Some(("stop_in_progress", "the daemon is already stopping")),
                ),
                restart_refused(),
            ],
        );
        assert_eq!(code, 1, "{arguments}: {stdout}{stderr}");
        assert_eq!(paths, ["/restart"], "{arguments}");
        assert!(
            stderr.contains("refused the restart"),
            "{arguments}: {stderr}"
        );
    }
}

/// What the tests that answer a restart's question start and read: the command, its lines of
/// output and the way to answer it.
struct Asked {
    child: Child,
    lines: Receiver<String>,
    said: Vec<String>,
}

impl Asked {
    fn start(home: &Path) -> Self {
        let mut child = in_home(home, "cli::restart_tests::restart_asked_child")
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in stdout.lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let mut asked = Self {
            child,
            lines,
            said: Vec::new(),
        };
        asked.await_line("asking");
        asked
    }

    fn await_line(&mut self, expected: &str) {
        let deadline = Instant::now() + PATIENCE;
        loop {
            match self
                .lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(line) if line == expected => return,
                Ok(line) => self.said.push(line),
                Err(_) => panic!(
                    "the command did not say {expected:?} in time: {:?}",
                    self.said
                ),
            }
        }
    }

    /// Answers the question yes, and returns without waiting for what the command does next.
    fn say_yes(&mut self) {
        let mut stdin = self.child.stdin.take().unwrap();
        writeln!(stdin, "y").unwrap();
    }

    /// Answers the question, and gives the exit status and what the command said by the time it
    /// ended, its error output included.
    fn answer(mut self) -> (i32, String) {
        self.say_yes();
        self.finish()
    }

    /// Waits for the command to end, once the question is answered: its exit status and what it
    /// said, its error output included.
    fn finish(mut self) -> (i32, String) {
        let mut stderr = String::new();
        std::io::Read::read_to_string(&mut self.child.stderr.take().unwrap(), &mut stderr).unwrap();
        let status = self.child.wait().unwrap();
        // The reader thread ends with the pipe, which closes the channel: all it read is queued.
        self.said.extend(self.lines.iter());
        let said = format!("{}\n{stderr}", self.said.join("\n"));
        (status.code().expect("an exit status"), said)
    }
}

impl Drop for Asked {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

/// A daemon that has ended while the user was asked is not asked again, whichever ended it. Where
/// another runs in its place it has restarted already, and the command says so and succeeds.
#[test]
fn a_daemon_restarted_while_the_user_was_asked_is_not_restarted_again() {
    let home = ScratchDir::new("restart-asked-replaced");
    let daemon = HttpDaemon::start(&[restart_refused()]);
    let mut first = StandIn::start_on(&home, daemon.port);
    let asked = Asked::start(&home);
    first.end();
    let second = StandIn::start_now(&home, false);
    let (code, said) = asked.answer();
    assert_eq!(code, 0, "{said}");
    assert!(
        said.contains("restarted while the restart was being decided"),
        "{said}"
    );
    assert!(
        daemon.paths().is_empty(),
        "the daemon that ended was asked: {:?}",
        daemon.paths()
    );
    drop(second);
}

/// Where no daemon runs in its place the command says the daemon stopped, and fails.
#[test]
fn a_daemon_stopped_while_the_user_was_asked_is_not_restarted_and_the_command_fails() {
    let home = ScratchDir::new("restart-asked-stopped");
    let daemon = HttpDaemon::start(&[restart_refused()]);
    let mut stand_in = StandIn::start_on(&home, daemon.port);
    let asked = Asked::start(&home);
    stand_in.end();
    let (code, said) = asked.answer();
    assert_eq!(code, 1, "{said}");
    assert!(
        said.contains("stopped while the restart was being decided"),
        "{said}"
    );
    // A daemon nothing restarts is started by hand.
    assert!(said.contains("`octoboardd start`"), "{said}");
    assert!(daemon.paths().is_empty(), "{:?}", daemon.paths());
}

/// The data directory a command run with `home` as its home directory reads.
fn data_of(home: &Path) -> std::path::PathBuf {
    home.join(".octoboard")
}

/// A daemon a service manager runs, ended while the user was asked, is not told to be started with
/// `octoboardd start`: the service manager starts the daemon it runs, and that command would start
/// another beside it.
#[test]
fn a_service_managed_daemon_that_stopped_while_asked_is_not_told_to_be_started() {
    let home = ScratchDir::new("restart-asked-service-stopped");
    let daemon = HttpDaemon::start(&[restart_refused()]);
    let mut stand_in = StandIn::start_service_managed_on(&home, daemon.port);
    let asked = Asked::start(&home);
    stand_in.end();
    let (code, said) = asked.answer();
    assert_eq!(code, 1, "{said}");
    assert!(
        said.contains("stopped while the restart was being decided"),
        "{said}"
    );
    assert!(!said.contains("octoboardd start"), "{said}");
    assert!(daemon.paths().is_empty(), "{:?}", daemon.paths());
}

/// A daemon that ended with a handover noted, a service manager's to restart, is waited for while
/// the user is asked: its successor takes the lock and records itself only later, and is reported
/// as having restarted already, with nothing requested of either.
#[test]
fn a_handover_under_way_while_the_user_was_asked_is_waited_out() {
    let home = ScratchDir::new("restart-asked-handover");
    let daemon = HttpDaemon::start(&[restart_refused()]);
    let mut old = StandIn::start_service_managed_on(&home, daemon.port);
    let mut asked = Asked::start(&home);
    let data = data_of(&home);
    let recorded = Owner::recorded_in(&data).expect("the daemon recorded itself");
    // The note stays fresh, so that how long the successor takes to start is no part of the test.
    recorded.note_handover_in(&data).unwrap();
    let renewal = Renewal::start(recorded.clone(), data);
    old.end();
    asked.say_yes();
    // It holds the lock and has not recorded itself: the command must wait for it.
    let mut successor = StandIn::start_held_after(&home, Predecessor::Daemon(recorded.identity));
    drop(renewal);
    successor.record();
    let (code, said) = asked.finish();
    assert_eq!(code, 0, "{said}");
    assert!(
        said.contains("restarted while the restart was being decided"),
        "{said}"
    );
    assert!(said.contains(&format!("pid {}", successor.pid())), "{said}");
    assert!(daemon.paths().is_empty(), "{:?}", daemon.paths());
}

/// A stop or restart already under way is followed, not added to: where a daemon takes over from
/// the one that ends, the command says it restarted and succeeds, having asked the check and
/// nothing more.
#[test]
fn restart_follows_a_stop_under_way_to_the_daemon_that_takes_over() {
    use crate::coordinator::handover_check::RestartVerdict;
    let home = ScratchDir::new("restart-follows-successor");
    let daemon = HttpDaemon::start(&[
        check_answer(
            RestartVerdict::Interrupts,
            Some(("stop_in_progress", "the daemon is already stopping")),
        ),
        restart_refused(),
    ]);
    let mut old = StandIn::start_service_managed_on(&home, daemon.port);
    let restart = command(&home, "restart --if-safe");
    assert!(
        crate::session::wait::wait_for(PATIENCE, || !daemon.paths().is_empty()),
        "the check was not asked"
    );
    let data = data_of(&home);
    let recorded = Owner::recorded_in(&data).expect("the daemon recorded itself");
    // The restart under way: the daemon notes a handover and ends, its successor following.
    recorded.note_handover_in(&data).unwrap();
    let renewal = Renewal::start(recorded.clone(), data);
    old.end();
    let successor = StandIn::start_after(
        &home,
        Duration::ZERO,
        Predecessor::Daemon(recorded.identity),
    );
    let (code, stdout, stderr) = finished(restart);
    drop(renewal);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(
        stdout.contains(&format!("restarted: pid {}", successor.pid())),
        "{stdout}"
    );
    assert!(
        stdout.contains("already stopping or restarting; following that"),
        "{stdout}"
    );
    assert_eq!(daemon.paths(), ["/restart-check"]);
}
