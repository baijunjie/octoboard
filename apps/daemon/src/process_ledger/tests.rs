//! The ledger over real processes: what a stand-in agent's shell commands leave running, the
//! environment tag, the stops and the reconcile at start. The attribution rules on their own are
//! tested over made-up tables in `attribute.rs`.

use std::collections::HashSet;
use std::path::Path;
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::attribute::{Attributed, Book, ENDED};
use super::table::{self, Identity, Proc, Table};
use super::{
    boot_id, follow_ups, persist, stop, ProcessLedger, BOOT_ID_OVERRIDE, SETTLED_OVERRIDE,
    TAG_VARIABLE,
};
use crate::protocol::{Agent, Session, SessionStatus};
use crate::state::AppState;
use crate::test_support::{
    app_state, assert_stops_running, console_with_project_and_accounts, detached, detached_in,
    fake_live_session, is_running, perl_write, project_session, stand_in_launch_running,
    tagged_live_session, wait_for_pid, Children, KillOnDrop, Launched, ScratchDir, StandIn,
    PATIENCE,
};

/// Where [`sleeper`] reads how long to sleep.
const SLEEP_VAR: &str = "OCTOBOARDD_TEST_LEDGER_SLEEP";

/// Where [`sleeper`] reads the tag to run under once [`GO_VAR`]'s file exists: it runs itself
/// again with that tag, keeping its pid and start time.
const RETAG_VAR: &str = "OCTOBOARDD_TEST_LEDGER_RETAG";
const GO_VAR: &str = "OCTOBOARDD_TEST_LEDGER_GO";

/// When set, [`sleeper`] ignores `SIGTERM`, as a process that will not stop when asked does,
/// and then writes its pid to the file the variable names.
const IGNORE_TERM_VAR: &str = "OCTOBOARDD_TEST_LEDGER_IGNORE_TERM";

/// When set, [`sleeper`] writes its pid to `gen-<n>` in the directory the variable names, `n`
/// being [`RESPAWN_GEN_VAR`] (0 when unset), and on `SIGTERM`, while `n` is below
/// [`RESPAWN_LAST`], first starts itself again in a terminal session of its own as generation
/// `n + 1`, then exits: a process that each round of a stop finds anew, somewhere new.
const RESPAWN_VAR: &str = "OCTOBOARDD_TEST_LEDGER_RESPAWN";
const RESPAWN_GEN_VAR: &str = "OCTOBOARDD_TEST_LEDGER_RESPAWN_GEN";
/// The last generation [`RESPAWN_VAR`] starts: one per round of a stop, `STOP_PASSES` in all.
const RESPAWN_LAST: usize = super::STOP_PASSES - 1;

/// Set by [`sleeper`]'s `SIGTERM` handler under [`RESPAWN_VAR`].
static TERMINATED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

extern "C" fn note_terminated(_: libc::c_int) {
    TERMINATED.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Keeps the scanner thread from starting, so that the table is read only when a test says so and
/// what each read finds is the test's to decide.
fn quiet(ledger: &ProcessLedger) {
    ledger.without_scanner();
}

fn identity(pid: i32) -> Identity {
    Identity {
        pid,
        start: table::start_of(pid).expect("the process is running"),
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
        .find(|(id, _)| id.pid == pid)
        .map(|(_, attributed)| attributed.launch.clone())
}

/// A stand-in agent registered with `state` running `script` and then sleeping, so the agent
/// stays up until the test ends it.
fn agent(state: &Arc<AppState>, id: &str, script: &str) -> StandIn {
    let live = fake_live_session(
        id,
        Agent::Claude,
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
fn in_own_session(body: &str) -> String {
    format!("/usr/bin/perl -MPOSIX -e 'POSIX::setsid(); {body}'")
}

/// Perl that waits for `path` to exist.
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
const SLEEPER: [&str; 4] = [
    "--exact",
    "process_ledger::tests::sleeper",
    "--ignored",
    "--test-threads=1",
];

/// This test binary run as [`sleeper`], detached, with `tag` as its tag: a process whose
/// environment can be read on macOS, unlike a platform binary's, and that descends from no
/// agent. Answers once its environment shows the tag.
fn tagged_sleeper(tag: &str) -> i32 {
    let pid = detached(
        &test_binary(),
        &SLEEPER,
        &[(SLEEP_VAR, Some("60")), (TAG_VARIABLE, Some(tag))],
    );
    wait_for_tag(pid);
    pid
}

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
        unsafe { libc::signal(libc::SIGTERM, libc::SIG_IGN) };
        let partial = Path::new(&ready).with_extension("tmp");
        std::fs::write(&partial, std::process::id().to_string()).unwrap();
        std::fs::rename(&partial, &ready).unwrap();
    }
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
    if let Some(dir) = std::env::var_os(RESPAWN_VAR) {
        respawning(Path::new(&dir));
    }
    if let Some(seconds) = std::env::var(SLEEP_VAR).ok().and_then(|s| s.parse().ok()) {
        std::thread::sleep(Duration::from_secs(seconds));
    }
}

/// [`sleeper`] under [`RESPAWN_VAR`]: never returns.
fn respawning(dir: &Path) -> ! {
    use std::os::unix::process::CommandExt;
    let generation: usize = std::env::var(RESPAWN_GEN_VAR)
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    // SAFETY: the handler only stores to an atomic.
    unsafe {
        libc::signal(
            libc::SIGTERM,
            note_terminated as extern "C" fn(libc::c_int) as libc::sighandler_t,
        )
    };
    let file = dir.join(format!("gen-{generation}"));
    let partial = file.with_extension("tmp");
    std::fs::write(&partial, std::process::id().to_string()).unwrap();
    std::fs::rename(&partial, &file).unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    while !TERMINATED.load(std::sync::atomic::Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    if generation < RESPAWN_LAST && TERMINATED.load(std::sync::atomic::Ordering::SeqCst) {
        let mut next = Command::new(std::env::current_exe().expect("the test binary"));
        next.args(SLEEPER)
            .env(RESPAWN_GEN_VAR, (generation + 1).to_string())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        // SAFETY: `setsid` is async-signal-safe.
        unsafe {
            next.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        // Never waited for: this process exits at once, and the next is reparented.
        #[allow(clippy::zombie_processes)]
        next.spawn().expect("starting the next generation");
    }
    std::process::exit(0);
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
    state.watch_exit((*live).clone());
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
        .and_then(|table| table.get(child).map(|proc| proc.ppid))
        .is_some_and(|ppid| ppid != 1)
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
/// environment unreadable on macOS — is the launch's through the session the shell was seen in.
#[test]
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
    let deadline = Instant::now() + PATIENCE;
    while table::start_of(shell).is_some() {
        assert!(Instant::now() < deadline, "the shell never exited");
        std::thread::sleep(Duration::from_millis(20));
    }
    let table = table::Table::read().unwrap();
    assert_eq!(
        table.get(second).map(|proc| proc.ppid),
        Some(1),
        "reparented"
    );
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
fn a_tag_attributes_and_another_instances_tag_is_ignored() {
    let (state, _dir) = app_state("ledger-tag");
    let live = agent(&state, "s1", "true");
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
    let mut children = Children(vec![Command::new("/bin/sleep").arg("60").spawn().unwrap()]);
    let pid = children.0[0].id() as i32;
    let real = identity(pid);
    stop::stop(
        &[Identity {
            pid,
            start: real.start + 1,
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

/// Whatever asks, the daemon itself is never signalled.
#[test]
fn a_stop_never_signals_the_daemon() {
    let own = identity(std::process::id() as i32);
    assert!(own.is_running());
    assert!(!stop::signal(&own, 0));
}

/// A daemon starting stops what the previous run of its data directory left: a process its file
/// names that still has the same start time, and one carrying this instance's tag; and leaves a
/// process its file names under another start time, a kept one, and another instance's.
#[test]
fn reconcile_stops_what_the_previous_run_left_and_nothing_else() {
    let dir = ScratchDir::new("ledger-reconcile");
    let ledger = ProcessLedger::new(&dir);
    quiet(&ledger);
    let sleep = || detached("/bin/sleep", &["60"], &[]);
    let (left, reused, kept) = (sleep(), sleep(), sleep());
    let tagged = tagged_sleeper(&format!("{}:{}", ledger.instance, uuid::Uuid::new_v4()));
    let theirs = tagged_sleeper(&format!(
        "{}:{}",
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4()
    ));
    let _guard = KillOnDrop(vec![left, reused, kept, tagged, theirs]);
    let persisted = |id: Identity| persist::PersistedProcess {
        pid: id.pid,
        start: id.start,
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
                    start: reused_id.start + 1,
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
    assert_stops_running(tagged);
    for pid in [reused, kept, theirs] {
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
    let left = detached("/bin/sleep", &["60"], &[]);
    let _guard = KillOnDrop(vec![left]);
    let id = identity(left);
    persist::save(
        &dir.join(persist::LEDGER_FILE),
        &persist::Persisted {
            boot: Some("another-boot".to_string()),
            attributed: vec![persist::PersistedProcess {
                pid: id.pid,
                start: id.start,
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
                kept: vec![Identity { start: 1, ..id }],
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
                leader_start: Some(leader_id.start),
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
    let dir = ScratchDir::new("ledger-late-reconcile");
    let left = detached("/bin/sleep", &["60"], &[]);
    let _guard = KillOnDrop(vec![left]);
    let id = identity(left);
    persist::save(
        &dir.join(persist::LEDGER_FILE),
        &persist::Persisted {
            attributed: vec![persist::PersistedProcess {
                pid: id.pid,
                start: id.start,
                launch: uuid::Uuid::new_v4().to_string(),
                sid: 0,
            }],
            ..this_boot()
        },
    )
    .unwrap();
    let ledger = ProcessLedger::new(&dir);
    ledger.start(false);
    ledger.start(true);
    assert_stops_running(left);
}

/// A process working in a directory that was removed is stopped once that has lasted the grace,
/// and not before.
#[test]
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
    let listener = in_own_session(&format!(
        "use IO::Socket::INET; my $s = IO::Socket::INET->new(Listen => 5, LocalAddr => \"127.0.0.1:0\") or die; \
         {} {} sleep 300;",
        perl_write(&port_file, "$s->sockport"),
        perl_write(&listener_file, "$$"),
    ));
    let other = in_own_session(&format!(
        "{} exec \"/bin/sleep\", \"300\";",
        perl_write(&other_file, "$$")
    ));
    let _live = agent(&state, "s1", &format!("{listener} &\n{other} &"));
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
    assert!(listening.command.contains("perl"), "{listening:?}");
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
        vec![listening.id.clone()]
    );
    assert_eq!(
        ledger.stop_trees(std::slice::from_ref(&other_key), &[]),
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
    let tree = in_own_session(&format!(
        "my $b = fork(); if ($b == 0) {{ {} {} \
         my $c = fork(); if ($c == 0) {{ exec \"/bin/sleep\", \"300\" }} {} {} exit 0; }} {} {} exit 0;",
        perl_write(&b_file, "$$"),
        perl_wait_for(&b_fork),
        perl_write(&c_file, "$c"),
        perl_wait_for(&b_end),
        perl_write(&a_file, "$$"),
        perl_wait_for(&a_end),
    ));
    let other = in_own_session(&format!("{} sleep 300;", perl_write(&other_file, "$$")));
    let live = agent(&state, "s1", &format!("{tree} &\n{other} &"));
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
    ledger.note_listed("s1".to_string(), original);

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
        vec![a_id.key()]
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
    crate::process_session::lead_session(&mut shell);
    let shell = Children(vec![shell.spawn().unwrap()]);
    let command = wait_for_pid(&command_file);
    let _guard = KillOnDrop(vec![left, intact_pid, elsewhere_pid, command]);
    let left = identity(left);
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
    assert!(
        ledger.stop_trees(&[left.key()], &[]).is_empty(),
        "not a leftover without the project's directories"
    );
    assert_eq!(
        ledger.stop_trees(&[left.key()], std::slice::from_ref(&project)),
        vec![left.key()]
    );
    assert_stops_running(left.pid);
    for pid in [intact_pid, elsewhere_pid, command, shell.0[0].id() as i32] {
        assert!(is_running(pid), "{pid} was stopped");
    }
}

/// What a bash script runs to start this test binary as [`sleeper`], detached, writing its pid to
/// `pid_file`, carrying whatever tag the script's environment has.
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
async fn a_launch_is_followed_under_the_tag_it_reserved_before_its_agent_started() {
    let (state, dir) = console_with_project_and_accounts("ledger-hand-off");
    state
        .store
        .insert_session(&Session {
            agent_session_id: Some("conversation-s1".to_string()),
            has_conversation: true,
            ..project_session("s1", Agent::Claude, SessionStatus::Interrupted)
        })
        .unwrap();
    let pid_file = dir.join("tagged");
    let script = format!("{} &\nexec sleep 30", detached_sleeper_in_script(&pid_file));
    let started = Arc::new(std::sync::Mutex::new(Vec::<StandIn>::new()));
    let launches = Arc::new(std::sync::Mutex::new(Vec::<Launched>::new()));
    let claim = state.begin_launch("s1").unwrap();
    crate::coordinator::relaunch_session_with(
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
    agent(state, "s0", "true")
}

/// A scan between a launch's reservation and its start finds the agent's internals carrying its
/// tag and gives them to it; the start takes them back at once, before any other scan.
#[test]
fn a_launch_takes_back_its_agents_internals_a_scan_before_its_start_gave_it() {
    let (state, dir) = app_state("ledger-early-scan");
    let _first = following(&state);
    let ledger = &state.process_ledger;
    let pending = ledger.begin_launch("s1");
    let helper_file = dir.join("helper");
    let script = format!(
        "{SLEEP_VAR}=60 '{}' {} >/dev/null 2>&1 &\necho $! > '{}.tmp' && mv '{}.tmp' '{}'\nexec sleep 300",
        test_binary(),
        SLEEPER.join(" "),
        helper_file.display(),
        helper_file.display(),
        helper_file.display()
    );
    let live = tagged_live_session("s1", Agent::Claude, &script, pending.tag());
    let helper = wait_for_pid(&helper_file);
    let _guard = KillOnDrop(vec![helper]);
    wait_for_tag(helper);
    ledger.scan(false);
    assert_eq!(
        launch_of(ledger, helper).as_deref(),
        Some(pending.tag().launch_id()),
        "in the agent's session, but the agent is not known yet"
    );

    ledger.launch_started(pending.tag().launch_id(), "s1", live.pid);
    assert_eq!(launch_of(ledger, helper), None);
    ledger.scan(false);
    assert_eq!(launch_of(ledger, helper), None);
}

/// A process carrying the tag of a launch that is reserved but not started yet is that launch's,
/// and left running; once the reservation is given up without the launch starting, it is an
/// ended launch's, and stopped.
#[test]
fn a_reserved_launch_keeps_what_carries_its_tag_until_it_is_given_up() {
    let (state, _dir) = app_state("ledger-reserved");
    let _first = following(&state);
    let ledger = &state.process_ledger;
    let pending = ledger.begin_launch("s1");
    let launch = pending.tag().launch_id().to_string();
    let tagged = tagged_sleeper(&pending.tag().value());
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
    let child_script = in_own_session(&format!(
        "$SIG{{TERM}} = \"IGNORE\"; {} sleep 300;",
        perl_write(&pid_file, "$$")
    ));
    let live = agent(&state, "s1", &format!("{child_script} &"));
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
        .is_empty());
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
    let go = dir.join("go");
    // Perl and then `sleep`: platform binaries both, whose environment the system withholds.
    let script = format!("{} exec \"/bin/sleep\", \"60\";", perl_wait_for(&go));
    let pid = detached("/usr/bin/perl", &["-e", &script], &[]);
    let _guard = KillOnDrop(vec![pid]);
    let id = identity(pid);
    let reads = || ledger.scans.lock().unwrap().env_reads.get(&id).copied();

    ledger.scan(false);
    ledger.scan(false);
    assert_eq!(reads(), Some(1));
    std::fs::write(&go, "").unwrap();
    let deadline = Instant::now() + PATIENCE;
    while table::Table::read()
        .and_then(|table| table.get(pid).map(|proc| proc.comm.clone()))
        .as_deref()
        != Some("sleep")
    {
        assert!(Instant::now() < deadline, "{pid} never ran sleep");
        std::thread::sleep(Duration::from_millis(20));
    }
    ledger.scan(false);
    ledger.scan(false);
    assert_eq!(reads(), Some(2));
}

/// Removed-directory cleanup respects the same retry delay as ended-launch cleanup when a
/// process survives `SIGKILL`.
#[test]
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
    let wrapper = in_own_session(&format!(
        "my $c = fork(); if ($c == 0) {{ exec \"/bin/sleep\", \"300\" }} {} {} exit 0;",
        perl_write(&child_file, "$c"),
        perl_wait_for(&go)
    ));
    let live = agent(&state, "s1", &format!("{wrapper} &"));
    state.watch_exit((*live).clone());
    let child = wait_for_pid(&child_file);
    let mut guard = KillOnDrop(vec![child]);
    let child_id = identity(child);

    crate::coordinator::stop_process(&state, (*live).clone())
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
            pid,
            start: pid as u64,
        },
        ppid,
        sid,
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
                pid,
                start: pid as u64,
            },
            Attributed {
                launch: launch.to_string(),
                sid: 0,
            },
        );
    }
    let id = |pid: i32| Identity {
        pid,
        start: pid as u64,
    };
    let stopped: HashSet<Identity> = [id(10), id(20)].into();
    let led: HashSet<i32> = [10].into();
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
    let kept = detached("/bin/sleep", &["60"], &[]);
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
                start: named_id.start,
                launch: uuid::Uuid::new_v4().to_string(),
                sid: leader,
            }],
            sessions: vec![persist::PersistedSession {
                sid: leader,
                leader_start: Some(leader_id.start),
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
    let before = |pid: i32| Identity { pid, start: 1 };
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
        while table::start_of(sid).is_some() {
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
    assert!(!book.kept_sessions.contains_key(&kept_sid));
    assert!(!book.launches[ENDED].sessions.contains_key(&ended_sid));
}

/// What the ledger file holds: the sessions recorded for the launches, and those of kept
/// processes, which a restart reads back.
#[test]
fn the_ledger_file_holds_the_recorded_and_the_kept_sessions() {
    let (state, dir) = app_state("ledger-write-sessions");
    let (kept_file, other_file) = (dir.join("kept"), dir.join("other"));
    let script = format!(
        "{} &\n{} &",
        in_own_session(&format!("{} sleep 300;", perl_write(&kept_file, "$$"))),
        in_own_session(&format!("{} sleep 300;", perl_write(&other_file, "$$")))
    );
    let _live = agent(&state, "s1", &script);
    let (kept, other) = (wait_for_pid(&kept_file), wait_for_pid(&other_file));
    let _guard = KillOnDrop(vec![kept, other]);
    let ledger = &state.process_ledger;
    ledger.scan(false);
    let kept_id = identity(kept);
    assert_eq!(
        ledger.keep_trees(&[kept_id.key()], &[], &in_a_project()),
        vec![kept_id.key()]
    );

    let persist::Loaded::Read(written) = persist::load(&dir.join(persist::LEDGER_FILE)) else {
        panic!("the ledger file was not written");
    };
    assert!(written.kept_sessions.contains(&persist::PersistedSession {
        sid: kept,
        leader_start: Some(kept_id.start),
    }));
    assert!(written.sessions.contains(&persist::PersistedSession {
        sid: other,
        leader_start: Some(identity(other).start),
    }));
}

/// An end whose read of the table fails leaves nothing it did not stop to nobody: what was the
/// launch's is held for an ended launch, and the next tick stops it.
#[test]
fn what_an_end_could_not_stop_is_held_for_an_ended_launch() {
    let (state, dir) = app_state("ledger-end-fails");
    let pid_file = dir.join("child");
    let script = in_own_session(&format!("{} sleep 300;", perl_write(&pid_file, "$$")));
    let live = agent(&state, "s1", &format!("{script} &"));
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
    let script = in_own_session(&format!("{} sleep 300;", perl_write(&pid_file, "$$")));
    let _live = agent(&state, "s1", &format!("{script} &"));
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

/// A process found running after its `SIGKILL` is tried again only after a while, with one
/// warning, or once another launch ends; not at every tick.
#[test]
fn a_process_that_outlives_sigkill_is_tried_again_only_later() {
    let (state, dir) = app_state("ledger-stubborn");
    let first = following(&state);
    let ledger = &state.process_ledger;
    let ready = dir.join("ready");
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
    let pid = wait_for_pid(&ready);
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
            "process_ledger::tests::forget_inherited_tag_child",
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
    let command = in_own_session(&format!(
        "{} exec \"/bin/sleep\", \"300\";",
        perl_write(pid_file, "$$")
    ));
    let live = agent(state, id, &format!("{command} &"));
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
fn an_ended_launch_marked_to_be_carried_over_gives_its_processes_to_the_receiving_launch() {
    let (state, dir) = app_state("ledger-carry-over");
    let (giver, child) = agent_leaving_one(&state, "s1", &dir.join("child"));
    let taker = agent(&state, "s2", "true");
    let ledger = &state.process_ledger;
    let mut guard = KillOnDrop(vec![child]);

    assert!(ledger.hand_over(&giver.launch_id, "s2"));
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
    assert!(ledger.hand_over(&first.launch_id, "s1"));
    ledger.end_launch(&first.launch_id);
    assert!(ledger.book().launches[&first.launch_id].held);
    ledger.tick();
    assert!(is_running(child), "stopped while held");

    let next = agent(&state, "s1", "true");
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
    ledger.hand_over(&first.launch_id, "s1");
    ledger.end_launch(&first.launch_id);
    assert!(is_running(held_child));
    drop(held);
    assert_eq!(launch_of(ledger, held_child).as_deref(), Some(ENDED));
    ledger.tick();
    assert_stops_running(held_child);

    ledger.hand_over(&second.launch_id, "nobody");
    ledger.end_launch(&second.launch_id);
    assert_stops_running(unheld_child);

    let _receiving = agent(&state, "s4", "true");
    ledger.hand_over(&third.launch_id, "s4");
    ledger.withdraw_hand_over(&third.launch_id);
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
    let receiving = agent(&state, "s2", "true");

    let switch = ledger.hold_for_launch("s2");
    assert!(ledger.hand_over(&original.launch_id, "s2"));
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
    ledger.hand_over(&first.launch_id, "s1");
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
    ledger.hand_over(&first.launch_id, "s1");
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
    ledger.hand_over(&first.launch_id, "s1");
    ledger.end_launch(&first.launch_id);
    assert!(is_running(held_child));

    ledger.close();
    ledger.hand_over(&second.launch_id, "s2");
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
    let command = |file: &Path| {
        in_own_session(&format!(
            "{} exec \"/bin/sleep\", \"300\";",
            perl_write(file, "$$")
        ))
    };
    let giver = agent(
        &state,
        "s1",
        &format!("{} &\n{} &", command(&moved_file), command(&staying_file)),
    );
    let (moved, staying) = (wait_for_pid(&moved_file), wait_for_pid(&staying_file));
    let taker = agent(&state, "s2", "true");
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
        vec![key.clone()]
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
        restarted.stop_trees(std::slice::from_ref(&key), &[]),
        vec![key.clone()]
    );
    assert_stops_running(child);
    assert!(unowned(&restarted, "p1").is_empty());
}

/// A tree left behind in a removed directory of a project and kept is listed under that project
/// as kept, and a stop the user asks for reaches it without the project's directories.
#[test]
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
        vec![key.clone()]
    );
    let listed = ledger.trees_of_project(&[], "p1", std::slice::from_ref(&project.to_path_buf()));
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].id, key);
    assert!(listed[0].kept && listed[0].cwd_missing);
    assert_eq!(listed[0].session_id, None);

    assert_eq!(
        ledger.stop_trees(std::slice::from_ref(&key), &[]),
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
    ledger.hand_over(&first.launch_id, "s1");
    ledger.end_launch(&first.launch_id);
    let relaunch = agent(&state, "s1", "true");
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
fn the_file_written_at_a_launch_end_names_no_session_its_stop_emptied() {
    let (state, dir) = app_state("ledger-end-sessions");
    let ledger = &state.process_ledger;
    let _first = following(&state);
    let pending = ledger.begin_launch("s1");
    let script = format!(
        "{RESPAWN_VAR}='{}' /usr/bin/perl -MPOSIX -e 'POSIX::setsid(); exec @ARGV' -- '{}' {} \
         >/dev/null 2>&1 &\nexec sleep 300",
        dir.display(),
        test_binary(),
        SLEEPER.join(" "),
    );
    let live = tagged_live_session("s1", Agent::Claude, &script, pending.tag());
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
