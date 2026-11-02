//! Reading a configuration document back: what `inspect_configuration` answers without changing
//! anything, and what `import_configuration` then creates. `apps/daemon/PROTOCOL.md`
//! ("Configuration import") is the specification; the document itself is
//! [`crate::config_document::export`]'s.
//!
//! The document is untrusted input. Its paths and URLs are data to check against this machine,
//! never values to act on: a project's directory comes from one of four choices, each offered only
//! when its state allows, and every one of them ends in the ordinary association, which requires
//! an existing directory or clones a remote. An account's directory is proposed in the review and
//! created through the same checks as Add account, and sharing its configuration goes through the
//! normal sharing path. A document names no database id and no folder-trust state, so a project
//! created here is as untrusted as one added by hand.

mod accounts;
mod projects;

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config_document::export::{
    self, AccountName, ConfigurationDocument, DocumentConsole, DocumentProject, DocumentSettings,
    ProjectAccountRef,
};
use crate::coordinator::{self, consoles::NewConsole, settings::SettingsChanges};
use crate::git_workflow::GitWorkflowSettings;
use crate::project_git::remote_url;
use crate::protocol::{error_code, Account, Agent, CodedError, Params, PerAgent, ProjectAccount};
use crate::state::AppState;

use accounts::{apply_accounts, index_account_resolutions, inspect_accounts};
pub use accounts::{AccountResolution, ImportedAccounts, InspectedAccounts};
use projects::{
    clone_directory, derived_directory, exported_directory, place_project, probe_disk,
    probe_offers, Probe, PROBE_DEADLINE,
};
pub use projects::{ChoiceKind, DirectoryChoice, Offers};

/// The choice for the project at `project` of the document's console at `console`, both indexes
/// into the document as it was sent.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Deserialize)]
pub struct ProjectResolution {
    pub console: usize,
    pub project: usize,
    pub choice: DirectoryChoice,
}

/// An account reference of the document that names no account of its agent on this machine, and
/// so is bound to that agent's default account.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccountFallback {
    pub agent: Agent,
    pub name: String,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct InspectedProject {
    pub name: String,
    /// The path as the document records it.
    pub path: String,
    pub remote_url: Option<String>,
    pub offers: Offers,
    #[cfg_attr(
        test,
        ts(type = "\"exported_path\" | \"default_clone_dir\" | \"clone\" | null")
    )]
    pub default: Option<ChoiceKind>,
    pub account_fallbacks: Vec<AccountFallback>,
}

/// The application settings a document carries, beside the value this machine has now, so a client
/// can show what applying them would change.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct InspectedSettings {
    pub auto_pull_repositories: bool,
    pub current_auto_pull_repositories: bool,
    pub auto_delete_unresumable_archives: bool,
    pub current_auto_delete_unresumable_archives: bool,
    pub auto_select_account: bool,
    pub current_auto_select_account: bool,
    pub auto_switch_on_usage_limit: bool,
    pub current_auto_switch_on_usage_limit: bool,
    pub sidebar_order_by_status: bool,
    pub current_sidebar_order_by_status: bool,
    pub auto_start_quota_windows: bool,
    pub current_auto_start_quota_windows: bool,
    pub auto_approve_session_requests: bool,
    pub current_auto_approve_session_requests: bool,
    pub auto_approve_resume_requests: bool,
    pub current_auto_approve_resume_requests: bool,
    pub auto_approve_account_switches: bool,
    pub current_auto_approve_account_switches: bool,
    pub auto_approve_handoffs: bool,
    pub current_auto_approve_handoffs: bool,
    pub ask_questions_as_choices: bool,
    pub current_ask_questions_as_choices: bool,
    pub git_workflow: GitWorkflowSettings,
    pub current_git_workflow: GitWorkflowSettings,
    #[cfg_attr(test, ts(type = "Partial<Record<Agent, boolean>>"))]
    pub claude_compatibility: BTreeMap<String, bool>,
    #[cfg_attr(test, ts(type = "Partial<Record<Agent, boolean>>"))]
    pub current_claude_compatibility: BTreeMap<String, bool>,
    #[cfg_attr(test, ts(type = "Partial<Record<Agent, Array<string>>>"))]
    pub agent_tags: BTreeMap<String, Vec<String>>,
    #[cfg_attr(test, ts(type = "Partial<Record<Agent, Array<string>>>"))]
    pub current_agent_tags: BTreeMap<String, Vec<String>>,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct Inspection {
    /// Absent when the document carries no settings.
    pub settings: Option<InspectedSettings>,
    /// Absent when the document carries no accounts.
    pub accounts: Option<InspectedAccounts>,
    pub consoles: Vec<InspectedConsole>,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct Imported {
    /// The settings applied to this machine; absent unless the caller asked for them and they
    /// were applied.
    pub settings_applied: Option<DocumentSettings>,
    /// Why the settings asked for were not applied.
    pub settings_error: Option<ImportFailure>,
    /// What came of the accounts; absent unless the caller asked for them.
    pub accounts: Option<ImportedAccounts>,
    pub consoles: Vec<ImportedConsole>,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct InspectedConsole {
    pub name: String,
    pub account_fallbacks: Vec<AccountFallback>,
    pub projects: Vec<InspectedProject>,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStatus {
    Created,
    /// No resolution was given for it.
    LeftOut,
    /// Its directory could not be obtained, or the association was refused; of a console, that it
    /// could not be created.
    Failed,
    /// Its console could not be created, so it was never tried.
    NotAttempted,
}

/// Why something was not created or applied: an `error` reply's `code`, `params` and `message`.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct ImportFailure {
    pub code: crate::protocol::Code,
    pub params: Params,
    pub message: String,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct ImportedProject {
    pub name: String,
    pub status: ImportStatus,
    /// How its directory was obtained, for a created project.
    pub via: Option<ChoiceKind>,
    /// The id of the project created.
    pub project: Option<String>,
    pub error: Option<ImportFailure>,
    /// Empty unless the project was created.
    pub account_fallbacks: Vec<AccountFallback>,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct ImportedConsole {
    /// The id of the console created; absent when it could not be created.
    pub console: Option<String>,
    pub name: String,
    /// `created` or `failed`.
    #[cfg_attr(test, ts(type = "\"created\" | \"failed\""))]
    pub status: ImportStatus,
    pub error: Option<ImportFailure>,
    /// Empty unless the console was created.
    pub account_fallbacks: Vec<AccountFallback>,
    /// All `not_attempted` when the console could not be created.
    pub projects: Vec<ImportedProject>,
}

/// The document `value` holds, or the refusal that applies: it is not one at all, it is of a
/// version this daemon does not read, or it cannot be read. `format` and `version` are looked at
/// before the rest, since a document of another version may well have a shape this one cannot
/// read, and the useful answer to that is its version.
fn parse(value: &serde_json::Value) -> Result<ConfigurationDocument> {
    let unreadable = |detail: &str| {
        CodedError::raised(
            error_code::CONFIGURATION_UNREADABLE,
            format!("the configuration could not be read: {detail}"),
            &[("detail", detail)],
        )
    };
    let object = value
        .as_object()
        .ok_or_else(|| unreadable("it is not a JSON object"))?;
    if object.get("format").and_then(|format| format.as_str()) != Some(export::FORMAT) {
        return Err(CodedError::raised(
            error_code::CONFIGURATION_NOT_RECOGNISED,
            "this is not an Octoboard configuration document",
            &[],
        ));
    }
    let version = object
        .get("version")
        .and_then(|version| version.as_u64())
        .ok_or_else(|| unreadable("`version` is not a whole number"))?;
    if version != u64::from(export::VERSION) {
        return Err(CodedError::raised(
            error_code::CONFIGURATION_VERSION_UNSUPPORTED,
            format!(
                "the configuration is of version {version}; this Octoboard reads only version {}",
                export::VERSION
            ),
            &[
                ("version", &version.to_string()),
                ("supported", &export::VERSION.to_string()),
            ],
        ));
    }
    serde_json::from_value(value.clone()).map_err(|err| unreadable(&err.to_string()))
}

/// The document's account references bound to this machine's accounts.
#[derive(Default)]
struct Rebinder {
    accounts: Vec<Account>,
}

struct ConsoleAccounts {
    accounts: PerAgent<Option<String>>,
    fallbacks: Vec<AccountFallback>,
}

struct ProjectAccounts {
    accounts: PerAgent<ProjectAccount>,
    fallbacks: Vec<AccountFallback>,
}

impl Rebinder {
    /// The id of the account of `agent` named `name`, compared as account names are unique, without
    /// regard to case.
    fn find(&self, agent: Agent, name: &str) -> Option<String> {
        let wanted = crate::accounts::account_name_key(name);
        self.accounts
            .iter()
            .find(|account| {
                account.agent == agent && crate::accounts::account_name_key(&account.name) == wanted
            })
            .map(|account| account.id.clone())
    }

    /// A console's reference: the matching account, or the default account (`None`) both for no
    /// reference and, listed in `fallbacks`, for a name that matches none.
    fn console_reference(
        &self,
        agent: Agent,
        reference: &Option<AccountName>,
        fallbacks: &mut Vec<AccountFallback>,
    ) -> Option<String> {
        let AccountName { name } = reference.as_ref()?;
        let found = self.find(agent, name);
        if found.is_none() {
            fallbacks.push(AccountFallback {
                agent,
                name: name.to_string(),
            });
        }
        found
    }

    /// A project's reference: the matching account, or the default account pinned (`Default`, not
    /// `Inherit`) for a name that matches none, listed in `fallbacks`.
    fn project_reference(
        &self,
        agent: Agent,
        reference: &ProjectAccountRef,
        fallbacks: &mut Vec<AccountFallback>,
    ) -> ProjectAccount {
        match reference {
            ProjectAccountRef::Inherit => ProjectAccount::Inherit,
            ProjectAccountRef::Default => ProjectAccount::Default,
            ProjectAccountRef::Account { name } => match self.find(agent, name) {
                Some(id) => ProjectAccount::Account { id },
                None => {
                    fallbacks.push(AccountFallback {
                        agent,
                        name: name.to_string(),
                    });
                    ProjectAccount::Default
                }
            },
        }
    }

    fn console(&self, console: &DocumentConsole) -> ConsoleAccounts {
        let mut fallbacks = Vec::new();
        ConsoleAccounts {
            accounts: PerAgent::from_fn(|agent| {
                self.console_reference(agent, console.accounts.get(agent), &mut fallbacks)
            }),
            fallbacks,
        }
    }

    fn project(&self, project: &DocumentProject) -> ProjectAccounts {
        let mut fallbacks = Vec::new();
        ProjectAccounts {
            accounts: PerAgent::from_fn(|agent| {
                self.project_reference(agent, project.accounts.get(agent), &mut fallbacks)
            }),
            fallbacks,
        }
    }
}

/// The entries of an exported `claude_compatibility` object that name an agent with the setting; a
/// document from a build that knows more agents lists keys this one skips.
fn known_claude_compatibility(
    choices: &BTreeMap<String, bool>,
) -> impl Iterator<Item = (String, bool)> + '_ {
    choices
        .iter()
        .filter(|(name, _)| {
            Agent::from_wire_name(name).is_some_and(Agent::has_claude_compatibility)
        })
        .map(|(name, enabled)| (name.clone(), *enabled))
}

/// The entries of an exported `agent_tags` object that name an agent this build knows; a document
/// from a build that knows more agents lists keys this one skips.
fn known_agent_tags(
    tags: &BTreeMap<String, Vec<String>>,
) -> impl Iterator<Item = (String, Vec<String>)> + '_ {
    tags.iter()
        .filter(|(name, _)| Agent::from_wire_name(name).is_some())
        .map(|(name, tags)| (name.clone(), tags.clone()))
}

/// Reads the document and answers, per project, which directory choices its state allows and the
/// account references that fall back. Changes nothing.
///
/// The existence checks run on blocking threads a few at a time (see [`probe_offers`]), so a
/// document of hundreds of projects does not take over the blocking pool, and the whole set has a
/// deadline: a path on a stalled mount holds its thread for as long as the mount stalls, and the
/// answer does not wait for it. A project whose check did not come back is offered nothing.
pub async fn inspect(state: &Arc<AppState>, document: &serde_json::Value) -> Result<Inspection> {
    let document = parse(document)?;
    let host_accounts = state.store.list_accounts()?;
    let rebinder = Rebinder {
        accounts: host_accounts,
    };
    let accounts = match &document.accounts {
        Some(document_accounts) => Some(inspect_accounts(state, document_accounts, &rebinder)?),
        None => None,
    };
    let clone_dir = state.store.default_clone_dir()?;

    let mut probes = Vec::new();
    for (console, document_console) in document.consoles.iter().enumerate() {
        for (project, document_project) in document_console.projects.iter().enumerate() {
            probes.push(Probe {
                console,
                project,
                exported: exported_directory(&document_project.path),
                derived: derived_directory(&clone_dir, &document_project.path),
                clone: document_project
                    .remote_url
                    .as_deref()
                    .and_then(|url| clone_directory(&clone_dir, url)),
            });
        }
    }
    let mut offers = probe_offers(probes, PROBE_DEADLINE, probe_disk).await;

    let settings = match &document.settings {
        Some(settings) => Some(InspectedSettings {
            auto_pull_repositories: settings.auto_pull_repositories,
            current_auto_pull_repositories: state.store.auto_pull_repositories()?,
            auto_delete_unresumable_archives: settings.auto_delete_unresumable_archives,
            current_auto_delete_unresumable_archives: state
                .store
                .auto_delete_unresumable_archives()?,
            auto_select_account: settings.auto_select_account,
            current_auto_select_account: state.store.auto_select_account()?,
            auto_switch_on_usage_limit: settings.auto_switch_on_usage_limit,
            current_auto_switch_on_usage_limit: state.store.auto_switch_on_usage_limit()?,
            sidebar_order_by_status: settings.sidebar_order_by_status,
            current_sidebar_order_by_status: state.store.sidebar_order_by_status()?,
            auto_start_quota_windows: settings.auto_start_quota_windows,
            current_auto_start_quota_windows: state.store.auto_start_quota_windows()?,
            auto_approve_session_requests: settings.auto_approve_session_requests,
            current_auto_approve_session_requests: state.store.auto_approve_session_requests()?,
            auto_approve_resume_requests: settings.auto_approve_resume_requests,
            current_auto_approve_resume_requests: state.store.auto_approve_resume_requests()?,
            auto_approve_account_switches: settings.auto_approve_account_switches,
            current_auto_approve_account_switches: state.store.auto_approve_account_switches()?,
            auto_approve_handoffs: settings.auto_approve_handoffs,
            current_auto_approve_handoffs: state.store.auto_approve_handoffs()?,
            ask_questions_as_choices: settings.ask_questions_as_choices,
            current_ask_questions_as_choices: state.store.ask_questions_as_choices()?,
            git_workflow: settings.git_workflow,
            current_git_workflow: state.store.git_workflow_settings()?,
            claude_compatibility: known_claude_compatibility(&settings.claude_compatibility)
                .collect(),
            current_claude_compatibility: state.store.claude_compatibility()?,
            agent_tags: known_agent_tags(&settings.agent_tags).collect(),
            current_agent_tags: state.store.agent_tags()?,
        }),
        None => None,
    };
    let consoles = document
        .consoles
        .iter()
        .enumerate()
        .map(|(console, document_console)| InspectedConsole {
            name: document_console.name.clone(),
            account_fallbacks: rebinder.console(document_console).fallbacks,
            projects: document_console
                .projects
                .iter()
                .enumerate()
                .map(|(project, document_project)| {
                    // A probe that did not come back is absent: nothing is offered.
                    let offers = offers.remove(&(console, project)).unwrap_or_default();
                    InspectedProject {
                        name: document_project.name.clone(),
                        path: document_project.path.clone(),
                        remote_url: document_project
                            .remote_url
                            .as_deref()
                            .map(remote_url::without_credentials),
                        default: offers.default_choice(),
                        offers,
                        account_fallbacks: rebinder.project(document_project).fallbacks,
                    }
                })
                .collect(),
        })
        .collect();
    Ok(Inspection {
        settings,
        accounts,
        consoles,
    })
}

/// The resolutions keyed by project, refused as a whole when one cannot be right.
fn index_resolutions(
    document: &ConfigurationDocument,
    resolutions: Vec<ProjectResolution>,
) -> Result<HashMap<(usize, usize), DirectoryChoice>> {
    let mut chosen = HashMap::new();
    for resolution in resolutions {
        let ProjectResolution {
            console,
            project,
            choice,
        } = resolution;
        let invalid = |reason: &str| {
            CodedError::raised(
                error_code::IMPORT_RESOLUTION_INVALID,
                format!("resolution for console {console}, project {project}: {reason}"),
                &[
                    ("console", &console.to_string()),
                    ("project", &project.to_string()),
                ],
            )
        };
        let exists = document
            .consoles
            .get(console)
            .is_some_and(|console| project < console.projects.len());
        if !exists {
            return Err(invalid("the document has no such project"));
        }
        if let DirectoryChoice::Directory { path } = &choice {
            coordinator::validate::absolute_path(path)
                .map_err(|_| invalid("the directory is not absolute"))?;
        }
        if chosen.insert((console, project), choice).is_some() {
            return Err(invalid("the project is resolved twice"));
        }
    }
    Ok(chosen)
}

/// The new console a document console describes, its account references bound as `accounts` says.
fn new_console(document_console: DocumentConsole, accounts: ConsoleAccounts) -> NewConsole {
    NewConsole {
        name: document_console.name,
        default_agent: document_console.default_agent,
        project_session_agent: document_console.project_session_agent,
        accounts: accounts.accounts,
        launch: document_console.launch,
        project_session_launch: document_console.project_session_launch,
        icon: document_console.icon,
        console_session_prompt: document_console.console_session_prompt,
    }
}

/// `err` with the console it refuses named (`console_name`) when it is a refused avatar: a document
/// may carry several consoles, and the refusal has to say whose avatar it is.
fn naming_console(mut err: anyhow::Error, name: &str) -> anyhow::Error {
    if let Some(coded) = err.downcast_mut::<CodedError>() {
        if coded.code == error_code::ICON_NOT_AN_IMAGE || coded.code == error_code::ICON_TOO_LARGE {
            coded
                .params
                .insert("console_name".to_string(), name.trim().to_string());
        }
    }
    err
}

/// Creates a console for each console of the document and associates its projects as `resolutions`
/// say, through the same association a hand-added project goes through.
///
/// The document's consoles are created only when `apply_consoles`; without it no console or
/// project is created and `resolutions` are not read.
///
/// What can be refused without side effects is refused before the first console is created: the
/// document, the resolutions, whatever `create_console` would refuse of any console, and a request
/// for settings or accounts the document does not carry. After that nothing is undone and nothing
/// is thrown away: a console that cannot be created, a project that cannot be placed, or settings
/// that cannot be applied is reported in the result, and the rest carry on. A console whose
/// projects all failed is still created.
///
/// The document's accounts are created only when `accounts` (the resolutions for them) is given,
/// first, so the consoles' and projects' account references bind to them; see [`apply_accounts`].
///
/// The document's application settings are applied only when `apply_settings` asks for it, last,
/// through the path a settings update takes, since they change how this machine behaves rather
/// than add something to it.
///
/// Projects are placed one at a time, so a clone, which may run for minutes, holds one blocking
/// thread for its whole length and no document, however large, holds more.
pub async fn apply(
    state: &Arc<AppState>,
    document: &serde_json::Value,
    resolutions: Vec<ProjectResolution>,
    apply_settings: bool,
    accounts: Option<Vec<AccountResolution>>,
    apply_consoles: bool,
) -> Result<Imported> {
    let mut document = parse(document)?;
    let mut chosen = if apply_consoles {
        index_resolutions(&document, resolutions)?
    } else {
        document.consoles.clear();
        HashMap::new()
    };
    if apply_settings && document.settings.is_none() {
        return Err(CodedError::raised(
            error_code::IMPORT_SETTINGS_MISSING,
            "the document carries no application settings to apply",
            &[],
        ));
    }
    let account_step = match accounts {
        None => None,
        Some(resolutions) => {
            let document_accounts = document.accounts.as_ref().ok_or_else(|| {
                CodedError::raised(
                    error_code::IMPORT_ACCOUNTS_MISSING,
                    "the document carries no accounts to create",
                    &[],
                )
            })?;
            Some((
                document_accounts,
                index_account_resolutions(document_accounts, resolutions)?,
            ))
        }
    };
    let clone_dir = state.store.default_clone_dir()?;

    // What `create_console` would refuse of any console is refused before the first account is
    // created; the account references are not bound yet, and nothing in the check depends on them.
    let unbound = Rebinder::default();
    for document_console in &document.consoles {
        let new = new_console(document_console.clone(), unbound.console(document_console));
        coordinator::consoles::check_new_console(state, &new)
            .map_err(|err| naming_console(err, &new.name))?;
    }

    let imported_accounts = match account_step {
        Some((document_accounts, chosen)) => {
            Some(apply_accounts(state, document_accounts, chosen).await?)
        }
        None => None,
    };
    // After the accounts, so a reference binds to one the import created.
    let rebinder = Rebinder {
        accounts: state.store.list_accounts()?,
    };

    let mut prepared = Vec::new();
    for document_console in document.consoles {
        let accounts = rebinder.console(&document_console);
        let fallbacks = accounts.fallbacks.clone();
        let projects = document_console.projects.clone();
        prepared.push((new_console(document_console, accounts), fallbacks, projects));
    }

    let mut imported = Vec::new();
    for (console_index, (new, account_fallbacks, document_projects)) in
        prepared.into_iter().enumerate()
    {
        let name = new.name.clone();
        let created = match coordinator::consoles::create_console(state, new) {
            Ok(created) => created,
            Err(err) => {
                let projects = document_projects
                    .iter()
                    .map(|project| ImportedProject::not_attempted(&project.name))
                    .collect();
                imported.push(ImportedConsole {
                    console: None,
                    name,
                    status: ImportStatus::Failed,
                    error: Some(ImportFailure::of(&err)),
                    account_fallbacks: Vec::new(),
                    projects,
                });
                continue;
            }
        };
        let mut projects = Vec::new();
        for (project_index, project) in document_projects.into_iter().enumerate() {
            let outcome = match chosen.remove(&(console_index, project_index)) {
                None => ImportedProject::left_out(&project.name),
                Some(choice) => {
                    let accounts = rebinder.project(&project);
                    place_project(state, &created.id, project, accounts, &choice, &clone_dir).await
                }
            };
            projects.push(outcome);
        }
        imported.push(ImportedConsole {
            console: Some(created.id),
            name: created.name,
            status: ImportStatus::Created,
            error: None,
            account_fallbacks,
            projects,
        });
    }

    let (mut settings_applied, mut settings_error) = (None, None);
    if let Some(settings) = document.settings.filter(|_| apply_settings) {
        let changes = SettingsChanges {
            auto_pull_repositories: Some(settings.auto_pull_repositories),
            auto_delete_unresumable_archives: Some(settings.auto_delete_unresumable_archives),
            auto_approve_session_requests: Some(settings.auto_approve_session_requests),
            auto_approve_resume_requests: Some(settings.auto_approve_resume_requests),
            auto_approve_account_switches: Some(settings.auto_approve_account_switches),
            auto_approve_handoffs: Some(settings.auto_approve_handoffs),
            ask_questions_as_choices: Some(settings.ask_questions_as_choices),
            // Reported by a client, not carried by a document.
            interface_language: None,
            auto_select_account: Some(settings.auto_select_account),
            auto_switch_on_usage_limit: Some(settings.auto_switch_on_usage_limit),
            sidebar_order_by_status: Some(settings.sidebar_order_by_status),
            auto_start_quota_windows: Some(settings.auto_start_quota_windows),
            default_clone_dir: None,
            git_workflow: settings.git_workflow.into(),
            // An agent this build has no such setting for is skipped, so a document from a build
            // that knows more agents still applies the rest of its settings.
            claude_compatibility: known_claude_compatibility(&settings.claude_compatibility)
                .collect(),
            agent_tags: known_agent_tags(&settings.agent_tags).collect(),
        };
        match coordinator::settings::update_settings(state, changes) {
            Ok(()) => settings_applied = Some(settings),
            Err(err) => settings_error = Some(ImportFailure::of(&err)),
        }
    }
    Ok(Imported {
        settings_applied,
        settings_error,
        accounts: imported_accounts,
        consoles: imported,
    })
}

impl ImportFailure {
    fn of(err: &anyhow::Error) -> Self {
        let (code, params, message) = crate::protocol::error_parts(err);
        Self {
            code,
            params,
            message,
        }
    }
}

impl ImportedProject {
    fn not_attempted(name: &str) -> Self {
        Self {
            status: ImportStatus::NotAttempted,
            ..Self::left_out(name)
        }
    }

    fn left_out(name: &str) -> Self {
        Self {
            name: name.to_string(),
            status: ImportStatus::LeftOut,
            via: None,
            project: None,
            error: None,
            account_fallbacks: Vec::new(),
        }
    }
}

fn failed(name: String, err: anyhow::Error) -> ImportedProject {
    ImportedProject {
        name,
        status: ImportStatus::Failed,
        via: None,
        project: None,
        error: Some(ImportFailure::of(&err)),
        account_fallbacks: Vec::new(),
    }
}

#[cfg(test)]
mod tests;
