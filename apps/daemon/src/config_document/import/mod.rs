//! Reading a configuration document back: what `inspect_configuration` answers without changing
//! anything, and what `import_configuration` then creates. `apps/daemon/PROTOCOL.md`
//! ("Configuration import") is the specification; the document and the findings are `protocol`
//! types, and the document is built by [`crate::config_document::export`].
//!
//! The document is untrusted input. Its paths and URLs are data to check against this machine,
//! never values to act on: a project's directory comes from one of four choices, each offered only
//! when its state allows, and every one of them ends in the ordinary association, which requires
//! an existing directory or clones a remote. An account's directory is proposed in the review and
//! created through the same checks as Add account; whether it shares its agent's default
//! configuration is not read, since turning that on modifies files outside Octoboard's own
//! directory. A document names no database id and no folder-trust state, so a project created here
//! is as untrusted as one added by hand.

mod accounts;
mod progress;
mod projects;

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use anyhow::Result;

use crate::config_document::upgrade;
use crate::coordinator::{self, consoles::NewConsole, settings::SettingsChanges};
use crate::project_git::remote_url;
use crate::protocol::{
    error_code, Account, AccountFallback, AccountName, AccountResolution, AgentId, CodedError,
    ConfigurationDocument, DirectoryChoice, DirectoryOffers, DocumentConsole, DocumentProject,
    DocumentSettings, ImportFailure, ImportItem, ImportItemState, ImportStatus, ImportedAccounts,
    ImportedConsole, ImportedProject, InspectedAccounts, InspectedConsole, InspectedProject,
    InspectedSettings, OffersUnderCloneDir, PerAgent, ProjectAccount, ProjectAccountRef,
    ProjectResolution, CONFIGURATION_FORMAT, CONFIGURATION_VERSION,
};
use crate::providers::SettingValue;
use crate::state::AppState;

use accounts::{
    announce_accounts, apply_accounts, index_account_resolutions, inspect_accounts,
    prepare_accounts,
};
use progress::Progress;
use projects::{
    clone_directory, derived_directory, exported_directory, is_foreign, place_project, probe_disk,
    probe_offers, Probe, PROBE_DEADLINE,
};

#[derive(Debug, Clone)]
pub struct Inspection {
    /// Absent when the document carries no settings.
    pub settings: Option<InspectedSettings>,
    /// Absent when the document carries no accounts.
    pub accounts: Option<InspectedAccounts>,
    pub consoles: Vec<InspectedConsole>,
}

#[derive(Debug, Clone)]
pub struct Imported {
    /// The settings applied to this machine; absent unless the caller asked for them and they
    /// were applied.
    pub settings_applied: Option<DocumentSettings>,
    /// Why the settings asked for were not applied.
    pub settings_error: Option<ImportFailure>,
    /// The default clone directory written, which is written before anything is created and so
    /// stays written when the settings as a whole then could not be applied.
    pub clone_dir_applied: Option<String>,
    /// What came of the accounts; absent unless the caller asked for them.
    pub accounts: Option<ImportedAccounts>,
    pub consoles: Vec<ImportedConsole>,
}

/// What an import asks to apply. Consoles and accounts are created only when asked for, the
/// application settings only when `settings`, and the default clone directory only when
/// `clone_dir` allows it too.
#[derive(Debug)]
pub struct ApplyOptions {
    /// Apply the document's application settings.
    pub settings: bool,
    /// The resolutions for the document's accounts, when they are to be created.
    pub accounts: Option<Vec<AccountResolution>>,
    /// Create the document's consoles and projects.
    pub consoles: bool,
    /// Take the document's default clone directory, rather than keep this machine's own.
    pub clone_dir: bool,
}

/// [`apply_reporting`] for the tests, which have no client to report progress to.
#[cfg(test)]
pub async fn apply(
    state: &Arc<AppState>,
    document: &serde_json::Value,
    resolutions: Vec<ProjectResolution>,
    options: ApplyOptions,
) -> Result<Imported> {
    apply_reporting(state, document, resolutions, options, None).await
}

/// The document `value` holds, upgraded to the current version, or the refusal that applies: it is
/// not one at all, it is of a version this daemon does not read, or it cannot be read. `format` and
/// `version` are looked at before the rest, since a document of another version may well have a
/// shape this one cannot read, and the useful answer to that is its version.
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
    if object.get("format").and_then(|format| format.as_str()) != Some(CONFIGURATION_FORMAT) {
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
    if !(u64::from(upgrade::OLDEST_VERSION)..=u64::from(CONFIGURATION_VERSION)).contains(&version) {
        return Err(CodedError::raised(
            error_code::CONFIGURATION_VERSION_UNSUPPORTED,
            format!(
                "the configuration is of version {version}; this Octoboard reads versions {} to {}",
                upgrade::OLDEST_VERSION,
                CONFIGURATION_VERSION
            ),
            &[
                ("version", &version.to_string()),
                ("oldest", &upgrade::OLDEST_VERSION.to_string()),
                ("supported", &CONFIGURATION_VERSION.to_string()),
            ],
        ));
    }
    let mut upgraded = object.clone();
    upgrade::upgrade(&mut upgraded, version as u32);
    serde_json::from_value(serde_json::Value::Object(upgraded))
        .map_err(|err| unreadable(&err.to_string()))
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
    fn find(&self, agent: AgentId, name: &str) -> Option<String> {
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
        agent: AgentId,
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
        agent: AgentId,
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

/// The values in effect of the settings a document carries, out of every provider's.
fn exported_in_effect(
    in_effect: BTreeMap<String, BTreeMap<String, Option<SettingValue>>>,
) -> BTreeMap<String, BTreeMap<String, Option<SettingValue>>> {
    crate::providers::settings_where(in_effect, |setting, _| setting.exported)
}

/// The entries of an exported `agent_tags` object that name an agent this build knows; a document
/// from a build that knows more agents lists keys this one skips.
fn known_agent_tags(
    tags: &BTreeMap<String, Vec<String>>,
) -> impl Iterator<Item = (String, Vec<String>)> + '_ {
    tags.iter()
        .filter(|(name, _)| AgentId::from_wire_name(name).is_some())
        .map(|(name, tags)| (name.clone(), tags.clone()))
}

/// The default clone directory `text` stands for on this machine (`~/` expanded), or `None` for
/// blank text or text that is neither absolute nor starts with `~/`: a document may carry anything,
/// and an unusable directory is left out rather than refusing the settings.
fn usable_clone_dir(text: Option<&str>) -> Option<String> {
    let text = text?.trim();
    if text.is_empty() {
        return None;
    }
    coordinator::validate::absolute_path(text)
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
}

/// The language tag of a document to apply: one this build offers.
fn offered_language(tag: Option<&str>) -> Option<String> {
    tag.filter(|tag| crate::interface_language::is_offered(tag))
        .map(str::to_string)
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

    // The document's own clone directory, where an import could take it and it is another one: the
    // review shows the targets of whichever the user chooses.
    let document_clone_dir = document
        .settings
        .as_ref()
        .and_then(|settings| usable_clone_dir(settings.default_clone_dir.as_deref()))
        .filter(|dir| *dir != clone_dir);

    // `exported` is the same under either directory, so only the first set checks it.
    let probes_under = |dir: &str, with_exported: bool| {
        let mut probes = Vec::new();
        for (console, document_console) in document.consoles.iter().enumerate() {
            for (project, document_project) in document_console.projects.iter().enumerate() {
                probes.push(Probe {
                    console,
                    project,
                    exported: exported_directory(&document_project.path).filter(|_| with_exported),
                    derived: derived_directory(dir, &document_project.path),
                    clone: document_project
                        .remote_url
                        .as_deref()
                        .and_then(|url| clone_directory(dir, url)),
                });
            }
        }
        probes
    };
    // Both sets run at once, so the one deadline bounds the whole inspection.
    let (mut offers, mut offers_in_document_dir) = tokio::join!(
        probe_offers(probes_under(&clone_dir, true), PROBE_DEADLINE, probe_disk),
        async {
            match &document_clone_dir {
                Some(dir) => {
                    Some(probe_offers(probes_under(dir, false), PROBE_DEADLINE, probe_disk).await)
                }
                None => None,
            }
        }
    );

    let resume_sessions_on_start_now = state.store.resume_sessions_on_start()?;
    let settings = match &document.settings {
        Some(settings) => Some(InspectedSettings {
            auto_close_idle_shell_terminals: settings.auto_close_idle_shell_terminals,
            current_auto_close_idle_shell_terminals: state
                .store
                .auto_close_idle_shell_terminals()?,
            auto_pull_repositories: settings.auto_pull_repositories,
            current_auto_pull_repositories: state.store.auto_pull_repositories()?,
            auto_delete_unresumable_archives: settings.auto_delete_unresumable_archives,
            current_auto_delete_unresumable_archives: state
                .store
                .auto_delete_unresumable_archives()?,
            sidebar_order_by_status: settings.sidebar_order_by_status,
            current_sidebar_order_by_status: state.store.sidebar_order_by_status()?,
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
            resume_sessions_on_start: settings
                .resume_sessions_on_start
                .unwrap_or(resume_sessions_on_start_now),
            current_resume_sessions_on_start: resume_sessions_on_start_now,
            git_workflow: settings.git_workflow,
            current_git_workflow: state.store.git_workflow_settings()?,
            default_clone_dir: usable_clone_dir(settings.default_clone_dir.as_deref()),
            foreign_default_clone_dir: settings
                .default_clone_dir
                .as_deref()
                .is_some_and(|text| is_foreign(text.trim())),
            current_default_clone_dir: clone_dir,
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
                    let exported_path = offers.exported_path.clone();
                    InspectedProject {
                        name: document_project.name.clone(),
                        path: document_project.path.clone(),
                        foreign_path: is_foreign(&document_project.path),
                        remote_url: document_project
                            .remote_url
                            .as_deref()
                            .map(remote_url::without_credentials),
                        default: offers.default_choice(),
                        offers,
                        in_document_clone_dir: offers_in_document_dir.as_mut().map(|all| {
                            let under = all.remove(&(console, project)).unwrap_or_default();
                            let offers = DirectoryOffers {
                                exported_path: exported_path.clone(),
                                ..under
                            };
                            OffersUnderCloneDir {
                                default: offers.default_choice(),
                                offers,
                            }
                        }),
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
/// The document's consoles are created only when `options.consoles`; without it no console or
/// project is created and `resolutions` are not read.
///
/// What can be refused without side effects is refused before the first console is created: the
/// document, the resolutions, whatever `create_console` would refuse of any console, and a request
/// for settings or accounts the document does not carry. After that nothing is undone and nothing
/// is thrown away: a console that cannot be created, a project that cannot be placed, or settings
/// that cannot be applied is reported in the result, and the rest carry on. A console whose
/// projects all failed is still created.
///
/// The document's accounts are created only when `options.accounts` (the resolutions for them) is
/// given, first, so the consoles' and projects' account references bind to them; see
/// [`apply_accounts`].
///
/// The document's application settings are applied only when `options.settings` asks for it, last,
/// through the path a settings update takes, since they change how this machine behaves rather
/// than add something to it. The one exception is the default clone directory, which is written
/// before anything is created when the settings are applied and `options.clone_dir` allows it, so
/// a project cloned by the same import goes where the user's choice puts it; without
/// `options.clone_dir` this machine's own directory stays and is the one used. Where the settings
/// then fail to apply, the directory stays written and is reported as `clone_dir_applied`. A
/// directory the document carries that is not usable here (neither absolute nor starting with
/// `~/`) is left out. The settings of the Agent accounts section are applied with the accounts
/// (see [`apply_accounts`]), a document of an earlier version having been upgraded so that they
/// are there.
///
/// Projects are placed one at a time, so a clone, which may run for minutes, holds one blocking
/// thread for its whole length and no document, however large, holds more.
///
/// With an `import_id`, every item is announced as waiting once nothing can be refused any more,
/// in the order they are handled, and then reported as each is (`configuration_import_progress`;
/// see [`ImportItem`]): the clone directory, the accounts and the Agent accounts settings, the
/// consoles and their projects, the application settings. A project with no resolution is left
/// out and never announced. A console is `done` as soon as it is created, its projects reporting
/// for themselves after it. What can fail of the accounts runs before the announcements; the
/// reads after them, of this machine's clone directory and of the accounts the consoles bind to,
/// can still fail, which ends the import with an error reply.
pub async fn apply_reporting(
    state: &Arc<AppState>,
    document: &serde_json::Value,
    resolutions: Vec<ProjectResolution>,
    options: ApplyOptions,
    import_id: Option<String>,
) -> Result<Imported> {
    let ApplyOptions {
        settings: apply_settings,
        accounts,
        consoles: apply_consoles,
        clone_dir: apply_clone_dir,
    } = options;
    let progress = Progress::new(state, import_id);
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

    // What `create_console` would refuse of any console is refused before the first account is
    // created; the account references are not bound yet, and nothing in the check depends on them.
    let unbound = Rebinder::default();
    for document_console in &document.consoles {
        let new = new_console(document_console.clone(), unbound.console(document_console));
        coordinator::consoles::check_new_console(state, &new)
            .map_err(|err| naming_console(err, &new.name))?;
    }

    let accounts_ready = match account_step {
        Some((document_accounts, account_choices)) => Some((
            document_accounts,
            account_choices,
            prepare_accounts(state, document_accounts)?,
        )),
        None => None,
    };
    // The directory this import writes: the document's, when it is usable here and the user takes
    // it.
    let clone_dir_choice = document
        .settings
        .as_ref()
        .filter(|_| apply_settings)
        .and_then(|settings| usable_clone_dir(settings.default_clone_dir.as_deref()))
        .filter(|_| apply_clone_dir);
    if clone_dir_choice.is_some() {
        progress.waiting(ImportItem::CloneDir);
    }
    if let Some((document_accounts, account_choices, prepared)) = &accounts_ready {
        announce_accounts(document_accounts, prepared, account_choices, &progress);
    }
    for (console_index, document_console) in document.consoles.iter().enumerate() {
        progress.waiting(ImportItem::Console {
            console: console_index,
        });
        for project_index in 0..document_console.projects.len() {
            if chosen.contains_key(&(console_index, project_index)) {
                progress.waiting(ImportItem::Project {
                    console: console_index,
                    project: project_index,
                });
            }
        }
    }
    if apply_settings {
        progress.waiting(ImportItem::Settings);
    }

    // After everything that can be refused is, and before any account or project is created: a
    // clone made by this import goes into the directory in effect once the user's choice is made.
    // A failure is reported with the settings, which are then left unapplied.
    let (mut clone_dir_applied, mut clone_dir_error) = (None, None);
    if let Some(dir) = clone_dir_choice {
        progress.report(ImportItem::CloneDir, ImportItemState::Creating);
        let changes = SettingsChanges {
            default_clone_dir: Some(dir.clone()),
            ..Default::default()
        };
        match coordinator::settings::update_settings(state, changes) {
            Ok(()) => {
                clone_dir_applied = Some(dir);
                progress.report(ImportItem::CloneDir, ImportItemState::Done);
            }
            Err(err) => {
                let failure = ImportFailure::of(&err);
                progress.failed(ImportItem::CloneDir, &failure);
                clone_dir_error = Some(failure);
            }
        }
    }
    let clone_dir = state.store.default_clone_dir()?;

    let imported_accounts = accounts_ready.map(|(document_accounts, account_choices, prepared)| {
        apply_accounts(
            state,
            document_accounts,
            prepared,
            account_choices,
            &progress,
        )
    });
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
        let console_item = ImportItem::Console {
            console: console_index,
        };
        progress.report(console_item.clone(), ImportItemState::Creating);
        let created = match coordinator::consoles::create_console(state, new) {
            Ok(created) => created,
            Err(err) => {
                let failure = ImportFailure::of(&err);
                progress.failed(console_item, &failure);
                for project_index in 0..document_projects.len() {
                    if chosen.contains_key(&(console_index, project_index)) {
                        progress.report(
                            ImportItem::Project {
                                console: console_index,
                                project: project_index,
                            },
                            ImportItemState::NotAttempted,
                        );
                    }
                }
                let projects = document_projects
                    .iter()
                    .map(|project| ImportedProject::not_attempted(&project.name))
                    .collect();
                imported.push(ImportedConsole {
                    console: None,
                    name,
                    status: ImportStatus::Failed,
                    error: Some(failure),
                    account_fallbacks: Vec::new(),
                    projects,
                });
                continue;
            }
        };
        // The console is done once it exists; its projects report for themselves.
        progress.report(console_item, ImportItemState::Done);
        let mut projects = Vec::new();
        for (project_index, project) in document_projects.into_iter().enumerate() {
            let outcome = match chosen.remove(&(console_index, project_index)) {
                None => ImportedProject::left_out(&project.name),
                Some(choice) => {
                    let item = ImportItem::Project {
                        console: console_index,
                        project: project_index,
                    };
                    progress.report(
                        item.clone(),
                        match choice {
                            DirectoryChoice::Clone => ImportItemState::Cloning,
                            _ => ImportItemState::Creating,
                        },
                    );
                    let accounts = rebinder.project(&project);
                    let outcome =
                        place_project(state, &created.id, project, accounts, &choice, &clone_dir)
                            .await;
                    match &outcome.error {
                        Some(failure) => progress.failed(item, failure),
                        None => progress.report(item, ImportItemState::Done),
                    }
                    outcome
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

    let (mut settings_applied, mut settings_error) = (None, clone_dir_error);
    if let Some(failure) = &settings_error {
        // The settings are then left unapplied, which the item says as the result does.
        progress.failed(ImportItem::Settings, failure);
    }
    if let Some(settings) = document
        .settings
        .filter(|_| apply_settings && settings_error.is_none())
    {
        progress.report(ImportItem::Settings, ImportItemState::Creating);
        let interface_language = offered_language(settings.interface_language.as_deref());
        let changes = SettingsChanges {
            auto_close_idle_shell_terminals: Some(settings.auto_close_idle_shell_terminals),
            auto_pull_repositories: Some(settings.auto_pull_repositories),
            auto_delete_unresumable_archives: Some(settings.auto_delete_unresumable_archives),
            auto_approve_session_requests: Some(settings.auto_approve_session_requests),
            auto_approve_resume_requests: Some(settings.auto_approve_resume_requests),
            auto_approve_account_switches: Some(settings.auto_approve_account_switches),
            auto_approve_handoffs: Some(settings.auto_approve_handoffs),
            ask_questions_as_choices: Some(settings.ask_questions_as_choices),
            interface_language: interface_language.clone(),
            sidebar_order_by_status: Some(settings.sidebar_order_by_status),
            resume_sessions_on_start: settings.resume_sessions_on_start,
            // Written above, before the projects.
            default_clone_dir: None,
            git_workflow: settings.git_workflow.into(),
            ..Default::default()
        };
        match coordinator::settings::update_settings(state, changes) {
            // The echo says what was applied, so the directory only where it was written above and
            // the language only where this build offers it.
            Ok(()) => {
                settings_applied = Some(DocumentSettings {
                    default_clone_dir: clone_dir_applied.clone(),
                    interface_language,
                    ..settings
                });
                progress.report(ImportItem::Settings, ImportItemState::Done);
            }
            Err(err) => {
                let failure = ImportFailure::of(&err);
                progress.failed(ImportItem::Settings, &failure);
                settings_error = Some(failure);
            }
        }
    }
    Ok(Imported {
        settings_applied,
        settings_error,
        clone_dir_applied,
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
