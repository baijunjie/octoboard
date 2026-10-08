//! The Octoboard MCP server: the orchestration tools the console session drives Octoboard with, the
//! narrower set an unbound project session drives its own project with, and the reporting tool a
//! bound project session answers through.
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

pub mod exec;
pub mod role;
pub mod stdio;

use serde_json::{json, Value};

use crate::protocol::{Agent, Role};

/// The key the adapters register the server under. It must not collide with a server the project
/// defines: on Claude Code a colliding key means the *project's* definition is silently never
/// spawned.
pub const SERVER_KEY: &str = "octoboard";

/// Who a bound session reports to, as far as what the session is told depends on it. Fixed for the
/// session's lifetime, like the binding itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    Console,
    /// An unbound project session, named by Octoboard's id for it: a title can be renamed after the
    /// text naming it has been recorded for good.
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
/// `bound` says whether the session reports to another one; it is fixed for the session's
/// lifetime once set, which is what lets the catalogue follow from it. A console session is never
/// bound, and it is ignored for one.
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
        description: "List this console's projects: name, host, directory, the tags the user \
                      gave each, and the sessions currently running in each — every session of \
                      the console, whichever session owns it. Each carries `owner`, the \
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
                      directory.",
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
                                        Defaults to the console's.",
                    },
                }),
                &["source"],
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
        description: "The session's status, plus a tail of what it has printed. Use it to follow \
                      up; a session that is waiting for the user must be left alone until they \
                      have answered. Any session can be read, but one with `yours: false` is not \
                      this caller's to act on, whether it is unbound (`owner: null` on a project \
                      session) or reports to a different session, console or project: leave it \
                      alone. `owner` says whose it is, `owner_kind` whether that is a console or \
                      a project session, and `role` whether the session itself is a console \
                      session.",
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

/// A project session that reports to another session has the one tool it answers through.
const BOUND_PROJECT_SESSION_TOOLS: &[ToolDef] = &[REPORT];

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
        description: "The session's status, plus a tail of what it has printed. Any session of \
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
    REPORT,
];

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
    /// project session may start sessions, and only a project session can report.
    #[test]
    fn the_tools_offered_follow_from_role_and_binding() {
        let names = |role, bound| -> Vec<&'static str> {
            tools_for(role, bound)
                .iter()
                .map(|tool| tool.name)
                .collect()
        };
        assert_eq!(names(Role::Project, true), ["report"]);
        assert_eq!(
            names(Role::Project, false),
            [
                "start_session",
                "send_message",
                "get_session",
                "archive_session",
                "reopen_session",
                "report"
            ]
        );
        // Console sessions are never bound, so the flag changes nothing for them.
        assert_eq!(names(Role::Console, false), names(Role::Console, true));
        assert!(tool_by_name(Role::Console, false, "start_session").is_some());
        assert!(tool_by_name(Role::Console, false, "report").is_none());
        assert!(tool_by_name(Role::Project, true, "start_session").is_none());
        assert!(tool_by_name(Role::Project, false, "list_projects").is_none());
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
