//! What a launch of a session takes from its account's sharing: the shared directory, and the
//! repair of the links an agent replaced.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use super::link::{self, Cause, DirsError, Failure};
use super::{default_dir, describe, items_param, moves, sharing_in_progress};
use crate::account_dirs::account_dir_key;
use crate::protocol::{notice_code, Account, Notice, Session};
use crate::state::AppState;

/// What a launch of a session takes from its account's sharing.
#[derive(Default)]
pub struct Prepared {
    /// The default account's directory, fully resolved, when the session's account shares it.
    pub shared_default_dir: Option<PathBuf>,
    /// What the launch has to tell the user about the account's directory.
    pub notices: Vec<Notice>,
}

/// Before `session` starts or resumes: refused while its account is locked for a change of its
/// sharing. While the account shares, the launch is told the shared directory, and the directory
/// is repaired first. A repair that cannot fix something does not hold the launch up; it is told
/// as a notice. An account whose last change is incomplete, either way, is left as it is for the
/// user to retry, and the launch is told the shared directory only while the account's
/// conversations still lead into it ([`links_conversations_into`]). A session that holds another
/// directory than its account's (the account was repointed after it was opened) takes nothing
/// from the account.
pub async fn before_launch(state: &Arc<AppState>, session: &Session) -> Result<Prepared> {
    if let Some(locked) = state.sharing_lock_over(session) {
        return Err(sharing_in_progress(&locked));
    }
    let Some(account) = account_of(state, session)? else {
        return Ok(Prepared::default());
    };
    if !links_anything(&account) {
        return Ok(Prepared::default());
    }
    let _guard = state.account_sharing.locks.lock(account.agent).await;
    let Some(account) = account_of(state, session)? else {
        return Ok(Prepared::default());
    };
    if !links_anything(&account) {
        return Ok(Prepared::default());
    }
    let Some(start) = state.store.account_sharing_start(&account.id)? else {
        return Ok(Prepared::default());
    };
    let current = default_dir(state, account.agent).await?;
    let (agent, dir) = (account.agent, PathBuf::from(&account.config_dir));
    let incomplete = account.share_default_config_incomplete;
    let checked =
        tokio::task::spawn_blocking(move || {
            let recorded = moves::resolve_or_spelled(Path::new(&start.default_dir));
            if incomplete {
                let shared = links_conversations_into(&dir, &recorded).then_some(recorded);
                return Repair::Skipped(shared);
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

/// Whether the account's directory may hold links sharing made: it shares, or the last change of
/// its sharing, either way, is incomplete.
fn links_anything(account: &Account) -> bool {
    account.share_default_config || account.share_default_config_incomplete
}

/// Whether the account's `projects/` links into `default`, a resolved directory, directly or to
/// where the default's own `projects/` leads. It is where Claude Code keeps auto memory, which
/// the shared directory is given for. Blocking.
fn links_conversations_into(dir: &Path, default: &Path) -> bool {
    moves::link_target(&dir.join("projects")).is_some_and(|target| {
        target.starts_with(default)
            || moves::resolve(&default.join("projects")).ok().flatten() == Some(target)
    })
}

/// What became of the repair before a launch.
enum Repair {
    /// Not attempted, the account's conversations leading into this directory, if any.
    Skipped(Option<PathBuf>),
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
    let repair_failed = |message: String, cause: Cause, failures: &[Failure]| {
        Notice::new(
            notice_code::ACCOUNT_SHARING_REPAIR_FAILED,
            message,
            &[
                ("account", &account.id),
                ("name", &account.name),
                ("cause", cause.as_str()),
                ("items", &items_param(failures)),
            ],
        )
    };
    let repaired = match repair {
        Repair::Skipped(default) => return (default, Vec::new()),
        Repair::DefaultMoved { recorded, current } => {
            let message = format!(
                "account `{}` shares `{}`, but the default configuration is now `{}`; its links \
                 were not repaired. Turn its sharing off and on again to share the new one",
                account.name,
                recorded.display(),
                current.display()
            );
            tracing::warn!(account = %account.id, %message, "a sharing account's default directory moved");
            return (
                Some(recorded),
                vec![repair_failed(message, Cause::DefaultMoved, &[])],
            );
        }
        Repair::Done(repaired) => repaired,
    };
    let (repaired, default) = match repaired {
        Ok(Some(repaired)) => repaired,
        Ok(None) => return (None, Vec::new()),
        Err(err) => {
            let (cause, detail) = err.cause_and_detail();
            tracing::warn!(account = %account.id, %detail, "checking a sharing account's links failed");
            let message = format!(
                "account `{}`'s links to the default configuration could not be checked: {detail}",
                account.name
            );
            return (None, vec![repair_failed(message, cause, &[])]);
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
        let cause = Cause::of_all(&repaired.failures);
        notices.push(repair_failed(message, cause, &repaired.failures));
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
