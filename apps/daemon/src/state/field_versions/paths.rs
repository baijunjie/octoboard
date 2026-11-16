//! The versioned fields of each record a form edits: the one place that lists their paths and reads
//! each path's value out of a record. A reader that needs the settings paths without a record
//! (`SettingsChanges`, which paths a write touches) takes them from [`SETTINGS_FIELDS`] and
//! `GIT_WORKFLOW_ITEMS` rather than spelling them again.
//!
//! A path is dotted and its vocabulary fixed: a field that has one value per agent, per git
//! workflow item or per provider setting has one path for each, enumerated from the registry and
//! not from the record, so a record always yields every path it ever can.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::protocol::{Account, Console, Project, Settings, GIT_WORKFLOW_ITEMS};
use crate::providers::AgentId;

/// A record's value at each of its versioned paths.
pub(crate) type Paths = BTreeMap<String, Value>;

/// The settings fields `update_settings` takes as a path of their own.
pub(crate) const SETTINGS_FIELDS: [&str; 19] = [
    "auto_close_idle_shell_terminals",
    "auto_pull_repositories",
    "auto_delete_unresumable_archives",
    "auto_approve_session_requests",
    "auto_approve_resume_requests",
    "auto_approve_account_switches",
    "auto_approve_handoffs",
    "ask_questions_as_choices",
    "keep_sessions_in_own_project",
    "interface_language",
    "keep_awake",
    "keep_awake_when",
    "auto_select_account",
    "auto_switch_on_usage_limit",
    "auto_start_quota_windows",
    "default_clone_dir",
    "resume_sessions_on_start",
    "daemon_update_check",
    "daemon_update_channel",
];

/// The launch fields of one agent's pair.
const LAUNCH_FIELDS: [&str; 2] = ["model", "effort"];

/// `record` as JSON, which every path is read out of.
fn json(record: &impl Serialize) -> Value {
    serde_json::to_value(record).unwrap_or_default()
}

/// The value at `keys` below `value`, `null` where there is none.
fn at(value: &Value, keys: &[&str]) -> Value {
    keys.iter()
        .try_fold(value, |value, key| value.get(*key))
        .cloned()
        .unwrap_or_default()
}

/// A path of a field, read from the record at `keys`.
fn field(paths: &mut Paths, value: &Value, path: &str, keys: &[&str]) {
    paths.insert(path.to_string(), at(value, keys));
}

/// The paths of a field kept once per agent, `prefix.<agent>`, read from `prefix` of the record.
fn per_agent(paths: &mut Paths, value: &Value, prefix: &str) {
    for agent in AgentId::ALL {
        let name = agent.wire_name();
        field(paths, value, &format!("{prefix}.{name}"), &[prefix, name]);
    }
}

/// The paths of a launch pair kept once per agent, `prefix.<agent>.model` and `.effort`, and of
/// each compatibility switch the agent lets a launch hold,
/// `prefix.<agent>.compatibility.<setting>`.
fn per_agent_launch(paths: &mut Paths, value: &Value, prefix: &str) {
    for agent in AgentId::ALL {
        let name = agent.wire_name();
        for launch in LAUNCH_FIELDS {
            let path = format!("{prefix}.{name}.{launch}");
            field(paths, value, &path, &[prefix, name, launch]);
        }
        for setting in agent.descriptor().overridable_settings() {
            let path = format!("{prefix}.{name}.compatibility.{}", setting.id);
            field(
                paths,
                value,
                &path,
                &[prefix, name, "compatibility", setting.id],
            );
        }
    }
}

/// The paths of the git workflow items, `git_workflow.<item>`.
fn git_workflow(paths: &mut Paths, value: &Value) {
    for item in GIT_WORKFLOW_ITEMS {
        let path = format!("git_workflow.{item}");
        field(paths, value, &path, &["git_workflow", item]);
    }
}

pub(crate) fn settings_paths(settings: &Settings) -> Paths {
    let value = json(settings);
    let mut paths = Paths::new();
    for name in SETTINGS_FIELDS {
        field(&mut paths, &value, name, &[name]);
    }
    git_workflow(&mut paths, &value);
    for agent in AgentId::ALL {
        let name = agent.wire_name();
        // An agent with no tags has no entry; its tags are the empty list.
        let tags = at(&value, &["agent_tags", name]);
        let tags = if tags.is_null() {
            Value::Array(Vec::new())
        } else {
            tags
        };
        paths.insert(format!("agent_tags.{name}"), tags);
        for setting in agent.descriptor().settings {
            let path = format!("provider_settings.{name}.{}", setting.id);
            field(
                &mut paths,
                &value,
                &path,
                &["provider_settings", name, setting.id],
            );
        }
    }
    paths
}

pub(crate) fn console_paths(console: &Console) -> Paths {
    let value = json(console);
    let mut paths = Paths::new();
    for name in [
        "name",
        "default_agent",
        "project_session_agent",
        "icon",
        "console_session_prompt",
    ] {
        field(&mut paths, &value, name, &[name]);
    }
    per_agent(&mut paths, &value, "accounts");
    per_agent_launch(&mut paths, &value, "launch");
    per_agent_launch(&mut paths, &value, "project_session_launch");
    paths
}

pub(crate) fn project_paths(project: &Project) -> Paths {
    let value = json(project);
    let mut paths = Paths::new();
    for name in ["name", "default_agent", "pinned", "tags", "description"] {
        field(&mut paths, &value, name, &[name]);
    }
    per_agent(&mut paths, &value, "accounts");
    per_agent_launch(&mut paths, &value, "launch");
    git_workflow(&mut paths, &value);
    paths
}

pub(crate) fn account_paths(account: &Account) -> Paths {
    let value = json(account);
    let mut paths = Paths::new();
    for name in ["name", "config_dir", "disabled", "share_default_config"] {
        field(&mut paths, &value, name, &[name]);
    }
    paths
}
