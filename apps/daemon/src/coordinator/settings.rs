//! The daemon-wide settings, the choice for each setting of a provider, and the broadcast of both.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;

use crate::accounts::quotas;
use crate::project_git;
use crate::protocol::{AgentId, GitWorkflowSettingsChanges, KeepAwakeWhen, GIT_WORKFLOW_ITEMS};
use crate::providers::{SettingDescriptor, SettingValue};
use crate::state::{broadcast_settings, AppState, RecordKey, SETTINGS_FIELDS};

use super::validate::{absolute_path, normalise_tags};

/// Refuses, with `unreadable_request`, a choice for a setting `agent` does not declare or a value
/// the setting does not take; `None` (clearing the choice) is always taken.
fn declared_setting(
    agent: AgentId,
    setting: &str,
    value: Option<&SettingValue>,
) -> Result<&'static SettingDescriptor> {
    let refused = crate::agents::launch_settings::refused_setting;
    let Some(descriptor) = agent.descriptor().setting(setting) else {
        return Err(refused(format!(
            "{} has no setting `{setting}`",
            agent.wire_name()
        )));
    };
    match value {
        Some(value) if !descriptor.accepts(value) => Err(refused(format!(
            "{}'s setting `{setting}` does not take {}",
            agent.wire_name(),
            serde_json::to_string(value)?
        ))),
        _ => Ok(descriptor),
    }
}

/// Records the user's choice for a setting of `agent`'s provider (`None` clears it) and broadcasts
/// the settings unless the stored choice is unchanged.
pub(super) fn set_provider_setting(
    state: &Arc<AppState>,
    agent: AgentId,
    setting: &str,
    value: Option<SettingValue>,
) -> Result<()> {
    declared_setting(agent, setting, value.as_ref())?;
    if state
        .store
        .set_provider_setting(agent, setting, value.as_ref())?
    {
        broadcast_settings(state)?;
    }
    Ok(())
}

/// What `update_settings` changes; a field left at its default is left alone.
#[derive(Default)]
pub struct SettingsChanges {
    pub auto_close_idle_shell_terminals: Option<bool>,
    pub auto_pull_repositories: Option<bool>,
    pub auto_delete_unresumable_archives: Option<bool>,
    pub auto_approve_session_requests: Option<bool>,
    pub auto_approve_resume_requests: Option<bool>,
    pub auto_approve_account_switches: Option<bool>,
    pub auto_approve_handoffs: Option<bool>,
    pub ask_questions_as_choices: Option<bool>,
    pub keep_sessions_in_own_project: Option<bool>,
    /// Refused unless it is a tag the interface offers.
    pub interface_language: Option<String>,
    pub keep_awake: Option<bool>,
    pub keep_awake_when: Option<KeepAwakeWhen>,
    pub auto_select_account: Option<bool>,
    pub auto_switch_on_usage_limit: Option<bool>,
    pub auto_start_quota_windows: Option<bool>,
    pub resume_sessions_on_start: Option<bool>,
    /// Blank text goes back to the built-in default.
    pub default_clone_dir: Option<String>,
    pub git_workflow: GitWorkflowSettingsChanges,
    /// The user's choices for provider settings, by provider id and then setting id; a setting
    /// left out is left alone. Refused, applying none of the request, for a provider that is no
    /// provider, a setting it does not declare or a value the setting does not take.
    pub provider_settings: BTreeMap<String, BTreeMap<String, SettingValue>>,
    /// By agent wire name, replacing that agent's tags, which are normalised as a project's are;
    /// an agent left out is left alone. Refused for a name that is no agent.
    pub agent_tags: std::collections::BTreeMap<String, Vec<String>>,
}

impl SettingsChanges {
    /// The versioned settings paths these changes write (see `crate::state::FieldVersions`), in no
    /// particular order, named by the constants that list the paths of a settings record.
    fn paths(&self) -> Vec<String> {
        // Each in the order of `SETTINGS_FIELDS` and `GIT_WORKFLOW_ITEMS`; the tests pair every
        // flag with its name.
        let fields: [bool; SETTINGS_FIELDS.len()] = [
            self.auto_close_idle_shell_terminals.is_some(),
            self.auto_pull_repositories.is_some(),
            self.auto_delete_unresumable_archives.is_some(),
            self.auto_approve_session_requests.is_some(),
            self.auto_approve_resume_requests.is_some(),
            self.auto_approve_account_switches.is_some(),
            self.auto_approve_handoffs.is_some(),
            self.ask_questions_as_choices.is_some(),
            self.keep_sessions_in_own_project.is_some(),
            self.interface_language.is_some(),
            self.keep_awake.is_some(),
            self.keep_awake_when.is_some(),
            self.auto_select_account.is_some(),
            self.auto_switch_on_usage_limit.is_some(),
            self.auto_start_quota_windows.is_some(),
            self.default_clone_dir.is_some(),
            self.resume_sessions_on_start.is_some(),
        ];
        let git = &self.git_workflow;
        let git_items: [bool; GIT_WORKFLOW_ITEMS.len()] = [
            git.development.is_some(),
            git.squash_before_delivery.is_some(),
            git.rebase_before_delivery.is_some(),
            git.delivery.is_some(),
            git.revert_gate.is_some(),
        ];
        let mut paths: Vec<String> = SETTINGS_FIELDS
            .into_iter()
            .zip(fields)
            .filter(|(_, set)| *set)
            .map(|(name, _)| name.to_string())
            .collect();
        paths.extend(
            GIT_WORKFLOW_ITEMS
                .into_iter()
                .zip(git_items)
                .filter(|(_, set)| *set)
                .map(|(item, _)| format!("git_workflow.{item}")),
        );
        for (agent, settings) in &self.provider_settings {
            paths.extend(
                settings
                    .keys()
                    .map(|setting| format!("provider_settings.{agent}.{setting}")),
            );
        }
        paths.extend(
            self.agent_tags
                .keys()
                .map(|agent| format!("agent_tags.{agent}")),
        );
        paths
    }

    /// Of `versions` (the settings versions a client read), those of the paths these changes
    /// write.
    pub fn versions_read(&self, versions: &BTreeMap<String, u64>) -> BTreeMap<String, u64> {
        let paths = self.paths();
        versions
            .iter()
            .filter(|(path, _)| paths.contains(path))
            .map(|(path, version)| (path.clone(), *version))
            .collect()
    }
}

/// [`update_settings`] for a write based on the versions `base` (none: unchecked), checked and
/// applied under the lock of the checked writes (see [`AppState::checked_write`]).
pub(crate) async fn update_settings_checked(
    state: &Arc<AppState>,
    changes: SettingsChanges,
    base: &BTreeMap<String, u64>,
) -> Result<()> {
    let _checked = state.checked_write(&RecordKey::Settings, base).await?;
    update_settings(state, changes)
}

/// Applies `changes`: absent means "leave it alone", as in `update_project`. Broadcasts
/// `settings_updated` only when something actually changed, the trusted-directories pattern.
pub(crate) fn update_settings(state: &Arc<AppState>, changes: SettingsChanges) -> Result<()> {
    let SettingsChanges {
        auto_close_idle_shell_terminals,
        auto_pull_repositories,
        auto_delete_unresumable_archives,
        auto_approve_session_requests,
        auto_approve_resume_requests,
        auto_approve_account_switches,
        auto_approve_handoffs,
        ask_questions_as_choices,
        keep_sessions_in_own_project,
        interface_language,
        keep_awake,
        keep_awake_when,
        auto_select_account,
        auto_switch_on_usage_limit,
        auto_start_quota_windows,
        resume_sessions_on_start,
        default_clone_dir,
        git_workflow,
        provider_settings,
        agent_tags,
    } = changes;
    if let Some(tag) = &interface_language {
        if !crate::interface_language::is_offered(tag) {
            anyhow::bail!("`{tag}` is not a language the interface is offered in");
        }
    }
    let mut choices = Vec::new();
    for (name, settings) in provider_settings {
        let Some(agent) = AgentId::from_wire_name(&name) else {
            anyhow::bail!("`{name}` is not an agent");
        };
        for (setting, value) in settings {
            declared_setting(agent, &setting, Some(&value))?;
            choices.push((agent, setting, value));
        }
    }
    let agent_tags = agent_tags
        .into_iter()
        .map(|(name, tags)| match AgentId::from_wire_name(&name) {
            Some(agent) => Ok((agent, normalise_tags(tags))),
            None => Err(anyhow::anyhow!("`{name}` is not an agent")),
        })
        .collect::<Result<Vec<_>>>()?;
    // Checked before anything is written, so a refused directory leaves the other field unapplied
    // too rather than half the request taking effect.
    let default_clone_dir = match default_clone_dir {
        Some(text) if text.trim().is_empty() => Some(None),
        Some(text) => Some(Some(absolute_path(&text)?.to_string_lossy().into_owned())),
        None => None,
    };
    let mut changed = false;
    if let Some(value) = auto_close_idle_shell_terminals {
        changed |= state.store.set_auto_close_idle_shell_terminals(value)?;
    }
    let mut pull_turned_on = false;
    let mut quota_windows_turned_on = false;
    if let Some(value) = auto_pull_repositories {
        let updated = state.store.set_auto_pull_repositories(value)?;
        pull_turned_on = updated && value;
        changed |= updated;
    }
    if let Some(value) = auto_delete_unresumable_archives {
        changed |= state.store.set_auto_delete_unresumable_archives(value)?;
    }
    if let Some(value) = auto_approve_session_requests {
        changed |= state.store.set_auto_approve_session_requests(value)?;
    }
    if let Some(value) = auto_approve_resume_requests {
        changed |= state.store.set_auto_approve_resume_requests(value)?;
    }
    if let Some(value) = auto_approve_account_switches {
        changed |= state.store.set_auto_approve_account_switches(value)?;
    }
    if let Some(value) = auto_approve_handoffs {
        changed |= state.store.set_auto_approve_handoffs(value)?;
    }
    if let Some(value) = ask_questions_as_choices {
        changed |= state.store.set_ask_questions_as_choices(value)?;
    }
    if let Some(value) = keep_sessions_in_own_project {
        changed |= state.store.set_keep_sessions_in_own_project(value)?;
    }
    if let Some(tag) = interface_language {
        changed |= state.store.set_interface_language(&tag)?;
    }
    if let Some(value) = keep_awake {
        changed |= state.store.set_keep_awake(value)?;
    }
    if let Some(value) = keep_awake_when {
        changed |= state.store.set_keep_awake_when(value)?;
    }
    if let Some(value) = auto_select_account {
        changed |= state.store.set_auto_select_account(value)?;
    }
    if let Some(value) = auto_switch_on_usage_limit {
        changed |= state.store.set_auto_switch_on_usage_limit(value)?;
        if !value {
            state.quota_recovery.clear();
        }
    }
    if let Some(value) = auto_start_quota_windows {
        let updated = state.store.set_auto_start_quota_windows(value)?;
        quota_windows_turned_on = updated && value;
        changed |= updated;
    }
    if let Some(value) = resume_sessions_on_start {
        changed |= state.store.set_resume_sessions_on_start(value)?;
    }
    if let Some(value) = default_clone_dir {
        changed |= state.store.set_default_clone_dir(value.as_deref())?;
    }
    for (agent, setting, value) in choices {
        changed |= state
            .store
            .set_provider_setting(agent, &setting, Some(&value))?;
    }
    for (agent, tags) in agent_tags {
        changed |= state.store.set_agent_tags(agent, tags)?;
    }
    changed |= state.store.change_git_workflow_settings(&git_workflow)?;
    if !changed {
        return Ok(());
    }
    broadcast_settings(state)?;
    // Turning the automatic pull on takes effect at once rather than at the client's next
    // five-minute sweep: the branches already known to be behind are fast-forwarded now (see
    // `project_git::status::pull_behind_projects`).
    if pull_turned_on {
        project_git::status::pull_behind_projects(state);
    }
    // Likewise the readings already held are looked over now, so an account whose window has not
    // started is not left waiting for its next reading.
    if quota_windows_turned_on {
        quotas::kick_held(state);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
