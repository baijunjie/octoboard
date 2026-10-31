//! An agent's models as `list_agent_models` reports them, why there is no list, and what a model
//! change did. See "Agent models" in `apps/daemon/PROTOCOL.md`.

use serde::Serialize;

use super::codes::Code;
use super::errors::{params, Params};

/// What a `set_session_model` did (`Event::SessionModelSet`).
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelChangeOutcome {
    /// The session had those values already: nothing was written or restarted.
    Unchanged,
    /// The session had no process: the values were recorded for its next resume.
    Recorded,
    /// The session's process was ended and relaunched with the values.
    Restarted,
}

/// One model an agent offers, as `list_agent_models` reports it.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentModel {
    /// What Octoboard passes to the agent as its model: an alias Claude Code resolves itself, or
    /// the identifier the Codex or Grok Build catalogue lists.
    pub id: String,
    /// The name to show; the catalogue's own where it has one, else `id`.
    pub name: String,
    /// The reasoning-effort levels to offer for the model, in the agent's own spelling: the
    /// catalogue's own, in its order, where the model's entry gives them, else the agent's whole
    /// measured set; empty only for a model that takes none. Never unknown. A level the model
    /// does not advertise (the fallback is per agent, not per model) can still be refused by the
    /// agent, or quietly lowered, as Claude Code does with a level its model does not support, so
    /// a listed level is one to offer, not a promise that it is honoured.
    pub efforts: Vec<String>,
}

/// Why `list_agent_models` has no list, as a coded message of the same shape as an `error`. It
/// travels inside a reply because failing to ask the agent must never fail the form that asked.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelsUnavailable {
    pub code: Code,
    pub params: Params,
    pub message: String,
}

impl ModelsUnavailable {
    pub fn new(code: Code, message: impl Into<String>, pairs: &[(&str, &str)]) -> Self {
        Self {
            code,
            params: params(pairs),
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::{Event, ModelChangeOutcome};

    /// The answer to `set_session_model` names its outcome, and only a restart says whether it
    /// continued the session's task.
    #[test]
    fn a_session_model_set_reply_serializes_its_outcome() {
        let reply = |outcome, continued| {
            serde_json::to_value(Event::SessionModelSet {
                id: Some("request-1".to_owned()),
                session: "s-1".to_owned(),
                outcome,
                continued,
            })
            .unwrap()
        };
        assert_eq!(
            reply(ModelChangeOutcome::Restarted, Some(true)),
            serde_json::json!({
                "type": "session_model_set",
                "id": "request-1",
                "session": "s-1",
                "outcome": "restarted",
                "continued": true,
            })
        );
        for (outcome, word) in [
            (ModelChangeOutcome::Unchanged, "unchanged"),
            (ModelChangeOutcome::Recorded, "recorded"),
        ] {
            assert_eq!(
                reply(outcome, None),
                serde_json::json!({
                    "type": "session_model_set",
                    "id": "request-1",
                    "session": "s-1",
                    "outcome": word,
                })
            );
        }
    }
}
