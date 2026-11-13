//! The configuration document: what the consoles' and projects' configuration, the application's
//! settings and the account structure look like once they have been lifted out of this machine's
//! database, each region on its own. `apps/daemon/PROTOCOL.md`
//! ("Configuration export") is the specification; the types are in `protocol` and this module
//! builds them.
//!
//! The document is the input of a later import as well as a file the user keeps and shares, so its
//! shape is an interface and not an internal detail: [`CONFIGURATION_FORMAT`] and
//! [`CONFIGURATION_VERSION`] identify it, and nothing in it is a database id.
//!
//! What it leaves out is deliberate. A session, a page, a transcript and a trust record are state
//! of this machine and not configuration. A console's `workdir` is left out because the daemon
//! derives it from the console's id when the console is created, so it would carry that id and
//! Octoboard's own layout, and an import makes its own. An account referenced by a console or a
//! project travels, when the export holds the accounts region and only then, as its name alone
//! ([`AccountName`]) and never as an id or a `config_dir`, since an id means nothing elsewhere and
//! the directory is this machine's layout. The accounts region ([`DocumentAccounts`]) is the one
//! place accounts are described, and it never holds a credential, an id or a quota reading. What
//! stays behind besides is whatever only turns on by modifying files outside Octoboard's own
//! directory: an account's sharing of its agent's default configuration
//! (`docs/product/configuration-export-and-import.md`, "What is not configuration and never
//! travels").

use std::collections::HashMap;
use std::path::{Component, Path};

use anyhow::Result;

use crate::hostfs::dir_key;
use crate::project_git::remote_url;
use crate::protocol::{
    error_code, Account, AccountName, AgentId, CodedError, ConfigurationDocument, Console,
    DocumentAccount, DocumentAccounts, DocumentAgentAccounts, DocumentAgentSettings,
    DocumentConsole, DocumentDefaultAccount, DocumentProject, DocumentSettings, ExportedConsoles,
    PerAgent, Project, ProjectAccount, ProjectAccountRef, SettingsSection, CONFIGURATION_FORMAT,
    CONFIGURATION_VERSION,
};
use crate::store::Store;

/// Which consoles an export holds, as `build` takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsoleScope {
    One(String),
    All,
    None,
}

/// What an export holds: its consoles and projects, the sections of the application settings and
/// the accounts, each chosen on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub consoles: ConsoleScope,
    /// In Settings order, without repeats; empty writes no `settings` region.
    pub settings: Vec<SettingsSection>,
    pub accounts: bool,
    /// Whether a project's absolute path is written in full rather than as its last segment.
    pub paths: bool,
}

impl Selection {
    /// Every console, every section of the application settings and no accounts: what an export
    /// with no choices made holds.
    #[cfg(test)]
    pub fn full(paths: bool) -> Self {
        Self {
            consoles: ConsoleScope::All,
            settings: SettingsSection::ALL.to_vec(),
            accounts: false,
            paths,
        }
    }

    /// The console `id` alone.
    #[cfg(test)]
    pub fn console(id: &str, paths: bool) -> Self {
        Self {
            consoles: ConsoleScope::One(id.to_string()),
            settings: Vec::new(),
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
        settings_sections: Option<Vec<SettingsSection>>,
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
            settings: match settings_sections {
                Some(sections) => SettingsSection::normalized(&sections),
                None if consoles == ConsoleScope::All => SettingsSection::ALL.to_vec(),
                None => Vec::new(),
            },
            accounts: include_accounts.unwrap_or(false),
            consoles,
            paths,
        };
        if selection.consoles == ConsoleScope::None
            && selection.settings.is_empty()
            && !selection.accounts
        {
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
/// is (see `config_document::import`).
pub fn build(store: &Store, selection: &Selection) -> Result<ConfigurationDocument> {
    let mut consoles = match &selection.consoles {
        ConsoleScope::One(id) => vec![store.require_console(id)?],
        ConsoleScope::All => store.list_consoles()?,
        ConsoleScope::None => Vec::new(),
    };
    let include_paths = selection.paths;
    // Ordered here and not left to the queries, which `snapshot` shares and which leave ties
    // undefined: consoles are created in quick succession and a console's projects may share a
    // name. The id only breaks a tie and does not reach the document.
    consoles.sort_by(|a, b| (a.created_at, &a.id).cmp(&(b.created_at, &b.id)));
    let all_accounts = store.list_accounts()?;
    // Without the accounts region nothing names an account: a console or project that referenced
    // one is written as it is once the account is gone, using the agent's default account (or
    // inheriting, for a project), so an import elsewhere finds no account missing.
    let accounts: HashMap<String, Account> = if selection.accounts {
        all_accounts
            .iter()
            .cloned()
            .map(|account| (account.id.clone(), account))
            .collect()
    } else {
        HashMap::new()
    };
    let mut projects = store.list_projects()?;
    projects.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
    let settings = if selection.settings.is_empty() {
        None
    } else {
        // Only the keys the daemon holds; the client adds the ones it holds to the document it
        // saves.
        let mut settings = DocumentSettings::default();
        for section in &selection.settings {
            match section {
                SettingsSection::General => {
                    settings.interface_language = store.interface_language()?;
                    settings.auto_close_idle_shell_terminals =
                        Some(store.auto_close_idle_shell_terminals()?);
                    settings.resume_sessions_on_start = Some(store.resume_sessions_on_start()?);
                    settings.keep_awake = Some(store.keep_awake()?);
                    settings.keep_awake_when =
                        Some(store.keep_awake_when()?.wire_name().to_string());
                }
                // The client holds every key of this section.
                SettingsSection::Appearance => {}
                SettingsSection::Git => {
                    settings.auto_pull_repositories = Some(store.auto_pull_repositories()?);
                    settings.git_workflow = Some(store.git_workflow_settings()?);
                    settings.default_clone_dir = Some(document_directory(
                        &store.default_clone_dir()?,
                        &crate::paths::home_dir(),
                    ));
                }
                SettingsSection::AgentBehavior => {
                    settings.auto_approve_session_requests =
                        Some(store.auto_approve_session_requests()?);
                    settings.auto_approve_resume_requests =
                        Some(store.auto_approve_resume_requests()?);
                    settings.auto_approve_account_switches =
                        Some(store.auto_approve_account_switches()?);
                    settings.auto_approve_handoffs = Some(store.auto_approve_handoffs()?);
                    settings.ask_questions_as_choices = Some(store.ask_questions_as_choices()?);
                    settings.keep_sessions_in_own_project =
                        Some(store.keep_sessions_in_own_project()?);
                }
                SettingsSection::KeyboardShortcuts => {}
                SettingsSection::Cleanup => {
                    settings.auto_delete_unresumable_archives =
                        Some(store.auto_delete_unresumable_archives()?);
                }
            }
        }
        Some(settings)
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
        format: CONFIGURATION_FORMAT.to_string(),
        version: CONFIGURATION_VERSION,
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
    let disabled_defaults = store.disabled_default_accounts()?;
    let added_agents = store.added_agents()?;
    let mut agents = Vec::new();
    for agent in AgentId::ALL {
        let owned: Vec<&Account> = all
            .iter()
            .filter(|account| account.agent == agent)
            .collect();
        let default_name = names.get(agent.wire_name()).cloned();
        let default_disabled = disabled_defaults.contains(agent.wire_name());
        // An added agent is listed even with nothing else to carry, so an import can tell its
        // default account is enabled and turn on one this machine disabled. A list not yet decided
        // counts every agent as added.
        let added = added_agents
            .as_ref()
            .is_none_or(|list| list.contains(&agent));
        if !added && owned.is_empty() && default_name.is_none() && !default_disabled {
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
                disabled: account.disabled,
            })
            .collect();
        agents.push(DocumentAgentAccounts {
            agent,
            default_account: DocumentDefaultAccount {
                name: default_name,
                position,
                disabled: Some(default_disabled),
            },
            accounts,
        });
    }
    Ok(DocumentAccounts {
        added_agents,
        agents,
        agent_settings: Some(DocumentAgentSettings {
            auto_select_account: store.auto_select_account()?,
            auto_switch_on_usage_limit: store.auto_switch_on_usage_limit()?,
            auto_start_quota_windows: store.auto_start_quota_windows()?,
            agent_tags: store.agent_tags()?,
            provider_settings: super::exported_provider_settings(
                &store.provider_setting_choices()?,
            ),
        }),
    })
}

/// An account directory as the document writes it: `~/<relative>` under `home`, otherwise the
/// path itself. The relative part is written with `/` whatever the host's separator is, since
/// the document moves between machines and the importing host translates it (`hostfs::expand`).
/// Home is matched segment by segment through [`dir_key`], so a directory spelled in another
/// letter case than home on a case-insensitive file system still reads as under it, and the
/// absolute path with the user name in it is not exported; the relative part keeps its own
/// spelling.
fn document_directory(dir: &str, home: &Path) -> String {
    if home == Path::new("/") {
        return dir.to_string();
    }
    let key = |segment: Component<'_>| dir_key(&segment.as_os_str().to_string_lossy());
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
        let parts: Vec<_> = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect();
        format!("~/{}", parts.join("/"))
    }
}

fn document_console(
    console: &Console,
    projects: Vec<Project>,
    accounts: &HashMap<String, Account>,
    include_paths: bool,
) -> DocumentConsole {
    DocumentConsole {
        name: console.name.clone(),
        icon: console.icon.clone(),
        default_agent: console.default_agent,
        project_session_agent: console.project_session_agent,
        accounts: PerAgent::from_fn(|agent| {
            account_ref(accounts, agent, console.accounts.get(agent).as_deref())
        }),
        launch: super::exported_launches(console.launch.clone()),
        project_session_launch: super::exported_launches(console.project_session_launch.clone()),
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
    let reference = |agent: AgentId, account: &ProjectAccount| match account {
        ProjectAccount::Inherit => ProjectAccountRef::Inherit,
        ProjectAccount::Default => ProjectAccountRef::Default,
        // An account that is gone is dropped to what deleting it leaves behind: inheriting.
        ProjectAccount::Account { id } => account_ref(accounts, agent, Some(id))
            .map(|AccountName { name }| ProjectAccountRef::Account { name })
            .unwrap_or(ProjectAccountRef::Inherit),
    };
    DocumentProject {
        accounts: project
            .accounts
            .map(|agent, account| reference(agent, &account)),
        name: project.name,
        path: if include_paths {
            project.path
        } else {
            directory_name(&project.path)
        },
        source: project.source,
        // A remote recorded with a credential in it (`https://token@host/...`) must not leave the
        // machine in a file meant to be shared.
        remote_url: project
            .remote_url
            .map(|url| remote_url::without_credentials(&url))
            .filter(|url| include_paths || remote_url::is_clonable_url(url)),
        default_agent: project.default_agent,
        pinned: project.pinned,
        tags: project.tags,
        description: project.description,
        launch: super::exported_launches(project.launch),
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
    agent: AgentId,
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
    use crate::protocol::{
        error_code, GitWorkflowSettings, Page, ProjectSource, RequestBody, Session, SessionKind,
    };
    use crate::store::LOCAL_HOST_ID;
    use crate::test_support::bare_session;

    fn console(id: &str, name: &str) -> Console {
        Console {
            id: id.to_string(),
            name: name.to_string(),
            workdir: format!("/consoles/{id}"),
            default_agent: AgentId::CODEX,
            project_session_agent: None,
            accounts: Default::default(),
            launch: Default::default(),
            project_session_launch: Default::default(),
            icon: None,
            console_session_prompt: None,
            created_at: 0,
            versions: Default::default(),
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
            accounts: Default::default(),
            launch: Default::default(),
            trust_consent: false,
            pinned: false,
            tags: Vec::new(),
            description: None,
            git_workflow: Default::default(),
            versions: Default::default(),
        }
    }

    fn account(id: &str, agent: AgentId, name: &str, config_dir: &str) -> Account {
        Account {
            id: id.to_string(),
            agent,
            name: name.to_string(),
            config_dir: config_dir.to_string(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
            versions: Default::default(),
        }
    }

    /// One console holding every kind of project, and two more beside it. Consoles and projects are
    /// inserted in the opposite order to the one they are exported in, so insertion order (what a
    /// query without an `ORDER BY` returns) cannot pass for the right one.
    fn populated(store: &Store) {
        store
            .insert_account(&account(
                "acct-1",
                AgentId::CLAUDE,
                "Work",
                "/home/u/.claude-work",
            ))
            .unwrap();
        store
            .insert_account(&account(
                "acct-2",
                AgentId::CODEX,
                "Team",
                "/home/u/.codex-team",
            ))
            .unwrap();

        let mut hub = console("console-1", "Hub");
        hub.created_at = 2000;
        hub.icon = Some("data:image/png;base64,AAAA".to_string());
        hub.accounts[AgentId::CLAUDE] = Some("acct-1".to_string());
        // Names an account that no longer exists, and one of another agent.
        hub.accounts[AgentId::CODEX] = Some("acct-gone".to_string());
        hub.accounts[AgentId::GROK] = Some("acct-1".to_string());
        hub.launch[AgentId::CLAUDE].model = Some("opus".to_string());
        hub.launch[AgentId::CLAUDE].effort = Some("some-future-level".to_string());
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
        cloned.default_agent = Some(AgentId::GROK);
        cloned.pinned = true;
        cloned.tags = vec!["backend".to_string(), "Rust".to_string()];
        cloned.description = Some("Mirrors the team's service.".to_string());
        cloned.accounts[AgentId::CLAUDE] = ProjectAccount::Account {
            id: "acct-1".to_string(),
        };
        cloned.accounts[AgentId::CODEX] = ProjectAccount::Default;
        cloned.accounts[AgentId::GROK] = ProjectAccount::Account {
            id: "acct-gone".to_string(),
        };
        cloned.launch[AgentId::CODEX].model = Some("gpt-x".to_string());
        cloned.launch[AgentId::CODEX].effort = Some("high".to_string());
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
        exported_with(match console {
            Some(id) => Selection::console(id, true),
            None => Selection::full(true),
        })
    }

    fn exported_with(selection: Selection) -> (ConfigurationDocument, String) {
        let (document, text, _) = exported_with_store(selection);
        (document, text)
    }

    /// Like [`exported_with`], with the clone directory the store holds, written as the export
    /// writes it.
    fn exported_with_store(selection: Selection) -> (ConfigurationDocument, String, String) {
        let (state, _dir) = crate::test_support::app_state("config-export");
        populated(&state.store);
        let document = build(&state.store, &selection).expect("built");
        let text = serde_json::to_string(&document).unwrap();
        let clone_dir = document_directory(
            &state.store.default_clone_dir().unwrap(),
            &crate::paths::home_dir(),
        );
        (document, text, clone_dir)
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
        let (document, _, clone_dir) = exported_with_store(Selection::full(true));

        assert_eq!(
            (document.format.as_str(), document.version),
            (CONFIGURATION_FORMAT, CONFIGURATION_VERSION)
        );
        assert_eq!(
            document.exported_by.as_deref(),
            Some(env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(
            document.settings,
            Some(DocumentSettings {
                auto_close_idle_shell_terminals: Some(false),
                auto_pull_repositories: Some(true),
                auto_delete_unresumable_archives: Some(false),
                auto_approve_session_requests: Some(false),
                auto_approve_resume_requests: Some(false),
                auto_approve_account_switches: Some(false),
                auto_approve_handoffs: Some(false),
                ask_questions_as_choices: Some(true),
                keep_sessions_in_own_project: Some(false),
                resume_sessions_on_start: Some(false),
                keep_awake: Some(false),
                keep_awake_when: Some("working".to_string()),
                git_workflow: Some(GitWorkflowSettings::default()),
                default_clone_dir: Some(clone_dir),
                ..Default::default()
            })
        );
        // The Agent accounts settings travel with the accounts, not here.
        let written = serde_json::to_value(document.settings.as_ref().unwrap()).unwrap();
        for moved in [
            "auto_select_account",
            "auto_switch_on_usage_limit",
            "auto_start_quota_windows",
            "agent_tags",
            "provider_settings",
        ] {
            assert!(written.get(moved).is_none(), "{moved} is still in settings");
        }
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
        assert_eq!(hub.launch[AgentId::CLAUDE].model.as_deref(), Some("opus"));
        assert_eq!(hub.console_session_prompt.as_deref(), Some("Be brief."));
        assert_eq!(
            hub.launch[AgentId::CLAUDE].effort.as_deref(),
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
        assert_eq!(cloned.default_agent, Some(AgentId::GROK));
        assert!(cloned.pinned);
        assert_eq!(cloned.tags, ["backend", "Rust"]);
        assert_eq!(
            cloned.description.as_deref(),
            Some("Mirrors the team's service.")
        );
        assert_eq!(
            cloned.launch[AgentId::CODEX].model.as_deref(),
            Some("gpt-x")
        );
        assert_eq!(
            cloned.launch[AgentId::CODEX].effort.as_deref(),
            Some("high")
        );
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
    fn a_credential_in_a_recorded_remote_is_left_out_of_the_document() {
        let (state, _dir) = crate::test_support::app_state("config-export-credentials");
        populated(&state.store);
        let mut secret = project("project-secret", "console-1", "secret");
        secret.source = ProjectSource::Git;
        secret.remote_url = Some("https://user:token@example.com/team/secret.git".to_string());
        state.store.insert_project(&secret).unwrap();

        for include_paths in [true, false] {
            let remote = build(&state.store, &Selection::full(include_paths))
                .expect("built")
                .consoles
                .into_iter()
                .flat_map(|console| console.projects)
                .find(|project| project.name == "secret")
                .and_then(|project| project.remote_url);
            assert_eq!(
                remote.as_deref(),
                Some("https://example.com/team/secret.git")
            );
        }
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
        let (document, text) = exported_with(Selection {
            accounts: true,
            ..Selection::console("console-1", true)
        });

        let hub = &document.consoles[0];
        assert_eq!(
            hub.accounts[AgentId::CLAUDE],
            Some(AccountName {
                name: "Work".to_string()
            })
        );
        assert_eq!(hub.accounts[AgentId::CODEX], None, "the account is gone");
        assert_eq!(
            hub.accounts[AgentId::GROK],
            None,
            "the account belongs to another agent"
        );
        let cloned = &hub.projects[0];
        assert_eq!(
            cloned.accounts[AgentId::CLAUDE],
            ProjectAccountRef::Account {
                name: "Work".to_string()
            }
        );
        assert_eq!(cloned.accounts[AgentId::CODEX], ProjectAccountRef::Default);
        assert_eq!(cloned.accounts[AgentId::GROK], ProjectAccountRef::Inherit);
        assert_eq!(
            hub.projects[1].accounts[AgentId::CLAUDE],
            ProjectAccountRef::Inherit
        );

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
        ] {
            assert!(!text.contains(leaked), "{leaked} is in {text}");
        }
        assert!(text.contains(r#""kind":"account""#) && text.contains("Work"));
    }

    #[test]
    fn an_export_without_the_accounts_region_names_no_account_anywhere() {
        let (document, text) = exported_with(Selection::console("console-1", true));

        let hub = &document.consoles[0];
        for agent in AgentId::ALL {
            assert_eq!(hub.accounts[agent], None, "the console uses the default");
            for project in &hub.projects {
                assert!(
                    !matches!(project.accounts[agent], ProjectAccountRef::Account { .. }),
                    "{} names an account",
                    project.name
                );
            }
        }
        let cloned = hub.projects.iter().find(|p| p.name == "cloned").unwrap();
        assert_eq!(
            cloned.accounts[AgentId::CLAUDE],
            ProjectAccountRef::Inherit,
            "a named account is no longer named"
        );
        assert_eq!(
            cloned.accounts[AgentId::CODEX],
            ProjectAccountRef::Default,
            "the default stays the default"
        );
        assert!(!text.contains(r#""kind":"account""#) && !text.contains("Work"));
        assert!(document.accounts.is_none());
        assert!(!text.contains(".claude-work") && !text.contains(".codex-team"));
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
            agent_session_id: Some("agent-session-xyz".to_string()),
            project_id: Some("project-1".to_string()),
            host_id: LOCAL_HOST_ID.to_string(),
            title: "Distinctive session title".to_string(),
            has_conversation: true,
            account_id: Some("acct-1".to_string()),
            config_dir: Some("/home/u/.claude-work".to_string()),
            ..bare_session("session-1", SessionKind::Project)
        };
        state.store.insert_session(&session_1()).unwrap();
        state
            .store
            .insert_session(&Session {
                id: "session-2".to_string(),
                kind: SessionKind::Console,
                project_id: None,
                title: "Hub".to_string(),
                ..session_1()
            })
            .unwrap();
        state
            .store
            .insert_page(&Page {
                id: "page-1".to_string(),
                session_id: "session-2".to_string(),
                content: crate::test_support::test_page_content("Distinctive page body"),
                submission: None,
                submission_revision: 0,
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
        // The clone directory travels with the settings, for the importing user to take or leave.
        assert!(text.contains("default_clone_dir"));
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
        settings_sections: Option<Vec<SettingsSection>>,
        include_accounts: Option<bool>,
    ) -> Result<Selection> {
        Selection::from_request(
            console.map(str::to_string),
            consoles,
            settings_sections,
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
                settings_sections,
                include_accounts,
                include_paths,
            } => assert_eq!(
                (
                    console.as_deref(),
                    consoles,
                    settings_sections,
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
        let all = || SettingsSection::ALL.to_vec();
        let selection = |consoles, settings, accounts| {
            request(Some("console-1"), Some(consoles), settings, accounts).unwrap()
        };
        let one = selection(One, None, None);
        assert_eq!(
            (one.consoles.clone(), one.settings, one.accounts),
            (
                ConsoleScope::One("console-1".to_string()),
                Vec::new(),
                false
            )
        );
        let one_with_settings = selection(One, Some(all()), Some(true));
        assert!(one_with_settings.settings == all() && one_with_settings.accounts);
        let all_without_settings = selection(All, Some(Vec::new()), None);
        assert_eq!(all_without_settings.consoles, ConsoleScope::All);
        assert!(all_without_settings.settings.is_empty());
        // `console` is read for `one` alone.
        let none_with_accounts = selection(NoConsoles, None, Some(true));
        assert_eq!(none_with_accounts.consoles, ConsoleScope::None);
        assert!(none_with_accounts.settings.is_empty() && none_with_accounts.accounts);
        assert_eq!(selection(NoConsoles, Some(all()), None).settings, all());
        assert!(request(None, Some(NoConsoles), None, Some(true)).is_ok());
        // Without a console, accounts alone leave the default meaning of the rest.
        let only_accounts = request(None, None, None, Some(true)).unwrap();
        assert_eq!(only_accounts.consoles, ConsoleScope::All);
        assert!(only_accounts.settings == all() && only_accounts.accounts);
    }

    #[test]
    fn the_chosen_sections_are_kept_once_each_in_settings_order() {
        use SettingsSection::{Cleanup, General, Git};
        let selection =
            request(None, None, Some(vec![Cleanup, General, Cleanup, Git]), None).unwrap();
        assert_eq!(selection.settings, [General, Git, Cleanup]);
    }

    #[test]
    fn an_export_of_some_sections_writes_only_their_keys() {
        use SettingsSection::{
            AgentBehavior, Appearance, Cleanup, General, Git, KeyboardShortcuts,
        };
        let keys = |sections: Vec<SettingsSection>| {
            let (state, _dir) = crate::test_support::app_state("config-export-sections");
            state.store.set_interface_language("de").unwrap();
            let selection = Selection {
                consoles: ConsoleScope::None,
                settings: sections,
                accounts: true,
                paths: true,
            };
            let document = serde_json::to_value(build(&state.store, &selection).unwrap()).unwrap();
            let mut keys: Vec<String> = document
                .get("settings")
                .and_then(|settings| settings.as_object())
                .map(|settings| settings.keys().cloned().collect())
                .unwrap_or_default();
            keys.sort();
            keys
        };
        assert_eq!(
            keys(vec![Git]),
            [
                "auto_pull_repositories",
                "default_clone_dir",
                "git_workflow"
            ]
        );
        assert_eq!(keys(vec![Cleanup]), ["auto_delete_unresumable_archives"]);
        assert_eq!(
            keys(vec![General]),
            [
                "auto_close_idle_shell_terminals",
                "interface_language",
                "keep_awake",
                "keep_awake_when",
                "resume_sessions_on_start"
            ]
        );
        assert_eq!(
            keys(vec![AgentBehavior]),
            [
                "ask_questions_as_choices",
                "auto_approve_account_switches",
                "auto_approve_handoffs",
                "auto_approve_resume_requests",
                "auto_approve_session_requests",
                "keep_sessions_in_own_project",
            ]
        );
        // The client holds every key of these sections, so the daemon's region is an empty object.
        assert!(keys(vec![Appearance]).is_empty());
        assert!(keys(vec![KeyboardShortcuts]).is_empty());
        assert!(keys(Vec::new()).is_empty());
    }

    #[test]
    fn an_export_of_nothing_is_refused_and_so_is_one_console_without_its_name() {
        use ExportedConsoles::{None as NoConsoles, One};
        for refused in [
            request(None, Some(NoConsoles), None, None),
            request(None, Some(NoConsoles), Some(Vec::new()), Some(false)),
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
        let built = |consoles, settings: Vec<SettingsSection>, accounts| {
            let selection = Selection {
                consoles,
                settings,
                accounts,
                paths: true,
            };
            build(&state.store, &selection).unwrap()
        };
        let none = built(ConsoleScope::None, SettingsSection::ALL.to_vec(), false);
        assert!(none.consoles.is_empty() && none.settings.is_some() && none.accounts.is_none());
        let one = built(
            ConsoleScope::One("console-2".to_string()),
            SettingsSection::ALL.to_vec(),
            true,
        );
        assert_eq!(one.consoles.len(), 1);
        assert!(one.settings.is_some() && one.accounts.is_some());
        let accounts_alone = built(ConsoleScope::None, Vec::new(), true);
        assert!(accounts_alone.consoles.is_empty() && accounts_alone.settings.is_none());
        let text = serde_json::to_string(&accounts_alone).unwrap();
        assert!(
            text.contains(r#""accounts":{"#) && text.contains(r#""settings":null"#),
            "{text}"
        );
        let all = built(ConsoleScope::All, Vec::new(), false);
        assert_eq!(all.consoles.len(), 3);
        assert!(all.settings.is_none());
        assert!(serde_json::to_value(&all)
            .unwrap()
            .get("accounts")
            .is_none());
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
        let mut work = account("acct-1", AgentId::CLAUDE, "Work", &under_home);
        // A shared account: sharing is never written to the document.
        work.share_default_config = true;
        let mut team = account("acct-2", AgentId::CLAUDE, "Team", &outside);
        team.disabled = true;
        store.insert_account(&work).unwrap();
        store.insert_account(&team).unwrap();
        store
            .insert_account(&account(
                "acct-3",
                AgentId::CODEX,
                "Solo",
                "/srv/agents/codex",
            ))
            .unwrap();
        // The saved order: Team, the default, then Work.
        store
            .reorder_accounts(
                AgentId::CLAUDE,
                &[Some("acct-2".to_string()), None, Some("acct-1".to_string())],
            )
            .unwrap();
        store
            .set_default_account_name(AgentId::CLAUDE, Some("Personal"))
            .unwrap();
        store
            .set_added_agents_if_undecided(&[AgentId::CODEX, AgentId::CLAUDE])
            .unwrap();
        (under_home, outside)
    }

    fn exported_accounts(store: &Store, paths: bool) -> DocumentAccounts {
        let selection = Selection {
            consoles: ConsoleScope::None,
            settings: Vec::new(),
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
            Some(vec![AgentId::CODEX, AgentId::CLAUDE])
        );
        let agents: Vec<AgentId> = accounts.agents.iter().map(|entry| entry.agent).collect();
        assert_eq!(
            agents,
            [AgentId::CLAUDE, AgentId::CODEX],
            "grok has nothing to carry"
        );
        let claude = &accounts.agents[0];
        let names: Vec<&str> = claude.accounts.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Team", "Work"], "in the saved display order");
        assert_eq!(
            claude.default_account,
            DocumentDefaultAccount {
                name: Some("Personal".to_string()),
                position: 1,
                disabled: Some(false),
            }
        );
        assert!(claude.accounts[0].disabled && !claude.accounts[1].disabled);
        let codex = &accounts.agents[1];
        assert_eq!(
            codex.default_account,
            DocumentDefaultAccount {
                name: None,
                position: 0,
                disabled: Some(false),
            }
        );

        let text = serde_json::to_string(&accounts).unwrap();
        for leaked in [
            "acct-",
            "incomplete",
            "token",
            "credential",
            "used_percent",
            "share_default_config",
        ] {
            assert!(!text.contains(leaked), "{leaked} is in {text}");
        }
    }

    /// An import turns a default account on only for an agent the document lists, so every added
    /// agent is listed, however little else it has to carry.
    #[test]
    fn an_added_agent_is_listed_with_its_default_account_enabled_and_nothing_else() {
        let (state, _dir) = crate::test_support::app_state("config-export-added-agent");
        with_accounts(&state.store);
        state.store.add_added_agent(AgentId::GROK).unwrap();

        let accounts = exported_accounts(&state.store, true);

        let grok = accounts
            .agents
            .iter()
            .find(|entry| entry.agent == AgentId::GROK)
            .expect("grok is added");
        assert!(grok.accounts.is_empty());
        assert_eq!(
            grok.default_account,
            DocumentDefaultAccount {
                name: None,
                position: 0,
                disabled: Some(false),
            }
        );
    }

    #[test]
    fn every_agent_is_listed_while_the_added_agents_are_not_decided() {
        let (state, _dir) = crate::test_support::app_state("config-export-undecided-agents");

        let accounts = exported_accounts(&state.store, true);

        assert_eq!(accounts.added_agents, None);
        assert_eq!(accounts.agents.len(), AgentId::ALL.len());
        assert!(accounts
            .agents
            .iter()
            .all(|entry| entry.default_account.disabled == Some(false)));
    }

    #[test]
    fn an_account_without_a_saved_order_follows_the_default_account_first() {
        let (state, _dir) = crate::test_support::app_state("config-export-account-order");
        state
            .store
            .insert_account(&account("acct-1", AgentId::GROK, "One", "/srv/one"))
            .unwrap();
        state
            .store
            .insert_account(&account("acct-2", AgentId::GROK, "Two", "/srv/two"))
            .unwrap();
        // A saved order that lacks the newer account and names a deleted one.
        state
            .store
            .reorder_accounts(
                AgentId::GROK,
                &[Some("acct-1".to_string()), None, Some("acct-2".to_string())],
            )
            .unwrap();
        state.store.delete_account("acct-2").unwrap();
        state
            .store
            .insert_account(&account("acct-4", AgentId::GROK, "Four", "/srv/four"))
            .unwrap();

        let exported = exported_accounts(&state.store, true);
        let grok = exported
            .agents
            .iter()
            .find(|entry| entry.agent == AgentId::GROK)
            .unwrap();

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
        assert_eq!(
            document_directory(&home().join("a").join("b").to_string_lossy(), &home()),
            "~/a/b"
        );
    }

    #[test]
    fn a_directory_under_home_in_another_letter_case_is_written_from_home_where_case_is_folded() {
        let home = Path::new("/Users/me");

        // The relative part keeps the spelling it was stored with.
        let recased = document_directory("/users/me/.Claude-X", home);
        if crate::platform::names::FOLDS_CASE {
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
                settings_sections: None,
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
                settings_sections: None,
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
