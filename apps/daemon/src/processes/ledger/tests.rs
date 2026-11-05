//! The ledger over real processes: what a stand-in agent's shell commands leave running, the
//! environment tag, the stops and the reconcile at start. The attribution rules on their own are
//! tested over made-up tables in `attribute.rs`.

use std::collections::HashSet;
use std::path::Path;
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::attribute::{Attributed, Book, ENDED};
use super::looks::LOOK_DEADLINE_OVERRIDE;
#[cfg(target_os = "macos")]
use super::scan::UNREADABLE_OVERRIDE;
use super::stop::{self, follow_ups};
use super::table::{self, Identity, Proc, Table};
use super::trees::{TreesStopped, AGENT_CONSOLE_OVERRIDE};
#[cfg(unix)]
use super::SETTLED_OVERRIDE;
use super::{boot_id, persist, ProcessLedger, BOOT_ID_OVERRIDE, TAG_VARIABLE};
use crate::platform::proc::{ContainerId, ProcessId};
use crate::platform::proc::{Polite, Signal};
#[cfg(unix)]
use crate::protocol::AgentId;
#[cfg(unix)]
use crate::protocol::{Session, SessionStatus};
use crate::protocol::{UnkeepableReason, UnstoppedReason};
use crate::state::AppState;
#[cfg(windows)]
use crate::test_support::fixture::Steps;
use crate::test_support::{
    app_state, assert_stops_running, detached_sleep_in, is_running, stand_in_starting,
    wait_for_pid, Background, Children, KillOnDrop, ScratchDir, StandIn, PATIENCE,
};
#[cfg(unix)]
use crate::test_support::{
    console_with_project_and_accounts, project_session, stand_in_launch_running,
    tagged_live_session, Launched,
};
#[cfg(unix)]
use crate::test_support::{detached, detached_in, fake_live_session, perl_write};

/// Where [`sleeper`] reads how long to sleep.
const SLEEP_VAR: &str = "OCTOBOARDD_TEST_LEDGER_SLEEP";

/// Where [`sleeper`] reads the tag to run under once [`GO_VAR`]'s file exists: it runs itself
/// again with that tag, keeping its pid and start time.
#[cfg(unix)]
const RETAG_VAR: &str = "OCTOBOARDD_TEST_LEDGER_RETAG";
#[cfg(unix)]
const GO_VAR: &str = "OCTOBOARDD_TEST_LEDGER_GO";

/// When set, [`sleeper`] ignores `SIGTERM`, as a process that will not stop when asked does,
/// and then writes its pid to the file the variable names.
const IGNORE_TERM_VAR: &str = "OCTOBOARDD_TEST_LEDGER_IGNORE_TERM";

/// When set, [`sleeper`] writes its pid to `gen-<n>` in the directory the variable names, `n`
/// being [`RESPAWN_GEN_VAR`] (0 when unset), and on `SIGTERM`, while `n` is below
/// [`RESPAWN_LAST`], first starts itself again in a terminal session of its own as generation
/// `n + 1`, then exits: a process that each round of a stop finds anew, somewhere new. Its
/// process must be started with `SIGTERM` blocked (see [`respawning`]).
#[cfg(unix)]
const RESPAWN_VAR: &str = "OCTOBOARDD_TEST_LEDGER_RESPAWN";
#[cfg(unix)]
const RESPAWN_GEN_VAR: &str = "OCTOBOARDD_TEST_LEDGER_RESPAWN_GEN";
/// The last generation [`RESPAWN_VAR`] starts: one per round of a stop, `STOP_PASSES` in all.
#[cfg(unix)]
const RESPAWN_LAST: usize = super::STOP_PASSES - 1;

/// Keeps the scanner thread from starting, so that the table is read only when a test says so and
/// what each read finds is the test's to decide.
fn quiet(ledger: &ProcessLedger) {
    ledger.without_scanner();
}

fn identity(pid: i32) -> Identity {
    let pid = ProcessId(pid as u32);
    Identity {
        pid,
        start_micros: table::start_of(pid).expect("the process is running"),
    }
}

/// A ledger file's content written in this boot of the system, naming nothing.
fn this_boot() -> persist::Persisted {
    persist::Persisted {
        boot: boot_id(),
        ..persist::Persisted::default()
    }
}

fn launch_of(ledger: &ProcessLedger, pid: i32) -> Option<String> {
    ledger
        .book()
        .attributed
        .iter()
        .find(|(id, _)| id.pid.0 as i32 == pid)
        .map(|(_, attributed)| attributed.launch.clone())
}

/// A stand-in agent registered with `state` starting `commands` in the background and then staying
/// up until the test ends it.
fn agent_of(state: &Arc<AppState>, id: &str, commands: Vec<Background>) -> StandIn {
    let live = stand_in_starting(id, commands);
    state.register_live(live.clone());
    live
}

/// A command that starts a `sleep`, writes its pid to `child_file`, waits for `go` to exist and
/// exits, leaving the `sleep` running.
fn wrapper_leaving_a_sleeper(child_file: &Path, go: &Path) -> Background {
    #[cfg(unix)]
    return Background::perl(&format!(
        "my $c = fork(); if ($c == 0) {{ exec \"/bin/sleep\", \"300\" }} {} {} exit 0;",
        perl_write(child_file, "$c"),
        perl_wait_for(go)
    ));
    #[cfg(windows)]
    return Background::process(
        "wrapper",
        Steps::new()
            .spawn("sleep", Steps::new().sleep(300))
            .write_child_pid(child_file)
            .wait_for(go)
            .exit(),
    );
}

/// [`agent_of`] running a shell `script`.
#[cfg(unix)]
fn agent(state: &Arc<AppState>, id: &str, script: &str) -> StandIn {
    let live = fake_live_session(
        id,
        AgentId::CLAUDE,
        80,
        24,
        &format!("{script}\nexec sleep 300"),
    );
    state.register_live(live.clone());
    live
}

/// A Perl one-liner that starts a terminal session of its own (`setsid`) — as an agent CLI does
/// for each shell command — and runs `body` in it. `/usr/bin/perl` because it can call `setsid`
/// and fork on macOS and Linux alike, and is there on both.
#[cfg(unix)]
fn in_own_session(body: &str) -> String {
    format!("/usr/bin/perl -MPOSIX -e 'POSIX::setsid(); {body}'")
}

/// Perl that waits for `path` to exist.
#[cfg(unix)]
fn perl_wait_for(path: &Path) -> String {
    format!(
        "select(undef, undef, undef, 0.02) until -e \"{}\";",
        path.display()
    )
}

fn test_binary() -> String {
    std::env::current_exe()
        .expect("the test binary")
        .to_string_lossy()
        .into_owned()
}

/// The arguments that run this test binary as [`sleeper`].
#[cfg(unix)]
const SLEEPER: [&str; 4] = [
    "--exact",
    "processes::ledger::tests::sleeper",
    "--ignored",
    "--test-threads=1",
];

/// This test binary run as [`sleeper`], detached, with `tag` as its tag: a process whose
/// environment can be read on macOS, unlike a platform binary's with SIP on, and that descends
/// from no agent. Answers once its environment shows the tag.
#[cfg(unix)]
fn tagged_sleeper(tag: &str) -> i32 {
    let pid = detached(
        &test_binary(),
        &SLEEPER,
        &[(SLEEP_VAR, Some("60")), (TAG_VARIABLE, Some(tag))],
    );
    wait_for_tag(pid);
    pid
}

#[cfg(unix)]
fn wait_for_tag(pid: i32) {
    let deadline = Instant::now() + PATIENCE;
    while !matches!(table::env_tag(identity(pid)), table::EnvTag::Tagged { .. }) {
        assert!(Instant::now() < deadline, "{pid} never showed a tag");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Sleeps for as long as [`SLEEP_VAR`] says, first running itself again under [`RETAG_VAR`]'s
/// tag once [`GO_VAR`]'s file exists when both are set; does nothing when run any other way.
#[test]
#[ignore = "run as a tagged process by the ledger's tests"]
fn sleeper() {
    if let Some(ready) = std::env::var_os(IGNORE_TERM_VAR) {
        // SAFETY: ignoring a signal changes nothing but how this process takes it.
        #[cfg(unix)]
        unsafe {
            libc::signal(libc::SIGTERM, libc::SIG_IGN)
        };
        let partial = Path::new(&ready).with_extension("tmp");
        std::fs::write(&partial, std::process::id().to_string()).unwrap();
        std::fs::rename(&partial, &ready).unwrap();
    }
    #[cfg(unix)]
    if let (Ok(tag), Ok(go)) = (std::env::var(RETAG_VAR), std::env::var(GO_VAR)) {
        use std::os::unix::process::CommandExt;
        while !Path::new(&go).exists() {
            std::thread::sleep(Duration::from_millis(20));
        }
        let err = Command::new(std::env::current_exe().expect("the test binary"))
            .args(SLEEPER)
            .env(TAG_VARIABLE, tag)
            .env_remove(RETAG_VAR)
            .env_remove(GO_VAR)
            .exec();
        panic!("running itself again failed: {err}");
    }
    #[cfg(unix)]
    if let Some(dir) = std::env::var_os(RESPAWN_VAR) {
        respawning(Path::new(&dir));
    }
    if let Some(seconds) = std::env::var(SLEEP_VAR).ok().and_then(|s| s.parse().ok()) {
        std::thread::sleep(Duration::from_secs(seconds));
    }
}

/// [`sleeper`] under [`RESPAWN_VAR`]: never returns.
///
/// A round of a stop can find a generation, and signal it, the moment its process has started,
/// long before this runs. So the process is started with `SIGTERM` blocked, which `exec` keeps
/// and every thread inherits, and takes the signal with `sigwait`, never unblocking it. A handler
/// installed here and then unblocked would not do, since libtest runs this on a thread of its
/// own: macOS leaves a signal sent while every thread blocked it pending on one of them, and
/// another thread unblocking it never receives it, while `sigwait` takes it wherever it waits.
#[cfg(unix)]
fn respawning(dir: &Path) -> ! {
    use std::os::unix::process::CommandExt;
    let generation: usize = std::env::var(RESPAWN_GEN_VAR)
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    let file = dir.join(format!("gen-{generation}"));
    let partial = file.with_extension("tmp");
    std::fs::write(&partial, std::process::id().to_string()).unwrap();
    std::fs::rename(&partial, &file).unwrap();
    let mut signal = 0;
    // A minute without a stop ends the process, by `SIGALRM`'s default action.
    // SAFETY: `alarm` and `sigwait` act only on this process's own signals.
    let waited = unsafe {
        libc::alarm(60);
        libc::sigwait(&sigterm_set(), &mut signal)
    };
    assert_eq!(waited, 0, "waiting for SIGTERM failed");
    // macOS reports an interrupted wait as a success with `signal` untouched.
    assert_eq!(signal, libc::SIGTERM, "the wait ended without SIGTERM");
    if generation < RESPAWN_LAST {
        let mut next = Command::new(std::env::current_exe().expect("the test binary"));
        next.args(SLEEPER)
            .env(RESPAWN_GEN_VAR, (generation + 1).to_string())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        // Blocked here, after the mask `spawn` clears in the child, and kept across `exec`.
        let blocked = sigterm_set();
        // SAFETY: `setsid` and `pthread_sigmask` are async-signal-safe.
        unsafe {
            next.pre_exec(move || {
                libc::setsid();
                libc::pthread_sigmask(libc::SIG_BLOCK, &blocked, std::ptr::null_mut());
                Ok(())
            });
        }
        // Never waited for: this process exits at once, and the next is reparented.
        #[allow(clippy::zombie_processes)]
        next.spawn().expect("starting the next generation");
    }
    std::process::exit(0);
}

/// A signal set holding only `SIGTERM`.
#[cfg(unix)]
fn sigterm_set() -> libc::sigset_t {
    // SAFETY: `sigemptyset` initializes the set before `sigaddset` reads it.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTERM);
        set
    }
}

/// Waits for the child to be reaped, which a stopped child of this test process needs before it
/// is gone.
fn reaped(child: &mut Child) {
    let deadline = Instant::now() + PATIENCE;
    while child.try_wait().expect("try_wait").is_none() {
        assert!(Instant::now() < deadline, "{} kept running", child.id());
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A shell command's process that started a session of its own and outlived its shell is stopped
/// when the agent ends, and before the session reads as having no process: what waits for that,
/// the removal of an archived session's worktree, never finds it still running.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn what_a_launch_left_outside_its_session_is_stopped_before_the_session_reads_stopped() {
    let (state, dir) = app_state("ledger-end");
    let (child_file, go) = (dir.join("child"), dir.join("go"));
    let wrapper = in_own_session(&format!(
        "my $c = fork(); if ($c == 0) {{ exec \"/bin/sleep\", \"300\" }} {} {} exit 0;",
        perl_write(&child_file, "$c"),
        perl_wait_for(&go)
    ));
    let live = agent(&state, "s1", &format!("{wrapper} &"));
    crate::coordinator::exit_watch::watch_exit(&state, (*live).clone());
    let child = wait_for_pid(&child_file);
    let mut guard = KillOnDrop(vec![child]);
    // SAFETY: `getsid` only reads.
    assert_ne!(
        unsafe { libc::getsid(child) },
        live.pid as i32,
        "outside the agent's session"
    );
    state.process_ledger.scan(false);
    assert_eq!(
        launch_of(&state.process_ledger, child).as_deref(),
        Some(live.launch_id.as_str())
    );

    // Its shell exits, and it is reparented away from the agent.
    std::fs::write(&go, "").unwrap();
    let deadline = Instant::now() + PATIENCE;
    while table::Table::read()
        .and_then(|table| table.get(ProcessId(child as u32)).map(|proc| proc.ppid))
        .is_some_and(|ppid| ppid != ProcessId(1))
    {
        assert!(Instant::now() < deadline, "the shell never exited");
        std::thread::sleep(Duration::from_millis(20));
    }
    let child_id = identity(child);

    let ending = {
        let live = (*live).clone();
        tokio::task::spawn_blocking(move || live.end())
    };
    let deadline = Instant::now() + PATIENCE;
    while state.has_process("s1") {
        assert!(Instant::now() < deadline, "the session never stopped");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        !child_id.is_running(),
        "still running once the session read stopped"
    );
    guard.0.clear();
    ending.await.unwrap();
    assert!(!state
        .process_ledger
        .book()
        .launches
        .contains_key(&live.launch_id));
}

/// A platform binary first seen after the shell that started it exited — reparented, its
/// environment unreadable on macOS with SIP on — is the launch's through the session the shell
/// was seen in.
#[test]
// Unix only: a terminal session is a POSIX grouping; Windows follows a launch by its job.
#[cfg(unix)]
fn a_process_first_seen_after_its_shell_exited_is_adopted_through_its_session() {
    let (state, dir) = app_state("ledger-session");
    let (first, second, shell, go) = (
        dir.join("first"),
        dir.join("second"),
        dir.join("shell"),
        dir.join("go"),
    );
    let wrapper = in_own_session(&format!(
        "{} my $a = fork(); if ($a == 0) {{ exec \"/bin/sleep\", \"300\" }} {} {} \
         my $b = fork(); if ($b == 0) {{ exec \"/bin/sleep\", \"301\" }} {} exit 0;",
        perl_write(&shell, "$$"),
        perl_write(&first, "$a"),
        perl_wait_for(&go),
        perl_write(&second, "$b"),
    ));
    let live = agent(&state, "s1", &format!("{wrapper} &"));
    let first = wait_for_pid(&first);
    let shell = wait_for_pid(&shell);
    let mut guard = KillOnDrop(vec![first]);
    state.process_ledger.scan(false);
    assert_eq!(
        launch_of(&state.process_ledger, first).as_deref(),
        Some(live.launch_id.as_str())
    );

    std::fs::write(&go, "").unwrap();
    let second = wait_for_pid(&second);
    guard.0.push(second);
    // The shell reads as gone (`start_of`) once its exit begins, but the kernel reparents its
    // children later in that exit (seen on macOS): the reparenting is what is waited for.
    let deadline = Instant::now() + PATIENCE;
    while table::start_of(ProcessId(shell as u32)).is_some()
        || table::Table::read()
            .unwrap()
            .get(ProcessId(second as u32))
            .map(|proc| proc.ppid)
            != Some(ProcessId(1))
    {
        assert!(
            Instant::now() < deadline,
            "the shell never exited, or its child was never reparented"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        launch_of(&state.process_ledger, second),
        None,
        "not seen before"
    );

    state.process_ledger.scan(false);
    assert_eq!(
        launch_of(&state.process_ledger, second).as_deref(),
        Some(live.launch_id.as_str())
    );
}

/// A tag of this instance and a launch being followed reaches a process that no chain or session
/// leads to; one of an ended launch has its process stopped; one of another instance is ignored.
#[test]
// Unix only: a tag is read from another process's environment, which Windows does not read.
#[cfg(unix)]
fn a_tag_attributes_and_another_instances_tag_is_ignored() {
    let (state, _dir) = app_state("ledger-tag");
    let live = agent_of(&state, "s1", Vec::new());
    let instance = state.process_ledger.instance.clone();
    let ours = tagged_sleeper(&format!("{instance}:{}", live.launch_id));
    let ended = tagged_sleeper(&format!("{instance}:{}", uuid::Uuid::new_v4()));
    let theirs = tagged_sleeper(&format!("{}:{}", uuid::Uuid::new_v4(), live.launch_id));
    let _guard = KillOnDrop(vec![ours, ended, theirs]);

    state.process_ledger.scan(false);
    assert_eq!(
        launch_of(&state.process_ledger, ours).as_deref(),
        Some(live.launch_id.as_str())
    );
    assert_eq!(
        launch_of(&state.process_ledger, ended).as_deref(),
        Some(ENDED)
    );
    assert_eq!(launch_of(&state.process_ledger, theirs), None);

    state.process_ledger.tick();
    assert_stops_running(ended);
    assert!(is_running(ours), "a live launch's process was stopped");
    assert!(is_running(theirs), "another instance's process was stopped");
}

/// A signal goes only to the process that has the identity: a running pid with another start time
/// is a process the pid has passed to, and is left alone.
#[test]
fn a_stop_never_signals_a_pid_that_has_another_start_time() {
    let mut children = Children(vec![crate::test_support::programs::sleep(60)
        .command()
        .spawn()
        .unwrap()]);
    let pid = children.0[0].id() as i32;
    let real = identity(pid);
    stop::stop(
        &[Identity {
            start_micros: real.start_micros + 1,
            ..real
        }],
        &|_| false,
    );
    assert!(
        children.0[0].try_wait().unwrap().is_none(),
        "the pid's new process was stopped"
    );
    stop::stop(&[real], &|_| false);
    reaped(&mut children.0[0]);
}

/// A stop answers a process the system refused to kill for lack of permission, as it refuses one
/// running with more rights than the daemon, as left running for that reason.
#[test]
fn a_stop_answers_a_process_the_system_refused_to_kill_as_denied() {
    let mut children = Children(vec![crate::test_support::programs::sleep(60)
        .command()
        .spawn()
        .unwrap()]);
    let pid = children.0[0].id() as i32;
    let id = identity(pid);

    stop::DENIED.with_borrow_mut(|denied| denied.insert(pid));
    let left = stop::stop(&[id], &|_| false);
    stop::DENIED.with_borrow_mut(|denied| denied.remove(&pid));
    assert_eq!(left, vec![(id, UnstoppedReason::AccessDenied)]);
    assert!(id.is_running());

    assert!(stop::stop(&[id], &|_| false).is_empty());
    reaped(&mut children.0[0]);
}

/// A stop on request that the system refuses for a process of the tree, as it refuses one running
/// with more rights than the daemon, does not count the tree as stopped: it names the process,
/// with the tree, and leaves it to be tried again only after a while. Asked again once the system
/// lets it, the stop goes through.
#[test]
fn a_stop_on_request_the_system_refuses_names_the_process_left_running() {
    let (state, dir) = app_state("ledger-stop-denied");
    let (_live, child) = agent_leaving_one(&state, "s1", &dir.join("child"));
    let _guard = KillOnDrop(vec![child]);
    let ledger = &state.process_ledger;
    let id = identity(child);
    let key = id.key();

    stop::DENIED.with_borrow_mut(|denied| denied.insert(child));
    let result = ledger.stop_trees(std::slice::from_ref(&key), &[]);
    stop::DENIED.with_borrow_mut(|denied| denied.remove(&child));
    assert!(result.stopped.is_empty(), "{result:?}");
    let [process] = result.not_stopped.as_slice() else {
        panic!("not one process left running: {result:?}");
    };
    assert_eq!(
        (process.tree_id.as_str(), process.pid, process.reason),
        (key.as_str(), child as u32, UnstoppedReason::AccessDenied)
    );
    assert!(!process.name.is_empty(), "{process:?}");
    assert!(id.is_running());
    assert!(ledger.stubborn.lock().unwrap().contains_key(&id));

    assert_eq!(
        ledger.stop_trees(std::slice::from_ref(&key), &[]),
        TreesStopped {
            stopped: vec![key],
            not_stopped: Vec::new(),
        }
    );
    assert_stops_running(child);
}

/// A stop on request of a kept tree that the system refuses leaves the process the user's, kept
/// and listed under its project, rather than an ended launch's that no list shows; a stop asked
/// again once the system lets it goes through.
#[test]
fn a_kept_process_a_refused_stop_leaves_running_stays_kept() {
    let (state, dir) = app_state("ledger-kept-stop-denied");
    let (live, child) = agent_leaving_one(&state, "s1", &dir.join("child"));
    let _guard = KillOnDrop(vec![child]);
    let ledger = &state.process_ledger;
    let id = identity(child);
    let key = id.key();
    assert_eq!(
        ledger.keep_trees(std::slice::from_ref(&key), &[], &in_a_project()),
        Ok(vec![key.clone()])
    );
    ledger.end_launch(&live.launch_id);
    let listed = |ledger: &ProcessLedger| -> Vec<(String, bool)> {
        ledger
            .trees_of_project(&[], "p1", &[])
            .into_iter()
            .map(|tree| (tree.id, tree.kept))
            .collect()
    };

    stop::DENIED.with_borrow_mut(|denied| denied.insert(child));
    let result = ledger.stop_trees(std::slice::from_ref(&key), &[]);
    stop::DENIED.with_borrow_mut(|denied| denied.remove(&child));
    assert!(result.stopped.is_empty(), "{result:?}");
    assert_eq!(result.not_stopped.len(), 1, "{result:?}");
    assert!(id.is_running());
    {
        let book = ledger.book();
        assert!(book.kept.contains(&id));
        assert!(!book.attributed.contains_key(&id));
        assert_eq!(book.kept_projects.get(&id).map(String::as_str), Some("p1"));
    }
    assert_eq!(listed(ledger), vec![(key.clone(), true)]);

    assert_eq!(
        ledger.stop_trees(std::slice::from_ref(&key), &[]).stopped,
        vec![key]
    );
    assert_stops_running(child);
    assert!(listed(ledger).is_empty());
}

/// Whatever asks, the daemon itself is never signalled.
#[test]
fn a_stop_never_signals_the_daemon() {
    let own = identity(std::process::id() as i32);
    assert!(own.is_running());
    assert!(!stop::signal(&own, Signal::Polite(Polite::Terminate)));
}

/// A daemon starting stops what the previous run of its data directory left: a process its file
/// names that still has the same start time, and one carrying this instance's tag; and leaves a
/// process its file names under another start time, a kept one, and another instance's.
#[test]
fn reconcile_stops_what_the_previous_run_left_and_nothing_else() {
    let dir = ScratchDir::new("ledger-reconcile");
    let ledger = ProcessLedger::new(&dir);
    quiet(&ledger);
    let here = std::env::current_dir().unwrap();
    let sleep = || detached_sleep_in(&here, 60);
    let (left, reused, kept) = (sleep(), sleep(), sleep());
    // Unix only, a tag being read from another process's environment, which Windows does not read.
    #[cfg(unix)]
    let (tagged, theirs) = (
        tagged_sleeper(&format!("{}:{}", ledger.instance, uuid::Uuid::new_v4())),
        tagged_sleeper(&format!(
            "{}:{}",
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4()
        )),
    );
    #[cfg(unix)]
    let _guard = KillOnDrop(vec![left, reused, kept, tagged, theirs]);
    #[cfg(windows)]
    let _guard = KillOnDrop(vec![left, reused, kept]);
    let persisted = |id: Identity| persist::PersistedProcess {
        pid: id.pid.0 as i32,
        start: id.start_micros,
        launch: uuid::Uuid::new_v4().to_string(),
        sid: 0,
    };
    let reused_id = identity(reused);
    persist::save(
        &dir.join(persist::LEDGER_FILE),
        &persist::Persisted {
            attributed: vec![
                persisted(identity(left)),
                persisted(Identity {
                    start_micros: reused_id.start_micros + 1,
                    ..reused_id
                }),
                persisted(identity(kept)),
            ],
            kept: vec![identity(kept)],
            kept_sessions: Vec::new(),
            sessions: Vec::new(),
            ..this_boot()
        },
    )
    .unwrap();

    ledger.reconcile();
    assert_stops_running(left);
    #[cfg(unix)]
    {
        assert_stops_running(tagged);
        assert!(is_running(theirs), "{theirs} was stopped");
    }
    for pid in [reused, kept] {
        assert!(is_running(pid), "{pid} was stopped");
    }
}

/// Makes this test's system one whose boot id is `id`.
fn in_boot(id: Option<&str>) {
    BOOT_ID_OVERRIDE.set(Some(id.map(str::to_string)));
}

/// A ledger file written in another boot of the system names nothing that is running, though a
/// process now has the pid and the start time it names: on Linux, start times count from boot.
#[test]
fn reconcile_takes_nothing_from_a_file_written_in_another_boot() {
    in_boot(Some("this-boot"));
    let dir = ScratchDir::new("ledger-other-boot");
    let ledger = ProcessLedger::new(&dir);
    quiet(&ledger);
    let left = detached_sleep_in(&std::env::current_dir().unwrap(), 60);
    let _guard = KillOnDrop(vec![left]);
    let id = identity(left);
    persist::save(
        &dir.join(persist::LEDGER_FILE),
        &persist::Persisted {
            boot: Some("another-boot".to_string()),
            attributed: vec![persist::PersistedProcess {
                pid: id.pid.0 as i32,
                start: id.start_micros,
                launch: uuid::Uuid::new_v4().to_string(),
                sid: left,
            }],
            ..this_boot()
        },
    )
    .unwrap();

    ledger.reconcile();
    assert!(is_running(left), "stopped on a file of another boot");
    let persist::Loaded::Read(written) = persist::load(&dir.join(persist::LEDGER_FILE)) else {
        panic!("the ledger file was not written");
    };
    assert_eq!(written.boot.as_deref(), Some("this-boot"));
}

/// A ledger file whose boot cannot be compared with this run's — it records none on a system
/// that has boot ids, written before the file recorded one, or this run cannot read the boot id
/// it records — may count start times from another point: what it kept cannot be recognized, so
/// what only a tag reaches is kept, as for a file that cannot be read.
#[test]
// Unix only: a tag is read from another process's environment, which Windows does not read.
#[cfg(unix)]
fn a_file_whose_boot_cannot_be_compared_keeps_what_a_tag_reaches() {
    for (written_in, this_run) in [(None, Some("this-boot")), (Some("a-boot"), None)] {
        in_boot(this_run);
        let dir = ScratchDir::new("ledger-unknown-boot");
        let ledger = ProcessLedger::new(&dir);
        quiet(&ledger);
        let tagged = tagged_sleeper(&format!("{}:{}", ledger.instance, uuid::Uuid::new_v4()));
        let _guard = KillOnDrop(vec![tagged]);
        let id = identity(tagged);
        let file = dir.join(persist::LEDGER_FILE);
        persist::save(
            &file,
            &persist::Persisted {
                boot: written_in.map(str::to_string),
                kept: vec![Identity {
                    start_micros: 1,
                    ..id
                }],
                ..this_boot()
            },
        )
        .unwrap();

        ledger.reconcile();
        assert!(
            is_running(tagged),
            "{written_in:?}: stopped on its tag alone"
        );
        assert!(ledger.book().kept.contains(&id), "{written_in:?}");
        let persist::Loaded::Read(written) = persist::load(&file) else {
            panic!("{written_in:?}: the ledger file was not written");
        };
        assert_eq!(written.boot.as_deref(), this_run);
        assert!(written.kept.contains(&id), "{written_in:?}");
    }
}

/// A ledger file that cannot be read is moved aside, and what only a tag would reach is kept,
/// since the file may have said so: it is written down as kept, so neither this run nor the next
/// stops it.
#[test]
// Unix only: a tag is read from another process's environment, which Windows does not read.
#[cfg(unix)]
fn an_unreadable_ledger_file_is_moved_aside_and_what_a_tag_reaches_is_kept() {
    let dir = ScratchDir::new("ledger-unreadable");
    let file = dir.join(persist::LEDGER_FILE);
    std::fs::write(&file, "{ not json").unwrap();
    let ledger = ProcessLedger::new(&dir);
    quiet(&ledger);
    let tagged = tagged_sleeper(&format!("{}:{}", ledger.instance, uuid::Uuid::new_v4()));
    let _guard = KillOnDrop(vec![tagged]);
    let id = identity(tagged);

    ledger.reconcile();
    assert!(is_running(tagged), "stopped on its tag alone");
    assert_eq!(
        std::fs::read_to_string(file.with_extension("json.unreadable")).unwrap(),
        "{ not json"
    );
    assert!(ledger.book().kept.contains(&id));
    ledger.tick();
    assert!(is_running(tagged), "stopped by this run");
    let persist::Loaded::Read(written) = persist::load(&file) else {
        panic!("the ledger file was not written");
    };
    assert!(written.kept.contains(&id));

    let next = ProcessLedger::new(&dir);
    quiet(&next);
    next.reconcile();
    assert!(is_running(tagged), "stopped by the next run");
}

/// A process the previous run's launch tagged stays running when it is in the terminal session
/// of a process the user kept, the kept process having exited since and the file not naming the
/// tagged one: another kept process the file names still in the session shows it is the one kept.
#[test]
// Unix only: a terminal session is a POSIX grouping; Windows follows a launch by its job.
#[cfg(unix)]
fn reconcile_leaves_a_tagged_process_in_a_kept_session_whose_leader_exited() {
    let dir = ScratchDir::new("ledger-kept-session");
    let ledger = ProcessLedger::new(&dir);
    quiet(&ledger);
    let (named_file, later_file, go) = (dir.join("named"), dir.join("later"), dir.join("go"));
    let fork_sleeper = "my $c = fork(); if ($c == 0) { exec @ARGV }";
    let leader_script = format!(
        "{{ {fork_sleeper} {} }} {} {{ {fork_sleeper} {} }} exit 0;",
        perl_write(&named_file, "$c"),
        perl_wait_for(&go),
        perl_write(&later_file, "$c"),
    );
    let mut args = vec!["-e", leader_script.as_str(), "--"];
    let binary = test_binary();
    args.push(&binary);
    args.extend(SLEEPER);
    let tag = format!("{}:{}", ledger.instance, uuid::Uuid::new_v4());
    let leader = detached(
        "/usr/bin/perl",
        &args,
        &[(SLEEP_VAR, Some("60")), (TAG_VARIABLE, Some(&tag))],
    );
    let named = wait_for_pid(&named_file);
    let _guard = KillOnDrop(vec![leader, named]);
    let leader_id = identity(leader);
    persist::save(
        &dir.join(persist::LEDGER_FILE),
        &persist::Persisted {
            kept: vec![leader_id, identity(named)],
            kept_sessions: vec![persist::PersistedSession {
                sid: leader,
                leader_start: Some(leader_id.start_micros),
            }],
            ..this_boot()
        },
    )
    .unwrap();
    std::fs::write(&go, "").unwrap();
    let later = wait_for_pid(&later_file);
    let _later_guard = KillOnDrop(vec![later]);
    let deadline = Instant::now() + PATIENCE;
    while leader_id.is_running() {
        assert!(Instant::now() < deadline, "the leader never exited");
        std::thread::sleep(Duration::from_millis(20));
    }
    wait_for_tag(later);

    ledger.reconcile();
    ledger.tick();
    assert!(is_running(later), "a process in a kept session was stopped");
    assert!(ledger.book().kept.contains(&identity(later)));
}

/// A reconcile asked for once the scanner thread runs, started by a launch, still runs.
#[test]
fn a_reconcile_asked_for_after_the_scanner_started_runs() {
    let mut dir = ScratchDir::new("ledger-late-reconcile");
    let left = detached_sleep_in(&std::env::current_dir().unwrap(), 60);
    let _guard = KillOnDrop(vec![left]);
    let id = identity(left);
    persist::save(
        &dir.join(persist::LEDGER_FILE),
        &persist::Persisted {
            attributed: vec![persist::PersistedProcess {
                pid: id.pid.0 as i32,
                start: id.start_micros,
                launch: uuid::Uuid::new_v4().to_string(),
                sid: 0,
            }],
            ..this_boot()
        },
    )
    .unwrap();
    let ledger = ProcessLedger::new(&dir);
    // The scanner thread holds the ledger through a tick, which writes the ledger file.
    dir.removed_again_after(&ledger);
    ledger.start(false);
    ledger.start(true);
    assert_stops_running(left);
}

/// A process working in a directory that was removed is stopped once that has lasted the grace,
/// and not before.
#[test]
// Unix only: a directory a process works in cannot be removed on Windows.
#[cfg(unix)]
fn a_process_whose_directory_was_removed_is_stopped_after_the_grace() {
    let (state, dir) = app_state("ledger-cwd");
    let work = dir.join("work");
    std::fs::create_dir(&work).unwrap();
    let pid_file = dir.join("child");
    let script = format!(
        "(cd '{}' && exec {}) &",
        work.display(),
        in_own_session(&format!(
            "{} exec \"/bin/sleep\", \"300\";",
            perl_write(&pid_file, "$$")
        ))
    );
    let ledger = &state.process_ledger;
    *ledger.cwd_grace.lock().unwrap() = Some(Duration::from_millis(500));
    let _live = agent(&state, "s1", &script);
    let child = wait_for_pid(&pid_file);
    let _guard = KillOnDrop(vec![child]);
    let deadline = Instant::now() + PATIENCE;
    while table::argv(identity(child))
        .is_none_or(|argv| argv.first().map(String::as_str) != Some("/bin/sleep"))
    {
        assert!(Instant::now() < deadline, "the child never became sleep");
        std::thread::sleep(Duration::from_millis(20));
    }
    let child_id = identity(child);
    ledger.tick();
    assert!(launch_of(ledger, child).is_some());

    std::fs::remove_dir(&work).unwrap();
    ledger.tick();
    assert!(child_id.is_running(), "stopped before the grace was over");
    std::thread::sleep(Duration::from_millis(600));
    ledger.tick();
    assert_stops_running(child);
}

/// A process working in a directory keeps it from being removed on Windows, so no process works in
/// a removed one, nothing is left behind in one and the ledger never stops a process for it.
#[test]
#[cfg(windows)]
fn a_directory_a_process_works_in_cannot_be_removed_so_nothing_is_stopped_for_it() {
    let (state, dir) = app_state("ledger-cwd-kept");
    let work = dir.join("work");
    std::fs::create_dir(&work).unwrap();
    let pid_file = dir.join("child");
    let ledger = &state.process_ledger;
    *ledger.cwd_grace.lock().unwrap() = Some(Duration::ZERO);
    let _live = agent_of(
        &state,
        "s1",
        vec![Background::sleeper(&pid_file).in_dir(&work)],
    );
    let child = wait_for_pid(&pid_file);
    let _guard = KillOnDrop(vec![child]);
    ledger.tick();
    assert!(launch_of(ledger, child).is_some());

    assert!(
        std::fs::remove_dir(&work).is_err(),
        "the directory a process works in was removed"
    );
    ledger.tick();
    ledger.tick();
    assert!(identity(child).is_running(), "stopped for its directory");
}

/// A look at the working directory of a followed process that does not answer, as one on a dead
/// network mount does not, holds a tick of the scanner for no longer than a look's deadline, and
/// while that look is stuck no later tick starts another at the same directory.
#[test]
fn a_working_directory_that_does_not_answer_does_not_hold_the_scanner() {
    let (state, dir) = app_state("ledger-cwd-stuck");
    let work = dir.join("work");
    std::fs::create_dir(&work).unwrap();
    let pid_file = dir.join("child");
    let ledger = &state.process_ledger;
    let _live = agent_of(
        &state,
        "s1",
        vec![Background::sleeper(&pid_file).in_dir(&work)],
    );
    let child = wait_for_pid(&pid_file);
    let _guard = KillOnDrop(vec![child]);
    let deadline = Instant::now() + PATIENCE;
    while launch_of(ledger, child).is_none() {
        assert!(Instant::now() < deadline, "the child was never attributed");
        ledger.tick();
        std::thread::sleep(Duration::from_millis(20));
    }

    let work = crate::hostfs::canonical(&work).unwrap();
    let (release, held) = std::sync::mpsc::channel::<()>();
    let held = std::sync::Mutex::new(held);
    let looks = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = looks.clone();
    *ledger.cwd_look.lock().unwrap() = Some(Arc::new(move |report: &table::CwdReport| {
        if report.path() == work {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            held.lock().unwrap().recv().ok();
        }
        report.looked()
    }));
    LOOK_DEADLINE_OVERRIDE.set(Some(Duration::from_millis(100)));
    let begun = Instant::now();
    ledger.tick();
    assert!(begun.elapsed() < PATIENCE, "the tick waited for the look");
    // Past its own deadline, the look still running counts as stuck.
    std::thread::sleep(Duration::from_millis(150));
    ledger.tick();
    LOOK_DEADLINE_OVERRIDE.set(None);
    assert_eq!(
        looks.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "a second look was started at a directory still stuck"
    );
    assert!(
        is_running(child),
        "a directory that did not answer counted as removed"
    );
    release.send(()).ok();
}

/// A read of what the system reports of a process's working directory that does not answer — on
/// macOS the kernel asks the directory's file system for its identity — holds a call for no
/// longer than a look's deadline, and while it is stuck no later call reads that process's
/// again; the other processes' directories are read and looked at all the same.
#[test]
fn a_working_directory_report_that_does_not_answer_is_left_out_and_not_read_again_while_stuck() {
    let dir = ScratchDir::new("ledger-cwd-report-stuck");
    let me = identity(std::process::id() as i32);
    let stuck = Identity {
        pid: me.pid,
        start_micros: me.start_micros + 1,
    };
    let patience = Instant::now() + PATIENCE;
    let (ledger, reads, release, first) = loop {
        assert!(
            Instant::now() < patience,
            "the stuck read never began in time"
        );
        let ledger = ProcessLedger::new(&dir);
        quiet(&ledger);
        let (release, held) = std::sync::mpsc::channel::<()>();
        let held = std::sync::Mutex::new(held);
        let reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = reads.clone();
        *ledger.cwd_read.lock().unwrap() = Some(Arc::new(move |id: Identity| {
            if id == stuck {
                counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                held.lock().unwrap().recv().ok();
                return None;
            }
            table::cwd_report(id)
        }));
        LOOK_DEADLINE_OVERRIDE.set(Some(Duration::from_millis(500)));
        let begun = Instant::now();
        let first = ledger.cwds([stuck], None);
        assert!(begun.elapsed() < PATIENCE, "the call waited for the read");
        // A pass whose worker never began before its deadline did not test a stuck read.
        if reads.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            continue;
        }
        break (ledger, reads, release, first);
    };
    // Past its own deadline, the read still running counts as stuck. The healthy read is
    // introduced only on the later pass, so its first read gets the whole patience too.
    std::thread::sleep(Duration::from_millis(550));
    LOOK_DEADLINE_OVERRIDE.set(Some(PATIENCE));
    let again = ledger.cwds([me, stuck], None);
    LOOK_DEADLINE_OVERRIDE.set(None);
    assert!(again.contains_key(&me), "{again:?}");
    for cwds in [&first, &again] {
        assert!(!cwds.contains_key(&stuck), "{cwds:?}");
    }
    assert_eq!(
        reads.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "a second read was started for a process whose read is still stuck"
    );
    release.send(()).ok();
}

/// The project of the session `s1`, which every keep in these tests needs: a session in none, a
/// console session, keeps nothing.
fn in_a_project() -> std::collections::HashMap<String, String> {
    std::collections::HashMap::from([("s1".to_string(), "p1".to_string())])
}

/// A launch's trees are listed with what a client is shown, a tree the user keeps is no longer
/// listed under the session nor stopped with it, and a stop names a tree by its top.
#[test]
fn trees_are_listed_kept_and_stopped_by_their_top() {
    let (state, dir) = app_state("ledger-trees");
    let (listener_file, port_file, other_file) =
        (dir.join("listener"), dir.join("port"), dir.join("other"));
    #[cfg(unix)]
    let listener = Background::perl(&format!(
        "use IO::Socket::INET; my $s = IO::Socket::INET->new(Listen => 5, LocalAddr => \"127.0.0.1:0\") or die; \
         {} {} sleep 300;",
        perl_write(&port_file, "$s->sockport"),
        perl_write(&listener_file, "$$"),
    ));
    #[cfg(windows)]
    let listener = Background::process(
        "listener",
        Steps::new()
            .listen(&port_file)
            .write_pid(&listener_file)
            .sleep(300),
    );
    let other = Background::sleeper(&other_file);
    let _live = agent_of(&state, "s1", vec![listener, other]);
    let listener = wait_for_pid(&listener_file);
    let other = wait_for_pid(&other_file);
    let _guard = KillOnDrop(vec![listener, other]);
    let port: u16 = std::fs::read_to_string(&port_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let ledger = &state.process_ledger;

    let deadline = Instant::now() + PATIENCE;
    let trees = loop {
        let trees = ledger.trees_of_sessions(&["s1".to_string()], Duration::ZERO);
        if trees.len() == 2 {
            break trees;
        }
        assert!(
            Instant::now() < deadline,
            "the trees were never listed: {trees:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    let listening = trees
        .iter()
        .find(|tree| tree.id == identity(listener).key())
        .expect("the listener's tree");
    assert_eq!(listening.session_id.as_deref(), Some("s1"));
    assert!(
        listening
            .command
            .contains(if cfg!(windows) { "listener" } else { "perl" }),
        "{listening:?}"
    );
    assert_eq!(listening.ports, vec![port]);
    assert_eq!(listening.process_count, 1);
    assert!(listening.memory_bytes > 0);
    assert!(!listening.cwd_missing);
    assert!(ledger
        .trees_of_sessions(&["s2".to_string()], Duration::ZERO)
        .is_empty());

    let other_key = identity(other).key();
    assert_eq!(
        ledger.keep_trees(std::slice::from_ref(&listening.id), &[], &in_a_project()),
        Ok(vec![listening.id.clone()])
    );
    assert_eq!(
        ledger
            .stop_trees(std::slice::from_ref(&other_key), &[])
            .stopped,
        vec![other_key.clone()]
    );
    assert_stops_running(other);
    assert!(identity(listener).is_running());
    assert!(ledger
        .trees_of_sessions(&["s1".to_string()], Duration::ZERO)
        .is_empty());
}

/// A tree's original name still keeps its descendants across successive exits of its top,
/// without keeping another tree of the same launch.
#[test]
fn an_original_tree_name_keeps_descendants_across_successive_top_exits() {
    let (state, dir) = app_state("ledger-tree-alias");
    let (a_file, b_file, c_file, other_file) = (
        dir.join("a"),
        dir.join("b"),
        dir.join("c"),
        dir.join("other"),
    );
    let (a_end, b_fork, b_end) = (dir.join("a-end"), dir.join("b-fork"), dir.join("b-end"));
    #[cfg(unix)]
    let tree = Background::perl(&format!(
        "my $b = fork(); if ($b == 0) {{ {} {} \
         my $c = fork(); if ($c == 0) {{ exec \"/bin/sleep\", \"300\" }} {} {} exit 0; }} {} {} exit 0;",
        perl_write(&b_file, "$$"),
        perl_wait_for(&b_fork),
        perl_write(&c_file, "$c"),
        perl_wait_for(&b_end),
        perl_write(&a_file, "$$"),
        perl_wait_for(&a_end),
    ));
    #[cfg(windows)]
    let tree = Background::process(
        "a",
        Steps::new()
            .spawn(
                "b",
                Steps::new()
                    .write_pid(&b_file)
                    .wait_for(&b_fork)
                    .spawn("sleep", Steps::new().sleep(300))
                    .write_child_pid(&c_file)
                    .wait_for(&b_end)
                    .exit(),
            )
            .write_pid(&a_file)
            .wait_for(&a_end)
            .exit(),
    );
    #[cfg(unix)]
    let other = Background::perl(&format!("{} sleep 300;", perl_write(&other_file, "$$")));
    #[cfg(windows)]
    let other = Background::sleeper(&other_file);
    let live = agent_of(&state, "s1", vec![tree, other]);
    let (a, b, other) = (
        wait_for_pid(&a_file),
        wait_for_pid(&b_file),
        wait_for_pid(&other_file),
    );
    let mut guard = KillOnDrop(vec![a, b, other]);
    let a_id = identity(a);
    let ledger = &state.process_ledger;
    let sessions = ["s1".to_string()];
    let original = ledger.trees_of_sessions(&sessions, Duration::ZERO);
    assert!(original
        .iter()
        .any(|tree| tree.id == a_id.key() && tree.process_count == 2));
    ledger.note_listed("s1".to_string(), ledger.lists_generation(), original);

    std::fs::write(&a_end, "").unwrap();
    assert_stops_running(a);
    std::fs::write(&b_fork, "").unwrap();
    let c = wait_for_pid(&c_file);
    guard.0.push(c);
    ledger.scan(false);
    assert!(ledger.book().attributed.contains_key(&identity(c)));
    // The archive dialog still holds the original response; only the regular scanner has
    // observed the surviving child and its new descendant.
    assert!(ledger
        .listed_lately("s1", Duration::from_secs(60))
        .unwrap()
        .iter()
        .any(|tree| tree.id == a_id.key()));
    std::fs::write(&b_end, "").unwrap();
    assert_stops_running(b);
    let final_trees = ledger.trees_of_sessions(&sessions, Duration::ZERO);
    assert!(final_trees.iter().any(|tree| tree.id == identity(c).key()));

    assert_eq!(
        ledger.keep_trees(&[a_id.key()], &[], &in_a_project()),
        Ok(vec![a_id.key()])
    );
    assert!(ledger.book().kept.contains(&identity(c)));
    assert!(!ledger.book().kept.contains(&identity(other)));
    ledger.end_launch(&live.launch_id);
    assert!(
        is_running(c),
        "the descendant kept by the original name was stopped"
    );
    assert_stops_running(other);
}

/// A process no launch has, working in a removed directory under a project's, is listed as left
/// behind and stopped when asked by its id; one whose directory is still there, one whose
/// removed directory was elsewhere, and the daemon's own child with what runs in the terminal
/// session it leads (a shell terminal of the user's, and a command in it) are neither.
#[test]
// Unix only: a leftover is a process working in a removed directory, which Windows never lets a
// process's directory be.
#[cfg(unix)]
fn what_was_left_in_a_removed_directory_of_a_project_is_listed_and_stopped_on_request() {
    let dir = ScratchDir::new("ledger-leftovers");
    let ledger = ProcessLedger::new(&dir);
    quiet(&ledger);
    let project = dir.join("project");
    let (removed, intact, elsewhere) = (
        project.join(".worktrees").join("gone"),
        project.join("kept"),
        dir.join("elsewhere"),
    );
    for path in [&removed, &intact, &elsewhere] {
        std::fs::create_dir_all(path).unwrap();
    }
    let sleep_in = |cwd: &Path| detached_in(cwd, "/bin/sleep", &["60"], &[]);
    let (left, intact_pid, elsewhere_pid) =
        (sleep_in(&removed), sleep_in(&intact), sleep_in(&elsewhere));
    // A shell terminal: the daemon's child, leading a session, and a command run in it.
    let command_file = dir.join("command");
    let mut shell = Command::new("/usr/bin/perl");
    shell
        .args([
            "-e",
            &format!(
                "my $c = fork(); if ($c == 0) {{ exec \"/bin/sleep\", \"60\" }} {} sleep 60;",
                perl_write(&command_file, "$c")
            ),
        ])
        .current_dir(&removed);
    crate::platform::proc::lead_session(&mut shell);
    let shell = Children(vec![shell.spawn().unwrap()]);
    let command = wait_for_pid(&command_file);
    let _guard = KillOnDrop(vec![left, intact_pid, elsewhere_pid, command]);
    let left = identity(left);
    let (intact_real, elsewhere_real) = (
        crate::hostfs::canonical(&intact).unwrap(),
        crate::hostfs::canonical(&elsewhere).unwrap(),
    );
    std::fs::remove_dir(&removed).unwrap();
    std::fs::remove_dir(&elsewhere).unwrap();

    let leftovers = ledger.trees_of_project(&[], "project", std::slice::from_ref(&project));
    assert_eq!(
        leftovers
            .iter()
            .map(|tree| tree.id.as_str())
            .collect::<Vec<_>>(),
        vec![left.key().as_str()]
    );
    assert!(leftovers[0].cwd_missing);
    assert_eq!(leftovers[0].session_id, None);
    // Only a directory the system reports under the project's is looked at.
    let looked = ledger.looks.looked();
    assert!(looked.contains(&intact_real), "{looked:?}");
    assert!(
        !looked
            .iter()
            .any(|path| path.starts_with(&elsewhere) || path.starts_with(&elsewhere_real)),
        "a directory under no project was looked at: {looked:?}"
    );
    assert!(
        ledger.stop_trees(&[left.key()], &[]).stopped.is_empty(),
        "not a leftover without the project's directories"
    );
    assert_eq!(
        ledger
            .stop_trees(&[left.key()], std::slice::from_ref(&project))
            .stopped,
        vec![left.key()]
    );
    assert_stops_running(left.pid.0 as i32);
    for pid in [intact_pid, elsewhere_pid, command, shell.0[0].id() as i32] {
        assert!(is_running(pid), "{pid} was stopped");
    }
}

/// What a bash script runs to start this test binary as [`sleeper`], detached, writing its pid to
/// `pid_file`, carrying whatever tag the script's environment has.
#[cfg(unix)]
fn detached_sleeper_in_script(pid_file: &Path) -> String {
    let pid_file = pid_file.display();
    format!(
        "{SLEEP_VAR}=60 /usr/bin/perl -MPOSIX -e 'my $c = fork(); if ($c == 0) {{ POSIX::setsid(); \
         open(STDOUT, \">\", \"/dev/null\"); open(STDERR, \">\", \"/dev/null\"); exec @ARGV }} \
         open(my $f, \">\", \"{pid_file}.tmp\"); print $f $c; close $f; rename \"{pid_file}.tmp\", \"{pid_file}\";' \
         -- '{}' {}",
        test_binary(),
        SLEEPER.join(" ")
    )
}

/// The launch id a session's launch reserves before its agent starts is the one the agent's
/// processes carry and the one the ledger follows once it has started: a process that carries
/// it and that no chain or session reaches is the running launch's, not an ended one's.
#[tokio::test(flavor = "multi_thread")]
// Unix only: a tag is read from another process's environment, which Windows does not read.
#[cfg(unix)]
async fn a_launch_is_followed_under_the_tag_it_reserved_before_its_agent_started() {
    let (state, dir) = console_with_project_and_accounts("ledger-handoff");
    state
        .store
        .insert_session(&Session {
            agent_session_id: Some("conversation-s1".to_string()),
            has_conversation: true,
            ..project_session("s1", AgentId::CLAUDE, SessionStatus::Interrupted)
        })
        .unwrap();
    let pid_file = dir.join("tagged");
    let script = format!("{} &\nexec sleep 30", detached_sleeper_in_script(&pid_file));
    let started = Arc::new(std::sync::Mutex::new(Vec::<StandIn>::new()));
    let launches = Arc::new(std::sync::Mutex::new(Vec::<Launched>::new()));
    let claim = state.begin_launch("s1").unwrap();
    crate::coordinator::launch::relaunch_session_with(
        &state,
        &claim,
        "s1",
        None,
        true,
        None,
        None,
        stand_in_launch_running(&started, &launches, &script, || {}),
    )
    .await
    .unwrap();
    drop(claim);
    let tagged = wait_for_pid(&pid_file);
    let _guard = KillOnDrop(vec![tagged]);
    wait_for_tag(tagged);
    let live = state.live_session("s1").expect("the session runs");

    let ledger = &state.process_ledger;
    ledger.scan(false);
    assert_eq!(
        launch_of(ledger, tagged).as_deref(),
        Some(live.launch_id.as_str())
    );
    ledger.tick();
    assert!(is_running(tagged), "taken for an ended launch's");
}

/// Agent processes started at once, as the suite's other tests do, so that a scan reads the
/// environment of what starts after them.
fn following(state: &Arc<AppState>) -> StandIn {
    agent_of(state, "s0", Vec::new())
}

/// A scan between a launch's reservation and its start leaves the agent's internals alone, tag
/// and all: they sit in the terminal session the daemon's direct child leads, though the agent is
/// not known yet. Its start changes nothing.
#[test]
// Unix only: a terminal session is a POSIX grouping; Windows follows a launch by its job.
#[cfg(unix)]
fn a_scan_before_a_launch_starts_leaves_its_agents_internals_alone() {
    let (state, dir) = app_state("ledger-early-scan");
    let _first = following(&state);
    let ledger = &state.process_ledger;
    let pending = ledger.begin_launch("s1").unwrap();
    let helper_file = dir.join("helper");
    let script = format!(
        "{SLEEP_VAR}=60 '{}' {} >/dev/null 2>&1 &\necho $! > '{}.tmp' && mv '{}.tmp' '{}'\nexec sleep 300",
        test_binary(),
        SLEEPER.join(" "),
        helper_file.display(),
        helper_file.display(),
        helper_file.display()
    );
    let live = tagged_live_session("s1", AgentId::CLAUDE, &script, pending.tag());
    let helper = wait_for_pid(&helper_file);
    let _guard = KillOnDrop(vec![helper]);
    wait_for_tag(helper);
    ledger.scan(false);
    assert_eq!(
        launch_of(ledger, helper),
        None,
        "in the agent's session, though the agent is not known yet"
    );

    ledger.launch_started(pending.tag().launch_id(), "s1", live.handle());
    ledger.scan(false);
    assert_eq!(launch_of(ledger, helper), None);
}

/// A process carrying the tag of a launch that is reserved but not started yet is that launch's,
/// and left running; once the reservation is given up without the launch starting, it is an
/// ended launch's, and stopped.
#[test]
// Unix only: a tag is read from another process's environment, which Windows does not read.
#[cfg(unix)]
fn a_reserved_launch_keeps_what_carries_its_tag_until_it_is_given_up() {
    let (state, _dir) = app_state("ledger-reserved");
    let _first = following(&state);
    let ledger = &state.process_ledger;
    let pending = ledger.begin_launch("s1").unwrap();
    let launch = pending.tag().launch_id().to_string();
    let tagged = tagged_sleeper(&crate::platform::proc::tag_value(&ledger.instance, &launch));
    let _guard = KillOnDrop(vec![tagged]);

    ledger.scan(false);
    assert_eq!(launch_of(ledger, tagged), Some(launch));
    ledger.tick();
    assert!(is_running(tagged), "stopped while its launch was reserved");

    drop(pending);
    assert_eq!(launch_of(ledger, tagged).as_deref(), Some(ENDED));
    ledger.tick();
    assert_stops_running(tagged);
}

/// Two ends of one launch both return only once its processes are stopped, the second waiting
/// for the first; and a keep of its tree while it is ending keeps nothing.
#[test]
fn a_launch_being_ended_keeps_nothing_and_each_end_returns_once_it_is_done() {
    let (state, dir) = app_state("ledger-end-twice");
    let pid_file = dir.join("child");
    // A process that takes no polite request to end, as a Windows process has none.
    #[cfg(unix)]
    let child = Background::perl(&format!(
        "$SIG{{TERM}} = \"IGNORE\"; {} sleep 300;",
        perl_write(&pid_file, "$$")
    ));
    #[cfg(windows)]
    let child = Background::sleeper(&pid_file);
    let live = agent_of(&state, "s1", vec![child]);
    let child = wait_for_pid(&pid_file);
    let _guard = KillOnDrop(vec![child]);
    let child_id = identity(child);
    let ledger = Arc::clone(&state.process_ledger);
    ledger.scan(false);
    assert_eq!(
        launch_of(&ledger, child).as_deref(),
        Some(live.launch_id.as_str())
    );

    let ends: Vec<_> = (0..2)
        .map(|_| {
            let (ledger, launch) = (Arc::clone(&ledger), live.launch_id.clone());
            std::thread::spawn(move || {
                ledger.end_launch(&launch);
                child_id.is_running()
            })
        })
        .collect();
    let deadline = Instant::now() + PATIENCE;
    while !ledger
        .book()
        .launches
        .get(&live.launch_id)
        .is_some_and(|launch| launch.ending)
    {
        assert!(Instant::now() < deadline, "the launch never began ending");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(ledger
        .keep_trees(&[child_id.key()], &[], &in_a_project())
        .is_ok_and(|kept| kept.is_empty()));
    for end in ends {
        assert!(
            !end.join().unwrap(),
            "an end returned with the child running"
        );
    }
    assert!(!ledger.book().launches.contains_key(&live.launch_id));
}

/// An environment read without a tag is read again while the process is young, since it may
/// not have run its own program yet; once it has run for long enough, the read stands.
#[test]
// Unix only: a tag is read from another process's environment, which Windows does not read.
#[cfg(unix)]
fn an_untagged_read_stands_only_for_a_settled_process() {
    let (state, dir) = app_state("ledger-settled");
    let live = following(&state);
    let ledger = &state.process_ledger;
    let tag = format!("{}:{}", ledger.instance, live.launch_id);
    // A process that shows no tag until `go` exists, then runs itself again with the tag.
    let retagging = |go: &Path| {
        let pid = detached(
            &test_binary(),
            &SLEEPER,
            &[
                (SLEEP_VAR, Some("60")),
                (TAG_VARIABLE, None),
                (RETAG_VAR, Some(&tag)),
                (GO_VAR, Some(&go.to_string_lossy())),
            ],
        );
        let deadline = Instant::now() + PATIENCE;
        while table::env_tag(identity(pid)) != table::EnvTag::Untagged {
            assert!(Instant::now() < deadline, "{pid} never showed no tag");
            std::thread::sleep(Duration::from_millis(20));
        }
        pid
    };

    let young_go = dir.join("young");
    let young = retagging(&young_go);
    let _young = KillOnDrop(vec![young]);
    ledger.scan(false);
    assert_eq!(launch_of(ledger, young), None);
    std::fs::write(&young_go, "").unwrap();
    wait_for_tag(young);
    ledger.scan(false);
    assert_eq!(
        launch_of(ledger, young).as_deref(),
        Some(live.launch_id.as_str()),
        "a young process's read without a tag was kept"
    );

    let settled_go = dir.join("settled");
    let settled = retagging(&settled_go);
    let _settled = KillOnDrop(vec![settled]);
    SETTLED_OVERRIDE.set(Some(Duration::ZERO));
    ledger.scan(false);
    SETTLED_OVERRIDE.set(None);
    std::fs::write(&settled_go, "").unwrap();
    wait_for_tag(settled);
    ledger.scan(false);
    assert_eq!(
        launch_of(ledger, settled),
        None,
        "a settled process's read without a tag was not kept"
    );
}

/// An environment that cannot be read is read again only once its process runs another program.
#[cfg(target_os = "macos")]
#[test]
fn an_unreadable_environment_is_read_again_only_once_another_program_runs() {
    let (state, dir) = app_state("ledger-unreadable-env");
    let _first = following(&state);
    let ledger = &state.process_ledger;
    let (ready, go) = (dir.join("ready"), dir.join("go"));
    // Perl and then `sleep`: platform binaries both, whose environment the system withholds where
    // SIP is on. Perl says it is ready from its own program, past the execs that start it, so
    // that what its environment reads as below is perl's.
    let script = format!(
        "{} {} exec \"/bin/sleep\", \"60\";",
        perl_write(&ready, "$$"),
        perl_wait_for(&go)
    );
    let pid = detached("/usr/bin/perl", &["-e", &script], &[]);
    let _guard = KillOnDrop(vec![pid]);
    assert_eq!(wait_for_pid(&ready), pid);
    let id = identity(pid);
    // Where SIP is off, as on GitHub's macOS runners, the system hands the environment out: the
    // scans then read it as unreadable through the override instead.
    let withheld = table::env_tag(id) == table::EnvTag::Unreadable;
    UNREADABLE_OVERRIDE.set((!withheld).then_some(pid));
    let reads = || ledger.scans.lock().unwrap().env_reads.get(&id).copied();

    ledger.scan(false);
    ledger.scan(false);
    assert_eq!(reads(), Some(1));
    std::fs::write(&go, "").unwrap();
    let deadline = Instant::now() + PATIENCE;
    while table::Table::read()
        .and_then(|table| {
            table
                .get(ProcessId(pid as u32))
                .map(|proc| proc.comm.clone())
        })
        .as_deref()
        != Some("sleep")
    {
        assert!(Instant::now() < deadline, "{pid} never ran sleep");
        std::thread::sleep(Duration::from_millis(20));
    }
    ledger.scan(false);
    ledger.scan(false);
    assert_eq!(reads(), Some(2));
    UNREADABLE_OVERRIDE.set(None);
}

/// Removed-directory cleanup respects the same retry delay as ended-launch cleanup when a
/// process survives `SIGKILL`.
#[test]
// Unix only: a directory a process works in cannot be removed on Windows.
#[cfg(unix)]
fn a_removed_directory_process_that_outlives_sigkill_is_tried_again_only_later() {
    let (state, dir) = app_state("ledger-cwd-stubborn");
    let work = dir.join("work");
    std::fs::create_dir(&work).unwrap();
    let ready = dir.join("ready");
    let script = format!(
        "(cd '{}' && exec {}) &",
        work.display(),
        in_own_session(&format!(
            "$SIG{{TERM}} = \"IGNORE\"; {} sleep 300;",
            perl_write(&ready, "$$")
        ))
    );
    let _live = agent(&state, "s1", &script);
    let pid = wait_for_pid(&ready);
    let _guard = KillOnDrop(vec![pid]);
    let id = identity(pid);
    let ledger = &state.process_ledger;
    *ledger.cwd_grace.lock().unwrap() = Some(Duration::ZERO);
    ledger.scan(false);
    assert!(launch_of(ledger, pid).is_some());
    std::fs::remove_dir(&work).unwrap();

    stop::SPARED.with_borrow_mut(|spared| spared.insert(pid));
    ledger.tick();
    stop::SPARED.with_borrow_mut(|spared| spared.remove(&pid));
    assert!(id.is_running());
    assert!(ledger.stubborn.lock().unwrap().contains_key(&id));
    ledger.tick();
    assert!(
        id.is_running(),
        "removed-directory cleanup bypassed the retry delay"
    );

    ledger
        .stubborn
        .lock()
        .unwrap()
        .insert(id, Some(Instant::now() - super::STUBBORN_RETRY));
    ledger.tick();
    assert_stops_running(pid);
}

/// The daemon ending an agent reads the table first, so that what the agent started a moment
/// before, which no scan has seen, is attributed while its chain through the agent shows: on
/// macOS a platform binary in a session of its own is reached no other way once its wrapper
/// shell has outlived the agent.
#[tokio::test(flavor = "multi_thread")]
async fn ending_an_agent_first_attributes_what_it_started_a_moment_before() {
    let (state, dir) = app_state("ledger-end-scan");
    let (child_file, go) = (dir.join("child"), dir.join("go"));
    let live = agent_of(
        &state,
        "s1",
        vec![wrapper_leaving_a_sleeper(&child_file, &go)],
    );
    crate::coordinator::exit_watch::watch_exit(&state, (*live).clone());
    let child = wait_for_pid(&child_file);
    let mut guard = KillOnDrop(vec![child]);
    let child_id = identity(child);

    crate::coordinator::launch::stop_process(&state, (*live).clone())
        .await
        .unwrap();
    std::fs::write(&go, "").unwrap();
    let deadline = Instant::now() + PATIENCE;
    while state.has_process("s1") {
        assert!(Instant::now() < deadline, "the session never stopped");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(!child_id.is_running(), "left running");
    guard.0.clear();
}

/// What a stop goes on to after a round of signals: a new process of the same launch that
/// descends from a stopped one, by its parent or by sitting in a session one led; not one that
/// merely shares a session a stopped one did not lead, nor another launch's, nor one seen
/// before.
#[test]
fn a_stop_goes_on_only_to_new_descendants_of_the_same_launch() {
    let proc = |pid: i32, ppid: i32, sid: i32| Proc {
        id: Identity {
            pid: ProcessId(pid as u32),
            start_micros: pid as u64,
        },
        ppid: ProcessId(ppid as u32),
        sid: Some(ContainerId(sid as u32)),
        comm: String::new(),
    };
    let table = Table::from_procs([
        proc(10, 1, 10),
        proc(11, 10, 10),
        proc(12, 1, 10),
        proc(20, 1, 5),
        proc(21, 1, 5),
        proc(22, 10, 10),
        proc(30, 1, 30),
    ]);
    let mut book = Book::default();
    for (pid, launch) in [(11, "a"), (12, "a"), (21, "a"), (22, "b"), (30, "a")] {
        book.attributed.insert(
            Identity {
                pid: ProcessId(pid),
                start_micros: pid as u64,
            },
            Attributed {
                launch: launch.to_string(),
                sid: None,
            },
        );
    }
    let id = |pid: u32| Identity {
        pid: ProcessId(pid),
        start_micros: pid as u64,
    };
    let stopped: HashSet<Identity> = [id(10), id(20)].into();
    let led: HashSet<ContainerId> = [ContainerId(10)].into();
    let known: HashSet<Identity> = [id(30)].into();
    let launches: HashSet<String> = ["a".to_string()].into();
    let mut found = follow_ups(&book, &table, &known, &stopped, &led, &launches);
    found.sort();
    assert_eq!(found, vec![id(11), id(12)]);
}

/// A reconcile whose read of the table fails still takes in what the file says was kept, so no
/// later scan takes a kept process for an ended launch's.
#[test]
fn a_reconcile_that_cannot_read_the_table_still_takes_in_what_was_kept() {
    let dir = ScratchDir::new("ledger-reconcile-no-table");
    let ledger = ProcessLedger::new(&dir);
    quiet(&ledger);
    let kept = detached_sleep_in(&std::env::current_dir().unwrap(), 60);
    let _guard = KillOnDrop(vec![kept]);
    let id = identity(kept);
    persist::save(
        &dir.join(persist::LEDGER_FILE),
        &persist::Persisted {
            kept: vec![id],
            ..this_boot()
        },
    )
    .unwrap();

    ledger
        .failing_reads
        .store(1, std::sync::atomic::Ordering::Release);
    ledger.reconcile();
    assert!(ledger.book().kept.contains(&id));
}

/// A reconcile stops what sits in a terminal session the previous run recorded for a launch,
/// whose leader is gone, though the file does not name it and no tag can be read: a process the
/// file names still in the session shows it is the one recorded.
#[test]
// Unix only: a terminal session is a POSIX grouping; Windows follows a launch by its job.
#[cfg(unix)]
fn reconcile_stops_what_sits_in_a_session_the_previous_run_recorded() {
    let dir = ScratchDir::new("ledger-reconcile-session");
    let ledger = ProcessLedger::new(&dir);
    quiet(&ledger);
    let (named_file, later_file, go) = (dir.join("named"), dir.join("later"), dir.join("go"));
    let fork_sleep = "my $c = fork(); if ($c == 0) { exec \"/bin/sleep\", \"60\" }";
    let leader_script = format!(
        "{{ {fork_sleep} {} }} {} {{ {fork_sleep} {} }} exit 0;",
        perl_write(&named_file, "$c"),
        perl_wait_for(&go),
        perl_write(&later_file, "$c"),
    );
    let leader = detached(
        "/usr/bin/perl",
        &["-e", &leader_script],
        &[(TAG_VARIABLE, None)],
    );
    let named = wait_for_pid(&named_file);
    let _guard = KillOnDrop(vec![leader, named]);
    let leader_id = identity(leader);
    let named_id = identity(named);
    persist::save(
        &dir.join(persist::LEDGER_FILE),
        &persist::Persisted {
            attributed: vec![persist::PersistedProcess {
                pid: named,
                start: named_id.start_micros,
                launch: uuid::Uuid::new_v4().to_string(),
                sid: leader,
            }],
            sessions: vec![persist::PersistedSession {
                sid: leader,
                leader_start: Some(leader_id.start_micros),
            }],
            ..this_boot()
        },
    )
    .unwrap();
    std::fs::write(&go, "").unwrap();
    let later = wait_for_pid(&later_file);
    let _later_guard = KillOnDrop(vec![later]);
    let deadline = Instant::now() + PATIENCE;
    while leader_id.is_running() {
        assert!(Instant::now() < deadline, "the leader never exited");
        std::thread::sleep(Duration::from_millis(20));
    }

    ledger.reconcile();
    assert_stops_running(named);
    assert_stops_running(later);
}

/// A terminal session the previous run recorded, for a launch or as kept, whose members have all
/// exited may have had its id handed to an unrelated process since, which started a session and
/// left a member in it with no leader. A reconcile neither stops nor keeps what sits in it.
#[test]
// Unix only: a terminal session is a POSIX grouping; Windows follows a launch by its job.
#[cfg(unix)]
fn reconcile_takes_no_session_whose_id_has_passed_to_unrelated_processes() {
    let dir = ScratchDir::new("ledger-reconcile-reused-session");
    let ledger = ProcessLedger::new(&dir);
    quiet(&ledger);
    let leaderless = |name: &str| {
        let child_file = dir.join(name);
        let script = format!(
            "my $c = fork(); if ($c == 0) {{ exec \"/bin/sleep\", \"60\" }} {} exit 0;",
            perl_write(&child_file, "$c")
        );
        let leader = detached("/usr/bin/perl", &["-e", &script], &[(TAG_VARIABLE, None)]);
        (leader, wait_for_pid(&child_file))
    };
    let (ended_sid, in_ended) = leaderless("ended");
    let (kept_sid, in_kept) = leaderless("kept");
    let _guard = KillOnDrop(vec![in_ended, in_kept]);
    // What the file named of those sessions are processes that had their ids before.
    let before = |pid: i32| Identity {
        pid: ProcessId(pid as u32),
        start_micros: 1,
    };
    persist::save(
        &dir.join(persist::LEDGER_FILE),
        &persist::Persisted {
            attributed: vec![persist::PersistedProcess {
                pid: ended_sid,
                start: 1,
                launch: uuid::Uuid::new_v4().to_string(),
                sid: ended_sid,
            }],
            kept: vec![before(kept_sid)],
            sessions: vec![persist::PersistedSession {
                sid: ended_sid,
                leader_start: Some(1),
            }],
            kept_sessions: vec![persist::PersistedSession {
                sid: kept_sid,
                leader_start: None,
            }],
            ..this_boot()
        },
    )
    .unwrap();
    for sid in [ended_sid, kept_sid] {
        let deadline = Instant::now() + PATIENCE;
        while table::start_of(ProcessId(sid as u32)).is_some() {
            assert!(Instant::now() < deadline, "the leader {sid} never exited");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    ledger.reconcile();
    ledger.tick();
    for pid in [in_ended, in_kept] {
        assert!(is_running(pid), "{pid} was stopped");
        assert!(launch_of(&ledger, pid).is_none(), "{pid} was attributed");
    }
    let book = ledger.book();
    assert!(!book.kept.contains(&identity(in_kept)), "kept");
    assert!(!book
        .kept_sessions
        .contains_key(&ContainerId(kept_sid as u32)));
    assert!(!book.launches[ENDED]
        .sessions
        .contains_key(&ContainerId(ended_sid as u32)));
}

/// What the ledger file holds: the sessions recorded for the launches, and those of kept
/// processes, which a restart reads back.
#[test]
fn the_ledger_file_holds_the_recorded_and_the_kept_sessions() {
    let (state, dir) = app_state("ledger-write-sessions");
    let (kept_file, other_file) = (dir.join("kept"), dir.join("other"));
    #[cfg(unix)]
    let commands = vec![
        Background::perl(&format!("{} sleep 300;", perl_write(&kept_file, "$$"))),
        Background::perl(&format!("{} sleep 300;", perl_write(&other_file, "$$"))),
    ];
    #[cfg(windows)]
    let commands = vec![
        Background::sleeper(&kept_file),
        Background::sleeper(&other_file),
    ];
    let _live = agent_of(&state, "s1", commands);
    let (kept, other) = (wait_for_pid(&kept_file), wait_for_pid(&other_file));
    let _guard = KillOnDrop(vec![kept, other]);
    let ledger = &state.process_ledger;
    ledger.scan(false);
    let kept_id = identity(kept);
    assert_eq!(
        ledger.keep_trees(&[kept_id.key()], &[], &in_a_project()),
        Ok(vec![kept_id.key()])
    );

    let persist::Loaded::Read(written) = persist::load(&dir.join(persist::LEDGER_FILE)) else {
        panic!("the ledger file was not written");
    };
    #[cfg(unix)]
    {
        assert!(written.kept_sessions.contains(&persist::PersistedSession {
            sid: kept,
            leader_start: Some(kept_id.start_micros),
        }));
        assert!(written.sessions.contains(&persist::PersistedSession {
            sid: other,
            leader_start: Some(identity(other).start_micros),
        }));
    }
    // No session is recorded on Windows, where a launch is followed by its job: the file holds the
    // kept process and the one still attributed.
    #[cfg(windows)]
    {
        assert!(written.kept_sessions.is_empty() && written.sessions.is_empty());
        assert!(written.kept.contains(&kept_id), "{written:?}");
        assert!(
            written
                .attributed
                .iter()
                .any(|process| process.pid == other),
            "{written:?}"
        );
    }
}

/// An end whose read of the table fails leaves nothing it did not stop to nobody: what was the
/// launch's is held for an ended launch, and the next tick stops it.
#[test]
fn what_an_end_could_not_stop_is_held_for_an_ended_launch() {
    let (state, dir) = app_state("ledger-end-fails");
    let pid_file = dir.join("child");
    let live = agent_of(&state, "s1", vec![Background::sleeper(&pid_file)]);
    let child = wait_for_pid(&pid_file);
    let _guard = KillOnDrop(vec![child]);
    let ledger = &state.process_ledger;
    ledger.scan(false);
    assert_eq!(
        launch_of(ledger, child).as_deref(),
        Some(live.launch_id.as_str())
    );

    ledger
        .failing_reads
        .store(1, std::sync::atomic::Ordering::Release);
    ledger.end_launch(&live.launch_id);
    assert!(is_running(child), "stopped without a read of the table");
    assert_eq!(launch_of(ledger, child).as_deref(), Some(ENDED));
    ledger.tick();
    assert_stops_running(child);
}

/// A process kept after a stop listed it is not signalled: neither in a round that begins after
/// the keep, nor by the `SIGKILL` that ends a round it ignored the `SIGTERM` of.
#[test]
fn a_process_kept_while_a_stop_runs_is_not_signalled() {
    let (state, dir) = app_state("ledger-kept-between");
    let pid_file = dir.join("child");
    let _live = agent_of(&state, "s1", vec![Background::sleeper(&pid_file)]);
    let child = wait_for_pid(&pid_file);
    let _guard = KillOnDrop(vec![child]);
    let ledger = &state.process_ledger;
    let table = ledger.scan(false).unwrap();
    let id = identity(child);
    {
        let mut book = ledger.book();
        book.attributed.remove(&id);
        book.kept.insert(id);
    }
    ledger.stop_set(vec![id], &table);
    assert!(
        is_running(child),
        "signalled in a round that began after the keep"
    );

    // Unix only: a stop of a process that takes no polite request, as on Windows, is one kill, so
    // there is no round for a keep to land in the middle of.
    #[cfg(unix)]
    {
        let ready = dir.join("ignoring");
        let ignoring_script = format!(
            "$SIG{{TERM}} = \"IGNORE\"; {} sleep 60;",
            perl_write(&ready, "$$")
        );
        detached("/usr/bin/perl", &["-e", &ignoring_script], &[]);
        let ignoring = wait_for_pid(&ready);
        let _ignoring = KillOnDrop(vec![ignoring]);
        let kept = std::sync::atomic::AtomicBool::new(false);
        let target = identity(ignoring);
        std::thread::scope(|scope| {
            let stopping = scope.spawn(|| {
                stop::stop(&[target], &|_| {
                    kept.load(std::sync::atomic::Ordering::Acquire)
                })
            });
            std::thread::sleep(Duration::from_millis(200));
            kept.store(true, std::sync::atomic::Ordering::Release);
            assert!(stopping.join().unwrap().is_empty());
        });
        assert!(is_running(ignoring), "killed once kept");
    }
}

/// A process found running after its `SIGKILL` is tried again only after a while, with one
/// warning, or once another launch ends; not at every tick.
#[test]
fn a_process_that_outlives_sigkill_is_tried_again_only_later() {
    let (state, _dir) = app_state("ledger-stubborn");
    let first = following(&state);
    let ledger = &state.process_ledger;
    // The process of an ended launch: on Unix one that carries the tag of one, which nothing else
    // reaches; on Windows, which reads no tag, one the ledger takes to be an ended launch's.
    #[cfg(unix)]
    let pid = {
        let ready = _dir.join("ready");
        detached(
            &test_binary(),
            &SLEEPER,
            &[
                (SLEEP_VAR, Some("60")),
                (IGNORE_TERM_VAR, Some(&ready.to_string_lossy())),
                (
                    TAG_VARIABLE,
                    Some(&format!("{}:{}", ledger.instance, uuid::Uuid::new_v4())),
                ),
            ],
        );
        wait_for_pid(&ready)
    };
    #[cfg(windows)]
    let pid = {
        let pid = detached_sleep_in(&std::env::current_dir().unwrap(), 60);
        ledger.book().attributed.insert(
            identity(pid),
            Attributed {
                launch: ENDED.to_string(),
                sid: None,
            },
        );
        pid
    };
    let _guard = KillOnDrop(vec![pid]);
    let id = identity(pid);
    ledger.scan(false);
    assert_eq!(launch_of(ledger, pid).as_deref(), Some(ENDED));

    stop::SPARED.with_borrow_mut(|spared| spared.insert(pid));
    ledger.tick();
    stop::SPARED.with_borrow_mut(|spared| spared.remove(&pid));
    assert!(id.is_running());
    assert!(ledger.stubborn.lock().unwrap().contains_key(&id));
    ledger.tick();
    assert!(id.is_running(), "tried again at the next tick");

    ledger.end_launch(&first.launch_id);
    ledger.tick();
    assert_stops_running(pid);
}

/// The daemon takes a tag it inherited out of its own environment, so that nothing it starts
/// carries it. Run in a child process: the environment is the whole test process's.
#[test]
fn the_daemon_forgets_a_tag_it_inherited() {
    let dir = ScratchDir::new("ledger-forget-tag");
    let done = dir.join("done");
    let status = Command::new(test_binary())
        .args([
            "--exact",
            "processes::ledger::tests::forget_inherited_tag_child",
            "--ignored",
            "--test-threads=1",
        ])
        .env(TAG_VARIABLE, "instance:launch")
        .env(FORGET_DONE_VAR, &done)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(std::fs::read_to_string(&done).unwrap(), "forgotten");
}

/// Where [`forget_inherited_tag_child`] writes what it found.
const FORGET_DONE_VAR: &str = "OCTOBOARDD_TEST_LEDGER_FORGET_DONE";

#[test]
#[ignore = "run in a process of its own by the_daemon_forgets_a_tag_it_inherited"]
fn forget_inherited_tag_child() {
    let Some(done) = std::env::var_os(FORGET_DONE_VAR) else {
        return;
    };
    assert!(std::env::var_os(TAG_VARIABLE).is_some());
    super::forget_inherited_tag();
    let found = if std::env::var_os(TAG_VARIABLE).is_none() {
        "forgotten"
    } else {
        "still there"
    };
    std::fs::write(done, found).unwrap();
}

/// A stand-in agent for session `id` whose shell command leaves one process running in a terminal
/// session of its own, which writes its pid to `pid_file` and becomes `sleep`. Answers the agent
/// and that process, attributed to the agent's launch.
fn agent_leaving_one(state: &Arc<AppState>, id: &str, pid_file: &Path) -> (StandIn, i32) {
    let live = agent_of(state, id, vec![Background::sleeper(pid_file)]);
    let child = wait_for_pid(pid_file);
    state.process_ledger.scan(false);
    assert_eq!(
        launch_of(&state.process_ledger, child).as_deref(),
        Some(live.launch_id.as_str())
    );
    (live, child)
}

/// A launch marked to be carried over gives what it left running, when it ends, to the running
/// launch of the session it was marked for: listed under that session, found later by the ended
/// launch's tag as that one's, and stopped when that launch ends.
#[test]
// Unix only: a tag is read from another process's environment, which Windows does not read.
#[cfg(unix)]
fn an_ended_launch_marked_to_be_carried_over_gives_its_processes_to_the_receiving_launch() {
    let (state, dir) = app_state("ledger-carry-over");
    let (giver, child) = agent_leaving_one(&state, "s1", &dir.join("child"));
    let taker = agent_of(&state, "s2", Vec::new());
    let ledger = &state.process_ledger;
    let mut guard = KillOnDrop(vec![child]);

    assert!(ledger.mark_carry_over(&giver.launch_id, "s2"));
    ledger.end_launch(&giver.launch_id);
    assert!(is_running(child), "stopped with the launch that started it");
    assert_eq!(
        launch_of(ledger, child).as_deref(),
        Some(taker.launch_id.as_str())
    );
    assert!(!ledger.book().launches.contains_key(&giver.launch_id));
    let trees = ledger.trees_of_sessions(&["s2".to_string()], Duration::ZERO);
    assert_eq!(trees.len(), 1, "{trees:?}");
    assert_eq!(trees[0].id, identity(child).key());
    assert_eq!(trees[0].session_id.as_deref(), Some("s2"));
    assert!(ledger
        .trees_of_sessions(&["s1".to_string()], Duration::ZERO)
        .is_empty());

    let tagged = tagged_sleeper(&format!("{}:{}", ledger.instance, giver.launch_id));
    guard.0.push(tagged);
    ledger.scan(false);
    assert_eq!(
        launch_of(ledger, tagged).as_deref(),
        Some(taker.launch_id.as_str()),
        "the ended launch's tag reads as the launch that took its processes"
    );

    ledger.end_launch(&taker.launch_id);
    assert_stops_running(child);
    assert_stops_running(tagged);
}

/// Carried over while its session has no launch running and a hold for it is open, what a launch
/// left is held, never stopped by the scanner, and taken in by the session's next launch.
#[test]
fn what_is_carried_over_while_the_session_has_no_launch_is_held_for_its_next_one() {
    let (state, dir) = app_state("ledger-carry-held");
    let (first, child) = agent_leaving_one(&state, "s1", &dir.join("child"));
    let _guard = KillOnDrop(vec![child]);
    let ledger = &state.process_ledger;

    let held = ledger.hold_for_launch("s1");
    assert!(ledger.mark_carry_over(&first.launch_id, "s1"));
    ledger.end_launch(&first.launch_id);
    assert!(ledger.book().launches[&first.launch_id].held);
    ledger.tick();
    assert!(is_running(child), "stopped while held");

    let next = agent_of(&state, "s1", Vec::new());
    assert_eq!(
        launch_of(ledger, child).as_deref(),
        Some(next.launch_id.as_str())
    );
    drop(held);
    ledger.tick();
    assert!(
        is_running(child),
        "stopped once the next launch had taken it"
    );

    ledger.end_launch(&next.launch_id);
    assert_stops_running(child);
}

/// What is held for a session's next launch is stopped once no hold for the session is open
/// without a launch having taken it; and a launch marked to be carried over to a session that
/// has neither a launch running nor a hold open is stopped as any, as is one whose mark was
/// taken back.
#[test]
fn what_nothing_takes_in_is_stopped() {
    let (state, dir) = app_state("ledger-carry-dropped");
    let (first, held_child) = agent_leaving_one(&state, "s1", &dir.join("held"));
    let (second, unheld_child) = agent_leaving_one(&state, "s2", &dir.join("unheld"));
    let (third, withdrawn_child) = agent_leaving_one(&state, "s3", &dir.join("withdrawn"));
    let _guard = KillOnDrop(vec![held_child, unheld_child, withdrawn_child]);
    let ledger = &state.process_ledger;

    let held = ledger.hold_for_launch("s1");
    ledger.mark_carry_over(&first.launch_id, "s1");
    ledger.end_launch(&first.launch_id);
    assert!(is_running(held_child));
    drop(held);
    assert_eq!(launch_of(ledger, held_child).as_deref(), Some(ENDED));
    ledger.tick();
    assert_stops_running(held_child);

    ledger.mark_carry_over(&second.launch_id, "nobody");
    ledger.end_launch(&second.launch_id);
    assert_stops_running(unheld_child);

    let _receiving = agent_of(&state, "s4", Vec::new());
    ledger.mark_carry_over(&third.launch_id, "s4");
    ledger.withdraw_carry_over(&third.launch_id);
    ledger.end_launch(&third.launch_id);
    assert_stops_running(withdrawn_child);
}

/// The mark a handoff puts on the original's launch for the receiving session survives that
/// session's own account switch ending: the hold of the switch only clears the marks of its own
/// session's launches, so the original's processes are still carried over, not stopped.
#[test]
fn a_hold_given_up_leaves_the_mark_on_another_sessions_launch() {
    let (state, dir) = app_state("ledger-carry-other-mark");
    let (original, child) = agent_leaving_one(&state, "s1", &dir.join("child"));
    let _guard = KillOnDrop(vec![child]);
    let ledger = &state.process_ledger;
    let receiving = agent_of(&state, "s2", Vec::new());

    let switch = ledger.hold_for_launch("s2");
    assert!(ledger.mark_carry_over(&original.launch_id, "s2"));
    drop(switch);
    ledger.end_launch(&original.launch_id);

    assert!(is_running(child), "stopped, though the handoff marked it");
    assert_eq!(
        launch_of(ledger, child).as_deref(),
        Some(receiving.launch_id.as_str())
    );
}

/// A hold given up as kept after the daemon began shutting down stops what it holds instead:
/// nothing is left for a user to find once the daemon is gone.
#[test]
fn a_hold_given_up_as_kept_while_the_daemon_shuts_down_stops_what_it_holds() {
    let (state, dir) = app_state("ledger-carry-kept-closing");
    let (first, child) = agent_leaving_one(&state, "s1", &dir.join("child"));
    let _guard = KillOnDrop(vec![child]);
    let ledger = &state.process_ledger;
    let held = ledger.hold_for_launch("s1");
    ledger.mark_carry_over(&first.launch_id, "s1");
    ledger.end_launch(&first.launch_id);
    assert!(is_running(child));

    ledger.close();
    held.keep_what_is_held("p1".to_string());
    assert!(ledger.trees_of_project(&[], "p1", &[]).is_empty());
    ledger.tick();
    assert_stops_running(child);
}

/// A daemon that restarts while what a launch left is held for the session's next launch stops
/// it at the reconcile: no launch of the previous run is running after a restart.
#[test]
fn what_is_held_when_the_daemon_restarts_is_stopped_by_the_reconcile() {
    let (state, dir) = app_state("ledger-carry-restart");
    let (first, child) = agent_leaving_one(&state, "s1", &dir.join("child"));
    let _guard = KillOnDrop(vec![child]);
    let ledger = &state.process_ledger;
    let _held = ledger.hold_for_launch("s1");
    ledger.mark_carry_over(&first.launch_id, "s1");
    ledger.end_launch(&first.launch_id);
    assert!(is_running(child));

    let restarted = ProcessLedger::new(&dir);
    quiet(&restarted);
    restarted.reconcile();
    assert_stops_running(child);
}

/// Once the daemon is shutting down nothing is carried over, and what was held already is stopped
/// by `end_held_launches`.
#[test]
fn a_daemon_shutting_down_carries_nothing_over_and_stops_what_is_held() {
    let (state, dir) = app_state("ledger-carry-close");
    let (first, held_child) = agent_leaving_one(&state, "s1", &dir.join("held"));
    let (second, child) = agent_leaving_one(&state, "s2", &dir.join("child"));
    let _guard = KillOnDrop(vec![held_child, child]);
    let ledger = &state.process_ledger;
    let _held = ledger.hold_for_launch("s1");
    let _held_too = ledger.hold_for_launch("s2");
    ledger.mark_carry_over(&first.launch_id, "s1");
    ledger.end_launch(&first.launch_id);
    assert!(is_running(held_child));

    ledger.close();
    ledger.mark_carry_over(&second.launch_id, "s2");
    ledger.end_launch(&second.launch_id);
    assert_stops_running(child);
    ledger.end_held_launches();
    assert_stops_running(held_child);
}

/// Chosen trees of a running launch move to another session's running launch, which stops them
/// when it ends, while the rest stay with the launch that started them; a tree that is not the
/// giving session's, or a session with no launch to take them, moves nothing.
#[test]
fn trees_moved_to_another_session_are_stopped_with_it() {
    let (state, dir) = app_state("ledger-move-trees");
    let (moved_file, staying_file) = (dir.join("moved"), dir.join("staying"));
    let giver = agent_of(
        &state,
        "s1",
        vec![
            Background::sleeper(&moved_file),
            Background::sleeper(&staying_file),
        ],
    );
    let (moved, staying) = (wait_for_pid(&moved_file), wait_for_pid(&staying_file));
    let taker = agent_of(&state, "s2", Vec::new());
    let _guard = KillOnDrop(vec![moved, staying]);
    let ledger = &state.process_ledger;
    ledger.scan(false);
    let (moved_key, staying_key) = (identity(moved).key(), identity(staying).key());

    assert_eq!(
        ledger.move_trees(&[moved_key.clone(), "1:1".to_string()], "s1", "s2"),
        Err(super::MoveRefused::NotFrom(vec!["1:1".to_string()]))
    );
    assert_eq!(
        ledger.move_trees(std::slice::from_ref(&moved_key), "s1", "s3"),
        Err(super::MoveRefused::NoLaunch)
    );
    assert_eq!(
        launch_of(ledger, moved).as_deref(),
        Some(giver.launch_id.as_str()),
        "a refused move moved nothing"
    );

    assert_eq!(
        ledger.move_trees(std::slice::from_ref(&moved_key), "s1", "s2"),
        Ok(vec![moved_key.clone()])
    );
    assert_eq!(
        launch_of(ledger, moved).as_deref(),
        Some(taker.launch_id.as_str())
    );
    let listed = |session: &str| -> Vec<String> {
        ledger
            .trees_of_sessions(&[session.to_string()], Duration::ZERO)
            .into_iter()
            .map(|tree| tree.id)
            .collect()
    };
    assert_eq!(listed("s1"), vec![staying_key]);
    assert_eq!(listed("s2"), vec![moved_key]);

    ledger.end_launch(&giver.launch_id);
    assert_stops_running(staying);
    assert!(
        is_running(moved),
        "stopped with the session that gave it away"
    );
    ledger.end_launch(&taker.launch_id);
    assert_stops_running(moved);
}

/// A tree with a process on its agent's own console, which ends it with the session whatever
/// keeps it, is listed as one that cannot be kept, and a keep naming it is refused as a whole,
/// with nothing kept, rather than answered as kept and then stopped with the launch.
#[test]
fn a_tree_on_its_agent_s_console_is_listed_unkeepable_and_its_keep_refused() {
    let (state, dir) = app_state("ledger-agent-console");
    let (live, child) = agent_leaving_one(&state, "s1", &dir.join("child"));
    let _guard = KillOnDrop(vec![child]);
    let ledger = &state.process_ledger;
    let key = identity(child).key();
    let sessions = std::collections::HashMap::from([("s1".to_string(), "p1".to_string())]);
    let listed = || {
        ledger
            .trees_of_sessions(&["s1".to_string()], Duration::ZERO)
            .into_iter()
            .map(|tree| (tree.id, tree.keepable, tree.unkeepable_reason))
            .collect::<Vec<_>>()
    };
    assert_eq!(listed(), vec![(key.clone(), true, None)]);

    AGENT_CONSOLE_OVERRIDE.lock().unwrap().push(identity(child));
    assert_eq!(
        listed(),
        vec![(key.clone(), false, Some(UnkeepableReason::AgentConsole))]
    );
    let refused = ledger.keep_trees(std::slice::from_ref(&key), &[], &sessions);
    AGENT_CONSOLE_OVERRIDE
        .lock()
        .unwrap()
        .retain(|shared| *shared != identity(child));
    let Err(refused) = refused else {
        panic!("a tree on its agent's console was kept: {refused:?}");
    };
    assert_eq!(refused.pid, ProcessId(child as u32));
    assert!(ledger.book().kept.is_empty());

    ledger.end_launch(&live.launch_id);
    assert_stops_running(child);
}

/// A kept tree is listed among its project's trees that no session owns, marked kept, under the
/// project of the session whose tree it was, also after a restart; it is never stopped by the
/// end of that session's launch, and a stop the user asks for stops it.
#[test]
fn a_kept_tree_stays_listed_under_its_project_until_the_user_stops_it() {
    let (state, dir) = app_state("ledger-kept-listed");
    let (live, child) = agent_leaving_one(&state, "s1", &dir.join("child"));
    let _guard = KillOnDrop(vec![child]);
    let ledger = &state.process_ledger;
    let key = identity(child).key();
    let sessions = std::collections::HashMap::from([("s1".to_string(), "p1".to_string())]);
    assert_eq!(
        ledger.keep_trees(std::slice::from_ref(&key), &[], &sessions),
        Ok(vec![key.clone()])
    );
    ledger.end_launch(&live.launch_id);
    assert!(is_running(child));

    let unowned = |ledger: &ProcessLedger, project: &str| -> Vec<(String, Option<String>, bool)> {
        ledger
            .trees_of_project(&[], project, &[])
            .into_iter()
            .map(|tree| (tree.id, tree.session_id, tree.kept))
            .collect()
    };
    assert_eq!(unowned(ledger, "p1"), vec![(key.clone(), None, true)]);
    assert!(unowned(ledger, "p2").is_empty());

    let restarted = ProcessLedger::new(&dir);
    quiet(&restarted);
    restarted.reconcile();
    assert!(is_running(child));
    assert_eq!(unowned(&restarted, "p1"), vec![(key.clone(), None, true)]);

    assert_eq!(
        restarted
            .stop_trees(std::slice::from_ref(&key), &[])
            .stopped,
        vec![key.clone()]
    );
    assert_stops_running(child);
    assert!(unowned(&restarted, "p1").is_empty());
}

/// A tree left behind in a removed directory of a project and kept is listed under that project
/// as kept, and a stop the user asks for reaches it without the project's directories.
#[test]
// Unix only: a leftover is a process working in a removed directory, which Windows never lets a
// process's directory be.
#[cfg(unix)]
fn a_kept_leftover_is_listed_under_the_project_it_was_left_in() {
    let dir = ScratchDir::new("ledger-kept-leftover");
    let ledger = ProcessLedger::new(&dir);
    quiet(&ledger);
    let project = dir.join("project");
    let removed = project.join(".worktrees").join("gone");
    std::fs::create_dir_all(&removed).unwrap();
    let left = detached_in(&removed, "/bin/sleep", &["60"], &[]);
    let _guard = KillOnDrop(vec![left]);
    let key = identity(left).key();
    std::fs::remove_dir(&removed).unwrap();

    let projects = [("p1".to_string(), project.to_path_buf())];
    assert_eq!(
        ledger.keep_trees(
            std::slice::from_ref(&key),
            &projects,
            &std::collections::HashMap::new()
        ),
        Ok(vec![key.clone()])
    );
    let listed = ledger.trees_of_project(&[], "p1", std::slice::from_ref(&project.to_path_buf()));
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].id, key);
    assert!(listed[0].kept && listed[0].cwd_missing);
    assert_eq!(listed[0].session_id, None);

    assert_eq!(
        ledger.stop_trees(std::slice::from_ref(&key), &[]).stopped,
        vec![key]
    );
    assert_stops_running(left);
}

/// What a hold gives up as kept, rather than stopped, is the user's under the project it names:
/// listed among the project's processes and left running by the scanner. A launch of the
/// session that ends while the hold is open, as a relaunch that does not come up does, leaves
/// what it holds, carried over or its own, held too.
#[test]
fn what_a_hold_gives_up_as_kept_is_listed_under_the_project() {
    let (state, dir) = app_state("ledger-carry-kept");
    let (first, child) = agent_leaving_one(&state, "s1", &dir.join("child"));
    let _guard = KillOnDrop(vec![child]);
    let ledger = &state.process_ledger;
    let held = ledger.hold_for_launch("s1");
    ledger.mark_carry_over(&first.launch_id, "s1");
    ledger.end_launch(&first.launch_id);
    let relaunch = agent_of(&state, "s1", Vec::new());
    assert_eq!(
        launch_of(ledger, child).as_deref(),
        Some(relaunch.launch_id.as_str())
    );
    ledger.end_launch(&relaunch.launch_id);
    assert!(
        is_running(child),
        "stopped with a relaunch that did not come up"
    );
    assert!(ledger.book().launches[&relaunch.launch_id].held);

    held.keep_what_is_held("p1".to_string());
    ledger.tick();
    assert!(is_running(child));
    let listed: Vec<(String, Option<String>, bool)> = ledger
        .trees_of_project(&[], "p1", &[])
        .into_iter()
        .map(|tree| (tree.id, tree.session_id, tree.kept))
        .collect();
    assert_eq!(listed, vec![(identity(child).key(), None, true)]);
}

/// What a launch's end writes down names no terminal session its stop emptied, though every
/// round of the stop found a process anew, the last in a session no read showed empty yet: the
/// next run would otherwise take whatever comes to hold that id for the launch's.
#[test]
// Unix only: a terminal session is a POSIX grouping; Windows follows a launch by its job.
#[cfg(unix)]
fn the_file_written_at_a_launch_end_names_no_session_its_stop_emptied() {
    let (state, dir) = app_state("ledger-end-sessions");
    let ledger = &state.process_ledger;
    let _first = following(&state);
    let pending = ledger.begin_launch("s1").unwrap();
    let script = format!(
        "{RESPAWN_VAR}='{}' /usr/bin/perl -MPOSIX -e 'POSIX::setsid(); \
         POSIX::sigprocmask(SIG_BLOCK, POSIX::SigSet->new(SIGTERM)); exec @ARGV' -- '{}' {} \
         >/dev/null 2>&1 &\nexec sleep 300",
        dir.display(),
        test_binary(),
        SLEEPER.join(" "),
    );
    let live = tagged_live_session("s1", AgentId::CLAUDE, &script, pending.tag());
    state.register_live(live.clone());
    let first = wait_for_pid(&dir.join("gen-0"));
    let _guard = KillOnDrop(vec![first]);
    wait_for_tag(first);
    ledger.scan(false);
    assert_eq!(
        launch_of(ledger, first).as_deref(),
        Some(live.launch_id.as_str())
    );

    ledger.end_launch(&live.launch_id);
    let generations: Vec<i32> = (1..=RESPAWN_LAST)
        .map(|generation| wait_for_pid(&dir.join(format!("gen-{generation}"))))
        .collect();
    let _later_guard = KillOnDrop(generations.clone());
    let last = generations[RESPAWN_LAST - 1];
    assert_stops_running(last);
    let persist::Loaded::Read(written) = persist::load(&dir.join(persist::LEDGER_FILE)) else {
        panic!("the ledger file was not written");
    };
    assert!(
        written.sessions.iter().all(|session| session.sid != last),
        "{written:?}"
    );
}

/// Which of the trees a session's listing named still run: what a launch holds for the next one
/// does, what a launch that is ending is stopping does not, a tree whose top exited while a member
/// ran on does, a name of something that never ran does not, and a table that cannot be read says
/// nothing.
#[test]
fn the_trees_still_running_are_told_apart_from_those_that_ended() {
    let (state, dir) = app_state("ledger-still-running");
    let (first, child) = agent_leaving_one(&state, "s1", &dir.join("child"));
    let _guard = KillOnDrop(vec![child]);
    let ledger = &state.process_ledger;
    let key = identity(child).key();
    let named = vec![key.clone()];
    let listed = ledger.trees_of_sessions(&["s1".to_string()], Duration::ZERO);
    assert_eq!(listed.len(), 1, "{listed:?}");

    assert_eq!(
        ledger.trees_still_running("s1", &named),
        Some(named.clone())
    );
    assert_eq!(
        ledger.trees_still_running("s2", &named),
        Some(Vec::new()),
        "another session's"
    );
    assert_eq!(
        ledger.trees_still_running("s1", &["1:1".to_string()]),
        Some(Vec::new()),
        "a tree that never ran"
    );

    let held = ledger.hold_for_launch("s1");
    assert!(ledger.mark_carry_over(&first.launch_id, "s1"));
    ledger.end_launch(&first.launch_id);
    assert!(ledger.book().launches[&first.launch_id].held);
    assert_eq!(
        ledger.trees_still_running("s1", &named),
        Some(named.clone())
    );

    ledger
        .book()
        .launches
        .get_mut(&first.launch_id)
        .unwrap()
        .ending = true;
    assert_eq!(
        ledger.trees_still_running("s1", &named),
        Some(Vec::new()),
        "what an ending launch is stopping"
    );
    ledger
        .book()
        .launches
        .get_mut(&first.launch_id)
        .unwrap()
        .ending = false;

    ledger
        .failing_reads
        .store(1, std::sync::atomic::Ordering::Release);
    assert_eq!(ledger.trees_still_running("s1", &named), None);
    drop(held);
}

/// A tree whose top exited while one of its members ran on is still the tree a listing named.
#[test]
fn a_tree_whose_top_exited_is_still_running_through_the_member_that_ran_on() {
    let (state, dir) = app_state("ledger-still-running-member");
    let (a_file, b_file, a_end) = (dir.join("a"), dir.join("b"), dir.join("a-end"));
    #[cfg(unix)]
    let tree = Background::perl(&format!(
        "my $b = fork(); if ($b == 0) {{ exec \"/bin/sleep\", \"300\" }} {} {} {} exit 0;",
        perl_write(&b_file, "$b"),
        perl_write(&a_file, "$$"),
        perl_wait_for(&a_end),
    ));
    #[cfg(windows)]
    let tree = Background::process(
        "a",
        Steps::new()
            .spawn("sleep", Steps::new().sleep(300))
            .write_child_pid(&b_file)
            .write_pid(&a_file)
            .wait_for(&a_end)
            .exit(),
    );
    let _live = agent_of(&state, "s1", vec![tree]);
    let (a, b) = (wait_for_pid(&a_file), wait_for_pid(&b_file));
    let _guard = KillOnDrop(vec![a, b]);
    let ledger = &state.process_ledger;
    let key = identity(a).key();
    let deadline = Instant::now() + PATIENCE;
    while !ledger
        .trees_of_sessions(&["s1".to_string()], Duration::ZERO)
        .iter()
        .any(|tree| tree.id == key && tree.process_count == 2)
    {
        assert!(Instant::now() < deadline, "the tree was never listed");
        std::thread::sleep(Duration::from_millis(50));
    }

    std::fs::write(&a_end, "").unwrap();
    assert_stops_running(a);
    assert!(is_running(b));
    assert_eq!(
        ledger.trees_still_running("s1", std::slice::from_ref(&key)),
        Some(vec![key])
    );
}
