//! The tool with which a session reads how much plan quota each account has left,
//! `get_account_quotas`. It returns the readings the daemon already holds (`AccountQuotas`) and
//! never waits for a probe, so what it says is as old as each reading's `fetched_at`.

use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Map, Value};

use crate::coordinator;
use crate::protocol::{now_millis, Agent, QuotaWindow};
use crate::state::AppState;

use super::args::optional_agent;

/// What is said of an account that no reading is held for yet.
const NOT_READ_YET: &str = "Not read yet: the daemon has not taken a reading of this account's \
                            quota. Try again shortly.";

/// Every account of every agent a session could be opened on (or of `agent`, when the caller names
/// one), with the reading held for it: the default account first, then the stored ones in the
/// order they are kept. An agent that is not installed or not added has none (the poller still
/// reads one that is installed but not added); one not yet detected is listed, its accounts not
/// read yet. An account is judged
/// exhausted by the rule that keeps it from opening a session (`AccountQuotas::exhausted`).
pub(super) fn get_account_quotas(
    state: &Arc<AppState>,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let only = optional_agent(arguments, "agent")?;
    let accounts = state.store.list_accounts()?;
    let now = now_millis();
    let mut listed = Vec::new();
    for agent in Agent::ALL {
        if only.is_some_and(|only| only != agent) || !coordinator::agent_can_open(state, agent)? {
            continue;
        }
        let stored = accounts.iter().filter(|account| account.agent == agent);
        let candidates = std::iter::once((None, None, state.store.default_account_name(agent)?))
            .chain(stored.map(|account| {
                (
                    Some(account.id.as_str()),
                    Some(account.config_dir.as_str()),
                    account.name.clone(),
                )
            }));
        for (id, config_dir, name) in candidates {
            let (held, exhausted) = state
                .account_quotas
                .held_and_exhausted(agent, id, config_dir, now);
            listed.push(json!({
                "agent": agent,
                "account": id.unwrap_or("default"),
                "account_name": name,
                "plan": held.as_ref().and_then(|quota| quota.plan.as_deref()),
                "fetched_at": held.as_ref().map(|quota| quota.fetched_at),
                "exhausted": exhausted.is_some(),
                "exhausted_until": exhausted.and_then(|exhausted| exhausted.resets_at),
                "unavailable": match &held {
                    Some(quota) => quota.unavailable.as_ref().map(|reason| reason.message.as_str()),
                    None => Some(NOT_READ_YET),
                },
                "windows": held
                    .as_ref()
                    .map(|quota| quota.windows.iter().map(describe_window).collect::<Vec<_>>())
                    .unwrap_or_default(),
            }));
        }
    }
    Ok(json!({ "accounts": listed }))
}

fn describe_window(window: &QuotaWindow) -> Value {
    json!({
        "kind": window.kind,
        "label": window.label,
        "used_percent": window.used_percent,
        "remaining_percent": window.remaining_percent(),
        "resets_at": window.resets_at,
        "window_minutes": window.window_minutes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::exec::tests::{arguments, shared_console_state};
    use crate::protocol::{
        Account, AccountQuota, AgentAvailability, Availability, QuotaUnavailable, QuotaWindowKind,
        DEFAULT_ACCOUNT_NAME,
    };

    fn available(state: &Arc<AppState>, agents: &[Agent]) {
        state.set_agent_availability(
            agents
                .iter()
                .map(|agent| AgentAvailability {
                    agent: *agent,
                    availability: Availability::Available,
                    default_account_dir: None,
                })
                .collect(),
        );
    }

    fn insert_account(state: &Arc<AppState>, agent: Agent, id: &str) {
        state
            .store
            .insert_account(&Account {
                id: id.to_string(),
                agent,
                name: format!("{id} name"),
                config_dir: format!("/tmp/{id}"),
                approval_reviewer: None,
                approval_mode: None,
                share_default_config: false,
                share_default_config_pending: false,
            })
            .unwrap();
    }

    fn weekly(used_percent: f64, resets_at: i64) -> QuotaWindow {
        QuotaWindow {
            kind: QuotaWindowKind::Weekly,
            label: None,
            used_percent,
            resets_at: Some(resets_at),
            window_minutes: Some(10_080),
        }
    }

    fn quota(agent: Agent, account: Option<&str>, windows: Vec<QuotaWindow>) -> AccountQuota {
        AccountQuota {
            agent,
            account: account.map(str::to_string),
            fetched_at: 1_000,
            plan: Some("max".to_string()),
            windows,
            unavailable: None,
        }
    }

    fn entry<'a>(result: &'a Value, agent: &str, account: &str) -> &'a Value {
        result["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["agent"] == agent && entry["account"] == account)
            .unwrap_or_else(|| panic!("no entry for {agent} {account}: {result}"))
    }

    /// Every account of an available agent is listed, the default one first, with the reading
    /// held for it: the remaining share derived from the used one, and an account no reading is
    /// held for marked as not read yet. A reading that failed carries the reason.
    #[test]
    fn every_account_is_listed_with_its_reading_or_a_note_that_there_is_none() {
        let (state, _dir) = shared_console_state("quotas-listed");
        available(&state, &[Agent::Claude]);
        insert_account(&state, Agent::Claude, "claude-a");
        insert_account(&state, Agent::Claude, "claude-b");
        let future = now_millis() + 3_600_000;
        state
            .account_quotas
            .hold_for_test(&quota(Agent::Claude, None, vec![weekly(30.5, future)]));
        let mut failed = quota(Agent::Claude, Some("claude-b"), vec![weekly(120.0, future)]);
        failed.unavailable = Some(QuotaUnavailable::new("command_failed", "it failed", &[]));
        state
            .account_quotas
            .hold_in_dir_for_test(&failed, "/tmp/claude-b");

        let result = get_account_quotas(&state, &arguments(json!({ "agent": "claude" }))).unwrap();

        let ids: Vec<&Value> = result["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| &entry["account"])
            .collect();
        assert_eq!(ids, ["default", "claude-a", "claude-b"]);

        let default = entry(&result, "claude", "default");
        assert_eq!(default["account_name"], DEFAULT_ACCOUNT_NAME);
        assert_eq!(default["plan"], "max");
        assert_eq!(default["fetched_at"], 1_000);
        assert_eq!(default["exhausted"], false);
        assert_eq!(default["exhausted_until"], Value::Null);
        assert_eq!(default["unavailable"], Value::Null);
        assert_eq!(
            default["windows"],
            json!([{
                "kind": "weekly",
                "label": null,
                "used_percent": 30.5,
                "remaining_percent": 69.5,
                "resets_at": future,
                "window_minutes": 10_080,
            }])
        );

        let unread = entry(&result, "claude", "claude-a");
        assert_eq!(unread["account_name"], "claude-a name");
        assert_eq!(unread["fetched_at"], Value::Null);
        assert_eq!(unread["windows"], json!([]));
        assert_eq!(unread["exhausted"], false);
        assert_eq!(unread["unavailable"], NOT_READ_YET);

        let failed = entry(&result, "claude", "claude-b");
        assert_eq!(failed["unavailable"], "it failed");
        assert_eq!(failed["windows"][0]["remaining_percent"], 0.0);
    }

    /// An account is exhausted by the rule that keeps it from opening a session: a used-up
    /// window that has not reset. One that has reset is not.
    #[test]
    fn an_account_with_a_used_up_window_that_has_not_reset_is_exhausted() {
        let (state, _dir) = shared_console_state("quotas-exhausted");
        available(&state, &[Agent::Codex]);
        insert_account(&state, Agent::Codex, "codex-a");
        insert_account(&state, Agent::Codex, "codex-b");
        let future = now_millis() + 3_600_000;
        for (id, window) in [
            ("codex-a", weekly(100.0, future)),
            ("codex-b", weekly(100.0, 1)),
        ] {
            state.account_quotas.hold_in_dir_for_test(
                &quota(Agent::Codex, Some(id), vec![window]),
                &format!("/tmp/{id}"),
            );
        }

        let result = get_account_quotas(&state, &arguments(json!({}))).unwrap();

        assert_eq!(entry(&result, "codex", "codex-a")["exhausted"], true);
        assert_eq!(
            entry(&result, "codex", "codex-a")["exhausted_until"],
            future
        );
        assert_eq!(
            entry(&result, "codex", "codex-b")["exhausted_until"],
            Value::Null
        );
        assert_eq!(entry(&result, "codex", "codex-b")["exhausted"], false);
        assert_eq!(entry(&result, "codex", "default")["exhausted"], false);
    }

    /// `agent` narrows the list to one agent. An agent that is not installed, or not on the added
    /// list, has no accounts listed; one not detected yet lists them, not read yet.
    #[test]
    fn the_agent_argument_filters_and_an_uninstalled_or_unadded_agent_is_left_out() {
        let (state, _dir) = shared_console_state("quotas-filter");
        state.set_agent_availability(vec![AgentAvailability {
            agent: Agent::Grok,
            availability: Availability::Unavailable,
            default_account_dir: None,
        }]);
        insert_account(&state, Agent::Grok, "grok-a");

        let all = get_account_quotas(&state, &arguments(json!({}))).unwrap();
        let agents: Vec<&Value> = all["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| &entry["agent"])
            .collect();
        assert_eq!(agents, ["claude", "codex"]);
        assert_eq!(all["accounts"][0]["fetched_at"], Value::Null);

        let codex = get_account_quotas(&state, &arguments(json!({ "agent": "codex" }))).unwrap();
        assert_eq!(codex["accounts"].as_array().unwrap().len(), 1);
        assert_eq!(codex["accounts"][0]["agent"], "codex");

        state
            .store
            .set_added_agents_if_undecided(&[Agent::Claude])
            .unwrap();
        let added = get_account_quotas(&state, &arguments(json!({}))).unwrap();
        assert_eq!(added["accounts"].as_array().unwrap().len(), 1);
        assert_eq!(added["accounts"][0]["agent"], "claude");

        let grok = get_account_quotas(&state, &arguments(json!({ "agent": "grok" }))).unwrap();
        assert_eq!(grok["accounts"], json!([]));

        let err = get_account_quotas(&state, &arguments(json!({ "agent": "gemini" })))
            .expect_err("unknown agent");
        assert!(err.to_string().contains("gemini"), "{err}");
    }
}
