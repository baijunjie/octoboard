//! The whole bodies of a change's two sides, read end to end against real repositories: each kind
//! of diff side, a version that no longer matches the patch's, a body past the file budget, and a
//! path with a clean filter in effect, whose disk side is refused rather than read — including the
//! check that deciding so reads the repository without writing to it.

use super::change_tests::{present, read_body, read_lines, write};
use super::compare_tests::{commit_on, Compared};
use super::tests::{code, Fixture};
use super::*;
use crate::protocol::{ChangeGroup, ChangeRef, ComparedChangeRef, SideRead, SideRef, SideVersion};
use crate::test_support::{
    fixture_git, git, git_dir_state, move_modification_time_back, repo_with,
};

/// The version a diff reply reported for `side`, as a client names it back.
pub(super) fn version(side: &SideRead) -> SideVersion {
    match side {
        SideRead::Present { source, .. } => SideVersion::Present {
            source: source.clone(),
        },
        _ => SideVersion::Absent,
    }
}

impl Fixture {
    fn bodies(
        &self,
        project: &str,
        group: ChangeGroup,
        (old, new): (SideRef, SideRef),
        (old_version, new_version): (SideVersion, SideVersion),
    ) -> anyhow::Result<(SideRead, SideRead)> {
        let event = self.serve(BrowseBody::ReadProjectChangeBodies {
            project: project.to_string(),
            worktree: None,
            change: ChangeRef { group, old, new },
            old: old_version,
            new: new_version,
        })?;
        match event {
            Event::ProjectChangeBodies {
                group: echoed,
                old,
                new,
                ..
            } => {
                assert_eq!(echoed, group, "the reply's echo");
                Ok((*old, *new))
            }
            other => panic!("not change bodies: {other:?}"),
        }
    }

    /// The bodies of the change `group` over `path`, the versions being those its diff reports now.
    pub(super) fn bodies_of_diff(
        &self,
        project: &str,
        group: ChangeGroup,
        path: &str,
    ) -> (SideRead, SideRead) {
        let diff = self
            .change(project, None, group, present(path), present(path))
            .unwrap();
        self.bodies(
            project,
            group,
            (present(path), present(path)),
            (version(&diff.old), version(&diff.new)),
        )
        .unwrap()
    }

    pub(super) fn compared_bodies(
        &self,
        project: &str,
        compared: &Compared,
        path: &str,
        (old_version, new_version): (SideVersion, SideVersion),
    ) -> anyhow::Result<(SideRead, SideRead)> {
        let event = self.serve(BrowseBody::ReadProjectComparisonChangeBodies {
            project: project.to_string(),
            left: compared.left.clone(),
            right: compared.right.clone(),
            change: ComparedChangeRef {
                old: present(path),
                new: present(path),
            },
            old: old_version,
            new: new_version,
        })?;
        match event {
            Event::ProjectComparisonChangeBodies {
                left,
                right,
                old,
                new,
                ..
            } => {
                assert_eq!(
                    (&left, &right),
                    (&compared.left, &compared.right),
                    "the echo"
                );
                Ok((*old, *new))
            }
            other => panic!("not comparison bodies: {other:?}"),
        }
    }
}

fn text(side: &SideRead) -> Option<String> {
    read_body(side).map(|bytes| String::from_utf8(bytes).unwrap())
}

#[test]
fn bodies_come_from_the_index_the_disk_head_and_a_compared_commit() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "bodies-sides",
        &[(b"a.txt", b"1\n2\n3\n"), (b"gone.txt", b"bye\n")],
    );
    write(repo.join("a.txt"), b"1\nTWO\n3\n");
    git(&repo, &["add", "a.txt"]);
    write(repo.join("a.txt"), b"1\nTWO\nTHREE\n");
    write(repo.join("new.txt"), b"fresh\n");
    git(&repo, &["add", "new.txt"]);
    git(&repo, &["rm", "-q", "gone.txt"]);
    let project = fixture.project("p", &repo);

    // Staged: `HEAD` against the index.
    let (old, new) = fixture.bodies_of_diff(&project, ChangeGroup::Staged, "a.txt");
    assert_eq!(text(&old).as_deref(), Some("1\n2\n3\n"));
    assert_eq!(text(&new).as_deref(), Some("1\nTWO\n3\n"));
    // Each body carries the count of the very bytes it holds.
    assert_eq!((read_lines(&old), read_lines(&new)), (Some(3), Some(3)));
    // Unstaged: the index against the disk.
    let (old, new) = fixture.bodies_of_diff(&project, ChangeGroup::Unstaged, "a.txt");
    assert_eq!(text(&old).as_deref(), Some("1\nTWO\n3\n"));
    assert_eq!(text(&new).as_deref(), Some("1\nTWO\nTHREE\n"));
    assert_eq!(
        read_lines(&new),
        Some(3),
        "the disk's body, counted as read"
    );

    // An added file has no old side and a deleted one no new side.
    let diff = fixture
        .change(
            &project,
            None,
            ChangeGroup::Staged,
            SideRef::Absent,
            present("new.txt"),
        )
        .unwrap();
    let (old, new) = fixture
        .bodies(
            &project,
            ChangeGroup::Staged,
            (SideRef::Absent, present("new.txt")),
            (version(&diff.old), version(&diff.new)),
        )
        .unwrap();
    assert!(matches!(old, SideRead::Absent));
    assert_eq!(text(&new).as_deref(), Some("fresh\n"));
    assert_eq!(read_lines(&new), Some(1));
    let diff = fixture
        .change(
            &project,
            None,
            ChangeGroup::Staged,
            present("gone.txt"),
            SideRef::Absent,
        )
        .unwrap();
    let (old, new) = fixture
        .bodies(
            &project,
            ChangeGroup::Staged,
            (present("gone.txt"), SideRef::Absent),
            (version(&diff.old), version(&diff.new)),
        )
        .unwrap();
    assert_eq!(text(&old).as_deref(), Some("bye\n"));
    assert!(matches!(new, SideRead::Absent));

    // Two compared commits.
    git(&repo, &["commit", "-q", "-am", "base"]);
    commit_on(
        &repo,
        "feature",
        "main",
        &[("a.txt", Some(b"1\nTWO\nTHREE\nFOUR\n"))],
    );
    let compared = fixture.compare(&project, "main", "feature").unwrap();
    let diff = fixture
        .compared_change(&project, &compared, present("a.txt"), present("a.txt"))
        .unwrap();
    let (old, new) = fixture
        .compared_bodies(
            &project,
            &compared,
            "a.txt",
            (version(&diff.old), version(&diff.new)),
        )
        .unwrap();
    assert_eq!(text(&old).as_deref(), Some("1\nTWO\nTHREE\n"));
    assert_eq!(text(&new).as_deref(), Some("1\nTWO\nTHREE\nFOUR\n"));
    assert_eq!((read_lines(&old), read_lines(&new)), (Some(3), Some(4)));
}

#[test]
fn bodies_of_a_version_the_patch_was_not_made_from_are_rejected() {
    let fixture = Fixture::new();
    let repo = repo_with("bodies-stale", &[(b"a.txt", b"one\n")]);
    write(repo.join("a.txt"), b"two\n");
    git(&repo, &["add", "a.txt"]);
    write(repo.join("a.txt"), b"three, longer\n");
    let project = fixture.project("p", &repo);
    let sides = || (present("a.txt"), present("a.txt"));
    let patch_of = |group| {
        let diff = fixture
            .change(&project, None, group, present("a.txt"), present("a.txt"))
            .unwrap();
        (version(&diff.old), version(&diff.new))
    };
    let staged = patch_of(ChangeGroup::Staged);
    let unstaged = patch_of(ChangeGroup::Unstaged);

    // The disk moved on after the unstaged patch, the index after the staged one.
    write(repo.join("a.txt"), b"four, and longer still\n");
    let moved = fixture.bodies(&project, ChangeGroup::Unstaged, sides(), unstaged.clone());
    assert_eq!(code(moved), error_code::SOURCE_CHANGED);
    git(&repo, &["add", "a.txt"]);
    let moved = fixture.bodies(&project, ChangeGroup::Staged, sides(), staged.clone());
    assert_eq!(code(moved), error_code::SOURCE_CHANGED);

    // A side named absent that is there, and one named present that is not.
    let absent = (SideVersion::Absent, staged.1.clone());
    let wrong = fixture.bodies(&project, ChangeGroup::Staged, sides(), absent);
    assert_eq!(code(wrong), error_code::SOURCE_CHANGED);

    // Nothing was refused for the right version.
    let (old, new) = fixture.bodies_of_diff(&project, ChangeGroup::Staged, "a.txt");
    assert_eq!(text(&old).as_deref(), Some("one\n"));
    assert_eq!(text(&new).as_deref(), Some("four, and longer still\n"));

    // A comparison names its commits, so a stale blob is the mismatch there.
    commit_on(&repo, "feature", "main", &[("a.txt", Some(b"feature\n"))]);
    let compared = fixture.compare(&project, "main", "feature").unwrap();
    let diff = fixture
        .compared_change(&project, &compared, present("a.txt"), present("a.txt"))
        .unwrap();
    let stale = (version(&diff.new), version(&diff.new));
    let refused = fixture.compared_bodies(&project, &compared, "a.txt", stale);
    assert_eq!(code(refused), error_code::SOURCE_CHANGED);
}

#[test]
fn an_added_path_that_is_gone_again_is_a_changed_source_not_a_failure() {
    let fixture = Fixture::new();
    let repo = repo_with("bodies-reset", &[(b"a.txt", b"one\n")]);
    write(repo.join("new.txt"), b"fresh\n");
    git(&repo, &["add", "new.txt"]);
    let project = fixture.project("p", &repo);
    let sides = || (SideRef::Absent, present("new.txt"));
    let diff = fixture
        .change(
            &project,
            None,
            ChangeGroup::Staged,
            SideRef::Absent,
            present("new.txt"),
        )
        .unwrap();

    // Both sides now resolve absent, yet the new one was named present.
    git(&repo, &["reset", "-q", "new.txt"]);
    let gone = fixture.bodies(
        &project,
        ChangeGroup::Staged,
        sides(),
        (version(&diff.old), version(&diff.new)),
    );
    assert_eq!(code(gone), error_code::SOURCE_CHANGED);
}

#[test]
fn a_body_over_the_file_budget_is_refused_whole_and_other_sides_have_no_bodies() {
    let fixture = Fixture::new();
    let line = "x".repeat(99) + "\n";
    let big = line.repeat(budget::MAX_FILE_BYTES / 100 + 1);
    assert!(big.len() > budget::MAX_FILE_BYTES);
    let repo = repo_with(
        "bodies-limit",
        &[(b"big.txt", big.as_bytes()), (b"t.txt", b"t\n")],
    );
    // A change of one line keeps the patch far under its own budget.
    let changed = big.replacen("x", "y", 1);
    write(repo.join("big.txt"), changed.as_bytes());
    let project = fixture.project("p", &repo);
    let limit = |result: anyhow::Result<_>| {
        let err = result
            .map(|_: (SideRead, SideRead)| ())
            .expect_err("a refusal");
        let coded = err.downcast_ref::<CodedError>().unwrap();
        (coded.code, coded.params.get("limit").cloned())
    };

    // The disk's body and, once staged, the index's and `HEAD`'s.
    let diff = fixture
        .change(
            &project,
            None,
            ChangeGroup::Unstaged,
            present("big.txt"),
            present("big.txt"),
        )
        .unwrap();
    assert!(diff.patch.unwrap().len() < budget::MAX_PATCH_BYTES / 4);
    let refused = fixture.bodies(
        &project,
        ChangeGroup::Unstaged,
        (present("big.txt"), present("big.txt")),
        (version(&diff.old), version(&diff.new)),
    );
    assert_eq!(
        limit(refused),
        (error_code::LIMIT_EXCEEDED, Some("file_bytes".to_string()))
    );
    git(&repo, &["add", "big.txt"]);
    let diff = fixture
        .change(
            &project,
            None,
            ChangeGroup::Staged,
            present("big.txt"),
            present("big.txt"),
        )
        .unwrap();
    let refused = fixture.bodies(
        &project,
        ChangeGroup::Staged,
        (present("big.txt"), present("big.txt")),
        (version(&diff.old), version(&diff.new)),
    );
    assert_eq!(
        limit(refused),
        (error_code::LIMIT_EXCEEDED, Some("file_bytes".to_string()))
    );

    // A side outside the project means no patch to expand, refused before the in-scope side's
    // body, which is over the budget here, is read.
    let outside = fixture.bodies(
        &project,
        ChangeGroup::Staged,
        (SideRef::OutOfScope, present("big.txt")),
        (SideVersion::Absent, version(&diff.new)),
    );
    assert_eq!(code(outside), error_code::INVALID_CHANGE);

    // A link has no body to read, and an untracked file has no patch to expand.
    std::fs::remove_file(repo.join("t.txt")).unwrap();
    std::os::unix::fs::symlink("elsewhere", repo.join("t.txt")).unwrap();
    let diff = fixture
        .change(
            &project,
            None,
            ChangeGroup::Unstaged,
            present("t.txt"),
            present("t.txt"),
        )
        .unwrap();
    let refused = fixture.bodies(
        &project,
        ChangeGroup::Unstaged,
        (present("t.txt"), present("t.txt")),
        (version(&diff.old), version(&diff.new)),
    );
    assert_eq!(code(refused), error_code::UNSUPPORTED_FILE_TYPE);
    write(repo.join("u.txt"), b"untracked\n");
    let untracked = fixture.bodies(
        &project,
        ChangeGroup::Untracked,
        (SideRef::Absent, present("u.txt")),
        (SideVersion::Absent, SideVersion::Absent),
    );
    assert_eq!(code(untracked), error_code::INVALID_CHANGE);
}

/// A clean filter (Git LFS's being the common case) makes the content `git` numbers the patch from
/// something other than the file on disk, so the disk side's body is refused rather than read.
#[test]
fn a_clean_filtered_path_refuses_the_body_of_its_disk_side() {
    let fixture = Fixture::new();
    let repo = repo_with("bodies-clean-filter", &[(b"plain.txt", b"one\ntwo\n")]);
    // A minimal stand-in for Git LFS: the content that goes into the repository is the file's
    // first two lines, so `git` numbers that side two lines whatever the file holds.
    git(&repo, &["config", "filter.collapse.clean", "sed -n '1,2p'"]);
    // A driver with no `clean` and no `process`, and one whose `clean` is configured as the empty
    // string, clean nothing: both of their paths keep their expansion.
    git(&repo, &["config", "filter.passthrough.smudge", "cat"]);
    git(&repo, &["config", "filter.hollow.clean", ""]);
    write(
        repo.join(".gitattributes"),
        b"collapsed.txt filter=collapse\nsmudged.txt filter=passthrough\n\
          hollow.txt filter=hollow\n",
    );
    let eight = b"one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n";
    write(repo.join("collapsed.txt"), eight);
    write(repo.join("smudged.txt"), eight);
    write(repo.join("hollow.txt"), eight);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "filtered paths"]);
    let changed = b"ONE\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n";
    write(repo.join("collapsed.txt"), changed);
    write(repo.join("smudged.txt"), changed);
    write(repo.join("hollow.txt"), changed);
    write(repo.join("untracked.txt"), eight);
    write(
        repo.join(".gitattributes"),
        b"collapsed.txt filter=collapse\nsmudged.txt filter=passthrough\n\
          hollow.txt filter=hollow\nuntracked.txt filter=collapse\n",
    );
    let project = fixture.project("p", &repo);
    let sides = || (present("collapsed.txt"), present("collapsed.txt"));

    // The patch numbers the side from the cleaned content: two lines, not the file's eight.
    let diff = fixture
        .change(
            &project,
            None,
            ChangeGroup::Unstaged,
            present("collapsed.txt"),
            present("collapsed.txt"),
        )
        .unwrap();
    let patch = diff.patch.clone().unwrap();
    assert!(patch.contains("@@ -1,2 +1,2 @@"), "{patch}");
    let versions = (version(&diff.old), version(&diff.new));
    let refused = fixture.bodies(&project, ChangeGroup::Unstaged, sides(), versions);
    assert_eq!(code(refused), error_code::CLEAN_FILTERED);

    // Staged, both sides are blobs, which hold the cleaned content already.
    git(&repo, &["add", "collapsed.txt"]);
    let (old, new) = fixture.bodies_of_diff(&project, ChangeGroup::Staged, "collapsed.txt");
    assert_eq!(text(&old).as_deref(), Some("one\ntwo\n"));
    assert_eq!(text(&new).as_deref(), Some("ONE\ntwo\n"));
    assert_eq!((read_lines(&old), read_lines(&new)), (Some(2), Some(2)));

    // A `filter` attribute whose driver cleans nothing leaves the disk side readable.
    for path in ["smudged.txt", "hollow.txt"] {
        let (_, new) = fixture.bodies_of_diff(&project, ChangeGroup::Unstaged, path);
        assert_eq!(
            text(&new).map(|body| body.lines().count()),
            Some(8),
            "{path}"
        );
        assert_eq!(read_lines(&new), Some(8), "{path}");
    }

    // An untracked file is shown as itself and has no patch to expand, filter or no filter.
    let untracked = fixture
        .change(
            &project,
            None,
            ChangeGroup::Untracked,
            SideRef::Absent,
            present("untracked.txt"),
        )
        .unwrap();
    assert_eq!(untracked.patch, None);
    assert_eq!(read_body(&untracked.new).as_deref(), Some(&eight[..]));
    let no_patch = fixture.bodies(
        &project,
        ChangeGroup::Untracked,
        (SideRef::Absent, present("untracked.txt")),
        (SideVersion::Absent, version(&untracked.new)),
    );
    assert_eq!(code(no_patch), error_code::INVALID_CHANGE);
}

/// The refusal is decided from the attributes and the configuration alone: nothing of it writes.
#[test]
fn deciding_a_path_is_clean_filtered_writes_nothing_to_the_repository() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "bodies-filter-readonly",
        &[
            (b".gitattributes", b"collapsed.txt filter=collapse\n"),
            (b"stale.txt", b"one\n"),
        ],
    );
    git(&repo, &["config", "filter.collapse.clean", "sed -n '1,2p'"]);
    write(repo.join("collapsed.txt"), b"one\ntwo\nthree\n");
    git(&repo, &["add", "collapsed.txt"]);
    git(&repo, &["commit", "-q", "-m", "collapsed"]);
    write(repo.join("collapsed.txt"), b"ONE\ntwo\nthree\n");
    let project = fixture.project("p", &repo);
    let sides = || (present("collapsed.txt"), present("collapsed.txt"));
    let diff = fixture
        .change(
            &project,
            None,
            ChangeGroup::Unstaged,
            present("collapsed.txt"),
            present("collapsed.txt"),
        )
        .unwrap();
    let versions = (version(&diff.old), version(&diff.new));
    // Last, after every `git` the setup runs, and on a file the read does not look at, whose
    // version it would otherwise find changed: see `move_modification_time_back`.
    move_modification_time_back(&repo.join("stale.txt"));

    let before = git_dir_state(&repo);
    let refused = fixture.bodies(&project, ChangeGroup::Unstaged, sides(), versions);
    assert_eq!(code(refused), error_code::CLEAN_FILTERED);
    assert_eq!(git_dir_state(&repo), before, "the read wrote under .git");
    assert!(!repo.join(".git/index.lock").exists());

    // The control: a plain `git status` over the same fixture rewrites the index.
    let status = fixture_git(&repo).arg("status").output().unwrap();
    assert!(status.status.success());
    assert_ne!(
        git_dir_state(&repo),
        before,
        "the fixture does not provoke an index refresh"
    );
}
