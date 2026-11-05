use super::*;

#[test]
fn a_folder_holding_the_repository_takes_every_outside_worktree_but_another_projects() {
    let fixture = Fixture::new("cleanup-project-whole");
    fixture.store("s1", SessionStatus::Archived);
    let bound = fixture.worktree("bound", "s1");
    std::fs::write(bound.join("new.txt"), "untracked\n").unwrap();
    // Bound to no session, with a detached `HEAD`.
    let detached = fixture.worktrees.join("detached");
    git(
        &fixture.repo,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            detached.to_str().unwrap(),
        ],
    );
    let locked = fixture.worktrees.join("locked");
    git(
        &fixture.repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "locked",
            locked.to_str().unwrap(),
        ],
    );
    fixture.lock(&locked);
    std::fs::write(locked.join("new.txt"), "untracked\n").unwrap();
    // Another project's folder.
    let other = fixture.worktrees.join("other");
    git(
        &fixture.repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "other",
            other.to_str().unwrap(),
        ],
    );
    let mut theirs = project_p1(&fixture.state);
    theirs.id = "p2".to_string();
    theirs.path = other.to_string_lossy().into_owned();
    fixture.state.store.insert_project(&theirs).unwrap();
    branch_made_by(&fixture.state, &fixture.repo, "plain", "s1");

    let plan = |reads| {
        blocking(|| project::plan(&fixture.state, &project_p1(&fixture.state), reads, &[]))
            .unwrap()
            .unwrap()
    };
    let shown = plan(project::Reads::Inspection).items();
    assert_eq!(
        summary(&shown),
        expect(&[
            (true, "", SessionBranchState::Removable),
            (true, "bound", SessionBranchState::UncommittedChanges),
            (true, "locked", SessionBranchState::Locked),
        ])
    );
    assert!(shown
        .iter()
        .any(|item| item.state == SessionBranchState::Locked && item.uncommitted));
    let unbound = shown.iter().find(|item| item.branch.is_none()).unwrap();
    assert_eq!(
        (unbound.session.as_deref(), unbound.project.as_str()),
        (None, "p1")
    );

    // The removal's own plan reads no status of a worktree it takes whatever it holds: it does not
    // know of the changes.
    assert!(carry_out_now(&fixture, plan(project::Reads::Removal), &[]).is_empty());
    assert!(!bound.exists() && !detached.exists() && locked.exists() && other.exists());

    // Forced as shown, with its changes, the locked one goes too.
    let forced: Vec<SessionBranchTarget> = shown.iter().map(target).collect();
    assert!(carry_out_now(&fixture, plan(project::Reads::Removal), &forced).is_empty());
    assert!(!locked.exists());
    assert!(fixture.has_branch("plain"));
}

/// The folder holding the repository is in the Trash by the time a removal is carried out, so the
/// repository has gone with it and `git` can no longer read the locked worktree's status: the
/// plan's reading, made while it was there, decides. Forced as shown, without changes, the worktree
/// goes; one that had changes the confirmation did not show stays.
#[test]
fn a_forced_locked_worktree_goes_once_the_repository_has_gone_with_the_folder() {
    let fixture = Fixture::new("cleanup-project-gone");
    fixture.store("s1", SessionStatus::Archived);
    let clean = fixture.worktree("clean", "s1");
    fixture.lock(&clean);
    let dirty = fixture.worktree("dirty", "s1");
    fixture.lock(&dirty);
    let plan = |reads| {
        blocking(|| project::plan(&fixture.state, &project_p1(&fixture.state), reads, &[]))
            .unwrap()
            .unwrap()
    };
    let shown = plan(project::Reads::Inspection).items();
    assert!(shown.iter().all(|item| !item.uncommitted));
    let forced: Vec<SessionBranchTarget> = shown.iter().map(target).collect();
    std::fs::write(dirty.join("new.txt"), "untracked\n").unwrap();
    let removal = plan(project::Reads::Removal);
    std::fs::remove_dir_all(&*fixture.repo).unwrap();

    let left = carry_out_now(&fixture, removal, &forced);
    assert_eq!(
        summary(&left),
        expect(&[(true, "dirty", SessionBranchState::Locked)])
    );
    assert!(left[0].uncommitted);
    assert!(!clean.exists() && dirty.exists());
}

#[test]
fn a_locked_worktree_that_gained_changes_since_it_was_shown_stays() {
    let fixture = Fixture::new("cleanup-project-consent");
    fixture.store("s1", SessionStatus::Archived);
    let locked = fixture.worktree("locked", "s1");
    fixture.lock(&locked);
    let plan = |reads| {
        blocking(|| project::plan(&fixture.state, &project_p1(&fixture.state), reads, &[]))
            .unwrap()
            .unwrap()
    };
    let forced: Vec<SessionBranchTarget> = plan(project::Reads::Inspection)
        .items()
        .iter()
        .map(target)
        .collect();
    std::fs::write(locked.join("new.txt"), "untracked\n").unwrap();
    let left = carry_out_now(&fixture, plan(project::Reads::Removal), &forced);
    assert_eq!(
        summary(&left),
        expect(&[(true, "locked", SessionBranchState::Locked)])
    );
    assert!(left[0].uncommitted && locked.exists());
}

#[test]
fn a_surviving_repository_loses_the_bound_branches_and_keeps_what_it_must() {
    let fixture = Fixture::without_project("cleanup-project-part");
    // The project is a linked worktree of the repository, so the repository outlives its folder.
    let folder = fixture.worktree("folder", "s1");
    insert_project(&fixture.state, &folder);
    fixture.store("s1", SessionStatus::Archived);
    let outside = fixture.worktree("outside", "s1");
    commit_in(&outside);
    let locked = fixture.worktree("locked", "s1");
    fixture.lock(&locked);
    let kept = fixture.worktree("kept", "s1");
    fixture.lock(&kept);
    // Bound to no session: the repository survives, so it is not the project's to take.
    let unbound = fixture.worktrees.join("unbound");
    git(
        &fixture.repo,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            unbound.to_str().unwrap(),
        ],
    );
    branch_made_by(&fixture.state, &fixture.repo, "in-main", "s1");
    git(&fixture.repo, &["switch", "-q", "in-main"]);

    let shown = blocking(|| {
        project::plan(
            &fixture.state,
            &project_p1(&fixture.state),
            project::Reads::Inspection,
            &[],
        )
    })
    .unwrap()
    .unwrap()
    .items();
    assert_eq!(
        summary(&shown),
        expect(&[
            (true, "kept", SessionBranchState::Locked),
            (true, "locked", SessionBranchState::Locked),
            (true, "outside", SessionBranchState::Removable),
            (false, "folder", SessionBranchState::Removable),
            (false, "kept", SessionBranchState::Removable),
            (false, "locked", SessionBranchState::Removable),
            (false, "outside", SessionBranchState::Unmerged),
        ])
    );
    // As the confirmation names them back: the locked worktree forced, and the branches that go.
    let forced: Vec<SessionBranchTarget> = shown
        .iter()
        .filter(|item| match item.kind {
            SessionBranchKind::Worktree => item.path.as_deref().is_some_and(|path| {
                path.ends_with(&format!("{}locked", std::path::MAIN_SEPARATOR))
            }),
            SessionBranchKind::Branch => item.branch.as_deref() != Some("kept"),
        })
        .map(target)
        .collect();
    assert_eq!(forced.len(), 4);

    // The folder goes to the Trash after the plan and before the rest, as the removal runs.
    let removal = blocking(|| {
        project::plan(
            &fixture.state,
            &project_p1(&fixture.state),
            project::Reads::Removal,
            &[],
        )
    })
    .unwrap()
    .unwrap();
    std::fs::remove_dir_all(&folder).unwrap();
    let left = carry_out_now(&fixture, removal, &forced);
    assert!(left.is_empty(), "{left:?}");
    assert!(!outside.exists() && !locked.exists());
    assert!(kept.exists() && unbound.exists());
    for gone in ["folder", "outside", "locked"] {
        assert!(!fixture.has_branch(gone), "{gone}");
    }
    assert!(fixture.has_branch("kept") && fixture.has_branch("in-main"));
    let listed = listed_worktrees(&fixture.repo);
    assert!(
        !listed.contains("/folder\n")
            && !listed.contains("/outside\n")
            && !listed.contains("/locked\n")
    );
    assert!(listed.contains("/kept\n"));
}

/// An unmerged branch that has moved since the confirmation showed it, or that it did not name,
/// is left and reported; a merged one it did not name goes all the same.
#[test]
fn a_branch_goes_with_the_project_only_at_the_commit_shown() {
    let fixture = Fixture::without_project("cleanup-project-tip");
    let folder = fixture.worktree("folder", "s1");
    insert_project(&fixture.state, &folder);
    fixture.store("s1", SessionStatus::Archived);
    let moved = fixture.worktree("moved", "s1");
    commit_in(&moved);
    let steady = fixture.worktree("steady", "s1");
    commit_in(&steady);
    let unnamed = fixture.worktree("unnamed", "s1");
    commit_in(&unnamed);
    let plan = |reads| {
        blocking(|| project::plan(&fixture.state, &project_p1(&fixture.state), reads, &[]))
            .unwrap()
            .unwrap()
    };
    let forced: Vec<SessionBranchTarget> = plan(project::Reads::Inspection)
        .items()
        .iter()
        .filter(|item| {
            item.kind == SessionBranchKind::Branch
                && !matches!(item.branch.as_deref(), Some("unnamed" | "folder"))
        })
        .map(target)
        .collect();
    assert_eq!(forced.len(), 2);
    assert!(forced.iter().all(|target| target.tip.is_some()));

    let removal = plan(project::Reads::Removal);
    std::fs::write(moved.join("a.txt"), "three\n").unwrap();
    git(&moved, &["commit", "-qam", "three"]);
    std::fs::remove_dir_all(&folder).unwrap();
    let left = carry_out_now(&fixture, removal, &forced);
    assert_eq!(
        summary(&left),
        expect(&[
            (false, "moved", SessionBranchState::Unmerged),
            (false, "unnamed", SessionBranchState::Unmerged),
        ])
    );
    assert!(fixture.has_branch("moved") && fixture.has_branch("unnamed"));
    assert!(!fixture.has_branch("steady") && !fixture.has_branch("folder"));
}

#[test]
fn a_locked_project_folder_keeps_its_branch_and_says_so() {
    let fixture = Fixture::without_project("cleanup-project-locked-folder");
    let folder = fixture.worktree("folder", "s1");
    insert_project(&fixture.state, &folder);
    fixture.lock(&folder);
    fixture.store("s1", SessionStatus::Archived);
    let removal = blocking(|| {
        project::plan(
            &fixture.state,
            &project_p1(&fixture.state),
            project::Reads::Removal,
            &[],
        )
    })
    .unwrap()
    .unwrap();
    std::fs::remove_dir_all(&folder).unwrap();
    let left = carry_out_now(&fixture, removal, &[]);
    assert_eq!(
        summary(&left),
        expect(&[(false, "folder", SessionBranchState::CheckedOut)])
    );
    assert!(fixture.has_branch("folder"));
}

#[test]
fn a_locked_worktree_whose_move_to_the_trash_fails_keeps_its_lock() {
    let fixture = Fixture::without_project("cleanup-project-trash-fails");
    let folder = fixture.worktree("folder", "s1");
    insert_project(&fixture.state, &folder);
    fixture.store("s1", SessionStatus::Archived);
    let locked = fixture.worktree("pinned", "s1");
    fixture.lock(&locked);
    let plan = |reads| {
        blocking(|| project::plan(&fixture.state, &project_p1(&fixture.state), reads, &[]))
            .unwrap()
            .unwrap()
    };
    let forced: Vec<SessionBranchTarget> = plan(project::Reads::Inspection)
        .items()
        .iter()
        .filter(|item| item.state == SessionBranchState::Locked)
        .map(target)
        .collect();
    let removal = plan(project::Reads::Removal);
    std::fs::remove_dir_all(&folder).unwrap();
    let tools = fixture.state.host_tools.get();
    let left = project::carry_out(
        &crate::branch_cleanup::cleanup_git(&tools).unwrap(),
        removal,
        &forced,
        |path| {
            Err(crate::hostfs::trash::TrashFailure::new(
                path,
                "the Trash is full",
            ))
        },
    );
    assert!(left
        .iter()
        .any(|item| item.state == SessionBranchState::Failed
            && item.kind == SessionBranchKind::Worktree));
    assert!(locked.exists());
    // git's own record of the lock, in the worktree's administrative directory.
    let admin = crate::git::layout::admin_dir(&locked).unwrap();
    assert!(admin.join("locked").is_file());
}

/// The command line of the cleanup's read and of its write, pinned whole: neither pages or runs a
/// monitor, and only the read gets the index guard.
#[test]
fn the_cleanups_read_and_write_build_their_command_lines() {
    use crate::git::run::{Access, UserGit, WorkDir};
    use crate::test_support::{pinned_shell_env, CommandLine};

    let tools = crate::host_tools::Tools {
        git: || Ok(UserGit::new("/opt/git", pinned_shell_env())),
        ..crate::test_support::host_tools()
    };
    let git = crate::branch_cleanup::cleanup_git(&tools).unwrap();
    let read = git.command(
        Access::Read,
        WorkDir::At(Path::new("/repo")),
        ["worktree", "list"],
    );
    assert_eq!(
        CommandLine::of(&read),
        CommandLine::pinned(
            "/opt/git",
            &[
                "--no-pager",
                "-c",
                "core.fsmonitor=false",
                "-c",
                "core.fsmonitor=false",
                "-c",
                "diff.autoRefreshIndex=false",
                "worktree",
                "list"
            ],
            &[("GIT_OPTIONAL_LOCKS", "0")],
            Some("/repo")
        )
    );
    let write = git.command(
        Access::Write,
        WorkDir::At(Path::new("/repo")),
        ["worktree", "prune"],
    );
    assert_eq!(
        CommandLine::of(&write),
        CommandLine::pinned(
            "/opt/git",
            &[
                "--no-pager",
                "-c",
                "core.fsmonitor=false",
                "worktree",
                "prune"
            ],
            &[],
            Some("/repo")
        )
    );
}
