//! The tool with which a session reads the models an agent offers, `list_agent_models`: the values
//! the `model` and `effort` of every tool that starts or configures a session must be among. It
//! reads the listing the application's own pickers read, with its cache, so a call runs no agent
//! CLI while a listing is fresh.

use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Map, Value};

use crate::launch_settings;
use crate::state::AppState;

use super::args::{optional_string, parse_agent, required_str};
use super::settings::resolve_account;

/// The models `agent` offers under `account` (an account's id or name, else the default account),
/// each with its efforts. When the agent cannot be asked, `models` is empty and `unavailable` says
/// why, as the application's own listing does.
pub(super) async fn list_agent_models(
    state: &Arc<AppState>,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let agent = parse_agent(required_str(arguments, "agent")?)?;
    let account = match optional_string(arguments, "account") {
        Some(wanted) => resolve_account(state, agent, &wanted)?,
        None => None,
    };
    let listing = launch_settings::agent_listing(state, agent, &account).await?;
    Ok(json!({
        "agent": agent,
        "account": account.as_deref().unwrap_or("default"),
        "models": listing.models,
        "unavailable": listing
            .unavailable
            .map(|reason| json!({ "code": reason.code, "message": reason.message })),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_models::Listing;
    use crate::mcp::exec::call;
    use crate::mcp::exec::tests::{arguments, peer_state};
    use crate::protocol::{Account, Agent, AgentModel, ModelsUnavailable};

    async fn list(state: &Arc<AppState>, caller: &str, input: Value) -> Result<Value> {
        call(state, caller, "list_agent_models", &arguments(input)).await
    }

    fn account(id: &str, agent: Agent, name: &str) -> Account {
        Account {
            id: id.to_string(),
            agent,
            name: name.to_string(),
            config_dir: format!("/tmp/{id}"),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
        }
    }

    /// Every kind of caller reads an agent's models with their efforts, of the default account
    /// unless one is named, by id or by name.
    #[tokio::test]
    async fn the_models_of_an_agent_are_listed_with_their_efforts() {
        let (state, _dir) = peer_state("list-agent-models");
        state
            .store
            .insert_account(&account("claude-work", Agent::Claude, "Work"))
            .unwrap();
        // A console session, an unbound project session and a bound one.
        for caller in ["a", "starter", "mine"] {
            let listed = list(&state, caller, json!({ "agent": "claude" }))
                .await
                .unwrap();
            assert_eq!(listed["agent"], "claude", "{caller}");
            assert_eq!(listed["account"], "default", "{caller}");
            let models = listed["models"].as_array().unwrap();
            assert!(!models.is_empty(), "{caller}");
            assert!(models[0]["id"].is_string() && models[0]["name"].is_string());
            assert!(models[0]["efforts"].is_array());
            assert!(listed["unavailable"].is_null(), "{caller}");
        }
        for wanted in ["claude-work", "work", "Work", "default"] {
            let named = list(&state, "a", json!({ "agent": "claude", "account": wanted }))
                .await
                .unwrap();
            let expected = if wanted == "default" {
                "default"
            } else {
                "claude-work"
            };
            assert_eq!(named["account"], expected, "{wanted}");
            assert!(!named["models"].as_array().unwrap().is_empty());
        }
    }

    /// The catalogue read is the named account's: a listing held for its directory is what comes
    /// back, and a listing that failed comes back empty with its reason.
    #[tokio::test]
    async fn the_named_accounts_own_catalogue_is_read() {
        let (state, _dir) = peer_state("list-agent-models-account");
        for (id, name) in [("codex-ok", "Ok"), ("codex-down", "Down")] {
            state
                .store
                .insert_account(&account(id, Agent::Codex, name))
                .unwrap();
        }
        let held = |id: &str, listing: Listing| {
            state.agent_models.get_or_load(
                Agent::Codex,
                Some(std::path::Path::new(&format!("/tmp/{id}"))),
                || listing,
            );
        };
        held(
            "codex-ok",
            Listing {
                models: vec![AgentModel {
                    id: "gpt-test".to_string(),
                    name: "GPT Test".to_string(),
                    efforts: vec!["low".to_string(), "high".to_string()],
                }],
                unavailable: None,
            },
        );
        held(
            "codex-down",
            Listing {
                models: Vec::new(),
                unavailable: Some(ModelsUnavailable::new(
                    "test_down",
                    "the agent is down",
                    &[],
                )),
            },
        );
        for caller in ["a", "starter", "mine"] {
            let listed = list(&state, caller, json!({ "agent": "codex", "account": "Ok" }))
                .await
                .unwrap();
            assert_eq!(listed["account"], "codex-ok", "{caller}");
            assert_eq!(
                listed["models"],
                json!([{ "id": "gpt-test", "name": "GPT Test", "efforts": ["low", "high"] }]),
                "{caller}"
            );
            assert!(listed["unavailable"].is_null(), "{caller}");

            let down = list(
                &state,
                caller,
                json!({ "agent": "codex", "account": "codex-down" }),
            )
            .await
            .unwrap();
            assert_eq!(down["models"], json!([]), "{caller}");
            assert_eq!(
                down["unavailable"],
                json!({ "code": "test_down", "message": "the agent is down" }),
                "{caller}"
            );
        }
    }

    /// An agent that is not one, a missing agent, an account that does not exist and one of
    /// another agent are refused.
    #[tokio::test]
    async fn an_unknown_agent_or_account_is_refused() {
        let (state, _dir) = peer_state("list-agent-models-refused");
        state
            .store
            .insert_account(&account("codex-work", Agent::Codex, "Work"))
            .unwrap();
        for (input, reason) in [
            (json!({}), "`agent` is required"),
            (json!({ "agent": "gemini" }), "is not one of"),
            (
                json!({ "agent": "claude", "account": "missing" }),
                "missing",
            ),
            // An account of Codex is no account of Claude, by id or by name.
            (
                json!({ "agent": "claude", "account": "codex-work" }),
                "codex-work",
            ),
            (json!({ "agent": "claude", "account": "Work" }), "Work"),
        ] {
            let err = list(&state, "a", input.clone()).await.expect_err("refused");
            assert!(err.to_string().contains(reason), "{input}: {err}");
        }
    }
}
