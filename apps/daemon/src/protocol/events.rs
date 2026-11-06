//! What the daemon sends on `/ws/control`: every reply and broadcast, a session's notice, and what
//! approving or ending a session request does. See "Daemon to client" in
//! `apps/daemon/PROTOCOL.md`.

use serde::Serialize;

use super::accounts::{AccountLogin, AccountSharing, AccountTrashOutcome};
use super::agents::AgentId;
use super::browse::{
    BranchInfo, BrowseEntry, ChangeEntry, ChangeGroup, CommitChanges, ComparisonEndpoint,
    ContentSource, FileContent, ProjectSourceInfo, SideRead, UpstreamInfo,
};
use super::codes::{error_code, Code};
use super::configuration::{
    ConfigurationDocument, DocumentSettings, ImportFailure, ImportItem, ImportItemState,
    ImportedAccounts, ImportedConsole, InspectedAccounts, InspectedConsole, InspectedSettings,
};
use super::errors::{error_parts, params, Params};
use super::fields::is_true;
use super::history::{ConversationLocation, HistoryMessage, SessionRecordPresence};
use super::models::{AgentModel, DefaultModelSource, ModelChangeOutcome, ModelsUnavailable};
use super::path_links::PathLink;
use super::processes::{ProcessCount, ProcessTree};
use super::providers::ProviderInfo;
use super::quotas::{AccountQuota, ResetCreditOutcome};
use super::records::{
    AgentAvailability, Console, DirEntry, GitStatus, Host, Page, Project, Session, SessionKind,
    SessionStatus, ShellTerminal,
};
use super::session_branches::SessionBranchItem;
use super::settings::Settings;

#[cfg_attr(test, derive(ts_rs::TS))]
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
        /// The daemon host's home directory, so a client can show a path under it as `~/...`; it
        /// is the daemon's home, not the browser's, since the two may be on different machines.
        /// Null when the daemon cannot determine one.
        home_dir: Option<String>,
        /// Every shell terminal the daemon holds, running or exited, oldest first. Runtime state,
        /// not stored.
        shell_terminals: Vec<ShellTerminal>,
    },
    /// Availability or a default account's resolved directory changed for one or more agents —
    /// the whole three-entry list, like every other upsert. Broadcast once, when the daemon's
    /// one-time determination lands; never again afterwards, since nothing re-determines it
    /// during a run.
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
    ConsoleUpserted {
        console: Console,
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
        #[cfg_attr(test, ts(optional))]
        model: Option<Option<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        effort: Option<Option<String>>,
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
        accounts: Option<InspectedAccounts>,
        consoles: Vec<InspectedConsole>,
    },
    /// The reply to `import_configuration`: what came of the settings and accounts asked for, and
    /// one entry per console of the document, in its order, each naming the console created for
    /// it; no entry at all with `apply_consoles` false.
    ConfigurationImported {
        id: Option<String>,
        settings_applied: Option<DocumentSettings>,
        settings_error: Option<ImportFailure>,
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
        state: ImportItemState,
    },
    /// The reply to `list_pages`, oldest first. Pages are not in `snapshot`: a page carries a whole
    /// HTML document, and only the session the user is looking at needs its pages, so the
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
    /// Pages a session withdrew with `delete_pages`, by id. `session_id` names the session that
    /// pushed them. The panel showing that session drops them.
    PagesDeleted {
        session_id: String,
        pages: Vec<String>,
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
    /// The reply to `read_project_file`; `project`, `worktree` and `path` echo the request.
    ProjectFile {
        id: Option<String>,
        project: String,
        worktree: Option<String>,
        path: String,
        source: ContentSource,
        file: FileContent,
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

impl Event {
    /// The reply to a request the daemon could not read.
    pub fn unreadable_request(detail: &str) -> Self {
        Self::Error {
            id: None,
            code: error_code::UNREADABLE_REQUEST,
            params: params(&[("detail", detail)]),
            message: format!("unreadable request: {detail}"),
        }
    }

    /// The reply to the failure `err`: its own code and params when it is a [`CodedError`], and
    /// otherwise [`error_code::INTERNAL_ERROR`] carrying the whole context chain.
    ///
    /// [`CodedError`]: super::CodedError
    pub fn from_error(id: Option<String>, err: &anyhow::Error) -> Self {
        let (code, params, message) = error_parts(err);
        Self::Error {
            id,
            code,
            params,
            message,
        }
    }
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
#[cfg_attr(test, derive(ts_rs::TS))]
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
#[cfg_attr(test, derive(ts_rs::TS))]
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
