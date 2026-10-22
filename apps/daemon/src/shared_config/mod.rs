//! An account sharing its agent's default configuration: each account keeps the choice and
//! whether the last change toward it finished (`Account::share_default_config` and
//! `Account::share_default_config_incomplete`), and this module brings the directory in line.
//!
//! A change runs at once or is refused: it must not move files under anything running under the
//! account's directory, so it is refused while one of the account's sessions runs or starts, or
//! the account is being signed in, and waits only for Octoboard's short agent CLI reads there
//! (`cli_reads.rs`). While it runs the account is locked: none of its sessions is created,
//! launched, resumed or switched to. Nothing carries a change out later; one that fails is left
//! as it is, marked incomplete, for the user to ask again. Before every launch of a session of a
//! sharing account, the links an agent replaced are repaired. What the directory operations do is
//! `link.rs`'s; the whole contract is the "Sharing the default configuration" list under "Records"
//! in `apps/daemon/PROTOCOL.md`.

mod change;
mod cli_reads;
mod codex_state;
mod launch;
mod link;
mod locks;
mod moves;
mod registries;
mod status;
mod take_back;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::MutexGuard;

use crate::account_dirs::account_dir_key;
use crate::protocol::{error_code, Account, Agent, CodedError, Session};
use crate::state::{AppState, Running};
use link::{Cause, Failure, Left};

pub use change::{before_delete, before_repoint, set};
pub(crate) use cli_reads::{reading_account, running_long, Run};
pub use launch::before_launch;
pub use link::{Cause as FailureCause, Failure as FailedItem};
pub use status::{forget_session, note_session, publish, states};

/// What the daemon holds in memory of the accounts' sharing.
#[derive(Default)]
pub struct Sharing {
    locks: locks::Locks,
    states: status::States,
    activity: status::Activity,
    /// What resumes the sessions a change ended, in place of starting their agents.
    #[cfg(test)]
    pub(crate) relaunch: std::sync::Mutex<Option<Launcher>>,
    /// How long a change waits for a session it ended to be recorded down, in place of the
    /// switch's generous wait.
    #[cfg(test)]
    pub(crate) stop_timeout: std::sync::Mutex<Option<std::time::Duration>>,
}

/// A stand-in for `term::launch`.
#[cfg(test)]
pub(crate) type Launcher =
    Arc<dyn Fn(crate::term::LaunchRequest) -> Result<crate::term::Launch> + Send + Sync + 'static>;

/// Refuses what would work under the account `account`'s directory without launching there
/// (`None` for the default account, which no change touches) while a change of its sharing runs:
/// opening a session of it, before anything is recorded, a handoff from it, and a switch from or
/// to it, which asks before it ends the session's process so that a refusal leaves it running.
pub fn refuse_if_changing(state: &AppState, account: Option<&str>) -> Result<()> {
    match account.and_then(|id| state.sharing_locked(id)) {
        Some(locked) => Err(sharing_in_progress(&locked)),
        None => Ok(()),
    }
}

/// For an account switch of a session to the account `target` (`None` for the default account),
/// before it copies the conversation in: the agent's lock, for the switch to hold until it has
/// recorded the session on the target, so no change of any account's sharing moves the files the
/// copy writes meanwhile. Refused while a change of the target's sharing runs.
pub async fn before_switch_copy<'a>(
    state: &'a Arc<AppState>,
    agent: Agent,
    target: Option<&str>,
) -> Result<MutexGuard<'a, ()>> {
    let guard = state.account_sharing.locks.lock(agent).await;
    refuse_if_changing(state, target)?;
    Ok(guard)
}

/// Whether `session` runs under `account`'s directory: one of the account's sessions, or one
/// holding its directory.
pub(crate) fn runs_under(account: &Account, session: &Session) -> bool {
    session.agent == account.agent
        && (session.account_id.as_deref() == Some(&account.id)
            || session
                .config_dir
                .as_deref()
                .is_some_and(|dir| account_dir_key(dir) == account_dir_key(&account.config_dir)))
}

/// What runs under an account's directory, as a change of its sharing sees it.
struct Uses {
    /// The account's sessions working there, which refuse a change.
    working: usize,
    /// The account's idle sessions, which a change ends and resumes.
    idle: Vec<String>,
    /// Whether a long CLI run (a sign-in) runs there.
    signing_in: bool,
}

/// What runs under `account`'s directory among `running`, those running or holding a claim. The
/// sessions a change restarts are its own, and count as neither.
fn in_use(account: &Account, running: &[Running]) -> Uses {
    let mut uses = Uses {
        working: 0,
        idle: Vec::new(),
        signing_in: cli_reads::is_running_long(Path::new(&account.config_dir)),
    };
    for running in running
        .iter()
        .filter(|running| runs_under(account, &running.session))
    {
        if running.working {
            uses.working += 1;
        } else if !running.restarting {
            uses.idle.push(running.session.id.clone());
        }
    }
    uses
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

fn sharing_failed(
    account: &Account,
    share: bool,
    failures: &[Failure],
    cause: Cause,
    detail: &str,
) -> anyhow::Error {
    let doing = if share {
        "sharing the default configuration"
    } else {
        "ending the sharing of the default configuration"
    };
    CodedError::raised(
        error_code::ACCOUNT_SHARING_FAILED,
        format!(
            "{doing} with account `{}` did not complete: {detail}. Nothing was rolled back; \
             ask for it again to retry it",
            account.name
        ),
        &[
            ("account", &account.id),
            ("name", &account.name),
            ("cause", cause.as_str()),
            ("items", &items_param(failures)),
            ("detail", detail),
        ],
    )
}

/// `set_account_sharing` refused while `sessions` of the account run or start under its
/// directory, or it is `signing_in`.
fn account_in_use(account: &Account, sessions: usize, signing_in: bool) -> anyhow::Error {
    let why = match (sessions, signing_in) {
        (0, _) => "it is being signed in".to_string(),
        (1, false) => "one of its sessions is running or starting".to_string(),
        (1, true) => {
            "one of its sessions is running or starting, and it is being signed in".to_string()
        }
        (count, false) => format!("{count} of its sessions are running or starting"),
        (count, true) => {
            format!("{count} of its sessions are running or starting, and it is being signed in")
        }
    };
    CodedError::raised(
        error_code::ACCOUNT_IN_USE,
        format!(
            "account `{}` cannot change its sharing of the default configuration while {why}; \
             try again once nothing runs under it",
            account.name
        ),
        &[
            ("account", &account.id),
            ("name", &account.name),
            ("sessions", &sessions.to_string()),
            ("signing_in", &signing_in.to_string()),
        ],
    )
}

/// Removing or repointing a sharing account refused because its sharing cannot be turned off now.
fn still_sharing(account: &Account, then: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::ACCOUNT_STILL_SHARING,
        format!(
            "account `{}` shares the default configuration, and its sharing cannot be turned \
             off while its sessions run or start or it is being signed in; turn it off once \
             nothing does, then {then}",
            account.name
        ),
        &[("account", &account.id), ("name", &account.name)],
    )
}

/// Refused because a change of `account`'s sharing is running.
pub(crate) fn sharing_in_progress(account: &Account) -> anyhow::Error {
    CodedError::raised(
        error_code::ACCOUNT_SHARING_IN_PROGRESS,
        format!(
            "account `{}`'s sharing of the default configuration is being changed; try again \
             once it is done",
            account.name
        ),
        &[("account", &account.id), ("name", &account.name)],
    )
}

/// The failures as the JSON array a param carries: `[{item, left, cause, detail, sessions?}]`.
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
