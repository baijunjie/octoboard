//! A worktree's changes listed and read end to end against real repositories: what each group
//! compares, which sides a change has, what stays out of a project's reply, and what a change made
//! while it is read turns into.

use std::path::Path;

use super::body_tests::version;
use super::changes::AFTER_PATCH;
use super::tests::{bytes_of, code, Fixture};
use super::*;
use crate::protocol::{
    ChangeEntry, ChangeGroup, ChangeRef, ChangeSide, ConflictKind, ContentSource, SideKind,
    SideRead, SideRef,
};
use crate::test_support::{
    fixture_git, git, git_has_show_ref_exists, git_stdin, repo_with, ScratchDir,
};

/// A change as `read_project_change` answered it.
#[derive(Debug)]
pub(super) struct Diff {
    head: Option<String>,
    pub(super) old: SideRead,
    pub(super) new: SideRead,
    pub(super) patch: Option<String>,
    /// The whole reply as it goes on the wire.
    json: String,
}

impl Fixture {
    fn changes(
        &self,
        project: &str,
        worktree: Option<&str>,
    ) -> anyhow::Result<(Option<String>, Vec<ChangeEntry>, bool)> {
        match self.serve(BrowseBody::ListProjectChanges {
            project: project.to_string(),
            worktree: worktree.map(str::to_string),
        })? {
            Event::ProjectChanges {
                head,
                changes,
                complete,
                worktree: echoed,
                ..
            } => {
                assert_eq!(echoed.as_deref(), worktree, "the reply's echo");
                Ok((head, changes, complete))
            }
            other => panic!("not a change list: {other:?}"),
        }
    }

    pub(super) fn change(
        &self,
        project: &str,
        worktree: Option<&str>,
        group: ChangeGroup,
        old: SideRef,
        new: SideRef,
    ) -> anyhow::Result<Diff> {
        let event = self.serve(BrowseBody::ReadProjectChange {
            project: project.to_string(),
            worktree: worktree.map(str::to_string),
            change: ChangeRef { group, old, new },
        })?;
        let json = serde_json::to_string(&event).unwrap();
        match event {
            Event::ProjectChange {
                head,
                old,
                new,
                patch,
                group: echoed,
                ..
            } => {
                assert_eq!(echoed, group, "the reply's echo");
                Ok(Diff {
                    head,
                    old: *old,
                    new: *new,
                    patch: patch.map(|patch| String::from_utf8(bytes_of(&patch)).unwrap()),
                    json,
                })
            }
            other => panic!("not a change: {other:?}"),
        }
    }
}

pub(super) fn present(path: &str) -> SideRef {
    SideRef::Present {
        path: path.to_string(),
    }
}

/// The path a side names, inside the project or outside it.
fn side_path(side: &ChangeSide) -> Option<&str> {
    match side {
        ChangeSide::Present { path, .. } => Some(path),
        ChangeSide::OutOfScope { repository_path } => Some(repository_path),
        ChangeSide::Absent => None,
    }
}

/// The entry of `group` one of whose sides is `path`.
fn entry<'a>(
    changes: &'a [ChangeEntry],
    group: ChangeGroup,
    path: &str,
) -> (&'a ChangeSide, &'a ChangeSide) {
    changes
        .iter()
        .find_map(|entry| {
            let (g, old, new) = match entry {
                ChangeEntry::Staged { old, new } => (ChangeGroup::Staged, old, new),
                ChangeEntry::Unstaged { old, new } => (ChangeGroup::Unstaged, old, new),
                ChangeEntry::Untracked { old, new } => (ChangeGroup::Untracked, old, new),
                ChangeEntry::Conflicted { .. } | ChangeEntry::Committed { .. } => return None,
            };
            let names = side_path(old) == Some(path) || side_path(new) == Some(path);
            (g == group && names).then_some((old, new))
        })
        .unwrap_or_else(|| panic!("no {group:?} change of {path} in {changes:#?}"))
}

/// What every listed change names, as `(group, old, new)` with `-` for an absent side.
pub(super) fn summary(changes: &[ChangeEntry]) -> Vec<String> {
    let name = |side: &ChangeSide| side_path(side).unwrap_or("-").to_string();
    let mut all: Vec<String> = changes
        .iter()
        .map(|entry| match entry {
            ChangeEntry::Staged { old, new } => format!("staged {} {}", name(old), name(new)),
            ChangeEntry::Unstaged { old, new } => format!("unstaged {} {}", name(old), name(new)),
            ChangeEntry::Untracked { old, new } => format!("untracked {} {}", name(old), name(new)),
            ChangeEntry::Conflicted { path, conflict } => format!("conflicted {path} {conflict:?}"),
            ChangeEntry::Committed { old, new } => format!("committed {} {}", name(old), name(new)),
        })
        .collect();
    all.sort();
    all
}

fn source_of(side: &ChangeSide) -> &ContentSource {
    match side {
        ChangeSide::Present { source, .. } => source,
        other => panic!("not present: {other:?}"),
    }
}

pub(super) fn read_source(side: &SideRead) -> &ContentSource {
    match side {
        SideRead::Present { source, .. } => source,
        other => panic!("not present: {other:?}"),
    }
}

pub(super) fn read_body(side: &SideRead) -> Option<Vec<u8>> {
    match side {
        SideRead::Present { file, .. } => file.as_ref().map(bytes_of),
        _ => None,
    }
}

pub(super) fn read_kind(side: &SideRead) -> SideKind {
    match side {
        SideRead::Present { kind, .. } => *kind,
        other => panic!("not present: {other:?}"),
    }
}

pub(super) fn head_of(dir: &Path) -> String {
    String::from_utf8(git(dir, &["rev-parse", "HEAD"]))
        .unwrap()
        .trim()
        .to_string()
}

pub(super) fn write(path: impl AsRef<Path>, body: &[u8]) {
    let path = path.as_ref();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// Adds a linked worktree of `repo` at `<parent>/<name>` with `args` (a new branch, `--detach`).
pub(super) fn add_worktree(
    repo: &Path,
    parent: &Path,
    name: &str,
    args: &[&str],
) -> std::path::PathBuf {
    let path = parent.join(name);
    let mut all = vec!["worktree", "add", "-q"];
    all.extend_from_slice(args);
    all.push(path.to_str().unwrap());
    git(repo, &all);
    path
}

pub(super) fn worktree_id(fixture: &Fixture, project: &str, root: &Path) -> String {
    let source = fixture.source(project).git.unwrap();
    let root = super::tests::canonical(root);
    source
        .worktrees
        .into_iter()
        .find(|w| w.root == root)
        .unwrap()
        .id
}

#[test]
fn each_worktree_lists_its_own_changes_inside_the_project_only() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "changes-worktrees",
        &[
            (b"sub/a.txt", b"a\n"),
            (b"sub/b.txt", b"b\n"),
            (b"top.txt", b"top\n"),
            (b".gitignore", b"*.log\n"),
        ],
    );
    let parent = ScratchDir::new("changes-worktrees-linked");
    let side = add_worktree(&repo, &parent, "side", &["-b", "side"]);
    let detached = add_worktree(&repo, &parent, "detached", &["--detach"]);
    write(repo.join("sub/a.txt"), b"a changed\n");
    write(repo.join("sub/b.txt"), b"b staged\n");
    git(&repo, &["add", "sub/b.txt"]);
    write(repo.join("sub/new.txt"), b"new\n");
    write(repo.join("sub/ignored.log"), b"ignored\n");
    write(repo.join("top.txt"), b"outside the project\n");
    write(side.join("sub/b.txt"), b"b on side\n");
    write(detached.join("sub/a.txt"), b"a detached\n");
    git(&detached, &["add", "sub/a.txt"]);

    let project = fixture.project("sub", &repo.join("sub"));
    let (head, changes, complete) = fixture.changes(&project, None).unwrap();
    assert!(complete);
    assert_eq!(head.as_deref(), Some(head_of(&repo).as_str()));
    assert_eq!(
        summary(&changes),
        [
            "staged b.txt b.txt",
            "unstaged a.txt a.txt",
            "untracked - new.txt"
        ],
        "nothing outside the project, and no ignored file"
    );
    // The disk side's version is the one a live read reports.
    let (_, new) = entry(&changes, ChangeGroup::Unstaged, "a.txt");
    let (read, _) = fixture
        .read(&project, None, "a.txt", ReadFrom::Live)
        .unwrap();
    assert_eq!(source_of(new), &read);

    let side_id = worktree_id(&fixture, &project, &side);
    let (side_head, side_changes, _) = fixture.changes(&project, Some(&side_id)).unwrap();
    assert_eq!(side_head.as_deref(), Some(head_of(&side).as_str()));
    assert_eq!(summary(&side_changes), ["unstaged b.txt b.txt"]);

    let detached_id = worktree_id(&fixture, &project, &detached);
    let (detached_head, detached_changes, _) =
        fixture.changes(&project, Some(&detached_id)).unwrap();
    assert_eq!(detached_head.as_deref(), Some(head_of(&detached).as_str()));
    assert_eq!(summary(&detached_changes), ["staged a.txt a.txt"]);
    let diff = fixture
        .change(
            &project,
            Some(&detached_id),
            ChangeGroup::Staged,
            present("a.txt"),
            present("a.txt"),
        )
        .unwrap();
    assert!(diff.patch.unwrap().contains("+a detached\n"));
}

#[test]
fn a_partially_staged_file_is_one_change_per_group_each_against_its_own_baseline() {
    let fixture = Fixture::new();
    let repo = repo_with("changes-partial", &[(b"p.txt", b"1\n2\n3\n")]);
    write(repo.join("p.txt"), b"1\nTWO\n3\n");
    git(&repo, &["add", "p.txt"]);
    write(repo.join("p.txt"), b"1\nTWO\nTHREE\n");
    let project = fixture.project("p", &repo);
    let (head, changes, _) = fixture.changes(&project, None).unwrap();
    let (staged_old, staged_new) = entry(&changes, ChangeGroup::Staged, "p.txt");
    let (unstaged_old, unstaged_new) = entry(&changes, ChangeGroup::Unstaged, "p.txt");
    assert!(
        matches!(source_of(staged_old), ContentSource::Commit { commit, .. }
            if Some(commit) == head.as_ref())
    );
    assert!(matches!(source_of(staged_new), ContentSource::Index { .. }));
    assert_eq!(
        source_of(staged_new),
        source_of(unstaged_old),
        "one index blob, two roles"
    );
    assert!(matches!(
        source_of(unstaged_new),
        ContentSource::Live { .. }
    ));

    let staged = fixture
        .change(
            &project,
            None,
            ChangeGroup::Staged,
            present("p.txt"),
            present("p.txt"),
        )
        .unwrap();
    let patch = staged.patch.unwrap();
    assert!(
        patch.contains("-2\n+TWO\n") && !patch.contains("THREE"),
        "{patch}"
    );
    assert_eq!(staged.head, head);
    assert_eq!(read_source(&staged.old), source_of(staged_old));
    assert_eq!(read_source(&staged.new), source_of(staged_new));
    assert_eq!(
        read_body(&staged.new),
        None,
        "a text change is carried by its patch"
    );

    let unstaged = fixture
        .change(
            &project,
            None,
            ChangeGroup::Unstaged,
            present("p.txt"),
            present("p.txt"),
        )
        .unwrap();
    let patch = unstaged.patch.unwrap();
    assert!(
        patch.contains("-3\n+THREE\n") && !patch.contains("-2\n"),
        "{patch}"
    );
    assert_eq!(read_source(&unstaged.old), source_of(unstaged_old));
    assert_eq!(read_source(&unstaged.new), source_of(unstaged_new));
}

#[test]
fn an_unborn_head_compares_staged_additions_with_an_empty_baseline() {
    let fixture = Fixture::new();
    let repo = ScratchDir::new("changes-unborn");
    git(&repo, &["init", "-q"]);
    write(repo.join("sub/a.txt"), b"first\n");
    git(&repo, &["add", "sub/a.txt"]);
    let project = fixture.project("sub", &repo.join("sub"));
    let (head, changes, _) = fixture.changes(&project, None).unwrap();
    assert_eq!(head, None);
    let (old, new) = entry(&changes, ChangeGroup::Staged, "a.txt");
    assert_eq!(old, &ChangeSide::Absent);
    assert!(matches!(source_of(new), ContentSource::Index { .. }));
    let diff = fixture
        .change(
            &project,
            None,
            ChangeGroup::Staged,
            SideRef::Absent,
            present("a.txt"),
        )
        .unwrap();
    assert_eq!(diff.head, None);
    assert!(matches!(diff.old, SideRead::Absent));
    let patch = diff.patch.unwrap();
    assert!(
        patch.contains("new file mode 100644") && patch.contains("+first\n"),
        "{patch}"
    );
}

#[test]
fn untracked_files_additions_and_deletions_open_their_own_sources() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "changes-sides",
        &[
            (b"d.txt", b"deleted on disk\n"),
            (b"e.txt", b"deleted in the index\n"),
        ],
    );
    write(repo.join("u.txt"), b"untracked\n");
    std::fs::remove_file(repo.join("d.txt")).unwrap();
    git(&repo, &["rm", "-q", "e.txt"]);
    let project = fixture.project("p", &repo);
    let (_, changes, _) = fixture.changes(&project, None).unwrap();
    assert_eq!(
        summary(&changes),
        ["staged e.txt -", "unstaged d.txt -", "untracked - u.txt"]
    );

    let untracked = fixture
        .change(
            &project,
            None,
            ChangeGroup::Untracked,
            SideRef::Absent,
            present("u.txt"),
        )
        .unwrap();
    assert_eq!(
        untracked.patch, None,
        "no index comparison is made up for it"
    );
    assert!(matches!(untracked.old, SideRead::Absent));
    assert_eq!(read_body(&untracked.new).unwrap(), b"untracked\n");
    assert!(matches!(
        read_source(&untracked.new),
        ContentSource::Live { .. }
    ));

    for (group, path, text) in [
        (ChangeGroup::Unstaged, "d.txt", "-deleted on disk\n"),
        (ChangeGroup::Staged, "e.txt", "-deleted in the index\n"),
    ] {
        let diff = fixture
            .change(&project, None, group, present(path), SideRef::Absent)
            .unwrap();
        assert!(matches!(diff.new, SideRead::Absent), "{group:?}");
        let patch = diff.patch.unwrap();
        assert!(
            patch.contains("deleted file mode") && patch.contains(text),
            "{patch}"
        );
    }
}

#[test]
fn renames_type_changes_and_binary_changes_keep_their_sides() {
    let fixture = Fixture::new();
    let png = |marker: u8| [&b"\x89PNG\r\n\x1a\n\x00\x00"[..], &[marker; 64]].concat();
    let lines: String = (0..20).map(|n| format!("line {n}\n")).collect();
    let repo = repo_with(
        "changes-kinds",
        &[
            (b"r.txt", lines.as_bytes()),
            (b"t.txt", b"a file\n"),
            (b"i.png", &png(1)),
        ],
    );
    git(&repo, &["mv", "r.txt", "r2.txt"]);
    write(repo.join("r2.txt"), format!("{lines}line 20\n").as_bytes());
    git(&repo, &["add", "r2.txt"]);
    std::fs::remove_file(repo.join("t.txt")).unwrap();
    std::os::unix::fs::symlink("r2.txt", repo.join("t.txt")).unwrap();
    git(&repo, &["add", "t.txt"]);
    write(repo.join("i.png"), &png(2));
    let project = fixture.project("p", &repo);
    let (_, changes, _) = fixture.changes(&project, None).unwrap();
    assert_eq!(
        summary(&changes),
        [
            "staged r.txt r2.txt",
            "staged t.txt t.txt",
            "unstaged i.png i.png"
        ]
    );

    let rename = fixture
        .change(
            &project,
            None,
            ChangeGroup::Staged,
            present("r.txt"),
            present("r2.txt"),
        )
        .unwrap();
    let patch = rename.patch.unwrap();
    assert!(
        patch.contains("rename from r.txt\nrename to r2.txt\n") && patch.contains("+line 20\n"),
        "{patch}"
    );

    let (old, new) = entry(&changes, ChangeGroup::Staged, "t.txt");
    assert!(matches!(
        old,
        ChangeSide::Present {
            kind: SideKind::File,
            ..
        }
    ));
    assert!(matches!(
        new,
        ChangeSide::Present {
            kind: SideKind::Symlink,
            ..
        }
    ));
    let type_change = fixture
        .change(
            &project,
            None,
            ChangeGroup::Staged,
            present("t.txt"),
            present("t.txt"),
        )
        .unwrap();
    assert_eq!(
        (read_kind(&type_change.old), read_kind(&type_change.new)),
        (SideKind::File, SideKind::Symlink)
    );
    let patch = type_change.patch.unwrap();
    // `git` writes a type change as a deletion and an addition of the same path.
    assert_eq!(
        patch.matches("diff --git a/t.txt b/t.txt\n").count(),
        2,
        "{patch}"
    );
    assert!(
        patch.contains("+r2.txt\n"),
        "the link's target is in the patch: {patch}"
    );

    let image = fixture
        .change(
            &project,
            None,
            ChangeGroup::Unstaged,
            present("i.png"),
            present("i.png"),
        )
        .unwrap();
    assert!(image
        .patch
        .unwrap()
        .contains("Binary files a/i.png and b/i.png differ"));
    assert_eq!(read_body(&image.old).unwrap(), png(1), "the index's body");
    assert_eq!(read_body(&image.new).unwrap(), png(2), "the disk's body");
    let SideRead::Present {
        file: Some(file), ..
    } = &image.new
    else {
        panic!("no body");
    };
    assert_eq!(file.media_type, Some("image/png"));
}

#[test]
fn a_rename_across_the_project_boundary_never_returns_the_outside_content() {
    let fixture = Fixture::new();
    // Two unrelated bodies, so each rename pairs only with its own source.
    let coming_lines: String = (0..12).map(|n| format!("coming line {n}\n")).collect();
    let going_lines: String = (0..12).map(|n| format!("going {n}\n")).collect();
    let repo = repo_with(
        "changes-boundary",
        &[
            (
                b"other/secret.txt",
                format!("{coming_lines}OUTSIDE ONLY\n").as_bytes(),
            ),
            (
                b"sub/leaving.txt",
                format!("{going_lines}inside before\n").as_bytes(),
            ),
        ],
    );
    git(&repo, &["mv", "other/secret.txt", "sub/in.txt"]);
    write(
        repo.join("sub/in.txt"),
        format!("{coming_lines}inside now\n").as_bytes(),
    );
    git(&repo, &["mv", "sub/leaving.txt", "other/left.txt"]);
    write(
        repo.join("other/left.txt"),
        format!("{going_lines}OUTSIDE NOW\n").as_bytes(),
    );
    git(&repo, &["add", "-A"]);
    let project = fixture.project("sub", &repo.join("sub"));
    let (_, changes, _) = fixture.changes(&project, None).unwrap();
    assert_eq!(
        summary(&changes),
        [
            "staged leaving.txt other/left.txt",
            "staged other/secret.txt in.txt"
        ]
    );
    let (old, _) = entry(&changes, ChangeGroup::Staged, "in.txt");
    assert_eq!(
        old,
        &ChangeSide::OutOfScope {
            repository_path: "other/secret.txt".to_string()
        }
    );

    let coming = fixture
        .change(
            &project,
            None,
            ChangeGroup::Staged,
            SideRef::OutOfScope,
            present("in.txt"),
        )
        .unwrap();
    assert_eq!(
        coming.patch, None,
        "a patch would carry the outside side's lines"
    );
    assert!(matches!(coming.old, SideRead::OutOfScope));
    assert_eq!(
        read_body(&coming.new).unwrap(),
        format!("{coming_lines}inside now\n").as_bytes()
    );
    let going = fixture
        .change(
            &project,
            None,
            ChangeGroup::Staged,
            present("leaving.txt"),
            SideRef::OutOfScope,
        )
        .unwrap();
    assert_eq!(
        read_body(&going.old).unwrap(),
        format!("{going_lines}inside before\n").as_bytes()
    );
    for diff in [&coming, &going] {
        assert!(
            !diff.json.contains("OUTSIDE") && !diff.json.contains("other/"),
            "{}",
            diff.json
        );
    }
    assert_eq!(
        code(fixture.change(
            &project,
            None,
            ChangeGroup::Unstaged,
            SideRef::OutOfScope,
            present("in.txt")
        )),
        error_code::INVALID_CHANGE
    );
}

#[test]
fn a_deleted_scope_directory_in_another_worktree_still_lists_and_reads_its_deletions() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "changes-gone-dir",
        &[
            (b"sub/a.txt", b"a\n"),
            (b"sub/deep/b.txt", b"b\n"),
            (b"top.txt", b"top\n"),
        ],
    );
    let parent = ScratchDir::new("changes-gone-dir-linked");
    let on_disk = add_worktree(&repo, &parent, "disk", &["--detach"]);
    let in_index = add_worktree(&repo, &parent, "index", &["--detach"]);
    std::fs::remove_dir_all(on_disk.join("sub")).unwrap();
    git(&in_index, &["rm", "-q", "-r", "sub"]);
    let project = fixture.project("sub", &repo.join("sub"));

    let disk_id = worktree_id(&fixture, &project, &on_disk);
    let (_, changes, _) = fixture.changes(&project, Some(&disk_id)).unwrap();
    assert_eq!(
        summary(&changes),
        ["unstaged a.txt -", "unstaged deep/b.txt -"]
    );
    let diff = fixture
        .change(
            &project,
            Some(&disk_id),
            ChangeGroup::Unstaged,
            present("deep/b.txt"),
            SideRef::Absent,
        )
        .unwrap();
    assert!(diff.patch.unwrap().contains("-b\n"));
    assert!(matches!(
        read_source(&diff.old),
        ContentSource::Index { .. }
    ));

    let index_id = worktree_id(&fixture, &project, &in_index);
    let (_, changes, _) = fixture.changes(&project, Some(&index_id)).unwrap();
    assert_eq!(summary(&changes), ["staged a.txt -", "staged deep/b.txt -"]);
    let diff = fixture
        .change(
            &project,
            Some(&index_id),
            ChangeGroup::Staged,
            present("a.txt"),
            SideRef::Absent,
        )
        .unwrap();
    assert!(diff.patch.unwrap().contains("-a\n"));
    assert!(matches!(
        read_source(&diff.old),
        ContentSource::Commit { .. }
    ));
}

#[test]
fn conflicts_are_listed_as_conflicts_and_never_read_as_two_sided_changes() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "changes-conflict",
        &[(b"f.txt", b"base\n"), (b"del.txt", b"x\n")],
    );
    git(&repo, &["checkout", "-q", "-b", "other"]);
    write(repo.join("f.txt"), b"theirs\n");
    write(repo.join("del.txt"), b"changed\n");
    git(&repo, &["commit", "-q", "-am", "theirs"]);
    git(&repo, &["checkout", "-q", "main"]);
    write(repo.join("f.txt"), b"ours\n");
    git(&repo, &["rm", "-q", "del.txt"]);
    git(&repo, &["commit", "-q", "-am", "ours"]);
    let merge = fixture_git(&repo)
        .args(["merge", "-q", "other"])
        .output()
        .unwrap();
    assert!(!merge.status.success(), "the merge conflicts");
    let project = fixture.project("p", &repo);
    let (_, changes, _) = fixture.changes(&project, None).unwrap();
    assert_eq!(
        summary(&changes),
        [
            "conflicted del.txt DeletedByUs",
            "conflicted f.txt BothModified"
        ]
    );
    assert!(changes.contains(&ChangeEntry::Conflicted {
        path: "f.txt".to_string(),
        conflict: ConflictKind::BothModified
    }));
    assert_eq!(
        code(fixture.change(
            &project,
            None,
            ChangeGroup::Unstaged,
            present("f.txt"),
            present("f.txt")
        )),
        error_code::UNSUPPORTED_FILE_TYPE
    );
}

#[test]
fn a_removed_worktree_is_unavailable_rather_than_read_from_another_checkout() {
    let fixture = Fixture::new();
    let repo = repo_with("changes-removed", &[(b"a.txt", b"a\n")]);
    let parent = ScratchDir::new("changes-removed-linked");
    let linked = add_worktree(&repo, &parent, "wt", &["-b", "wt"]);
    write(linked.join("a.txt"), b"linked\n");
    write(repo.join("a.txt"), b"main\n");
    let project = fixture.project("p", &repo);
    let id = worktree_id(&fixture, &project, &linked);
    assert_eq!(
        summary(&fixture.changes(&project, Some(&id)).unwrap().1),
        ["unstaged a.txt a.txt"]
    );
    git(
        &repo,
        &["worktree", "remove", "--force", linked.to_str().unwrap()],
    );
    assert_eq!(
        code(fixture.changes(&project, Some(&id))),
        error_code::WORKTREE_UNAVAILABLE
    );
    assert_eq!(
        code(fixture.change(
            &project,
            Some(&id),
            ChangeGroup::Unstaged,
            present("a.txt"),
            present("a.txt")
        )),
        error_code::WORKTREE_UNAVAILABLE
    );
}

/// The two abbreviated object ids on a patch's `index` line.
fn patch_ids(patch: &str) -> (String, String) {
    let line = patch
        .lines()
        .find(|l| l.starts_with("index "))
        .expect("an index line");
    let ids = line[6..].split(' ').next().unwrap();
    let (old, new) = ids.split_once("..").unwrap();
    (old.to_string(), new.to_string())
}

fn blob_of(side: &SideRead) -> String {
    match read_source(side) {
        ContentSource::Index { blob, .. } | ContentSource::Commit { blob, .. } => blob.clone(),
        other => panic!("no blob: {other:?}"),
    }
}

#[test]
fn staging_committing_or_overwriting_after_a_listing_reads_one_newer_version_whole() {
    let fixture = Fixture::new();
    let repo = repo_with("changes-moved", &[(b"a.txt", b"one\n")]);
    write(repo.join("a.txt"), b"two\n");
    git(&repo, &["add", "a.txt"]);
    let project = fixture.project("p", &repo);
    let (head, changes, _) = fixture.changes(&project, None).unwrap();
    let (_, listed_new) = entry(&changes, ChangeGroup::Staged, "a.txt");
    let staged = |fixture: &Fixture| {
        fixture
            .change(
                &project,
                None,
                ChangeGroup::Staged,
                present("a.txt"),
                present("a.txt"),
            )
            .unwrap()
    };

    // Staged again after the listing: the reply names the new index blob, and its patch is that
    // blob's.
    write(repo.join("a.txt"), b"three\n");
    git(&repo, &["add", "a.txt"]);
    let diff = staged(&fixture);
    assert_ne!(read_source(&diff.new), source_of(listed_new));
    let patch = diff.patch.unwrap();
    let (old_id, new_id) = patch_ids(&patch);
    assert!(
        blob_of(&diff.old).starts_with(&old_id) && blob_of(&diff.new).starts_with(&new_id),
        "{patch}"
    );
    assert!(patch.contains("+three\n"));

    // Committed after the listing: read against the new `HEAD`, there is nothing left to show.
    git(&repo, &["commit", "-q", "-m", "three"]);
    let diff = staged(&fixture);
    assert_ne!(diff.head, head);
    assert_eq!(diff.head.as_deref(), Some(head_of(&repo).as_str()));
    assert_eq!(diff.patch.as_deref(), Some(""));
    assert_eq!(blob_of(&diff.old), blob_of(&diff.new));

    // Overwritten on disk after the listing: the disk side's version moves with the patch.
    write(repo.join("a.txt"), b"four\n");
    let (_, changes, _) = fixture.changes(&project, None).unwrap();
    let (_, listed_live) = entry(&changes, ChangeGroup::Unstaged, "a.txt");
    write(repo.join("a.txt"), b"five, longer\n");
    let diff = fixture
        .change(
            &project,
            None,
            ChangeGroup::Unstaged,
            present("a.txt"),
            present("a.txt"),
        )
        .unwrap();
    assert_ne!(read_source(&diff.new), source_of(listed_live));
    assert!(diff.patch.unwrap().contains("+five, longer\n"));
}

#[test]
fn a_source_changed_while_the_patch_is_made_is_refused_rather_than_mixed() {
    let fixture = Fixture::new();
    let repo = repo_with("changes-race", &[(b"a.txt", b"one\n")]);
    write(repo.join("a.txt"), b"two\n");
    git(&repo, &["add", "a.txt"]);
    write(repo.join("a.txt"), b"three\n");
    let project = fixture.project("p", &repo);
    let read = |group| fixture.change(&project, None, group, present("a.txt"), present("a.txt"));

    for (group, change) in [
        (ChangeGroup::Unstaged, "overwrite"),
        (ChangeGroup::Staged, "stage"),
    ] {
        let dir = repo.to_path_buf();
        AFTER_PATCH.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                write(
                    dir.join("a.txt"),
                    format!("{change}d during the read\n").as_bytes(),
                );
                if change == "stage" {
                    git(&dir, &["add", "a.txt"]);
                }
            }))
        });
        let result = read(group);
        AFTER_PATCH.with(|hook| hook.borrow_mut().take());
        assert_eq!(code(result), error_code::SOURCE_CHANGED, "{group:?}");
        // Read again with nothing moving, it is consistent.
        assert!(read(group).is_ok());
    }
}

#[test]
fn a_change_list_is_cut_at_its_entry_budget_and_a_patch_past_its_budget_is_refused() {
    let fixture = Fixture::new();
    let repo = repo_with("changes-budgets", &[(b"big.txt", b"small\n")]);
    for n in 0..=budget::MAX_CHANGE_ENTRIES {
        std::fs::write(repo.join(format!("u{n:05}.txt")), b"").unwrap();
    }
    let project = fixture.project("p", &repo);
    let event = fixture
        .serve(BrowseBody::ListProjectChanges {
            project: project.clone(),
            worktree: None,
        })
        .unwrap();
    let serialized = serde_json::to_string(&event).unwrap().len();
    let Event::ProjectChanges {
        changes, complete, ..
    } = event
    else {
        panic!("not a change list");
    };
    assert_eq!(
        (changes.len(), complete),
        (budget::MAX_CHANGE_ENTRIES, false)
    );
    assert!(serialized < budget::CHANGE_LIST_RESERVATION - budget::CHANGE_STATUS_STDOUT);

    let line = "x".repeat(99) + "\n";
    write(
        repo.join("big.txt"),
        line.repeat(budget::MAX_PATCH_BYTES / 100 + 1).as_bytes(),
    );
    let refused = fixture.change(
        &project,
        None,
        ChangeGroup::Unstaged,
        present("big.txt"),
        present("big.txt"),
    );
    let err = refused.expect_err("a refusal");
    let coded = err.downcast_ref::<CodedError>().unwrap();
    assert_eq!(coded.code, error_code::LIMIT_EXCEEDED);
    assert_eq!(
        coded.params.get("limit").map(String::as_str),
        Some("patch_bytes")
    );
}

#[test]
fn a_submodule_change_and_a_nested_repository_stay_listed_with_their_kind() {
    let fixture = Fixture::new();
    let repo = repo_with("changes-submodule", &[(b"a.txt", b"a\n")]);
    let inner = repo.join("lib");
    std::fs::create_dir(&inner).unwrap();
    git(&inner, &["init", "-q"]);
    write(inner.join("x.txt"), b"x\n");
    git(&inner, &["add", "x.txt"]);
    git(&inner, &["commit", "-q", "-m", "one"]);
    // `git add` of a repository inside the worktree records it as a submodule (a gitlink).
    git(&repo, &["add", "lib"]);
    git(&repo, &["commit", "-q", "-m", "with lib"]);
    write(inner.join("x.txt"), b"x two\n");
    git(&inner, &["commit", "-q", "-am", "two"]);
    let nested = repo.join("vendor/tool");
    std::fs::create_dir_all(&nested).unwrap();
    git(&nested, &["init", "-q"]);
    let project = fixture.project("p", &repo);
    let (_, changes, _) = fixture.changes(&project, None).unwrap();
    assert_eq!(
        summary(&changes),
        ["unstaged lib lib", "untracked - vendor/tool"]
    );
    let (_, new) = entry(&changes, ChangeGroup::Unstaged, "lib");
    assert!(matches!(
        new,
        ChangeSide::Present {
            kind: SideKind::Submodule,
            ..
        }
    ));
    let (_, nested) = entry(&changes, ChangeGroup::Untracked, "vendor/tool");
    assert!(matches!(
        nested,
        ChangeSide::Present {
            kind: SideKind::Submodule,
            ..
        }
    ));

    let diff = fixture
        .change(
            &project,
            None,
            ChangeGroup::Unstaged,
            present("lib"),
            present("lib"),
        )
        .unwrap();
    assert_eq!(
        (read_kind(&diff.old), read_kind(&diff.new)),
        (SideKind::Submodule, SideKind::Submodule)
    );
    let patch = diff.patch.unwrap();
    assert_eq!(patch.matches("Subproject commit").count(), 2, "{patch}");
    let alone = fixture
        .change(
            &project,
            None,
            ChangeGroup::Untracked,
            SideRef::Absent,
            present("vendor/tool"),
        )
        .unwrap();
    assert_eq!(read_kind(&alone.new), SideKind::Submodule);
    assert_eq!(
        read_body(&alone.new),
        None,
        "a repository has no body to read"
    );
}

/// Every file under `dir`, with its length, modification time and bytes.
pub(super) fn snapshot(dir: &Path) -> Vec<(std::path::PathBuf, Vec<u8>, std::time::SystemTime)> {
    let mut all = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let metadata = std::fs::symlink_metadata(&path).unwrap();
            if metadata.is_dir() {
                stack.push(path);
            } else {
                let bytes = std::fs::read(&path).unwrap_or_default();
                all.push((path, bytes, metadata.modified().unwrap()));
            }
        }
    }
    all.sort();
    all
}

#[test]
fn no_browse_request_writes_to_the_repository() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "changes-read-only",
        &[
            (b"same.txt", b"same\n"),
            (b"b.txt", b"b\n"),
            (b"c.txt", b"c\n"),
        ],
    );
    let parent = ScratchDir::new("changes-read-only-linked");
    let linked = add_worktree(&repo, &parent, "wt", &["-b", "wt"]);
    write(linked.join("d.txt"), b"committed on wt\n");
    git(&linked, &["add", "d.txt"]);
    git(&linked, &["commit", "-q", "-m", "wt"]);
    write(linked.join("b.txt"), b"linked\n");
    write(repo.join("c.txt"), b"staged\n");
    git(&repo, &["add", "c.txt"]);
    write(repo.join("b.txt"), b"changed\n");
    write(repo.join("new.txt"), b"untracked\n");
    // The same bytes written again: only the timestamps differ from what the index recorded, which
    // is what makes `git diff` want to refresh the index's stat information.
    let file = std::fs::File::options()
        .write(true)
        .open(repo.join("same.txt"))
        .unwrap();
    file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5))
        .unwrap();
    drop(file);
    let project = fixture.project("p", &repo);
    let linked_id = worktree_id(&fixture, &project, &linked);
    let before = snapshot(&repo.join(".git"));

    fixture.source(&project);
    fixture.list(&project, None, "").unwrap();
    fixture.list(&project, Some(&linked_id), "").unwrap();
    for from in [
        ReadFrom::Live,
        ReadFrom::Index,
        ReadFrom::Branch {
            branch: "main".to_string(),
        },
        ReadFrom::Commit {
            commit: head_of(&repo),
        },
    ] {
        fixture.read(&project, None, "b.txt", from).unwrap();
    }
    fixture.changes(&project, None).unwrap();
    fixture.changes(&project, Some(&linked_id)).unwrap();
    for (group, path, worktree) in [
        (ChangeGroup::Unstaged, "same.txt", None),
        (ChangeGroup::Unstaged, "b.txt", None),
        (ChangeGroup::Unstaged, "b.txt", Some(linked_id.as_str())),
        (ChangeGroup::Staged, "c.txt", None),
    ] {
        fixture
            .change(&project, worktree, group, present(path), present(path))
            .unwrap();
    }
    for (group, path) in [
        (ChangeGroup::Unstaged, "same.txt"),
        (ChangeGroup::Unstaged, "b.txt"),
        (ChangeGroup::Staged, "c.txt"),
    ] {
        fixture.bodies_of_diff(&project, group, path);
    }
    fixture
        .change(
            &project,
            None,
            ChangeGroup::Untracked,
            SideRef::Absent,
            present("new.txt"),
        )
        .unwrap();
    fixture.branches(&project).unwrap();
    for (left, right) in [("main", "wt"), ("wt", "main")] {
        let compared = fixture.compare(&project, left, right).unwrap();
        for (old, new) in [
            (SideRef::Absent, present("d.txt")),
            (present("d.txt"), SideRef::Absent),
        ] {
            fixture
                .compared_change(&project, &compared, old, new)
                .unwrap();
        }
    }
    let compared = fixture.compare(&project, "main", "wt").unwrap();
    let diff = fixture
        .compared_change(&project, &compared, SideRef::Absent, present("d.txt"))
        .unwrap();
    let versions = (version(&diff.old), version(&diff.new));
    fixture
        .compared_bodies(&project, &compared, "d.txt", versions)
        .unwrap();

    let after = snapshot(&repo.join(".git"));
    let changed: Vec<_> = before
        .iter()
        .zip(&after)
        .filter(|(b, a)| b != a)
        .map(|(b, _)| b.0.clone())
        .collect();
    assert!(
        before.len() == after.len() && changed.is_empty(),
        "written: {changed:?}"
    );

    // Before the first commit a staged change is compared with the empty tree, whose id is hashed,
    // never stored.
    let unborn = ScratchDir::new("changes-read-only-unborn");
    git(&unborn, &["init", "-q"]);
    write(unborn.join("a.txt"), b"first\n");
    git(&unborn, &["add", "a.txt"]);
    let project = fixture.project("unborn", &unborn);
    let before = snapshot(&unborn.join(".git"));
    fixture.changes(&project, None).unwrap();
    fixture
        .change(
            &project,
            None,
            ChangeGroup::Staged,
            SideRef::Absent,
            present("a.txt"),
        )
        .unwrap();
    assert!(
        before == snapshot(&unborn.join(".git")),
        "the unborn repository was written"
    );
}

#[test]
fn a_change_reads_only_its_own_paths_whatever_lies_across_the_boundary_or_below() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "changes-own-paths",
        &[
            (b"sub/a.txt", b"leaving with intent\n"),
            (b"other/x.txt", b"coming in\n"),
            (b"sub/f", b"a file becoming a directory\n"),
        ],
    );
    // Renames on disk across the boundary, their new halves added with intent to add.
    std::fs::rename(repo.join("sub/a.txt"), repo.join("other/b.txt")).unwrap();
    git(&repo, &["add", "-N", "other/b.txt"]);
    std::fs::rename(repo.join("other/x.txt"), repo.join("sub/y.txt")).unwrap();
    git(&repo, &["add", "-N", "sub/y.txt"]);
    // A file replaced by a directory, staged.
    git(&repo, &["rm", "-q", "sub/f"]);
    write(repo.join("sub/f/inner.txt"), b"INNER\n");
    git(&repo, &["add", "sub/f/inner.txt"]);
    let project = fixture.project("sub", &repo.join("sub"));
    let (_, changes, _) = fixture.changes(&project, None).unwrap();
    assert_eq!(
        summary(&changes),
        [
            "staged - f/inner.txt",
            "staged f -",
            "unstaged - y.txt",
            "unstaged a.txt -"
        ]
    );

    let leaving = fixture
        .change(
            &project,
            None,
            ChangeGroup::Unstaged,
            present("a.txt"),
            SideRef::Absent,
        )
        .unwrap();
    assert!(matches!(leaving.new, SideRead::Absent));
    assert!(leaving
        .patch
        .as_deref()
        .unwrap()
        .contains("-leaving with intent\n"));
    let coming = fixture
        .change(
            &project,
            None,
            ChangeGroup::Unstaged,
            SideRef::Absent,
            present("y.txt"),
        )
        .unwrap();
    assert!(
        matches!(coming.old, SideRead::Absent),
        "an intent-to-add entry holds nothing yet"
    );
    assert!(coming.patch.as_deref().unwrap().contains("new file mode"));
    for diff in [&leaving, &coming] {
        assert!(
            !diff.json.contains("other/") && !diff.json.contains("OUTSIDE"),
            "{}",
            diff.json
        );
    }

    let replaced = fixture
        .change(
            &project,
            None,
            ChangeGroup::Staged,
            present("f"),
            SideRef::Absent,
        )
        .unwrap();
    assert!(matches!(replaced.new, SideRead::Absent));
    let patch = replaced.patch.unwrap();
    assert_eq!(patch.matches("diff --git").count(), 1, "{patch}");
    assert!(!patch.contains("INNER"), "{patch}");
    // A path that is a directory on both sides is no file of any change: nothing is compared.
    let directory = fixture
        .change(
            &project,
            None,
            ChangeGroup::Unstaged,
            present("f"),
            present("f"),
        )
        .unwrap();
    assert!(matches!(
        (&directory.old, &directory.new),
        (SideRead::Absent, SideRead::Absent)
    ));
    assert_eq!(directory.patch.as_deref(), Some(""));
}

#[test]
fn a_head_that_names_no_commit_is_a_failure_not_an_empty_baseline() {
    let fixture = Fixture::new();
    let repo = repo_with("changes-broken-head", &[(b"a.txt", b"a\n")]);
    write(repo.join("a.txt"), b"staged\n");
    git(&repo, &["add", "a.txt"]);
    let project = fixture.project("p", &repo);
    let staged = |fixture: &Fixture| {
        fixture.change(
            &project,
            None,
            ChangeGroup::Staged,
            present("a.txt"),
            present("a.txt"),
        )
    };
    // A branch that is there but names a missing object, then a `HEAD` detached at one. Before
    // Git 2.43 the first cannot be told from a branch not born yet, so it is left out there.
    let missing = format!("{}\n", "1".repeat(40));
    if git_has_show_ref_exists(&repo) {
        std::fs::write(repo.join(".git/refs/heads/main"), &missing).unwrap();
        assert_eq!(code(staged(&fixture)), error_code::GIT_FAILED);
    }
    std::fs::write(repo.join(".git/HEAD"), &missing).unwrap();
    assert_eq!(code(staged(&fixture)), error_code::GIT_FAILED);
}

#[test]
fn an_unstaged_type_change_and_a_submodule_replaced_by_a_directory_keep_their_sides() {
    let fixture = Fixture::new();
    let repo = repo_with("changes-kinds-again", &[(b"t.txt", b"a file\n")]);
    let inner = repo.join("lib");
    std::fs::create_dir(&inner).unwrap();
    git(&inner, &["init", "-q"]);
    write(inner.join("x.txt"), b"x\n");
    git(&inner, &["add", "x.txt"]);
    git(&inner, &["commit", "-q", "-m", "one"]);
    git(&repo, &["add", "lib"]);
    git(&repo, &["commit", "-q", "-m", "with lib"]);
    // On disk only: the file becomes a link.
    std::fs::remove_file(repo.join("t.txt")).unwrap();
    std::os::unix::fs::symlink("elsewhere", repo.join("t.txt")).unwrap();
    // Staged: the submodule gives way to a directory of files.
    git(&repo, &["rm", "-q", "--cached", "lib"]);
    std::fs::remove_dir_all(&inner).unwrap();
    write(repo.join("lib/inner.txt"), b"INNER\n");
    git(&repo, &["add", "lib/inner.txt"]);
    let project = fixture.project("p", &repo);

    let type_change = fixture
        .change(
            &project,
            None,
            ChangeGroup::Unstaged,
            present("t.txt"),
            present("t.txt"),
        )
        .unwrap();
    assert_eq!(
        (read_kind(&type_change.old), read_kind(&type_change.new)),
        (SideKind::File, SideKind::Symlink),
        "a type change is no addition"
    );
    let replaced = fixture
        .change(
            &project,
            None,
            ChangeGroup::Staged,
            present("lib"),
            SideRef::Absent,
        )
        .unwrap();
    assert_eq!(read_kind(&replaced.old), SideKind::Submodule);
    assert!(matches!(replaced.new, SideRead::Absent));
    let patch = replaced.patch.unwrap();
    assert_eq!(patch.matches("diff --git").count(), 1, "{patch}");
    assert!(
        patch.contains("Subproject commit") && !patch.contains("INNER"),
        "{patch}"
    );
}

#[test]
fn a_rename_into_or_out_of_a_path_below_itself_reads_its_own_two_paths_alone() {
    let fixture = Fixture::new();
    let lines = |tag: &str, changed: &[usize]| -> String {
        (0..12)
            .map(|n| match changed.contains(&n) {
                true => format!("{tag} changed {n}\n"),
                false => format!("{tag} line {n}\n"),
            })
            .collect()
    };
    // One of them needs quoting in a patch header.
    let names: [&str; 2] = ["foo", "q\t\"x"];
    let mut files: Vec<(String, String)> = Vec::new();
    for name in names {
        files.push((name.to_string(), lines(name, &[])));
        // Nearer to `name` than `name/bar` will be, and moved below it whole: `git` would pair
        // `name` with it if it detected renames inside the change's paths.
        files.push((format!("elsewhere-{name}"), lines(name, &[0])));
        files.push((format!("up-{name}/bar"), lines(&format!("up {name}"), &[])));
        // A neighbour below the path that moves up, deleted with it.
        files.push((format!("up-{name}/other"), lines("OTHER", &[])));
    }
    let files: Vec<(&[u8], &[u8])> = files
        .iter()
        .map(|(n, b)| (n.as_bytes(), b.as_bytes()))
        .collect();
    let repo = repo_with("changes-nested", &files);
    for name in names {
        // `name` moves down into a directory of its own name, changed a little, beside a
        // neighbour and the file moved in from elsewhere.
        git(&repo, &["mv", name, "tmp"]);
        std::fs::create_dir(repo.join(name)).unwrap();
        git(&repo, &["mv", "tmp", &format!("{name}/bar")]);
        write(
            repo.join(format!("{name}/bar")),
            lines(name, &[3, 7, 11]).as_bytes(),
        );
        git(
            &repo,
            &["mv", &format!("elsewhere-{name}"), &format!("{name}/moved")],
        );
        write(repo.join(format!("{name}/neighbour.txt")), b"NEIGHBOUR\n");
        git(&repo, &["add", "-A"]);
        // `up-name/bar` moves up to take its directory's place; `up-name/other` goes.
        let up = format!("up-{name}");
        git(&repo, &["mv", &format!("{up}/bar"), "tmp"]);
        git(&repo, &["rm", "-q", &format!("{up}/other")]);
        git(&repo, &["mv", "tmp", &up]);
    }
    // A path in conflict below the first name, right after its new path, whose `* Unmerged path`
    // note must stay out.
    let conflicted = repo.join("conflicted-blob");
    write(&conflicted, b"UNMERGED\n");
    let oid = String::from_utf8(git(
        &repo,
        &["hash-object", "-w", conflicted.to_str().unwrap()],
    ))
    .unwrap();
    std::fs::remove_file(&conflicted).unwrap();
    let stages: String = (1..=3)
        .map(|stage| format!("100644 {} {stage}\tfoo/baz\n", oid.trim()))
        .collect();
    git_stdin(&repo, &["update-index", "--index-info"], stages.as_bytes());
    let project = fixture.project("p", &repo);
    let (_, changes, _) = fixture.changes(&project, None).unwrap();
    for name in names {
        let wire = |path: String| wire_path::encode(path.as_bytes());
        let (down_from, down_to) = (wire(name.to_string()), wire(format!("{name}/bar")));
        let (up_from, up_to) = (wire(format!("up-{name}/bar")), wire(format!("up-{name}")));
        for (from, to) in [(&down_from, &down_to), (&up_from, &up_to)] {
            // Listed as the rename it is, and read with both of its sides.
            let (old, _) = entry(&changes, ChangeGroup::Staged, to);
            assert_eq!(side_path(old), Some(from.as_str()), "{changes:#?}");
            let diff = fixture
                .change(
                    &project,
                    None,
                    ChangeGroup::Staged,
                    present(from),
                    present(to),
                )
                .unwrap();
            assert!(
                matches!(
                    (&diff.old, &diff.new),
                    (SideRead::Present { .. }, SideRead::Present { .. })
                ),
                "{from} -> {to}: {diff:?}"
            );
            let patch = diff.patch.unwrap();
            // Its old path's removal and its new path's addition, nothing paired, nothing else.
            assert_eq!(patch.matches("diff --git").count(), 2, "{patch}");
            assert!(
                patch.contains("deleted file mode") && patch.contains("new file mode"),
                "{patch}"
            );
            for stranger in ["NEIGHBOUR", "OTHER", "Unmerged", "moved", "rename from"] {
                assert!(!patch.contains(stranger), "{stranger} in {patch}");
            }
        }
    }
}
