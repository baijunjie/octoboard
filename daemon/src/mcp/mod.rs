//! The Octoboard MCP server: the orchestration tools the hub drives Octoboard with, and the one
//! reporting tool a project session answers through.
//!
//! **Transport is a stdio child process, not an HTTP endpoint the agent connects to.** Each
//! adapter registers `octoboardd mcp --session … --role … --port … --token …` as a `command`-type
//! MCP server, and that child ([`stdio`]) speaks MCP on its stdin/stdout while forwarding every
//! call to the daemon over loopback HTTP, where [`exec`] runs it against the real state. One
//! mechanism covers all three agents, which is why it was chosen over pointing them at a
//! Streamable HTTP endpoint in the daemon: Codex's verified injection takes a `command` and
//! `args` (`docs/mvp.md` section 6), and Grok's SSRF protection rejects loopback HTTP outright,
//! which is already why its hooks cannot be HTTP either.
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

/// One tool, as both sides of the stdio bridge see it.
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    /// The tool's JSON Schema, built on demand rather than stored: `rmcp` wants an owned object
    /// and the catalogue is walked once per `tools/list`.
    pub schema: fn() -> Value,
}

/// The tools a session of this role may call. Nothing else is announced to it, so a project
/// session cannot start or archive sessions and the hub cannot report to itself.
pub fn tools_for(role: Role) -> &'static [ToolDef] {
    match role {
        Role::Hub => HUB_TOOLS,
        Role::Worker => WORKER_TOOLS,
    }
}

pub fn tool_by_name(role: Role, name: &str) -> Option<&'static ToolDef> {
    tools_for(role).iter().find(|tool| tool.name == name)
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

const HUB_TOOLS: &[ToolDef] = &[
    ToolDef {
        name: "list_projects",
        description: "List this console's projects: name, host, directory, and the sessions \
                      currently running in each. A session with `include_in_hub: false` is one the \
                      user opened themselves and kept outside the orchestration — leave it alone.",
        schema: || object_schema(json!({}), &[]),
    },
    ToolDef {
        name: "add_project",
        description: "Associate a directory with this console, or clone a GitHub repository and \
                      associate the clone. `source` is `local` for one directory, `parent` for \
                      every git repository directly beneath `path`, or `github` to clone \
                      `remote_url` into `path`.",
        schema: || {
            object_schema(
                json!({
                    "source": {
                        "type": "string",
                        "enum": ["local", "parent", "github"],
                        "description": "How the directory is being associated.",
                    },
                    "path": {
                        "type": "string",
                        "description": "The directory to associate, the parent directory to scan, \
                                        or the parent directory to clone into.",
                    },
                    "remote_url": {
                        "type": "string",
                        "description": "The repository to clone. Required for `github`.",
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
                      opening prompt.",
        schema: || {
            object_schema(
                json!({
                    "project": {
                        "type": "string",
                        "description": "The project's id, or its name when that is unambiguous.",
                    },
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
                }),
                &["project", "brief"],
            )
        },
    },
    ToolDef {
        name: "send_message",
        description: "Append an instruction to a running session. Delivered straight away when \
                      the session is idle or mid-turn; held until the user is done when the \
                      session is waiting for them.",
        schema: || {
            object_schema(
                json!({
                    "session": { "type": "string", "description": "The session's id." },
                    "text": { "type": "string", "description": "The instruction, in natural language." },
                }),
                &["session", "text"],
            )
        },
    },
    ToolDef {
        name: "get_session",
        description: "The session's status, plus a tail of what it has printed. Use it to follow \
                      up; a session that is waiting for the user must be left alone until they \
                      have answered, and so must one with `include_in_hub: false`.",
        schema: || {
            object_schema(
                json!({
                    "session": { "type": "string", "description": "The session's id." },
                }),
                &["session"],
            )
        },
    },
    ToolDef {
        name: "archive_session",
        description: "End a session's process and archive it. Use it to wrap a session up \
                      explicitly; a session that finishes cleanly with nothing left open is \
                      archived without being asked.",
        schema: || {
            object_schema(
                json!({
                    "session": { "type": "string", "description": "The session's id." },
                }),
                &["session"],
            )
        },
    },
    ToolDef {
        name: "list_archived",
        description: "List the archived sessions of one project, so an earlier one can be \
                      reopened instead of starting over.",
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
    ToolDef {
        name: "reopen_session",
        description: "Relaunch an archived or interrupted session, continuing its conversation, \
                      and optionally hand it the next instruction.",
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
    },
    ToolDef {
        name: "show_page",
        description: "Push a page to the report panel the user sees beside this session. Every \
                      page pushed is kept, and the user can page back through earlier ones. `html` \
                      is a self-contained HTML document — inline any styles and scripts, and stick \
                      to generic font families rather than a custom typeface, since the page cannot \
                      load anything external (no subresources, fetch/XHR, external scripts or \
                      stylesheets, or fonts); an image has to be a `data:` URL for the same reason. \
                      Use it for anything better shown than typed into the terminal: a table, a \
                      comparison, a set of choices. \
                      A page may include a form; calling `octoboard.submit(data)` from it sends \
                      the user's answer back to this session as a message — only wire that call to \
                      a user action (a button, a submit), never to page load, or the page will post \
                      into this session on its own. The call's result carries the pushed page's id; \
                      keep it, since it is the only way to tell a later submission came from this \
                      page rather than one pushed after it.",
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

const WORKER_TOOLS: &[ToolDef] = &[ToolDef {
    name: "report",
    description: "Report the round of work back to the hub. `summary` is natural language; \
                  `status` and `open_items` are what the hub acts on. With `done` and no open \
                  items the session is archived once the report is delivered; anything else \
                  leaves it running and awaiting instructions.",
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
                                    be, `needs_decision` when the hub has to choose before it can \
                                    go on.",
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
}];

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

    #[test]
    fn each_role_sees_only_its_own_tools() {
        assert!(tool_by_name(Role::Worker, "start_session").is_none());
        assert!(tool_by_name(Role::Worker, "report").is_some());
        assert!(tool_by_name(Role::Hub, "report").is_none());
        for tool in [
            "list_projects",
            "start_session",
            "send_message",
            "show_page",
            "report",
        ] {
            // Every tool belongs to exactly one role, so a tool added to both lists by mistake is
            // caught here rather than by a hub reporting to itself.
            let roles = [Role::Hub, Role::Worker]
                .into_iter()
                .filter(|role| tool_by_name(*role, tool).is_some())
                .count();
            assert_eq!(roles, 1, "{tool} belongs to exactly one role");
        }
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
        for role in [Role::Hub, Role::Worker] {
            for tool in tools_for(role) {
                let schema = (tool.schema)();
                assert_eq!(schema["type"], "object", "{}", tool.name);
                assert!(schema["properties"].is_object(), "{}", tool.name);
                assert!(schema["required"].is_array(), "{}", tool.name);
            }
        }
    }
}
