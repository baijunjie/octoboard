use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

mod target_branch;

use super::*;
use crate::browse::compare;
use crate::git::isolated::IsolatedGit;
use crate::session_git_hooks::fixture::{project, succeeds, Fixture};
use crate::test_support::{git, isolated_git_env, ScratchDir, PATIENCE, SYSTEM_GIT};
use crate::time::now_millis;

impl Fixture {
    fn env(&self) -> IsolatedGit {
        IsolatedGit::new(SYSTEM_GIT.to_string(), isolated_git_env())
    }

    fn list(&self) -> BranchList {
        compare::branches_in(
            &self.env(),
            &self.common_dir,
            &self.common_dir,
            true,
            &AtomicBool::new(false),
        )
        .unwrap()
    }

    fn reconcile_full(&self) -> Vec<BranchOrigin> {
        reconcile(&self.state, &self.common_dir, &self.list()).unwrap()
    }
}

// -- attribution --------------------------------------------------------------

/// Two sessions working in one repository at the same moment, which no timing could tell apart.
#[test]
fn branches_two_sessions_create_at_once_go_each_to_its_own() {
    let fixture = Fixture::new();
    std::thread::scope(|scope| {
        for session in ["a", "b"] {
            let fixture = &fixture;
            scope.spawn(move || {
                for n in 0..4 {
                    fixture.as_session(session, &["branch", &format!("{session}-{n}")]);
                }
            });
        }
    });

    fixture.deliver(8);

    let rows = fixture.rows();
    assert_eq!(rows.len(), 8);
    for row in rows {
        assert_eq!(
            Some(row.responsible_session.as_str()),
            row.branch.split('-').next()
        );
        assert_eq!(
            row.responsible_title,
            format!("Session {}", row.responsible_session)
        );
    }
}

/// A clone has the same commits, and a branch of the same name made there at the same moment would
/// match the project's own reflog: only the repository the report names tells them apart.
#[test]
fn a_report_about_a_repository_the_sessions_project_is_not_in_is_ignored() {
    let fixture = Fixture::new();
    let clone = ScratchDir::new("branch-origin-clone");
    git(
        &fixture.repo,
        &[
            "clone",
            "-q",
            &fixture.repo.to_string_lossy(),
            &clone.to_string_lossy(),
        ],
    );
    let output = fixture
        .session_git("a", &clone, &[])
        .args(["branch", "elsewhere"])
        .output()
        .unwrap();
    assert!(succeeds(output));
    git(&fixture.repo, &["branch", "elsewhere"]);

    fixture.deliver(1);

    assert!(fixture.rows().is_empty());
    let key = repo_key(&repository_of(&clone).unwrap());
    assert!(fixture.state.store.branch_origins(&key).unwrap().is_empty());
}

#[test]
fn a_created_branch_is_its_creating_sessions_to_work_on_from_the_time_of_its_creation() {
    let fixture = Fixture::new();
    fixture.as_session("a", &["branch", "topic"]);
    fixture.deliver(1);

    let row = fixture.rows().pop().unwrap();
    assert_eq!(row.responsible_session, "a");
    assert_eq!(row.responsible_since, row.created_at);
    assert_eq!(row.responsible_title, "Session a");
}

#[test]
fn a_handover_moves_the_responsible_session_only_from_the_expected_one() {
    let fixture = Fixture::new();
    fixture.as_session("a", &["branch", "mine"]);
    fixture.as_session("b", &["branch", "theirs"]);
    fixture.deliver(2);
    let before = fixture.rows();
    let names = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();

    let moved = hand_over(
        &fixture.state,
        &fixture.common_dir,
        &names(&["mine", "theirs", "absent"]),
        "a",
        "c",
        "Session c",
    )
    .unwrap();

    assert_eq!(moved.moved, names(&["mine"]));
    assert_eq!(moved.unmoved, names(&["theirs", "absent"]));
    for row in fixture.rows() {
        let old = before.iter().find(|o| o.branch == row.branch).unwrap();
        if row.branch == "mine" {
            assert_eq!(row.responsible_session, "c");
            assert_eq!(row.responsible_title, "Session c");
            assert!(row.responsible_since > old.responsible_since);
        } else {
            assert_eq!(row.responsible_session, old.responsible_session);
            assert_eq!(row.responsible_title, old.responsible_title);
            assert_eq!(row.responsible_since, old.responsible_since);
        }
    }
    // The branch is no longer `a`'s to give: a second handover from it moves nothing.
    let again = hand_over(
        &fixture.state,
        &fixture.common_dir,
        &names(&["mine"]),
        "a",
        "d",
        "Session d",
    )
    .unwrap();
    assert!(again.moved.is_empty());
}

#[test]
fn a_handover_keeps_the_first_characters_of_a_long_title() {
    let fixture = Fixture::new();
    fixture.as_session("a", &["branch", "mine"]);
    fixture.deliver(1);
    let title = format!("é{}", "x".repeat(100));

    hand_over(
        &fixture.state,
        &fixture.common_dir,
        &["mine".to_string()],
        "a",
        "c",
        &title,
    )
    .unwrap();

    let row = fixture.rows().pop().unwrap();
    assert_eq!(
        row.responsible_title,
        title.chars().take(80).collect::<String>()
    );
    assert_eq!(row.responsible_title.chars().count(), 80);
}

#[test]
fn a_renamed_branch_keeps_its_responsible_session_and_a_copy_gets_none() {
    let fixture = Fixture::new();
    fixture.as_session("a", &["branch", "old"]);
    fixture.deliver(1);
    hand_over(
        &fixture.state,
        &fixture.common_dir,
        &["old".to_string()],
        "a",
        "c",
        "Session c",
    )
    .unwrap();
    git(&fixture.repo, &["branch", "-m", "old", "new"]);

    let rows = fixture.reconcile_full();
    assert_eq!(fixture.responsible("old"), None);
    assert_eq!(fixture.responsible("new").as_deref(), Some("c"));
    let row = rows.iter().find(|row| row.branch == "new").unwrap();
    assert_eq!(row.responsible_title, "Session c");

    // git reports no transaction for a copy; a branch made after it shows the report would have
    // arrived by then.
    fixture.as_session("b", &["branch", "-c", "new", "copy"]);
    fixture.as_session("b", &["branch", "after"]);
    fixture.deliver(2);
    assert_eq!(fixture.responsible("copy"), None);
    assert_eq!(fixture.responsible("after").as_deref(), Some("b"));
    assert_eq!(fixture.reports(2).len(), 2);
}

#[test]
fn a_branch_made_again_under_the_same_name_does_not_inherit_the_attribution() {
    let fixture = Fixture::new();
    fixture.as_session("a", &["branch", "topic"]);
    fixture.deliver(1);
    assert_eq!(fixture.responsible("topic").as_deref(), Some("a"));

    // A different tip, so the new reflog's first entry cannot equal the old one even within the
    // same second.
    git(&fixture.repo, &["branch", "-q", "-D", "topic"]);
    git(
        &fixture.repo,
        &["commit", "-q", "--allow-empty", "-m", "next"],
    );
    git(&fixture.repo, &["branch", "topic"]);
    fixture.reconcile_full();

    assert_eq!(fixture.responsible("topic"), None);
}

/// A report that comes after its branch was deleted and made again by someone else is about a
/// branch that is no longer there.
#[test]
fn a_report_about_a_branch_since_made_again_is_ignored() {
    let fixture = Fixture::new();
    fixture.as_session("a", &["branch", "topic"]);
    let reports = fixture.reports(1);
    git(&fixture.repo, &["branch", "-q", "-D", "topic"]);
    git(
        &fixture.repo,
        &["commit", "-q", "--allow-empty", "-m", "next"],
    );
    git(&fixture.repo, &["branch", "topic"]);

    for (session, report) in reports {
        ref_created(&fixture.state, &session, &report);
    }

    assert!(fixture.rows().is_empty());
}

/// `git branch -f`, `checkout -B` and `update-ref` without an old value name none for a branch
/// that exists, just as a creation does.
#[test]
fn a_forced_update_of_an_existing_branch_is_not_a_creation() {
    let fixture = Fixture::new();
    git(
        &fixture.repo,
        &["commit", "-q", "--allow-empty", "-m", "next"],
    );
    git(&fixture.repo, &["branch", "theirs"]);
    fixture.as_session("a", &["branch", "-f", "theirs", "HEAD~"]);
    fixture.as_session("a", &["branch", "mine"]);

    fixture.deliver(1);

    assert_eq!(fixture.responsible("theirs"), None);
    assert_eq!(fixture.responsible("mine").as_deref(), Some("a"));
    assert_eq!(
        fixture.reports(1).len(),
        1,
        "the forced update was never reported as a creation"
    );
}

/// git reports no transaction for a rename or a copy today; should one ever be reported, the new
/// name's reflog says where the branch came from. And a branch whose reflog `git gc` has emptied
/// begins it again with whatever moves it next, which is no creation.
#[test]
fn a_branch_whose_reflog_does_not_begin_with_its_creation_has_no_birth() {
    let fixture = Fixture::new();
    let now = now_millis() / 1000;
    let rev = |rev: &str| {
        let out = String::from_utf8(git(&fixture.repo, &["rev-parse", rev])).unwrap();
        out.trim().to_string()
    };
    git(
        &fixture.repo,
        &["commit", "-q", "--allow-empty", "-m", "next"],
    );
    git(&fixture.repo, &["branch", "made"]);
    git(&fixture.repo, &["branch", "-m", "made", "renamed"]);
    git(&fixture.repo, &["branch", "-c", "renamed", "copied"]);
    git(&fixture.repo, &["branch", "expired"]);
    git(
        &fixture.repo,
        &["reflog", "expire", "--expire=all", "refs/heads/expired"],
    );
    git(&fixture.repo, &["branch", "-f", "expired", "HEAD~"]);
    git(&fixture.repo, &["branch", "fresh"]);

    let birth = |branch: &str, at: &str| birth_of(&fixture.common_dir, branch, &rev(at), now);
    assert_eq!(birth("renamed", "HEAD"), None);
    assert_eq!(birth("copied", "HEAD"), None);
    assert_eq!(birth("expired", "HEAD~"), None);
    assert!(birth("fresh", "HEAD").is_some());
}

// -- the record's lifetime ----------------------------------------------------

#[test]
fn a_row_goes_with_its_branch_but_only_on_a_full_listing() {
    let fixture = Fixture::new();
    fixture.as_session("a", &["branch", "gone"]);
    fixture.deliver(1);
    git(&fixture.repo, &["branch", "-q", "-D", "gone"]);

    let cut = BranchList {
        complete: false,
        ..fixture.list()
    };
    reconcile(&fixture.state, &fixture.common_dir, &cut).unwrap();
    assert!(
        fixture.responsible("gone").is_some(),
        "a cut listing proves nothing"
    );

    fixture.reconcile_full();
    assert!(fixture.responsible("gone").is_none());
}

#[test]
fn a_listing_older_than_the_last_record_does_not_remove_it() {
    let fixture = Fixture::new();
    // A reply listed before the branch was made, as a client's request that overlapped the report
    // carries.
    let stale = fixture.list();
    fixture.as_session("a", &["switch", "-q", "-c", "feature"]);
    fixture.deliver(1);

    let mut list = stale;
    annotate(&fixture.state, &fixture.common_dir, &mut list);

    assert!(
        fixture.responsible("feature").is_some(),
        "the branch is missing from the listing because it was made after it"
    );
}

#[test]
fn deleting_a_project_leaves_the_origins_of_another_in_the_same_repository() {
    let fixture = Fixture::new();
    let inner = fixture.repo.join("inner");
    std::fs::create_dir(&inner).unwrap();
    fixture
        .state
        .store
        .insert_project(&project("other", &inner))
        .unwrap();
    fixture.as_session("a", &["branch", "topic"]);
    fixture.deliver(1);

    fixture.state.store.delete_project("project").unwrap();
    project_deleted(&fixture.state, &fixture.repo.to_string_lossy());
    assert!(
        fixture.responsible("topic").is_some(),
        "another project is still in the repository"
    );

    fixture.state.store.delete_project("other").unwrap();
    project_deleted(&fixture.state, &inner.to_string_lossy());
    assert!(fixture.rows().is_empty(), "the last project's deletion");
}

/// The rows of a repository are written under its gate, a project's deletion included: a report
/// holding the gate has read the rows it is about to write, and would put back what the deletion
/// just removed — leaving a repository with rows, no project to clear them and nothing that runs
/// here for it again.
#[test]
fn deleting_a_project_waits_for_a_write_holding_the_gate() {
    let fixture = Fixture::new();
    fixture.as_session("a", &["branch", "topic"]);
    fixture.deliver(1);

    let gate = fixture.state.branch_origins.gate(&fixture.common_dir);
    let held = gate.lock().unwrap();
    let (done, deleted) = std::sync::mpsc::channel();
    let state = Arc::clone(&fixture.state);
    let path = fixture.repo.to_string_lossy().into_owned();
    let worker = std::thread::spawn(move || {
        state.store.delete_project("project").unwrap();
        project_deleted(&state, &path);
        done.send(()).unwrap();
    });

    // The worker is at the gate once it is past the store's deletion; before that, nothing has
    // been waited for.
    let start = std::time::Instant::now();
    while fixture
        .state
        .store
        .get_project("project")
        .unwrap()
        .is_some()
    {
        assert!(start.elapsed() < PATIENCE, "the worker never started");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(fixture.responsible("topic").is_some());
    assert!(
        deleted.try_recv().is_err(),
        "the deletion waits for the gate"
    );
    drop(held);

    deleted
        .recv_timeout(PATIENCE)
        .expect("the deletion goes on");
    worker.join().unwrap();
    assert!(fixture.rows().is_empty());
}

/// A late listing must not set the in-memory record of the repository up again either: nothing is
/// left to clear it, and it would be held until the daemon restarts.
#[test]
fn a_listing_after_the_last_project_went_leaves_no_record_of_the_repository() {
    let fixture = Fixture::new();
    let mut list = fixture.list();
    fixture.state.store.delete_project("project").unwrap();
    project_deleted(&fixture.state, &fixture.repo.to_string_lossy());
    assert!(fixture.state.branch_origins.holds_no_repository());

    annotate(&fixture.state, &fixture.common_dir, &mut list);

    assert!(fixture.state.branch_origins.holds_no_repository());
}

#[test]
fn rows_past_the_cap_are_dropped_oldest_first() {
    let fixture = Fixture::new();
    let key = repo_key(&fixture.common_dir);
    let put: Vec<BranchOrigin> = (0..5)
        .map(|n| BranchOrigin {
            repository: key.clone(),
            branch: format!("b{n}"),
            created_at: n,
            birth_commit: "c".to_string(),
            birth_time: n,
            responsible_session: "a".to_string(),
            responsible_since: n,
            responsible_title: "Session a".to_string(),
            target_branch: None,
        })
        .collect();
    fixture
        .state
        .store
        .apply_branch_origins(&key, &[], &put, 2)
        .unwrap();
    let kept: Vec<_> = fixture.rows().into_iter().map(|row| row.branch).collect();
    assert_eq!(kept, ["b3", "b4"]);
}
