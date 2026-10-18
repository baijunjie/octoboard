//! The line counts a change listing carries: what is counted for each group, and what is left
//! uncounted.

use super::change_tests::write;
use super::commits_tests::commit_at;
use super::tests::Fixture;
use crate::protocol::{ChangeEntry, ChangeSide, LineStats};
use crate::test_support::{git, repo_with};

/// The counts of the change whose new side (or old, when it has no new one) is `path`, per group.
fn stats_of(changes: &[ChangeEntry], group: &str, path: &str) -> Option<LineStats> {
    let named =
        |side: &ChangeSide| matches!(side, ChangeSide::Present { path: p, .. } if p == path);
    changes
        .iter()
        .find_map(|entry| match (group, entry) {
            ("staged", ChangeEntry::Staged { old, new, stats })
            | ("unstaged", ChangeEntry::Unstaged { old, new, stats })
            | ("untracked", ChangeEntry::Untracked { old, new, stats })
            | ("committed", ChangeEntry::Committed { old, new, stats })
                if named(new) || (matches!(new, ChangeSide::Absent) && named(old)) =>
            {
                Some(*stats)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {group} change of {path} in {changes:#?}"))
}

fn lines(added: u64, removed: u64) -> Option<LineStats> {
    Some(LineStats { added, removed })
}

#[test]
fn a_worktrees_changes_carry_what_each_adds_and_removes() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "counts-worktree",
        &[
            (b"edited.txt", b"a\nb\nc\n"),
            (b"both.txt", b"1\n2\n"),
            (b"gone.txt", b"x\ny\n"),
            (b"old.txt", b"one\ntwo\nthree\nfour\n"),
            (b"image.bin", b"\0\x01\x02"),
            (b"mode.sh", b"echo\n"),
        ],
    );
    write(repo.join("edited.txt"), b"a\nB\nc\nd\n");
    write(repo.join("both.txt"), b"1\n2\n3\n");
    git(&repo, &["add", "both.txt"]);
    write(repo.join("both.txt"), b"1\n3\n");
    std::fs::remove_file(repo.join("gone.txt")).unwrap();
    git(&repo, &["mv", "old.txt", "new.txt"]);
    write(repo.join("new.txt"), b"one\ntwo\nthree\nfour\nfive\n");
    git(&repo, &["add", "new.txt"]);
    write(repo.join("image.bin"), b"\0\x01\x03");
    write(repo.join("fresh.txt"), b"p\nq\nr");
    write(repo.join("blob.dat"), b"\0binary");
    write(repo.join("empty.txt"), b"");
    let mut mode = std::fs::metadata(repo.join("mode.sh"))
        .unwrap()
        .permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut mode, 0o755);
    std::fs::set_permissions(repo.join("mode.sh"), mode).unwrap();
    let project = fixture.project("p", &repo);

    let (_, changes, _) = fixture.changes(&project, None).unwrap();
    assert_eq!(stats_of(&changes, "unstaged", "edited.txt"), lines(2, 1));
    assert_eq!(stats_of(&changes, "staged", "both.txt"), lines(1, 0));
    assert_eq!(
        stats_of(&changes, "unstaged", "both.txt"),
        lines(0, 1),
        "the unstaged half is counted against the index, not HEAD"
    );
    assert_eq!(stats_of(&changes, "unstaged", "gone.txt"), lines(0, 2));
    assert_eq!(
        stats_of(&changes, "staged", "new.txt"),
        lines(1, 0),
        "a rename counts what changed on top of it"
    );
    assert_eq!(stats_of(&changes, "unstaged", "image.bin"), None);
    assert_eq!(
        stats_of(&changes, "untracked", "fresh.txt"),
        lines(3, 0),
        "an unterminated last line counts"
    );
    assert_eq!(stats_of(&changes, "untracked", "blob.dat"), None);
    assert_eq!(stats_of(&changes, "untracked", "empty.txt"), lines(0, 0));
    assert_eq!(
        stats_of(&changes, "unstaged", "mode.sh"),
        lines(0, 0),
        "a change of mode alone counts nothing"
    );
}

#[test]
fn a_rename_across_the_projects_boundary_and_a_big_untracked_file_have_no_count() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "counts-scope",
        &[(b"sub/in.txt", b"1\n"), (b"out.txt", b"1\n")],
    );
    write(repo.join("sub/in.txt"), b"1\n2\n");
    git(&repo, &["mv", "out.txt", "sub/moved.txt"]);
    let huge = vec![b'x'; crate::browse::budget::MAX_FILE_BYTES + 1];
    write(repo.join("sub/huge.txt"), &huge);
    let project = fixture.project("p", &repo.join("sub"));

    let (_, changes, _) = fixture.changes(&project, None).unwrap();
    assert_eq!(stats_of(&changes, "unstaged", "in.txt"), lines(1, 0));
    assert_eq!(
        stats_of(&changes, "staged", "moved.txt"),
        None,
        "a rename from outside the project has no count of its own"
    );
    assert_eq!(stats_of(&changes, "untracked", "huge.txt"), None);
}

#[test]
fn a_count_run_that_pairs_a_rename_the_listing_does_not_gives_no_count() {
    let fixture = Fixture::new();
    let original: String = (1..=20).map(|n| format!("line {n}\n")).collect();
    let repo = repo_with("counts-pairing", &[(b"p/d.txt", original.as_bytes())]);
    std::fs::create_dir(repo.join("q")).unwrap();
    git(&repo, &["mv", "p/d.txt", "q/d.txt"]);
    let similar = original.replace("line 5\n", "five\n") + "line 21\n";
    write(repo.join("p/new.txt"), similar.as_bytes());
    git(&repo, &["add", "p/new.txt"]);
    let project = fixture.project("p", &repo.join("p"));

    // The listing pairs `p/d.txt` with its exact copy `q/d.txt`, so `p/new.txt` is an addition;
    // within `p` alone, the count run pairs the two files of `p` instead.
    let (_, changes, _) = fixture.changes(&project, None).unwrap();
    assert_eq!(stats_of(&changes, "staged", "new.txt"), None);
}

#[test]
fn untracked_files_past_the_listings_budget_are_listed_without_a_count() {
    let fixture = Fixture::new();
    let repo = repo_with("counts-untracked-budget", &[(b"a.txt", b"1\n")]);
    write(repo.join("a-small.txt"), b"1\n2\n");
    write(repo.join("b-big.txt"), &b"x\n".repeat(100));
    let project = fixture.project("p", &repo);

    super::line_counts::UNTRACKED_BUDGET_OVERRIDE.with(|limit| limit.set(Some(50)));
    let listed = fixture.changes(&project, None);
    super::line_counts::UNTRACKED_BUDGET_OVERRIDE.with(|limit| limit.set(None));
    let (_, changes, _) = listed.unwrap();
    assert_eq!(stats_of(&changes, "untracked", "a-small.txt"), lines(2, 0));
    assert_eq!(stats_of(&changes, "untracked", "b-big.txt"), None);
}

#[test]
fn a_failed_count_run_leaves_the_listing_without_counts() {
    let fixture = Fixture::new();
    let repo = repo_with("counts-failed", &[(b"a.txt", b"1\n")]);
    write(repo.join("a.txt"), b"1\n2\n");
    let project = fixture.project("p", &repo);

    let (_, counted, _) = fixture.changes(&project, None).unwrap();
    assert_eq!(stats_of(&counted, "unstaged", "a.txt"), lines(1, 0));
    super::line_counts::COUNT_STDOUT_OVERRIDE.with(|limit| limit.set(Some(1)));
    let uncounted = fixture.changes(&project, None);
    super::line_counts::COUNT_STDOUT_OVERRIDE.with(|limit| limit.set(None));
    let (_, uncounted, complete) = uncounted.unwrap();
    assert!(complete);
    assert_eq!(stats_of(&uncounted, "unstaged", "a.txt"), None);
}

#[test]
fn a_comparison_and_a_commit_list_carry_counts_too() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "counts-committed",
        &[
            (b"a.txt", b"1\n2\n3\n"),
            (b"b.txt", b"b\n"),
            (b"bin", b"\0a"),
        ],
    );
    let fork = super::compare_tests::rev_parse(&repo, "main");
    git(&repo, &["checkout", "-q", "-b", "feature"]);
    commit_at(
        &repo,
        "first",
        10,
        &[
            ("a.txt", Some(b"1\n3\n4\n5\n".as_slice())),
            ("bin", Some(b"\0b".as_slice())),
        ],
    );
    let second = commit_at(
        &repo,
        "second",
        20,
        &[("new.txt", Some(b"n\n".as_slice())), ("b.txt", None)],
    );
    let project = fixture.project("p", &repo);

    let compared = fixture.compare(&project, "main", "feature").unwrap();
    assert_eq!(
        stats_of(&compared.changes, "committed", "a.txt"),
        lines(2, 1)
    );
    assert_eq!(
        stats_of(&compared.changes, "committed", "new.txt"),
        lines(1, 0)
    );
    assert_eq!(
        stats_of(&compared.changes, "committed", "b.txt"),
        lines(0, 1)
    );
    assert_eq!(stats_of(&compared.changes, "committed", "bin"), None);

    let listed = fixture.commits(&project, &fork, &second).unwrap();
    let by_subject = |subject: &str| {
        &listed
            .commits
            .iter()
            .find(|commit| commit.subject == subject)
            .unwrap()
            .changes
    };
    assert_eq!(
        stats_of(by_subject("first"), "committed", "a.txt"),
        lines(2, 1)
    );
    assert_eq!(
        stats_of(by_subject("second"), "committed", "new.txt"),
        lines(1, 0)
    );
    assert_eq!(
        stats_of(by_subject("second"), "committed", "b.txt"),
        lines(0, 1)
    );
}
