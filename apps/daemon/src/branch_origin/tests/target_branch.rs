//! The target branch and fork point recorded for a created branch, and how a rename and a handover
//! treat them.

use super::*;
use crate::session_git_hooks::fixture::{project_with_remote, rev};

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
