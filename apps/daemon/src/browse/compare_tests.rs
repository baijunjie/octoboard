//! Branch comparisons listed and read end to end against real repositories: which branches there
//! are, what a pair of tips compares, which sides a change has inside and across a project's
//! boundary, and what a comparison reads once its branches have moved, gone, or lost their objects.

use std::path::Path;

use super::change_tests::{present, read_body, read_kind, read_lines, read_source, summary, write};
use super::tests::{code, Fixture};
use super::*;
use crate::protocol::{
    BranchInfo, ChangeEntry, ChangeSide, ComparedChangeRef, ComparisonEndpoint, SideKind, SideRead,
    SideRef, UpstreamInfo,
};
use crate::test_support::{
    fixture_git, git, git_stdin, isolated_git_env, repo_with, ScratchDir, SYSTEM_GIT,
};

/// A comparison as `compare_project_branches` answered it.
#[derive(Debug)]
pub(super) struct Compared {
    pub(super) left: ComparisonEndpoint,
    pub(super) right: ComparisonEndpoint,
    pub(super) base: String,
    pub(super) changes: Vec<ChangeEntry>,
    complete: bool,
}

/// One change of a comparison as `read_project_comparison_change` answered it.
#[derive(Debug)]
pub(super) struct ComparedRead {
    pub(super) old: SideRead,
    pub(super) new: SideRead,
    pub(super) patch: Option<String>,
    /// The whole reply as it goes on the wire.
    json: String,
}

impl Fixture {
    pub(super) fn branches(&self, project: &str) -> anyhow::Result<(Vec<BranchInfo>, bool)> {
        match self.serve(BrowseBody::ListProjectBranches {
            project: project.to_string(),
        })? {
            Event::ProjectBranches {
                branches, complete, ..
            } => Ok((branches, complete)),
            other => panic!("not a branch list: {other:?}"),
        }
    }

    pub(super) fn compare(
        &self,
        project: &str,
        left: &str,
        right: &str,
    ) -> anyhow::Result<Compared> {
        self.compare_from(project, (left, false), right)
    }

    /// As [`Fixture::compare`], with `left` a remote-tracking branch when its flag is set.
    pub(super) fn compare_from(
        &self,
        project: &str,
        (left, left_remote): (&str, bool),
        right: &str,
    ) -> anyhow::Result<Compared> {
        self.compare_with(project, (left, left_remote, false), right)
    }

    /// As [`Fixture::compare`], against the empty tree.
    pub(super) fn compare_from_nothing(
        &self,
        project: &str,
        right: &str,
    ) -> anyhow::Result<Compared> {
        self.compare_with(project, ("", false, true), right)
    }

    fn compare_with(
        &self,
        project: &str,
        (left, left_remote, left_empty): (&str, bool, bool),
        right: &str,
    ) -> anyhow::Result<Compared> {
        match self.serve(BrowseBody::CompareProjectBranches {
            project: project.to_string(),
            left: left.to_string(),
            left_remote,
            left_empty,
            right: right.to_string(),
        })? {
            Event::ProjectComparison {
                left: echoed_left,
                right: echoed_right,
                base,
                changes,
                complete,
                ..
            } => {
                assert_eq!(
                    (echoed_left.branch.as_str(), echoed_right.branch.as_str()),
                    (left, right),
                    "the reply's echo"
                );
                Ok(Compared {
                    left: echoed_left,
                    right: echoed_right,
                    base,
                    changes,
                    complete,
                })
            }
            other => panic!("not a comparison: {other:?}"),
        }
    }

    pub(super) fn compared_change(
        &self,
        project: &str,
        compared: &Compared,
        old: SideRef,
        new: SideRef,
    ) -> anyhow::Result<ComparedRead> {
        self.compared_change_at(
            project,
            (&compared.left, &compared.right, &compared.base),
            old,
            new,
        )
    }

    fn compared_change_at(
        &self,
        project: &str,
        (left, right, base): (&ComparisonEndpoint, &ComparisonEndpoint, &str),
        old: SideRef,
        new: SideRef,
    ) -> anyhow::Result<ComparedRead> {
        let event = self.serve(BrowseBody::ReadProjectComparisonChange {
            project: project.to_string(),
            left: left.clone(),
            right: right.clone(),
            base: base.to_string(),
            change: ComparedChangeRef { old, new },
        })?;
        let json = serde_json::to_string(&event).unwrap();
        match event {
            Event::ProjectComparisonChange {
                left: echoed_left,
                right: echoed_right,
                base: echoed_base,
                old,
                new,
                patch,
                ..
            } => {
                assert_eq!(
                    (&echoed_left, &echoed_right, echoed_base.as_str()),
                    (left, right, base),
                    "the echo"
                );
                Ok(ComparedRead {
                    old: *old,
                    new: *new,
                    patch: patch.map(|patch| String::from_utf8(tests::bytes_of(&patch)).unwrap()),
                    json,
                })
            }
            other => panic!("not a compared change: {other:?}"),
        }
    }
}

/// Commits `files` (a body, or `None` to delete it) on branch `branch`, started from `from`, and
/// checks `main` out again, so the branch is left as no worktree's checkout.
pub(super) fn commit_on(
    repo: &Path,
    branch: &str,
    from: &str,
    files: &[(&str, Option<&[u8]>)],
) -> String {
    git(repo, &["checkout", "-q", "-B", branch, from]);
    for (path, body) in files {
        match body {
            Some(body) => write(repo.join(path), body),
            None => std::fs::remove_file(repo.join(path)).unwrap(),
        }
    }
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", branch]);
    let commit = rev_parse(repo, "HEAD");
    git(repo, &["checkout", "-q", "main"]);
    commit
}

pub(super) fn rev_parse(repo: &Path, revision: &str) -> String {
    String::from_utf8(git(repo, &["rev-parse", revision]))
        .unwrap()
        .trim()
        .to_string()
}

/// The commit and blob a read side came from.
fn commit_and_blob(side: &SideRead) -> (String, String) {
    match read_source(side) {
        ContentSource::Commit {
            commit,
            blob,
            branch: None,
        } => (commit.clone(), blob.clone()),
        other => panic!("not a commit's blob alone: {other:?}"),
    }
}

#[test]
fn local_branches_are_listed_with_their_tips_and_cut_at_their_budget() {
    let fixture = Fixture::new();
    let repo = repo_with("compare-branches", &[(b"a.txt", b"a\n")]);
    let main = rev_parse(&repo, "main");
    let feature = commit_on(&repo, "feature/x", "main", &[("a.txt", Some(b"x\n"))]);
    git(&repo, &["tag", "v1", "main"]);
    git(&repo, &["update-ref", "refs/remotes/origin/main", &main]);
    git(&repo, &["config", "remote.origin.url", "/nowhere"]);
    let project = fixture.project("p", &repo);
    let (branches, complete) = fixture.branches(&project).unwrap();
    assert_eq!(
        branches,
        [
            BranchInfo {
                name: "feature/x".to_string(),
                commit: feature,
                upstream: None,
                origin: None
            },
            BranchInfo {
                name: "main".to_string(),
                commit: main.clone(),
                upstream: Some(UpstreamInfo {
                    name: "origin/main".to_string(),
                    commit: main.clone()
                }),
                origin: None
            }
        ],
        "local branches only, whether checked out or not; main has a configured remote's branch of its name"
    );
    assert!(complete);

    // A branch naming an object the repository does not have, or one that is no commit (a tag
    // object, which `git` itself would never write there), is listed as it is and fails no listing;
    // comparing it is refused.
    let tag = {
        git(&repo, &["tag", "-a", "-m", "annotated", "v2", "main"]);
        rev_parse(&repo, "v2")
    };
    let missing = "1".repeat(main.len());
    std::fs::write(repo.join(".git/refs/heads/broken"), format!("{missing}\n")).unwrap();
    std::fs::write(repo.join(".git/refs/heads/tagged"), format!("{tag}\n")).unwrap();
    let (branches, complete) = fixture.branches(&project).unwrap();
    let names: Vec<_> = branches
        .iter()
        .map(|b| (b.name.as_str(), b.commit.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("broken", missing.as_str()),
            ("feature/x", branches[1].commit.as_str()),
            ("main", main.as_str()),
            ("tagged", tag.as_str())
        ]
    );
    assert!(complete);
    for broken in ["broken", "tagged"] {
        assert_eq!(
            code(fixture.compare(&project, "main", broken)),
            error_code::UNKNOWN_BRANCH
        );
    }
    std::fs::remove_file(repo.join(".git/refs/heads/broken")).unwrap();
    std::fs::remove_file(repo.join(".git/refs/heads/tagged")).unwrap();

    let refs: String = (0..=budget::MAX_BRANCHES)
        .map(|n| format!("create refs/heads/b{n:05} {main}\n"))
        .collect();
    git_stdin(&repo, &["update-ref", "--stdin"], refs.as_bytes());
    let (branches, complete) = fixture.branches(&project).unwrap();
    assert_eq!((branches.len(), complete), (budget::MAX_BRANCHES, false));
}

/// What the branch list says of the main working copy's branch (`main`) after `setup` has
/// arranged the repository, given the commit `main` is at: its upstream, and whether the list
/// says the lookup was complete.
fn main_upstream(label: &str, setup: impl FnOnce(&Path, &str)) -> (Option<UpstreamInfo>, bool) {
    let repo = repo_with(label, &[(b"a.txt", b"a\n")]);
    main_upstream_in(&repo, setup)
}

/// [`main_upstream`] in the repository `repo` as it is.
fn main_upstream_in(repo: &Path, setup: impl FnOnce(&Path, &str)) -> (Option<UpstreamInfo>, bool) {
    let fixture = Fixture::new();
    let main = rev_parse(repo, "main");
    setup(repo, &main);
    let project = fixture.project("p", repo);
    match fixture
        .serve(BrowseBody::ListProjectBranches { project })
        .unwrap()
    {
        Event::ProjectBranches {
            branches,
            upstreams_complete,
            ..
        } => {
            let main = branches.iter().find(|b| b.name == "main").unwrap();
            (main.upstream.clone(), upstreams_complete)
        }
        other => panic!("not a branch list: {other:?}"),
    }
}

fn remote(repo: &Path, name: &str) {
    git(repo, &["config", &format!("remote.{name}.url"), "/nowhere"]);
    git(
        repo,
        &[
            "config",
            &format!("remote.{name}.fetch"),
            &format!("+refs/heads/*:refs/remotes/{name}/*"),
        ],
    );
}

fn tracks(repo: &Path, branch: &str, remote: &str, merge: &str) {
    git(
        repo,
        &["config", &format!("branch.{branch}.remote"), remote],
    );
    git(repo, &["config", &format!("branch.{branch}.merge"), merge]);
}

fn upstream(name: &str, commit: &str) -> Option<UpstreamInfo> {
    Some(UpstreamInfo {
        name: name.to_string(),
        commit: commit.to_string(),
    })
}

#[test]
fn the_main_working_copys_branch_has_the_remote_branch_it_tracks_as_its_upstream() {
    let (found, complete) = main_upstream("upstream-tracked", |repo, main| {
        remote(repo, "origin");
        tracks(repo, "main", "origin", "refs/heads/main");
        git(repo, &["update-ref", "refs/remotes/origin/main", main]);
        // The same name on another remote does not matter: the upstream is the one it tracks.
        remote(repo, "alpha");
        git(repo, &["update-ref", "refs/remotes/alpha/main", main]);
    });
    assert_eq!(
        found,
        upstream("origin/main", &found.clone().unwrap().commit)
    );
    assert!(complete);
}

#[test]
fn a_branch_without_a_remote_upstream_has_the_branch_of_its_name_on_a_configured_remote() {
    // Tracking nothing, `origin`'s is preferred to another remote's that sorts first.
    let (found, complete) = main_upstream("upstream-origin", |repo, main| {
        remote(repo, "origin");
        remote(repo, "alpha");
        git(repo, &["update-ref", "refs/remotes/origin/main", main]);
        git(repo, &["update-ref", "refs/remotes/alpha/main", main]);
    });
    assert_eq!(found.map(|u| u.name).as_deref(), Some("origin/main"));
    assert!(complete);

    // Else the first in byte order, a name with a slash in it included.
    let (found, _) = main_upstream("upstream-first", |repo, main| {
        remote(repo, "zeta");
        remote(repo, "alpha/b");
        git(repo, &["update-ref", "refs/remotes/zeta/main", main]);
        git(repo, &["update-ref", "refs/remotes/alpha/b/main", main]);
    });
    assert_eq!(found.map(|u| u.name).as_deref(), Some("alpha/b/main"));

    // An upstream whose reference is gone, or that is a local branch, is none.
    let (found, _) = main_upstream("upstream-gone", |repo, main| {
        remote(repo, "origin");
        tracks(repo, "main", "origin", "refs/heads/gone");
        git(repo, &["update-ref", "refs/remotes/origin/main", main]);
    });
    assert_eq!(found.map(|u| u.name).as_deref(), Some("origin/main"));
    let (found, _) = main_upstream("upstream-local", |repo, main| {
        remote(repo, "origin");
        tracks(repo, "main", ".", "refs/heads/other");
        git(repo, &["branch", "other"]);
        git(repo, &["update-ref", "refs/remotes/origin/main", main]);
    });
    assert_eq!(found.map(|u| u.name).as_deref(), Some("origin/main"));
    let (found, complete) = main_upstream("upstream-local-only", |repo, _| {
        remote(repo, "origin");
        tracks(repo, "main", ".", "refs/heads/other");
        git(repo, &["branch", "other"]);
    });
    assert_eq!((found, complete), (None, true));

    // A remote-tracking reference below a name is not on a remote that was never added.
    let (found, complete) = main_upstream("upstream-backup", |repo, main| {
        remote(repo, "origin");
        git(
            repo,
            &["update-ref", "refs/remotes/origin/backup/main", main],
        );
    });
    assert_eq!((found, complete), (None, true));
}

#[test]
fn only_the_main_working_copys_branch_has_an_upstream() {
    let fixture = Fixture::new();
    let repo = repo_with("upstream-only-main", &[(b"a.txt", b"a\n")]);
    let main = rev_parse(&repo, "main");
    remote(&repo, "origin");
    commit_on(&repo, "linked", "main", &[("a.txt", Some(b"l\n"))]);
    commit_on(&repo, "elsewhere", "main", &[("a.txt", Some(b"e\n"))]);
    for branch in ["main", "linked", "elsewhere"] {
        git(
            &repo,
            &[
                "update-ref",
                &format!("refs/remotes/origin/{branch}"),
                &main,
            ],
        );
        tracks(&repo, branch, "origin", &format!("refs/heads/{branch}"));
    }
    // `linked` is checked out in a worktree of its own, `elsewhere` nowhere.
    let trees = ScratchDir::new("upstream-only-main-trees");
    let path = trees.join("linked");
    git(
        &repo,
        &["worktree", "add", "-q", path.to_str().unwrap(), "linked"],
    );
    let project = fixture.project("p", &repo);
    let (branches, _) = fixture.branches(&project).unwrap();
    let upstreams: Vec<_> = branches
        .iter()
        .map(|b| (b.name.as_str(), b.upstream.is_some()))
        .collect();
    assert_eq!(
        upstreams,
        [("elsewhere", false), ("linked", false), ("main", true)]
    );

    // With a detached `HEAD` the main working copy has no branch, and none has an upstream.
    git(&repo, &["checkout", "-q", "--detach"]);
    let (branches, _) = fixture.branches(&project).unwrap();
    assert!(branches.iter().all(|b| b.upstream.is_none()));
}

#[test]
fn the_main_working_copys_branch_is_found_in_either_reference_format() {
    // A repository in reftable format keeps a placeholder in its `HEAD` file. Skipped where
    // `git` does not know the format.
    let repo = ScratchDir::new("upstream-reftable");
    let init = fixture_git(&repo)
        .args(["init", "-q", "--ref-format=reftable"])
        .output()
        .expect("git runs");
    if !init.status.success() {
        return;
    }
    std::fs::write(repo.join("a.txt"), b"a\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "fixture"]);
    let (found, complete) = main_upstream_in(&repo, |repo, main| {
        remote(repo, "origin");
        git(repo, &["update-ref", "refs/remotes/origin/main", main]);
    });
    assert_eq!(found.map(|u| u.name).as_deref(), Some("origin/main"));
    assert!(complete);
}

#[test]
fn a_head_git_cannot_read_leaves_the_upstream_unknown_and_says_so() {
    let repo = repo_with("upstream-bad-head", &[(b"a.txt", b"a\n")]);
    // A common directory `git` finds no repository in fails with something other than the status
    // of a detached `HEAD`.
    let elsewhere = ScratchDir::new("upstream-bad-head-dir");
    let env = GitEnv::new(SYSTEM_GIT.to_string(), isolated_git_env());
    let list = compare::branches_in(&env, &repo, &elsewhere, &AtomicBool::new(false)).unwrap();
    assert!(!list.upstreams_complete);
    assert!(list.branches.iter().all(|b| b.upstream.is_none()));
}

/// Appends `count` remotes named `prefix` and a number to the repository's configuration.
fn many_remotes(repo: &Path, prefix: &str, count: usize) {
    let mut config = std::fs::read_to_string(repo.join(".git/config")).unwrap();
    for n in 0..count {
        config.push_str(&format!("[remote \"{prefix}{n:05}\"]\n\turl = /nowhere\n"));
    }
    std::fs::write(repo.join(".git/config"), config).unwrap();
}

#[test]
fn a_lookup_past_one_of_its_budgets_says_so_and_keeps_what_it_found() {
    // The remotes' names passing the output of `git remote` (32 KiB), the names asked for
    // staying within theirs.
    let (found, complete) = main_upstream("upstream-many-names", |repo, main| {
        many_remotes(repo, "remote-with-a-rather-long-name-", 1_000);
        git(
            repo,
            &[
                "update-ref",
                "refs/remotes/remote-with-a-rather-long-name-00001/main",
                main,
            ],
        );
    });
    assert_eq!((found, complete), (None, false));

    // The names asked for (64 KiB) passing theirs, `git remote` printing less than its budget.
    let (found, complete) = main_upstream("upstream-many-asked", |repo, main| {
        many_remotes(repo, "r", 4_000);
        git(repo, &["update-ref", "refs/remotes/r00001/main", main]);
    });
    assert_eq!((found, complete), (None, false));

    // The lookup's output (128 KiB) passing its own, with what lies below the name asked for.
    let (found, complete) = main_upstream("upstream-much-output", |repo, main| {
        remote(repo, "origin");
        let refs: String = (0..3_000)
            .map(|n| format!("create refs/remotes/origin/main/sub{n:06} {main}\n"))
            .collect();
        git_stdin(repo, &["update-ref", "--stdin"], refs.as_bytes());
    });
    assert_eq!((found, complete), (None, false));

    // The upstream it tracks is found before any of that is asked, and kept when the fallback
    // would have passed a budget.
    let (found, complete) = main_upstream("upstream-kept", |repo, main| {
        remote(repo, "origin");
        tracks(repo, "main", "origin", "refs/heads/main");
        git(repo, &["update-ref", "refs/remotes/origin/main", main]);
        many_remotes(repo, "remote-with-a-rather-long-name-", 1_000);
    });
    assert_eq!(found.map(|u| u.name).as_deref(), Some("origin/main"));
    assert!(complete);
}

#[test]
fn a_comparison_from_nothing_lists_every_file_of_the_tip_and_its_commits_all_of_them() {
    let fixture = Fixture::new();
    let repo = repo_with("compare-nothing", &[(b"a.txt", b"a\n")]);
    let tip = commit_on(&repo, "topic", "main", &[("b.txt", Some(b"b\n"))]);
    git(&repo, &["checkout", "-q", "main"]);
    git(&repo, &["merge", "-q", "--ff-only", "topic"]);
    let project = fixture.project("p", &repo);

    let compared = fixture.compare_from_nothing(&project, "main").unwrap();
    assert_eq!(
        (
            compared.left.empty,
            compared.left.branch.as_str(),
            &compared.left.commit
        ),
        (true, "", &compared.base)
    );
    assert_eq!(compared.right.commit, tip);
    assert_eq!(compared.changes.len(), 2, "both files are additions");
    for change in &compared.changes {
        let ChangeEntry::Committed { old, .. } = change else {
            panic!("not a committed change: {change:?}");
        };
        assert!(matches!(old, ChangeSide::Absent), "{old:?}");
    }
    let read = fixture
        .compared_change(&project, &compared, SideRef::Absent, present("b.txt"))
        .unwrap();
    assert!(read.patch.unwrap().contains("+b"));

    // The commits listed from the same base are the whole history, the first one included.
    let listed = fixture.commits(&project, &compared.base, &tip).unwrap();
    assert_eq!(listed.commits.len(), 2);
    assert_eq!(listed.commits[1].parent, None);
    assert!(listed.complete && listed.commits_complete);

    // A right side that claims to be nothing, or a remote branch, is no local branch: refused
    // rather than read from whatever commit it names.
    for right in [
        ComparisonEndpoint {
            branch: String::new(),
            commit: "HEAD".to_string(),
            remote: false,
            empty: true,
        },
        ComparisonEndpoint {
            remote: true,
            ..compared.right.clone()
        },
    ] {
        let read = fixture.compared_change_at(
            &project,
            (&compared.left, &right, &compared.base),
            SideRef::Absent,
            present("b.txt"),
        );
        assert_eq!(code(read), error_code::INVALID_BRANCH_NAME);
    }

    // Only the empty tree stands for nothing.
    let mut elsewhere = compared.left.clone();
    elsewhere.commit = tip.clone();
    let read = fixture.compared_change_at(
        &project,
        (&elsewhere, &compared.right, &tip),
        SideRef::Absent,
        present("b.txt"),
    );
    assert_eq!(code(read), error_code::INVALID_COMMIT);
}

#[test]
fn a_remote_tracking_branch_is_compared_and_read_as_the_base() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "compare-remote",
        &[(b"a.txt", b"base\n"), (b"b.txt", b"b\n")],
    );
    let fork = rev_parse(&repo, "main");
    git(&repo, &["update-ref", "refs/remotes/origin/main", &fork]);
    let ahead = commit_on(&repo, "ahead", "main", &[("a.txt", Some(b"ahead\n"))]);
    // The remote has moved on from the fork too; what only it has done is not in the comparison.
    let remote = commit_on(&repo, "elsewhere", "main", &[("b.txt", Some(b"remote\n"))]);
    git(&repo, &["update-ref", "refs/remotes/origin/main", &remote]);
    let project = fixture.project("p", &repo);

    let compared = fixture
        .compare_from(&project, ("origin/main", true), "ahead")
        .unwrap();
    assert_eq!(
        (
            &compared.left.commit,
            compared.left.remote,
            &compared.right.commit,
            &compared.base
        ),
        (&remote, true, &ahead, &fork)
    );
    assert_eq!(compared.changes.len(), 1);
    let read = fixture
        .compared_change(&project, &compared, present("a.txt"), present("a.txt"))
        .unwrap();
    assert!(read.patch.unwrap().contains("+ahead"));

    // The same name is no local branch, and a remote-tracking one has to exist.
    assert_eq!(
        code(fixture.compare(&project, "origin/main", "ahead")),
        error_code::UNKNOWN_BRANCH
    );
    for (name, expected) in [
        ("origin/missing", error_code::UNKNOWN_BRANCH),
        ("origin/main^", error_code::INVALID_BRANCH_NAME),
        ("-x", error_code::INVALID_BRANCH_NAME),
    ] {
        assert_eq!(
            code(fixture.compare_from(&project, (name, true), "ahead")),
            expected,
            "{name}"
        );
    }
    let renamed = ComparisonEndpoint {
        branch: "origin/main^".to_string(),
        ..compared.left.clone()
    };
    let read = fixture.compared_change_at(
        &project,
        (&renamed, &compared.right, &compared.base),
        present("a.txt"),
        present("a.txt"),
    );
    assert_eq!(code(read), error_code::INVALID_BRANCH_NAME);
}

#[test]
fn a_comparison_is_what_the_right_branch_changed_since_the_merge_base() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "compare-diverged",
        &[(b"a.txt", b"base\n"), (b"b.txt", b"b\n")],
    );
    let fork = rev_parse(&repo, "main");
    let left = commit_on(
        &repo,
        "left",
        "main",
        &[("a.txt", Some(b"left\n")), ("l.txt", Some(b"l\n"))],
    );
    let right = commit_on(
        &repo,
        "right",
        "main",
        &[("b.txt", Some(b"right\n")), ("r.txt", Some(b"r\n"))],
    );
    let project = fixture.project("p", &repo);

    // The positive control: this is what `git diff left...right` shows.
    let from_base = String::from_utf8(git(&repo, &["diff", "--name-only", "left...right"]));
    assert_eq!(from_base.unwrap(), "b.txt\nr.txt\n");

    let compared = fixture.compare(&project, "left", "right").unwrap();
    assert_eq!(
        (
            &compared.left.commit,
            &compared.right.commit,
            &compared.base
        ),
        (&left, &right, &fork)
    );
    assert_eq!(
        summary(&compared.changes),
        ["committed - r.txt", "committed b.txt b.txt"],
        "what only `left` has done is not undone"
    );
    let read = fixture
        .compared_change(&project, &compared, present("b.txt"), present("b.txt"))
        .unwrap();
    assert!(read.patch.unwrap().contains("-b\n+right\n"));
    assert_eq!(
        commit_and_blob(&read.old),
        (fork.clone(), rev_parse(&repo, "main:b.txt"))
    );
    assert_eq!(
        commit_and_blob(&read.new),
        (right.clone(), rev_parse(&repo, "right:b.txt"))
    );

    // The other way round it is what `left` changed.
    let reversed = fixture.compare(&project, "right", "left").unwrap();
    assert_eq!(reversed.base, fork);
    assert_eq!(
        summary(&reversed.changes),
        ["committed - l.txt", "committed a.txt a.txt"]
    );

    // A branch that is an ancestor of the other forks at its own tip.
    let from_main = fixture.compare(&project, "main", "right").unwrap();
    assert_eq!(from_main.base, fork);
    assert_eq!(summary(&from_main.changes), summary(&compared.changes));
    let behind = fixture.compare(&project, "right", "main").unwrap();
    assert_eq!(behind.base, fork);
    assert!(behind.changes.is_empty() && behind.complete);
    assert_eq!(rev_parse(&repo, "HEAD"), rev_parse(&repo, "main"));
}

#[test]
fn branches_with_no_commit_in_common_are_refused() {
    let fixture = Fixture::new();
    let repo = repo_with("compare-unrelated", &[(b"a.txt", b"a\n")]);
    git(&repo, &["checkout", "-q", "--orphan", "unrelated"]);
    git(&repo, &["rm", "-rfq", "."]);
    write(repo.join("u.txt"), b"u\n");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "unrelated"]);
    git(&repo, &["checkout", "-q", "main"]);
    let project = fixture.project("p", &repo);
    for (left, right) in [("main", "unrelated"), ("unrelated", "main")] {
        assert_eq!(
            code(fixture.compare(&project, left, right)),
            error_code::NO_MERGE_BASE
        );
    }
}

#[test]
fn a_shallow_clone_says_its_history_may_be_incomplete_rather_than_unrelated() {
    let fixture = Fixture::new();
    let origin = repo_with("compare-shallow-origin", &[(b"a.txt", b"a\n")]);
    commit_on(&origin, "topic", "main", &[("a.txt", Some(b"topic\n"))]);
    // Each branch is cut to its tip, so the two share no commit in what the clone holds.
    let clone = ScratchDir::new("compare-shallow-clone");
    let url = format!("file://{}", origin.display());
    git(
        &origin,
        &[
            "clone",
            "-q",
            "--depth",
            "1",
            "--no-single-branch",
            &url,
            clone.to_str().unwrap(),
        ],
    );
    git(&clone, &["branch", "topic", "origin/topic"]);
    let project = fixture.project("p", &clone);
    assert_eq!(
        code(fixture.compare(&project, "main", "topic")),
        error_code::NO_MERGE_BASE_SHALLOW
    );
}

#[test]
fn a_pure_rename_between_two_commits_has_no_hunk_and_so_no_line_count() {
    let fixture = Fixture::new();
    let repo = repo_with("compare-pure-rename", &[(b"old.txt", b"a\nb\nc\n")]);
    commit_on(
        &repo,
        "renamed",
        "main",
        &[("old.txt", None), ("new.txt", Some(b"a\nb\nc\n"))],
    );
    let project = fixture.project("p", &repo);
    let compared = fixture.compare(&project, "main", "renamed").unwrap();
    let read = fixture
        .compared_change(&project, &compared, present("old.txt"), present("new.txt"))
        .unwrap();
    let patch = read.patch.unwrap();
    assert!(
        patch.contains("similarity index 100%") && !patch.contains("@@"),
        "{patch}"
    );
    // Both sides are file blobs, yet with no hunk there is no separator bar for a count to serve.
    assert_eq!((read_lines(&read.old), read_lines(&read.new)), (None, None));
}

#[test]
fn the_same_commit_on_both_sides_is_an_explicit_empty_comparison() {
    let fixture = Fixture::new();
    let repo = repo_with("compare-same", &[(b"a.txt", b"a\n")]);
    git(&repo, &["branch", "copy", "main"]);
    let project = fixture.project("p", &repo);
    for (left, right) in [("main", "copy"), ("main", "main")] {
        let compared = fixture.compare(&project, left, right).unwrap();
        assert_eq!(compared.left.commit, compared.right.commit);
        assert_eq!(compared.base, compared.left.commit);
        assert!(compared.changes.is_empty() && compared.complete);
    }
}

#[test]
fn a_subdirectory_project_keeps_its_scope_and_reads_only_its_side_across_the_boundary() {
    let fixture = Fixture::new();
    let coming: String = (0..12).map(|n| format!("coming line {n}\n")).collect();
    let going: String = (0..12).map(|n| format!("going {n}\n")).collect();
    let repo = repo_with(
        "compare-boundary",
        &[
            (
                b"other/secret.txt",
                format!("{coming}OUTSIDE ONLY\n").as_bytes(),
            ),
            (
                b"sub/leaving.txt",
                format!("{going}inside before\n").as_bytes(),
            ),
            (b"sub/history.txt", b"only in the past\n"),
            (b"top.txt", b"top\n"),
        ],
    );
    git(&repo, &["checkout", "-q", "-b", "moved"]);
    git(&repo, &["mv", "other/secret.txt", "sub/in.txt"]);
    write(
        repo.join("sub/in.txt"),
        format!("{coming}inside now\n").as_bytes(),
    );
    git(&repo, &["mv", "sub/leaving.txt", "other/left.txt"]);
    write(
        repo.join("other/left.txt"),
        format!("{going}OUTSIDE NOW\n").as_bytes(),
    );
    git(&repo, &["rm", "-q", "sub/history.txt"]);
    write(repo.join("top.txt"), b"changed outside\n");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "moved"]);
    git(&repo, &["checkout", "-q", "main"]);
    // The worktree's own copy of the project's folder is gone: what a comparison reads comes from
    // the two commits alone.
    std::fs::remove_file(repo.join("sub/history.txt")).unwrap();
    let project = fixture.project("sub", &repo.join("sub"));

    let compared = fixture.compare(&project, "main", "moved").unwrap();
    assert_eq!(
        summary(&compared.changes),
        [
            "committed history.txt -",
            "committed leaving.txt other/left.txt",
            "committed other/secret.txt in.txt"
        ]
    );
    let gone = fixture
        .compared_change(&project, &compared, present("history.txt"), SideRef::Absent)
        .unwrap();
    assert!(gone.patch.unwrap().contains("-only in the past\n"));

    let coming_in = fixture
        .compared_change(&project, &compared, SideRef::OutOfScope, present("in.txt"))
        .unwrap();
    assert_eq!(
        coming_in.patch, None,
        "a patch would carry the outside lines"
    );
    assert!(matches!(coming_in.old, SideRead::OutOfScope));
    assert_eq!(
        read_body(&coming_in.new).unwrap(),
        format!("{coming}inside now\n").as_bytes()
    );
    let going_out = fixture
        .compared_change(
            &project,
            &compared,
            present("leaving.txt"),
            SideRef::OutOfScope,
        )
        .unwrap();
    assert_eq!(
        read_body(&going_out.old).unwrap(),
        format!("{going}inside before\n").as_bytes()
    );
    for read in [&coming_in, &going_out] {
        assert!(
            !read.json.contains("OUTSIDE") && !read.json.contains("other/"),
            "{}",
            read.json
        );
    }
}

#[test]
fn additions_deletions_renames_images_and_binaries_read_both_sides_from_their_commits() {
    let fixture = Fixture::new();
    let png = |marker: u8| [&b"\x89PNG\r\n\x1a\n\x00\x00"[..], &[marker; 64]].concat();
    let lines: String = (0..20).map(|n| format!("line {n}\n")).collect();
    let repo = repo_with(
        "compare-kinds",
        &[
            (b"r.txt", lines.as_bytes()),
            (b"gone.txt", b"gone\n"),
            (b"i.png", &png(1)),
            (b"data.bin", b"\x00\x01binary one"),
            (b"t.txt", b"a file\n"),
        ],
    );
    git(&repo, &["checkout", "-q", "-b", "next"]);
    git(&repo, &["mv", "r.txt", "r2.txt"]);
    write(repo.join("r2.txt"), format!("{lines}line 20\n").as_bytes());
    std::fs::remove_file(repo.join("gone.txt")).unwrap();
    write(repo.join("new.txt"), b"new\n");
    write(repo.join("i.png"), &png(2));
    write(repo.join("data.bin"), b"\x00\x02binary two");
    std::fs::remove_file(repo.join("t.txt")).unwrap();
    std::os::unix::fs::symlink("r2.txt", repo.join("t.txt")).unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "next"]);
    git(&repo, &["checkout", "-q", "main"]);
    let project = fixture.project("p", &repo);
    let compared = fixture.compare(&project, "main", "next").unwrap();
    assert_eq!(
        summary(&compared.changes),
        [
            "committed - new.txt",
            "committed data.bin data.bin",
            "committed gone.txt -",
            "committed i.png i.png",
            "committed r.txt r2.txt",
            "committed t.txt t.txt"
        ]
    );
    let read = |old: SideRef, new: SideRef| {
        fixture
            .compared_change(&project, &compared, old, new)
            .unwrap()
    };

    let added = read(SideRef::Absent, present("new.txt"));
    assert!(matches!(added.old, SideRead::Absent));
    assert!(added.patch.unwrap().contains("new file mode 100644"));
    assert_eq!(
        read_lines(&added.new),
        Some(1),
        "both sides come from a commit"
    );
    let deleted = read(present("gone.txt"), SideRef::Absent);
    assert!(matches!(deleted.new, SideRead::Absent));
    assert!(deleted.patch.unwrap().contains("-gone\n"));
    assert_eq!(read_lines(&deleted.old), Some(1));
    let renamed = read(present("r.txt"), present("r2.txt"));
    let patch = renamed.patch.unwrap();
    assert!(
        patch.contains("rename from r.txt\nrename to r2.txt\n") && patch.contains("+line 20\n"),
        "{patch}"
    );
    assert_eq!(
        (read_lines(&renamed.old), read_lines(&renamed.new)),
        (Some(20), Some(21))
    );

    let image = read(present("i.png"), present("i.png"));
    assert!(image
        .patch
        .unwrap()
        .contains("Binary files a/i.png and b/i.png differ"));
    assert_eq!(read_body(&image.old).unwrap(), png(1), "main's body");
    assert_eq!(read_body(&image.new).unwrap(), png(2), "next's body");
    let SideRead::Present {
        file: Some(file), ..
    } = &image.new
    else {
        panic!("no body");
    };
    assert_eq!(file.media_type, Some("image/png"));
    // The signature holds two newlines, and the run of marker bytes after them is unterminated.
    assert_eq!(
        (read_lines(&image.old), read_lines(&image.new)),
        (Some(3), Some(3))
    );
    assert_eq!(commit_and_blob(&image.old).0, compared.base);
    assert_eq!(commit_and_blob(&image.new).0, compared.right.commit);

    let binary = read(present("data.bin"), present("data.bin"));
    assert_eq!(read_body(&binary.old).unwrap(), b"\x00\x01binary one");
    assert_eq!(read_body(&binary.new).unwrap(), b"\x00\x02binary two");
    assert_eq!(
        (read_lines(&binary.old), read_lines(&binary.new)),
        (Some(1), Some(1))
    );

    let type_change = read(present("t.txt"), present("t.txt"));
    assert_eq!(
        (read_kind(&type_change.old), read_kind(&type_change.new)),
        (SideKind::File, SideKind::Symlink)
    );
    assert_eq!(
        (read_lines(&type_change.old), read_lines(&type_change.new)),
        (Some(1), None)
    );
    let patch = type_change.patch.unwrap();
    assert_eq!(
        patch.matches("diff --git a/t.txt b/t.txt\n").count(),
        2,
        "{patch}"
    );
}

#[test]
fn a_branch_moved_or_deleted_after_its_comparison_never_repoints_what_it_reads() {
    let fixture = Fixture::new();
    let repo = repo_with("compare-moved", &[(b"a.txt", b"base\n")]);
    let first = commit_on(&repo, "topic", "main", &[("a.txt", Some(b"first\n"))]);
    let project = fixture.project("p", &repo);
    let compared = fixture.compare(&project, "main", "topic").unwrap();
    assert_eq!(compared.right.commit, first);

    let second = commit_on(&repo, "topic", "topic", &[("a.txt", Some(b"second\n"))]);
    let read = fixture
        .compared_change(&project, &compared, present("a.txt"), present("a.txt"))
        .unwrap();
    let patch = read.patch.unwrap();
    assert!(
        patch.contains("+first\n") && !patch.contains("second"),
        "the comparison stays on the commits it resolved: {patch}"
    );
    let again = fixture.compare(&project, "main", "topic").unwrap();
    assert_eq!(
        again.right.commit, second,
        "comparing again resolves the new tip"
    );

    git(&repo, &["branch", "-q", "-D", "topic"]);
    assert_eq!(
        code(fixture.compare(&project, "main", "topic")),
        error_code::UNKNOWN_BRANCH
    );
    // The deleted branch's commits are still in the repository, and still read as they were.
    let read = fixture
        .compared_change(&project, &compared, present("a.txt"), present("a.txt"))
        .unwrap();
    assert!(read.patch.unwrap().contains("+first\n"));

    // Once they are pruned, the comparison's commits are unavailable, never replaced.
    git(&repo, &["reflog", "expire", "--expire=now", "--all"]);
    git(&repo, &["gc", "-q", "--prune=now"]);
    assert_eq!(
        code(fixture.compared_change(&project, &compared, present("a.txt"), present("a.txt"))),
        error_code::UNKNOWN_COMMIT
    );
}

#[test]
fn a_missing_object_fails_the_read_rather_than_reading_something_else() {
    let fixture = Fixture::new();
    let repo = repo_with("compare-missing", &[(b"a.txt", b"base\n")]);
    commit_on(
        &repo,
        "topic",
        "main",
        &[("a.txt", Some(b"only on topic\n"))],
    );
    let blob = rev_parse(&repo, "topic:a.txt");
    std::fs::remove_file(repo.join(".git/objects").join(&blob[..2]).join(&blob[2..])).unwrap();
    let project = fixture.project("p", &repo);
    let compared = fixture.compare(&project, "main", "topic").unwrap();
    assert_eq!(summary(&compared.changes), ["committed a.txt a.txt"]);
    assert_eq!(
        code(fixture.compared_change(&project, &compared, present("a.txt"), present("a.txt"))),
        error_code::GIT_FAILED
    );
}

#[test]
fn an_endpoint_is_a_branch_name_or_a_full_commit_id_and_nothing_else() {
    let fixture = Fixture::new();
    let repo = repo_with("compare-endpoints", &[(b"a.txt", b"a\n")]);
    git(&repo, &["tag", "-a", "-m", "tag", "v1", "main"]);
    let project = fixture.project("p", &repo);
    for (left, expected) in [
        ("main~1", error_code::INVALID_BRANCH_NAME),
        ("@{-1}", error_code::INVALID_BRANCH_NAME),
        ("-x", error_code::INVALID_BRANCH_NAME),
        ("v1", error_code::UNKNOWN_BRANCH),
        ("HEAD", error_code::UNKNOWN_BRANCH),
    ] {
        assert_eq!(
            code(fixture.compare(&project, left, "main")),
            expected,
            "{left}"
        );
    }
    let compared = fixture.compare(&project, "main", "main").unwrap();
    let main = compared.left.clone();
    let tag = rev_parse(&repo, "v1");
    for (commit, expected) in [
        (main.commit[..12].to_string(), error_code::INVALID_COMMIT),
        (tag, error_code::INVALID_COMMIT),
        ("0".repeat(40), error_code::UNKNOWN_COMMIT),
    ] {
        let endpoint = ComparisonEndpoint {
            branch: "main".to_string(),
            commit,
            remote: false,
            empty: false,
        };
        let read = fixture.compared_change_at(
            &project,
            (&main, &endpoint, &main.commit),
            present("a.txt"),
            present("a.txt"),
        );
        assert_eq!(code(read), expected);
    }
    // The base is held to being a commit of the repository too.
    let read = fixture.compared_change_at(
        &project,
        (&main, &main, &"0".repeat(40)),
        present("a.txt"),
        present("a.txt"),
    );
    assert_eq!(code(read), error_code::UNKNOWN_COMMIT);
    let renamed = ComparisonEndpoint {
        branch: "main^".to_string(),
        ..main.clone()
    };
    let read = fixture.compared_change_at(
        &project,
        (&main, &renamed, &main.commit),
        present("a.txt"),
        present("a.txt"),
    );
    assert_eq!(code(read), error_code::INVALID_BRANCH_NAME);
    let read = fixture.compared_change_at(
        &project,
        (&main, &main, &main.commit),
        SideRef::Absent,
        SideRef::Absent,
    );
    assert_eq!(code(read), error_code::INVALID_CHANGE);
}

#[test]
fn a_comparison_is_cut_at_its_entry_budget_and_a_patch_past_its_budget_is_refused() {
    let fixture = Fixture::new();
    let repo = repo_with("compare-budgets", &[(b"big.txt", b"small\n")]);
    git(&repo, &["checkout", "-q", "-b", "many"]);
    for n in 0..=budget::MAX_CHANGE_ENTRIES {
        std::fs::write(repo.join(format!("u{n:05}.txt")), format!("{n}\n")).unwrap();
    }
    let line = "x".repeat(99) + "\n";
    write(
        repo.join("big.txt"),
        line.repeat(budget::MAX_PATCH_BYTES / 100 + 1).as_bytes(),
    );
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "many"]);
    git(&repo, &["checkout", "-q", "main"]);
    let project = fixture.project("p", &repo);
    let event = fixture
        .serve(BrowseBody::CompareProjectBranches {
            project: project.clone(),
            left: "main".to_string(),
            left_remote: false,
            left_empty: false,
            right: "many".to_string(),
        })
        .unwrap();
    let serialized = serde_json::to_string(&event).unwrap().len();
    let Event::ProjectComparison {
        changes,
        complete,
        left,
        right,
        base,
        ..
    } = event
    else {
        panic!("not a comparison");
    };
    assert_eq!(
        (changes.len(), complete),
        (budget::MAX_CHANGE_ENTRIES, false)
    );
    assert!(serialized < budget::COMPARISON_RESERVATION - budget::TREE_DIFF_STDOUT);

    let compared = Compared {
        left,
        right,
        base,
        changes,
        complete,
    };
    let err = fixture
        .compared_change(&project, &compared, present("big.txt"), present("big.txt"))
        .expect_err("a refusal");
    let coded = err.downcast_ref::<CodedError>().unwrap();
    assert_eq!(
        (coded.code, coded.params.get("limit").map(String::as_str)),
        (error_code::LIMIT_EXCEEDED, Some("patch_bytes"))
    );
}

#[test]
fn a_comparison_past_its_budget_is_made_within_a_subdirectory_project_alone() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "compare-fallback",
        &[(b"sub/a.txt", b"a\n"), (b"other/moved.txt", b"moved\n")],
    );
    let many: Vec<(String, Option<&[u8]>)> = (0..50)
        .map(|n| (format!("other/n{n:02}.txt"), Some(&b"n\n"[..])))
        .collect();
    let mut files: Vec<(&str, Option<&[u8]>)> =
        many.iter().map(|(p, b)| (p.as_str(), *b)).collect();
    files.push(("sub/a.txt", Some(b"changed\n")));
    commit_on(&repo, "topic", "main", &files);
    git(&repo, &["checkout", "-q", "topic"]);
    git(&repo, &["mv", "other/moved.txt", "sub/moved.txt"]);
    git(&repo, &["commit", "-q", "-m", "moved in"]);
    git(&repo, &["checkout", "-q", "main"]);
    let sub = fixture.project("sub", &repo.join("sub"));
    let whole = fixture.project("whole", &repo);

    let full = fixture.compare(&sub, "main", "topic").unwrap();
    assert_eq!(
        summary(&full.changes),
        [
            "committed a.txt a.txt",
            "committed other/moved.txt moved.txt"
        ]
    );
    // Well under the whole tree's output, well over the subdirectory's.
    super::tree_diff::TREE_DIFF_STDOUT_OVERRIDE.with(|limit| limit.set(Some(1024)));
    let scoped = fixture.compare(&sub, "main", "topic");
    let at_root = fixture.compare(&whole, "main", "topic");
    // Under even the subdirectory's own output: refused, as a whole tree past it is.
    super::tree_diff::TREE_DIFF_STDOUT_OVERRIDE.with(|limit| limit.set(Some(16)));
    let still_over = fixture.compare(&sub, "main", "topic");
    super::tree_diff::TREE_DIFF_STDOUT_OVERRIDE.with(|limit| limit.set(None));
    assert_eq!(
        summary(&scoped.unwrap().changes),
        ["committed - moved.txt", "committed a.txt a.txt"],
        "within the project alone, the rename into it is the addition it is there"
    );
    for refused in [at_root, still_over] {
        let err = refused.expect_err("a refusal");
        let coded = err.downcast_ref::<CodedError>().unwrap();
        assert_eq!(
            (coded.code, coded.params.get("limit").map(String::as_str)),
            (error_code::LIMIT_EXCEEDED, Some("git_output"))
        );
    }
}
