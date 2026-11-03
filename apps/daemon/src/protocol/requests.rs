//! What a client sends on `/ws/control`: the request envelope and every request but the browse
//! ones, with the request-only values they carry. See "Client to daemon" in
//! `apps/daemon/PROTOCOL.md`.

use serde::Deserialize;

use super::agents::{Agent, Change, Launch, LaunchChange, PerAgent};
#[cfg(test)]
use super::bindings;
use super::configuration::{AccountResolution, ExportedConsoles, ProjectResolution};
use super::fields::{default_true, present_option};
use super::git_workflow::{
    GitWorkflowOverride, GitWorkflowOverrideChanges, GitWorkflowSettingsChanges,
};
use super::records::{ConsoleSessionColour, ProjectAccount, ProjectSource};
use super::session_branches::SessionBranchTarget;
use super::settings::{ApprovalReviewer, ClaudeApprovalMode};

/// The appearance a client has resolved for its window, sent with a launch so Grok's theme matches
/// the terminal that will show it. It is not stored: a later launch sends its own.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalAppearance {
    Light,
    Dark,
}

/// One control-socket frame from the client: an optional request id the daemon echoes back, plus
/// the request itself. The id lets the UI pair a reply with the request that caused it; state
/// changes are broadcast to every client instead and carry no id.
///
/// `id` belongs to the envelope alone. Inside a request, what it acts on is named for its kind —
/// `console`, `project`, `session` — because one flattened object carries both: a request field
/// named `id` would collide with the envelope's and silently lose one of the two, leaving the
/// daemon acting on the request id as if it were a record id.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Deserialize)]
pub struct Request {
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub id: Option<String>,
    #[serde(flatten)]
    pub body: RequestBody,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RequestBody {
    CreateConsole {
        name: String,
        default_agent: Agent,
        /// Absent means project sessions follow `default_agent`.
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        project_session_agent: Option<Agent>,
        /// Each agent's account, by id; an agent left out, or `null`, has its default account. A
        /// reference that names no account of that agent is refused with `unknown_account`.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable, as = "bindings::PartialPerAgent<String>"))]
        accounts: PerAgent<Option<String>>,
        /// Each agent's model and reasoning effort for the console's console sessions; an agent
        /// or a field left out, or blank, means the agent's own default.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable, as = "bindings::PartialPerAgent<bindings::LaunchRequest>"))]
        launch: PerAgent<Launch>,
        /// Each agent's model and reasoning effort for the console's project sessions whose
        /// project and session name none, with the same rule as `launch`.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable, as = "bindings::PartialPerAgent<bindings::LaunchRequest>"))]
        project_session_launch: PerAgent<Launch>,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        icon: Option<String>,
        /// Absent or blank means no addition to the console session's instructions.
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        console_session_prompt: Option<String>,
    },
    UpdateConsole {
        console: String,
        #[cfg_attr(test, ts(optional))]
        name: Option<String>,
        #[cfg_attr(test, ts(optional))]
        default_agent: Option<Agent>,
        /// Absent leaves the setting alone; an explicit `null` makes project sessions follow
        /// `default_agent` again.
        #[serde(default, deserialize_with = "present_option")]
        #[cfg_attr(test, ts(optional))]
        project_session_agent: Option<Option<Agent>>,
        /// Each agent's account: an agent left out is left alone; an explicit `null` clears it
        /// back to that agent's default account. A reference that names no account of that agent
        /// is refused with `unknown_account`.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable, as = "bindings::PartialPerAgent<Change<String>>"))]
        accounts: PerAgent<Change<String>>,
        /// Each agent's console session model and effort: an agent or a field left out is left
        /// alone; an explicit `null` (or a blank string) clears it back to the agent's own
        /// default.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable, as = "bindings::PartialPerAgent<LaunchChange>"))]
        launch: PerAgent<LaunchChange>,
        /// Each agent's project session model and effort, with the same rule as `launch`.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable, as = "bindings::PartialPerAgent<LaunchChange>"))]
        project_session_launch: PerAgent<LaunchChange>,
        /// Absent leaves the avatar alone; an explicit `null` (or a blank string) clears it back
        /// to the default glyph.
        #[serde(default, deserialize_with = "present_option")]
        #[cfg_attr(test, ts(optional))]
        icon: Option<Option<String>>,
        /// Absent leaves the addition alone; an explicit `null` (or a blank string) clears it.
        #[serde(default, deserialize_with = "present_option")]
        #[cfg_attr(test, ts(optional))]
        console_session_prompt: Option<Option<String>>,
    },
    DeleteConsole {
        console: String,
    },
    AddProject {
        console_id: String,
        source: ProjectSource,
        #[cfg_attr(test, ts(optional))]
        path: Option<String>,
        #[cfg_attr(test, ts(optional))]
        remote_url: Option<String>,
        #[cfg_attr(test, ts(optional))]
        name: Option<String>,
        #[cfg_attr(test, ts(optional))]
        default_agent: Option<Agent>,
        /// Each agent's account as a [`ProjectAccount`]; an agent left out, or `null`, inherits
        /// the console's reference. An account that names none of that agent's is refused with
        /// `unknown_account`.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable, as = "bindings::PartialPerAgent<ProjectAccount>"))]
        accounts: PerAgent<Option<ProjectAccount>>,
        /// Each agent's model and reasoning effort for the project's sessions; an agent or a
        /// field left out, or blank, means the agent's own default.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable, as = "bindings::PartialPerAgent<bindings::LaunchRequest>"))]
        launch: PerAgent<Launch>,
        /// Whether an absent `default_agent` is filled in from the directory's marker files. Absent
        /// is true; a client that has already shown the user the detection, and so sends what it
        /// showed, sends false to have "inherit from console" kept as chosen.
        #[serde(default = "default_true")]
        #[cfg_attr(test, ts(optional = nullable))]
        detect_default_agent: bool,
        /// Absent means no tags.
        #[cfg_attr(test, ts(optional))]
        tags: Option<Vec<String>>,
        /// Absent or blank means no description. Ignored when a parent directory adds more than
        /// one project, as `name` is.
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        description: Option<String>,
        /// The project's own git workflow settings; the object, or an item in it, absent or `null`
        /// follows the application-wide value. A parent directory gives the same values to every
        /// project it adds. A `development` that is none of `in_place`, `branch` or `worktree`, or
        /// a `delivery` that is none of `ask`, `merge` or `pull_request`, is refused.
        #[serde(default)]
        #[cfg_attr(test, ts(optional, type = "Partial<GitWorkflowOverride>"))]
        git_workflow: Option<GitWorkflowOverride>,
    },
    UpdateProject {
        project: String,
        #[cfg_attr(test, ts(optional))]
        name: Option<String>,
        /// Absent leaves the project's default agent alone; an explicit `null` clears it, which is
        /// how the project goes back to following the console's project session agent (else its
        /// `default_agent`). The attribute is what keeps those two apart: serde otherwise reads a
        /// present `null` into the outer `Option` as `None`, exactly as if the field had not been
        /// sent.
        #[serde(default, deserialize_with = "present_option")]
        #[cfg_attr(test, ts(optional))]
        default_agent: Option<Option<Agent>>,
        /// Each agent's account as a [`ProjectAccount`]: an agent left out is left alone; an
        /// explicit `null` puts it back to inheriting the console's reference. An account that
        /// names none of that agent's is refused with `unknown_account`.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable, as = "bindings::PartialPerAgent<Change<ProjectAccount>>"))]
        accounts: PerAgent<Change<ProjectAccount>>,
        /// Each agent's model and effort for the project's sessions: an agent or a field left out
        /// is left alone; an explicit `null` (or a blank string) clears it back to the agent's own
        /// default.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable, as = "bindings::PartialPerAgent<LaunchChange>"))]
        launch: PerAgent<LaunchChange>,
        /// Absent leaves the project's pin alone.
        #[cfg_attr(test, ts(optional))]
        pinned: Option<bool>,
        /// Absent leaves the project's tags alone; a present array replaces them wholesale.
        #[cfg_attr(test, ts(optional))]
        tags: Option<Vec<String>>,
        /// Absent leaves the description alone; an explicit `null` (or a blank string) clears it.
        #[serde(default, deserialize_with = "present_option")]
        #[cfg_attr(test, ts(optional))]
        description: Option<Option<String>>,
        /// Absent leaves the project's git workflow settings alone. Within it, per item: absent
        /// leaves it alone, an explicit `null` goes back to following the application-wide value,
        /// and a value sets the project's own. An unknown `development` or `delivery` is refused.
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        git_workflow: Option<GitWorkflowOverrideChanges>,
    },
    DeleteProject {
        project: String,
        /// Ends the project's running sessions first, as `archive_session` does, instead of
        /// refusing while there are any. Absent is false.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        stop_sessions: bool,
        /// Moves the project's directory to the system Trash once its sessions are stopped,
        /// and removes the project only if that worked. Absent is false.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        trash_directory: bool,
        /// With `trash_directory`, the locked worktrees to take along as well, and the branches
        /// that go, each at the `tip` shown (see `inspect_project_removal`); absent means none.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        force_remove: Vec<SessionBranchTarget>,
        /// With `trash_directory`, the `inspection` token of the `inspect_project_removal`
        /// refusal this removal was confirmed after, which it then repeats; absent means none.
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        inspection: Option<String>,
    },
    ListDir {
        path: String,
    },
    /// Which agent the directory at `path` is set up for, answered with `agent_detected`, or
    /// refused as `add_project` refuses a `local` path that is not an existing directory.
    DetectDirectoryAgent {
        path: String,
    },
    /// Whether the git remote `remote_url` can be read with the user's credentials, and which
    /// agent its top level is set up for, answered with `agent_detected` or, when it cannot be
    /// read, `git_remote_unreachable`.
    ProbeGitRemote {
        remote_url: String,
    },
    /// The models `agent` offers right now, answered with `agent_models`. `account` names the
    /// account whose catalogue to read, by id; absent is the default account. An account that
    /// does not exist or belongs to another agent is refused with `unknown_account`; anything
    /// that goes wrong while asking the agent is not a refusal but an empty answer with a reason.
    ListAgentModels {
        agent: Agent,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        account: Option<String>,
    },
    /// Where the conversation `agent_session_id` (an id the agent wrote, as `open_session`'s
    /// `resume_agent_session_id`) is held, for the new-session dialog of the project `project`,
    /// answered with `conversation_located`. A session that records it comes first, whatever its
    /// agent or project: an archived session of `project` (to reopen), another session of
    /// `project` (running already), or a session outside `project`. Otherwise the on-disk record
    /// is looked for in the default account and every account of each agent that is available
    /// and added, in the directory a launch would use, and a conversation recorded in more than
    /// one is placed in the one whose record was written to last. An id that is not a UUID is
    /// refused with `resume_id_invalid`, a `project` that does not exist with `unknown_project`.
    /// Reads the disk only.
    LocateConversationToResume {
        project: String,
        agent_session_id: String,
    },
    /// The configuration document, answered with `configuration_exported`: the regions picked by
    /// `consoles` (`one`, the console `console`; `all`; or `none`), `include_settings` (the
    /// application settings) and `include_accounts` (the account structure, never a credential).
    /// Absent fields keep their older meaning: `consoles` is `one` when `console` is given and
    /// `all` otherwise, `include_settings` is true exactly when every console is exported, and
    /// `include_accounts` is false. A request that would export nothing is refused with
    /// `export_empty`, `consoles: "one"` without a `console` with `field_required`, and a console
    /// that does not exist with `unknown_console`. `console` is read only for `one`. Reads what is
    /// stored; touches no session, runs no `git`. `include_paths` says whether each project's
    /// absolute directory path is written; absent is true. False writes only the directory's name,
    /// and a remote that cannot be cloned from (a local path) is left out, so the file reveals no
    /// project path of this machine. An account directory is always written, whatever it says.
    ExportConfiguration {
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        console: Option<String>,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        consoles: Option<ExportedConsoles>,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        include_settings: Option<bool>,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        include_accounts: Option<bool>,
        #[serde(default = "default_true")]
        #[cfg_attr(test, ts(optional = nullable))]
        include_paths: bool,
    },
    /// Reads a configuration document and answers with `configuration_inspected`: for each project
    /// which directories it could be placed in on this machine, for each account reference that
    /// matches no account here (it then falls back to the agent's default account; one the
    /// document's accounts would create is listed too, and the client tells them apart from
    /// `accounts`), and, when the document carries accounts, what importing each of them would do.
    /// Changes nothing. `document` is the JSON a client read from a file and is checked here, not
    /// trusted: a `format` that is not a configuration document, a `version` newer than this daemon
    /// reads, or a body that cannot be read is refused with `configuration_not_recognised`,
    /// `configuration_version_unsupported` or `configuration_unreadable`.
    InspectConfiguration {
        #[cfg_attr(test, ts(type = "unknown"))]
        document: serde_json::Value,
    },
    /// Applies a configuration document and answers with `configuration_imported`: creates a new
    /// console for each console in it, and associates each project with the directory its
    /// `resolutions` entry chooses. A project with no entry is left out. The document is refused
    /// as `inspect_configuration` refuses it, and a resolution naming a console or project the
    /// document does not have, naming one twice, or choosing a `directory` that is neither an
    /// absolute path nor starts with `~/`, is refused with `import_resolution_invalid`; all of
    /// that before anything is created.
    ///
    /// With `apply_accounts` the document's accounts are created first, so the consoles' and
    /// projects' account references bind to them, then the consoles and projects, then the
    /// settings. `account_resolutions` give a document account another directory or leave it out.
    ImportConfiguration {
        #[cfg_attr(test, ts(type = "unknown"))]
        document: serde_json::Value,
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        resolutions: Vec<ProjectResolution>,
        /// Whether to apply the document's application settings to this machine; absent means
        /// false. Asking for them of a document that carries none is refused with
        /// `import_settings_missing`.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        apply_settings: bool,
        /// Whether to create the document's accounts on this machine; absent means false. Asking
        /// for them of a document that carries none is refused with `import_accounts_missing`.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        apply_accounts: bool,
        /// The choices for document accounts; one that names an account the document lacks,
        /// names one twice, gives both or neither of `directory` and `leave_out`, or gives a
        /// `directory` that is neither an absolute path nor starts with `~/`, is refused with
        /// `import_resolution_invalid`.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        account_resolutions: Vec<AccountResolution>,
        /// Whether to create the document's consoles and projects; absent means true. Without
        /// them no console or project is created and `resolutions` are not read.
        #[serde(default = "default_true")]
        #[cfg_attr(test, ts(optional = nullable))]
        apply_consoles: bool,
    },
    OpenSession {
        console_id: String,
        #[cfg_attr(test, ts(optional))]
        project_id: Option<String>,
        /// The session's agent; absent takes the project's default, else the console's project
        /// session agent, else its `default_agent`.
        #[cfg_attr(test, ts(optional))]
        agent: Option<Agent>,
        /// The account this session's agent reads, by id. Absent leaves the choice to the
        /// project's reference for that agent, else the console's, else the default account; an
        /// explicit `null` chooses the default account outright, whatever the project or console
        /// refers to. A reference that names no account of the session's agent is refused with
        /// `unknown_account`.
        #[serde(default, deserialize_with = "present_option")]
        #[cfg_attr(test, ts(optional))]
        account: Option<Option<String>>,
        /// Absent inherits the project's (or console's) current setting; null or blank uses the
        /// agent's own default. A string is trimmed and kept for every later launch.
        #[serde(default, deserialize_with = "present_option")]
        #[cfg_attr(test, ts(optional))]
        model: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        #[cfg_attr(test, ts(optional))]
        effort: Option<Option<String>>,
        #[cfg_attr(test, ts(optional))]
        task: Option<String>,
        #[cfg_attr(test, ts(optional))]
        title: Option<String>,
        /// The session this (project) session should report to. Absent means none: a session the
        /// user opens by hand stays outside the orchestration unless they choose one. Ignored for
        /// the console session itself, which is never bound. Must name a console session of
        /// `console_id`, or an unbound project session of `project_id`, or the request is refused
        /// with `unknown_session`.
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        bound_to: Option<String>,
        /// The agent's own id of a conversation started outside Octoboard, to continue in this
        /// session. Trimmed, blank meaning absent. Only a project session takes one, never
        /// together with `bound_to`; it must be a UUID (kept lowercase and hyphenated), found in
        /// the session's account directory and recorded by no session already. Each refusal has a
        /// code of its own (see `apps/daemon/PROTOCOL.md`'s "Coded messages" section) and leaves
        /// no session behind.
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        resume_agent_session_id: Option<String>,
        /// The appearance the asking client's terminal is showing. Absent (a session an agent
        /// opens, or a client that does not send it) leaves Grok following the operating system.
        /// Not recorded on the session.
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        terminal_appearance: Option<TerminalAppearance>,
    },
    ResumeSession {
        session: String,
        /// See `OpenSession::terminal_appearance`.
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        terminal_appearance: Option<TerminalAppearance>,
    },
    ArchiveSession {
        session: String,
        /// Bound branches and worktrees to remove even though git would refuse it (see
        /// `SessionBranchTarget`); absent means none.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        force_remove: Vec<SessionBranchTarget>,
        /// The ones the confirmation listed as refused and the user chose to keep, as they were
        /// listed: one left in that state is not reported. Absent means none.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        keep: Vec<SessionBranchTarget>,
        /// The ids of process trees (see `ProcessTree`) the confirmation listed and the user chose
        /// to keep running: they are made the user's before the processes are ended, and never
        /// stopped by the daemon. Absent means none.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        keep_process_ids: Vec<String>,
    },
    /// Moves a session to another account of its own agent: ends its process, copies its
    /// conversation record into the target account's directory, records the account on the
    /// session and relaunches it as a resume does. `account` is required: an id, or an explicit
    /// `null` for the default account. See `coordinator::account_switch::switch_session_account`.
    SwitchSessionAccount {
        session: String,
        #[serde(default, deserialize_with = "present_option")]
        #[cfg_attr(test, ts(type = "string | null"))]
        account: Option<Option<String>>,
        /// See `OpenSession::terminal_appearance`.
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        terminal_appearance: Option<TerminalAppearance>,
    },
    /// Changes the model and reasoning effort of a session that is not archived, as its own
    /// choice: each absent keeps what the session has, an explicit `null` (or blank) leaves it to
    /// the agent, a string pins it; at least one is given. A running session is restarted on its
    /// account, resuming its conversation with them, as an account switch does without moving it;
    /// an interrupted one only has them recorded for its next resume. Answered with
    /// `session_model_set`. See `coordinator::model_change::set_session_model`.
    SetSessionModel {
        session: String,
        #[serde(default, deserialize_with = "present_option")]
        #[cfg_attr(test, ts(optional))]
        model: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        #[cfg_attr(test, ts(optional))]
        effort: Option<Option<String>>,
        /// See `OpenSession::terminal_appearance`.
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        terminal_appearance: Option<TerminalAppearance>,
    },
    /// Hands a session off to a new session of another agent, which takes its whole place: kind,
    /// project or console, role, owner, the branches it is responsible for and the sessions it
    /// owns. `agent` is required and must differ from the session's own (moving to another account
    /// of the same agent is `switch_session_account`). `account`, `model` and `effort` are as in
    /// `open_session`. The session must exist, not be archived, locked or starting, and not be in
    /// the middle of a turn. Answered with `session_opened` for the receiving session once it is
    /// running and the place has moved. See `crate::coordinator::handoff`.
    HandoffSession {
        session: String,
        agent: Agent,
        #[serde(default, deserialize_with = "present_option")]
        #[cfg_attr(test, ts(optional))]
        account: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        #[cfg_attr(test, ts(optional))]
        model: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        #[cfg_attr(test, ts(optional))]
        effort: Option<Option<String>>,
        /// See `OpenSession::terminal_appearance`. Applied to the receiving session's launch.
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        terminal_appearance: Option<TerminalAppearance>,
    },
    /// Fresh native-record presence for exactly the supplied sessions, aggregated across accounts.
    InspectSessionRecords {
        sessions: Vec<String>,
    },
    /// The user-facing history of one session, read from the agent's own conversation record.
    ReadSessionHistory {
        session: String,
    },
    /// What removing `project` with its folder moved to the Trash would also remove: the linked
    /// worktrees outside the folder (every one when the folder holds the repository, those bound
    /// to the project's sessions otherwise) and, when the repository outlives the folder, the bound
    /// branches. Refused as `delete_project` with `trash_directory` would be. See
    /// `branch_cleanup::project`.
    InspectProjectRemoval {
        project: String,
    },
    /// The git branches and linked worktrees bound to exactly the supplied sessions, as an archive
    /// or a deletion of them would find them now. See `branch_cleanup`.
    InspectSessionBranches {
        sessions: Vec<String>,
        /// Also answer with the process trees of these sessions that an archive would stop and
        /// that have run long enough to ask about (`crate::coordinator::processes::ASK_AFTER`).
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        processes: bool,
    },
    /// The process trees a session's launch started, or those of every running session of a
    /// project followed by the project's processes, the trees no session owns: its leftovers and
    /// the trees kept under it (`kept: true`). See `crate::coordinator::processes`.
    ListProcesses {
        #[serde(flatten)]
        target: ProcessTarget,
    },
    /// How many process trees each project and each console session has: running sessions' trees,
    /// leftovers and kept trees alike. See `crate::coordinator::processes`.
    ListAllProcesses,
    /// Stops the process trees `ids` name that still qualify: a tree of a launch being followed,
    /// a leftover of a local project, or a kept tree, which is the user's no more once stopped.
    /// See `crate::coordinator::processes`.
    StopProcesses {
        ids: Vec<String>,
    },
    /// Makes the process trees `ids` name the user's, with what descends from them: no session
    /// owns them, and the daemon stops them only when `stop_processes` names them or their
    /// project is removed. They are listed among their project's processes with `kept: true`.
    /// See `crate::coordinator::processes`.
    KeepProcesses {
        ids: Vec<String>,
    },
    /// Removes Octoboard's record of one session, archived or never used. A session that is not
    /// archived is deleted only while it has had no turn and nothing is bound to it or from it, and
    /// its process is stopped. Unused sessions' agent records are moved to the Trash automatically;
    /// `trash_agent_records` also moves the records of used sessions.
    DeleteSession {
        session: String,
        /// Requires the atomic unused-session delete, including when the session is archived.
        /// Never falls back to an archived-session cascade.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        only_if_unused: bool,
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        trash_agent_records: bool,
        /// As `ArchiveSession`'s.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        force_remove: Vec<SessionBranchTarget>,
        /// As `ArchiveSession`'s.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        keep: Vec<SessionBranchTarget>,
    },
    /// Removes every archived session of `project`, or every archived session under
    /// `console_session` (bound to it, or to an archived lead session bound to it), or, with
    /// neither, every archived console session of `console`. Naming both is refused as
    /// unreadable.
    DeleteArchivedSessions {
        console: String,
        #[cfg_attr(test, ts(optional))]
        project: Option<String>,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        console_session: Option<String>,
        /// Moves native records to the Trash for every session deleted; defaults to false.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        trash_agent_records: bool,
        /// As `ArchiveSession`'s.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        force_remove: Vec<SessionBranchTarget>,
        /// As `ArchiveSession`'s.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        keep: Vec<SessionBranchTarget>,
    },
    SetSessionPinned {
        session: String,
        pinned: bool,
    },
    SetSessionLocked {
        session: String,
        locked: bool,
    },
    /// Refused for a project session, which carries no colour.
    SetSessionColour {
        session: String,
        colour: ConsoleSessionColour,
    },
    SendMessage {
        session: String,
        text: String,
    },
    /// Stops the turn the session's agent is running, keeping its process and conversation; see
    /// `crate::session::interrupt`.
    InterruptSession {
        session: String,
    },
    RenameSession {
        session: String,
        title: String,
    },
    /// Unbinds a bound project session from its owner: it becomes the user's own, unbound session
    /// (see `crate::coordinator::bindings::release_session`).
    ReleaseSession {
        session: String,
    },
    /// The pages a session pushed. `session` is the session that pushed them, a console session
    /// or a project session.
    ListPages {
        session: String,
    },
    /// What a report panel form was submitted with. The page it came from is named rather than the
    /// console session, because that is what the panel knows and it is also what decides whether the
    /// submission is allowed at all: only its console session's newest page is live. The page also
    /// names the console session the submission is delivered to.
    SubmitPage {
        page: String,
        #[cfg_attr(test, ts(type = "unknown"))]
        data: serde_json::Value,
    },
    /// The user's answer to a `trust_prompt`: Octoboard may press that session's trust
    /// confirmation. `remember` also records the project's consent, so a later confirmation of any
    /// agent in the project is pressed without asking; `trust_parent_dir` records the project's
    /// parent directory as trusted instead, so every project under it is, and when set `remember`
    /// adds nothing. The directory is the daemon's to derive from the session's project, never the
    /// client's to name. Either is recorded only once the confirmation has been pressed.
    ConfirmTrust {
        session: String,
        remember: bool,
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        trust_parent_dir: bool,
    },
    /// Stops trusting a directory. Projects' own consents are left as they are.
    RemoveTrustedDirectory {
        path: String,
    },
    /// The user's answer to a `session_request`. `approve` starts the session the request
    /// describes, unbound; anything else, dismissing the dialog included, refuses it and changes
    /// nothing. The first answer wins: one for a request already answered, timed out or withdrawn
    /// changes nothing and is refused. An approval is answered with `session_opened` for the
    /// session it started, a refusal with an ack. For the request of a `switch` or a `handoff`
    /// the user may change what it asks for in the dialog, and an approval carries the values the
    /// dialog ended on: `agent` (a handoff only), `account` (a switch's target account or a
    /// handoff's receiving account; null for the agent's default account), and `model` and
    /// `effort`, each absent to leave what the request had, null for the agent's own default, or a
    /// value. Every other request ignores them.
    AnswerSessionRequest {
        request_id: String,
        approve: bool,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        agent: Option<Agent>,
        #[serde(default, deserialize_with = "present_option")]
        #[cfg_attr(test, ts(optional))]
        account: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        #[cfg_attr(test, ts(optional))]
        model: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        #[cfg_attr(test, ts(optional))]
        effort: Option<Option<String>>,
    },
    /// Each settable field absent means "leave it alone". Broadcasts `settings_updated` only when
    /// something actually changed, as `remove_trusted_directory` does.
    UpdateSettings {
        #[cfg_attr(test, ts(optional))]
        auto_pull_repositories: Option<bool>,
        #[cfg_attr(test, ts(optional))]
        auto_delete_unresumable_archives: Option<bool>,
        #[cfg_attr(test, ts(optional))]
        auto_approve_session_requests: Option<bool>,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        auto_approve_resume_requests: Option<bool>,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        auto_approve_account_switches: Option<bool>,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        auto_approve_handoffs: Option<bool>,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        ask_questions_as_choices: Option<bool>,
        /// A tag the interface offers (`interface_language::display_name` knows it); any other
        /// is refused.
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        interface_language: Option<String>,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        auto_select_account: Option<bool>,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        auto_switch_on_usage_limit: Option<bool>,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        sidebar_order_by_status: Option<bool>,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        auto_start_quota_windows: Option<bool>,
        /// A blank value goes back to the built-in default.
        #[cfg_attr(test, ts(optional))]
        default_clone_dir: Option<String>,
        /// Absent leaves the git workflow settings alone; within it, each item absent is left
        /// alone.
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        git_workflow: Option<GitWorkflowSettingsChanges>,
        /// Per agent other than Claude Code, by wire name: whether its sessions load Claude Code's
        /// instructions and skills. An agent left out is left alone; Claude Code or an unknown
        /// name is refused.
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        #[cfg_attr(test, ts(type = "Partial<Record<Agent, boolean>>"))]
        claude_compatibility: Option<std::collections::BTreeMap<String, bool>>,
        /// Per agent, by wire name: its tags, replacing the ones it has, normalised as a project's
        /// are (trimmed, empty ones dropped, case-insensitive duplicates dropped keeping the first
        /// spelling); an empty list clears them. An agent left out is left alone; an unknown name
        /// is refused.
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        #[cfg_attr(test, ts(type = "Partial<Record<Agent, Array<string>>>"))]
        agent_tags: Option<std::collections::BTreeMap<String, Vec<String>>>,
    },
    /// Both fields are required: an account always has a name and a directory. Broadcasts
    /// `settings_updated`.
    CreateAccount {
        agent: Agent,
        name: String,
        config_dir: String,
    },
    /// Renames the account, repoints it, or both, independently; either field absent leaves it
    /// alone. Broadcasts `settings_updated`.
    UpdateAccount {
        account: String,
        #[cfg_attr(test, ts(optional))]
        name: Option<String>,
        #[cfg_attr(test, ts(optional))]
        config_dir: Option<String>,
    },
    /// Clears the reference of every console that refers to this account, which puts each one back
    /// on its agent's default account, then removes the account. A session holding it is left
    /// alone — it already carries its own copy of the directory it launches with. Broadcasts
    /// `settings_updated`, and a `console_upserted` for every console whose reference was cleared.
    ///
    /// With `trash_directory`, the account's config directory is moved to the system Trash first
    /// (never deleted outright), and the account removed only if that worked. Judged once before
    /// anything is changed, and again once the account is locked, after its sharing was turned
    /// off, so a refusal from the second judgement leaves sharing turned off. Refused with
    /// `account_has_running_sessions` while a session of the account has a process or is being
    /// launched, resumed or switched, `account_directory_protected` and `account_directory_shared`
    /// for a directory that must stay, and `account_directory_trash_failed` when it cannot be
    /// looked at or the account's sign-in does not end in time; a move that fails keeps the
    /// account. While it runs, a sign-in for the account is refused with `account_being_removed`.
    /// A directory that does not exist is skipped.
    DeleteAccount {
        account: String,
        /// Absent is false: only the record goes.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        trash_directory: bool,
    },
    /// What `delete_account` with `trash_directory` would do with the account's config directory
    /// now, judged as it judges it before changing anything, and answered as
    /// `account_trash_inspected` data rather than a refusal. Running sessions are not counted: a
    /// client lists them itself, and `delete_account` still refuses them. See
    /// `crate::accounts::trash`.
    InspectAccountTrash {
        account: String,
    },
    /// Puts an agent at the end of `Settings.added_agents`. Refused with `agent_already_added` when
    /// it is there, with `agent_not_installed` when it has been determined not installed, and with
    /// `agent_availability_pending` while that is not yet determined. Broadcasts `settings_updated`.
    AddAgent {
        agent: Agent,
    },
    /// Takes an agent off `Settings.added_agents`; its accounts, and the consoles, projects and
    /// sessions that use it, are left as they are. An undecided list is first filled, as for
    /// `add_agent`, and the request is refused with `added_agents_undecided` while it stays
    /// undecided. Answered with `ack` and no `settings_updated` of its own when the agent is not
    /// on the list; otherwise broadcasts `settings_updated`.
    RemoveAgent {
        agent: Agent,
    },
    /// Puts `Settings.added_agents` in the given order, which must name exactly the agents on it,
    /// each once. Refused with `agent_order_invalid` when an agent is missing, repeated or not
    /// added, and with `added_agents_undecided` while the list is undecided (it is first filled, as
    /// for `add_agent`). A request that leaves the order as it is answers with `ack` and no
    /// `settings_updated` of its own; otherwise broadcasts `settings_updated`.
    ReorderAgents {
        agents: Vec<Agent>,
    },
    /// Sets the order of all accounts of the agent, including its default (null), each
    /// exactly once. Refused with `account_order_invalid` for stale or mismatched lists.
    /// Broadcasts `settings_updated` only when the stored order changed.
    ReorderAccounts {
        agent: Agent,
        accounts: Vec<Option<String>>,
    },
    /// Names an agent's default account, or with a blank `name`, or `Default` in any letter case,
    /// clears the name back to the client's own word for it. The name is trimmed and compared like an account's: refused with
    /// `account_name_taken` when another account of the agent has it. Broadcasts `settings_updated`
    /// when the name changed.
    SetDefaultAccountName {
        agent: Agent,
        name: String,
    },
    /// Turns an account off for new work, or back on. A null account names `agent`'s default
    /// account. Broadcasts `settings_updated` when the stored flag changed; sessions already on the
    /// account are not affected.
    SetAccountDisabled {
        agent: Agent,
        #[cfg_attr(test, ts(type = "string | null"))]
        account: Option<String>,
        disabled: bool,
    },
    /// Sets the approval reviewer of every Codex account. A null reviewer follows each account's
    /// configuration. Takes effect on the next launch or resume.
    SetCodexApprovalReviewer {
        #[cfg_attr(test, ts(type = "ApprovalReviewer | null"))]
        reviewer: Option<ApprovalReviewer>,
    },
    /// Sets the approval mode of every Claude Code account. A null mode follows each account's
    /// configuration. Takes effect on the next launch or resume.
    SetClaudeApprovalMode {
        #[cfg_attr(test, ts(type = "ClaudeApprovalMode | null"))]
        mode: Option<ClaudeApprovalMode>,
    },
    /// Sets whether an account shares the agent's default configuration. A null account names
    /// the default account, which is refused. The change is carried out at once and answered when
    /// it ends, or refused: while sessions of the account run or start, or it is being signed in
    /// (`account_in_use`), and while a change of it already runs (`account_sharing_in_progress`).
    SetAccountSharing {
        #[cfg_attr(test, ts(type = "string | null"))]
        account: Option<String>,
        share: bool,
    },
    /// Re-reads the plan quotas of the accounts it selects whose last reading is older than the
    /// polling interval (and not backed off after failures): `agent` limits it to one agent's
    /// accounts, `account` (an account id) to that one account, and with neither it covers every
    /// account of every available agent. An `account` that does not exist or belongs to another
    /// `agent` is refused with `unknown_account`. Answered with `ack` at once; each reading that
    /// finishes follows as `account_quotas_updated`.
    RefreshAccountQuotas {
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        agent: Option<Agent>,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        account: Option<String>,
    },
    /// Signs an account in with its agent's own CLI, run in the background against the account's
    /// config directory (no terminal, no window): `account` is an account id, null for the agent's
    /// default account, whose directory is whatever the agent resolves on its own. Answered with
    /// `ack` once the CLI has started; its progress and outcome follow as `account_login_updated`.
    /// An `account` that does not exist or belongs to another `agent` is refused with
    /// `unknown_account`, an agent determined unavailable with `agent_not_available`, a login shell
    /// whose environment could not be captured with `shell_environment_timeout`, a CLI that does
    /// not resolve with `binary_not_found`, an account that is already being signed in with
    /// `login_already_running`, a second Codex login while one runs for any account with
    /// `login_codex_busy`, and an account whose removal is moving its directory to the Trash with
    /// `account_being_removed`. A success re-reads the account's quota at once.
    StartAccountLogin {
        agent: Agent,
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        account: Option<String>,
    },
    /// Uses one of a Codex account's rate limit reset credits, after the user confirmed it: the
    /// credit of the held reading that expires first (one with no expiry last), or the one the
    /// backend picks when the reading holds no details. `account` is an account id, null for
    /// Codex's default account. Answered with `reset_credit_used` once Codex has answered, and
    /// the account's quota is then read again. A credit is never used on any other path.
    /// Refused with `unknown_account` for an account that does not exist or is not Codex's,
    /// `agent_not_available` for a Codex that is not available, `shell_environment_timeout`,
    /// `binary_not_found`, `reset_credit_not_signed_in` for an account Codex says is signed in to
    /// nothing, `reset_credit_in_progress` while a use of the same account is running,
    /// `daemon_shutting_down`, `reset_credit_timed_out` and `reset_credit_failed`.
    UseCodexResetCredit {
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        account: Option<String>,
    },
    /// Ends the login of an account that is being signed in, and its CLI with it. Answered with
    /// `ack`; the `cancelled` update follows. Refused with `login_not_running` when there is none.
    CancelAccountLogin {
        agent: Agent,
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        account: Option<String>,
    },
    /// Checks every project of this console's current git status against its remote, concurrently.
    /// Answered with `ack` at once; the statuses follow as `project_git_status` broadcasts, one per
    /// project as its check finishes. A project already being checked — by an earlier call this
    /// client raced ahead of, or one from another client — is not checked again.
    RefreshGitStatus {
        console: String,
    },
    /// Checks one project against its remote and fast-forwards it right now, the same work
    /// `refresh_git_status` does for a console, but ignoring the one-minute floor and whether
    /// `auto_pull_repositories` is on. Answered with `ack` at once; the status follows as
    /// `project_git_status` broadcasts. A project already being checked gets one more forced check
    /// once that check ends.
    PullProjectBranch {
        project: String,
    },
    /// Starts a shell terminal in the project's directory, or with `worktree` in the project's
    /// place in that worktree of its repository (the same scoped root as browsing). A non-empty
    /// `path`, a canonical wire path as in `reveal_project_path`, starts it in that folder below
    /// there instead; a path that is not a folder is refused with `path_not_a_directory`. Answered
    /// with `shell_terminal_created`, and broadcast as `shell_terminal_upserted`.
    CreateShellTerminal {
        project: String,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        worktree: Option<String>,
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        path: String,
    },
    /// Renames a shell terminal; from then on the shell's own titles no longer change it. The
    /// title is stored trimmed and cannot be blank.
    RenameShellTerminal {
        shell_terminal: String,
        title: String,
    },
    /// Ends a shell terminal's shell, if it still runs, and removes the terminal; broadcast as
    /// `shell_terminal_deleted`.
    CloseShellTerminal {
        shell_terminal: String,
    },
    /// Opens the system terminal at the project's directory, on the daemon's host. Answered with
    /// `ack` once the terminal has been asked to open, not when its window closes. Refused with
    /// `terminal_unavailable` when the directory is missing, the operating system has none, or the
    /// launch fails.
    OpenProjectTerminal {
        project: String,
    },
    /// Opens the project's directory in the system file manager on the daemon's host. Answered
    /// with `ack` once `open` succeeds on macOS, or once `xdg-open` is spawned on Linux (later
    /// failures are not observed). Refused with `directory_unavailable` if validation or launch fails.
    OpenProjectDirectory {
        project: String,
    },
    /// Reveals an existing live path in the daemon host's file manager: a file is selected in its
    /// folder, a folder is opened (a symbolic link counts as a file, and a folder macOS treats as
    /// a package is selected in its parent, as opening it would launch it).
    /// Uses the same canonical wire path and optional worktree identity as live browsing.
    RevealProjectPath {
        project: String,
        path: String,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        worktree: Option<String>,
    },
    /// Opens an existing live path with its default application on the daemon host.
    OpenProjectPath {
        project: String,
        path: String,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        worktree: Option<String>,
    },
    /// Asks what each of `paths` names on the daemon host: a file, a folder or nothing reachable
    /// (see `crate::path_links`). Answered with `path_links`.
    ProbePathLinks {
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        project: Option<String>,
        paths: Vec<String>,
    },
    /// Opens a path `probe_path_links` found with its default application on the daemon host; a
    /// folder opens in the file manager.
    OpenPathLink {
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        project: Option<String>,
        path: String,
    },
    /// Pushes the project's current branch, and only that branch, never with `--force`. Answered
    /// with `ack` once `git push` has exited; a success is followed by `project_git_status` from a
    /// local re-read, with no fetch. Refused with `not_a_git_repository`, `not_on_a_branch`,
    /// `no_commits_to_push`, `no_push_remote`, `upstream_not_a_remote`, `push_in_progress` or
    /// `git_push_failed`.
    PushProjectBranch {
        project: String,
    },
    /// Switches to an existing local branch in canonical wire form. A success publishes a local
    /// status re-read, without fetching or fast-forwarding. Answered after Git exits.
    SwitchProjectBranch {
        project: String,
        branch: String,
    },
    /// Discards this project's staged and unstaged tracked changes against HEAD, including
    /// staged additions. Untracked files, ignored files and submodules are preserved. Requires
    /// client confirmation. Answered with `ack` after Git exits; commits and branch stay put.
    RestoreProjectChanges {
        project: String,
    },
    Shutdown,
}

/// What a `list_processes` asks about: one session, or one project.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ProcessTarget {
    /// The trees of the session's current launch.
    Session { session_id: String },
    /// The trees of every running session of the project, then the project's processes: its
    /// leftovers and the trees kept under it.
    Project { project_id: String },
}

#[cfg(test)]
mod tests;
