//! Fixtures the daemon's tests share: a scratch directory that removes itself, an `AppState` over
//! a store in one, and a stand-in live session on a PTY that ends itself — each a guard, so a
//! test that fails part-way cleans up as surely as one that passes — the waits on a fixture
//! process a test spawned (the pid it recorded, running a scenario again when it was killed
//! before recording one, and its being gone), and the environment a test's `git` runs in together
//! with the repositories it builds with it. Compiled only for tests, and one module rather than a
//! `tests/` directory because some of the fixtures reach private items (`LiveSession::new`,
//! `AppState::new`).

use std::collections::HashMap;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::protocol::Agent;
use crate::session::{LiveSession, NewSession};
use crate::state::AppState;
use crate::store::Store;

/// How long a test waits for something a spawned shell does: a snapshot it should produce, a file
/// it should write, a process it should end. Not a measure of speed: on a machine whose
/// endpoint-security software stalls every `exec` for seconds at a time under a parallel burst, a
/// shell can take well over ten seconds to start, and a wait returns as soon as its condition
/// holds, so a generous value costs a passing test nothing.
pub(crate) const PATIENCE: Duration = Duration::from_secs(30);

/// A scratch directory for one test, removed when whatever holds it is dropped. Dereferences to
/// the path, so it is passed as one.
///
/// The name is `octoboardd-<label>-<pid>-<counter>`: the label attributes a leftover directory to
/// the test that made it, and the counter (not a thread id, which is reused within a process)
/// keeps two directories of one run apart. Across runs a pid can be recycled, which is why the
/// directory is also removed before being created: a run killed outright leaves its directory
/// behind, and a leftover file in it (a SQLite database, say) would be picked up by the next run
/// that reuses the name.
///
/// Bind it to a named local: a temporary guard, such as one passed straight into `Store::open`,
/// removes the directory at the end of that statement.
#[derive(Debug)]
pub(crate) struct ScratchDir(PathBuf);

impl ScratchDir {
    pub(crate) fn new(label: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "octoboardd-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("temporary directory");
        Self(dir)
    }
}

impl Deref for ScratchDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for ScratchDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// The path of one file inside a scratch directory that goes away with it. Dereferences to the
/// path, so it is passed as one; the file itself is not created.
#[derive(Debug)]
pub(crate) struct ScratchFile {
    _dir: ScratchDir,
    path: PathBuf,
}

impl ScratchFile {
    pub(crate) fn new(label: &str, file_name: &str) -> Self {
        let dir = ScratchDir::new(label);
        Self {
            path: dir.join(file_name),
            _dir: dir,
        }
    }
}

impl Deref for ScratchFile {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for ScratchFile {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

/// An `AppState` over a fresh store in a scratch directory labelled `label`, with no console in
/// it. The caller keeps the directory alive for as long as the state is used. A console added to
/// the state needs a workdir of its own rather than this directory, because deleting a console
/// removes its workdir.
pub(crate) fn app_state(label: &str) -> (Arc<AppState>, ScratchDir) {
    let dir = ScratchDir::new(label);
    let store = Store::open(&dir.join("octoboard.db")).expect("store");
    (
        Arc::new(AppState::new(
            store,
            1234,
            "/opt/octoboardd".to_string(),
            dir.join("output"),
        )),
        dir,
    )
}

/// A stand-in live session that stays alive on a PTY (`sleep 30`) with nothing reading it.
pub(crate) fn idle_stand_in(id: &str) -> StandIn {
    fake_live_session(id, Agent::Claude, 80, 24, "sleep 30")
}

/// A stand-in live session on a real PTY, with `/bin/bash` running `script` behind it instead of a
/// real agent: enough for whatever a test needs a `LiveSession` to write into, read from, or just
/// stay alive on. `LiveSession` has no `Drop` that ends its process, so this guard does: the
/// process is terminated when the guard drops, which a test that fails part-way reaches too.
/// Dereferences to the `Arc<LiveSession>`, so `live.clone()` registers it with an `AppState`.
/// Declare it after any `ScratchDir` its script uses, so the process ends before the directory
/// goes.
pub(crate) struct StandIn(Arc<LiveSession>);

impl Deref for StandIn {
    type Target = Arc<LiveSession>;

    fn deref(&self) -> &Arc<LiveSession> {
        &self.0
    }
}

impl Drop for StandIn {
    fn drop(&mut self) {
        self.0.terminate();
    }
}

pub(crate) fn fake_live_session(
    id: &str,
    agent: Agent,
    cols: u16,
    rows: u16,
    script: &str,
) -> StandIn {
    let live = watched_live_session(id, agent, cols, rows, script, None);
    // Past its trust screen, as a session that has run a hook is, so that messages are written
    // into it rather than held.
    live.trust.end_watch();
    live
}

/// [`fake_live_session`] with its trust watch still looking for the agent's screen, as a freshly
/// launched session's is, and, for a Grok one, where its trust record is carried after a press.
pub(crate) fn watched_live_session(
    id: &str,
    agent: Agent,
    cols: u16,
    rows: u16,
    script: &str,
    carried_trust: Option<crate::trust::CarriedTrust>,
) -> StandIn {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};

    let pty = native_pty_system()
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("a PTY");
    // Bash rather than `/bin/sh`: callers' scripts may use bash builtins and syntax that dash, the
    // `/bin/sh` of Debian and Ubuntu, lacks.
    let mut cmd = CommandBuilder::new("/bin/bash");
    cmd.args(["-c", script]);
    let child = pty.slave.spawn_command(cmd).expect("the stand-in");
    let pid = child.process_id().expect("a pid");
    let fd = pty.master.as_raw_fd().expect("a descriptor");
    crate::ptyio::set_nonblocking(fd).expect("non-blocking");
    StandIn(Arc::new(LiveSession::new(NewSession {
        id: id.to_string(),
        agent,
        pid,
        fd,
        master: pty.master,
        child,
        scratch_dir: None,
        resolves_approvals_itself: false,
        carried_trust,
    })))
}

/// The pid `path` holds, if the fixture shell has written one. Read once, without waiting:
/// the shell writes it within moments of starting, and the tests that read it do so after a
/// timeout of seconds.
pub(crate) fn read_pid_file(path: &std::path::Path) -> Option<i32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Runs `attempt` until it returns a value, for up to [`PATIENCE`]. An attempt returns `None`
/// when its fixture shell was killed by the timeout under test before it got as far as
/// recording the pids the test checks: when an exec stalls for longer than that timeout, as it
/// can on a machine running endpoint-security software, nothing was tested, so the scenario is
/// run again from scratch instead.
pub(crate) fn until_the_shell_ran<T>(mut attempt: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(done) = attempt() {
            return done;
        }
        assert!(
            Instant::now() < deadline,
            "the fixture shell was killed before it recorded its pids, again and again"
        );
    }
}

fn errno_is_esrch() -> bool {
    std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

/// Waits for `pid` to no longer exist, failing the test if it outlives `deadline`. Used to
/// confirm a process the fixture backgrounded was actually reaped rather than leaked, whether
/// by the production code's own group kill or by a test cleaning up after a successful
/// snapshot that (correctly) never touched it.
pub(crate) fn assert_gone_by(pid: i32, deadline: Instant) {
    loop {
        if unsafe { libc::kill(pid, 0) } == -1 && errno_is_esrch() {
            return;
        }
        if Instant::now() >= deadline {
            panic!("pid {pid} outlived the test's deadline");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The environment a test's `git` runs in: this process's own, without any `GIT_*` variable, plus
/// no global or system configuration. A variable of the developer's shell (`GIT_DIR`, say) cannot
/// redirect a command, and their own configuration, hooks or signing settings cannot decide what a
/// test observes.
pub(crate) fn isolated_git_env() -> HashMap<String, String> {
    let mut env: HashMap<String, String> = std::env::vars().collect();
    env.retain(|key, _| !key.starts_with("GIT_"));
    env.insert("GIT_CONFIG_GLOBAL".to_string(), "/dev/null".to_string());
    env.insert("GIT_CONFIG_NOSYSTEM".to_string(), "1".to_string());
    env
}

/// The `git` a test runs: the system one, already exec'd countless times, so no
/// fresh-executable cost.
pub(crate) const SYSTEM_GIT: &str = "/usr/bin/git";

/// A `git` command in `dir` for fixture setup, over exactly [`isolated_git_env`] and nothing
/// more. The caller adds the subcommand, and runs it where it needs its output, its exit status
/// or a pipe on its stdin.
///
/// It arrives with `-c` options already queued, so a caller adding one of its own adds it before
/// the subcommand: a `-c` after the subcommand is an argument to that subcommand instead.
///
/// The identity and the default branch name are passed per command because that environment
/// leaves no configuration to take them from: without them a commit fails for want of an author
/// and the first branch is whatever `git`'s own built-in default is.
pub(crate) fn fixture_git(dir: &Path) -> Command {
    let mut command = Command::new(SYSTEM_GIT);
    command
        .current_dir(dir)
        .env_clear()
        .envs(isolated_git_env())
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(["-c", "init.defaultBranch=main"]);
    command
}

/// Runs plain `git` in `dir` through [`fixture_git`], failing the test on a non-zero exit.
/// Returns what it printed.
pub(crate) fn git(dir: &Path, args: &[&str]) -> Vec<u8> {
    let args: Vec<&[u8]> = args.iter().map(|arg| arg.as_bytes()).collect();
    git_bytes(dir, &args)
}

/// Whether the `git` on this machine knows `show-ref --exists` (Git 2.43), which is what tells a
/// branch that is broken from one not born yet; before it a broken branch is taken for an unborn
/// one. `dir` must be inside a repository, since outside one `git` exits 128 and this panics.
pub(crate) fn git_has_show_ref_exists(dir: &Path) -> bool {
    let status = fixture_git(dir)
        .args(["show-ref", "--exists", "refs/heads/none"])
        .output()
        .expect("git runs")
        .status;
    // Exit 2 is "no such reference"; an older Git rejects the option itself with 129.
    match status.code() {
        Some(0 | 2) => true,
        Some(129) => false,
        other => panic!("git show-ref --exists exited {other:?}"),
    }
}

/// [`git`] with arguments that need not be UTF-8.
pub(crate) fn git_bytes(dir: &Path, args: &[&[u8]]) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    let args: Vec<&std::ffi::OsStr> = args
        .iter()
        .map(|arg| std::ffi::OsStr::from_bytes(arg))
        .collect();
    let output = fixture_git(dir).args(&args).output().expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

/// [`git`] with `input` on its stdin, for a subcommand that reads one. Its own output is left to
/// the test's, so a failure reports the arguments alone rather than `git`'s complaint.
pub(crate) fn git_stdin(dir: &Path, args: &[&str], input: &[u8]) {
    use std::io::Write as _;
    let mut child = fixture_git(dir)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    assert!(child.wait().unwrap().success(), "git {args:?}");
}

/// A repository in a scratch directory with one commit holding `files`.
pub(crate) fn repo_with(label: &str, files: &[(&[u8], &[u8])]) -> ScratchDir {
    use std::os::unix::ffi::OsStrExt;

    let dir = ScratchDir::new(label);
    git(&dir, &["init", "-q"]);
    for (name, body) in files {
        let path = dir.join(std::ffi::OsStr::from_bytes(name));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "fixture"]);
    dir
}
