//! What every host's process layer must do, written once against the neutral signatures. The
//! processes it starts are this test binary run again as a helper (see [`a_helper_process`] and
//! `platform::test_helper`), so that no test needs a program of the host's own.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use super::*;
use crate::platform::proc_table::{self, EnvRead};
use crate::platform::test_helper::{self, write_pid, PID_FILE, ROLE};
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
        other => panic!("unknown role {other}"),
    };
    std::thread::sleep(PATIENCE * 2);
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

/// The containment of an agent launch with the group a Linux host lets the calling process make,
/// or `None`, said on the test's output, where it does not: a plain container's read-only
/// `/sys/fs/cgroup`, a cgroup owned by someone else.
#[cfg(target_os = "linux")]
fn grouped_launch(name: &str) -> Option<(String, Containment)> {
    let launch = format!("{name}-{}", std::process::id());
    let containment = Containment::for_launch("contract", &launch).expect("the containment");
    if containment.members().is_none() {
        eprintln!("skipped: no writable cgroup v2 subtree to make a launch's group in");
        return None;
    }
    Some((launch, containment))
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
    let child = start(role, pid_file, |command| {
        Reach::Session.lead(command);
        containment.prepare(command);
    });
    let handle = ChildHandle::started(ProcessId(child.id()), Reach::Session, Some(containment))
        .expect("the child is taken on");
    (child, handle)
}

/// On Linux a launch's group holds its process and what that started however it detached — a
/// double fork into a session of its own — and its end kills all of it and removes the group.
#[cfg(target_os = "linux")]
#[test]
fn a_launch_s_group_ends_what_its_process_started_however_it_detached() {
    let Some((_, containment)) = grouped_launch("group-end") else {
        return;
    };
    let dir = ScratchDir::new("proc-group-end");
    let pid_file = dir.join("leader");
    let (mut leader, handle) = start_in(&containment, "lead_orphan", &pid_file);
    let orphan = wait_for_pid(&pid_file);
    let mut guard = KillOnDrop(vec![leader.id() as i32, orphan]);
    let info = proc_table::list()
        .expect("the process table is listed")
        .into_iter()
        .find(|info| info.pid == orphan)
        .expect("the orphan runs");
    assert_ne!(info.ppid, leader.id() as i32, "the orphan was reparented");
    assert_eq!(
        info.sid,
        Some(ContainerId(orphan as u32)),
        "it leads its own session"
    );

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
    let Some((_, containment)) = grouped_launch("group-spare") else {
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
    let Some((launch, containment)) = grouped_launch("group-leftover") else {
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
    let Some((launch, containment)) = grouped_launch("group-join") else {
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
    // Longer than a poll, so that a watch that fires on a running process is caught.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(gone_rx.try_recv().is_err(), "the process was still running");

    parent.kill().expect("the helper is ended");
    parent.wait().expect("the helper is reaped");
    tokio::time::timeout(PATIENCE, gone_rx)
        .await
        .expect("the watch noticed within the bound")
        .expect("the callback ran");
}
