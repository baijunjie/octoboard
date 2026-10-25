use std::path::Path;

use super::push::decide_push;
use super::*;
use crate::test_support::{git, isolated_git_env, repo_with, ScratchDir, SYSTEM_GIT};

/// The gate's `git` in `repo`, over the isolated environment the fixtures use.
fn git_in(repo: &Path) -> Git {
    Git::with(
        SYSTEM_GIT,
        repo.to_path_buf(),
        isolated_git_env()
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect(),
    )
}

/// Lines `<word> line 01` to `<word> line <last>`, none of them trivial.
fn numbered(word: &str, range: std::ops::RangeInclusive<u32>) -> String {
    range.map(|n| format!("{word} line {n:02}\n")).collect()
}

fn rev(repo: &Path, spec: &str) -> String {
    String::from_utf8(git(repo, &["rev-parse", spec]))
        .unwrap()
        .trim()
        .to_string()
}

fn commit(repo: &Path, name: &str, content: &str, message: &str) -> String {
    std::fs::write(repo.join(name), content).unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", message]);
    rev(repo, "HEAD")
}

/// A repository whose `a.txt` is ten `alpha` lines.
fn base_repo() -> ScratchDir {
    repo_with(
        "revert-gate",
        &[
            (b"a.txt", numbered("alpha", 1..=10).as_bytes()),
            (b"other.txt", numbered("other", 1..=6).as_bytes()),
        ],
    )
}

/// A clone of `origin`, with `main` tracking `origin/main`.
fn clone_of(origin: &Path) -> ScratchDir {
    let dir = ScratchDir::new("revert-gate-clone");
    git(&dir, &["clone", "-q", origin.to_str().unwrap(), "."]);
    dir
}

/// What the hook sees at `prepared` for the command `args` run on the checked-out branch: the
/// branch still at its old commit, and the commit the command would move it to.
fn prepared(repo: &Path, args: &[&str]) -> (String, String) {
    let old = rev(repo, "HEAD");
    git(repo, args);
    let new = rev(repo, "HEAD");
    git(repo, &["reset", "-q", "--hard", &old]);
    (old, new)
}

/// The gate's verdict on `branch` moving from `old` to `new` at `state`: `None` to allow, else
/// the rejection. `forks` are the recorded fork points, `(branch, commit)`.
fn gate(
    repo: &Path,
    guard: &[&str],
    forks: &[(&str, &str)],
    state: &str,
    branch: &str,
    old: &str,
    new: &str,
) -> Option<String> {
    let git = git_in(repo);
    let inputs = Inputs {
        guarded: guard.iter().map(|name| name.to_string()).collect(),
        targets: HashMap::new(),
        fork_points: forks
            .iter()
            .map(|(name, fork)| (name.to_string(), fork.to_string()))
            .collect(),
    };
    let stdin = format!("{old} {new} refs/heads/{branch}\n");
    decide(&git, &inputs, state, stdin.as_bytes()).expect("the gate runs")
}

fn main_move(repo: &Path, forks: &[(&str, &str)], old: &str, new: &str) -> Option<String> {
    gate(repo, &["main"], forks, "prepared", "main", old, new)
}

/// `feature` is squashed after `git reset --soft main`, so its commit has the latest `main` as
/// parent and the content of the old base, so main's own commit `main_change` is undone; the
/// feature also appends to `a.txt`, so that file is no longer exactly the old base's. Returns
/// the repository, the fork point, and main's tip before and the squash after.
fn squash_with_changed_base(
    main_change: impl FnOnce(&Path) -> String,
) -> (ScratchDir, String, String, String) {
    let repo = base_repo();
    let fork = rev(&repo, "HEAD");
    git(&repo, &["switch", "-q", "-c", "feature"]);
    std::fs::write(repo.join("feature.txt"), numbered("feature", 1..=4)).unwrap();
    commit(
        &repo,
        "a.txt",
        &(numbered("alpha", 1..=10) + &numbered("feature", 1..=3)),
        "add the feature",
    );
    git(&repo, &["switch", "-q", "main"]);
    main_change(&repo);
    let old = rev(&repo, "HEAD");
    git(&repo, &["switch", "-q", "feature"]);
    git(&repo, &["reset", "-q", "--soft", "main"]);
    git(&repo, &["commit", "-q", "-m", "add the feature, squashed"]);
    let new = rev(&repo, "HEAD");
    git(&repo, &["switch", "-q", "main"]);
    (repo, fork, old, new)
}

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
    assert!(main_move(&repo, &[("feature", &fork)], &old, &new).is_some());
}

#[test]
fn a_wiped_out_additive_commit_is_caught_only_through_the_fork_point() {
    let (repo, fork, old, new) = squash_with_changed_base(|repo| {
        commit(
            repo,
            "added.txt",
            &numbered("added", 1..=8),
            "add added.txt",
        )
    });
    // No line is deleted by the commit, so no content criterion sees it undone.
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

#[test]
fn only_a_prepared_update_of_a_guarded_branch_is_checked() {
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

#[test]
fn the_arguments_become_the_guarded_branches_and_the_fork_points() {
    let args = parse(&[
        "prepared",
        "--guard",
        "main",
        "--target",
        "feature",
        "develop",
        "--target",
        "fix",
        "main",
        "--fork-point",
        "feature",
        "abc123",
        "--fork-point",
        "fix",
        "v1.0",
    ]);
    assert_eq!(args.state.as_deref(), Some("prepared"));
    assert!(args.pre_push.is_none());
    let inputs = Inputs::new(&args.options.inputs());
    assert_eq!(
        inputs.guarded,
        HashSet::from(["main".to_string(), "develop".to_string()])
    );
    assert_eq!(inputs.targets["feature"], "develop");
    assert_eq!(inputs.fork_points["feature"], "abc123");
    assert_eq!(inputs.fork_points["fix"], "v1.0");
}

#[test]
fn the_pre_push_subcommand_takes_the_remote_the_url_and_the_same_options() {
    let args = parse(&[
        "pre-push",
        "origin",
        "https://example.com/r.git",
        "--target",
        "feature",
        "main",
    ]);
    let Some(PrePush::PrePush {
        remote,
        url,
        options,
    }) = args.pre_push
    else {
        panic!("not the pre-push subcommand");
    };
    assert_eq!(
        (remote.as_str(), url.as_str()),
        ("origin", "https://example.com/r.git")
    );
    assert_eq!(
        options.inputs().targets,
        [("feature".into(), "main".into())]
    );
}

fn parse(args: &[&str]) -> Args {
    use clap::Parser;
    let cli = crate::Cli::try_parse_from(
        ["octoboardd", "revert-gate"]
            .into_iter()
            .chain(args.iter().copied()),
    )
    .unwrap();
    let Some(crate::Command::RevertGate(args)) = cli.command else {
        panic!("not the revert-gate command");
    };
    args
}

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

/// The verdict on pushing `local` as `branch`, where the remote held `remote`.
fn push(
    repo: &Path,
    guard: &[&str],
    targets: &[(&str, &str)],
    forks: &[(&str, &str)],
    branch: &str,
    local: &str,
    remote: &str,
) -> Verdict {
    push_lines(
        repo,
        guard,
        targets,
        forks,
        &format!("refs/heads/{branch} {local} refs/heads/{branch} {remote}\n"),
    )
}

/// The verdict on a push whose `pre-push` lines are `stdin`.
fn push_lines(
    repo: &Path,
    guard: &[&str],
    targets: &[(&str, &str)],
    forks: &[(&str, &str)],
    stdin: &str,
) -> Verdict {
    let pairs = |pairs: &[(&str, &str)]| {
        pairs
            .iter()
            .map(|(name, other)| (name.to_string(), other.to_string()))
            .collect()
    };
    let inputs = Inputs::new(&GateInputs {
        guard: guard.iter().map(|name| name.to_string()).collect(),
        targets: pairs(targets),
        fork_points: pairs(forks),
    });
    decide_push(&git_in(repo), &inputs, stdin.as_bytes()).expect("the gate runs")
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
            && verdict.text.contains("rewrite the head of a.txt"),
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
fn a_push_involving_no_guarded_branch_or_recorded_target_runs_no_git() {
    // A program that does not exist fails any `git` run, and the verdict is still a plain allow.
    let nowhere = ScratchDir::new("revert-gate-nowhere");
    let git = Git::with("/nonexistent/git", nowhere.to_path_buf(), Vec::new());
    let inputs = Inputs::new(&GateInputs {
        guard: vec!["main".to_string()],
        targets: vec![("feature".to_string(), "develop".to_string())],
        fork_points: Vec::new(),
    });
    let (local, remote) = ("1".repeat(40), "2".repeat(40));
    let stdin = format!(
        "refs/heads/topic {local} refs/heads/topic {remote}\nrefs/heads/main {} refs/heads/main {remote}\n",
        "0".repeat(40)
    );
    let verdict = decide_push(&git, &inputs, stdin.as_bytes()).expect("nothing to check");
    assert!(!verdict.rejected && verdict.text.is_empty());

    // The control: a guarded branch being pushed does start one.
    let stdin = format!("refs/heads/main {local} refs/heads/main {remote}\n");
    assert!(decide_push(&git, &inputs, stdin.as_bytes()).is_err());
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
fn a_pushed_additive_commit_wiped_out_is_caught_only_through_the_fork_point() {
    let (repo, fork, squashed) = additive_squash();
    let targets = [("feature", "main")];
    let nothing = "0".repeat(40);

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
    for refused in ["push feature", "push second", "push main"] {
        assert!(
            verdict.text.contains(&format!("Refusing to {refused}")),
            "{refused}: {}",
            verdict.text
        );
    }
    assert!(!verdict.text.contains("push topic"));
}
