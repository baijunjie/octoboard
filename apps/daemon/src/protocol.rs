//! The wire types of the daemon's only external interface. `apps/daemon/PROTOCOL.md` is the
//! specification; this module is its Rust form.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

/// The named values a coded message is filled with. A parameter that names a console, a project or
/// a session is that record's id under the param name `console`, `project` or `session`, so the
/// client can show the record's current name; anything else is passed as text, verbatim.
pub type Params = BTreeMap<String, String>;

fn params(pairs: &[(&str, &str)]) -> Params {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Agent {
    Claude,
    Codex,
    Grok,
}

impl Agent {
    /// How the agent is named in a message shown to the user.
    pub fn label(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Codex => "Codex",
            Agent::Grok => "Grok Build",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Hub,
    Worker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Hub,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Working,
    WaitingUser,
    Idle,
    Interrupted,
    Archived,
}

impl SessionStatus {
    /// True for the two statuses that mean "no process is running for this session". Both are
    /// resumable; the difference is only how the session got there.
    pub fn is_dormant(self) -> bool {
        matches!(self, SessionStatus::Interrupted | SessionStatus::Archived)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostKind {
    Local,
    Ssh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectSource {
    Local,
    Parent,
    Github,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub id: String,
    pub name: String,
    pub kind: HostKind,
    pub ssh_config: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Console {
    pub id: String,
    pub name: String,
    pub workdir: String,
    pub hub_agent: Agent,
    pub default_agent: Agent,
    /// Where each agent's sessions opened in this console keep their configuration, login and
    /// transcripts, as an absolute path; one setting per agent, and a session reads only its own
    /// agent's. Claude Code gets it as `CLAUDE_CONFIG_DIR`, Codex as `CODEX_HOME`, and for Grok it
    /// is the directory its per-session home is built from instead of `~/.grok`. Set over whatever
    /// the user's shell environment exports; unset leaves that as it is.
    pub claude_config_dir: Option<String>,
    /// Codex's own directory; see `claude_config_dir`.
    pub codex_config_dir: Option<String>,
    /// Grok's own directory; see `claude_config_dir`.
    pub grok_config_dir: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub console_id: String,
    pub host_id: String,
    pub name: String,
    pub path: String,
    pub default_agent: Option<Agent>,
    pub source: ProjectSource,
    pub remote_url: Option<String>,
    /// The user has agreed that Octoboard may answer Claude Code's workspace-trust screen for this
    /// project's directory, by sending the keystrokes that accept it (see `crate::trust`). Recorded
    /// when they confirm the dialog the daemon asks them with; it covers every later Claude Code
    /// session in the project.
    pub claude_trust_consent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub agent: Agent,
    pub agent_session_id: Option<String>,
    pub console_id: String,
    pub project_id: Option<String>,
    pub host_id: String,
    pub role: Role,
    pub origin: Origin,
    pub title: String,
    pub status: SessionStatus,
    /// Whether this session has ever had a turn. Until it has, the agent has stored no
    /// conversation to resume: `claude --resume` on such an id fails outright with "No conversation
    /// found", and a session the user opened and never typed into is the common case of that. A
    /// resume therefore starts a fresh conversation rather than failing.
    pub has_conversation: bool,
    /// Whether this session's reports go to its console's hub. Always true for a session the hub
    /// started; a session the user opened by hand is outside the orchestration unless they asked
    /// for it to be included.
    pub include_in_hub: bool,
    /// The configuration directory of this session's own agent that it was started with, fixed at
    /// creation: the console's setting for that agent at the time. An agent keeps a conversation's
    /// transcript under that directory, so a resume finds it only when relaunched with the same one
    /// — which is why this is the session's own copy and a later edit of the console's setting never
    /// reaches a session that already exists. Unset for a session started with no directory
    /// pinned; such a session resumes under whatever the shell exports at that moment.
    pub config_dir: Option<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
}

/// One page the hub pushed to its console's report panel. Every page is kept, so the panel can be
/// paged back through; `anchor_message_id` records the conversation position the page was pushed at
/// and is stored only (see "Data model" in `docs/architecture.md`), and no agent exposes a message
/// id to put in it yet.
#[derive(Debug, Clone, Serialize)]
pub struct Page {
    pub id: String,
    pub console_id: String,
    pub html: String,
    pub anchor_message_id: Option<String>,
    pub created_at: i64,
}

/// One entry of a directory listing. Only directories are ever listed — a project is a directory —
/// so there is no "is this a directory" field to carry.
#[derive(Debug, Clone, Serialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_git_repo: bool,
}

/// One control-socket frame from the client: an optional request id the daemon echoes back, plus
/// the request itself. The id lets the UI pair a reply with the request that caused it; state
/// changes are broadcast to every client instead and carry no id.
///
/// `id` belongs to the envelope alone. Inside a request, what it acts on is named for its kind —
/// `console`, `project`, `session` — because one flattened object carries both: a request field
/// named `id` would collide with the envelope's and silently lose one of the two, leaving the
/// daemon acting on the request id as if it were a record id.
#[derive(Debug, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(flatten)]
    pub body: RequestBody,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RequestBody {
    CreateConsole {
        name: String,
        hub_agent: Agent,
        default_agent: Agent,
        #[serde(default)]
        claude_config_dir: Option<String>,
        #[serde(default)]
        codex_config_dir: Option<String>,
        #[serde(default)]
        grok_config_dir: Option<String>,
    },
    UpdateConsole {
        console: String,
        name: Option<String>,
        hub_agent: Option<Agent>,
        default_agent: Option<Agent>,
        /// Each config directory: absent leaves the setting alone; an explicit `null` (or a blank
        /// string) clears it, so the user's shell environment applies again.
        #[serde(default, deserialize_with = "present_option")]
        claude_config_dir: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        codex_config_dir: Option<Option<String>>,
        #[serde(default, deserialize_with = "present_option")]
        grok_config_dir: Option<Option<String>>,
    },
    DeleteConsole {
        console: String,
    },
    AddProject {
        console_id: String,
        source: ProjectSource,
        path: Option<String>,
        remote_url: Option<String>,
        name: Option<String>,
        default_agent: Option<Agent>,
    },
    UpdateProject {
        project: String,
        name: Option<String>,
        /// Absent leaves the project's default agent alone; an explicit `null` clears it, which is
        /// how the project goes back to inheriting the console's default. The attribute is what
        /// keeps those two apart: serde otherwise reads a present `null` into the outer `Option` as
        /// `None`, exactly as if the field had not been sent.
        #[serde(default, deserialize_with = "present_option")]
        default_agent: Option<Option<Agent>>,
    },
    DeleteProject {
        project: String,
    },
    ListDir {
        path: String,
    },
    OpenSession {
        console_id: String,
        project_id: Option<String>,
        agent: Option<Agent>,
        task: Option<String>,
        title: Option<String>,
        /// Whether the session reports to the hub. Absent is false: a session the user opens by
        /// hand stays outside the orchestration unless they check "include in hub".
        #[serde(default)]
        include_in_hub: bool,
    },
    ResumeSession {
        session: String,
    },
    ArchiveSession {
        session: String,
    },
    SendMessage {
        session: String,
        text: String,
    },
    RenameSession {
        session: String,
        title: String,
    },
    ListPages {
        console: String,
    },
    /// What a report panel form was submitted with. The page it came from is named rather than the
    /// hub session, because that is what the panel knows and it is also what decides whether the
    /// submission is allowed at all: only the console's newest page is live.
    SubmitPage {
        page: String,
        data: serde_json::Value,
    },
    /// The user's answer to a `claude_trust_prompt`: Octoboard may answer that session's trust
    /// screen. `remember` also records the project's consent, so its later sessions are answered
    /// without asking; `trust_parent_dir` records the project's parent directory as trusted instead,
    /// so every project under it is, and when set `remember` adds nothing. The directory is the
    /// daemon's to derive from the session's project, never the client's to name. Either is
    /// recorded only once the screen has been answered.
    ConfirmClaudeTrust {
        session: String,
        remember: bool,
        #[serde(default)]
        trust_parent_dir: bool,
    },
    /// Stops trusting a directory. Projects' own consents are left as they are.
    RemoveTrustedDirectory {
        path: String,
    },
    Shutdown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Snapshot {
        hosts: Vec<Host>,
        consoles: Vec<Console>,
        projects: Vec<Project>,
        sessions: Vec<Session>,
        trusted_directories: Vec<String>,
    },
    /// The directories whose projects Octoboard answers Claude Code's trust screen for changed: the
    /// whole list, like every other upsert.
    TrustedDirectoriesUpdated {
        trusted_directories: Vec<String>,
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
    /// Something about a session the user has to be told, which no status field carries: an
    /// injected capability that will not apply, a setting of theirs Octoboard had to work around.
    /// Broadcast once, when it is found; nothing stores it. `code` and `params` name the notice for
    /// a client to word in its own language; `message` is the English text for a client that does
    /// not know the code.
    SessionNotice {
        session: String,
        code: String,
        params: Params,
        message: String,
    },
    /// A Claude Code session of a project the user has not yet agreed Octoboard may answer for —
    /// not by its own consent and not through a trusted directory — is sitting at its
    /// workspace-trust screen. Broadcast once per screen, and sent again to a client after each
    /// `snapshot` while the screen waits. `trust_dir` is the directory `confirm_claude_trust` with
    /// `trust_parent_dir` would trust, and null when there is none to offer: it would be the
    /// filesystem root, the home directory or one containing it, the home directory cannot be
    /// determined, or the project's path is not absolute.
    ClaudeTrustPrompt {
        session: String,
        project: String,
        path: String,
        trust_dir: Option<String>,
    },
    /// The reply to `open_session`: the session that was started. The same record is broadcast as
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
    /// The reply to `list_pages`, oldest first. Pages are not in `snapshot`: a page carries a whole
    /// HTML document, and only a console whose hub the user is looking at needs its pages, so the
    /// panel asks for them instead — and asks again after every `snapshot`, which is what keeps it
    /// correct across a `page_created` the client was too far behind to receive. A lagging client is
    /// sent a fresh snapshot in place of the events it missed, on the socket it already has, so
    /// nothing else tells it that its list is now short.
    PageList {
        id: Option<String>,
        console_id: String,
        pages: Vec<Page>,
    },
    /// A page the hub just pushed. The panel showing that console's hub refreshes to it.
    PageCreated {
        page: Page,
    },
    Ack {
        id: Option<String>,
    },
    /// A failure, with a stable `code` and the `params` that fill it. A client words the failure from
    /// them in its own language and branches on the code where it has to act rather than just show;
    /// `message` is the English text, for a client that does not know the code.
    Error {
        id: Option<String>,
        code: String,
        params: Params,
        message: String,
    },
}

impl Event {
    /// The reply to a request the daemon could not read.
    pub fn unreadable_request(detail: &str) -> Self {
        Self::Error {
            id: None,
            code: error_code::UNREADABLE_REQUEST.to_string(),
            params: params(&[("detail", detail)]),
            message: format!("unreadable request: {detail}"),
        }
    }

    /// The reply to a failure that has no code of its own: it is not meant to be read in detail,
    /// so its text is carried whole under one generic code.
    pub fn internal_error(id: Option<String>, message: String) -> Self {
        Self::Error {
            id,
            code: error_code::INTERNAL_ERROR.to_string(),
            params: params(&[("detail", &message)]),
            message,
        }
    }
}

/// Something a session's user has to be told, as a coded message. Carried from where it is found to
/// where it is broadcast as a `session_notice`.
#[derive(Debug, Clone)]
pub struct Notice {
    pub code: &'static str,
    pub message: String,
    pub params: Params,
}

impl Notice {
    pub fn new(code: &'static str, message: impl Into<String>, pairs: &[(&str, &str)]) -> Self {
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
            code: self.code.to_string(),
            params: self.params,
            message: self.message,
        }
    }
}

/// Client-sent text frame on `/ws/term/:session`. Binary frames on that socket are raw PTY input
/// and never reach this type.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TermControl {
    Resize { cols: u16, rows: u16 },
}

/// Deserializes a present field — `null` included — as `Some`, leaving `None` to mean "the field
/// was not sent at all". Used for the fields where clearing a value and leaving it alone are
/// different requests.
fn present_option<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// An error that carries a code and its params, so a client can word it in its own language and
/// branch on it without matching prose. Travels inside `anyhow::Error` and is recovered by
/// downcasting where the reply is built.
#[derive(Debug)]
pub struct CodedError {
    pub code: &'static str,
    pub message: String,
    pub params: Params,
}

impl CodedError {
    /// Returns the `anyhow::Error` rather than the bare value: every caller wants it in that
    /// shape, and the one place that reads it back downcasts out of exactly this.
    pub fn raised(
        code: &'static str,
        message: impl Into<String>,
        pairs: &[(&str, &str)],
    ) -> anyhow::Error {
        anyhow::Error::new(Self {
            code,
            message: message.into(),
            params: params(pairs),
        })
    }

    /// The `error` reply to the request `id` that failed with this.
    pub fn reply(&self, id: Option<String>) -> Event {
        Event::Error {
            id,
            code: self.code.to_string(),
            params: self.params.clone(),
            message: self.message.clone(),
        }
    }

    pub fn unknown_console(id: &str) -> anyhow::Error {
        Self::raised(
            error_code::UNKNOWN_CONSOLE,
            format!("unknown console {id}"),
            &[("console", id)],
        )
    }

    pub fn unknown_project(id: &str) -> anyhow::Error {
        Self::raised(
            error_code::UNKNOWN_PROJECT,
            format!("unknown project {id}"),
            &[("project", id)],
        )
    }

    pub fn unknown_session(id: &str) -> anyhow::Error {
        Self::raised(
            error_code::UNKNOWN_SESSION,
            format!("unknown session {id}"),
            &[("session", id)],
        )
    }
}

impl std::fmt::Display for CodedError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CodedError {}

/// Error codes carried by `Event::Error`; `PROTOCOL.md` lists each with its params. A code never
/// changes its meaning or its params, because a client words the failure from them. A new code also
/// needs a `daemon.<code>` message in the UI's catalogs (`packages/ui/src/i18n/messages/`): without
/// one nothing fails, and the UI just shows the English `message`.
pub mod error_code {
    // -- what a client acts on, rather than just shows ---------------------------------------

    /// A launch was asked for while one was already running or already starting for that session.
    /// The client's own double click is the ordinary cause, so it is shown as nothing at all. The
    /// hub variants below mean the same for a console's hub.
    pub const SESSION_ALREADY_RUNNING: &str = "session_already_running";
    /// [`SESSION_ALREADY_RUNNING`], where the launch had begun but not yet registered.
    pub const SESSION_ALREADY_STARTING: &str = "session_already_starting";
    /// [`SESSION_ALREADY_RUNNING`], for opening a hub while the console has a live one.
    pub const HUB_ALREADY_RUNNING: &str = "hub_already_running";
    /// [`SESSION_ALREADY_RUNNING`], for reopening a hub while the console has another live one.
    pub const HUB_REOPEN_BLOCKED: &str = "hub_reopen_blocked";
    /// [`SESSION_ALREADY_RUNNING`], for a console whose hub is already being started.
    pub const HUB_ALREADY_STARTING: &str = "hub_already_starting";
    /// A directory to trust as a whole cannot be offered: it is the filesystem root, the user's home
    /// directory or one that contains it. Nothing was answered. The dialog the request came from
    /// stays open, because the user can still choose another way to answer, and so it does for the
    /// two codes below, which are the same refusal for other reasons.
    pub const TRUST_DIRECTORY_TOO_BROAD: &str = "trust_directory_too_broad";
    /// [`TRUST_DIRECTORY_TOO_BROAD`], where the project's path is not absolute.
    pub const TRUST_PATH_NOT_ABSOLUTE: &str = "trust_path_not_absolute";
    /// [`TRUST_DIRECTORY_TOO_BROAD`], where the home directory is unknown, so nothing can be
    /// checked against it.
    pub const TRUST_HOME_UNKNOWN: &str = "trust_home_unknown";
    /// A go-ahead for a trust screen that is no longer waiting for one: answered already, by this
    /// or another client or in the terminal, or gone with its session. Nothing is wrong, so a
    /// client shows nothing.
    pub const CLAUDE_TRUST_NOT_WAITING: &str = "claude_trust_not_waiting";

    // -- for the user to read ----------------------------------------------------------------

    /// Octoboard could not answer a trust screen it had accepted a go-ahead for. The `reason_code`
    /// param says why, one of [`trust_reason`].
    pub const CLAUDE_TRUST_ANSWER_FAILED: &str = "claude_trust_answer_failed";
    /// A go-ahead for the trust screen of a session that is not Claude Code's.
    pub const NOT_A_CLAUDE_SESSION: &str = "not_a_claude_session";
    /// A go-ahead for a hub session's trust screen, which Octoboard answers without asking.
    pub const HUB_TRUST_NOT_ASKED: &str = "hub_trust_not_asked";

    /// A request that is not valid JSON of a known shape.
    pub const UNREADABLE_REQUEST: &str = "unreadable_request";
    /// Anything that is not meant for the user to read in detail: an unexpected failure with no
    /// meaning of its own. The text is carried as the `detail` param.
    pub const INTERNAL_ERROR: &str = "internal_error";
    pub const UNKNOWN_CONSOLE: &str = "unknown_console";
    pub const UNKNOWN_PROJECT: &str = "unknown_project";
    pub const UNKNOWN_SESSION: &str = "unknown_session";
    pub const UNKNOWN_PAGE: &str = "unknown_page";
    /// A request lacks a field its kind needs.
    pub const FIELD_REQUIRED: &str = "field_required";
    pub const CONSOLE_HAS_RUNNING_SESSIONS: &str = "console_has_running_sessions";
    pub const PROJECT_HAS_RUNNING_SESSIONS: &str = "project_has_running_sessions";
    pub const PATH_NOT_ABSOLUTE: &str = "path_not_absolute";
    pub const PATH_NOT_FOUND: &str = "path_not_found";
    pub const PATH_NOT_A_DIRECTORY: &str = "path_not_a_directory";
    pub const PATH_ALREADY_EXISTS: &str = "path_already_exists";
    pub const DIRECTORY_UNREADABLE: &str = "directory_unreadable";
    /// A session's or a launch's working directory cannot be reached by the daemon.
    pub const DIRECTORY_UNREACHABLE: &str = "directory_unreachable";
    pub const NO_REPOSITORIES_FOUND: &str = "no_repositories_found";
    pub const ALL_PROJECTS_ALREADY_ADDED: &str = "all_projects_already_added";
    pub const REPOSITORY_NAME_MISSING: &str = "repository_name_missing";
    pub const GIT_CLONE_FAILED: &str = "git_clone_failed";
    pub const CONFIG_DIR_NOT_ABSOLUTE: &str = "config_dir_not_absolute";
    pub const CONFIG_DIR_NOT_A_DIRECTORY: &str = "config_dir_not_a_directory";
    /// A session's pinned configuration directory has gone, which refuses the launch.
    pub const CONFIG_DIR_UNREACHABLE: &str = "config_dir_unreachable";
    pub const SESSION_NOT_RUNNING: &str = "session_not_running";
    pub const SESSION_WAITING_FOR_USER: &str = "session_waiting_for_user";
    pub const QUEUED_MESSAGES_LOST: &str = "queued_messages_lost";
    pub const PAGE_NOT_CURRENT: &str = "page_not_current";
    pub const HUB_MISSING: &str = "hub_missing";
    pub const BINARY_NOT_FOUND: &str = "binary_not_found";
    pub const SHELL_ENVIRONMENT_TIMEOUT: &str = "shell_environment_timeout";
}

/// The `reason_code` param of a failed answer to a trust screen, in the `error` and in the
/// `session_notice` alike; `PROTOCOL.md` lists each. A client that does not know one shows the
/// English `reason` instead. A new reason also needs a `daemon.trust_reason.<code>` message in the
/// UI's catalogs.
pub mod trust_reason {
    pub const SCREEN_GONE: &str = "screen_gone";
    pub const CURSOR_NOT_ON_DECLINE: &str = "cursor_not_on_decline";
    pub const CURSOR_DID_NOT_MOVE: &str = "cursor_did_not_move";
    pub const TERMINAL_NOT_SETTLED: &str = "terminal_not_settled";
    pub const CURSOR_MOVED_AWAY: &str = "cursor_moved_away";
    pub const SCREEN_NOT_DISMISSED: &str = "screen_not_dismissed";
    pub const SCREEN_REDRAWN: &str = "screen_redrawn";
    pub const INPUT_TOUCHED: &str = "input_touched";
    pub const TERMINAL_WRITE_FAILED: &str = "terminal_write_failed";
}

/// Codes carried by `Event::SessionNotice`; `PROTOCOL.md` lists each with its params. Like an error
/// code, each needs a `daemon.<code>` message in the UI's catalogs.
pub mod notice_code {
    pub const CLAUDE_WORKSPACE_UNTRUSTED: &str = "claude_workspace_untrusted";
    pub const QUEUED_MESSAGES_DROPPED: &str = "queued_messages_dropped";
    pub const CLAUDE_TRUST_ANSWER_FAILED: &str = "claude_trust_answer_failed";
}

pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{Request, RequestBody};

    /// The envelope's request id and the request's own fields arrive in one flat object, so a
    /// request field named `id` would be indistinguishable from the envelope's — and the daemon
    /// would act on the request id as if it were a record id. These are the requests that address
    /// a stored record; each must survive an envelope id sitting next to it.
    #[test]
    fn an_envelope_id_never_shadows_what_a_request_acts_on() {
        type Target = fn(RequestBody) -> Option<String>;
        let cases: &[(&str, Target)] = &[
            (
                r#"{"type":"delete_console","id":"request-1","console":"console-7"}"#,
                |body| match body {
                    RequestBody::DeleteConsole { console } => Some(console),
                    _ => None,
                },
            ),
            (
                r#"{"type":"delete_project","id":"request-1","project":"project-7"}"#,
                |body| match body {
                    RequestBody::DeleteProject { project } => Some(project),
                    _ => None,
                },
            ),
            (
                r#"{"type":"update_project","id":"request-1","project":"project-7"}"#,
                |body| match body {
                    RequestBody::UpdateProject { project, .. } => Some(project),
                    _ => None,
                },
            ),
            (
                r#"{"type":"archive_session","id":"request-1","session":"session-7"}"#,
                |body| match body {
                    RequestBody::ArchiveSession { session } => Some(session),
                    _ => None,
                },
            ),
            (
                r#"{"type":"submit_page","id":"request-1","page":"page-7","data":{}}"#,
                |body| match body {
                    RequestBody::SubmitPage { page, .. } => Some(page),
                    _ => None,
                },
            ),
            (
                r#"{"type":"confirm_claude_trust","id":"request-1","session":"session-7","remember":true}"#,
                |body| match body {
                    RequestBody::ConfirmClaudeTrust { session, .. } => Some(session),
                    _ => None,
                },
            ),
            (
                r#"{"type":"list_pages","id":"request-1","console":"console-7"}"#,
                |body| match body {
                    RequestBody::ListPages { console } => Some(console),
                    _ => None,
                },
            ),
        ];

        for (json, target) in cases {
            let request: Request = serde_json::from_str(json).expect(json);
            assert_eq!(request.id.as_deref(), Some("request-1"), "{json}");
            let target = target(request.body).expect(json);
            assert!(target.ends_with("-7"), "{json} acted on `{target}`");
        }
    }

    /// An absent optional field is "leave it alone" and an explicit null is "clear it", which is
    /// the only way a project can go back to inheriting its console's default agent.
    #[test]
    fn clearing_a_projects_default_agent_is_distinguishable_from_leaving_it() {
        let absent: Request =
            serde_json::from_str(r#"{"type":"update_project","project":"p"}"#).unwrap();
        let cleared: Request =
            serde_json::from_str(r#"{"type":"update_project","project":"p","default_agent":null}"#)
                .unwrap();
        match (absent.body, cleared.body) {
            (
                RequestBody::UpdateProject {
                    default_agent: absent,
                    ..
                },
                RequestBody::UpdateProject {
                    default_agent: cleared,
                    ..
                },
            ) => {
                assert_eq!(absent, None);
                assert_eq!(cleared, Some(None));
            }
            _ => panic!("both parse as update_project"),
        }
    }

    /// Absent versus null is the only way a console's config directory can be cleared as well as
    /// set, and each agent's is independent of the others.
    #[test]
    fn clearing_a_consoles_config_dir_is_distinguishable_from_leaving_it() {
        let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
            RequestBody::UpdateConsole {
                claude_config_dir,
                codex_config_dir,
                grok_config_dir,
                ..
            } => (claude_config_dir, codex_config_dir, grok_config_dir),
            _ => panic!("parses as update_console"),
        };
        assert_eq!(
            parse(r#"{"type":"update_console","console":"c"}"#),
            (None, None, None)
        );
        assert_eq!(
            parse(
                r#"{"type":"update_console","console":"c","claude_config_dir":null,
                    "codex_config_dir":"~/.codex-alt"}"#
            ),
            (Some(None), Some(Some("~/.codex-alt".to_string())), None)
        );
        assert_eq!(
            parse(r#"{"type":"update_console","console":"c","grok_config_dir":"~/.grok-alt"}"#),
            (None, None, Some(Some("~/.grok-alt".to_string())))
        );
    }

    #[test]
    fn creating_a_console_needs_no_config_dir() {
        let request: Request = serde_json::from_str(
            r#"{"type":"create_console","name":"n","hub_agent":"claude","default_agent":"codex"}"#,
        )
        .unwrap();
        match request.body {
            RequestBody::CreateConsole {
                claude_config_dir,
                codex_config_dir,
                grok_config_dir,
                ..
            } => assert_eq!(
                (claude_config_dir, codex_config_dir, grok_config_dir),
                (None, None, None)
            ),
            _ => panic!("parses as create_console"),
        }
    }

    /// `trust_parent_dir` is newer than `remember`, so a request without it still reads, as "this
    /// project only".
    #[test]
    fn a_trust_confirmation_without_a_scope_means_this_project() {
        let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
            RequestBody::ConfirmClaudeTrust {
                remember,
                trust_parent_dir,
                ..
            } => (remember, trust_parent_dir),
            _ => panic!("parses as confirm_claude_trust"),
        };
        assert_eq!(
            parse(r#"{"type":"confirm_claude_trust","session":"s","remember":true}"#),
            (true, false)
        );
        assert_eq!(
            parse(
                r#"{"type":"confirm_claude_trust","session":"s","remember":false,"trust_parent_dir":true}"#
            ),
            (false, true)
        );
        match serde_json::from_str::<Request>(
            r#"{"type":"remove_trusted_directory","path":"/work"}"#,
        )
        .unwrap()
        .body
        {
            RequestBody::RemoveTrustedDirectory { path } => assert_eq!(path, "/work"),
            _ => panic!("parses as remove_trusted_directory"),
        }
    }

    #[test]
    fn the_trusted_directories_travel_under_their_own_names() {
        let updated = serde_json::to_value(super::Event::TrustedDirectoriesUpdated {
            trusted_directories: vec!["/work".to_string()],
        })
        .unwrap();
        assert_eq!(updated["type"], "trusted_directories_updated");
        assert_eq!(updated["trusted_directories"][0], "/work");
        let snapshot = serde_json::to_value(super::Event::Snapshot {
            hosts: vec![],
            consoles: vec![],
            projects: vec![],
            sessions: vec![],
            trusted_directories: vec!["/work".to_string()],
        })
        .unwrap();
        assert_eq!(snapshot["trusted_directories"][0], "/work");
    }

    /// A client words a failure and a notice from their code and params; the English message rides
    /// along for one that does not know the code.
    #[test]
    fn an_error_and_a_notice_carry_a_code_and_named_params() {
        let error = super::CodedError::unknown_console("console-7");
        let coded = error.downcast_ref::<super::CodedError>().expect("coded");
        let event = serde_json::to_value(coded.reply(Some("request-1".to_string()))).unwrap();
        assert_eq!(event["code"], "unknown_console");
        assert_eq!(event["params"]["console"], "console-7");
        assert_eq!(event["message"], "unknown console console-7");

        let event =
            serde_json::to_value(super::Event::internal_error(None, "boom".to_string())).unwrap();
        assert_eq!(event["code"], "internal_error");
        assert_eq!(event["params"]["detail"], "boom");
        assert_eq!(event["message"], "boom");

        let notice = serde_json::to_value(
            super::Notice::new("a_notice", "English", &[("reason", "why")]).about("session-1"),
        )
        .unwrap();
        assert_eq!(notice["type"], "session_notice");
        assert_eq!(notice["session"], "session-1");
        assert_eq!(notice["code"], "a_notice");
        assert_eq!(notice["params"]["reason"], "why");
        assert_eq!(notice["message"], "English");
    }
}
