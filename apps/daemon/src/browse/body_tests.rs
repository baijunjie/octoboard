//! The whole bodies of a change's two sides, read end to end against real repositories: each kind
//! of diff side, a version that no longer matches the patch's, and a body past the file budget.

use super::change_tests::{present, read_body, write};
use super::compare_tests::{commit_on, Compared};
use super::tests::{code, Fixture};
use super::*;
use crate::protocol::{ChangeGroup, ChangeRef, ComparedChangeRef, SideRead, SideRef, SideVersion};
use crate::test_support::{git, repo_with};

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
    // Unstaged: the index against the disk.
    let (old, new) = fixture.bodies_of_diff(&project, ChangeGroup::Unstaged, "a.txt");
    assert_eq!(text(&old).as_deref(), Some("1\nTWO\n3\n"));
    assert_eq!(text(&new).as_deref(), Some("1\nTWO\nTHREE\n"));

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
