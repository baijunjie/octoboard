//! The model and reasoning effort a console, a project or a session is given, checked against what
//! the agent offers.
//!
//! The list is the one the `list_agent_models` request answers with, so a value the dialogs would
//! not offer is refused wherever it comes from: the control socket, the console session's tools, a
//! session opened with an override. Only what a request newly *gives* is checked — a stored value
//! the agent has since dropped must not stop an unrelated edit — and a list that could not be read
//! checks nothing, since the dialogs stay usable without one and so does this.
//!
//! Asking the agent can take as long as its command timeout, so a caller that is about to write a
//! record builds its checks and runs them first, then applies its change to the record as it is
//! afterwards, rather than holding a copy across the wait.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use crate::agent_models::Listing;
use crate::protocol::{error_code, Agent, CodedError, Present};
use crate::state::AppState;

/// A model or effort as stored: trimmed, with blank read as unset.
pub(crate) fn normalize_launch_setting(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// A session override keeps its presence even when blank means explicitly unset.
pub(crate) fn normalize_session_launch_setting(
    value: Option<Option<String>>,
) -> Option<Option<String>> {
    value.map(|text| normalize_launch_setting(text.as_deref()))
}

/// A change to one agent's model and effort: each absent for "leave it alone", `Some(None)` or a
/// blank string for the agent's own default.
#[derive(Default)]
pub struct LaunchChange {
    pub model: Present<String>,
    pub effort: Present<String>,
}

/// The per-agent `(agent, model, effort)` triples of a request as the map the changes carry,
/// leaving out an agent whose two are both absent.
pub fn changes_of(
    fields: [(Agent, Present<String>, Present<String>); 3],
) -> HashMap<Agent, LaunchChange> {
    fields
        .into_iter()
        .filter(|(_, model, effort)| model.is_some() || effort.is_some())
        .map(|(agent, model, effort)| (agent, LaunchChange { model, effort }))
        .collect()
}

/// What a record was newly given for one agent's model and effort, and what to check it against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCheck {
    pub agent: Agent,
    /// The account whose catalogue is asked: the one the record's sessions of `agent` would run
    /// under (`None` for the default account).
    pub account_id: Option<String>,
    /// Trimmed and non-blank; `None` where nothing new was given for the model.
    pub model: Option<String>,
    pub effort: Option<String>,
    /// The record's model once the change is applied, which decides what efforts are on offer.
    pub effective_model: Option<String>,
}

impl LaunchCheck {
    /// For a record created with `model` and `effort`, blank read as unset.
    pub fn given(
        agent: Agent,
        account_id: Option<String>,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Self {
        let model = normalize_launch_setting(model);
        Self {
            agent,
            account_id,
            effective_model: model.clone(),
            model,
            effort: normalize_launch_setting(effort),
        }
    }

    /// Whether there is anything to check.
    pub fn is_empty(&self) -> bool {
        self.model.is_none() && self.effort.is_none()
    }

    /// Refuses a model or effort the agent does not offer with `invalid_launch_setting`.
    pub async fn run(&self, state: &Arc<AppState>) -> Result<()> {
        if self.is_empty() {
            return Ok(());
        }
        let agent = self.agent;
        let listing = agent_listing(state, agent, &self.account_id).await?;
        if listing.unavailable.is_some() || listing.models.is_empty() {
            return Ok(());
        }
        let refused = |field: &str, value: &str, offered: Vec<&str>| {
            CodedError::raised(
                error_code::INVALID_LAUNCH_SETTING,
                format!(
                    "`{value}` is not a {field} {} offers; it offers: {}",
                    agent.label(),
                    offered.join(", ")
                ),
                &[("agent", agent.label()), ("field", field), ("value", value)],
            )
        };
        if let Some(model) = &self.model {
            if !listing.models.iter().any(|offered| offered.id == *model) {
                let offered = listing.models.iter().map(|m| m.id.as_str()).collect();
                return Err(refused("model", model, offered));
            }
        }
        if let Some(effort) = &self.effort {
            // The model's own levels when it is one the list holds, else every listed level.
            let chosen = listing
                .models
                .iter()
                .find(|offered| Some(&offered.id) == self.effective_model.as_ref());
            let mut offered: Vec<&str> = Vec::new();
            let levels = match chosen {
                Some(model) => std::slice::from_ref(model),
                None => listing.models.as_slice(),
            };
            for level in levels.iter().flat_map(|model| &model.efforts) {
                if !offered.contains(&level.as_str()) {
                    offered.push(level);
                }
            }
            if !offered.contains(&effort.as_str()) {
                return Err(refused("effort", effort, offered));
            }
        }
        Ok(())
    }
}

/// Applies `change` to a record's pair of launch fields, blank read as unset, and returns what is
/// left to check: only a value that differs from the one stored.
pub fn apply_change(
    agent: Agent,
    account_id: Option<String>,
    model: &mut Option<String>,
    effort: &mut Option<String>,
    change: &LaunchChange,
) -> LaunchCheck {
    let mut check = LaunchCheck {
        agent,
        account_id,
        model: None,
        effort: None,
        effective_model: None,
    };
    for (field, change, set) in [
        (&mut *model, &change.model, &mut check.model),
        (&mut *effort, &change.effort, &mut check.effort),
    ] {
        if let Some(value) = change {
            let value = normalize_launch_setting(value.as_deref());
            if value != *field {
                *set = value.clone();
            }
            *field = value;
        }
    }
    check.effective_model = model.clone();
    check
}

/// The models `agent` offers under the account `account_id` (`None` for the default account), from
/// the cache or by asking the agent, which is why this runs off the runtime.
pub async fn agent_listing(
    state: &Arc<AppState>,
    agent: Agent,
    account_id: &Option<String>,
) -> Result<Listing> {
    let config_dir = state.store.account_dir(account_id)?;
    let state = state.clone();
    Ok(tokio::task::spawn_blocking(move || {
        let dir = config_dir.as_deref().map(Path::new);
        state
            .agent_models
            .get_or_load(agent, dir, || crate::agent_models::load(agent, dir))
    })
    .await?)
}
