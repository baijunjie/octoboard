use crate::test_support::symlink;
use std::path::Path;

use super::*;
use crate::coordinator::handle;
use crate::hostfs::trash::TrashFailure;
use crate::protocol::{
    AgentAvailability, Availability, Event, Params, RequestBody, Session, SessionStatus,
};
use crate::test_support::{
    console_with_project_and_accounts, project_session, running, ScratchDir, PATIENCE,
};

/// A daemon with the accounts `claude-a`, `codex-a` and `grok-a` at `<dir>/<id>`, each agent's
/// default directory at `<dir>/default-<agent>`, and the project at `<dir>/no-such-project`.
fn fixture(label: &str) -> (Arc<AppState>, ScratchDir) {
    let (state, dir) = console_with_project_and_accounts(label);
    state.set_agent_availability(
        AgentId::ALL
            .into_iter()
            .map(|agent| AgentAvailability {
                agent,
                availability: Availability::Available,
                default_account_dir: Some(default_of(&dir, agent).to_string_lossy().into()),
            })
            .collect(),
    );
    (state, dir)
}

fn default_of(dir: &Path, agent: AgentId) -> PathBuf {
    dir.join(format!("default-{}", agent.label()))
}

fn account(state: &AppState, id: &str) -> Account {
    state.store.get_account(id).unwrap().unwrap()
}

/// `claude-a`'s directory, made to exist with a file in it.
fn create_directory(state: &AppState) -> PathBuf {
    let directory = PathBuf::from(account(state, "claude-a").config_dir);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("settings.json"), b"{}").unwrap();
    directory
}

/// A session of `account` that has a process.
fn run_on(
    state: &Arc<AppState>,
    id: &str,
    agent: AgentId,
    account: &str,
) -> crate::test_support::StandIn {
    let session = Session {
        account_id: Some(account.to_string()),
        ..project_session(id, agent, SessionStatus::Idle)
    };
    state.store.insert_session(&session).unwrap();
    running(state, id)
}

async fn remove(state: &Arc<AppState>, trash_directory: bool) -> Result<()> {
    handle(
        state,
        None,
        RequestBody::DeleteAccount {
            account: "claude-a".to_string(),
            trash_directory,
        },
    )
    .await
    .map(|_| ())
}

fn coded(err: &anyhow::Error) -> &CodedError {
    err.downcast_ref::<CodedError>().expect("a coded error")
}

/// What `claude-a` is let or refused to move when its directory is `config_dir`.
async fn judged_at(
    state: &Arc<AppState>,
    config_dir: &Path,
    home: &Path,
    data: &Path,
) -> Result<Option<PathBuf>> {
    let mut account = account(state, "claude-a");
    account.config_dir = config_dir.to_string_lossy().into_owned();
    let defaults: Vec<_> = AgentId::ALL
        .into_iter()
        .map(|agent| {
            let dir = state.agent_availability_of(agent).default_account_dir;
            (agent, PathBuf::from(dir.unwrap()))
        })
        .collect();
    judged(
        state,
        CONFIRMING,
        &account,
        &state.store.list_accounts().unwrap(),
        &defaults,
        &state.store.list_projects().unwrap(),
        Some(home.to_path_buf()),
        data.to_path_buf(),
    )
    .await
    .map_err(Refusal::raised)
}

/// The code and params `claude-a` is refused with when its directory is `config_dir`.
async fn refusal_at(
    state: &Arc<AppState>,
    config_dir: &Path,
    home: &Path,
    data: &Path,
) -> (crate::protocol::Code, Params) {
    let err = judged_at(state, config_dir, home, data).await.unwrap_err();
    (coded(&err).code, coded(&err).params.clone())
}

/// Without the option only the record goes; with it the directory goes to the Trash first.
#[tokio::test]
async fn the_directory_is_moved_to_the_trash_only_when_asked() {
    let (state, _dir) = fixture("account-trash-off");
    let directory = create_directory(&state);
    remove(&state, false).await.unwrap();
    assert!(directory.join("settings.json").exists());
    assert!(state.store.get_account("claude-a").unwrap().is_none());

    let (state, _dir) = fixture("account-trash-on");
    let directory = create_directory(&state);
    remove(&state, true).await.unwrap();
    assert!(!directory.exists());
    assert!(state.store.get_account("claude-a").unwrap().is_none());
}

/// A directory that is not there is skipped, and a link is moved as the link.
#[tokio::test]
async fn a_missing_directory_is_skipped_and_a_link_is_moved_as_itself() {
    let (state, _dir) = fixture("account-trash-missing");
    assert_eq!(directory_to_trash(&state, "claude-a").await.unwrap(), None);
    remove(&state, true).await.unwrap();
    assert!(state.store.get_account("claude-a").unwrap().is_none());

    let (state, dir) = fixture("account-trash-link");
    let target = dir.join("elsewhere");
    std::fs::create_dir_all(&target).unwrap();
    let link = PathBuf::from(account(&state, "claude-a").config_dir);
    symlink(&target, &link).unwrap();
    assert_eq!(
        directory_to_trash(&state, "claude-a").await.unwrap(),
        Some(link.clone())
    );
    remove(&state, true).await.unwrap();
    assert!(link.symlink_metadata().is_err());
    assert!(target.exists(), "what the link led to stays");
}

/// A session of the account with a process refuses the removal with the option and changes
/// nothing, one of another account does not, and without the option none does.
#[tokio::test]
async fn a_running_session_refuses_the_removal_that_moves_the_directory() {
    let (state, _dir) = fixture("account-trash-running");
    let directory = create_directory(&state);
    let _elsewhere = run_on(&state, "elsewhere", AgentId::CODEX, "codex-a");
    assert!(directory_to_trash(&state, "claude-a").await.is_ok());

    let _here = run_on(&state, "here", AgentId::CLAUDE, "claude-a");
    let err = remove(&state, true).await.unwrap_err();
    assert_eq!(coded(&err).code, error_code::ACCOUNT_HAS_RUNNING_SESSIONS);
    assert_eq!(coded(&err).params["sessions"], "1");
    assert_eq!(coded(&err).params["account"], "claude-a");
    assert!(directory.join("settings.json").exists());
    assert!(state.store.get_account("claude-a").unwrap().is_some());

    remove(&state, false).await.unwrap();
    assert!(state.store.get_account("claude-a").unwrap().is_none());
    assert!(directory.exists());
}

/// A move that fails keeps the account, and says why.
#[tokio::test]
async fn a_failed_move_keeps_the_account() {
    let (state, _dir) = fixture("account-trash-failed");
    let directory = create_directory(&state);
    state.host_tools.replace(crate::host_tools::Tools {
        trash: |path| Err(TrashFailure::new(path, "the Trash is full")),
        ..crate::test_support::host_tools()
    });
    let err = remove(&state, true).await.unwrap_err();
    assert_eq!(coded(&err).code, error_code::ACCOUNT_DIRECTORY_TRASH_FAILED);
    assert_eq!(coded(&err).params["detail"], "the Trash is full");
    assert!(directory.exists());
    assert!(state.store.get_account("claude-a").unwrap().is_some());
}

/// The home directory, a disk's root and a directory holding Octoboard's data are protected.
#[tokio::test]
async fn protected_directories_are_refused() {
    let (state, dir) = fixture("account-trash-protected");
    let (home, data) = (dir.join("home"), dir.join("holder").join("data"));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    for protected in [
        &crate::test_support::disk_root(),
        &home,
        &dir.join("holder"),
    ] {
        let (code, _) = refusal_at(&state, protected, &home, &data).await;
        assert_eq!(
            code,
            error_code::ACCOUNT_DIRECTORY_PROTECTED,
            "{protected:?}"
        );
    }
}

/// An agent's default directory, another account's directory and a project's folder are not
/// the account's to take, and each refusal names what is in the way.
#[tokio::test]
async fn what_belongs_to_something_else_is_refused() {
    let (state, dir) = fixture("account-trash-shared");
    let (home, data) = (dir.join("home"), dir.join("elsewhere").join("data"));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    let refusal = |path: PathBuf| {
        let (state, home, data) = (state.clone(), home.clone(), data.clone());
        async move { refusal_at(&state, &path, &home, &data).await }
    };

    // An agent's default directory: the same, inside it, and a directory holding it.
    let codex = default_of(&dir, AgentId::CODEX);
    std::fs::create_dir_all(codex.join("inside")).unwrap();
    let holding = dir.join("holding");
    std::fs::create_dir_all(holding.join("default")).unwrap();
    let mut moved = state.agent_availability_of(AgentId::GROK);
    moved.default_account_dir = Some(holding.join("default").to_string_lossy().into());
    state.set_agent_availability(vec![moved]);
    for (path, agent) in [
        (codex.clone(), "Codex"),
        (codex.join("inside"), "Codex"),
        (holding.clone(), "Grok Build"),
    ] {
        let (code, params) = refusal(path.clone()).await;
        assert_eq!(code, error_code::ACCOUNT_DIRECTORY_SHARED, "{path:?}");
        assert_eq!(params["kind"], "default_account");
        assert_eq!(params["agent"], agent);
    }

    // Another account's directory, of any agent: the same, a link to it, inside it, holding it.
    let grok = PathBuf::from(account(&state, "grok-a").config_dir);
    std::fs::create_dir_all(grok.join("inside")).unwrap();
    let link = dir.join("link-to-grok");
    symlink(&grok, &link).unwrap();
    let mut other = account(&state, "codex-a");
    let holder = dir.join("account-holder");
    std::fs::create_dir_all(holder.join("codex")).unwrap();
    other.config_dir = holder.join("codex").to_string_lossy().into_owned();
    state.store.update_account(&other).unwrap();
    for (path, agent, name) in [
        (grok.clone(), "Grok Build", "grok-a"),
        (link, "Grok Build", "grok-a"),
        (grok.join("inside"), "Grok Build", "grok-a"),
        (holder, "Codex", "codex-a"),
    ] {
        let (code, params) = refusal(path.clone()).await;
        assert_eq!(code, error_code::ACCOUNT_DIRECTORY_SHARED, "{path:?}");
        assert_eq!(params["kind"], "account");
        assert_eq!(
            (params["agent"].as_str(), params["name"].as_str()),
            (agent, name)
        );
    }

    // A project's folder inside the directory would lose its files, so the directory holding
    // it is refused; the folder holding the directory loses nothing.
    let project = PathBuf::from(&state.store.list_projects().unwrap()[0].path);
    std::fs::create_dir_all(project.join("account")).unwrap();
    let held = dir.join("holds-project");
    let mut second = state.store.list_projects().unwrap()[0].clone();
    second.id = "held".to_string();
    second.name = "Held".to_string();
    second.path = held.join("work").to_string_lossy().into_owned();
    std::fs::create_dir_all(&second.path).unwrap();
    state.store.insert_project(&second).unwrap();
    let (code, params) = refusal(held.clone()).await;
    assert_eq!(code, error_code::ACCOUNT_DIRECTORY_SHARED);
    assert_eq!(
        (params["kind"].as_str(), params["name"].as_str()),
        ("project", "Held")
    );
    for inside in [project.join("account"), held.join("work").join("account")] {
        std::fs::create_dir_all(&inside).unwrap();
        assert_eq!(
            judged_at(&state, &inside, &home, &data).await.unwrap(),
            Some(inside.clone()),
            "a directory inside a project's folder loses that project nothing"
        );
    }
}

/// A directory that cannot be told is no reason to take it along: refused as a failed move.
#[tokio::test]
async fn a_directory_that_cannot_be_looked_at_is_refused() {
    let (state, dir) = fixture("account-trash-unlooked");
    create_directory(&state);
    let guard = Arc::new(());
    state.directory_looks.record(
        crate::hostfs::lexical_key(&dir.join("claude-a")),
        Arc::downgrade(&guard),
    );
    let err = confirmed_within(&state, SHORT).await.unwrap_err();
    assert_eq!(coded(&err).code, error_code::ACCOUNT_DIRECTORY_TRASH_FAILED);
    assert_eq!(coded(&err).params["cause"], "unanswered");
}

/// The removal's look with `deadline` in place of its five seconds.
async fn confirmed_within(state: &Arc<AppState>, deadline: Duration) -> Result<Option<PathBuf>> {
    let looking = Looking {
        deadline,
        stuck_after: deadline,
        ..CONFIRMING
    };
    judged_now(state, "claude-a", looking)
        .await?
        .map_err(Refusal::raised)
}

const SHORT: Duration = Duration::from_millis(200);

/// A path of another account that cannot be told in time is not named as in the way: the
/// removal is refused as a failed move with its own cause, and an inspection leaves it
/// undecided when the look is slow and unlookable when it failed at once.
#[tokio::test]
async fn another_path_that_cannot_be_looked_at_is_not_called_shared() {
    let (state, _dir) = fixture("account-trash-other-unlooked");
    create_directory(&state);
    let stuck_dir = PathBuf::from(account(&state, "grok-a").config_dir);
    std::fs::create_dir_all(&stuck_dir).unwrap();
    let guard = Arc::new(());
    state.directory_looks.record(
        crate::hostfs::lexical_key(&stuck_dir),
        Arc::downgrade(&guard),
    );
    assert_eq!(inspected(&state).await, AccountTrashOutcome::Undecided);
    let err = confirmed_within(&state, SHORT).await.unwrap_err();
    assert_eq!(coded(&err).code, error_code::ACCOUNT_DIRECTORY_TRASH_FAILED);
    assert_eq!(coded(&err).params["cause"], OTHER_UNLOOKED);
    assert!(coded(&err).params["detail"].contains("did not respond in time"));
    drop(guard);

    // A project path that is not absolute fails at once.
    let mut project = state.store.list_projects().unwrap()[0].clone();
    project.id = "relative".to_string();
    project.name = "Relative".to_string();
    project.path = "relative/folder".to_string();
    state.store.insert_project(&project).unwrap();
    assert_eq!(
        inspected(&state).await,
        AccountTrashOutcome::Unlookable {
            cause: Some(OTHER_UNLOOKED.to_string())
        }
    );
    let err = remove(&state, true).await.unwrap_err();
    assert_eq!(coded(&err).code, error_code::ACCOUNT_DIRECTORY_TRASH_FAILED);
    assert_eq!(coded(&err).params["cause"], OTHER_UNLOOKED);
    assert!(coded(&err).params["detail"].contains("project \"Relative\""));
    assert!(state.store.get_account("claude-a").unwrap().is_some());
}

/// A look an inspection left stuck is waited for by the removal, which then looks afresh.
#[tokio::test]
async fn the_removal_waits_for_a_look_an_inspection_left_stuck() {
    let (state, dir) = fixture("account-trash-wait-stuck");
    let directory = create_directory(&state);
    let guard = Arc::new(());
    state.directory_looks.record(
        crate::hostfs::lexical_key(&dir.join("claude-a")),
        Arc::downgrade(&guard),
    );
    assert_eq!(inspected(&state).await, AccountTrashOutcome::Undecided);
    let ending = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        drop(guard);
    });
    assert_eq!(
        confirmed_within(&state, Duration::from_secs(3))
            .await
            .unwrap(),
        Some(directory)
    );
    ending.await.unwrap();
}

/// What the removal judged before it held the account's lock is judged again under it.
#[tokio::test]
async fn the_directory_is_judged_again_under_the_lock() {
    let (state, _dir) = fixture("account-trash-rejudged");
    let directory = create_directory(&state);
    assert!(directory_to_trash(&state, "claude-a")
        .await
        .unwrap()
        .is_some());
    let mut mine = account(&state, "claude-a");
    mine.config_dir = account(&state, "grok-a").config_dir;
    std::fs::create_dir_all(&mine.config_dir).unwrap();
    state.store.update_account(&mine).unwrap();
    let err = move_directory(&state, "claude-a").await.unwrap_err();
    assert_eq!(coded(&err).code, error_code::ACCOUNT_DIRECTORY_SHARED);
    assert!(directory.exists());
}

/// A sign-in of the account is ended before the move.
#[tokio::test]
async fn the_move_waits_for_the_accounts_sign_in_to_end() {
    let (state, dir) = fixture("account-trash-login");
    let directory = create_directory(&state);
    let bin = dir.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let env =
        crate::test_support::fake_agent(&bin, AgentId::CLAUDE.descriptor().binary, "sleep 30");
    crate::accounts::login::AccountLogins::start_for_test(
        &state,
        crate::accounts::login::Start {
            agent: AgentId::CLAUDE,
            account: Some("claude-a".to_string()),
            config_dir: Some(directory.to_string_lossy().into_owned()),
            resolved_dir: Some(directory.to_string_lossy().into_owned()),
        },
        env,
        dir.join("run"),
    )
    .unwrap();
    move_directory(&state, "claude-a").await.unwrap();
    assert!(state.account_logins.running().is_empty());
    assert!(!directory.exists());
}

/// A sign-in that has not ended when the wait is up refuses the move, with its own cause, and
/// the directory stays.
#[tokio::test]
async fn a_sign_in_that_does_not_end_in_time_refuses_the_move() {
    let (state, dir) = fixture("account-trash-login-stuck");
    let directory = create_directory(&state);
    let bin = dir.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let env =
        crate::test_support::fake_agent(&bin, AgentId::CLAUDE.descriptor().binary, "sleep 30");
    crate::accounts::login::AccountLogins::start_for_test(
        &state,
        crate::accounts::login::Start {
            agent: AgentId::CLAUDE,
            account: Some("claude-a".to_string()),
            config_dir: Some(directory.to_string_lossy().into_owned()),
            resolved_dir: Some(directory.to_string_lossy().into_owned()),
        },
        env,
        dir.join("run"),
    )
    .unwrap();
    // No wait at all, on a runtime that has not yet let the login's task see the cancel.
    let err = moved_waiting(&state, "claude-a", Duration::ZERO)
        .await
        .unwrap_err();
    assert_eq!(coded(&err).code, error_code::ACCOUNT_DIRECTORY_TRASH_FAILED);
    assert_eq!(coded(&err).params["cause"], SIGNING_IN);
    assert!(directory.join("settings.json").exists());
    assert!(state.store.get_account("claude-a").unwrap().is_some());
}

/// A fake `claude` that signs in for a long time, and the sign-in of `claude-a` it is started
/// with against `directory`.
fn sign_in_start(
    dir: &Path,
    directory: &Path,
) -> (
    crate::accounts::login::Start,
    std::collections::HashMap<String, String>,
) {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let env =
        crate::test_support::fake_agent(&bin, AgentId::CLAUDE.descriptor().binary, "sleep 30");
    let start = crate::accounts::login::Start {
        agent: AgentId::CLAUDE,
        account: Some("claude-a".to_string()),
        config_dir: Some(directory.to_string_lossy().into_owned()),
        resolved_dir: Some(directory.to_string_lossy().into_owned()),
    };
    (start, env)
}

/// No sign-in starts for an account whose removal is moving its directory to the Trash, which
/// would write into the directory after the sign-in was ended and before it moves; a change of
/// the account's sharing does not refuse one, and once the removal ends one starts again.
#[tokio::test]
async fn no_sign_in_starts_while_the_account_is_being_removed() {
    let (state, dir) = fixture("account-trash-login-refused");
    let directory = create_directory(&state);
    let start = || sign_in_start(&dir, &directory);
    let begin = |start: crate::accounts::login::Start, env| {
        crate::accounts::login::AccountLogins::start_for_test(&state, start, env, dir.join("run"))
    };

    // A change of the sharing does not refuse it.
    let (_, lock) = state
        .lock_account_sharing(&account(&state, "claude-a"), |_, _| Ok(()))
        .unwrap();
    let (first, env) = start();
    begin(first, env).unwrap();
    assert_eq!(state.account_logins.running().len(), 1);
    drop(lock);
    let ended = state
        .account_logins
        .end(AgentId::CLAUDE, "claude-a", PATIENCE)
        .await;
    assert!(ended);

    // A removal does, from its mark on.
    let removing = state.begin_account_removal(&account(&state, "claude-a"));
    let (second, env) = start();
    let err = begin(second, env).unwrap_err();
    assert_eq!(coded(&err).code, error_code::ACCOUNT_BEING_REMOVED);
    assert_eq!(coded(&err).params["account"], "claude-a");
    assert_eq!(coded(&err).params["name"], account(&state, "claude-a").name);
    assert!(state.account_logins.running().is_empty());
    drop(removing);
    let (third, env) = start();
    begin(third, env).unwrap();
    assert_eq!(state.account_logins.running().len(), 1);
}

/// Waits, bounded, for a removal to mark the account as being removed.
async fn wait_until_marked(state: &AppState) {
    let deadline = std::time::Instant::now() + PATIENCE;
    while state.account_being_removed("claude-a").is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "the removal marks the account"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Waits, bounded, for a move to hold `directory` against the CLIs' short reads.
async fn wait_until_held(directory: &Path) {
    let deadline = std::time::Instant::now() + PATIENCE;
    while !crate::accounts::shared_config::is_held(directory) {
        assert!(
            std::time::Instant::now() < deadline,
            "the move holds the directory"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// A removal that moves the directory marks the account for as long as it runs, however it
/// ends: here it waits for the agent's lock, then is refused.
#[tokio::test]
async fn the_removal_marks_the_account_until_it_ends() {
    let (state, dir) = fixture("account-trash-marked");
    let directory = create_directory(&state);
    let agent = crate::accounts::shared_config::lock_agent(&state, AgentId::CLAUDE).await;
    let removal = {
        let state = state.clone();
        tokio::spawn(async move { remove(&state, true).await })
    };
    wait_until_marked(&state).await;
    let (start, env) = sign_in_start(&dir, &directory);
    let err =
        crate::accounts::login::AccountLogins::start_for_test(&state, start, env, dir.join("run"))
            .unwrap_err();
    assert_eq!(coded(&err).code, error_code::ACCOUNT_BEING_REMOVED);
    let _here = run_on(&state, "here", AgentId::CLAUDE, "claude-a");
    drop(agent);
    let err = removal.await.unwrap().unwrap_err();
    assert_eq!(coded(&err).code, error_code::ACCOUNT_HAS_RUNNING_SESSIONS);
    assert!(state.account_being_removed("claude-a").is_none());
}

/// The removal judges the directory under the agent's lock, after waiting for it: whatever
/// held it may have changed another account.
#[tokio::test]
async fn a_removal_that_waits_for_the_agents_lock_judges_again() {
    let (state, _dir) = fixture("account-trash-lock-rejudged");
    let directory = create_directory(&state);
    let agent = crate::accounts::shared_config::lock_agent(&state, AgentId::CLAUDE).await;
    let removal = {
        let state = state.clone();
        tokio::spawn(async move { move_directory(&state, "claude-a").await })
    };
    wait_until_held(&directory).await;
    let mut other = account(&state, "grok-a");
    other.config_dir = directory.join("nested").to_string_lossy().into_owned();
    state.store.update_account(&other).unwrap();
    drop(agent);
    let err = removal.await.unwrap().unwrap_err();
    assert_eq!(coded(&err).code, error_code::ACCOUNT_DIRECTORY_SHARED);
    assert!(directory.exists());
}

/// A switch admitted before the removal locks its target can still record the session there.
/// The removal must count the switch's claim after acquiring the agent's lock, even before
/// the session's new process exists.
#[tokio::test]
async fn a_switch_recording_its_target_under_the_copy_lock_refuses_removal() {
    let (state, _dir) = fixture("account-trash-switch-entering");
    let directory = create_directory(&state);
    state
        .store
        .insert_session(&project_session(
            "switching",
            AgentId::CLAUDE,
            SessionStatus::Interrupted,
        ))
        .unwrap();
    let claim = state.begin_account_switch("switching", false).unwrap();
    let copying = crate::accounts::shared_config::before_switch_copy(
        &state,
        AgentId::CLAUDE,
        Some("claude-a"),
    )
    .await
    .unwrap();
    let removal = {
        let state = state.clone();
        tokio::spawn(async move { remove(&state, true).await })
    };
    wait_until_held(&directory).await;
    assert!(state.sharing_locked("claude-a").is_some());
    assert!(!removal.is_finished());

    state
        .set_session_account(
            "switching",
            Some("claude-a"),
            Some(&account(&state, "claude-a").config_dir),
        )
        .unwrap();
    drop(copying);
    let err = removal.await.unwrap().unwrap_err();
    assert_eq!(coded(&err).code, error_code::ACCOUNT_HAS_RUNNING_SESSIONS);
    assert_eq!(coded(&err).params["account"], "claude-a");
    assert_eq!(coded(&err).params["sessions"], "1");
    assert!(directory.join("settings.json").exists());
    assert!(state.store.get_account("claude-a").unwrap().is_some());
    assert!(state.live_session("switching").is_none());
    drop(claim);
}

/// A removal already holding the target account refuses a switch's copy before it writes
/// there; the pending short read keeps the removal at a known wait point.
#[tokio::test]
async fn the_switch_copy_guard_refuses_an_account_held_by_removal() {
    let (state, _dir) = fixture("account-trash-switch-refused");
    let directory = create_directory(&state);
    let read = crate::accounts::shared_config::reading_account(Some(&directory)).unwrap();
    let removal = {
        let state = state.clone();
        tokio::spawn(async move { remove(&state, true).await })
    };
    wait_until_held(&directory).await;
    assert!(state.sharing_locked("claude-a").is_some());
    assert!(!removal.is_finished());

    let err = crate::accounts::shared_config::before_switch_copy(
        &state,
        AgentId::CLAUDE,
        Some("claude-a"),
    )
    .await
    .unwrap_err();
    assert_eq!(coded(&err).code, error_code::ACCOUNT_SHARING_IN_PROGRESS);
    assert_eq!(coded(&err).params["account"], "claude-a");
    assert!(directory.join("settings.json").exists());
    drop(read);
    removal.await.unwrap().unwrap();
    assert!(!directory.exists());
    assert!(state.store.get_account("claude-a").unwrap().is_none());
}

/// The last judgement comes after the wait for the short reads: a project added inside the
/// directory meanwhile is caught.
#[tokio::test]
async fn a_project_added_while_the_removal_waits_for_reads_is_caught() {
    let (state, _dir) = fixture("account-trash-reads-rejudged");
    let directory = create_directory(&state);
    let read = crate::accounts::shared_config::reading_account(Some(&directory)).unwrap();
    let removal = {
        let state = state.clone();
        tokio::spawn(async move { move_directory(&state, "claude-a").await })
    };
    wait_until_held(&directory).await;
    let inside = directory.join("project");
    std::fs::create_dir_all(&inside).unwrap();
    crate::test_support::insert_project(&state, &inside);
    drop(read);
    let err = removal.await.unwrap().unwrap_err();
    assert_eq!(coded(&err).code, error_code::ACCOUNT_DIRECTORY_SHARED);
    assert!(directory.exists());
}

/// A directory found missing still has the account's sign-in ended first, and where it does
/// not end the removal is refused as for a directory that is there.
#[tokio::test]
async fn a_missing_directory_still_has_the_sign_in_ended() {
    let (state, dir) = fixture("account-trash-missing-login");
    let directory = PathBuf::from(account(&state, "claude-a").config_dir);
    assert!(!directory.exists());
    let (start, env) = sign_in_start(&dir, &directory);
    crate::accounts::login::AccountLogins::start_for_test(&state, start, env, dir.join("run"))
        .unwrap();
    let err = moved_waiting(&state, "claude-a", Duration::ZERO)
        .await
        .unwrap_err();
    assert_eq!(coded(&err).code, error_code::ACCOUNT_DIRECTORY_TRASH_FAILED);
    assert_eq!(coded(&err).params["cause"], SIGNING_IN);

    move_directory(&state, "claude-a").await.unwrap();
    assert!(state.account_logins.running().is_empty());
}

/// A short CLI read under the directory ends before the move, and none starts meanwhile.
#[tokio::test]
async fn the_move_waits_for_the_short_reads_under_the_directory() {
    let (state, _dir) = fixture("account-trash-reads");
    let directory = create_directory(&state);
    let read = crate::accounts::shared_config::reading_account(Some(&directory)).unwrap();
    let removal = {
        let state = state.clone();
        tokio::spawn(async move { move_directory(&state, "claude-a").await })
    };
    wait_until_held(&directory).await;
    assert!(!removal.is_finished(), "waits for the read");
    assert!(directory.exists());
    drop(read);
    removal.await.unwrap().unwrap();
    assert!(!directory.exists());
}

/// A listing of the account's models that waits behind the move runs nothing once the
/// directory is gone, which would create it again. Reaching the wait behind the hold is
/// best-effort (nothing signals it); a listing admitted after the hold ends takes the same
/// check, so the test cannot fail spuriously either way.
#[tokio::test]
async fn a_listing_waiting_for_the_move_runs_nothing_under_the_moved_directory() {
    let (state, dir) = fixture("account-trash-listing-waits");
    let directory = create_directory(&state);
    let ran = dir.join("ran");
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let env = crate::test_support::fake_agent(
        &bin,
        AgentId::CODEX.descriptor().binary,
        &format!("touch '{}'", ran.display()),
    );
    let read = crate::accounts::shared_config::reading_account(Some(&directory)).unwrap();
    let removal = {
        let state = state.clone();
        tokio::spawn(async move { move_directory(&state, "claude-a").await })
    };
    wait_until_held(&directory).await;
    let listing = {
        let directory = directory.clone();
        tokio::task::spawn_blocking(move || {
            crate::agents::models::list(
                AgentId::CODEX,
                &env,
                Some(&directory),
                Duration::from_secs(10),
            )
        })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(read);
    removal.await.unwrap().unwrap();
    let listing = listing.await.unwrap();
    assert!(listing.models.is_empty());
    assert_eq!(
        listing.unavailable.expect("a reason").code,
        crate::protocol::models_unavailable_code::COMMAND_FAILED
    );
    assert!(!ran.exists(), "the CLI was not run");
    assert!(!directory.exists());
}

async fn inspected(state: &Arc<AppState>) -> AccountTrashOutcome {
    match handle(
        state,
        Some("request-1".to_string()),
        RequestBody::InspectAccountTrash {
            account: "claude-a".to_string(),
        },
    )
    .await
    .unwrap()
    {
        Some(Event::AccountTrashInspected { id, outcome }) => {
            assert_eq!(id.as_deref(), Some("request-1"));
            outcome
        }
        other => panic!("expected account_trash_inspected, got {other:?}"),
    }
}

/// A directory that is there is `ok`, one that is not is `missing`, and a running session of
/// the account changes neither: the inspection leaves counting them to its client.
#[tokio::test]
async fn an_inspection_tells_a_directory_that_may_go_from_one_that_is_missing() {
    let (state, _dir) = fixture("account-trash-inspect-ok");
    assert_eq!(inspected(&state).await, AccountTrashOutcome::Missing);
    let directory = create_directory(&state);
    assert_eq!(inspected(&state).await, AccountTrashOutcome::Ok);
    let _here = run_on(&state, "here", AgentId::CLAUDE, "claude-a");
    assert_eq!(inspected(&state).await, AccountTrashOutcome::Ok);
    assert!(directory.join("settings.json").exists());
    assert!(state.store.get_account("claude-a").unwrap().is_some());
}

/// An unknown account is an error, as for any request about one.
#[tokio::test]
async fn an_inspection_of_an_unknown_account_is_an_error() {
    let (state, _dir) = fixture("account-trash-inspect-unknown");
    let err = handle(
        &state,
        None,
        RequestBody::InspectAccountTrash {
            account: "nobody".to_string(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(coded(&err).code, error_code::UNKNOWN_ACCOUNT);
}

/// A refusal is answered as data, the same one `delete_account` refuses with, and nothing is
/// changed.
#[tokio::test]
async fn an_inspection_answers_each_refusal_as_data() {
    // Another account's directory, an agent's default directory and a project's folder.
    let (state, _dir) = fixture("account-trash-inspect-shared");
    create_directory(&state);
    let mut other = account(&state, "codex-a");
    other.config_dir = account(&state, "claude-a").config_dir;
    state.store.update_account(&other).unwrap();
    assert_eq!(
        inspected(&state).await,
        AccountTrashOutcome::Shared {
            kind: AccountTrashSharing::Account,
            name: Some("codex-a".to_string()),
            agent: Some("Codex".to_string()),
        }
    );
    let err = remove(&state, true).await.unwrap_err();
    assert_eq!(coded(&err).code, error_code::ACCOUNT_DIRECTORY_SHARED);
    assert_eq!(coded(&err).params["kind"], "account");

    let (state, dir) = fixture("account-trash-inspect-default");
    let mut mine = account(&state, "claude-a");
    mine.config_dir = default_of(&dir, AgentId::CODEX)
        .to_string_lossy()
        .into_owned();
    std::fs::create_dir_all(&mine.config_dir).unwrap();
    state.store.update_account(&mine).unwrap();
    assert_eq!(
        inspected(&state).await,
        AccountTrashOutcome::Shared {
            kind: AccountTrashSharing::DefaultAccount,
            name: None,
            agent: Some("Codex".to_string()),
        }
    );

    let (state, dir) = fixture("account-trash-inspect-project");
    let held = dir.join("holds-project");
    let mut project = state.store.list_projects().unwrap()[0].clone();
    project.id = "held".to_string();
    project.name = "Held".to_string();
    project.path = held.join("work").to_string_lossy().into_owned();
    std::fs::create_dir_all(&project.path).unwrap();
    state.store.insert_project(&project).unwrap();
    let mut mine = account(&state, "claude-a");
    mine.config_dir = held.to_string_lossy().into_owned();
    state.store.update_account(&mine).unwrap();
    assert_eq!(
        inspected(&state).await,
        AccountTrashOutcome::Shared {
            kind: AccountTrashSharing::Project,
            name: Some("Held".to_string()),
            agent: None,
        }
    );
}

/// The root of a disk is protected, and the removal refuses it the same way.
#[tokio::test]
async fn an_inspection_answers_a_disks_root_as_protected() {
    let (state, _dir) = fixture("account-trash-inspect-protected");
    let mut mine = account(&state, "claude-a");
    mine.config_dir = crate::test_support::disk_root()
        .to_string_lossy()
        .into_owned();
    state.store.update_account(&mine).unwrap();
    assert_eq!(inspected(&state).await, AccountTrashOutcome::Protected);
    let err = remove(&state, true).await.unwrap_err();
    assert_eq!(coded(&err).code, error_code::ACCOUNT_DIRECTORY_PROTECTED);
}
