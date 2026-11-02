use super::*;
use crate::revert_gate::git::{
    busy_branches, creation_point, remote_refs, remote_tips, tips_at, upstream_of,
};

#[test]
fn branches_checked_out_in_a_worktree_whose_path_holds_a_newline_are_busy() {
    let repo = base_repo();
    git(&repo, &["branch", "other"]);
    let linked = ScratchDir::new("revert-gate-linked");
    let path = linked.join("new\nbranch refs/heads/forged");
    git(
        &repo,
        &["worktree", "add", "-q", path.to_str().unwrap(), "other"],
    );

    let busy = busy_branches(&git_in(&repo)).expect("the listing is read");
    assert!(busy.contains("other"), "{busy:?}");
    assert!(!busy.contains("forged"), "{busy:?}");
}

#[test]
fn a_rebase_state_names_its_branch_as_busy() {
    let repo = base_repo();
    let gitdir = repo.join(".git");
    std::fs::create_dir(gitdir.join("rebase-merge")).unwrap();
    std::fs::write(
        gitdir.join("rebase-merge/head-name"),
        "refs/heads/feature\n",
    )
    .unwrap();
    let busy = busy_branches(&git_in(&repo)).expect("the state is read");
    assert!(busy.contains("feature"), "{busy:?}");
}

/// A branch made by `update-ref` from nothing is created as much as one made by `checkout -b`,
/// and the same definition holds for the attribution (`branch_origin`).
#[test]
fn a_branch_made_by_update_ref_has_its_creation_point_as_one_made_by_checkout_has() {
    let repo = base_repo();
    let head = rev(&repo, "HEAD");
    git(&repo, &["checkout", "-q", "-b", "by-checkout"]);
    git(
        &repo,
        &["update-ref", "-m", "imported", "refs/heads/by-ref", &head],
    );
    let gate = git_in(&repo);
    assert_eq!(
        creation_point(&gate, "by-checkout").unwrap(),
        Some(head.clone())
    );
    assert_eq!(creation_point(&gate, "by-ref").unwrap(), Some(head));
    assert_eq!(creation_point(&gate, "missing").unwrap(), None);
}

/// A remote may have a slash in its name, so a remote-tracking ref is split by the remotes that
/// exist, not at its first slash.
#[test]
fn the_branches_of_a_remote_named_with_a_slash_keep_the_rest_of_their_name() {
    let repo = base_repo();
    let head = rev(&repo, "HEAD");
    git(
        &repo,
        &[
            "remote",
            "add",
            "team/origin",
            "https://example.invalid/r.git",
        ],
    );
    git(
        &repo,
        &["update-ref", "refs/remotes/team/origin/feature", &head],
    );
    git(
        &repo,
        &["update-ref", "refs/remotes/origin/topic/feature", &head],
    );
    let gate = git_in(&repo);
    let tips = tips_at(&gate, &head).unwrap();
    let names: Vec<_> = tips
        .iter()
        .filter(|tip| tip.remote)
        .map(|tip| tip.name.as_str())
        .collect();
    assert_eq!(names, ["topic/feature", "feature"]);
    // `topic/feature` of `origin` is another branch, and a `*` would not have found the first.
    assert_eq!(
        remote_refs(&gate, "feature").unwrap(),
        ["refs/remotes/team/origin/feature"]
    );
}

#[test]
fn the_upstream_of_a_remote_named_with_a_slash_is_the_tracked_branch() {
    let repo = base_repo();
    let head = rev(&repo, "HEAD");
    git(
        &repo,
        &[
            "remote",
            "add",
            "team/origin",
            "https://example.invalid/r.git",
        ],
    );
    git(
        &repo,
        &["update-ref", "refs/remotes/team/origin/theirs", &head],
    );
    git(&repo, &["checkout", "-q", "-b", "mine"]);
    git(&repo, &["config", "branch.mine.remote", "team/origin"]);
    git(&repo, &["config", "branch.mine.merge", "refs/heads/theirs"]);
    let gate = git_in(&repo);
    assert_eq!(
        remote_tips(&gate, "mine", &|_| false).unwrap(),
        vec![head.clone()]
    );
    assert!(remote_tips(&gate, "mine", &|name| name == "theirs")
        .unwrap()
        .is_empty());
}

/// `for-each-ref` takes a pattern as a prefix up to a slash, so the nested branch `feat/sub` of
/// `origin` is listed for the pattern of `feat`; it is another branch, neither a tip of `feat` nor
/// its upstream.
#[test]
fn a_nested_remote_branch_is_not_the_remote_branch_of_the_one_it_is_nested_under() {
    let repo = base_repo();
    let head = rev(&repo, "HEAD");
    git(&repo, &["branch", "feat"]);
    git(
        &repo,
        &["update-ref", "refs/remotes/origin/feat/sub", &head],
    );
    let gate = git_in(&repo);
    assert!(remote_refs(&gate, "feat").unwrap().is_empty());
    assert!(remote_tips(&gate, "feat", &|_| false).unwrap().is_empty());
}

#[test]
fn a_branch_below_a_missing_one_lends_it_no_upstream() {
    let repo = base_repo();
    let head = rev(&repo, "HEAD");
    git(
        &repo,
        &["update-ref", "refs/remotes/origin/elsewhere", &head],
    );
    git(&repo, &["branch", "release/x"]);
    git(
        &repo,
        &["branch", "--set-upstream-to=origin/elsewhere", "release/x"],
    );
    let gate = git_in(&repo);
    assert_eq!(
        upstream_of(&gate, "release").unwrap(),
        (None, "<remote>/release".to_string())
    );
}

#[test]
fn a_branch_below_a_missing_one_lends_it_no_published_tips() {
    let repo = base_repo();
    let head = rev(&repo, "HEAD");
    git(
        &repo,
        &["update-ref", "refs/remotes/origin/elsewhere", &head],
    );
    git(&repo, &["branch", "release/x"]);
    git(
        &repo,
        &["branch", "--set-upstream-to=origin/elsewhere", "release/x"],
    );
    let gate = git_in(&repo);
    assert!(remote_tips(&gate, "release", &|_| false)
        .unwrap()
        .is_empty());
}
