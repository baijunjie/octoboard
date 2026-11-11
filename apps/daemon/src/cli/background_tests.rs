//! `stop`, `status` and `start` against a daemon caught half-way through a planned restart: one
//! that holds the lock but has not recorded itself yet, and the free lock between a daemon that
//! noted it hands over and its successor.
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
use crate::owner_record::Owner;
use crate::test_support::{ScratchDir, PATIENCE, TEST_KEY};

/// The variable [`command_child`] takes its command line from, its arguments apart by spaces.
const ARGUMENTS: &str = "OCTOBOARD_TEST_COMMAND_ARGUMENTS";

/// The variable [`stand_in_child`] takes how long to hold the lock before it records itself from,
/// in milliseconds.
const RECORD_AFTER: &str = "OCTOBOARD_TEST_STAND_IN_RECORD_AFTER_MS";

/// The variable that, set, has [`stand_in_child`] hold the lock without recording itself until it
/// is told to on its stdin ([`StandIn::record`]), in place of [`RECORD_AFTER`].
const RECORD_WHEN_TOLD: &str = "OCTOBOARD_TEST_STAND_IN_RECORD_WHEN_TOLD";

/// The port the stand-in records; nothing listens there, and none of these commands connects.
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
/// its key after [`RECORD_AFTER`] (or once told, see [`RECORD_WHEN_TOLD`]), removes the note of a
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
    let owner = Owner::current(PORT, false, true)
        .unwrap()
        .with_service_manager(false);
    owner.record_key(TEST_KEY).unwrap();
    owner.record().unwrap();
    crate::owner_record::settle_handover();
    println!("recorded");
    while std::io::stdin()
        .read_line(&mut line)
        .is_ok_and(|read| read > 0)
    {}
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

    /// Starts one that records itself only once [`StandIn::record`] tells it to, and returns once
    /// it holds the lock.
    fn start_held(home: &Path) -> Self {
        Self::spawn(home, |command| {
            command.env(RECORD_WHEN_TOLD, "1");
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
/// that one.
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
    std::thread::sleep(HALF_WAY);
    let mut stand_in = StandIn::start(&home, Duration::ZERO);
    let (code, stdout, stderr) = finished(stop);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert_eq!(
        said(&stdout),
        format!("{} stopped", crate::APP_NAME),
        "{stderr}"
    );
    assert!(stand_in.ends(), "the successor still runs");
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
    std::thread::sleep(HALF_WAY);
    assert!(status.running(), "it answered with the lock free");
    let mut stand_in = StandIn::start_held(&home);
    // And still waiting with the lock held and no record, which only a note of the handover the
    // command saw justifies.
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
