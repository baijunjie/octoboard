//! The daemon's part of the clean-up before Octoboard is uninstalled: what its features have in
//! effect on the host outside Octoboard's own directories, and clearing all of it at once.
//!
//! Every write of the daemon's outside its own directories that is a registered exception (P9 in
//! `docs/conventions/principles.md`) and has something to undo is one line of [`EXCEPTIONS`], with
//! its two answers: what of it is in effect now, and clear all of it. The module of the feature
//! holds both, and clearing goes through the one function the feature's own switch goes through.
//! What has nothing to undo is not registered: the user's data a feature moved or copied stays
//! where it is, and is named by the result of the clearing that leaves it (`SystemChangeKept`).
//! Nothing here is keyed by an agent: a feature that differs between agents answers through the
//! generic module the providers declare themselves to.
//!
//! Listing and clearing are done on the daemon's own machine and never asked for over a client
//! connection: `octoboardd uninstall`, as its first step, and the desktop application's own
//! window reach [`listed`] and [`cleared`] through the daemon's local routes
//! (`crate::server::system_changes`), which take the launch key alone, and no request of the
//! control socket and no tool of the MCP server reaches either.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;

use crate::protocol::error_parts;
use crate::state::AppState;
use crate::store::Store;

use super::account_sharing;

mod wire;

pub use wire::*;

/// What clearing every modification of one exception answers, once it has ended.
pub type ClearAll<'a> = Pin<Box<dyn Future<Output = Result<Vec<SystemChangeResult>>> + Send + 'a>>;

/// A registered exception of the daemon's that has something to undo.
pub struct Exception {
    /// The `kind` of its modifications on the wire, which also names it in a failure and a log
    /// line.
    pub kind: &'static str,
    /// What of it is in effect now, one item per thing its feature's switch would undo, read from
    /// the store alone: `octoboardd uninstall` reads it with no daemon running, to know whether
    /// to start one for the clearing.
    pub in_effect: fn(&Store) -> Result<Vec<SystemChange>>,
    /// Clears all of it and turns its feature off, as the feature's own switch does, one result
    /// per item: an item that cannot be cleared is answered with its failure and does not stop the
    /// others. An error is a failure to tell what there is to clear at all.
    pub clear_all: for<'a> fn(&'a Arc<AppState>) -> ClearAll<'a>,
}

/// Every registered exception of the daemon's that has something to undo.
pub const EXCEPTIONS: &[Exception] = &[account_sharing::EXCEPTION];

/// What the daemon's features have in effect outside Octoboard's own directories, as `store`
/// records it.
pub fn list(store: &Store) -> Result<Vec<SystemChange>> {
    list_of(EXCEPTIONS, store)
}

/// Clears everything [`list`] names, each exception after the other, and answers what became of
/// each item, those cleared first, and the exceptions that failed as a whole (they could not tell
/// what they have in effect), which stop no other.
pub async fn clear(
    state: &Arc<AppState>,
) -> (Vec<SystemChangeResult>, Vec<SystemChangeFeatureFailure>) {
    clear_of(EXCEPTIONS, state).await
}

/// [`list`], as the running daemon answers it on its local route.
pub fn listed(state: &Arc<AppState>) -> Result<SystemChangesListed> {
    Ok(SystemChangesListed {
        changes: list(&state.store)?,
    })
}

/// [`clear`], as the running daemon answers it on its local route once everything it could clear
/// is cleared.
pub async fn cleared(state: &Arc<AppState>) -> SystemChangesCleared {
    let (results, failed) = clear(state).await;
    SystemChangesCleared { results, failed }
}

fn list_of(exceptions: &[Exception], store: &Store) -> Result<Vec<SystemChange>> {
    let mut changes = Vec::new();
    for exception in exceptions {
        changes.extend((exception.in_effect)(store)?);
    }
    Ok(changes)
}

async fn clear_of(
    exceptions: &[Exception],
    state: &Arc<AppState>,
) -> (Vec<SystemChangeResult>, Vec<SystemChangeFeatureFailure>) {
    let mut results = Vec::new();
    let mut failed = Vec::new();
    for exception in exceptions {
        match (exception.clear_all)(state).await {
            Ok(cleared) => results.extend(cleared),
            Err(err) => {
                let kind = exception.kind;
                tracing::warn!(kind, err = %format!("{err:#}"), "clearing an exception failed");
                let (code, params, message) = error_parts(&err);
                failed.push(SystemChangeFeatureFailure {
                    kind: kind.to_owned(),
                    failure: SystemChangeFailure {
                        code,
                        params,
                        message,
                    },
                });
            }
        }
    }
    // Stable, so the items of one exception keep the order it answered them in.
    results.sort_by_key(|result| result.failure.is_some());
    (results, failed)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::protocol::{error_code, AgentId};
    use crate::test_support::console_with_project_and_accounts;

    /// How many times [`COUNTED`] was cleared.
    static COUNTED_CLEARINGS: AtomicUsize = AtomicUsize::new(0);

    fn change(account: &str) -> SystemChange {
        SystemChange::AccountSharing {
            account: account.into(),
            name: account.into(),
            agent: AgentId::CLAUDE,
            config_dir: format!("/accounts/{account}"),
        }
    }

    fn result(account: &str, failed: bool) -> SystemChangeResult {
        SystemChangeResult {
            change: change(account),
            failure: failed.then(|| SystemChangeFailure {
                code: error_code::ACCOUNT_IN_USE,
                params: Default::default(),
                message: "in use".into(),
            }),
            kept: None,
        }
    }

    fn answering(answer: Result<Vec<SystemChangeResult>>) -> ClearAll<'static> {
        Box::pin(async move { answer })
    }

    fn unreadable<T>() -> Result<T> {
        Err(anyhow::anyhow!("unreadable"))
    }

    /// An exception with one item in effect that is not cleared, and one that is.
    const MIXED: Exception = Exception {
        kind: "mixed",
        in_effect: |_| Ok(vec![change("stuck"), change("free")]),
        clear_all: |_| answering(Ok(vec![result("stuck", true), result("free", false)])),
    };

    /// An exception that cannot tell what it has in effect.
    const BROKEN: Exception = Exception {
        kind: "broken",
        in_effect: |_| unreadable(),
        clear_all: |_| answering(unreadable()),
    };

    /// An exception with one item, which it clears.
    const PLAIN: Exception = Exception {
        kind: "plain",
        in_effect: |_| Ok(vec![change("other")]),
        clear_all: |_| answering(Ok(vec![result("other", false)])),
    };

    /// [`PLAIN`], counting its clearings.
    const COUNTED: Exception = Exception {
        kind: "counted",
        in_effect: |_| Ok(vec![change("other")]),
        clear_all: |_| {
            COUNTED_CLEARINGS.fetch_add(1, Ordering::SeqCst);
            answering(Ok(vec![result("other", false)]))
        },
    };

    #[tokio::test]
    async fn the_list_holds_every_exception_s_items_in_their_order() {
        let (state, _dir) = console_with_project_and_accounts("system-changes-list");
        assert_eq!(
            list_of(&[MIXED, PLAIN], &state.store).unwrap(),
            [change("stuck"), change("free"), change("other")]
        );
        assert!(list_of(&[], &state.store).unwrap().is_empty());
        assert!(list_of(&[MIXED, BROKEN], &state.store).is_err());
    }

    #[tokio::test]
    async fn clearing_runs_every_exception_and_answers_the_cleared_items_first() {
        let (state, _dir) = console_with_project_and_accounts("system-changes-clear");
        let (results, failed) = clear_of(&[MIXED, PLAIN], &state).await;
        assert!(failed.is_empty());
        assert_eq!(
            results,
            [
                result("free", false),
                result("other", false),
                result("stuck", true)
            ]
        );
    }

    #[tokio::test]
    async fn an_exception_that_fails_as_a_whole_is_answered_beside_the_items_of_the_others() {
        let (state, _dir) = console_with_project_and_accounts("system-changes-broken");
        let (results, failed) = clear_of(&[MIXED, BROKEN, COUNTED], &state).await;
        assert_eq!(COUNTED_CLEARINGS.load(Ordering::SeqCst), 1);
        assert_eq!(
            results,
            [
                result("free", false),
                result("other", false),
                result("stuck", true)
            ]
        );
        let [broken] = failed.as_slice() else {
            panic!("{failed:?}");
        };
        assert_eq!(broken.kind, "broken");
        assert_eq!(broken.failure.code, error_code::INTERNAL_ERROR);
        assert_eq!(broken.failure.message, "unreadable");
    }

    #[tokio::test]
    async fn with_nothing_registered_in_effect_the_answers_are_empty() {
        let (state, _dir) = console_with_project_and_accounts("system-changes-empty");
        assert_eq!(
            listed(&state).unwrap(),
            SystemChangesListed {
                changes: Vec::new()
            }
        );
        assert_eq!(
            cleared(&state).await,
            SystemChangesCleared {
                results: Vec::new(),
                failed: Vec::new()
            }
        );
    }
}
