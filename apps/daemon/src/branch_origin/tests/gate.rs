//! The revert gate in the hook dispatchers: the target branch and fork point recorded for a
//! created branch, what the daemon answers a gate-hook, and the dispatchers' handling of a branch
//! that moves or is pushed.
//!
//! Most of the dispatcher tests run the real `gate-hook` against a daemon served from the test
//! process, so that a rejection is the engine's own: the stand-in the dispatchers are written with
//! starts this test binary to run [`gate_hook_child`] where `OCTOBOARD_TEST_GATE_BINARY` is set.
//! The rest use the stand-in's plain mode to see exactly what a dispatcher starts the gate with
//! and how it takes the gate's status.

use std::net::TcpListener;
use std::os::unix::ffi::OsStrExt;

use super::*;
use crate::branch_origin::revert_gate_inputs::{inputs, GateRequest};
use crate::test_support::fixture_git;
use crate::worktree_settings::WorktreeSettingsChanges;

// -- the daemon the gate-hook asks ----------------------------------------------

/// The daemon's router served from a thread of this test process, until dropped.
struct Daemon {
    port: u16,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Daemon {
    fn start(state: &Arc<AppState>) -> Self {
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

/// A port nothing listens on.
fn closed_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// What the stand-in starts for `octoboardd gate-hook` where it is told to run the real one: this
/// test binary, running only this test, with the arguments in the environment, whose status is the
/// hook's.
#[test]
#[ignore = "started by the dispatcher tests as the gate-hook process"]
fn gate_hook_child() {
    use clap::Parser;
    let args = std::env::var("OCTOBOARD_TEST_GATE_ARGS").unwrap();
    let cli =
        crate::Cli::try_parse_from(["octoboardd"].into_iter().chain(args.split(' '))).unwrap();
    let Some(crate::Command::GateHook {
        session,
        port,
        hook,
    }) = cli.command
    else {
        panic!("not the gate-hook command");
    };
    std::process::exit(crate::gate_hook_mode::run(&session, port, &hook));
}

// -- a repository with a remote ----------------------------------------------

fn numbered(word: &str, lines: usize) -> String {
    (1..=lines)
        .map(|n| format!("{word} line {n:02}\n"))
        .collect()
}

fn commit_file(repo: &Path, name: &str, content: &str, message: &str) {
    std::fs::write(repo.join(name), content).unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", message]);
}

fn rev(repo: &Path, spec: &str) -> String {
    let out = String::from_utf8(git(repo, &["rev-parse", spec])).unwrap();
    out.trim().to_string()
}

/// The fixture's project repository, with two files committed and a bare `origin` it has pushed
/// `main` to, whose `HEAD` is the default branch. Returns it and the remote's path.
fn project_with_remote() -> (Fixture, PathBuf) {
    let fixture = Fixture::new();
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

/// `git <args>` run as session `a` in `dir`, whose dispatchers start the real gate-hook.
fn real_gate(fixture: &Fixture, dir: &Path, args: &[&str]) -> Output {
    fixture
        .session_git("a", dir, &[])
        .env(
            "OCTOBOARD_TEST_GATE_BINARY",
            std::env::current_exe().unwrap(),
        )
        .args(args)
        .output()
        .unwrap()
}

fn ok(fixture: &Fixture, args: &[&str]) {
    let output = real_gate(fixture, &fixture.repo, args);
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn set_revert_gate(fixture: &Fixture, on: bool) {
    fixture
        .state
        .store
        .change_worktree_settings(&WorktreeSettingsChanges {
            revert_gate: Some(on),
            ..Default::default()
        })
        .unwrap();
}

// -- the target branch ------------------------------------

/// The reflog says what a branch was created from, and a `HEAD` start point is the branch the
/// creating git was on while the transaction was prepared.
#[test]
fn a_created_branch_records_its_target_branch_and_birth_commit() {
    let (fixture, _) = project_with_remote();
    fixture.as_session("a", &["branch", "other", "main"]);
    fixture.as_session("a", &["branch", "from-other", "other"]);
    fixture.as_session("a", &["switch", "-q", "-c", "from-head"]);
    fixture.as_session("a", &["branch", "from-remote", "origin/main"]);
    fixture.as_session("a", &["branch", "from-commit", "HEAD~1"]);
    let worktree = fixture.dir.join("linked");
    fixture.as_session(
        "a",
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "linked",
            worktree.to_str().unwrap(),
        ],
    );
    fixture.deliver(6);

    let main = rev(&fixture.repo, "main");
    let row = |branch: &str| {
        let row = fixture.rows().into_iter().find(|row| row.branch == branch);
        let row = row.unwrap_or_else(|| panic!("no row for {branch}"));
        (row.target_branch, Some(row.birth_commit))
    };
    let some = |branch: &str| Some(branch.to_string());
    assert_eq!(row("other"), (some("main"), Some(main.clone())));
    assert_eq!(row("from-other"), (some("other"), Some(main.clone())));
    assert_eq!(row("from-head"), (some("main"), Some(main.clone())));
    assert_eq!(row("from-remote"), (some("main"), Some(main.clone())));
    assert_eq!(row("linked").0, some("from-head"), "the branch HEAD was on");
    assert_eq!(row("from-commit").0, None, "`HEAD~1` is no branch");
    assert_eq!(row("from-commit").1, Some(rev(&fixture.repo, "HEAD~1")));
}

#[test]
fn a_report_from_an_older_dispatcher_still_parses() {
    let report: RefReport = serde_json::from_str(
        r#"{"cwd":"/r","git_dir":null,"refs":[{"name":"refs/heads/a","oid":"1"}],"unix_time":5}"#,
    )
    .unwrap();
    assert_eq!(report.head, None);
}

#[test]
fn a_rename_keeps_the_target_branch() {
    let (fixture, _) = project_with_remote();
    fixture.as_session("a", &["branch", "old", "main"]);
    fixture.deliver(1);
    git(&fixture.repo, &["branch", "-m", "old", "new"]);

    let rows = fixture.reconcile_full();

    let row = rows.iter().find(|row| row.branch == "new").unwrap();
    assert_eq!(row.target_branch.as_deref(), Some("main"));
}

#[test]
fn a_handover_leaves_the_target_branch_alone() {
    let (fixture, _) = project_with_remote();
    fixture.as_session("a", &["branch", "topic", "main"]);
    fixture.deliver(1);
    let before = fixture.rows();

    hand_over(
        &fixture.state,
        &fixture.common_dir,
        &["topic".to_string()],
        "a",
        "b",
        "B",
    )
    .unwrap();

    let after = fixture.rows();
    assert_eq!(after[0].responsible_session, "b");
    assert_eq!(after[0].target_branch, before[0].target_branch);
}

// -- what the daemon answers ---------------------------------------------------

fn request(dir: &Path) -> GateRequest {
    GateRequest {
        cwd: wire_path::encode(dir.as_os_str().as_bytes()),
        git_dir: None,
    }
}

#[test]
fn the_daemon_answers_with_the_default_branch_and_the_recorded_branches() {
    let (fixture, _) = project_with_remote();
    fixture.as_session("a", &["branch", "topic", "main"]);
    fixture.as_session("a", &["branch", "loose", "HEAD~1"]);
    fixture.deliver(2);

    let answer = inputs(&fixture.state, "a", &request(&fixture.repo)).expect("an answer");

    assert_eq!(answer.guard, ["main"]);
    assert_eq!(answer.targets, [("topic".to_string(), "main".to_string())]);
    let forks: Vec<&str> = answer.fork_points.iter().map(|(b, _)| b.as_str()).collect();
    assert_eq!(forks, ["loose", "topic"]);
    assert_eq!(answer.fork_points[1].1, rev(&fixture.repo, "main"));
}

#[test]
fn the_default_branch_of_a_remote_that_is_not_origin_is_read_from_its_head_file() {
    let (fixture, _) = project_with_remote();
    git(&fixture.repo, &["remote", "rename", "origin", "upstream"]);
    let ask = || {
        inputs(&fixture.state, "a", &request(&fixture.repo))
            .unwrap()
            .guard
    };

    assert_eq!(ask(), ["main"]);

    git(
        &fixture.repo,
        &["update-ref", "-d", "refs/remotes/upstream/main"],
    );
    assert!(
        ask().is_empty(),
        "a head naming a branch the repository lacks"
    );
}

/// A reftable repository has no `HEAD` files to read, so the default branch is asked of `git`.
/// Skipped where `git` does not know the format.
#[test]
fn the_default_branch_of_a_reftable_repository_is_asked_of_git() {
    let fixture = Fixture::new();
    let repo = ScratchDir::new("branch-origin-gate-reftable");
    let init = fixture_git(&repo)
        .args(["init", "-q", "--ref-format=reftable"])
        .output()
        .unwrap();
    if !init.status.success() {
        return;
    }
    commit_file(&repo, "a.txt", &numbered("alpha", 3), "base");
    git(&repo, &["config", "remote.origin.url", "/nowhere"]);
    git(
        &repo,
        &[
            "config",
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/origin/*",
        ],
    );
    git(&repo, &["update-ref", "refs/remotes/origin/trunk", "HEAD"]);
    git(
        &repo,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/trunk",
        ],
    );
    fixture
        .state
        .store
        .insert_project(&project("reftable", &repo))
        .unwrap();
    fixture
        .state
        .store
        .insert_session(&session_record("r", "reftable"))
        .unwrap();

    let answer = inputs(&fixture.state, "r", &request(&repo)).expect("an answer");

    assert_eq!(answer.guard, ["trunk"]);
}

#[test]
fn a_branch_deleted_and_made_again_outside_the_session_contributes_nothing() {
    let (fixture, _) = project_with_remote();
    fixture.as_session("a", &["branch", "topic", "main"]);
    fixture.as_session("a", &["branch", "kept", "main"]);
    fixture.deliver(2);
    git(&fixture.repo, &["branch", "-D", "topic"]);
    commit_file(&fixture.repo, "a.txt", &numbered("alpha", 11), "moves on");
    git(&fixture.repo, &["branch", "topic", "main"]);

    let answer = inputs(&fixture.state, "a", &request(&fixture.repo)).expect("an answer");

    assert_eq!(answer.targets, [("kept".to_string(), "main".to_string())]);
    assert_eq!(answer.fork_points.len(), 1);
}

#[test]
fn a_repository_with_no_remote_has_no_resident_guarded_branch() {
    let fixture = Fixture::new();

    let answer = inputs(&fixture.state, "a", &request(&fixture.repo)).expect("an answer");

    assert!(answer.guard.is_empty());
}

#[test]
fn the_daemon_says_to_skip_for_a_session_that_is_no_projects_or_a_repository_that_is_not() {
    let (fixture, _) = project_with_remote();
    assert!(inputs(&fixture.state, "a", &request(&fixture.repo)).is_some());

    assert_eq!(
        inputs(&fixture.state, "nobody", &request(&fixture.repo)),
        None
    );

    let mut console = session_record("console-session", "project");
    console.project_id = None;
    console.role = Role::Console;
    fixture.state.store.insert_session(&console).unwrap();
    assert_eq!(
        inputs(&fixture.state, "console-session", &request(&fixture.repo)),
        None
    );

    let clone = ScratchDir::new("branch-origin-gate-clone");
    git(
        &fixture.repo,
        &[
            "clone",
            "-q",
            &fixture.repo.to_string_lossy(),
            &clone.to_string_lossy(),
        ],
    );
    assert_eq!(inputs(&fixture.state, "a", &request(&clone)), None);
    assert_eq!(
        inputs(&fixture.state, "a", &request(Path::new("/nonexistent"))),
        None
    );
}

#[test]
fn the_switch_is_read_at_every_request_and_a_projects_own_value_wins() {
    let (fixture, _) = project_with_remote();
    let ask = || inputs(&fixture.state, "a", &request(&fixture.repo));
    assert!(ask().is_some());

    set_revert_gate(&fixture, false);
    assert_eq!(ask(), None, "the application-wide switch is off");

    let mut project = fixture.state.store.get_project("project").unwrap().unwrap();
    project.worktrees.revert_gate = Some(true);
    fixture.state.store.update_project(&project).unwrap();
    assert!(ask().is_some(), "the project holds its own value");

    project.worktrees.revert_gate = Some(false);
    fixture.state.store.update_project(&project).unwrap();
    set_revert_gate(&fixture, true);
    assert_eq!(ask(), None, "and so does it when it is off");
}

#[test]
fn the_hook_side_takes_only_an_answer_as_a_reason_to_run_the_gate() {
    let (fixture, _) = project_with_remote();
    let daemon = Daemon::start(&fixture.state);
    let repo = fixture.repo.to_path_buf();
    let ask =
        |port: u16, session: &str| crate::gate_hook_mode::inputs_for(port, session, &repo, None);

    let answer = ask(daemon.port, "a").expect("an answer");
    assert_eq!(answer.guard, ["main"]);
    assert_eq!(ask(daemon.port, "nobody"), None);
    set_revert_gate(&fixture, false);
    assert_eq!(ask(daemon.port, "a"), None);
    assert_eq!(ask(closed_port(), "a"), None, "no daemon is no answer");
}

// -- the dispatchers, against the stand-in ----------------------------------

/// What the stand-in was started with and given, one line per call: its arguments, a tab, and the
/// input with its newlines turned to spaces.
fn gate_calls(fixture: &Fixture) -> Vec<String> {
    std::fs::read_to_string(&*fixture.gates)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

fn with_gate_status<'a>(command: &'a mut Command, status: &str) -> &'a mut Command {
    command.env("OCTOBOARD_TEST_GATE_STATUS", status)
}

#[test]
fn an_existing_branch_moving_is_put_to_the_gate_once_and_a_creation_is_not() {
    let fixture = Fixture::new();
    fixture.as_session("a", &["commit", "-q", "--allow-empty", "-m", "next"]);

    let calls = gate_calls(&fixture);
    assert_eq!(calls.len(), 1, "prepared only: {calls:?}");
    let (args, input) = calls[0].split_once('\t').unwrap();
    assert_eq!(
        args,
        "gate-hook --session a --port 1 reference-transaction prepared"
    );
    assert!(input.ends_with(" refs/heads/main "), "{input}");

    fixture.as_session("a", &["branch", "topic"]);
    fixture.as_session("a", &["tag", "t"]);
    fixture.as_session("a", &["branch", "-q", "-D", "topic"]);
    assert_eq!(gate_calls(&fixture).len(), 1, "no existing branch moved");
}

#[test]
fn an_existing_branch_forced_without_its_old_value_is_put_to_the_gate() {
    let fixture = Fixture::new();
    let first = rev(&fixture.repo, "HEAD");
    fixture.as_session("a", &["commit", "-q", "--allow-empty", "-m", "next"]);
    fixture.as_session("a", &["branch", "topic", &first]);
    let before = gate_calls(&fixture).len();

    fixture.as_session("a", &["update-ref", "refs/heads/main", &first]);
    fixture.as_session("a", &["branch", "-f", "topic", "main"]);
    let calls = gate_calls(&fixture);
    assert_eq!(calls.len(), before + 2, "{calls:?}");
    assert!(calls[before].ends_with(" refs/heads/main "), "{calls:?}");
    assert!(
        calls[before + 1].ends_with(" refs/heads/topic "),
        "{calls:?}"
    );
}

#[test]
fn a_gate_that_rejects_fails_the_transaction_and_anything_else_lets_it_through() {
    let fixture = Fixture::new();
    let head = rev(&fixture.repo, "HEAD");
    let commit = |status: &str| {
        let mut command = fixture.session_git("a", &fixture.repo, &[]);
        with_gate_status(&mut command, status)
            .args(["commit", "-q", "--allow-empty", "-m", status])
            .output()
            .unwrap()
    };

    assert!(!succeeds(commit("3")));
    assert_eq!(rev(&fixture.repo, "HEAD"), head, "the branch did not move");
    for status in ["1", "2", "127", "0"] {
        assert!(succeeds(commit(status)), "{status}");
    }
}

#[test]
fn the_users_hook_has_the_first_word_and_the_gate_is_not_asked_when_it_fails() {
    let fixture = Fixture::new();
    write_hook(
        &fixture.common_dir.join("hooks/reference-transaction"),
        "cat >/dev/null\n[ \"$1\" = prepared ] && exit 1\nexit 0",
    );

    let output = fixture
        .session_git("a", &fixture.repo, &[])
        .args(["commit", "-q", "--allow-empty", "-m", "next"])
        .output()
        .unwrap();

    assert!(!succeeds(output));
    assert!(gate_calls(&fixture).is_empty());
}

#[test]
fn a_long_transaction_that_moves_branches_is_gated_whole() {
    let fixture = Fixture::new();
    let first = rev(&fixture.repo, "HEAD");
    fixture.as_session("a", &["commit", "-q", "--allow-empty", "-m", "next"]);
    let second = rev(&fixture.repo, "HEAD");
    let update = |verb: &str, old: &str, new: &str, status: &str| {
        let lines: String = (0..300)
            .map(|n| match verb {
                "create" => format!("create refs/heads/bulk-{n:03} {new}\n"),
                _ => format!("update refs/heads/bulk-{n:03} {new} {old}\n"),
            })
            .collect();
        let mut command = fixture.session_git("a", &fixture.repo, &[]);
        let mut child = with_gate_status(&mut command, status)
            .args(["update-ref", "--stdin"])
            .stdin(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        std::io::Write::write_all(&mut child.stdin.take().unwrap(), lines.as_bytes()).unwrap();
        child.wait().unwrap().success()
    };
    assert!(update("create", "", &first, "0"));
    let before = gate_calls(&fixture).len();

    assert!(!update("update", &first, &second, "3"));
    assert!(update("update", &first, &second, "0"));

    let calls = gate_calls(&fixture);
    assert_eq!(calls.len(), before + 2);
    assert_eq!(calls[before].matches(" refs/heads/bulk-").count(), 300);
}

#[test]
fn a_session_without_the_variables_is_not_gated() {
    let fixture = Fixture::new();
    write_hook(
        &fixture.common_dir.join("hooks/reference-transaction"),
        "cat >/dev/null",
    );

    let output = fixture
        .session_git("a", &fixture.repo, &[])
        .env_remove("OCTOBOARD_SESSION_ID")
        .args(["commit", "-q", "--allow-empty", "-m", "next"])
        .output()
        .unwrap();

    assert!(succeeds(output));
    assert!(gate_calls(&fixture).is_empty());
}

#[test]
fn a_gate_that_cannot_start_lets_the_update_through() {
    let fixture = Fixture::new();
    let root = fixture.dir.join("hooks-without-binary");
    let names: BTreeSet<String> = BUILT_IN_HOOKS.iter().map(|n| n.to_string()).collect();
    let state = fixture.dir.join("state");
    std::fs::create_dir(&state).unwrap();
    let script = render(&Rendering {
        interpreter: interpreter(),
        self_exe: "/nonexistent/octoboardd",
        exec_path: &system_exec_path(),
        state: &state,
        system_files: &[],
    });
    let hooks = write_dir(&root, &names, &script).unwrap();

    let output = fixture
        .session_git_with(&hooks, "a", &fixture.repo, &[])
        .args(["commit", "-q", "--allow-empty", "-m", "next"])
        .output()
        .unwrap();

    assert!(succeeds(output));
}

/// A bare remote and a push of `main` to it, as session `a`.
fn push_setup(fixture: &Fixture) -> PathBuf {
    let remote = fixture.dir.join("remote.git");
    git(
        &fixture.repo,
        &["init", "-q", "--bare", remote.to_str().unwrap()],
    );
    git(
        &fixture.repo,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    remote
}

#[test]
fn a_push_of_a_branch_is_put_to_the_gate_after_the_users_hook() {
    let fixture = Fixture::new();
    push_setup(&fixture);
    fixture.as_session("a", &["push", "-q", "origin", "main"]);

    let calls = gate_calls(&fixture);
    assert_eq!(calls.len(), 1, "{calls:?}");
    let (args, input) = calls[0].split_once('\t').unwrap();
    assert_eq!(args, "gate-hook --session a --port 1 pre-push");
    assert!(input.starts_with("refs/heads/main "), "{input}");
    assert!(input.contains(" refs/heads/main "), "{input}");

    fixture.as_session("a", &["push", "-q", "origin", "main:topic"]);
    assert_eq!(gate_calls(&fixture).len(), 2);
    fixture.as_session("a", &["tag", "t"]);
    fixture.as_session("a", &["push", "-q", "origin", "t"]);
    fixture.as_session("a", &["push", "-q", "origin", ":topic"]);
    assert_eq!(
        gate_calls(&fixture).len(),
        2,
        "a tag and a deletion are not gated"
    );

    write_hook(
        &fixture.common_dir.join("hooks/pre-push"),
        "cat >/dev/null\nexit 1",
    );
    let output = fixture
        .session_git("a", &fixture.repo, &[])
        .args(["push", "-q", "origin", "main"])
        .output()
        .unwrap();
    assert!(!succeeds(output));
    assert_eq!(
        gate_calls(&fixture).len(),
        2,
        "the users hook refused first"
    );
}

/// A push of hundreds of refs is spooled rather than read line by line; the user's hook and the
/// gate both get every line.
#[test]
fn a_long_push_is_handed_to_the_users_hook_and_the_gate_whole() {
    let fixture = Fixture::new();
    push_setup(&fixture);
    let seen = fixture.dir.join("seen");
    write_hook(
        &fixture.common_dir.join("hooks/pre-push"),
        &format!("cat >>'{}'\nexit 0", seen.display()),
    );
    let head = rev(&fixture.repo, "HEAD");
    let updates: String = (0..300)
        .map(|n| format!("create refs/heads/bulk-{n:03} {head}\n"))
        .collect();
    let mut child = fixture_git(&fixture.repo)
        .args(["update-ref", "--stdin"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::io::Write::write_all(&mut child.stdin.take().unwrap(), updates.as_bytes()).unwrap();
    assert!(child.wait().unwrap().success());

    fixture.as_session(
        "a",
        &[
            "push",
            "-q",
            "origin",
            "refs/heads/bulk-*:refs/heads/bulk-*",
        ],
    );

    let seen = std::fs::read_to_string(&seen).unwrap();
    assert_eq!(seen.lines().count(), 300);
    let calls = gate_calls(&fixture);
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0].matches("refs/heads/bulk-").count(), 600);
}

#[test]
fn a_gate_that_rejects_refuses_the_push() {
    let fixture = Fixture::new();
    let remote = push_setup(&fixture);

    let mut command = fixture.session_git("a", &fixture.repo, &[]);
    let output = with_gate_status(&mut command, "3")
        .args(["push", "-q", "origin", "main"])
        .output()
        .unwrap();

    assert!(!succeeds(output));
    assert!(git(&remote, &["branch", "--list"]).is_empty());
}

// -- the dispatchers, against the real gate --------------------------------

/// Writes `content` to `name` and commits it as session `a`, so the dispatchers see the move.
fn session_commit(fixture: &Fixture, name: &str, content: &str, message: &str) {
    std::fs::write(fixture.repo.join(name), content).unwrap();
    ok(fixture, &["add", "-A"]);
    ok(fixture, &["commit", "-q", "-m", message]);
}

/// A squash that changed the base: `feature` gets the content of the old `main` on a commit whose
/// parent is the latest, so `main`'s own change is undone. Returns `main`'s tip before and the
/// squash, with `main` checked out again.
fn squash_with_changed_base(fixture: &Fixture) -> (String, String) {
    let repo = fixture.repo.to_path_buf();
    ok(fixture, &["switch", "-q", "-c", "feature"]);
    fixture.deliver(1);
    let feature = numbered("alpha", 10) + &numbered("feature", 3);
    session_commit(fixture, "a.txt", &feature, "feature");
    ok(fixture, &["switch", "-q", "main"]);
    session_commit(fixture, "other.txt", &numbered("beta", 6), "main moves on");
    let old = rev(&repo, "main");
    ok(fixture, &["switch", "-q", "feature"]);
    ok(fixture, &["reset", "-q", "--soft", "main"]);
    ok(fixture, &["commit", "-q", "-m", "feature, squashed"]);
    let squashed = rev(&repo, "feature");
    ok(fixture, &["switch", "-q", "main"]);
    (old, squashed)
}

#[test]
fn a_move_that_undoes_merged_content_is_rejected_until_the_switch_is_turned_off() {
    let (fixture, _) = project_with_remote();
    let daemon = Daemon::start(&fixture.state);
    fixture.port.store(daemon.port, Ordering::Relaxed);
    let repo = fixture.repo.to_path_buf();
    let (old, squashed) = squash_with_changed_base(&fixture);

    let output = real_gate(
        &fixture,
        &repo,
        &["update-ref", "refs/heads/main", &squashed, &old],
    );
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(!succeeds(output), "{stderr}");
    assert!(
        stderr.contains("[revert-gate] Refusing to move main"),
        "{stderr}"
    );
    assert_eq!(rev(&repo, "main"), old);

    set_revert_gate(&fixture, false);
    let output = real_gate(
        &fixture,
        &repo,
        &["update-ref", "refs/heads/main", &squashed, &old],
    );
    assert!(succeeds(output), "the same move, with the same session");
    assert_eq!(rev(&repo, "main"), squashed);
}

/// Someone else lands a change on the remote's `main`.
fn land_on_remote(fixture: &Fixture, origin: &Path, name: &str, content: &str) {
    let other = ScratchDir::new("branch-origin-gate-other");
    git(&other, &["clone", "-q", origin.to_str().unwrap(), "."]);
    commit_file(&other, name, content, "landed elsewhere");
    git(&other, &["push", "-q", "origin", "main"]);
    ok(fixture, &["fetch", "-q", "origin"]);
}

/// `update-ref` and `branch -f` name no old value, so the transaction cannot tell the update from
/// a creation; it is gated all the same.
#[test]
fn a_forced_update_of_a_guarded_branch_is_rejected_by_the_real_gate() {
    let (fixture, _) = project_with_remote();
    let daemon = Daemon::start(&fixture.state);
    fixture.port.store(daemon.port, Ordering::Relaxed);
    let repo = fixture.repo.to_path_buf();
    let (old, squashed) = squash_with_changed_base(&fixture);
    // Git refuses `branch -f` on a checked-out branch by itself, which would pass the test
    // without the gate.
    ok(&fixture, &["switch", "-q", "feature"]);

    let output = real_gate(
        &fixture,
        &repo,
        &["update-ref", "refs/heads/main", &squashed],
    );
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(!succeeds(output), "{stderr}");
    assert!(
        stderr.contains("[revert-gate] Refusing to move main"),
        "{stderr}"
    );
    assert_eq!(rev(&repo, "main"), old);

    let output = real_gate(&fixture, &repo, &["branch", "-f", "main", &squashed]);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(!succeeds(output), "{stderr}");
    assert!(
        stderr.contains("[revert-gate] Refusing to move main"),
        "{stderr}"
    );
    assert_eq!(rev(&repo, "main"), old);
}

#[test]
fn a_pull_request_branch_pushed_with_a_changed_base_is_rejected() {
    let (fixture, origin) = project_with_remote();
    let daemon = Daemon::start(&fixture.state);
    fixture.port.store(daemon.port, Ordering::Relaxed);
    let repo = fixture.repo.to_path_buf();

    ok(&fixture, &["switch", "-q", "-c", "feature"]);
    fixture.deliver(1);
    let feature = numbered("alpha", 10) + &numbered("feature", 3);
    session_commit(&fixture, "a.txt", &feature, "feature");
    ok(&fixture, &["push", "-q", "origin", "feature"]);
    let pushed = rev(&origin, "feature");
    land_on_remote(&fixture, &origin, "other.txt", &numbered("beta", 6));
    ok(&fixture, &["reset", "-q", "--soft", "origin/main"]);
    ok(&fixture, &["commit", "-q", "-m", "feature, squashed"]);
    let squashed = rev(&repo, "feature");

    let output = real_gate(
        &fixture,
        &repo,
        &["push", "-q", "--force", "origin", "feature"],
    );
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(!succeeds(output), "{stderr}");
    assert!(
        stderr.contains("[revert-gate] Refusing to push feature"),
        "{stderr}"
    );
    assert_eq!(rev(&origin, "feature"), pushed);

    set_revert_gate(&fixture, false);
    ok(&fixture, &["push", "-q", "--force", "origin", "feature"]);
    assert_eq!(rev(&origin, "feature"), squashed);
}

#[test]
fn a_guarded_branch_pushed_over_remote_commits_the_clone_lacks_is_rejected() {
    let (fixture, origin) = project_with_remote();
    let daemon = Daemon::start(&fixture.state);
    fixture.port.store(daemon.port, Ordering::Relaxed);
    let remote_tip = rev(&origin, "main");
    let other = ScratchDir::new("branch-origin-gate-other");
    git(&other, &["clone", "-q", origin.to_str().unwrap(), "."]);
    commit_file(
        &other,
        "other.txt",
        &numbered("beta", 6),
        "landed elsewhere",
    );
    git(&other, &["push", "-q", "origin", "main"]);
    session_commit(&fixture, "a.txt", &numbered("alpha", 11), "local");

    let output = real_gate(
        &fixture,
        &fixture.repo,
        &["push", "-q", "--force", "origin", "main"],
    );
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    assert!(!succeeds(output), "{stderr}");
    assert!(
        stderr.contains("[revert-gate] Refusing to push main"),
        "{stderr}"
    );
    assert_ne!(
        rev(&origin, "main"),
        remote_tip,
        "the other clone's commit is there"
    );
    assert!(git(&origin, &["log", "--format=%s", "main"]).starts_with(b"landed elsewhere"));
}

#[test]
fn ordinary_commits_rebases_and_pushes_pass_the_real_gate() {
    let (fixture, origin) = project_with_remote();
    let daemon = Daemon::start(&fixture.state);
    fixture.port.store(daemon.port, Ordering::Relaxed);

    ok(&fixture, &["switch", "-q", "-c", "feature"]);
    fixture.deliver(1);
    session_commit(
        &fixture,
        "a.txt",
        &(numbered("alpha", 10) + "more\n"),
        "feature",
    );
    land_on_remote(&fixture, &origin, "other.txt", &numbered("beta", 6));
    ok(&fixture, &["rebase", "-q", "origin/main"]);
    ok(&fixture, &["push", "-q", "--force", "origin", "feature"]);
    ok(&fixture, &["switch", "-q", "main"]);
    ok(&fixture, &["merge", "-q", "--ff-only", "origin/main"]);
    ok(&fixture, &["merge", "-q", "--no-edit", "feature"]);
    ok(&fixture, &["push", "-q", "origin", "main"]);
}

#[test]
fn a_daemon_that_cannot_be_reached_lets_everything_through() {
    let (fixture, _) = project_with_remote();
    fixture.port.store(closed_port(), Ordering::Relaxed);

    ok(&fixture, &["commit", "-q", "--allow-empty", "-m", "next"]);
    ok(&fixture, &["push", "-q", "--force", "origin", "main"]);
}
