use super::*;

/// Records a look at `path` as still stuck for as long as `stuck` lives, so the path counts as one
/// whose look outran its deadline.
fn stall(state: &AppState, path: &Path, stuck: &Arc<()>) {
    state
        .directory_looks
        .record(crate::hostfs::lexical_key(path), Arc::downgrade(stuck));
}

/// [`project::plan`] of `p1`'s removal, on a blocking thread as the daemon runs it.
async fn plan_now(
    state: &Arc<AppState>,
    reads: project::Reads,
    held_back: Vec<PathBuf>,
) -> Option<project::Removal> {
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        project::plan(&state, &project_p1(&state), reads, &held_back).unwrap()
    })
    .await
    .unwrap()
}

/// The state the inspection gives the worktree at `root`, with its detail.
async fn worktree_stands(
    fixture: &Fixture,
    sessions: &[Session],
    root: &Path,
) -> (SessionBranchState, Option<String>) {
    let path = wire_path::encode(&crate::platform::names::name_bytes(root.as_os_str()));
    let item = fixture
        .inspect(sessions)
        .await
        .into_iter()
        .find(|item| item.path.as_deref() == Some(path.as_str()))
        .expect("the worktree is listed");
    (item.state, item.detail)
}

/// How a project's removal with its folder lists the worktree at `root`: the item, `None` where it
/// is not listed.
async fn removal_lists(fixture: &Fixture, root: &Path) -> Option<SessionBranchItem> {
    let path = wire_path::encode(&crate::platform::names::name_bytes(root.as_os_str()));
    plan_now(&fixture.state, project::Reads::Inspection, Vec::new())
        .await?
        .items()
        .into_iter()
        .find(|item| item.path.as_deref() == Some(path.as_str()))
}

/// Whether `item` is one kept because whether it is or holds another project's folder could not be
/// told.
fn is_untold(item: &SessionBranchItem) -> bool {
    item.state == SessionBranchState::Failed
        && item.cause == Some(SessionBranchCause::ProjectFolderUnknown)
        && item.detail.as_deref() == Some(PROJECT_FOLDER_UNTOLD)
}

/// A worktree holding another project's folder is that project's however the folder's path is
/// spelled: through the second real path a macOS firmlink gives it (as a bind mount does on
/// Linux), through a link, or in another letter case on a file system that ignores case. It is
/// never removed, nor taken along with a project's folder. A scratch directory on macOS's data
/// volume always has a second real path, so its absence fails there.
#[tokio::test]
async fn a_worktree_holding_another_projects_folder_by_another_path_is_never_removed() {
    let fixture = Fixture::new("cleanup-holds-other");
    let archived = [fixture.store("s1", SessionStatus::Archived)];
    let root = crate::hostfs::canonical(fixture.worktree("mine", "s1")).unwrap();
    std::fs::create_dir(root.join("inner")).unwrap();

    let mut spellings = Vec::new();
    if let Some(second) = crate::test_support::firmlink_spelling(&root) {
        spellings.push(("firmlink", second.join("inner")));
    }
    let link = fixture.worktrees.join("link");
    crate::test_support::symlink(&root, &link).unwrap();
    spellings.push(("link", link.join("inner")));
    let name = root.file_name().unwrap().to_string_lossy().to_uppercase();
    let respelled = root.with_file_name(name);
    let identity =
        |path: &Path| crate::platform::fs::identity(path, crate::platform::fs::Links::Follow).ok();
    if identity(&respelled) == identity(&root) {
        spellings.push(("case", respelled.join("INNER")));
    } else {
        eprintln!("no case-only respelling: the file system tells case apart");
    }

    for (how, path) in &spellings {
        other_project_at(&fixture.state, path);
        assert_eq!(
            worktree_stands(&fixture, &archived, &root).await,
            (SessionBranchState::ProjectFolder, None),
            "{how}"
        );
        assert!(removal_lists(&fixture, &root).await.is_none(), "{how}");
    }
    let left = fixture.remove(&archived, &[]).await;
    assert!(root.exists());
    assert!(left
        .iter()
        .any(|item| item.state == SessionBranchState::ProjectFolder));
}

/// Where whether a worktree holds another project's folder cannot be told — that project's path
/// did not answer in time, or the worktree's own root did not — the worktree is kept as if it did:
/// left with a reason of its own, which the removal reports, and not listed as going along with a
/// project's folder but held back. Once both answer, it goes as before.
#[tokio::test]
async fn a_worktree_whose_holding_cannot_be_told_is_kept_and_reported() {
    let fixture = Fixture::new("cleanup-holds-untold");
    let archived = [fixture.store("s1", SessionStatus::Archived)];
    let root = crate::hostfs::canonical(fixture.worktree("mine", "s1")).unwrap();
    let elsewhere = fixture.worktrees.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    other_project_at(&fixture.state, &elsewhere);
    let untold = (
        SessionBranchState::Failed,
        Some(PROJECT_FOLDER_UNTOLD.to_string()),
    );

    for stalled in [&elsewhere, &root] {
        let stuck = Arc::new(());
        stall(&fixture.state, stalled, &stuck);
        assert_eq!(
            worktree_stands(&fixture, &archived, &root).await,
            untold,
            "{}",
            stalled.display()
        );
        let removal = plan_now(&fixture.state, project::Reads::Inspection, Vec::new())
            .await
            .unwrap();
        let listed: Vec<_> = removal
            .items()
            .into_iter()
            .filter(|item| item.path.is_some())
            .collect();
        assert!(listed.len() == 1 && is_untold(&listed[0]), "{listed:?}");
        assert_eq!(removal.held_back(), vec![root.clone()]);
    }

    let stuck = Arc::new(());
    stall(&fixture.state, &root, &stuck);
    let left = fixture.remove(&archived, &[]).await;
    assert!(root.exists() && fixture.has_branch("mine"));
    assert_eq!(
        summary(&left),
        expect(&[
            (true, "mine", SessionBranchState::Failed),
            (false, "mine", SessionBranchState::CheckedOut),
        ])
    );
    assert!(is_untold(&left[0]));
    let told: Value = serde_json::from_str(&for_client(&left)).unwrap();
    assert_eq!(told[0]["cause"], "project_folder_unknown");
    assert_eq!(told[1]["cause"], Value::Null);
    drop(stuck);

    assert_eq!(
        worktree_stands(&fixture, &archived, &root).await,
        (SessionBranchState::Removable, None)
    );
    assert_eq!(
        removal_lists(&fixture, &root).await.map(|item| item.state),
        Some(SessionBranchState::Removable)
    );
    // Held back by the inspection before it, it stays held back.
    let removal = plan_now(&fixture.state, project::Reads::Removal, vec![root.clone()])
        .await
        .unwrap();
    let left = carry_out_now(&fixture, removal, &[]);
    assert_eq!(
        summary(&left),
        expect(&[(true, "mine", SessionBranchState::Failed)])
    );
    assert!(root.exists());
}

/// Another project's path with nothing at it, or that cannot be read for want of permission, is
/// compared by its text, as written and as resolved: inside the worktree it keeps the worktree,
/// elsewhere it does not. A sibling whose name merely starts the same is not inside. Skipped as
/// root, who reads a folder whatever its permissions, for the unreadable ones.
#[tokio::test]
async fn another_project_with_nothing_there_or_unreadable_is_compared_by_its_text() {
    let fixture = Fixture::new("cleanup-holds-text");
    let archived = [fixture.store("s1", SessionStatus::Archived)];
    let root = crate::hostfs::canonical(fixture.worktree("mine", "s1")).unwrap();
    let stands = |path: PathBuf| {
        let (fixture, archived, root) = (&fixture, &archived, &root);
        async move {
            other_project_at(&fixture.state, &path);
            worktree_stands(fixture, archived, root).await.0
        }
    };
    let held = SessionBranchState::ProjectFolder;
    let clear = SessionBranchState::Removable;
    assert_eq!(stands(root.join("missing")).await, held);
    assert_eq!(
        stands(fixture.worktrees.join("elsewhere/missing")).await,
        clear
    );

    #[cfg(unix)]
    {
        if unsafe { libc::geteuid() } != 0 {
            use std::os::unix::fs::PermissionsExt;
            let elsewhere = crate::hostfs::canonical(&*fixture.worktrees).unwrap();
            let locked = [root.join("locked"), elsewhere.join("locked")];
            for path in &locked {
                std::fs::create_dir_all(path.join("project")).unwrap();
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).unwrap();
            }
            let inside = stands(locked[0].join("project")).await;
            let outside = stands(locked[1].join("project")).await;
            for path in &locked {
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
                std::fs::remove_dir_all(path).unwrap();
            }
            assert_eq!((inside, outside), (held, clear));
        } else {
            eprintln!("skipped the unreadable paths: root reads a folder whatever its permissions");
        }
    }

    let sibling = fixture.worktrees.join("mine-2");
    std::fs::create_dir(&sibling).unwrap();
    assert_eq!(stands(sibling).await, clear);
    let left = fixture.remove(&archived, &[]).await;
    assert!(left.is_empty(), "{left:?}");
    assert!(!root.exists());
}

/// The removal confirmed after an inspection that held a worktree back, given that inspection's
/// token, leaves it and reports it, even once it can be told; without the token, one whose root
/// does not answer during the removal is left and reported all the same.
#[tokio::test]
async fn a_removal_after_an_inspection_that_held_a_worktree_back_leaves_it() {
    for with_token in [true, false] {
        let fixture = Fixture::new(&format!("cleanup-held-back-{with_token}"));
        let state = &fixture.state;
        fixture.store("s1", SessionStatus::Archived);
        let root = crate::hostfs::canonical(fixture.worktree("mine", "s1")).unwrap();
        let elsewhere = fixture.worktrees.join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        other_project_at(state, &elsewhere);

        let mut stuck = Some(Arc::new(()));
        stall(state, &root, stuck.as_ref().unwrap());
        let inspected = crate::coordinator::handle(
            state,
            None,
            RequestBody::InspectProjectRemoval {
                project: "p1".to_string(),
            },
        )
        .await
        .unwrap();
        let Some(Event::ProjectRemovalInspected {
            items, inspection, ..
        }) = inspected
        else {
            panic!("not inspected: {inspected:?}");
        };
        let listed: Vec<_> = items.iter().filter(|item| item.path.is_some()).collect();
        assert!(listed.len() == 1 && is_untold(listed[0]), "{items:?}");
        assert!(inspection.is_some());
        // Without the token the root stays stuck through the removal.
        if with_token {
            stuck.take();
        }

        let mut events = state.subscribe();
        crate::coordinator::handle(
            state,
            None,
            RequestBody::DeleteProject {
                project: "p1".to_string(),
                stop_sessions: true,
                trash_directory: true,
                force_remove: Vec::new(),
                inspection: inspection.filter(|_| with_token),
            },
        )
        .await
        .unwrap();
        let told = reported(&mut events).await;
        assert_eq!(told[0]["state"], "failed", "{told}");
        assert_eq!(told[0]["cause"], "project_folder_unknown", "{told}");
        assert_eq!(told[0]["detail"], PROJECT_FOLDER_UNTOLD, "{told}");
        assert!(root.exists() && !fixture.repo.exists());
        assert_eq!(state.unresolved_removals.len(), 0);
    }
}

/// A removal that fails before it stands, such as one whose folder cannot be moved to the Trash,
/// leaves the token of the inspection that held a worktree back as it was, so the retry confirmed
/// with it holds that worktree back too.
#[tokio::test]
async fn a_retried_removal_still_holds_back_what_its_inspection_held_back() {
    let fixture = Fixture::new("cleanup-held-back-retry");
    let state = &fixture.state;
    fixture.store("s1", SessionStatus::Archived);
    let root = crate::hostfs::canonical(fixture.worktree("mine", "s1")).unwrap();
    let elsewhere = fixture.worktrees.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    other_project_at(state, &elsewhere);
    let stuck = Arc::new(());
    stall(state, &root, &stuck);
    let Some(Event::ProjectRemovalInspected { inspection, .. }) = crate::coordinator::handle(
        state,
        None,
        RequestBody::InspectProjectRemoval {
            project: "p1".to_string(),
        },
    )
    .await
    .unwrap() else {
        panic!("not inspected");
    };
    drop(stuck);
    let delete = || {
        crate::coordinator::handle(
            state,
            None,
            RequestBody::DeleteProject {
                project: "p1".to_string(),
                stop_sessions: true,
                trash_directory: true,
                force_remove: Vec::new(),
                inspection: inspection.clone(),
            },
        )
    };

    state.host_tools.replace(Tools {
        trash: |path| {
            Err(crate::hostfs::trash::TrashFailure::new(
                path,
                "the Trash is full",
            ))
        },
        ..host_tools()
    });
    assert!(delete().await.is_err());
    assert!(state.store.get_project("p1").unwrap().is_some() && fixture.repo.exists());
    assert_eq!(
        state.unresolved_removals.len(),
        1,
        "the token is still kept"
    );

    state.host_tools.replace(host_tools());
    let mut events = state.subscribe();
    delete().await.unwrap();
    let told = reported(&mut events).await;
    assert_eq!(told[0]["cause"], "project_folder_unknown", "{told}");
    assert!(root.exists() && !fixture.repo.exists());
    assert_eq!(
        state.unresolved_removals.len(),
        0,
        "dropped with the project"
    );
}

/// A removal that cannot run git at all leaves everything it would carry out as failed for that
/// reason, while what it held back keeps its own reason, and so its group and note.
#[tokio::test]
async fn a_removal_that_cannot_run_git_keeps_why_it_held_a_worktree_back() {
    let fixture = Fixture::new("cleanup-held-back-no-git");
    let state = &fixture.state;
    fixture.store("s1", SessionStatus::Archived);
    let held = crate::hostfs::canonical(fixture.worktree("mine", "s1")).unwrap();
    fixture.worktree("going", "s1");
    let elsewhere = fixture.worktrees.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    other_project_at(state, &elsewhere);
    let stuck = Arc::new(());
    stall(state, &held, &stuck);
    let removal = plan_now(state, project::Reads::Removal, Vec::new())
        .await
        .unwrap();
    drop(stuck);
    state.host_tools.replace(Tools {
        git: || anyhow::bail!("git is not on the PATH"),
        ..host_tools()
    });
    let left = project::carry_out_once_stopped(
        state.clone(),
        Vec::new(),
        removal,
        Vec::new(),
        &state.track_change(),
    )
    .await;
    let path = wire_path::encode(&crate::platform::names::name_bytes(held.as_os_str()));
    let (kept, others): (Vec<_>, Vec<_>) = left
        .iter()
        .partition(|item| item.path.as_deref() == Some(path.as_str()));
    assert!(kept.len() == 1 && is_untold(kept[0]), "{left:?}");
    assert!(!others.is_empty());
    assert!(others
        .iter()
        .all(|item| item.state == SessionBranchState::Failed
            && item.cause.is_none()
            && item.detail.as_deref() == Some("git is not on the PATH")));
}

/// A project's removal looks again when it carries out: an outside worktree that came to hold
/// another project's folder after the plan stays, unreported, and so does the branch it has
/// checked out.
#[tokio::test]
async fn a_removal_leaves_a_worktree_that_came_to_hold_a_project_folder_after_its_plan() {
    let fixture = Fixture::without_project("cleanup-project-look-again");
    let folder = fixture.worktree("folder", "s1");
    insert_project(&fixture.state, &folder);
    fixture.store("s1", SessionStatus::Archived);
    let outside = crate::hostfs::canonical(fixture.worktree("outside", "s1")).unwrap();
    let removal = plan_now(&fixture.state, project::Reads::Removal, Vec::new())
        .await
        .unwrap();
    assert!(removal
        .items()
        .iter()
        .any(|item| item.kind == SessionBranchKind::Worktree
            && item.branch.as_deref() == Some("outside")));

    let inner = outside.join("inner");
    std::fs::create_dir(&inner).unwrap();
    other_project_at(&fixture.state, &inner);
    let left = project::carry_out_once_stopped(
        fixture.state.clone(),
        Vec::new(),
        removal,
        Vec::new(),
        &fixture.state.track_change(),
    )
    .await;
    assert!(inner.exists());
    assert!(fixture.has_branch("outside"));
    assert!(
        !left
            .iter()
            .any(|item| item.branch.as_deref() == Some("outside")),
        "{left:?}"
    );
}

/// A worktree of which it cannot be told, when the removal looks again, whether it holds another
/// project's folder is held back then: left and reported as such, with the branch it has checked
/// out staying, unreported.
#[tokio::test]
async fn a_removal_holds_back_a_worktree_it_can_no_longer_tell_about() {
    let fixture = Fixture::without_project("cleanup-project-look-again-untold");
    let folder = fixture.worktree("folder", "s1");
    insert_project(&fixture.state, &folder);
    fixture.store("s1", SessionStatus::Archived);
    let outside = crate::hostfs::canonical(fixture.worktree("outside", "s1")).unwrap();
    let elsewhere = fixture.worktrees.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    other_project_at(&fixture.state, &elsewhere);
    let removal = plan_now(&fixture.state, project::Reads::Removal, Vec::new())
        .await
        .unwrap();

    let stuck = Arc::new(());
    stall(&fixture.state, &outside, &stuck);
    let left = project::carry_out_once_stopped(
        fixture.state.clone(),
        Vec::new(),
        removal,
        Vec::new(),
        &fixture.state.track_change(),
    )
    .await;
    drop(stuck);
    assert!(outside.exists());
    assert!(fixture.has_branch("outside"));
    let about: Vec<_> = left
        .iter()
        .filter(|item| item.branch.as_deref() == Some("outside"))
        .collect();
    assert!(
        about.len() == 1 && about[0].kind == SessionBranchKind::Worktree && is_untold(about[0]),
        "{left:?}"
    );
}

/// A worktree holding another project's repository — its main worktree, with the common directory
/// a linked worktree elsewhere that is that project's folder works from — would take that
/// project's history along, so it counts as holding that project's folder.
#[tokio::test]
async fn a_worktree_holding_another_projects_repository_is_never_removed() {
    let fixture = Fixture::new("cleanup-holds-repository");
    let archived = [fixture.store("s1", SessionStatus::Archived)];
    let root = crate::hostfs::canonical(fixture.worktree("mine", "s1")).unwrap();
    let nested = root.join("nested");
    std::fs::create_dir(&nested).unwrap();
    git(&nested, &["init", "-q", "-b", "main"]);
    git(&nested, &["commit", "-q", "--allow-empty", "-m", "init"]);
    let linked = fixture.worktrees.join("linked");
    git(
        &nested,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "theirs",
            linked.to_str().unwrap(),
        ],
    );
    other_project_at(&fixture.state, &linked);
    assert_eq!(
        worktree_stands(&fixture, &archived, &root).await,
        (SessionBranchState::ProjectFolder, None)
    );
    assert!(removal_lists(&fixture, &root).await.is_none());
    fixture.remove(&archived, &[]).await;
    assert!(root.exists() && linked.exists());
}

/// A project's folder that holds a worktree, as one with its worktrees laid out under it does,
/// loses nothing when the worktree goes: the worktree is removed as any other.
#[tokio::test]
async fn a_worktree_inside_another_projects_folder_goes() {
    let fixture = Fixture::new("cleanup-inside-other");
    let archived = [fixture.store("s1", SessionStatus::Archived)];
    let root = crate::hostfs::canonical(fixture.worktree("mine", "s1")).unwrap();
    other_project_at(&fixture.state, &fixture.worktrees);
    let left = fixture.remove(&archived, &[]).await;
    assert!(left.is_empty(), "{left:?}");
    assert!(!root.exists());
}

/// What removing what `archived` is bound to leaves, with `home` and `data` the home directory and
/// Octoboard's data directory.
fn removed_with(
    fixture: &Fixture,
    archived: &[Session],
    home: PathBuf,
    data: PathBuf,
) -> Vec<SessionBranchItem> {
    blocking(|| {
        let git =
            crate::branch_cleanup::cleanup_git(&fixture.state.host_tools.get(), Pace::Scheduled)
                .unwrap();
        let bound = bound_in(&fixture.state, archived).unwrap();
        let mut judge = WorktreeRemovability::of(&fixture.state, None, Some(home), data).unwrap();
        remove_in(&fixture.state, &git, &bound[0], &mut judge, &[])
    })
}

/// A worktree that is the home directory, or holds Octoboard's data directory, is never removed:
/// it is left as `failed`, saying why, and its branch stays with it.
#[test]
fn a_worktree_that_is_home_or_holds_octoboards_data_is_never_removed() {
    for case in ["home", "data"] {
        let fixture = Fixture::new(&format!("cleanup-protected-{case}"));
        let archived = [fixture.store("s1", SessionStatus::Archived)];
        let root = crate::hostfs::canonical(fixture.worktree("mine", "s1")).unwrap();
        std::fs::create_dir(root.join("data")).unwrap();
        let elsewhere = fixture.worktrees.join("elsewhere");
        let (home, data) = match case {
            "home" => (root.clone(), elsewhere),
            _ => (elsewhere, root.join("data")),
        };
        let left = removed_with(&fixture, &archived, home, data);
        assert!(root.exists() && fixture.has_branch("mine"), "{case}");
        assert_eq!(
            summary(&left),
            expect(&[
                (true, "mine", SessionBranchState::Failed),
                (false, "mine", SessionBranchState::CheckedOut),
            ]),
            "{case}"
        );
        assert_eq!(
            (left[0].detail.as_deref(), left[0].cause),
            (Some(PROTECTED), Some(SessionBranchCause::ProtectedFolder)),
            "{case}"
        );
    }
}

/// Where the home directory, or Octoboard's data directory, does not answer in time, whether the
/// worktree is the one or holds the other cannot be told: it is kept as one of which that cannot
/// be told, saying which did not answer, and its branch stays with it.
#[test]
fn a_worktree_is_kept_while_home_or_octoboards_data_cannot_be_looked_at() {
    for (case, detail) in [
        (
            "home",
            "the home folder did not respond in time, so whether this worktree is it could not \
             be told",
        ),
        (
            "data",
            "Octoboard's data folder did not respond in time, so whether it is in this worktree \
             could not be told",
        ),
    ] {
        let fixture = Fixture::new(&format!("cleanup-unlooked-{case}"));
        let archived = [fixture.store("s1", SessionStatus::Archived)];
        let root = crate::hostfs::canonical(fixture.worktree("mine", "s1")).unwrap();
        let (home, data) = (
            fixture.worktrees.join("home"),
            fixture.worktrees.join("data"),
        );
        let stuck = Arc::new(());
        stall(
            &fixture.state,
            if case == "home" { &home } else { &data },
            &stuck,
        );
        let left = removed_with(&fixture, &archived, home, data);
        assert!(root.exists() && fixture.has_branch("mine"), "{case}");
        assert_eq!(
            summary(&left),
            expect(&[
                (true, "mine", SessionBranchState::Failed),
                (false, "mine", SessionBranchState::CheckedOut),
            ]),
            "{case}"
        );
        assert_eq!(
            (left[0].detail.as_deref(), left[0].cause),
            (Some(detail), Some(SessionBranchCause::ProjectFolderUnknown)),
            "{case}"
        );
    }
}

/// A project's removal never takes along an outside worktree that is the home directory: not when
/// it plans, nor when it looks again just before carrying out.
#[test]
fn a_projects_removal_never_takes_a_worktree_that_is_home() {
    let fixture = Fixture::without_project("cleanup-project-protected");
    let folder = fixture.worktree("folder", "s1");
    insert_project(&fixture.state, &folder);
    fixture.store("s1", SessionStatus::Archived);
    let outside = crate::hostfs::canonical(fixture.worktree("outside", "s1")).unwrap();
    let (elsewhere, data) = (
        fixture.worktrees.join("home"),
        fixture.worktrees.join("data"),
    );
    let project = project_p1(&fixture.state);
    let planned = |home: &Path| {
        blocking(|| {
            let home = Some(home.to_path_buf());
            project::plan_with(
                &fixture.state,
                &project,
                project::Reads::Removal,
                &[],
                home,
                data.clone(),
            )
            .unwrap()
            .unwrap()
        })
    };
    let takes_outside = |removal: &project::Removal| {
        removal.items().iter().any(|item| {
            item.kind == SessionBranchKind::Worktree && item.branch.as_deref() == Some("outside")
        })
    };

    assert!(!takes_outside(&planned(&outside)));
    let mut removal = planned(&elsewhere);
    assert!(takes_outside(&removal));
    blocking(|| removal.look_again_with(&fixture.state, Some(outside.clone()), data.clone()));
    assert!(!takes_outside(&removal));
}
