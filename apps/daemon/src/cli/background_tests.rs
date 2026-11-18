//! `stop`, `status` and `start` against a daemon caught half-way through a planned restart: one
//! that holds the lock but has not recorded itself yet, and the free lock between a daemon that
//! noted it hands over and its successor; and `restart` against a daemon that refuses it.
//!
//! The commands run as processes of their own (this test binary, running only
//! [`command_child`]) with a scratch home directory, against a stand-in daemon, another such
//! process running only [`stand_in_child`], which takes the lock and records itself as a daemon
//! does. `stop` ends the stand-in by its termination signal, which is why these run on Unix only.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;

use super::Cli;
use crate::cli::background::tests::answer_once;
use crate::owner_record::{Owner, Predecessor};
use crate::platform::proc::ProcessIdentity;
use crate::test_support::{ScratchDir, PATIENCE, TEST_KEY};

/// The variable [`command_child`] takes its command line from, its arguments apart by spaces.
const ARGUMENTS: &str = "OCTOBOARD_TEST_COMMAND_ARGUMENTS";

/// The variable [`stand_in_child`] takes how long to hold the lock before it records itself from,
/// in milliseconds.
const RECORD_AFTER: &str = "OCTOBOARD_TEST_STAND_IN_RECORD_AFTER_MS";

/// The variable that, set, has [`stand_in_child`] hold the lock without recording itself until it
/// is told to on its stdin ([`StandIn::record`]), in place of [`RECORD_AFTER`].
const RECORD_WHEN_TOLD: &str = "OCTOBOARD_TEST_STAND_IN_RECORD_WHEN_TOLD";

/// The variable that has [`stand_in_child`] record the daemon it took over from: `none`, or that
/// daemon's identity key (`<pid>:<start>`). Without it the stand-in records no predecessor, as a
/// record of an earlier build does not say.
const RECORD_AFTER_DAEMON: &str = "OCTOBOARD_TEST_STAND_IN_AFTER";

/// The variable that has [`stand_in_child`] record the port it holds instead of [`PORT`].
const RECORD_PORT: &str = "OCTOBOARD_TEST_STAND_IN_PORT";

/// The variable that, set, has [`stand_in_child`] record that it accepts the owner key.
const RECORD_OWNER_KEY: &str = "OCTOBOARD_TEST_STAND_IN_OWNER_KEY";

/// The variable that, set, has [`stand_in_child`] record that it follows the desktop application.
const RECORD_APPLICATION: &str = "OCTOBOARD_TEST_STAND_IN_APPLICATION";

/// The port the stand-in records unless [`RECORD_PORT`] gives another; nothing listens there, and
/// the commands that use it do not connect.
const PORT: u16 = 47_999;

/// How long the stand-in holds the lock without a record, or the lock stays free after a note,
/// before it records itself: long enough that a command that did not wait would have answered.
const HALF_WAY: Duration = Duration::from_millis(700);

/// What the tests below start for `octoboardd <arguments>`: the mode runs as it does in the real
/// binary, and its exit status is the process's.
#[test]
#[ignore = "started by the tests below as the command"]
fn command_child() {
    let arguments = std::env::var(ARGUMENTS).unwrap();
    let cli = Cli::try_parse_from(["octoboardd"].into_iter().chain(arguments.split(' '))).unwrap();
    super::run(cli.command.expect("a mode")).unwrap();
}

/// What the tests below start as the daemon: it takes the lock, says `locked`, records itself and
/// its key and, where [`RECORD_AFTER_DAEMON`] says, its predecessor after [`RECORD_AFTER`] (or
/// once told, see [`RECORD_WHEN_TOLD`]), removes the note of a
/// handover as a daemon does once it has recorded itself, says `recorded`, and holds the lock
/// until its stdin closes or a termination signal ends it. As a daemon does, it takes the lock
/// before it records itself, so that a command looking in between finds the lock held with no
/// record, however short the delay.
#[test]
#[ignore = "started by the tests below as the stand-in daemon"]
fn stand_in_child() {
    let _lock = crate::instance_lock::acquire().unwrap();
    println!("locked");
    let mut line = String::new();
    if std::env::var_os(RECORD_WHEN_TOLD).is_some() {
        std::io::stdin().read_line(&mut line).unwrap();
    } else {
        let record_after: u64 = std::env::var(RECORD_AFTER).unwrap().parse().unwrap();
        std::thread::sleep(Duration::from_millis(record_after));
    }
    let predecessor = match std::env::var(RECORD_AFTER_DAEMON).as_deref() {
        Ok("none") => Predecessor::None,
        Ok(key) => Predecessor::Daemon(ProcessIdentity::parse(key).unwrap()),
        Err(_) => Predecessor::Unsaid,
    };
    let port = std::env::var(RECORD_PORT).map_or(PORT, |port| port.parse().unwrap());
    let owner = Owner::current(port, std::env::var_os(RECORD_APPLICATION).is_some(), true)
        .unwrap()
        .with_service_manager(false)
        .with_predecessor(predecessor)
        .with_owner_key(std::env::var_os(RECORD_OWNER_KEY).is_some());
    owner.record_key(TEST_KEY).unwrap();
    owner.record().unwrap();
    crate::owner_record::settle_handover();
    println!("recorded");
    while std::io::stdin()
        .read_line(&mut line)
        .is_ok_and(|read| read > 0)
    {}
}

/// What [`RECORD_AFTER_DAEMON`] holds for `predecessor`.
fn predecessor_text(predecessor: Predecessor) -> String {
    match predecessor {
        Predecessor::Daemon(identity) => identity.key(),
        Predecessor::None => "none".to_owned(),
        Predecessor::Unsaid => unreachable!("an unsaid predecessor is recorded by leaving it out"),
    }
}

fn in_home(home: &Path, test: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", test, "--ignored", "--nocapture"])
        .env("HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

/// A command started by [`command`], ended with the test, whether or not it finished.
struct CommandChild(Option<Child>);

impl CommandChild {
    /// Whether it has not ended yet.
    fn running(&mut self) -> bool {
        self.0.as_mut().unwrap().try_wait().unwrap().is_none()
    }

    /// Whether it ends within the test's patience.
    fn ends(&mut self) -> bool {
        crate::session::wait::wait_for(PATIENCE, || !self.running())
    }
}

impl Drop for CommandChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            child.kill().ok();
            child.wait().ok();
        }
    }
}

/// Starts `octoboardd <arguments>` with `home` as its home directory.
fn command(home: &Path, arguments: &str) -> CommandChild {
    CommandChild(Some(
        in_home(home, "cli::background_tests::command_child")
            .env(ARGUMENTS, arguments)
            .spawn()
            .unwrap(),
    ))
}

/// A stand-in daemon running with `home` as its home directory.
struct StandIn {
    child: Child,
    /// The lines it prints, read on a thread of their own so that waiting for one is bounded.
    lines: Receiver<String>,
}

impl StandIn {
    /// Starts one that records itself `record_after` after it takes the lock, and returns once it
    /// holds the lock.
    fn start(home: &Path, record_after: Duration) -> Self {
        Self::spawn(home, |command| {
            command.env(RECORD_AFTER, record_after.as_millis().to_string());
        })
    }

    /// [`StandIn::start`] for one that records `port` as the port it listens on, and records itself
    /// at once.
    fn start_on(home: &Path, port: u16) -> Self {
        Self::spawn(home, |command| {
            command
                .env(RECORD_AFTER, "0")
                .env(RECORD_PORT, port.to_string());
        })
    }

    /// [`StandIn::start`] for one that records itself at once, as accepting the owner key when
    /// `owner_key`, and returns once it has.
    fn start_now(home: &Path, owner_key: bool) -> Self {
        let mut stand_in = Self::spawn(home, |command| {
            command.env(RECORD_AFTER, "0");
            if owner_key {
                command.env(RECORD_OWNER_KEY, "1");
            }
        });
        stand_in.await_line("recorded");
        stand_in
    }

    /// [`StandIn::start_now`] for the desktop application's daemon, which takes the owner key only
    /// while remote access is on.
    fn start_application_now(home: &Path, remote_access: bool) -> Self {
        let mut stand_in = Self::spawn(home, |command| {
            command.env(RECORD_AFTER, "0").env(RECORD_APPLICATION, "1");
            if remote_access {
                command.env(RECORD_OWNER_KEY, "1");
            }
        });
        stand_in.await_line("recorded");
        stand_in
    }

    /// [`StandIn::start`] for one that records the daemon `after` as its predecessor.
    fn start_after(home: &Path, record_after: Duration, after: Predecessor) -> Self {
        Self::spawn(home, |command| {
            command
                .env(RECORD_AFTER, record_after.as_millis().to_string())
                .env(RECORD_AFTER_DAEMON, predecessor_text(after));
        })
    }

    /// Starts one that records itself only once [`StandIn::record`] tells it to, and returns once
    /// it holds the lock.
    fn start_held(home: &Path) -> Self {
        Self::spawn(home, |command| {
            command.env(RECORD_WHEN_TOLD, "1");
        })
    }

    /// [`StandIn::start_held`] for one that records the daemon `after` as its predecessor.
    fn start_held_after(home: &Path, after: Predecessor) -> Self {
        Self::spawn(home, |command| {
            command
                .env(RECORD_WHEN_TOLD, "1")
                .env(RECORD_AFTER_DAEMON, predecessor_text(after));
        })
    }

    fn spawn(home: &Path, configure: impl FnOnce(&mut Command)) -> Self {
        let mut command = in_home(home, "cli::background_tests::stand_in_child");
        configure(&mut command);
        let mut child = command.stdin(Stdio::piped()).spawn().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in stdout.lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let mut stand_in = Self { child, lines };
        stand_in.await_line("locked");
        stand_in
    }

    fn await_line(&mut self, expected: &str) {
        let deadline = Instant::now() + PATIENCE;
        loop {
            match self
                .lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(line) if line == expected => return,
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => {
                    panic!("the stand-in did not say {expected:?} within the test's patience")
                }
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("the stand-in ended before saying {expected:?}")
                }
            }
        }
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Tells one started by [`StandIn::start_held`] to record itself, and returns once it has.
    fn record(&mut self) {
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin).unwrap();
        stdin.flush().unwrap();
        self.await_line("recorded");
    }

    /// Whether it ends within the test's patience.
    fn ends(&mut self) -> bool {
        crate::session::wait::wait_for(PATIENCE, || self.child.try_wait().unwrap().is_some())
    }

    /// Whether it has not ended.
    fn running(&mut self) -> bool {
        self.child.try_wait().unwrap().is_none()
    }
}

impl Drop for StandIn {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

fn finished(mut child: CommandChild) -> (i32, String, String) {
    let Output {
        status,
        stdout,
        stderr,
    } = child.0.take().unwrap().wait_with_output().unwrap();
    (
        status.code().expect("an exit status"),
        String::from_utf8_lossy(&stdout).into_owned(),
        String::from_utf8_lossy(&stderr).into_owned(),
    )
}

/// The lines of a command's output that are its own, without what the test harness prints.
fn said(output: &str) -> String {
    output
        .lines()
        .filter(|line| line.starts_with(crate::APP_NAME))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Writes a daemon's note of its handover again every half second until dropped, which keeps the
/// note fresh for as long as a command under test takes.
struct Renewal {
    done: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Renewal {
    fn start(owner: Owner, data: std::path::PathBuf) -> Self {
        let done = Arc::new(AtomicBool::new(false));
        let ended = done.clone();
        let thread = std::thread::spawn(move || {
            while !ended.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(500));
                owner.note_handover_in(&data).ok();
            }
        });
        Self {
            done,
            thread: Some(thread),
        }
    }
}

impl Drop for Renewal {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            thread.join().ok();
        }
    }
}

/// A daemon that holds the lock without having recorded itself, as a restarting daemon does once
/// it has removed its record and its successor does until it writes its own, is waited for, and
/// stopped once it has recorded itself, rather than left running unnamed.
#[test]
fn stop_waits_for_a_daemon_that_holds_the_lock_to_record_itself_and_stops_it() {
    let home = ScratchDir::new("stop-unrecorded");
    let mut stand_in = StandIn::start(&home, HALF_WAY);
    let (code, stdout, stderr) = finished(command(&home, "stop"));
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert_eq!(
        said(&stdout),
        format!("{} stopped", crate::APP_NAME),
        "{stderr}"
    );
    assert!(stand_in.ends(), "the stand-in still runs");
}

/// With the lock free while the daemon that noted it hands over still runs, `stop` does not take
/// the daemon for gone: it waits for the successor to take the lock and record itself, and stops
/// that one, which shows itself the successor by recording the daemon that handed over as its
/// predecessor.
#[test]
fn stop_waits_through_the_free_lock_of_a_handover_for_the_successor_and_stops_it() {
    let home = ScratchDir::new("stop-handover");
    let data = home.join(".octoboard");
    std::fs::create_dir_all(&data).unwrap();
    // This process stands in for the daemon handing over: it runs throughout.
    let handing_over = Owner::current(PORT, false, true)
        .unwrap()
        .with_service_manager(false);
    handing_over.note_handover_in(&data).unwrap();
    let stop = command(&home, "stop");
    // The CLI only reports when it returns; it does not publish its handover wait.
    std::thread::sleep(HALF_WAY);
    let mut stand_in = StandIn::start_after(
        &home,
        Duration::ZERO,
        Predecessor::Daemon(handing_over.identity),
    );
    let (code, stdout, stderr) = finished(stop);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert_eq!(
        said(&stdout),
        format!("{} stopped", crate::APP_NAME),
        "{stderr}"
    );
    assert!(stand_in.ends(), "the successor still runs");
}

/// A process that runs until it is killed, standing in for a daemon handing over that ends once
/// the test says so, and its identity.
struct HandingOver {
    child: Child,
    identity: ProcessIdentity,
}

impl HandingOver {
    fn start() -> Self {
        let child = Command::new("sleep").arg("600").spawn().unwrap();
        let identity = ProcessIdentity {
            pid: crate::platform::proc::ProcessId(child.id()),
            start_micros: crate::platform::proc_table::start_of(i32::try_from(child.id()).unwrap())
                .expect("the start time of a process that runs"),
        };
        Self { child, identity }
    }

    /// Ends it, and returns once it is gone for good.
    fn end(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

impl Drop for HandingOver {
    fn drop(&mut self) {
        self.end();
    }
}

/// After a handover `stop` waited through, a daemon that records another predecessor, or none, is
/// not the successor of the daemon that handed over: `stop` leaves it running and says so once the
/// daemon that handed over has ended.
#[test]
fn stop_leaves_a_daemon_that_records_another_predecessor_running() {
    let other = Predecessor::Daemon(ProcessIdentity::parse("1:1").unwrap());
    for (label, recorded) in [("other", other), ("none", Predecessor::None)] {
        let home = ScratchDir::new(&format!("stop-other-predecessor-{label}"));
        let data = home.join(".octoboard");
        std::fs::create_dir_all(&data).unwrap();
        let mut handing_over = HandingOver::start();
        let noted = Owner {
            identity: handing_over.identity,
            ..Owner::current(PORT, false, true)
                .unwrap()
                .with_service_manager(false)
        };
        noted.note_handover_in(&data).unwrap();
        let mut stop = command(&home, "stop");
        // The daemon that handed over runs, with its note, while `stop` looks, and the other daemon
        // takes the lock and records itself only once `stop` has been seen waiting.
        // The CLI only reports when it returns; it does not publish its handover wait.
        std::thread::sleep(HALF_WAY);
        let mut stand_in = StandIn::start_held_after(&home, recorded);
        // The CLI only reports when it returns; it does not publish its handover wait.
        std::thread::sleep(HALF_WAY);
        assert!(
            stop.running(),
            "{label}: it answered before the other daemon recorded itself"
        );
        stand_in.record();
        handing_over.end();
        let (code, stdout, stderr) = finished(stop);
        assert_eq!(code, 1, "{label}: {stdout}{stderr}");
        let said = said(&stderr);
        assert!(
            said.contains(&format!("pid {}", stand_in.pid())) && said.contains("left running"),
            "{label}: {said}"
        );
        assert!(
            stand_in.child.try_wait().unwrap().is_none(),
            "{label}: the other daemon was stopped"
        );
    }
}

/// After the handover of a daemon whose successor a service manager starts, a daemon that takes
/// the lock and records itself as one started otherwise, as `start` would, is not that successor:
/// `stop` leaves it running and says so.
#[test]
fn stop_leaves_a_daemon_that_is_not_the_service_managed_successor_running() {
    let home = ScratchDir::new("stop-not-successor");
    let data = home.join(".octoboard");
    std::fs::create_dir_all(&data).unwrap();
    // A daemon that has exited for its service manager to start the next one: its process is
    // gone, and its note was written just now.
    let this = Owner::current(PORT, false, true).unwrap();
    let exited = Owner {
        identity: crate::platform::proc::ProcessIdentity {
            start_micros: this.identity.start_micros.wrapping_add(1),
            ..this.identity
        },
        ..this
    }
    .with_service_manager(true);
    // The other daemon holds the lock, unrecorded, while `stop` waits through the handover, and
    // records itself once `stop` has been seen still running. The note is written again until
    // `stop` is done, so that it is within the grace whenever `stop` looks.
    let mut stand_in = StandIn::start_held(&home);
    exited.note_handover_in(&data).unwrap();
    let renewal = Renewal::start(exited, data);
    let mut stop = command(&home, "stop");
    // The CLI only reports when it returns; it does not publish its handover wait.
    std::thread::sleep(HALF_WAY);
    assert!(
        stop.running(),
        "it answered before the other daemon recorded itself"
    );
    stand_in.record();
    let (code, stdout, stderr) = finished(stop);
    drop(renewal);
    assert_eq!(code, 1, "{stdout}{stderr}");
    let said = said(&stderr);
    assert!(
        said.contains(&format!("pid {}", stand_in.pid())) && said.contains("is left running"),
        "{said}"
    );
    assert!(
        stand_in.child.try_wait().unwrap().is_none(),
        "the other daemon was stopped"
    );
}

/// `status` does not say no daemon runs while a handover is half-way, neither while the lock is
/// free between a daemon that noted it hands over and its successor, nor while the successor holds
/// it without a record: it reports the successor once that one has recorded itself.
#[test]
fn status_waits_through_the_free_lock_of_a_handover_for_the_successor() {
    let home = ScratchDir::new("status-handover");
    let data = home.join(".octoboard");
    std::fs::create_dir_all(&data).unwrap();
    let handing_over = Owner::current(PORT, false, true)
        .unwrap()
        .with_service_manager(false);
    handing_over.note_handover_in(&data).unwrap();
    let mut status = command(&home, "status");
    // Still waiting with the lock free after the note.
    // The CLI only reports when it returns; it does not publish its handover wait.
    std::thread::sleep(HALF_WAY);
    assert!(status.running(), "it answered with the lock free");
    let mut stand_in = StandIn::start_held(&home);
    // And still waiting with the lock held and no record, which only a note of the handover the
    // command saw justifies.
    // The CLI only reports when it returns; it does not publish its handover wait.
    std::thread::sleep(HALF_WAY);
    assert!(
        status.running(),
        "it answered with the lock held, unrecorded"
    );
    stand_in.record();
    let (code, stdout, stderr) = finished(status);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert_eq!(
        said(&stdout),
        format!(
            "{} is running: pid {}, http://127.0.0.1:{PORT}",
            crate::APP_NAME,
            stand_in.pid()
        ),
        "{stderr}"
    );
}

/// Once the note of a handover is gone, `status` waits no longer on a daemon that holds the lock
/// and records nothing, which the note's removal says will not record: it answers that the daemon
/// runs at an address not known, rather than wait out the 90 seconds.
#[test]
fn status_does_not_wait_on_an_unrecorded_daemon_once_the_handover_note_is_gone() {
    let home = ScratchDir::new("status-note-gone");
    let data = home.join(".octoboard");
    std::fs::create_dir_all(&data).unwrap();
    let handing_over = Owner::current(PORT, false, true)
        .unwrap()
        .with_service_manager(false);
    handing_over.note_handover_in(&data).unwrap();
    let _stand_in = StandIn::start_held(&home);
    let mut status = command(&home, "status");
    // Waiting while the note stands.
    // The CLI only reports when it returns; it does not publish its handover wait.
    std::thread::sleep(HALF_WAY);
    assert!(status.running(), "it answered while the note stood");
    std::fs::remove_file(data.join("daemon.handover")).unwrap();
    assert!(status.ends(), "it went on waiting with no note");
    let (code, stdout, stderr) = finished(status);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert_eq!(
        said(&stdout),
        format!("{} is running; its address is not known", crate::APP_NAME),
        "{stderr}"
    );
}

/// `start` against a daemon that holds the lock without having recorded itself waits for its
/// record, and names it, rather than refusing with an address it does not know.
#[test]
fn start_names_a_daemon_that_records_itself_while_it_waits() {
    let home = ScratchDir::new("start-unrecorded");
    let stand_in = StandIn::start(&home, HALF_WAY);
    let (code, stdout, stderr) = finished(command(&home, &format!("start --port {PORT}")));
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert_eq!(
        said(&stderr),
        format!(
            "{} is already running: pid {}, http://127.0.0.1:{PORT}",
            crate::APP_NAME,
            stand_in.pid()
        ),
        "{stdout}"
    );
}

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

/// `proxy` and `address --json` exit 3 with no daemon running, 4 with a daemon that has no owner
/// endpoint and 5 with one whose endpoint cannot be reached, each with the reason on stderr and
/// nothing of their own on stdout.
#[test]
fn the_owner_endpoint_modes_tell_apart_why_there_is_no_endpoint() {
    let assert_status = |home: &Path, expected: i32, reason: &str| {
        for arguments in ["proxy", "address --json"] {
            let (code, stdout, stderr) = finished(command(home, arguments));
            assert_eq!(code, expected, "{arguments}: {stdout}{stderr}");
            assert!(stderr.contains(reason), "{arguments}: {stderr}");
            assert!(
                !stdout.lines().any(|line| line.starts_with('{')),
                "{arguments}: {stdout}"
            );
        }
    };

    let home = ScratchDir::new("endpoint-none");
    assert_status(&home, 3, "is not running");

    let home = ScratchDir::new("endpoint-without");
    let _stand_in = StandIn::start_now(&home, false);
    assert_status(&home, 4, "has no owner endpoint");

    let home = ScratchDir::new("endpoint-unreachable");
    let _stand_in = StandIn::start_now(&home, true);
    assert_status(&home, 5, "cannot be reached");
}

/// Serves `router` on the browser socket of `home`'s data directory until dropped.
fn serve_endpoint(home: &Path, router: axum::Router) -> crate::browser::ServeTask {
    let data = home.join(".octoboard");
    std::fs::create_dir_all(&data).unwrap();
    let listener = crate::platform::local_socket::Listener::bind_replacing_stale(
        &crate::browser::socket::path_in(&data),
    )
    .unwrap();
    crate::browser::serve_on(listener, router)
}

/// An HTTP request written to `proxy`'s standard input is answered on its standard output as the
/// daemon's owner endpoint answers it, and the command exits 0 once the daemon closes.
#[tokio::test(flavor = "multi_thread")]
async fn proxy_carries_a_request_and_its_answer_through_its_standard_input_and_output() {
    let home = ScratchDir::new("proxy-process");
    let _served = serve_endpoint(&home, crate::browser::key_check_router());
    let home_path = home.to_path_buf();
    let (code, stdout, stderr) = tokio::task::spawn_blocking(move || {
        let _stand_in = StandIn::start_now(&home_path, true);
        let mut child = in_home(&home_path, "cli::background_tests::command_child")
            .env(ARGUMENTS, "proxy")
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        stdin
            .write_all(
                b"GET /key-check?key=good HTTP/1.1\r\nHost: 127.0.0.1:3369\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        drop(stdin);
        finished(CommandChild(Some(child)))
    })
    .await
    .unwrap();
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(
        stdout.contains("HTTP/1.1 204 No Content\r\n"),
        "{stdout}{stderr}"
    );
}

/// `address --json` prints the owner key in `owner.key`, with the generation and version the daemon
/// states in the hello it opens a control connection with on its owner endpoint; a key the
/// endpoint refuses is status 1 with nothing printed.
#[tokio::test(flavor = "multi_thread")]
async fn address_json_prints_the_owner_key_and_the_hello_of_the_endpoint() {
    use axum::extract::ws::{Message, WebSocketUpgrade};
    use axum::extract::RawQuery;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    const OWNER_KEY: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let home = ScratchDir::new("address-json");
    let router = axum::Router::new().route(
        "/ws/control",
        axum::routing::get(
            |RawQuery(query): RawQuery, ws: WebSocketUpgrade| async move {
                if query.as_deref() != Some(&format!("key={OWNER_KEY}")) {
                    return StatusCode::UNAUTHORIZED.into_response();
                }
                ws.on_upgrade(|mut socket| async move {
                    let hello = r#"{"type":"hello","generation":7,"daemon_version":"9.8.7"}"#;
                    let _ = socket.send(Message::Text(hello.into())).await;
                    while let Some(Ok(_)) = socket.recv().await {}
                })
                .into_response()
            },
        ),
    );
    let _served = serve_endpoint(&home, router);
    let home_path = home.to_path_buf();
    let (accepted, refused) = tokio::task::spawn_blocking(move || {
        let _stand_in = StandIn::start_now(&home_path, true);
        let data = home_path.join(".octoboard");
        crate::owner_key::write_in(&data, OWNER_KEY);
        let accepted = finished(command(&home_path, "address --json"));
        crate::owner_key::write_in(&data, &"0".repeat(64));
        let refused = finished(command(&home_path, "address --json"));
        (accepted, refused)
    })
    .await
    .unwrap();
    let (code, stdout, stderr) = accepted;
    assert_eq!(code, 0, "{stdout}{stderr}");
    let line = stdout
        .lines()
        .find(|line| line.starts_with('{'))
        .expect("a JSON line");
    assert_eq!(
        line,
        format!(r#"{{"owner_key":"{OWNER_KEY}","generation":7,"daemon_version":"9.8.7"}}"#)
    );
    let (code, stdout, stderr) = refused;
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert!(stderr.contains("refuses the owner key"), "{stderr}");
    assert!(
        !stdout.lines().any(|line| line.starts_with('{')),
        "{stdout}"
    );
}

/// A daemon that holds the lock without having recorded where it listens (it is starting or
/// stopping) has no known endpoint: `proxy` and `address --json` exit 1, saying so, and write
/// nothing of their own on standard output.
#[test]
fn the_owner_endpoint_modes_say_a_daemon_not_yet_recorded_is_status_1() {
    let home = ScratchDir::new("endpoint-unrecorded");
    let _stand_in = StandIn::start_held(&home);
    for arguments in ["proxy", "address --json"] {
        let (code, stdout, stderr) = finished(command(&home, arguments));
        assert_eq!(code, 1, "{arguments}: {stdout}{stderr}");
        assert!(stderr.contains("not known"), "{arguments}: {stderr}");
        assert_eq!(said(&stdout), "", "{arguments}");
        assert!(
            !stdout.lines().any(|line| line.starts_with('{')),
            "{arguments}: {stdout}"
        );
    }
}

/// The desktop application's daemon has an owner endpoint only while remote access is on: with it
/// off, `proxy` and `address --json` exit 4 and say which setting turns it on; with it on and the
/// endpoint not answering, 5.
#[test]
fn the_application_daemon_has_an_owner_endpoint_only_while_remote_access_is_on() {
    let home = ScratchDir::new("endpoint-application-off");
    let _stand_in = StandIn::start_application_now(&home, false);
    for arguments in ["proxy", "address --json"] {
        let (code, stdout, stderr) = finished(command(&home, arguments));
        assert_eq!(code, 4, "{arguments}: {stdout}{stderr}");
        assert!(
            stderr.contains("Allow remote access"),
            "{arguments}: {stderr}"
        );
        assert_eq!(said(&stdout), "", "{arguments}");
    }

    let home = ScratchDir::new("endpoint-application-on");
    let _stand_in = StandIn::start_application_now(&home, true);
    for arguments in ["proxy", "address --json"] {
        let (code, stdout, stderr) = finished(command(&home, arguments));
        assert_eq!(code, 5, "{arguments}: {stdout}{stderr}");
        assert!(
            stderr.contains("cannot be reached"),
            "{arguments}: {stderr}"
        );
        assert_eq!(said(&stdout), "", "{arguments}");
    }
}

/// `octoboardd` run as the copy inside the desktop application's bundle, with no daemon running,
/// says that the application is not running and exits 6, apart from the 3 any other copy exits
/// with: the application's daemon runs only while the application does.
#[cfg(target_os = "macos")]
#[test]
fn the_applications_own_copy_with_no_daemon_running_is_status_6() {
    let home = ScratchDir::new("endpoint-application-copy");
    let macos = home.join("Octoboard.app/Contents/MacOS");
    std::fs::create_dir_all(&macos).unwrap();
    let copy = macos.join("octoboardd");
    let exe = std::env::current_exe().unwrap();
    // A link rather than a copy of the whole test binary; where the scratch directory is on
    // another volume, a copy.
    if std::fs::hard_link(&exe, &copy).is_err() {
        std::fs::copy(&exe, &copy).unwrap();
    }
    for arguments in ["proxy", "address --json"] {
        let child = Command::new(&copy)
            .args(["--exact", "cli::background_tests::command_child"])
            .args(["--ignored", "--nocapture"])
            .env("HOME", &*home)
            .env(ARGUMENTS, arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let (code, stdout, stderr) = finished(CommandChild(Some(child)));
        assert_eq!(code, 6, "{arguments}: {stdout}{stderr}");
        assert!(
            stderr.contains("application is not running"),
            "{arguments}: {stderr}"
        );
        assert_eq!(said(&stdout), "", "{arguments}");
    }
    // Any other copy exits 3.
    let (code, stdout, stderr) = finished(command(&home, "proxy"));
    assert_eq!(code, 3, "{stdout}{stderr}");
}

/// `address --json` with no usable owner key in `owner.key` exits 1 with nothing printed, though
/// the endpoint answers: it has no key to ask the daemon with.
#[tokio::test(flavor = "multi_thread")]
async fn address_json_without_a_usable_owner_key_is_status_1() {
    let home = ScratchDir::new("address-json-no-key");
    let _served = serve_endpoint(&home, crate::browser::key_check_router());
    let home_path = home.to_path_buf();
    let results = tokio::task::spawn_blocking(move || {
        let _stand_in = StandIn::start_now(&home_path, true);
        let missing = finished(command(&home_path, "address --json"));
        crate::owner_key::write_in(&home_path.join(".octoboard"), "not a key");
        let malformed = finished(command(&home_path, "address --json"));
        (missing, malformed)
    })
    .await
    .unwrap();
    for (code, stdout, stderr) in [results.0, results.1] {
        assert_eq!(code, 1, "{stdout}{stderr}");
        assert!(stderr.contains("owner.key"), "{stderr}");
        assert!(
            !stdout.lines().any(|line| line.starts_with('{')),
            "{stdout}"
        );
    }
}

/// An owner endpoint that serves `first_frame` to a control connection opened with `OWNER_KEY`.
fn control_router_opening_with(first_frame: &'static str) -> axum::Router {
    use axum::extract::ws::{Message, WebSocketUpgrade};
    use axum::extract::RawQuery;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    axum::Router::new().route(
        "/ws/control",
        axum::routing::get(
            move |RawQuery(query): RawQuery, ws: WebSocketUpgrade| async move {
                if query.as_deref() != Some(&format!("key={OWNER_KEY}")) {
                    return StatusCode::UNAUTHORIZED.into_response();
                }
                ws.on_upgrade(move |mut socket| async move {
                    let _ = socket.send(Message::Text(first_frame.into())).await;
                    while let Some(Ok(_)) = socket.recv().await {}
                })
                .into_response()
            },
        ),
    )
}

/// The owner key the endpoints of the tests below accept.
const OWNER_KEY: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

/// What `address --json` finds of the daemon's first frame: a hello of an earlier generation, of
/// which fields may have been added, is read as the hello it is, since its shape is frozen across
/// generations; a first frame that is no hello is status 1 with nothing printed.
#[tokio::test(flavor = "multi_thread")]
async fn address_json_reads_the_hello_of_any_generation_and_nothing_else() {
    let home = ScratchDir::new("address-json-frames");
    let older = r#"{"type":"hello","generation":4,"daemon_version":"0.0.9","added_later":[1]}"#;
    let (code, stdout, stderr) = address_json_against(&home, older).await;
    assert_eq!(code, 0, "{stdout}{stderr}");
    let line = stdout
        .lines()
        .find(|line| line.starts_with('{'))
        .expect("a JSON line");
    assert_eq!(
        line,
        format!(r#"{{"owner_key":"{OWNER_KEY}","generation":4,"daemon_version":"0.0.9"}}"#)
    );
    assert_eq!(
        stdout.lines().filter(|line| line.starts_with('{')).count(),
        1
    );

    let home = ScratchDir::new("address-json-no-hello");
    let (code, stdout, stderr) =
        address_json_against(&home, r#"{"type":"snapshot","hosts":[]}"#).await;
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert!(stderr.contains("could not be asked"), "{stderr}");
    assert!(
        !stdout.lines().any(|line| line.starts_with('{')),
        "{stdout}"
    );
}

/// Runs `address --json` against a stand-in daemon whose owner endpoint opens a control connection
/// with `first_frame`, `owner.key` holding [`OWNER_KEY`].
async fn address_json_against(
    home: &ScratchDir,
    first_frame: &'static str,
) -> (i32, String, String) {
    let _served = serve_endpoint(home, control_router_opening_with(first_frame));
    let home_path = home.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let _stand_in = StandIn::start_now(&home_path, true);
        crate::owner_key::write_in(&home_path.join(".octoboard"), OWNER_KEY);
        finished(command(&home_path, "address --json"))
    })
    .await
    .unwrap()
}

/// `proxy` ends with status 0 once the daemon closes the connection while its own input is still
/// open, and carries what the daemon said before closing.
#[tokio::test(flavor = "multi_thread")]
async fn proxy_ends_with_status_0_when_the_daemon_closes_first() {
    let home = ScratchDir::new("proxy-daemon-closes");
    let _served = serve_endpoint(&home, crate::browser::key_check_router());
    let home_path = home.to_path_buf();
    let (code, stdout, stderr) = tokio::task::spawn_blocking(move || {
        let _stand_in = StandIn::start_now(&home_path, true);
        let mut child = in_home(&home_path, "cli::background_tests::command_child")
            .env(ARGUMENTS, "proxy")
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        // `Connection: close` has the daemon close after answering; the input stays open.
        stdin
            .write_all(
                b"GET /key-check?key=bad HTTP/1.1\r\nHost: 127.0.0.1:3369\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        stdin.flush().unwrap();
        let held_open = stdin;
        let finished = finished(CommandChild(Some(child)));
        drop(held_open);
        finished
    })
    .await
    .unwrap();
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(
        stdout.contains("HTTP/1.1 401 Unauthorized\r\n"),
        "{stdout}{stderr}"
    );
}

/// What `octoboardd connection <arguments>` prints, run as a process of its own in `home` with
/// the SSH session `ssh_connection` (none when `None`): its status, standard output and standard
/// error.
fn connection_with(
    home: &Path,
    arguments: &str,
    ssh_connection: Option<&str>,
) -> (i32, String, String) {
    let mut command = in_home(home, "cli::background_tests::command_child");
    command.env(ARGUMENTS, format!("connection {arguments}"));
    match ssh_connection {
        Some(session) => command.env("SSH_CONNECTION", session),
        None => command.env_remove("SSH_CONNECTION"),
    };
    finished(CommandChild(Some(command.spawn().unwrap())))
}

/// The line of `stdout` that is the connection details, which there is exactly one of.
fn details_line(stdout: &str) -> &str {
    let lines: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("octoboard+ssh://"))
        .collect();
    assert_eq!(lines.len(), 1, "{stdout}");
    lines[0]
}

/// `connection --text` prints the one line of text alone, `connection` the QR code, an empty line
/// and then that line; the host and port are the ones given, else the SSH session's, `--host`
/// alone keeping the session's port; and it touches nothing, so it runs with no daemon and creates
/// no data directory.
#[test]
fn connection_prints_the_details_as_text_after_the_qr_code_and_touches_nothing() {
    let home = ScratchDir::new("connection-process");
    let session = Some("192.0.2.9 51000 198.51.100.7 2200");

    let (code, stdout, stderr) =
        connection_with(&home, "--host box.example --ssh-port 2222 --text", session);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let text = details_line(&stdout).to_owned();
    assert!(text.contains("@box.example:2222"), "{text}");
    assert!(!text.contains(char::is_whitespace), "{text}");
    assert!(
        stderr
            .lines()
            .last()
            .is_some_and(|note| note.contains("Connection details")),
        "the note on pasting is last: {stderr}"
    );
    assert!(
        !stderr.contains("the address this SSH session reached"),
        "a host given is not explained: {stderr}"
    );

    let (code, stdout, stderr) = connection_with(&home, "--host box.example --ssh-port 2222", None);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    let at = lines
        .iter()
        .position(|line| *line == text)
        .expect("the same text");
    assert_eq!(lines[at - 1], "", "an empty line before the text");
    let code_lines: Vec<&&str> = lines[..at - 1]
        .iter()
        .filter(|line| line.contains(['▀', '▄', '█']))
        .collect();
    assert!(code_lines.len() > 8, "the QR code: {stdout}");
    assert!(
        code_lines.iter().all(|line| !line.contains('\x1b')),
        "no color where standard output is no terminal"
    );
    assert_eq!(at, lines.len() - 1, "nothing after the text");

    let (code, stdout, stderr) = connection_with(&home, "--text", session);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(details_line(&stdout).contains("@198.51.100.7:2200"));
    assert!(
        stderr.contains("this SSH session") && stderr.contains("--host"),
        "{stderr}"
    );

    let (code, stdout, stderr) = connection_with(&home, "--host box.example --text", session);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(
        details_line(&stdout).contains("@box.example:2200"),
        "the session's port stays: {stdout}"
    );

    assert!(
        !home.join(".octoboard").exists(),
        "creates no data directory"
    );
}

/// The entry's name in the details is the name the machine gives itself, left out where that is
/// the host.
#[test]
fn connection_names_the_entry_after_the_machine_unless_that_is_the_host() {
    let Some(machine) = crate::platform::net::host_name() else {
        return;
    };
    if machine.contains(char::is_whitespace) {
        // The arguments of the child are split at spaces.
        return;
    }
    let home = ScratchDir::new("connection-name");
    let (code, stdout, stderr) = connection_with(&home, &format!("--host {machine} --text"), None);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(
        !details_line(&stdout).contains("name="),
        "the host is the name: {stdout}"
    );
    let (code, stdout, stderr) = connection_with(&home, "--host other.example --text", None);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(details_line(&stdout).contains("name="), "{stdout}");
}

/// `address` for the desktop application's daemon prints the browser address at the fixed forward
/// port, not at the port the system assigned the daemon, while remote access is on; with it off it
/// prints none, says which setting turns it on and exits 1, and `--rotate` replaces the key for
/// later and exits 0.
#[tokio::test(flavor = "multi_thread")]
async fn address_for_the_application_daemon_follows_remote_access() {
    let home = ScratchDir::new("address-application-on");
    let _served = serve_endpoint(&home, crate::browser::key_check_router());
    let home_path = home.to_path_buf();
    let (on, off, rotated, after) = tokio::task::spawn_blocking(move || {
        let data = home_path.join(".octoboard");
        crate::owner_key::write_in(&data, OWNER_KEY);
        let on = {
            let _stand_in = StandIn::start_application_now(&home_path, true);
            finished(command(&home_path, "address"))
        };
        let (off, rotated, after) = {
            let _stand_in = StandIn::start_application_now(&home_path, false);
            let off = finished(command(&home_path, "address"));
            let kept = crate::owner_key::read_in(&data);
            let rotated = finished(command(&home_path, "address --rotate"));
            (off, (kept, rotated), crate::owner_key::read_in(&data))
        };
        (on, off, rotated, after)
    })
    .await
    .unwrap();

    let (code, stdout, stderr) = on;
    assert_eq!(code, 0, "{stdout}{stderr}");
    let line = stdout
        .lines()
        .find(|line| line.starts_with("http://"))
        .expect("an address");
    let fixed = crate::coordinator::remote_access::FORWARD_PORT;
    assert_eq!(line, format!("http://127.0.0.1:{fixed}/?key={OWNER_KEY}"));

    let (code, stdout, stderr) = off;
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert!(
        !stdout.lines().any(|line| line.starts_with("http://")),
        "{stdout}"
    );
    assert!(stderr.contains("Allow remote access"), "{stderr}");

    let (kept, (code, stdout, stderr)) = rotated;
    assert_eq!(
        kept.as_deref(),
        Some(OWNER_KEY),
        "`address` replaced nothing"
    );
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(
        !stdout.lines().any(|line| line.starts_with("http://")),
        "{stdout}"
    );
    let after = after.expect("a key");
    assert_ne!(after, OWNER_KEY);
}
