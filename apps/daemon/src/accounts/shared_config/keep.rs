//! Keeping the default directory free of paths through a sharing account's directory. A session
//! of an account that shares may leave such a path in a plugin registry (an install or an update
//! records the plugin under the spelling the agent ran by), and whatever is left there must not
//! depend on the account's directory, which the user may delete. So the registries are pointed
//! back at the default directory after each of the account's sessions ends, before each of its
//! launches, as the daemon stops, and at daemon start for the sessions a stopped daemon did not
//! see end.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;

use super::link::{self, DirsError};
use super::registries;
use crate::protocol::{Account, AgentId, Session, SharingItemFailure};
use crate::state::AppState;
use crate::store::SharingStart;

/// How long a stopping daemon goes on beginning accounts, so that many of them, or a default
/// directory slow to answer, do not hold the stop up for long.
const STOP_LIMIT: Duration = Duration::from_secs(5);

/// Points `agent`'s plugin registries in `default`, the directory the account's links were made
/// into, at that directory's own spelling wherever they name a path through `account`'s; an
/// account directory that is gone is known by its recorded spelling. Nothing is done for a
/// directory that is the default's own. Answers the registries that could not be rewritten.
/// Blocking.
pub(super) fn registries_in_default(
    agent: AgentId,
    account: &Path,
    default: &Path,
) -> Vec<SharingItemFailure> {
    match link::Dirs::for_keeping(account, default) {
        Ok(dirs) => registries::keep(agent, &dirs, &mut |_| {}),
        Err(DirsError::SameAsDefault) => Vec::new(),
        Err(err) => {
            let (_, detail) = err.cause_and_detail();
            tracing::warn!(account = %account.display(), %detail, "a sharing account's directory could not be checked for its plugin registries");
            Vec::new()
        }
    }
}

/// After `session`'s process has ended: the registries of the account it ran under are kept in
/// the default directory. `session` is the record as it stood when the process was first watched,
/// so a session an account switch has since recorded on another account still names the one it
/// left.
pub(crate) async fn after_exit(state: &Arc<AppState>, session: &Session) {
    if !registries::has_registries(session.agent) {
        return;
    }
    let account = match super::launch::account_of(state, session) {
        Ok(Some(account)) => account,
        Ok(None) => return,
        Err(err) => {
            tracing::warn!(session = %session.id, %err, "reading a session's account after its exit failed");
            return;
        }
    };
    keep(state, &account.id).await;
}

/// As the daemon stops, once its sessions are ended: every account's registries are kept. The
/// sessions' own exits may not get to it before the daemon is gone, and the user may uninstall
/// and delete an account's directory before a daemon starts again. No account is begun once
/// [`STOP_LIMIT`] has passed, and one that is begun is waited for: a rewrite left behind in the
/// middle could leave the file it stages beside the user's registry, which nothing removes.
pub(crate) async fn at_stop(state: &Arc<AppState>) {
    every_account(state, Some(Instant::now() + STOP_LIMIT)).await;
}

/// At daemon start: every account that may hold links sharing made has its registries kept, for
/// the sessions the previous daemon did not see end.
pub(crate) async fn at_start(state: &Arc<AppState>) {
    every_account(state, None).await;
}

/// Keeps the registries of every account that may hold links sharing made, beginning none after
/// `until`.
async fn every_account(state: &Arc<AppState>, until: Option<Instant>) {
    let accounts = match state.store.list_accounts() {
        Ok(accounts) => accounts,
        Err(err) => {
            tracing::warn!(%err, "listing the accounts to keep their plugin registries failed");
            return;
        }
    };
    for account in accounts {
        if !registries::has_registries(account.agent) || !super::launch::links_anything(&account) {
            continue;
        }
        if until.is_some_and(|until| Instant::now() >= until) {
            tracing::warn!(account = %account.id, "the daemon stopped before every sharing account's plugin registries were kept");
            return;
        }
        keep(state, &account.id).await;
    }
}

/// Keeps the registries of account `id` in the default directory. An account whose last change
/// is incomplete is kept too: a link it still holds into the default directory names the same
/// place by two spellings all the same. A failure is logged and waits for the next of these
/// moments.
///
/// Not under the agent's sharing lock, which request handling takes while this reads and parses
/// an agent's file: nothing here depends on the links staying as they are. A path is rewritten
/// only to another spelling of the place it names, which holds whatever a change of sharing does
/// meanwhile, and the file is replaced only if it still holds what was read.
async fn keep(state: &Arc<AppState>, id: &str) {
    if let Err(err) = try_keep(state, id).await {
        tracing::warn!(account = %id, %err, "keeping a sharing account's plugin registries in the default directory failed");
    }
}

async fn try_keep(state: &Arc<AppState>, id: &str) -> Result<()> {
    let Some((account, start)) = linked(state, id)? else {
        return Ok(());
    };
    let (agent, dir) = (account.agent, PathBuf::from(&account.config_dir));
    let failures = tokio::task::spawn_blocking(move || {
        registries_in_default(agent, &dir, Path::new(&start.default_dir))
    })
    .await?;
    if !failures.is_empty() {
        tracing::warn!(account = %account.id, ?failures, "some of a sharing account's plugin registries still name its directory");
    }
    Ok(())
}

/// Account `id` and what turning its sharing on recorded, when it may hold links sharing made.
fn linked(state: &AppState, id: &str) -> Result<Option<(Account, SharingStart)>> {
    let Some(account) = state.store.get_account(id)? else {
        return Ok(None);
    };
    if !super::launch::links_anything(&account) {
        return Ok(None);
    }
    Ok(state
        .store
        .account_sharing_start(id)?
        .map(|start| (account, start)))
}
