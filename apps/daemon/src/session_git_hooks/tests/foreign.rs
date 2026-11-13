//! The verdict on a repository of another project, in what the daemon answers a gate-hook and in
//! the hook that acts on it: with the setting that keeps sessions to their own project on, a ref
//! update made from a project session's git in a repository that belongs to another registered
//! project, and to none of the session's own, is refused.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use crate::cross_project_guard::ForeignRepository;
use crate::protocol::{AgentId, SessionKind};
use crate::revert_gate::GateInputs;
use crate::session_git_hooks::fixture::{
    project, project_with_remote, rev, session_record, set_revert_gate, succeeds, Daemon, Fixture,
};
use crate::session_git_hooks::gate_inputs::{answer, GateAnswer, GateRequest};
use crate::session_git_hooks::gate_mode;
use crate::session_git_hooks::route::Routes;
use crate::test_support::{git, repo_with, ClosedPort, ScratchDir, TEST_KEY};
use crate::wire_path;

use super::gate::{answering_with, inputs, real_gate, real_gate_command, squash_with_changed_base};

/// The name of the project the other repository belongs to.
const OTHER: &str = "other";

/// A fixture whose switch is on and which has a second project, [`OTHER`], in a repository of its
/// own, which is returned. Session `a` is of the fixture's own project.
fn with_foreign_repository() -> (Fixture, ScratchDir) {
    let fixture = Fixture::new();
    fixture
        .state
        .store
        .set_keep_sessions_in_own_project(true)
        .unwrap();
    let other = repo_with("foreign-repo", &[(b"f", b"f")]);
    fixture
        .state
        .store
        .insert_project(&project(OTHER, &other))
        .unwrap();
    (fixture, other)
}

fn request(dir: &Path) -> GateRequest {
    GateRequest {
        cwd: wire_path::encode(&crate::platform::names::name_bytes(dir.as_os_str())),
        git_dir: None,
    }
}

/// The verdict the daemon gives session `session` for a hook run in `dir`.
fn verdict(fixture: &Fixture, session: &str, dir: &Path) -> Option<ForeignRepository> {
    answer(&fixture.state, session, &request(dir)).and_then(|answer| answer.foreign_repository)
}

/// What the refusal names as the tool to call, for the agent of the fixture's sessions.
fn request_session_tool() -> String {
    // Claude Code's own name for the `request_session` tool of Octoboard's MCP server.
    "mcp__octoboard__request_session".to_string()
}

/// The refusal as the contract words it, for the working tree `worktree`.
fn refusal(project: &str, tool: &str, worktree: &Path) -> String {
    format!(
        "Refused: this repository belongs to project \"{project}\", not this session's. Keep the \
         changes as they are (no revert, stash or reset) and call {tool} for a \"{project}\" \
         session: list the changed files, say they are uncommitted in {}, and ask it to review \
         them there and take over the rest of the work.",
        worktree.display()
    )
}

fn commit_empty(fixture: &Fixture, dir: &Path) -> std::process::Output {
    real_gate(
        fixture,
        dir,
        &["commit", "-q", "--allow-empty", "-m", "next"],
    )
}

// -- what the daemon answers ---------------------------------------------------

#[test]
fn a_repository_of_another_project_is_refused_naming_the_project_and_the_tool() {
    let (fixture, other) = with_foreign_repository();
    let expected = ForeignRepository {
        project: OTHER.to_string(),
        tool: request_session_tool(),
    };

    assert_eq!(verdict(&fixture, "a", &other), Some(expected));

    // The verdict comes first: it is given whether or not the revert gate applies, and none of
    // the gate's inputs goes with it.
    for gate in [false, true] {
        set_revert_gate(&fixture, gate);
        let reply = answer(&fixture.state, "a", &request(&other)).expect("an answer");
        assert_eq!(reply.inputs, None, "gate {gate}");
        assert!(reply.foreign_repository.is_some(), "gate {gate}");
    }
}

#[test]
fn the_tool_named_is_the_qualified_request_session_of_the_sessions_agent() {
    let (fixture, other) = with_foreign_repository();
    for agent in AgentId::ALL {
        let mut session = session_record("a", "project");
        session.agent = agent;
        fixture.state.store.update_session(&session).unwrap();

        let foreign = verdict(&fixture, "a", &other).expect("a verdict");

        assert_eq!(
            foreign.tool,
            crate::session_tools::catalogue::qualified_tool_name(agent, "request_session")
        );
        assert!(
            foreign.tool.ends_with("request_session"),
            "{}",
            foreign.tool
        );
    }
}

#[test]
fn the_sessions_own_repository_and_the_worktrees_of_it_pass() {
    let (fixture, _other) = with_foreign_repository();
    let linked = fixture.dir.join("linked");
    git(
        &fixture.repo,
        &[
            "worktree",
            "add",
            "-q",
            linked.to_str().unwrap(),
            "-b",
            "wt",
        ],
    );

    assert_eq!(verdict(&fixture, "a", &fixture.repo), None);
    assert_eq!(verdict(&fixture, "a", &linked), None);
}

#[test]
fn a_repository_shared_with_the_sessions_own_project_passes_whichever_project_is_asked_of() {
    let (fixture, other) = with_foreign_repository();
    // A second project in the session's own repository, through a linked worktree of it.
    let linked = fixture.dir.join("sibling");
    git(
        &fixture.repo,
        &[
            "worktree",
            "add",
            "-q",
            linked.to_str().unwrap(),
            "-b",
            "sib",
        ],
    );
    fixture
        .state
        .store
        .insert_project(&project("sibling", &linked))
        .unwrap();
    // And a repository of two projects, none of them the session's.
    let second = other.join("second-worktree");
    git(
        &other,
        &[
            "worktree",
            "add",
            "-q",
            second.to_str().unwrap(),
            "-b",
            "second",
        ],
    );
    fixture
        .state
        .store
        .insert_project(&project("other-second", &second))
        .unwrap();

    assert_eq!(verdict(&fixture, "a", &fixture.repo), None);
    assert_eq!(verdict(&fixture, "a", &linked), None);
    let foreign =
        verdict(&fixture, "a", &other).expect("foreign: none of its projects is the session's");
    assert!(
        [OTHER, "other-second"].contains(&foreign.project.as_str()),
        "{}",
        foreign.project
    );
}

#[test]
fn a_repository_no_project_is_in_and_one_the_daemon_cannot_trace_pass() {
    let (fixture, _other) = with_foreign_repository();
    let unregistered = repo_with("unregistered-repo", &[(b"u", b"u")]);

    assert_eq!(verdict(&fixture, "a", &unregistered), None);
    assert_eq!(verdict(&fixture, "a", Path::new("/nonexistent")), None);
    let plain = ScratchDir::new("foreign-plain-directory");
    assert_eq!(verdict(&fixture, "a", &plain), None);
}

#[test]
fn console_sessions_and_unknown_sessions_pass() {
    let (fixture, other) = with_foreign_repository();
    let mut console = session_record("console-session", "project");
    console.project_id = None;
    console.kind = SessionKind::Console;
    fixture.state.store.insert_session(&console).unwrap();

    assert_eq!(verdict(&fixture, "console-session", &other), None);
    assert_eq!(verdict(&fixture, "nobody", &other), None);
}

#[test]
fn the_verdict_follows_the_switch_at_every_request() {
    let (fixture, other) = with_foreign_repository();
    assert!(verdict(&fixture, "a", &other).is_some());

    fixture
        .state
        .store
        .set_keep_sessions_in_own_project(false)
        .unwrap();
    assert_eq!(verdict(&fixture, "a", &other), None, "switched off");

    fixture
        .state
        .store
        .set_keep_sessions_in_own_project(true)
        .unwrap();
    assert!(
        verdict(&fixture, "a", &other).is_some(),
        "switched on again"
    );
}

#[test]
fn an_own_repository_is_answered_with_the_gates_inputs_as_before() {
    let (fixture, _) = project_with_remote();
    fixture
        .state
        .store
        .set_keep_sessions_in_own_project(true)
        .unwrap();
    let other = repo_with("foreign-repo-gated", &[(b"f", b"f")]);
    fixture
        .state
        .store
        .insert_project(&project(OTHER, &other))
        .unwrap();
    let expected = inputs(&fixture.state, "a", &request(&fixture.repo)).expect("inputs");

    let reply = answer(&fixture.state, "a", &request(&fixture.repo)).expect("an answer");

    assert_eq!(reply.foreign_repository, None);
    assert_eq!(reply.inputs.as_ref(), Some(&expected));
    assert_eq!(expected.guard, ["main"]);
}

// -- the wire form -------------------------------------------------------------

#[test]
fn an_older_daemons_reply_without_the_verdict_reads_as_no_verdict() {
    let old = serde_json::to_string(&GateInputs {
        guard: vec!["main".to_string()],
        targets: vec![("topic".to_string(), "main".to_string())],
        fork_points: vec![("topic".to_string(), "abc".to_string())],
    })
    .unwrap();

    let reply: Option<GateAnswer> = serde_json::from_str(&old).unwrap();

    let reply = reply.expect("an answer");
    assert_eq!(reply.foreign_repository, None);
    let inputs = reply.inputs.expect("the inputs");
    assert_eq!(inputs.guard, ["main"]);
    assert_eq!(inputs.targets.len(), 1);
    assert_eq!(
        serde_json::from_str::<Option<GateAnswer>>("null").unwrap(),
        None,
        "a daemon saying to skip is still that"
    );
}

/// An older hook parses the reply as the gate's inputs alone. A reply with no verdict reads as
/// them; one with a verdict carries none, so an older hook cannot read it and skips, rather than
/// run the gate on inputs the daemon meant to refuse the update over.
#[test]
fn an_older_hook_reads_an_answer_without_a_verdict_and_skips_one_that_has_it() {
    let inputs = GateInputs {
        guard: vec!["main".to_string()],
        ..GateInputs::default()
    };
    let plain = serde_json::to_string(&GateAnswer {
        inputs: Some(GateInputs {
            guard: inputs.guard.clone(),
            ..GateInputs::default()
        }),
        foreign_repository: None,
    })
    .unwrap();
    let as_an_older_hook: Option<GateInputs> = serde_json::from_str(&plain).expect("readable");
    assert_eq!(as_an_older_hook, Some(inputs));

    let refused = serde_json::to_string(&GateAnswer {
        inputs: None,
        foreign_repository: Some(ForeignRepository {
            project: OTHER.to_string(),
            tool: request_session_tool(),
        }),
    })
    .unwrap();
    assert!(
        serde_json::from_str::<Option<GateInputs>>(&refused).is_err(),
        "an older hook cannot read it: {refused}"
    );
    // A newer hook reads it back whole.
    let back: GateAnswer = serde_json::from_str(&refused).unwrap();
    assert_eq!(back.inputs, None);
    assert_eq!(back.foreign_repository.unwrap().project, OTHER);
}

/// A stand-in for a daemon that answers the one request it gets with `body` as JSON.
fn answering_with_body(body: &'static str) -> u16 {
    use std::io::{Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (mut helper, _) = listener.accept().unwrap();
        let mut request = [0u8; 4096];
        let _ = helper.read(&mut request);
        write!(
            helper,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let _ = helper.shutdown(std::net::Shutdown::Write);
        let mut rest = Vec::new();
        let _ = helper.read_to_end(&mut rest);
    });
    port
}

#[test]
fn the_hook_side_reads_the_verdict_from_the_daemons_reply_and_an_older_reply_as_none() {
    let (fixture, other) = with_foreign_repository();
    let daemon = Daemon::start(&fixture.state);
    let route = Routes {
        launch: Some((daemon.port, TEST_KEY.to_string())),
        data: crate::test_support::isolated_home().join(".octoboard"),
    };

    let reply =
        gate_mode::answer_for(&route, "a", &other, None, gate_mode::DEADLINES).expect("an answer");
    assert_eq!(
        reply.foreign_repository,
        Some(ForeignRepository {
            project: OTHER.to_string(),
            tool: request_session_tool(),
        })
    );

    let port = answering_with_body(r#"{"guard":["main"],"targets":[],"fork_points":[]}"#);
    let route = Routes {
        launch: Some((port, TEST_KEY.to_string())),
        data: crate::test_support::isolated_home().join(".octoboard"),
    };
    let reply = gate_mode::answer_for(&route, "a", &other, None, gate_mode::DEADLINES)
        .expect("an older daemon's answer");
    assert_eq!(reply.foreign_repository, None);
    assert_eq!(reply.inputs.expect("the inputs").guard, ["main"]);
}

// -- the hook, against a daemon ------------------------------------------------

/// A fixture with a foreign repository, and the daemon its sessions' dispatchers call.
fn served() -> (Fixture, ScratchDir, Daemon) {
    let (fixture, other) = with_foreign_repository();
    let daemon = Daemon::start(&fixture.state);
    fixture.port.store(daemon.port, Ordering::Relaxed);
    (fixture, other, daemon)
}

fn worktree_of(dir: &Path) -> PathBuf {
    std::fs::canonicalize(dir).unwrap()
}

#[test]
fn a_branch_moving_in_a_repository_of_another_project_is_refused_with_the_hand_over() {
    let (fixture, other, _daemon) = served();
    let head = rev(&other, "HEAD");

    let output = commit_empty(&fixture, &other);

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.contains(&refusal(
            OTHER,
            &request_session_tool(),
            &worktree_of(&other)
        )),
        "{stderr}"
    );
    assert_eq!(rev(&other, "HEAD"), head, "the branch did not move");
}

#[test]
fn the_sessions_own_repository_and_unregistered_ones_are_not_refused() {
    let (fixture, _other, _daemon) = served();
    let linked = fixture.dir.join("linked");
    git(
        &fixture.repo,
        &[
            "worktree",
            "add",
            "-q",
            linked.to_str().unwrap(),
            "-b",
            "wt",
        ],
    );
    let unregistered = repo_with("unregistered-repo-hook", &[(b"u", b"u")]);

    for dir in [
        &fixture.repo.to_path_buf(),
        &linked,
        &unregistered.to_path_buf(),
    ] {
        let output = commit_empty(&fixture, dir);
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(succeeds(output), "{}: {stderr}", dir.display());
    }
}

#[test]
fn a_flipped_switch_takes_effect_in_the_running_session() {
    let (fixture, other, _daemon) = served();
    assert!(!succeeds(commit_empty(&fixture, &other)));

    fixture
        .state
        .store
        .set_keep_sessions_in_own_project(false)
        .unwrap();
    assert!(succeeds(commit_empty(&fixture, &other)), "switched off");

    fixture
        .state
        .store
        .set_keep_sessions_in_own_project(true)
        .unwrap();
    assert!(!succeeds(commit_empty(&fixture, &other)), "switched on");
}

#[test]
fn skipping_the_revert_gate_does_not_skip_the_refusal_of_another_projects_repository() {
    let (fixture, other, _daemon) = served();
    let head = rev(&other, "HEAD");

    let output = real_gate_command(
        &fixture,
        &other,
        &["commit", "-q", "--allow-empty", "-m", "next"],
    )
    .env("REVERT_GATE_SKIP", "1")
    .output()
    .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(!succeeds(output), "{stderr}");
    assert!(
        stderr.contains("Refused: this repository belongs to project"),
        "{stderr}"
    );
    assert_eq!(rev(&other, "HEAD"), head);
}

#[test]
fn skipping_the_revert_gate_still_skips_the_gate_itself_in_the_sessions_own_repository() {
    let (fixture, _) = project_with_remote();
    fixture
        .state
        .store
        .set_keep_sessions_in_own_project(true)
        .unwrap();
    let daemon = Daemon::start(&fixture.state);
    fixture.port.store(daemon.port, Ordering::Relaxed);
    let repo = fixture.repo.to_path_buf();
    let (old, squashed) = squash_with_changed_base(&fixture);
    let args = ["update-ref", "refs/heads/main", &squashed, &old];

    // The control: without the request the gate rejects the move.
    let rejected = real_gate(&fixture, &repo, &args);
    assert!(!succeeds(rejected));
    assert_eq!(rev(&repo, "main"), old);

    let output = real_gate_command(&fixture, &repo, &args)
        .env("REVERT_GATE_SKIP", "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(succeeds(output), "{stderr}");
    assert_eq!(rev(&repo, "main"), squashed);
}

#[test]
fn a_push_is_not_refused_by_the_verdict() {
    let (fixture, other, _daemon) = served();
    let remote = fixture.dir.join("foreign-remote.git");
    git(&other, &["init", "-q", "--bare", remote.to_str().unwrap()]);
    git(
        &other,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );

    let output = real_gate(
        &fixture,
        &other,
        &["push", "-q", "origin", "HEAD:refs/heads/pushed"],
    );

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(succeeds(output), "{stderr}");
    assert!(!stderr.contains("Refused"), "{stderr}");
    assert!(!git(&remote, &["branch", "--list", "pushed"]).is_empty());
    // The verdict is in force all the while: a branch moving here is still refused.
    assert!(!succeeds(commit_empty(&fixture, &other)));
}

#[test]
fn a_daemon_that_cannot_be_reached_does_not_refuse_the_update() {
    let (fixture, other) = with_foreign_repository();
    fixture
        .port
        .store(ClosedPort::new().port, Ordering::Relaxed);

    let output = commit_empty(&fixture, &other);

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(succeeds(output), "{stderr}");
}

#[test]
fn a_daemon_error_is_reported_unless_the_user_asked_to_skip_the_gate() {
    let fixture = Fixture::new();
    set_revert_gate(&fixture, true);
    let repo = fixture.repo.to_path_buf();

    fixture.port.store(answering_with(500), Ordering::Relaxed);
    let output = commit_empty(&fixture, &repo);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(succeeds(output), "{stderr}");
    assert!(
        stderr.contains("the revert gate did not check this update"),
        "{stderr}"
    );

    fixture.port.store(answering_with(500), Ordering::Relaxed);
    let output = real_gate_command(
        &fixture,
        &repo,
        &["commit", "-q", "--allow-empty", "-m", "skipped"],
    )
    .env("REVERT_GATE_SKIP", "1")
    .output()
    .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(succeeds(output), "{stderr}");
    assert!(!stderr.contains("did not check"), "{stderr}");
}
