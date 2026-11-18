use super::*;
use crate::host_tools::Tools;
use crate::project_git::status::check_project;
use crate::test_support::{app_state, checked_project, git, git_not_found, repo_with};

/// A project whose branch tracks a bare remote another clone pushes to, for the manual pull.
struct Pulled {
    state: Arc<AppState>,
    repo: PathBuf,
    other: PathBuf,
    project: Project,
    _dir: crate::test_support::ScratchDir,
    _repo: crate::test_support::ScratchDir,
}

impl Pulled {
    fn new(label: &str) -> Self {
        Self::at(label, false)
    }

    /// As [`Self::new`], with the project a linked worktree of the repository (on branch
    /// `linked`, tracking the remote's), whose `MERGE_HEAD` is not under its own `.git`.
    fn linked(label: &str) -> Self {
        Self::at(label, true)
    }

    fn at(label: &str, linked: bool) -> Self {
        let (state, dir) = app_state(label);
        let repo_dir = repo_with(&format!("{label}-repo"), &[(b"a.txt", b"one\n")]);
        let repo = repo_dir.to_path_buf();
        let bare = dir.join("remote.git");
        git(&dir, &["init", "-q", "--bare", bare.to_str().unwrap()]);
        for (key, value) in [
            ("user.name", "Tester"),
            ("user.email", "tester@example.com"),
        ] {
            git(&repo, &["config", key, value]);
        }
        git(&repo, &["remote", "add", "origin", bare.to_str().unwrap()]);
        git(&repo, &["push", "-q", "-u", "origin", "HEAD"]);
        let other = dir.join("other");
        git(
            &dir,
            &[
                "clone",
                "-q",
                bare.to_str().unwrap(),
                other.to_str().unwrap(),
            ],
        );
        for (key, value) in [("user.name", "Other"), ("user.email", "other@example.com")] {
            git(&other, &["config", key, value]);
        }
        let work = if linked {
            let work = dir.join("linked");
            let upstream = format!("origin/{}", branch_of(&repo));
            git(
                &repo,
                &[
                    "worktree",
                    "add",
                    "-q",
                    "--track",
                    "-b",
                    "linked",
                    work.to_str().unwrap(),
                    &upstream,
                ],
            );
            work
        } else {
            repo
        };
        let project = checked_project(&state, &dir, &work);
        Self {
            state,
            repo: work,
            other,
            project,
            _dir: dir,
            _repo: repo_dir,
        }
    }

    fn head(&self) -> String {
        String::from_utf8(git(&self.repo, &["rev-parse", "HEAD"]))
            .unwrap()
            .trim()
            .to_string()
    }

    /// A commit on the remote, from the other clone.
    fn upstream_commit(&self, file: &str, body: &str) {
        commit(&self.other, file, body);
        git(&self.other, &["push", "-q", "origin", "HEAD"]);
    }

    /// Leaves the project on a branch with no commit yet, `fresh`, whose upstream is the remote's
    /// `upstream` ref, whether or not it exists.
    fn on_a_branch_with_no_commit_tracking(&self, upstream: &str) {
        git(&self.repo, &["checkout", "-q", "--orphan", "fresh"]);
        git(&self.repo, &["rm", "-rfq", "."]);
        for (key, value) in [
            ("branch.fresh.remote", "origin"),
            ("branch.fresh.merge", upstream),
        ] {
            git(&self.repo, &["config", key, value]);
        }
    }

    fn fetch(&self) {
        git(&self.repo, &["fetch", "-q", "origin"]);
    }

    /// Where `git` keeps this working tree's `MERGE_HEAD`, which is under `.git` only for the
    /// main working tree.
    fn merge_head_file(&self) -> PathBuf {
        let located = git(&self.repo, &["rev-parse", "--git-path", "MERGE_HEAD"]);
        self.repo.join(String::from_utf8(located).unwrap().trim())
    }

    async fn pull(&self, merge: bool) -> PullAnswer {
        pull_project_now(&self.state, self.project.clone(), merge).await
    }

    fn clean(&self) -> bool {
        git(&self.repo, &["status", "--porcelain"]).is_empty()
    }

    /// What the project's badge carries as the failure of its last check, if it has a status.
    fn badge_error(&self) -> Option<String> {
        self.state
            .git_status(&self.project.id)
            .and_then(|status| status.error)
    }
}

fn commit(repo: &Path, file: &str, body: &str) {
    std::fs::write(repo.join(file), body).unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", file]);
}

fn refusal_code(answer: PullAnswer) -> (crate::protocol::Code, String) {
    let err = answer.expect_err("the pull was refused");
    (
        err.code,
        err.params.get("detail").cloned().unwrap_or_default(),
    )
}

#[tokio::test]
async fn a_pull_of_a_branch_that_is_not_behind_is_up_to_date_and_changes_nothing() {
    let pulled = Pulled::new("pull-up-to-date");
    assert_eq!(pulled.pull(false).await, Ok(PullOutcome::UpToDate));
    commit(&pulled.repo, "b.txt", "mine\n");
    let head = pulled.head();
    assert_eq!(pulled.pull(false).await, Ok(PullOutcome::UpToDate));
    assert_eq!(pulled.head(), head, "ahead alone is nothing to pull");
}

#[tokio::test]
async fn a_pull_fast_forwards_a_branch_with_no_commits_of_its_own() {
    let pulled = Pulled::new("pull-fast-forward");
    pulled.upstream_commit("b.txt", "theirs\n");
    assert_eq!(pulled.pull(false).await, Ok(PullOutcome::FastForwarded));
    let theirs = String::from_utf8(git(&pulled.other, &["rev-parse", "HEAD"])).unwrap();
    assert_eq!(pulled.head(), theirs.trim());
    let status = pulled.state.git_status("p1").unwrap();
    assert_eq!((status.behind, status.activity), (0, GitActivity::Idle));
}

#[tokio::test]
async fn a_pull_of_a_diverged_branch_asks_for_a_merge_and_changes_nothing() {
    let pulled = Pulled::new("pull-needs-merge");
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    let head = pulled.head();
    assert_eq!(
        pulled.pull(false).await,
        Ok(PullOutcome::NeedsMerge {
            ahead: 1,
            behind: 1
        })
    );
    assert_eq!(pulled.head(), head);
    assert!(pulled.clean());
}

#[tokio::test]
async fn a_confirmed_pull_merges_a_diverged_branch_without_a_fetch() {
    let pulled = Pulled::new("pull-merged");
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    // Without the fetch the branch is not behind as far as the confirmation knew.
    assert_eq!(pulled.pull(true).await, Ok(PullOutcome::UpToDate));
    pulled.fetch();
    assert_eq!(pulled.pull(true).await, Ok(PullOutcome::Merged));
    git(&pulled.repo, &["rev-parse", "--verify", "HEAD^2"]);
    assert!(pulled.repo.join("c.txt").exists());
    assert!(pulled.clean());
    let status = pulled.state.git_status("p1").unwrap();
    assert_eq!((status.ahead, status.behind), (2, 0));
    assert_eq!(status.activity, GitActivity::Idle);
    assert!(
        pulled.state.claim_git_check_queued("p1").is_some(),
        "the claim is released"
    );
}

#[tokio::test]
async fn a_confirmed_pull_fast_forwards_when_the_branch_has_no_commits_of_its_own() {
    let pulled = Pulled::new("pull-confirmed-ff");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    assert_eq!(pulled.pull(true).await, Ok(PullOutcome::FastForwarded));
    assert!(pulled.repo.join("c.txt").exists());
}

#[tokio::test]
async fn a_merge_with_conflicts_is_aborted_and_names_the_conflicted_files() {
    let pulled = Pulled::new("pull-conflict");
    commit(&pulled.repo, "a.txt", "mine\n");
    pulled.upstream_commit("a.txt", "theirs\n");
    pulled.fetch();
    let head = pulled.head();
    let (code, detail) = refusal_code(pulled.pull(true).await);
    assert_eq!(code, error_code::PULL_MERGE_CONFLICT);
    assert_eq!(detail, "a.txt");
    assert_eq!(pulled.head(), head);
    assert!(pulled.clean());
    assert!(!pulled.repo.join(".git/MERGE_HEAD").exists());
    assert_eq!(
        std::fs::read_to_string(pulled.repo.join("a.txt")).unwrap(),
        "mine\n"
    );
    assert_eq!(
        pulled.badge_error(),
        None,
        "the refused request is the report"
    );
}

/// Every conflicted path is named, one per line, whatever its directory or spelling.
#[tokio::test]
async fn a_merge_with_conflicts_in_several_files_names_each_one_per_line() {
    let pulled = Pulled::new("pull-conflicts");
    std::fs::create_dir_all(pulled.repo.join("sub dir")).unwrap();
    std::fs::write(pulled.repo.join("sub dir/b c.txt"), "mine\n").unwrap();
    commit(&pulled.repo, "a.txt", "mine\n");
    std::fs::create_dir_all(pulled.other.join("sub dir")).unwrap();
    std::fs::write(pulled.other.join("sub dir/b c.txt"), "theirs\n").unwrap();
    pulled.upstream_commit("a.txt", "theirs\n");
    pulled.fetch();
    let head = pulled.head();
    let (code, detail) = refusal_code(pulled.pull(true).await);
    assert_eq!(code, error_code::PULL_MERGE_CONFLICT);
    let mut paths: Vec<&str> = detail.lines().collect();
    paths.sort_unstable();
    assert_eq!(paths, ["a.txt", "sub dir/b c.txt"], "{detail:?}");
    assert_eq!(pulled.head(), head);
    assert!(pulled.clean());
    assert!(!pulled.repo.join(".git/MERGE_HEAD").exists());
}

#[tokio::test]
async fn a_merge_git_refuses_for_another_reason_is_a_failed_pull_and_leaves_no_merge_behind() {
    let pulled = Pulled::new("pull-merge-refused");
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    git(&pulled.repo, &["config", "core.hooksPath", ".git/hooks"]);
    let hook = pulled.repo.join(".git/hooks/pre-merge-commit");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(&hook, "#!/bin/sh\necho refused by the hook >&2\nexit 1\n").unwrap();
    crate::test_support::make_executable(&hook);
    let head = pulled.head();
    let (code, detail) = refusal_code(pulled.pull(true).await);
    assert_eq!(code, error_code::GIT_PULL_FAILED);
    assert!(detail.contains("refused by the hook"), "{detail}");
    assert_eq!(pulled.head(), head);
    assert!(!pulled.repo.join(".git/MERGE_HEAD").exists());
    assert_eq!(pulled.badge_error(), None);
}

#[tokio::test]
async fn a_pull_of_a_branch_with_no_upstream_is_refused() {
    let (state, dir) = app_state("pull-no-upstream");
    let repo = repo_with("pull-no-upstream-repo", &[(b"a.txt", b"one\n")]);
    let project = checked_project(&state, &dir, &repo);
    for merge in [false, true] {
        let answer = pull_project_now(&state, project.clone(), merge).await;
        assert_eq!(refusal_code(answer).0, error_code::NO_UPSTREAM_TO_PULL);
        let recorded = state.git_status("p1").and_then(|status| status.error);
        assert_eq!(recorded, None, "a refusal is not the badge's failure");
    }
}

#[tokio::test]
async fn a_pull_of_a_detached_head_is_refused() {
    let pulled = Pulled::new("pull-detached");
    git(&pulled.repo, &["checkout", "-q", "--detach"]);
    for merge in [false, true] {
        let answer = pulled.pull(merge).await;
        assert_eq!(refusal_code(answer).0, error_code::NOT_ON_A_BRANCH);
        assert_eq!(pulled.badge_error(), None);
    }
}

#[tokio::test]
async fn a_pull_of_a_directory_that_is_not_a_repository_is_refused() {
    let (state, dir) = app_state("pull-not-a-repository");
    let plain = dir.join("plain");
    std::fs::create_dir(&plain).unwrap();
    let project = checked_project(&state, &dir, &plain);
    for merge in [false, true] {
        let answer = pull_project_now(&state, project.clone(), merge).await;
        assert_eq!(refusal_code(answer).0, error_code::NOT_A_GIT_REPOSITORY);
    }
}

#[tokio::test]
async fn a_pull_without_git_is_refused_as_git_not_found() {
    let (state, dir) = app_state("pull-no-git");
    let repo = repo_with("pull-no-git-repo", &[(b"a.txt", b"one\n")]);
    let project = checked_project(&state, &dir, &repo);
    state.host_tools.replace(Tools {
        git: git_not_found,
        fresh_git: git_not_found,
        ..state.host_tools.get()
    });
    for merge in [false, true] {
        let answer = pull_project_now(&state, project.clone(), merge).await;
        assert_eq!(refusal_code(answer).0, error_code::BINARY_NOT_FOUND);
    }
}

/// A check that cannot fetch pulls nothing, though the refs it has say the branch is behind.
#[tokio::test]
async fn a_pull_whose_fetch_fails_is_refused_and_fast_forwards_nothing() {
    let pulled = Pulled::new("pull-fetch-fails");
    pulled.upstream_commit("b.txt", "theirs\n");
    pulled.fetch();
    git(
        &pulled.repo,
        &["remote", "set-url", "origin", "/nonexistent"],
    );
    let head = pulled.head();
    let (code, detail) = refusal_code(pulled.pull(false).await);
    assert_eq!(code, error_code::GIT_PULL_FAILED);
    assert!(!detail.is_empty());
    assert_eq!(pulled.head(), head);
    let status = pulled.state.git_status("p1").unwrap();
    assert_eq!(status.behind, 1, "the badge still shows what was read");
    assert!(
        status.error.is_some(),
        "a failed fetch is recorded as any check's"
    );
}

/// The fast-forward `git` refuses (a local change it would overwrite) is reported with its
/// message, and the badge does not carry it.
#[tokio::test]
async fn a_fast_forward_git_refuses_is_a_failed_pull_not_recorded_on_the_badge() {
    let pulled = Pulled::new("pull-ff-refused");
    pulled.upstream_commit("a.txt", "theirs\n");
    std::fs::write(pulled.repo.join("a.txt"), "local edit\n").unwrap();
    let head = pulled.head();
    let (code, detail) = refusal_code(pulled.pull(false).await);
    assert_eq!(code, error_code::GIT_PULL_FAILED);
    assert!(detail.contains("a.txt"), "{detail}");
    assert_eq!(pulled.head(), head);
    assert_eq!(pulled.state.git_status("p1").unwrap().error, None);
}

/// A merge the person left unfinished is theirs: the request is refused as a failed pull and
/// nothing of the unfinished merge is aborted, resolved or moved.
#[tokio::test]
async fn a_merge_already_in_progress_is_refused_and_left_as_the_person_has_it() {
    let pulled = Pulled::new("pull-merge-in-progress");
    commit(&pulled.repo, "a.txt", "mine\n");
    pulled.upstream_commit("a.txt", "theirs\n");
    pulled.fetch();
    let begun = crate::test_support::fixture_git(&pulled.repo)
        .args(["merge", "--no-edit", "@{upstream}"])
        .output()
        .unwrap();
    assert!(!begun.status.success(), "the fixture merge conflicts");
    let merge_head = || std::fs::read_to_string(pulled.repo.join(".git/MERGE_HEAD")).unwrap();
    let (head, merging, conflicted) = (
        pulled.head(),
        merge_head(),
        std::fs::read_to_string(pulled.repo.join("a.txt")).unwrap(),
    );
    let (code, detail) = refusal_code(pulled.pull(true).await);
    assert_eq!(code, error_code::GIT_PULL_FAILED);
    assert!(!detail.is_empty());
    assert_eq!(pulled.head(), head);
    assert_eq!(merge_head(), merging, "the merge is still under way");
    assert_eq!(
        std::fs::read_to_string(pulled.repo.join("a.txt")).unwrap(),
        conflicted
    );
    assert_eq!(pulled.badge_error(), None);
}

/// A merge `git` cannot commit for want of a committer is refused with its message and leaves
/// the branch and the tree as they were.
#[tokio::test]
async fn a_merge_without_a_committer_identity_is_a_failed_pull_and_changes_nothing() {
    let pulled = Pulled::new("pull-merge-no-identity");
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    // Empty, not unset: an identity of the machine's own would otherwise stand in for it.
    for key in ["user.name", "user.email"] {
        git(&pulled.repo, &["config", key, ""]);
    }
    let head = pulled.head();
    let (code, detail) = refusal_code(pulled.pull(true).await);
    assert_eq!(code, error_code::GIT_PULL_FAILED);
    assert!(!detail.is_empty());
    assert_eq!(pulled.head(), head);
    assert!(pulled.clean());
    assert!(!pulled.repo.join(".git/MERGE_HEAD").exists());
    assert!(!pulled.repo.join("c.txt").exists());
    assert_eq!(pulled.badge_error(), None);
}

/// The merge is the one the person confirmed whatever `pull.rebase` or `pull.ff` say.
#[tokio::test]
async fn a_confirmed_pull_merges_whatever_the_pull_configuration_says() {
    let pulled = Pulled::new("pull-merge-configuration");
    for (key, value) in [("pull.rebase", "true"), ("pull.ff", "only")] {
        git(&pulled.repo, &["config", key, value]);
    }
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    assert_eq!(pulled.pull(true).await, Ok(PullOutcome::Merged));
    git(&pulled.repo, &["rev-parse", "--verify", "HEAD^2"]);
    assert!(pulled.repo.join("b.txt").exists() && pulled.repo.join("c.txt").exists());
}

/// A branch `git` cannot read is a failed pull, never a guess from the numbers the badge held.
#[tokio::test]
async fn a_pull_that_cannot_read_the_branch_is_a_failed_pull() {
    let pulled = Pulled::new("pull-unreadable");
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    pulled.state.publish_git_status(GitStatus {
        project: "p1".to_string(),
        repository: true,
        branch: Some("main".to_string()),
        detached: false,
        upstream: Some("origin/main".to_string()),
        ahead: 0,
        behind: 1,
        activity: GitActivity::Idle,
        error: None,
        error_code: None,
    });
    std::fs::write(pulled.repo.join(".git/index"), b"not an index").unwrap();
    let head = pulled.head();
    for merge in [false, true] {
        let (code, detail) = refusal_code(pulled.pull(merge).await);
        assert_eq!(code, error_code::GIT_PULL_FAILED, "merge: {merge}");
        assert!(!detail.is_empty(), "merge: {merge}");
        assert_eq!(pulled.head(), head, "merge: {merge}");
    }
}

/// The user's own pull is past the one-minute floor and does not wait for the setting.
#[tokio::test]
async fn a_manual_pull_ignores_the_floor_and_the_setting() {
    let pulled = Pulled::new("pull-floor-and-setting");
    pulled
        .state
        .store
        .set_auto_pull_repositories(false)
        .unwrap();
    assert_eq!(pulled.pull(false).await, Ok(PullOutcome::UpToDate));
    assert!(
        pulled.state.git_check_completed_at("p1").is_some(),
        "a check just completed"
    );
    pulled.upstream_commit("b.txt", "theirs\n");
    assert_eq!(pulled.pull(false).await, Ok(PullOutcome::FastForwarded));
}

/// A sweep's automatic pull stays what it was: it never merges, and a diverged branch passes
/// without a word, the refs left as they were and no failure on the badge.
#[test]
fn an_automatic_check_leaves_a_diverged_branch_alone_and_silent() {
    let pulled = Pulled::new("pull-automatic-diverged");
    pulled.state.store.set_auto_pull_repositories(true).unwrap();
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    let head = pulled.head();
    check_project(&pulled.state, &pulled.project, Asked::Sweep);
    let status = pulled.state.git_status("p1").expect("the check published");
    assert_eq!((status.ahead, status.behind), (1, 1));
    assert_eq!((status.error, status.error_code), (None, None));
    assert_eq!(pulled.head(), head);
    assert!(pulled.clean());
}

/// A pull asked for while another check of the project runs is not answered by that check but
/// by the one it is owed, which looks again: here the running check finds no `git`, the
/// pull's own finds it.
#[tokio::test]
async fn a_pull_during_a_running_check_is_answered_by_its_own_check_once_that_ends() {
    let pulled = Pulled::new("pull-during-check");
    pulled.upstream_commit("b.txt", "theirs\n");
    pulled.state.host_tools.replace(Tools {
        git: git_not_found,
        ..pulled.state.host_tools.get()
    });
    let mut claim = pulled
        .state
        .claim_git_check_queued("p1")
        .expect("the sweep's claim");
    assert_eq!(claim.start(), None, "the sweep's check runs");

    let pull = pulled.pull(false);
    tokio::pin!(pull);
    // One poll registers the request: a zero timeout polls the future once before giving up.
    assert!(tokio::time::timeout(Duration::ZERO, &mut pull)
        .await
        .is_err());
    let (state, project) = (pulled.state.clone(), pulled.project.clone());
    tokio::task::spawn_blocking(move || run_checks(&state, project, claim, Asked::Sweep))
        .await
        .unwrap();
    let answer = tokio::time::timeout(crate::test_support::PATIENCE, pull)
        .await
        .expect("answered once the rerun ended");
    assert_eq!(answer, Ok(PullOutcome::FastForwarded));
    let theirs = String::from_utf8(git(&pulled.other, &["rev-parse", "HEAD"])).unwrap();
    assert_eq!(pulled.head(), theirs.trim());
    assert!(pulled.state.claim_git_check_queued("p1").is_some());
}

/// A check abandoned before it ends (its claim dropped, as a removed project's or a panic's
/// is) refuses the pull waiting on it instead of leaving it waiting.
#[tokio::test]
async fn a_pull_whose_check_is_abandoned_is_refused() {
    let pulled = Pulled::new("pull-abandoned");
    let claim = pulled
        .state
        .claim_git_check_queued("p1")
        .expect("the sweep's claim");
    let pull = pulled.pull(false);
    tokio::pin!(pull);
    assert!(tokio::time::timeout(Duration::ZERO, &mut pull)
        .await
        .is_err());
    drop(claim);
    let answer = tokio::time::timeout(crate::test_support::PATIENCE, pull)
        .await
        .expect("the wait ended with the claim");
    assert_eq!(refusal_code(answer).0, error_code::GIT_PULL_FAILED);
}

/// A pull whose check does not end within `MANUAL_ANSWER_WAIT` is refused, the check going on.
#[tokio::test(start_paused = true)]
async fn a_pull_whose_check_outlasts_the_wait_is_refused() {
    let pulled = Pulled::new("pull-timeout");
    let mut claim = pulled
        .state
        .claim_git_check_queued("p1")
        .expect("the sweep's claim");
    assert_eq!(claim.start(), None, "the check runs, and does not end");
    // Nothing else is pending, so the paused clock runs on to the end of the wait.
    let answer = pulled.pull(false).await;
    assert_eq!(refusal_code(answer).0, error_code::GIT_PULL_FAILED);
    assert!(
        pulled.state.claim_git_check_queued("p1").is_none(),
        "the check still holds the project"
    );
    drop(claim);
}

/// A confirmed pull never runs beside a check of the project: it waits until the check lets
/// go, then merges.
#[tokio::test]
async fn a_confirmed_pull_waits_for_a_running_check_then_merges() {
    let pulled = Pulled::new("pull-merge-waits");
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    let claim = pulled
        .state
        .claim_git_check_queued("p1")
        .expect("the sweep's claim");
    let head = pulled.head();
    let merge = pulled.pull(true);
    tokio::pin!(merge);
    assert!(
        tokio::time::timeout(Duration::ZERO, &mut merge)
            .await
            .is_err(),
        "not refused, not done"
    );
    assert_eq!(pulled.head(), head, "nothing runs under the check's claim");
    drop(claim);
    let answer = tokio::time::timeout(crate::test_support::PATIENCE, merge)
        .await
        .expect("the merge ran once the check let go");
    assert_eq!(answer, Ok(PullOutcome::Merged));
    git(&pulled.repo, &["rev-parse", "--verify", "HEAD^2"]);
}

/// A pull asked for while a merge runs waits for it, is not answered with the merge's result,
/// and is answered by a check of its own made after the merge.
#[cfg(unix)]
#[tokio::test]
async fn a_pull_asked_for_while_a_merge_runs_is_answered_after_it_by_its_own_check() {
    let pulled = Pulled::new("pull-during-merge");
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    // A hook that holds the merge open until the test opens the pipe it reads.
    let started = pulled._dir.join("merge-started");
    let release = pulled._dir.join("merge-release");
    let made = std::process::Command::new("mkfifo")
        .arg(&release)
        .status()
        .unwrap();
    assert!(made.success());
    git(&pulled.repo, &["config", "core.hooksPath", ".git/hooks"]);
    let hook = pulled.repo.join(".git/hooks/pre-merge-commit");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\n: > '{}'\ncat '{}' > /dev/null\n",
            started.display(),
            release.display()
        ),
    )
    .unwrap();
    crate::test_support::make_executable(&hook);

    let (state, project) = (pulled.state.clone(), pulled.project.clone());
    let merge = tokio::spawn(async move { pull_project_now(&state, project, true).await });
    let deadline = Instant::now() + crate::test_support::PATIENCE;
    while !started.exists() {
        assert!(
            Instant::now() < deadline,
            "the merge never reached its hook"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let pull = pulled.pull(false);
    tokio::pin!(pull);
    assert!(
        tokio::time::timeout(Duration::ZERO, &mut pull)
            .await
            .is_err(),
        "asked for while the merge holds the project"
    );
    tokio::task::spawn_blocking(move || std::fs::write(release, "go").unwrap())
        .await
        .unwrap();
    let merged = tokio::time::timeout(crate::test_support::PATIENCE, merge)
        .await
        .expect("the merge ended")
        .unwrap();
    assert_eq!(merged, Ok(PullOutcome::Merged));
    let pulled_after = tokio::time::timeout(crate::test_support::PATIENCE, pull)
        .await
        .expect("the pull was answered once the merge ended");
    assert_eq!(pulled_after, Ok(PullOutcome::UpToDate));
    git(&pulled.repo, &["rev-parse", "--verify", "HEAD^2"]);
    assert!(pulled.clean());
    assert!(pulled.state.claim_git_check_queued("p1").is_some());
}

#[test]
fn a_pull_is_planned_from_the_branch_header() {
    let header = |upstream: Option<&str>, detached: bool, ahead: u32, behind: u32| BranchHeader {
        branch: Some("main".to_string()),
        detached,
        upstream: upstream.map(str::to_string),
        ahead,
        behind,
        upstream_gone: false,
    };
    let plan = |header: &BranchHeader, merge| plan_pull(header, merge).map_err(|e| refused(e).code);
    let up = Some("origin/main");
    assert_eq!(
        plan(&header(up, false, 0, 0), false),
        Ok(PullPlan::UpToDate)
    );
    assert_eq!(plan(&header(up, false, 3, 0), true), Ok(PullPlan::UpToDate));
    assert_eq!(
        plan(&header(up, false, 0, 2), false),
        Ok(PullPlan::FastForward)
    );
    assert_eq!(
        plan(&header(up, false, 1, 2), false),
        Ok(PullPlan::NeedsMerge {
            ahead: 1,
            behind: 2
        })
    );
    assert_eq!(plan(&header(up, false, 1, 2), true), Ok(PullPlan::Merge));
    assert_eq!(
        plan(&header(None, false, 0, 0), true),
        Err(error_code::NO_UPSTREAM_TO_PULL)
    );
    assert_eq!(
        plan(&header(None, true, 0, 0), true),
        Err(error_code::NOT_ON_A_BRANCH)
    );
    // Detached is refused before anything else is looked at.
    let detached_and_gone = BranchHeader {
        upstream_gone: true,
        ..header(up, true, 0, 0)
    };
    assert_eq!(
        plan(&detached_and_gone, false),
        Err(error_code::NOT_ON_A_BRANCH)
    );
    let gone = BranchHeader {
        upstream_gone: true,
        ..header(up, false, 0, 0)
    };
    assert_eq!(plan(&gone, false), Err(error_code::NO_UPSTREAM_TO_PULL));
    // A branch with no commit yet is left to `git`, which has the upstream's commit or says
    // why it has none.
    let unborn = BranchHeader {
        branch: None,
        ..header(up, false, 0, 0)
    };
    assert_eq!(plan(&unborn, false), Ok(PullPlan::FastForward));
}

fn branch_of(repo: &Path) -> String {
    String::from_utf8(git(repo, &["rev-parse", "--abbrev-ref", "HEAD"]))
        .unwrap()
        .trim()
        .to_string()
}

/// An upstream that is configured but whose branch is gone is refused, whether it was pruned by
/// the check's own fetch or is missing from the refs the confirmed pull reads.
#[tokio::test]
async fn a_pull_of_a_branch_whose_upstream_is_gone_is_refused() {
    let pulled = Pulled::new("pull-upstream-gone");
    let branch = branch_of(&pulled.repo);
    let tracking = format!("refs/remotes/origin/{branch}");
    git(&pulled.repo, &["update-ref", "-d", &tracking]);
    let head = pulled.head();
    let answer = pulled.pull(true).await;
    assert_eq!(refusal_code(answer).0, error_code::NO_UPSTREAM_TO_PULL);

    let remote = pulled._dir.join("remote.git");
    git(
        &remote,
        &["update-ref", "-d", &format!("refs/heads/{branch}")],
    );
    git(&pulled.repo, &["config", "fetch.prune", "true"]);
    let answer = pulled.pull(false).await;
    assert_eq!(refusal_code(answer).0, error_code::NO_UPSTREAM_TO_PULL);
    assert_eq!(pulled.head(), head);
    assert_eq!(pulled.badge_error(), None);
}

/// A branch with no commit yet, whose upstream exists, is fast-forwarded onto it.
#[tokio::test]
async fn a_pull_of_a_branch_with_no_commit_yet_takes_the_upstream_onto_it() {
    let pulled = Pulled::new("pull-unborn");
    let upstream = format!("refs/heads/{}", branch_of(&pulled.repo));
    pulled.on_a_branch_with_no_commit_tracking(&upstream);
    assert_eq!(pulled.pull(true).await, Ok(PullOutcome::FastForwarded));
    assert!(pulled.repo.join("a.txt").exists());
    assert_eq!(branch_of(&pulled.repo), "fresh");
}

/// A detached head or a branch with no upstream is what the person fixes first, whatever else
/// went wrong in the check.
#[tokio::test]
async fn a_refusal_of_the_branch_comes_before_a_failed_fetch() {
    let pulled = Pulled::new("pull-order-fetch");
    let branch = branch_of(&pulled.repo);
    git(
        &pulled.repo,
        &["remote", "set-url", "origin", "/nonexistent"],
    );
    git(&pulled.repo, &["checkout", "-q", "--detach"]);
    let answer = pulled.pull(false).await;
    assert_eq!(refusal_code(answer).0, error_code::NOT_ON_A_BRANCH);
    git(&pulled.repo, &["checkout", "-q", &branch]);
    git(
        &pulled.repo,
        &["config", "--unset", &format!("branch.{branch}.merge")],
    );
    let answer = pulled.pull(false).await;
    assert_eq!(refusal_code(answer).0, error_code::NO_UPSTREAM_TO_PULL);
}

/// A branch that cannot be read is reported as that, not as the fetch that failed beside it.
#[tokio::test]
async fn a_header_that_cannot_be_read_is_reported_before_a_failed_fetch() {
    let pulled = Pulled::new("pull-order-read");
    git(
        &pulled.repo,
        &["remote", "set-url", "origin", "/nonexistent"],
    );
    std::fs::write(pulled.repo.join(".git/index"), b"not an index").unwrap();
    let (code, detail) = refusal_code(pulled.pull(false).await);
    assert_eq!(code, error_code::GIT_PULL_FAILED);
    assert!(
        !detail.is_empty() && !detail.contains("nonexistent"),
        "{detail}"
    );
}

/// The merge is a committed merge commit, with `git`'s own subject for a remote-tracking branch
/// merged into `main` (which names no destination), whatever `merge.ff` and the branch's
/// `mergeOptions` say.
#[tokio::test]
async fn a_confirmed_pull_is_a_merge_commit_whatever_the_merge_configuration_says() {
    let pulled = Pulled::new("pull-merge-options");
    let branch = branch_of(&pulled.repo);
    for (key, value) in [
        ("merge.ff".to_string(), "only"),
        (
            format!("branch.{branch}.mergeOptions"),
            "--squash --no-commit",
        ),
    ] {
        git(&pulled.repo, &["config", &key, value]);
    }
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    assert_eq!(pulled.pull(true).await, Ok(PullOutcome::Merged));
    git(&pulled.repo, &["rev-parse", "--verify", "HEAD^2"]);
    assert!(pulled.clean());
    assert!(
        !pulled.repo.join(".git/MERGE_HEAD").exists(),
        "committed, not left under way"
    );
    let subject = String::from_utf8(git(&pulled.repo, &["log", "-1", "--format=%s"])).unwrap();
    assert_eq!(subject.trim(), "Merge remote-tracking branch 'origin/main'");
}

/// `--ff-only` among the branch's `mergeOptions`, with `pull.rebase` and `pull.ff` set against a
/// merge as well, still gives a committed merge of two parents, a clean tree and no merge under
/// way.
#[tokio::test]
async fn a_confirmed_pull_is_a_two_parent_commit_whatever_ff_only_and_the_pull_settings_say() {
    let pulled = Pulled::new("pull-merge-ff-only");
    let branch = branch_of(&pulled.repo);
    for (key, value) in [
        (format!("branch.{branch}.mergeOptions"), "--ff-only"),
        ("merge.ff".to_string(), "only"),
        ("pull.rebase".to_string(), "true"),
        ("pull.ff".to_string(), "only"),
    ] {
        git(&pulled.repo, &["config", &key, value]);
    }
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    assert_eq!(pulled.pull(true).await, Ok(PullOutcome::Merged));
    let parents = String::from_utf8(git(
        &pulled.repo,
        &["rev-list", "--parents", "-n", "1", "HEAD"],
    ))
    .unwrap();
    assert_eq!(
        parents.split_whitespace().count(),
        3,
        "the commit and two parents"
    );
    assert!(pulled.clean());
    assert!(!pulled.merge_head_file().exists());
}

/// The subject is `git`'s own for a merge of a remote-tracking branch, named from the upstream's
/// short name, whichever remote it is on and whatever the upstream branch is called; merged into
/// `main` it names no destination.
#[tokio::test]
async fn the_merge_subject_names_an_upstream_on_another_remote_under_another_branch_name() {
    let pulled = Pulled::new("pull-merge-subject-other-remote");
    let bare = pulled._dir.join("backup.git");
    git(
        &pulled._dir,
        &["init", "-q", "--bare", bare.to_str().unwrap()],
    );
    git(
        &pulled.repo,
        &["remote", "add", "backup", bare.to_str().unwrap()],
    );
    git(&pulled.repo, &["push", "-q", "-u", "backup", "HEAD:shared"]);
    git(
        &pulled.other,
        &["remote", "add", "backup", bare.to_str().unwrap()],
    );
    git(&pulled.other, &["fetch", "-q", "backup"]);
    git(
        &pulled.other,
        &["checkout", "-q", "-b", "work", "backup/shared"],
    );
    commit(&pulled.other, "c.txt", "theirs\n");
    git(&pulled.other, &["push", "-q", "backup", "HEAD:shared"]);
    commit(&pulled.repo, "b.txt", "mine\n");
    git(&pulled.repo, &["fetch", "-q", "backup"]);
    assert_eq!(pulled.pull(true).await, Ok(PullOutcome::Merged));
    let subject = String::from_utf8(git(&pulled.repo, &["log", "-1", "--format=%s"])).unwrap();
    assert_eq!(
        subject.trim(),
        "Merge remote-tracking branch 'backup/shared'"
    );
    let theirs = String::from_utf8(git(&pulled.other, &["rev-parse", "HEAD"])).unwrap();
    let merged = String::from_utf8(git(&pulled.repo, &["rev-parse", "HEAD^2"])).unwrap();
    assert_eq!(merged.trim(), theirs.trim());
    assert!(pulled.clean());
}

/// An upstream on this repository (`branch.<name>.remote` is `.`) is a branch, not a
/// remote-tracking branch, and is worded as `git` words it, with the destination named.
#[tokio::test]
async fn the_merge_subject_of_a_local_upstream_names_a_branch() {
    let pulled = Pulled::new("pull-merge-subject-local");
    git(&pulled.repo, &["checkout", "-q", "-b", "topic"]);
    commit(&pulled.repo, "b.txt", "mine\n");
    git(&pulled.repo, &["checkout", "-q", "main"]);
    commit(&pulled.repo, "c.txt", "theirs\n");
    git(&pulled.repo, &["checkout", "-q", "topic"]);
    for (key, value) in [
        ("branch.topic.remote", "."),
        ("branch.topic.merge", "refs/heads/main"),
    ] {
        git(&pulled.repo, &["config", key, value]);
    }
    assert_eq!(pulled.pull(true).await, Ok(PullOutcome::Merged));
    let subject = String::from_utf8(git(&pulled.repo, &["log", "-1", "--format=%s"])).unwrap();
    assert_eq!(subject.trim(), "Merge branch 'main' into topic");
    assert!(pulled.clean());
}

/// The subject names the branch the linked worktree is on, not the repository's main one.
#[tokio::test]
async fn the_merge_subject_names_the_branch_of_a_linked_worktree() {
    let pulled = Pulled::linked("pull-merge-subject-linked");
    let upstream = format!("origin/{}", branch_of(&pulled.other));
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    assert_eq!(pulled.pull(true).await, Ok(PullOutcome::Merged));
    let subject = String::from_utf8(git(&pulled.repo, &["log", "-1", "--format=%s"])).unwrap();
    assert_eq!(
        subject.trim(),
        format!("Merge remote-tracking branch '{upstream}' into linked")
    );
}

/// The merge commit's subject of the project's current diverged state, as the pull wrote it.
fn merge_subject(pulled: &Pulled) -> String {
    String::from_utf8(git(&pulled.repo, &["log", "-1", "--format=%s"]))
        .unwrap()
        .trim()
        .to_string()
}

/// Leaves the project on `feature`, tracking the remote's `main`, diverged from it by one commit
/// each way, with the remote-tracking branch fetched.
fn diverged_on_feature(pulled: &Pulled) {
    git(
        &pulled.repo,
        &["checkout", "-q", "-b", "feature", "origin/main"],
    );
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
}

/// `git fmt-merge-msg` leaves the destination out of a branch matching `merge.suppressDest`
/// (`main` and `master` by default), so a branch of another name is named.
#[tokio::test]
async fn the_merge_subject_names_the_destination_of_a_branch_that_git_does_not_suppress() {
    let pulled = Pulled::new("pull-merge-subject-feature");
    diverged_on_feature(&pulled);
    assert_eq!(pulled.pull(true).await, Ok(PullOutcome::Merged));
    assert_eq!(
        merge_subject(&pulled),
        "Merge remote-tracking branch 'origin/main' into feature"
    );
}

/// The user's `merge.suppressDest` decides which destinations are left out, not a list of the
/// daemon's own: it can leave out a branch `git` would name by default.
#[tokio::test]
async fn the_merge_subject_leaves_out_the_destination_the_user_configured_to_suppress() {
    let pulled = Pulled::new("pull-merge-subject-suppress-feature");
    diverged_on_feature(&pulled);
    git(&pulled.repo, &["config", "merge.suppressDest", "feature"]);
    assert_eq!(pulled.pull(true).await, Ok(PullOutcome::Merged));
    assert_eq!(
        merge_subject(&pulled),
        "Merge remote-tracking branch 'origin/main'"
    );
}

/// ... and it can name a branch `git` leaves out by default: a setting that does not match `main`
/// replaces the default.
#[tokio::test]
async fn the_merge_subject_names_main_when_the_user_configured_other_destinations_to_be_suppressed()
{
    let pulled = Pulled::new("pull-merge-subject-suppress-other");
    git(&pulled.repo, &["config", "merge.suppressDest", "feature"]);
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    assert_eq!(pulled.pull(true).await, Ok(PullOutcome::Merged));
    assert_eq!(
        merge_subject(&pulled),
        "Merge remote-tracking branch 'origin/main' into main"
    );
}

/// A local upstream merged into `main` (or `master`) names no destination either.
#[tokio::test]
async fn the_merge_subject_of_a_local_upstream_names_no_destination_on_main() {
    let pulled = Pulled::new("pull-merge-subject-local-main");
    git(&pulled.repo, &["branch", "topic"]);
    git(&pulled.repo, &["checkout", "-q", "topic"]);
    commit(&pulled.repo, "c.txt", "theirs\n");
    git(&pulled.repo, &["checkout", "-q", "main"]);
    commit(&pulled.repo, "b.txt", "mine\n");
    for (key, value) in [
        ("branch.main.remote", "."),
        ("branch.main.merge", "refs/heads/topic"),
    ] {
        git(&pulled.repo, &["config", key, value]);
    }
    assert_eq!(pulled.pull(true).await, Ok(PullOutcome::Merged));
    assert_eq!(merge_subject(&pulled), "Merge branch 'topic'");
}

/// An upstream that is neither a local branch nor a remote-tracking one (here a tag of this
/// repository) is not merged, and nothing is left behind.
#[tokio::test]
async fn an_upstream_that_is_not_a_branch_is_refused_and_nothing_is_merged() {
    let pulled = Pulled::new("pull-merge-upstream-tag");
    git(&pulled.repo, &["checkout", "-q", "-b", "tagged"]);
    commit(&pulled.repo, "c.txt", "theirs\n");
    git(&pulled.repo, &["tag", "v1"]);
    git(&pulled.repo, &["checkout", "-q", "main"]);
    commit(&pulled.repo, "b.txt", "mine\n");
    for (key, value) in [
        ("branch.main.remote", "."),
        ("branch.main.merge", "refs/tags/v1"),
    ] {
        git(&pulled.repo, &["config", key, value]);
    }
    let head = pulled.head();
    let (code, detail) = refusal_code(pulled.pull(true).await);
    assert_eq!(code, error_code::GIT_PULL_FAILED);
    assert!(detail.contains("refs/tags/v1"), "{detail}");
    assert_eq!(pulled.head(), head);
    assert!(pulled.clean());
    assert!(!pulled.repo.join("c.txt").exists());
    assert!(!pulled.merge_head_file().exists());
}

/// The files `git fmt-merge-msg` is given are removed once the merge has been worded, whether the
/// merge then worked or met conflicts. A file is the pull's own when it names the commit the
/// merge takes in.
fn left_behind_for(upstream: &str) -> Vec<PathBuf> {
    std::fs::read_dir(std::env::temp_dir())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("octoboard-merge-")
        })
        .map(|entry| entry.path())
        .filter(|path| std::fs::read_to_string(path).is_ok_and(|text| text.contains(upstream)))
        .collect()
}

#[tokio::test]
async fn the_file_the_merge_message_is_worded_from_is_removed_after_a_merge() {
    let pulled = Pulled::new("pull-merge-message-file");
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c-message-file.txt", "theirs\n");
    pulled.fetch();
    let upstream = String::from_utf8(git(&pulled.repo, &["rev-parse", "@{upstream}"])).unwrap();
    assert_eq!(pulled.pull(true).await, Ok(PullOutcome::Merged));
    assert_eq!(left_behind_for(upstream.trim()), Vec::<PathBuf>::new());
}

#[tokio::test]
async fn the_file_the_merge_message_is_worded_from_is_removed_after_a_merge_that_fails() {
    let pulled = Pulled::new("pull-merge-message-file-conflict");
    commit(&pulled.repo, "a.txt", "mine\n");
    pulled.upstream_commit("a.txt", "theirs, message file\n");
    pulled.fetch();
    let upstream = String::from_utf8(git(&pulled.repo, &["rev-parse", "@{upstream}"])).unwrap();
    assert_eq!(
        refusal_code(pulled.pull(true).await).0,
        error_code::PULL_MERGE_CONFLICT
    );
    assert_eq!(left_behind_for(upstream.trim()), Vec::<PathBuf>::new());
}

/// What a plain `git merge --no-edit origin/main` commits in `pulled`'s repository, under the
/// configuration it has now: the merge is made for real and taken back, so the head is as before.
fn plain_merge_message(pulled: &Pulled) -> String {
    let head = pulled.head();
    git(&pulled.repo, &["merge", "--no-edit", "origin/main"]);
    let message = String::from_utf8(git(&pulled.repo, &["log", "-1", "--format=%B"])).unwrap();
    git(&pulled.repo, &["reset", "-q", "--hard", &head]);
    message
}

/// The message the confirmed pull committed.
fn merge_message_of(pulled: &Pulled) -> String {
    String::from_utf8(git(&pulled.repo, &["log", "-1", "--format=%B"])).unwrap()
}

/// With `merge.log` on, the confirmed pull commits what a plain `git merge --no-edit` writes in
/// the same repository: `git`'s subject, one shortlog of the upstream branch, no second one
/// labelled with a commit, and none of the `# By` and `# Via` comment lines `git fmt-merge-msg`
/// credits the authors with (the upstream commits are by another author here), whichever
/// character comments are written with.
#[tokio::test]
async fn a_confirmed_pull_with_merge_log_on_commits_the_message_a_plain_merge_writes() {
    for (label, comment_char) in [
        ("pull-merge-log", "#"),
        ("pull-merge-log-semicolon", ";"),
        ("pull-merge-log-auto", "auto"),
    ] {
        let pulled = Pulled::new(label);
        git(&pulled.repo, &["config", "merge.log", "true"]);
        git(&pulled.repo, &["config", "core.commentChar", comment_char]);
        commit(&pulled.repo, "b.txt", "mine\n");
        pulled.upstream_commit("c.txt", "theirs\n");
        pulled.upstream_commit("d.txt", "theirs too\n");
        pulled.fetch();
        let expected = plain_merge_message(&pulled);
        // The comparison is no good if the plain merge did not write a shortlog.
        assert_eq!(expected.matches("* origin/main:").count(), 1, "{expected}");
        assert_eq!(pulled.pull(true).await, Ok(PullOutcome::Merged));
        let message = merge_message_of(&pulled);
        assert_eq!(message, expected, "core.commentChar {comment_char}");
        assert_eq!(message.matches("* origin/main:").count(), 1, "{message}");
        assert!(!message.contains("* commit '"), "{message}");
        assert!(
            !message.contains("# By") && !message.contains("# Via"),
            "{message}"
        );
    }
}

/// Installs a `prepare-commit-msg` hook running `body` on the message file `$1`.
fn prepare_commit_msg_hook(pulled: &Pulled, body: &str) {
    git(&pulled.repo, &["config", "core.hooksPath", ".git/hooks"]);
    let hook = pulled.repo.join(".git/hooks/prepare-commit-msg");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(&hook, format!("#!/bin/sh\n{body}\n")).unwrap();
    crate::test_support::make_executable(&hook);
}

/// What a `prepare-commit-msg` hook writes into a merge message stays there, comment lines
/// included, as it does for a plain `git merge --no-edit`, and the merge still succeeds.
#[tokio::test]
async fn a_confirmed_pull_keeps_what_a_prepare_commit_msg_hook_writes_as_a_plain_merge_does() {
    let hooks = [
        (
            "pull-merge-hook-subject",
            "printf '#42 ' | cat - \"$1\" > \"$1.new\" && mv \"$1.new\" \"$1\"",
            "#42 Merge remote-tracking branch 'origin/main'",
        ),
        (
            "pull-merge-hook-trailer",
            "printf '# Generated-by: tool\\n' >> \"$1\"",
            "# Generated-by: tool",
        ),
    ];
    for (label, body, written) in hooks {
        for merge_log in ["false", "true"] {
            let pulled = Pulled::new(&format!("{label}-{merge_log}"));
            git(&pulled.repo, &["config", "merge.log", merge_log]);
            prepare_commit_msg_hook(&pulled, body);
            commit(&pulled.repo, "b.txt", "mine\n");
            pulled.upstream_commit("c.txt", "theirs\n");
            pulled.fetch();
            let expected = plain_merge_message(&pulled);
            // The comparison is no good if the hook did nothing to the plain merge.
            assert!(expected.contains(written), "{expected}");
            assert_eq!(pulled.pull(true).await, Ok(PullOutcome::Merged), "{label}");
            assert_eq!(
                merge_message_of(&pulled),
                expected,
                "{label}, merge.log {merge_log}"
            );
        }
    }
}

/// `--log` among the branch's `mergeOptions` alone, with no `merge.log`, adds no shortlog: the
/// message is `git`'s subject. (A plain `git merge` would add one; the confirmed pull does not
/// take the option, which is accepted.)
#[tokio::test]
async fn a_confirmed_pull_with_log_only_among_the_merge_options_commits_the_subject_alone() {
    let pulled = Pulled::new("pull-merge-options-log");
    let branch = branch_of(&pulled.repo);
    git(
        &pulled.repo,
        &["config", &format!("branch.{branch}.mergeOptions"), "--log"],
    );
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    assert_eq!(pulled.pull(true).await, Ok(PullOutcome::Merged));
    assert_eq!(
        merge_message_of(&pulled).trim_end(),
        "Merge remote-tracking branch 'origin/main'"
    );
}

/// A merge state `git` cannot be asked about is not guessed to be absent.
#[tokio::test]
async fn a_merge_state_that_cannot_be_read_refuses_the_merge() {
    let pulled = Pulled::new("pull-merge-state-unread");
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    std::fs::create_dir(pulled.repo.join(".git/MERGE_HEAD")).unwrap();
    let head = pulled.head();
    let (code, detail) = refusal_code(pulled.pull(true).await);
    assert_eq!(code, error_code::GIT_PULL_FAILED);
    assert!(detail.contains("could not be read"), "{detail}");
    assert_eq!(pulled.head(), head);
    assert!(!pulled.repo.join("c.txt").exists());
}

/// A `MERGE_HEAD` larger than any object id is not read whole and not taken for no merge: the
/// state is unread, the merge refused, and the file left as it was.
#[tokio::test]
async fn an_oversized_merge_head_refuses_the_merge_and_is_left_alone() {
    let pulled = Pulled::new("pull-merge-head-oversized");
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    let junk = "0".repeat(1 << 20);
    std::fs::write(pulled.repo.join(".git/MERGE_HEAD"), &junk).unwrap();
    let head = pulled.head();
    let (code, detail) = refusal_code(pulled.pull(true).await);
    assert_eq!(code, error_code::GIT_PULL_FAILED);
    assert!(detail.contains("could not be read"), "{detail}");
    assert_eq!(pulled.head(), head);
    assert!(!pulled.repo.join("c.txt").exists());
    assert_eq!(
        std::fs::read_to_string(pulled.repo.join(".git/MERGE_HEAD")).unwrap(),
        junk
    );
}

/// A `MERGE_HEAD` that is not a regular file (a FIFO, whose open would wait for a writer) fails
/// closed instead of holding the pull or counting as no merge.
#[cfg(unix)]
#[tokio::test]
async fn a_merge_head_that_is_not_a_regular_file_refuses_the_merge_without_waiting() {
    let pulled = Pulled::new("pull-merge-head-fifo");
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    crate::test_support::mkfifo(&pulled.repo.join(".git/MERGE_HEAD"));
    let head = pulled.head();
    let (code, detail) = refusal_code(pulled.pull(true).await);
    assert_eq!(code, error_code::GIT_PULL_FAILED);
    assert!(detail.contains("could not be read"), "{detail}");
    assert_eq!(pulled.head(), head);
    assert!(!pulled.repo.join("c.txt").exists());
}

/// A merge state that cannot be read in a linked worktree (whose git directory is not under the
/// project's path) fails closed too.
#[tokio::test]
async fn an_unreadable_merge_state_in_a_linked_worktree_refuses_the_merge() {
    let pulled = Pulled::linked("pull-merge-head-linked-unread");
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    std::fs::create_dir(pulled.merge_head_file()).unwrap();
    let head = pulled.head();
    let (code, detail) = refusal_code(pulled.pull(true).await);
    assert_eq!(code, error_code::GIT_PULL_FAILED);
    assert!(detail.contains("could not be read"), "{detail}");
    assert_eq!(pulled.head(), head);
    assert!(!pulled.repo.join("c.txt").exists());
}

/// A submodule whose commit both sides moved differently is a conflicted path like any file: the
/// merge is aborted and the submodule's path is named.
#[tokio::test]
async fn a_merge_with_a_conflicted_submodule_is_aborted_and_names_it() {
    let pulled = Pulled::new("pull-conflict-submodule");
    let module = repo_with("pull-conflict-submodule-module", &[(b"s", b"1\n")]);
    // A local submodule is a `file` transport, which `git` refuses unless it is allowed.
    let allowing = |dir: &Path, args: &[&str]| {
        let mut all = vec!["-c", "protocol.file.allow=always"];
        all.extend_from_slice(args);
        git(dir, &all)
    };
    allowing(
        &pulled.repo,
        &["submodule", "add", "-q", module.to_str().unwrap(), "sm"],
    );
    git(&pulled.repo, &["commit", "-q", "-m", "add the submodule"]);
    git(&pulled.repo, &["push", "-q", "origin", "HEAD"]);
    git(&pulled.other, &["pull", "-q"]);
    allowing(&pulled.other, &["submodule", "update", "-q", "--init"]);
    commit(&pulled.other.join("sm"), "t", "theirs\n");
    // Published, so that the fetch that recurses into the submodule finds the commit the pointer
    // names.
    git(
        &pulled.other.join("sm"),
        &["push", "-q", "origin", "HEAD:refs/heads/theirs"],
    );
    git(&pulled.other, &["add", "sm"]);
    git(&pulled.other, &["commit", "-q", "-m", "move the submodule"]);
    git(&pulled.other, &["push", "-q", "origin", "HEAD"]);
    commit(&pulled.repo.join("sm"), "m", "mine\n");
    git(&pulled.repo, &["add", "sm"]);
    git(&pulled.repo, &["commit", "-q", "-m", "move it elsewhere"]);
    pulled.fetch();
    let head = pulled.head();
    let (code, detail) = refusal_code(pulled.pull(true).await);
    assert_eq!(code, error_code::PULL_MERGE_CONFLICT);
    assert_eq!(detail, "sm");
    assert_eq!(pulled.head(), head);
    assert!(pulled.clean());
    assert!(!pulled.merge_head_file().exists());
}

/// Only the merge the pull began is aborted: one under way for another commit is left as it is,
/// and one for the pull's own commit is aborted and reported.
fn only_the_pulls_own_merge_is_aborted(pulled: &Pulled) {
    commit(&pulled.repo, "a.txt", "mine\n");
    pulled.upstream_commit("a.txt", "theirs\n");
    pulled.fetch();
    let theirs = String::from_utf8(git(&pulled.repo, &["rev-parse", "@{upstream}"])).unwrap();
    let theirs = theirs.trim();
    let begun = crate::test_support::fixture_git(&pulled.repo)
        .args(["merge", "--no-edit", theirs])
        .output()
        .unwrap();
    assert!(!begun.status.success(), "the fixture merge conflicts");
    let runner = crate::project_git::test_git();
    let merge_head = pulled.merge_head_file();
    assert!(merge_head.exists(), "the fixture merge is under way");

    let other = "0".repeat(theirs.len());
    let err = merge_failed(&runner, &pulled.repo, "boom", &other);
    assert_eq!(refused(err).code, error_code::GIT_PULL_FAILED);
    assert!(merge_head.exists(), "somebody else's merge is not aborted");

    let err = merge_failed(&runner, &pulled.repo, "boom", theirs);
    let refusal = refused(err);
    assert_eq!(refusal.code, error_code::PULL_MERGE_CONFLICT);
    assert_eq!(
        refusal.params.get("detail").map(String::as_str),
        Some("a.txt")
    );
    assert!(!merge_head.exists());
}

#[test]
fn a_failed_merge_is_aborted_only_when_it_is_the_pulls_own() {
    only_the_pulls_own_merge_is_aborted(&Pulled::new("pull-abort-own"));
}

#[test]
fn a_failed_merge_is_aborted_only_when_it_is_the_pulls_own_in_a_linked_worktree() {
    only_the_pulls_own_merge_is_aborted(&Pulled::linked("pull-abort-own-linked"));
}

/// The merge's wait for the project's claim ends at its deadline on the runtime's clock, so a
/// paused one runs it out, and the check that held the project goes on.
#[tokio::test(start_paused = true)]
async fn a_confirmed_pull_whose_project_stays_checked_is_refused_at_the_deadline() {
    let pulled = Pulled::new("pull-merge-deadline");
    let claim = pulled
        .state
        .claim_git_check_queued("p1")
        .expect("the sweep's claim");
    let answer = pulled.pull(true).await;
    assert_eq!(refusal_code(answer).0, error_code::GIT_PULL_FAILED);
    assert!(pulled.state.claim_git_check_now("p1").is_none());
    drop(claim);
}

/// A project that is a linked worktree is pulled and merged like any other.
#[tokio::test]
async fn a_confirmed_pull_merges_in_a_linked_worktree() {
    let pulled = Pulled::linked("pull-linked-merge");
    assert!(
        pulled.repo.join(".git").is_file(),
        "a linked worktree's .git is a file"
    );
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    assert_eq!(pulled.pull(true).await, Ok(PullOutcome::Merged));
    git(&pulled.repo, &["rev-parse", "--verify", "HEAD^2"]);
    assert!(pulled.repo.join("c.txt").exists());
    assert!(pulled.clean());
    assert!(!pulled.merge_head_file().exists());
}

/// A merge with conflicts in a linked worktree is aborted and named, which needs the merge state
/// read from where `git` keeps it for that worktree.
#[tokio::test]
async fn a_merge_with_conflicts_in_a_linked_worktree_is_aborted_and_named() {
    let pulled = Pulled::linked("pull-linked-conflict");
    commit(&pulled.repo, "a.txt", "mine\n");
    pulled.upstream_commit("a.txt", "theirs\n");
    pulled.fetch();
    let head = pulled.head();
    let (code, detail) = refusal_code(pulled.pull(true).await);
    assert_eq!(code, error_code::PULL_MERGE_CONFLICT);
    assert_eq!(detail, "a.txt");
    assert_eq!(pulled.head(), head);
    assert!(pulled.clean());
    assert!(!pulled.merge_head_file().exists());
    assert_eq!(
        std::fs::read_to_string(pulled.repo.join("a.txt")).unwrap(),
        "mine\n"
    );
}

/// A merge the person left unfinished in a linked worktree is theirs, as in the main one.
#[tokio::test]
async fn a_merge_in_progress_in_a_linked_worktree_is_refused_and_left_as_it_is() {
    let pulled = Pulled::linked("pull-linked-in-progress");
    commit(&pulled.repo, "a.txt", "mine\n");
    pulled.upstream_commit("a.txt", "theirs\n");
    pulled.fetch();
    let begun = crate::test_support::fixture_git(&pulled.repo)
        .args(["merge", "--no-edit", "@{upstream}"])
        .output()
        .unwrap();
    assert!(!begun.status.success(), "the fixture merge conflicts");
    let merge_head = pulled.merge_head_file();
    let (head, merging, conflicted) = (
        pulled.head(),
        std::fs::read_to_string(&merge_head).unwrap(),
        std::fs::read_to_string(pulled.repo.join("a.txt")).unwrap(),
    );
    let (code, detail) = refusal_code(pulled.pull(true).await);
    assert_eq!(code, error_code::GIT_PULL_FAILED);
    assert!(!detail.is_empty());
    assert_eq!(pulled.head(), head);
    assert_eq!(std::fs::read_to_string(&merge_head).unwrap(), merging);
    assert_eq!(
        std::fs::read_to_string(pulled.repo.join("a.txt")).unwrap(),
        conflicted
    );
}

/// The merge is of the commit the upstream is at, not of whatever a name resolves to: a local
/// branch spelled as the upstream (`origin/main`) does not stand in for it.
#[tokio::test]
async fn a_confirmed_pull_merges_the_upstream_not_a_local_branch_of_its_name() {
    let pulled = Pulled::new("pull-merge-shadowed");
    let branch = branch_of(&pulled.repo);
    let before = pulled.head();
    git(
        &pulled.repo,
        &["branch", &format!("origin/{branch}"), &before],
    );
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    let theirs = String::from_utf8(git(&pulled.other, &["rev-parse", "HEAD"])).unwrap();
    assert_eq!(pulled.pull(true).await, Ok(PullOutcome::Merged));
    let merged = String::from_utf8(git(&pulled.repo, &["rev-parse", "HEAD^2"])).unwrap();
    assert_eq!(merged.trim(), theirs.trim());
    assert!(pulled.repo.join("c.txt").exists());
}

/// A branch with commits of its own whose upstream no longer exists is not merged, whichever
/// way the pull is asked for.
#[tokio::test]
async fn a_diverged_branch_whose_upstream_is_gone_is_refused_on_both_paths() {
    let pulled = Pulled::new("pull-diverged-upstream-gone");
    let branch = branch_of(&pulled.repo);
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    git(
        &pulled.repo,
        &["update-ref", "-d", &format!("refs/remotes/origin/{branch}")],
    );
    let head = pulled.head();
    let answer = pulled.pull(true).await;
    assert_eq!(refusal_code(answer).0, error_code::NO_UPSTREAM_TO_PULL);
    assert_eq!(pulled.head(), head);
    assert!(pulled.clean());
    assert!(!pulled.merge_head_file().exists());
    assert!(!pulled.repo.join("c.txt").exists());
    assert_eq!(pulled.badge_error(), None);
}

/// The check's own refusals come in the order the person fixes them, an upstream that is gone
/// before a fetch that failed.
#[tokio::test]
async fn an_upstream_that_is_gone_is_refused_before_a_failed_fetch() {
    let pulled = Pulled::new("pull-order-gone-fetch");
    let branch = branch_of(&pulled.repo);
    git(
        &pulled.repo,
        &["remote", "set-url", "origin", "/nonexistent"],
    );
    git(
        &pulled.repo,
        &["update-ref", "-d", &format!("refs/remotes/origin/{branch}")],
    );
    let answer = pulled.pull(false).await;
    assert_eq!(refusal_code(answer).0, error_code::NO_UPSTREAM_TO_PULL);
}

/// A branch that cannot be read is reported as that, even when it is also detached.
#[tokio::test]
async fn a_header_that_cannot_be_read_is_reported_before_a_detached_head() {
    let pulled = Pulled::new("pull-order-read-detached");
    git(&pulled.repo, &["checkout", "-q", "--detach"]);
    std::fs::write(pulled.repo.join(".git/index"), b"not an index").unwrap();
    let (code, detail) = refusal_code(pulled.pull(false).await);
    assert_eq!(code, error_code::GIT_PULL_FAILED);
    assert!(!detail.is_empty());
}

/// A branch with no commit yet is taken onto its upstream by the pull that checks, as by the
/// confirmed one.
#[tokio::test]
async fn a_checking_pull_of_a_branch_with_no_commit_yet_takes_the_upstream_onto_it() {
    let pulled = Pulled::new("pull-unborn-check");
    let upstream = format!("refs/heads/{}", branch_of(&pulled.repo));
    pulled.on_a_branch_with_no_commit_tracking(&upstream);
    assert_eq!(pulled.pull(false).await, Ok(PullOutcome::FastForwarded));
    assert!(pulled.repo.join("a.txt").exists());
    assert_eq!(branch_of(&pulled.repo), "fresh");
}

/// A branch with no commit yet whose upstream `git` has no commit for is a failed pull with
/// `git`'s message, not a pull of nothing.
#[tokio::test]
async fn a_pull_of_a_branch_with_no_commit_yet_and_no_upstream_commit_is_a_failed_pull() {
    let pulled = Pulled::new("pull-unborn-missing-upstream");
    pulled.on_a_branch_with_no_commit_tracking("refs/heads/never-pushed");
    let (code, detail) = refusal_code(pulled.pull(true).await);
    assert_eq!(code, error_code::GIT_PULL_FAILED);
    assert!(!detail.is_empty());
    let branch = git(&pulled.repo, &["symbolic-ref", "--short", "HEAD"]);
    assert_eq!(String::from_utf8(branch).unwrap().trim(), "fresh");
    assert!(!pulled.repo.join("a.txt").exists());
    assert_eq!(pulled.badge_error(), None);
}

/// A sweep's automatic pull stays what it was for a branch whose upstream is gone: nothing is
/// pulled and the badge carries no failure.
#[test]
fn an_automatic_check_leaves_a_branch_whose_upstream_is_gone_alone_and_silent() {
    let pulled = Pulled::new("pull-automatic-gone");
    pulled.state.store.set_auto_pull_repositories(true).unwrap();
    let branch = branch_of(&pulled.repo);
    git(
        &pulled.repo,
        &["update-ref", "-d", &format!("refs/remotes/origin/{branch}")],
    );
    let head = pulled.head();
    check_project(&pulled.state, &pulled.project, Asked::Sweep);
    let status = pulled.state.git_status("p1").expect("the check published");
    assert_eq!((status.ahead, status.behind), (0, 0));
    assert_eq!((status.error, status.error_code), (None, None));
    assert_eq!(pulled.head(), head);
    assert!(pulled.clean());
}

/// The confirmed merge looks for its project's claim again and again, so one that is released
/// after a few looks is taken and the merge runs; nothing runs while the check holds it.
#[tokio::test(start_paused = true)]
async fn a_confirmed_pull_takes_the_claim_a_check_lets_go_of_while_it_polls() {
    let pulled = Pulled::new("pull-merge-polls");
    commit(&pulled.repo, "b.txt", "mine\n");
    pulled.upstream_commit("c.txt", "theirs\n");
    pulled.fetch();
    let claim = pulled
        .state
        .claim_git_check_queued("p1")
        .expect("the sweep's claim");
    let head = pulled.head();
    let started = Instant::now();
    let merge = pulled.pull(true);
    tokio::pin!(merge);
    tokio::select! {
        _ = &mut merge => panic!("answered while a check held the project"),
        () = tokio::time::sleep(CLAIM_POLL * 3) => {}
    }
    assert_eq!(pulled.head(), head, "nothing runs under the check's claim");
    drop(claim);
    assert_eq!(merge.await, Ok(PullOutcome::Merged));
    assert!(
        started.elapsed() < MANUAL_ANSWER_WAIT,
        "taken well before the deadline"
    );
    git(&pulled.repo, &["rev-parse", "--verify", "HEAD^2"]);
}
