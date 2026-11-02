use super::*;

#[tokio::test]
async fn a_clean_worktree_and_its_merged_branch_go_and_another_sessions_stay() {
    let fixture = Fixture::new("cleanup-safe");
    let root = fixture.worktree("mine", "s1");
    branch_made_by(&fixture.state, &fixture.repo, "theirs", "s2");
    let archived = [session("s1", SessionStatus::Archived)];

    assert_eq!(
        summary(&fixture.inspect(&archived).await),
        expect(&[
            (true, "mine", SessionBranchState::Removable),
            (false, "mine", SessionBranchState::Removable),
        ])
    );
    assert!(fixture.remove(&archived, &[]).await.is_empty());
    assert!(!root.exists());
    assert!(!fixture.has_branch("mine"));
    assert!(fixture.has_branch("theirs"));
}

#[tokio::test]
async fn uncommitted_changes_and_an_unmerged_branch_go_only_when_forced() {
    let fixture = Fixture::new("cleanup-forced");
    let root = fixture.worktree("work", "s1");
    commit_in(&root);
    std::fs::write(root.join("new.txt"), "untracked\n").unwrap();
    let archived = [session("s1", SessionStatus::Archived)];

    let shown = fixture.inspect(&archived).await;
    assert_eq!(
        summary(&shown),
        expect(&[
            (true, "work", SessionBranchState::UncommittedChanges),
            (false, "work", SessionBranchState::Unmerged),
        ])
    );
    // Kept for its changes, the worktree keeps the branch it has checked out, merged or not.
    let left = fixture.remove(&archived, &[]).await;
    assert_eq!(
        summary(&left),
        expect(&[
            (true, "work", SessionBranchState::UncommittedChanges),
            (false, "work", SessionBranchState::CheckedOut),
        ])
    );
    let told = to_report(left, &[], &[]);
    assert!(told.len() == 1 && told[0].kind == SessionBranchKind::Worktree);
    assert!(root.join("new.txt").exists());
    assert!(fixture.has_branch("work"));

    let forced: Vec<SessionBranchTarget> = shown.iter().map(target).collect();
    assert!(fixture.remove(&archived, &forced).await.is_empty());
    assert!(!root.exists());
    assert!(!fixture.has_branch("work"));
}

#[tokio::test]
async fn what_an_agents_archive_will_leave_is_what_it_leaves() {
    let fixture = Fixture::new("cleanup-agent-outlook");
    let root = fixture.worktree("work", "s1");
    commit_in(&root);
    std::fs::write(root.join("new.txt"), "untracked\n").unwrap();
    let locked = fixture.worktree("locked", "s1");
    commit_in(&locked);
    fixture.lock(&locked);
    let clean = fixture.worktree("clean", "s1");
    commit_in(&clean);
    let archived = [session("s1", SessionStatus::Archived)];

    let (state, sessions) = (fixture.state.clone(), archived.to_vec());
    let foreseen = tokio::task::spawn_blocking(move || will_be_left(&state, &sessions))
        .await
        .unwrap();
    // A worktree git refuses keeps the branch it has checked out; one that goes leaves an
    // unmerged branch that `git branch -d` refuses.
    assert_eq!(
        summary(&foreseen),
        expect(&[
            (true, "locked", SessionBranchState::Locked),
            (true, "work", SessionBranchState::UncommittedChanges),
            (false, "clean", SessionBranchState::Unmerged),
            (false, "locked", SessionBranchState::CheckedOut),
            (false, "work", SessionBranchState::CheckedOut),
        ])
    );
    assert_eq!(
        summary(&fixture.remove(&archived, &[]).await),
        summary(&foreseen)
    );
}

#[tokio::test]
async fn what_git_must_keep_stays_even_when_forced_and_a_locked_worktree_goes_only_when_forced() {
    let fixture = Fixture::new("cleanup-kept");
    // Checked out in the main worktree.
    branch_made_by(&fixture.state, &fixture.repo, "in-main", "s1");
    git(&fixture.repo, &["switch", "-q", "in-main"]);
    let locked = fixture.worktree("locked", "s1");
    fixture.lock(&locked);
    // A worktree that is another project's folder, locked too.
    let folder = fixture.worktree("folder", "s1");
    fixture.lock(&folder);
    let mut other = fixture.state.store.get_project("p1").unwrap().unwrap();
    other.id = "p2".to_string();
    other.path = folder.to_string_lossy().into_owned();
    fixture.state.store.insert_project(&other).unwrap();
    let archived = [session("s1", SessionStatus::Archived)];

    let left = fixture.remove(&archived, &[]).await;
    assert_eq!(
        summary(&left),
        expect(&[
            (true, "folder", SessionBranchState::ProjectFolder),
            (true, "locked", SessionBranchState::Locked),
            (false, "folder", SessionBranchState::CheckedOut),
            (false, "in-main", SessionBranchState::CheckedOut),
            (false, "locked", SessionBranchState::CheckedOut),
        ])
    );
    assert!(locked.exists());

    let forced: Vec<SessionBranchTarget> = left.iter().map(target).collect();
    assert_eq!(fixture.remove(&archived, &forced).await.len(), 3);
    assert!(!locked.exists() && !fixture.has_branch("locked"));
    assert!(folder.exists() && fixture.has_branch("folder") && fixture.has_branch("in-main"));
}

#[tokio::test]
async fn an_unmerged_branch_checked_out_in_a_worktree_that_stays_is_left_as_checked_out() {
    let fixture = Fixture::new("cleanup-held-unmerged");
    let folder = fixture.worktree("folder", "s1");
    commit_in(&folder);
    other_project_at(&fixture.state, &folder);
    let archived = [session("s1", SessionStatus::Archived)];

    let shown = fixture.inspect(&archived).await;
    assert_eq!(
        summary(&shown),
        expect(&[
            (true, "folder", SessionBranchState::ProjectFolder),
            (false, "folder", SessionBranchState::CheckedOut),
        ])
    );
    let left = fixture.remove(&archived, &[]).await;
    assert_eq!(summary(&left), summary(&shown));
    assert!(to_report(left, &[], &[]).is_empty());
    assert!(fixture.has_branch("folder"));
}

#[tokio::test]
async fn a_force_goes_no_further_than_what_was_shown() {
    let fixture = Fixture::new("cleanup-consent");
    let changed = fixture.worktree("changed", "s1");
    std::fs::write(changed.join("new.txt"), "untracked\n").unwrap();
    let locked = fixture.worktree("locked", "s1");
    fixture.lock(&locked);
    let archived = [session("s1", SessionStatus::Archived)];
    let shown: Vec<SessionBranchTarget> = fixture
        .inspect(&archived)
        .await
        .iter()
        .filter(|item| item.kind == SessionBranchKind::Worktree)
        .map(target)
        .collect();
    assert!(shown.iter().all(
        |target| !target.uncommitted || target.state == SessionBranchState::UncommittedChanges
    ));

    // Since the confirmation: the one with changes was locked, the locked one gained changes.
    fixture.lock(&changed);
    std::fs::write(locked.join("new.txt"), "untracked\n").unwrap();
    let left = fixture.remove(&archived, &shown).await;
    assert!(changed.exists() && locked.exists());
    let worktrees: Vec<_> = left
        .iter()
        .filter(|item| item.kind == SessionBranchKind::Worktree)
        .collect();
    assert!(worktrees
        .iter()
        .all(|item| item.state == SessionBranchState::Locked && item.uncommitted));
    assert_eq!(to_report(left, &shown, &[]).len(), 2);
}

#[tokio::test]
async fn a_worktree_whose_directory_is_gone_goes_with_its_branch() {
    let fixture = Fixture::new("cleanup-prunable");
    let root = fixture.worktree("gone", "s1");
    std::fs::remove_dir_all(&root).unwrap();
    let archived = [session("s1", SessionStatus::Archived)];
    assert!(fixture.remove(&archived, &[]).await.is_empty());
    assert!(!fixture.has_branch("gone"));
}

#[tokio::test]
async fn a_worktree_with_submodules_goes_only_when_forced() {
    let fixture = Fixture::new("cleanup-submodules");
    let inner = repo_with("cleanup-submodules-inner", &[(b"b.txt", b"b\n")]);
    let root = fixture.worktree("subs", "s1");
    git(
        &root,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            inner.to_str().unwrap(),
            "inner",
        ],
    );
    git(&root, &["commit", "-qm", "inner"]);
    let archived = [session("s1", SessionStatus::Archived)];
    let shown = fixture.inspect(&archived).await;
    let worktree = shown
        .iter()
        .find(|item| item.kind == SessionBranchKind::Worktree)
        .unwrap();
    assert_eq!(worktree.state, SessionBranchState::Submodules);

    assert_eq!(fixture.remove(&archived, &[]).await.len(), 2);
    assert!(root.exists());
    let forced: Vec<SessionBranchTarget> = shown.iter().map(target).collect();
    assert!(fixture.remove(&archived, &forced).await.is_empty());
    assert!(!root.exists() && !fixture.has_branch("subs"));
}

#[test]
fn a_branch_counts_as_merged_into_its_upstream_where_it_has_one() {
    let fixture = Fixture::new("cleanup-upstream");
    let git_ = crate::branch_cleanup::cleanup_git(&fixture.state.host_tools.get()).unwrap();
    let repo = repository_of(&fixture.repo).unwrap();
    // `ahead` is ahead of `HEAD` but merged into its upstream, `up`.
    git(&fixture.repo, &["switch", "-q", "-c", "up"]);
    commit_in(&fixture.repo);
    git(&fixture.repo, &["branch", "ahead"]);
    git(&fixture.repo, &["switch", "-q", "main"]);
    git(&fixture.repo, &["branch", "--set-upstream-to=up", "ahead"]);
    // `behind` is merged into `HEAD` but not into its upstream, an unrelated branch.
    git(&fixture.repo, &["branch", "behind"]);
    git(&fixture.repo, &["switch", "-q", "--orphan", "elsewhere"]);
    git(
        &fixture.repo,
        &["commit", "-q", "--allow-empty", "-m", "orphan"],
    );
    git(&fixture.repo, &["switch", "-q", "main"]);
    git(
        &fixture.repo,
        &["branch", "--set-upstream-to=elsewhere", "behind"],
    );

    assert!(is_merged(&git_, &repo, b"ahead"));
    assert!(!is_merged(&git_, &repo, b"behind"));
    assert!(is_merged(&git_, &repo, b"main"));
}

#[tokio::test]
async fn a_git_that_cannot_run_leaves_every_bound_item_failed() {
    let fixture = Fixture::new("cleanup-no-git");
    branch_made_by(&fixture.state, &fixture.repo, "mine", "s1");
    fixture.state.host_tools.replace(crate::host_tools::Tools {
        git: || anyhow::bail!("git is not on the PATH"),
        ..host_tools()
    });
    let archived = [session("s1", SessionStatus::Archived)];
    let (state, sessions) = (fixture.state.clone(), archived.clone());
    let inspected =
        tokio::task::spawn_blocking(move || inspect(&state, &sessions).map(|_| ())).await;
    assert!(inspected.unwrap().is_err());
    let left = fixture.remove(&archived, &[]).await;
    assert_eq!(
        summary(&left),
        expect(&[(false, "mine", SessionBranchState::Failed)])
    );
    assert!(fixture.has_branch("mine"));
}

#[tokio::test]
async fn removal_skips_a_session_no_longer_archived_and_takes_a_deleted_ones() {
    let fixture = Fixture::new("cleanup-resumed");
    let resumed = fixture.store("s1", SessionStatus::Idle);
    branch_made_by(&fixture.state, &fixture.repo, "resumed", "s1");
    branch_made_by(&fixture.state, &fixture.repo, "deleted", "s2");

    let archived = vec![
        Session {
            status: SessionStatus::Archived,
            ..resumed
        },
        session("s2", SessionStatus::Archived),
    ];
    assert!(fixture.remove(&archived, &[]).await.is_empty());
    assert!(fixture.has_branch("resumed"));
    assert!(!fixture.has_branch("deleted"));
}

/// A session resumed while its removal waits has a process again, which is no reason to wait on.
#[tokio::test]
async fn waiting_ends_for_a_session_resumed_meanwhile() {
    let fixture = Fixture::new("cleanup-wait");
    fixture.store("s1", SessionStatus::Idle);
    let _live = running(&fixture.state, "s1");
    let started = Instant::now();
    fixture
        .remove(&[session("s1", SessionStatus::Archived)], &[])
        .await;
    assert!(started.elapsed() < STOP_TIMEOUT / 2);
}

#[test]
fn the_user_is_told_of_what_was_not_kept_on_purpose() {
    let item = |state, uncommitted| SessionBranchItem {
        kind: SessionBranchKind::Worktree,
        repository: "/r/.git".to_string(),
        branch: Some("b".to_string()),
        path: Some("/w".to_string()),
        project: "p1".to_string(),
        session: Some("s1".to_string()),
        state,
        uncommitted,
        detail: None,
        cause: None,
    };
    let kept = target(&item(SessionBranchState::UncommittedChanges, true));
    for (left, forced, keep, told) in [
        (
            item(SessionBranchState::Failed, false),
            vec![],
            vec![],
            true,
        ),
        (
            item(SessionBranchState::Locked, false),
            vec![kept.clone()],
            vec![],
            true,
        ),
        (
            item(SessionBranchState::UncommittedChanges, true),
            vec![],
            vec![],
            true,
        ),
        (
            item(SessionBranchState::UncommittedChanges, true),
            vec![],
            vec![kept.clone()],
            false,
        ),
        (
            item(SessionBranchState::Locked, true),
            vec![],
            vec![kept.clone()],
            true,
        ),
        (
            item(SessionBranchState::CheckedOut, false),
            vec![],
            vec![],
            false,
        ),
        (
            item(SessionBranchState::ProjectFolder, false),
            vec![],
            vec![],
            false,
        ),
        (
            item(SessionBranchState::ProjectFolder, false),
            vec![kept.clone()],
            vec![],
            true,
        ),
    ] {
        let state = left.state;
        assert_eq!(
            !to_report(vec![left], &forced, &keep).is_empty(),
            told,
            "{state:?}"
        );
    }
}

#[test]
fn an_agent_is_told_each_item_left_in_plain_text_with_its_reason() {
    let item = SessionBranchItem {
        kind: SessionBranchKind::Worktree,
        repository: "/r/.git".to_string(),
        branch: Some("caf%E9".to_string()),
        path: Some("/r/wt%25".to_string()),
        project: "p1".to_string(),
        session: Some("s1".to_string()),
        state: SessionBranchState::Locked,
        uncommitted: true,
        detail: None,
        cause: None,
    };
    assert_eq!(
        for_agent(&[item]),
        json!([{
            "kind": "worktree",
            "branch": "caf\u{FFFD}",
            "path": "/r/wt%",
            "reason": "the worktree is locked, with uncommitted changes",
        }])
    );
}

#[test]
fn a_client_is_told_each_item_left_as_structured_data() {
    let item = SessionBranchItem {
        kind: SessionBranchKind::Branch,
        repository: "/r/.git".to_string(),
        branch: Some("a,b".to_string()),
        path: None,
        project: "p1".to_string(),
        session: None,
        state: SessionBranchState::Failed,
        uncommitted: false,
        detail: Some("error: refused".to_string()),
        cause: None,
    };
    let parsed: Value = serde_json::from_str(&for_client(&[item])).unwrap();
    assert_eq!(
        parsed,
        json!([{ "kind": "branch", "name": "a,b", "branch": "a,b", "state": "failed", "uncommitted": false, "detail": "error: refused", "cause": null }])
    );
}
