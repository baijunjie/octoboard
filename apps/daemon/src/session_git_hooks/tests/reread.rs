//! The re-read of a project's branch header that a session's own git has the daemon make when a
//! committed transaction moves `HEAD`, a branch or a remote-tracking branch. The fixture's state
//! never arms the ref watch, so no file event takes part: a status published here is the hook's.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crate::protocol::{Event, GitStatus};
use crate::session_git_hooks::fixture::{
    parse_report, project, project_with_remote, Daemon, Fixture,
};
use crate::test_support::{git, PATIENCE, SYSTEM_GIT, TEST_KEY};

/// Posts each report the stand-in for `octoboardd git-ref` has received past the first `posted` to
/// the daemon, as the real one would, until the git statuses the daemon has published since `until`
/// was last satisfied satisfy it. Returns them.
fn reread_until(
    fixture: &Fixture,
    daemon: &Daemon,
    events: &mut tokio::sync::broadcast::Receiver<Event>,
    posted: &mut usize,
    until: impl Fn(&[GitStatus]) -> bool,
) -> Vec<GitStatus> {
    let deadline = Instant::now() + PATIENCE;
    let mut published = Vec::new();
    loop {
        let text = std::fs::read_to_string(&*fixture.reports).unwrap_or_default();
        for line in text.lines().skip(*posted) {
            let (session, report) = parse_report(line);
            assert!(report.moved, "every transaction here moves a ref: {line}");
            let body = serde_json::to_vec(&report).unwrap();
            crate::loopback::post(
                daemon.port,
                TEST_KEY,
                &format!("/git-ref/{session}"),
                &body,
                Duration::from_secs(5),
            )
            .unwrap();
            *posted += 1;
        }
        while let Ok(event) = events.try_recv() {
            if let Event::ProjectGitStatus { status } = event {
                published.push(status);
            }
        }
        if until(&published) {
            return published;
        }
        assert!(
            Instant::now() < deadline,
            "the projects' headers are re-read: {published:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The version of the git the tests run.
fn git_version() -> (u32, u32) {
    let output = std::process::Command::new(SYSTEM_GIT)
        .arg("--version")
        .output()
        .unwrap();
    crate::session_git_hooks::parse_version(&String::from_utf8_lossy(&output.stdout)).unwrap()
}

/// Whether `statuses` hold one of `project` that `wanted` accepts.
fn has(statuses: &[GitStatus], project: &str, wanted: impl Fn(&GitStatus) -> bool) -> bool {
    statuses
        .iter()
        .any(|status| status.project == project && wanted(status))
}

#[test]
fn a_sessions_commits_merges_and_switches_reread_the_projects() {
    let (fixture, _origin) = project_with_remote();
    git(
        &fixture.repo,
        &["branch", "-q", "-u", "origin/main", "main"],
    );
    let daemon = Daemon::start(&fixture.state);
    fixture.port.store(daemon.port, Ordering::Relaxed);
    let mut events = fixture.state.subscribe();
    let mut posted = 0;

    fixture.as_session("a", &["commit", "-q", "--allow-empty", "-m", "on main"]);
    let published = reread_until(&fixture, &daemon, &mut events, &mut posted, |s| {
        has(s, "project", |s| s.ahead == 1)
    });
    assert!(has(&published, "project", |s| {
        s.branch.as_deref() == Some("main") && s.upstream.as_deref() == Some("origin/main")
    }));

    // An agent's usual round: a branch in a linked worktree, which is a project of its own,
    // merged back into the project's checked-out branch by a fast-forward made from the project's
    // directory. A commit in the linked worktree moves a branch both share the refs of.
    let linked = fixture.dir.join("linked");
    fixture.as_session(
        "a",
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature",
            linked.to_str().unwrap(),
        ],
    );
    fixture
        .state
        .store
        .insert_project(&project("linked", &linked))
        .unwrap();
    git(&linked, &["branch", "-q", "-u", "origin/main", "feature"]);
    reread_until(&fixture, &daemon, &mut events, &mut posted, |s| {
        has(s, "linked", |s| s.branch.as_deref() == Some("feature")) && has(s, "project", |_| true)
    });
    for message in ["one", "two"] {
        let output = fixture
            .session_git("a", Path::new(&linked), &[])
            .args(["commit", "-q", "--allow-empty", "-m", message])
            .output()
            .unwrap();
        assert!(output.status.success());
    }
    // Both projects are re-read from the linked worktree's reports.
    reread_until(&fixture, &daemon, &mut events, &mut posted, |s| {
        has(s, "linked", |s| s.ahead == 3) && has(s, "project", |s| s.ahead == 1)
    });
    fixture.as_session("a", &["merge", "-q", "--ff-only", "feature"]);
    reread_until(&fixture, &daemon, &mut events, &mut posted, |s| {
        has(s, "project", |s| s.ahead == 3)
    });

    fixture.as_session("a", &["switch", "-q", "-c", "side"]);
    let published = reread_until(&fixture, &daemon, &mut events, &mut posted, |s| {
        has(s, "project", |s| s.branch.as_deref() == Some("side"))
    });
    assert!(has(&published, "project", |s| s.upstream.is_none()));
    // A switch to an existing branch updates `HEAD` alone, which git runs the hook for from 2.46.
    if git_version() < (2, 46) {
        return;
    }
    fixture.as_session("a", &["switch", "-q", "main"]);
    reread_until(&fixture, &daemon, &mut events, &mut posted, |s| {
        has(s, "project", |s| {
            s.branch.as_deref() == Some("main") && s.ahead == 3
        })
    });
}
