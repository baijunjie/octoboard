use super::*;

/// A clone whose `feature` branch records `main` as its target and `fork` as its fork point, and
/// whose `origin/main` is `tip`. Returns the clone and the fork point.
fn feature_clone(main_change: impl FnOnce(&Path) -> String) -> (ScratchDir, String, String) {
    let origin = base_repo();
    let fork = rev(&origin, "HEAD");
    let local = clone_of(&origin);
    git(&local, &["switch", "-q", "-c", "feature"]);
    commit(
        &local,
        "other.txt",
        &(numbered("other", 1..=6) + &numbered("feature", 1..=3)),
        "add the feature",
    );
    main_change(&origin);
    git(&local, &["fetch", "-q"]);
    let tip = rev(&local, "origin/main");
    (local, fork, tip)
}

fn rebase_onto_origin_main(repo: &Path) -> String {
    git(repo, &["rebase", "-q", "origin/main"]);
    rev(repo, "HEAD")
}

/// Squashes `feature` onto `origin/main` after `git reset --soft origin/main`, so the content of
/// the old base comes back.
fn squash_onto_origin_main(repo: &Path) -> String {
    git(repo, &["reset", "-q", "--soft", "origin/main"]);
    git(repo, &["commit", "-q", "-m", "add the feature, squashed"]);
    rev(repo, "HEAD")
}

fn rewrite_head_of_a(repo: &Path) -> String {
    commit(
        repo,
        "a.txt",
        &numbered("beta", 1..=5),
        "rewrite the head of a.txt",
    )
}

#[test]
fn a_pushed_squash_that_changed_the_base_is_rejected_and_an_ordinary_push_is_not() {
    let (repo, fork, _) = feature_clone(rewrite_head_of_a);
    let targets = [("feature", "main")];
    let forks = [("feature", fork.as_str())];

    let squashed = squash_onto_origin_main(&repo);
    let verdict = push(
        &repo,
        &[],
        &targets,
        &forks,
        "feature",
        &squashed,
        &"0".repeat(40),
    );
    assert!(verdict.rejected, "{}", verdict.text);
    assert!(
        verdict.text.contains("Refusing to push feature")
            && verdict.text.contains("relative to origin/main")
            && verdict.text.contains("rewrite the head of a.txt")
            && verdict
                .text
                .contains("git rebase -i $(git merge-base HEAD origin/main)")
            && !verdict.text.contains("git reset --soft $(git merge-base"),
        "{}",
        verdict.text
    );

    // Rebased the ordinary way, the branch keeps the target's change.
    git(&repo, &["reset", "-q", "--hard", "ORIG_HEAD"]);
    let rebased = rebase_onto_origin_main(&repo);
    let verdict = push(
        &repo,
        &[],
        &targets,
        &forks,
        "feature",
        &rebased,
        &"0".repeat(40),
    );
    assert!(
        !verdict.rejected && verdict.text.is_empty(),
        "{}",
        verdict.text
    );
}

#[test]
fn a_push_not_yet_rebased_onto_the_target_tip_is_not_checked() {
    let (repo, fork, _) = feature_clone(rewrite_head_of_a);
    let unrebased = squash_onto_origin_main(&repo);
    git(&repo, &["reset", "-q", "--hard", "ORIG_HEAD"]);
    // `feature` does not descend from `origin/main`, even with the squash's content.
    let behind = rev(&repo, "HEAD");
    assert_ne!(behind, unrebased);
    let verdict = push(
        &repo,
        &[],
        &[("feature", "main")],
        &[("feature", &fork)],
        "feature",
        &behind,
        &"0".repeat(40),
    );
    assert!(
        !verdict.rejected && verdict.text.is_empty(),
        "{}",
        verdict.text
    );
}

#[test]
fn a_declared_revert_is_allowed_in_a_push() {
    let (repo, fork, _) = feature_clone(rewrite_head_of_a);
    let reverted = rev(&repo, "origin/main");
    git(&repo, &["rebase", "-q", "origin/main"]);
    let original = numbered("alpha", 1..=10);
    let message = format!("undo it\n\nReverts: {}", &reverted[..12]);
    let declared = commit(&repo, "a.txt", &original, &message);
    let plain = {
        git(&repo, &["reset", "-q", "--hard", "HEAD~1"]);
        commit(&repo, "a.txt", &original, "undo it")
    };
    let targets = [("feature", "main")];
    let forks = [("feature", fork.as_str())];
    let nothing = "0".repeat(40);

    let verdict = push(&repo, &[], &targets, &forks, "feature", &plain, &nothing);
    assert!(verdict.rejected, "{}", verdict.text);
    let verdict = push(&repo, &[], &targets, &forks, "feature", &declared, &nothing);
    assert!(!verdict.rejected, "{}", verdict.text);
}

#[test]
fn a_guarded_branch_pushed_over_a_remote_commit_missing_locally_is_rejected() {
    let repo = base_repo();
    let local = rev(&repo, "HEAD");
    let remote = "1".repeat(40);
    let verdict = push(&repo, &["main"], &[], &[], "main", &local, &remote);
    assert!(verdict.rejected);
    assert!(
        verdict
            .text
            .contains("Refusing to push main: remote main has commits the local one does not"),
        "{}",
        verdict.text
    );
}

#[test]
fn a_guarded_branch_is_checked_against_its_remote_value_when_pushed() {
    let origin = base_repo();
    let local = clone_of(&origin);
    let remote = rev(&local, "HEAD");
    let new = commit(&local, "a.txt", &numbered("alpha", 1..=9), "trim a.txt");
    let verdict = push(&local, &["main"], &[], &[], "main", &new, &remote);
    assert!(!verdict.rejected, "{}", verdict.text);

    // The remote value is not an ancestor of the pushed one.
    git(&local, &["reset", "-q", "--hard", &remote]);
    let other = commit(
        &local,
        "other.txt",
        &numbered("other", 1..=9),
        "rewrite other.txt",
    );
    let verdict = push(&local, &["main"], &[], &[], "main", &other, &new);
    assert!(verdict.rejected);
    assert!(
        verdict.text.contains("Refusing to rewrite remote main"),
        "{}",
        verdict.text
    );
}

#[test]
fn a_push_involving_no_guarded_branch_or_target_runs_no_git() {
    // A program that does not exist fails any `git` run, and the verdict is still a plain allow.
    let nowhere = ScratchDir::new("revert-gate-nowhere");
    let git = Git::with("/nonexistent/git", nowhere.to_path_buf(), Vec::new());
    let inputs = |guard: &[&str]| {
        Inputs::new(&GateInputs {
            guard: guard.iter().map(|name| name.to_string()).collect(),
            targets: vec![("feature".to_string(), "develop".to_string())],
            fork_points: Vec::new(),
        })
    };
    let (local, remote) = ("1".repeat(40), "2".repeat(40));
    // With no guarded branch a branch that records no target branch has none to be checked
    // against; a deleted branch has nothing to check at all.
    let stdin = format!(
        "refs/heads/topic {local} refs/heads/topic {remote}\nrefs/heads/main {} refs/heads/main {remote}\n",
        "0".repeat(40)
    );
    let verdict = decide_push(&git, &inputs(&[]), stdin.as_bytes()).expect("nothing to check");
    assert!(!verdict.rejected && verdict.text.is_empty());
    let verdict = decide_push(&git, &inputs(&["main"]), stdin.as_bytes());
    assert!(
        verdict.is_err(),
        "the first guarded branch stands in for topic's target"
    );

    // The controls: a recorded target branch, and a guarded branch being pushed, start one too.
    let stdin = format!("refs/heads/feature {local} refs/heads/feature {remote}\n");
    assert!(decide_push(&git, &inputs(&[]), stdin.as_bytes()).is_err());
    let stdin = format!("refs/heads/main {local} refs/heads/main {remote}\n");
    assert!(decide_push(&git, &inputs(&["main"]), stdin.as_bytes()).is_err());
}

#[test]
fn the_skip_request_lets_a_rejected_push_through() {
    let (repo, fork, _) = feature_clone(rewrite_head_of_a);
    let squashed = squash_onto_origin_main(&repo);
    let git = git_in(&repo);
    let inputs = GateInputs {
        targets: vec![("feature".to_string(), "main".to_string())],
        fork_points: vec![("feature".to_string(), fork)],
        ..GateInputs::default()
    };
    let hook = Hook::PrePush;
    let stdin = format!(
        "refs/heads/feature {squashed} refs/heads/feature {}\n",
        "0".repeat(40)
    );
    assert_eq!(
        run_in(&git, false, &hook, &inputs, stdin.as_bytes()).0,
        REJECTED
    );
    assert_eq!(
        run_in(&git, true, &hook, &inputs, stdin.as_bytes()),
        (0, String::new())
    );
}

/// A clone whose `feature` is a squash with a changed base onto `origin/main`, which gained
/// `added.txt`, a purely additive commit no content criterion sees undone. Returns the clone, the
/// fork point and the squash.
fn additive_squash() -> (ScratchDir, String, String) {
    let (repo, fork, _) = feature_clone(|origin| {
        commit(
            origin,
            "added.txt",
            &numbered("added", 1..=8),
            "add added.txt",
        )
    });
    let squashed = squash_onto_origin_main(&repo);
    (repo, fork, squashed)
}

#[test]
fn a_pushed_additive_commit_wiped_out_is_caught_only_through_a_fork_point_recorded_or_from_the_reflog(
) {
    let (repo, fork, squashed) = additive_squash();
    let targets = [("feature", "main")];
    let nothing = "0".repeat(40);

    let verdict = push(&repo, &[], &targets, &[], "feature", &squashed, &nothing);
    assert!(verdict.rejected, "{}", verdict.text);

    forget_reflog(&repo, "feature");
    let verdict = push(&repo, &[], &targets, &[], "feature", &squashed, &nothing);
    assert!(!verdict.rejected, "{}", verdict.text);

    let forks = [("feature", fork.as_str())];
    let verdict = push(&repo, &[], &targets, &forks, "feature", &squashed, &nothing);
    assert!(verdict.rejected, "{}", verdict.text);
    assert!(
        verdict
            .text
            .contains("added.txt: this file was deleted wholesale"),
        "{}",
        verdict.text
    );
}

#[test]
fn a_branch_pushed_under_another_name_is_checked_by_the_local_branchs_records() {
    let (repo, fork, _) = feature_clone(rewrite_head_of_a);
    let squashed = squash_onto_origin_main(&repo);
    let stdin = format!(
        "refs/heads/feature {squashed} refs/heads/review {}\n",
        "0".repeat(40)
    );

    let verdict = push_lines(
        &repo,
        &[],
        &[("feature", "main")],
        &[("feature", &fork)],
        &stdin,
    );
    assert!(verdict.rejected, "{}", verdict.text);
    assert!(
        verdict.text.contains("Refusing to push feature"),
        "{}",
        verdict.text
    );

    // Records kept under the remote name are not the local branch's.
    let verdict = push_lines(
        &repo,
        &[],
        &[("review", "main")],
        &[("review", &fork)],
        &stdin,
    );
    assert!(
        !verdict.rejected && verdict.text.is_empty(),
        "{}",
        verdict.text
    );
}

#[test]
fn a_target_with_several_remote_branches_and_no_upstream_is_noted_not_checked() {
    let (repo, fork, _) = feature_clone(rewrite_head_of_a);
    let squashed = squash_onto_origin_main(&repo);
    git(&repo, &["branch", "--unset-upstream", "main"]);
    git(
        &repo,
        &["remote", "add", "second", "https://example.invalid/s.git"],
    );
    git(
        &repo,
        &["update-ref", "refs/remotes/second/main", "origin/main"],
    );

    let verdict = push(
        &repo,
        &[],
        &[("feature", "main")],
        &[("feature", &fork)],
        "feature",
        &squashed,
        &"0".repeat(40),
    );

    assert!(!verdict.rejected);
    assert!(
        verdict
            .text
            .contains("feature was not checked: main's remote state could not be pinned down"),
        "{}",
        verdict.text
    );
}

#[test]
fn a_target_that_is_on_no_remote_is_passed_over_silently() {
    let (repo, fork, _) = feature_clone(rewrite_head_of_a);
    let squashed = squash_onto_origin_main(&repo);
    git(&repo, &["update-ref", "-d", "refs/remotes/origin/main"]);

    let verdict = push(
        &repo,
        &[],
        &[("feature", "main")],
        &[("feature", &fork)],
        "feature",
        &squashed,
        &"0".repeat(40),
    );

    assert!(
        !verdict.rejected && verdict.text.is_empty(),
        "{}",
        verdict.text
    );
}

#[test]
fn a_guarded_fast_forward_push_that_undoes_content_is_rejected_with_the_push_advice() {
    let (repo, fork, squashed) = additive_squash();
    let remote = rev(&repo, "origin/main");
    // `feature` is another local branch at the pushed commit, which is where its fork point is
    // found, and the push is a fast-forward of `main` on the remote.
    let stdin = format!("refs/heads/feature {squashed} refs/heads/main {remote}\n");

    forget_reflog(&repo, "feature");
    let verdict = push_lines(&repo, &["main"], &[], &[], &stdin);
    assert!(!verdict.rejected, "{}", verdict.text);

    let verdict = push_lines(&repo, &["main"], &[], &[("feature", &fork)], &stdin);
    assert!(verdict.rejected, "{}", verdict.text);
    assert!(
        verdict
            .text
            .contains("add a commit in the working copy that holds main")
            && !verdict.text.contains("git reset --merge"),
        "{}",
        verdict.text
    );
}

#[test]
fn every_ref_of_a_push_is_checked_and_the_rejections_are_all_given() {
    let (repo, fork, _) = feature_clone(rewrite_head_of_a);
    let squashed = squash_onto_origin_main(&repo);
    git(&repo, &["branch", "second", &squashed]);
    let (nothing, other) = ("0".repeat(40), "1".repeat(40));
    let stdin = format!(
        "refs/heads/feature {squashed} refs/heads/feature {nothing}\n\
         refs/heads/topic {squashed} refs/heads/topic {nothing}\n\
         refs/heads/second {squashed} refs/heads/second {nothing}\n\
         refs/heads/main {squashed} refs/heads/main {other}\n"
    );

    let verdict = push_lines(
        &repo,
        &["main"],
        &[("feature", "main"), ("second", "main")],
        &[("feature", &fork), ("second", &fork)],
        &stdin,
    );

    assert!(verdict.rejected);
    // `topic` records no target branch, and the first guarded branch stands in for it.
    for refused in ["push feature", "push second", "push topic", "push main"] {
        assert!(
            verdict.text.contains(&format!("Refusing to {refused}")),
            "{refused}: {}",
            verdict.text
        );
    }
}

#[test]
fn a_pushed_branch_with_no_recorded_target_is_checked_against_the_first_guarded_branch() {
    let (repo, _, _) = feature_clone(rewrite_head_of_a);
    let squashed = squash_onto_origin_main(&repo);
    let nothing = "0".repeat(40);

    let verdict = push(&repo, &["main"], &[], &[], "feature", &squashed, &nothing);
    assert!(verdict.rejected, "{}", verdict.text);
    assert!(
        verdict.text.contains("Refusing to push feature")
            && verdict.text.contains("relative to origin/main")
            && verdict.text.contains("rewrite the head of a.txt"),
        "{}",
        verdict.text
    );
    // With no guarded branch there is no target branch to stand in.
    let verdict = push(&repo, &[], &[], &[], "feature", &squashed, &nothing);
    assert!(
        !verdict.rejected && verdict.text.is_empty(),
        "{}",
        verdict.text
    );
    // Rebased the ordinary way, it keeps the change.
    git(&repo, &["reset", "-q", "--hard", "ORIG_HEAD"]);
    let rebased = rebase_onto_origin_main(&repo);
    let verdict = push(&repo, &["main"], &[], &[], "feature", &rebased, &nothing);
    assert!(
        !verdict.rejected && verdict.text.is_empty(),
        "{}",
        verdict.text
    );
}

#[test]
fn the_guarded_branch_pushed_is_not_checked_against_itself_as_a_fallback_target() {
    let origin = base_repo();
    let local = clone_of(&origin);
    let remote = rev(&local, "HEAD");
    let new = commit(&local, "a.txt", &numbered("alpha", 1..=9), "trim a.txt");
    let verdict = push(&local, &["main"], &[], &[], "main", &new, &remote);
    assert!(
        !verdict.rejected && verdict.text.is_empty(),
        "{}",
        verdict.text
    );
    // Not even under another name on the remote: no `git` runs, as for a push of nothing.
    let nowhere = ScratchDir::new("revert-gate-nowhere");
    let git = Git::with("/nonexistent/git", nowhere.to_path_buf(), Vec::new());
    let inputs = Inputs::new(&GateInputs {
        guard: vec!["main".to_string()],
        ..GateInputs::default()
    });
    let stdin = format!("refs/heads/main {new} refs/heads/other {remote}\n");
    let verdict = decide_push(&git, &inputs, stdin.as_bytes()).expect("nothing to check");
    assert!(!verdict.rejected && verdict.text.is_empty());
}

#[test]
fn a_branch_cut_from_a_local_target_ahead_of_its_remote_is_pushed() {
    let (local, fork) = feature_cut_from_a_local_main_ahead(|origin| rev(origin, "HEAD"));
    let new = rev(&local, "feature");
    let targets = [("feature", "main")];
    let forks = [("feature", fork.as_str())];
    let nothing = "0".repeat(40);
    let verdict = push(&local, &[], &targets, &forks, "feature", &new, &nothing);
    assert!(!verdict.rejected, "{}", verdict.text);
}

/// A clone whose local `main` is two commits ahead of `origin/main` when `feature` is cut from it;
/// `remote_change` then runs in the origin, and the clone fetches from it. Returns the clone and
/// `main`'s tip, the fork point to record for `feature`.
fn feature_cut_from_a_local_main_ahead(
    remote_change: impl FnOnce(&Path) -> String,
) -> (ScratchDir, String) {
    let origin = base_repo();
    let local = clone_of(&origin);
    commit(&local, "a.txt", &numbered("alpha", 1..=9), "trim a.txt");
    let fork = commit(
        &local,
        "other.txt",
        &numbered("other", 1..=4),
        "trim other.txt",
    );
    git(&local, &["switch", "-q", "-c", "feature"]);
    commit(
        &local,
        "feature.txt",
        &numbered("feature", 1..=4),
        "add the feature",
    );
    remote_change(&origin);
    git(&local, &["fetch", "-q"]);
    (local, fork)
}

#[test]
fn a_branch_cut_from_a_local_target_ahead_of_its_remote_is_pushed_after_the_remote_moves_on() {
    let (local, fork) = feature_cut_from_a_local_main_ahead(|origin| {
        commit(origin, "r.txt", &numbered("remote", 1..=4), "add r.txt")
    });
    // The faithful rebase replays the unpushed commits of `main` along with the feature's.
    let new = rebase_onto_origin_main(&local);
    let targets = [("feature", "main")];
    let forks = [("feature", fork.as_str())];
    let nothing = "0".repeat(40);
    let verdict = push(&local, &[], &targets, &forks, "feature", &new, &nothing);
    assert!(!verdict.rejected, "{}", verdict.text);
}

#[test]
fn a_changed_base_against_a_target_rebased_onto_its_remote_is_still_rejected() {
    let (local, fork) = feature_cut_from_a_local_main_ahead(|origin| {
        commit(
            origin,
            "added.txt",
            &numbered("added", 1..=8),
            "add added.txt",
        )
    });
    // `main` aligns with its remote, so the fork point is no longer its ancestor; `feature` is
    // then squashed onto it after `git reset --soft main`, which wipes out `added.txt`.
    git(&local, &["switch", "-q", "main"]);
    git(&local, &["rebase", "-q", "origin/main"]);
    let old = rev(&local, "main");
    git(&local, &["switch", "-q", "feature"]);
    git(&local, &["reset", "-q", "--soft", "main"]);
    git(&local, &["commit", "-q", "-m", "add the feature, squashed"]);
    let new = rev(&local, "HEAD");
    git(&local, &["switch", "-q", "main"]);
    let found =
        main_move(&local, &[("feature", &fork)], &old, &new).expect("added.txt is wiped out");
    assert!(
        found.contains("added.txt: this file was deleted wholesale"),
        "{found}"
    );
}

#[test]
fn a_pushed_squash_onto_a_newer_remote_is_rejected_for_the_remotes_file_only() {
    let (local, fork) = feature_cut_from_a_local_main_ahead(|origin| {
        commit(
            origin,
            "added.txt",
            &numbered("added", 1..=8),
            "add added.txt",
        )
    });
    let squashed = squash_onto_origin_main(&local);
    let targets = [("feature", "main")];
    let forks = [("feature", fork.as_str())];
    let nothing = "0".repeat(40);
    let verdict = push(
        &local,
        &[],
        &targets,
        &forks,
        "feature",
        &squashed,
        &nothing,
    );
    assert!(verdict.rejected, "{}", verdict.text);
    assert!(
        verdict
            .text
            .contains("added.txt: this file was deleted wholesale"),
        "{}",
        verdict.text
    );
    // The unpushed commits of `main` the branch carries are not held against it.
    assert!(!verdict.text.contains("a.txt:"), "{}", verdict.text);
    assert!(!verdict.text.contains("other.txt:"), "{}", verdict.text);
}

#[test]
fn a_mode_change_on_the_remote_lost_by_a_pushed_squash_is_rejected() {
    let (local, fork) = feature_cut_from_a_local_main_ahead(|origin| {
        git(origin, &["update-index", "--chmod=+x", "other.txt"]);
        git(origin, &["commit", "-q", "-m", "make other.txt executable"]);
        rev(origin, "HEAD")
    });
    // The squash keeps the fork point's `other.txt`, which drops the remote's mode change.
    let squashed = squash_onto_origin_main(&local);
    let targets = [("feature", "main")];
    let forks = [("feature", fork.as_str())];
    let nothing = "0".repeat(40);
    let verdict = push(
        &local,
        &[],
        &targets,
        &forks,
        "feature",
        &squashed,
        &nothing,
    );
    assert!(verdict.rejected, "{}", verdict.text);
    assert!(verdict.text.contains("other.txt:"), "{}", verdict.text);
}
