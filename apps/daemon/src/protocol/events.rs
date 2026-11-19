//! What the daemon sends on `/ws/control`: every reply and broadcast, a session's notice, and what
//! approving or ending a session request does. See "Daemon to client" in
//! `apps/daemon/PROTOCOL.md`.

use std::collections::BTreeMap;

use serde::Serialize;

use super::accounts::{AccountLogin, AccountSharing, AccountTrashOutcome};
use super::agent_cli::AgentCliStatus;
use super::agents::AgentId;
use super::attachments::AttachmentLimits;
use super::browse::{
    BranchInfo, BrowseEntry, ChangeEntry, ChangeGroup, CommitChanges, ComparisonEndpoint,
    ContentMatch, ContentSearchStop, ContentSource, FileContent, ProjectSourceInfo, SideRead,
    UpstreamInfo,
};
use super::codes::Code;
use super::configuration::{
    ConfigurationDocument, DocumentSettings, ImportFailure, ImportItem, ImportItemState,
    ImportedAccounts, ImportedConsole, InspectedAccounts, InspectedConsole, InspectedSettings,
    SettingsSection,
};
use super::conversation::{ConversationEntry, PendingEntry};
use super::daemon_update::DaemonUpdateStatus;
use super::errors::{params, Params};
use super::fields::is_true;
use super::history::{ConversationLocation, HistoryMessage, SessionRecordPresence};
use super::host::HostInfo;
use super::mcp_inventory::{AccountMcpInventory, McpServerAuthOutcome};
use super::memory::{MemoryEntry, MemoryIndexEntry, MemoryScope, OrphanedMemory};
use super::models::{AgentModel, DefaultModelSource, ModelChangeOutcome, ModelsUnavailable};
use super::page_content::Submission;
#[cfg(test)]
use super::page_content::SubmittedValue;
use super::path_links::PathLink;
use super::processes::{ProcessCount, ProcessTree, UnstoppedProcess};
use super::providers::ProviderInfo;
use super::quotas::{AccountQuota, ResetCreditOutcome};
use super::records::{
    AgentAvailability, Console, DirEntry, GitStatus, Host, Page, Project, PullOutcome, Session,
    SessionKind, SessionStatus, ShellTerminal,
};
use super::remote_access::RemoteAccess;
use super::session_branches::SessionBranchItem;
use super::settings::Settings;

#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// The first frame on every control connection, before the `snapshot`. Its shape is frozen
    /// across generations so that a client of any generation can read it: fields may be added,
    /// never removed or given another meaning.
    Hello {
        generation: u32,
        daemon_version: String,
    },
    /// The answer to `interrupt_session`: whether a running turn was stopped, the session's status
    /// once the interrupt had settled, and what the caller is to know, in prose.
    SessionInterrupted {
        id: Option<String>,
        session: String,
        interrupted: bool,
        status: SessionStatus,
        note: String,
    },
    /// The answer to `set_session_model`: what the change did to the session, and for a restart
    /// whether the relaunch was sent the prompt to carry on with its task.
    SessionModelSet {
        id: Option<String>,
        session: String,
        outcome: ModelChangeOutcome,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        continued: Option<bool>,
    },
    SessionHistory {
        id: Option<String>,
        session: String,
        messages: Vec<HistoryMessage>,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        #[cfg_attr(test, ts(optional = nullable))]
        unavailable: bool,
    },
    /// The answer to `subscribe_session_conversation`: the latest page of the conversation, and the
    /// entries the agent is still producing.
    SessionConversationSubscribed {
        id: Option<String>,
        session: String,
        epoch: u64,
        entries: Vec<ConversationEntry>,
        has_earlier: bool,
        pending: Vec<PendingEntry>,
    },
    /// The answer to `read_session_conversation`: the page before the one the client holds.
    SessionConversation {
        id: Option<String>,
        session: String,
        epoch: u64,
        entries: Vec<ConversationEntry>,
        has_earlier: bool,
    },
    /// What changed in a conversation the connection follows. Sent only to the connections that
    /// subscribed to the session, never broadcast.
    SessionConversationChanged {
        session: String,
        epoch: u64,
        /// Entries added after the last one the client holds, in order.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        #[cfg_attr(test, ts(optional = nullable))]
        appended: Vec<ConversationEntry>,
        /// Entries the client holds that changed (a tool call's result arrived), each whole and
        /// named by its `seq`.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        #[cfg_attr(test, ts(optional = nullable))]
        updated: Vec<ConversationEntry>,
        /// The entries the agent is still producing, whole; absent when they did not change, and
        /// empty when there are none now.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        pending: Option<Vec<PendingEntry>>,
    },
    /// The conversation as the connection follows it was rewritten (the agent went back to an
    /// earlier point, compacted, or the record was replaced): the client drops what it holds of
    /// the session and takes this as the latest page. Sent as `session_conversation_changed` is.
    SessionConversationReplaced {
        session: String,
        epoch: u64,
        entries: Vec<ConversationEntry>,
        has_earlier: bool,
        pending: Vec<PendingEntry>,
    },
    /// The daemon stopped following a conversation this connection follows: the session was
    /// deleted, or the daemon failed while reading it. A read error is no end: it is tried again
    /// at the next look, or, after a first read that failed part-way, once the file has changed.
    /// The subscription is gone, nothing more will be sent for the session, and the client may
    /// subscribe again. Sent as `session_conversation_changed` is, but never for the connection's
    /// own `unsubscribe_session_conversation`.
    SessionConversationEnded {
        session: String,
    },
    SessionRecordsInspected {
        id: Option<String>,
        presence: SessionRecordPresence,
    },
    SessionBranchesInspected {
        id: Option<String>,
        items: Vec<SessionBranchItem>,
        /// Only for an `inspect_session_branches` that asked for `processes`.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        #[cfg_attr(test, ts(optional = nullable))]
        processes: Vec<ProcessTree>,
    },
    ProcessesListed {
        id: Option<String>,
        trees: Vec<ProcessTree>,
    },
    AllProcessesListed {
        id: Option<String>,
        counts: Vec<ProcessCount>,
    },
    ProcessesStopped {
        id: Option<String>,
        ids: Vec<String>,
        /// The processes the stop left running, each with the tree it was stopped with, which
        /// `ids` leaves out.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        #[cfg_attr(test, ts(optional = nullable))]
        not_stopped: Vec<UnstoppedProcess>,
    },
    ProcessesKept {
        id: Option<String>,
        ids: Vec<String>,
    },
    AccountTrashInspected {
        id: Option<String>,
        outcome: AccountTrashOutcome,
    },
    ProjectRemovalInspected {
        id: Option<String>,
        items: Vec<SessionBranchItem>,
        /// The project's processes, the trees no session owns (left behind in it, or kept under
        /// it), which its removal stops, whether or not the folder goes.
        processes: Vec<ProcessTree>,
        /// The token the worktrees it held back are kept under, for `delete_project` to send back
        /// (`crate::branch_cleanup::project`); absent when it held none back.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        inspection: Option<String>,
    },
    Snapshot {
        hosts: Vec<Host>,
        consoles: Vec<Console>,
        projects: Vec<Project>,
        sessions: Vec<Session>,
        trusted_directories: Vec<String>,
        settings: Settings,
        /// Every registered provider's descriptor data, in the registry's order.
        providers: Vec<ProviderInfo>,
        /// Every git status the daemon currently holds; empty on a fresh start. Carried here, like
        /// the trusted directories, so a reconnecting client never has to ask for it separately.
        git_statuses: Vec<GitStatus>,
        /// One entry per registered provider, each starting `not_determined` and
        /// replaced once the daemon's one-time login-shell snapshot lands; see
        /// `crate::agents::availability`.
        agent_availability: Vec<AgentAvailability>,
        /// The plan quota last read for each account of every available agent, the default
        /// account included; empty until the first readings land. Runtime state, not stored.
        account_quotas: Vec<AccountQuota>,
        /// The logins that have started and not ended, whose later updates a reconnecting client
        /// would otherwise have missed; empty when none is running. Runtime state, not stored.
        account_logins: Vec<AccountLogin>,
        /// The sharing state of every account the user owns, as `account_sharing` sends it.
        /// Runtime state, not stored.
        account_sharing: Vec<AccountSharing>,
        /// The installed CLI of every agent checked since the daemon started
        /// (`check_agent_clis`), the update of one that runs or ran included; empty until the
        /// first check ends. Runtime state, not stored.
        agent_cli_statuses: Vec<AgentCliStatus>,
        /// The daemon's own update as it stands (`check_daemon_update`, `update_daemon`); null
        /// where this daemon does not update itself. Runtime state, not stored.
        daemon_update: Option<Box<DaemonUpdateStatus>>,
        /// The daemon host's home directory, so a client can show a path under it as `~/...`; it
        /// is the daemon's home, not the browser's, since the two may be on different machines.
        /// Null when the daemon cannot determine one.
        home_dir: Option<String>,
        /// The daemon host's own facts: its operating system, how it writes a path, whether it
        /// can make file links, and how its interface names the Trash (`trash`) and its settings
        /// app (`settings`).
        host: HostInfo,
        /// Whether the daemon is holding its host awake right now (Prevent sleep, see
        /// `crate::keep_awake`): the setting is on, its `keep_awake_when` is met and the host
        /// granted the hold. Runtime state, not stored.
        keep_awake_held: bool,
        /// Every shell terminal the daemon holds, running or exited, oldest first. Runtime state,
        /// not stored.
        shell_terminals: Vec<ShellTerminal>,
        /// Remote access to this daemon (Allow remote access), where the daemon follows the
        /// desktop application on a host with a way in for the browser (the owner socket on Linux
        /// and macOS, a loopback port of its own on Windows); null elsewhere: a daemon that runs
        /// on its own (the server form) serves its browser as it always does.
        remote_access: Option<RemoteAccess>,
        /// The limits of a file a client attaches to a session (`create_attachment_upload`), sent
        /// by every daemon that takes attachments; absent from one that predates them, and a client
        /// then offers no way to attach a file.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        attachments: Option<AttachmentLimits>,
    },
    /// Availability or a default account's resolved directory changed for one or more agents —
    /// the whole three-entry list, like every other upsert. Broadcast when the daemon's one-time
    /// determination lands, and again when an update of an agent's CLI has ended and its entry is
    /// not what it was; nothing else re-determines it during a run.
    AgentAvailabilityUpdated {
        agent_availability: Vec<AgentAvailability>,
    },
    /// A plan quota reading finished, or an account's was dropped: the whole list, like every other
    /// upsert.
    AccountQuotasUpdated {
        account_quotas: Vec<AccountQuota>,
    },
    /// An account's login started, moved on or ended: its whole state, like every other upsert. The
    /// ones that have not ended are also replayed in a `snapshot`; an ended one is only announced
    /// here.
    AccountLoginUpdated {
        login: AccountLogin,
    },
    /// A check of an agent's installed CLI ended, or an update of it started, moved on or ended:
    /// the agent's whole status, like every other upsert. Every agent's last one is also in a
    /// `snapshot`.
    AgentCliStatusUpdated {
        status: AgentCliStatus,
    },
    /// Remote access changed: the setting was turned on or off, the browser port was changed, the
    /// owner key was rotated, or the way in failed to open. The whole state, like every other
    /// upsert; it is also in a `snapshot`. Never sent by a daemon whose snapshot carries no
    /// `remote_access`.
    RemoteAccessUpdated {
        remote_access: RemoteAccess,
    },
    /// The daemon's own update moved on: a check started or ended, a download made progress,
    /// ended or failed, or the executable was replaced. The whole status, like every other upsert;
    /// it is also in a `snapshot`. Never sent by a daemon that does not update itself.
    DaemonUpdateStatusUpdated {
        status: DaemonUpdateStatus,
    },
    /// What stands in the way of a change of an account's sharing, the change running and the last
    /// one that failed: the account's whole sharing state, sent whenever any of it changes. Every
    /// owned account's is also in a `snapshot`.
    AccountSharing(AccountSharing),
    /// The directories whose projects Octoboard presses every agent's trust confirmation for
    /// changed: the whole list, like every other upsert.
    TrustedDirectoriesUpdated {
        trusted_directories: Vec<String>,
    },
    /// The user settings changed, whole: the single setting today, and whatever is added to
    /// `Settings` later.
    SettingsUpdated {
        settings: Settings,
    },
    /// Whether the daemon is holding its host awake changed (`keep_awake_held` of the `snapshot`).
    /// It follows the setting, the sessions' work and what the host allows, so it is sent on its
    /// own rather than with the settings.
    KeepAwakeHeldUpdated {
        keep_awake_held: bool,
    },
    ConsoleUpserted {
        console: Box<Console>,
    },
    ConsoleDeleted {
        console: String,
    },
    ProjectUpserted {
        project: Project,
    },
    ProjectDeleted {
        project: String,
    },
    SessionUpserted {
        session: Session,
    },
    SessionDeleted {
        session: String,
    },
    /// A shell terminal was created or changed: its title, its state, whether it is busy. The whole
    /// record, like every other upsert.
    ShellTerminalUpserted {
        shell_terminal: ShellTerminal,
    },
    /// A shell terminal was closed, or ended with its project or the daemon.
    ShellTerminalDeleted {
        shell_terminal: String,
    },
    /// The reply to `create_shell_terminal`, naming the terminal it started. The same record is
    /// broadcast as `shell_terminal_upserted`, which carries no request id.
    ShellTerminalCreated {
        id: Option<String>,
        shell_terminal: ShellTerminal,
    },
    /// The reply to `refresh_project_git_status`: the status the check that answers it ended with,
    /// as also broadcast in `project_git_status`, or `None` when that check was abandoned before
    /// it ended (its project removed before it ran or reran, a panic) or had not ended when the
    /// daemon stopped waiting.
    ProjectGitRechecked {
        id: Option<String>,
        status: Option<GitStatus>,
    },
    /// The reply to `pull_project_branch`: what the pull did. The statuses it went through are
    /// broadcast as `project_git_status`; a pull that could not be done is an `error` reply
    /// instead.
    ProjectBranchPulled {
        id: Option<String>,
        outcome: PullOutcome,
    },
    /// A project's `GitStatus` changed, including every transition of `activity` — the animated
    /// icon has something to follow. Nothing is sent when a project is removed; the client drops
    /// its status along with it.
    ProjectGitStatus {
        status: GitStatus,
    },
    /// Something about a session the user has to be told, which no status field carries: an
    /// injected capability that will not apply, a setting of theirs Octoboard had to work around.
    /// Broadcast once, when it is found; nothing stores it. `code` and `params` name the notice for
    /// a client to word in its own language; `message` is the English text for a client that does
    /// not know the code.
    SessionNotice {
        session: String,
        code: Code,
        params: Params,
        message: String,
    },
    /// The `session_notice` with `code` about `session` that was broadcast last no longer holds: a
    /// `message_not_sent` whose message the agent has since reported sending. A client still
    /// showing that notice drops it; one that is not ignores this.
    SessionNoticeWithdrawn {
        session: String,
        code: Code,
    },
    /// A session of a project the user has not yet agreed Octoboard may press trust confirmations
    /// for — not by its own consent and not through a trusted directory — is sitting at its
    /// agent's folder-trust confirmation; `agent` is the one asking. Broadcast once per
    /// confirmation, and sent again to a client after each `snapshot` while it waits. `trust_dir`
    /// is the directory `confirm_trust` with `trust_parent_dir` would trust, and null when there
    /// is none to offer: it would be the filesystem root, the home directory or one containing it,
    /// the home directory cannot be determined, or the project's path is not absolute.
    TrustPrompt {
        session: String,
        agent: AgentId,
        project: String,
        path: String,
        trust_dir: Option<String>,
    },
    /// A session asked with `request_session` for a session that has no owner, or with
    /// `request_resume_session` to resume a conversation, or an owner called `switch_session_account`
    /// or `handoff_session` while the user's approval is required, and its call is waiting for the
    /// user's answer. Broadcast once, when the call is made, and sent again to a client after each
    /// `snapshot`, oldest first, for every request still waiting.
    SessionRequest {
        request_id: String,
        /// The requesting session.
        session: String,
        /// What an approval does.
        action: SessionRequestAction,
        /// Whether the session asked for would be a console session or a project session; for
        /// `switch` and `handoff`, which the target is.
        kind: SessionKind,
        /// The console the new session would be in, or the console of the session to bring back,
        /// switch or hand off.
        console: String,
        /// The project the new session would be in, or the project of the session to bring back,
        /// switch or hand off; null for a console session.
        project: Option<String>,
        /// The agent the new session would run, or the agent of the session to bring back or
        /// switch; for `handoff`, the receiving agent.
        agent: AgentId,
        /// For `reopen`, `resume`, `switch` and `handoff`: the session an approval brings back,
        /// switches or hands off. Null otherwise.
        target: Option<String>,
        /// For `reopen` and `resume`: the session the target reports to, which is where an approval
        /// leaves it; null when it has no owner. Null otherwise.
        owner: Option<String>,
        /// For `reopen` and `resume`: the archived owners an approval brings back together with
        /// the target, topmost first, as they stood when the request was made. Empty otherwise.
        reopened_owners: Vec<String>,
        /// For `resume_record`: the account that holds the conversation's record; for `switch`,
        /// the account the call asked to move the session to; for `handoff`, the account the
        /// receiving session would run on, settled when the call was made. Null for the agent's
        /// default account, and for every other request.
        account: Option<String>,
        /// For `resume_record`: the conversation's id as the agent writes it. Null otherwise.
        conversation: Option<String>,
        /// The branches an approval would hand to the new session, as wire paths below
        /// `refs/heads/`: ones the requesting session is responsible for, checked when it asked.
        /// Empty when the request transfers none.
        branches: Vec<String>,
        /// For `handoff`: the model and effort the call asked for, as `Session::model` states an
        /// override: absent when the call left it out, null for the agent's own default.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional), schemars(transform = super::schema::nullable))]
        model: Option<Option<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional), schemars(transform = super::schema::nullable))]
        effort: Option<Option<String>>,
        /// For `handoff`: the compatibility switches the call asked the receiving session to hold,
        /// by setting id; empty for every other request and where it asked for none.
        #[serde(default)]
        compatibility: BTreeMap<String, bool>,
        /// Milliseconds since the epoch, which is what orders several requests oldest first.
        requested_at: i64,
    },
    /// A `session_request` is no longer waiting: every client closes its dialog.
    SessionRequestClosed {
        request_id: String,
        reason: SessionRequestEnding,
    },
    /// The reply to `open_session`, and to an approving `answer_session_request`: the session that
    /// was opened. For `open_session` it is `starting`, the launch still going on; for an approving
    /// answer the launch has settled and it is running. The same record is broadcast as
    /// `session_upserted` as well, but a broadcast carries no request id, so this is the only way
    /// the client that asked can tell which of the sessions appearing in the tree is the one it
    /// just opened — and therefore the only way it can select it.
    SessionOpened {
        id: Option<String>,
        session: Session,
    },
    DirListing {
        id: Option<String>,
        path: String,
        entries: Vec<DirEntry>,
    },
    /// The reply to `detect_directory_agent` and `probe_git_remote`: the one agent the directory
    /// or remote is set up for, or `null` for none, several, or one known not to be installed.
    AgentDetected {
        id: Option<String>,
        agent: Option<AgentId>,
        /// Whether the local directory exists; absent for a remote repository probe.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        directory_exists: Option<bool>,
    },
    /// The reply to `list_agent_models`: `agent` and `account` echo the request. `models` is
    /// empty exactly when `unavailable` says why; a client then offers a free-text field or the
    /// agent's own default rather than an error. `default_model` is what a session of the account
    /// gets when no setting names a model (for Claude Code the model its own configuration names,
    /// else the best one the account may use), absent where the agent's own default applies.
    AgentModels {
        id: Option<String>,
        agent: AgentId,
        account: Option<String>,
        models: Vec<AgentModel>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        default_model: Option<String>,
        /// Where `default_model` came from; absent with it.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        default_model_source: Option<DefaultModelSource>,
        unavailable: Option<ModelsUnavailable>,
    },
    /// The reply to `locate_conversation_to_resume`: where the conversation is held.
    ConversationLocated {
        id: Option<String>,
        location: ConversationLocation,
    },
    /// The reply to `use_reset_credit`: what the provider did with the request.
    ResetCreditUsed {
        id: Option<String>,
        outcome: ResetCreditOutcome,
    },
    /// The reply to `read_account_mcp_inventory`: what the account's agent reports.
    AccountMcpInventoryRead {
        id: Option<String>,
        inventory: AccountMcpInventory,
    },
    /// The reply to `sign_in_account_mcp_server` and `sign_out_account_mcp_server`, sent once the
    /// agent's CLI has ended.
    AccountMcpServerAuthEnded {
        id: Option<String>,
        outcome: McpServerAuthOutcome,
    },
    /// The reply to `export_configuration`: the configuration document, which a client saves as
    /// the file the user keeps (see "Configuration export" in `apps/daemon/PROTOCOL.md`).
    ConfigurationExported {
        id: Option<String>,
        document: ConfigurationDocument,
    },
    /// The reply to `inspect_configuration` (see "Configuration import" in
    /// `apps/daemon/PROTOCOL.md`): the settings and accounts the document carries,
    /// and one entry per console of the document, in its order.
    ConfigurationInspected {
        id: Option<String>,
        settings: Option<InspectedSettings>,
        /// The sections of Settings the document's settings carry, in Settings order; empty when
        /// it carries no settings.
        settings_sections: Vec<SettingsSection>,
        accounts: Option<InspectedAccounts>,
        consoles: Vec<InspectedConsole>,
        /// The version of every settings field as it was read before the current values the
        /// reply shows, for `import_configuration`'s `settings_versions`.
        #[cfg_attr(test, ts(type = "Record<string, number>"))]
        settings_versions: BTreeMap<String, u64>,
    },
    /// The reply to `import_configuration`: what came of the settings and accounts asked for, and
    /// one entry per console of the document, in its order, each naming the console created for
    /// it; no entry at all with `apply_consoles` false.
    ConfigurationImported {
        id: Option<String>,
        settings_applied: Option<DocumentSettings>,
        settings_error: Option<ImportFailure>,
        /// The default clone directory written, which stays written when the settings as a whole
        /// could not be applied (`settings_error`).
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        clone_dir_applied: Option<String>,
        accounts: Option<ImportedAccounts>,
        consoles: Vec<ImportedConsole>,
    },
    /// Where one item of a running `import_configuration` stands, broadcast to every client; a
    /// client keeps the events whose `import_id` is the one it sent. Every item the import will
    /// handle is announced `waiting` first, in the order they are handled, then each moves on to
    /// `creating` (or `cloning`) and ends `done`, `failed` or `not_attempted`; an item left out is
    /// never announced. They are all broadcast before the `configuration_imported` reply is sent
    /// but may reach a client after it, so a client that has the reply ignores later progress for
    /// that `import_id`.
    ConfigurationImportProgress {
        import_id: String,
        item: ImportItem,
        #[serde(flatten)]
        #[cfg_attr(test, schemars(with = "super::schema::Flattened<ImportItemState>"))]
        state: ImportItemState,
    },
    /// The reply to `list_pages`, oldest first. Pages are not in `snapshot`: a page carries its whole
    /// component tree, and only the session the user is looking at needs its pages, so the
    /// panel asks for them instead — and asks again after every `snapshot`, which is what keeps it
    /// correct across a `page_created` the client was too far behind to receive. A
    /// lagging client is sent a fresh snapshot in place of the events it missed, on the socket it
    /// already has, so nothing else tells it that its list is now short.
    PageList {
        id: Option<String>,
        session_id: String,
        pages: Vec<Page>,
    },
    /// A page a session just pushed. The panel showing that session refreshes to it.
    PageCreated {
        page: Page,
    },
    /// A page's form was submitted, or the submission was taken back. `submission` is what it was
    /// submitted with, sent when the daemon accepts it for delivery to the session, before the
    /// message is written; `null` when the delivery certainly failed and the page can be submitted
    /// again. `submission_revision` is the page's revision of it (`Page::submission_revision`):
    /// announcements of one page can arrive out of order, and a client keeps whichever of this and
    /// what it holds has the higher revision, including for a page whose `page_list` it is still
    /// waiting for.
    PageSubmission {
        session_id: String,
        page: String,
        #[cfg_attr(
            test,
            ts(as = "Option<BTreeMap<String, SubmittedValue>>"),
            schemars(with = "Option<BTreeMap<String, SubmittedValue>>")
        )]
        submission: Option<Submission>,
        submission_revision: i64,
    },
    /// Pages a session withdrew with `delete_pages`, by id. `session_id` names the session that
    /// pushed them. The panel showing that session drops them.
    PagesDeleted {
        session_id: String,
        pages: Vec<String>,
    },
    /// The reply to `list_memories`: the index of a memory, the most recently updated first. A
    /// client that keeps it asks again after every `snapshot`, which is what keeps it correct
    /// across a `memory_saved` or `memories_deleted` it was too far behind to receive.
    MemoryList {
        id: Option<String>,
        scope: MemoryScope,
        memories: Vec<MemoryIndexEntry>,
    },
    /// The reply to `read_memory`: one entry, content included.
    MemoryContent {
        id: Option<String>,
        entry: MemoryEntry,
    },
    /// An entry of a memory was created or updated, by a session's `save_memory` tool or by a
    /// client's `save_memory`. Broadcast in the order the writes were made. Its `scope` names the
    /// memory: a place's reaches the windows showing any project at that place.
    MemorySaved {
        entry: MemoryEntry,
    },
    /// Entries of a memory were deleted, by id: by a session's `delete_memory` tool, or by a
    /// client's `delete_memory`, `delete_memories`, `delete_orphaned_memory`,
    /// `delete_orphaned_memories` or a `delete_project` that deleted its place's memory. Never
    /// empty.
    MemoriesDeleted {
        scope: MemoryScope,
        /// The ids of the entries deleted.
        memories: Vec<String>,
    },
    /// The places whose memory no project is at any more, the most recently updated first: the
    /// reply to `list_orphaned_memories`, and broadcast (with no `id`) whenever that may have changed: a project added or removed, a
    /// place's memory deleted.
    OrphanedMemoryList {
        id: Option<String>,
        places: Vec<OrphanedMemory>,
    },
    /// The reply to `save_pasted_image`: the absolute path, on the daemon host, of the file the image
    /// was saved as.
    PastedImageSaved {
        id: Option<String>,
        path: String,
    },
    /// The reply to `create_pasted_image_upload`: `upload` is the handle its parts and its
    /// `save_pasted_image` name, valid on this connection only.
    PastedImageUploadCreated {
        id: Option<String>,
        upload: String,
    },
    /// The reply to `save_attachment`: the absolute path, on the daemon host, of the file the
    /// attachment was saved as.
    AttachmentSaved {
        id: Option<String>,
        path: String,
    },
    /// The reply to `create_attachment_upload`: `upload` is the handle its parts and its
    /// `save_attachment` name, valid on this connection only.
    AttachmentUploadCreated {
        id: Option<String>,
        upload: String,
    },
    /// The reply to `probe_native_apps`: the agents whose desktop application is installed on the
    /// daemon's host.
    NativeApps {
        id: Option<String>,
        agents: Vec<AgentId>,
        /// The asked `sessions` whose conversation the applications can see.
        sessions_seen: Vec<String>,
        /// The asked `sessions` judged not to be seen, which the application may not show, though
        /// it is still opened. One asked about and in neither list was not judged.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        #[cfg_attr(test, ts(optional = nullable))]
        sessions_unseen: Vec<String>,
    },
    /// The reply to `probe_path_links`: one entry per asked path, in the order asked.
    PathLinks {
        id: Option<String>,
        links: Vec<PathLink>,
    },
    /// The reply to `get_project_source`.
    ProjectSource {
        id: Option<String>,
        source: ProjectSourceInfo,
    },
    /// The reply to `list_project_dir`. `complete` is false when the listing was cut at a budget.
    ProjectDir {
        id: Option<String>,
        project: String,
        worktree: Option<String>,
        path: String,
        /// The identity of the directory the listing was scoped to.
        root_id: String,
        entries: Vec<BrowseEntry>,
        complete: bool,
    },
    /// Recursive filename matches, possibly cut at a traversal or reply budget.
    ProjectFileMatches {
        id: Option<String>,
        project: String,
        query: String,
        paths: Vec<String>,
        /// The `paths` Git ignores, sent only when there are any; see `browse::ignored`.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        #[cfg_attr(test, ts(optional = nullable))]
        ignored: Vec<String>,
        complete: bool,
    },
    /// A batch of a content search's matches, sent as the search finds them, after the `ack` to
    /// `search_project_content`; `search` and `project` echo the request.
    ProjectContentMatches {
        search: String,
        project: String,
        matches: Vec<ContentMatch>,
    },
    /// The last event of a content search: how many matches (`matches`, counting every match in a
    /// line) and files with a match (`files`) it found, and whether it searched every file it was
    /// to (`complete`). `stopped_by` names the bound that ended it early; an incomplete search
    /// without one could not read some file or folder.
    ProjectContentSearchEnded {
        search: String,
        project: String,
        matches: u64,
        files: u64,
        complete: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        stopped_by: Option<ContentSearchStop>,
    },
    /// The reply to `read_project_file`; `project`, `worktree` and `path` echo the request.
    ProjectFile {
        id: Option<String>,
        project: String,
        worktree: Option<String>,
        path: String,
        source: ContentSource,
        file: FileContent,
    },
    /// The reply to `read_path_link`; `path` echoes the request and `version` is the live file's
    /// version at the read.
    PathLinkFile {
        id: Option<String>,
        path: String,
        version: String,
        file: FileContent,
    },
    /// The reply to `read_file_range`: `data` is the base64 of the bytes read from `offset`, empty
    /// at or past the end of the file; `size` and `version` are the file's at this read.
    /// `launchable` is set only when true: the file has an executable bit. What a name says the
    /// system would run is for the client to judge by the system that opens the copy.
    FileRange {
        id: Option<String>,
        offset: u64,
        size: u64,
        version: String,
        data: String,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        #[cfg_attr(test, ts(optional = nullable))]
        launchable: bool,
    },
    /// One part of a browse reply delivered in parts: the reply to a browse request that carried
    /// `part_bytes` and whose reply is larger, and to each `read_reply_part`. `reply` is the handle
    /// of the reply the daemon holds, `total` the length in bytes of its serialized form, and `data`
    /// the base64 of its bytes from `offset`. Joined in order, the parts are the reply as it would
    /// have been sent whole, the id of the request that asked for it included.
    ReplyPart {
        id: Option<String>,
        reply: String,
        offset: u64,
        total: u64,
        data: String,
    },
    /// The reply to `list_project_changes`. `head` is the commit the staged changes are against,
    /// `null` before the first commit; `complete` is false when the list was cut at a budget.
    ProjectChanges {
        id: Option<String>,
        project: String,
        worktree: Option<String>,
        head: Option<String>,
        changes: Vec<ChangeEntry>,
        complete: bool,
    },
    /// The reply to `read_project_change`. `head` is the commit a staged change was read against
    /// (`null` before the first commit, and for the other groups); `patch` is the change's unified
    /// patch as `git` writes it, `null` when the reply makes none.
    ProjectChange {
        id: Option<String>,
        project: String,
        worktree: Option<String>,
        group: ChangeGroup,
        head: Option<String>,
        // Boxed: two sides with their bodies would make every `Event` this large.
        old: Box<SideRead>,
        new: Box<SideRead>,
        patch: Option<FileContent>,
    },
    /// The reply to `list_project_branches`: the repository's local branches in byte order of
    /// their names; `complete` is false when the list was cut at a budget.
    ProjectBranches {
        id: Option<String>,
        project: String,
        branches: Vec<BranchInfo>,
        complete: bool,
        /// Absent when true; false when the upstream lookup was past a budget, so that the main
        /// working copy's branch may lack its `upstream` or `default_branch` though it has one.
        #[serde(skip_serializing_if = "is_true")]
        #[cfg_attr(test, ts(optional = nullable))]
        upstreams_complete: bool,
        /// The remote branch the repository's remote default branch points at, when the main
        /// working copy's branch has no `upstream`; absent when there is none to give.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        default_branch: Option<UpstreamInfo>,
    },
    /// The reply to `compare_project_branches`: the two branches with the commits they were
    /// resolved to, the commit they forked at (`base`, their merge base), and the changes from
    /// that commit to the right branch's that touch the project, each in the `committed` group;
    /// `complete` is false when the list was cut at a budget.
    ProjectComparison {
        id: Option<String>,
        project: String,
        left: ComparisonEndpoint,
        right: ComparisonEndpoint,
        base: String,
        changes: Vec<ChangeEntry>,
        complete: bool,
    },
    /// The reply to `read_project_comparison_change`: `left`, `right` and `base` echo the request,
    /// and both sides and the patch were read from exactly the base and the right commit.
    ProjectComparisonChange {
        id: Option<String>,
        project: String,
        left: ComparisonEndpoint,
        right: ComparisonEndpoint,
        base: String,
        old: Box<SideRead>,
        new: Box<SideRead>,
        patch: Option<FileContent>,
    },
    /// The reply to `read_project_change_bodies`: both sides with their whole bodies in `file`,
    /// each present or absent, never out of scope.
    ProjectChangeBodies {
        id: Option<String>,
        project: String,
        worktree: Option<String>,
        group: ChangeGroup,
        old: Box<SideRead>,
        new: Box<SideRead>,
    },
    /// The reply to `read_project_comparison_change_bodies`: `left`, `right` and `base` echo the
    /// request.
    ProjectComparisonChangeBodies {
        id: Option<String>,
        project: String,
        left: ComparisonEndpoint,
        right: ComparisonEndpoint,
        base: String,
        old: Box<SideRead>,
        new: Box<SideRead>,
    },
    /// The reply to `list_project_commits`: `base` and `tip` echo the request; `commits` are newest
    /// first. `complete` is false when the changes were cut at a budget, and `commits_complete`
    /// when there were more commits than are listed.
    ProjectCommits {
        id: Option<String>,
        project: String,
        base: String,
        tip: String,
        commits: Vec<CommitChanges>,
        complete: bool,
        commits_complete: bool,
    },
    /// The reply to `read_project_commit_change`: `commit` and `parent` echo the request, and both
    /// sides and the patch were read from exactly the parent (the empty tree for none) and the
    /// commit.
    ProjectCommitChange {
        id: Option<String>,
        project: String,
        commit: String,
        parent: Option<String>,
        old: Box<SideRead>,
        new: Box<SideRead>,
        patch: Option<FileContent>,
    },
    /// The reply to `read_project_commit_change_bodies`: `commit` and `parent` echo the request.
    ProjectCommitChangeBodies {
        id: Option<String>,
        project: String,
        commit: String,
        parent: Option<String>,
        old: Box<SideRead>,
        new: Box<SideRead>,
    },
    Ack {
        id: Option<String>,
    },
    /// The reply to `check_daemon_update`: whether it started a check (`started`), and, when it
    /// did, the `last_check.at` of the status as the check started (null for none), so that the
    /// end of this check is the status whose `last_check.at` is another. A check that did not start
    /// changes nothing; `last_check_at` is then null.
    DaemonUpdateCheck {
        id: Option<String>,
        started: bool,
        last_check_at: Option<i64>,
    },
    /// The reply to `update_daemon`, once the download has started: the `install.started_at` the
    /// status gives it, by which a client tells this install's end from another's.
    DaemonUpdateStarted {
        id: Option<String>,
        started_at: i64,
    },
    /// The reply to `handover_check`: the running sessions and shell terminals, by id, that the
    /// planned stop would not carry over. `known` is false when the request did not give a floor
    /// and target version that read, so the lists hold everything running rather than a proven
    /// answer. `unreached_shell_terminals` are those of a pending holder that would not be
    /// carried over: not registered with the daemon, their busy state unknown.
    HandoverCheck {
        id: Option<String>,
        known: bool,
        sessions: Vec<String>,
        shell_terminals: Vec<String>,
        unreached_shell_terminals: Vec<String>,
    },
    /// A failure, with a stable `code` and the `params` that fill it. A client words the failure from
    /// them in its own language and branches on the code where it has to act rather than just show;
    /// `message` is the English text, for a client that does not know the code.
    Error {
        id: Option<String>,
        code: Code,
        params: Params,
        message: String,
    },
}

/// Something a session's user has to be told, as a coded message. Carried from where it is found to
/// where it is broadcast as a `session_notice`.
#[derive(Debug, Clone)]
pub struct Notice {
    pub code: Code,
    pub message: String,
    pub params: Params,
}

impl Notice {
    pub fn new(code: Code, message: impl Into<String>, pairs: &[(&str, &str)]) -> Self {
        Self {
            code,
            message: message.into(),
            params: params(pairs),
        }
    }

    /// The `session_notice` event telling the user about this, for `session`.
    pub fn about(self, session: &str) -> Event {
        Event::SessionNotice {
            session: session.to_string(),
            code: self.code,
            params: self.params,
            message: self.message,
        }
    }
}

/// What approving a `session_request` does.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionRequestAction {
    /// Starts a new session that has no owner, from the requester's brief.
    Start,
    /// Reopens the archived session that holds a conversation.
    Reopen,
    /// Resumes the interrupted session that holds a conversation.
    Resume,
    /// Opens a new session, with no owner, on an agent's own record of a conversation that no
    /// session holds.
    ResumeRecord,
    /// Moves the session `target`, which the asking session owns, to another account of its agent.
    Switch,
    /// Hands the session `target`, which the asking session owns, off to a new session of another
    /// agent.
    Handoff,
}

/// Why a `session_request` stopped waiting.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionRequestEnding {
    Approved,
    /// Refused by the user, dismissing the dialog included.
    Refused,
    /// Nobody answered within the request's time limit.
    TimedOut,
    /// The requesting session stopped waiting: its process ended, its call's connection to the
    /// daemon was dropped, or its agent cancelled the call.
    Withdrawn,
}

#[cfg(test)]
mod tests {
    use crate::protocol::{
        error_code, Event, ImportFailure, ImportItem, ImportItemState, SessionStatus,
    };

    #[test]
    fn a_session_interrupted_reply_serializes_with_its_type_and_fields() {
        assert_eq!(
            serde_json::to_value(Event::SessionInterrupted {
                id: Some("request-1".to_owned()),
                session: "s-1".to_owned(),
                interrupted: true,
                status: SessionStatus::Idle,
                note: "Interrupted.".to_owned(),
            })
            .unwrap(),
            serde_json::json!({
                "type": "session_interrupted",
                "id": "request-1",
                "session": "s-1",
                "interrupted": true,
                "status": "idle",
                "note": "Interrupted.",
            })
        );
    }

    #[test]
    fn a_configuration_import_progress_event_flattens_its_state_beside_its_item() {
        assert_eq!(
            serde_json::to_value(Event::ConfigurationImportProgress {
                import_id: "import-1".to_owned(),
                item: ImportItem::Project {
                    console: 0,
                    project: 2,
                },
                state: ImportItemState::Waiting,
            })
            .unwrap(),
            serde_json::json!({
                "type": "configuration_import_progress",
                "import_id": "import-1",
                "item": {"kind": "project", "console": 0, "project": 2},
                "state": "waiting",
            })
        );
        assert_eq!(
            serde_json::to_value(Event::ConfigurationImportProgress {
                import_id: "import-1".to_owned(),
                item: ImportItem::Settings,
                state: ImportItemState::Failed {
                    error: ImportFailure {
                        code: error_code::ICON_NOT_AN_IMAGE,
                        params: Default::default(),
                        message: "not an image".to_owned(),
                    },
                },
            })
            .unwrap(),
            serde_json::json!({
                "type": "configuration_import_progress",
                "import_id": "import-1",
                "item": {"kind": "settings"},
                "state": "failed",
                "error": {
                    "code": "icon_not_an_image",
                    "params": {},
                    "message": "not an image",
                },
            })
        );
    }
}
