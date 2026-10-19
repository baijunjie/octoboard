//! Reading a configuration document back: what `inspect_configuration` answers without changing
//! anything, and what `import_configuration` then creates. `apps/daemon/PROTOCOL.md`
//! ("Configuration import") is the specification; the document itself is
//! [`crate::config_export`]'s.
//!
//! The document is untrusted input. Its paths and URLs are data to check against this machine,
//! never values to act on: a project's directory comes from one of four choices, each offered only
//! when its state allows, and every one of them ends in the ordinary association, which requires
//! an existing directory or clones a remote. A document names no database id and no folder-trust
//! state, so a project created here is as untrusted as one added by hand.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config_export::{
    self, AccountName, ConfigurationDocument, DocumentConsole, DocumentProject, DocumentSettings,
    ProjectAccountRef,
};
use crate::coordinator::{
    self, AddProjectRequest, NewConsole, NewProjectSettings, SettingsChanges,
};
use crate::git_remote;
use crate::git_remote::read_bounded;
use crate::hostfs;
use crate::protocol::{
    error_code, Account, Agent, CodedError, Params, ProjectAccount, ProjectSource,
};
use crate::state::AppState;

/// Which directory a project is placed in. [`DirectoryChoice`] is the caller's pick of one, and
/// this is the bare name that results report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChoiceKind {
    /// The path the document records.
    ExportedPath,
    /// The last segment of the recorded path, under this machine's default clone directory.
    DefaultCloneDir,
    /// A fresh clone of the recorded remote, into the default clone directory.
    Clone,
    /// A directory the user pointed at.
    Directory,
}

impl ChoiceKind {
    /// The name it has on the wire, which is what the serialization writes.
    fn wire_name(self) -> String {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::String(name)) => name,
            _ => unreachable!("a unit variant serializes as a string"),
        }
    }
}

/// The choice made for one project when applying.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DirectoryChoice {
    ExportedPath,
    DefaultCloneDir,
    Clone,
    Directory { path: String },
}

impl DirectoryChoice {
    fn kind(&self) -> ChoiceKind {
        match self {
            Self::ExportedPath => ChoiceKind::ExportedPath,
            Self::DefaultCloneDir => ChoiceKind::DefaultCloneDir,
            Self::Clone => ChoiceKind::Clone,
            Self::Directory { .. } => ChoiceKind::Directory,
        }
    }
}

/// The choice for the project at `project` of the document's console at `console`, both indexes
/// into the document as it was sent.
#[derive(Debug, Deserialize)]
pub struct ProjectResolution {
    pub console: usize,
    pub project: usize,
    pub choice: DirectoryChoice,
}

/// An account reference of the document that names no account of its agent on this machine, and
/// so is bound to that agent's default account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccountFallback {
    pub agent: Agent,
    pub name: String,
}

/// The directory each choice would use for a project, present only for the choices its state
/// allows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Offers {
    pub exported_path: Option<String>,
    pub default_clone_dir: Option<String>,
    pub clone: Option<String>,
}

impl Offers {
    /// The recorded path wherever it exists, otherwise the derived one wherever that does,
    /// otherwise a clone; `None` leaves the project for the user to place or leave out.
    fn default_choice(&self) -> Option<ChoiceKind> {
        if self.exported_path.is_some() {
            Some(ChoiceKind::ExportedPath)
        } else if self.default_clone_dir.is_some() {
            Some(ChoiceKind::DefaultCloneDir)
        } else if self.clone.is_some() {
            Some(ChoiceKind::Clone)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InspectedProject {
    pub name: String,
    /// The path as the document records it.
    pub path: String,
    pub remote_url: Option<String>,
    pub offers: Offers,
    pub default: Option<ChoiceKind>,
    pub account_fallbacks: Vec<AccountFallback>,
}

/// The application settings a document carries, beside the value this machine has now, so a client
/// can show what applying them would change.
#[derive(Debug, Clone, Serialize)]
pub struct InspectedSettings {
    pub auto_pull_repositories: bool,
    pub current_auto_pull_repositories: bool,
    /// Absent when the document predates the setting, which applying it then leaves as it is.
    pub auto_delete_unresumable_archives: Option<bool>,
    pub current_auto_delete_unresumable_archives: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Inspection {
    /// Absent when the document holds one console and so carries no settings.
    pub settings: Option<InspectedSettings>,
    pub consoles: Vec<InspectedConsole>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Imported {
    /// The settings applied to this machine; absent unless the caller asked for them and they
    /// were applied.
    pub settings_applied: Option<DocumentSettings>,
    /// Why the settings asked for were not applied.
    pub settings_error: Option<ImportFailure>,
    pub consoles: Vec<ImportedConsole>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InspectedConsole {
    pub name: String,
    pub account_fallbacks: Vec<AccountFallback>,
    pub projects: Vec<InspectedProject>,
}

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
#[derive(Debug, Clone, Serialize)]
pub struct ImportFailure {
    pub code: String,
    pub params: Params,
    pub message: String,
}

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

#[derive(Debug, Clone, Serialize)]
pub struct ImportedConsole {
    /// The id of the console created; absent when it could not be created.
    pub console: Option<String>,
    pub name: String,
    /// `created` or `failed`.
    pub status: ImportStatus,
    pub error: Option<ImportFailure>,
    /// Empty unless the console was created.
    pub account_fallbacks: Vec<AccountFallback>,
    /// All `not_attempted` when the console could not be created.
    pub projects: Vec<ImportedProject>,
}

/// The document `value` holds, or the refusal that applies: it is not one at all, it is of a
/// version this daemon does not read, or it cannot be read. `format` and `version` are looked at
/// before the rest, since a newer version may well have a shape this one cannot read, and the
/// useful answer to that is that it is newer.
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
    if object.get("format").and_then(|format| format.as_str()) != Some(config_export::FORMAT) {
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
    if version == 0 || version > u64::from(config_export::VERSION) {
        return Err(CodedError::raised(
            error_code::CONFIGURATION_VERSION_UNSUPPORTED,
            format!(
                "the configuration is of version {version}; this Octoboard reads up to {}",
                config_export::VERSION
            ),
            &[
                ("version", &version.to_string()),
                ("supported", &config_export::VERSION.to_string()),
            ],
        ));
    }
    serde_json::from_value(value.clone()).map_err(|err| unreadable(&err.to_string()))
}

/// The document's account references bound to this machine's accounts.
struct Rebinder {
    accounts: Vec<Account>,
}

struct ConsoleAccounts {
    claude: Option<String>,
    codex: Option<String>,
    grok: Option<String>,
    fallbacks: Vec<AccountFallback>,
}

struct ProjectAccounts {
    claude: ProjectAccount,
    codex: ProjectAccount,
    grok: ProjectAccount,
    fallbacks: Vec<AccountFallback>,
}

impl Rebinder {
    /// The id of the account of `agent` named `name`, compared as account names are unique, without
    /// regard to case.
    fn find(&self, agent: Agent, name: &str) -> Option<String> {
        let wanted = coordinator::account_name_key(name);
        self.accounts
            .iter()
            .find(|account| {
                account.agent == agent && coordinator::account_name_key(&account.name) == wanted
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
                name: name.clone(),
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
                        name: name.clone(),
                    });
                    ProjectAccount::Default
                }
            },
        }
    }

    fn console(&self, console: &DocumentConsole) -> ConsoleAccounts {
        let mut fallbacks = Vec::new();
        ConsoleAccounts {
            claude: self.console_reference(Agent::Claude, &console.claude_account, &mut fallbacks),
            codex: self.console_reference(Agent::Codex, &console.codex_account, &mut fallbacks),
            grok: self.console_reference(Agent::Grok, &console.grok_account, &mut fallbacks),
            fallbacks,
        }
    }

    fn project(&self, project: &DocumentProject) -> ProjectAccounts {
        let mut fallbacks = Vec::new();
        ProjectAccounts {
            claude: self.project_reference(Agent::Claude, &project.claude_account, &mut fallbacks),
            codex: self.project_reference(Agent::Codex, &project.codex_account, &mut fallbacks),
            grok: self.project_reference(Agent::Grok, &project.grok_account, &mut fallbacks),
            fallbacks,
        }
    }
}

/// The recorded path, when it is an absolute path to take as it is. A relative one, or one starting
/// with `~`, would mean something else on this machine than where it was written.
fn exported_directory(recorded: &str) -> Option<PathBuf> {
    let path = Path::new(recorded);
    path.is_absolute()
        .then(|| hostfs::lexically_normalise(path))
}

/// The recorded path's last segment under `clone_dir`. `None` when the path has no last segment
/// (the filesystem root), and so never a path outside `clone_dir`.
fn derived_directory(clone_dir: &str, recorded: &str) -> Option<PathBuf> {
    let clone_dir = Path::new(clone_dir);
    if !clone_dir.is_absolute() {
        return None;
    }
    Some(clone_dir.join(exported_directory(recorded)?.file_name()?))
}

/// The directory a clone of `remote_url` into `clone_dir` would create, when the URL may be cloned
/// from and names a repository.
fn clone_directory(clone_dir: &str, remote_url: &str) -> Option<PathBuf> {
    let clone_dir = Path::new(clone_dir);
    if !clone_dir.is_absolute() || !git_remote::is_clonable_url(remote_url) {
        return None;
    }
    let name = hostfs::repo_name(remote_url)?;
    let mut components = Path::new(&name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Some(clone_dir.join(name)),
        _ => None,
    }
}

/// What a project's directory choices come to on the disk of this machine.
struct Probe {
    console: usize,
    project: usize,
    exported: Option<PathBuf>,
    derived: Option<PathBuf>,
    clone: Option<PathBuf>,
}

fn text(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

/// How long the existence checks of one inspection may take in all. Generous for a local disk,
/// where they take microseconds; it is there for the path that never answers.
const PROBE_DEADLINE: Duration = Duration::from_secs(15);

/// What a project's three choices come to on this machine's disk. A clone is offered only while
/// its target is free, since the clone would refuse otherwise; the other two only while the
/// directory exists.
fn probe_disk(probe: Probe) -> ((usize, usize), Offers) {
    let offers = Offers {
        exported_path: probe.exported.filter(|path| path.is_dir()).map(text),
        default_clone_dir: probe.derived.filter(|path| path.is_dir()).map(text),
        clone: probe.clone.filter(|path| !path.exists()).map(text),
    };
    ((probe.console, probe.project), offers)
}

/// Runs `work` over `probes` on blocking threads, at most `git_remote::read_bounded`'s width at a
/// time, and returns what came back within `deadline`. After it the probes still running are
/// given up on, along with the ones not yet started: a blocked thread cannot be interrupted, but
/// nothing waits for it and no more are started behind it.
async fn probe_offers(
    probes: Vec<Probe>,
    deadline: Duration,
    work: impl Fn(Probe) -> ((usize, usize), Offers) + Send + Sync + 'static,
) -> HashMap<(usize, usize), Offers> {
    let done = Arc::new(Mutex::new(HashMap::new()));
    let sink = done.clone();
    // Each result is kept as it arrives, since `read_bounded` returns only when all have.
    let all = read_bounded(probes, move |probe| {
        let (key, offers) = work(probe);
        sink.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key, offers);
    });
    let _ = tokio::time::timeout(deadline, all).await;
    let mut done = done.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::mem::take(&mut *done)
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
    let rebinder = Rebinder {
        accounts: state.store.list_accounts()?,
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
                        remote_url: document_project.remote_url.clone(),
                        default: offers.default_choice(),
                        offers,
                        account_fallbacks: rebinder.project(document_project).fallbacks,
                    }
                })
                .collect(),
        })
        .collect();
    Ok(Inspection { settings, consoles })
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
            coordinator::absolute_path(path)
                .map_err(|_| invalid("the directory is not absolute"))?;
        }
        if chosen.insert((console, project), choice).is_some() {
            return Err(invalid("the project is resolved twice"));
        }
    }
    Ok(chosen)
}

/// Creates a console for each console of the document and associates its projects as `resolutions`
/// say, through the same association a hand-added project goes through.
///
/// What can be refused without side effects is refused before the first console is created: the
/// document, the resolutions, whatever `create_console` would refuse of any console, and a request
/// for settings the document does not carry. After that nothing is undone and nothing is thrown
/// away: a console that cannot be created, a project that cannot be placed, or settings that
/// cannot be applied is reported in the result, and the rest carry on. A console whose projects
/// all failed is still created.
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
) -> Result<Imported> {
    let document = parse(document)?;
    let mut chosen = index_resolutions(&document, resolutions)?;
    if apply_settings && document.settings.is_none() {
        return Err(CodedError::raised(
            error_code::IMPORT_SETTINGS_MISSING,
            "the document carries no application settings to apply",
            &[],
        ));
    }
    let rebinder = Rebinder {
        accounts: state.store.list_accounts()?,
    };
    let clone_dir = state.store.default_clone_dir()?;

    let mut prepared = Vec::new();
    for document_console in document.consoles {
        let accounts = rebinder.console(&document_console);
        let new = NewConsole {
            name: document_console.name,
            default_agent: document_console.default_agent,
            project_session_agent: document_console.project_session_agent,
            claude_account_id: accounts.claude,
            codex_account_id: accounts.codex,
            grok_account_id: accounts.grok,
            claude_console_session_model: document_console.claude_console_session_model,
            claude_console_session_effort: document_console.claude_console_session_effort,
            codex_console_session_model: document_console.codex_console_session_model,
            codex_console_session_effort: document_console.codex_console_session_effort,
            grok_console_session_model: document_console.grok_console_session_model,
            grok_console_session_effort: document_console.grok_console_session_effort,
            claude_project_session_model: document_console.claude_project_session_model,
            claude_project_session_effort: document_console.claude_project_session_effort,
            codex_project_session_model: document_console.codex_project_session_model,
            codex_project_session_effort: document_console.codex_project_session_effort,
            grok_project_session_model: document_console.grok_project_session_model,
            grok_project_session_effort: document_console.grok_project_session_effort,
            icon: document_console.icon,
            console_session_prompt: document_console.console_session_prompt,
        };
        coordinator::check_new_console(state, &new)?;
        prepared.push((new, accounts.fallbacks, document_console.projects));
    }

    let mut imported = Vec::new();
    for (console_index, (new, account_fallbacks, document_projects)) in
        prepared.into_iter().enumerate()
    {
        let name = new.name.clone();
        let created = match coordinator::create_console(state, new) {
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
            auto_delete_unresumable_archives: settings.auto_delete_unresumable_archives,
            default_clone_dir: None,
        };
        match coordinator::update_settings(state, changes) {
            Ok(()) => settings_applied = Some(settings),
            Err(err) => settings_error = Some(ImportFailure::of(&err)),
        }
    }
    Ok(Imported {
        settings_applied,
        settings_error,
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

/// Associates `project` with the console `console_id` in the directory `choice` names.
async fn place_project(
    state: &Arc<AppState>,
    console_id: &str,
    project: DocumentProject,
    accounts: ProjectAccounts,
    choice: &DirectoryChoice,
    clone_dir: &str,
) -> ImportedProject {
    let name = project.name.clone();
    let unavailable = || {
        CodedError::raised(
            error_code::IMPORT_CHOICE_UNAVAILABLE,
            format!("`{name}` has nothing to use for that choice"),
            &[("choice", &choice.kind().wire_name())],
        )
    };
    let placement: Result<(ProjectSource, Option<String>, Option<String>)> = match choice {
        DirectoryChoice::ExportedPath => exported_directory(&project.path)
            .map(|path| (ProjectSource::Local, Some(text(path)), None))
            .ok_or_else(unavailable),
        DirectoryChoice::DefaultCloneDir => derived_directory(clone_dir, &project.path)
            .map(|path| (ProjectSource::Local, Some(text(path)), None))
            .ok_or_else(unavailable),
        DirectoryChoice::Clone => project
            .remote_url
            .clone()
            .filter(|url| clone_directory(clone_dir, url).is_some())
            .map(|url| (ProjectSource::Git, None, Some(url)))
            .ok_or_else(unavailable),
        DirectoryChoice::Directory { path } => Ok((ProjectSource::Local, Some(path.clone()), None)),
    };
    let added = match placement {
        Ok((source, path, remote_url)) => {
            coordinator::add_project(
                state,
                AddProjectRequest {
                    console_id: console_id.to_string(),
                    source,
                    path,
                    remote_url,
                    name: Some(project.name),
                    default_agent: project.default_agent,
                    settings: NewProjectSettings {
                        pinned: project.pinned,
                        claude_account_id: accounts.claude,
                        codex_account_id: accounts.codex,
                        grok_account_id: accounts.grok,
                        claude_session_model: project.claude_session_model,
                        claude_session_effort: project.claude_session_effort,
                        codex_session_model: project.codex_session_model,
                        codex_session_effort: project.codex_session_effort,
                        grok_session_model: project.grok_session_model,
                        grok_session_effort: project.grok_session_effort,
                    },
                    // The document says what the project's default agent is, including none.
                    detect_default_agent: false,
                    tags: Some(project.tags),
                    description: project.description,
                    // The document's values were chosen on the machine it came from.
                    check_launch: false,
                    check_text_limits: false,
                },
            )
            .await
        }
        Err(err) => Err(err),
    };
    match added.map(|mut added| added.pop()) {
        Ok(Some(created)) => ImportedProject {
            name,
            status: ImportStatus::Created,
            via: Some(choice.kind()),
            project: Some(created.id),
            error: None,
            account_fallbacks: accounts.fallbacks,
        },
        Ok(None) => failed(name, anyhow::anyhow!("no project was created")),
        Err(err) => failed(name, err),
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
mod tests {
    use super::*;
    use crate::protocol::Project;
    use crate::test_support::{app_state, ScratchDir, PATIENCE};
    use serde_json::json;

    fn code_of(err: &anyhow::Error) -> &'static str {
        err.downcast_ref::<CodedError>().expect("coded").code
    }

    fn project(name: &str, path: &str) -> DocumentProject {
        DocumentProject {
            name: name.to_string(),
            path: path.to_string(),
            source: ProjectSource::Local,
            remote_url: None,
            default_agent: None,
            pinned: false,
            tags: Vec::new(),
            description: None,
            claude_account: ProjectAccountRef::Inherit,
            codex_account: ProjectAccountRef::Inherit,
            grok_account: ProjectAccountRef::Inherit,
            claude_session_model: None,
            claude_session_effort: None,
            codex_session_model: None,
            codex_session_effort: None,
            grok_session_model: None,
            grok_session_effort: None,
        }
    }

    fn console(name: &str, projects: Vec<DocumentProject>) -> DocumentConsole {
        DocumentConsole {
            name: name.to_string(),
            icon: None,
            default_agent: Agent::Codex,
            project_session_agent: None,
            claude_project_session_model: None,
            claude_project_session_effort: None,
            codex_project_session_model: None,
            codex_project_session_effort: None,
            grok_project_session_model: None,
            grok_project_session_effort: None,
            claude_account: None,
            codex_account: None,
            grok_account: None,
            claude_console_session_model: None,
            claude_console_session_effort: None,
            codex_console_session_model: None,
            codex_console_session_effort: None,
            grok_console_session_model: None,
            grok_console_session_effort: None,
            console_session_prompt: None,
            projects,
        }
    }

    fn document(consoles: Vec<DocumentConsole>) -> serde_json::Value {
        serde_json::to_value(ConfigurationDocument {
            format: config_export::FORMAT.to_string(),
            version: config_export::VERSION,
            settings: None,
            consoles,
        })
        .unwrap()
    }

    fn account(id: &str, agent: Agent, name: &str) -> Account {
        Account {
            id: id.to_string(),
            agent,
            name: name.to_string(),
            config_dir: format!("/accounts/{id}"),
            approval_reviewer: None,
            approval_mode: None,
        }
    }

    fn directory(parent: &Path, name: &str) -> PathBuf {
        let path = parent.join(name);
        std::fs::create_dir_all(&path).expect("directory");
        path
    }

    fn at(console: usize, project: usize, choice: DirectoryChoice) -> ProjectResolution {
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
        Ok(apply(state, document, resolutions, false).await?.consoles)
    }

    fn with_settings(
        mut document: serde_json::Value,
        auto_pull: bool,
        auto_delete: bool,
    ) -> serde_json::Value {
        document["settings"] = json!({
            "auto_pull_repositories": auto_pull,
            "auto_delete_unresumable_archives": auto_delete,
        });
        document
    }

    fn stored_projects(state: &Arc<AppState>) -> Vec<Project> {
        state.store.list_projects().unwrap()
    }

    #[test]
    fn a_document_is_refused_by_what_it_is_before_it_is_read_any_further() {
        let cases = [
            (json!([]), error_code::CONFIGURATION_UNREADABLE),
            (json!({}), error_code::CONFIGURATION_NOT_RECOGNISED),
            (
                json!({ "format": "something-else", "version": 1 }),
                error_code::CONFIGURATION_NOT_RECOGNISED,
            ),
            (
                json!({ "format": config_export::FORMAT }),
                error_code::CONFIGURATION_UNREADABLE,
            ),
            // A newer version may be shaped in ways this one cannot read; it is told apart from
            // an unreadable body by being newer.
            (
                json!({ "format": config_export::FORMAT, "version": config_export::VERSION + 1 }),
                error_code::CONFIGURATION_VERSION_UNSUPPORTED,
            ),
            (
                json!({ "format": config_export::FORMAT, "version": 0 }),
                error_code::CONFIGURATION_VERSION_UNSUPPORTED,
            ),
            (
                json!({ "format": config_export::FORMAT, "version": config_export::VERSION }),
                error_code::CONFIGURATION_UNREADABLE,
            ),
        ];
        for (value, code) in cases {
            let err = parse(&value).expect_err(&value.to_string());
            assert_eq!(code_of(&err), code, "{value}");
        }
        parse(&document(vec![console("Hub", Vec::new())])).expect("a valid document is read");
    }

    #[test]
    fn a_reference_is_rebound_by_agent_and_name_ignoring_case_and_falls_back_to_the_default() {
        let rebinder = Rebinder {
            accounts: vec![
                account("claude-work", Agent::Claude, "Work"),
                account("codex-work", Agent::Codex, "work"),
            ],
        };
        let named = |name: &str| {
            Some(AccountName {
                name: name.to_string(),
            })
        };
        let mut hub = console("Hub", Vec::new());
        // Matches by case-insensitive name in its own agent; the Grok one has no account at all;
        // the Codex one names an account that exists for Claude only.
        hub.claude_account = named("WORK");
        hub.codex_account = named("Work");
        hub.grok_account = named("Work");
        let bound = rebinder.console(&hub);
        assert_eq!(bound.claude.as_deref(), Some("claude-work"));
        assert_eq!(bound.codex.as_deref(), Some("codex-work"));
        assert_eq!(bound.grok, None);
        assert_eq!(
            bound.fallbacks,
            vec![AccountFallback {
                agent: Agent::Grok,
                name: "Work".to_string()
            }]
        );

        let mut unmatched = project("p", "/p");
        unmatched.claude_account = ProjectAccountRef::Account {
            name: "missing".to_string(),
        };
        unmatched.codex_account = ProjectAccountRef::Default;
        unmatched.grok_account = ProjectAccountRef::Account {
            name: "work".to_string(),
        };
        let bound = rebinder.project(&unmatched);
        // The default account is pinned, not left to the console's reference.
        assert_eq!(bound.claude, ProjectAccount::Default);
        assert_eq!(bound.codex, ProjectAccount::Default);
        assert_eq!(bound.grok, ProjectAccount::Default);
        assert_eq!(
            bound.fallbacks.iter().map(|f| f.agent).collect::<Vec<_>>(),
            [Agent::Claude, Agent::Grok]
        );
        assert_eq!(
            rebinder.project(&project("q", "/q")).claude,
            ProjectAccount::Inherit
        );
    }

    #[test]
    fn a_documents_paths_are_taken_only_as_an_absolute_path_and_its_last_segment() {
        assert_eq!(
            exported_directory("/work/./a/../b/").as_deref(),
            Some(Path::new("/work/b"))
        );
        assert_eq!(exported_directory("work/b"), None);
        assert_eq!(exported_directory("~/work/b"), None);
        assert_eq!(exported_directory(""), None);

        assert_eq!(
            derived_directory("/home/u/Projects", "/Users/x/code/app").as_deref(),
            Some(Path::new("/home/u/Projects/app"))
        );
        // Resolved before the last segment is taken, so a `..` cannot lead out of the directory.
        assert_eq!(
            derived_directory("/home/u/Projects", "/Users/x/code/app/../tool").as_deref(),
            Some(Path::new("/home/u/Projects/tool"))
        );
        assert_eq!(derived_directory("/home/u/Projects", "/"), None);
        assert_eq!(derived_directory("/home/u/Projects", "app"), None);
        assert_eq!(derived_directory("Projects", "/Users/x/app"), None);

        assert_eq!(
            clone_directory("/home/u/Projects", "git@github.com:o/r.git").as_deref(),
            Some(Path::new("/home/u/Projects/r"))
        );
        assert_eq!(clone_directory("/home/u/Projects", "https://h/.."), None);
        assert_eq!(clone_directory("/home/u/Projects", "https://h/."), None);
        assert_eq!(clone_directory("/home/u/Projects", "/srv/git/r"), None);
        assert_eq!(clone_directory("Projects", "https://h/o/r"), None);
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

    #[tokio::test]
    async fn each_project_is_offered_the_choices_its_state_allows_and_the_first_is_the_default() {
        let area = ScratchDir::new("import-inspect-area");
        let clone_dir = directory(&area, "projects");
        let (state, _dir) = state_cloning_into("import-inspect", &clone_dir);
        let here = directory(&area, "elsewhere/present");
        directory(&clone_dir, "moved");
        directory(&clone_dir, "taken");
        directory(&clone_dir, "present");

        let mut present = project("present", &here.to_string_lossy());
        present.remote_url = Some("https://example.com/o/present.git".to_string());
        let mut moved = project("moved", "/gone/layout/moved");
        moved.remote_url = Some("https://example.com/o/moved-repo.git".to_string());
        let mut cloneable = project("cloneable", "/gone/layout/cloneable");
        cloneable.remote_url = Some("https://example.com/o/cloneable.git".to_string());
        // Its clone would land on the directory that exists, so there is nothing to clone into.
        let mut taken = project("taken", "/gone/layout/elsewhere");
        taken.remote_url = Some("https://example.com/o/taken.git".to_string());
        let mut unclonable = project("unclonable", "/gone/layout/unclonable-x/..");
        unclonable.remote_url = Some("ext::sh -c true".to_string());
        let hopeless = project("hopeless", "relative/hopeless");

        let consoles = inspect(
            &state,
            &document(vec![console(
                "Hub",
                vec![present, moved, cloneable, taken, unclonable, hopeless],
            )]),
        )
        .await
        .expect("inspected")
        .consoles;

        let projects = &consoles[0].projects;
        let summary: Vec<_> = projects
            .iter()
            .map(|p| {
                (
                    p.offers.exported_path.is_some(),
                    p.offers.default_clone_dir.is_some(),
                    p.offers.clone.is_some(),
                    p.default,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                // The recorded path and the derived directory both exist; the recorded wins. A
                // clone would land on the derived directory, so it is not offered.
                (true, true, false, Some(ChoiceKind::ExportedPath)),
                (false, true, true, Some(ChoiceKind::DefaultCloneDir)),
                (false, false, true, Some(ChoiceKind::Clone)),
                (false, false, false, None),
                // The derived directory ("unclonable-x/.." resolves to "gone") does not exist.
                (false, false, false, None),
                (false, false, false, None),
            ]
        );
        assert_eq!(
            projects[1].offers.default_clone_dir.as_deref(),
            Some(&*clone_dir.join("moved").to_string_lossy())
        );
        assert_eq!(
            projects[2].offers.clone.as_deref(),
            Some(&*clone_dir.join("cloneable").to_string_lossy())
        );
        assert_eq!(
            projects[0].path,
            here.to_string_lossy(),
            "the recorded path is echoed as recorded"
        );
    }

    #[tokio::test]
    async fn inspecting_reports_the_fallbacks_and_creates_nothing() {
        let (state, _dir) = app_state("import-inspect-readonly");
        let mut hub = console("Hub", vec![project("p", "/nowhere/p")]);
        hub.claude_account = Some(AccountName {
            name: "Missing".to_string(),
        });
        hub.projects[0].codex_account = ProjectAccountRef::Account {
            name: "Gone".to_string(),
        };

        let consoles = inspect(&state, &document(vec![hub]))
            .await
            .unwrap()
            .consoles;

        assert_eq!(consoles[0].account_fallbacks.len(), 1);
        assert_eq!(consoles[0].projects[0].account_fallbacks.len(), 1);
        assert!(state.store.list_consoles().unwrap().is_empty());
        assert!(stored_projects(&state).is_empty());
    }

    #[tokio::test]
    async fn an_import_creates_a_console_and_its_projects_with_their_configuration() {
        let area = ScratchDir::new("import-apply-area");
        let (state, _dir) = app_state("import-apply");
        state
            .store
            .insert_account(&account("acct-1", Agent::Claude, "work"))
            .unwrap();
        // The document's Codex models below are not ones Codex offers here; they are stored as they
        // are all the same.
        crate::test_support::offer_models(&state, Agent::Codex, &[("offered", &[])]);
        let repo = directory(&area, "repo");
        let mut hub = console("Hub", vec![project("renamed", &repo.to_string_lossy())]);
        hub.icon = Some("data:image/png;base64,AAAA".to_string());
        hub.codex_console_session_model = Some("gpt-x".to_string());
        hub.console_session_prompt = Some("\n Be brief.\n".to_string());
        hub.claude_account = Some(AccountName {
            name: "WORK".to_string(),
        });
        let p = &mut hub.projects[0];
        p.pinned = true;
        p.tags = vec!["backend".to_string(), "Backend".to_string()];
        p.description = Some("\n A billing service.\n".to_string());
        p.default_agent = Some(Agent::Grok);
        p.codex_session_model = Some("gpt-y".to_string());
        p.grok_session_effort = Some("high".to_string());
        p.claude_account = ProjectAccountRef::Account {
            name: "Work".to_string(),
        };
        p.codex_account = ProjectAccountRef::Default;

        let imported = import(
            &state,
            &document(vec![hub]),
            vec![at(0, 0, DirectoryChoice::ExportedPath)],
        )
        .await
        .expect("imported");

        let stored_console = state.store.list_consoles().unwrap().remove(0);
        assert_eq!(imported[0].console.as_deref(), Some(&*stored_console.id));
        assert_eq!(imported[0].status, ImportStatus::Created);
        assert_eq!(stored_console.name, "Hub");
        assert_eq!(
            stored_console.icon.as_deref(),
            Some("data:image/png;base64,AAAA")
        );
        assert_eq!(
            stored_console.codex_console_session_model.as_deref(),
            Some("gpt-x")
        );
        assert_eq!(
            stored_console.console_session_prompt.as_deref(),
            Some("Be brief.")
        );
        assert_eq!(stored_console.claude_account_id.as_deref(), Some("acct-1"));
        assert!(Path::new(&stored_console.workdir).is_dir());

        let created = stored_projects(&state).remove(0);
        assert_eq!(
            imported[0].projects[0].project.as_deref(),
            Some(&*created.id)
        );
        assert_eq!(imported[0].projects[0].status, ImportStatus::Created);
        assert_eq!(imported[0].projects[0].via, Some(ChoiceKind::ExportedPath));
        assert_eq!(created.console_id, stored_console.id);
        assert_eq!(created.name, "renamed");
        assert_eq!(created.path, repo.to_string_lossy());
        assert!(created.pinned);
        assert_eq!(created.tags, ["backend"]);
        assert_eq!(created.description.as_deref(), Some("A billing service."));
        assert_eq!(created.default_agent, Some(Agent::Grok));
        assert_eq!(created.codex_session_model.as_deref(), Some("gpt-y"));
        assert_eq!(created.grok_session_effort.as_deref(), Some("high"));
        assert_eq!(
            created.claude_account_id,
            ProjectAccount::Account {
                id: "acct-1".to_string()
            }
        );
        assert_eq!(created.codex_account_id, ProjectAccount::Default);
        assert_eq!(created.grok_account_id, ProjectAccount::Inherit);
        // Trust does not travel: an imported project asks like a hand-added one.
        assert!(!created.trust_consent);
    }

    #[tokio::test]
    async fn an_unmatched_account_is_bound_to_the_default_and_listed_in_the_result() {
        let area = ScratchDir::new("import-fallback-area");
        let (state, _dir) = app_state("import-fallback");
        let repo = directory(&area, "repo");
        let mut hub = console("Hub", vec![project("p", &repo.to_string_lossy())]);
        hub.grok_account = Some(AccountName {
            name: "Team".to_string(),
        });
        hub.projects[0].codex_account = ProjectAccountRef::Account {
            name: "Solo".to_string(),
        };

        let imported = import(
            &state,
            &document(vec![hub]),
            vec![at(0, 0, DirectoryChoice::ExportedPath)],
        )
        .await
        .unwrap();

        assert_eq!(
            state.store.list_consoles().unwrap()[0].grok_account_id,
            None
        );
        assert_eq!(
            stored_projects(&state)[0].codex_account_id,
            ProjectAccount::Default
        );
        let fallback = |agent, name: &str| AccountFallback {
            agent,
            name: name.to_string(),
        };
        assert_eq!(
            imported[0].account_fallbacks,
            [fallback(Agent::Grok, "Team")]
        );
        assert_eq!(
            imported[0].projects[0].account_fallbacks,
            [fallback(Agent::Codex, "Solo")]
        );
    }

    #[tokio::test]
    async fn each_choice_places_the_project_in_its_own_directory() {
        let area = ScratchDir::new("import-choices-area");
        let clone_dir = directory(&area, "projects");
        let (state, _dir) = state_cloning_into("import-choices", &clone_dir);
        let recorded = directory(&area, "layout/recorded");
        // A marker the association would detect an agent from if it were asked to; the document
        // says the project has none.
        std::fs::write(recorded.join("CLAUDE.md"), "").unwrap();
        let derived = directory(&clone_dir, "derived");
        let chosen = directory(&area, "chosen");
        let hub = console(
            "Hub",
            vec![
                project("recorded", &recorded.to_string_lossy()),
                project("derived", "/gone/layout/derived"),
                project("pointed", "/gone/layout/pointed"),
            ],
        );

        let imported = import(
            &state,
            &document(vec![hub]),
            vec![
                at(0, 0, DirectoryChoice::ExportedPath),
                at(0, 1, DirectoryChoice::DefaultCloneDir),
                at(
                    0,
                    2,
                    DirectoryChoice::Directory {
                        path: chosen.to_string_lossy().into_owned(),
                    },
                ),
            ],
        )
        .await
        .unwrap();

        let paths: Vec<String> = stored_projects(&state)
            .into_iter()
            .map(|p| format!("{}={}", p.name, p.path))
            .collect();
        for expected in [
            format!("recorded={}", recorded.display()),
            format!("derived={}", derived.display()),
            format!("pointed={}", chosen.display()),
        ] {
            assert!(paths.contains(&expected), "{expected} in {paths:?}");
        }
        let recorded_project = stored_projects(&state)
            .into_iter()
            .find(|p| p.name == "recorded")
            .unwrap();
        assert_eq!(recorded_project.default_agent, None);
        let via: Vec<_> = imported[0].projects.iter().map(|p| p.via).collect();
        assert_eq!(
            via,
            [
                Some(ChoiceKind::ExportedPath),
                Some(ChoiceKind::DefaultCloneDir),
                Some(ChoiceKind::Directory)
            ]
        );
    }

    #[tokio::test]
    async fn a_project_that_cannot_be_placed_is_reported_and_the_rest_are_imported() {
        let area = ScratchDir::new("import-partial-area");
        let (state, _dir) = app_state("import-partial");
        let good = directory(&area, "good");
        let mut no_remote = project("no-remote", "/gone/no-remote");
        no_remote.remote_url = None;
        let mut bad_remote = project("bad-remote", "/gone/bad-remote");
        bad_remote.remote_url = Some("ext::sh -c true".to_string());
        let hub = console(
            "Hub",
            vec![
                project("missing", "/gone/missing"),
                no_remote,
                bad_remote,
                project("good", &good.to_string_lossy()),
                project("left-out", &good.to_string_lossy()),
            ],
        );

        let imported = import(
            &state,
            &document(vec![hub]),
            vec![
                at(0, 0, DirectoryChoice::ExportedPath),
                at(0, 1, DirectoryChoice::Clone),
                at(0, 2, DirectoryChoice::Clone),
                at(0, 3, DirectoryChoice::ExportedPath),
            ],
        )
        .await
        .expect("a failing project does not fail the import");

        let outcomes: Vec<(ImportStatus, Option<String>)> = imported[0]
            .projects
            .iter()
            .map(|p| (p.status, p.error.as_ref().map(|e| e.code.clone())))
            .collect();
        assert_eq!(
            outcomes,
            [
                (
                    ImportStatus::Failed,
                    Some(error_code::PATH_NOT_FOUND.to_string())
                ),
                (
                    ImportStatus::Failed,
                    Some(error_code::IMPORT_CHOICE_UNAVAILABLE.to_string())
                ),
                (
                    ImportStatus::Failed,
                    Some(error_code::IMPORT_CHOICE_UNAVAILABLE.to_string())
                ),
                (ImportStatus::Created, None),
                (ImportStatus::LeftOut, None),
            ]
        );
        let names: Vec<String> = stored_projects(&state)
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["good"]);
    }

    #[tokio::test]
    async fn a_directory_already_associated_with_the_console_is_not_associated_twice() {
        let area = ScratchDir::new("import-duplicate-area");
        let (state, _dir) = app_state("import-duplicate");
        let repo = directory(&area, "repo");
        let path = repo.to_string_lossy().into_owned();
        let hub = console(
            "Hub",
            vec![project("first", &path), project("second", &path)],
        );

        let imported = import(
            &state,
            &document(vec![hub]),
            vec![
                at(0, 0, DirectoryChoice::ExportedPath),
                at(0, 1, DirectoryChoice::ExportedPath),
            ],
        )
        .await
        .unwrap();

        assert_eq!(imported[0].projects[0].status, ImportStatus::Created);
        assert_eq!(imported[0].projects[1].status, ImportStatus::Failed);
        assert_eq!(
            imported[0].projects[1].error.as_ref().unwrap().code,
            error_code::ALL_PROJECTS_ALREADY_ADDED
        );
        assert_eq!(stored_projects(&state).len(), 1);
    }

    #[tokio::test]
    async fn importing_always_creates_a_console_even_beside_one_of_the_same_name() {
        let (state, _dir) = app_state("import-same-name");
        let hub = document(vec![console("Hub", Vec::new())]);

        import(&state, &hub, Vec::new()).await.unwrap();
        import(&state, &hub, Vec::new()).await.unwrap();

        let consoles = state.store.list_consoles().unwrap();
        assert_eq!(consoles.len(), 2);
        assert_ne!(consoles[0].id, consoles[1].id);
        assert_ne!(consoles[0].workdir, consoles[1].workdir);
    }

    #[tokio::test]
    async fn a_refused_import_leaves_nothing_behind() {
        let area = ScratchDir::new("import-refused-area");
        let (state, _dir) = app_state("import-refused");
        let repo = directory(&area, "repo");
        let valid = |extra: Vec<DocumentConsole>| {
            let mut consoles = vec![console("Hub", vec![project("p", &repo.to_string_lossy())])];
            consoles.extend(extra);
            document(consoles)
        };
        let mut bad_icon = console("Second", Vec::new());
        bad_icon.icon = Some("not-an-image".to_string());
        let mut newer = valid(Vec::new());
        newer["version"] = json!(config_export::VERSION + 1);
        let resolve = || DirectoryChoice::ExportedPath;

        let refusals = [
            (
                newer,
                Vec::new(),
                error_code::CONFIGURATION_VERSION_UNSUPPORTED,
            ),
            // The first console is fine; the second one's avatar refuses the whole import.
            (
                valid(vec![bad_icon]),
                Vec::new(),
                error_code::ICON_NOT_AN_IMAGE,
            ),
            (
                valid(Vec::new()),
                vec![at(1, 0, resolve())],
                error_code::IMPORT_RESOLUTION_INVALID,
            ),
            (
                valid(Vec::new()),
                vec![at(0, 1, resolve())],
                error_code::IMPORT_RESOLUTION_INVALID,
            ),
            (
                valid(Vec::new()),
                vec![at(0, 0, resolve()), at(0, 0, DirectoryChoice::Clone)],
                error_code::IMPORT_RESOLUTION_INVALID,
            ),
            (
                valid(Vec::new()),
                vec![at(
                    0,
                    0,
                    DirectoryChoice::Directory {
                        path: "relative/dir".to_string(),
                    },
                )],
                error_code::IMPORT_RESOLUTION_INVALID,
            ),
        ];
        for (document, resolutions, code) in refusals {
            let err = apply(&state, &document, resolutions, false)
                .await
                .expect_err(code);
            assert_eq!(code_of(&err), code);
            assert!(state.store.list_consoles().unwrap().is_empty(), "{code}");
            assert!(stored_projects(&state).is_empty(), "{code}");
        }
    }

    #[tokio::test]
    async fn an_exported_configuration_is_imported_as_it_is() {
        let area = ScratchDir::new("import-roundtrip-area");
        let (source, _source_dir) = app_state("import-roundtrip-source");
        let repo = directory(&area, "repo");
        let exported_console = crate::protocol::Console {
            id: "console-1".to_string(),
            name: "Hub".to_string(),
            workdir: "/consoles/console-1".to_string(),
            console_session_agent: Agent::Claude,
            default_agent: Agent::Claude,
            project_session_agent: Some(Agent::Grok),
            claude_project_session_model: Some("opus".to_string()),
            claude_project_session_effort: Some("high".to_string()),
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
            created_at: 1,
        };
        source.store.insert_console(&exported_console).unwrap();
        let mut stored = Project {
            id: "project-1".to_string(),
            console_id: "console-1".to_string(),
            host_id: crate::store::LOCAL_HOST_ID.to_string(),
            name: "repo".to_string(),
            path: repo.to_string_lossy().into_owned(),
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
            trust_consent: true,
            pinned: false,
            tags: vec!["t".to_string()],
            description: Some("Round trip.".to_string()),
        };
        stored.default_agent = Some(Agent::Codex);
        source.store.insert_project(&stored).unwrap();
        let exported =
            serde_json::to_value(config_export::build(&source.store, None).unwrap()).unwrap();

        let (target, _target_dir) = app_state("import-roundtrip-target");
        let consoles = inspect(&target, &exported).await.unwrap().consoles;
        assert_eq!(
            consoles[0].projects[0].default,
            Some(ChoiceKind::ExportedPath)
        );
        import(
            &target,
            &exported,
            vec![at(0, 0, DirectoryChoice::ExportedPath)],
        )
        .await
        .unwrap();

        let created = stored_projects(&target).remove(0);
        assert_eq!(
            (created.name.as_str(), created.default_agent),
            ("repo", Some(Agent::Codex))
        );
        assert_eq!(created.tags, ["t"]);
        assert_eq!(created.description.as_deref(), Some("Round trip."));
        assert!(!created.trust_consent, "trust is not carried over");
        let console = target.store.list_consoles().unwrap().remove(0);
        assert_eq!(console.project_session_agent, Some(Agent::Grok));
        assert_eq!(
            console.claude_project_session_model.as_deref(),
            Some("opus")
        );
        assert_eq!(
            console.claude_project_session_effort.as_deref(),
            Some("high")
        );

        // A document written before the setting was carried names none, so project sessions
        // follow `default_agent`.
        let mut legacy = exported.clone();
        legacy["consoles"][0]
            .as_object_mut()
            .unwrap()
            .remove("project_session_agent")
            .expect("the export carries the setting");
        for key in [
            "claude_project_session_model",
            "claude_project_session_effort",
        ] {
            legacy["consoles"][0]
                .as_object_mut()
                .unwrap()
                .remove(key)
                .expect("the export carries the setting");
        }
        let (older, _older_dir) = app_state("import-roundtrip-legacy");
        import(
            &older,
            &legacy,
            vec![at(0, 0, DirectoryChoice::ExportedPath)],
        )
        .await
        .unwrap();
        let console = older.store.list_consoles().unwrap().remove(0);
        assert_eq!(console.project_session_agent, None);
        assert_eq!(console.project_session_default(), Agent::Claude);
        assert_eq!(console.claude_project_session_model, None);
        assert_eq!(console.claude_project_session_effort, None);
    }

    #[tokio::test]
    async fn the_documents_settings_are_shown_by_inspecting_and_applied_only_when_asked_for() {
        let (state, _dir) = app_state("import-settings");
        let full = with_settings(document(vec![console("Hub", Vec::new())]), true, false);
        assert!(!state.store.auto_pull_repositories().unwrap());
        assert!(state.store.auto_delete_unresumable_archives().unwrap());

        let inspection = inspect(&state, &full).await.unwrap();
        let settings = inspection.settings.expect("the document carries settings");
        assert!(settings.auto_pull_repositories);
        assert!(!settings.current_auto_pull_repositories);
        assert_eq!(settings.auto_delete_unresumable_archives, Some(false));
        assert!(settings.current_auto_delete_unresumable_archives);
        assert!(!state.store.auto_pull_repositories().unwrap());
        assert!(state.store.auto_delete_unresumable_archives().unwrap());
        assert!(inspect(&state, &document(Vec::new()))
            .await
            .unwrap()
            .settings
            .is_none());

        let imported = apply(&state, &full, Vec::new(), false).await.unwrap();
        assert_eq!(imported.settings_applied, None);
        assert!(!state.store.auto_pull_repositories().unwrap());
        assert!(state.store.auto_delete_unresumable_archives().unwrap());

        let imported = apply(&state, &full, Vec::new(), true).await.unwrap();
        assert_eq!(
            imported.settings_applied,
            Some(DocumentSettings {
                auto_pull_repositories: true,
                auto_delete_unresumable_archives: Some(false),
            })
        );
        assert!(state.store.auto_pull_repositories().unwrap());
        assert!(!state.store.auto_delete_unresumable_archives().unwrap());
    }

    /// A full export carries both application settings and a single console's carries none, and
    /// importing the full one on another machine sets both.
    #[tokio::test]
    async fn a_full_export_round_trips_both_settings_and_a_console_export_carries_none() {
        let (source, source_dir) = app_state("import-round-trip-source");
        source.store.set_auto_pull_repositories(true).unwrap();
        source
            .store
            .set_auto_delete_unresumable_archives(false)
            .unwrap();
        let hub = crate::test_support::console(&source_dir);
        source.store.insert_console(&hub).unwrap();
        let full = serde_json::to_value(crate::config_export::build(&source.store, None).unwrap())
            .unwrap();
        let single = serde_json::to_value(
            crate::config_export::build(&source.store, Some(&hub.id)).unwrap(),
        )
        .unwrap();
        assert!(single
            .get("settings")
            .is_none_or(|settings| settings.is_null()));

        let (target, _target_dir) = app_state("import-round-trip-target");
        apply(&target, &single, Vec::new(), false).await.unwrap();
        assert!(!target.store.auto_pull_repositories().unwrap());
        assert!(target.store.auto_delete_unresumable_archives().unwrap());
        apply(&target, &full, Vec::new(), true).await.unwrap();
        assert!(target.store.auto_pull_repositories().unwrap());
        assert!(!target.store.auto_delete_unresumable_archives().unwrap());
    }

    /// A document from before the cleanup setting existed leaves it as the importing machine has
    /// it.
    #[tokio::test]
    async fn a_document_without_the_cleanup_setting_leaves_it_as_it_is() {
        let (state, _dir) = app_state("import-older-settings");
        state
            .store
            .set_auto_delete_unresumable_archives(false)
            .unwrap();
        let mut older = document(vec![console("Hub", Vec::new())]);
        older["settings"] = json!({ "auto_pull_repositories": true });

        let inspection = inspect(&state, &older).await.unwrap();
        let settings = inspection.settings.unwrap();
        assert_eq!(settings.auto_delete_unresumable_archives, None);
        assert!(!settings.current_auto_delete_unresumable_archives);

        let imported = apply(&state, &older, Vec::new(), true).await.unwrap();
        assert_eq!(
            imported.settings_applied,
            Some(DocumentSettings {
                auto_pull_repositories: true,
                auto_delete_unresumable_archives: None,
            })
        );
        assert!(state.store.auto_pull_repositories().unwrap());
        assert!(!state.store.auto_delete_unresumable_archives().unwrap());
    }

    #[tokio::test]
    async fn a_document_exported_with_the_older_setting_name_still_applies_it() {
        let (state, _dir) = app_state("import-legacy-setting");
        let mut older = document(vec![console("Hub", Vec::new())]);
        older["settings"] = json!({ "auto_sync_repositories": true });

        let imported = apply(&state, &older, Vec::new(), true).await.unwrap();
        assert_eq!(
            imported.settings_applied,
            Some(DocumentSettings {
                auto_pull_repositories: true,
                auto_delete_unresumable_archives: None,
            })
        );
        assert!(state.store.auto_pull_repositories().unwrap());
    }

    #[tokio::test]
    async fn a_document_without_a_console_session_prompt_imports_a_console_with_none() {
        let (state, _dir) = app_state("import-no-prompt");
        let mut older = document(vec![console("Hub", Vec::new())]);
        older["consoles"][0]
            .as_object_mut()
            .unwrap()
            .remove("console_session_prompt");

        apply(&state, &older, Vec::new(), false).await.unwrap();
        let stored = state.store.list_consoles().unwrap().remove(0);
        assert_eq!(stored.console_session_prompt, None);
    }

    #[tokio::test]
    async fn importing_preserves_legacy_project_text_beyond_editing_limits() {
        let area = ScratchDir::new("import-legacy-project-text-area");
        let (state, _dir) = app_state("import-legacy-project-text");
        let repo = directory(&area, "repo");
        let mut legacy = project(&"界".repeat(101), &repo.to_string_lossy());
        legacy.description = Some("😀".repeat(2_001));
        let original = document(vec![console("Hub", vec![legacy.clone()])]);
        import(
            &state,
            &original,
            vec![at(0, 0, DirectoryChoice::ExportedPath)],
        )
        .await
        .unwrap();
        let exported =
            serde_json::to_value(config_export::build(&state.store, None).unwrap()).unwrap();
        let (target, _target_dir) = app_state("import-legacy-project-text-target");
        import(
            &target,
            &exported,
            vec![at(0, 0, DirectoryChoice::ExportedPath)],
        )
        .await
        .unwrap();
        let restored = stored_projects(&target).remove(0);
        assert_eq!(restored.name, legacy.name);
        assert_eq!(restored.description, legacy.description);
    }

    #[tokio::test]
    async fn a_document_without_a_project_description_imports_a_project_with_none() {
        let area = ScratchDir::new("import-no-description-area");
        let (state, _dir) = app_state("import-no-description");
        let repo = directory(&area, "repo");
        let mut older = document(vec![console(
            "Hub",
            vec![project("repo", &repo.to_string_lossy())],
        )]);
        older["consoles"][0]["projects"][0]
            .as_object_mut()
            .unwrap()
            .remove("description");

        import(
            &state,
            &older,
            vec![at(0, 0, DirectoryChoice::ExportedPath)],
        )
        .await
        .unwrap();
        assert_eq!(stored_projects(&state).remove(0).description, None);
    }

    #[tokio::test]
    async fn asking_for_settings_a_document_does_not_carry_is_refused_before_anything_is_created() {
        let (state, _dir) = app_state("import-settings-missing");

        let err = apply(
            &state,
            &document(vec![console("Hub", Vec::new())]),
            Vec::new(),
            true,
        )
        .await
        .expect_err("refused");

        assert_eq!(code_of(&err), error_code::IMPORT_SETTINGS_MISSING);
        assert!(state.store.list_consoles().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_existence_check_that_does_not_come_back_offers_nothing_and_is_not_waited_for() {
        let probe = |project| Probe {
            console: 0,
            project,
            exported: None,
            derived: None,
            clone: None,
        };
        let offered = Offers {
            exported_path: Some("/there".to_string()),
            ..Offers::default()
        };
        let answer = offered.clone();

        // The deadline stays short, so a machine that stalls for seconds can keep the quick probe
        // from landing inside it; the scenario then runs again until `PATIENCE` is used up.
        let start = std::time::Instant::now();
        loop {
            let (answer, stalled_for) = (answer.clone(), Duration::from_secs(2));
            let found = probe_offers(
                vec![probe(0), probe(1)],
                Duration::from_millis(200),
                move |probe| {
                    if probe.project == 0 {
                        // A stat on a mount that has stopped answering.
                        std::thread::sleep(stalled_for);
                    }
                    ((probe.console, probe.project), answer.clone())
                },
            )
            .await;
            assert!(!found.contains_key(&(0, 0)));
            if found.get(&(0, 1)) == Some(&offered) {
                break;
            }
            assert!(
                start.elapsed() < PATIENCE,
                "the quick probe never landed in time"
            );
        }
    }

    #[tokio::test]
    async fn a_console_that_cannot_be_created_is_reported_and_the_import_goes_on_to_the_result() {
        let area = ScratchDir::new("import-console-fails-area");
        let (state, dir) = app_state("import-console-fails");
        // The directory consoles' working directories are made in is a file, so none can be.
        std::fs::write(dir.join("consoles"), "").unwrap();
        let repo = directory(&area, "repo");
        let hub = console("Hub", vec![project("p", &repo.to_string_lossy())]);
        let applied = with_settings(document(vec![hub]), true, false);

        let imported = apply(
            &state,
            &applied,
            vec![at(0, 0, DirectoryChoice::ExportedPath)],
            true,
        )
        .await
        .expect("a console that fails is reported, not raised");

        let console = &imported.consoles[0];
        assert_eq!(console.status, ImportStatus::Failed);
        assert_eq!(console.console, None);
        assert!(console.error.is_some());
        assert_eq!(console.projects[0].status, ImportStatus::NotAttempted);
        assert!(stored_projects(&state).is_empty());
        // The rest of the import still happens and is still reported.
        assert!(imported.settings_applied.is_some());
        assert!(state.store.auto_pull_repositories().unwrap());
    }

    #[tokio::test]
    async fn a_clone_whose_target_directory_exists_fails_that_project_alone() {
        let area = ScratchDir::new("import-clone-exists-area");
        let clone_dir = directory(&area, "projects");
        let (state, _dir) = state_cloning_into("import-clone-exists", &clone_dir);
        directory(&clone_dir, "taken");
        let good = directory(&area, "good");
        let mut cloned = project("cloned", "/gone/cloned");
        cloned.remote_url = Some("https://example.invalid/o/taken.git".to_string());
        let hub = console(
            "Hub",
            vec![cloned, project("good", &good.to_string_lossy())],
        );

        let imported = import(
            &state,
            &document(vec![hub]),
            vec![
                at(0, 0, DirectoryChoice::Clone),
                at(0, 1, DirectoryChoice::ExportedPath),
            ],
        )
        .await
        .unwrap();

        assert_eq!(imported[0].projects[0].status, ImportStatus::Failed);
        assert_eq!(
            imported[0].projects[0].error.as_ref().unwrap().code,
            error_code::PATH_ALREADY_EXISTS
        );
        assert_eq!(imported[0].projects[1].status, ImportStatus::Created);
        assert_eq!(stored_projects(&state).len(), 1);
    }
}
