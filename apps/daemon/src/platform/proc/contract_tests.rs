//! What every host's process layer must do, written once against the neutral signatures. The
//! processes it starts are this test binary run again as a helper (see [`a_helper_process`] and
//! `platform::test_helper`), so that no test needs a program of the host's own.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use super::*;
use crate::platform::proc_table::{self, EnvRead};
use crate::platform::test_helper::{self, write_pid, PID_FILE, ROLE};
#[cfg(target_os = "linux")]
use crate::test_support::assert_detached_session_leader;
#[cfg(target_os = "linux")]
use crate::test_support::cgroups::{cgroup_of, grouped_launch, skip};
use crate::test_support::{
    assert_gone_by, is_running, wait_for_pid, KillOnDrop, ScratchDir, PATIENCE,
};

/// A helper process, started through `lead` (what the test says it leads); it is ready once it
/// has written `pid_file`.
fn start(role: &str, pid_file: &Path, lead: impl FnOnce(&mut Command)) -> Child {
    let mut command = Command::new(test_helper::program());
    command
        .args(test_helper::args(
            "platform::proc::contract_tests::a_helper_process",
        ))
        .env(ROLE, role)
        .env(PID_FILE, pid_file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    lead(&mut command);
    command.spawn().expect("the test binary starts")
}

/// Waits for `child` to exit, without a way to tell it to, for up to [`PATIENCE`].
fn wait_for_exit(child: &mut Child) -> std::process::ExitStatus {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(status) = child.try_wait().expect("the child is waited on") {
            return status;
        }
        assert!(Instant::now() < deadline, "the child kept running");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A handle on `child`, started leading what `reach` names, which the test holds unreaped.
fn handle_of(child: &Child, reach: Reach) -> ChildHandle {
    ChildHandle::started(ProcessId(child.id()), reach, None).expect("the child is taken on")
}

/// Waits until `child` has exited, found out without reaping it.
fn wait_for_unreaped_exit(child: &ChildHandle) {
    let deadline = Instant::now() + PATIENCE;
    while child.exited_unreaped() != Some(true) {
        assert!(Instant::now() < deadline, "the child kept running");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The process a test starts and signals: run as `ROLE` says, and a no-op in an ordinary test run.
///
/// - `park`: waits to be ended.
/// - `park_deaf`: waits to be ended, and does not take a polite request to terminate.
/// - `lead_same_group` and `lead_own_group`: starts a `park` helper, in its own process group for
///   the second and not for the first, writes its pid, and waits to be ended.
/// - `lead_detached`: starts a `park` helper apart from itself ([`detach`]), writes its pid, and
///   waits to be ended.
/// - `launch_and_exit`: starts a `park` helper into the containment of the launch its pid file is
///   named after, writes the helper's pid, and exits at once, as a daemon that goes away does.
/// - `orphan_detached`: starts a `park` helper apart from itself ([`detach`]), writes the helper's
///   pid, and exits at once, leaving the helper to be reparented: a double fork into a session of
///   its own.
/// - `lead_orphan`: starts an `orphan_detached` helper, waits for it to exit, writes the pid of the
///   process it left, and waits to be ended.
/// - `start_background_beside_inheritable`: see [`background_beside_inheritable`]; it
///   exits once done.
#[test]
fn a_helper_process() {
    let (Ok(role), Some(pid_file)) = (std::env::var(ROLE), std::env::var_os(PID_FILE)) else {
        return;
    };
    let pid_file = Path::new(&pid_file);
    let _helper: Option<Child> = match role.as_str() {
        "park" => {
            write_pid(pid_file, std::process::id());
            None
        }
        "park_deaf" => {
            ignore_terminate();
            write_pid(pid_file, std::process::id());
            None
        }
        "lead_same_group" | "lead_own_group" => {
            let own_group = role == "lead_own_group";
            let helper = start("park", &pid_file.with_extension("helper"), |command| {
                if own_group {
                    Reach::Group.lead(command);
                }
            });
            let _handle = own_group.then(|| handle_of(&helper, Reach::Group));
            wait_for_pid(&pid_file.with_extension("helper"));
            write_pid(pid_file, helper.id());
            Some(helper)
        }
        "lead_detached" => {
            let helper = start("park", &pid_file.with_extension("helper"), detach);
            wait_for_pid(&pid_file.with_extension("helper"));
            write_pid(pid_file, helper.id());
            Some(helper)
        }
        "launch_and_exit" => {
            let launch = pid_file.file_stem().expect("a named pid file");
            let containment = Containment::create("contract", &launch.to_string_lossy())
                .expect("the containment");
            let helper_file = pid_file.with_extension("helper");
            #[allow(
                clippy::zombie_processes,
                reason = "the helper outlives this process, as a launch outlives its daemon"
            )]
            let helper = start("park", &helper_file, |command| {
                Reach::Group.lead(command);
                containment.prepare(command);
            });
            ChildHandle::started(ProcessId(helper.id()), Reach::Group, Some(&containment))
                .expect("the helper is taken on");
            wait_for_pid(&helper_file);
            write_pid(pid_file, helper.id());
            return;
        }
        "orphan_detached" => {
            let helper_file = pid_file.with_extension("helper");
            #[allow(
                clippy::zombie_processes,
                reason = "the helper outlives this process, which leaves it to be reparented"
            )]
            let helper = start("park", &helper_file, detach);
            wait_for_pid(&helper_file);
            write_pid(pid_file, helper.id());
            return;
        }
        "lead_orphan" => {
            let middle_file = pid_file.with_extension("middle");
            let mut middle = start("orphan_detached", &middle_file, |_| {});
            assert!(wait_for_exit(&mut middle).success());
            write_pid(pid_file, wait_for_pid(&middle_file) as u32);
            None
        }
        "start_background_beside_inheritable" => {
            background_beside_inheritable(pid_file);
            return;
        }
        other => panic!("unknown role {other}"),
    };
    std::thread::sleep(PATIENCE * 2);
}

/// A process started for the background keeps the stdio the starter piped, which carries the
/// daemon's handshake, and runs on its own until it is ended.
#[test]
fn a_process_started_in_the_background_keeps_its_pipes() {
    use std::io::{BufRead, BufReader};

    let dir = ScratchDir::new("proc-background");
    let pid_file = dir.join("background");
    let mut command = Command::new(test_helper::program());
    command
        .args(test_helper::args(
            "platform::proc::contract_tests::a_helper_process",
        ))
        .env(ROLE, "park")
        .env(PID_FILE, &pid_file);
    let (mut child, _in_starter_job) =
        spawn_in_background(&mut command, false).expect("the background start spawns");
    let mut guard = KillOnDrop(vec![child.id() as i32]);

    // The test binary prints a blank line before "running 1 test": read until that line, which the
    // pipe carries to this reader, or the end of the pipe, within a bound.
    let mut reader = BufReader::new(child.stdout.take().expect("stdout was piped"));
    let mut seen = String::new();
    for _ in 0..8 {
        let mut line = String::new();
        if reader.read_line(&mut line).expect("a line on the pipe") == 0 {
            break;
        }
        seen.push_str(&line);
        if line.contains("running") {
            break;
        }
    }
    assert!(seen.contains("running 1 test"), "{seen:?}");
    assert_eq!(wait_for_pid(&pid_file), child.id() as i32);

    child.kill().expect("the child is ended");
    child.wait().expect("the child is reaped");
    guard.0.clear();
    // Held to here, so the pipe stays open while the child runs.
    drop(reader);
}

/// A program that cannot be started is reported by the background start, which marks the
/// starter's descriptors close-on-exec between `fork` and `exec` without touching the pipe `std`
/// reports a failed `exec` through.
#[cfg(unix)]
#[test]
fn a_program_that_cannot_be_started_in_the_background_is_an_error() {
    let mut command = Command::new("/nonexistent/octoboard-background-test");
    let err = spawn_in_background(&mut command, false).err();
    assert_eq!(
        err.map(|err| err.kind()),
        Some(std::io::ErrorKind::NotFound)
    );
}

/// A process started for the background inherits nothing of the starter's but its pipes: an
/// inheritable handle or descriptor the starter holds, as the standard handles a shell gives it
/// are (and a descriptor such as `3>&1`), is not kept open by the process (see
/// [`background_beside_inheritable`]).
#[test]
fn a_process_started_in_the_background_inherits_nothing_else() {
    let dir = ScratchDir::new("proc-background-handles");
    let pid_file = dir.join("starter");
    let mut starter = start("start_background_beside_inheritable", &pid_file, |_| {});
    let mut guard = KillOnDrop(vec![starter.id() as i32]);
    wait_for_exit(&mut starter);
    guard.0.clear();
    let outcome = std::fs::read_to_string(pid_file.with_extension("outcome"))
        .unwrap_or_else(|err| format!("the starter wrote no outcome: {err}"));
    assert_eq!(outcome, "ok");
}

/// What the `start_background_beside_inheritable` helper does: holds the write end of a pipe open
/// for inheritance (a handle on Windows, a descriptor above the standard three on Unix), starts a
/// `park` helper the plain way and another for the background ([`spawn_in_background`]), lets go
/// of the write end, and writes to the `outcome` sibling of `pid_file` `ok` when a read on the
/// pipe ends only once the plain helper has been ended (which shows the handle was inheritable),
/// while the other runs on; what went otherwise if not. Then it ends both. It runs in a process
/// of its own, where no other test starts a process meanwhile that would inherit the write end
/// too and hold the read up.
fn background_beside_inheritable(pid_file: &Path) {
    use std::io::Read;
    use std::sync::mpsc;

    let (mut reader, writer) = std::io::pipe().expect("a pipe");
    make_inheritable(&writer);

    let plain_file = pid_file.with_extension("plain");
    let mut plain = start("park", &plain_file, |_| {});
    let background_file = pid_file.with_extension("background");
    let mut command = Command::new(test_helper::program());
    command
        .args(test_helper::args(
            "platform::proc::contract_tests::a_helper_process",
        ))
        .env(ROLE, "park")
        .env(PID_FILE, &background_file);
    let (mut background, _in_starter_job) =
        spawn_in_background(&mut command, false).expect("the background start spawns");
    wait_for_pid(&plain_file);
    wait_for_pid(&background_file);
    drop(writer);

    // The read ends only once no process holds the write end; it goes on in a thread of its own
    // until then.
    let (ended_tx, ended) = mpsc::channel();
    std::thread::spawn(move || {
        let mut byte = [0u8; 1];
        let _ = ended_tx.send(reader.read(&mut byte).map(|read| read == 0));
    });
    let outcome = if ended.recv_timeout(Duration::from_millis(500)).is_ok() {
        "the read ended while the plain helper ran: the write end was not inheritable"
    } else {
        plain.kill().expect("the plain helper is ended");
        plain.wait().expect("the plain helper is reaped");
        if !matches!(ended.recv_timeout(PATIENCE / 2), Ok(Ok(true))) {
            "the read did not end once the plain helper was ended: the background helper holds \
             the write end"
        } else if !is_running(background.id() as i32) {
            "the background helper ended"
        } else {
            "ok"
        }
    };
    plain.kill().ok();
    plain.wait().ok();
    background.kill().ok();
    background.wait().ok();
    std::fs::write(pid_file.with_extension("outcome"), outcome).expect("the outcome is written");
}

/// Makes `writer` inherited by every process started the plain way.
#[cfg(windows)]
fn make_inheritable(writer: &std::io::PipeWriter) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT};

    // SAFETY: the handle is open.
    let marked = unsafe {
        SetHandleInformation(
            writer.as_raw_handle(),
            HANDLE_FLAG_INHERIT,
            HANDLE_FLAG_INHERIT,
        )
    };
    assert_ne!(marked, 0, "the write end is made inheritable");
}

/// Makes `writer` inherited by every process started the plain way: `std` makes it close-on-exec.
#[cfg(unix)]
fn make_inheritable(writer: &std::io::PipeWriter) {
    use std::os::fd::AsRawFd;

    // SAFETY: the descriptor is open; clearing its flags only changes it for this process.
    let cleared = unsafe { libc::fcntl(writer.as_raw_fd(), libc::F_SETFD, 0) };
    assert_ne!(cleared, -1, "the write end is made inheritable");
}

/// What a helper that does not take a polite request to terminate does to ask for it.
#[cfg(unix)]
fn ignore_terminate() {
    // SAFETY: ignoring a signal in a process of the test's own.
    unsafe { libc::signal(libc::SIGTERM, libc::SIG_IGN) };
}

#[cfg(not(unix))]
fn ignore_terminate() {}

/// Whether a polite request reached `child`: where the host has none for a process without a
/// terminal (Windows), it says so and leaves the process running for the kill.
fn asked_politely(child: &ChildHandle) -> bool {
    match child.stop(Polite::Terminate) {
        Sent::Delivered => true,
        Sent::Failed(err) if err.kind() == std::io::ErrorKind::Unsupported => false,
        other => panic!("the polite request failed: {other:?}"),
    }
}

/// A polite request ends a process that takes it; one that does not take it is left running by
/// the request and ends with the kill.
#[test]
fn a_polite_stop_is_followed_by_a_kill() {
    let dir = ScratchDir::new("proc-polite");

    let ready = dir.join("polite");
    let mut polite = start("park", &ready, |command| Reach::Group.lead(command));
    let polite_handle = handle_of(&polite, Reach::Group);
    let mut guard = KillOnDrop(vec![polite.id() as i32]);
    wait_for_pid(&ready);
    if !asked_politely(&polite_handle) {
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(polite_handle.exited_unreaped(), Some(false));
        assert!(polite_handle.kill().delivered());
    }
    wait_for_unreaped_exit(&polite_handle);
    assert!(!wait_for_exit(&mut polite).success());
    guard.0.clear();

    let ready = dir.join("deaf");
    let mut deaf = start("park_deaf", &ready, |command| Reach::Group.lead(command));
    let deaf_handle = handle_of(&deaf, Reach::Group);
    let mut guard = KillOnDrop(vec![deaf.id() as i32]);
    wait_for_pid(&ready);
    asked_politely(&deaf_handle);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(deaf_handle.exited_unreaped(), Some(false));
    assert!(deaf_handle.kill().delivered());
    wait_for_unreaped_exit(&deaf_handle);
    assert!(!wait_for_exit(&mut deaf).success());
    guard.0.clear();
}

/// The kill of a group reaches what its leader started without moving it elsewhere.
#[test]
fn a_group_kill_reaches_what_the_leader_started() {
    let dir = ScratchDir::new("proc-group");
    let pid_file = dir.join("leader");
    let mut leader = start("lead_same_group", &pid_file, |command| {
        Reach::Group.lead(command)
    });
    let leader_handle = handle_of(&leader, Reach::Group);
    let helper = wait_for_pid(&pid_file);
    let mut guard = KillOnDrop(vec![leader.id() as i32, helper]);

    assert_eq!(leader_handle.group_has_members(), Some(true));
    assert!(leader_handle.kill().delivered());
    assert_gone_by(helper, Instant::now() + PATIENCE);
    assert!(!wait_for_exit(&mut leader).success());
    guard.0.clear();
}

/// A helper moved to a group of its own is out of the reach of a group kill and in that of the
/// sweep of its leader's session, which is made while the leader is unreaped.
#[test]
fn a_session_sweep_reaches_what_a_group_kill_misses() {
    let dir = ScratchDir::new("proc-session");
    let pid_file = dir.join("leader");
    let mut leader = start("lead_own_group", &pid_file, |command| {
        Reach::Session.lead(command)
    });
    let leader_handle = handle_of(&leader, Reach::Session);
    let helper = wait_for_pid(&pid_file);
    let mut guard = KillOnDrop(vec![leader.id() as i32, helper]);

    assert!(leader_handle.kill().delivered());
    wait_for_unreaped_exit(&leader_handle);
    assert!(
        is_running(helper),
        "the group kill reached a helper in a group of its own"
    );

    let swept = leader_handle.kill_leftovers(&[]);
    assert!(swept.complete);
    assert!(swept.killed >= 1);
    assert_gone_by(helper, Instant::now() + PATIENCE);
    assert!(!wait_for_exit(&mut leader).success());
    guard.0.clear();
}

/// A helper moved to a group of its own is out of the reach of a group kill, and one the sweep
/// is told to keep survives it and the stop of its leader; swept once it is not kept.
#[test]
fn a_kept_identity_survives_a_stop_and_a_sweep() {
    let dir = ScratchDir::new("proc-keep");
    let pid_file = dir.join("leader");
    let mut leader = start("lead_own_group", &pid_file, |command| {
        Reach::Session.lead(command)
    });
    let leader_handle = handle_of(&leader, Reach::Session);
    let helper = wait_for_pid(&pid_file);
    let mut guard = KillOnDrop(vec![leader.id() as i32, helper]);
    let kept = ProcessIdentity {
        pid: ProcessId(helper as u32),
        start_micros: proc_table::start_of(helper).expect("the helper runs"),
    };

    assert!(leader_handle.kill().delivered());
    wait_for_unreaped_exit(&leader_handle);
    let swept = leader_handle.kill_leftovers(&[kept]);
    assert!(swept.complete);
    assert_eq!(swept.killed, 0);
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        is_running(helper),
        "a kept process was killed with its leader"
    );

    // The same pid with another start is another process, which is not kept.
    let reused = ProcessIdentity {
        start_micros: kept.start_micros + 1,
        ..kept
    };
    assert!(leader_handle.kill_leftovers(&[reused]).killed >= 1);
    assert_gone_by(helper, Instant::now() + PATIENCE);
    assert!(!wait_for_exit(&mut leader).success());
    guard.0.clear();
}

/// A helper started apart from its leader has left the leader's session, so the sweep of that
/// session leaves it running: what a launch keeps or carries over outlives its agent.
#[test]
fn a_helper_that_left_the_session_outlives_its_sweep() {
    let dir = ScratchDir::new("proc-left");
    let pid_file = dir.join("leader");
    let mut leader = start("lead_detached", &pid_file, |command| {
        Reach::Session.lead(command)
    });
    let leader_handle = handle_of(&leader, Reach::Session);
    let helper = wait_for_pid(&pid_file);
    let mut guard = KillOnDrop(vec![leader.id() as i32, helper]);

    assert!(leader_handle.kill().delivered());
    wait_for_unreaped_exit(&leader_handle);
    let swept = leader_handle.kill_leftovers(&[]);
    assert!(swept.complete);
    assert_eq!(swept.killed, 0);
    std::thread::sleep(Duration::from_millis(300));
    assert!(is_running(helper), "the sweep reached a helper that left");
    assert!(!wait_for_exit(&mut leader).success());
    // The leader is reaped and its pid free for another process; the helper is ended here.
    guard.0 = vec![helper];
}

/// A helper its leader started on its own console shares the leader's console, which ends it with
/// the leader where the console is a pseudoconsole; one started apart does not. Nothing does on
/// Unix, where a terminal outlives its session's leader.
#[test]
fn a_helper_on_its_leader_s_console_is_told_apart_from_one_started_apart() {
    for (role, shared) in [("lead_same_group", cfg!(windows)), ("lead_detached", false)] {
        let dir = ScratchDir::new("proc-console");
        let pid_file = dir.join("leader");
        let mut leader = start(role, &pid_file, |command| Reach::Session.lead(command));
        let leader_handle = handle_of(&leader, Reach::Session);
        let helper = wait_for_pid(&pid_file);
        let mut guard = KillOnDrop(vec![leader.id() as i32, helper]);
        let identity = |pid: i32| ProcessIdentity {
            pid: ProcessId(pid as u32),
            start_micros: proc_table::start_of(pid).expect("the process runs"),
        };

        assert_eq!(
            shares_console(&identity(leader.id() as i32), &identity(helper)),
            Some(shared),
            "{role}"
        );

        assert!(leader_handle.kill().delivered());
        assert!(!wait_for_exit(&mut leader).success());
        guard.0 = vec![helper];
    }
}

/// What a launch's containment counts as its members: the processes it started and what those
/// started, found as the host lists them, or, where it lists none, by the tag every one of them
/// carries.
fn members_of(containment: &Containment, tag: &str) -> Vec<ProcessId> {
    if let Some(members) = containment.members() {
        return members;
    }
    proc_table::list()
        .expect("the process table is listed")
        .into_iter()
        .filter(|info| match proc_table::environment(info.pid) {
            EnvRead::Readable(environment) => environment
                .split(|&byte| byte == 0)
                .any(|entry| entry == format!("{TAG_VARIABLE}={tag}").as_bytes()),
            EnvRead::Unreadable | EnvRead::Unsupported => false,
        })
        .map(|info| ProcessId(info.pid as u32))
        .collect()
}

/// A process the launch's process started is one of its members, as the process itself is, and
/// one that was started outside it is not.
#[test]
fn a_grandchild_counts_as_a_member_of_the_launch() {
    let dir = ScratchDir::new("proc-members");
    let launch = format!("members-{}", std::process::id());
    let containment = Containment::create("contract", &launch).expect("the containment");
    let tag = tag_value("contract", &launch);

    let pid_file = dir.join("leader");
    let mut leader = start("lead_same_group", &pid_file, |command| {
        Reach::Group.lead(command);
        containment.prepare(command);
    });
    let leader_handle =
        ChildHandle::started(ProcessId(leader.id()), Reach::Group, Some(&containment))
            .expect("the child is taken on");
    let grandchild = wait_for_pid(&pid_file);
    let mut guard = KillOnDrop(vec![leader.id() as i32, grandchild]);

    let outsider_file = dir.join("outsider");
    let mut outsider = start("park", &outsider_file, |_| {});
    let outsider_pid = wait_for_pid(&outsider_file);
    guard.0.push(outsider_pid);

    let members = members_of(&containment, &tag);
    assert!(members.contains(&ProcessId(leader.id())), "{members:?}");
    assert!(
        members.contains(&ProcessId(grandchild as u32)),
        "{members:?}"
    );
    assert!(
        !members.contains(&ProcessId(outsider_pid as u32)),
        "{members:?}"
    );

    assert!(leader_handle.kill().delivered());
    assert_gone_by(grandchild, Instant::now() + PATIENCE);
    assert!(!wait_for_exit(&mut leader).success());
    outsider.kill().expect("the outsider is killed");
    outsider.wait().expect("the outsider is reaped");
    guard.0.clear();
}

/// A launch whose processes outlive the daemon that started them is found again by its name by
/// the next start, with them in it, while one of them runs; once none does, a host that lists
/// members finds none.
#[test]
fn a_launch_is_found_again_after_the_daemon_that_started_it_exits() {
    let dir = ScratchDir::new("proc-reopen");
    let launch = format!("reopen-{}", std::process::id());
    let pid_file = dir.join(&launch);
    let mut daemon = start("launch_and_exit", &pid_file, |_| {});
    assert!(wait_for_exit(&mut daemon).success());
    let survivor = wait_for_pid(&pid_file);
    let mut guard = KillOnDrop(vec![survivor]);
    assert!(is_running(survivor));

    let reopened = Containment::reopen("contract", &launch).expect("the launch is found again");
    if let Some(members) = reopened.members() {
        assert!(members.contains(&ProcessId(survivor as u32)), "{members:?}");
    }
    drop(reopened);

    crate::test_support::kill(survivor);
    assert_gone_by(survivor, Instant::now() + PATIENCE);
    guard.0.clear();
    let deadline = Instant::now() + PATIENCE;
    while let Some(members) = Containment::reopen("contract", &launch).and_then(|c| c.members()) {
        if members.is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "{members:?} left in the launch");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The identity of the running process `pid`.
#[cfg(target_os = "linux")]
fn identity_of(pid: i32) -> ProcessIdentity {
    ProcessIdentity {
        pid: ProcessId(pid as u32),
        start_micros: proc_table::start_of(pid).expect("the process runs"),
    }
}

/// Starts `role` leading a session of its own into `containment`, as an agent is started.
#[cfg(target_os = "linux")]
fn start_in(containment: &Containment, role: &str, pid_file: &Path) -> (Child, ChildHandle) {
    let child = start_entering_only(containment, role, pid_file);
    let handle = ChildHandle::started(ProcessId(child.id()), Reach::Session, Some(containment))
        .expect("the child is taken on");
    (child, handle)
}

/// On Linux a launch's group holds its process and what that started however it detached — a
/// double fork into a session of its own — and its end kills all of it and removes the group.
#[cfg(target_os = "linux")]
#[test]
fn a_launch_s_group_ends_what_its_process_started_however_it_detached() {
    let Some((_, containment)) = grouped_launch("contract", "group-end") else {
        return;
    };
    let dir = ScratchDir::new("proc-group-end");
    let pid_file = dir.join("leader");
    let (mut leader, handle) = start_in(&containment, "lead_orphan", &pid_file);
    let orphan = wait_for_pid(&pid_file);
    let mut guard = KillOnDrop(vec![leader.id() as i32, orphan]);
    assert_detached_session_leader(orphan, leader.id());

    let members = containment.members().expect("the group is read");
    assert!(members.contains(&ProcessId(leader.id())), "{members:?}");
    assert!(members.contains(&ProcessId(orphan as u32)), "{members:?}");

    assert!(containment.end(&[]), "the group is gone");
    assert_gone_by(orphan, Instant::now() + PATIENCE);
    wait_for_unreaped_exit(&handle);
    assert!(!wait_for_exit(&mut leader).success());
    assert_eq!(
        containment.members(),
        None,
        "the group's directory is removed"
    );
    guard.0.clear();
}

/// What a launch's group spares outlives its end: a process moved out of it beforehand, and one
/// named to its end with the child it started, which is still in the group; the rest is killed.
#[cfg(target_os = "linux")]
#[test]
fn what_a_launch_s_group_spares_outlives_its_end() {
    let Some((_, containment)) = grouped_launch("contract", "group-spare") else {
        return;
    };
    let dir = ScratchDir::new("proc-group-spare");
    let (mut released, _released_handle) = start_in(&containment, "park", &dir.join("released"));
    let (mut spared, _spared_handle) =
        start_in(&containment, "lead_same_group", &dir.join("spared"));
    let (mut doomed, _doomed_handle) = start_in(&containment, "park", &dir.join("doomed"));
    wait_for_pid(&dir.join("released"));
    let spared_child = wait_for_pid(&dir.join("spared"));
    wait_for_pid(&dir.join("doomed"));
    let mut guard = KillOnDrop(vec![
        released.id() as i32,
        spared.id() as i32,
        spared_child,
        doomed.id() as i32,
    ]);

    assert!(containment.release(&[identity_of(released.id() as i32)], None));
    let members = containment.members().expect("the group is read");
    assert!(!members.contains(&ProcessId(released.id())), "{members:?}");
    assert!(
        members.contains(&ProcessId(spared_child as u32)),
        "{members:?}"
    );

    assert!(containment.end(&[(identity_of(spared.id() as i32), None)]));
    assert!(!wait_for_exit(&mut doomed).success());
    assert!(is_running(released.id() as i32));
    assert!(is_running(spared.id() as i32));
    assert!(
        is_running(spared_child),
        "the spared process's child went with it"
    );
    assert_eq!(
        containment.members(),
        None,
        "the group's directory is removed"
    );

    for child in [&mut released, &mut spared] {
        child.kill().expect("the child is killed");
        child.wait().expect("the child is reaped");
    }
    crate::test_support::kill(spared_child);
    assert_gone_by(spared_child, Instant::now() + PATIENCE);
    guard.0.clear();
}

/// A group an earlier run left is found again among its instance's leftovers while it is there,
/// and dropped with nothing in it, it is removed.
#[cfg(target_os = "linux")]
#[test]
fn a_leftover_group_is_found_again_and_removed_once_empty() {
    let Some((launch, containment)) = grouped_launch("contract", "group-leftover") else {
        return;
    };
    assert!(Containment::leftovers("contract").contains(&launch));
    let reopened = Containment::reopen("contract", &launch).expect("the launch is found again");
    assert_eq!(reopened.members(), Some(Vec::new()));
    drop(containment);
    drop(reopened);
    assert!(!Containment::leftovers("contract").contains(&launch));
    assert_eq!(
        Containment::reopen("contract", &launch).and_then(|c| c.members()),
        None
    );
}

/// A launch's group joined by a process that starts the launch's program on its maker's behalf, as
/// a holder does, takes in what that process starts, and is left to its maker when the joined
/// containment is dropped: its maker's end still finds it and ends it.
#[cfg(target_os = "linux")]
#[test]
fn a_joined_launch_s_group_takes_what_is_started_and_stays_its_maker_s() {
    let Some((launch, containment)) = grouped_launch("contract", "group-join") else {
        return;
    };
    let joined = Containment::join_launch("contract", &launch).expect("the containment");
    let dir = ScratchDir::new("proc-group-join");
    let (mut child, handle) = start_in(&joined, "park", &dir.join("child"));
    wait_for_pid(&dir.join("child"));
    let mut guard = KillOnDrop(vec![child.id() as i32]);
    drop(joined);

    let members = containment.members().expect("the group is still there");
    assert!(members.contains(&ProcessId(child.id())), "{members:?}");
    assert!(containment.end(&[]), "the group is gone");
    wait_for_unreaped_exit(&handle);
    assert!(!wait_for_exit(&mut child).success());
    assert_eq!(containment.members(), None);
    guard.0.clear();
}

/// The identity of this test process.
fn own_identity() -> ProcessIdentity {
    ProcessIdentity {
        pid: ProcessId(std::process::id()),
        start_micros: proc_table::start_of(std::process::id() as i32).expect("the process runs"),
    }
}

/// A held launch carries its tag as any launch does, whatever a host makes of the placement and
/// of noting a host, which change nothing of it.
#[test]
fn a_held_launch_carries_its_tag_whether_or_not_it_is_placed() {
    let launch = format!("held-tag-{}", std::process::id());
    let held = Containment::for_held_launch("contract", &launch).expect("the containment");
    let plain = Containment::create("contract", &launch).expect("the containment");
    let tagged = |containment: &Containment| {
        let mut environment = std::collections::HashMap::new();
        containment.prepare(&mut environment);
        environment
    };
    let expected = tagged(&plain);
    assert_eq!(
        expected.get(TAG_VARIABLE),
        Some(&tag_value("contract", &launch))
    );
    assert_eq!(tagged(&held), expected);

    Containment::note_host(&own_identity());
    held.place_with_host(&own_identity());
    assert_eq!(tagged(&held), expected);
}

/// Until it is placed a held launch lists no members, moves and ends nothing, and is told by its
/// tag alone. Where the host has no groups, placing it changes none of that.
#[cfg(unix)]
#[test]
fn an_unplaced_held_launch_has_no_group() {
    let launch = format!("held-unplaced-{}", std::process::id());
    let held = Containment::for_held_launch("contract", &launch).expect("the containment");
    let clone = held.clone();
    assert_eq!(held.members(), None);
    assert!(!held.releases_members());
    assert!(held.release(&[own_identity()], None), "nothing to release");
    assert!(held.end(&[]), "no group is left");
    held.adopt(&[own_identity()]);
    assert_eq!(held.members(), None);
    assert!(!clone.releases_members());

    #[cfg(not(target_os = "linux"))]
    {
        held.place_with_host(&own_identity());
        assert_eq!(held.members(), None);
        assert!(!clone.releases_members());
    }
}

/// On Linux a held launch placed with a host whose cgroup lets the calling process make a group
/// has a group like any launch's: what it starts is in it however it detached, and its end kills
/// all of it and removes the group.
#[cfg(target_os = "linux")]
#[test]
fn a_held_launch_placed_with_its_host_ends_what_it_started_like_any_launch() {
    let launch = format!("held-end-{}", std::process::id());
    let containment = Containment::for_held_launch("contract", &launch).expect("the containment");
    containment.place_with_host(&own_identity());
    if containment.members().is_none() {
        skip("no writable cgroup v2 subtree to make a launch's group in");
        return;
    }
    assert!(containment.releases_members());
    assert_eq!(containment.members(), Some(Vec::new()));

    let dir = ScratchDir::new("proc-held-end");
    let pid_file = dir.join("leader");
    let (mut leader, handle) = start_in(&containment, "lead_orphan", &pid_file);
    let orphan = wait_for_pid(&pid_file);
    let mut guard = KillOnDrop(vec![leader.id() as i32, orphan]);
    let members = containment.members().expect("the group is read");
    assert!(members.contains(&ProcessId(leader.id())), "{members:?}");
    assert!(members.contains(&ProcessId(orphan as u32)), "{members:?}");

    assert!(containment.end(&[]), "the group is gone");
    assert_gone_by(orphan, Instant::now() + PATIENCE);
    wait_for_unreaped_exit(&handle);
    assert!(!wait_for_exit(&mut leader).success());
    assert_eq!(containment.members(), None);
    guard.0.clear();
}

/// The wire form of a process identity names its pid and its start, and nothing else parses.
#[test]
fn a_process_identity_keeps_its_wire_form() {
    let identity = ProcessIdentity {
        pid: ProcessId(4242),
        start_micros: 1_700_000_000_123_456,
    };
    assert_eq!(identity.key(), "4242:1700000000123456");
    assert_eq!(ProcessIdentity::parse(&identity.key()), Some(identity));
    for malformed in ["", "42", "0:1", "x:1", "1:x", "-1:1"] {
        assert_eq!(ProcessIdentity::parse(malformed), None, "{malformed:?}");
    }
}

/// The watch on a process calls back once that process is gone, and not while it runs.
#[tokio::test]
async fn the_parent_watch_calls_back_once_the_process_is_gone() {
    let dir = ScratchDir::new("proc-parent-watch");
    let pid_file = dir.join("pid");
    let mut parent = start("park", &pid_file, |_| {});
    wait_for_pid(&pid_file);

    let (gone, mut gone_rx) = tokio::sync::oneshot::channel();
    watch_parent(parent.id(), move || {
        gone.send(()).ok();
    });
    // Several of the polls a watch on Unix makes, each run to its end on the paused clock before
    // the next, so that a watch that fires on a running process is caught. A watch that waits on
    // the process itself (Windows) has no poll for this to run.
    #[cfg(unix)]
    const WATCHED_FOR: Duration = super::unix::PARENT_POLL.saturating_mul(5);
    #[cfg(windows)]
    const WATCHED_FOR: Duration = Duration::from_secs(5);
    crate::test_support::runtime_advanced(WATCHED_FOR).await;
    assert!(gone_rx.try_recv().is_err(), "the process was still running");

    parent.kill().expect("the helper is ended");
    parent.wait().expect("the helper is reaped");
    tokio::time::timeout(PATIENCE, gone_rx)
        .await
        .expect("the watch noticed within the bound")
        .expect("the callback ran");
}

/// Starts `role` as [`start_in`] does, but without taking it on afterwards: nothing moves it into
/// the group once it was started, so what the group holds is only what entered it before the
/// program ran.
#[cfg(unix)]
fn start_entering_only(containment: &Containment, role: &str, pid_file: &Path) -> Child {
    start(role, pid_file, |command| {
        Reach::Session.lead(command);
        containment.prepare(command);
        containment.enter_before_exec(command);
    })
}

/// A process started with the hook registered is in its launch's group by the time the start
/// returns, with nothing having moved it in afterwards: [`start_entering_only`] does not take it
/// on, so the group holds it only if the hook put it there.
#[cfg(target_os = "linux")]
#[test]
fn a_process_is_in_its_launch_s_group_from_before_its_program_runs() {
    let Some((_, containment)) = grouped_launch("contract", "pre-exec-entry") else {
        return;
    };
    assert!(containment.releases_members());
    let dir = ScratchDir::new("proc-pre-exec");
    let pid_file = dir.join("child");
    let mut child = start_entering_only(&containment, "park", &pid_file);
    let mut guard = KillOnDrop(vec![child.id() as i32]);

    let members = containment.members().expect("the group is read");
    assert!(members.contains(&ProcessId(child.id())), "{members:?}");
    assert_ne!(
        cgroup_of(child.id()),
        cgroup_of(std::process::id()),
        "the child is in the group, not in the test's own cgroup"
    );

    wait_for_pid(&pid_file);
    assert!(containment.end(&[]), "the group is gone");
    assert!(!wait_for_exit(&mut child).success());
    assert_eq!(containment.members(), None);
    guard.0.clear();
}

/// What a process forks and detaches at once, into a session of its own and away from its parent
/// by a double fork, is in the group too, with nothing to move it in afterwards: it inherited the
/// group from a process that was in it before its program ran.
#[cfg(target_os = "linux")]
#[test]
fn a_descendant_that_detaches_at_once_is_in_the_group_without_a_move_after_the_start() {
    let Some((_, containment)) = grouped_launch("contract", "pre-exec-orphan") else {
        return;
    };
    let dir = ScratchDir::new("proc-pre-exec-orphan");
    let pid_file = dir.join("leader");
    let mut leader = start_entering_only(&containment, "lead_orphan", &pid_file);
    let orphan = wait_for_pid(&pid_file);
    let mut guard = KillOnDrop(vec![leader.id() as i32, orphan]);
    assert_detached_session_leader(orphan, leader.id());

    let own = cgroup_of(std::process::id());
    let group = cgroup_of(leader.id());
    assert_ne!(group, own, "the leader is in the group");
    assert_eq!(cgroup_of(orphan as u32), group);
    let members = containment.members().expect("the group is read");
    assert!(members.contains(&ProcessId(leader.id())), "{members:?}");
    assert!(members.contains(&ProcessId(orphan as u32)), "{members:?}");

    assert!(containment.end(&[]), "the group is gone");
    assert_gone_by(orphan, Instant::now() + PATIENCE);
    assert!(!wait_for_exit(&mut leader).success());
    guard.0.clear();
}

/// A hook whose entry fails, as it does for a group whose directory is gone, does not fail the
/// start: the program runs, outside any group.
#[cfg(target_os = "linux")]
#[test]
fn a_start_whose_group_entry_fails_still_runs_the_program() {
    let Some((_, containment)) = grouped_launch("contract", "pre-exec-gone") else {
        return;
    };
    assert!(containment.end(&[]), "the group is gone");
    assert_eq!(containment.members(), None);
    assert!(
        containment.releases_members(),
        "the containment still names its group"
    );

    let dir = ScratchDir::new("proc-pre-exec-gone");
    let pid_file = dir.join("child");
    let mut child = start_entering_only(&containment, "park", &pid_file);
    let mut guard = KillOnDrop(vec![child.id() as i32]);
    assert_eq!(wait_for_pid(&pid_file), child.id() as i32);
    assert!(is_running(child.id() as i32));

    child.kill().expect("the child is ended");
    child.wait().expect("the child is reaped");
    guard.0.clear();
}

/// Without a group (the tag alone, a held launch not yet placed, or any launch on a host with no
/// groups) there is nothing to enter: the hook does nothing, and the program starts and runs
/// outside any group.
#[cfg(unix)]
#[test]
fn a_start_without_a_group_starts_the_program_without_a_group() {
    let launch = format!("pre-exec-none-{}", std::process::id());
    let tag_only = Containment::create("contract", &launch).expect("the containment");
    let unplaced = Containment::for_held_launch("contract", &launch).expect("the containment");
    for (name, containment) in [("tag-only", &tag_only), ("unplaced", &unplaced)] {
        assert!(!containment.releases_members(), "{name}");
        let dir = ScratchDir::new("proc-pre-exec-none");
        let pid_file = dir.join("child");
        let mut child = start_entering_only(containment, "park", &pid_file);
        let mut guard = KillOnDrop(vec![child.id() as i32]);
        assert_eq!(wait_for_pid(&pid_file), child.id() as i32, "{name}");
        assert_eq!(containment.members(), None, "{name}");

        child.kill().expect("the child is ended");
        child.wait().expect("the child is reaped");
        guard.0.clear();
    }

    #[cfg(not(target_os = "linux"))]
    assert!(!Containment::for_launch("contract", &launch)
        .expect("the containment")
        .releases_members());
}

/// The path the process runs from leads to the running image, and so does any other name of the same
/// file; another file with the same bytes, and a path that leads nowhere, do not.
#[test]
fn only_the_file_this_process_runs_names_the_running_image() {
    let image = running_image().expect("the running executable");
    assert!(names_running_image(&image));

    let dir = ScratchDir::new("proc-running-image");
    assert!(!names_running_image(&dir.join("missing")));
    let copy = dir.join("copy");
    std::fs::copy(&image, &copy).expect("a copy of the running executable");
    assert!(!names_running_image(&copy));

    #[cfg(unix)]
    {
        let link = dir.join("link");
        crate::test_support::symlink(&image, &link).expect("a link to the running executable");
        assert!(names_running_image(&link));
        let other_name = dir.join("other-name");
        if std::fs::hard_link(&image, &other_name).is_ok() {
            assert!(names_running_image(&other_name));
        }
    }
}

/// The open image reads as the file at the running executable's path, so reading it for a copy gives
/// the bytes this process runs.
#[test]
fn the_open_running_image_reads_as_the_file_at_the_running_path() {
    use std::io::Read;
    let image = running_image().expect("the running executable");
    let mut opened = running_image_file().expect("the open image");
    let mut head = [0u8; 4];
    opened
        .read_exact(&mut head)
        .expect("the image's first bytes");
    let mut at_path = [0u8; 4];
    std::fs::File::open(&image)
        .and_then(|mut file| file.read_exact(&mut at_path))
        .expect("the file's first bytes");
    assert_eq!(head, at_path);
    assert_eq!(
        crate::platform::fs::stat_of(&running_image_file().unwrap())
            .unwrap()
            .identity,
        crate::platform::fs::stat(&image, crate::platform::fs::Links::Follow)
            .unwrap()
            .identity
    );
}

/// The server form's update is offered on Linux for the two architectures there is a release for, and
/// nowhere else.
#[test]
fn only_linux_on_x86_64_and_arm64_is_a_host_that_updates_itself() {
    #[cfg(not(target_os = "linux"))]
    assert!(!self_update_host());
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    assert!(self_update_host());
    #[cfg(all(
        target_os = "linux",
        not(any(target_arch = "x86_64", target_arch = "aarch64"))
    ))]
    assert!(!self_update_host());
}
