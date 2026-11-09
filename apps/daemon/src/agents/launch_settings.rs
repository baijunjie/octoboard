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
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use crate::adapter::provider;
use crate::agents::models::{Listing, COMMAND_TIMEOUT};
use crate::protocol::{
    error_code, AgentId, CodedError, DefaultModelSource, Launch, LaunchChange, PerAgent,
};
use crate::state::AppState;
use crate::text::trimmed_or_none;

/// A model or effort as stored.
pub(crate) fn normalize_launch_setting(text: Option<&str>) -> Option<String> {
    trimmed_or_none(text)
}

/// Each agent's launch pair as stored.
pub(crate) fn normalized_launches(launches: PerAgent<Launch>) -> PerAgent<Launch> {
    launches.map(|_, launch| Launch {
        model: normalize_launch_setting(launch.model.as_deref()),
        effort: normalize_launch_setting(launch.effort.as_deref()),
    })
}

/// A session override keeps its presence even when blank means explicitly unset.
pub(crate) fn normalize_session_launch_setting(
    value: Option<Option<String>>,
) -> Option<Option<String>> {
    value.map(|text| normalize_launch_setting(text.as_deref()))
}

impl From<PerAgent<LaunchChange>> for HashMap<AgentId, LaunchChange> {
    /// The map the in-memory changes carry, leaving out an agent whose two fields are absent.
    fn from(changes: PerAgent<LaunchChange>) -> Self {
        changes
            .into_iter()
            .filter(|(_, change)| !change.is_keep())
            .collect()
    }
}

/// What a record was newly given for one agent's model and effort, and what to check it against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCheck {
    pub agent: AgentId,
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
        agent: AgentId,
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

    /// The check for a record whose own model, when it has none, is `fallback` — the model its
    /// sessions take from the record behind it — which then decides what efforts are on offer.
    pub fn behind(mut self, fallback: &Option<String>) -> Self {
        if self.effective_model.is_none() {
            self.effective_model = fallback.clone();
        }
        self
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

/// Applies `change` to a record's launch pair, blank read as unset, and returns what is
/// left to check: only a value that differs from the one stored.
pub fn apply_change(
    agent: AgentId,
    account_id: Option<String>,
    launch: &mut Launch,
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
        (&mut launch.model, &change.model, &mut check.model),
        (&mut launch.effort, &change.effort, &mut check.effort),
    ] {
        if let Some(value) = change {
            let value = normalize_launch_setting(value.as_deref());
            if value != *field {
                *set = value.clone();
            }
            *field = value;
        }
    }
    check.effective_model = launch.model.clone();
    check
}

/// The models `agent` offers under the account `account_id` (`None` for the default account), from
/// the cache or by asking the agent, which is why this runs off the runtime.
pub async fn agent_listing(
    state: &Arc<AppState>,
    agent: AgentId,
    account_id: &Option<String>,
) -> Result<Listing> {
    let config_dir = state.store.account_dir(account_id)?;
    let request = Resolve {
        configuration: false,
        ..Resolve::new(agent, config_dir)
    };
    Ok(resolve(state, request).await?.listing)
}

/// The model a session resolves to when no setting names one, and where that came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultModel {
    pub id: String,
    pub source: DefaultModelSource,
}

/// The models `agent` offers under the account `account_id` and the model a session of the
/// account with no setting naming one resolves to as far as the account can tell: for an agent
/// that names a default model, the model its own configuration names (see
/// [`ModelCatalogue::configured_model`]), else the best one the account may use. `None` for the
/// other agents, and for one that names a default when nothing is known, which leaves the choice
/// to the agent. Project settings are not known per account, so a session may resolve to another
/// model at launch (see [`launch_default_model`]).
///
/// [`ModelCatalogue::configured_model`]: crate::agents::models::ModelCatalogue::configured_model
pub async fn agent_models(
    state: &Arc<AppState>,
    agent: AgentId,
    account_id: &Option<String>,
) -> Result<(Listing, Option<DefaultModel>)> {
    let config_dir = state.store.account_dir(account_id)?;
    let resolved = resolve(state, Resolve::new(agent, config_dir)).await?;
    let default = match resolved.configured {
        Some(id) => Some(DefaultModel {
            id,
            source: DefaultModelSource::Configured,
        }),
        None => resolved.listing.default_model().map(|id| DefaultModel {
            id: id.to_string(),
            source: DefaultModelSource::BestAvailable,
        }),
    };
    Ok((resolved.listing, default))
}

/// The model to pass to a session of `agent` that no setting names one for: for an agent that names
/// a default model, the best model the account may use, so that it never falls to the one the agent
/// would pick on its own, which does not check what the account may use. `None` leaves the choice
/// to the agent: for the other agents, when the account's models are not known, and for one that
/// names a default when its own configuration names a model (the user's, a managed one, an
/// environment variable, or the project's in `cwd`), which `--model` would override. `config_dir`
/// is the session's pinned account directory and `shell_env` the environment the launch runs in,
/// taken here when the caller has none.
pub async fn launch_default_model(
    state: &Arc<AppState>,
    agent: AgentId,
    config_dir: Option<String>,
    cwd: &Path,
    shell_env: Option<HashMap<String, String>>,
) -> Result<Option<String>> {
    let request = Resolve {
        shell_env,
        cwd: Some(cwd.to_path_buf()),
        for_launch: true,
        ..Resolve::new(agent, config_dir)
    };
    let resolved = resolve(state, request).await?;
    Ok(match resolved.configured {
        Some(_) => None,
        None => resolved.listing.default_model().map(str::to_string),
    })
}

/// What [`resolve`] found.
struct Resolved {
    listing: Listing,
    /// The model the agent's own configuration names; `None` for an agent that names no default
    /// model and when the request did not ask for it.
    configured: Option<String>,
}

/// What [`resolve`] is asked.
struct Resolve {
    agent: AgentId,
    config_dir: Option<String>,
    /// The environment to read the agent's files in; else a snapshot is taken.
    shell_env: Option<HashMap<String, String>>,
    /// The session's working directory, whose project settings count too.
    cwd: Option<PathBuf>,
    /// Whether the configured model is wanted, or the listing alone.
    configuration: bool,
    /// Whether this runs for a launch, which takes the launch's snapshot, rather than for a
    /// picker or a tool call, which takes the scheduled one and so never forks a login shell.
    for_launch: bool,
}

impl Resolve {
    fn new(agent: AgentId, config_dir: Option<String>) -> Self {
        Self {
            agent,
            config_dir,
            shell_env: None,
            cwd: None,
            configuration: true,
            for_launch: false,
        }
    }
}

/// The listing for the agent and, for one that names a default model and when asked, what its
/// configuration says. The listing of a provider that names a default model is cheap to read (see
/// [`crate::agents::models::ModelCatalogue::names_default_model`]), so it is read afresh each time
/// rather than cached: a cached one would let a default that is no longer true drive launches.
async fn resolve(state: &Arc<AppState>, request: Resolve) -> Result<Resolved> {
    let state = state.clone();
    Ok(tokio::task::spawn_blocking(move || {
        let Resolve {
            agent,
            config_dir,
            shell_env,
            cwd,
            configuration,
            for_launch,
        } = request;
        let dir = config_dir.as_deref().map(Path::new);
        let catalogue = provider(agent).catalogue();
        if !catalogue.names_default_model() {
            let listing = state
                .agent_models
                .get_or_load(agent, dir, || crate::agents::models::load(agent, dir));
            return Resolved {
                listing,
                configured: None,
            };
        }
        // A failed snapshot still reads the provider's default configuration directory.
        let shell_env = shell_env.unwrap_or_else(|| {
            let tools = state.host_tools.get();
            let take = if for_launch {
                tools.shell_env
            } else {
                tools.scheduled_shell_env
            };
            take().unwrap_or_default()
        });
        let listing = catalogue.list(&shell_env, dir, COMMAND_TIMEOUT);
        #[cfg(test)]
        let listing = state.agent_models.seeded(agent, dir).unwrap_or(listing);
        let configured = configuration
            .then(|| catalogue.configured_model(&shell_env, dir, cwd.as_deref()))
            .flatten();
        Resolved {
            listing,
            configured,
        }
    })
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::claude::models::fixtures::{
        claude_home, write_claude_account, CLAUDE_CATALOGUE,
    };

    /// Claude Code gets the first model of its listing the account may use, unless its own
    /// configuration names one (then it is left to apply it), and the other agents and an
    /// unknown account are left to the agent's own default.
    #[tokio::test]
    async fn an_unset_claude_model_defaults_to_the_best_the_account_may_use() {
        let (state, dir) = crate::test_support::app_state("launch-settings-default-model");
        let env = claude_home(&dir, "[]", Some(CLAUDE_CATALOGUE));
        let cwd = dir.join("project");
        std::fs::create_dir_all(cwd.join(".claude")).unwrap();
        let default = |agent: AgentId, env: &HashMap<String, String>| {
            launch_default_model(&state, agent, None, &cwd, Some(env.clone()))
        };
        assert_eq!(
            default(AgentId::CLAUDE, &env).await.unwrap().as_deref(),
            Some("opus")
        );
        assert_eq!(default(AgentId::CODEX, &env).await.unwrap(), None);
        // Each place Claude Code reads a model from leaves the choice to it.
        let mut named = env.clone();
        named.insert("ANTHROPIC_MODEL".to_string(), "sonnet".to_string());
        assert_eq!(default(AgentId::CLAUDE, &named).await.unwrap(), None);
        let user = dir.join(".claude/settings.json");
        std::fs::create_dir_all(user.parent().unwrap()).unwrap();
        std::fs::write(&user, r#"{"model":"haiku"}"#).unwrap();
        assert_eq!(default(AgentId::CLAUDE, &env).await.unwrap(), None);
        std::fs::write(&user, "{}").unwrap();
        assert!(default(AgentId::CLAUDE, &env).await.unwrap().is_some());
        std::fs::write(
            cwd.join(".claude/settings.local.json"),
            r#"{"model":"opus"}"#,
        )
        .unwrap();
        assert_eq!(default(AgentId::CLAUDE, &env).await.unwrap(), None);
        // Nothing known to be usable leaves the choice to Claude Code.
        let empty = HashMap::from([("HOME".to_string(), "/nonexistent".to_string())]);
        assert_eq!(default(AgentId::CLAUDE, &empty).await.unwrap(), None);
    }

    /// What an unset picker shows is the model Claude Code's account-level configuration names,
    /// else the best the account may use; the project's settings are not part of it.
    #[tokio::test]
    async fn the_default_model_of_an_account_is_what_its_configuration_names_else_the_best() {
        let (state, dir) = crate::test_support::app_state("launch-settings-account-default");
        let pinned = dir.join("pinned");
        std::fs::create_dir_all(&pinned).unwrap();
        write_claude_account(
            &pinned.join(".claude.json"),
            &pinned,
            "[]",
            Some(CLAUDE_CATALOGUE),
        );
        let account = crate::protocol::Account {
            id: "claude-work".to_string(),
            agent: AgentId::CLAUDE,
            name: "Work".to_string(),
            config_dir: pinned.to_string_lossy().into_owned(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
            versions: Default::default(),
        };
        state.store.insert_account(&account).unwrap();
        let id = Some("claude-work".to_string());
        let (listing, default) = agent_models(&state, AgentId::CLAUDE, &id).await.unwrap();
        assert_eq!(listing.models[1].version.as_deref(), Some("Opus 5.5"));
        assert_eq!(default.as_ref().map(|d| d.id.as_str()), Some("opus"));
        assert_eq!(default.unwrap().source, DefaultModelSource::BestAvailable);
        std::fs::write(pinned.join("settings.json"), r#"{"model":"claude-x-1"}"#).unwrap();
        let (_, default) = agent_models(&state, AgentId::CLAUDE, &id).await.unwrap();
        assert_eq!(
            default,
            Some(DefaultModel {
                id: "claude-x-1".to_string(),
                source: DefaultModelSource::Configured
            })
        );
        // The other agents have none to name, whatever Claude Code's files hold.
        let (_, default) = agent_models(&state, AgentId::CODEX, &None).await.unwrap();
        assert_eq!(default, None);
    }

    /// A record that sets no model of its own is checked against the model it takes from the record
    /// behind it, and one that sets its own keeps it.
    #[test]
    fn the_model_behind_a_record_decides_the_efforts_only_while_it_sets_none() {
        let behind = Some("fallback".to_string());
        let none = LaunchCheck::given(AgentId::CLAUDE, None, None, Some("high")).behind(&behind);
        assert_eq!(none.effective_model.as_deref(), Some("fallback"));
        assert_eq!(none.model, None);
        let own =
            LaunchCheck::given(AgentId::CLAUDE, None, Some("own"), Some("high")).behind(&behind);
        assert_eq!(own.effective_model.as_deref(), Some("own"));
    }
}
