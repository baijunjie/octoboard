use super::*;
use crate::revert_gate::git::creation_point;
use crate::revert_gate::local::{fork_points_of, target_side};

#[test]
fn a_non_fast_forward_is_allowed_only_when_it_drops_no_published_commit() {
    let origin = base_repo();
    let local = clone_of(&origin);
    commit(
        &origin,
        "other.txt",
        &numbered("other", 1..=9),
        "published elsewhere",
    );
    commit(
        &local,
        "a.txt",
        &numbered("alpha", 2..=10),
        "local, unpushed",
    );
    git(&local, &["fetch", "-q"]);

    // `git rebase origin/main` replays the one local commit on the latest remote commit.
    let (old, new) = prepared(&local, &["rebase", "origin/main"]);
    assert_eq!(main_move(&local, &[], &old, &new), None);

    git(&local, &["reset", "-q", "--hard", "origin/main"]);
    let (old, new) = prepared(&local, &["reset", "-q", "--hard", "HEAD~1"]);
    let rejected = main_move(&local, &[], &old, &new).expect("a published commit is dropped");
    assert!(
        rejected.contains("it would drop published commits"),
        "{rejected}"
    );
}

#[test]
fn aligning_with_the_remote_drops_no_unpushed_commit_and_is_allowed_outright() {
    let origin = base_repo();
    git(&origin, &["switch", "-q", "-c", "other"]);
    commit(
        &origin,
        "a.txt",
        &numbered("beta", 1..=10),
        "published on another branch",
    );
    git(&origin, &["switch", "-q", "main"]);
    commit(
        &origin,
        "other.txt",
        &numbered("other", 1..=9),
        "published on main",
    );
    let local = clone_of(&origin);
    git(&local, &["reset", "-q", "--hard", "origin/other"]);

    // The one commit this drops is on the remote already, and the move lands on `origin/main`.
    let (old, new) = prepared(&local, &["reset", "-q", "--hard", "origin/main"]);
    assert_eq!(main_move(&local, &[], &old, &new), None);

    // Without a remote-tracking branch at its end it rewrites published history.
    git(&local, &["update-ref", "-d", "refs/remotes/origin/main"]);
    assert!(main_move(&local, &[], &old, &new).is_some());
}

#[test]
fn an_unpushed_commit_kept_on_another_branch_is_not_dropped_unless_the_branch_is_in_use() {
    let origin = base_repo();
    let local = clone_of(&origin);
    let upstream = rev(&local, "origin/main");
    commit(
        &local,
        "a.txt",
        &numbered("beta", 1..=5),
        "local rewrite of the head",
    );
    let old = rev(&local, "HEAD");
    git(&local, &["switch", "-q", "-c", "keep"]);

    // `main` is checked out nowhere, and `keep` still holds the commit the move drops.
    assert_eq!(main_move(&local, &[], &old, &upstream), None);

    git(&local, &["switch", "-q", "main"]);
    assert!(main_move(&local, &[], &old, &upstream).is_some());
}

#[test]
fn a_squash_with_a_changed_base_is_rejected_with_and_without_a_recorded_fork_point() {
    let (repo, fork, old, new) = squash_with_changed_base(|repo| {
        commit(
            repo,
            "a.txt",
            &numbered("beta", 1..=5),
            "rewrite the head of a.txt",
        )
    });
    let found = main_move(&repo, &[], &old, &new).expect("rejected by content");
    assert!(found.contains("rewrite the head of a.txt"), "{found}");
    assert!(
        found.contains("git rebase -i $(git merge-base HEAD <target branch>)"),
        "{found}"
    );
    assert!(
        !found.contains("git reset --soft $(git merge-base"),
        "{found}"
    );
    assert!(main_move(&repo, &[("feature", &fork)], &old, &new).is_some());
}

#[test]
fn the_reflog_gives_the_fork_point_of_a_branch_with_no_record() {
    let (repo, fork, old, new) = squash_with_changed_base(|repo| {
        commit(
            repo,
            "added.txt",
            &numbered("added", 1..=8),
            "add added.txt",
        )
    });
    // The branch never touched `added.txt`, which the changed base drags out of `main`: found from
    // the commit the branch was created at, without a record.
    let found = main_move(&repo, &[], &old, &new).expect("changed base");
    assert!(
        found.contains("added.txt: this file was deleted wholesale"),
        "{found}"
    );
    let creation = creation_point(&git_in(&repo), "feature").unwrap();
    assert_eq!(creation, Some(fork));
    // A branch whose oldest reflog entry is not its creation has none.
    forget_reflog(&repo, "feature");
    git(
        &repo,
        &[
            "update-ref",
            "-m",
            "reset: moving to x",
            "refs/heads/feature",
            &new,
        ],
    );
    assert_eq!(creation_point(&git_in(&repo), "feature").unwrap(), None);
}

#[test]
fn a_wiped_out_additive_commit_is_caught_only_through_a_fork_point_recorded_or_from_the_reflog() {
    let (repo, fork, old, new) = squash_with_changed_base(|repo| {
        commit(
            repo,
            "added.txt",
            &numbered("added", 1..=8),
            "add added.txt",
        )
    });
    // No line is deleted by the commit, so no content criterion sees it undone, and a branch
    // whose reflog is gone has no fork point to find the changed base from.
    forget_reflog(&repo, "feature");
    assert_eq!(main_move(&repo, &[], &old, &new), None);
    let found = main_move(&repo, &[("feature", &fork)], &old, &new).expect("changed base");
    assert!(
        found.contains("added.txt: this file was deleted wholesale"),
        "{found}"
    );
    // `old` is the additive commit itself; declaring its revert lets the same move through.
    let message = format!("add the feature, squashed\n\nReverts: {old}");
    let declared = git(
        &repo,
        &[
            "commit-tree",
            &format!("{new}^{{tree}}"),
            "-p",
            &format!("{new}^"),
            "-m",
            &message,
        ],
    );
    let declared = String::from_utf8(declared).unwrap().trim().to_string();
    git(&repo, &["branch", "-f", "feature", &declared]);
    assert_eq!(
        main_move(&repo, &[("feature", &fork)], &old, &declared),
        None
    );
}

#[test]
fn a_conflict_resolution_keeping_only_the_local_side_undoes_a_published_commit() {
    let origin = base_repo();
    let local = clone_of(&origin);
    let mut upstream = numbered("alpha", 1..=1);
    upstream += &numbered("beta", 2..=5);
    upstream += &numbered("alpha", 6..=10);
    commit(&origin, "a.txt", &upstream, "rewrite lines 2 to 5");
    let mut mine = numbered("alpha", 1..=4);
    mine += &numbered("gamma", 5..=7);
    mine += &numbered("alpha", 8..=10);
    commit(&local, "a.txt", &mine, "rewrite lines 5 to 7");
    git(&local, &["fetch", "-q"]);

    let old = rev(&local, "HEAD");
    let conflicted = crate::test_support::fixture_git(&local)
        .args(["rebase", "origin/main"])
        .output()
        .unwrap();
    assert!(!conflicted.status.success(), "the rebase must conflict");
    git(&local, &["checkout", "--theirs", "a.txt"]);
    git(&local, &["add", "a.txt"]);
    crate::test_support::fixture_git(&local)
        .env("GIT_EDITOR", "true")
        .args(["rebase", "--continue"])
        .output()
        .unwrap();
    let new = rev(&local, "HEAD");
    git(&local, &["reset", "-q", "--hard", &old]);

    let found = main_move(&local, &[], &old, &new).expect("the published commit is undone");
    assert!(
        found.contains("Against main on the remote-tracking branch (published commits):")
            && found.contains("rewrite lines 2 to 5")
            && found.contains("Most likely the wrong side was kept"),
        "{found}"
    );
}

#[test]
fn a_declared_revert_is_allowed_where_the_same_move_without_it_is_rejected() {
    let repo = base_repo();
    let target = commit(
        &repo,
        "a.txt",
        &numbered("beta", 1..=5),
        "rewrite the head of a.txt",
    );
    let original = numbered("alpha", 1..=10);
    git(&repo, &["switch", "-q", "-c", "plain"]);
    let plain = commit(&repo, "a.txt", &original, "undo it");
    git(&repo, &["switch", "-q", "-c", "declared", "main"]);
    let message = format!("undo it\n\nReverts: {}", &target[..12]);
    let declared = commit(&repo, "a.txt", &original, &message);
    git(&repo, &["switch", "-q", "main"]);

    assert!(main_move(&repo, &[], &target, &plain).is_some());
    assert_eq!(main_move(&repo, &[], &target, &declared), None);
}

#[test]
fn taking_back_what_a_commit_only_added_is_allowed_even_when_the_file_returns_exactly() {
    let repo = base_repo();
    let original = numbered("alpha", 1..=10);
    let added = commit(
        &repo,
        "a.txt",
        &(original.clone() + &numbered("added", 1..=8)),
        "append to a.txt",
    );
    // The file goes back byte for byte to how it was before the commit, yet the commit deleted
    // no line, so nothing it took away comes back: removing what was only added is ordinary.
    std::fs::write(repo.join("a.txt"), &original).unwrap();
    git(&repo, &["add", "-A"]);
    let (_, removed) = prepared(&repo, &["commit", "-q", "-m", "drop the appended lines"]);
    assert_eq!(main_move(&repo, &[], &added, &removed), None);

    // The same return to the earlier file is refused when the commit also deleted lines.
    let mut rewritten = numbered("beta", 1..=4);
    rewritten += &numbered("alpha", 5..=10);
    let rewrote = commit(
        &repo,
        "a.txt",
        &(rewritten + &numbered("added", 1..=8)),
        "rewrite a.txt",
    );
    std::fs::write(
        repo.join("a.txt"),
        numbered("alpha", 1..=10) + &numbered("added", 1..=8),
    )
    .unwrap();
    git(&repo, &["add", "-A"]);
    let (_, restored) = prepared(&repo, &["commit", "-q", "-m", "restore a.txt"]);
    assert!(main_move(&repo, &[], &rewrote, &restored).is_some());
}

#[test]
fn an_ordinary_fast_forward_merge_is_allowed() {
    let repo = base_repo();
    let fork = rev(&repo, "HEAD");
    git(&repo, &["switch", "-q", "-c", "feature"]);
    commit(
        &repo,
        "feature.txt",
        &numbered("feature", 1..=4),
        "add the feature",
    );
    commit(&repo, "a.txt", &numbered("alpha", 1..=9), "trim a.txt");
    let new = rev(&repo, "HEAD");
    assert_eq!(main_move(&repo, &[("feature", &fork)], &fork, &new), None);
}

/// A repository whose unguarded `topic` carries `rewrite the head of a.txt`, a commit of its own.
/// Returns the repository and topic's tip.
fn topic_with_a_change() -> (ScratchDir, String) {
    let repo = base_repo();
    git(&repo, &["switch", "-q", "-c", "topic"]);
    let tip = commit(
        &repo,
        "a.txt",
        &numbered("beta", 1..=5),
        "rewrite the head of a.txt",
    );
    (repo, tip)
}

/// The verdict on the unguarded `branch` moving from `old` to `new`.
fn unguarded_move(repo: &Path, branch: &str, old: &str, new: &str) -> Option<String> {
    gate(repo, &[], &[], "prepared", branch, old, new)
}

#[test]
fn a_fast_forward_merge_into_an_unguarded_branch_is_checked_by_content() {
    let (repo, old) = topic_with_a_change();
    git(&repo, &["switch", "-q", "-c", "undoing"]);
    let undoing = commit(&repo, "a.txt", &numbered("alpha", 1..=10), "undo it");
    git(&repo, &["switch", "-q", "-c", "adding", &old]);
    let adding = commit(
        &repo,
        "added.txt",
        &numbered("added", 1..=8),
        "add added.txt",
    );

    let found = unguarded_move(&repo, "topic", &old, &undoing).expect("topic's commit is undone");
    assert!(found.contains("rewrite the head of a.txt"), "{found}");
    assert_eq!(unguarded_move(&repo, "topic", &old, &adding), None);
}

#[test]
fn a_merge_commit_into_an_unguarded_branch_is_checked_by_content() {
    let (repo, old) = topic_with_a_change();
    git(&repo, &["switch", "-q", "-c", "feature", "main"]);
    let feature = commit(
        &repo,
        "other.txt",
        &numbered("other", 1..=9),
        "rewrite other.txt",
    );
    // A merge that keeps only the other side's content drops what topic's commit did.
    let dropping = git(
        &repo,
        &[
            "commit-tree",
            &format!("{feature}^{{tree}}"),
            "-p",
            &old,
            "-p",
            &feature,
            "-m",
            "merge feature",
        ],
    );
    let dropping = String::from_utf8(dropping).unwrap().trim().to_string();
    git(&repo, &["switch", "-q", "topic"]);
    let (_, merged) = prepared(&repo, &["merge", "-q", "--no-ff", "--no-edit", "feature"]);

    let found = unguarded_move(&repo, "topic", &old, &dropping).expect("topic's commit is undone");
    assert!(found.contains("rewrite the head of a.txt"), "{found}");
    assert_eq!(unguarded_move(&repo, "topic", &old, &merged), None);

    // The merged side can be a remote-tracking branch alone.
    git(
        &repo,
        &["update-ref", "refs/remotes/origin/feature", &feature],
    );
    git(&repo, &["branch", "-q", "-D", "feature"]);
    assert!(unguarded_move(&repo, "topic", &old, &dropping).is_some());
    assert_eq!(unguarded_move(&repo, "topic", &old, &merged), None);

    // With no branch at all at the second parent it is not a merge of a branch.
    git(&repo, &["update-ref", "-d", "refs/remotes/origin/feature"]);
    assert_eq!(unguarded_move(&repo, "topic", &old, &dropping), None);
}

#[test]
fn ordinary_moves_of_an_unguarded_branch_that_undo_its_own_commit_are_allowed() {
    let (repo, old) = topic_with_a_change();
    let original = numbered("alpha", 1..=10);
    std::fs::write(repo.join("a.txt"), &original).unwrap();
    git(&repo, &["add", "-A"]);
    let (_, committed) = prepared(&repo, &["commit", "-q", "-m", "undo it"]);
    assert_eq!(unguarded_move(&repo, "topic", &old, &committed), None);

    // Amending, resetting to where `main` is and rebasing the commit away all undo it as well.
    std::fs::write(repo.join("a.txt"), &original).unwrap();
    git(&repo, &["add", "-A"]);
    git(
        &repo,
        &["commit", "-q", "--amend", "--allow-empty", "-m", "amended"],
    );
    let amended = rev(&repo, "HEAD");
    git(&repo, &["reset", "-q", "--hard", &old]);
    assert_eq!(unguarded_move(&repo, "topic", &old, &amended), None);

    let (_, reset) = prepared(&repo, &["reset", "-q", "--hard", "main"]);
    assert_eq!(unguarded_move(&repo, "topic", &old, &reset), None);
    let (_, rebased) = prepared(&repo, &["rebase", "-q", "--onto", "main", "HEAD~1"]);
    assert_eq!(unguarded_move(&repo, "topic", &old, &rebased), None);
}

#[test]
fn a_deletion_or_no_op_update_of_an_unguarded_branch_runs_no_git() {
    // A program that does not exist fails any `git` run: deleting and no-op updates of an
    // unguarded branch are allowed before one starts.
    let nowhere = ScratchDir::new("revert-gate-nowhere");
    let git = Git::with("/nonexistent/git", nowhere.to_path_buf(), Vec::new());
    let inputs = Inputs::new(&GateInputs::default());
    let (some, other, none) = ("1".repeat(40), "2".repeat(40), "0".repeat(40));
    let stdin = format!(
        "{some} {none} refs/heads/b\n{some} {some} refs/heads/c\n\
         {some} ref:refs/heads/x refs/heads/d\n{some} {other} refs/remotes/origin/e\n"
    );
    assert_eq!(
        decide(&git, &inputs, "prepared", stdin.as_bytes()).unwrap(),
        None
    );
    // The controls: a move of it does run one, and so does an update with no expected old value,
    // which has to be read from the branch.
    let stdin = format!("{some} {other} refs/heads/a\n");
    assert!(decide(&git, &inputs, "prepared", stdin.as_bytes()).is_err());
    let stdin = format!("{none} {other} refs/heads/a\n");
    assert!(decide(&git, &inputs, "prepared", stdin.as_bytes()).is_err());
}

/// `commit-tree` of `tree` with `parents`.
fn commit_tree(repo: &Path, tree: &str, parents: &[&str]) -> String {
    let mut args = vec!["commit-tree".to_string(), tree.to_string()];
    for parent in parents {
        args.extend(["-p".to_string(), parent.to_string()]);
    }
    args.extend(["-m".to_string(), "merge".to_string()]);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    String::from_utf8(git(repo, &args))
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn a_branch_stacked_on_unmerged_work_has_no_fork_point_from_its_creation() {
    // `stacked` was created on the tip of `first`, which `topic` does not have: its creation point
    // is not where it left `topic`, and `first`'s edit of a.txt is not its own.
    let repo = base_repo();
    git(&repo, &["switch", "-q", "-c", "first"]);
    commit(&repo, "a.txt", &numbered("alpha", 1..=9), "edit a.txt");
    git(&repo, &["switch", "-q", "-c", "stacked"]);
    let stacked = commit(&repo, "g.txt", &numbered("stacked", 1..=4), "add g.txt");
    git(&repo, &["switch", "-q", "-c", "topic", "main"]);
    let old = rev(&repo, "HEAD");

    assert_eq!(unguarded_move(&repo, "topic", &old, &stacked), None);
    assert_eq!(
        gate(&repo, &["main"], &[], "prepared", "main", &old, &stacked),
        None
    );
}

#[test]
fn a_branch_created_from_a_remote_tracking_branch_is_pushed_after_a_faithful_rebase() {
    let origin = base_repo();
    git(&origin, &["switch", "-q", "-c", "colleague"]);
    commit(
        &origin,
        "other.txt",
        &numbered("other", 1..=5),
        "colleague edits other.txt",
    );
    git(&origin, &["switch", "-q", "main"]);
    commit(
        &origin,
        "a.txt",
        &numbered("alpha", 1..=9),
        "main edits a.txt",
    );
    let local = clone_of(&origin);
    // `git switch colleague` creates it from `origin/colleague`: not from the target branch.
    git(&local, &["switch", "-q", "colleague"]);
    git(&local, &["rebase", "-q", "origin/main"]);
    let new = rev(&local, "HEAD");

    let verdict = push(
        &local,
        &["main"],
        &[],
        &[],
        "colleague",
        &new,
        &"0".repeat(40),
    );
    assert!(
        !verdict.rejected && verdict.text.is_empty(),
        "{}",
        verdict.text
    );
}

#[test]
fn a_clean_merge_of_a_branch_started_on_unmerged_work_is_allowed() {
    let repo = base_repo();
    git(&repo, &["switch", "-q", "-c", "feature_one"]);
    commit(
        &repo,
        "other.txt",
        &numbered("other", 1..=5),
        "edit other.txt",
    );
    git(&repo, &["switch", "-q", "-c", "colleague"]);
    commit(&repo, "c.txt", &numbered("colleague", 1..=4), "add c.txt");
    git(&repo, &["switch", "-q", "-c", "topic", "main"]);
    let old = commit(
        &repo,
        "a.txt",
        &numbered("alpha", 1..=9),
        "topic's own work",
    );
    let (_, merged) = prepared(&repo, &["merge", "-q", "--no-ff", "--no-edit", "colleague"]);
    assert_eq!(unguarded_move(&repo, "topic", &old, &merged), None);
}

/// A repository where `main` rewrote a.txt, a feature branch was created, and `main` then backed
/// it out without a `Reverts:` line. Returns the repository, the feature's tip, and the merge of
/// `main` into it.
fn feature_merging_a_main_that_backed_out_a_commit() -> (ScratchDir, String, String) {
    let repo = base_repo();
    commit(
        &repo,
        "a.txt",
        &numbered("beta", 1..=5),
        "rewrite the head of a.txt",
    );
    git(&repo, &["switch", "-q", "-c", "feature"]);
    let old = commit(
        &repo,
        "f.txt",
        &numbered("feature", 1..=4),
        "add the feature",
    );
    git(&repo, &["switch", "-q", "main"]);
    commit(&repo, "a.txt", &numbered("alpha", 1..=10), "back it out");
    git(&repo, &["switch", "-q", "feature"]);
    let (_, merged) = prepared(&repo, &["merge", "-q", "--no-ff", "--no-edit", "main"]);
    (repo, old, merged)
}

#[test]
fn merging_the_target_or_a_guarded_branch_into_a_feature_branch_is_not_checked() {
    let (repo, old, merged) = feature_merging_a_main_that_backed_out_a_commit();
    // Nothing says `main` is special: the merge undoes a commit the feature has.
    assert!(unguarded_move(&repo, "feature", &old, &merged).is_some());

    let guarded = |repo: &Path| gate(repo, &["main"], &[], "prepared", "feature", &old, &merged);
    assert_eq!(guarded(&repo), None);
    let targeting = GateInputs {
        targets: vec![("feature".to_string(), "main".to_string())],
        ..GateInputs::default()
    };
    // The target is guarded as the target of this branch, when the daemon says so.
    assert_eq!(
        decide_with(&repo, &targeting, "prepared", "feature", &old, &merged),
        None
    );
    // As a remote-tracking branch alone.
    let main = rev(&repo, "main");
    git(&repo, &["update-ref", "refs/remotes/origin/main", &main]);
    git(&repo, &["branch", "-q", "-D", "main"]);
    assert_eq!(guarded(&repo), None);
    assert_eq!(
        decide_with(&repo, &targeting, "prepared", "feature", &old, &merged),
        None
    );
}

#[test]
fn a_fast_forward_to_the_target_is_not_checked_and_one_to_the_own_remote_branch_drops_nothing() {
    let (repo, old) = topic_with_a_change();
    git(&repo, &["switch", "-q", "-c", "scratch"]);
    let undoing = commit(&repo, "a.txt", &numbered("alpha", 1..=10), "undo it");
    git(&repo, &["switch", "-q", "topic"]);
    // `git rebase main` on a topic that has not moved fast-forwards it to `main`'s tip; what
    // lands on `main` is checked when it does.
    git(&repo, &["branch", "-q", "-f", "main", &undoing]);
    git(&repo, &["branch", "-q", "-D", "scratch"]);
    assert!(unguarded_move(&repo, "topic", &old, &undoing).is_some());
    assert_eq!(
        gate(&repo, &["main"], &[], "prepared", "topic", &old, &undoing),
        None
    );
    let targeting = GateInputs {
        targets: vec![("topic".to_string(), "main".to_string())],
        ..GateInputs::default()
    };
    assert_eq!(
        decide_with(&repo, &targeting, "prepared", "topic", &old, &undoing),
        None
    );

    // A fast-forward `git pull`: the tip is the topic's own remote-tracking branch's, which
    // holds everything topic does.
    git(
        &repo,
        &["update-ref", "refs/remotes/origin/topic", &undoing],
    );
    git(&repo, &["branch", "-q", "-D", "main"]);
    assert_eq!(unguarded_move(&repo, "topic", &old, &undoing), None);
}

#[test]
fn a_pull_merge_that_drops_what_was_pushed_to_the_own_remote_branch_is_checked() {
    let (repo, old) = topic_with_a_change();
    git(&repo, &["switch", "-q", "-c", "theirs", "main"]);
    let theirs = commit(
        &repo,
        "other.txt",
        &numbered("other", 1..=9),
        "pushed by someone",
    );
    git(&repo, &["update-ref", "refs/remotes/origin/topic", &theirs]);
    git(&repo, &["switch", "-q", "topic"]);
    git(&repo, &["branch", "-q", "-D", "theirs"]);
    // A conflict resolution that keeps only the local side.
    let dropping = commit_tree(&repo, &format!("{old}^{{tree}}"), &[&old, &theirs]);
    let found = unguarded_move(&repo, "topic", &old, &dropping).expect("theirs is dropped");
    assert!(found.contains("pushed by someone"), "{found}");
    let (_, pulled) = prepared(&repo, &["merge", "-q", "--no-ff", "--no-edit", &theirs]);
    assert_eq!(unguarded_move(&repo, "topic", &old, &pulled), None);
}

#[test]
fn an_update_with_no_expected_old_value_is_read_from_the_branch() {
    let (repo, _) = topic_with_a_change();
    git(&repo, &["switch", "-q", "-c", "colleague"]);
    let undoing = commit(&repo, "a.txt", &numbered("alpha", 1..=10), "undo it");
    git(&repo, &["switch", "-q", "topic"]);
    // `git branch -f topic colleague` arrives with all zeros as the old value.
    let stdin = format!("{} {undoing} refs/heads/topic\n", "0".repeat(40));
    let inputs = Inputs::new(&GateInputs::default());
    let found = decide(&git_in(&repo), &inputs, "prepared", stdin.as_bytes())
        .unwrap()
        .expect("topic's commit is undone");
    assert!(found.contains("rewrite the head of a.txt"), "{found}");
    // A creation resolves to nothing, and is not checked.
    let stdin = format!("{} {undoing} refs/heads/fresh\n", "0".repeat(40));
    let allowed = decide(&git_in(&repo), &inputs, "prepared", stdin.as_bytes()).unwrap();
    assert_eq!(allowed, None);
}

#[test]
fn a_creation_point_ahead_of_the_local_branch_but_on_its_remote_is_a_fork_point() {
    let origin = base_repo();
    let local = clone_of(&origin);
    commit(&origin, "a.txt", &numbered("alpha", 1..=9), "main moves on");
    git(&local, &["fetch", "-q"]);
    // `main` is behind `origin/main`, and `feat` is created from the latter.
    git(&local, &["switch", "-q", "-c", "feat", "origin/main"]);
    let new = commit(&local, "g.txt", &numbered("g", 1..=4), "add g.txt");
    let creation = rev(&local, "origin/main");
    let main = rev(&local, "main");
    let inputs = Inputs::new(&GateInputs::default());
    let git = git_in(&local);
    let feat = ["feat".to_string()];
    assert!(
        fork_points_of(&git, &inputs, &feat, &|| Ok(vec![main.clone()]))
            .unwrap()
            .is_empty()
    );
    let within = target_side(&git, std::slice::from_ref(&creation), &main, &new).unwrap();
    assert_eq!(
        fork_points_of(&git, &inputs, &feat, &|| Ok(within.clone())).unwrap(),
        [creation]
    );
    assert_eq!(
        gate(&local, &["main"], &[], "prepared", "main", &main, &new),
        None
    );
}

/// `main` moves on from `daemon` with three commits; `tags`, at `main`'s tip, records the second
/// as its fork point. Returns the repository, `daemon`'s tip, the second commit (after
/// `daemon`'s tip) and `main`'s tip.
fn fork_after_old() -> (ScratchDir, String, String, String) {
    let repo = base_repo();
    let old = rev(&repo, "HEAD");
    git(&repo, &["branch", "-q", "daemon"]);
    commit(&repo, "a.txt", &numbered("alpha", 1..=9), "trim a.txt");
    let fork = commit(
        &repo,
        "other.txt",
        &numbered("other", 1..=4),
        "trim other.txt",
    );
    let tip = commit(&repo, "tags.txt", &numbered("tags", 1..=4), "add tags.txt");
    git(&repo, &["branch", "-q", "tags"]);
    (repo, old, fork, tip)
}

#[test]
fn a_fast_forward_to_a_branch_whose_fork_point_lies_after_the_old_value_is_allowed() {
    let (repo, old, fork, tip) = fork_after_old();
    let forks = [("tags", fork.as_str())];
    assert_eq!(
        gate(&repo, &["main"], &forks, "prepared", "daemon", &old, &tip),
        None
    );
}

#[test]
fn a_fork_point_at_the_old_value_does_not_hide_a_changed_base_of_another_merged_branch() {
    let repo = base_repo();
    let fork = rev(&repo, "HEAD");
    git(&repo, &["switch", "-q", "-c", "feature"]);
    commit(
        &repo,
        "feature.txt",
        &numbered("feature", 1..=4),
        "add the feature",
    );
    git(&repo, &["switch", "-q", "-c", "topic", &fork]);
    let old = commit(
        &repo,
        "added.txt",
        &numbered("added", 1..=8),
        "add added.txt",
    );
    // `feature` squashed onto `topic` after `git reset --soft topic`: `added.txt` is wiped out.
    git(&repo, &["switch", "-q", "feature"]);
    git(&repo, &["reset", "-q", "--soft", "topic"]);
    git(&repo, &["commit", "-q", "-m", "add the feature, squashed"]);
    let new = rev(&repo, "HEAD");
    git(&repo, &["branch", "-q", "tags"]);
    git(&repo, &["switch", "-q", "topic"]);
    let found = gate(
        &repo,
        &[],
        &[("feature", &fork), ("tags", &old)],
        "prepared",
        "topic",
        &old,
        &new,
    )
    .expect("added.txt is wiped out");
    assert!(
        found.contains("added.txt: this file was deleted wholesale"),
        "{found}"
    );
}

#[test]
fn a_fast_forward_to_a_remote_tracking_branch_of_someone_else_is_checked() {
    let (repo, old) = topic_with_a_change();
    git(&repo, &["switch", "-q", "-c", "scratch"]);
    let undoing = commit(&repo, "a.txt", &numbered("alpha", 1..=10), "undo it");
    git(&repo, &["switch", "-q", "topic"]);
    git(
        &repo,
        &["update-ref", "refs/remotes/origin/colleague", &undoing],
    );
    git(&repo, &["branch", "-q", "-D", "scratch"]);
    let found = unguarded_move(&repo, "topic", &old, &undoing).expect("topic's commit is undone");
    assert!(found.contains("rewrite the head of a.txt"), "{found}");
}

#[test]
fn an_octopus_merge_is_checked_by_each_of_its_merged_branches() {
    let (repo, old) = topic_with_a_change();
    for (name, file) in [("one", "one.txt"), ("two", "two.txt")] {
        git(&repo, &["switch", "-q", "-c", name, "main"]);
        commit(&repo, file, &numbered(name, 1..=4), "add a file");
    }
    git(&repo, &["switch", "-q", "topic"]);
    // The merge keeps only the second branch's tree, which has none of topic's work.
    let dropping = commit_tree(&repo, "two^{tree}", &[&old, "one", "two"]);
    let found = unguarded_move(&repo, "topic", &old, &dropping).expect("topic's commit is undone");
    assert!(found.contains("rewrite the head of a.txt"), "{found}");
    let (_, merged) = prepared(&repo, &["merge", "-q", "one", "two"]);
    assert_eq!(unguarded_move(&repo, "topic", &old, &merged), None);
}

#[test]
fn a_guarded_branch_cut_from_another_branchs_remote_is_not_published_there() {
    // `release` was cut from `origin/main`, which is its upstream; `origin/main` then took the
    // hotfix and backed it out by hand. Pulling that into `release` is not aligning with its own.
    let repo = base_repo();
    git(&repo, &["switch", "-q", "-c", "release"]);
    let old = commit(&repo, "a.txt", &numbered("beta", 1..=5), "hotfix");
    git(&repo, &["switch", "-q", "-c", "scratch"]);
    let backed_out = commit(
        &repo,
        "a.txt",
        &numbered("alpha", 1..=10),
        "back out by hand",
    );
    git(
        &repo,
        &["update-ref", "refs/remotes/origin/main", &backed_out],
    );
    git(&repo, &["switch", "-q", "release"]);
    git(&repo, &["branch", "-q", "-D", "scratch"]);
    for (key, value) in [
        ("remote.origin.url", "."),
        ("remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*"),
        ("branch.release.remote", "origin"),
        ("branch.release.merge", "refs/heads/main"),
    ] {
        git(&repo, &["config", key, value]);
    }
    let found = gate(
        &repo,
        &["main", "release"],
        &[],
        "prepared",
        "release",
        &old,
        &backed_out,
    )
    .expect("the hotfix is backed out");
    assert!(found.contains("hotfix"), "{found}");
}

#[test]
fn a_signed_merge_is_read_whatever_log_shows_of_signatures() {
    let (repo, old) = topic_with_a_change();
    let keys = ScratchDir::new("revert-gate-keys");
    let key = keys.join("key");
    let made = std::process::Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", ""])
        .arg("-f")
        .arg(&key)
        .output();
    if !made.is_ok_and(|made| made.status.success()) {
        eprintln!("skipped: no ssh-keygen to sign with");
        return;
    }
    git(&repo, &["config", "gpg.format", "ssh"]);
    git(
        &repo,
        &[
            "config",
            "user.signingkey",
            key.with_extension("pub").to_str().unwrap(),
        ],
    );
    // With this `git log` prints a line about each signed commit before its format.
    git(&repo, &["config", "log.showSignature", "true"]);
    git(&repo, &["switch", "-q", "-c", "feature", "main"]);
    let feature = commit(
        &repo,
        "other.txt",
        &numbered("other", 1..=9),
        "rewrite other.txt",
    );
    git(&repo, &["switch", "-q", "topic"]);
    let signed = git(
        &repo,
        &[
            "commit-tree",
            "-S",
            &format!("{feature}^{{tree}}"),
            "-p",
            &old,
            "-p",
            &feature,
            "-m",
            "merge",
        ],
    );
    let signed = String::from_utf8(signed).unwrap().trim().to_string();
    let found = unguarded_move(&repo, "topic", &old, &signed).expect("topic's commit is dropped");
    assert!(found.contains("rewrite the head of a.txt"), "{found}");
}

#[test]
fn a_remote_tracking_tip_the_move_does_not_contain_is_not_the_target_side() {
    // `origin/topic` already merged `colleague`, which was created from `origin/colleague`; the
    // local `topic` is behind it and merges `colleague` on its own.
    let (repo, old) = topic_with_a_change();
    git(&repo, &["switch", "-q", "-c", "source", "main"]);
    let colleague = commit(
        &repo,
        "other.txt",
        &numbered("other", 1..=9),
        "colleague edits other",
    );
    git(
        &repo,
        &["update-ref", "refs/remotes/origin/colleague", &colleague],
    );
    git(&repo, &["switch", "-q", "topic"]);
    git(&repo, &["branch", "-q", "-D", "source"]);
    git(
        &repo,
        &["branch", "-q", "colleague", "refs/remotes/origin/colleague"],
    );
    let (_, merged) = prepared(&repo, &["merge", "-q", "--no-ff", "--no-edit", "colleague"]);
    let published = commit_tree(&repo, &format!("{merged}^{{tree}}"), &[&old, &colleague]);
    git(
        &repo,
        &["update-ref", "refs/remotes/origin/topic", &published],
    );

    assert_eq!(unguarded_move(&repo, "topic", &old, &merged), None);
}

#[test]
fn a_pull_into_a_branch_with_an_upstream_of_another_name_drops_nothing_published() {
    let repo = base_repo();
    git(&repo, &["switch", "-q", "-c", "mytopic"]);
    let mut worked = numbered("other", 3..=6);
    worked += &numbered("new", 1..=3);
    let published = commit(&repo, "other.txt", &worked, "X: rework other.txt");
    git(&repo, &["switch", "-q", "-c", "scratch"]);
    let backed_out = commit(
        &repo,
        "other.txt",
        &numbered("other", 1..=6),
        "back out X by hand",
    );
    git(
        &repo,
        &["update-ref", "refs/remotes/origin/topic", &backed_out],
    );
    git(&repo, &["switch", "-q", "mytopic"]);
    git(&repo, &["branch", "-q", "-D", "scratch"]);
    for (key, value) in [
        ("remote.origin.url", "."),
        ("remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*"),
        ("branch.mytopic.remote", "origin"),
        ("branch.mytopic.merge", "refs/heads/topic"),
    ] {
        git(&repo, &["config", key, value]);
    }
    // A fast-forward pull, and a merge pull over local work.
    assert_eq!(
        unguarded_move(&repo, "mytopic", &published, &backed_out),
        None
    );
    let old = commit(&repo, "a.txt", &numbered("alpha", 1..=9), "local work");
    let (_, pulled) = prepared(&repo, &["merge", "-q", "--no-ff", "--no-edit", &backed_out]);
    assert_eq!(unguarded_move(&repo, "mytopic", &old, &pulled), None);
}

#[test]
fn a_merge_commit_whose_first_parent_is_not_the_old_value_is_not_a_merge_into_the_branch() {
    let (repo, old) = topic_with_a_change();
    git(&repo, &["switch", "-q", "-c", "feature", "main"]);
    let feature = commit(
        &repo,
        "other.txt",
        &numbered("other", 1..=9),
        "rewrite other.txt",
    );
    // What `git rebase --rebase-merges` leaves: a merge rebuilt on another base.
    let rebuilt = commit_tree(&repo, &format!("{feature}^{{tree}}"), &["main", &feature]);
    git(&repo, &["switch", "-q", "topic"]);
    assert_eq!(unguarded_move(&repo, "topic", &old, &rebuilt), None);
}

/// A repository whose `a.txt` was rewritten by a commit with `fillers` empty commits on top of it,
/// and the move from that tip to a commit that restores `a.txt`, which no branch points at.
fn revert_buried_under(fillers: usize) -> (ScratchDir, String, String) {
    let repo = base_repo();
    let original = rev(&repo, "HEAD^{tree}");
    commit(
        &repo,
        "a.txt",
        &numbered("beta", 1..=5),
        "rewrite the head of a.txt",
    );
    for _ in 0..fillers {
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "filler"]);
    }
    let old = rev(&repo, "HEAD");
    let new = git(
        &repo,
        &["commit-tree", &original, "-p", &old, "-m", "undo it"],
    );
    let new = String::from_utf8(new).unwrap().trim().to_string();
    (repo, old, new)
}

#[test]
fn only_the_most_recent_commits_are_checked_and_twice_as_many_without_a_fork_point() {
    // The rewrite is the 100th commit back: the last one the wider window reaches.
    let (repo, old, new) = revert_buried_under(99);
    assert!(main_move(&repo, &[], &old, &new).is_some());
    // The rewrite is the 71st: past the window of 50 that a fork point leaves in place, since the
    // fork point at `old` adds no commit to it.
    let (repo, old, new) = revert_buried_under(70);
    git(&repo, &["branch", "-q", "feature", &new]);
    forget_reflog(&repo, "feature");
    assert!(main_move(&repo, &[], &old, &new).is_some());
    assert_eq!(main_move(&repo, &[("feature", &old)], &old, &new), None);
    // The 50th is still inside it.
    let (repo, old, new) = revert_buried_under(49);
    git(&repo, &["branch", "-q", "feature", &new]);
    assert!(main_move(&repo, &[("feature", &old)], &old, &new).is_some());
    // The 101st is beyond both.
    let (repo, old, new) = revert_buried_under(100);
    assert_eq!(main_move(&repo, &[], &old, &new), None);
}

#[test]
fn an_update_is_checked_only_when_prepared_and_for_a_guarded_branch_or_a_merge() {
    let (repo, _, old, new) = squash_with_changed_base(|repo| {
        commit(
            repo,
            "a.txt",
            &numbered("beta", 1..=5),
            "rewrite the head of a.txt",
        )
    });
    assert!(gate(&repo, &["main"], &[], "prepared", "main", &old, &new).is_some());
    assert_eq!(
        gate(&repo, &["main"], &[], "committed", "main", &old, &new),
        None
    );
    assert_eq!(
        gate(&repo, &["main"], &[], "aborted", "main", &old, &new),
        None
    );
    // Not guarded, and `feature` no longer at `new`: an ordinary move of one's own branch.
    git(&repo, &["branch", "-q", "-D", "feature"]);
    assert_eq!(
        gate(&repo, &["trunk"], &[], "prepared", "main", &old, &new),
        None
    );
}

#[test]
fn a_failure_of_the_gate_allows_the_update_and_says_so() {
    let nowhere = ScratchDir::new("revert-gate-nowhere");
    let git = git_in(&nowhere);
    let inputs = Inputs {
        guarded: HashSet::from(["main".to_string()]),
        default_target: None,
        targets: HashMap::new(),
        fork_points: HashMap::new(),
    };
    let (old, new) = ("1".repeat(40), "2".repeat(40));
    let stdin = format!("{old} {new} refs/heads/main\n");
    let failed = decide(&git, &inputs, "prepared", stdin.as_bytes());
    assert!(failed.is_err());
    let (status, message) = outcome(failed.map(Verdict::rejection));
    assert_eq!(status, 0);
    assert!(
        message.starts_with("[revert-gate] Check failed, allowed anyway: "),
        "{message}"
    );
    assert_eq!(
        outcome(Ok(Verdict::rejection(Some("no".to_string())))),
        (REJECTED, "no".to_string())
    );
    let mut noted = Verdict::default();
    noted.note("fyi".to_string());
    assert_eq!(outcome(Ok(noted)), (0, "fyi".to_string()));
}
