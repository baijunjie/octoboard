//! The tests of what `spawn` does of its own: the tests start this test binary again as a probe
//! (`a_pty_probe`) on the terminal, which writes down what it finds of itself to a file, so that
//! nothing is read off the terminal.

use std::collections::BTreeMap;
use std::ffi::{CString, OsString};
use std::fmt::Write as _;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::time::Instant;

#[cfg(target_os = "linux")]
use super::pty_size;
use super::spawn;
use super::start::process_of;
#[cfg(target_os = "linux")]
use super::start::spawn_on_slave;
use crate::platform::fs;
use crate::platform::proc::Containment;
#[cfg(target_os = "linux")]
use crate::platform::proc::ProcessId;
use crate::platform::term::{Command, Size, Spawned};
use crate::platform::test_helper::{self, PID_FILE, ROLE};
use crate::session::wait::wait_for;
#[cfg(target_os = "linux")]
use crate::test_support::cgroups::{cgroup_of, grouped_launch};
#[cfg(target_os = "linux")]
use crate::test_support::{assert_detached_session_leader, assert_gone_by, wait_for_pid};
use crate::test_support::{symlink, KillOnDrop, ScratchDir, PATIENCE};

const SIZE: Size = Size { rows: 24, cols: 80 };
const PROBE: &str = "pty_probe";
const SPAWNER: &str = "pty_spawner";

/// The process the tests of [`spawn`] start on a terminal: run as `ROLE` says, and a no-op in
/// an ordinary test run. As `pty_probe` it writes what it finds of itself to the file
/// `PID_FILE` names, whole and through a rename, as `key=value` lines, and then waits to be
/// ended: its process, session, terminal, working directory, argument zero, environment
/// (`env.NAME`), the descriptors it holds above the standard three (`fds`), what `SIGALRM`
/// does, and, on Linux, its cgroup.
#[test]
fn a_pty_probe() {
    let (Ok(role), Some(report)) = (std::env::var(ROLE), std::env::var_os(PID_FILE)) else {
        return;
    };
    if role != PROBE {
        return;
    }
    let mut out = String::new();
    // SAFETY: queries about this process and its descriptors, and the opening and closing of
    // one descriptor of its own; nothing is written.
    unsafe {
        writeln!(out, "pid={}", libc::getpid()).unwrap();
        writeln!(out, "sid={}", libc::getsid(0)).unwrap();
        writeln!(out, "pgid={}", libc::getpgrp()).unwrap();
        writeln!(out, "foreground={}", libc::tcgetpgrp(0)).unwrap();
        let all_terminals = (0..3).all(|fd| libc::isatty(fd) == 1);
        writeln!(out, "stdio_is_terminal={all_terminals}").unwrap();
        let tty = libc::open(c"/dev/tty".as_ptr(), libc::O_RDWR | libc::O_NOCTTY);
        writeln!(out, "controlling_terminal={}", tty >= 0).unwrap();
        if tty >= 0 {
            libc::close(tty);
        }
        let open: Vec<String> = (3..256)
            .filter(|&fd| libc::fcntl(fd, libc::F_GETFD) != -1)
            .map(|fd| fd.to_string())
            .collect();
        writeln!(out, "fds={}", open.join(",")).unwrap();
        let mut action: libc::sigaction = std::mem::zeroed();
        libc::sigaction(libc::SIGALRM, std::ptr::null(), &mut action);
        let disposition = match action.sa_sigaction {
            libc::SIG_DFL => "default",
            libc::SIG_IGN => "ignored",
            _ => "handler",
        };
        writeln!(out, "sigalrm={disposition}").unwrap();
    }
    let argv0 = std::env::args_os().next().unwrap_or_default();
    writeln!(out, "argv0={}", argv0.to_string_lossy()).unwrap();
    let cwd = std::env::current_dir().unwrap_or_default();
    writeln!(out, "cwd={}", cwd.display()).unwrap();
    #[cfg(target_os = "linux")]
    {
        let cgroup = std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
        writeln!(out, "cgroup={}", cgroup.replace('\n', "|")).unwrap();
    }
    let vars: BTreeMap<_, _> = std::env::vars_os().collect();
    for (name, value) in vars {
        writeln!(
            out,
            "env.{}={}",
            name.to_string_lossy(),
            value.to_string_lossy()
        )
        .unwrap();
    }
    let partial = Path::new(&report).with_extension("partial");
    std::fs::write(&partial, out).expect("the report is written");
    std::fs::rename(partial, &report).expect("the report is in place");
    std::thread::sleep(PATIENCE * 2);
}

/// What the probe wrote down.
struct Report(BTreeMap<String, String>);

impl Report {
    fn get(&self, key: &str) -> &str {
        self.0
            .get(key)
            .unwrap_or_else(|| panic!("no {key} in {:?}", self.0))
    }

    fn env(&self, name: &str) -> Option<&str> {
        self.0.get(&format!("env.{name}")).map(String::as_str)
    }

    /// The names of the environment variables the process has.
    fn env_names(&self) -> Vec<&str> {
        self.0
            .keys()
            .filter_map(|key| key.strip_prefix("env."))
            .collect()
    }
}

/// Makes `command` start the probe, reporting to `report`, with what it is started with
/// otherwise left as it is.
fn run_probe_as(command: &mut Command, report: &Path) {
    command
        .args(test_helper::args(
            "platform::term::unix::start_tests::a_pty_probe",
        ))
        .env(ROLE, PROBE)
        .env(PID_FILE, report);
}

/// The probe, as a command: this test binary, by its absolute path.
fn probe_command(report: &Path) -> Command {
    let mut command = Command::new(test_helper::program());
    run_probe_as(&mut command, report);
    command
}

fn read_report(report: &Path) -> Report {
    let mut text = String::new();
    assert!(
        wait_for(PATIENCE, || match std::fs::read_to_string(report) {
            Ok(read) => {
                text = read;
                true
            }
            Err(_) => false,
        }),
        "the probe never reported"
    );
    Report(
        text.lines()
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect(),
    )
}

/// A probe started on a terminal by [`spawn`], with what it reported.
struct Probe {
    spawned: Spawned,
    report: Report,
    guard: KillOnDrop,
}

fn start_probe(command: Command, report: &Path, containment: Option<&Containment>) -> Probe {
    let spawned = spawn(command, SIZE, containment).expect("the probe starts on a terminal");
    let guard = KillOnDrop(vec![spawned.pid.0 as i32]);
    // What the probe prints is read and dropped, so it never blocks writing to a terminal
    // whose buffer is full, which on macOS holds only about 1 KB.
    let reader = spawned.pty.reader();
    std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        while matches!(reader.read_chunk(&mut buf), Ok(Some(n)) if n > 0) {}
    });
    let report = read_report(report);
    Probe {
        spawned,
        report,
        guard,
    }
}

fn reap(spawned: &Spawned) {
    assert!(
        wait_for(PATIENCE, || spawned
            .child
            .reap_if_exited()
            .expect("the child is reaped")),
        "the child kept running"
    );
}

impl Probe {
    /// Ends the probe and reaps it.
    fn end(mut self) {
        assert!(self.spawned.child.kill().delivered());
        reap(&self.spawned);
        self.spawned.pty.stop_reader();
        self.guard.0.clear();
    }
}

/// The probe as the default command starts it, over the directory's report.
fn plain_probe(dir: &ScratchDir, containment: Option<&Containment>) -> Probe {
    let report = dir.join("report");
    start_probe(probe_command(&report), &report, containment)
}

fn canonical(path: impl AsRef<Path>) -> PathBuf {
    std::fs::canonicalize(path.as_ref()).expect("the path exists")
}

/// The process leads a session of its own, in a process group of its own, with the terminal as
/// its controlling terminal and its standard streams, so that a Ctrl-C and a resize reach it;
/// with a containment that has no group, the same.
#[test]
fn a_process_leads_a_session_of_its_own_with_the_terminal_as_its_controlling_terminal() {
    let tag_only =
        Containment::create("term-spawn", &format!("tag-{}", std::process::id())).unwrap();
    for containment in [None, Some(&tag_only)] {
        let dir = ScratchDir::new("pty-session");
        let probe = plain_probe(&dir, containment);
        let pid = probe.spawned.pid.0.to_string();
        assert_eq!(probe.report.get("pid"), pid);
        assert_eq!(probe.report.get("sid"), pid, "a session leader");
        assert_eq!(probe.report.get("pgid"), pid, "a process group of its own");
        assert_eq!(
            probe.report.get("foreground"),
            pid,
            "the foreground job of its terminal"
        );
        assert_eq!(probe.report.get("stdio_is_terminal"), "true");
        assert_eq!(probe.report.get("controlling_terminal"), "true");
        probe.end();
    }
}

/// A descriptor the daemon holds that is not close-on-exec does not reach the process: only
/// the standard three do.
#[test]
fn descriptors_beyond_the_standard_three_are_not_inherited() {
    let leaked: Vec<OwnedFd> = (0..2)
        .map(|_| {
            // SAFETY: opens a descriptor nothing else owns, and hands it to the `OwnedFd`.
            let fd = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY) };
            assert!(fd > 2, "a descriptor above the standard three");
            // SAFETY: `open` succeeded, so `fd` is open and owned by nothing else.
            unsafe { OwnedFd::from_raw_fd(fd) }
        })
        .collect();
    for fd in &leaked {
        // SAFETY: `fd` is open for as long as this call.
        let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFD) };
        assert_eq!(flags & libc::FD_CLOEXEC, 0, "a descriptor that would leak");
    }

    let dir = ScratchDir::new("pty-fds");
    let probe = plain_probe(&dir, None);
    assert_eq!(probe.report.get("fds"), "");
    probe.end();
    drop(leaked);
}

/// The process that ignores `SIGALRM` and then starts the probe on a terminal, as the daemon
/// would, run as `ROLE` says (`PID_FILE` being the probe's report) and a no-op in an ordinary
/// test run. It is a process of its own so that the disposition it sets is not the test
/// process's, which every process started concurrently by another test inherits.
#[test]
fn a_pty_spawner_that_ignores_a_signal() {
    let (Ok(role), Some(report)) = (std::env::var(ROLE), std::env::var_os(PID_FILE)) else {
        return;
    };
    if role != SPAWNER {
        return;
    }
    // SAFETY: ignoring a signal nothing in this process waits for.
    unsafe { libc::signal(libc::SIGALRM, libc::SIG_IGN) };
    let report = PathBuf::from(report);
    start_probe(probe_command(&report), &report, None).end();
}

/// A signal the daemon ignores is the default again in the process, as for any program started
/// on a terminal: an ignored disposition would otherwise outlive the exec. The daemon is a
/// process the test starts for the purpose ([`a_pty_spawner_that_ignores_a_signal`]), with
/// `SIGALRM` the signal it ignores.
#[test]
fn a_signal_the_daemon_ignores_is_default_in_the_process() {
    let dir = ScratchDir::new("pty-signals");
    let report = dir.join("report");
    let status = std::process::Command::new(test_helper::program())
        .args(test_helper::args(
            "platform::term::unix::start_tests::a_pty_spawner_that_ignores_a_signal",
        ))
        .env(ROLE, SPAWNER)
        .env(PID_FILE, &report)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("the spawner runs");
    assert!(status.success(), "the spawner failed: {status}");
    assert_eq!(read_report(&report).get("sigalrm"), "default");
}

/// A relative program is the file of that name in the working directory if there is one, else
/// the first executable of that name on the `PATH` the process will have, and runs under the
/// name it was given.
#[test]
fn a_relative_program_is_looked_up_in_the_directory_then_on_the_path() {
    let linked = ScratchDir::new("pty-linked");
    symlink(test_helper::program(), linked.join("probe-link")).expect("the link");
    let elsewhere = ScratchDir::new("pty-elsewhere");
    let decoy = ScratchDir::new("pty-decoy");
    symlink("/bin/sh", decoy.join("probe-link")).expect("the decoy");

    // On the path the process will have, not in its directory.
    let report = elsewhere.join("report");
    let mut command = Command::new("probe-link");
    run_probe_as(&mut command, &report);
    command.cwd(&*elsewhere).env("PATH", &*linked);
    let probe = start_probe(command, &report, None);
    assert_eq!(probe.report.get("argv0"), "probe-link");
    assert_eq!(
        canonical(probe.report.get("cwd")),
        canonical(&*elsewhere),
        "started in the directory asked for"
    );
    probe.end();

    // In its directory, ahead of a program of that name on the path.
    let report = linked.join("report");
    let mut command = Command::new("probe-link");
    run_probe_as(&mut command, &report);
    command.cwd(&*linked).env("PATH", &*decoy);
    let probe = start_probe(command, &report, None);
    assert_eq!(probe.report.get("argv0"), "probe-link");
    probe.end();
}

/// An absolute program runs under the path it was given.
#[test]
fn an_absolute_program_runs_under_the_path_it_was_given() {
    let dir = ScratchDir::new("pty-absolute");
    let link = dir.join("probe-link");
    symlink(test_helper::program(), &link).expect("the link");
    let report = dir.join("report");
    let mut command = Command::new(&link);
    run_probe_as(&mut command, &report);
    let probe = start_probe(command, &report, None);
    assert_eq!(probe.report.get("argv0"), link.to_string_lossy());
    probe.end();
}

/// A program that is not there, is not on the path, or is not executable is an error that
/// names it, and nothing is started.
#[test]
fn a_program_that_cannot_be_run_is_an_error_before_anything_starts() {
    let dir = ScratchDir::new("pty-missing");
    let report = dir.join("report");
    let plain = dir.join("not-executable");
    std::fs::write(&plain, "#!/bin/sh\n").expect("the file");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    let missing = dir.join("missing");
    let cases = [
        (
            "a relative program found nowhere",
            "no-such-program-anywhere",
            None,
        ),
        (
            "an absolute program that is missing",
            missing.to_str().unwrap(),
            None,
        ),
        (
            "an absolute program that is not executable",
            plain.to_str().unwrap(),
            None,
        ),
        (
            "a relative program that is on the path but not executable",
            "not-executable",
            Some(&*dir),
        ),
    ];
    for (what, program, path) in cases {
        let mut command = Command::new(program);
        run_probe_as(&mut command, &report);
        command.cwd("/");
        if let Some(path) = path {
            command.env("PATH", path);
        }
        let error = spawn(command, SIZE, None)
            .err()
            .unwrap_or_else(|| panic!("{what} started"));
        assert!(
            error.to_string().contains(program),
            "{what}: the error does not name the program: {error}"
        );
    }
    assert!(!report.exists(), "something ran");
}

/// The process starts in the working directory the command asks for if that is a directory,
/// and in the home directory (`HOME`) where there is none, it is not a directory, or it does
/// not exist.
#[test]
fn the_working_directory_is_the_one_asked_for_if_it_is_a_directory_and_home_otherwise() {
    let wanted = ScratchDir::new("pty-cwd");
    let home = ScratchDir::new("pty-home");
    let file = wanted.join("a-file");
    std::fs::write(&file, "").expect("the file");
    let missing = wanted.join("missing");
    let cases: [(Option<&Path>, &Path, &str); 4] = [
        (Some(&wanted), &wanted, "a directory"),
        (Some(&missing), &home, "a missing directory"),
        (Some(&file), &home, "a file"),
        (None, &home, "none"),
    ];
    for (asked, expected, what) in cases {
        let dir = ScratchDir::new("pty-cwd-report");
        let report = dir.join("report");
        let mut command = probe_command(&report);
        command.env("HOME", &*home);
        if let Some(asked) = asked {
            command.cwd(asked);
        }
        let probe = start_probe(command, &report, None);
        assert_eq!(
            canonical(probe.report.get("cwd")),
            canonical(expected),
            "{what}"
        );
        probe.end();
    }
}

/// The environment is the daemon's with the command's on top of it.
#[test]
fn the_environment_is_the_daemon_s_with_the_command_s_on_top() {
    let dir = ScratchDir::new("pty-env");
    let report = dir.join("report");
    let mut command = probe_command(&report);
    command
        .env("OCTOBOARD_TEST_PTY_VAR", "from the command")
        .env("PATH", "/probe/path");
    let probe = start_probe(command, &report, None);
    assert_eq!(
        probe.report.env("OCTOBOARD_TEST_PTY_VAR"),
        Some("from the command")
    );
    assert_eq!(
        probe.report.env("PATH"),
        Some("/probe/path"),
        "the command's wins"
    );
    if let Some(home) = std::env::var_os("HOME") {
        assert_eq!(
            probe.report.env("HOME"),
            home.to_str(),
            "what the daemon has is inherited"
        );
    }
    probe.end();
}

/// A command that clears its environment starts over only what it sets afterwards, and the
/// shell variable.
#[test]
fn a_command_that_clears_its_environment_starts_over_its_own() {
    let dir = ScratchDir::new("pty-env-clear");
    let report = dir.join("report");
    let mut command = Command::new(test_helper::program());
    command.env_clear();
    run_probe_as(&mut command, &report);
    command.env("OCTOBOARD_TEST_PTY_VAR", "kept");
    let probe = start_probe(command, &report, None);
    // macOS adds this one to any process it starts, whatever it was started with.
    let mut names: Vec<_> = probe
        .report
        .env_names()
        .into_iter()
        .filter(|name| *name != "__CF_USER_TEXT_ENCODING")
        .collect();
    names.sort_unstable();
    let mut expected = vec!["OCTOBOARD_TEST_PTY_VAR", PID_FILE, ROLE, "SHELL"];
    expected.sort_unstable();
    assert_eq!(names, expected);
    assert_eq!(probe.report.env("OCTOBOARD_TEST_PTY_VAR"), Some("kept"));
    probe.end();
}

/// The login shell the user database names for this user if it is executable, else `/bin/sh`.
fn login_shell() -> PathBuf {
    fs::account_shell()
        .filter(|shell| {
            CString::new(shell.as_os_str().as_bytes())
                // SAFETY: a valid C string.
                .is_ok_and(|path| unsafe { libc::access(path.as_ptr(), libc::X_OK) } == 0)
        })
        .unwrap_or_else(|| PathBuf::from("/bin/sh"))
}

/// `SHELL` is the one the command sets; where it sets none, the user's login shell, or
/// `/bin/sh` where that is not executable, in whatever environment the daemon hands on and
/// whether or not the command starts over its own.
#[test]
fn the_shell_is_the_command_s_own_else_the_login_shell() {
    let own = test_helper::program();
    assert_ne!(own, login_shell());
    let daemon_shell = [(
        OsString::from("SHELL"),
        OsString::from("/the/daemons/own/shell"),
    )];
    let shell_of = |command: &Command| {
        let process = process_of(command, daemon_shell.clone()).expect("the process");
        process
            .get_envs()
            .find(|(name, _)| *name == "SHELL")
            .and_then(|(_, value)| value.map(PathBuf::from))
    };
    let program = || Command::new("/bin/sh");

    // Handed on, the daemon's own `SHELL` is not the process's.
    assert_eq!(shell_of(&program()), Some(login_shell()));
    let mut clearing = program();
    clearing.env_clear();
    assert_eq!(shell_of(&clearing), Some(login_shell()));

    // The command's own wins, in either.
    let mut setting = program();
    setting.env("SHELL", &own);
    assert_eq!(shell_of(&setting), Some(own.clone()));
    setting.env_clear();
    setting.env("SHELL", &own);
    assert_eq!(shell_of(&setting), Some(own));
}

/// The process of the probe has the login shell for `SHELL` when its command starts over its own
/// environment and sets none.
#[test]
fn a_process_that_starts_over_its_environment_has_the_login_shell() {
    let dir = ScratchDir::new("pty-shell");
    let report = dir.join("report");
    let mut command = Command::new(test_helper::program());
    command.env_clear();
    run_probe_as(&mut command, &report);
    let probe = start_probe(command, &report, None);
    assert_eq!(
        probe.report.env("SHELL").map(PathBuf::from),
        Some(login_shell())
    );
    probe.end();
}

/// A process started on a PTY's slave without anything taking it on afterwards, as [`spawn`]
/// does: what its group holds is only what entered it before its program ran.
#[cfg(target_os = "linux")]
struct OnSlave {
    _pty: portable_pty::PtyPair,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    pid: u32,
    guard: KillOnDrop,
}

#[cfg(target_os = "linux")]
fn start_on_slave(command: &Command, containment: &Containment) -> OnSlave {
    use portable_pty::native_pty_system;
    let pty = native_pty_system().openpty(pty_size(SIZE)).expect("a PTY");
    let fd = pty.master.as_raw_fd().expect("a descriptor");
    let child = spawn_on_slave(command, fd, Some(containment)).expect("the process starts");
    let pid = child.process_id().expect("a pid");
    OnSlave {
        _pty: pty,
        child,
        pid,
        guard: KillOnDrop(vec![pid as i32]),
    }
}

/// A process on a terminal is in its launch's group from the start, with nothing moving it in
/// afterwards (`start_on_slave` does not take it on): the probe reports the cgroup it is in, and
/// it is the group's, with the descriptors above the standard three still closed.
#[cfg(target_os = "linux")]
#[test]
fn a_process_on_a_terminal_starts_in_its_launch_s_group() {
    let Some((_, containment)) = grouped_launch("term-spawn", "pty-in-group") else {
        return;
    };
    let dir = ScratchDir::new("pty-group");
    let report = dir.join("report");
    let mut started = start_on_slave(&probe_command(&report), &containment);
    let reported = read_report(&report);

    let own = cgroup_of(std::process::id());
    assert_ne!(
        reported.get("cgroup").replace('|', "\n"),
        own,
        "the probe started outside the group"
    );
    assert_eq!(
        reported.get("cgroup").replace('|', "\n"),
        cgroup_of(started.pid)
    );
    let members = containment.members().expect("the group is read");
    assert!(members.contains(&ProcessId(started.pid)));
    assert_eq!(reported.get("fds"), "", "the entry left no descriptor open");
    assert_eq!(reported.get("sid"), started.pid.to_string());

    assert!(containment.end(&[]), "the group is gone");
    started.child.wait().expect("the child is reaped");
    started.guard.0.clear();
}

/// What a process on a terminal forks and detaches at once, by a double fork into a session of
/// its own, is in the group, with nothing moving it in after the start.
#[cfg(target_os = "linux")]
#[test]
fn what_a_process_on_a_terminal_detaches_at_once_is_in_its_launch_s_group() {
    let Some((_, containment)) = grouped_launch("term-spawn", "pty-orphan") else {
        return;
    };
    let dir = ScratchDir::new("pty-group-orphan");
    let pid_file = dir.join("leader");
    let mut command = Command::new(test_helper::program());
    command
        .args(test_helper::args(
            "platform::proc::contract_tests::a_helper_process",
        ))
        .env(ROLE, "lead_orphan")
        .env(PID_FILE, &pid_file);
    let mut started = start_on_slave(&command, &containment);
    let orphan = wait_for_pid(&pid_file);
    started.guard.0.push(orphan);

    assert_detached_session_leader(orphan, started.pid);
    let group = cgroup_of(started.pid);
    assert_ne!(
        group,
        cgroup_of(std::process::id()),
        "the leader is in the group"
    );
    assert_eq!(cgroup_of(orphan as u32), group);
    let members = containment.members().expect("the group is read");
    assert!(members.contains(&ProcessId(started.pid)));
    assert!(members.contains(&ProcessId(orphan as u32)));

    assert!(containment.end(&[]), "the group is gone");
    assert_gone_by(orphan, Instant::now() + PATIENCE);
    started.child.wait().expect("the child is reaped");
    started.guard.0.clear();
}

/// `spawn` with a launch's group puts the process in it, and ending the group ends the
/// process.
#[cfg(target_os = "linux")]
#[test]
fn a_process_started_with_a_group_is_ended_with_it() {
    let Some((_, containment)) = grouped_launch("term-spawn", "pty-end") else {
        return;
    };
    let dir = ScratchDir::new("pty-group-end");
    let mut probe = plain_probe(&dir, Some(&containment));
    let members = containment.members().expect("the group is read");
    assert!(members.contains(&probe.spawned.pid), "{members:?}");

    assert!(containment.end(&[]), "the group is gone");
    reap(&probe.spawned);
    probe.spawned.pty.stop_reader();
    probe.guard.0.clear();
    assert_eq!(containment.members(), None);
}

/// A group whose directory is gone is no reason for the start to fail: the process starts,
/// outside any group.
#[cfg(target_os = "linux")]
#[test]
fn a_start_whose_group_is_gone_still_starts_the_process() {
    let Some((_, containment)) = grouped_launch("term-spawn", "pty-gone") else {
        return;
    };
    assert!(containment.end(&[]), "the group is gone");

    let dir = ScratchDir::new("pty-group-gone");
    let probe = plain_probe(&dir, Some(&containment));
    assert_eq!(
        probe.report.get("cgroup").replace('|', "\n"),
        cgroup_of(std::process::id()),
        "the process is not in a group"
    );
    probe.end();
}
