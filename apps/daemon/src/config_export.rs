//! The configuration document: what one console's configuration, or all of the application's,
//! looks like once it has been lifted out of this machine's database. `apps/daemon/PROTOCOL.md`
//! ("Configuration export") is the specification; this module is its Rust form and builds it.
//!
//! The document is the input of a later import as well as a file the user keeps and shares, so its
//! shape is an interface and not an internal detail: [`FORMAT`] and [`VERSION`] identify it, and
//! nothing in it is a database id.
//!
//! What it leaves out is deliberate. A session, a page, a transcript and a trust record are state
//! of this machine and not configuration. A console's `workdir` is left out because the daemon
//! derives it from the console's id when the console is created, so it would carry that id and
//! Octoboard's own layout, and an import makes its own. An account travels as its name alone
//! ([`AccountName`]) and never as an id or a `config_dir`, since an id means nothing elsewhere and
//! the directory is this machine's layout. `Settings.default_clone_dir` stays behind for the same
//! reason: an import places projects under the *importing* machine's own.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::git_remote;
use crate::git_workflow::{GitWorkflowOverride, GitWorkflowSettings};
use crate::protocol::{
    Account, Agent, CodedError, Console, Project, ProjectAccount, ProjectSource,
};
use crate::store::Store;

/// Names the kind of file, so one that is not a configuration document is told apart from one
/// whose version is unsupported.
pub const FORMAT: &str = "octoboard-configuration";

/// Raised when a change makes an older reader misread a document: a field removed or given another
/// meaning. A field added, which a reader that does not know it ignores, does not raise it.
///
/// Raising it needs a step in `config_upgrade` that upgrades a document of the previous version,
/// so an export made before stays importable.
pub const VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigurationDocument {
    pub format: String,
    pub version: u32,
    /// The Octoboard version that produced the document, for diagnosing one that misbehaves. An
    /// import never reads it, and it is absent in a document exported before it existed.
    pub exported_by: Option<String>,
    /// The application settings that are not console-scoped. Present in a full export only: its
    /// absence is what says the document holds one console.
    pub settings: Option<DocumentSettings>,
    pub consoles: Vec<DocumentConsole>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentSettings {
    /// Documents exported before the setting was renamed carry it as `auto_sync_repositories`.
    #[serde(alias = "auto_sync_repositories")]
    pub auto_pull_repositories: bool,
    /// Absent in a document exported before the setting existed: importing it leaves the setting
    /// as it is.
    #[serde(default)]
    pub auto_delete_unresumable_archives: Option<bool>,
    /// Absent in a document exported before the setting existed: importing it leaves the setting
    /// as it is.
    #[serde(default)]
    pub auto_select_account: Option<bool>,
    /// Absent in a document exported before the setting existed: importing it leaves the setting
    /// as it is.
    #[serde(default)]
    pub sidebar_order_by_status: Option<bool>,
    /// Absent in a document exported before the setting existed: importing it leaves the setting
    /// as it is.
    #[serde(default)]
    pub auto_start_quota_windows: Option<bool>,
    /// Absent in a document exported before the setting was carried: importing it leaves the
    /// setting as it is.
    #[serde(default)]
    pub auto_approve_session_requests: Option<bool>,
    /// Absent in a document exported before the setting was carried: importing it leaves the
    /// setting as it is.
    #[serde(default)]
    pub auto_approve_resume_requests: Option<bool>,
    /// Absent in a document exported before the setting was carried: importing it leaves the
    /// setting as it is.
    #[serde(default)]
    pub auto_approve_account_switches: Option<bool>,
    /// Absent in a document exported before the setting was carried: importing it leaves the
    /// setting as it is.
    #[serde(default)]
    pub auto_approve_handoffs: Option<bool>,
    /// Absent in a document exported before the setting was carried: importing it leaves the
    /// setting as it is.
    #[serde(default)]
    pub ask_questions_as_choices: Option<bool>,
    /// The application-wide git workflow settings. Absent in a document exported before they
    /// existed: importing it leaves them as they are. A version 1 document carries them as
    /// `worktrees`, which `config_upgrade` converts.
    #[serde(default)]
    pub git_workflow: Option<GitWorkflowSettings>,
    /// The keyboard shortcuts the exporting client has changed, by command id, which only the client
    /// reads and writes: the client adds them to a full export it saves (an empty object when it has
    /// none changed, which on import means every shortcut at its default) and applies them from an
    /// import. The daemon builds none, stores them nowhere and does not read them, so they pass
    /// through an import's reply as they came. Absent in a document exported before they were
    /// carried, or one a client saved without them: importing it leaves the client's shortcuts as
    /// they are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyboard_shortcuts: Option<serde_json::Value>,
}

/// A reference to an account: its name, which is unique within its agent. The agent is the one
/// whose field holds the reference (`claude_account` is a Claude Code account), so it is not
/// repeated here, where a hand-edited document could contradict it. The importing machine rebinds
/// the reference to the account of that agent and name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountName {
    pub name: String,
}

/// A project's choice of account for one agent; [`ProjectAccount`] with the account named rather
/// than numbered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectAccountRef {
    /// Take the console's reference for the agent.
    Inherit,
    /// The agent's default account, even where the console references a named one.
    Default,
    Account {
        name: String,
    },
}

/// A console's own settings and the projects associated with it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentConsole {
    pub name: String,
    pub icon: Option<String>,
    pub default_agent: Agent,
    /// The agent a project session falls back to; absent or `null` follows `default_agent`, as
    /// every document written before it was carried does.
    pub project_session_agent: Option<Agent>,
    /// Each agent's account, by name; `null` is that agent's default account.
    pub claude_account: Option<AccountName>,
    pub codex_account: Option<AccountName>,
    pub grok_account: Option<AccountName>,
    pub claude_console_session_model: Option<String>,
    pub claude_console_session_effort: Option<String>,
    pub codex_console_session_model: Option<String>,
    pub codex_console_session_effort: Option<String>,
    pub grok_console_session_model: Option<String>,
    pub grok_console_session_effort: Option<String>,
    /// The model and effort a project session falls back to; absent in an older document is none.
    pub claude_project_session_model: Option<String>,
    pub claude_project_session_effort: Option<String>,
    pub codex_project_session_model: Option<String>,
    pub codex_project_session_effort: Option<String>,
    pub grok_project_session_model: Option<String>,
    pub grok_project_session_effort: Option<String>,
    /// The user's own addition to the console session's instructions; absent is none.
    pub console_session_prompt: Option<String>,
    pub projects: Vec<DocumentProject>,
}

/// One project's configuration. `path` is an absolute path of the exporting machine, or only the
/// directory's name when the export left paths out. `remote_url` is what was recorded for the
/// project, absent for a directory that is not a repository or a repository with no remote, and
/// also when the export left paths out and the remote is not one that can be cloned from (a local
/// path would reveal one).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentProject {
    pub name: String,
    pub path: String,
    pub source: ProjectSource,
    pub remote_url: Option<String>,
    pub default_agent: Option<Agent>,
    pub pinned: bool,
    pub tags: Vec<String>,
    /// The user's own summary of the project; absent in an older document is none.
    pub description: Option<String>,
    pub claude_account: ProjectAccountRef,
    pub codex_account: ProjectAccountRef,
    pub grok_account: ProjectAccountRef,
    pub claude_session_model: Option<String>,
    pub claude_session_effort: Option<String>,
    pub codex_session_model: Option<String>,
    pub codex_session_effort: Option<String>,
    pub grok_session_model: Option<String>,
    pub grok_session_effort: Option<String>,
    /// The project's own git workflow settings, each item `null` when it follows the
    /// application-wide value. Absent in an older document: every item follows.
    #[serde(default)]
    pub git_workflow: GitWorkflowOverride,
}

/// The document for the console `console`, or for every console and the application settings when
/// it is `None`. A console that does not exist is refused rather than exported as an empty
/// document. With `include_paths` false a project's `path` is only its last segment, so the
/// document carries nothing of this machine's layout; an import takes such a name for what it is
/// (see `config_import`).
pub fn build(
    store: &Store,
    console: Option<&str>,
    include_paths: bool,
) -> Result<ConfigurationDocument> {
    let mut consoles = match console {
        Some(id) => vec![store
            .get_console(id)?
            .ok_or_else(|| CodedError::unknown_console(id))?],
        None => store.list_consoles()?,
    };
    // Ordered here and not left to the queries, which `snapshot` shares and which leave ties
    // undefined: consoles are created in quick succession and a console's projects may share a
    // name. The id only breaks a tie and does not reach the document.
    consoles.sort_by(|a, b| (a.created_at, &a.id).cmp(&(b.created_at, &b.id)));
    let accounts: HashMap<String, Account> = store
        .list_accounts()?
        .into_iter()
        .map(|account| (account.id.clone(), account))
        .collect();
    let mut projects = store.list_projects()?;
    projects.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
    let settings = match console {
        Some(_) => None,
        None => Some(DocumentSettings {
            auto_pull_repositories: store.auto_pull_repositories()?,
            auto_delete_unresumable_archives: Some(store.auto_delete_unresumable_archives()?),
            auto_select_account: Some(store.auto_select_account()?),
            sidebar_order_by_status: Some(store.sidebar_order_by_status()?),
            auto_start_quota_windows: Some(store.auto_start_quota_windows()?),
            auto_approve_session_requests: Some(store.auto_approve_session_requests()?),
            auto_approve_resume_requests: Some(store.auto_approve_resume_requests()?),
            auto_approve_account_switches: Some(store.auto_approve_account_switches()?),
            auto_approve_handoffs: Some(store.auto_approve_handoffs()?),
            ask_questions_as_choices: Some(store.ask_questions_as_choices()?),
            git_workflow: Some(store.git_workflow_settings()?),
            keyboard_shortcuts: None,
        }),
    };

    let consoles = consoles
        .into_iter()
        .map(|console| {
            let own = projects
                .iter()
                .filter(|project| project.console_id == console.id)
                .cloned()
                .collect();
            document_console(&console, own, &accounts, include_paths)
        })
        .collect();
    Ok(ConfigurationDocument {
        format: FORMAT.to_string(),
        version: VERSION,
        exported_by: Some(env!("CARGO_PKG_VERSION").to_string()),
        settings,
        consoles,
    })
}

fn document_console(
    console: &Console,
    projects: Vec<Project>,
    accounts: &HashMap<String, Account>,
    include_paths: bool,
) -> DocumentConsole {
    let reference = |agent: Agent, id: &Option<String>| account_ref(accounts, agent, id.as_deref());
    DocumentConsole {
        name: console.name.clone(),
        icon: console.icon.clone(),
        default_agent: console.default_agent,
        project_session_agent: console.project_session_agent,
        claude_account: reference(Agent::Claude, &console.claude_account_id),
        codex_account: reference(Agent::Codex, &console.codex_account_id),
        grok_account: reference(Agent::Grok, &console.grok_account_id),
        claude_console_session_model: console.claude_console_session_model.clone(),
        claude_console_session_effort: console.claude_console_session_effort.clone(),
        codex_console_session_model: console.codex_console_session_model.clone(),
        codex_console_session_effort: console.codex_console_session_effort.clone(),
        grok_console_session_model: console.grok_console_session_model.clone(),
        grok_console_session_effort: console.grok_console_session_effort.clone(),
        claude_project_session_model: console.claude_project_session_model.clone(),
        claude_project_session_effort: console.claude_project_session_effort.clone(),
        codex_project_session_model: console.codex_project_session_model.clone(),
        codex_project_session_effort: console.codex_project_session_effort.clone(),
        grok_project_session_model: console.grok_project_session_model.clone(),
        grok_project_session_effort: console.grok_project_session_effort.clone(),
        console_session_prompt: console.console_session_prompt.clone(),
        projects: projects
            .into_iter()
            .map(|project| document_project(project, accounts, include_paths))
            .collect(),
    }
}

fn document_project(
    project: Project,
    accounts: &HashMap<String, Account>,
    include_paths: bool,
) -> DocumentProject {
    let reference = |agent: Agent, account: &ProjectAccount| match account {
        ProjectAccount::Inherit => ProjectAccountRef::Inherit,
        ProjectAccount::Default => ProjectAccountRef::Default,
        // An account that is gone is dropped to what deleting it leaves behind: inheriting.
        ProjectAccount::Account { id } => account_ref(accounts, agent, Some(id))
            .map(|AccountName { name }| ProjectAccountRef::Account { name })
            .unwrap_or(ProjectAccountRef::Inherit),
    };
    DocumentProject {
        claude_account: reference(Agent::Claude, &project.claude_account_id),
        codex_account: reference(Agent::Codex, &project.codex_account_id),
        grok_account: reference(Agent::Grok, &project.grok_account_id),
        name: project.name,
        path: if include_paths {
            project.path
        } else {
            directory_name(&project.path)
        },
        source: project.source,
        remote_url: project
            .remote_url
            .filter(|url| include_paths || git_remote::is_clonable_url(url)),
        default_agent: project.default_agent,
        pinned: project.pinned,
        tags: project.tags,
        description: project.description,
        claude_session_model: project.claude_session_model,
        claude_session_effort: project.claude_session_effort,
        codex_session_model: project.codex_session_model,
        codex_session_effort: project.codex_session_effort,
        grok_session_model: project.grok_session_model,
        grok_session_effort: project.grok_session_effort,
        git_workflow: project.git_workflow,
    }
}

/// The last segment of `path`, or an empty string when it has none (the filesystem root), which an
/// import rejects as naming no directory rather than reading `path` as a recorded absolute path.
fn directory_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The reference for the account `id` as the setting of `agent`, or `None` when there is none:
/// nothing was referenced, the account no longer exists, or it belongs to another agent.
fn account_ref(
    accounts: &HashMap<String, Account>,
    agent: Agent,
    id: Option<&str>,
) -> Option<AccountName> {
    let account = accounts.get(id?).filter(|account| account.agent == agent)?;
    Some(AccountName {
        name: account.name.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{error_code, Origin, Page, RequestBody, Role, Session, SessionStatus};
    use crate::store::LOCAL_HOST_ID;

    fn console(id: &str, name: &str) -> Console {
        Console {
            id: id.to_string(),
            name: name.to_string(),
            workdir: format!("/consoles/{id}"),
            console_session_agent: Agent::Codex,
            default_agent: Agent::Codex,
            project_session_agent: None,
            claude_project_session_model: None,
            claude_project_session_effort: None,
            codex_project_session_model: None,
            codex_project_session_effort: None,
            grok_project_session_model: None,
            grok_project_session_effort: None,
            claude_account_id: None,
            codex_account_id: None,
            grok_account_id: None,
            claude_console_session_model: None,
            claude_console_session_effort: None,
            codex_console_session_model: None,
            codex_console_session_effort: None,
            grok_console_session_model: None,
            grok_console_session_effort: None,
            icon: None,
            console_session_prompt: None,
            created_at: 0,
        }
    }

    fn project(id: &str, console_id: &str, name: &str) -> Project {
        Project {
            id: id.to_string(),
            console_id: console_id.to_string(),
            host_id: LOCAL_HOST_ID.to_string(),
            name: name.to_string(),
            path: format!("/work/{name}"),
            default_agent: None,
            source: ProjectSource::Local,
            remote_url: None,
            claude_account_id: ProjectAccount::Inherit,
            codex_account_id: ProjectAccount::Inherit,
            grok_account_id: ProjectAccount::Inherit,
            claude_session_model: None,
            claude_session_effort: None,
            codex_session_model: None,
            codex_session_effort: None,
            grok_session_model: None,
            grok_session_effort: None,
            trust_consent: false,
            pinned: false,
            tags: Vec::new(),
            description: None,
            git_workflow: Default::default(),
        }
    }

    fn account(id: &str, agent: Agent, name: &str, config_dir: &str) -> Account {
        Account {
            id: id.to_string(),
            agent,
            name: name.to_string(),
            config_dir: config_dir.to_string(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
        }
    }

    /// One console holding every kind of project, and two more beside it. Consoles and projects are
    /// inserted in the opposite order to the one they are exported in, so insertion order (what a
    /// query without an `ORDER BY` returns) cannot pass for the right one.
    fn populated(store: &Store) {
        store
            .insert_account(&account(
                "acct-1",
                Agent::Claude,
                "Work",
                "/home/u/.claude-work",
            ))
            .unwrap();
        store
            .insert_account(&account(
                "acct-2",
                Agent::Codex,
                "Team",
                "/home/u/.codex-team",
            ))
            .unwrap();

        let mut hub = console("console-1", "Hub");
        hub.created_at = 2000;
        hub.icon = Some("data:image/png;base64,AAAA".to_string());
        hub.claude_account_id = Some("acct-1".to_string());
        // Names an account that no longer exists, and one of another agent.
        hub.codex_account_id = Some("acct-gone".to_string());
        hub.grok_account_id = Some("acct-1".to_string());
        hub.claude_console_session_model = Some("opus".to_string());
        hub.claude_console_session_effort = Some("some-future-level".to_string());
        hub.console_session_prompt = Some("Be brief.".to_string());
        store.insert_console(&hub).unwrap();
        // Created at the same instant as each other and before `hub`; the id breaks the tie.
        for id in ["console-3", "console-2"] {
            let mut tied = console(id, if id == "console-2" { "Other" } else { "Third" });
            tied.created_at = 1000;
            store.insert_console(&tied).unwrap();
        }
        store.set_auto_pull_repositories(true).unwrap();
        store.set_auto_delete_unresumable_archives(false).unwrap();

        let mut cloned = project("project-1", "console-1", "cloned");
        cloned.source = ProjectSource::Git;
        cloned.remote_url = Some("git@example.com:team/cloned.git".to_string());
        cloned.default_agent = Some(Agent::Grok);
        cloned.pinned = true;
        cloned.tags = vec!["backend".to_string(), "Rust".to_string()];
        cloned.description = Some("Mirrors the team's service.".to_string());
        cloned.claude_account_id = ProjectAccount::Account {
            id: "acct-1".to_string(),
        };
        cloned.codex_account_id = ProjectAccount::Default;
        cloned.grok_account_id = ProjectAccount::Account {
            id: "acct-gone".to_string(),
        };
        cloned.codex_session_model = Some("gpt-x".to_string());
        cloned.codex_session_effort = Some("high".to_string());
        // Not a git repository: exported like any other.
        store
            .insert_project(&project("project-0", "console-1", "plain"))
            .unwrap();
        store.insert_project(&cloned).unwrap();
        // Two projects of one name, in the opposite order to their ids; the paths differ, as two
        // projects of one console cannot share one.
        for (id, path) in [("project-5", "/work/twin-b"), ("project-4", "/work/twin-a")] {
            let mut twin = project(id, "console-1", "twin");
            twin.path = path.to_string();
            store.insert_project(&twin).unwrap();
        }
        store
            .insert_project(&project("project-3", "console-2", "elsewhere"))
            .unwrap();
    }

    /// Everything a document is built from, for the single-console and full-export cases alike.
    fn exported(console: Option<&str>) -> (ConfigurationDocument, String) {
        let (state, _dir) = crate::test_support::app_state("config-export");
        populated(&state.store);
        let document = build(&state.store, console, true).expect("built");
        let text = serde_json::to_string(&document).unwrap();
        (document, text)
    }

    /// The keyboard shortcuts are the client's to add: the daemon's document has none, and one
    /// that carries them reads and writes them back as they were.
    #[test]
    fn the_daemon_builds_no_keyboard_shortcuts_and_passes_a_clients_through() {
        let (_, text) = exported(None);
        let built: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(built["settings"].get("keyboard_shortcuts").is_none());

        let mut carried = built.clone();
        carried["settings"]["keyboard_shortcuts"] = serde_json::json!({});
        let document: ConfigurationDocument = serde_json::from_value(carried).unwrap();
        assert_eq!(
            document.settings.as_ref().unwrap().keyboard_shortcuts,
            Some(serde_json::json!({}))
        );
        let written = serde_json::to_value(&document).unwrap();
        assert_eq!(
            written["settings"]["keyboard_shortcuts"],
            serde_json::json!({})
        );
    }

    #[test]
    fn a_full_export_holds_every_console_with_its_projects_and_the_application_settings() {
        let (document, _) = exported(None);

        assert_eq!(
            (document.format.as_str(), document.version),
            (FORMAT, VERSION)
        );
        assert_eq!(
            document.exported_by.as_deref(),
            Some(env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(
            document.settings,
            Some(DocumentSettings {
                auto_pull_repositories: true,
                auto_delete_unresumable_archives: Some(false),
                auto_select_account: Some(false),
                sidebar_order_by_status: Some(true),
                auto_start_quota_windows: Some(false),
                auto_approve_session_requests: Some(false),
                auto_approve_resume_requests: Some(false),
                auto_approve_account_switches: Some(false),
                auto_approve_handoffs: Some(false),
                ask_questions_as_choices: Some(true),
                git_workflow: Some(GitWorkflowSettings::default()),
                keyboard_shortcuts: None,
            })
        );
        assert_eq!(
            serde_json::to_value(document.settings.as_ref().unwrap()).unwrap()
                ["auto_select_account"],
            false
        );
        let names = |console: &DocumentConsole| {
            console
                .projects
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<_>>()
        };
        let order: Vec<&str> = document.consoles.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(order, ["Other", "Third", "Hub"]);
        assert_eq!(names(&document.consoles[0]), ["elsewhere"]);
        assert_eq!(
            names(&document.consoles[2]),
            ["cloned", "plain", "twin", "twin"]
        );
        let twin_paths: Vec<&str> = document.consoles[2].projects[2..]
            .iter()
            .map(|p| p.path.as_str())
            .collect();
        assert_eq!(
            twin_paths,
            ["/work/twin-a", "/work/twin-b"],
            "the id breaks the tie"
        );

        let hub = &document.consoles[2];
        assert_eq!(hub.icon.as_deref(), Some("data:image/png;base64,AAAA"));
        assert_eq!(hub.claude_console_session_model.as_deref(), Some("opus"));
        assert_eq!(hub.console_session_prompt.as_deref(), Some("Be brief."));
        assert_eq!(
            hub.claude_console_session_effort.as_deref(),
            Some("some-future-level"),
            "a value is exported as it is stored"
        );
        let cloned = &hub.projects[0];
        assert_eq!(cloned.path, "/work/cloned");
        assert_eq!(cloned.source, ProjectSource::Git);
        assert_eq!(
            cloned.remote_url.as_deref(),
            Some("git@example.com:team/cloned.git")
        );
        assert_eq!(cloned.default_agent, Some(Agent::Grok));
        assert!(cloned.pinned);
        assert_eq!(cloned.tags, ["backend", "Rust"]);
        assert_eq!(
            cloned.description.as_deref(),
            Some("Mirrors the team's service.")
        );
        assert_eq!(cloned.codex_session_model.as_deref(), Some("gpt-x"));
        assert_eq!(cloned.codex_session_effort.as_deref(), Some("high"));
        let plain = &hub.projects[1];
        assert_eq!(
            (plain.source, plain.remote_url.as_deref()),
            (ProjectSource::Local, None)
        );
    }

    #[test]
    fn an_export_without_paths_records_each_projects_directory_name_and_nothing_of_the_layout() {
        let (state, _dir) = crate::test_support::app_state("config-export-no-paths");
        populated(&state.store);
        let document = build(&state.store, None, false).expect("built");
        let hub = document
            .consoles
            .iter()
            .find(|console| console.name == "Hub")
            .unwrap();
        assert_eq!(hub.projects[0].path, "cloned");
        let paths: Vec<&str> = document
            .consoles
            .iter()
            .flat_map(|console| &console.projects)
            .map(|project| project.path.as_str())
            .collect();
        assert!(paths.iter().all(|path| !path.contains('/')), "{paths:?}");
        assert!(paths.contains(&"twin-a") && paths.contains(&"twin-b"));
        let text = serde_json::to_string(&document).unwrap();
        assert!(!text.contains("/work"), "{text}");
    }

    #[test]
    fn an_export_without_paths_keeps_only_a_remote_that_can_be_cloned_from() {
        let (state, _dir) = crate::test_support::app_state("config-export-remotes");
        populated(&state.store);
        let mut local = project("project-local", "console-1", "local-remote");
        local.source = ProjectSource::Git;
        local.remote_url = Some("/Users/someone/origin.git".to_string());
        state.store.insert_project(&local).unwrap();

        let projects = |include_paths| {
            build(&state.store, None, include_paths)
                .expect("built")
                .consoles
                .into_iter()
                .flat_map(|console| console.projects)
                .map(|project| (project.name, project.remote_url))
                .collect::<HashMap<_, _>>()
        };
        let without = projects(false);
        assert_eq!(
            without["cloned"].as_deref(),
            Some("git@example.com:team/cloned.git")
        );
        assert_eq!(without["local-remote"], None);
        assert_eq!(
            projects(true)["local-remote"].as_deref(),
            Some("/Users/someone/origin.git")
        );
    }

    #[test]
    fn a_directory_name_is_the_last_segment_and_a_path_without_one_has_none() {
        assert_eq!(directory_name("/work/app"), "app");
        assert_eq!(directory_name("/work/app/"), "app");
        assert_eq!(directory_name("app"), "app");
        assert_eq!(directory_name("/"), "");
    }

    #[test]
    fn a_single_console_export_holds_that_console_alone_and_no_application_settings() {
        let (document, _) = exported(Some("console-2"));

        assert_eq!(document.settings, None);
        assert_eq!(document.consoles.len(), 1);
        assert_eq!(document.consoles[0].name, "Other");
        assert_eq!(document.consoles[0].projects.len(), 1);
        assert_eq!(document.consoles[0].projects[0].name, "elsewhere");
    }

    #[test]
    fn an_account_travels_as_its_name_alone_and_a_reference_to_a_missing_account_is_dropped() {
        let (document, text) = exported(Some("console-1"));

        let hub = &document.consoles[0];
        assert_eq!(
            hub.claude_account,
            Some(AccountName {
                name: "Work".to_string()
            })
        );
        assert_eq!(hub.codex_account, None, "the account is gone");
        assert_eq!(
            hub.grok_account, None,
            "the account belongs to another agent"
        );
        let cloned = &hub.projects[0];
        assert_eq!(
            cloned.claude_account,
            ProjectAccountRef::Account {
                name: "Work".to_string()
            }
        );
        assert_eq!(cloned.codex_account, ProjectAccountRef::Default);
        assert_eq!(cloned.grok_account, ProjectAccountRef::Inherit);
        assert_eq!(hub.projects[1].claude_account, ProjectAccountRef::Inherit);

        for leaked in [
            "acct-1",
            "acct-2",
            "acct-gone",
            "console-1",
            "console-2",
            "console-3",
            "project-1",
            "project-0",
            "project-3",
            "project-4",
            "project-5",
            "workdir",
            "config_dir",
            ".claude-work",
            ".codex-team",
        ] {
            assert!(!text.contains(leaked), "{leaked} is in {text}");
        }
        assert!(text.contains(r#""kind":"account""#) && text.contains("Work"));
    }

    #[test]
    fn nothing_that_belongs_to_this_machine_alone_is_in_the_document() {
        let (state, _dir) = crate::test_support::app_state("config-export-machine-state");
        populated(&state.store);
        state
            .store
            .add_trusted_directory("/trusted/secret-dir")
            .unwrap();
        let session_1 = || Session {
            id: "session-1".to_string(),
            agent: Agent::Claude,
            agent_session_id: Some("agent-session-xyz".to_string()),
            console_id: "console-1".to_string(),
            project_id: Some("project-1".to_string()),
            host_id: LOCAL_HOST_ID.to_string(),
            role: Role::Project,
            origin: Origin::User,
            title: "Distinctive session title".to_string(),
            status: SessionStatus::Idle,
            terminal_title: None,
            record_titles: Default::default(),
            background_tasks: Vec::new(),
            switching_account: false,
            default_effort: None,
            context: None,
            observed_model: None,
            observed_effort: None,
            model: None,
            effort: None,
            has_conversation: true,
            bound_to: None,
            lead: false,
            colour: None,
            ordinal: None,
            account_id: Some("acct-1".to_string()),
            config_dir: Some("/home/u/.claude-work".to_string()),
            pinned: false,
            locked: false,
            started_at: 0,
            ended_at: None,
        };
        state.store.insert_session(&session_1()).unwrap();
        state
            .store
            .insert_session(&Session {
                id: "session-2".to_string(),
                role: Role::Console,
                project_id: None,
                title: "Hub".to_string(),
                ..session_1()
            })
            .unwrap();
        state
            .store
            .insert_page(&Page {
                id: "page-1".to_string(),
                console_session_id: "session-2".to_string(),
                html: "<p>Distinctive page body</p>".to_string(),
                anchor_message_id: None,
                created_at: 0,
            })
            .unwrap();
        let mut trusted = state.store.get_project("project-1").unwrap().unwrap();
        trusted.trust_consent = true;
        state.store.update_project(&trusted).unwrap();

        let text = serde_json::to_string(&build(&state.store, None, true).unwrap()).unwrap();

        for absent in [
            "secret-dir",
            "agent-session-xyz",
            "Distinctive session title",
            "Distinctive page body",
            "trust",
        ] {
            assert!(!text.contains(absent), "{absent} is in {text}");
        }
        assert!(
            !text.contains("default_clone_dir"),
            "the clone directory is the importing machine's own"
        );
    }

    #[test]
    fn the_document_reads_back_as_the_same_document() {
        let (document, text) = exported(None);
        let read: ConfigurationDocument = serde_json::from_str(&text).unwrap();
        assert_eq!(read, document);
    }

    #[tokio::test]
    async fn an_unknown_console_is_refused_with_the_protocol_error_instead_of_an_empty_document() {
        let (state, _dir) = crate::test_support::app_state("config-export-unknown-console");
        populated(&state.store);

        let refused = crate::coordinator::handle(
            &state,
            Some("req-1".to_string()),
            RequestBody::ExportConfiguration {
                console: Some("no-such-console".to_string()),
                include_paths: true,
            },
        )
        .await
        .expect_err("no such console");
        let coded = refused.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(coded.code, error_code::UNKNOWN_CONSOLE);

        let reply = crate::coordinator::handle(
            &state,
            Some("req-2".to_string()),
            RequestBody::ExportConfiguration {
                console: None,
                include_paths: true,
            },
        )
        .await
        .expect("answered");
        match reply {
            Some(crate::protocol::Event::ConfigurationExported { id, document }) => {
                assert_eq!(id.as_deref(), Some("req-2"));
                assert_eq!(document.consoles.len(), 3);
            }
            other => panic!("expected configuration_exported, got {other:?}"),
        }
    }
}
