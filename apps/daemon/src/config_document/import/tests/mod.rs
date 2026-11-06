mod creating;
mod failures;
mod inspecting;
pub(super) mod progress;
mod reading;
mod settings;

use super::*;
use crate::config_document::export;
use crate::protocol::{
    DirectoryChoiceKind, DirectoryOffers, GitWorkflowSettings, Launch, Project, ProjectSource,
};
use crate::test_support::{app_state, code_of, ScratchDir, PATIENCE};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(super) fn project(name: &str, path: &str) -> DocumentProject {
    DocumentProject {
        name: name.to_string(),
        path: path.to_string(),
        source: ProjectSource::Local,
        remote_url: None,
        default_agent: None,
        pinned: false,
        tags: Vec::new(),
        description: None,
        accounts: Default::default(),
        launch: Default::default(),
        git_workflow: Default::default(),
    }
}

pub(super) fn console(name: &str, projects: Vec<DocumentProject>) -> DocumentConsole {
    DocumentConsole {
        name: name.to_string(),
        icon: None,
        default_agent: AgentId::CODEX,
        project_session_agent: None,
        project_session_launch: Default::default(),
        accounts: Default::default(),
        launch: Default::default(),
        console_session_prompt: None,
        projects,
    }
}

pub(super) fn document(consoles: Vec<DocumentConsole>) -> serde_json::Value {
    serde_json::to_value(ConfigurationDocument {
        format: CONFIGURATION_FORMAT.to_string(),
        version: CONFIGURATION_VERSION,
        exported_by: None,
        settings: None,
        accounts: None,
        consoles,
    })
    .unwrap()
}

pub(super) fn account(id: &str, agent: AgentId, name: &str) -> Account {
    Account {
        id: id.to_string(),
        agent,
        name: name.to_string(),
        config_dir: format!("/accounts/{id}"),
        share_default_config: false,
        share_default_config_incomplete: false,
        disabled: false,
    }
}

pub(super) fn directory(parent: &Path, name: &str) -> PathBuf {
    let path = parent.join(name);
    std::fs::create_dir_all(&path).expect("directory");
    path
}

pub(super) fn at(console: usize, project: usize, choice: DirectoryChoice) -> ProjectResolution {
    ProjectResolution {
        console,
        project,
        choice,
    }
}

/// An import that leaves the application settings alone, as a request that does not ask for
/// them.
async fn import(
    state: &Arc<AppState>,
    document: &serde_json::Value,
    resolutions: Vec<ProjectResolution>,
) -> Result<Vec<ImportedConsole>> {
    Ok(apply(state, document, resolutions, false, None, true)
        .await?
        .consoles)
}

fn with_settings(
    mut document: serde_json::Value,
    auto_pull: bool,
    auto_delete: bool,
) -> serde_json::Value {
    document["settings"] = json!({
        "auto_close_idle_shell_terminals": false,
        "auto_pull_repositories": auto_pull,
        "auto_delete_unresumable_archives": auto_delete,
        "auto_select_account": false,
        "auto_switch_on_usage_limit": false,
        "sidebar_order_by_status": true,
        "auto_start_quota_windows": false,
        "auto_approve_session_requests": false,
        "auto_approve_resume_requests": false,
        "auto_approve_account_switches": false,
        "auto_approve_handoffs": false,
        "ask_questions_as_choices": true,
        "git_workflow": GitWorkflowSettings::default(),
        "provider_settings": {},
        "agent_tags": {},
    });
    document
}

pub(super) fn stored_projects(state: &Arc<AppState>) -> Vec<Project> {
    state.store.list_projects().unwrap()
}

/// A state whose default clone directory is `clone_dir`.
fn state_cloning_into(label: &str, clone_dir: &Path) -> (Arc<AppState>, ScratchDir) {
    let (state, dir) = app_state(label);
    state
        .store
        .set_default_clone_dir(Some(&clone_dir.to_string_lossy()))
        .unwrap();
    (state, dir)
}
