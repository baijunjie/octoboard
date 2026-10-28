//! The Octoboard MCP server: the orchestration and settings tools the console session drives
//! Octoboard with, the narrower set a project session that can own sessions drives its own project
//! with, the reporting tool a bound project session answers through, the reads every session has
//! with one scope, and the tools every session shares information and asks the user for a session
//! with no owner through.
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
mod history;
mod models;
mod pages;
mod project_changes;
mod quotas;
pub(crate) mod reads;
mod resume_request;
pub mod role;
mod session_branches;
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
/// while the name is for the user, who may speak another. The text stays static: the user's
/// language is the one a section of the session's instructions names (see
/// `role::user_language_guidance`) where there is one, else the one the user writes in.
const NAMING_LANGUAGE: &str = "Write it in the user's language: the one your instructions name \
     under \"The user's language\" where they have that section, else the one the user converses \
     with you in. Not that of the project, its docs and code, or a brief. Where your instructions \
     name none and you take work from an owner session rather than from the user directly, use the \
     user's language as your owner passes it on, else the language of your owner's messages.";

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

/// What the descriptions of a project say of its `git_status`. A macro so the read tools'
/// `&'static str` descriptions can take it with `concat!`.
macro_rules! git_status_field {
    () => {
        "`git_status` is `null` while the daemon has read none yet; otherwise it is the project's \
         git state as the daemon last read it, without going to the remote now: `repository` \
         (false when the directory is no git repository, and the rest is then empty), `branch` \
         (the short commit id when `detached`), `detached`, `upstream`, how far the branch is \
         `ahead` and `behind` it, `activity` (`idle`, `checking`, `pulling` or `pushing`) and \
         the `error` of a failed check. `checked_seconds_ago` is how long ago the last check \
         completed, `null` when none has since the daemon started, so the counts may be stale."
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

/// What every description of a session says of the fields that tell what a write tool depends on.
/// A macro so the read tools' `&'static str` descriptions can take it with `concat!`.
macro_rules! session_record_fields {
    () => {
        "`status` is `starting` (its agent is still launching), `working` (in a turn), \
         `waiting_user` (needs the user's answer), `idle` (awaiting instructions), `interrupted` \
         (its process ended) or `archived`; `reopen_session` reopens either when it is yours. \
         `account` is the account the session runs on, an account's id or `default` for the \
         agent's default account as `get_account_quotas` lists them, with its `account_name`; \
         `switch_session_account` refuses the account a session is on already. \
         `observed_model` and `observed_effort` are what the agent's records show the session \
         running with, null until reported; `default_effort` is the effort the agent runs that \
         model at by itself, given when the records name the model but no effort. `pinned` is \
         whether the session is pinned (`set_session_pinned`), and `locked` whether the user \
         locked it, which archiving refuses. `branches` names the git branches the session is \
         responsible for; `handover_branches`, and the `branches` of `new_session` and \
         `request_session`, need the caller to be responsible for each, and `get_session_branches` \
         gives each one's worktree. `display_title` is the name the user's session list shows (the \
         agent's current task before `title`), `agent_session_id` the agent's own id of the \
         conversation (null until reported), which `request_resume_session` takes as its \
         `conversation`, and only when the user asked for that conversation, `origin` is `console` \
         for a session started with `new_session`, `user` for the user's own and for one opened \
         through `request_session` or `request_resume_session`, and a handoff's replacement copies \
         the original's, and `has_conversation` says a turn was taken, not that the agent still \
         holds the record: a `reopen_session` that finds it gone is refused. `model` and `effort`, \
         when present, are the session's own choices, `null` leaving them to the agent's default; \
         absent, it follows its project's (else console's) setting. `lead` marks a lead session, \
         and `owned_sessions` counts the sessions bound to it in any status, archived ones \
         included, which is what decides whom it can be transferred to. "
    };
}

const STARTED_BY_A_SESSION: &str = started_by_a_session!();

const CONSOLE_SESSION_TOOLS: &[ToolDef] = &[
    LIST_PROJECTS,
    ToolDef {
        name: "add_project",
        description: "Associate a directory with this console, or clone a git repository and \
                      associate the clone. `source` is `local` for one directory, `parent` for \
                      every git repository directly beneath `path`, or `git` to clone \
                      `remote_url` into `path`, which defaults to the user's default clone \
                      directory. Returns the settings of each project added, as `list_projects` \
                      gives them; a `parent` source gives the same settings, `git_workflow` \
                      included, to every project it adds. A `git` URL must be an http(s), \
                      ssh or git URL without credentials in it: the user's own git \
                      credentials are used.",
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
                    "git_workflow": git_workflow_schema(GitWorkflowFor::NewProject),
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
                    "git_workflow": git_workflow_schema(GitWorkflowFor::Project),
                }),
                &["project"],
            )
        },
    },
    ToolDef {
        name: "get_console",
        description: concat!(
            "This console's own settings: its id (`console`), `name`, `default_agent` (the agent \
             of console sessions), `project_session_agent` (the agent a project session without \
             its own default uses; `null` follows `default_agent`), `accounts`, `launch` (the \
             model and effort of console sessions like this one), `project_session_launch` (the \
             model and effort a project session takes when neither it nor its project names one), \
             whether it has an avatar (`has_icon`), and its working directory (`workdir`, \
             read-only). Only the console this session belongs to can be read. ",
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
                      `sidebar_order_by_status`, `default_clone_dir`, the `git_workflow` settings, \
                      `claude_compatibility` (per agent other than Claude Code, whether its \
                      sessions load Claude Code's instructions (CLAUDE.md and rules), skills, \
                      agents, commands, MCP servers, hooks and permissions; until the user \
                      chooses, Grok Build is on and Codex is on while Claude Code is an added \
                      agent), the `accounts` (each agent also has a default account, listed with \
                      the id `default`; `disabled` marks one the user turned off for new \
                      sessions), and the `trusted_directories`. Accounts and \
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
                    "sidebar_order_by_status": {
                        "type": "boolean",
                        "description": "Whether the user's sidebar orders sessions and projects by \
                                        status. When off, it ranks none by status: sessions go \
                                        pinned first, then most recently started; projects keep \
                                        the active-first grouping, then pinned, then name.",
                    },
                    "default_clone_dir": {
                        "type": "string",
                        "description": "Where a `git` association clones into when no path is \
                                        given. An absolute path or one starting with `~/`; blank \
                                        goes back to the built-in default.",
                    },
                    "git_workflow": git_workflow_schema(GitWorkflowFor::Application),
                    "claude_compatibility": {
                        "type": "object",
                        "description": "Per agent other than Claude Code (`codex`, `grok`), \
                                        whether that agent's sessions load Claude Code's \
                                        instructions (CLAUDE.md and rules), skills, agents, \
                                        commands, MCP servers, hooks and permissions. An agent \
                                        left out is left as it is. Setting it records a choice \
                                        that then holds whatever agents are added. Read when a \
                                        session starts, resumes or switches account.",
                        "properties": {
                            "codex": { "type": "boolean" },
                            "grok": { "type": "boolean" },
                        },
                        "additionalProperties": false,
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
                      you instruct it with `instruct_session`, archive it once it is done and \
                      reopen it with `reopen_session` when it has another task. Needs no approval. \
                      Returns the new session's id, its agent, the account it opened on \
                      (`account`, `account_name`) and the `model` and `effort` it was launched \
                      with (`null` is the agent's own default). The brief is rendered into the \
                      session's opening prompt; `title` names the session in the user's language. \
                      It runs on the project's default agent. While \
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
    INSTRUCT_SESSION,
    INTERRUPT_SESSION,
    GET_SESSION,
    RENAME_SESSION,
    SET_SESSION_PINNED,
    ARCHIVE_SESSION,
    ARCHIVE_SELF,
    LIST_ARCHIVED,
    REOPEN_SESSION,
    HANDOFF_SESSION,
    HANDOVER_BRANCHES,
    SWITCH_SESSION_ACCOUNT,
    TRANSFER_SESSION,
    RELEASE_SESSION,
    REQUEST_SESSION,
    REQUEST_RESUME_SESSION,
    LIST_SESSIONS,
    GET_SESSION_BRANCHES,
    LIST_PROJECT_CHANGES,
    READ_PROJECT_DIFF,
    READ_SESSION_HISTORY,
    LIST_AGENT_MODELS,
    LIST_CONSOLES,
    SEND_MESSAGE,
    GET_ACCOUNT_QUOTAS,
    LIST_PAGES,
    SHOW_PAGE,
    DELETE_PAGES,
];

const LIST_PAGES: ToolDef = ToolDef {
    name: "list_pages",
    description: "The pages this session has pushed with `show_page`, \
                  oldest first, read-only: each one's id (`page`), `created_at` \
                  (milliseconds since the epoch), `html_bytes` and `title` (its `<title>`; \
                  null when it has none). Only this session's own pages are listed. Name a \
                  `page` to read that one in full, `html` included. Use it to find the id of \
                  an earlier page, which tells a form submission apart from one of another \
                  page, or to build on what an earlier page showed.",
    schema: || {
        object_schema(
            json!({
                "page": {
                    "type": "string",
                    "description": "A page's id, as `show_page` returned it. Left out, \
                                    every page is listed without its HTML.",
                },
            }),
            &[],
        )
    },
};
const SHOW_PAGE: ToolDef = ToolDef {
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
                  the pushed page's id; keep it, since it is how a later submission is told to \
                  have come from this page rather than one pushed after it (`list_pages` \
                  lists the ids again). Only the newest page accepts a submission: pushing a \
                  new page makes the previous page's form read-only, so push a fresh form \
                  rather than expecting an answer from an old one.",
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
};

const DELETE_PAGES: ToolDef = ToolDef {
    name: "delete_pages",
    description: "Withdraw pages this session pushed with `show_page` from its report panel, for \
                  good: the user's panel drops them at once. When the last one goes, a \
                  project session's Report panel and its toggle go with it, and a console \
                  session's panel shows its empty state. Name them in `pages` (ids, as \
                  `show_page` and `list_pages` give them), or pass `all: true` for every page \
                  this session pushed; exactly one of the two. Only this session's own pages \
                  can be deleted: an id that is not one of them is refused, the same way as an \
                  id that names no page, and then nothing is deleted. Deleting the newest page \
                  makes the one before it the newest, the one a form submission is accepted \
                  from. Answers with the `deleted` ids.",
    schema: || {
        object_schema(
            json!({
                "pages": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 1,
                    "description": "The ids of the pages to delete.",
                },
                "all": {
                    "type": "boolean",
                    "description": "True to delete every page this session pushed. Not with \
                                    `pages`.",
                },
            }),
            &[],
        )
    },
};

const INSTRUCT_SESSION: ToolDef = ToolDef {
    name: "instruct_session",
    description: "Append an instruction to a running session of yours: it directs the session; \
                  to tell a session something without instructing it, use `send_message`. \
                  Delivered straight away when the session is idle or mid-turn; held until the \
                  user is done when the session is waiting for them. The answer says if it was \
                  queued, or written but left unsent by the session's agent. Refused for a \
                  session that is not yours (`yours: false`): it is somebody else's, to be left \
                  alone. Refused for an interrupted or archived session of yours, which has no \
                  process to take it: `reopen_session` with `text` relaunches it and delivers \
                  the instruction.",
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

const INTERRUPT_SESSION: ToolDef = ToolDef {
    name: "interrupt_session",
    description: "Stop the turn a running session of yours is in the middle of, as the user \
                  pressing its interrupt key would. The session keeps its conversation and \
                  goes back to awaiting instructions; redirect it with `instruct_session`. Use it \
                  when the session is working on the wrong thing or should stop now, not to \
                  hurry it along. The answer says whether a turn was stopped (`interrupted`), \
                  the session's `status` afterwards, and in `note` what became of the messages \
                  you sent it during that turn. A session not in a turn is left alone. Refused \
                  for a session that is waiting for the user, not running or still starting, or \
                  being switched, handed off or restarted, and for a session that is not yours \
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

const ARCHIVE_SESSION: ToolDef = ToolDef {
    name: "archive_session",
    description: "End a session's process and archive it. Use it to wrap a session up explicitly; \
                  a session that finishes cleanly with nothing left open is archived without being \
                  asked. Refused for a session that is not yours (`yours: false`): it is somebody \
                  else's, to be left alone, and for a session that is `locked` (see \
                  `get_session`). The git branches the sessions archived created, and the linked \
                  worktrees that have them checked out, are removed once their processes have \
                  ended, wherever git can do it without force; `branches_left` in the answer names \
                  any that stay, and why. With `keep` nothing is removed and nothing is waited \
                  for: they stay bound to their sessions and come back with them when those are \
                  reopened (a session under a lead session is reopened by its lead), the answer \
                  carries `branches_kept: true`, and its `note` names the branches of the sessions \
                  archived with it, a lead session's own (`get_session_branches` lists the \
                  branches of the session itself). To move them to a session of yours \
                  now, use `handover_branches` with `from` the session you archived; the branches \
                  of the sessions archived with it are not yours to name as `from`, so \
                  `reopen_session` the session you archived with a `text` asking it to \
                  `handover_branches` them from each of those to itself, then hand them on with \
                  `from` set to it once it is idle.",
    schema: || {
        object_schema(
            json!({
                "session": { "type": "string", "description": "The session's id." },
                "keep": {
                    "type": "boolean",
                    "description": "Leave the session's git branches and worktrees as they are \
                                    instead of removing them. Defaults to false. Use it when \
                                    the work on them is not done with or someone may want it.",
                },
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
    description: "Change the title of a session of yours, in the user's language. Refused \
                  for a session that is not yours (`yours: false`): it is \
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
                  left alone. `get_session` shows whether a session is `pinned`.",
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
                  conversation, and optionally hand it the next instruction or reopen it with \
                  another model or reasoning effort (it stays on its agent and its account; to \
                  move it to another account, use `switch_session_account`). A `model` or \
                  `effort` the agent does not offer is refused, with nothing reopened. Returns \
                  the session's id, its account (`account`, `account_name`) and the `model` \
                  and `effort` it was launched with (`null` is the agent's own default). \
                  Refused for a session that is not yours (`yours: false`): it is somebody \
                  else's, to be left alone.",
    schema: || {
        object_schema(
            json!({
                "session": { "type": "string", "description": "The session's id." },
                "text": {
                    "type": "string",
                    "description": "An instruction to deliver once it is running.",
                },
                "model": {
                    "type": "string",
                    "description": "Reopen it with this model, which then becomes the \
                                    session's own; blank is the agent's own default. \
                                    `list_agent_models` lists the ones its agent offers \
                                    under its account. Left out, it keeps the model it had.",
                },
                "effort": {
                    "type": "string",
                    "description": "Reopen it with this reasoning effort, which then becomes \
                                    the session's own; blank is the agent's own default. \
                                    `list_agent_models` lists the levels each model offers. \
                                    Left out, it keeps the effort it had.",
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
                  is responsible for them by then. `branches` in `get_session` and \
                  `list_sessions` names the branches a session is responsible for, and \
                  `get_session_branches` gives their worktrees, so read them first.",
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

/// How an owner moves a session of its own to another agent, with the user's approval: the
/// receiving session takes the original's whole place and stays the owner's, and the original is
/// archived.
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
                  original is archived. The user is asked first, and may change the agent, the \
                  account, the model and the reasoning effort you chose, so read what the result \
                  says was done; the call waits for their answer, for several minutes at most — \
                  unless the user has chosen to have handoffs approved \
                  automatically, in which case the call does not wait. A refusal, or no answer in \
                  time, comes back as an error and changes nothing. Refused, changing nothing, \
                  while the session is in the middle of a turn, and for the agent the session \
                  already runs on (switching it to another account of that agent keeps the \
                  conversation). An account whose quota is used up, or that is not logged in, is \
                  refused only once the user has approved (they can pick another in the form), or \
                  at once when handoffs are approved without asking. Returns the new session's id, \
                  its agent, account and, where set, model and effort, the branches that moved and \
                  the sessions it now owns. Pass `account` only when the user asked for a \
                  particular one.",
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
                                    `default` for the agent's default account. It must belong to \
                                    `agent`. Left out while `auto_select_account` is on, it is the \
                                    account of `agent` whose weekly quota resets soonest and that \
                                    can still open a session. When none does, or while it is off, \
                                    it is your own account when `agent` is your own agent and that \
                                    account can still open a session (it is not disabled, is \
                                    logged in and its quota is not used up); otherwise the account \
                                    the session's project (else its console) sets for `agent`; a \
                                    project session reads it in `get_project`'s \
                                    `effective.accounts`.",
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
                                    `list_agent_models` lists the ones the agent offers. Left \
                                    out, it takes the model its project (else its console) \
                                    sets for sessions; a project session reads it in \
                                    `get_project`'s `effective.launch`.",
                },
                "effort": {
                    "type": "string",
                    "description": "The new session's reasoning effort; blank is the agent's \
                                    own default. `list_agent_models` lists the levels each \
                                    model offers. Left out, it takes the effort its project \
                                    (else its console) sets for sessions; a project session \
                                    reads it in `get_project`'s `effective.launch`.",
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
/// conversation, as the user's Switch account does, with the user's approval.
const SWITCH_SESSION_ACCOUNT: ToolDef = ToolDef {
    name: "switch_session_account",
    description: "Move one of your sessions (`yours: true`) to another account of the agent it \
                  runs on, for instance when its account has run out of quota. The session keeps \
                  its conversation: its process is ended, the conversation is copied to the \
                  account and the session is relaunched resuming it. It is not sent a message for \
                  this: it comes back awaiting instructions, and a turn in progress is cut off, so \
                  send it the next step with `instruct_session` — except where the result's \
                  `note` says it was sent a prompt to continue its task (a turn the switch cut off \
                  or a usage limit had stopped), which you must not repeat. The user is asked \
                  first, and may change the account and the session's model and reasoning effort, \
                  so read what the result says was done; the call waits for their answer, for \
                  several minutes at most — unless the user has chosen to have account switches \
                  approved automatically, in which case the call does not wait. A refusal, or no \
                  answer in time, comes back as an error and changes nothing. Refused at once, \
                  before the user is asked and changing nothing, for an account of another agent, \
                  an account the user turned off, the account the session is on already, and an \
                  archived session; other refusals (an account whose quota is used up or that is \
                  not logged in, say) can come after the user has chosen. Returns the session with \
                  the account it is now on and its model and reasoning effort where it has its \
                  own. `account` in `get_session` shows the account a session is on, to match \
                  against `get_account_quotas`.",
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
                  and `open_items` are what your owner acts on. An owner that is interrupted, \
                  with no process running, has the report kept and delivered once it is resumed; \
                  an archived owner takes none, and the report is refused. With `done` and no open \
                  items the session is archived once the report is delivered or kept, and the git \
                  branches it created, with the linked worktrees that have them checked out, are \
                  removed once it has ended wherever git can do it without force (the answer names \
                  any that will stay); anything else leaves it running and awaiting instructions. \
                  Refused while this session is unbound: nobody owns it but the user.",
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

/// The branches a session is responsible for, with their worktrees: what `handover_branches`
/// needs the caller to be responsible for, and the branches and worktrees archiving would try to
/// remove. Any session may read any session's.
const GET_SESSION_BRANCHES: ToolDef = ToolDef {
    name: "get_session_branches",
    description: "The git branches a session is responsible for, read-only, each with the \
                  project (`project`) it is in and the worktree it is checked out in: \
                  `worktree_status` is `checked_out`, `gone` (git's record of the worktree is \
                  left, the directory is not), `none` (no worktree has it) or `unknown` (it \
                  could not be read), `worktree` its path when there is one, and \
                  `uncommitted_changes` whether a checked-out worktree holds uncommitted changes \
                  (null when there is no worktree or it could not be read). Use it to see which \
                  branches `handover_branches`, or the `branches` of `new_session` and \
                  `request_session`, can take (each must be one the session is responsible for), \
                  and which branches and worktrees archiving the session would try to remove. \
                  `session` is any session, of any console or project, and defaults to this \
                  one. A console session is responsible for no branch, so its list is empty. \
                  `branches` in `get_session` and `list_sessions` names the same branches \
                  without the worktrees.",
    schema: || {
        object_schema(
            json!({
                "session": {
                    "type": "string",
                    "description": "The session's id. Defaults to this session.",
                },
            }),
            &[],
        )
    },
};

/// What has changed in a project's repository, read through the browse layer instead of by running
/// `git status` or `git diff` there. See `project_changes`.
const LIST_PROJECT_CHANGES: ToolDef = ToolDef {
    name: "list_project_changes",
    description: "Read-only: what has changed in a project's git repository, without running git \
                  there (`git status` and `git diff` rewrite the repository's index; this does \
                  not). The arguments choose one of three views. With neither `worktree` nor \
                  `branch`: the uncommitted changes of the worktree holding the project's \
                  directory (not necessarily this session's own worktree: reach that, or any \
                  other, with `worktree`). With `worktree`: the uncommitted changes of that \
                  worktree of the project's repository. With `branch` and `base`: what `branch` \
                  has changed since it forked from `base` (as `git diff base...branch` shows \
                  it), and the commits it made since; with `commit` besides, the changes of that \
                  one commit. Only changes that touch the project's directory are listed. Every \
                  answer has `project` (its id), `view` (`uncommitted`, `branch` or `commit`), \
                  `changes`, `total` (all the changes there are), `offset` (where `changes` \
                  starts), `next_offset` (where the next page starts, `null` on the last) and \
                  `complete` (false when the daemon cut the list at its own limit, so `total` \
                  and the changes after it are not known). Each change has `status` (`added`, \
                  `deleted`, `modified`, `renamed`, `type_changed`, or for a conflicted path how \
                  it conflicts: `both_modified`, `both_added`, `both_deleted`, `added_by_us`, \
                  `added_by_them`, `deleted_by_us`, `deleted_by_them`), `path` (inside the \
                  project, spelled as `read_project_diff` takes it, with `%XX` escapes for a \
                  name that is not UTF-8 or holds a `%`; the removed path for a deletion), \
                  `old_path` (a rename's former path), `kind` (only for a `symlink` or a \
                  `submodule`; for a type change, the new side's), `outside_project` (only when \
                  the other end of a rename lies outside the project's directory: `side`, `old` \
                  or `new`, and its `repository_path`; such a change is still `renamed`, with no \
                  `old_path`, and inside the project amounts to an addition or a deletion), and \
                  `lines_added` and `lines_removed`, `null` where git counts none (a binary \
                  file, a link, a submodule, a change with a side outside the project, a very \
                  large change). In the uncommitted views each change also has `group`: \
                  `staged`, `unstaged`, `untracked` or `conflicted`; one path can be \
                  in two groups, and a conflicted path has no diff to read. The uncommitted \
                  views add `worktree`: `path` and `branch` as plain text, as \
                  `get_session_branches` gives them (`branch` is `null` when HEAD is detached or \
                  unborn), and `head`, the commit the staged changes are against (`null` before \
                  the first commit). The `branch` view adds `branch`, `base`, `branch_commit` \
                  and `base_commit` (the full ids of the two tips), `merge_base` (the commit \
                  they forked at; the changes are between it and `branch_commit`), and \
                  `commits`, newest first and at most 100: `commit`, `parent` (its first \
                  parent, `null` for a root commit), `subject` and `files` (how many project \
                  files it changed); `commits_complete` is false when there were more commits \
                  than listed or the daemon cut the last commit's files short. The `commit` view \
                  has the commit's `commit`, `parent` and `subject`. Read the diff of a change \
                  with `read_project_diff`. Reading a changing worktree twice can give \
                  different lists, so a page of one is not a page of the other. To check a \
                  branch a session reports, give the branch and, as `base`, the one it was \
                  made against, usually the target branch in the session's report or the \
                  branch the project's directory has checked out (`git_status.branch` in \
                  `list_projects`). Only projects of this session's own console can be read, \
                  even when the worktree or branch came from a session of another console.",
    schema: || {
        object_schema(
            json!({
                "project": {
                    "type": "string",
                    "description": "The project's id, or its name when that is unambiguous. \
                                    Defaults to this session's own project; a console session \
                                    has to give it. It must be a project of this session's own \
                                    console.",
                },
                "worktree": {
                    "type": "string",
                    "description": "Uncommitted changes of this worktree of the project's \
                                    repository instead of the one holding the project's \
                                    directory: its path (`worktree` in `get_session_branches`) \
                                    or the branch checked out in it. It must be a worktree of \
                                    this project's repository. Not with `branch`.",
                },
                "branch": {
                    "type": "string",
                    "description": "A local branch to list the changes of since it forked from \
                                    `base`. Needs `base`. Not with `worktree`.",
                },
                "base": {
                    "type": "string",
                    "description": "With `branch`: the branch it is compared with, a local \
                                    branch (a remote-tracking one, such as `origin/main`, with \
                                    `base_remote`).",
                },
                "base_remote": {
                    "type": "boolean",
                    "description": "With `branch`: `base` names a remote-tracking branch, as of \
                                    the last fetch; nothing contacts the remote. Defaults to \
                                    false.",
                },
                "commit": {
                    "type": "string",
                    "description": "With `branch`: the full id of one of the commits the \
                                    answer's `commits` lists; the answer then holds that \
                                    commit's changes against its first parent.",
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 1000,
                    "description": "How many changes to return, 200 when left out.",
                },
                "offset": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Skip this many changes: a previous answer's `next_offset`. \
                                    Defaults to 0.",
                },
            }),
            &[],
        )
    },
};

/// The diff of one change `list_project_changes` lists. See `project_changes`.
const READ_PROJECT_DIFF: ToolDef = ToolDef {
    name: "read_project_diff",
    description: "Read-only: the diff of one changed file of a project, as `list_project_changes` \
                  lists the change, without running git there. Name the file with `path` (and \
                  `old_path` for a rename), then say which change it is: an uncommitted one with \
                  `group` (and `worktree` for a worktree other than the one holding the \
                  project's directory), or one between two commits with `commit` and `parent`: \
                  for a commit of the `branch` view pass its `commit` and `parent`, and for the \
                  whole branch against where it forked pass the answer's `branch_commit` as \
                  `commit` and its `merge_base` as `parent`. The answer has `project`, `path`, \
                  `old_path` (when given), `format`, `text`, `size_bytes`, `offset`, \
                  `next_offset`, `truncated` and `note`. `format` says what `text` holds: \
                  `patch` is the unified diff as git writes it (a file changed only in its mode \
                  or only renamed has a patch with no hunk; an empty `text` means the path has \
                  no difference there, which is no change); `file` is the whole content of an \
                  untracked file, which has no diff, every line added; `binary` is a binary \
                  change, whose `text` is git's one-line `Binary files ... differ` for a patch \
                  and `null` for an untracked file; `none` is a link or submodule with no \
                  content, `text` `null`. `size_bytes` is the full size of the diff or file in \
                  bytes as git has it. At most `max_bytes` of `text` are returned, ending on a \
                  line where it can: when the diff is longer `truncated` is true and \
                  `next_offset` is the `offset` to pass to read on (`null` otherwise); `offset` \
                  and `next_offset` count the bytes of the returned text, which differ from \
                  `size_bytes` when `note` says bytes were replaced. An uncommitted change is \
                  read as it is now, so reading on after it changed can give a different text. \
                  A diff over 4 MiB, and a file over 4 MiB, are refused rather than cut. `note` \
                  is `null` unless something about `text` is worth knowing (a patch that is not \
                  plain text reads the bytes that are not UTF-8 as U+FFFD). Only projects of \
                  this session's own console can be read.",
    schema: || {
        object_schema(
            json!({
                "project": {
                    "type": "string",
                    "description": "The project's id, or its name when that is unambiguous. \
                                    Defaults to this session's own project; a console session \
                                    has to give it. It must be a project of this session's own \
                                    console.",
                },
                "path": {
                    "type": "string",
                    "description": "The file, relative to the project's directory, as a change \
                                    of `list_project_changes` gives it: a name that is not \
                                    UTF-8 or holds a `%` is spelled with `%XX` escapes there, \
                                    and the same here.",
                },
                "old_path": {
                    "type": "string",
                    "description": "For a renamed file: its former path, the change's \
                                    `old_path`.",
                },
                "group": {
                    "type": "string",
                    "enum": ["staged", "unstaged", "untracked"],
                    "description": "Which uncommitted change of the path: the change's `group`. \
                                    Not with `commit`.",
                },
                "worktree": {
                    "type": "string",
                    "description": "With `group`: the worktree the change is in, as for \
                                    `list_project_changes`. Defaults to the one holding the \
                                    project's directory.",
                },
                "commit": {
                    "type": "string",
                    "description": "The full id of the commit holding the new version. Not with \
                                    `group`.",
                },
                "parent": {
                    "type": "string",
                    "description": "With `commit`: the full id of the commit holding the old \
                                    version. Left out, the old version is nothing, which is a \
                                    root commit's.",
                },
                "offset": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Where in the returned text to start, in bytes: a previous \
                                    answer's `next_offset`. Defaults to 0.",
                },
                "max_bytes": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 131072,
                    "description": "The most bytes of the diff to return, 32768 when left out.",
                },
            }),
            &["path"],
        )
    },
};

/// A session's record and a tail of its output. Any session may read any session, whichever its
/// role: what a session may do to another is narrower and is each write tool's own rule.
const GET_SESSION: ToolDef = ToolDef {
    name: "get_session",
    description: concat!(
        "The session's status, plus a tail of what it has printed (the last 8 KiB, or \
         `output_bytes` of it; a session that has ended keeps its last output; `null` if \
         none was kept). With `session` left out it reads this session itself, which is how a \
         session learns its own Octoboard id (`session`) and record. Any session can \
         be read, in any project or console, but one with `yours: false` is not \
         yours to act on, whether nobody owns it (`owner: null` on a project session) or \
         it reports to a different session: leave it alone. A session that is waiting \
         for the user must be left alone until they have answered. `owner` says whose \
         it is, `owner_kind` whether that is a console or a project session, and `role` \
         whether the session itself is a console session. An idle \
         session with a non-empty `background_tasks` takes messages like any idle \
         session but still has work running in the background. \
         A non-null `writes_held` says why messages for a running session are \
         queued rather than written into it, whatever its `status` reads; \
         `queued_messages` counts them, and the messages kept for an interrupted \
         session to take when it is resumed (shared information, and the reports and notices for \
         an owner). ",
        session_record_fields!(),
        session_context_field!()
    ),
    schema: || {
        object_schema(
            json!({
                "session": {
                    "type": "string",
                    "description": "The session's id. Defaults to this session.",
                },
                "output_bytes": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 65536,
                    "description": "How many bytes of the session's output to return, counted \
                                    before escape sequences are stripped. Defaults to 8192; \
                                    a value above 65536 or below 1 is refused, not clamped.",
                },
            }),
            &[],
        )
    },
};

/// The projects of a console, the caller's own unless `console` names another. Offered to every
/// role: reads are not limited to the caller's console, writes are.
const LIST_PROJECTS: ToolDef = ToolDef {
    name: "list_projects",
    description: concat!(
        "List the projects of this session's console (or another console's, see `console`): name, \
         host, directory, source, remote URL (credentials removed), pin, the tags and the \
         description (what the project is for) the user gave each, the settings (`default_agent`, \
         `accounts`, `launch`: each agent's model and reasoning effort, as stored), `effective` \
         (what they resolve to under the console), the `git_workflow` settings (the project's own \
         `override`, `null` following the application-wide value, and the `effective` values), \
         whether the user has `trust_consent`ed to Octoboard pressing the agent's folder-trust \
         confirmation (read-only), its `git_status`, and the sessions currently running in each — \
         every session of the console, whichever session owns it. Each carries `owner`, the \
         session it reports to, `owner_kind` (a `console` or a `project` session) and `yours`; a \
         project session with `owner: null` is unbound, which the user opened themselves and kept \
         outside the orchestration. A session with `yours: false` is not this caller's to drive — \
         it is unbound, or reports to a different session — leave it alone. An idle session with a \
         non-empty `background_tasks` takes messages like any idle session but still has work \
         running in the background. A non-null `writes_held` says why messages for a running \
         session are queued rather than written into it, whatever its `status` reads; \
         `queued_messages` counts them, and the messages kept for an interrupted session to \
         take when it is resumed (shared information, and the reports and notices for an owner). \
         With `console` naming another console (`list_consoles` lists them), only what is \
         needed to target a project of it with `request_session` or `request_resume_session` is \
         returned: each project's id (`project`), name, description and tags, and none of its \
         sessions or settings. `project` narrows the list to one \
         project. With `refresh` the git status of the projects listed is checked against their \
         remotes first, as the application's own refresh does, so you need not run git for it; the \
         answer then carries `git_refresh`, whose `finished` is false, with the `unfinished` \
         projects, when a check had not finished after 30 seconds. ",
        git_status_field!(),
        " ",
        session_record_fields!(),
        session_context_field!()
    ),
    schema: || {
        object_schema(
            json!({
                "console": {
                    "type": "string",
                    "description": "Another console to list the projects of: its id, or its \
                                    name when that is unambiguous (`list_consoles`). Defaults \
                                    to this session's own console, and giving that one lists \
                                    the whole entry of each project. Reads reach any console; \
                                    what a call changes stays in your own.",
                },
                "project": {
                    "type": "string",
                    "description": "Only this project of the console: its id, or its name when \
                                    that is unambiguous.",
                },
                "refresh": {
                    "type": "boolean",
                    "description": "Check the git status of the projects listed against their \
                                    remotes before answering. The call then waits, for 30 \
                                    seconds at most; a project checked within the last minute \
                                    or being checked already is not started again. Like the \
                                    application's own check, it fast-forwards a branch that is \
                                    behind its upstream and has not diverged while \
                                    `auto_pull_repositories` is on. Not for another console's \
                                    projects, which carry no git status. Defaults to false; \
                                    ask only when a status is too old to decide on.",
                },
            }),
            &[],
        )
    },
};

/// The consoles there are, for the `console` of `list_projects` and `request_session`.
const LIST_CONSOLES: ToolDef = ToolDef {
    name: "list_consoles",
    description: "List the consoles, read-only: each one's id (`console`) and `name`, and \
                  whether it is this session's own (`yours`). Use the id, or the name when it is \
                  unambiguous, as the `console` of `list_projects` to find the projects of \
                  another console, and of `request_session` to ask for a session there.",
    schema: || object_schema(json!({}), &[]),
};

/// The archived sessions of a project of any console, whichever role asks.
const LIST_ARCHIVED: ToolDef = ToolDef {
    name: "list_archived",
    description: concat!(
        "List archived sessions, in any console, whichever session owns them: those of one \
         project, or with `project` left out those of every project and the console sessions too. \
         `console`, `project`, `owner`, `yours` and `kind` narrow the list, and a session must \
         match every one given; give `yours`, `owner` or `project` when you can, since an \
         unfiltered list is long. The newest archived come first, at most `limit` of them; `total` \
         is how many match, and `older` how many of them come after the page: when there are some, \
         `next` is a cursor, to pass as `before` to read the next page (it is null on the last). \
         Each carries its `owner`, `owner_kind` and `yours`, so an earlier one of yours can be \
         reopened instead of starting over (`yours: true` lists just those; `owner` with another \
         session's id lists that one's). A session with `yours: false` is somebody else's: leave \
         it alone. ",
        session_record_fields!(),
        session_context_field!()
    ),
    schema: || {
        object_schema(
            json!({
                "console": {
                    "type": "string",
                    "description": "Only sessions in this console, by id or name.",
                },
                "project": {
                    "type": "string",
                    "description": "Only sessions in this project: its id, or its name when \
                                    that is unambiguous (among `console`'s projects when it \
                                    is given, else among this console's projects first, then \
                                    among all).",
                },
                "owner": {
                    "type": "string",
                    "description": "Only sessions that were reporting to this session when \
                                    archived, by id.",
                },
                "yours": {
                    "type": "boolean",
                    "description": "`true`: only the sessions that were reporting to you when \
                                    archived. `false`: only the ones that were not.",
                },
                "kind": {
                    "type": "string",
                    "enum": ["console", "project"],
                    "description": "Only console sessions, or only project sessions.",
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 200,
                    "description": "How many sessions to return, 50 when left out.",
                },
                "before": {
                    "type": "string",
                    "description": "Return only the sessions that come after a position in the \
                                    list, the older ones: the `next` the previous page answered \
                                    with, passed back unchanged. It is a position, not a session, \
                                    so it stays good if that session has been reopened or deleted \
                                    since. Anything else is refused. Left out, the newest are \
                                    returned.",
                },
            }),
            &[],
        )
    },
};

/// The models an agent offers, and the efforts of each: what the `model` and `effort` that
/// `new_session`, `request_session`, `request_resume_session` and the `launch` settings take must
/// be among. Offered to every role, since every role can pass them.
const LIST_AGENT_MODELS: ToolDef = ToolDef {
    name: "list_agent_models",
    description: "The models an agent offers right now, read-only, each with its `id` (what \
                  a `model` takes), `name` and `efforts` (the reasoning-effort levels an `effort` \
                  takes for that model, in the agent's own spelling). Use it before passing a \
                  `model` or `effort`, or a `launch` that holds them, to any tool you have that \
                  takes one; an `effort` can still be refused or lowered by the agent \
                  for a model that does not advertise it. A catalogue belongs to an account, so \
                  name `account` to read another account's. The answer is kept for ten minutes, \
                  as it is for the application's own model pickers, because reading it can run \
                  the agent. When the agent cannot be asked, `models` is empty and `unavailable` \
                  (`code` and `message`) says why: leave `model` out then, which is the agent's \
                  own default.",
    schema: || {
        object_schema(
            json!({
                "agent": {
                    "type": "string",
                    "enum": ["claude", "codex", "grok"],
                    "description": "The agent whose models to list.",
                },
                "account": {
                    "type": "string",
                    "description": "An account's id or name, or `default` for the agent's \
                                    default account. It must belong to `agent`. Defaults to the \
                                    agent's default account.",
                },
            }),
            &["agent"],
        )
    },
};

/// What a session's conversation held, for an owner or a peer to review: more than the tail of
/// output `get_session` gives. Any session may read any session's, as `get_session` does.
const READ_SESSION_HISTORY: ToolDef = ToolDef {
    name: "read_session_history",
    description: "Read a session's conversation, read-only: the user's messages, the messages \
                  Octoboard wrote into it (a brief, a report, shared information, a notice) and \
                  the agent's final reply to each, never its tool calls or the progress notes \
                  before a reply. In a session that is running, the last `agent` entry of the \
                  turn still open can be a progress note rather than a final reply. Any session \
                  can be read, in any project or console, as with `get_session`. It is paged by \
                  entry count from the most recent entry backwards: `entries` are the latest \
                  `limit` ones, oldest first, each with its `index` (its position in the whole \
                  history from 0), `speaker` (`user`, `octoboard` or `agent`), `text`, `turn` \
                  and `at` (milliseconds since the Unix epoch, null when the record gives \
                  none). `older` counts the entries before the page and `total` all there are; \
                  pass the first entry's `index` as `before` to read the page before. Indices \
                  hold only while entries are appended: a `total` smaller than an earlier one \
                  means the history was rewritten (a rewind, a compaction), so start again from \
                  the latest page. `unavailable` is true when the agent's record of the \
                  conversation cannot be found or read, and a session that has had no \
                  conversation answers no entries.",
    schema: || {
        object_schema(
            json!({
                "session": { "type": "string", "description": "The session's id." },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 100,
                    "description": "How many entries to return, 20 when left out.",
                },
                "before": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Return only the entries older than the entry with this \
                                    `index`. Left out, the latest entries are returned.",
                },
            }),
            &["session"],
        )
    },
};

/// What every session, of whatever kind, may do with every other session: look at the running ones,
/// and tell any of them things (an interrupted one takes it when resumed). Kept apart from
/// `instruct_session` so that a message arriving in a session is framed by which tool sent it: an
/// instruction comes from its owner, and information from anyone.
const LIST_SESSIONS: ToolDef = ToolDef {
    name: "list_sessions",
    description: concat!(
        "List the running sessions other than this one, in every console and project, \
         console sessions included (`status` selects the interrupted ones instead, or \
         both): id, title, agent, status, `role` (a console or a project session), \
         `console`, `project` (null for a console session), and `owner` (the session it \
         reports to, `owner_kind` saying whether that is a console or a project session; \
         null when nobody owns it and it reports to the user). `yours` marks a session \
         that reports to you, and `your_owner` the session you report to, which is also \
         given at the top level. `console`, `project`, `owner` and `yours` narrow the \
         list, and a session must match every one given. Use it to find who to share \
         information with, or an owner to hand a session to or bind to. An idle session \
         with a non-empty `background_tasks` takes messages like any idle session but \
         still has work running in the background. A non-null `writes_held` says why \
         messages for a running session are queued rather than written into it, whatever \
         its `status` reads; `queued_messages` counts them, and the messages kept for \
         an interrupted session to take when it is resumed (shared information, and the reports \
         and notices for an owner). ",
        session_record_fields!(),
        session_context_field!()
    ),
    schema: || {
        object_schema(
            json!({
                "console": {
                    "type": "string",
                    "description": "Only sessions in this console, by id or name.",
                },
                "project": {
                    "type": "string",
                    "description": "Only sessions in this project: its id, or its name when \
                                    that is unambiguous (among `console`'s projects when it \
                                    is given, else among this console's projects first, then \
                                    among all).",
                },
                "owner": {
                    "type": "string",
                    "description": "Only sessions that report to this session, by id.",
                },
                "yours": {
                    "type": "boolean",
                    "description": "`true`: only the sessions that report to you. `false`: only \
                                    the ones that do not.",
                },
                "status": {
                    "type": "string",
                    "enum": ["live", "interrupted", "any"],
                    "description": "`live` (the default): the sessions with a process running \
                                    (each record's `status` is then `starting`, `working`, \
                                    `waiting_user` or `idle`). `interrupted`: the ones whose \
                                    process ended without being archived, resumable with \
                                    `reopen_session` when they are yours. `any`: both. Archived \
                                    sessions are `list_archived`'s.",
                },
            }),
            &[],
        )
    },
};

const SEND_MESSAGE: ToolDef = ToolDef {
    name: "send_message",
    description: "Tell a session something it may find useful: what you found, decided or \
                  changed that touches its work. Any session can be told, in any project or \
                  console, console sessions included; no session needs to own the other. It is \
                  delivered marked as information from you, not as an instruction, and the \
                  session it is for takes work only from its owner. When that session reports to \
                  another session, that owner is sent a copy. Held until the user is done when \
                  the session is waiting for them. For an interrupted session, with no process \
                  running, it is kept and delivered when the session is resumed, and dropped if \
                  the session is archived or deleted first; the answer says `Queued`. Refused \
                  for this session itself, and for an archived session, which takes nothing \
                  until it is reopened.",
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
        "This project's settings in Octoboard, read-only: its id (`project`), name, `host`, \
         directory, source, remote URL (credentials removed), pin, tags, description, \
         `trust_consent` (whether the user allowed Octoboard to press the agent's folder-trust \
         confirmation), `default_agent`, `accounts` and `launch` (the model and effort sessions \
         of each agent start with) as set on the project, where `null` or `inherit` means \
         unset, and `effective`, those three resolved against the console's settings: the \
         project's default agent, the account id (or `default`) and the model and effort \
         (`null` is the agent's own default) a session of each agent gets when nothing names \
         otherwise, before the picks described below, and `git_workflow` (the project's own \
         `override` of the git workflow settings, `null` following the application-wide value, \
         and the `effective` resolved values; `rebase_before_merge` and `delivery` apply only \
         while `development` is not `in_place`), and `git_status`. ",
        git_status_field!(),
        " With `refresh` the status is checked against the remote first, as the application's \
         own refresh does, so you need not run git for it; `git_refresh` in the answer then says \
         whether the check `finished` within 30 seconds. Only this session's own project can be \
         read. ",
        started_by_a_session!()
    ),
    schema: || {
        object_schema(
            json!({
                "refresh": {
                    "type": "boolean",
                    "description": "Check this project's git status against its remote before \
                                    answering. The call then waits, for 30 seconds at most; a \
                                    project checked within the last minute or being checked \
                                    already is not started again. Like the application's own \
                                    check, it fast-forwards a branch that is behind its upstream \
                                    and has not diverged while `auto_pull_repositories` is on. \
                                    Defaults to false; ask only when the status is too old to \
                                    decide on.",
                },
            }),
            &[],
        )
    },
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
                  own, taking work only from the user in its terminal. You can no longer \
                  instruct or drive it, but `send_message` still lets either of you tell the \
                  other something. Needs no approval. It stops reporting to anyone, becomes an \
                  ordinary unbound session that can start and drive sessions of its own, and is \
                  no longer archived with you or by its own `done` report. A lead session keeps \
                  the sessions it started, and the branches the session is responsible for stay \
                  with it. A session is told it has no owner now, when it next starts if it is \
                  not running, which replaces what it was told of its owner when it was \
                  started. An optional `note` is passed on to it as your parting words, so say \
                  there what it should know to carry on without you.",
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
                  bound. To tell a session something without binding, use `send_message`.",
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
                  (`send_message`). It can be a project session in any project of any console, or \
                  a console session. A session you want to drive yourself is one `new_session` \
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
                  branch is checked out. `title` names the session in the user's language. In your \
                  own console it runs on your own agent, or on the project's default agent when a \
                  project is named from a console session. While `auto_select_account` is on, a \
                  left-out account is the one of the session's agent whose weekly quota resets \
                  soonest and that can still open a session. When none does, your own account is \
                  used in your own console only for your own agent when that account can still \
                  open a session; any other session takes the account its project, else its \
                  console, sets. Otherwise a session in your own console of your own agent runs on \
                  your own account, unless that account can no longer open a session (it is \
                  disabled, not logged in or used up), and any other takes the account its \
                  project, else its console, sets (a project session reads its own project's in \
                  `get_project`'s `effective.accounts`). Pass an account only when the user asked \
                  for one.",
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
                  any session. No text can be passed along, so afterwards `send_message` tells the \
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
                  agent that the user has not turned off, the agent's default account (`account: \
                  \"default\"`) and the stored ones, with its `account_name`, `plan`, and its \
                  quota `windows` (`kind` `session` is the five-hour window (Claude Code's session \
                  limit, Codex's 5h limit), `weekly` the weekly one across all models, \
                  `weekly_model` a weekly window of the model named by `label`, `credit_period` a \
                  pool of credits over a billing period). All values are percentages of a window: \
                  `used_percent` and `remaining_percent`, with `resets_at` (milliseconds since the \
                  epoch) and `window_minutes`; no absolute token or dollar amount is available. \
                  `exhausted` says the account is used up and cannot open a session until it \
                  resets, and `exhausted_until` is when (milliseconds since the epoch; null when \
                  it is not exhausted or the reset time is unknown). `can_open` says a session \
                  could be opened on the account now; when it is false `cannot_open_reason` says \
                  why (used up, not logged in, or a change of its sharing settings is running). \
                  `auto_select_account` is whether a session opened without naming an account is \
                  put on one Octoboard chooses (the account of the agent whose weekly quota resets \
                  soonest and still has some left, among those with `can_open`). Only agents a \
                  session could be opened on are listed: one that is not installed or not added \
                  lists no accounts. These are the readings Octoboard last took, refreshed about \
                  every five minutes, and this call takes a new one only when `refresh` is true, \
                  and then for the accounts it lists and those of an agent that is installed but \
                  not added, which Octoboard reads as well: `fetched_at` says how old a reading \
                  is, `unavailable` gives the reason a reading is missing or could not be \
                  refreshed (an account not read yet has `fetched_at: null`, as has one of an \
                  agent not detected yet). Use it to check an account before relying on it, or to \
                  pick the account for `switch_session_account` or `handoff_session`.",
    schema: || {
        object_schema(
            json!({
                "agent": {
                    "type": "string",
                    "enum": ["claude", "codex", "grok"],
                    "description": "List only this agent's accounts. Left out, every agent's.",
                },
                "refresh": {
                    "type": "boolean",
                    "description": "Take new readings before answering, of the accounts listed. \
                                    The call then waits for them, which can take several \
                                    seconds. An account is read at most once a minute, whoever \
                                    asked: within it the held reading is returned as it is, \
                                    with no error. A refresh only reads quota and never starts \
                                    a quota window. Defaults to false, which spends nothing; \
                                    ask only when a reading is too old to decide on.",
                },
            }),
            &[],
        )
    },
};

/// A project session that reports to another session has the one tool it answers through, the
/// reads every session has, the read of its project, and the two requests the user approves: for
/// a session with no owner, and for a conversation to be resumed. Its tools are a subset of an
/// unbound session's.
const BOUND_PROJECT_SESSION_TOOLS: &[ToolDef] = &[
    REPORT,
    GET_SESSION,
    LIST_SESSIONS,
    GET_SESSION_BRANCHES,
    LIST_PROJECT_CHANGES,
    READ_PROJECT_DIFF,
    READ_SESSION_HISTORY,
    LIST_AGENT_MODELS,
    LIST_CONSOLES,
    LIST_PROJECTS,
    LIST_ARCHIVED,
    SEND_MESSAGE,
    GET_PROJECT,
    GET_ACCOUNT_QUOTAS,
    REQUEST_SESSION,
    REQUEST_RESUME_SESSION,
];

/// A project session that can own sessions — unbound, or a lead session — drives its own project
/// the way a console session drives its console's, and what it starts reports back to it.
/// `report` is announced too, though a call is refused while the session is unbound: the
/// catalogue does not change when an unbound session binds itself to a console session. For the
/// same reason `archive_self` is announced to a lead session and refused at the call. It also has
/// the report panel's three tools, since it is the project session that faces the user; a bound
/// session reports to its owner and has none.
const UNBOUND_PROJECT_SESSION_TOOLS: &[ToolDef] = &[
    ToolDef {
        name: "new_session",
        description: "Start a session in this project and hand it a task. It is yours for its \
                      whole life: it reports to this session, its owner, and you instruct it with \
                      `instruct_session`, archive it once it is done and reopen it with \
                      `reopen_session` when it has another closely related task and room for it. \
                      Needs no approval. It runs on your own agent. While `auto_select_account` is \
                      on, a left-out account is the one of that agent whose weekly quota resets \
                      soonest and that can still open a session. When none does, your own account \
                      is used only when it is your own agent and can still open a session; \
                      otherwise the account is the project's, else the console's (`get_project` \
                      gives it in `effective.accounts`). Name another account only when the user \
                      asked for it, or when it is the way to reach another agent. Name another \
                      agent only when the user asked for it or when you judge it fits the work \
                      better and an account of it can open a session. Returns the new session's \
                      id, its agent, the account it opened on (`account`, `account_name`) and the \
                      `model` and `effort` it was launched with (`null` is the agent's own \
                      default), and with `branches` the ones that moved and a `note` on how the \
                      handover went. The brief is rendered into the session's opening prompt; \
                      `title` names the session in the user's language. A session started this way \
                      cannot start sessions of its own, and only this project is available; a \
                      session elsewhere, or one that should not report to you, is one \
                      `request_session` asks the user for. To give the new session a branch you \
                      are responsible for, pass it in `branches`; `handover_branches` moves the \
                      branches of any session of yours later. Use this only when you should keep \
                      directing the session and receiving its reports. To hand your own work to a \
                      peer in the next session, use `request_session`; if the user only wants \
                      handover notes and an end, leave the notes and end. Ask the user if their \
                      intent is unclear.",
        schema: || new_session_schema(false),
    },
    INSTRUCT_SESSION,
    INTERRUPT_SESSION,
    GET_SESSION,
    ARCHIVE_SESSION,
    ARCHIVE_SELF,
    LIST_ARCHIVED,
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
    GET_SESSION_BRANCHES,
    LIST_PROJECT_CHANGES,
    READ_PROJECT_DIFF,
    READ_SESSION_HISTORY,
    LIST_AGENT_MODELS,
    LIST_CONSOLES,
    LIST_PROJECTS,
    SEND_MESSAGE,
    GET_PROJECT,
    GET_ACCOUNT_QUOTAS,
    LIST_PAGES,
    SHOW_PAGE,
    DELETE_PAGES,
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
        "description": "For `console`: the console's id, or its name when that is unambiguous \
                        (`list_consoles`). Defaults to this session's own console.",
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
         soonest, among accounts that can still open a session (not disabled, logged in, quota \
         not used up). Your own account, for a session in your own console of your own agent \
         that can still open a session, or the account its project (else its console) sets, is \
         used only when none does. Otherwise a session in your own console of your own agent \
         runs on your own account, unless that account can no longer open a session (it is \
         disabled, not logged in or used up), and any other takes the account its project (else \
         its console) sets for the agent (a project session reads its own project's in \
         `get_project`'s `effective.accounts`). Pass one only when the user asked for it."
    );
    schema["properties"]["model"]["description"] = json!(
        "Overrides the model for this one session; blank is the agent's own default. \
         `list_agent_models` lists the ones the agent offers. Left out, a project session takes \
         its project's, else the console's project session model (a project session reads its \
         own project's in `get_project`'s `effective.launch`)."
    );
    schema["properties"]["effort"]["description"] = json!(
        "Overrides the reasoning effort for this one session; blank is the agent's own default. \
         `list_agent_models` lists the levels each model offers. Left out, a project session \
         takes its project's, else the console's project session effort (a project session \
         reads its own project's in `get_project`'s `effective.launch`)."
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
                                session: overrides its model; blank is the agent's own default \
                                (`list_agent_models` lists the ones the agent offers). A \
                                session that holds the conversation keeps its own, and the \
                                call is refused if this is given.",
            },
            "effort": {
                "type": "string",
                "description": "Only when no session holds the conversation, which opens a new \
                                session: overrides its reasoning effort; blank is the agent's \
                                own default (`list_agent_models` lists the levels each model \
                                offers). A session that holds the conversation keeps its own, \
                                and the call is refused if this is given.",
            },
        }),
        &["conversation"],
    )
}

/// The schema of `new_session`. `with_project` is for the console session, which chooses a
/// project; a project session always starts in its own, so it is not asked.
fn new_session_schema(with_project: bool) -> Value {
    let launch_pointer = if with_project {
        ""
    } else {
        " (`get_project` gives it in `effective.launch`)"
    };
    let mut properties = json!({
        "brief": {
            "type": "object",
            "description": "The task, in natural language per field.",
            "properties": {
                "goal": { "type": "string", "description": "The outcome to achieve." },
                "context": {
                    "type": "string",
                    "description": "Background and relevant leads. Include the user's \
                                    language, so the session can name any sessions it starts in \
                                    it.",
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
                "The account the session runs on: an account's id or name{pick_from}, or \
                 `default` for the agent's default account. It must belong to the session's \
                 agent. While `auto_select_account` is on, a left-out account is the one of the \
                 session's agent whose weekly quota resets soonest, among accounts that can \
                 still open a session (not disabled, logged in, quota not used up). Your own \
                 account, for a session of your own agent that can still open a session, or the \
                 account its project (else its console) sets, is used only when none does. \
                 Otherwise a session of your own agent runs on your own account, unless that \
                 account can no longer open a session (it is disabled, not logged in or used \
                 up), and any other takes the account its project (else its console) sets for \
                 the agent{pointer}. Pass one only when the user asked for it{other_agent}.",
                pick_from = if with_project { " (see `get_settings`)" } else { "" },
                pointer = if with_project {
                    ""
                } else {
                    " (`get_project` gives it in `effective.accounts`)"
                },
                other_agent = if with_project {
                    ""
                } else {
                    ", or when it is the way to reach another agent"
                },
            ),
        },
        "model": {
            "type": "string",
            "description": format!(
                "Overrides the model for this one session; blank is the agent's own default. \
                 `list_agent_models` lists the ones the agent offers. Left out, a project \
                 session takes its project's, else the console's project session \
                 model{launch_pointer}.",
            ),
        },
        "effort": {
            "type": "string",
            "description": format!(
                "Overrides the reasoning effort for this one session; blank is the agent's own \
                 default. `list_agent_models` lists the levels each model offers. Left out, a \
                 project session takes its project's, else the console's project session \
                 effort{launch_pointer}.",
            ),
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
             agent offers: `list_agent_models` lists them, with the efforts of each. Claude Code's \
             models are always {}.",
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

/// When a change to an item of the development rule takes effect. The rule is part of the role
/// text, which Claude Code and Grok record on a session's first turn and replay on every resume.
const RULE_REACHES: &str = "Unlike other settings, a change is sure to reach only project \
                            sessions begun after it: one begun earlier may keep the rule it \
                            started with, even when resumed.";

/// Whose git workflow settings a tool's `git_workflow` argument holds.
#[derive(Clone, Copy, PartialEq)]
enum GitWorkflowFor {
    Application,
    /// An existing project's own choices: an item left out is left as it is.
    Project,
    /// A project being added: an item left out follows the application-wide value.
    NewProject,
}

/// The `git_workflow` argument: the four git workflow settings, application-wide or as a project's
/// own.
fn git_workflow_schema(whose: GitWorkflowFor) -> Value {
    let project = whose != GitWorkflowFor::Application;
    let (owner, left_out) = match whose {
        GitWorkflowFor::Application => ("the application-wide values", "is left as it is"),
        GitWorkflowFor::Project => (
            "this project's own choices",
            "is left as it is; `null` goes back to following the application-wide value",
        ),
        GitWorkflowFor::NewProject => (
            "this project's own choices",
            "or `null` follows the application-wide value",
        ),
    };
    let nullable = |kind: Value| {
        if project {
            json!([kind, "null"])
        } else {
            kind
        }
    };
    let mut development = json!({
        "type": nullable(json!("string")),
        "description": format!(
            "Where project sessions develop: `in_place` (directly on the current branch; they are \
             given no rule), `branch` (on a branch of their own in the project directory, which \
             they share, so it suits one session at a time) or `worktree` (on a branch of their \
             own in a worktree of their own). {RULE_REACHES}"
        ),
    });
    development["enum"] = if project {
        json!(["in_place", "branch", "worktree", null])
    } else {
        json!(["in_place", "branch", "worktree"])
    };
    let mut delivery = json!({
        "type": nullable(json!("string")),
        "description": format!(
            "How finished work is delivered: `ask` (ask the user each time), `merge` (merge into \
             the target branch locally) or `pull_request` (push and open a pull request). \
             Applies only while `development` is not `in_place`. {RULE_REACHES}"
        ),
    });
    delivery["enum"] = if project {
        json!(["ask", "merge", "pull_request", null])
    } else {
        json!(["ask", "merge", "pull_request"])
    };
    json!({
        "type": "object",
        "description": format!(
            "The git workflow settings for project sessions, {owner}; an item left out \
             {left_out}."
        ),
        "properties": {
            "development": development,
            "rebase_before_merge": {
                "type": nullable(json!("boolean")),
                "description": format!(
                    "Whether a branch is rebased onto its target branch before it is merged. \
                     Applies only while `development` is not `in_place`. {RULE_REACHES}"
                ),
            },
            "delivery": delivery,
            "revert_gate": {
                "type": nullable(json!("boolean")),
                "description": "Whether a branch update that quietly undoes changes already on \
                                the target branch is blocked, whatever `development` is. A \
                                change takes effect at once, in running sessions too.",
            },
        },
        "additionalProperties": false,
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
    /// project session can report, every session shares information and asks for a session with no
    /// owner, and every read-only tool but `get_project` is offered to all three alike.
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
                "get_session",
                "list_sessions",
                "get_session_branches",
                "list_project_changes",
                "read_project_diff",
                "read_session_history",
                "list_agent_models",
                "list_consoles",
                "list_projects",
                "list_archived",
                "send_message",
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
                "instruct_session",
                "interrupt_session",
                "get_session",
                "archive_session",
                "archive_self",
                "list_archived",
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
                "get_session_branches",
                "list_project_changes",
                "read_project_diff",
                "read_session_history",
                "list_agent_models",
                "list_consoles",
                "list_projects",
                "send_message",
                "get_project",
                "get_account_quotas",
                "list_pages",
                "show_page",
                "delete_pages"
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
                "instruct_session",
                "interrupt_session",
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
                "get_session_branches",
                "list_project_changes",
                "read_project_diff",
                "read_session_history",
                "list_agent_models",
                "list_consoles",
                "send_message",
                "get_account_quotas",
                "list_pages",
                "show_page",
                "delete_pages"
            ]
        );
        // The reads every role has, with one definition each, so that their descriptions cannot
        // differ by role.
        for read in [
            "get_session",
            "list_sessions",
            "read_session_history",
            "get_session_branches",
            "list_agent_models",
            "list_consoles",
            "list_projects",
            "list_archived",
            "get_account_quotas",
        ] {
            let described = |role, bound| {
                let tool = tool_by_name(role, bound, read).unwrap_or_else(|| panic!("{read}"));
                (tool.description.to_string(), (tool.schema)())
            };
            let console = described(Role::Console, false);
            assert_eq!(console, described(Role::Project, false), "{read}");
            assert_eq!(console, described(Role::Project, true), "{read}");
        }
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

    /// Every parameter that takes a model or an effort, `launch` settings included, sends the
    /// caller to `list_agent_models` for the values, and each role has that tool.
    #[test]
    fn every_model_parameter_points_to_the_tool_that_lists_the_models() {
        let mut checked = 0;
        for (role, bound) in [
            (Role::Console, false),
            (Role::Project, true),
            (Role::Project, false),
        ] {
            let tools = tools_for(role, bound);
            assert!(tools.iter().any(|tool| tool.name == "list_agent_models"));
            for tool in tools {
                let schema = (tool.schema)();
                let mut described = Vec::new();
                for key in ["model", "effort", "launch", "project_session_launch"] {
                    described.extend(schema["properties"][key]["description"].as_str());
                }
                for text in described {
                    assert!(
                        text.contains("`list_agent_models`"),
                        "{}: {text}",
                        tool.name
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 10, "{checked}");
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

    /// The additions that close the gaps of the reads and writes: each is in the schema its tool
    /// announces, and the descriptions name the fields the results really carry.
    #[test]
    fn the_schemas_and_descriptions_name_what_the_tools_take_and_return() {
        let tool = |name| tool_by_name(Role::Console, false, name).unwrap();
        let schema = |name| (tool(name).schema)();
        // A session reads itself by leaving `session` out.
        assert_eq!(schema("get_session")["required"], json!([]));
        assert!(
            tool("get_session")
                .description
                .contains("your own Octoboard id")
                || tool("get_session")
                    .description
                    .contains("its own Octoboard id")
        );
        for (name, key) in [
            ("list_archived", "before"),
            ("list_archived", "yours"),
            ("list_projects", "project"),
            ("list_projects", "refresh"),
            ("archive_session", "keep"),
        ] {
            assert!(schema(name)["properties"][key].is_object(), "{name}.{key}");
            assert!(
                tool(name).description.contains(&format!("`{key}`")),
                "{name}.{key}"
            );
        }
        let project = tool_by_name(Role::Project, true, "get_project").unwrap();
        assert!((project.schema)()["properties"]["refresh"].is_object());
        // The project entry is described by what it carries.
        assert!(!tool("list_projects").description.contains("`worktrees`"));
        assert!(tool("list_projects").description.contains("`git_workflow`"));
        for field in ["`project`", "`host`", "`trust_consent`"] {
            assert!(project.description.contains(field), "get_project: {field}");
        }
        assert!(tool("get_console").description.contains("`console`"));
        for field in ["`account_name`", "`account`"] {
            assert!(
                tool("new_session").description.contains(field),
                "new_session: {field}"
            );
        }
        assert!(tool("reopen_session").description.contains("`effort`"));
        for field in [
            "`lead`",
            "`owned_sessions`",
            "`agent_session_id`",
            "`display_title`",
            "`origin`",
        ] {
            assert!(
                tool("get_session").description.contains(field),
                "get_session: {field}"
            );
        }
    }

    /// A project session is pointed at `get_project` for what its project sets, a console session
    /// at `get_settings` for the accounts to choose from, and neither at what it cannot read.
    #[test]
    fn the_new_session_schema_points_each_role_at_what_it_can_read() {
        let text = |schema: &Value, field: &str| {
            schema["properties"][field]["description"]
                .as_str()
                .unwrap()
                .to_string()
        };

        let project = new_session_schema(false);
        let account = text(&project, "account");
        assert!(
            account
                .contains("for the agent (`get_project` gives it in `effective.accounts`). Pass"),
            "{account}"
        );
        assert!(
            account.contains("an account's id or name, or `default`"),
            "{account}"
        );
        assert!(!account.contains("get_settings"), "{account}");
        for field in ["model", "effort"] {
            let text = text(&project, field);
            assert!(
                text.contains("`get_project` gives it in `effective.launch`"),
                "{text}"
            );
        }

        let console = new_session_schema(true);
        let account = text(&console, "account");
        assert!(
            account.contains("an account's id or name (see `get_settings`), or `default`"),
            "{account}"
        );
        assert!(account.contains("for the agent. Pass"), "{account}");
        assert!(!account.contains("get_project"), "{account}");
        for field in ["model", "effort"] {
            let text = text(&console, field);
            assert!(!text.contains("get_project"), "{text}");
        }

        // `request_session` can target any project and is a console session's tool too, so its
        // pointer names a project session's own project.
        let request = request_session_schema();
        for field in ["account", "model", "effort"] {
            let text = text(&request, field);
            assert!(
                text.contains("a project session reads its own project's in `get_project`'s"),
                "{text}"
            );
        }
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
