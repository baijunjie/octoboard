//! An account sharing its agent's default configuration: each account keeps the choice and
//! whether its directory has been brought in line with it yet (`Account::share_default_config`
//! and `Account::share_default_config_pending`), and this module brings the directory in line.
//!
//! A change is recorded whenever it is asked for and carried out only while nothing runs under
//! the account's directory — none of its sessions, and no agent CLI reading its quota or models —
//! since a running agent holds files the change moves: at once when nothing does, otherwise
//! before the account's next session starts or resumes (an account switch to it included) at a
//! moment nothing else does. Before every launch of a session of a sharing account, the links an
//! agent replaced are repaired. What the directory operations do is `link.rs`'s; the whole
//! contract is the "Sharing the default configuration" list of `apps/daemon/PROTOCOL.md`.

mod cli_reads;
mod codex_state;
mod link;
mod moves;
mod registries;
mod take_back;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::MutexGuard;

use crate::account_dirs::account_dir_key;
use crate::protocol::{error_code, notice_code, Account, Agent, CodedError, Notice, Session};
use crate::state::AppState;
use crate::store::SharingStart;
use link::{DirsError, Failure, Left};

pub(crate) use cli_reads::reading_account;

/// One lock per agent over carrying out and repairing the sharing of its accounts, and over an
/// account switch's copy into an account: every account of an agent writes into the same default
/// directory, and none of these may work on a directory another is changing.
#[derive(Default)]
pub struct Locks {
    claude: tokio::sync::Mutex<()>,
    codex: tokio::sync::Mutex<()>,
    grok: tokio::sync::Mutex<()>,
}

impl Locks {
    async fn lock(&self, agent: Agent) -> MutexGuard<'_, ()> {
        match agent {
            Agent::Claude => self.claude.lock().await,
            Agent::Codex => self.codex.lock().await,
            Agent::Grok => self.grok.lock().await,
        }
    }
}

/// Records `share` as the account's choice and carries it out if nothing runs under the account's
/// directory; `account` is the account's id, and `None` names the default account, which has no
/// choice to make, being the directory the others share. Asking again for the choice already
/// recorded retries a change still pending. Refused, with nothing recorded, for the default
/// account and for an account whose directory is the default account's under another spelling.
/// A change that fails part-way stays pending and is answered with what it left where.
pub async fn set(state: &Arc<AppState>, account: Option<String>, share: bool) -> Result<()> {
    let Some(id) = account else {
        return Err(not_shareable(None));
    };
    let agent = state
        .store
        .get_account(&id)?
        .ok_or_else(|| CodedError::unknown_account(&id))?
        .agent;
    let _guard = state.account_sharing.lock(agent).await;
    let mut account = state
        .store
        .get_account(&id)?
        .ok_or_else(|| CodedError::unknown_account(&id))?;
    if account.share_default_config != share {
        if share {
            let default = default_dir(state, agent).await?;
            let dir = PathBuf::from(&account.config_dir);
            if tokio::task::spawn_blocking(move || is_same_directory(&dir, &default)).await? {
                return Err(not_shareable(Some(&account)));
            }
        }
        account.share_default_config = share;
        account.share_default_config_pending = true;
        state.store.set_account_sharing(&id, share, true)?;
        crate::coordinator::broadcast_settings(state)?;
    }
    if account.share_default_config_pending {
        carry_out(state, &account, None).await?;
    }
    Ok(())
}

/// What a launch of a session takes from its account's sharing.
#[derive(Default)]
pub struct Prepared {
    /// The default account's directory, fully resolved, when the session's account shares it.
    pub shared_default_dir: Option<PathBuf>,
    /// What the launch has to tell the user about the account's directory.
    pub notices: Vec<Notice>,
}

/// Before `session` starts or resumes: carries out its account's pending change when nothing
/// else runs under the account's directory, refusing the launch when that fails. While the
/// directory shares, the launch is told the shared directory, and, unless sharing is being turned
/// off, the directory is repaired first. A repair that cannot fix something does not hold the
/// launch up; it is told as a notice. A session that holds another directory than its account's
/// (the account was repointed after it was opened) takes nothing from the account.
pub async fn before_launch(state: &Arc<AppState>, session: &Session) -> Result<Prepared> {
    let Some(account) = account_of(state, session)? else {
        return Ok(Prepared::default());
    };
    if !account.share_default_config && !account.share_default_config_pending {
        return Ok(Prepared::default());
    }
    let _guard = state.account_sharing.lock(account.agent).await;
    let Some(mut account) = account_of(state, session)? else {
        return Ok(Prepared::default());
    };
    if account.share_default_config_pending && carry_out(state, &account, Some(&session.id)).await?
    {
        account.share_default_config_pending = false;
    }
    // A change still waiting has not touched the directory, which shares as long as it did.
    if account.share_default_config == account.share_default_config_pending {
        return Ok(Prepared::default());
    }
    let Some(start) = state.store.account_sharing_start(&account.id)? else {
        return Ok(Prepared::default());
    };
    let current = default_dir(state, account.agent).await?;
    let (agent, dir) = (account.agent, PathBuf::from(&account.config_dir));
    let turning_off = account.share_default_config_pending;
    let checked =
        tokio::task::spawn_blocking(move || {
            let recorded = moves::resolve_or_spelled(Path::new(&start.default_dir));
            if turning_off {
                // Sharing is being turned off: the links stay as they are until it is.
                return Repair::Skipped(recorded);
            }
            if moves::resolve_or_spelled(&current) != recorded {
                return Repair::DefaultMoved { recorded, current };
            }
            Repair::Done(link::Dirs::new(&dir, &recorded, false).map(|dirs| {
                dirs.map(|dirs| (link::repair(agent, &dirs, &start.stamp), dirs.default))
            }))
        })
        .await?;
    let (shared_default_dir, notices) = repair_notices(&account, checked);
    Ok(Prepared {
        shared_default_dir,
        notices,
    })
}

/// What became of the repair before a launch.
enum Repair {
    /// Not attempted, the links leading into this directory.
    Skipped(PathBuf),
    /// Not attempted: the agent's default directory is no longer the one the links were made into.
    DefaultMoved {
        recorded: PathBuf,
        current: PathBuf,
    },
    Done(Result<Option<(link::Repaired, PathBuf)>, DirsError>),
}

/// What a repair of `account`'s directory comes to for the launch: the shared directory, and a
/// notice for each thing it could not fix or must not let pass.
fn repair_notices(account: &Account, repair: Repair) -> (Option<PathBuf>, Vec<Notice>) {
    let repair_failed = |message: String, failures: &[Failure]| {
        Notice::new(
            notice_code::ACCOUNT_SHARING_REPAIR_FAILED,
            message,
            &[
                ("account", &account.id),
                ("name", &account.name),
                ("items", &items_param(failures)),
            ],
        )
    };
    let repaired = match repair {
        Repair::Skipped(default) => return (Some(default), Vec::new()),
        Repair::DefaultMoved { recorded, current } => {
            let message = format!(
                "account `{}` shares `{}`, but the default configuration is now `{}`; its links \
                 were not repaired. Turn its sharing off and on again to share the new one",
                account.name,
                recorded.display(),
                current.display()
            );
            tracing::warn!(account = %account.id, %message, "a sharing account's default directory moved");
            return (Some(recorded), vec![repair_failed(message, &[])]);
        }
        Repair::Done(repaired) => repaired,
    };
    let (repaired, default) = match repaired {
        Ok(Some(repaired)) => repaired,
        Ok(None) => return (None, Vec::new()),
        Err(err) => {
            let detail = match err {
                DirsError::SameAsDefault => "it is the default account's own directory".into(),
                DirsError::Io(err) => err.to_string(),
            };
            tracing::warn!(account = %account.id, %detail, "checking a sharing account's links failed");
            let message = format!(
                "account `{}`'s links to the default configuration could not be checked: {detail}",
                account.name
            );
            return (None, vec![repair_failed(message, &[])]);
        }
    };
    let mut notices = Vec::new();
    if !repaired.failures.is_empty() {
        tracing::warn!(account = %account.id, failures = ?repaired.failures, "repairing a sharing account's links failed");
        let message = format!(
            "some of account `{}`'s links to the default configuration could not be repaired: {}",
            account.name,
            describe(&repaired.failures)
        );
        notices.push(repair_failed(message, &repaired.failures));
    }
    for database in &repaired.unshared_databases {
        tracing::warn!(account = %account.id, database = %database.display(), "a Codex state database the shared configuration does not cover");
        notices.push(Notice::new(
            notice_code::UNSHARED_STATE_DATABASE,
            format!(
                "`{}` is a Codex state database the default configuration does not share, so \
                 this account keeps its threads apart from the others",
                database.display()
            ),
            &[
                ("account", &account.id),
                ("name", &account.name),
                ("path", &database.to_string_lossy()),
            ],
        ));
    }
    (Some(default), notices)
}

/// For an account switch of `session` to the account `target` (`None` for the default account):
/// carries out the target's pending change, refusing the switch when that fails. The switch asks
/// before it ends the session's process, so a refusal leaves the session running, and again
/// before it copies the conversation in, in case the change came back meanwhile, so the copy
/// lands where it stays. The answer is the agent's lock, for the switch to hold until it has
/// recorded the session on the target: no change of any account's sharing may move the files
/// the copy writes meanwhile.
pub async fn before_switch_copy<'a>(
    state: &'a Arc<AppState>,
    agent: Agent,
    target: Option<&str>,
    session: &str,
) -> Result<MutexGuard<'a, ()>> {
    let guard = state.account_sharing.lock(agent).await;
    if let Some(account) = target
        .map(|id| state.store.get_account(id))
        .transpose()?
        .flatten()
    {
        if account.share_default_config_pending {
            carry_out(state, &account, Some(session)).await?;
        }
    }
    Ok(guard)
}

/// Before account `id` is deleted: an account that shares, or has a change pending, has its
/// sharing turned off first, so its directory is not left linked into the default one with
/// nothing to undo it. Refused while anything runs under the directory (`account_still_sharing`)
/// and when turning it off fails; the answer is the agent's lock, for the deletion to hold while
/// it removes the account.
pub async fn before_delete<'a>(
    state: &'a Arc<AppState>,
    id: &str,
) -> Result<Option<MutexGuard<'a, ()>>> {
    turn_off_first(state, id, "remove it").await
}

/// Before account `id` is pointed at `config_dir`: as [`before_delete`], when the directory
/// changes, the sharing being undone in the directory it was carried out on.
pub async fn before_repoint<'a>(
    state: &'a Arc<AppState>,
    id: &str,
    config_dir: &str,
) -> Result<Option<MutexGuard<'a, ()>>> {
    let unchanged = state
        .store
        .get_account(id)?
        .is_none_or(|account| account_dir_key(&account.config_dir) == account_dir_key(config_dir));
    if unchanged {
        return Ok(None);
    }
    turn_off_first(state, id, "point it at another directory").await
}

async fn turn_off_first<'a>(
    state: &'a Arc<AppState>,
    id: &str,
    then: &str,
) -> Result<Option<MutexGuard<'a, ()>>> {
    let Some(account) = state.store.get_account(id)? else {
        return Ok(None);
    };
    let guard = state.account_sharing.lock(account.agent).await;
    let Some(account) = state.store.get_account(id)? else {
        return Ok(Some(guard));
    };
    if !account.share_default_config && !account.share_default_config_pending {
        return Ok(Some(guard));
    }
    let still_sharing = || {
        CodedError::raised(
            error_code::ACCOUNT_STILL_SHARING,
            format!(
                "account `{}` shares the default configuration and something is running under \
                 it (one of its sessions, or its agent's CLI reading its quota or models or \
                 signing it in); turn its sharing off once nothing does, then {then}",
                account.name
            ),
            &[("account", &account.id), ("name", &account.name)],
        )
    };
    if is_in_use(state, &account, None)? {
        return Err(still_sharing());
    }
    let off = Account {
        share_default_config: false,
        share_default_config_pending: true,
        ..account.clone()
    };
    state.store.set_account_sharing(id, false, true)?;
    match carry_out(state, &off, None).await {
        Ok(true) => Ok(Some(guard)),
        Ok(false) => {
            // Something started under the directory after all: the choice is as it was.
            state.store.set_account_sharing(
                id,
                account.share_default_config,
                account.share_default_config_pending,
            )?;
            Err(still_sharing())
        }
        Err(err) => {
            crate::coordinator::broadcast_settings(state)?;
            Err(err)
        }
    }
}

/// The account `session` launches under, when the session holds that account's directory.
fn account_of(state: &AppState, session: &Session) -> Result<Option<Account>> {
    let Some(id) = session.account_id.as_deref() else {
        return Ok(None);
    };
    let Some(account) = state.store.get_account(id)? else {
        return Ok(None);
    };
    let holds_it = session
        .config_dir
        .as_deref()
        .is_some_and(|dir| account_dir_key(dir) == account_dir_key(&account.config_dir));
    Ok((account.agent == session.agent && holds_it).then_some(account))
}

/// Whether something runs under `account`'s directory: one of its sessions other than
/// `launching`, running or being started, or an agent CLI reading the account's quota or models.
fn is_in_use(state: &AppState, account: &Account, launching: Option<&str>) -> Result<bool> {
    let key = account_dir_key(&account.config_dir);
    let sessions = state.has_live_sessions_where(|session| {
        Some(session.id.as_str()) != launching
            && session.agent == account.agent
            && (session.account_id.as_deref() == Some(&account.id)
                || session
                    .config_dir
                    .as_deref()
                    .is_some_and(|dir| account_dir_key(dir) == key))
    })?;
    Ok(sessions || cli_reads::is_reading(Path::new(&account.config_dir)))
}

/// Carries out `account`'s pending change unless something other than the session `launching`
/// runs under its directory; answers whether it did. The caller holds the agent's lock.
///
/// Turning sharing on records when it starts (the stamp its backups are named with, and the
/// default directory its links lead into) before touching anything, so a retry backs up under
/// the same names and turning it off restores exactly those. A retry into another default
/// directory than the recorded one is refused: the links already made lead into that one.
/// Turning it off works from that record: with none, nothing was ever linked and there is nothing
/// to undo; with one, the links are recognised by where that default directory is, even when the
/// default has moved since or is gone. The outcome is recorded only for the directory it was
/// carried out on: an account repointed meanwhile stays pending, for its new directory.
async fn carry_out(
    state: &Arc<AppState>,
    account: &Account,
    launching: Option<&str>,
) -> Result<bool> {
    if is_in_use(state, account, launching)? {
        return Ok(false);
    }
    let dir = PathBuf::from(&account.config_dir);
    let Some(_changing) = cli_reads::begin_change(&dir) else {
        return Ok(false);
    };
    let share = account.share_default_config;
    let start = state.store.account_sharing_start(&account.id)?;
    let agent = account.agent;
    let current = default_dir(state, agent).await?;
    let outcome = if share {
        let start = match start {
            Some(start) => start,
            None => {
                let resolved = {
                    let current = current.clone();
                    tokio::task::spawn_blocking(move || moves::resolve_or_spelled(&current)).await?
                };
                let start = SharingStart {
                    stamp: moves::new_stamp(),
                    default_dir: resolved.to_string_lossy().into_owned(),
                    completed: false,
                };
                if !state
                    .store
                    .start_account_sharing(&account.id, &account.config_dir, &start)?
                {
                    return Ok(false);
                }
                start
            }
        };
        tokio::task::spawn_blocking(move || {
            let recorded = moves::resolve_or_spelled(Path::new(&start.default_dir));
            if moves::resolve_or_spelled(&current) != recorded {
                return Err(DirsError::Io(std::io::Error::other(format!(
                    "the default configuration is now `{}`, not `{}`, the one sharing was \
                     started into; turn sharing off, then on again",
                    current.display(),
                    recorded.display()
                ))));
            }
            let dirs = link::Dirs::new(&dir, &recorded, true)?;
            Ok(dirs.map_or_else(Vec::new, |dirs| {
                link::turn_on(agent, &dirs, &start.stamp, start.completed)
            }))
        })
        .await?
    } else {
        match start {
            None => Ok(Vec::new()),
            Some(start) => {
                let conversations = take_back::own_conversations(state, account)?;
                tokio::task::spawn_blocking(move || {
                    match link::Dirs::for_undoing(&dir, Path::new(&start.default_dir), &current) {
                        // The directory never linked anything to itself: nothing to undo.
                        Err(DirsError::SameAsDefault) | Ok(None) => Ok(Vec::new()),
                        Err(err) => Err(err),
                        Ok(Some(dirs)) => {
                            Ok(link::turn_off(agent, &dirs, &start.stamp, &conversations))
                        }
                    }
                })
                .await?
            }
        }
    };
    let failures = match outcome {
        Ok(failures) => failures,
        Err(DirsError::SameAsDefault) => return Err(not_shareable(Some(account))),
        Err(DirsError::Io(err)) => return Err(sharing_failed(account, &[], &err.to_string())),
    };
    if !failures.is_empty() {
        tracing::warn!(account = %account.id, ?failures, "carrying out an account's sharing change failed part-way");
        return Err(sharing_failed(account, &failures, &describe(&failures)));
    }
    if !state
        .store
        .finish_account_sharing(&account.id, &account.config_dir, share)?
    {
        return Ok(false);
    }
    crate::coordinator::broadcast_settings(state)?;
    Ok(true)
}

/// The directory the agent's default account resolves to, as the default account's own sessions
/// see it: what availability found at start-up, else the launch environment's.
async fn default_dir(state: &Arc<AppState>, agent: Agent) -> Result<PathBuf> {
    if let Some(dir) = state.agent_availability_of(agent).default_account_dir {
        return Ok(PathBuf::from(dir));
    }
    let shell_env = tokio::task::spawn_blocking(crate::env_shell::launch_snapshot)
        .await?
        .context("snapshotting the user's shell environment")?;
    Ok(crate::adapter::default_account_dir(agent, &shell_env))
}

/// Whether the two spellings resolve to one directory. Blocking.
fn is_same_directory(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn not_shareable(account: Option<&Account>) -> anyhow::Error {
    match account {
        None => CodedError::raised(
            error_code::DEFAULT_ACCOUNT_NOT_SHAREABLE,
            "the default account is the configuration the other accounts share",
            &[],
        ),
        Some(account) => CodedError::raised(
            error_code::DEFAULT_ACCOUNT_NOT_SHAREABLE,
            format!(
                "account `{}`'s directory is the default account's own",
                account.name
            ),
            &[("account", &account.id), ("name", &account.name)],
        ),
    }
}

fn sharing_failed(account: &Account, failures: &[Failure], detail: &str) -> anyhow::Error {
    let doing = if account.share_default_config {
        "sharing the default configuration"
    } else {
        "ending the sharing of the default configuration"
    };
    CodedError::raised(
        error_code::ACCOUNT_SHARING_FAILED,
        format!(
            "{doing} with account `{}` did not complete: {detail}. The change stays pending and \
             is tried again before the account's next session starts",
            account.name
        ),
        &[
            ("account", &account.id),
            ("name", &account.name),
            ("items", &items_param(failures)),
            ("detail", detail),
        ],
    )
}

/// The failures as the JSON array a param carries: `[{item, left, detail, sessions?}]`.
fn items_param(failures: &[Failure]) -> String {
    serde_json::to_string(failures).unwrap_or_else(|_| "[]".to_string())
}

fn describe(failures: &[Failure]) -> String {
    failures
        .iter()
        .map(|failure| {
            let conversation = !failure.sessions.is_empty();
            let left = match failure.left {
                Left::Unchanged if conversation => "left in the default directory",
                Left::Unchanged => "left unchanged",
                Left::BackedUp => "left backed up, not linked",
                Left::PartlyMoved if conversation => "left partly taken back",
                Left::PartlyMoved => "left partly moved into the default directory",
                Left::Linked => "left linked",
                Left::Absent => "left absent",
            };
            match failure.sessions.as_slice() {
                [] => format!("`{}` {left} ({})", failure.item, failure.detail),
                sessions => format!(
                    "the conversation `{}` of session {} {left} ({})",
                    failure.item,
                    sessions.join(", "),
                    failure.detail
                ),
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}
