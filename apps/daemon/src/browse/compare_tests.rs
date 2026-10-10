//! Branch comparisons listed and read end to end against real repositories: which branches there
//! are, what a pair of tips compares, which sides a change has inside and across a project's
//! boundary, and what a comparison reads once its branches have moved, gone, or lost their objects.

use std::path::Path;

use super::change_tests::{present, read_body, read_kind, read_source, summary, write};
use super::tests::{code, Fixture};
use super::*;
use crate::protocol::{
    BranchInfo, ChangeEntry, ComparedChangeRef, ComparisonEndpoint, SideKind, SideRead, SideRef,
};
use crate::test_support::{git, git_stdin, repo_with};

/// A comparison as `compare_project_branches` answered it.
#[derive(Debug)]
pub(super) struct Compared {
    pub(super) left: ComparisonEndpoint,
    pub(super) right: ComparisonEndpoint,
    pub(super) changes: Vec<ChangeEntry>,
    complete: bool,
}

/// One change of a comparison as `read_project_comparison_change` answered it.
#[derive(Debug)]
pub(super) struct ComparedRead {
    pub(super) old: SideRead,
    pub(super) new: SideRead,
    patch: Option<String>,
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
        match self.serve(BrowseBody::CompareProjectBranches {
            project: project.to_string(),
            left: left.to_string(),
            right: right.to_string(),
        })? {
            Event::ProjectComparison {
                left: echoed_left,
                right: echoed_right,
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
        self.compared_change_at(project, &compared.left, &compared.right, old, new)
    }

    fn compared_change_at(
        &self,
        project: &str,
        left: &ComparisonEndpoint,
        right: &ComparisonEndpoint,
        old: SideRef,
        new: SideRef,
    ) -> anyhow::Result<ComparedRead> {
        let event = self.serve(BrowseBody::ReadProjectComparisonChange {
            project: project.to_string(),
            left: left.clone(),
            right: right.clone(),
            change: ComparedChangeRef { old, new },
        })?;
        let json = serde_json::to_string(&event).unwrap();
        match event {
            Event::ProjectComparisonChange {
                left: echoed_left,
                right: echoed_right,
                old,
                new,
                patch,
                ..
            } => {
                assert_eq!((&echoed_left, &echoed_right), (left, right), "the echo");
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

fn rev_parse(repo: &Path, revision: &str) -> String {
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
    let project = fixture.project("p", &repo);
    let (branches, complete) = fixture.branches(&project).unwrap();
    assert_eq!(
        branches,
        [
            BranchInfo {
                name: "feature/x".to_string(),
                commit: feature
            },
            BranchInfo {
                name: "main".to_string(),
                commit: main.clone()
            }
        ],
        "local branches only, whether checked out or not"
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

#[test]
fn a_comparison_is_tip_to_tip_in_either_direction_never_from_the_merge_base() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "compare-diverged",
        &[(b"a.txt", b"base\n"), (b"b.txt", b"b\n")],
    );
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

    // The positive control: from their merge base, `right` changes only what it committed.
    let from_base = String::from_utf8(git(&repo, &["diff", "--name-only", "left...right"]));
    assert_eq!(from_base.unwrap(), "b.txt\nr.txt\n");

    let compared = fixture.compare(&project, "left", "right").unwrap();
    assert_eq!(
        (&compared.left.commit, &compared.right.commit),
        (&left, &right)
    );
    assert_eq!(
        summary(&compared.changes),
        [
            "committed - r.txt",
            "committed a.txt a.txt",
            "committed b.txt b.txt",
            "committed l.txt -"
        ],
        "what `left` has that `right` does not is undone, as two tips compare"
    );
    let read = fixture
        .compared_change(&project, &compared, present("a.txt"), present("a.txt"))
        .unwrap();
    assert!(read.patch.unwrap().contains("-left\n+base\n"));
    assert_eq!(
        commit_and_blob(&read.old),
        (left.clone(), rev_parse(&repo, "left:a.txt"))
    );
    assert_eq!(
        commit_and_blob(&read.new),
        (right.clone(), rev_parse(&repo, "right:a.txt"))
    );

    let reversed = fixture.compare(&project, "right", "left").unwrap();
    assert_eq!(
        summary(&reversed.changes),
        [
            "committed - l.txt",
            "committed a.txt a.txt",
            "committed b.txt b.txt",
            "committed r.txt -"
        ]
    );
    let read = fixture
        .compared_change(&project, &reversed, present("a.txt"), present("a.txt"))
        .unwrap();
    assert!(read.patch.unwrap().contains("-base\n+left\n"));

    let from_main = fixture.compare(&project, "main", "right").unwrap();
    assert_eq!(
        summary(&from_main.changes),
        ["committed - r.txt", "committed b.txt b.txt"]
    );
    assert_eq!(rev_parse(&repo, "HEAD"), rev_parse(&repo, "main"));
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
    let deleted = read(present("gone.txt"), SideRef::Absent);
    assert!(matches!(deleted.new, SideRead::Absent));
    assert!(deleted.patch.unwrap().contains("-gone\n"));
    let renamed = read(present("r.txt"), present("r2.txt"));
    let patch = renamed.patch.unwrap();
    assert!(
        patch.contains("rename from r.txt\nrename to r2.txt\n") && patch.contains("+line 20\n"),
        "{patch}"
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
    assert_eq!(commit_and_blob(&image.old).0, compared.left.commit);
    assert_eq!(commit_and_blob(&image.new).0, compared.right.commit);

    let binary = read(present("data.bin"), present("data.bin"));
    assert_eq!(read_body(&binary.old).unwrap(), b"\x00\x01binary one");
    assert_eq!(read_body(&binary.new).unwrap(), b"\x00\x02binary two");

    let type_change = read(present("t.txt"), present("t.txt"));
    assert_eq!(
        (read_kind(&type_change.old), read_kind(&type_change.new)),
        (SideKind::File, SideKind::Symlink)
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
        };
        let read = fixture.compared_change_at(
            &project,
            &main,
            &endpoint,
            present("a.txt"),
            present("a.txt"),
        );
        assert_eq!(code(read), expected);
    }
    let renamed = ComparisonEndpoint {
        branch: "main^".to_string(),
        ..main.clone()
    };
    let read = fixture.compared_change_at(
        &project,
        &main,
        &renamed,
        present("a.txt"),
        present("a.txt"),
    );
    assert_eq!(code(read), error_code::INVALID_BRANCH_NAME);
    let read = fixture.compared_change_at(&project, &main, &main, SideRef::Absent, SideRef::Absent);
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
            right: "many".to_string(),
        })
        .unwrap();
    let serialized = serde_json::to_string(&event).unwrap().len();
    let Event::ProjectComparison {
        changes,
        complete,
        left,
        right,
        ..
    } = event
    else {
        panic!("not a comparison");
    };
    assert_eq!(
        (changes.len(), complete),
        (budget::MAX_CHANGE_ENTRIES, false)
    );
    assert!(serialized < budget::COMPARISON_RESERVATION - budget::COMPARISON_STDOUT);

    let compared = Compared {
        left,
        right,
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
    super::compare::COMPARISON_STDOUT_OVERRIDE.with(|limit| limit.set(Some(1024)));
    let scoped = fixture.compare(&sub, "main", "topic");
    let at_root = fixture.compare(&whole, "main", "topic");
    // Under even the subdirectory's own output: refused, as a whole tree past it is.
    super::compare::COMPARISON_STDOUT_OVERRIDE.with(|limit| limit.set(Some(16)));
    let still_over = fixture.compare(&sub, "main", "topic");
    super::compare::COMPARISON_STDOUT_OVERRIDE.with(|limit| limit.set(None));
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
