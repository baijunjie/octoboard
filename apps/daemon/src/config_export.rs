//! The configuration document: what the consoles' and projects' configuration, the application's
//! settings and the account structure look like once they have been lifted out of this machine's
//! database, each region on its own. `apps/daemon/PROTOCOL.md`
//! ("Configuration export") is the specification; this module is its Rust form and builds it.
//!
//! The document is the input of a later import as well as a file the user keeps and shares, so its
//! shape is an interface and not an internal detail: [`FORMAT`] and [`VERSION`] identify it, and
//! nothing in it is a database id.
//!
//! What it leaves out is deliberate. A session, a page, a transcript and a trust record are state
//! of this machine and not configuration. A console's `workdir` is left out because the daemon
//! derives it from the console's id when the console is created, so it would carry that id and
//! Octoboard's own layout, and an import makes its own. An account referenced by a console or a
//! project travels as its name alone ([`AccountName`]) and never as an id or a `config_dir`, since
//! an id means nothing elsewhere and the directory is this machine's layout. The accounts region
//! ([`DocumentAccounts`]) is the one place accounts are described, and it never holds a
//! credential, an id or a quota reading. `Settings.default_clone_dir` stays behind for the same
//! reason: an import places projects under the *importing* machine's own.

use std::collections::HashMap;
use std::path::{Component, Path};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::account_dirs::account_dir_key;
use crate::git_remote;
use crate::git_workflow::{GitWorkflowOverride, GitWorkflowSettings};
use crate::protocol::{
    error_code, Account, Agent, CodedError, Console, Project, ProjectAccount, ProjectSource,
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
    /// The application settings that are not console-scoped. Present exactly when the export
    /// included them, whichever consoles it holds.
    pub settings: Option<DocumentSettings>,
    /// The account structure. Present exactly when the export included it; a reader that does not
    /// know the field ignores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accounts: Option<DocumentAccounts>,
    /// The consoles exported, which may be none.
    pub consoles: Vec<DocumentConsole>,
}

/// The account structure of this machine, never a credential: which agents the user has added and
/// each agent's accounts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentAccounts {
    /// The agents the user has added, in their order. Absent when the exporting machine had not
    /// yet decided the list (it was still finding out which agents are installed), which on import
    /// adds none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_agents: Option<Vec<Agent>>,
    /// One entry for each agent that has an account of its own or a named default account, in
    /// `Agent::ALL` order.
    pub agents: Vec<DocumentAgentAccounts>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentAgentAccounts {
    pub agent: Agent,
    #[serde(default)]
    pub default_account: DocumentDefaultAccount,
    /// The accounts the user owns, in the agent's display order.
    #[serde(default)]
    pub accounts: Vec<DocumentAccount>,
}

/// What the document says of an agent's default account. Its directory is whatever the agent
/// resolves on the machine, so it is not carried, and neither is its disabled flag.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentDefaultAccount {
    /// The name the user gave it; `null` when it is unnamed.
    #[serde(default)]
    pub name: Option<String>,
    /// Its place in the display order: how many of the agent's accounts come before it.
    #[serde(default)]
    pub position: usize,
}

/// An account the user owns. Its id, its login and everything the agent keeps in its directory
/// stay behind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentAccount {
    pub name: String,
    /// `~/<relative path>` for a directory under the exporting machine's home directory (also `~`
    /// for the home directory itself); otherwise the absolute path. Whatever the export's paths
    /// choice: an account cannot exist without its directory.
    pub directory: String,
    /// Whether the account shares the agent's default configuration.
    #[serde(default)]
    pub share_default_config: bool,
    #[serde(default)]
    pub disabled: bool,
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
    pub auto_switch_on_usage_limit: Option<bool>,
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
    /// The choices the user made on whether an agent other than Claude Code (by wire name) loads
    /// Claude Code's instructions and skills. An agent they never chose for is not listed, so the
    /// importing machine keeps following its own default for it. Absent in a document exported
    /// before the setting was carried: importing it leaves the setting as it is. An agent the
    /// document does not list is left as it is.
    #[serde(default)]
    pub claude_compatibility: Option<std::collections::BTreeMap<String, bool>>,
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

/// Which consoles an export holds. On the wire it is the request's `consoles` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportedConsoles {
    /// The console the request names.
    One,
    All,
    None,
}

/// Which consoles an export holds, as `build` takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsoleScope {
    One(String),
    All,
    None,
}

/// What an export holds: its consoles and projects, the application settings and the accounts,
/// each region chosen on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub consoles: ConsoleScope,
    pub settings: bool,
    pub accounts: bool,
    /// Whether a project's absolute path is written in full rather than as its last segment.
    pub paths: bool,
}

impl Selection {
    /// Every console, the application settings and no accounts: what an export with no choices
    /// made holds.
    #[cfg(test)]
    pub fn full(paths: bool) -> Self {
        Self {
            consoles: ConsoleScope::All,
            settings: true,
            accounts: false,
            paths,
        }
    }

    /// The console `id` alone.
    #[cfg(test)]
    pub fn console(id: &str, paths: bool) -> Self {
        Self {
            consoles: ConsoleScope::One(id.to_string()),
            settings: false,
            accounts: false,
            paths,
        }
    }

    /// The selection a request's fields make, with the absent ones as `export_configuration`
    /// documents them. A selection that holds nothing is refused with `export_empty`, and `one`
    /// without a console with `field_required`.
    pub fn from_request(
        console: Option<String>,
        consoles: Option<ExportedConsoles>,
        include_settings: Option<bool>,
        include_accounts: Option<bool>,
        paths: bool,
    ) -> Result<Self> {
        let consoles = match consoles {
            Some(ExportedConsoles::None) => ConsoleScope::None,
            Some(ExportedConsoles::All) => ConsoleScope::All,
            Some(ExportedConsoles::One) => ConsoleScope::One(console.ok_or_else(|| {
                CodedError::raised(
                    error_code::FIELD_REQUIRED,
                    "`console` is required when `consoles` is `one`",
                    &[("field", "console")],
                )
            })?),
            None => console.map_or(ConsoleScope::All, ConsoleScope::One),
        };
        let selection = Self {
            settings: include_settings.unwrap_or(consoles == ConsoleScope::All),
            accounts: include_accounts.unwrap_or(false),
            consoles,
            paths,
        };
        if selection.consoles == ConsoleScope::None && !selection.settings && !selection.accounts {
            return Err(CodedError::raised(
                error_code::EXPORT_EMPTY,
                "nothing is selected to export",
                &[],
            ));
        }
        Ok(selection)
    }
}

/// The document `selection` asks for. A console that does not exist is refused rather than
/// exported as an empty document. Without `paths` a project's `path` is only its last segment, so
/// the document carries nothing of this machine's layout; an import takes such a name for what it
/// is (see `config_import`).
pub fn build(store: &Store, selection: &Selection) -> Result<ConfigurationDocument> {
    let mut consoles = match &selection.consoles {
        ConsoleScope::One(id) => vec![store
            .get_console(id)?
            .ok_or_else(|| CodedError::unknown_console(id))?],
        ConsoleScope::All => store.list_consoles()?,
        ConsoleScope::None => Vec::new(),
    };
    let include_paths = selection.paths;
    // Ordered here and not left to the queries, which `snapshot` shares and which leave ties
    // undefined: consoles are created in quick succession and a console's projects may share a
    // name. The id only breaks a tie and does not reach the document.
    consoles.sort_by(|a, b| (a.created_at, &a.id).cmp(&(b.created_at, &b.id)));
    let all_accounts = store.list_accounts()?;
    let accounts: HashMap<String, Account> = all_accounts
        .iter()
        .cloned()
        .map(|account| (account.id.clone(), account))
        .collect();
    let mut projects = store.list_projects()?;
    projects.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
    let settings = if selection.settings {
        Some(DocumentSettings {
            auto_pull_repositories: store.auto_pull_repositories()?,
            auto_delete_unresumable_archives: Some(store.auto_delete_unresumable_archives()?),
            auto_select_account: Some(store.auto_select_account()?),
            auto_switch_on_usage_limit: Some(store.auto_switch_on_usage_limit()?),
            sidebar_order_by_status: Some(store.sidebar_order_by_status()?),
            auto_start_quota_windows: Some(store.auto_start_quota_windows()?),
            auto_approve_session_requests: Some(store.auto_approve_session_requests()?),
            auto_approve_resume_requests: Some(store.auto_approve_resume_requests()?),
            auto_approve_account_switches: Some(store.auto_approve_account_switches()?),
            auto_approve_handoffs: Some(store.auto_approve_handoffs()?),
            ask_questions_as_choices: Some(store.ask_questions_as_choices()?),
            git_workflow: Some(store.git_workflow_settings()?),
            claude_compatibility: Some(store.claude_compatibility_choices()?),
            keyboard_shortcuts: None,
        })
    } else {
        None
    };
    let document_accounts = if selection.accounts {
        Some(document_accounts(store, &all_accounts)?)
    } else {
        None
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
        accounts: document_accounts,
        consoles,
    })
}

/// The agents' display order of accounts as a client shows it: the saved order without what no
/// longer exists or repeats, then the default account (`None`) and the accounts added since, in
/// `owned` order, wherever the saved order does not place them. `owned` is the ids of the agent's
/// accounts in their stored order.
pub(crate) fn display_order(saved: &[Option<String>], owned: &[String]) -> Vec<Option<String>> {
    let mut remaining: Vec<Option<String>> = std::iter::once(None)
        .chain(owned.iter().cloned().map(Some))
        .collect();
    let mut order = Vec::new();
    let later = std::iter::once(None).chain(owned.iter().cloned().map(Some));
    for id in saved.iter().cloned().chain(later) {
        if let Some(at) = remaining.iter().position(|candidate| *candidate == id) {
            remaining.remove(at);
            order.push(id);
        }
    }
    order
}

fn document_accounts(store: &Store, all: &[Account]) -> Result<DocumentAccounts> {
    let home = crate::paths::home_dir();
    let names = store.default_account_names()?;
    let mut agents = Vec::new();
    for agent in Agent::ALL {
        let owned: Vec<&Account> = all
            .iter()
            .filter(|account| account.agent == agent)
            .collect();
        let default_name = names.get(agent.wire_name()).cloned();
        if owned.is_empty() && default_name.is_none() {
            continue;
        }
        let ids: Vec<String> = owned.iter().map(|account| account.id.clone()).collect();
        let order = display_order(&store.account_order(agent)?, &ids);
        let position = order.iter().position(Option::is_none).unwrap_or(0);
        let accounts = order
            .into_iter()
            .flatten()
            .filter_map(|id| owned.iter().find(|account| account.id == id))
            .map(|account| DocumentAccount {
                name: account.name.clone(),
                directory: document_directory(&account.config_dir, &home),
                share_default_config: account.share_default_config,
                disabled: account.disabled,
            })
            .collect();
        agents.push(DocumentAgentAccounts {
            agent,
            default_account: DocumentDefaultAccount {
                name: default_name,
                position,
            },
            accounts,
        });
    }
    Ok(DocumentAccounts {
        added_agents: store.added_agents()?,
        agents,
    })
}

/// An account directory as the document writes it: `~/<relative>` under `home`, otherwise the
/// path itself. Home is matched segment by segment through [`account_dir_key`], so a directory
/// spelled in another letter case than home on a case-insensitive file system still reads as under
/// it, and the absolute path with the user name in it is not exported; the relative part keeps its
/// own spelling.
fn document_directory(dir: &str, home: &Path) -> String {
    if home == Path::new("/") {
        return dir.to_string();
    }
    let key = |segment: Component<'_>| account_dir_key(&segment.as_os_str().to_string_lossy());
    let mut rest = Path::new(dir).components();
    for segment in home.components() {
        if !rest.next().is_some_and(|under| key(under) == key(segment)) {
            return dir.to_string();
        }
    }
    let relative = rest.as_path();
    if relative.as_os_str().is_empty() {
        "~".to_string()
    } else {
        format!("~/{}", relative.to_string_lossy())
    }
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
        let document = build(
            &state.store,
            &match console {
                Some(id) => Selection::console(id, true),
                None => Selection::full(true),
            },
        )
        .expect("built");
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
                auto_switch_on_usage_limit: Some(false),
                sidebar_order_by_status: Some(true),
                auto_start_quota_windows: Some(false),
                auto_approve_session_requests: Some(false),
                auto_approve_resume_requests: Some(false),
                auto_approve_account_switches: Some(false),
                auto_approve_handoffs: Some(false),
                ask_questions_as_choices: Some(true),
                git_workflow: Some(GitWorkflowSettings::default()),
                claude_compatibility: Some(std::collections::BTreeMap::new()),
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
        let document = build(&state.store, &Selection::full(false)).expect("built");
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
            build(&state.store, &Selection::full(include_paths))
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
            orchestration_role: None,
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

        let text =
            serde_json::to_string(&build(&state.store, &Selection::full(true)).unwrap()).unwrap();

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

    fn request(
        console: Option<&str>,
        consoles: Option<ExportedConsoles>,
        include_settings: Option<bool>,
        include_accounts: Option<bool>,
    ) -> Result<Selection> {
        Selection::from_request(
            console.map(str::to_string),
            consoles,
            include_settings,
            include_accounts,
            true,
        )
    }

    #[test]
    fn a_request_without_the_new_fields_means_what_it_always_did() {
        let selection = request(None, None, None, None).unwrap();
        assert_eq!(selection, Selection::full(true));
        let selection = request(Some("console-1"), None, None, None).unwrap();
        assert_eq!(selection, Selection::console("console-1", true));
        let parsed: crate::protocol::Request =
            serde_json::from_str(r#"{"type":"export_configuration","console":"c"}"#).unwrap();
        match parsed.body {
            RequestBody::ExportConfiguration {
                console,
                consoles,
                include_settings,
                include_accounts,
                include_paths,
            } => assert_eq!(
                (
                    console.as_deref(),
                    consoles,
                    include_settings,
                    include_accounts,
                    include_paths
                ),
                (Some("c"), None, None, None, true)
            ),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn each_region_is_chosen_on_its_own_and_settings_follow_the_consoles_only_while_unsaid() {
        use ExportedConsoles::{All, None as NoConsoles, One};
        let selection = |consoles, settings, accounts| {
            request(Some("console-1"), Some(consoles), settings, accounts).unwrap()
        };
        let one = selection(One, None, None);
        assert_eq!(
            (one.consoles.clone(), one.settings, one.accounts),
            (ConsoleScope::One("console-1".to_string()), false, false)
        );
        let one_with_settings = selection(One, Some(true), Some(true));
        assert!(one_with_settings.settings && one_with_settings.accounts);
        let all_without_settings = selection(All, Some(false), None);
        assert_eq!(all_without_settings.consoles, ConsoleScope::All);
        assert!(!all_without_settings.settings);
        // `console` is read for `one` alone.
        let none_with_accounts = selection(NoConsoles, None, Some(true));
        assert_eq!(none_with_accounts.consoles, ConsoleScope::None);
        assert!(!none_with_accounts.settings && none_with_accounts.accounts);
        assert!(selection(NoConsoles, Some(true), None).settings);
        assert!(request(None, Some(NoConsoles), None, Some(true)).is_ok());
        // Without a console, accounts alone leave the default meaning of the rest.
        let only_accounts = request(None, None, None, Some(true)).unwrap();
        assert_eq!(only_accounts.consoles, ConsoleScope::All);
        assert!(only_accounts.settings && only_accounts.accounts);
    }

    #[test]
    fn an_export_of_nothing_is_refused_and_so_is_one_console_without_its_name() {
        use ExportedConsoles::{None as NoConsoles, One};
        for refused in [
            request(None, Some(NoConsoles), None, None),
            request(None, Some(NoConsoles), Some(false), Some(false)),
            request(Some("console-1"), Some(NoConsoles), None, Some(false)),
        ] {
            let err = refused.expect_err("nothing to export");
            assert_eq!(
                err.downcast_ref::<CodedError>().unwrap().code,
                error_code::EXPORT_EMPTY
            );
        }
        let err = request(None, Some(One), None, None).expect_err("no console named");
        assert_eq!(
            err.downcast_ref::<CodedError>().unwrap().code,
            error_code::FIELD_REQUIRED
        );
    }

    #[test]
    fn the_regions_of_a_document_follow_the_selection_whatever_the_consoles() {
        let (state, _dir) = crate::test_support::app_state("config-export-regions");
        populated(&state.store);
        let built = |consoles, settings, accounts| {
            let selection = Selection {
                consoles,
                settings,
                accounts,
                paths: true,
            };
            build(&state.store, &selection).unwrap()
        };
        let none = built(ConsoleScope::None, true, false);
        assert!(none.consoles.is_empty() && none.settings.is_some() && none.accounts.is_none());
        let one = built(ConsoleScope::One("console-2".to_string()), true, true);
        assert_eq!(one.consoles.len(), 1);
        assert!(one.settings.is_some() && one.accounts.is_some());
        let accounts_alone = built(ConsoleScope::None, false, true);
        assert!(accounts_alone.consoles.is_empty() && accounts_alone.settings.is_none());
        let text = serde_json::to_string(&accounts_alone).unwrap();
        assert!(
            text.contains(r#""accounts":{"#) && text.contains(r#""settings":null"#),
            "{text}"
        );
        let all = built(ConsoleScope::All, false, false);
        assert_eq!(all.consoles.len(), 3);
        assert!(all.settings.is_none());
        assert!(!serde_json::to_string(&all)
            .unwrap()
            .contains(r#""accounts""#));
        let read: ConfigurationDocument =
            serde_json::from_str(&serde_json::to_string(&accounts_alone).unwrap()).unwrap();
        assert_eq!(read, accounts_alone);
    }

    fn home() -> std::path::PathBuf {
        crate::paths::home_dir()
    }

    /// Accounts of every kind: under the home directory and outside it, in a saved order that
    /// differs from the stored one, one shared and one disabled.
    fn with_accounts(store: &Store) -> (String, String) {
        let under_home = home().join(".claude-work").to_string_lossy().into_owned();
        let outside = "/srv/agents/claude-team".to_string();
        let mut work = account("acct-1", Agent::Claude, "Work", &under_home);
        work.share_default_config = true;
        let mut team = account("acct-2", Agent::Claude, "Team", &outside);
        team.disabled = true;
        store.insert_account(&work).unwrap();
        store.insert_account(&team).unwrap();
        store
            .insert_account(&account(
                "acct-3",
                Agent::Codex,
                "Solo",
                "/srv/agents/codex",
            ))
            .unwrap();
        // The saved order: Team, the default, then Work.
        store
            .reorder_accounts(
                Agent::Claude,
                &[Some("acct-2".to_string()), None, Some("acct-1".to_string())],
            )
            .unwrap();
        store
            .set_default_account_name(Agent::Claude, Some("Personal"))
            .unwrap();
        store
            .set_added_agents_if_undecided(&[Agent::Codex, Agent::Claude])
            .unwrap();
        (under_home, outside)
    }

    fn exported_accounts(store: &Store, paths: bool) -> DocumentAccounts {
        let selection = Selection {
            consoles: ConsoleScope::None,
            settings: false,
            accounts: true,
            paths,
        };
        build(store, &selection).unwrap().accounts.unwrap()
    }

    #[test]
    fn the_accounts_region_carries_the_structure_in_display_order_and_nothing_to_log_in_with() {
        let (state, _dir) = crate::test_support::app_state("config-export-accounts");
        with_accounts(&state.store);

        let accounts = exported_accounts(&state.store, true);

        assert_eq!(
            accounts.added_agents,
            Some(vec![Agent::Codex, Agent::Claude])
        );
        let agents: Vec<Agent> = accounts.agents.iter().map(|entry| entry.agent).collect();
        assert_eq!(
            agents,
            [Agent::Claude, Agent::Codex],
            "grok has nothing to carry"
        );
        let claude = &accounts.agents[0];
        let names: Vec<&str> = claude.accounts.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Team", "Work"], "in the saved display order");
        assert_eq!(
            claude.default_account,
            DocumentDefaultAccount {
                name: Some("Personal".to_string()),
                position: 1
            }
        );
        assert!(claude.accounts[0].disabled && !claude.accounts[0].share_default_config);
        assert!(!claude.accounts[1].disabled && claude.accounts[1].share_default_config);
        let codex = &accounts.agents[1];
        assert_eq!(codex.default_account, DocumentDefaultAccount::default());

        let text = serde_json::to_string(&accounts).unwrap();
        for leaked in ["acct-", "incomplete", "token", "credential", "quota"] {
            assert!(!text.contains(leaked), "{leaked} is in {text}");
        }
    }

    #[test]
    fn an_account_without_a_saved_order_follows_the_default_account_first() {
        let (state, _dir) = crate::test_support::app_state("config-export-account-order");
        state
            .store
            .insert_account(&account("acct-1", Agent::Grok, "One", "/srv/one"))
            .unwrap();
        state
            .store
            .insert_account(&account("acct-2", Agent::Grok, "Two", "/srv/two"))
            .unwrap();
        // A saved order that lacks the newer account and names a deleted one.
        state
            .store
            .reorder_accounts(
                Agent::Grok,
                &[Some("acct-1".to_string()), None, Some("acct-2".to_string())],
            )
            .unwrap();
        state.store.delete_account("acct-2").unwrap();
        state
            .store
            .insert_account(&account("acct-4", Agent::Grok, "Four", "/srv/four"))
            .unwrap();

        let grok = &exported_accounts(&state.store, true).agents[0];

        let names: Vec<&str> = grok.accounts.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["One", "Four"]);
        assert_eq!(grok.default_account.position, 1);
        assert_eq!(
            display_order(&[], &["a".to_string(), "b".to_string()]),
            [None, Some("a".to_string()), Some("b".to_string())]
        );
    }

    #[test]
    fn a_directory_under_home_is_written_from_it_and_one_outside_in_full_whatever_the_paths_choice()
    {
        let (state, _dir) = crate::test_support::app_state("config-export-directories");
        let (_, outside) = with_accounts(&state.store);
        let directories = |paths| {
            let accounts = exported_accounts(&state.store, paths);
            accounts.agents[0]
                .accounts
                .iter()
                .map(|account| (account.name.clone(), account.directory.clone()))
                .collect::<HashMap<_, _>>()
        };

        let full = directories(true);
        assert_eq!(full["Work"], "~/.claude-work");
        assert_eq!(full["Team"], outside);
        assert_eq!(
            directories(false),
            full,
            "an account cannot exist without its directory"
        );

        assert_eq!(document_directory(&home().to_string_lossy(), &home()), "~");
        assert_eq!(document_directory("/srv/a", Path::new("/")), "/srv/a");
    }

    #[test]
    fn a_directory_under_home_in_another_letter_case_is_written_from_home_where_case_is_folded() {
        let home = Path::new("/Users/me");

        // The relative part keeps the spelling it was stored with.
        let recased = document_directory("/users/me/.Claude-X", home);
        if cfg!(target_os = "macos") {
            assert_eq!(recased, "~/.Claude-X");
            assert_eq!(document_directory("/USERS/ME", home), "~");
        } else {
            assert_eq!(recased, "/users/me/.Claude-X");
        }
        assert_eq!(
            document_directory("/Users/me/.Claude-X", home),
            "~/.Claude-X"
        );
        assert_eq!(
            document_directory("/Users/mentor/.x", home),
            "/Users/mentor/.x"
        );
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
                consoles: None,
                include_settings: None,
                include_accounts: None,
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
                consoles: None,
                include_settings: None,
                include_accounts: None,
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
