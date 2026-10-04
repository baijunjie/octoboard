//! The wire types of the daemon's only external interface. `daemon/PROTOCOL.md` is the
//! specification; this module is its Rust form.

use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Agent {
    Claude,
    Codex,
    Grok,
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
    pub started_at: i64,
    pub ended_at: Option<i64>,
}

/// One page the hub pushed to its console's report panel. Every page is kept, so the panel can be
/// paged back through; `anchor_message_id` records the conversation position the page was pushed at
/// and is stored only — the rewind linkage that reads it is after the MVP (see "Report panel" in
/// `docs/mvp.md`), and no agent exposes a message id to put in it yet.
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
    },
    UpdateConsole {
        console: String,
        name: Option<String>,
        hub_agent: Option<Agent>,
        default_agent: Option<Agent>,
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
    /// Broadcast once, when it is found; nothing stores it.
    SessionNotice {
        session: String,
        message: String,
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
    Error {
        id: Option<String>,
        /// A stable name for the failures a client has to act on rather than just show. The
        /// message is prose and gets reworded; anything a client branches on needs a name that
        /// does not.
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<String>,
        message: String,
    },
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

/// An error that carries a code, so a client can branch on it without matching prose. Travels
/// inside `anyhow::Error` and is recovered by downcasting where the reply is built.
#[derive(Debug)]
pub struct CodedError {
    pub code: &'static str,
    pub message: String,
}

impl CodedError {
    /// Returns the `anyhow::Error` rather than the bare value: every caller wants it in that
    /// shape, and the one place that reads it back downcasts out of exactly this.
    pub fn raised(code: &'static str, message: impl Into<String>) -> anyhow::Error {
        anyhow::Error::new(Self {
            code,
            message: message.into(),
        })
    }
}

impl std::fmt::Display for CodedError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CodedError {}

/// Error codes carried by `Event::Error`. Only failures a client must recognise get one.
pub mod error_code {
    /// A launch was asked for while one was already running or already starting for that session.
    /// The client's own double click is the ordinary cause, so it is shown as nothing at all.
    pub const SESSION_ALREADY_RUNNING: &str = "session_already_running";
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
}
