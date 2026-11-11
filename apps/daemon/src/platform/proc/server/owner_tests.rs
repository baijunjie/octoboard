//! A real server owner and its children, run from this test binary so forced owner death cannot
//! accidentally exercise Rust's drops or the holder's crash handler.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::Duration;

use super::{Readiness, ServerChild};
use crate::platform::proc::{signal_identified, Containment, ProcessId, ProcessIdentity, Signal};
use crate::platform::proc_table;
use crate::platform::term::Command;
use crate::test_support::{wait_for_pid, ScratchDir, PATIENCE};

const FIXTURE: &str = "platform::proc::server::owner_tests::owner_process_fixture";
const ROLE: &str = "OCTOBOARDD_TEST_SERVER_OWNER_ROLE";
const DIRECTORY: &str = "OCTOBOARDD_TEST_SERVER_OWNER_DIRECTORY";
const READY: &str = "server-ready:";

fn command(role: &str, directory: &Path) -> Command {
    let mut command = Command::new(std::env::current_exe().expect("the test executable"));
    command
        .args(["--exact", FIXTURE, "--nocapture"])
        .env(ROLE, role)
        .env(DIRECTORY, directory);
    command
}

fn spawn_fixture(role: &str, directory: &Path) -> Child {
    command(role, directory)
        .plain()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the fixture starts")
}

fn own_identity() -> ProcessIdentity {
    let pid = ProcessId(std::process::id());
    ProcessIdentity {
        pid,
        start_micros: proc_table::start_of(pid.0 as i32).expect("the fixture is running"),
    }
}

fn record_identity(directory: &Path, name: &str) {
    let file = directory.join(name);
    let temporary = directory.join(format!("{name}.tmp"));
    std::fs::write(&temporary, own_identity().key()).expect("the fixture identity is written");
    std::fs::rename(temporary, file).expect("the whole identity is published");
}

fn read_identity(directory: &Path, name: &str) -> Option<ProcessIdentity> {
    ProcessIdentity::parse(&std::fs::read_to_string(directory.join(name)).ok()?)
}

/// Fixtures are real processes whose start and exit cannot be driven by a fake clock. Wait on
/// their atomic identity files using the daemon tests' patience window.
fn wait_identity(directory: &Path, name: &str) -> ProcessIdentity {
    assert!(
        crate::session::wait::wait_for(PATIENCE, || read_identity(directory, name).is_some()),
        "the fixture never recorded {name}"
    );
    read_identity(directory, name).expect("the published identity")
}

struct IdentityCleanup(Vec<ProcessIdentity>);

impl Drop for IdentityCleanup {
    fn drop(&mut self) {
        for identity in &self.0 {
            let _ = signal_identified(identity, Signal::Kill);
        }
    }
}

fn assert_ended(identity: ProcessIdentity) {
    assert!(
        crate::session::wait::wait_for(PATIENCE, || !identity.is_running()),
        "{} kept running after its owner ended",
        identity.key()
    );
}

/// Cleans up by identity even if setup or an assertion panics. The owner is kept as a child until
/// termination, and each server records its own identity before starting any descendant.
pub(crate) struct Owner {
    child: Child,
    directory: PathBuf,
}

impl Owner {
    fn start(role: &str, directory: &Path) -> Self {
        Self {
            child: spawn_fixture(role, directory),
            directory: directory.to_path_buf(),
        }
    }

    pub(crate) fn force_end(&mut self) {
        self.child.kill().expect("the owner is force-terminated");
        self.child.wait().expect("the owner is reaped");
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        for name in ["server", "descendant"] {
            if let Some(identity) = read_identity(&self.directory, name) {
                let _ = signal_identified(&identity, Signal::Kill);
            }
        }
    }
}

#[test]
fn owner_process_fixture() {
    let Some(role) = std::env::var_os(ROLE) else {
        return;
    };
    let directory = PathBuf::from(std::env::var_os(DIRECTORY).expect("the fixture directory"));
    match role.to_str().expect("a plain role") {
        "descendant" => record_identity(&directory, "descendant"),
        "server" | "unready-server" => {
            record_identity(&directory, "server");
            // Deliberately left running: the test checks that the owner's death ends it.
            #[allow(clippy::zombie_processes)]
            let _child = spawn_fixture("descendant", &directory);
            if role == "server" {
                println!("{READY}4321");
                std::io::stdout()
                    .flush()
                    .expect("readiness reaches the owner");
            }
        }
        "owner" | "unready-owner" | "exiting-owner" => {
            let containment =
                Containment::create("test-server-owner", &uuid::Uuid::new_v4().to_string())
                    .expect("the launch containment");
            let mut server_command = command(
                if role == "unready-owner" {
                    "unready-server"
                } else {
                    "server"
                },
                &directory,
            );
            containment.prepare(&mut server_command);
            let mut server =
                ServerChild::start(server_command, true, Some(&containment), "owner fixture")
                    .expect("the server starts");
            std::fs::write(directory.join("started"), std::process::id().to_string())
                .expect("the owner has started its server");
            server
                .wait_ready(Readiness::PortAfter(READY), PATIENCE)
                .expect("the server becomes ready");
            std::fs::write(directory.join("ready"), std::process::id().to_string())
                .expect("the owner has observed readiness");
            if role == "exiting-owner" {
                assert!(crate::session::wait::wait_for(PATIENCE, || directory
                    .join("exit")
                    .exists()));
                std::process::exit(0);
            }
            // The owning value must remain live until this fixture is force-terminated.
            loop {
                std::thread::park();
                std::hint::black_box(&server);
            }
        }
        #[cfg(windows)]
        "legacy-owner" => {
            use crate::platform::proc::{ChildHandle, Reach};
            let instance = std::env::var("OCTOBOARDD_TEST_SERVER_INSTANCE").unwrap();
            let launch = std::env::var("OCTOBOARDD_TEST_SERVER_LAUNCH").unwrap();
            let containment = Containment::join_launch(&instance, &launch).unwrap();
            let mut server_command = command("server", &directory);
            containment.prepare(&mut server_command);
            let mut plain = server_command.plain();
            plain
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            Reach::Session.lead(&mut plain);
            let child = plain.spawn().expect("the legacy server starts suspended");
            let handle =
                ChildHandle::started(ProcessId(child.id()), Reach::Session, Some(&containment))
                    .expect("the legacy server joins its launch and runs");
            // This fixture models an older holder with no forced-death lifetime guard.
            std::mem::forget((child, handle));
        }
        other => panic!("unknown fixture role {other}"),
    }
    loop {
        std::thread::park();
    }
}

fn force_owner_during(readiness: bool) {
    let directory = ScratchDir::new("server-owner-death");
    let mut owner = Owner::start(
        if readiness { "owner" } else { "unready-owner" },
        &directory,
    );
    wait_for_pid(&directory.join(if readiness { "ready" } else { "started" }));
    let server = wait_identity(&directory, "server");
    let descendant = wait_identity(&directory, "descendant");
    assert!(server.is_running());
    assert!(descendant.is_running());
    owner.force_end();
    assert_ended(server);
    assert_ended(descendant);
}

#[test]
fn a_ready_server_and_its_descendant_end_when_their_owner_is_force_terminated() {
    force_owner_during(true);
}

#[test]
fn a_server_and_its_descendant_end_when_the_owner_dies_before_readiness() {
    force_owner_during(false);
}

#[test]
fn a_server_ends_when_its_owner_exits_without_running_drops() {
    let directory = ScratchDir::new("server-owner-exit");
    let mut owner = Owner::start("exiting-owner", &directory);
    wait_for_pid(&directory.join("ready"));
    let server = wait_identity(&directory, "server");
    let descendant = wait_identity(&directory, "descendant");
    std::fs::write(directory.join("exit"), "exit").expect("the owner is told to exit");
    assert!(owner.child.wait().expect("the owner exits").success());
    assert_ended(server);
    assert_ended(descendant);
}

#[test]
fn ending_a_server_still_ends_its_descendant() {
    let directory = ScratchDir::new("server-owner-ordinary-end");
    let mut server = ServerChild::start(
        command("server", &directory),
        true,
        None,
        "ordinary end fixture",
    )
    .expect("the server starts");
    server
        .wait_ready(Readiness::PortAfter(READY), PATIENCE)
        .expect("the server is ready");
    let server_id = wait_identity(&directory, "server");
    let descendant = wait_identity(&directory, "descendant");
    let _cleanup = IdentityCleanup(vec![server_id, descendant]);
    server.end(Duration::ZERO);
    assert_ended(server_id);
    assert_ended(descendant);
    assert_group_gone(server_id.pid);
}

/// A server whose processes were killed from outside its owner, as the kill of a launch's cgroup
/// group kills them, still has its owner-death watcher, which no such kill reaches, ended once the
/// owner ends it: nothing of its process group is left.
#[cfg(unix)]
#[test]
fn ending_a_server_killed_from_outside_ends_its_owner_death_watcher() {
    let directory = ScratchDir::new("server-owner-outside-kill");
    let mut server = ServerChild::start(
        command("server", &directory),
        true,
        None,
        "outside kill fixture",
    )
    .expect("the server starts");
    server
        .wait_ready(Readiness::PortAfter(READY), PATIENCE)
        .expect("the server is ready");
    let server_id = wait_identity(&directory, "server");
    let descendant = wait_identity(&directory, "descendant");
    let _cleanup = IdentityCleanup(vec![server_id, descendant]);
    for identity in [&server_id, &descendant] {
        let _ = signal_identified(identity, Signal::Kill);
    }
    assert_ended(descendant);
    // The watcher is still there, so that what follows tests its end and not an absence.
    assert_eq!(
        crate::platform::proc::group_has_members(server_id.pid),
        Some(true),
        "the owner-death watcher did not outlive the outside kill"
    );
    server.end(Duration::ZERO);
    assert_group_gone(server_id.pid);
}

#[cfg(unix)]
fn assert_group_gone(leader: ProcessId) {
    assert!(
        crate::session::wait::wait_for(PATIENCE, || {
            crate::platform::proc::group_has_members(leader) == Some(false)
        }),
        "a process of the server's group, its owner-death watcher, outlived the server's end"
    );
}

#[cfg(windows)]
fn assert_group_gone(_leader: ProcessId) {}

/// Starts an older holder's companion in an existing launch job. Killing this owner deliberately
/// leaves its child alive, so startup must discover it from the held launch record alone.
#[cfg(windows)]
pub(crate) fn legacy_orphan(
    instance: &str,
    launch: &str,
    directory: &Path,
) -> (Owner, ProcessIdentity, ProcessIdentity) {
    let child = command("legacy-owner", directory)
        .env("OCTOBOARDD_TEST_SERVER_INSTANCE", instance)
        .env("OCTOBOARDD_TEST_SERVER_LAUNCH", launch)
        .plain()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the legacy owner starts");
    let mut owner = Owner {
        child,
        directory: directory.to_path_buf(),
    };
    let server = wait_identity(directory, "server");
    let descendant = wait_identity(directory, "descendant");
    owner.force_end();
    assert!(server.is_running(), "the legacy setup must leave an orphan");
    assert!(descendant.is_running());
    (owner, server, descendant)
}
