//! The revert gate in the hook dispatchers: what the daemon answers a gate-hook, and the
//! dispatchers' handling of a branch that moves or is pushed.
//!
//! Most of the dispatcher tests run the real `gate-hook` against a daemon served from the test
//! process, so that a rejection is the engine's own: the stand-in the dispatchers are written with
//! starts this test binary to run [`gate_hook_child`] where `OCTOBOARD_TEST_GATE_BINARY` is set.
//! The rest use the stand-in's plain mode to see exactly what a dispatcher starts the gate with
//! and how it takes the gate's status.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::Ordering;

use crate::platform::shell::posix_interpreter;
use crate::protocol::{GitDevelopment, GitWorkflowSettingsChanges, SessionKind};
use crate::session_git_hooks::fixture::{
    commit_file, data_dir_of, dispatchers, dispatchers_in, gated_fixture, numbered, project,
    project_with_remote, rev, session_record, set_revert_gate, succeeds, system_exec_path,
    write_hook, Daemon, Fixture,
};
use crate::session_git_hooks::gate_inputs::{answer, GateRequest};
use crate::session_git_hooks::gate_mode;
use crate::session_git_hooks::route::Routes;
use crate::session_git_hooks::{render, write_dir, Rendering, BUILT_IN_HOOKS};
use crate::test_support::{fixture_git, git, ClosedPort, ScratchDir, PATIENCE, TEST_KEY};
use crate::wire_path;

// -- the gate-hook process --------------------------------------------------------

/// A port nothing listens on, that no parallel test is given.
fn closed_port() -> u16 {
    ClosedPort::new().port
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
        crate::cli::Cli::try_parse_from(["octoboardd"].into_iter().chain(args.split(' '))).unwrap();
    let Some(crate::cli::Command::GateHook {
        session,
        port,
        data_dir,
        hook,
    }) = cli.command
    else {
        panic!("not the gate-hook command");
    };
    std::process::exit(crate::session_git_hooks::gate_mode::run(
        &session,
        port,
        // The dispatchers' own, as the helper takes it; never the developer's home.
        &data_dir.unwrap_or_else(|| crate::test_support::isolated_home().join(".octoboard")),
        &hook,
    ));
}

/// What the gate is asked through: the launch's route on `port` with `key`, and the daemon's
/// route through a data directory that holds no record of a daemon.
fn launch_route(port: u16, key: &str) -> Routes {
    Routes {
        launch: Some((port, key.to_string())),
        data: crate::test_support::isolated_home().join(".octoboard"),
    }
}

/// [`gate_mode::answer_for`] through `routes`, with the deadlines the gate has.
fn ask_through(
    routes: &Routes,
    session: &str,
    repo: &Path,
) -> Option<crate::revert_gate::GateInputs> {
    gate_mode::answer_for(routes, session, repo, None, gate_mode::DEADLINES)
        .and_then(|answer| answer.inputs)
}

/// `git <args>` run as session `a` in `dir`, whose dispatchers start the real gate-hook.
pub(super) fn real_gate(fixture: &Fixture, dir: &Path, args: &[&str]) -> Output {
    real_gate_command(fixture, dir, args).output().unwrap()
}

/// The command [`real_gate`] runs.
pub(super) fn real_gate_command(fixture: &Fixture, dir: &Path, args: &[&str]) -> Command {
    real_gate_command_under(fixture, dispatchers(), dir, args)
}

/// [`real_gate_command`] with the dispatchers at `hooks`, whose data directory is the one the real
/// `gate-hook` finds the daemon's records in.
fn real_gate_command_under(fixture: &Fixture, hooks: &Path, dir: &Path, args: &[&str]) -> Command {
    let mut command = fixture.session_git_with(hooks, "a", dir, &[]);
    command
        .env(
            "OCTOBOARD_TEST_GATE_BINARY",
            std::env::current_exe().unwrap(),
        )
        .args(args);
    command
}

fn ok(fixture: &Fixture, args: &[&str]) {
    let output = real_gate(fixture, &fixture.repo, args);
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// -- what the daemon answers ---------------------------------------------------

/// The gate's inputs the daemon answers session `session_id` with, `None` where the hook is to
/// skip the gate.
pub(super) fn inputs(
    state: &crate::state::AppState,
    session_id: &str,
    request: &GateRequest,
) -> Option<crate::revert_gate::GateInputs> {
    answer(state, session_id, request).and_then(|answer| answer.inputs)
}

fn request(dir: &Path) -> GateRequest {
    GateRequest {
        cwd: wire_path::encode(&crate::platform::names::name_bytes(dir.as_os_str())),
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
    reftable_default_branch("origin");
}

/// The branch comes back wire-encoded, remote included, so a `%` in the remote's name has to be
/// decoded before the name is split by the remotes `git` lists.
#[test]
fn the_default_branch_of_a_reftable_repository_whose_remote_name_holds_a_percent_sign() {
    reftable_default_branch("team%one");
}

fn reftable_default_branch(remote: &str) {
    let fixture = gated_fixture();
    let repo = ScratchDir::new("branch-origin-gate-reftable");
    let init = fixture_git(&repo)
        .args(["init", "-q", "--ref-format=reftable"])
        .output()
        .unwrap();
    if !init.status.success() {
        return;
    }
    commit_file(&repo, "a.txt", &numbered("alpha", 3), "base");
    git(
        &repo,
        &["config", &format!("remote.{remote}.url"), "/nowhere"],
    );
    git(
        &repo,
        &[
            "config",
            &format!("remote.{remote}.fetch"),
            &format!("+refs/heads/*:refs/remotes/{remote}/*"),
        ],
    );
    git(
        &repo,
        &[
            "update-ref",
            &format!("refs/remotes/{remote}/trunk"),
            "HEAD",
        ],
    );
    git(
        &repo,
        &[
            "symbolic-ref",
            &format!("refs/remotes/{remote}/HEAD"),
            &format!("refs/remotes/{remote}/trunk"),
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
    let fixture = gated_fixture();

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
    console.kind = SessionKind::Console;
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
    project.git_workflow.revert_gate = Some(true);
    fixture.state.store.update_project(&project).unwrap();
    assert!(ask().is_some(), "the project holds its own value");

    project.git_workflow.revert_gate = Some(false);
    fixture.state.store.update_project(&project).unwrap();
    set_revert_gate(&fixture, true);
    assert_eq!(ask(), None, "and so does it when it is off");
}

fn set_development(fixture: &Fixture, development: GitDevelopment) {
    fixture
        .state
        .store
        .change_git_workflow_settings(&GitWorkflowSettingsChanges {
            development: Some(development),
            ..Default::default()
        })
        .unwrap();
}

#[test]
fn the_gate_runs_whatever_the_development_setting_is() {
    let (fixture, _) = project_with_remote();
    let ask = || inputs(&fixture.state, "a", &request(&fixture.repo));
    for development in [
        GitDevelopment::InPlace,
        GitDevelopment::Branch,
        GitDevelopment::Worktree,
    ] {
        set_development(&fixture, development);
        assert!(ask().is_some(), "{development:?}");
    }

    set_development(&fixture, GitDevelopment::InPlace);
    set_revert_gate(&fixture, false);
    assert_eq!(ask(), None, "the gate switch alone decides");
}

#[test]
fn a_projects_own_development_does_not_decide_whether_the_gate_runs() {
    let (fixture, _) = project_with_remote();
    let ask = || inputs(&fixture.state, "a", &request(&fixture.repo));
    let mut project = fixture.state.store.get_project("project").unwrap().unwrap();

    project.git_workflow.development = Some(GitDevelopment::InPlace);
    fixture.state.store.update_project(&project).unwrap();
    assert!(ask().is_some(), "the project works on the current branch");

    set_revert_gate(&fixture, false);
    assert_eq!(ask(), None, "its own development does not turn the gate on");
    project.git_workflow.revert_gate = Some(true);
    fixture.state.store.update_project(&project).unwrap();
    assert!(ask().is_some(), "its own gate switch does");
}

#[test]
fn the_hook_side_takes_only_an_answer_as_a_reason_to_run_the_gate() {
    let (fixture, _) = project_with_remote();
    let daemon = Daemon::start(&fixture.state);
    let repo = fixture.repo.to_path_buf();
    let ask = |port: u16, session: &str| ask_through(&launch_route(port, TEST_KEY), session, &repo);

    let answer = ask(daemon.port, "a").expect("an answer");
    assert_eq!(answer.guard, ["main"]);
    assert_eq!(ask(daemon.port, "nobody"), None);
    set_revert_gate(&fixture, false);
    assert_eq!(ask(daemon.port, "a"), None);
    assert_eq!(ask(closed_port(), "a"), None, "no daemon is no answer");
    assert_eq!(
        ask_through(&launch_route(daemon.port, "wrong"), "a", &repo),
        None,
        "a refusal is no answer either, whatever the daemon's body says"
    );
}

/// A stand-in for a holder that answers the one request it gets with `status`, and says nothing
/// else. Returns its port.
pub(super) fn answering_with(status: u16) -> u16 {
    use std::io::{Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (mut helper, _) = listener.accept().unwrap();
        let mut request = [0u8; 1024];
        let _ = helper.read(&mut request);
        write!(
            helper,
            "HTTP/1.1 {status} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let _ = helper.shutdown(std::net::Shutdown::Write);
        let mut rest = Vec::new();
        let _ = helper.read_to_end(&mut rest);
    });
    port
}

#[test]
fn a_holder_with_no_daemon_or_one_that_was_lost_is_no_answer_to_run_the_gate_with() {
    let (fixture, _) = project_with_remote();
    let repo = fixture.repo.to_path_buf();
    for status in [crate::loopback::NO_DAEMON, crate::loopback::DAEMON_LOST] {
        let port = answering_with(status);
        assert_eq!(
            ask_through(&launch_route(port, TEST_KEY), "a", &repo),
            None,
            "{status}"
        );
    }
}

#[test]
fn a_refused_call_reaches_the_hook_side_as_a_refusal_not_as_an_answer() {
    let (fixture, _) = project_with_remote();
    let daemon = Daemon::start(&fixture.state);
    let post = |key: &str, body: &[u8]| {
        crate::loopback::post(daemon.port, key, "/revert-gate/a", body, PATIENCE)
    };
    let err = post("wrong", b"{}").expect_err("the key is wrong");
    assert_eq!(crate::loopback::error_status(&err), Some(401));
    let err = post(TEST_KEY, b"not json").expect_err("the request is unreadable");
    assert_eq!(crate::loopback::error_status(&err), Some(400));
}

// -- the dispatchers, against the stand-in ----------------------------------

/// The old value git gives a forced update, which names no value the ref had.
const ZERO_OID: &str = "0000000000000000000000000000000000000000";

/// What the stand-in was started with and given, one line per call: its arguments, a tab, and the
/// input with its newlines turned to spaces.
fn gate_calls(fixture: &Fixture) -> Vec<String> {
    std::fs::read_to_string(&*fixture.gates)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// The updates of a reference-transaction input as [`gate_calls`] gives it: an old value, a new
/// value and a ref name each, in the order git listed them, which is not always the same.
fn updates(input: &str) -> Vec<[&str; 3]> {
    let words: Vec<&str> = input.split_whitespace().collect();
    assert_eq!(words.len() % 3, 0, "{input}");
    words
        .chunks(3)
        .map(|update| [update[0], update[1], update[2]])
        .collect()
}

/// The update of `name` in the call `call` of [`gate_calls`].
fn update_of<'a>(call: &'a str, name: &str) -> Option<[&'a str; 3]> {
    let (_, input) = call.split_once('\t')?;
    updates(input)
        .into_iter()
        .find(|[_, _, updated]| *updated == name)
}

fn with_gate_status<'a>(command: &'a mut Command, status: &str) -> &'a mut Command {
    command.env("OCTOBOARD_TEST_GATE_STATUS", status)
}

#[test]
fn an_existing_branch_moving_is_put_to_the_gate_once_and_a_creation_is_not() {
    let fixture = gated_fixture();
    fixture.as_session("a", &["commit", "-q", "--allow-empty", "-m", "next"]);

    let calls = gate_calls(&fixture);
    assert_eq!(calls.len(), 1, "prepared only: {calls:?}");
    let (args, input) = calls[0].split_once('\t').unwrap();
    assert_eq!(
        args,
        format!(
            "gate-hook --session a --port 1 --data-dir {} reference-transaction prepared",
            data_dir_of(dispatchers())
        )
    );
    assert!(
        updates(input)
            .iter()
            .any(|[_, _, name]| *name == "refs/heads/main"),
        "{input}"
    );

    fixture.as_session("a", &["branch", "topic"]);
    fixture.as_session("a", &["tag", "t"]);
    fixture.as_session("a", &["branch", "-q", "-D", "topic"]);
    assert_eq!(gate_calls(&fixture).len(), 1, "no existing branch moved");
}

#[test]
fn an_existing_branch_forced_without_its_old_value_is_put_to_the_gate() {
    let fixture = gated_fixture();
    let first = rev(&fixture.repo, "HEAD");
    fixture.as_session("a", &["commit", "-q", "--allow-empty", "-m", "next"]);
    fixture.as_session("a", &["branch", "topic", &first]);
    let before = gate_calls(&fixture).len();

    fixture.as_session("a", &["update-ref", "refs/heads/main", &first]);
    fixture.as_session("a", &["branch", "-f", "topic", "main"]);
    let calls = gate_calls(&fixture);
    assert_eq!(calls.len(), before + 2, "{calls:?}");
    // Git may list `HEAD`, which follows `main`, before or after it.
    let forced =
        |call: &str, name: &str| update_of(call, name) == Some([ZERO_OID, first.as_str(), name]);
    assert!(forced(&calls[before], "refs/heads/main"), "{calls:?}");
    assert!(forced(&calls[before + 1], "refs/heads/topic"), "{calls:?}");
}

#[test]
fn a_gate_that_rejects_fails_the_transaction_and_anything_else_lets_it_through() {
    let fixture = gated_fixture();
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
    let fixture = gated_fixture();
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
    let fixture = gated_fixture();
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
    let fixture = gated_fixture();
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

/// A git whose environment names the session but no port and no key file (a job the session left
/// running, with its launch's variables gone) still has its helpers started, without a port, which
/// is what sends them to the daemon's own route; one that does not name the session has none.
#[test]
fn a_session_whose_environment_names_no_port_or_key_file_still_starts_its_helpers() {
    let fixture = gated_fixture();
    let without_route = |args: &[&str]| {
        let mut command = fixture.session_git("a", &fixture.repo, &[]);
        command
            .env_remove("OCTOBOARD_DAEMON_PORT")
            .env_remove("OCTOBOARD_DAEMON_CREDENTIAL_PATH")
            .args(args);
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };

    without_route(&["commit", "-q", "--allow-empty", "-m", "next"]);
    let calls = gate_calls(&fixture);
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(
        calls[0].split_once('\t').unwrap().0,
        format!(
            "gate-hook --session a --data-dir {} reference-transaction prepared",
            data_dir_of(dispatchers())
        )
    );

    without_route(&["branch", "topic"]);
    let deadline = std::time::Instant::now() + PATIENCE;
    let lines = loop {
        let lines = fixture.report_lines();
        if lines.iter().any(|line| line.contains("refs/heads/topic")) {
            break lines;
        }
        assert!(std::time::Instant::now() < deadline, "no report: {lines:?}");
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    for line in &lines {
        let args = line.splitn(4, '\t').nth(3).unwrap();
        assert!(args.starts_with("git-ref --session a "), "{args}");
        assert!(!args.contains("--port"), "{args}");
        assert!(
            args.contains(&format!(" --data-dir {} ", data_dir_of(dispatchers()))),
            "{args}"
        );
    }
}

#[test]
fn a_gate_that_cannot_start_lets_the_update_through() {
    let fixture = gated_fixture();
    let root = fixture.dir.join("hooks-without-binary");
    let names: BTreeSet<String> = BUILT_IN_HOOKS.iter().map(|n| n.to_string()).collect();
    let state = fixture.dir.join("state");
    std::fs::create_dir(&state).unwrap();
    let script = render(&Rendering {
        interpreter: posix_interpreter(),
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
    let fixture = gated_fixture();
    push_setup(&fixture);
    fixture.as_session("a", &["push", "-q", "origin", "main"]);

    let calls = gate_calls(&fixture);
    assert_eq!(calls.len(), 1, "{calls:?}");
    let (args, input) = calls[0].split_once('\t').unwrap();
    assert_eq!(
        args,
        format!(
            "gate-hook --session a --port 1 --data-dir {} pre-push",
            data_dir_of(dispatchers())
        )
    );
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
    let fixture = gated_fixture();
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
    let fixture = gated_fixture();
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
pub(super) fn squash_with_changed_base(fixture: &Fixture) -> (String, String) {
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

/// With no `OCTOBOARD_DAEMON_CREDENTIAL_PATH`, an empty one, or one naming a file that is missing,
/// unreadable, empty or blank, `gate-hook` finds no launch key and goes to the daemon's own route;
/// with no daemon recorded in the data directory either, it lets the update through.
#[test]
fn a_move_the_gate_would_reject_passes_when_neither_route_can_be_used() {
    let (fixture, _) = project_with_remote();
    let daemon = Daemon::start(&fixture.state);
    fixture.port.store(daemon.port, Ordering::Relaxed);
    let repo = fixture.repo.to_path_buf();
    let (old, squashed) = squash_with_changed_base(&fixture);
    let args = ["update-ref", "refs/heads/main", &squashed, &old];

    let with_key = real_gate(&fixture, &repo, &args);
    assert!(!succeeds(with_key));
    assert_eq!(rev(&repo, "main"), old);

    let empty = fixture.dir.join("empty-key");
    std::fs::write(&empty, "").unwrap();
    let blank = fixture.dir.join("blank-key");
    std::fs::write(&blank, "\n").unwrap();
    let missing = fixture.dir.join("missing-key");
    // A path that names something that cannot be read as a file.
    let unreadable = fixture.dir.to_path_buf();
    let no_path = std::ffi::OsString::new();
    for key_file in [
        None,
        Some(no_path.as_os_str()),
        Some(empty.as_os_str()),
        Some(blank.as_os_str()),
        Some(missing.as_os_str()),
        Some(unreadable.as_os_str()),
    ] {
        let mut command = real_gate_command(&fixture, &repo, &args);
        match key_file {
            Some(path) => command.env("OCTOBOARD_DAEMON_CREDENTIAL_PATH", path),
            None => command.env_remove("OCTOBOARD_DAEMON_CREDENTIAL_PATH"),
        };
        let output = command.output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(succeeds(output), "{key_file:?}: {stderr}");
        assert!(!stderr.contains("revert-gate"), "{key_file:?}: {stderr}");
        assert_eq!(rev(&repo, "main"), squashed, "{key_file:?}");
        git(&repo, &["update-ref", "refs/heads/main", &old]);
    }
}

/// A job left running after its session ended: the session is archived, its scratch directory and
/// key file are gone, and nothing listens on the port its environment names any more (the holder
/// has exited). The gate still asks the daemon, through the record it keeps in the data directory
/// the dispatchers are under, and still rejects a move it would reject; so does the same job with
/// no port at all.
#[test]
fn a_job_its_session_left_running_is_still_gated_through_the_daemons_record() {
    let (fixture, _) = project_with_remote();
    let daemon = Daemon::start(&fixture.state);
    // The dispatchers' own data directory holds the daemon's records; the home the job's
    // environment names is not it.
    let data = fixture.dir.join("data");
    crate::owner_record::Owner::current(daemon.port, false, true)
        .expect("this process's identity")
        .record_in(&data, TEST_KEY)
        .unwrap();
    let hooks = dispatchers_in(&data.join("githooks"));
    let home = crate::test_support::isolated_home();
    let mut ended = session_record("a", "project");
    ended.status = crate::protocol::SessionStatus::Archived;
    ended.ended_at = Some(crate::time::now_millis());
    fixture.state.store.update_session(&ended).unwrap();
    fixture.port.store(closed_port(), Ordering::Relaxed);
    let repo = fixture.repo.to_path_buf();
    let (old, squashed) = squash_with_changed_base(&fixture);
    let args = ["update-ref", "refs/heads/main", &squashed, &old];

    for port in [true, false] {
        let mut command = real_gate_command_under(&fixture, &hooks, &repo, &args);
        command.env("HOME", &home).env("USERPROFILE", &home).env(
            "OCTOBOARD_DAEMON_CREDENTIAL_PATH",
            fixture.dir.join("run/a/key"),
        );
        if !port {
            command.env_remove("OCTOBOARD_DAEMON_PORT");
        }
        let output = command.output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(!succeeds(output), "port {port}: {stderr}");
        assert!(
            stderr.contains("[revert-gate] Refusing to move main"),
            "port {port}: {stderr}"
        );
        assert_eq!(rev(&repo, "main"), old, "port {port}");
    }

    // The control: without the daemon's record the same move goes through.
    let output = real_gate_command(&fixture, &repo, &args)
        .env(
            "OCTOBOARD_DAEMON_CREDENTIAL_PATH",
            fixture.dir.join("run/a/key"),
        )
        .output()
        .unwrap();
    assert!(succeeds(output));
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

#[test]
fn squashing_on_the_merge_base_passes_even_when_the_unpushed_branch_is_guarded() {
    for guarded in [false, true] {
        let (fixture, _) = project_with_remote();
        let daemon = Daemon::start(&fixture.state);
        fixture.port.store(daemon.port, Ordering::Relaxed);
        ok(&fixture, &["switch", "-q", "-c", "feature"]);
        fixture.deliver(1);
        session_commit(&fixture, "a.txt", &numbered("feature", 10), "replace alpha");
        session_commit(
            &fixture,
            "other.txt",
            &numbered("changed", 6),
            "replace other",
        );
        let old = rev(&fixture.repo, "HEAD");
        let tree = rev(&fixture.repo, "HEAD^{tree}");
        ok(&fixture, &["switch", "-q", "main"]);
        session_commit(&fixture, "new.txt", "target work\n", "target moves on");
        ok(&fixture, &["switch", "-q", "feature"]);
        let base = String::from_utf8(git(
            &fixture.repo,
            &["merge-base", "HEAD", "refs/heads/main"],
        ))
        .unwrap()
        .trim()
        .to_string();
        assert_ne!(base, rev(&fixture.repo, "main"));
        if guarded {
            ok(&fixture, &["branch", "child", "feature"]);
            fixture.deliver(2);
            assert!(fixture
                .rows()
                .iter()
                .any(|row| row.target_branch.as_deref() == Some("feature")));
        }
        let output = real_gate(&fixture, &fixture.repo, &["reset", "--soft", &base]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.success(), !guarded, "{stderr}");
        if guarded {
            assert!(
                stderr.contains("[revert-gate] Refusing to move feature"),
                "{stderr}"
            );
            assert!(
                stderr.contains("this update undoes changes made by existing commits"),
                "{stderr}"
            );
            assert!(!stderr.contains("drop published commits"), "{stderr}");
            assert_eq!(rev(&fixture.repo, "HEAD"), old);
        } else {
            ok(&fixture, &["reset", "--soft", &old]);
        }
        let output = fixture
            .session_git("a", &fixture.repo, &[])
            .env(
                "OCTOBOARD_TEST_GATE_BINARY",
                std::env::current_exe().unwrap(),
            )
            .env("GIT_SEQUENCE_EDITOR", "sed -i.bak '2,$s/^pick /fixup /'")
            .env("GIT_EDITOR", "true")
            .args(["rebase", "-i", &base])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "interactive squash: {stderr}");
        assert!(!stderr.contains("[revert-gate] Check failed"), "{stderr}");
        assert_eq!(rev(&fixture.repo, "HEAD^{tree}"), tree);
        ok(&fixture, &["diff", "--exit-code", &old, "HEAD", "--"]);
        assert_eq!(
            String::from_utf8(git(
                &fixture.repo,
                &["rev-list", "--count", &format!("{base}..HEAD")]
            ))
            .unwrap()
            .trim(),
            "1"
        );
        assert_eq!(rev(&fixture.repo, "HEAD^"), base);
        if guarded {
            let squashed = rev(&fixture.repo, "HEAD");
            let output = real_gate(&fixture, &fixture.repo, &["reset", "--hard", &base]);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "{stderr}");
            assert!(
                stderr.contains("[revert-gate] Refusing to move feature"),
                "{stderr}"
            );
            assert_eq!(rev(&fixture.repo, "HEAD"), squashed);
        }
    }
}
