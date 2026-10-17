//! Fixtures the daemon's tests share: a scratch directory that removes itself, an `AppState` over
//! a store in one, and a stand-in live session on a PTY that ends itself — each a guard, so a
//! test that fails part-way cleans up as surely as one that passes — the waits on a fixture
//! process a test spawned (the pid it recorded, running a scenario again when it was killed
//! before recording one, and its being gone), and the environment a test's `git` runs in together
//! with the repositories it builds with it, the state a read must leave untouched and the stale
//! stat that makes a read want to write. Compiled only for tests, and one module rather than a
//! `tests/` directory because some of the fixtures reach private items (`LiveSession::new`,
//! `AppState::new`).

use std::collections::HashMap;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
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
            dir.join("consoles"),
        )),
        dir,
    )
}

/// A console whose working directory is below `dir`, for a store that needs one to hold projects
/// and sessions.
pub(crate) fn console(dir: &Path) -> crate::protocol::Console {
    crate::protocol::Console {
        id: "console-1".to_string(),
        name: "Console".to_string(),
        workdir: dir.join("workdir").to_string_lossy().into_owned(),
        console_session_agent: Agent::Claude,
        default_agent: Agent::Claude,
        claude_account_id: None,
        codex_account_id: None,
        grok_account_id: None,
        claude_console_session_model: None,
        claude_console_session_effort: None,
        codex_console_session_model: None,
        codex_console_session_effort: None,
        grok_console_session_model: None,
        grok_console_session_effort: None,
        icon: None,
        console_session_prompt: None,
        created_at: 0,
    }
}

/// The local project `p1` of [`console`]'s console, at `path`, inserted into `state`'s store.
pub(crate) fn insert_project(state: &AppState, path: &Path) -> crate::protocol::Project {
    let project = serde_json::from_value(serde_json::json!({
        "id": "p1", "console_id": "console-1", "host_id": crate::store::LOCAL_HOST_ID,
        "name": "Project", "path": path, "source": "local",
        "trust_consent": false, "pinned": false, "tags": [],
    }))
    .unwrap();
    state.store.insert_project(&project).unwrap();
    project
}

/// Seeds the models `agent` offers under its default account as `(id, efforts)`, so that a test that
/// gives a model or an effort is checked against this list and not against the agent installed on
/// the machine, which running its catalogue command would make it depend on.
pub(crate) fn offer_models(state: &AppState, agent: Agent, models: &[(&str, &[&str])]) {
    let models = models
        .iter()
        .map(|(id, efforts)| crate::protocol::AgentModel {
            id: id.to_string(),
            name: id.to_string(),
            efforts: efforts.iter().map(|effort| effort.to_string()).collect(),
        })
        .collect();
    state
        .agent_models
        .get_or_load(agent, None, || crate::agent_models::Listing {
            models,
            unavailable: None,
        });
}

/// A stand-in live session that stays alive on a PTY (`sleep 30`) with nothing reading it.
pub(crate) fn idle_stand_in(id: &str) -> StandIn {
    fake_live_session(id, Agent::Claude, 80, 24, "sleep 30")
}

/// A stand-in session for `id` that is registered with `state` and has its output read, so a test
/// can wait for what was written into it with [`output_containing`].
pub(crate) fn running(state: &Arc<AppState>, id: &str) -> StandIn {
    let live = idle_stand_in(id);
    state.register_live(live.clone());
    crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
    live
}

/// Polls a stand-in session's output until it holds `needle`.
pub(crate) fn output_containing(live: &LiveSession, needle: &str) -> String {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let output = String::from_utf8_lossy(&live.recent_output(8 * 1024)).into_owned();
        if output.contains(needle) || Instant::now() >= deadline {
            return output;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A stand-in live session on a real PTY, with `/bin/bash` running `script` behind it instead of a
/// real agent: enough for whatever a test needs a `LiveSession` to write into, read from, or just
/// stay alive on. `LiveSession` has no `Drop` that ends its process, so this guard does: the
/// process is terminated when the guard drops, which a test that fails part-way reaches too.
/// Dereferences to the `Arc<LiveSession>`, so `live.clone()` registers it with an `AppState`.
/// Declare it after any `ScratchDir` its script uses, so the process ends before the directory
/// goes.
pub(crate) struct StandIn(Arc<LiveSession>);

impl StandIn {
    pub(crate) fn with_auto_review(mut self) -> Self {
        Arc::get_mut(&mut self.0)
            .expect("configure before registering the stand-in")
            .resolves_approvals_itself = true;
        self
    }
}

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

/// The one executable every fake agent CLI is a symlink to, written once per test binary. A
/// script of its own per fake would pay macOS's first-`exec` code-signature check each time,
/// serialised across processes; this one is exec'd once and then reused. It sources the
/// companion file next to the symlink (`$0.body`), which is where each fake's behaviour lives.
/// Installed by writing a file of the process's own and renaming it into place, since a test
/// binary of another worktree may be executing it.
fn fake_agent_dispatcher() -> &'static Path {
    static DISPATCHER: OnceLock<PathBuf> = OnceLock::new();
    DISPATCHER.get_or_init(|| {
        let path = std::env::temp_dir().join("octoboardd-fake-agent-dispatcher");
        let staging = path.with_extension(format!("{}", std::process::id()));
        std::fs::write(&staging, "#!/bin/sh\n. \"$0.body\"\n")
            .expect("write fake agent dispatcher");
        let mut perms = std::fs::metadata(&staging).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o700);
        std::fs::set_permissions(&staging, perms).expect("chmod fake agent dispatcher");
        std::fs::rename(&staging, &path).expect("install fake agent dispatcher");
        path
    })
}

/// Makes `dir/<binary>` a fake agent CLI that runs `body` as shell script (sourced, so `exit`
/// ends it and `$@` are its arguments), and returns an environment whose `PATH` is `dir` and the
/// system directories and whose `HOME` is `dir`.
pub(crate) fn fake_agent(dir: &Path, binary: &str, body: &str) -> HashMap<String, String> {
    let path = dir.join(binary);
    std::fs::write(dir.join(format!("{binary}.body")), body).expect("write fake agent body");
    std::fs::remove_file(&path).ok();
    std::os::unix::fs::symlink(fake_agent_dispatcher(), &path).expect("symlink fake agent");
    HashMap::from([
        (
            "PATH".to_string(),
            format!("{}:/usr/bin:/bin", dir.display()),
        ),
        ("HOME".to_string(), dir.to_string_lossy().into_owned()),
    ])
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
    git_stdin_env(dir, args, &[], input);
}

/// [`git_stdin`] with `env` set for that one `git`, for a fixture that needs a variable the
/// isolated environment leaves out: a committer date, which is the time a reflog entry `git`
/// writes carries.
pub(crate) fn git_stdin_env(dir: &Path, args: &[&str], env: &[(&str, &str)], input: &[u8]) {
    use std::io::Write as _;
    let mut child = fixture_git(dir)
        .args(args)
        .envs(env.iter().copied())
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

/// Makes `branch` in the repository at `repo` and records `session` as its responsible
/// session, as the hook's report would have; the branch is the one the record was made for, which
/// is what a handover asks.
pub(crate) fn branch_made_by(state: &AppState, repo: &Path, branch: &str, session: &str) {
    git(repo, &["branch", branch]);
    let reflog = std::fs::read_to_string(repo.join(".git/logs/refs/heads").join(branch)).unwrap();
    let head = reflog.lines().next().unwrap().split('\t').next().unwrap();
    let mut fields = head.split(' ');
    let commit = fields.nth(1).unwrap().to_string();
    let time: i64 = head.rsplit(' ').nth(1).unwrap().parse().unwrap();
    let key = crate::branch_origin::repo_key(&crate::branch_origin::repository_of(repo).unwrap());
    state
        .store
        .apply_branch_origins(
            &key,
            &[],
            &[crate::store::BranchOrigin {
                repository: key.clone(),
                branch: branch.to_string(),
                created_at: time * 1000,
                birth_commit: commit,
                birth_time: time,
                responsible_session: session.to_string(),
                responsible_since: time * 1000,
                responsible_title: format!("Work {session}"),
            }],
            100,
        )
        .unwrap();
}

/// Two repositories whose `a.txt` differs, `(ours, theirs)`: a command that reads `a.txt` says
/// which of them it ran against.
pub(crate) fn repo_pair(label: &str) -> (ScratchDir, ScratchDir) {
    (
        repo_with(&format!("{label}-ours"), &[(b"a.txt", b"ours\n")]),
        repo_with(&format!("{label}-theirs"), &[(b"a.txt", b"theirs\n")]),
    )
}

/// The variables a login shell could export that send a `git` to `theirs` instead of the
/// repository its working directory names: the repository, index and common directory, and a
/// `remote.origin.url` passed through the environment by both of its channels. The tests that run
/// a command over these (`git_status`, `git_remote`, `browse`) prove the filter is wired into each
/// path, each by what its own read can see — a file's content sees the first three, a remote's
/// URL the last two, and `GIT_WORK_TREE` none. Which names stay kept is pinned by the unit test
/// in `git_env`, which takes its dropped names from here.
pub(crate) fn redirecting_overrides(theirs: &Path) -> Vec<(&'static str, String)> {
    let git_dir = theirs.join(".git");
    vec![
        ("GIT_DIR", git_dir.to_string_lossy().into_owned()),
        ("GIT_WORK_TREE", theirs.to_string_lossy().into_owned()),
        (
            "GIT_INDEX_FILE",
            git_dir.join("index").to_string_lossy().into_owned(),
        ),
        ("GIT_COMMON_DIR", git_dir.to_string_lossy().into_owned()),
        (
            "GIT_CONFIG_PARAMETERS",
            "'remote.origin.url'='https://example.com/other.git'".to_string(),
        ),
        ("GIT_CONFIG_COUNT", "1".to_string()),
        ("GIT_CONFIG_KEY_0", "remote.origin.url".to_string()),
        (
            "GIT_CONFIG_VALUE_0",
            "https://example.com/other.git".to_string(),
        ),
    ]
}

/// Every file under `.git`, by path, with its length, a digest of its bytes and its modification
/// time: what a read that must leave the repository alone is checked against. The modification
/// time is part of it because an index rewritten with the same stat information it already held
/// would compare equal by content alone; the digest stands in for the bytes so that a failure
/// prints an entry per file rather than tens of kilobytes of byte literals.
pub(crate) fn git_dir_state(repo: &Path) -> Vec<(PathBuf, u64, u64, std::time::SystemTime)> {
    use std::hash::{Hash, Hasher};

    let mut state = Vec::new();
    let mut stack = vec![repo.join(".git")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("the .git directory is readable") {
            let path = entry.expect("its entries are readable").path();
            let meta = std::fs::symlink_metadata(&path).expect("an entry has metadata");
            if meta.is_dir() {
                stack.push(path);
                continue;
            }
            let body = std::fs::read(&path).expect("an entry is readable");
            let mut digest = std::hash::DefaultHasher::new();
            body.hash(&mut digest);
            let modified = meta.modified().expect("an entry has a modification time");
            state.push((path, body.len() as u64, digest.finish(), modified));
        }
    }
    state.sort_by(|a, b| a.0.cmp(&b.0));
    state
}

/// Moves `path`'s modification time a minute back, leaving its content alone: a tracked file
/// whose stat information is stale, which is what makes a `git` read want to refresh the index.
///
/// Call it after every `git` the fixture runs. `commit` and `checkout` refresh the whole index,
/// so a file backdated before them has the moved time recorded as the index entry's own stat
/// data and the fixture is not stale at all.
pub(crate) fn move_modification_time_back(path: &Path) {
    let file = std::fs::File::options()
        .write(true)
        .open(path)
        .expect("the tracked file is writable");
    file.set_modified(std::time::SystemTime::now() - Duration::from_secs(60))
        .expect("its modification time is settable");
}
