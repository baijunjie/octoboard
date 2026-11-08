//! The configuration document and the findings of its import, as they go on the wire: the
//! document an export builds and a client keeps, what an inspection says importing it would do,
//! and what an import then did. `apps/daemon/PROTOCOL.md` ("Configuration export" and
//! "Configuration import") is the specification.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::agents::{AgentId, Launch, PerAgent};
use super::codes::Code;
use super::errors::Params;
use super::git_workflow::{GitWorkflowOverride, GitWorkflowSettings};
use super::records::ProjectSource;
use crate::providers::SettingValue;

/// Names the kind of file, so one that is not a configuration document is told apart from one
/// whose version is unsupported.
pub const CONFIGURATION_FORMAT: &str = "octoboard-configuration";

/// Raised when a change makes a reader of a released version misread a document: a field removed
/// or given another meaning. A field added, which a reader that does not know it ignores, does not raise it.
///
/// An import reads this version and, through the steps of `config_document::upgrade`, every older
/// one from the first released; it refuses any other. Version changes and upgrade steps are written
/// only between released versions: a change no release has carried is made in place, at this
/// version, and raising it past a released version adds the step that upgrades a document of that
/// version, so an export made by the release stays importable. Version 1 is what the beta releases
/// wrote; 2 moved the Agent accounts settings into the accounts region and dropped an account's
/// sharing flag.
pub const CONFIGURATION_VERSION: u32 = 2;

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigurationDocument {
    #[cfg_attr(test, ts(type = "\"octoboard-configuration\""))]
    pub format: String,
    pub version: u32,
    /// The Octoboard version that produced the document, for diagnosing one that misbehaves. An
    /// import never reads it; `null` when the producer did not record one.
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
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentAccounts {
    /// The agents the user has added, in their order. Absent when the exporting machine had not
    /// yet decided the list (it was still finding out which agents are installed), which on import
    /// adds none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_agents: Option<Vec<AgentId>>,
    /// One entry for each agent that has an account of its own, a named default account or a
    /// turned-off default account, in `AgentId::ALL` order.
    pub agents: Vec<DocumentAgentAccounts>,
    /// The settings of the Agent accounts section that are not an account's own. `null` in a
    /// document upgraded from version 1 that had no `settings` region to take them from: importing
    /// it leaves them as they are.
    pub agent_settings: Option<DocumentAgentSettings>,
}

/// The settings of the Agent accounts section that are not an account's own: the three switches,
/// the tags of each agent and the user's choices for the agents' provider settings. They travel
/// with the accounts, since they only mean something to the agents and accounts of the machine
/// they are applied to.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentAgentSettings {
    pub auto_select_account: bool,
    pub auto_switch_on_usage_limit: bool,
    pub auto_start_quota_windows: bool,
    /// The tags the user put on each agent (by wire name), for the agents that have any, removed
    /// agents' included. An agent the document does not list is left as it is on import.
    #[cfg_attr(test, ts(type = "Partial<Record<Agent, Array<string>>>"))]
    pub agent_tags: BTreeMap<String, Vec<String>>,
    /// The choices the user made for the settings of each provider (by provider id, then setting
    /// id) that a configuration export carries (`SettingDescriptor::exported`). A setting they
    /// never chose for is not listed, so the importing machine keeps following its own default for
    /// it, and a setting the document does not list is left as it is.
    #[cfg_attr(
        test,
        ts(type = "Partial<Record<Agent, Record<string, boolean | string>>>")
    )]
    pub provider_settings: BTreeMap<String, BTreeMap<String, SettingValue>>,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentAgentAccounts {
    pub agent: AgentId,
    pub default_account: DocumentDefaultAccount,
    /// The accounts the user owns, in the agent's display order.
    pub accounts: Vec<DocumentAccount>,
}

/// What the document says of an agent's default account. Its directory is whatever the agent
/// resolves on the machine, so it is not carried.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentDefaultAccount {
    /// The name the user gave it; `null` when it is unnamed.
    pub name: Option<String>,
    /// Its place in the display order: how many of the agent's accounts come before it.
    pub position: usize,
    /// Whether the user turned it off. `null` in a document upgraded from version 1, which did not
    /// carry the flag: importing it leaves this machine's flag as it is.
    pub disabled: Option<bool>,
}

/// An account the user owns. Its id, its login, everything the agent keeps in its directory and
/// whether it shares the agent's default configuration (which moves files outside Octoboard's own
/// directory, so it is turned on by hand) stay behind.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentAccount {
    pub name: String,
    /// `~/<relative path>` for a directory under the exporting machine's home directory (also `~`
    /// for the home directory itself); otherwise the absolute path. Whatever the export's paths
    /// choice: an account cannot exist without its directory.
    pub directory: String,
    pub disabled: bool,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentSettings {
    pub auto_close_idle_shell_terminals: bool,
    pub auto_pull_repositories: bool,
    pub auto_delete_unresumable_archives: bool,
    pub sidebar_order_by_status: bool,
    pub auto_approve_session_requests: bool,
    pub auto_approve_resume_requests: bool,
    pub auto_approve_account_switches: bool,
    pub auto_approve_handoffs: bool,
    pub ask_questions_as_choices: bool,
    /// Whether the daemon resumes the interrupted sessions when it starts (see
    /// `Settings.resume_sessions_on_start`). Absent in a document from a build that had no such
    /// setting, which an import then leaves as this machine has it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_sessions_on_start: Option<bool>,
    /// The application-wide git workflow settings.
    pub git_workflow: GitWorkflowSettings,
    /// The default clone directory as `~/<relative path>` under the exporting machine's home
    /// directory (also `~` for the home directory itself), otherwise the absolute path, whatever
    /// the export's paths choice. An import applies it only when the user asks to, and it must be
    /// usable here (absolute, or starting with `~/`). `null` in a document upgraded from version
    /// 1, which did not carry it: importing it leaves this machine's as it is.
    pub default_clone_dir: Option<String>,
    /// The language tag of the exporting client's interface (see `Settings.interface_language`):
    /// the daemon writes the one it holds and the client replaces it with the one it shows, which
    /// is the language the user chose. An import applies it only when this build offers it. Absent
    /// when neither knew one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface_language: Option<String>,
    /// The window's appearance (`light`, `dark` or `system`), which only the client holds (in its
    /// `localStorage`): the client adds it to a document it saves and applies it from an import's
    /// reply, so the daemon builds none, stores it nowhere and passes it through an import's reply
    /// as it came. A value the importing client does not know is ignored. Absent in a document a
    /// client saved without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<String>,
    /// How a rendered Markdown document is styled (the style's id), carried like `appearance`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markdown_style: Option<String>,
    /// Whether closing the window leaves the app running in the background, carried like
    /// `appearance`. Only a client that has the setting (the desktop app) writes it, and only
    /// such a client applies it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_in_background: Option<bool>,
    /// Whether the menu bar icon (the notification area icon on Windows) is shown, carried like
    /// `run_in_background`: only a client that has the icon (the desktop app) writes it, and only
    /// such a client applies it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_menu_bar_icon: Option<bool>,
    /// Whether the desktop app holds the computer and its display awake (Prevent sleep), carried
    /// like `run_in_background`: only a client that can (the desktop app) writes it, and only such
    /// a client applies it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_awake: Option<bool>,
    /// When Prevent sleep holds: `always`, or `working` (while a session is working), carried like
    /// `keep_awake`. A value the importing client does not know is ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_awake_when: Option<String>,
    /// The keyboard shortcuts the exporting client has changed, by command id, which only the
    /// client reads and writes: the client adds them to a full export it saves (an empty object
    /// when it has none changed, which on import means every shortcut at its default) and applies
    /// them from an import. The daemon builds none, stores them nowhere and does not read them, so
    /// they pass through an import's reply as they came. Absent in a document a client saved
    /// without them: importing it leaves the client's shortcuts as they are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(type = "unknown"))]
    pub keyboard_shortcuts: Option<serde_json::Value>,
}

/// A reference to an account: its name, which is unique within its agent. The agent is the one
/// whose entry holds the reference (`accounts.claude` is a Claude Code account), so it is not
/// repeated here, where a hand-edited document could contradict it. The importing machine rebinds
/// the reference to the account of that agent and name.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountName {
    pub name: String,
}

/// A project's choice of account for one agent; `ProjectAccount` with the account named rather
/// than numbered.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectAccountRef {
    /// Take the console's reference for the agent.
    #[default]
    Inherit,
    /// The agent's default account, even where the console references a named one.
    Default,
    Account {
        name: String,
    },
}

/// A console's own settings and the projects associated with it.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentConsole {
    pub name: String,
    pub icon: Option<String>,
    pub default_agent: AgentId,
    /// The agent a project session falls back to; absent or `null` follows `default_agent`.
    pub project_session_agent: Option<AgentId>,
    /// Each agent's account, by name; `null` is that agent's default account.
    pub accounts: PerAgent<Option<AccountName>>,
    /// Each agent's model and effort for a console session.
    pub launch: PerAgent<Launch>,
    /// The model and effort a project session falls back to.
    pub project_session_launch: PerAgent<Launch>,
    /// The user's own addition to the console session's instructions; absent is none.
    pub console_session_prompt: Option<String>,
    pub projects: Vec<DocumentProject>,
}

/// One project's configuration. `path` is an absolute path of the exporting machine, or only the
/// directory's name when the export left paths out. `remote_url` is what was recorded for the
/// project, absent for a directory that is not a repository or a repository with no remote, and
/// also when the export left paths out and the remote is not one that can be cloned from (a local
/// path would reveal one).
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentProject {
    pub name: String,
    pub path: String,
    pub source: ProjectSource,
    pub remote_url: Option<String>,
    pub default_agent: Option<AgentId>,
    pub pinned: bool,
    pub tags: Vec<String>,
    /// The user's own summary of the project; absent is none.
    pub description: Option<String>,
    /// Each agent's account choice.
    pub accounts: PerAgent<ProjectAccountRef>,
    /// Each agent's model and effort for the project's sessions.
    pub launch: PerAgent<Launch>,
    /// The project's own git workflow settings, each item `null` when it follows the
    /// application-wide value.
    pub git_workflow: GitWorkflowOverride,
}

/// Which consoles an export holds. On the wire it is the request's `consoles` field.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportedConsoles {
    /// The console the request names.
    One,
    All,
    None,
}

/// Which directory a project is placed in. [`DirectoryChoice`] is the caller's pick of one, and
/// this is the bare name that results report.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryChoiceKind {
    /// The path the document records.
    ExportedPath,
    /// The last segment of the recorded path, under this machine's default clone directory.
    DefaultCloneDir,
    /// A fresh clone of the recorded remote, into the default clone directory.
    Clone,
    /// A directory the user pointed at.
    Directory,
}

impl DirectoryChoiceKind {
    /// The name it has on the wire, which is what the serialization writes.
    pub fn wire_name(self) -> String {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::String(name)) => name,
            _ => unreachable!("a unit variant serializes as a string"),
        }
    }
}

/// The choice made for one project when applying.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DirectoryChoice {
    ExportedPath,
    DefaultCloneDir,
    Clone,
    Directory { path: String },
}

impl DirectoryChoice {
    pub fn kind(&self) -> DirectoryChoiceKind {
        match self {
            Self::ExportedPath => DirectoryChoiceKind::ExportedPath,
            Self::DefaultCloneDir => DirectoryChoiceKind::DefaultCloneDir,
            Self::Clone => DirectoryChoiceKind::Clone,
            Self::Directory { .. } => DirectoryChoiceKind::Directory,
        }
    }
}

/// The directory each choice would use for a project, present only for the choices its state
/// allows.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DirectoryOffers {
    pub exported_path: Option<String>,
    pub default_clone_dir: Option<String>,
    pub clone: Option<String>,
}

impl DirectoryOffers {
    /// The recorded path wherever it exists, otherwise the derived one wherever that does,
    /// otherwise a clone; `None` leaves the project for the user to place or leave out.
    pub fn default_choice(&self) -> Option<DirectoryChoiceKind> {
        if self.exported_path.is_some() {
            Some(DirectoryChoiceKind::ExportedPath)
        } else if self.default_clone_dir.is_some() {
            Some(DirectoryChoiceKind::DefaultCloneDir)
        } else if self.clone.is_some() {
            Some(DirectoryChoiceKind::Clone)
        } else {
            None
        }
    }
}

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
    pub agent: AgentId,
    pub name: String,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct InspectedProject {
    pub name: String,
    /// The path as the document records it.
    pub path: String,
    /// Whether `path` is absolute in the other operating system's style (a Windows path read on
    /// macOS or Linux, a Unix one read on Windows). Such a path names nothing on this machine, so
    /// no directory is derived from it and `exported_path` and `default_clone_dir` are never offered.
    pub foreign_path: bool,
    pub remote_url: Option<String>,
    pub offers: DirectoryOffers,
    #[cfg_attr(
        test,
        ts(type = "\"exported_path\" | \"default_clone_dir\" | \"clone\" | null")
    )]
    pub default: Option<DirectoryChoiceKind>,
    /// What the clone-dependent choices (`default_clone_dir`, `clone`) come to under the document's
    /// own default clone directory, which an import takes when it applies the settings and is not
    /// told to keep this machine's. Absent unless the document carries a usable directory that
    /// differs from this machine's; `offers` and `default` are always this machine's.
    pub in_document_clone_dir: Option<OffersUnderCloneDir>,
    pub account_fallbacks: Vec<AccountFallback>,
}

/// The offers of a project, and the choice an import would preselect, with the clone-dependent
/// ones worked out against another default clone directory than this machine's.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct OffersUnderCloneDir {
    pub offers: DirectoryOffers,
    #[cfg_attr(
        test,
        ts(type = "\"exported_path\" | \"default_clone_dir\" | \"clone\" | null")
    )]
    pub default: Option<DirectoryChoiceKind>,
}

/// The application settings a document carries, beside the value this machine has now, so a client
/// can show what applying them would change.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct InspectedSettings {
    pub auto_close_idle_shell_terminals: bool,
    pub auto_pull_repositories: bool,
    pub current_auto_close_idle_shell_terminals: bool,
    pub current_auto_pull_repositories: bool,
    pub auto_delete_unresumable_archives: bool,
    pub current_auto_delete_unresumable_archives: bool,
    pub sidebar_order_by_status: bool,
    pub current_sidebar_order_by_status: bool,
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
    /// The document's value, or this machine's own when the document carries none, so the line
    /// reads as no change.
    pub resume_sessions_on_start: bool,
    pub current_resume_sessions_on_start: bool,
    pub git_workflow: GitWorkflowSettings,
    pub current_git_workflow: GitWorkflowSettings,
    /// The document's default clone directory as it resolves on this machine (`~/` expanded);
    /// absent when the document carries none or it is not usable here (neither absolute nor
    /// starting with `~/`), which an import leaves out.
    pub default_clone_dir: Option<String>,
    /// Whether the document's default clone directory is absolute in the other operating system's
    /// style (a Windows path read on macOS or Linux, a Unix one read on Windows). It is then the
    /// reason `default_clone_dir` is absent, which a client can say.
    pub foreign_default_clone_dir: bool,
    /// This machine's default clone directory.
    pub current_default_clone_dir: String,
}

/// The settings of the Agent accounts section a document carries, beside the value this machine
/// has now, so a client can show what applying them would change.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct InspectedAgentSettings {
    pub auto_select_account: bool,
    pub current_auto_select_account: bool,
    pub auto_switch_on_usage_limit: bool,
    pub current_auto_switch_on_usage_limit: bool,
    pub auto_start_quota_windows: bool,
    pub current_auto_start_quota_windows: bool,
    /// The document's choices for the exported provider settings, those this build declares only
    /// (see `DocumentAgentSettings::provider_settings`).
    #[cfg_attr(
        test,
        ts(type = "Partial<Record<Agent, Record<string, boolean | string>>>")
    )]
    pub provider_settings: BTreeMap<String, BTreeMap<String, SettingValue>>,
    /// The values in effect on this machine of the exported provider settings.
    #[cfg_attr(
        test,
        ts(type = "Partial<Record<Agent, Record<string, boolean | string | null>>>")
    )]
    pub current_provider_settings: BTreeMap<String, BTreeMap<String, Option<SettingValue>>>,
    #[cfg_attr(test, ts(type = "Partial<Record<Agent, Array<string>>>"))]
    pub agent_tags: BTreeMap<String, Vec<String>>,
    #[cfg_attr(test, ts(type = "Partial<Record<Agent, Array<string>>>"))]
    pub current_agent_tags: BTreeMap<String, Vec<String>>,
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
    pub code: Code,
    pub params: Params,
    pub message: String,
}

/// One thing an import creates or applies, named by its place in the document: the indices are
/// those of `resolutions` and `account_resolutions`.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportItem {
    Console {
        console: usize,
    },
    Project {
        console: usize,
        project: usize,
    },
    /// The `account`th account of `agent` in the document, the agent's parts read as one.
    Account {
        agent: AgentId,
        account: usize,
    },
    /// The default clone directory, written before anything else is created.
    CloneDir,
    /// The settings of the Agent accounts section, applied after the accounts.
    AgentSettings,
    /// The application settings, without the default clone directory.
    Settings,
}

/// Where an item of a running import stands.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ImportItemState {
    /// Not reached yet.
    Waiting,
    /// Being created or applied; for a console, only until it exists.
    Creating,
    /// A project being cloned, which may take minutes.
    Cloning,
    /// Created or applied, or an account that existed already and is used as it is. A console is
    /// done once it is created, before its projects are handled.
    Done,
    /// The same failure the result reports for it.
    Failed { error: ImportFailure },
    /// Its console could not be created, so it was never tried.
    NotAttempted,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct ImportedProject {
    pub name: String,
    pub status: ImportStatus,
    /// How its directory was obtained, for a created project.
    pub via: Option<DirectoryChoiceKind>,
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

/// The choice made for one account of the document when applying: another directory, or leaving
/// the account out. Exactly one of the two is given.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Deserialize)]
pub struct AccountResolution {
    pub agent: AgentId,
    /// The account's index among the document's accounts of `agent`, in the order it lists them.
    pub account: usize,
    /// An absolute path or one starting with `~/`.
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub directory: Option<String>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub leave_out: bool,
}

/// What importing one account of the document would do.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    /// It would be created.
    New,
    /// An account of the same agent has its name already; it is used as it is and never modified.
    Existing,
    /// The proposed directory is already used by another account of the agent, the default account
    /// included.
    DirectoryInUse,
    /// The Add account form would refuse its name or directory.
    Invalid,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct InspectedAccount {
    pub name: String,
    /// The directory as the document records it.
    pub directory: String,
    /// Whether `directory` is absolute in the other operating system's style (a Windows path read
    /// on macOS or Linux, a Unix one read on Windows). It then names nothing on this machine, and
    /// `proposed_directory` is absent; a client can say why.
    pub foreign_directory: bool,
    /// The directory as it resolves on this machine (`~/` expanded); absent when the document's
    /// cannot be resolved.
    pub proposed_directory: Option<String>,
    pub status: AccountStatus,
    /// The name of the account that uses the proposed directory, for `directory_in_use`.
    pub in_use_by: Option<String>,
    /// Why it is `invalid`.
    pub error: Option<ImportFailure>,
    pub disabled: bool,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct InspectedAgentAccounts {
    pub agent: AgentId,
    pub accounts: Vec<InspectedAccount>,
    /// The name the document gives the agent's default account, when it gives one.
    pub default_name: Option<String>,
    /// Whether importing would apply `default_name`: this machine's default account has no name of
    /// its own and the name collides with no account of the agent, those the import creates
    /// included.
    pub default_name_applies: bool,
    /// Whether the document says the default account is turned off; absent when it does not say.
    pub default_disabled: Option<bool>,
    /// Whether importing would change this machine's default account's flag to `default_disabled`:
    /// the document says and this machine's differs.
    pub default_disabled_applies: bool,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct InspectedAccounts {
    /// The agents of the document not yet on this machine's added agents, which importing would
    /// add, in the document's order.
    pub agents_to_add: Vec<AgentId>,
    pub agents: Vec<InspectedAgentAccounts>,
    /// Absent when the document carries no settings of the Agent accounts section.
    pub agent_settings: Option<InspectedAgentSettings>,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountOutcome {
    Created,
    /// An account of the agent with the name was there; used as it is.
    Existing,
    /// A resolution said to leave it out.
    LeftOut,
    /// It could not be created; `error` says why.
    Failed,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct ImportedAccount {
    pub name: String,
    pub status: AccountOutcome,
    /// The id of the account created, or of the one that existed.
    pub account: Option<String>,
    pub error: Option<ImportFailure>,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct ImportedAgentAccounts {
    pub agent: AgentId,
    pub accounts: Vec<ImportedAccount>,
    /// Whether the document's name for the default account was applied.
    pub default_name_applied: bool,
    /// The flag this machine's default account was changed to, as the document says (`true`
    /// disabled, `false` enabled); `None` when the flag was not changed.
    pub default_disabled_applied: Option<bool>,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct ImportedAccounts {
    /// The agents appended to the added agents, in the order they were.
    pub agents_added: Vec<AgentId>,
    pub agents: Vec<ImportedAgentAccounts>,
    /// The settings of the Agent accounts section applied to this machine; absent when the document
    /// carries none, or they could not be applied (`agent_settings_error`).
    pub agent_settings_applied: Option<DocumentAgentSettings>,
    /// Why the settings of the Agent accounts section were not applied.
    pub agent_settings_error: Option<ImportFailure>,
}
