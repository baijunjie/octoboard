//! The tool with which a session reads the agents it could start a session on, `list_agents`: each
//! one's tags and the plan quota each of its accounts has left. It returns the readings the daemon
//! already holds (`AccountQuotas`), so what it says is as old as each reading's `fetched_at`; only
//! a call that asks to `refresh` waits for new ones, and then only for the accounts not read within
//! the last minute, whichever way the last reading came (`quotas::refresh_on_demand`). A refresh
//! never starts a quota window.

use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Map, Value};

use crate::accounts::choice::{self, CannotOpen};
use crate::accounts::quotas::{self, ShellSource};
use crate::protocol::{AgentId, QuotaWindow};
use crate::state::AppState;
use crate::time::now_millis;

use crate::mcp::lookup::{optional_agent, optional_bool};

/// What is said of an account that no reading is held for yet.
const NOT_READ_YET: &str = "Not read yet: the daemon has not taken a reading of this account's \
                            quota. Try again shortly.";

/// What is said of an account that is used up, as the reason it cannot open a session.
const USED_UP: &str = "Its plan quota is used up; `exhausted_until` says when it resets.";

/// What is said of an account the last reading found not logged in, as the reason it cannot open
/// a session.
const NOT_SIGNED_IN: &str = "It is not logged in; log in to it in Settings → Agent accounts.";

/// What is said of an account whose change of sharing is running, as the reason it cannot open a
/// session.
const SHARING_IN_PROGRESS: &str = "A change of its sharing settings is in progress; try again \
                                   when it has ended.";

/// Every agent a session could be opened on (or `agent`, when the caller names one), each with
/// its tags and its accounts and the reading held for each. The agents are the added ones in the
/// order the user keeps them in, every agent while the list is undecided; a removed agent is not
/// listed, so its tags are not shown, and neither is one that is not installed. One not yet
/// detected is listed, its accounts not read yet. The accounts are in the order automatic account
/// selection weighs them (`account_candidates` in `accounts::choice`): the order the user keeps
/// them in, the ones it does not name after it. An account the user turned off is left out, the caller's
/// own included: it is shown on the account settings page alone. An account is judged exhausted by
/// the rule that keeps it from opening a session (`AccountQuotas::exhausted`), and `can_open` also
/// takes in an account the last reading found not logged in and a running change of its sharing,
/// the reasons automatic account selection passes an account over for. `auto_select_account` is
/// the setting, so a caller that cannot read the settings knows whether an account it leaves out
/// is chosen for it.
///
/// With `refresh` the readings of those accounts, and of an agent that is installed but not added
/// (which the poller reads too), the ones the user turned off excepted, are first taken again
/// where the last attempt is over a minute old; a call within the minute is not an error, it gets
/// what is held.
pub(in crate::mcp) async fn list_agents(
    state: &Arc<AppState>,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    list_agents_with(
        state,
        arguments,
        Arc::new(crate::env_shell::cached_snapshot),
    )
    .await
}

/// [`list_agents`] reading in the shell environment `shell` gives.
async fn list_agents_with(
    state: &Arc<AppState>,
    arguments: &Map<String, Value>,
    shell: ShellSource,
) -> Result<Value> {
    let only = optional_agent(arguments, "agent")?;
    if optional_bool(arguments, "refresh")?.unwrap_or(false) {
        let scope = quotas::Scope {
            agent: only,
            account: None,
            enabled_only: true,
        };
        quotas::refresh_on_demand(state, &scope, shell).await;
    }
    let now = now_millis();
    let added = state
        .store
        .added_agents()?
        .unwrap_or_else(|| AgentId::ALL.to_vec());
    let tags = state.store.agent_tags()?;
    let mut listed = Vec::new();
    for agent in added {
        if only.is_some_and(|only| only != agent) {
            continue;
        }
        if !choice::agent_is_available(state, agent) {
            continue;
        }
        listed.push(json!({
            "agent": agent,
            "tags": tags.get(agent.wire_name()).cloned().unwrap_or_default(),
            "accounts": agent_accounts(state, agent, now)?,
        }));
    }
    Ok(json!({
        "auto_select_account": state.store.auto_select_account()?,
        "agents": listed,
    }))
}

/// The accounts of `agent` a session could be opened on, with the reading held for each, as
/// [`list_agents`] describes them.
fn agent_accounts(state: &Arc<AppState>, agent: AgentId, now: i64) -> Result<Vec<Value>> {
    let mut listed = Vec::new();
    for candidate in choice::candidate_accounts(state, agent)? {
        if candidate.disabled {
            continue;
        }
        let id = candidate.id.as_deref();
        let cannot_open = match choice::cannot_open(state, agent, id)? {
            // Turned off or removed meanwhile: left out, as a turned-off account always is.
            Some(CannotOpen::Unusable) => continue,
            Some(CannotOpen::SignedOut) => Some(NOT_SIGNED_IN),
            Some(CannotOpen::QuotaUsedUp(_)) => Some(USED_UP),
            Some(CannotOpen::SharingInProgress) => Some(SHARING_IN_PROGRESS),
            None => None,
        };
        let (held, exhausted) = state.account_quotas.held_and_exhausted(
            agent,
            id,
            candidate.config_dir.as_deref(),
            now,
        );
        listed.push(json!({
            "agent": agent,
            "account": id.unwrap_or("default"),
            "account_name": candidate.name,
            "plan": held.as_ref().and_then(|quota| quota.plan.as_deref()),
            "fetched_at": held.as_ref().map(|quota| quota.fetched_at),
            "exhausted": exhausted.is_some(),
            "exhausted_until": exhausted.and_then(|exhausted| exhausted.resets_at),
            "can_open": cannot_open.is_none(),
            "cannot_open_reason": cannot_open,
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
    Ok(listed)
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::mcp::test_support::{account, arguments, shared_console_state, weekly};
    use crate::protocol::{
        AccountQuota, AgentAvailability, Availability, QuotaUnavailable, DEFAULT_ACCOUNT_NAME,
    };

    fn available(state: &Arc<AppState>, agents: &[AgentId]) {
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

    fn insert_account(state: &Arc<AppState>, agent: AgentId, id: &str) {
        state
            .store
            .insert_account(&account(
                id,
                agent,
                format!("{id} name"),
                format!("/tmp/{id}"),
            ))
            .unwrap();
    }

    fn quota(agent: AgentId, account: Option<&str>, windows: Vec<QuotaWindow>) -> AccountQuota {
        AccountQuota {
            reset_credits: None,
            agent,
            account: account.map(str::to_string),
            fetched_at: 1_000,
            plan: Some("max".to_string()),
            email: None,
            windows,
            unavailable: None,
        }
    }

    /// The agent entries of a result, by agent.
    fn agents_of(result: &Value) -> Vec<&Value> {
        result["agents"].as_array().unwrap().iter().collect()
    }

    /// The entry of `agent`, which must be listed.
    fn agent_entry<'a>(result: &'a Value, agent: &str) -> &'a Value {
        agents_of(result)
            .into_iter()
            .find(|entry| entry["agent"] == agent)
            .unwrap_or_else(|| panic!("no entry for {agent}: {result}"))
    }

    /// The account ids of `agent`, in the order listed.
    fn account_ids<'a>(result: &'a Value, agent: &str) -> Vec<&'a Value> {
        agent_entry(result, agent)["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| &entry["account"])
            .collect()
    }

    fn entry<'a>(result: &'a Value, agent: &str, account: &str) -> &'a Value {
        agent_entry(result, agent)["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["account"] == account)
            .unwrap_or_else(|| panic!("no entry for {agent} {account}: {result}"))
    }

    /// Every account of an available agent is listed, the default one first, with the reading
    /// held for it: the remaining share derived from the used one, and an account no reading is
    /// held for marked as not read yet. A reading that failed carries the reason.
    #[tokio::test]
    async fn every_account_is_listed_with_its_reading_or_a_note_that_there_is_none() {
        let (state, _dir) = shared_console_state("quotas-listed");
        available(&state, &[AgentId::CLAUDE]);
        insert_account(&state, AgentId::CLAUDE, "claude-a");
        insert_account(&state, AgentId::CLAUDE, "claude-b");
        let future = now_millis() + 3_600_000;
        state.account_quotas.hold_for_test(&quota(
            AgentId::CLAUDE,
            None,
            vec![weekly(30.5, future)],
        ));
        let mut failed = quota(
            AgentId::CLAUDE,
            Some("claude-b"),
            vec![weekly(120.0, future)],
        );
        failed.unavailable = Some(QuotaUnavailable::new(
            crate::protocol::quota_unavailable_code::COMMAND_FAILED,
            "it failed",
            &[],
        ));
        state
            .account_quotas
            .hold_in_dir_for_test(&failed, "/tmp/claude-b");

        let result = list_agents(&state, &arguments(json!({ "agent": "claude" })))
            .await
            .unwrap();

        let ids = account_ids(&result, "claude");
        assert_eq!(ids, ["default", "claude-a", "claude-b"]);

        let default = entry(&result, "claude", "default");
        assert_eq!(default["account_name"], DEFAULT_ACCOUNT_NAME);
        assert_eq!(default["plan"], "max");
        assert_eq!(default["fetched_at"], 1_000);
        assert_eq!(default["exhausted"], false);
        assert_eq!(default["exhausted_until"], Value::Null);
        assert_eq!(default["can_open"], true);
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
    #[tokio::test]
    async fn an_account_with_a_used_up_window_that_has_not_reset_is_exhausted() {
        let (state, _dir) = shared_console_state("quotas-exhausted");
        available(&state, &[AgentId::CODEX]);
        insert_account(&state, AgentId::CODEX, "codex-a");
        insert_account(&state, AgentId::CODEX, "codex-b");
        let future = now_millis() + 3_600_000;
        for (id, window) in [
            ("codex-a", weekly(100.0, future)),
            ("codex-b", weekly(100.0, 1)),
        ] {
            state.account_quotas.hold_in_dir_for_test(
                &quota(AgentId::CODEX, Some(id), vec![window]),
                &format!("/tmp/{id}"),
            );
        }

        let result = list_agents(&state, &arguments(json!({}))).await.unwrap();

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

    /// The accounts are listed in the order automatic account selection weighs them: the order
    /// the user keeps them in, not the order they were added in.
    #[tokio::test]
    async fn accounts_are_listed_in_the_order_the_user_keeps_them_in() {
        let (state, _dir) = shared_console_state("quotas-order");
        available(&state, &[AgentId::CLAUDE]);
        insert_account(&state, AgentId::CLAUDE, "claude-a");
        insert_account(&state, AgentId::CLAUDE, "claude-b");
        state
            .store
            .reorder_accounts(
                AgentId::CLAUDE,
                &[
                    Some("claude-b".to_string()),
                    None,
                    Some("claude-a".to_string()),
                ],
            )
            .unwrap();

        let result = list_agents(&state, &arguments(json!({ "agent": "claude" })))
            .await
            .unwrap();
        let ids = account_ids(&result, "claude");
        assert_eq!(ids, ["claude-b", "default", "claude-a"]);
    }

    /// A turned-off account is left out, the default account of its agent included; the others of
    /// the agent stay, and turning it back on lists it again.
    #[tokio::test]
    async fn a_turned_off_account_is_not_listed() {
        let (state, _dir) = shared_console_state("quotas-disabled");
        available(&state, &[AgentId::CLAUDE]);
        insert_account(&state, AgentId::CLAUDE, "claude-a");
        insert_account(&state, AgentId::CLAUDE, "claude-b");
        state.store.set_account_disabled("claude-a", true).unwrap();
        state
            .store
            .set_default_account_disabled(AgentId::CLAUDE, true)
            .unwrap();

        let result = list_agents(&state, &arguments(json!({ "agent": "claude" })))
            .await
            .unwrap();
        let ids = account_ids(&result, "claude");
        assert_eq!(ids, ["claude-b"]);

        state.store.set_account_disabled("claude-a", false).unwrap();
        let result = list_agents(&state, &arguments(json!({ "agent": "claude" })))
            .await
            .unwrap();
        assert_eq!(account_ids(&result, "claude").len(), 2);
        assert_eq!(entry(&result, "claude", "claude-a")["can_open"], true);
    }

    /// `can_open` is false, with the reason, for an account that is used up or whose change of
    /// sharing is running: the two things besides being turned off that make automatic account
    /// selection pass an account over.
    #[tokio::test]
    async fn can_open_says_whether_a_session_could_open_on_the_account_and_why_not() {
        let (state, _dir) = shared_console_state("quotas-can-open");
        available(&state, &[AgentId::CODEX]);
        insert_account(&state, AgentId::CODEX, "codex-a");
        insert_account(&state, AgentId::CODEX, "codex-b");
        let future = now_millis() + 3_600_000;
        state.account_quotas.hold_in_dir_for_test(
            &quota(AgentId::CODEX, Some("codex-a"), vec![weekly(100.0, future)]),
            "/tmp/codex-a",
        );
        let account = state.store.get_account("codex-b").unwrap().unwrap();
        let (_, lock) = state.lock_account_sharing(&account, |_, _| Ok(())).unwrap();

        let result = list_agents(&state, &arguments(json!({}))).await.unwrap();
        assert_eq!(entry(&result, "codex", "default")["can_open"], true);
        assert_eq!(
            entry(&result, "codex", "default")["cannot_open_reason"],
            Value::Null
        );
        assert_eq!(entry(&result, "codex", "codex-a")["can_open"], false);
        assert_eq!(
            entry(&result, "codex", "codex-a")["cannot_open_reason"],
            USED_UP
        );
        assert_eq!(entry(&result, "codex", "codex-b")["can_open"], false);
        assert_eq!(
            entry(&result, "codex", "codex-b")["cannot_open_reason"],
            SHARING_IN_PROGRESS
        );

        drop(lock);
        let result = list_agents(&state, &arguments(json!({}))).await.unwrap();
        assert_eq!(entry(&result, "codex", "codex-b")["can_open"], true);
    }

    /// An account the last reading found not logged in cannot open a session, and says so; one
    /// whose reading failed for another reason still can.
    #[tokio::test]
    async fn can_open_is_false_for_an_account_that_is_not_logged_in() {
        let (state, _dir) = shared_console_state("quotas-not-signed-in");
        available(&state, &[AgentId::CODEX]);
        insert_account(&state, AgentId::CODEX, "codex-a");
        insert_account(&state, AgentId::CODEX, "codex-b");
        let unavailable = |code: crate::protocol::Code| {
            Some(QuotaUnavailable::new(code, "x", &[("agent", "Codex")]))
        };
        for (id, code) in [
            (
                "codex-a",
                crate::protocol::quota_unavailable_code::NOT_SIGNED_IN,
            ),
            (
                "codex-b",
                crate::protocol::quota_unavailable_code::TIMED_OUT,
            ),
        ] {
            let mut held = quota(AgentId::CODEX, Some(id), Vec::new());
            held.unavailable = unavailable(code);
            state
                .account_quotas
                .hold_in_dir_for_test(&held, &format!("/tmp/{id}"));
        }

        let result = list_agents(&state, &arguments(json!({}))).await.unwrap();

        assert_eq!(entry(&result, "codex", "codex-a")["can_open"], false);
        assert_eq!(
            entry(&result, "codex", "codex-a")["cannot_open_reason"],
            NOT_SIGNED_IN
        );
        assert_eq!(entry(&result, "codex", "codex-b")["can_open"], true);
    }

    /// `auto_select_account` is the setting as it stands.
    #[tokio::test]
    async fn the_result_carries_the_auto_select_account_setting() {
        let (state, _dir) = shared_console_state("quotas-auto-select");
        available(&state, &[AgentId::CLAUDE]);

        let off = list_agents(&state, &arguments(json!({}))).await.unwrap();
        assert_eq!(off["auto_select_account"], false);

        state.store.set_auto_select_account(true).unwrap();
        let on = list_agents(&state, &arguments(json!({}))).await.unwrap();
        assert_eq!(on["auto_select_account"], true);
    }

    /// `agent` narrows the list to one agent. An agent that is not installed is not listed, nor is
    /// one not on the added list; one not detected yet lists its accounts, not read yet.
    #[tokio::test]
    async fn the_agent_argument_filters_and_an_uninstalled_agent_is_not_listed() {
        let (state, _dir) = shared_console_state("agents-filter");
        state.set_agent_availability(
            [AgentId::GROK, AgentId::TEST]
                .into_iter()
                .map(|agent| AgentAvailability {
                    agent,
                    availability: Availability::Unavailable,
                    default_account_dir: None,
                })
                .collect(),
        );
        insert_account(&state, AgentId::GROK, "grok-a");

        let all = list_agents(&state, &arguments(json!({}))).await.unwrap();
        let agents: Vec<&Value> = agents_of(&all)
            .into_iter()
            .map(|entry| &entry["agent"])
            .collect();
        assert_eq!(agents, ["claude", "codex"]);
        assert_eq!(entry(&all, "claude", "default")["fetched_at"], Value::Null);

        let codex = list_agents(&state, &arguments(json!({ "agent": "codex" })))
            .await
            .unwrap();
        assert_eq!(agents_of(&codex).len(), 1);
        assert_eq!(account_ids(&codex, "codex"), ["default"]);

        let grok = list_agents(&state, &arguments(json!({ "agent": "grok" })))
            .await
            .unwrap();
        assert_eq!(grok["agents"], json!([]));

        state
            .store
            .set_added_agents_if_undecided(&[AgentId::CLAUDE])
            .unwrap();
        let added = list_agents(&state, &arguments(json!({}))).await.unwrap();
        assert_eq!(agents_of(&added).len(), 1);
        assert_eq!(agent_entry(&added, "claude")["agent"], "claude");

        let err = list_agents(&state, &arguments(json!({ "agent": "gemini" })))
            .await
            .expect_err("unknown agent");
        assert!(err.to_string().contains("gemini"), "{err}");
    }

    /// The agents are the added ones in the order the user keeps them in, each with its tags (an
    /// empty list when it has none); a removed agent is not listed, so its tags are not shown, and
    /// they come back when it is added again.
    #[tokio::test]
    async fn the_agents_carry_their_tags_in_the_added_order_and_a_removed_agent_is_not_listed() {
        let (state, _dir) = shared_console_state("agents-tags");
        available(&state, &[AgentId::CLAUDE, AgentId::CODEX, AgentId::GROK]);
        state
            .store
            .set_added_agents_if_undecided(&[AgentId::CODEX, AgentId::CLAUDE, AgentId::GROK])
            .unwrap();
        state
            .store
            .set_agent_tags(
                AgentId::CODEX,
                vec!["review".to_string(), "tests".to_string()],
            )
            .unwrap();
        state
            .store
            .set_agent_tags(AgentId::GROK, vec!["search".to_string()])
            .unwrap();

        let result = list_agents(&state, &arguments(json!({}))).await.unwrap();
        let agents: Vec<&Value> = agents_of(&result)
            .into_iter()
            .map(|entry| &entry["agent"])
            .collect();
        assert_eq!(agents, ["codex", "claude", "grok"]);
        assert_eq!(
            agent_entry(&result, "codex")["tags"],
            json!(["review", "tests"])
        );
        assert_eq!(agent_entry(&result, "claude")["tags"], json!([]));
        assert_eq!(agent_entry(&result, "grok")["tags"], json!(["search"]));
        assert_eq!(account_ids(&result, "codex"), ["default"]);

        state.store.remove_added_agent(AgentId::GROK).unwrap();
        let removed = list_agents(&state, &arguments(json!({}))).await.unwrap();
        assert_eq!(agents_of(&removed).len(), 2);
        assert!(!removed.to_string().contains("search"), "{removed}");

        state.store.add_added_agent(AgentId::GROK).unwrap();
        let back = list_agents(&state, &arguments(json!({ "agent": "grok" })))
            .await
            .unwrap();
        assert_eq!(agent_entry(&back, "grok")["tags"], json!(["search"]));
    }

    /// `refresh` takes new readings before answering, once per account a minute: a second call
    /// within it is answered from what is held, with no error, and a call without `refresh`
    /// never reads.
    #[tokio::test]
    async fn refresh_reads_an_account_once_a_minute_and_answers_from_the_cache_in_between() {
        let (state, _dir) = shared_console_state("quotas-refresh");
        available(&state, &[AgentId::CLAUDE]);
        let reads = Arc::new(AtomicUsize::new(0));
        let shell: ShellSource = {
            let reads = reads.clone();
            Arc::new(move || {
                reads.fetch_add(1, Ordering::SeqCst);
                anyhow::bail!("no shell under test")
            })
        };
        let ask = |refresh: bool| {
            let (state, shell) = (state.clone(), shell.clone());
            async move {
                let arguments = arguments(json!({ "agent": "claude", "refresh": refresh }));
                list_agents_with(&state, &arguments, shell).await
            }
        };

        let plain = ask(false).await.unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        assert_eq!(
            entry(&plain, "claude", "default")["unavailable"],
            NOT_READ_YET
        );

        let refreshed = ask(true).await.unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        assert_ne!(
            entry(&refreshed, "claude", "default")["unavailable"],
            NOT_READ_YET
        );

        let again = ask(true).await.unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        assert_eq!(again, refreshed);

        let err = list_agents(&state, &arguments(json!({ "refresh": "yes" })))
            .await
            .expect_err("not a boolean");
        assert!(err.to_string().contains("refresh"), "{err}");
    }
}
