//! Commit lists listed and read end to end against real repositories: which commits a branch made
//! since it forked, what each of them changed against its first parent, what one of those changes
//! reads, and where a list is cut.

use std::path::Path;

use super::body_tests::version;
use super::change_tests::{present, read_body, read_source, summary, write};
use super::compare_tests::rev_parse;
use super::tests::{code, Fixture};
use super::*;
use crate::protocol::{
    ChangeEntry, ChangeSide, CommitChanges, ComparedChangeRef, SideRead, SideRef, SideVersion,
};
use crate::test_support::{fixture_git, git, git_stdin, repo_with};

/// A commit list as `list_project_commits` answered it.
#[derive(Debug)]
pub(super) struct Listed {
    pub(super) commits: Vec<CommitChanges>,
    pub(super) complete: bool,
    pub(super) commits_complete: bool,
}

/// One change of a commit as `read_project_commit_change` answered it.
#[derive(Debug)]
pub(super) struct CommitRead {
    pub(super) old: SideRead,
    pub(super) new: SideRead,
    patch: Option<String>,
}

impl Fixture {
    pub(super) fn commits(&self, project: &str, base: &str, tip: &str) -> anyhow::Result<Listed> {
        match self.serve(BrowseBody::ListProjectCommits {
            project: project.to_string(),
            base: base.to_string(),
            tip: tip.to_string(),
        })? {
            Event::ProjectCommits {
                base: echoed_base,
                tip: echoed_tip,
                commits,
                complete,
                commits_complete,
                ..
            } => {
                assert_eq!((echoed_base.as_str(), echoed_tip.as_str()), (base, tip));
                Ok(Listed {
                    commits,
                    complete,
                    commits_complete,
                })
            }
            other => panic!("not a commit list: {other:?}"),
        }
    }

    pub(super) fn commit_change(
        &self,
        project: &str,
        commit: &CommitChanges,
        old: SideRef,
        new: SideRef,
    ) -> anyhow::Result<CommitRead> {
        match self.serve(BrowseBody::ReadProjectCommitChange {
            project: project.to_string(),
            commit: commit.commit.clone(),
            parent: commit.parent.clone(),
            change: ComparedChangeRef { old, new },
        })? {
            Event::ProjectCommitChange {
                commit: echoed,
                parent,
                old,
                new,
                patch,
                ..
            } => {
                assert_eq!((&echoed, &parent), (&commit.commit, &commit.parent));
                Ok(CommitRead {
                    old: *old,
                    new: *new,
                    patch: patch.map(|patch| String::from_utf8(tests::bytes_of(&patch)).unwrap()),
                })
            }
            other => panic!("not a commit's change: {other:?}"),
        }
    }

    pub(super) fn commit_bodies(
        &self,
        project: &str,
        commit: &CommitChanges,
        path: &str,
        (old, new): (SideVersion, SideVersion),
    ) -> anyhow::Result<(SideRead, SideRead)> {
        match self.serve(BrowseBody::ReadProjectCommitChangeBodies {
            project: project.to_string(),
            commit: commit.commit.clone(),
            parent: commit.parent.clone(),
            change: ComparedChangeRef {
                old: present(path),
                new: present(path),
            },
            old,
            new,
        })? {
            Event::ProjectCommitChangeBodies { old, new, .. } => Ok((*old, *new)),
            other => panic!("not a commit change's bodies: {other:?}"),
        }
    }
}

/// Commits `files` (a body, or `None` to delete it) on the branch checked out in `repo` with
/// `message`, dated `second` seconds after a fixed moment so that `git log` orders the commits by
/// it rather than by a tie, and returns the commit.
pub(super) fn commit_at(
    repo: &Path,
    message: &str,
    second: u32,
    files: &[(&str, Option<&[u8]>)],
) -> String {
    for (path, body) in files {
        match body {
            Some(body) => write(repo.join(path), body),
            None => std::fs::remove_file(repo.join(path)).unwrap(),
        }
    }
    git(repo, &["add", "-A"]);
    let date = format!("@{} +0000", 1_700_000_000 + u64::from(second));
    let status = fixture_git(repo)
        .args(["commit", "-q", "--allow-empty", "-m", message])
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_DATE", &date)
        .status()
        .unwrap();
    assert!(status.success());
    rev_parse(repo, "HEAD")
}

/// What each listed commit is: its subject and what its changes name.
fn shape(commits: &[CommitChanges]) -> Vec<(String, Vec<String>)> {
    commits
        .iter()
        .map(|commit| (commit.subject.clone(), summary(&commit.changes)))
        .collect()
}

#[test]
fn a_commit_list_is_each_commit_since_the_fork_newest_first_against_its_first_parent() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "commits-listed",
        &[(b"a.txt", b"base\n"), (b"b.txt", b"b\n")],
    );
    let fork = rev_parse(&repo, "main");
    git(&repo, &["checkout", "-q", "-b", "feature"]);
    let first = commit_at(&repo, "first", 10, &[("a.txt", Some(b"one\n"))]);
    git(&repo, &["mv", "b.txt", "c.txt"]);
    let second = commit_at(&repo, "second", 20, &[("a.txt", Some(b"two\n"))]);
    git(&repo, &["checkout", "-q", "main"]);
    let on_main = commit_at(&repo, "on main", 30, &[("m.txt", Some(b"m\n"))]);
    git(&repo, &["checkout", "-q", "feature"]);
    git(
        &repo,
        &["merge", "-q", "--no-ff", "-m", "merge main", "main"],
    );
    let merge = rev_parse(&repo, "HEAD");
    git(&repo, &["checkout", "-q", "main"]);
    let project = fixture.project("p", &repo);

    // Merging main in moved the fork to main's tip: what the branch made since is the merge and
    // its own two commits, the merge's change being what it took in against its first parent.
    let base = rev_parse(&repo, "main");
    assert_eq!(base, on_main);
    let listed = fixture.commits(&project, &base, &merge).unwrap();
    assert_eq!(
        shape(&listed.commits),
        [
            (
                "merge main".to_string(),
                vec!["committed - m.txt".to_string()]
            ),
            (
                "second".to_string(),
                vec![
                    "committed a.txt a.txt".to_string(),
                    "committed b.txt c.txt".to_string()
                ]
            ),
            (
                "first".to_string(),
                vec!["committed a.txt a.txt".to_string()]
            ),
        ],
        "newest first, a file changed by two commits in each"
    );
    let parents: Vec<_> = listed.commits.iter().map(|c| c.parent.clone()).collect();
    assert_eq!(
        parents,
        [Some(second.clone()), Some(first.clone()), Some(fork)]
    );
    assert!(listed.complete && listed.commits_complete);
    let a_txt = |commit: &CommitChanges| match &commit.changes[0] {
        ChangeEntry::Committed { old, new, .. } => (old.clone(), new.clone()),
        other => panic!("not a committed change: {other:?}"),
    };
    let (old, new) = a_txt(&listed.commits[1]);
    let commit_of = |side: &ChangeSide| match side {
        ChangeSide::Present {
            source: ContentSource::Commit { commit, .. },
            ..
        } => commit.clone(),
        other => panic!("not a commit's side: {other:?}"),
    };
    assert_eq!((commit_of(&old), commit_of(&new)), (first, second));

    let none = fixture.commits(&project, &merge, &merge).unwrap();
    assert!(none.commits.is_empty() && none.complete && none.commits_complete);
    for (commit, expected) in [
        (merge[..12].to_string(), error_code::INVALID_COMMIT),
        ("0".repeat(merge.len()), error_code::UNKNOWN_COMMIT),
    ] {
        assert_eq!(code(fixture.commits(&project, &base, &commit)), expected);
        assert_eq!(code(fixture.commits(&project, &commit, &merge)), expected);
    }
}

#[test]
fn a_commit_change_is_read_between_its_parent_and_the_commit_alone() {
    let fixture = Fixture::new();
    let repo = repo_with("commits-read", &[(b"a.txt", b"base\n")]);
    let fork = rev_parse(&repo, "main");
    git(&repo, &["checkout", "-q", "-b", "feature"]);
    commit_at(&repo, "first", 10, &[("a.txt", Some(b"one\n"))]);
    commit_at(&repo, "second", 20, &[("a.txt", Some(b"two\n"))]);
    // An unrelated history merged in brings a commit with no parent along.
    git(&repo, &["checkout", "-q", "--orphan", "other"]);
    git(&repo, &["rm", "-q", "-r", "--cached", "."]);
    std::fs::remove_file(repo.join("a.txt")).unwrap();
    let root = commit_at(&repo, "unrelated", 5, &[("x.txt", Some(b"x\n"))]);
    git(&repo, &["checkout", "-q", "feature"]);
    git(
        &repo,
        &[
            "merge",
            "-q",
            "--allow-unrelated-histories",
            "-m",
            "join",
            "other",
        ],
    );
    let tip = rev_parse(&repo, "HEAD");
    git(&repo, &["checkout", "-q", "main"]);
    // Configured to leave a commit with no parent's changes out of a log, which the listing does
    // not follow.
    git(&repo, &["config", "log.showRoot", "false"]);
    let project = fixture.project("p", &repo);
    let listed = fixture.commits(&project, &fork, &tip).unwrap();
    let subjects: Vec<_> = listed.commits.iter().map(|c| c.subject.as_str()).collect();
    assert_eq!(subjects, ["join", "second", "first", "unrelated"]);

    let second = &listed.commits[1];
    let read = fixture
        .commit_change(&project, second, present("a.txt"), present("a.txt"))
        .unwrap();
    let patch = read.patch.unwrap();
    assert!(
        patch.contains("-one\n+two\n") && !patch.contains("base"),
        "the commit against its parent, not against the fork: {patch}"
    );
    let commit_of = |side: &SideRead| match read_source(side) {
        ContentSource::Commit { commit, .. } => commit.clone(),
        other => panic!("not a commit's side: {other:?}"),
    };
    assert_eq!(
        (commit_of(&read.old), commit_of(&read.new)),
        (second.parent.clone().unwrap(), second.commit.clone())
    );
    let (old, new) = fixture
        .commit_bodies(
            &project,
            second,
            "a.txt",
            (version(&read.old), version(&read.new)),
        )
        .unwrap();
    assert_eq!(
        (read_body(&old).unwrap(), read_body(&new).unwrap()),
        (b"one\n".to_vec(), b"two\n".to_vec())
    );

    let unrelated = &listed.commits[3];
    assert_eq!(
        (unrelated.commit.as_str(), unrelated.parent.as_ref()),
        (root.as_str(), None)
    );
    assert_eq!(summary(&unrelated.changes), ["committed - x.txt"]);
    let added = fixture
        .commit_change(&project, unrelated, SideRef::Absent, present("x.txt"))
        .unwrap();
    assert!(matches!(added.old, SideRead::Absent));
    assert!(added.patch.unwrap().contains("+x\n"));

    let wrong_parent = CommitChanges {
        parent: Some("0".repeat(root.len())),
        ..second.clone()
    };
    assert_eq!(
        code(fixture.commit_change(&project, &wrong_parent, present("a.txt"), present("a.txt"))),
        error_code::UNKNOWN_COMMIT
    );
}

#[test]
fn a_commit_list_is_cut_at_its_commit_and_entry_budgets() {
    let fixture = Fixture::new();
    let repo = repo_with("commits-budgets", &[(b"a.txt", b"a\n")]);
    let fork = rev_parse(&repo, "main");
    // One commit past the budget, in one `git` rather than one per commit.
    let stream: String = (0..=budget::MAX_COMMITS)
        .map(|n| {
            let from = if n == 0 {
                format!("from {fork}\n")
            } else {
                String::new()
            };
            let message = if n == budget::MAX_COMMITS {
                "s".repeat(budget::MAX_COMMIT_SUBJECT_CHARS + 50)
            } else {
                format!("c{n}")
            };
            format!(
                "commit refs/heads/many\ncommitter t <t@example.com> {} +0000\ndata {}\n{message}\n{from}\n",
                1_700_000_000 + n,
                message.len()
            )
        })
        .collect();
    git_stdin(&repo, &["fast-import", "--quiet"], stream.as_bytes());
    let tip = rev_parse(&repo, "many");
    let project = fixture.project("p", &repo);
    let listed = fixture.commits(&project, &fork, &tip).unwrap();
    assert_eq!(
        (
            listed.commits.len(),
            listed.complete,
            listed.commits_complete
        ),
        (budget::MAX_COMMITS, true, false)
    );
    assert_eq!(
        listed.commits[0].subject.chars().count(),
        budget::MAX_COMMIT_SUBJECT_CHARS,
        "the newest commit's long subject, cut"
    );

    git(&repo, &["checkout", "-q", "-b", "wide", "main"]);
    let files: Vec<String> = (0..budget::MAX_CHANGE_ENTRIES)
        .map(|n| format!("u{n:05}.txt"))
        .collect();
    for file in &files {
        std::fs::write(repo.join(file), b"u\n").unwrap();
    }
    commit_at(&repo, "wide", 10, &[]);
    commit_at(&repo, "narrow", 20, &[("a.txt", Some(b"b\n"))]);
    let wide = rev_parse(&repo, "HEAD");
    git(&repo, &["checkout", "-q", "main"]);
    let listed = fixture.commits(&project, &fork, &wide).unwrap();
    let counts: Vec<_> = listed.commits.iter().map(|c| c.changes.len()).collect();
    assert_eq!(
        (counts, listed.complete),
        (vec![1, budget::MAX_CHANGE_ENTRIES - 1], false),
        "the newer commit whole, the older one cut where the entries ran out"
    );
}

#[test]
fn a_commit_the_entry_budget_ran_out_before_is_left_out_not_listed_with_no_changes() {
    let fixture = Fixture::new();
    let repo = repo_with("commits-boundary", &[(b"a.txt", b"a\n")]);
    let fork = rev_parse(&repo, "main");
    git(&repo, &["checkout", "-q", "-b", "wide"]);
    commit_at(&repo, "narrow", 10, &[("a.txt", Some(b"b\n"))]);
    for n in 0..budget::MAX_CHANGE_ENTRIES {
        std::fs::write(repo.join(format!("u{n:05}.txt")), b"u\n").unwrap();
    }
    commit_at(&repo, "wide", 20, &[]);
    let tip = rev_parse(&repo, "HEAD");
    git(&repo, &["checkout", "-q", "main"]);
    let project = fixture.project("p", &repo);

    // The newer commit takes the whole entry budget, so the older one's change does not fit:
    // listing it with none would say it changed nothing.
    let listed = fixture.commits(&project, &fork, &tip).unwrap();
    let shape: Vec<_> = listed
        .commits
        .iter()
        .map(|c| (c.subject.as_str(), c.changes.len()))
        .collect();
    assert_eq!(
        (shape, listed.complete, listed.commits_complete),
        (vec![("wide", budget::MAX_CHANGE_ENTRIES)], false, true)
    );
}

#[test]
fn a_commit_list_is_cut_at_its_path_budget() {
    let fixture = Fixture::new();
    let repo = repo_with("commits-path-budget", &[(b"a.txt", b"a\n")]);
    let fork = rev_parse(&repo, "main");
    git(&repo, &["checkout", "-q", "-b", "long"]);
    // Names so long that the paths reach the budget well before the entries do.
    let count = budget::MAX_CHANGE_ENTRIES * 9 / 10;
    for n in 0..count {
        let name = format!("{n:05}{}.txt", "x".repeat(235));
        std::fs::write(repo.join(name), b"u\n").unwrap();
    }
    commit_at(&repo, "long", 10, &[]);
    let tip = rev_parse(&repo, "HEAD");
    git(&repo, &["checkout", "-q", "main"]);
    let project = fixture.project("p", &repo);

    let listed = fixture.commits(&project, &fork, &tip).unwrap();
    let [commit] = &listed.commits[..] else {
        panic!("not one commit: {:?}", listed.commits.len());
    };
    let bytes: Vec<usize> = commit
        .changes
        .iter()
        .map(super::changes::entry_path_bytes)
        .collect();
    let total: usize = bytes.iter().sum();
    assert!(!listed.complete && commit.changes.len() < count);
    assert!(
        total <= budget::MAX_CHANGE_PATH_BYTES && total + bytes[0] > budget::MAX_CHANGE_PATH_BYTES,
        "cut where the next path no longer fit: {total} bytes of paths"
    );
}

#[test]
fn a_worst_case_commit_list_fits_the_reservation_it_is_made_under() {
    use crate::protocol::{LineStats, SideKind};
    let id = "f".repeat(64);
    let side = |path: &str| ChangeSide::Present {
        path: path.to_string(),
        kind: SideKind::File,
        source: ContentSource::Commit {
            commit: id.clone(),
            branch: None,
            blob: id.clone(),
        },
    };
    // Every path byte the budget allows, on both sides of the most entries it allows; every
    // subject character needing the longest escape; ids of the longest object format.
    let path = "p".repeat(budget::MAX_CHANGE_PATH_BYTES / (2 * budget::MAX_CHANGE_ENTRIES));
    let entry = ChangeEntry::Committed {
        old: side(&path),
        new: side(&path),
        stats: Some(LineStats {
            added: u64::MAX,
            removed: u64::MAX,
        }),
    };
    let per_commit = budget::MAX_CHANGE_ENTRIES / budget::MAX_COMMITS;
    let commit = |changes: Vec<ChangeEntry>| CommitChanges {
        commit: id.clone(),
        parent: Some(id.clone()),
        subject: "\u{1}".repeat(budget::MAX_COMMIT_SUBJECT_CHARS),
        changes,
    };
    let serialized = |event: &Event| serde_json::to_string(event).unwrap().len();
    let reply = |commits: Vec<CommitChanges>| Event::ProjectCommits {
        id: None,
        project: String::new(),
        base: id.clone(),
        tip: id.clone(),
        commits,
        complete: true,
        commits_complete: true,
    };

    let empty = serialized(&reply(vec![commit(vec![])]));
    let bare = serialized(&reply(vec![]));
    assert!(
        empty - bare <= budget::COMMIT_ENTRY_BYTES,
        "a commit takes {} bytes beside its changes",
        empty - bare
    );
    let worst = reply(
        (0..budget::MAX_COMMITS)
            .map(|_| commit(vec![entry.clone(); per_commit]))
            .collect(),
    );
    let paths = budget::MAX_CHANGE_PATH_BYTES;
    let entries = budget::MAX_CHANGE_ENTRIES * budget::CHANGE_ENTRY_BYTES;
    let commits = budget::MAX_COMMITS * budget::COMMIT_ENTRY_BYTES;
    assert!(serialized(&worst) <= paths + entries + commits);
    assert!(
        2 * (paths + entries + commits) + budget::TREE_DIFF_STDOUT + budget::LINE_COUNT_RESERVATION
            <= budget::COMMITS_RESERVATION
    );
}

#[test]
fn a_commit_list_past_its_budget_is_made_within_a_subdirectory_project_alone() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "commits-fallback",
        &[(b"sub/a.txt", b"a\n"), (b"other/o.txt", b"o\n")],
    );
    let fork = rev_parse(&repo, "main");
    git(&repo, &["checkout", "-q", "-b", "topic"]);
    let many: Vec<(String, Option<&[u8]>)> = (0..50)
        .map(|n| (format!("other/n{n:02}.txt"), Some(&b"n\n"[..])))
        .collect();
    let files: Vec<(&str, Option<&[u8]>)> = many.iter().map(|(p, b)| (p.as_str(), *b)).collect();
    commit_at(&repo, "outside", 10, &files);
    commit_at(&repo, "inside", 20, &[("sub/a.txt", Some(b"changed\n"))]);
    let tip = rev_parse(&repo, "HEAD");
    git(&repo, &["checkout", "-q", "main"]);
    let sub = fixture.project("sub", &repo.join("sub"));
    let whole = fixture.project("whole", &repo);

    let full = fixture.commits(&sub, &fork, &tip).unwrap();
    assert_eq!(
        shape(&full.commits),
        [
            (
                "inside".to_string(),
                vec!["committed a.txt a.txt".to_string()]
            ),
            ("outside".to_string(), vec![])
        ],
        "a commit that changed nothing inside is listed with no changes"
    );
    super::tree_diff::TREE_DIFF_STDOUT_OVERRIDE.with(|limit| limit.set(Some(1024)));
    let scoped = fixture.commits(&sub, &fork, &tip);
    let at_root = fixture.commits(&whole, &fork, &tip);
    super::tree_diff::TREE_DIFF_STDOUT_OVERRIDE.with(|limit| limit.set(None));
    assert_eq!(
        shape(&scoped.unwrap().commits),
        [(
            "inside".to_string(),
            vec!["committed a.txt a.txt".to_string()]
        )],
        "within the project alone, only the commits that touch it"
    );
    let err = at_root.expect_err("a refusal");
    let coded = err.downcast_ref::<CodedError>().unwrap();
    assert_eq!(
        (coded.code, coded.params.get("limit").map(String::as_str)),
        (error_code::LIMIT_EXCEEDED, Some("git_output"))
    );
}
