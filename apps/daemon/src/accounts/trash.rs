//! Moving a removed account's configuration directory to the Trash: which directory may go, and
//! when. Removing an account with this option is refused, with the account kept and nothing
//! changed, while any of its sessions has a process or is being launched, since the directory
//! holds what they run on, and for a directory whose loss would be drastic or would take files
//! that belong to something else: the home directory, a disk's root, Octoboard's own data, an
//! agent's default configuration directory, another account's directory, or a registered project's
//! folder inside it (`trash_safety` tells directories apart, by device and inode). A path that
//! cannot be looked at refuses too, as a failed move: the directory itself, the home directory,
//! Octoboard's data directory, or another account's, an agent's default or a project's path that
//! decides whether the directory holds or overlaps it. `inspect_account_trash` answers the
//! judgement of the directory beforehand, as data, so a client can offer the option only where it
//! can be carried out; the removal itself judges again under the account's lock.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use crate::accounts::shared_config::runs_under;
use crate::hostfs::place::{Looking, Stuck};
use crate::protocol::{
    error_code, Account, AccountTrashOutcome, AccountTrashSharing, Agent, CodedError,
};
use crate::state::AppState;
use crate::trash_safety::{
    looked_at_for_trash, Blocked, Failure, Overlap, Unresolved, TRASH_LOOK_DEADLINE,
};

/// The `cause` of an `account_directory_trash_failed` where whether the directory holds or overlaps
/// another account's, an agent's default or a project's directory could not be told.
const OTHER_UNLOOKED: &str = "other_unlooked";

/// The `cause` of an `account_directory_trash_failed` where the account's sign-in did not end in
/// time, so it could still write into the directory.
const SIGNING_IN: &str = "signing_in";

/// What a path in the way of the move belongs to, for the refusal that names it.
enum Way {
    Account(Account),
    DefaultAccount(Agent),
    Project(String),
}

impl Way {
    /// What the path is, for the detail of a refusal that could not look at it.
    fn described(&self) -> String {
        match self {
            Way::Account(other) => format!(
                "the directory of the {} account `{}`",
                other.agent.label(),
                other.name
            ),
            Way::DefaultAccount(agent) => {
                format!("{}'s default configuration directory", agent.label())
            }
            Way::Project(name) => format!("the folder of project \"{name}\""),
        }
    }
}

/// How long `inspect_account_trash` waits for a look at one path: it runs as the option is ticked,
/// so anything slower is left undecided to `delete_account`, which waits [`TRASH_LOOK_DEADLINE`].
const INSPECTION_LOOK_DEADLINE: Duration = Duration::from_millis(500);

/// How long removing an account waits for its sign-in to end before refusing to move the directory
/// the sign-in writes into.
const LOGIN_END_WAIT: Duration = Duration::from_secs(5);

/// A quick look ahead of the removal: a path still stuck from an earlier look is left undecided,
/// and one that is only slow is not recorded as stuck, since other work would then skip it.
const INSPECTING: Looking = Looking {
    deadline: INSPECTION_LOOK_DEADLINE,
    stuck_after: TRASH_LOOK_DEADLINE,
    stuck: Stuck::Skip,
};

/// The look of the removal itself, which waits for what an inspection left stuck, for what is left
/// of its time.
const CONFIRMING: Looking = Looking {
    deadline: TRASH_LOOK_DEADLINE,
    stuck_after: TRASH_LOOK_DEADLINE,
    stuck: Stuck::Wait,
};

/// Refuses while any of account `id`'s sessions has a process or is being launched, resumed or
/// switched (`account_has_running_sessions`).
fn refuse_running_sessions(state: &AppState, id: &str) -> Result<()> {
    let account = state.store.require_account(id)?;
    let running = state
        .running_sessions()?
        .iter()
        .filter(|running| runs_under(&account, &running.session))
        .count();
    if running == 0 {
        return Ok(());
    }
    Err(CodedError::raised(
        error_code::ACCOUNT_HAS_RUNNING_SESSIONS,
        format!(
            "account `{}` still has running or starting sessions; end them or switch them to \
             another account first",
            account.name
        ),
        &[
            ("account", &account.id),
            ("name", &account.name),
            ("sessions", &running.to_string()),
        ],
    ))
}

/// The directory to move to the Trash for removing account `id`, or `None` when nothing is at its
/// path, for the check a removal makes before it changes anything, sharing turned off included.
/// Refused as [`refuse_running_sessions`] and [`judged`] say.
pub async fn directory_to_trash(state: &Arc<AppState>, id: &str) -> Result<Option<PathBuf>> {
    refuse_running_sessions(state, id)?;
    judged_now(state, id, CONFIRMING)
        .await?
        .map_err(|refusal| refusal.raised())
}

/// What removing account `id` with its directory moved to the Trash would be let or refused now,
/// as [`judged`] decides it within [`INSPECTION_LOOK_DEADLINE`], for `inspect_account_trash`. A
/// look that outruns it, or a path still stuck from one, is [`AccountTrashOutcome::Undecided`],
/// left to `delete_account`. Running sessions are not counted: a client lists them itself, and
/// `delete_account` refuses them as [`refuse_running_sessions`] says.
pub async fn inspect(state: &Arc<AppState>, id: &str) -> Result<AccountTrashOutcome> {
    Ok(match judged_now(state, id, INSPECTING).await? {
        Ok(Some(_)) => AccountTrashOutcome::Ok,
        Ok(None) => AccountTrashOutcome::Missing,
        Err(refusal) => refusal.outcome(),
    })
}

/// [`judged`] for account `id` as every account, default directory and project stand now.
async fn judged_now(
    state: &Arc<AppState>,
    id: &str,
    looking: Looking,
) -> Result<std::result::Result<Option<PathBuf>, Refusal>> {
    let account = state.store.require_account(id)?;
    let mut defaults = Vec::new();
    for agent in Agent::ALL {
        defaults.push((
            agent,
            crate::accounts::shared_config::default_dir(state, agent).await?,
        ));
    }
    Ok(judged(
        state,
        looking,
        &account,
        &state.store.list_accounts()?,
        &defaults,
        &state.store.list_projects()?,
        crate::paths::known_home_dir(),
        crate::paths::data_dir(),
    )
    .await)
}

/// Why the directory of an account is not let go to the Trash: what [`judged`] refuses with, as
/// the error `delete_account` answers with ([`Refusal::raised`]) or as the data
/// `inspect_account_trash` answers with ([`Refusal::outcome`]).
enum Refusal {
    Protected {
        path: String,
    },
    Shared(Way),
    /// A path that decides it could not be looked at, for `failure`. `cause` is the one the
    /// refusal carries where there is one.
    Unlooked {
        path: String,
        detail: String,
        cause: Option<&'static str>,
        failure: Failure,
    },
}

impl Refusal {
    fn raised(self) -> anyhow::Error {
        match self {
            Refusal::Unlooked {
                path,
                detail,
                cause,
                ..
            } => trash_failed(&path, &detail, cause),
            Refusal::Protected { path } => CodedError::raised(
                error_code::ACCOUNT_DIRECTORY_PROTECTED,
                "this directory is too important to move to the Trash from here",
                &[("path", &path)],
            ),
            Refusal::Shared(Way::Account(other)) => CodedError::raised(
                error_code::ACCOUNT_DIRECTORY_SHARED,
                format!(
                    "the directory is shared with the {} account `{}`",
                    other.agent.label(),
                    other.name
                ),
                &[
                    ("kind", "account"),
                    ("name", &other.name),
                    ("agent", other.agent.label()),
                ],
            ),
            Refusal::Shared(Way::DefaultAccount(agent)) => CodedError::raised(
                error_code::ACCOUNT_DIRECTORY_SHARED,
                format!(
                    "the directory is, holds or lies inside {}'s default configuration directory",
                    agent.label()
                ),
                &[("kind", "default_account"), ("agent", agent.label())],
            ),
            Refusal::Shared(Way::Project(name)) => CodedError::raised(
                error_code::ACCOUNT_DIRECTORY_SHARED,
                format!("the directory holds the folder of project \"{name}\""),
                &[("kind", "project"), ("name", &name)],
            ),
        }
    }

    fn outcome(self) -> AccountTrashOutcome {
        match self {
            Refusal::Protected { .. } => AccountTrashOutcome::Protected,
            Refusal::Shared(Way::Account(other)) => AccountTrashOutcome::Shared {
                kind: AccountTrashSharing::Account,
                name: Some(other.name),
                agent: Some(other.agent.label().to_string()),
            },
            Refusal::Shared(Way::DefaultAccount(agent)) => AccountTrashOutcome::Shared {
                kind: AccountTrashSharing::DefaultAccount,
                name: None,
                agent: Some(agent.label().to_string()),
            },
            Refusal::Shared(Way::Project(name)) => AccountTrashOutcome::Shared {
                kind: AccountTrashSharing::Project,
                name: Some(name),
                agent: None,
            },
            // A look that did not answer in time is not a verdict.
            Refusal::Unlooked {
                failure: Failure::Unanswered,
                ..
            } => AccountTrashOutcome::Undecided,
            Refusal::Unlooked { cause, .. } => AccountTrashOutcome::Unlookable {
                cause: cause.map(str::to_string),
            },
        }
    }
}

/// The directory to move to the Trash for `account`, or `None` when nothing is at its path, with
/// `accounts` every account, `defaults` each agent's default configuration directory, `projects`
/// every project, and `home` and `data` the home directory and Octoboard's data directory.
///
/// Refused as [`Refusal::Protected`] (`account_directory_protected`) for the home directory, the
/// root of a disk or a directory holding Octoboard's data, and as [`Refusal::Shared`]
/// (`account_directory_shared`) while it is, holds or lies inside an agent's default directory or
/// another account's directory, or holds a project's folder. A path that cannot be looked at is
/// refused as [`Refusal::Unlooked`] (`account_directory_trash_failed`): the directory itself, the
/// home directory or Octoboard's data directory with the `cause` of its failure, and another
/// account's, an agent's default or a project's path with the `cause` [`OTHER_UNLOOKED`], since
/// naming it as in the way would say it holds what it may not.
#[allow(clippy::too_many_arguments)]
async fn judged(
    state: &AppState,
    looking: Looking,
    account: &Account,
    accounts: &[Account],
    defaults: &[(Agent, PathBuf)],
    projects: &[crate::protocol::Project],
    home: Option<PathBuf>,
    data: PathBuf,
) -> std::result::Result<Option<PathBuf>, Refusal> {
    let written = PathBuf::from(&account.config_dir);
    let mut ways = Vec::new();
    let mut others = Vec::new();
    for other in accounts.iter().filter(|other| other.id != account.id) {
        others.push((PathBuf::from(&other.config_dir), Overlap::Nested));
        ways.push(Way::Account(other.clone()));
    }
    for (agent, dir) in defaults {
        others.push((dir.clone(), Overlap::Nested));
        ways.push(Way::DefaultAccount(*agent));
    }
    for project in projects {
        others.push((PathBuf::from(&project.path), Overlap::Inside));
        ways.push(Way::Project(project.name.clone()));
    }
    let path = account.config_dir.clone();
    match looked_at_for_trash(state, looking, &written, &others, home, data).await {
        // A symlink is trashed as itself, not as what it points to.
        Ok(directory) => Ok(directory),
        Err(Blocked::Unlooked(what, failure)) => {
            let (failed, cause) = failure.described();
            let detail = match what {
                Unresolved::Folder => format!(
                    "it {failed}, so whether files of another account or a project are in it \
                     could not be told"
                ),
                Unresolved::Home => {
                    format!("the home folder {failed}, so whether this is it could not be told")
                }
                Unresolved::Data => format!(
                    "Octoboard's data folder {failed}, so whether it is in this directory could \
                     not be told"
                ),
            };
            Err(Refusal::Unlooked {
                path,
                detail,
                cause,
                failure,
            })
        }
        Err(Blocked::Protected) => Err(Refusal::Protected { path }),
        Err(Blocked::InTheWay {
            index,
            unresolved: Some(failure),
        }) => {
            let (failed, _) = failure.described();
            let detail = format!(
                "{} {failed}, so whether this directory holds or overlaps it could not be told",
                ways[index].described()
            );
            Err(Refusal::Unlooked {
                path,
                detail,
                cause: Some(OTHER_UNLOOKED),
                failure,
            })
        }
        Err(Blocked::InTheWay {
            index,
            unresolved: None,
        }) => Err(Refusal::Shared(ways.swap_remove(index))),
    }
}

/// Moves the directory of account `id` to the Trash, for a removal that holds the account's lock
/// (so it cannot be repointed or start a session from here on) and has it marked as being removed
/// (so no sign-in starts), judging it again now: the early judgement of [`directory_to_trash`] was
/// made before the lock, and the account may have been repointed, or the file system changed,
/// since. A failure is [`ACCOUNT_DIRECTORY_TRASH_FAILED`] or one of [`directory_to_trash`]'s
/// refusals, and the account is kept.
///
/// The account's sign-in is ended first, and waited for, since one still writing into the
/// directory would recreate it, and also where the directory is found missing, so that it cannot
/// appear afterwards. Then the directory is held against the agent CLIs' short reads, which would
/// recreate it at the old path, and those running are waited for. Last, the agent's lock is taken
/// and the directory judged once more, after every wait. The lock is held from the sessions' check
/// to the end of the move: an account switch copying a conversation into the account holds it
/// until it has recorded the session there, so the switch has finished and is counted, or is
/// refused.
///
/// [`ACCOUNT_DIRECTORY_TRASH_FAILED`]: error_code::ACCOUNT_DIRECTORY_TRASH_FAILED
pub async fn move_directory(state: &Arc<AppState>, id: &str) -> Result<()> {
    moved_waiting(state, id, LOGIN_END_WAIT).await
}

/// The refusal of a move of `path` that failed for `detail`, with the `cause` where there is one.
fn trash_failed(path: &str, detail: &str, cause: Option<&str>) -> anyhow::Error {
    let mut pairs = vec![("path", path), ("detail", detail)];
    pairs.extend(cause.map(|cause| ("cause", cause)));
    CodedError::raised(
        error_code::ACCOUNT_DIRECTORY_TRASH_FAILED,
        format!("could not move {path} to the Trash: {detail}"),
        &pairs,
    )
}

/// [`move_directory`], waiting `login_wait` for the sign-in to end.
async fn moved_waiting(state: &Arc<AppState>, id: &str, login_wait: Duration) -> Result<()> {
    let account = state.store.require_account(id)?;
    let judged = judged_now(state, id, CONFIRMING)
        .await?
        .map_err(|refusal| refusal.raised())?;
    let shown = judged.as_ref().map_or_else(
        || account.config_dir.clone(),
        |directory| directory.to_string_lossy().into_owned(),
    );
    if !state
        .account_logins
        .end(account.agent, id, login_wait)
        .await
    {
        return Err(trash_failed(
            &shown,
            "its sign-in did not end in time",
            Some(SIGNING_IN),
        ));
    }
    if judged.is_none() {
        return Ok(());
    }
    // No short CLI read starts under the directory from here on, and those running end first:
    // one would recreate the directory at its old path once it has moved. Waited for before the
    // last judgement, since a read can take a minute and the file system, other accounts or
    // projects may change meanwhile. Refused only while a long run is going, which is a sign-in
    // the end of this account's did not cover (another account's, pointed at the same directory).
    let Some(hold) = crate::accounts::shared_config::hold(&PathBuf::from(&account.config_dir))
    else {
        return Err(trash_failed(
            &shown,
            "a sign-in is running there",
            Some(SIGNING_IN),
        ));
    };
    let waiter = hold.waiter();
    tokio::task::spawn_blocking(move || waiter.wait_for_reads())
        .await
        .map_err(|join| trash_failed(&shown, &join.to_string(), None))?;
    let _agent = crate::accounts::shared_config::lock_agent(state, account.agent).await;
    // Judged under the lock and after every wait, right before the move.
    let Some(directory) = judged_now(state, id, CONFIRMING)
        .await?
        .map_err(|refusal| refusal.raised())?
    else {
        return Ok(());
    };
    let shown = directory.to_string_lossy().into_owned();
    refuse_running_sessions(state, id)?;
    let trash = state.host_tools.get().trash;
    // Not bounded by a timeout: a move given up on could still complete later, with the account
    // kept.
    let moved = match tokio::task::spawn_blocking(move || trash(&directory)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(failure)) => Err(trash_failed(&shown, &failure.detail, None)),
        Err(join) => Err(trash_failed(&shown, &join.to_string(), None)),
    };
    drop(hold);
    moved
}

/// The refusal of a sign-in for account `account`, whose removal is moving its directory to the
/// Trash (`account_being_removed`).
pub(crate) fn being_removed(account: &Account) -> anyhow::Error {
    CodedError::raised(
        error_code::ACCOUNT_BEING_REMOVED,
        format!(
            "account `{}` is being removed; it cannot be signed in meanwhile",
            account.name
        ),
        &[("account", &account.id), ("name", &account.name)],
    )
}

#[cfg(test)]
mod tests;
