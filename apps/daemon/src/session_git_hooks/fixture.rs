//! What the tests of the hook dispatchers, and of the attribution their reports feed, share: the
//! dispatchers written for a stand-in `octoboardd`, a repository with sessions whose `git` runs
//! under them, the reports and gate calls the stand-in collects, and the daemon's router served
//! for the dispatchers' real calls to reach.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use super::{injected, render, write_dir, Rendering, BUILT_IN_HOOKS};
use crate::branch_origin::{ref_created, repo_key, RefReport};
use crate::git::repo_files::repository_of;
use crate::platform::shell;
use crate::protocol::{
    AgentId, Console, GitWorkflowSettingsChanges, Project, ProjectSource, Session, SessionKind,
    SessionStatus,
};
use crate::state::AppState;
use crate::store::{BranchOrigin, LOCAL_HOST_ID};
use crate::test_support::bare_session;
use crate::test_support::{
    app_state, fixture_git, git, make_executable, repo_with, ScratchDir, ScratchFile, PATIENCE,
    SYSTEM_GIT, TEST_KEY,
};
use crate::time::now_millis;

/// The dispatchers every test here runs git with, written once per test binary under a fixed
/// name, as a fresh executable per test would pay macOS's first-exec cost each time. Their report
/// goes to a stand-in for `octoboardd git-ref` that appends what it was started with — its
/// directory (as Windows spells it where the shell is MSYS's), `GIT_DIR` and arguments — to the
/// file `OCTOBOARD_TEST_REPORTS` names. The same stand-in answers `octoboardd gate-hook`: it
/// appends its arguments and the input it was given to the file `OCTOBOARD_TEST_GATES` names and
/// exits with the status `OCTOBOARD_TEST_GATE_STATUS` holds, 0 when unset, as the gate would for a
/// rejection (3) or any other outcome. With `OCTOBOARD_TEST_GATE_BINARY` set to the test binary it
/// instead runs the real `gate-hook` in that binary (see `tests::gate::gate_hook_child`), against
/// whatever daemon the session's port names.
pub(crate) fn dispatchers() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| dispatchers_for(&system_exec_path(), &[]))
}

/// What `git --exec-path` says for the git the tests run, which is what git exports to its hooks.
pub(crate) fn system_exec_path() -> String {
    let output = Command::new(SYSTEM_GIT)
        .arg("--exec-path")
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

/// Dispatchers written as for a git with `exec_path` that reads `system_files`.
pub(crate) fn dispatchers_for(exec_path: &str, system_files: &[PathBuf]) -> PathBuf {
    let state = std::env::temp_dir().join("octoboardd-test-githooks/state");
    std::fs::create_dir_all(&state).unwrap();
    dispatchers_with(exec_path, system_files, &state)
}

/// [`dispatchers_for`] with a state directory of the caller's.
pub(crate) fn dispatchers_with(exec_path: &str, system_files: &[PathBuf], state: &Path) -> PathBuf {
    let temp = std::env::temp_dir();
    let stand_in = installed(
        "octoboardd-test-git-ref",
        "#!/bin/sh\n\
         if [ \"$1\" = gate-hook ]; then\n\
         if [ -n \"${OCTOBOARD_TEST_GATE_BINARY-}\" ]; then\n\
         OCTOBOARD_TEST_GATE_ARGS=\"$*\" exec \"$OCTOBOARD_TEST_GATE_BINARY\" --exact \
         session_git_hooks::tests::gate::gate_hook_child --ignored --nocapture\n\
         fi\n\
         printf '%s\\t%s\\n' \"$*\" \"$(tr '\\n' ' ')\" >>\"${OCTOBOARD_TEST_GATES:-/dev/null}\"\n\
         exit \"${OCTOBOARD_TEST_GATE_STATUS:-0}\"\n\
         fi\n\
         printf '%s\\t%s\\t%s\\n' \"$(pwd -W 2>/dev/null || pwd)\" \"${GIT_DIR-}\" \"$*\" \
         >>\"$OCTOBOARD_TEST_REPORTS\"\n",
        true,
    );
    let root = temp.join("octoboardd-test-githooks");
    let names: BTreeSet<String> = BUILT_IN_HOOKS.iter().map(|n| n.to_string()).collect();
    let script = render(&Rendering {
        interpreter: shell::posix_interpreter(),
        self_exe: &stand_in.to_string_lossy(),
        exec_path,
        state,
        system_files,
    });
    write_dir(&root, &names, &script).unwrap()
}

/// A file of `body` under a fixed name in the temporary directory, written by renaming one of this
/// process's own into place: a test binary of another worktree may be reading or running it.
pub(crate) fn installed(name: &str, body: &str, executable: bool) -> PathBuf {
    let temp = std::env::temp_dir();
    let path = temp.join(name);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(body) {
        let own = temp.join(format!("{name}.{}", std::process::id()));
        std::fs::write(&own, body).unwrap();
        if executable {
            make_executable(&own);
        }
        std::fs::rename(&own, &path).unwrap();
    }
    path
}

/// A session of `project` that started a minute ago and is still running.
pub(crate) fn session_record(id: &str, project: &str) -> Session {
    Session {
        console_id: "console".to_string(),
        project_id: Some(project.to_string()),
        host_id: LOCAL_HOST_ID.to_string(),
        title: format!("Session {id}"),
        status: SessionStatus::Working,
        has_conversation: true,
        started_at: now_millis() - 60_000,
        ..bare_session(id, SessionKind::Project)
    }
}

/// A project of the fixture's console at `path`.
pub(crate) fn project(id: &str, path: &Path) -> Project {
    Project {
        id: id.to_string(),
        console_id: "console".to_string(),
        host_id: LOCAL_HOST_ID.to_string(),
        name: id.to_string(),
        path: path.to_string_lossy().into_owned(),
        default_agent: None,
        source: ProjectSource::Local,
        remote_url: None,
        accounts: Default::default(),
        launch: Default::default(),
        trust_consent: false,
        pinned: false,
        tags: Vec::new(),
        description: None,
        git_workflow: Default::default(),
    }
}

pub(crate) struct Fixture {
    pub(crate) state: Arc<AppState>,
    pub(crate) repo: ScratchDir,
    pub(crate) common_dir: PathBuf,
    pub(crate) reports: ScratchFile,
    /// What the stand-in for `octoboardd gate-hook` was started with and given.
    pub(crate) gates: ScratchFile,
    /// The port the sessions' dispatchers are told the daemon is on.
    pub(crate) port: AtomicU16,
    pub(crate) dir: ScratchDir,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        let (state, dir) = app_state("branch-origin");
        let workdir = dir.join("workdir");
        std::fs::create_dir(&workdir).unwrap();
        state
            .store
            .insert_console(&Console {
                id: "console".to_string(),
                name: "Console".to_string(),
                workdir: workdir.to_string_lossy().into_owned(),
                default_agent: AgentId::CLAUDE,
                project_session_agent: None,
                accounts: Default::default(),
                launch: Default::default(),
                project_session_launch: Default::default(),
                icon: None,
                console_session_prompt: None,
                created_at: 0,
            })
            .unwrap();
        let repo = repo_with("branch-origin-repo", &[(b"a", b"a")]);
        state
            .store
            .insert_project(&project("project", &repo))
            .unwrap();
        let common_dir = repository_of(&repo).unwrap();
        let fixture = Self {
            state,
            repo,
            common_dir,
            reports: ScratchFile::new("branch-origin-reports", "reports"),
            gates: ScratchFile::new("branch-origin-gates", "gates"),
            port: AtomicU16::new(1),
            dir,
        };
        fixture.session("a");
        fixture.session("b");
        fixture
    }

    pub(crate) fn session(&self, id: &str) {
        self.state
            .store
            .insert_session(&session_record(id, "project"))
            .unwrap();
    }

    /// `git <args>` in `dir` as the agent of `session` runs it: with the variables a project
    /// session is launched with, after any `GIT_CONFIG_*` entries of the user's own in `user`.
    pub(crate) fn session_git(&self, session: &str, dir: &Path, user: &[(&str, &str)]) -> Command {
        self.session_git_with(dispatchers(), session, dir, user)
    }

    /// [`Fixture::session_git`] pointed at `hooks` rather than the usual dispatchers.
    pub(crate) fn session_git_with(
        &self,
        hooks: &Path,
        session: &str,
        dir: &Path,
        user: &[(&str, &str)],
    ) -> Command {
        let slot = user
            .iter()
            .find(|(key, _)| *key == "GIT_CONFIG_COUNT")
            .map_or(0, |(_, count)| count.parse().unwrap());
        let mut command = fixture_git(dir);
        command
            .envs(user.iter().copied())
            .envs(injected(
                slot,
                hooks,
                session,
                self.port.load(Ordering::Relaxed),
                TEST_KEY,
            ))
            .env("OCTOBOARD_TEST_REPORTS", &*self.reports)
            .env("OCTOBOARD_TEST_GATES", &*self.gates);
        command
    }

    /// Runs [`Fixture::session_git`] and fails the test on a non-zero exit.
    pub(crate) fn as_session(&self, session: &str, args: &[&str]) {
        let output = self
            .session_git(session, &self.repo, &[])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// The reports of created branches the stand-in has received once there are `count` of them.
    /// A report of moved refs alone, which every commit of a session's git makes, is left out.
    pub(crate) fn reports(&self, count: usize) -> Vec<(String, RefReport)> {
        let deadline = std::time::Instant::now() + PATIENCE;
        loop {
            let text = std::fs::read_to_string(&*self.reports).unwrap_or_default();
            let reports: Vec<(String, RefReport)> = text
                .lines()
                .map(parse_report)
                .filter(|(_, report)| !report.refs.is_empty())
                .collect();
            if reports.len() >= count {
                return reports;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{} of {count} reports arrived",
                reports.len()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Hands every report received, once there are `count`, to the daemon.
    pub(crate) fn deliver(&self, count: usize) {
        for (session, report) in self.reports(count) {
            ref_created(&self.state, &session, &report);
        }
    }

    pub(crate) fn rows(&self) -> Vec<BranchOrigin> {
        self.state
            .store
            .branch_origins(&repo_key(&self.common_dir))
            .unwrap()
    }

    pub(crate) fn responsible(&self, branch: &str) -> Option<String> {
        self.rows()
            .into_iter()
            .find(|row| row.branch == branch)
            .map(|row| row.responsible_session)
    }
}

/// One line of the stand-in's: the session it reported for and the report `octoboardd git-ref`
/// would have posted.
pub(crate) fn parse_report(line: &str) -> (String, RefReport) {
    let mut fields = line.splitn(3, '\t');
    let cwd = PathBuf::from(fields.next().unwrap());
    let git_dir = fields.next().unwrap();
    let args: Vec<&str> = fields.next().unwrap().split(' ').collect();
    let session = args[args.iter().position(|a| *a == "--session").unwrap() + 1].to_string();
    assert_eq!(
        args[args.iter().position(|a| *a == "--key").unwrap() + 1],
        TEST_KEY,
        "the git-ref invocation carries the daemon's key"
    );
    let head = args
        .iter()
        .position(|a| *a == "--head")
        .map(|at| std::ffi::OsStr::new(args[at + 1]));
    let moved = args.contains(&"--moved");
    let refs: Vec<OsString> = args[args.iter().position(|a| *a == "--").unwrap() + 1..]
        .iter()
        .map(OsString::from)
        .collect();
    let git_dir = (!git_dir.is_empty()).then(|| std::ffi::OsStr::new(git_dir));
    let report = crate::session_git_hooks::ref_mode::report_of(
        &cwd,
        git_dir,
        head,
        moved,
        &refs,
        now_millis() / 1000,
    );
    (session, report)
}

/// Every line of a git trace2 event log naming a `git rev-parse` started. Traced rather than caught
/// by a stand-in `git` on `PATH`: inside a hook git puts its own exec path first on `PATH`, so a
/// stand-in is never reached.
pub(crate) fn rev_parses(trace: &Path) -> usize {
    std::fs::read_to_string(trace)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("\"event\":\"start\"") && line.contains("\"rev-parse\""))
        .count()
}

pub(crate) fn write_hook(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    make_executable(path);
}

pub(crate) fn succeeds(output: Output) -> bool {
    output.status.success()
}

pub(crate) fn numbered(word: &str, lines: usize) -> String {
    (1..=lines)
        .map(|n| format!("{word} line {n:02}\n"))
        .collect()
}

pub(crate) fn commit_file(repo: &Path, name: &str, content: &str, message: &str) {
    std::fs::write(repo.join(name), content).unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", message]);
}

pub(crate) fn rev(repo: &Path, spec: &str) -> String {
    let out = String::from_utf8(git(repo, &["rev-parse", spec])).unwrap();
    out.trim().to_string()
}

/// The fixture's project repository, with two files committed and a bare `origin` it has pushed
/// `main` to, whose `HEAD` is the default branch. Returns it and the remote's path.
pub(crate) fn project_with_remote() -> (Fixture, PathBuf) {
    let fixture = gated_fixture();
    let repo = &fixture.repo;
    commit_file(repo, "a.txt", &numbered("alpha", 10), "base");
    commit_file(repo, "other.txt", &numbered("other", 6), "base, other");
    let origin = fixture.dir.join("origin.git");
    git(
        repo,
        &["clone", "-q", "--bare", ".", origin.to_str().unwrap()],
    );
    git(repo, &["remote", "add", "origin", origin.to_str().unwrap()]);
    git(repo, &["fetch", "-q", "origin"]);
    git(repo, &["remote", "set-head", "origin", "main"]);
    (fixture, origin)
}

pub(crate) fn set_revert_gate(fixture: &Fixture, on: bool) {
    fixture
        .state
        .store
        .change_git_workflow_settings(&GitWorkflowSettingsChanges {
            revert_gate: Some(on),
            ..Default::default()
        })
        .unwrap();
}

/// A fixture with the revert gate switched on, which it is not by default.
pub(crate) fn gated_fixture() -> Fixture {
    let fixture = Fixture::new();
    set_revert_gate(&fixture, true);
    fixture
}

/// The daemon's router served from a thread of this test process, until dropped.
pub(crate) struct Daemon {
    pub(crate) port: u16,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Daemon {
    pub(crate) fn start(state: &Arc<AppState>) -> Self {
        let state = Arc::clone(state);
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let (ready, port) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                    ready.send(listener.local_addr().unwrap().port()).unwrap();
                    axum::serve(listener, crate::server::router(state))
                        .with_graceful_shutdown(async {
                            let _ = stopped.await;
                        })
                        .await
                        .unwrap();
                });
        });
        Self {
            port: port.recv().unwrap(),
            stop: Some(stop),
            thread: Some(thread),
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
