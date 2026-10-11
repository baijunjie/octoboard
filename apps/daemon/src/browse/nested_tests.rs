//! A rename into or out of a path below itself, read end to end against real repositories: the
//! diff `git` computes between its two versions in the staged group and in a comparison, what
//! falls back to the route that cuts a patch down to the change's own sections, and what the read
//! leaves alone.

use std::path::Path;

use super::change_tests::{present, read_body, read_kind, write};
use super::changes::AFTER_PATCH;
use super::compare_tests::commit_on;
use super::tests::{code, Fixture};
use super::*;
use crate::protocol::{ChangeGroup, SideKind, SideRead};
use crate::test_support::{
    fixture_git, git, git_dir_state, git_stdin, move_modification_time_back, repo_with,
};

/// Twelve lines headed `tag`, those of `changed` marked.
fn lines(tag: &str, changed: &[usize]) -> Vec<u8> {
    (0..12)
        .map(|n| match changed.contains(&n) {
            true => format!("{tag} changed {n}\n"),
            false => format!("{tag} line {n}\n"),
        })
        .collect::<String>()
        .into_bytes()
}

/// Moves `from` to `to`, a path below it or above it, with `body`, in the index.
fn stage_down(repo: &Path, from: &str, to: &str, body: &[u8]) {
    git(repo, &["rm", "-q", from]);
    write(repo.join(to), body);
    git(repo, &["add", to]);
}

fn staged(
    fixture: &Fixture,
    project: &str,
    from: &str,
    to: &str,
) -> anyhow::Result<super::change_tests::Diff> {
    fixture.change(
        project,
        None,
        ChangeGroup::Staged,
        present(from),
        present(to),
    )
}

fn sections(patch: &str) -> usize {
    patch.matches("diff --git").count()
}

#[test]
fn a_compared_rename_below_or_above_itself_is_one_diff_between_its_versions() {
    let fixture = Fixture::new();
    // `foo/other` sits beside the new path and may not reach the reply.
    let repo = repo_with(
        "nested-views",
        &[(b"foo", &lines("foo", &[])), (b"up", &lines("up", &[]))],
    );
    let project = fixture.project("p", &repo);
    commit_on(
        &repo,
        "moved",
        "main",
        &[
            ("foo", None),
            ("foo/bar", Some(&lines("foo", &[3, 7]))),
            ("foo/other", Some(b"OTHER\n")),
            ("up", None),
            ("up/bar", Some(&lines("up", &[]))),
        ],
    );

    // Both nesting directions and both comparison directions.
    for (left, right, from, to, edited) in [
        ("main", "moved", "foo", "foo/bar", true),
        ("moved", "main", "foo/bar", "foo", true),
        ("main", "moved", "up", "up/bar", false),
        ("moved", "main", "up/bar", "up", false),
    ] {
        let compared = fixture.compare(&project, left, right).unwrap();
        let read = fixture
            .compared_change(&project, &compared, present(from), present(to))
            .unwrap();
        let patch = read.patch.expect("a patch");
        assert_eq!(
            (read_kind(&read.old), read_kind(&read.new)),
            (SideKind::File, SideKind::File)
        );
        if edited {
            assert_eq!(sections(&patch), 1, "{left}..{right} {patch}");
            assert!(
                patch.starts_with(&format!("diff --git a/{from} b/{to}\n")),
                "{patch}"
            );
            assert!(patch.contains("@@") && patch.contains("changed"), "{patch}");
            assert!(!patch.contains("file mode"), "{patch}");
        } else {
            assert_eq!(patch, "", "identical content is an empty diff");
        }
        assert!(!patch.contains("OTHER"), "{patch}");
    }
}

#[test]
fn the_shapes_git_writes_for_a_pair_pass_through_as_they_are() {
    let fixture = Fixture::new();
    let binary: Vec<u8> = b"\0binary one\n".to_vec();
    let link = |target: &str, at: &Path| std::os::unix::fs::symlink(target, at).unwrap();
    let repo = repo_with(
        "nested-shapes",
        &[
            (b"exec", b"#!/bin/sh\n"),
            (b"bin", &binary),
            (b"typed", b"content\n"),
        ],
    );
    link("target-one", &repo.join("link"));
    git(&repo, &["add", "link"]);
    git(&repo, &["commit", "-q", "-m", "link"]);
    let project = fixture.project("p", &repo);

    // Only the executable bit differs.
    git(&repo, &["rm", "-q", "exec"]);
    write(repo.join("exec/bar"), b"#!/bin/sh\n");
    git(&repo, &["add", "exec/bar"]);
    git(&repo, &["update-index", "--chmod=+x", "exec/bar"]);
    let diff = staged(&fixture, &project, "exec", "exec/bar").unwrap();
    let patch = diff.patch.unwrap();
    assert_eq!(sections(&patch), 1, "{patch}");
    assert!(
        patch.contains("old mode 100644\nnew mode 100755\n") && !patch.contains("@@"),
        "{patch}"
    );

    // A binary side is one line, so both bodies still come with the reply.
    git(&repo, &["rm", "-q", "bin"]);
    write(repo.join("bin/bar"), b"\0binary two\n");
    git(&repo, &["add", "bin/bar"]);
    let diff = staged(&fixture, &project, "bin", "bin/bar").unwrap();
    let patch = diff.patch.clone().unwrap();
    assert_eq!(patch.matches("Binary files").count(), 1, "{patch}");
    assert!(patch.contains("Binary files a/bin and b/bin/bar differ"));
    assert_eq!(read_body(&diff.old).as_deref(), Some(binary.as_slice()));
    assert_eq!(
        read_body(&diff.new).as_deref(),
        Some(b"\0binary two\n".as_slice())
    );

    // A symbolic link renamed to another symbolic link.
    git(&repo, &["rm", "-q", "link"]);
    std::fs::create_dir(repo.join("link")).unwrap();
    link("target-two", &repo.join("link/bar"));
    git(&repo, &["add", "link/bar"]);
    let diff = staged(&fixture, &project, "link", "link/bar").unwrap();
    assert_eq!(
        (read_kind(&diff.old), read_kind(&diff.new)),
        (SideKind::Symlink, SideKind::Symlink)
    );
    let patch = diff.patch.unwrap();
    assert_eq!(sections(&patch), 1, "{patch}");
    assert!(patch.contains("120000") && patch.contains("+target-two"));

    // A type change as well stays two sections, which the client joins.
    git(&repo, &["rm", "-q", "typed"]);
    std::fs::create_dir(repo.join("typed")).unwrap();
    link("elsewhere", &repo.join("typed/bar"));
    git(&repo, &["add", "typed/bar"]);
    let diff = staged(&fixture, &project, "typed", "typed/bar").unwrap();
    assert_eq!(
        (read_kind(&diff.old), read_kind(&diff.new)),
        (SideKind::File, SideKind::Symlink)
    );
    let patch = diff.patch.unwrap();
    assert_eq!(sections(&patch), 2, "{patch}");
    assert!(patch.contains("deleted file mode") && patch.contains("new file mode"));
}

#[test]
fn a_submodule_side_and_an_absent_side_keep_the_route_that_cuts_the_patch_down() {
    let fixture = Fixture::new();
    let gitlink = |repo: &Path, path: &str, oid: &str| {
        git(
            repo,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("160000,{oid},{path}"),
            ],
        )
    };
    let (one, two) = ("1".repeat(40), "2".repeat(40));
    let repo = repo_with(
        "nested-fallbacks",
        &[(b"file", &lines("f", &[])), (b"other", b"OTHER\n")],
    );
    gitlink(&repo, "mod", &one);
    git(&repo, &["commit", "-q", "-m", "submodule"]);
    let before = String::from_utf8(git(&repo, &["rev-parse", "HEAD"])).unwrap();
    git(&repo, &["rm", "-q", "--cached", "mod"]);
    gitlink(&repo, "mod/sub", &two);
    git(&repo, &["commit", "-q", "-m", "nested"]);
    let project = fixture.project("p", &repo);

    // Two gitlinks named as `<commit>:<path>` would have `git` look for commits this repository
    // does not have and diff whole trees; they are read by their paths, as a removal and an
    // addition showing the commits they record.
    git(&repo, &["branch", "before", before.trim()]);
    let compared = fixture.compare(&project, "before", "main").unwrap();
    let read = fixture
        .compared_change(&project, &compared, present("mod"), present("mod/sub"))
        .unwrap();
    let patch = read.patch.unwrap();
    assert_eq!(sections(&patch), 2, "{patch}");
    assert!(
        patch.contains(&format!("-Subproject commit {one}"))
            && patch.contains(&format!("+Subproject commit {two}")),
        "{patch}"
    );
    assert!(!patch.contains("OTHER") && !patch.contains("changed"));

    // A side gone from the index since the change was listed: its path alone is read.
    stage_down(&repo, "file", "file/bar", &lines("f", &[1]));
    git(&repo, &["rm", "-q", "--cached", "file/bar"]);
    let diff = staged(&fixture, &project, "file", "file/bar").unwrap();
    assert!(matches!(diff.new, SideRead::Absent), "{diff:?}");
    let patch = diff.patch.unwrap();
    assert_eq!(sections(&patch), 1, "{patch}");
    assert!(patch.contains("deleted file mode"), "{patch}");
}

#[test]
fn a_file_named_like_an_operand_is_never_taken_for_the_path_of_the_diff() {
    let fixture = Fixture::new();
    let repo = repo_with("nested-operand", &[(b"foo", &lines("foo", &[]))]);
    let project = fixture.project("p", &repo);
    stage_down(&repo, "foo", "foo/bar", &lines("foo", &[2]));
    // `:0:foo/bar` as a file in the worktree.
    write(repo.join(":0:foo/bar"), b"in the way\n");
    // The control: without the trailing `--` this very pair of operands is refused.
    let refused = fixture_git(&repo)
        .args(["diff", "HEAD:foo", ":0:foo/bar"])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "the operands were not ambiguous");
    assert!(String::from_utf8_lossy(&refused.stderr).contains("ambiguous argument"));

    let patch = staged(&fixture, &project, "foo", "foo/bar")
        .unwrap()
        .patch
        .unwrap();
    assert_eq!(sections(&patch), 1, "{patch}");
    assert!(!patch.contains("in the way"), "{patch}");

    // The same in a comparison, where the operand starts with a commit id.
    git(&repo, &["commit", "-q", "-m", "moved"]);
    git(&repo, &["branch", "moved"]);
    git(&repo, &["checkout", "-q", "-B", "base", "HEAD~1"]);
    let base = String::from_utf8(git(&repo, &["rev-parse", "HEAD"])).unwrap();
    git(&repo, &["checkout", "-q", "main"]);
    write(repo.join(format!("{}:foo", base.trim())), b"in the way\n");
    let compared = fixture.compare(&project, "base", "moved").unwrap();
    let patch = fixture
        .compared_change(&project, &compared, present("foo"), present("foo/bar"))
        .unwrap()
        .patch
        .unwrap();
    assert_eq!(sections(&patch), 1, "{patch}");
    assert!(!patch.contains("in the way"), "{patch}");
}

#[test]
fn a_nesting_pair_is_looked_up_in_the_index_one_path_at_a_time() {
    let fixture = Fixture::new();
    let repo = repo_with("nested-index", &[(b"foo", &lines("foo", &[]))]);
    let project = fixture.project("p", &repo);
    stage_down(&repo, "foo", "foo/bar", &lines("foo", &[2]));
    // Far more index entries below `foo` than one metadata read may print.
    let blob = String::from_utf8(git(&repo, &["rev-parse", "HEAD:foo"])).unwrap();
    let padding = "x".repeat(60);
    let entries: String = (0..15_000)
        .map(|n| format!("100644 {} 0\tfoo/{n:05}-{padding}\n", blob.trim()))
        .collect();
    git_stdin(&repo, &["update-index", "--index-info"], entries.as_bytes());

    // The control: looked up together, the pair lists all of it and is refused.
    let env = super::git::tests::test_env(&[]);
    let together = blob::index_entries(
        &env,
        &repo,
        &[b"foo".as_slice(), b"foo/bar".as_slice()],
        &AtomicBool::new(false),
    );
    assert!(together.is_err(), "the pair lookup was within its budget");

    let patch = staged(&fixture, &project, "foo", "foo/bar")
        .unwrap()
        .patch
        .unwrap();
    assert_eq!(sections(&patch), 1, "{patch}");
    assert!(!patch.contains("xxxx"), "{patch}");
}

#[test]
fn a_patch_past_the_budget_is_refused_and_a_moved_source_is_reported() {
    let fixture = Fixture::new();
    let big = |tag: &str| -> Vec<u8> {
        format!("{tag} line\n")
            .repeat(budget::MAX_PATCH_BYTES / 8)
            .into_bytes()
    };
    let repo = repo_with("nested-limits", &[(b"foo", &big("a")), (b"small", b"s\n")]);
    let project = fixture.project("p", &repo);
    stage_down(&repo, "foo", "foo/bar", &big("b"));
    assert_eq!(
        code(staged(&fixture, &project, "foo", "foo/bar")),
        error_code::LIMIT_EXCEEDED
    );

    // The index moves while the patch is made.
    stage_down(&repo, "small", "small/bar", b"t\n");
    let dir = repo.to_path_buf();
    AFTER_PATCH.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            write(dir.join("small/bar"), b"moved\n");
            git(&dir, &["add", "small/bar"]);
        }))
    });
    let result = staged(&fixture, &project, "small", "small/bar");
    AFTER_PATCH.with(|hook| hook.borrow_mut().take());
    assert_eq!(code(result), error_code::SOURCE_CHANGED);
    assert!(staged(&fixture, &project, "small", "small/bar").is_ok());
}

#[test]
fn a_comparisons_pinned_commits_are_read_even_after_its_branch_moves() {
    let fixture = Fixture::new();
    let repo = repo_with("nested-pinned", &[(b"foo", &lines("foo", &[]))]);
    let project = fixture.project("p", &repo);
    commit_on(
        &repo,
        "moved",
        "main",
        &[("foo", None), ("foo/bar", Some(&lines("foo", &[4])))],
    );
    let compared = fixture.compare(&project, "main", "moved").unwrap();
    commit_on(
        &repo,
        "moved",
        "moved",
        &[("foo/bar", Some(&lines("foo", &[9])))],
    );
    let read = fixture
        .compared_change(&project, &compared, present("foo"), present("foo/bar"))
        .unwrap();
    let patch = read.patch.unwrap();
    assert!(patch.contains("+foo changed 4\n"), "{patch}");
    assert!(!patch.contains("changed 9"), "{patch}");
}

#[test]
fn reading_a_nesting_pair_writes_nothing_to_the_repository() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "nested-readonly",
        &[(b"foo", &lines("foo", &[])), (b"stale.txt", b"one\n")],
    );
    stage_down(&repo, "foo", "foo/bar", &lines("foo", &[2]));
    git(&repo, &["commit", "-q", "-m", "moved"]);
    git(&repo, &["branch", "moved"]);
    git(&repo, &["checkout", "-q", "-B", "base", "HEAD~1"]);
    git(&repo, &["checkout", "-q", "main"]);
    stage_down(&repo, "foo/bar", "foo", &lines("foo", &[5]));
    let project = fixture.project("p", &repo);
    // Last, after every `git` the setup runs: see `move_modification_time_back`.
    move_modification_time_back(&repo.join("stale.txt"));

    let before = git_dir_state(&repo);
    let staged_read = staged(&fixture, &project, "foo/bar", "foo").unwrap();
    assert_eq!(sections(&staged_read.patch.unwrap()), 1);
    let compared = fixture.compare(&project, "base", "moved").unwrap();
    fixture
        .compared_change(&project, &compared, present("foo"), present("foo/bar"))
        .unwrap();
    assert_eq!(git_dir_state(&repo), before, "the reads wrote under .git");
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

#[test]
fn the_fallback_route_never_pairs_a_path_with_a_third_one_below_the_outer() {
    let fixture = Fixture::new();
    let repo = repo_with("nested-hazard", &[(b"foo", &lines("foo", &[]))]);
    let project = fixture.project("p", &repo);
    // `foo` becomes a directory holding a submodule (so the pair route is not taken) and, beside
    // it, a file with nearly `foo`'s content: with rename detection on, `git` would pair `foo`
    // with that file, and the change's own sections would be filtered away.
    git(&repo, &["rm", "-q", "foo"]);
    write(repo.join("foo/moved"), &lines("foo", &[0]));
    git(&repo, &["add", "foo/moved"]);
    git(
        &repo,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{},foo/bar", "1".repeat(40)),
        ],
    );
    let diff = staged(&fixture, &project, "foo", "foo/bar").unwrap();
    let patch = diff.patch.unwrap();
    assert_eq!(sections(&patch), 2, "{patch}");
    assert!(patch.contains("deleted file mode") && patch.contains("Subproject commit"));
    assert!(
        !patch.contains("changed 0") && !patch.contains("rename from"),
        "{patch}"
    );
}

#[test]
fn the_pair_is_named_by_revision_and_path_so_the_repositorys_attributes_apply() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "nested-attributes",
        &[
            (b".gitattributes", b"foo -diff\nfoo/bar -diff\n"),
            (b"foo", &lines("foo", &[])),
        ],
    );
    let project = fixture.project("p", &repo);
    stage_down(&repo, "foo", "foo/bar", &lines("foo", &[3]));

    // The control: the same two versions, named by object id, have no path to look an attribute
    // up by, so they are diffed as the text they are.
    let ids = |spec: &str| String::from_utf8(git(&repo, &["rev-parse", spec])).unwrap();
    let (old, new) = (ids("HEAD:foo"), ids(":0:foo/bar"));
    let bare = git(&repo, &["diff", old.trim(), new.trim(), "--"]);
    assert!(String::from_utf8(bare).unwrap().contains("@@"));

    let patch = staged(&fixture, &project, "foo", "foo/bar")
        .unwrap()
        .patch
        .unwrap();
    assert!(
        patch.starts_with("diff --git a/foo b/foo/bar\n"),
        "the sides are named by their paths: {patch}"
    );
    assert!(
        patch.ends_with("Binary files a/foo and b/foo/bar differ\n") && !patch.contains("@@"),
        "a path marked -diff is not diffed as text: {patch}"
    );
}
