//! The configuration document and the findings of its import, as they go on the wire: the
//! document an export builds and a client keeps, what an inspection says importing it would do,
//! and what an import then did. `apps/daemon/PROTOCOL.md` ("Configuration export" and
//! "Configuration import") is the specification.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::agents::{Agent, Launch, PerAgent};
use super::codes::Code;
use super::errors::Params;
use super::git_workflow::{GitWorkflowOverride, GitWorkflowSettings};
use super::records::ProjectSource;

/// Names the kind of file, so one that is not a configuration document is told apart from one
/// whose version is unsupported.
pub const CONFIGURATION_FORMAT: &str = "octoboard-configuration";

/// Raised when a change makes an older reader misread a document: a field removed or given another
/// meaning. A field added, which a reader that does not know it ignores, does not raise it.
///
/// An import reads only this version and refuses any other. No build has been released yet, so
/// there are no upgrade steps; once a version is released, raising it adds a step that upgrades a
/// document of the previous version, so an export made before stays importable.
pub const CONFIGURATION_VERSION: u32 = 1;

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
    pub added_agents: Option<Vec<Agent>>,
    /// One entry for each agent that has an account of its own or a named default account, in
    /// `Agent::ALL` order.
    pub agents: Vec<DocumentAgentAccounts>,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentAgentAccounts {
    pub agent: Agent,
    pub default_account: DocumentDefaultAccount,
    /// The accounts the user owns, in the agent's display order.
    pub accounts: Vec<DocumentAccount>,
}

/// What the document says of an agent's default account. Its directory is whatever the agent
/// resolves on the machine, so it is not carried, and neither is its disabled flag.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentDefaultAccount {
    /// The name the user gave it; `null` when it is unnamed.
    pub name: Option<String>,
    /// Its place in the display order: how many of the agent's accounts come before it.
    pub position: usize,
}

/// An account the user owns. Its id, its login and everything the agent keeps in its directory
/// stay behind.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentAccount {
    pub name: String,
    /// `~/<relative path>` for a directory under the exporting machine's home directory (also `~`
    /// for the home directory itself); otherwise the absolute path. Whatever the export's paths
    /// choice: an account cannot exist without its directory.
    pub directory: String,
    /// Whether the account shares the agent's default configuration.
    pub share_default_config: bool,
    pub disabled: bool,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentSettings {
    pub auto_pull_repositories: bool,
    pub auto_delete_unresumable_archives: bool,
    pub auto_select_account: bool,
    pub auto_switch_on_usage_limit: bool,
    pub sidebar_order_by_status: bool,
    pub auto_start_quota_windows: bool,
    pub auto_approve_session_requests: bool,
    pub auto_approve_resume_requests: bool,
    pub auto_approve_account_switches: bool,
    pub auto_approve_handoffs: bool,
    pub ask_questions_as_choices: bool,
    /// The application-wide git workflow settings.
    pub git_workflow: GitWorkflowSettings,
    /// The choices the user made on whether an agent other than Claude Code (by wire name) loads
    /// Claude Code's instructions and skills. An agent they never chose for is not listed, so the
    /// importing machine keeps following its own default for it; an agent the document does not
    /// list is left as it is.
    #[cfg_attr(test, ts(type = "Partial<Record<Agent, boolean>>"))]
    pub claude_compatibility: BTreeMap<String, bool>,
    /// The tags the user put on each agent (by wire name), for the agents that have any, removed
    /// agents' included. An agent the document does not list is left as it is on import.
    #[cfg_attr(test, ts(type = "Partial<Record<Agent, Array<string>>>"))]
    pub agent_tags: BTreeMap<String, Vec<String>>,
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
    pub default_agent: Agent,
    /// The agent a project session falls back to; absent or `null` follows `default_agent`.
    pub project_session_agent: Option<Agent>,
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
    pub default_agent: Option<Agent>,
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
    pub offers: DirectoryOffers,
    #[cfg_attr(
        test,
        ts(type = "\"exported_path\" | \"default_clone_dir\" | \"clone\" | null")
    )]
    pub default: Option<DirectoryChoiceKind>,
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
    pub agent: Agent,
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
    /// The directory as it resolves on this machine (`~/` expanded); absent when the document's
    /// cannot be resolved.
    pub proposed_directory: Option<String>,
    pub status: AccountStatus,
    /// The name of the account that uses the proposed directory, for `directory_in_use`.
    pub in_use_by: Option<String>,
    /// Why it is `invalid`.
    pub error: Option<ImportFailure>,
    pub share_default_config: bool,
    pub disabled: bool,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct InspectedAgentAccounts {
    pub agent: Agent,
    pub accounts: Vec<InspectedAccount>,
    /// The name the document gives the agent's default account, when it gives one.
    pub default_name: Option<String>,
    /// Whether importing would apply `default_name`: this machine's default account has no name of
    /// its own and the name collides with no account of the agent, those the import creates
    /// included.
    pub default_name_applies: bool,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct InspectedAccounts {
    /// The agents of the document not yet on this machine's added agents, which importing would
    /// add, in the document's order.
    pub agents_to_add: Vec<Agent>,
    pub agents: Vec<InspectedAgentAccounts>,
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
    /// Why sharing the default configuration, which the document asked for, was not turned on for
    /// an account that was created all the same.
    pub sharing_warning: Option<ImportFailure>,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct ImportedAgentAccounts {
    pub agent: Agent,
    pub accounts: Vec<ImportedAccount>,
    /// Whether the document's name for the default account was applied.
    pub default_name_applied: bool,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct ImportedAccounts {
    /// The agents appended to the added agents, in the order they were.
    pub agents_added: Vec<Agent>,
    pub agents: Vec<ImportedAgentAccounts>,
}
