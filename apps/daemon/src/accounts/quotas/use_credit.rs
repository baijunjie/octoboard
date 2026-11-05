//! Spending one of an account's rate limit reset credits for the user: [`use_reset_credit`] picks
//! the credit from the held reading, has the provider's [`ResetCredits`] consume it under a
//! per-account guard, takes it out of the held reading and reads the account again whichever way
//! the use ends.

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::poll::{refresh_with, Scope, ShellSource, Trigger};
use super::reset_credit::Failure;
use super::{AccountQuotas, Key};
use crate::adapter::provider;
use crate::protocol::{error_code, AgentId, CodedError, ProviderOperation, ResetCreditOutcome};
use crate::state::AppState;
use crate::time::now_millis;

/// Holds an account's place among the uses of a reset credit, released on drop.
struct Resetting {
    key: Key,
    resetting: Arc<Mutex<HashSet<Key>>>,
}

impl Drop for Resetting {
    fn drop(&mut self) {
        self.resetting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.key);
    }
}

impl AccountQuotas {
    /// Takes the reset credit just used (`used` when it was named, else one the count alone
    /// stands for) out of the account's held reading and broadcasts the list, so that nothing
    /// offers it again before the reading that follows the use says what is left. The last credit
    /// takes `reset_credits` with it, and a use that named none leaves the count alone, since which
    /// credit went is not known.
    fn spend_held_credit(&self, state: &AppState, key: &Key, used: Option<&str>) {
        self.change(state, |inner| {
            let Some(entry) = inner.entries.get_mut(key) else {
                return false;
            };
            let Some(held) = entry.quota.reset_credits.as_mut() else {
                return false;
            };
            held.available_count = held.available_count.saturating_sub(1);
            match used {
                Some(used) => {
                    if let Some(credits) = held.credits.as_mut() {
                        credits.retain(|credit| credit.id != used);
                    }
                }
                None => held.credits = None,
            }
            if held.available_count == 0 {
                entry.quota.reset_credits = None;
            }
            true
        });
    }
}

/// Uses one reset credit of the `agent` account `account` (an account id, `None` for the default
/// account) in the shell environment `shell` gives, for a user who confirmed it, and reads the
/// account's quota again whichever way it ends. The credit used is the first the held reading lists
/// that has not expired, which is the one that expires first; with no details held, or a held
/// reading that is out of date (the account's last attempt failed), the provider picks one. A use
/// that worked takes the credit out of the held reading at once, until the next reading says what
/// is left. Nothing but a request from the user calls this: no automatic path may spend a credit.
pub(crate) async fn use_reset_credit(
    state: &Arc<AppState>,
    agent: AgentId,
    account: Option<String>,
    shell: ShellSource,
    timeout: Duration,
) -> anyhow::Result<ResetCreditOutcome> {
    let credits = CodedError::require_offered(
        agent,
        ProviderOperation::ResetCredit,
        provider(agent)
            .accounts()
            .and_then(|ops| ops.reset_credits())
            .filter(|_| agent.descriptor().capabilities.reset_credits),
    )?;
    let quotas = &state.account_quotas;
    if quotas.stopping.load(Ordering::Relaxed) {
        return Err(CodedError::raised(
            error_code::DAEMON_SHUTTING_DOWN,
            "the daemon is shutting down, so no reset credit is used",
            &[],
        ));
    }
    let config_dir = state.store.account_dir(&account)?;
    let key = Key::of(agent, account.as_deref(), config_dir.as_deref());
    let _resetting = {
        let mut resetting = quotas
            .resetting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !resetting.insert(key.clone()) {
            return Err(CodedError::raised(
                error_code::RESET_CREDIT_IN_PROGRESS,
                "A reset credit is already being used on this account.",
                &[("agent", agent.label())],
            ));
        }
        Resetting {
            key: key.clone(),
            resetting: quotas.resetting.clone(),
        }
    };
    let credit_id = quotas
        .lock()
        .entries
        .get(&key)
        .filter(|entry| entry.quota.unavailable.is_none())
        .and_then(|entry| entry.quota.reset_credits.as_ref())
        .and_then(|held| held.credits.as_ref())
        .and_then(|credits| {
            let now = now_millis();
            credits
                .iter()
                .find(|credit| credit.expires_at.is_none_or(|at| at > now))
        })
        .map(|credit| credit.id.clone());
    let idempotency_key = uuid::Uuid::new_v4().to_string();
    let _slot = quotas.slots.acquire().await;
    let stopping = quotas.stopping.clone();
    let dir = config_dir.clone();
    let named = credit_id.clone();
    let shell_for_reading = shell.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        let shell_env = shell().map_err(|err| {
            let detail = format!("{err:#}");
            CodedError::raised(
                error_code::SHELL_ENVIRONMENT_TIMEOUT,
                format!("The shell environment could not be captured: {detail}"),
                &[("detail", &detail)],
            )
        })?;
        Ok::<_, anyhow::Error>(credits.consume(
            &shell_env,
            dir.as_deref().map(Path::new),
            named.as_deref(),
            &idempotency_key,
            timeout,
            &stopping,
        ))
    })
    .await??;
    drop(_slot);
    if matches!(outcome, Ok(ResetCreditOutcome::Reset)) {
        quotas.spend_held_credit(state, &key, credit_id.as_deref());
    }
    // The credit may have been used even when the answer did not arrive, so the reading is taken
    // again after any run of the CLI; a probe that began before it is dropped by `make_due`.
    if !matches!(outcome, Err(Failure::BinaryNotFound(_))) {
        quotas.make_due(agent, account.as_deref());
        let (state, shell, scope) = (
            state.clone(),
            shell_for_reading,
            Scope {
                agent: Some(agent),
                account: account.clone(),
            },
        );
        tokio::spawn(async move { refresh_with(&state, &scope, Trigger::Request, shell).await });
    }
    outcome.map_err(|failure| {
        let label = agent.label();
        match failure {
            Failure::BinaryNotFound(binary) => CodedError::raised(
                error_code::BINARY_NOT_FOUND,
                format!("`{binary}` was not found on PATH in the shell environment."),
                &[("binary", &binary)],
            ),
            Failure::NotSignedIn => {
                let name = state
                    .store
                    .account_display_name(agent, account.as_deref())
                    .unwrap_or_default();
                CodedError::raised(
                    error_code::RESET_CREDIT_NOT_SIGNED_IN,
                    format!("{name} ({label}) is not logged in, so a reset credit cannot be used on it."),
                    &[("agent", label), ("account", name.as_str())],
                )
            }
            Failure::TimedOut => CodedError::raised(
                error_code::RESET_CREDIT_TIMED_OUT,
                format!("{label} did not answer the request to use a reset credit in time."),
                &[("agent", label)],
            ),
            Failure::Failed(detail) => CodedError::raised(
                error_code::RESET_CREDIT_FAILED,
                format!("Using a reset credit on {label} failed: {detail}"),
                &[("agent", label), ("detail", &detail)],
            ),
        }
    })
}

#[cfg(test)]
mod tests;
