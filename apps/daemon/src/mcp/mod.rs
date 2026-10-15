//! The Octoboard MCP server: the orchestration and settings tools the console session drives
//! Octoboard with, the narrower set an unbound project session drives its own project with and asks
//! for a console session through, the reporting tool a bound project session answers through, and
//! the tools every project session shares information with its project's other sessions through and
//! reads its own project's settings with.
//!
//! **Transport is a stdio child process, not an HTTP endpoint the agent connects to.** Each
//! adapter registers `octoboardd mcp --session … --role … --port … --token …` as a `command`-type
//! MCP server, and that child ([`stdio`]) speaks MCP on its stdin/stdout while forwarding every
//! call to the daemon over loopback HTTP, where [`exec`] runs it against the real state. One
//! mechanism covers all three agents, which is why it was chosen over pointing them at a
//! Streamable HTTP endpoint in the daemon: Codex's verified injection takes a `command` and
//! `args` ("Injecting Octoboard into each agent" in `docs/agent-cli-reference.md`), and Grok's SSRF
//! protection rejects loopback HTTP outright, which is already why its hooks cannot be HTTP either.
//!
//! The tool catalogue lives here, shared by both sides: the child announces it, the daemon
//! dispatches against it, and the role descriptions name the tools, so all three cannot drift
//! apart. The names an agent actually shows the model are prefixed per agent — `mcp__octoboard__x`
//! on Claude Code and Codex, `octoboard__x` on Grok (see "Across the three agents" in
//! `docs/agent-cli-reference.md`), which is why [`qualified_tool_name`] exists rather than the
//! role text spelling a prefix out.

mod args;
pub mod exec;
pub mod role;
mod settings;
pub mod stdio;

use serde_json::{json, Value};

use crate::protocol::{Agent, Role};

/// The key the adapters register the server under. It must not collide with a server the project
/// defines: on Claude Code a colliding key means the *project's* definition is silently never
/// spawned.
pub const SERVER_KEY: &str = "octoboard";

/// Who a session with the role of a bound one reports to, as far as what the session is told
/// depends on it. Fixed for the session's lifetime, like the binding itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    Console,
    /// A project session that is unbound or a lead session, named by Octoboard's id for it: a
    /// title can be renamed after the text naming it has been recorded for good.
    Project {
        session_id: String,
    },
}

/// One tool, as both sides of the stdio bridge see it.
#[derive(Clone, Copy)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    /// The tool's JSON Schema, built on demand rather than stored: `rmcp` wants an owned object
    /// and the catalogue is walked once per `tools/list`.
    pub schema: fn() -> Value,
}

/// The tools a session of this role may call. Nothing else is announced to it, so a bound project
/// session cannot start or archive sessions and the console session cannot report to itself.
///
/// `bound` says whether the session has the role of a bound one (`Session::has_bound_role`): it
/// reports to another session and owns none. That is fixed for the session's lifetime, which is
/// what lets the catalogue follow from it: a lead session, bound only after it was launched
/// unbound, keeps the unbound set. A console session is never bound, and it is ignored for one.
pub fn tools_for(role: Role, bound: bool) -> &'static [ToolDef] {
    match (role, bound) {
        (Role::Console, _) => CONSOLE_SESSION_TOOLS,
        (Role::Project, true) => BOUND_PROJECT_SESSION_TOOLS,
        (Role::Project, false) => UNBOUND_PROJECT_SESSION_TOOLS,
    }
}

pub fn tool_by_name(role: Role, bound: bool, name: &str) -> Option<&'static ToolDef> {
    tools_for(role, bound).iter().find(|tool| tool.name == name)
}

/// How the agent presents one of these tools to its model. Only the prefix differs between the
/// agents, but a role description that names the wrong one sends the model looking for a tool it
/// cannot see.
pub fn qualified_tool_name(agent: Agent, tool: &str) -> String {
    match agent {
        Agent::Claude | Agent::Codex => format!("mcp__{SERVER_KEY}__{tool}"),
        Agent::Grok => format!("{SERVER_KEY}__{tool}"),
    }
}

const CONSOLE_SESSION_TOOLS: &[ToolDef] = &[
    ToolDef {
        name: "list_projects",
        description: "List this console's projects: name, host, directory, source, remote URL \
                      (credentials removed), pin, the tags the user gave each, the settings \
                      (`default_agent`, `accounts`, `launch`; see `update_project`), whether the \
                      user has `trust_consent`ed to Octoboard pressing the agent's folder-trust \
                      confirmation (read-only), and the sessions currently running in each — \
                      every session of the console, whichever session owns it. Each carries `owner`, the \
                      session it reports to, `owner_kind` (a `console` or a `project` session) \
                      and `yours`; a project session with `owner: null` is unbound, which the user \
                      opened themselves and kept outside the orchestration. A session with \
                      `yours: false` is not this caller's to drive — it is unbound, or reports \
                      to a different session — leave it alone.",
        schema: || object_schema(json!({}), &[]),
    },
    ToolDef {
        name: "add_project",
        description: "Associate a directory with this console, or clone a git repository and \
                      associate the clone. `source` is `local` for one directory, `parent` for \
                      every git repository directly beneath `path`, or `git` to clone \
                      `remote_url` into `path`, which defaults to the user's default clone \
                      directory. Returns the settings of each project added, as `list_projects` \
                      gives them. A `git` URL must be an http(s), ssh or git URL without \
                      credentials in it: the user's own git credentials are used.",
        schema: || {
            object_schema(
                json!({
                    "source": {
                        "type": "string",
                        "enum": ["local", "parent", "git"],
                        "description": "How the directory is being associated.",
                    },
                    "path": {
                        "type": "string",
                        "description": "The directory to associate, the parent directory to scan, \
                                        or the parent directory to clone into. Must be an \
                                        absolute path or start with `~/`. Required for \
                                        `local` and `parent`; for `git` it defaults to the \
                                        user's default clone directory.",
                    },
                    "remote_url": {
                        "type": "string",
                        "description": "The repository to clone. Required for `git`.",
                    },
                    "name": {
                        "type": "string",
                        "description": "Display name. Only applies when a single project is being \
                                        associated; defaults to the directory's own name.",
                    },
                    "default_agent": {
                        "type": "string",
                        "enum": ["claude", "codex", "grok"],
                        "description": "The agent sessions in this project use by default. \
                                        When omitted, the agent the directory is set up for \
                                        (from its top-level marker files) if exactly one is \
                                        found and not known to be missing from PATH; \
                                        otherwise the project inherits the console's.",
                    },
                    "tags": tags_schema("The project's tags."),
                    "accounts": accounts_schema(true),
                    "launch": launch_schema(),
                    "pinned": {
                        "type": "boolean",
                        "description": "Pin the project to the top of the console's project list.",
                    },
                }),
                &["source"],
            )
        },
    },
    ToolDef {
        name: "update_project",
        description: "Change settings of one of this console's projects; what is omitted is left \
                      as it is. Returns the project's settings as `list_projects` gives them. \
                      Sessions already running keep what they were started with; a change \
                      reaches the ones opened or resumed afterwards. The project's directory, \
                      source and remote cannot be changed, and a project cannot be deleted. \
                      These are the user's settings: change one only when they asked you to.",
        schema: || {
            object_schema(
                json!({
                    "project": {
                        "type": "string",
                        "description": "The project's id, or its name when that is unambiguous.",
                    },
                    "name": { "type": "string", "description": "The display name. Not blank." },
                    "tags": tags_schema("Replaces the project's whole tag list. Not to be \
                                         combined with `add_tags` or `remove_tags`."),
                    "add_tags": tags_schema("Tags to add to the ones the project has."),
                    "remove_tags": tags_schema("Tags to remove, matched ignoring letter case."),
                    "pinned": {
                        "type": "boolean",
                        "description": "Pin the project to the top of the console's project list.",
                    },
                    "default_agent": {
                        "type": ["string", "null"],
                        "enum": ["claude", "codex", "grok", null],
                        "description": "The agent sessions in this project use by default; \
                                        `null` goes back to inheriting the console's.",
                    },
                    "accounts": accounts_schema(true),
                    "launch": launch_schema(),
                }),
                &["project"],
            )
        },
    },
    ToolDef {
        name: "get_console",
        description: "This console's own settings: `name`, `default_agent`, `accounts`, `launch` \
                      (the model and effort of console sessions like this one), whether it has an \
                      avatar (`has_icon`), and its working directory (`workdir`, read-only). \
                      Only the console this session belongs to can be read.",
        schema: || object_schema(json!({}), &[]),
    },
    ToolDef {
        name: "update_console",
        description: "Change this console's own settings; what is omitted is left as it is. \
                      Returns them as `get_console` gives them. Sessions already running keep \
                      what they were started with: a new account reaches only console sessions \
                      opened afterwards, and a new model or effort reaches this console \
                      session on its next launch unless it was given its own. Only this \
                      console can be changed, and not its avatar. These are the user's \
                      settings: change one only when they asked you to.",
        schema: || {
            object_schema(
                json!({
                    "name": { "type": "string", "description": "The display name. Not blank." },
                    "default_agent": {
                        "type": "string",
                        "enum": ["claude", "codex", "grok"],
                        "description": "The agent sessions use by default, console sessions \
                                        included.",
                    },
                    "accounts": accounts_schema(false),
                    "launch": launch_schema(),
                }),
                &[],
            )
        },
    },
    ToolDef {
        name: "get_settings",
        description: "The application-wide settings: `auto_pull_repositories`, \
                      `default_clone_dir`, the `accounts` (each agent also has a default account, \
                      listed with the id `default`), and the `trusted_directories`. Accounts and \
                      trusted directories are read-only here.",
        schema: || object_schema(json!({}), &[]),
    },
    ToolDef {
        name: "update_settings",
        description: "Change application-wide settings; what is omitted is left as it is. \
                      Returns them as `get_settings` gives them. Turning `auto_pull_repositories` \
                      on fast-forwards the projects already known to be behind at once, which \
                      changes files in the user's repositories. These are the user's settings: \
                      change one only when they asked you to.",
        schema: || {
            object_schema(
                json!({
                    "auto_pull_repositories": {
                        "type": "boolean",
                        "description": "Whether projects behind their upstream are pulled \
                                        (fast-forwarded) automatically.",
                    },
                    "default_clone_dir": {
                        "type": "string",
                        "description": "Where a `git` association clones into when no path is \
                                        given. An absolute path or one starting with `~/`; blank \
                                        goes back to the built-in default.",
                    },
                }),
                &[],
            )
        },
    },
    ToolDef {
        name: "start_session",
        description: "Start a session in one of this console's projects and hand it a task. \
                      Returns the new session's id. The brief is rendered into the session's \
                      opening prompt. The new session reports to this console session, \
                      which is its owner.",
        schema: || start_session_schema(true),
    },
    SEND_MESSAGE,
    ToolDef {
        name: "get_session",
        description: "The session's status, plus a tail of what it has printed (a session that \
                      has ended keeps its last output; `null` if none was kept). Use it to \
                      follow up; a session that is waiting for the user must be left alone until \
                      they have answered. Any session can be read, but one with `yours: false` is \
                      not this caller's to act on, whether it is unbound (`owner: null` on a \
                      project session) or reports to a different session, console or project: \
                      leave it alone. `owner` says whose it is, `owner_kind` whether that is a \
                      console or a project session, and `role` whether the session itself is a \
                      console session.",
        schema: || {
            object_schema(
                json!({
                    "session": { "type": "string", "description": "The session's id." },
                }),
                &["session"],
            )
        },
    },
    RENAME_SESSION,
    SET_SESSION_PINNED,
    ARCHIVE_SESSION,
    ToolDef {
        name: "list_archived",
        description: "List the archived sessions of one project, whichever session owns \
                      them. Each carries its `owner`, `owner_kind` and `yours`, so an earlier \
                      one of yours can be reopened instead of starting over. A session with \
                      `yours: false` is somebody else's: leave it alone.",
        schema: || {
            object_schema(
                json!({
                    "project": {
                        "type": "string",
                        "description": "The project's id, or its name when that is unambiguous.",
                    },
                }),
                &["project"],
            )
        },
    },
    REOPEN_SESSION,
    ToolDef {
        name: "show_page",
        description: "Push a page to the report panel the user sees beside this session. Every \
                      page pushed is kept, and the user can page back through earlier ones. `html` \
                      is a static, self-contained HTML document: scripts and event handlers are \
                      removed and never run, and nothing external loads (no `<link>` elements, \
                      frames or external URLs), so inline any styles, stick to generic font \
                      families rather than a custom typeface, and give an image as a `data:` URL. \
                      Use it for anything better shown than typed into the terminal: a table, a \
                      comparison, a set of choices. \
                      A page may include a native HTML form: when the user submits it, its fields \
                      are sent back to this session as a message, each field's `name` becoming a \
                      key, so give every field you want back a `name`. For a choice use \
                      `<button name=\"choice\" value=\"...\">`; the pressed button's value comes \
                      back. `required` and `pattern` validation works. The call's result carries \
                      the pushed page's id; keep it, since it is the only way to tell a later \
                      submission came from this page rather than one pushed after it.",
        schema: || {
            object_schema(
                json!({
                    "html": {
                        "type": "string",
                        "description": "The page's full HTML document.",
                    },
                }),
                &["html"],
            )
        },
    },
];

const SEND_MESSAGE: ToolDef = ToolDef {
    name: "send_message",
    description: "Append an instruction to a running session of yours. Delivered straight \
                  away when the session is idle or mid-turn; held until the user is done when \
                  the session is waiting for them. Refused for a session that is not yours \
                  (`yours: false`): it is somebody else's, to be left alone.",
    schema: || {
        object_schema(
            json!({
                "session": { "type": "string", "description": "The session's id." },
                "text": {
                    "type": "string",
                    "description": "The instruction, in natural language.",
                },
            }),
            &["session", "text"],
        )
    },
};

const ARCHIVE_SESSION: ToolDef = ToolDef {
    name: "archive_session",
    description: "End a session's process and archive it. Use it to wrap a session up \
                  explicitly; a session that finishes cleanly with nothing left open is \
                  archived without being asked. Refused for a session that is not yours \
                  (`yours: false`): it is somebody else's, to be left alone.",
    schema: || {
        object_schema(
            json!({
                "session": { "type": "string", "description": "The session's id." },
            }),
            &["session"],
        )
    },
};

const RENAME_SESSION: ToolDef = ToolDef {
    name: "rename_session",
    description: "Change the title of a session of yours. Refused for a session that is not \
                  yours (`yours: false`): it is somebody else's, to be left alone.",
    schema: || {
        object_schema(
            json!({
                "session": { "type": "string", "description": "The session's id." },
                "title": { "type": "string", "description": "The new title. Not blank." },
            }),
            &["session", "title"],
        )
    },
};

const SET_SESSION_PINNED: ToolDef = ToolDef {
    name: "set_session_pinned",
    description: "Pin a session of yours to the top of its list, or unpin it. Refused for a \
                  session that is not yours (`yours: false`): it is somebody else's, to be \
                  left alone.",
    schema: || {
        object_schema(
            json!({
                "session": { "type": "string", "description": "The session's id." },
                "pinned": { "type": "boolean", "description": "Whether the session is pinned." },
            }),
            &["session", "pinned"],
        )
    },
};

const REOPEN_SESSION: ToolDef = ToolDef {
    name: "reopen_session",
    description: "Relaunch an archived or interrupted session of yours, continuing its \
                  conversation, and optionally hand it the next instruction. Refused for a \
                  session that is not yours (`yours: false`): it is somebody else's, to be \
                  left alone.",
    schema: || {
        object_schema(
            json!({
                "session": { "type": "string", "description": "The session's id." },
                "text": {
                    "type": "string",
                    "description": "An instruction to deliver once it is running.",
                },
            }),
            &["session"],
        )
    },
};

const REPORT: ToolDef = ToolDef {
    name: "report",
    description: "Report the round of work back to the session that dispatched this one, a \
                  console session or a project session. `summary` is natural language; `status` \
                  and `open_items` are what the session that dispatched this one acts on. With \
                  `done` and no open items the session is archived once the report is delivered; \
                  anything else leaves it running and awaiting instructions. Refused for a \
                  session nobody dispatched.",
    schema: || {
        object_schema(
            json!({
                "summary": {
                    "type": "string",
                    "description": "What was done and what came of it, in natural language.",
                },
                "status": {
                    "type": "string",
                    "enum": ["done", "failed", "needs_decision"],
                    "description": "`done` when the task is finished, `failed` when it could not \
                                    be, `needs_decision` when the session that dispatched this one \
                                    has to choose before it can go on.",
                },
                "open_items": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Anything left unfinished. Empty when nothing is.",
                },
            }),
            &["summary", "status"],
        )
    },
};

/// What every project session, bound or not, may do with the other sessions of its project:
/// look at them, and tell them things. Kept apart from `send_message` so that a message arriving
/// in a session is framed by which tool sent it: an instruction comes from its owner, and
/// information from anyone.
const LIST_PROJECT_SESSIONS: ToolDef = ToolDef {
    name: "list_project_sessions",
    description: "List the running sessions of this project other than this one: id, title, \
                  agent, status, and `owner` (the session it reports to, `owner_kind` saying \
                  whether that is a console or a project session; null when the user opened it \
                  themselves). `your_owner` marks the session that owns this one, and is also \
                  given at the top level. Use it to find who to share information with.",
    schema: || object_schema(json!({}), &[]),
};

const SHARE_INFO: ToolDef = ToolDef {
    name: "share_info",
    description: "Tell a running session of this project something it may find useful: what you \
                  found, decided or changed that touches its work. It is delivered marked as \
                  information from you, not as an instruction, and the session it is for takes \
                  work only from its owner. When that session reports to another session, that \
                  owner is sent a copy. Held until the user is done when the session is waiting \
                  for them. Refused for a session of another project, a console session, and \
                  this session itself.",
    schema: || {
        object_schema(
            json!({
                "session": { "type": "string", "description": "The session's id." },
                "text": {
                    "type": "string",
                    "description": "The information, in natural language.",
                },
            }),
            &["session", "text"],
        )
    },
};

/// What every project session may read of its own project's settings: only its own, whatever the
/// session's binding, and nothing it could change.
const GET_PROJECT: ToolDef = ToolDef {
    name: "get_project",
    description:
        "This project's settings in Octoboard, read-only: name, directory, source, remote \
                  URL (credentials removed), pin, tags, `default_agent`, `accounts` and `launch` \
                  (the model and effort sessions of each agent start with). Only this session's \
                  own project can be read.",
    schema: || object_schema(json!({}), &[]),
};

/// A project session that reports to another session has the one tool it answers through, the
/// two it shares information with, and the read of its project.
const BOUND_PROJECT_SESSION_TOOLS: &[ToolDef] =
    &[REPORT, LIST_PROJECT_SESSIONS, SHARE_INFO, GET_PROJECT];

/// A project session nobody dispatched drives its own project the way a console session drives
/// its console's, and what it starts reports back to it. `report` is announced too, though a call
/// is refused: the catalogue follows from role and binding, and the role text says not to call it.
const UNBOUND_PROJECT_SESSION_TOOLS: &[ToolDef] = &[
    ToolDef {
        name: "start_session",
        description: "Start a session in this project and hand it a task. Any available agent \
                      can be chosen, not only this session's own. Returns the new session's id. \
                      The brief is rendered into the session's opening prompt. The new session \
                      reports to this session, which is its owner, and you can instruct, read, \
                      archive and reopen it. A session started this way cannot start sessions \
                      of its own, and only this project is available.",
        schema: || start_session_schema(false),
    },
    SEND_MESSAGE,
    ToolDef {
        name: "get_session",
        description: "The session's status, plus a tail of what it has printed (a session that \
                      has ended keeps its last output; `null` if none was kept). Any session of \
                      this project can be read, but one with `yours: false` is not yours to \
                      act on, whether it is unbound (`owner: null`) or reports to a different \
                      session: leave it alone. `owner` says whose it is and `owner_kind` \
                      whether that is a console or a project session.",
        schema: || {
            object_schema(
                json!({
                    "session": { "type": "string", "description": "The session's id." },
                }),
                &["session"],
            )
        },
    },
    ARCHIVE_SESSION,
    REOPEN_SESSION,
    RENAME_SESSION,
    SET_SESSION_PINNED,
    REQUEST_CONSOLE_SESSION,
    REPORT,
    LIST_PROJECT_SESSIONS,
    SHARE_INFO,
    GET_PROJECT,
];

/// How a project session nobody dispatched reaches other projects: through a console session the
/// user agrees to start for it. Announced to a lead session too, which keeps the unbound set, and
/// refused there in prose. Its arguments are shaped like a report, since what it hands over reaches
/// the console session as the caller's first report.
const REQUEST_CONSOLE_SESSION: ToolDef = ToolDef {
    name: "request_console_session",
    description: "Ask for a console session, when this work needs sessions in other projects of \
                  this console: you can start sessions only in this project. The user is asked \
                  first, and the call waits for their answer, for several minutes at most. On \
                  approval Octoboard starts a console session in this console and binds this \
                  session to it: what you hand over here reaches it as your first report, it \
                  dispatches the work in the other projects, and from then on you report to it \
                  with the `report` tool, keeping and driving the sessions you started here. A \
                  refusal, or no answer in time, comes back as an error and changes nothing. \
                  Refused once this session is bound to a console session.",
    schema: || {
        object_schema(
            json!({
                "request": {
                    "type": "string",
                    "description": "What you need done that takes sessions in other projects, in \
                                    natural language: the outcome, and which projects or what \
                                    kind of work it involves where you know.",
                },
                "summary": {
                    "type": "string",
                    "description": "What you have done and found out so far that the console \
                                    session needs to know, in natural language.",
                },
                "open_items": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "The separate pieces of work you need done elsewhere, when it \
                                    divides into several. Empty when it does not.",
                },
            }),
            &["request", "summary"],
        )
    },
};

/// The schema of `start_session`. `with_project` is for the console session, which chooses a
/// project; a project session always starts in its own, so it is not asked.
fn start_session_schema(with_project: bool) -> Value {
    let mut properties = json!({
        "brief": {
            "type": "object",
            "description": "The task, in natural language per field.",
            "properties": {
                "goal": { "type": "string", "description": "The outcome to achieve." },
                "context": { "type": "string", "description": "Background and relevant leads." },
                "acceptance": { "type": "string", "description": "Criteria for being done." },
                "constraints": {
                    "type": "string",
                    "description": "What must not be touched, whether committing or \
                                    pushing is allowed, and so on.",
                },
            },
            "required": ["goal"],
        },
        "agent": {
            "type": "string",
            "enum": ["claude", "codex", "grok"],
            "description": "Overrides the agent for this one session.",
        },
        "account": {
            "type": "string",
            "description": format!(
                "Overrides the account for this one session: an account's id or name{}, or \
                 `default` for the agent's default account. It must belong to the session's agent.",
                if with_project { " (see `get_settings`)" } else { "" }
            ),
        },
        "model": {
            "type": "string",
            "description": "Overrides the model for this one session; blank is the agent's own \
                            default.",
        },
        "effort": {
            "type": "string",
            "description": "Overrides the reasoning effort for this one session; blank is the \
                            agent's own default.",
        },
    });
    if !with_project {
        return object_schema(properties, &["brief"]);
    }
    properties["project"] = json!({
        "type": "string",
        "description": "The project's id, or its name when that is unambiguous.",
    });
    object_schema(properties, &["project", "brief"])
}

fn tags_schema(description: &str) -> Value {
    json!({
        "type": "array",
        "items": { "type": "string" },
        "description": description,
    })
}

/// The `accounts` argument and result: an object keyed by agent. A project's setting for an agent
/// can also be `inherit`, which takes the console's; a console has no one to inherit from, so its
/// `null` is the default account too.
fn accounts_schema(project: bool) -> Value {
    let (values, description) = if project {
        (
            "`inherit` (the console's account), `default` (the agent's default account), or an \
             account's id or name (see `get_settings`)",
            "Each agent's account for sessions opened here, keyed by agent (`claude`, `codex`, \
             `grok`); an agent left out is left as it is.",
        )
    } else {
        (
            "`default` or `null` (the agent's default account), or an account's id or name (see \
             `get_settings`)",
            "Each agent's account for sessions opened in this console, keyed by agent (`claude`, \
             `codex`, `grok`); an agent left out is left as it is.",
        )
    };
    json!({
        "type": "object",
        "description": format!("{description} Each value is {values}. An account must belong to \
                                that agent."),
        "additionalProperties": { "type": if project { json!("string") } else { json!(["string", "null"]) } },
    })
}

/// The `launch` argument and result: each agent's model and reasoning effort, `null` or blank for
/// the agent's own default.
fn launch_schema() -> Value {
    json!({
        "type": "object",
        "description": format!(
            "Each agent's model and reasoning effort, keyed by agent (`claude`, `codex`, `grok`) \
             as `{{\"model\": ..., \"effort\": ...}}`; a field left out is left as it is, and \
             `null` or blank is the agent's own default. They must be ones the agent offers; \
             Claude Code's models are {}.",
            crate::agent_models::claude_model_ids()
                .iter()
                .map(|id| format!("`{id}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        "additionalProperties": {
            "type": "object",
            "properties": {
                "model": { "type": ["string", "null"] },
                "effort": { "type": ["string", "null"] },
            },
        },
    })
}

/// Wraps a property map as a tool input schema. Every tool takes an object, so the envelope is the
/// same each time and only the properties and the required list differ.
fn object_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a session is offered follows from its role and whether it is bound: only an unbound
    /// project session may start sessions, only a project session can report, and every project
    /// session may share information with its project.
    #[test]
    fn the_tools_offered_follow_from_role_and_binding() {
        let names = |role, bound| -> Vec<&'static str> {
            tools_for(role, bound)
                .iter()
                .map(|tool| tool.name)
                .collect()
        };
        assert_eq!(
            names(Role::Project, true),
            [
                "report",
                "list_project_sessions",
                "share_info",
                "get_project"
            ]
        );
        assert_eq!(
            names(Role::Project, false),
            [
                "start_session",
                "send_message",
                "get_session",
                "archive_session",
                "reopen_session",
                "rename_session",
                "set_session_pinned",
                "request_console_session",
                "report",
                "list_project_sessions",
                "share_info",
                "get_project"
            ]
        );
        assert_eq!(
            names(Role::Console, false),
            [
                "list_projects",
                "add_project",
                "update_project",
                "get_console",
                "update_console",
                "get_settings",
                "update_settings",
                "start_session",
                "send_message",
                "get_session",
                "rename_session",
                "set_session_pinned",
                "archive_session",
                "list_archived",
                "reopen_session",
                "show_page"
            ]
        );
        // Console sessions are never bound, so the flag changes nothing for them.
        assert_eq!(names(Role::Console, false), names(Role::Console, true));
        assert!(tool_by_name(Role::Console, false, "start_session").is_some());
        assert!(tool_by_name(Role::Console, false, "report").is_none());
        assert!(tool_by_name(Role::Project, true, "start_session").is_none());
        // Asking for a console session is for a session nobody dispatched; a console session
        // already is one.
        assert!(tool_by_name(Role::Project, true, "request_console_session").is_none());
        assert!(tool_by_name(Role::Console, false, "request_console_session").is_none());
        assert!(tool_by_name(Role::Project, false, "list_projects").is_none());
        // Settings and other projects are the console session's; a project session reads only its
        // own project, and a bound one cannot touch another session's title or pin.
        assert!(tool_by_name(Role::Project, false, "update_project").is_none());
        assert!(tool_by_name(Role::Project, false, "update_settings").is_none());
        assert!(tool_by_name(Role::Project, true, "rename_session").is_none());
        assert!(tool_by_name(Role::Console, false, "get_project").is_none());
        // Console sessions are outside every project, so they take no part in the exchange.
        assert!(tool_by_name(Role::Console, false, "share_info").is_none());
        assert!(tool_by_name(Role::Console, false, "list_project_sessions").is_none());
    }

    /// An unbound project session starts sessions only in its own project, so it is not asked
    /// which one.
    #[test]
    fn only_the_console_sessions_start_session_asks_for_a_project() {
        let schema = |tools: &'static [ToolDef]| {
            let tool = tools
                .iter()
                .find(|tool| tool.name == "start_session")
                .unwrap();
            (tool.schema)()
        };
        let console = schema(tools_for(Role::Console, false));
        let project = schema(tools_for(Role::Project, false));
        assert!(console["properties"]["project"].is_object());
        assert!(project["properties"]["project"].is_null());
        assert_eq!(project["required"], json!(["brief"]));
    }

    /// The prefix is the agent's own, and a role description that names the wrong one points the
    /// model at a tool it cannot see.
    #[test]
    fn the_qualified_name_follows_the_agent() {
        assert_eq!(
            qualified_tool_name(Agent::Claude, "report"),
            "mcp__octoboard__report"
        );
        assert_eq!(
            qualified_tool_name(Agent::Codex, "report"),
            "mcp__octoboard__report"
        );
        assert_eq!(
            qualified_tool_name(Agent::Grok, "report"),
            "octoboard__report"
        );
    }

    #[test]
    fn every_tool_announces_an_object_schema() {
        for (role, bound) in [
            (Role::Console, false),
            (Role::Project, true),
            (Role::Project, false),
        ] {
            for tool in tools_for(role, bound) {
                let schema = (tool.schema)();
                assert_eq!(schema["type"], "object", "{}", tool.name);
                assert!(schema["properties"].is_object(), "{}", tool.name);
                assert!(schema["required"].is_array(), "{}", tool.name);
            }
        }
    }
}
