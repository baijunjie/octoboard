//! The Octoboard MCP server: the orchestration and settings tools the console session drives
//! Octoboard with, the narrower set a project session that can own sessions drives its own project
//! with, the reporting tool a bound project session answers through, and the tools every session
//! shares information and reads the accounts' plan quotas through, and asks the user for a
//! session with no owner through.
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
//! apart. The console session's instruction file is held to it by a test in
//! [`console_instructions`], which fails when a tool it names is not in the catalogue. The names
//! an agent actually shows the model are prefixed per agent — `mcp__octoboard__x` on Claude Code
//! and Codex, `octoboard__x` on Grok (see "Across the three agents" in
//! `docs/agent-cli-reference.md`), which is why [`qualified_tool_name`] exists rather than the
//! role text spelling a prefix out.

mod account_switch;
mod args;
mod bindings;
pub mod console_instructions;
pub mod exec;
mod handoff;
mod handover;
mod quotas;
mod resume_request;
pub mod role;
mod settings;
pub mod stdio;

use serde_json::{json, Value};

use crate::protocol::{Agent, Role};

pub(crate) use bindings::{
    is_running, owner_from_now_on, owner_notice, running_or_leave, tell_former_owner,
    tell_now_or_leave, tell_released,
};

/// The key the adapters register the server under. It must not collide with a server the project
/// defines: on Claude Code a colliding key means the *project's* definition is silently never
/// spawned.
pub const SERVER_KEY: &str = "octoboard";

/// Who a session with the role of a bound one reports to at launch, as far as what the session is
/// told depends on it. A binding can change while the session runs, and the session is told of
/// that change in a message (`crate::mcp::bindings`), or, when it binds itself, in the call's
/// result; its role text, recorded once by some agents, is written so that it still holds
/// afterwards.
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
/// reports to another session and owns none. A lead session, bound to a console session while
/// keeping sessions of its own, does not, and keeps the unbound set. A console session is never
/// bound, and it is ignored for one. The flag follows the binding as it stands, so it can change
/// while a session runs — when it binds itself to a project session, or is handed to one — and the
/// daemon checks every call against the set as it is then (`exec::call`); the session's MCP child
/// is told with each answer and tells its agent the list changed (`stdio`).
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

/// Which language a session's name is written in, said on every argument that names one. Agents
/// otherwise take the language of what they read most — the project, its docs and code, the brief —
/// while the name is for the user, who may speak another.
const NAMING_LANGUAGE: &str = "Write it in the language the user converses with you in, not \
     that of the project, its docs and code, or a brief. If you take work from an owner session \
     rather than from the user directly, use the user's language as your owner passes it on, else \
     the language of your owner's messages.";

/// What every default agent and account setting says of the sessions a session starts itself:
/// which follow the starting session, and which the project's default agent. A macro so the read
/// tools' `&'static str` descriptions can take it with `concat!`.
macro_rules! started_by_a_session {
    () => {
        "A session a project session starts itself (`new_session`, or `request_session` in its \
         own console) runs on that session's own agent unless it chooses another, and one a \
         console session starts for a project (`new_session` or `request_session`) on the \
         project's default agent; a console session's request for a console session runs on the \
         requester's own agent. While `auto_select_account` is on, a left-out account is the one \
         of that agent whose weekly quota resets soonest and that can still open a session. When \
         none does, the starting session's own account is used only when it is the same agent \
         and can still open a session; otherwise the account is the project's for the agent, \
         else the console's. A console session names another agent or account only when the user \
         asked for it."
    };
}

/// What every description of a session says of its `context`. A macro so the read tools'
/// `&'static str` descriptions can take it with `concat!`.
macro_rules! session_context_field {
    () => {
        "`context` is how much of its context window the session has used: `tokens`, plus \
         `window` and `used_percent` once the window is known. It is null while no size is \
         known: before the session's first report, and after a compaction until its next \
         request."
    };
}

const STARTED_BY_A_SESSION: &str = started_by_a_session!();

const CONSOLE_SESSION_TOOLS: &[ToolDef] = &[
    ToolDef {
        name: "list_projects",
        description: concat!(
            "List this console's projects: name, host, directory, source, remote URL \
             (credentials removed), pin, the tags and the description (what the project \
             is for) the user gave each, the settings (`default_agent`, `accounts`, \
             `launch`; see `update_project`), whether the \
             user has `trust_consent`ed to Octoboard pressing the agent's folder-trust \
             confirmation (read-only), and the sessions currently running in each — \
             every session of the console, whichever session owns it. Each carries `owner`, the \
             session it reports to, `owner_kind` (a `console` or a `project` session) \
             and `yours`; a project session with `owner: null` is unbound, which the user \
             opened themselves and kept outside the orchestration. A session with \
             `yours: false` is not this caller's to drive — it is unbound, or reports \
             to a different session — leave it alone. An idle session with a \
             non-empty `background_tasks` takes messages like any idle session but still \
             has work running in the background. \
             A non-null `writes_held` says why messages for a running session are \
             queued rather than written into it, whatever its `status` reads; \
             `queued_messages` counts them. ",
            session_context_field!()
        ),
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
                                        associated; defaults to the directory's own name. When given, at most \
                                        100 Unicode scalar values after trimming.",
                    },
                    "default_agent": {
                        "type": "string",
                        "enum": ["claude", "codex", "grok"],
                        "description": format!(
                            "The agent sessions in this project use by default. When omitted, \
                             the agent the directory is set up for (from its top-level marker \
                             files) if exactly one is found and not known to be missing from \
                             PATH; otherwise the project follows the console's project session \
                             agent (else its `default_agent`). {STARTED_BY_A_SESSION}"
                        ),
                    },
                    "description": {
                        "type": "string",
                        "description": "What the project is for, in free text. Blank is none. Only applies \
                                        when a single project is being associated. At most 2,000 Unicode \
                                        scalar values after trimming.",
                    },
                    "tags": tags_schema("The project's tags."),
                    "accounts": accounts_schema(true),
                    "launch": launch_schema("the project's sessions"),
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
                    "name": {
                        "type": "string",
                        "description": "The display name. Not blank. Changed text is limited to 100 \
                                        Unicode scalar values after trimming; unchanged legacy text \
                                        is accepted even when longer.",
                    },
                    "description": {
                        "type": ["string", "null"],
                        "description": "What the project is for, in free text; `null` or blank \
                                        clears it. Changed text is limited to 2,000 Unicode scalar values \
                                        after trimming; unchanged legacy text is accepted even when longer.",
                    },
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
                        "description": format!(
                            "The agent sessions in this project use by default; `null` goes back \
                             to following the console's project session agent (else its \
                             `default_agent`). {STARTED_BY_A_SESSION}"
                        ),
                    },
                    "accounts": accounts_schema(true),
                    "launch": launch_schema("the project's sessions"),
                }),
                &["project"],
            )
        },
    },
    ToolDef {
        name: "get_console",
        description: concat!(
            "This console's own settings: `name`, `default_agent` (the agent of console \
             sessions), `project_session_agent` (the agent a project session without its own \
             default uses; `null` follows `default_agent`), `accounts`, `launch` (the model and \
             effort of console sessions like this one), `project_session_launch` (the model and \
             effort a project session takes when neither it nor its project names one), whether \
             it has an avatar (`has_icon`), and its working directory (`workdir`, read-only). \
             Only the console this session belongs to can be read. ",
            started_by_a_session!()
        ),
        schema: || object_schema(json!({}), &[]),
    },
    ToolDef {
        name: "update_console",
        description: "Change this console's own settings; what is omitted is left as it is. \
                      Returns them as `get_console` gives them. Sessions already running keep \
                      what they were started with: a new account reaches only console sessions \
                      opened afterwards, and a new model or effort reaches this console \
                      session on its next launch unless it was given its own; a new \
                      `project_session_launch` reaches project sessions at their next launch, \
                      unless their project or the session sets its own. Only this \
                      console can be changed, and not its avatar. These are the user's \
                      settings: change one only when they asked you to.",
        schema: || {
            object_schema(
                json!({
                    "name": { "type": "string", "description": "The display name. Not blank." },
                    "default_agent": {
                        "type": "string",
                        "enum": ["claude", "codex", "grok"],
                        "description": format!(
                            "The agent console sessions use by default. {STARTED_BY_A_SESSION}"
                        ),
                    },
                    "project_session_agent": {
                        "type": ["string", "null"],
                        "enum": ["claude", "codex", "grok", null],
                        "description": format!(
                            "The agent a project session uses when its project has no default \
                             agent of its own. `null` makes it follow `default_agent`. \
                             {STARTED_BY_A_SESSION}"
                        ),
                    },
                    "accounts": accounts_schema(false),
                    "launch": launch_schema("this console's console sessions"),
                    "project_session_launch": launch_schema(
                        "this console's project sessions whose project and own choice leave them \
                         unset",
                    ),
                }),
                &[],
            )
        },
    },
    ToolDef {
        name: "get_settings",
        description: "The application-wide settings: `auto_pull_repositories`, \
                      `auto_delete_unresumable_archives`, `auto_select_account`, \
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
                    "auto_delete_unresumable_archives": {
                        "type": "boolean",
                        "description": "Whether archived sessions whose agent conversation is \
                                        gone are deleted automatically (pinned and locked ones \
                                        are kept).",
                    },
                    "auto_select_account": {
                        "type": "boolean",
                        "description": "When on, `new_session`, `request_session` and \
                                        `handoff_session` whose account was left out use the \
                                        account of that session's agent whose weekly quota \
                                        resets soonest, among accounts that can still open a \
                                        session. When none does, the account is the one used \
                                        while the setting is off.",
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
        name: "new_session",
        description: "Start a session in one of this console's projects and hand it a task. It is \
                      yours for its whole life: it reports to this console session, its owner, and \
                      you instruct it with `send_message`, archive it once it is done and reopen \
                      it with `reopen_session` when it has another task. Needs no approval. \
                      Returns the new session's id. The brief is rendered into the session's \
                      opening prompt; `title` names the session in the language the user converses \
                      with you in. It runs on the project's default agent. While \
                      `auto_select_account` is on, a left-out account is the one of the session's \
                      agent whose weekly quota resets soonest and that can still open a session. \
                      When none does, your own account is used only when the session's agent is \
                      your own and that account can still open a session; any other takes the \
                      account its project, else its console, sets. Name another only when the user \
                      asked for it. Use this when you should keep directing the session and \
                      receiving its reports. To hand your own work to a peer in the next session, \
                      use `request_session`; if the user only wants handover notes and an end, \
                      leave the notes and end. Ask the user if their intent is unclear.",
        schema: || new_session_schema(true),
    },
    SEND_MESSAGE,
    ToolDef {
        name: "get_session",
        description: concat!(
            "The session's status, plus a tail of what it has printed (a session that \
             has ended keeps its last output; `null` if none was kept). Use it to \
             follow up; a session that is waiting for the user must be left alone until \
             they have answered. Any session can be read, of any console, but one with \
             `yours: false` is \
             not this caller's to act on, whether it is unbound (`owner: null` on a \
             project session) or reports to a different session, console or project: \
             leave it alone. `owner` says whose it is, `owner_kind` whether that is a \
             console or a project session, and `role` whether the session itself is a \
             console session. An idle session with a non-empty `background_tasks` takes \
             messages like any idle session but still has work running in the background. \
             A non-null `writes_held` says why messages for a running session are \
             queued rather than written into it, whatever its `status` reads; \
             `queued_messages` counts them. ",
            session_context_field!()
        ),
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
    ARCHIVE_SELF,
    ToolDef {
        name: "list_archived",
        description: concat!(
            "List the archived sessions of one project, whichever session owns \
             them. Each carries its `owner`, `owner_kind` and `yours`, so an earlier \
             one of yours can be reopened instead of starting over. A session with \
             `yours: false` is somebody else's: leave it alone. ",
            session_context_field!()
        ),
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
    HANDOFF_SESSION,
    HANDOVER_BRANCHES,
    SWITCH_SESSION_ACCOUNT,
    TRANSFER_SESSION,
    RELEASE_SESSION,
    REQUEST_SESSION,
    REQUEST_RESUME_SESSION,
    LIST_SESSIONS,
    SHARE_INFO,
    GET_ACCOUNT_QUOTAS,
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
                  the session is waiting for them. The answer says if it was queued, or written \
                  but left unsent by the session's agent. Refused for a session that is not \
                  yours (`yours: false`): it is somebody else's, to be left alone.",
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
                  (`yours: false`): it is somebody else's, to be left alone. The git branches the \
                  sessions archived created, and the linked worktrees that have them checked \
                  out, are removed once their processes have ended, wherever git can do it without \
                  force; `branches_left` in the answer names any that stay, and why.",
    schema: || {
        object_schema(
            json!({
                "session": { "type": "string", "description": "The session's id." },
            }),
            &["session"],
        )
    },
};

const ARCHIVE_SELF: ToolDef = ToolDef {
    name: "archive_self",
    description: "End this session's own process and archive it, once its work is finished. \
                  Meant for a session that reports to nobody; one that reports to an owner (a \
                  lead session) wraps up with `report` instead and is refused here. Refused \
                  while any session under this one is not archived, running or interrupted: an \
                  owner archives its sessions once their tasks are done, so an unarchived one \
                  means its task is not finished. Finish it and archive it with \
                  `archive_session` first; for one bound to a lead session, wait for the lead \
                  session to finish it or archive the lead session with `archive_session`. \
                  Refused while this session or a session under it is locked. The call is \
                  answered before the process ends. The git branches this session created, and \
                  the linked worktrees that have them checked out, are removed once it has ended, \
                  wherever git can do it without force; `branches_left` in the answer names any \
                  that will stay, and why.",
    schema: || object_schema(json!({}), &[]),
};

const RENAME_SESSION: ToolDef = ToolDef {
    name: "rename_session",
    description: "Change the title of a session of yours, in the language the user converses with \
                  you in. Refused for a session that is not yours (`yours: false`): it is \
                  somebody else's, to be left alone.",
    schema: || {
        object_schema(
            json!({
                "session": { "type": "string", "description": "The session's id." },
                "title": {
                    "type": "string",
                    "description": format!("The new title. Not blank. {NAMING_LANGUAGE}"),
                },
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

/// How an owner moves the responsibility for git branches between sessions, without the user: the
/// branch records change hands, and no session is replaced.
const HANDOVER_BRANCHES: ToolDef = ToolDef {
    name: "handover_branches",
    description: "Move the responsibility for git branches from one session to another; it only \
                  changes which session is responsible for the branches and replaces no \
                  session (to move a whole session to another agent, use `handoff_session`). \
                  `from` and `to` are each this session or a session of yours (`yours: true`); \
                  leave either out for this session (a console session is responsible for no \
                  branch, so it names both). `from` may be archived, to recover the branches an \
                  archived session left behind (`branches_left`); `to` may not. Use \
                  it to take over a branch a session of yours developed, to give a branch of yours \
                  to one of your sessions, or to give one session's branch to another. Needs no \
                  approval. Each branch must be one `from` is responsible for, and `from` and `to` \
                  must be in projects of the same repository. Refused, changing nothing, while \
                  `from` is another session that is working, starting or waiting for the user; \
                  an interrupted or archived one can give branches up. Nothing changes in git or \
                  on disk: the answer lists, per branch, the worktree it is checked out in \
                  (`worktree_status` says `checked_out`, `gone`, `none` or `unknown` when it \
                  could not be read) and whether that holds uncommitted changes, which stay for \
                  the new session to deal with. The sessions concerned other than this one are \
                  told. When a session is archived, the cleanup of its branches follows whoever \
                  is responsible for them by then.",
    schema: || {
        object_schema(
            json!({
                "branches": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Local branches of the repository, by name. Each must be a \
                                    branch `from` is responsible for.",
                },
                "from": {
                    "type": "string",
                    "description": "The session handing the branches over: this session (the \
                                    default) or a session of yours, archived or not.",
                },
                "to": {
                    "type": "string",
                    "description": "The session taking the branches: this session (the \
                                    default) or a session of yours that is not archived.",
                },
            }),
            &["branches"],
        )
    },
};

/// How an owner moves a session of its own to another agent, without the user: the receiving
/// session takes the original's whole place and stays the owner's, and the original is archived.
const HANDOFF_SESSION: ToolDef = ToolDef {
    name: "handoff_session",
    description: "Hand one of your sessions (`yours: true`) off to a new session of another \
                  agent, to carry its work on, for instance when its account has run out of \
                  quota. The conversation does not move, because the agents' records cannot \
                  resume one another: the new session reads a package built from the original's \
                  conversation (every user message and reply, without tool output), its latest \
                  todo list and the files it changed. What moves is what only Octoboard holds: \
                  the branches the original is responsible for and the sessions it owns now \
                  belong to the new session, which reports to you like the original did, and the \
                  original is archived. Needs no approval. Refused, changing nothing, while the \
                  session is in the middle of a turn, and for the agent the session already runs \
                  on (switching it to another account of that agent keeps the conversation). \
                  Returns the new session's id, the branches that moved and the sessions it now \
                  owns. Pass `account` only when the user asked for a particular one.",
    schema: || {
        object_schema(
            json!({
                "session": {
                    "type": "string",
                    "description": "The id of the session of yours to hand off.",
                },
                "agent": {
                    "type": "string",
                    "enum": ["claude", "codex", "grok"],
                    "description": "The agent the new session runs on; not the session's own.",
                },
                "account": {
                    "type": "string",
                    "description": "The new session's account: an account's id or name, or \
                                    `default` for the agent's default account. It must belong \
                                    to `agent`. Left out while `auto_select_account` is on, \
                                    it is the account of `agent` whose weekly quota resets \
                                    soonest and that can still open a session. When none does, \
                                    or while it is off, it is your own account when `agent` is \
                                    your own agent and that account's quota is not used up; \
                                    otherwise the account the session's project (else its \
                                    console) sets for `agent`.",
                },
                "title": {
                    "type": "string",
                    "description": format!(
                        "The new session's name in Octoboard's session list. Left out or blank, \
                         it is the name the original is shown under, its task title included. \
                         {NAMING_LANGUAGE}"
                    ),
                },
                "model": {
                    "type": "string",
                    "description": "The new session's model; blank is the agent's own default. \
                                    Left out, it takes the model its project (else its console) \
                                    sets for sessions.",
                },
                "effort": {
                    "type": "string",
                    "description": "The new session's reasoning effort; blank is the agent's \
                                    own default. Left out, it takes the effort its project \
                                    (else its console) sets for sessions.",
                },
                "note": {
                    "type": "string",
                    "description": "What to tell the new session beyond the package, in natural \
                                    language: where the work stands and what to do first.",
                },
            }),
            &["session", "agent"],
        )
    },
};

/// How an owner moves a session of its own to another account of its agent, keeping the
/// conversation, as the user's Switch account does.
const SWITCH_SESSION_ACCOUNT: ToolDef = ToolDef {
    name: "switch_session_account",
    description: "Move one of your sessions (`yours: true`) to another account of the agent it \
                  runs on, for instance when its account has run out of quota. The session \
                  keeps its conversation: its process is ended, the conversation is copied to \
                  the account and the session is relaunched resuming it. It is not sent a \
                  message for this: it comes back awaiting instructions, and a turn in progress \
                  is cut off, so send it the next step with `send_message`. Needs no approval. \
                  Refused, changing nothing, for an account of another agent, the account the session is \
                  on already, and an archived session. Returns the session with its new \
                  account.",
    schema: || {
        object_schema(
            json!({
                "session": {
                    "type": "string",
                    "description": "The id of the session of yours to move.",
                },
                "account": {
                    "type": "string",
                    "description": "The account to move it to: an account's id or name, or \
                                    `default` for the agent's default account. It must belong \
                                    to the session's own agent.",
                },
            }),
            &["session", "account"],
        )
    },
};

const REPORT: ToolDef = ToolDef {
    name: "report",
    description: "Report the round of work back to your owner, the session this one is bound to: \
                  a console session or a project session. `summary` is natural language; `status` \
                  and `open_items` are what your owner acts on. With `done` and no open items the \
                  session is archived once the report is delivered, and the git branches it \
                  created, with the linked worktrees that have them checked out, are removed once \
                  it has ended wherever git can do it without force (the answer names any that \
                  will stay); anything else leaves it running and awaiting instructions. Refused \
                  while this session is unbound: nobody owns it but the user.",
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
                                    be, `needs_decision` when your owner has to choose before you \
                                    can go on.",
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

/// What every session, of whatever kind, may do with every other running session: look at them,
/// and tell them things. Kept apart from `send_message` so that a message arriving in a session is
/// framed by which tool sent it: an instruction comes from its owner, and information from anyone.
const LIST_SESSIONS: ToolDef = ToolDef {
    name: "list_sessions",
    description: concat!(
        "List the running sessions other than this one, in every console and project, \
         console sessions included: id, title, agent, status, `role` (a console or a \
         project session), `console`, `project` (null for a console session), and \
         `owner` (the session it reports to, `owner_kind` saying whether that is a \
         console or a project session; null when nobody owns it and it reports to the \
         user). `yours` marks a session that reports to you, and `your_owner` the \
         session you report to, which is also given at the top level. Use it to find \
         who to share information with, or an owner to hand a session to or bind to. An \
         idle session with a non-empty `background_tasks` takes messages like any idle \
         session but still has work running in the background. \
         A non-null `writes_held` says why messages for a running session are \
         queued rather than written into it, whatever its `status` reads; \
         `queued_messages` counts them. ",
        session_context_field!()
    ),
    schema: || object_schema(json!({}), &[]),
};

const SHARE_INFO: ToolDef = ToolDef {
    name: "share_info",
    description: "Tell a running session something it may find useful: what you found, decided \
                  or changed that touches its work. Any session can be told, in any project or \
                  console, console sessions included; no session needs to own the other. It is \
                  delivered marked as information from you, not as an instruction, and the \
                  session it is for takes work only from its owner. When that session reports to \
                  another session, that owner is sent a copy. Held until the user is done when \
                  the session is waiting for them. Refused for this session itself, and for a \
                  session with no process running.",
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
    description: concat!(
        "This project's settings in Octoboard, read-only: name, directory, source, remote URL \
         (credentials removed), pin, tags, description, `default_agent`, `accounts` and `launch` \
         (the model and effort sessions of each agent start with; a value left unset falls back \
         to the console's `project_session_launch`). Only this session's own project can be \
         read. ",
        started_by_a_session!()
    ),
    schema: || object_schema(json!({}), &[]),
};

/// How an owner — a console session, or a project session that can own sessions — passes one of
/// its sessions to another owner, without the user: the session is its to pass, and the rules a
/// binding keeps are checked as for any binding.
const TRANSFER_SESSION: ToolDef = ToolDef {
    name: "transfer_session",
    description: "Hand one of your sessions (`yours: true`) to another owner, to pass its task \
                  over: from then on it reports to that owner, which drives it, and it is no \
                  longer yours. Needs no approval. The new owner is a console session of this \
                  console, or a project session of the session's own project that can own \
                  sessions (unbound, or bound to a console session while keeping sessions of its \
                  own); a session that has sessions of its own can go only to a console session. \
                  The new owner must be running. The session is told who its owner now is, when it \
                  next starts if it is not running, and the new owner is sent `note`.",
    schema: || {
        object_schema(
            json!({
                "session": {
                    "type": "string",
                    "description": "The id of the session of yours to hand over.",
                },
                "to": { "type": "string", "description": "The new owner's session id." },
                "note": {
                    "type": "string",
                    "description": "The handover, in natural language, for the new owner: what \
                                    the session is doing, where it stands and what is left.",
                },
            }),
            &["session", "to", "note"],
        )
    },
};

/// How an owner — a console session, or a project session that can own sessions — lets one of its
/// sessions go, without the user: the session is its to release, and becomes the user's own.
const RELEASE_SESSION: ToolDef = ToolDef {
    name: "release_session",
    description: "Release one of your sessions (`yours: true`) from its binding to you, when it \
                  is not yours to drive any more: it has no owner afterwards and is the user's \
                  own, taking work only from the user in its terminal. Needs no approval. It \
                  stops reporting to anyone, becomes an ordinary unbound session that can start \
                  and drive sessions of its own, and is no longer archived with you or by its \
                  own `done` report. A lead session keeps the sessions it started, and the \
                  branches the session is responsible for stay with it. A session is told it \
                  has no owner now, when it next starts if it is not running, which replaces \
                  what it was told of its owner when it was started. An optional `note` is \
                  passed on to it as your parting words, so say there what it should know to \
                  carry on without you.",
    schema: || {
        object_schema(
            json!({
                "session": {
                    "type": "string",
                    "description": "The id of the session of yours to release.",
                },
                "note": {
                    "type": "string",
                    "description": "Optional. A parting note for the session, passed on as \
                                    yours when it is told it has no owner now: what it should \
                                    know to carry on without you. A blank note is ignored, and \
                                    one over 4000 characters is refused.",
                },
            }),
            &["session"],
        )
    },
};

/// How an unbound project session becomes one of another session's, without the user: it puts
/// itself under that owner, and the rules a binding keeps are checked as for any binding.
const BIND_TO_SESSION: ToolDef = ToolDef {
    name: "bind_to_session",
    description: "Bind this session, while it is unbound, to an owner and become one of its \
                  sessions: from then on you report to it with `report` and take work from it. \
                  Needs no approval. The owner is a console session of this console — you then \
                  keep the sessions you started and go on driving them — or a project session of \
                  this project that can own sessions; only a session with no sessions of its own \
                  may join one of those, and it can no longer start or drive sessions afterwards. \
                  The owner must be running, and is sent `summary` as your first report. The \
                  result says which tools you have from then on. Refused once this session is \
                  bound. To tell a session something without binding, use `share_info`.",
    schema: || {
        object_schema(
            json!({
                "owner": { "type": "string", "description": "The owner's session id." },
                "summary": {
                    "type": "string",
                    "description": "What you are doing and what you need from the owner, in \
                                    natural language. It reaches the owner as your first report.",
                },
            }),
            &["owner", "summary"],
        )
    },
};

/// How every session, of whatever kind, has a session started that is nobody's: through a
/// confirmation the user gives. The brief and the overrides are `new_session`'s, and are checked
/// the same way before the user is asked.
const REQUEST_SESSION: ToolDef = ToolDef {
    name: "request_session",
    description: "Ask the user to start a session that is nobody's — neither yours nor your \
                  owner's: it reports to the user, and you and it can only exchange information \
                  (`share_info`). It can be a project session in any project of any console, or a \
                  console session. A session you want to drive yourself is one `new_session` \
                  starts, where you have it. The user is asked first, and the call waits for their \
                  answer, for several minutes at most — unless the user has chosen to have such \
                  requests approved automatically, in which case the result says so and the call \
                  does not wait. On approval Octoboard starts it with your brief as its opening \
                  prompt; follow the user's direction on whether your own work ends after the \
                  handover. Use this for a peer to continue your current work in the next session. \
                  If the user only wants handover notes and an end, leave the notes and end \
                  without starting a session. If their intent is unclear, ask before acting. A \
                  refusal, or no answer in time, comes back as an error and changes nothing. When \
                  handing over work on a branch you are responsible for, pass it in `branches`: \
                  the user is shown them, and on approval the new session becomes responsible for \
                  them in your place. Leave `branches` out when you are only working on whatever \
                  branch is checked out. `title` names the session in the language the user \
                  converses with you in. In your own console it runs on your own agent, or on the \
                  project's default agent when a project is named from a console session. While \
                  `auto_select_account` is on, a left-out account is the one of the session's \
                  agent whose weekly quota resets soonest and that can still open a session. When \
                  none does, your own account is used in your own console only for your own agent \
                  when that account can still open a session; any other session takes the account \
                  its project, else its console, sets. Otherwise a session in your own console of \
                  your own agent runs on your own account, unless that account's quota is used up, \
                  and any other takes the account its project, else its console, sets. Pass an \
                  account only when the user asked for one.",
    schema: request_session_schema,
};

/// How every session, of whatever kind, has a conversation resumed that it does not own: through a
/// confirmation the user gives, with what the new-session dialog's Resume conversation tab gives
/// the user and the same limits.
const REQUEST_RESUME_SESSION: ToolDef = ToolDef {
    name: "request_resume_session",
    description: "Ask the user to resume a conversation by its id — the agent's own conversation \
                  id (a UUID, in any spelling), not an Octoboard session id — as the user does in \
                  the new-session dialog's Resume conversation tab, with the same limits. If a \
                  session of the project holds the conversation, it is brought back: an archived \
                  session is reopened (and renamed, with `title`), and an interrupted one \
                  resumed, continuing its conversation, with the owner it has (it may come back \
                  with its archived owners). It stays that owner's, or the user's: you do not own \
                  it and cannot drive it. If a session that is already running holds it there is \
                  nothing to resume, and the call is refused. If no session holds the \
                  conversation but an agent's own record of it is on this machine, a new session \
                  that is nobody's is opened on it, on the agent and account that hold the \
                  record, and it reports to the user. A conversation held by a session of another \
                  project, or by a console session, is refused. Where you have `reopen_session`, \
                  it reopens only sessions that are yours and needs no approval; this can resume \
                  any session. No text can be passed along, so afterwards `share_info` tells the \
                  session what you need. The user is asked first, and the call waits for their \
                  answer, for several minutes at most — unless the user has chosen to have such \
                  requests approved automatically, in which case the result says so and the call \
                  does not wait. Use it only when the user asked for that conversation to be \
                  resumed, or you have been told to carry on work whose conversation they named. \
                  A refusal, or no answer in time, comes back as an error and changes nothing. If \
                  their intent is unclear, ask before acting.",
    schema: request_resume_session_schema,
};

/// What every session may read of the plan quota each account has left, to choose an account or
/// an agent before one runs dry.
const GET_ACCOUNT_QUOTAS: ToolDef = ToolDef {
    name: "get_account_quotas",
    description: "How much plan quota each account has left, read-only: every account of every \
                  agent, the agent's default account (`account: \"default\"`) and the stored \
                  ones, with its `account_name`, `plan`, and its quota `windows` (`kind` \
                  `session` is the 5-hour window, `weekly` the weekly one across all models, \
                  `weekly_model` a weekly window of the model named by `label`, `credit_period` \
                  a pool of credits over a billing period). All values are percentages of a \
                  window: `used_percent` and `remaining_percent`, with `resets_at` \
                  (milliseconds since the epoch) and `window_minutes`; no absolute token or \
                  dollar amount is available. `exhausted` says the account is used up and cannot \
                  open a session until it resets, and `exhausted_until` is when (milliseconds \
                  since the epoch; null when it is not exhausted or the reset time is unknown). \
                  Only agents a session could be opened on are listed: one that is not installed \
                  or not added lists no accounts. These are the readings Octoboard last took, \
                  refreshed about every five minutes, and this call does not take a new one: \
                  `fetched_at` says how old a reading is, `unavailable` gives the reason a \
                  reading is missing or could not be refreshed (an account not read yet has \
                  `fetched_at: null`, as has one of an agent not detected yet). Use it to check \
                  an account before relying on it, or to pick the account for \
                  `switch_session_account` or `handoff_session`.",
    schema: || {
        object_schema(
            json!({
                "agent": {
                    "type": "string",
                    "enum": ["claude", "codex", "grok"],
                    "description": "List only this agent's accounts. Left out, every agent's.",
                },
            }),
            &[],
        )
    },
};

/// A project session that reports to another session has the one tool it answers through, the
/// two it shares information with, the read of its project, the read of the accounts' quotas, and
/// the two requests the user approves: for a session with no owner, and for a conversation to be
/// resumed.
const BOUND_PROJECT_SESSION_TOOLS: &[ToolDef] = &[
    REPORT,
    LIST_SESSIONS,
    SHARE_INFO,
    GET_PROJECT,
    GET_ACCOUNT_QUOTAS,
    REQUEST_SESSION,
    REQUEST_RESUME_SESSION,
];

/// A project session that can own sessions — unbound, or a lead session — drives its own project
/// the way a console session drives its console's, and what it starts reports back to it.
/// `report` is announced too, though a call is refused while the session is unbound: the
/// catalogue does not change when an unbound session binds itself to a console session. For the
/// same reason `archive_self` is announced to a lead session and refused at the call.
const UNBOUND_PROJECT_SESSION_TOOLS: &[ToolDef] = &[
    ToolDef {
        name: "new_session",
        description: "Start a session in this project and hand it a task. It is yours for its \
                      whole life: it reports to this session, its owner, and you instruct it with \
                      `send_message`, archive it once it is done and reopen it with \
                      `reopen_session` when it has another closely related task and room for it. \
                      Needs no approval. It runs on your own agent. While `auto_select_account` is \
                      on, a left-out account is the one of that agent whose weekly quota resets \
                      soonest and that can still open a session. When none does, your own account \
                      is used only when it is your own agent and can still open a session; \
                      otherwise the account is the project's, else the console's. Name another \
                      account only when the user asked for it, or when it is the way to reach \
                      another agent. Name another agent only when the user asked for it or when \
                      you judge it fits the work better and an account of it can open a session. \
                      Returns the new session's id. The brief is rendered into the session's \
                      opening prompt; `title` names the session in the language the user converses \
                      with you in. A session started this way cannot start sessions of its own, \
                      and only this project is available; a session elsewhere, or one that should \
                      not report to you, is one `request_session` asks the user for. To give the \
                      new session a branch you are responsible for, pass it in `branches`; \
                      `handover_branches` moves the branches of any session of yours later. Use \
                      this only when you should keep directing the session and receiving its \
                      reports. To hand your own work to a peer in the next session, use \
                      `request_session`; if the user only wants handover notes and an end, leave \
                      the notes and end. Ask the user if their intent is unclear.",
        schema: || new_session_schema(false),
    },
    SEND_MESSAGE,
    ToolDef {
        name: "get_session",
        description: concat!(
            "The session's status, plus a tail of what it has printed (a session that \
             has ended keeps its last output; `null` if none was kept). Any session can \
             be read, in any project or console, but one with `yours: false` is not \
             yours to act on, whether nobody owns it (`owner: null`) or it reports to a \
             different session: leave it alone. `owner` says whose it is and \
             `owner_kind` whether that is a console or a project session. An idle \
             session with a non-empty `background_tasks` takes messages like any idle \
             session but still has work running in the background. \
             A non-null `writes_held` says why messages for a running session are \
             queued rather than written into it, whatever its `status` reads; \
             `queued_messages` counts them. ",
            session_context_field!()
        ),
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
    ARCHIVE_SELF,
    REOPEN_SESSION,
    RENAME_SESSION,
    SET_SESSION_PINNED,
    HANDOFF_SESSION,
    HANDOVER_BRANCHES,
    SWITCH_SESSION_ACCOUNT,
    TRANSFER_SESSION,
    RELEASE_SESSION,
    BIND_TO_SESSION,
    REQUEST_SESSION,
    REQUEST_RESUME_SESSION,
    REPORT,
    LIST_SESSIONS,
    SHARE_INFO,
    GET_PROJECT,
    GET_ACCOUNT_QUOTAS,
];

/// The schema of `request_session`: `new_session`'s, and where the new session is to be.
fn request_session_schema() -> Value {
    let mut schema = new_session_schema(false);
    schema["properties"]["kind"] = json!({
        "type": "string",
        "enum": ["project", "console"],
        "description": "`project` for a session in a project, `console` for a console session.",
    });
    schema["properties"]["project"] = json!({
        "type": "string",
        "description": "For `project`: the project's id, or its name when that is unambiguous. \
                        Defaults to this session's own project; a console session has to give \
                        it.",
    });
    schema["properties"]["console"] = json!({
        "type": "string",
        "description": "For `console`: the console's id, or its name when that is unambiguous. \
                        Defaults to this session's own console.",
    });
    schema["properties"]["agent"]["description"] = json!(
        "The agent the session runs. Left out, it is your own agent for a session in your own \
         console, or the project's default agent when you are a console session and name a \
         project, and the agent the other console (and its project) sets for one elsewhere. \
         Pass one only when the user asked for it."
    );
    schema["properties"]["account"]["description"] = json!(
        "The account the session runs on: an account's id or name, or `default` for the agent's \
         default account. It must belong to the session's agent. While `auto_select_account` is \
         on, a left-out account is the one of the session's agent whose weekly quota resets \
         soonest, among accounts that can still open a session: one a used-up quota keeps from \
         opening is not chosen. Your own account, for a session in your own console of your own \
         agent whose quota is not used up, or the account its project (else its console) sets, \
         is used only when none does. Otherwise a session in your own console of your own \
         agent runs on your own account, unless that account's quota is used up, and any other \
         takes the account its project (else its console) sets for the agent. Pass one only when \
         the user asked for it."
    );
    schema["properties"]["branches"] = json!({
        "type": "array",
        "items": { "type": "string" },
        "description": "For `project`: local branches of the project's repository to hand over \
                        to the new session, by name. Each must be a branch you are responsible \
                        for: one you created or were handed. Pass them when you are handing over \
                        the work on those branches; leave this out when you are only working on \
                        the checked-out branch, or when there is nothing to hand over.",
    });
    schema["required"] = json!(["kind", "brief"]);
    schema
}

/// The schema of `request_resume_session`.
fn request_resume_session_schema() -> Value {
    object_schema(
        json!({
            "conversation": {
                "type": "string",
                "description": "The agent's conversation id: a UUID, in any spelling (the \
                                Session ID the agent shows). An Octoboard session id is not \
                                one.",
            },
            "project": {
                "type": "string",
                "description": "The project the conversation is resumed in: its id, or its name \
                                when that is unambiguous. Defaults to this session's own \
                                project; a console session has to give it. A conversation held \
                                by a session of another project is refused.",
            },
            "title": {
                "type": "string",
                "description": format!(
                    "The name of the session. For a new session opened on the agent's record, \
                     left out it is named for its project. For an archived session that holds \
                     the conversation, it is renamed to this after it is reopened. Refused for \
                     an interrupted session, which keeps its name. {NAMING_LANGUAGE}"
                ),
            },
            "model": {
                "type": "string",
                "description": "Only when no session holds the conversation, which opens a new \
                                session: overrides its model; blank is the agent's own default. \
                                A session that holds the conversation keeps its own, and the \
                                call is refused if this is given.",
            },
            "effort": {
                "type": "string",
                "description": "Only when no session holds the conversation, which opens a new \
                                session: overrides its reasoning effort; blank is the agent's \
                                own default. A session that holds the conversation keeps its \
                                own, and the call is refused if this is given.",
            },
        }),
        &["conversation"],
    )
}

/// The schema of `new_session`. `with_project` is for the console session, which chooses a
/// project; a project session always starts in its own, so it is not asked.
fn new_session_schema(with_project: bool) -> Value {
    let mut properties = json!({
        "brief": {
            "type": "object",
            "description": "The task, in natural language per field.",
            "properties": {
                "goal": { "type": "string", "description": "The outcome to achieve." },
                "context": {
                    "type": "string",
                    "description": "Background and relevant leads. Include which language the \
                                    user converses with you in, so the session can name any \
                                    sessions it starts in that language.",
                },
                "acceptance": { "type": "string", "description": "Criteria for being done." },
                "constraints": {
                    "type": "string",
                    "description": "What must not be touched, whether committing or \
                                    pushing is allowed, and so on.",
                },
            },
            "required": ["goal"],
        },
        "title": {
            "type": "string",
            "description": format!(
                "The session's name in Octoboard's session list: a few words saying what its task \
                 is. Left out or blank, the first line of `brief.goal` is used. {NAMING_LANGUAGE}"
            ),
        },
        "agent": {
            "type": "string",
            "enum": ["claude", "codex", "grok"],
            "description": if with_project {
                "The agent the session runs. Left out, it is the project's default agent. Pass \
                 another only when the user asked for it."
            } else {
                "The agent the session runs. Left out, it is your own agent. Pass another only \
                 when the user asked for it, or when you judge it fits the work better and an \
                 account of it can open a session."
            },
        },
        "account": {
            "type": "string",
            "description": format!(
                "The account the session runs on: an account's id or name{}, or `default` for \
                 the agent's default account. It must belong to the session's agent. While \
                 `auto_select_account` is on, a left-out account is the one of the session's \
                 agent whose weekly quota resets soonest, among accounts that can still open a \
                 session: one a used-up quota keeps from opening is not chosen. Your own \
                 account, for a session of your own agent whose quota is not used up, or the \
                 account its project (else its console) sets, is used only when none does. \
                 Otherwise a session of your own agent runs on your own account, unless that \
                 account's quota is used up, and any other takes the account its project (else \
                 its console) sets for the agent. Pass one only when the user asked for it{}.",
                if with_project { " (see `get_settings`)" } else { "" },
                if with_project { "" } else { ", or when it is the way to reach another agent" }
            ),
        },
        "model": {
            "type": "string",
            "description": "Overrides the model for this one session; blank is the agent's own \
                            default. Left out, a project session takes its project's, else the \
                            console's project session model.",
        },
        "effort": {
            "type": "string",
            "description": "Overrides the reasoning effort for this one session; blank is the \
                            agent's own default.",
        },
    });
    if !with_project {
        // Only a project session is ever responsible for a branch; a console session has none to
        // give.
        properties["branches"] = json!({
            "type": "array",
            "items": { "type": "string" },
            "description": "Local branches of this project's repository to hand over to the new \
                            session, by name. Each must be a branch you are responsible for: one \
                            you created or were handed. They move to it right after it starts, \
                            and it becomes responsible for them in your place. Leave this out \
                            when there is nothing to hand over.",
        });
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
                                that agent. {STARTED_BY_A_SESSION}"),
        "additionalProperties": { "type": if project { json!("string") } else { json!(["string", "null"]) } },
    })
}

/// The `launch` argument and result, or another of the same shape: each agent's model and
/// reasoning effort, `null` or blank for the agent's own default. `whose` says whose sessions the
/// pair is for.
fn launch_schema(whose: &str) -> Value {
    json!({
        "type": "object",
        "description": format!(
            "Each agent's model and reasoning effort for {whose}, keyed by agent (`claude`, \
             `codex`, `grok`) as `{{\"model\": ..., \"effort\": ...}}`; a field left out is left \
             as it is, and `null` or blank is the agent's own default. They must be ones the \
             agent offers; Claude Code's models are {}.",
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

    /// A project session that owns sessions may choose another agent for work that fits it better,
    /// where a console session starts another agent's session only when the user asked: both the
    /// tool's description and its `agent` argument say so.
    #[test]
    fn only_a_project_session_may_choose_another_agent_unasked() {
        let text = |role| {
            let tool = tool_by_name(role, false, "new_session").unwrap();
            let schema = (tool.schema)();
            (
                tool.description.to_string(),
                schema["properties"]["agent"]["description"]
                    .as_str()
                    .unwrap()
                    .to_string(),
            )
        };
        let (description, agent) = text(Role::Project);
        assert!(description.contains("fits the work better"));
        assert!(agent.contains("fits the work better"));
        let (description, agent) = text(Role::Console);
        assert!(!description.contains("fits the work better"));
        assert!(!agent.contains("fits the work better"));
        assert!(agent.contains("only when the user asked for it"));
    }

    /// What a session is offered follows from its role and whether it is bound: only a session that
    /// can own sessions starts, hands over or drives them, only an unbound one binds itself, only a
    /// project session can report, and every session shares information and asks for a session
    /// with no owner.
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
                "list_sessions",
                "share_info",
                "get_project",
                "get_account_quotas",
                "request_session",
                "request_resume_session"
            ]
        );
        assert_eq!(
            names(Role::Project, false),
            [
                "new_session",
                "send_message",
                "get_session",
                "archive_session",
                "archive_self",
                "reopen_session",
                "rename_session",
                "set_session_pinned",
                "handoff_session",
                "handover_branches",
                "switch_session_account",
                "transfer_session",
                "release_session",
                "bind_to_session",
                "request_session",
                "request_resume_session",
                "report",
                "list_sessions",
                "share_info",
                "get_project",
                "get_account_quotas"
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
                "new_session",
                "send_message",
                "get_session",
                "rename_session",
                "set_session_pinned",
                "archive_session",
                "archive_self",
                "list_archived",
                "reopen_session",
                "handoff_session",
                "handover_branches",
                "switch_session_account",
                "transfer_session",
                "release_session",
                "request_session",
                "request_resume_session",
                "list_sessions",
                "share_info",
                "get_account_quotas",
                "show_page"
            ]
        );
        // Console sessions are never bound, so the flag changes nothing for them.
        assert_eq!(names(Role::Console, false), names(Role::Console, true));
        // A bound project session is a subset of what it may have had while it could own sessions,
        // so a session whose binding changes while it runs never calls a tool its child did not
        // announce.
        for tool in names(Role::Project, true) {
            assert!(names(Role::Project, false).contains(&tool), "{tool}");
        }
    }

    /// An unbound project session starts sessions only in its own project, so it is not asked
    /// which one.
    #[test]
    fn only_the_console_sessions_new_session_asks_for_a_project() {
        let schema = |tools: &'static [ToolDef]| {
            let tool = tools
                .iter()
                .find(|tool| tool.name == "new_session")
                .unwrap();
            (tool.schema)()
        };
        let console = schema(tools_for(Role::Console, false));
        let project = schema(tools_for(Role::Project, false));
        assert!(console["properties"]["project"].is_object());
        assert!(project["properties"]["project"].is_null());
        assert_eq!(project["required"], json!(["brief"]));
    }

    /// Every tool that names a session says, on its `title`, which language to write the name in.
    #[test]
    fn every_title_says_which_language_to_write_the_name_in() {
        let mut naming = Vec::new();
        for (role, bound) in [
            (Role::Console, false),
            (Role::Project, true),
            (Role::Project, false),
        ] {
            for tool in tools_for(role, bound) {
                let schema = (tool.schema)();
                let Some(title) = schema["properties"]["title"]["description"].as_str() else {
                    continue;
                };
                assert!(title.ends_with(NAMING_LANGUAGE), "{}: {title}", tool.name);
                naming.push(tool.name);
            }
        }
        naming.sort_unstable();
        naming.dedup();
        assert_eq!(
            naming,
            [
                "handoff_session",
                "new_session",
                "rename_session",
                "request_resume_session",
                "request_session"
            ]
        );
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
