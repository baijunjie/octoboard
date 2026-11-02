//! The daemon-wide settings, the approval override each agent's sessions launch with, and the
//! broadcast of both.

use std::sync::Arc;

use anyhow::Result;

use crate::accounts::quotas;
use crate::git_workflow::GitWorkflowSettingsChanges;
use crate::project_git;
use crate::protocol::{Agent, ApprovalReviewer, ClaudeApprovalMode, Event, Session};
use crate::state::{broadcast_settings, AppState};
use crate::store::Store;

use super::validate::{absolute_path, normalise_tags};

/// Where one agent's launch override is kept: one setting for the agent, applied to a session of
/// any of its accounts. Codex's approval reviewer and Claude Code's approval mode are handled
/// alike through it.
pub(super) struct LaunchOverride<T> {
    agent: Agent,
    get: fn(&Store) -> crate::store::Result<Option<T>>,
    set: fn(&Store, Option<T>) -> crate::store::Result<()>,
}

pub(super) const CODEX_REVIEWER: LaunchOverride<ApprovalReviewer> = LaunchOverride {
    agent: Agent::Codex,
    get: Store::codex_approval_reviewer,
    set: Store::set_codex_approval_reviewer,
};

pub(super) const CLAUDE_MODE: LaunchOverride<ClaudeApprovalMode> = LaunchOverride {
    agent: Agent::Claude,
    get: Store::claude_approval_mode,
    set: Store::set_claude_approval_mode,
};

impl<T: PartialEq + Copy> LaunchOverride<T> {
    /// Resolves the override at every launch: the agent's setting, whichever account the session
    /// uses, and nothing for a session of another agent.
    pub(super) fn for_session(
        &self,
        state: &Arc<AppState>,
        session: &Session,
    ) -> Result<Option<T>> {
        if session.agent != self.agent {
            return Ok(None);
        }
        Ok((self.get)(&state.store)?)
    }

    /// Sets the agent's override and broadcasts the settings unless it already holds that value.
    pub(super) fn set(&self, state: &Arc<AppState>, value: Option<T>) -> Result<()> {
        if (self.get)(&state.store)? == value {
            return Ok(());
        }
        (self.set)(&state.store, value)?;
        broadcast_settings(state)?;
        Ok(())
    }
}

/// What `update_settings` changes; a field left at its default is left alone.
#[derive(Default)]
pub struct SettingsChanges {
    pub auto_pull_repositories: Option<bool>,
    pub auto_delete_unresumable_archives: Option<bool>,
    pub auto_approve_session_requests: Option<bool>,
    pub auto_approve_resume_requests: Option<bool>,
    pub auto_approve_account_switches: Option<bool>,
    pub auto_approve_handoffs: Option<bool>,
    pub ask_questions_as_choices: Option<bool>,
    /// Refused unless it is a tag the interface offers.
    pub interface_language: Option<String>,
    pub auto_select_account: Option<bool>,
    pub auto_switch_on_usage_limit: Option<bool>,
    pub sidebar_order_by_status: Option<bool>,
    pub auto_start_quota_windows: Option<bool>,
    /// Blank text goes back to the built-in default.
    pub default_clone_dir: Option<String>,
    pub git_workflow: GitWorkflowSettingsChanges,
    /// By agent wire name; an agent left out is left alone. Refused for Claude Code, which has no
    /// such setting, and for a name that is no agent.
    pub claude_compatibility: std::collections::BTreeMap<String, bool>,
    /// By agent wire name, replacing that agent's tags, which are normalised as a project's are;
    /// an agent left out is left alone. Refused for a name that is no agent.
    pub agent_tags: std::collections::BTreeMap<String, Vec<String>>,
}

/// Applies `changes`: absent means "leave it alone", as in `update_project`. Broadcasts
/// `settings_updated` only when something actually changed, the trusted-directories pattern.
pub(crate) fn update_settings(state: &Arc<AppState>, changes: SettingsChanges) -> Result<()> {
    let SettingsChanges {
        auto_pull_repositories,
        auto_delete_unresumable_archives,
        auto_approve_session_requests,
        auto_approve_resume_requests,
        auto_approve_account_switches,
        auto_approve_handoffs,
        ask_questions_as_choices,
        interface_language,
        auto_select_account,
        auto_switch_on_usage_limit,
        sidebar_order_by_status,
        auto_start_quota_windows,
        default_clone_dir,
        git_workflow,
        claude_compatibility,
        agent_tags,
    } = changes;
    if let Some(tag) = &interface_language {
        if !crate::interface_language::is_offered(tag) {
            anyhow::bail!("`{tag}` is not a language the interface is offered in");
        }
    }
    let claude_compatibility = claude_compatibility
        .into_iter()
        .map(|(name, enabled)| match Agent::from_wire_name(&name) {
            Some(agent) if agent.has_claude_compatibility() => Ok((agent, enabled)),
            _ => Err(anyhow::anyhow!(
                "`{name}` has no Claude compatibility setting; it applies to codex and grok"
            )),
        })
        .collect::<Result<Vec<_>>>()?;
    let agent_tags = agent_tags
        .into_iter()
        .map(|(name, tags)| match Agent::from_wire_name(&name) {
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
    if let Some(tag) = interface_language {
        changed |= state.store.set_interface_language(&tag)?;
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
    if let Some(value) = sidebar_order_by_status {
        changed |= state.store.set_sidebar_order_by_status(value)?;
    }
    if let Some(value) = auto_start_quota_windows {
        let updated = state.store.set_auto_start_quota_windows(value)?;
        quota_windows_turned_on = updated && value;
        changed |= updated;
    }
    if let Some(value) = default_clone_dir {
        changed |= state.store.set_default_clone_dir(value.as_deref())?;
    }
    for (agent, enabled) in claude_compatibility {
        changed |= state.store.set_claude_compatibility(agent, enabled)?;
    }
    for (agent, tags) in agent_tags {
        changed |= state.store.set_agent_tags(agent, tags)?;
    }
    changed |= state.store.change_git_workflow_settings(&git_workflow)?;
    if !changed {
        return Ok(());
    }
    state.broadcast(Event::SettingsUpdated {
        settings: state.store.get_settings()?,
    });
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
