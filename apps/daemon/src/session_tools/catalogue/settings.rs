//! The console session's tools for its projects, its console and the application settings,
//! and the schemas only they take.

use serde_json::{json, Value};

use super::schemas::{
    agent_enum, agent_enum_or_null, agent_id_list, object_schema, started_by_a_session,
    STARTED_BY_A_SESSION,
};
use super::ToolDef;
use crate::adapter::provider;
use crate::protocol::AgentId;

pub(super) const ADD_PROJECT: ToolDef = ToolDef {
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
                    "enum": agent_enum(),
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
};

pub(super) const UPDATE_PROJECT: ToolDef = ToolDef {
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
                    "enum": agent_enum_or_null(),
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
};

pub(super) const GET_CONSOLE: ToolDef = ToolDef {
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
};

pub(super) const UPDATE_CONSOLE: ToolDef = ToolDef {
    name: "update_console",
    description: "Change this console's own settings; what is omitted is left as it is. \
                  Returns them as `get_console` gives them. Sessions already running keep \
                  what they were started with: a new account reaches only console sessions \
                  opened afterwards, and a new model or effort reaches this console \
                  session on its next launch unless it was given its own; a new \
                  `project_session_launch` reaches project sessions at their next launch, \
                  unless their project or the session sets its own. Only this \
                  console can be changed. These are the user's \
                  settings: change one only when they asked you to.",
    schema: || {
        object_schema(
            json!({
                "name": { "type": "string", "description": "The display name. Not blank." },
                "default_agent": {
                    "type": "string",
                    "enum": agent_enum(),
                    "description": format!(
                        "The agent console sessions use by default. {STARTED_BY_A_SESSION}"
                    ),
                },
                "project_session_agent": {
                    "type": ["string", "null"],
                    "enum": agent_enum_or_null(),
                    "description": format!(
                        "The agent a project session uses when its project has no default \
                         agent of its own. `null` makes it follow `default_agent`. \
                         {STARTED_BY_A_SESSION}"
                    ),
                },
                "icon": {
                    "type": ["string", "null"],
                    "description": "The console's avatar, as an image `data:` URL (such as \
                                    `data:image/png;base64,…`) of at most 256 KiB; `null` \
                                    or blank removes it.",
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
};

pub(super) const GET_SETTINGS: ToolDef = ToolDef {
    name: "get_settings",
    description: "The application-wide settings: `auto_close_idle_shell_terminals`, `auto_pull_repositories`, \
                  `auto_delete_unresumable_archives`, `resume_sessions_on_start`, \
                  `auto_select_account`, `interface_language` (the user's language, a tag; \
                  `null` until a window reported one), `sidebar_order_by_status`, \
                  `default_clone_dir`, the `git_workflow` settings, \
                  `provider_settings` (the settings the user made for each agent's \
                  provider that you may read, by agent and then setting id, with the value \
                  in effect: a switch is `true` or `false`, a choice is its text or `null`. \
                  Among them are the switches that make an agent's sessions load another \
                  agent's instructions, skills, agents, commands, MCP servers, hooks and \
                  permissions; until the user chooses, such a switch follows its default \
                  rule, which depends on which agents are added), \
                  `agent_tags` (the tags the user put on each added agent, by agent, which \
                  say what it is good for; an agent without tags, and a removed agent, has no \
                  entry), the `accounts` (each agent also has a default account, listed with \
                  the id `default`; `disabled` marks one the user turned off for new \
                  sessions), and the `trusted_directories`. `auto_select_account`, \
                  `provider_settings`, `agent_tags`, the accounts and the trusted \
                  directories are the user's own and read-only here.",
    schema: || object_schema(json!({}), &[]),
};

pub(super) const UPDATE_SETTINGS: ToolDef = ToolDef {
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
                "auto_close_idle_shell_terminals": {
                    "type": "boolean",
                    "description": "Close idle shell terminals when the user closes their Terminal panel. Terminals with running programs stay open.",
                },
                "auto_delete_unresumable_archives": {
                    "type": "boolean",
                    "description": "Whether archived sessions whose agent conversation is \
                                    gone are deleted automatically (pinned and locked ones \
                                    are kept).",
                },
                "resume_sessions_on_start": {
                    "type": "boolean",
                    "description": "Whether, when Octoboard starts, the sessions the previous run \
                                    left running are resumed automatically (archived ones and \
                                    ones already interrupted before that run ended are not).",
                },
                "interface_language": {
                    "type": "string",
                    "description": "The language the daemon holds for the user, as a tag the \
                                    interface is offered in (such as `en`, `ja` or \
                                    `zh-Hans`). Sessions are told to talk to the user in \
                                    it; the user's windows keep their own choice.",
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
            }),
            &[],
        )
    },
};

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
    let agents = agent_id_list();
    let (values, description) = if project {
        (
            "`inherit` (the console's account), `default` (the agent's default account), or an \
             account's id or name (see `get_settings`)",
            format!(
                "Each agent's account for sessions opened here, keyed by agent ({agents}); an \
                 agent left out is left as it is."
            ),
        )
    } else {
        (
            "`default` or `null` (the agent's default account), or an account's id or name (see \
             `get_settings`)",
            format!(
                "Each agent's account for sessions opened in this console, keyed by agent \
                 ({agents}); an agent left out is left as it is."
            ),
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
/// reasoning effort, `null` or blank for unset (the default model, the agent's own default
/// effort). `whose` says whose sessions the pair is for.
fn launch_schema(whose: &str) -> Value {
    json!({
        "type": "object",
        "description": format!(
            "Each agent's model and reasoning effort for {whose}, keyed by agent ({}) as \
             `{{\"model\": ..., \"effort\": ...}}`; a field left out is left as it is, and \
             `null` or blank is unset (the default model, which `list_agent_models` names, and the \
             agent's own default effort). They must be ones the agent offers: `list_agent_models` \
             lists them, with the efforts of each.{}",
            agent_id_list(),
            fixed_models_sentences()
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

/// What the providers whose models never change say of them ("Claude Code's models are always
/// `fable`, `opus`, `sonnet`, `haiku`."), each sentence after a space; empty when no provider has
/// such models.
fn fixed_models_sentences() -> String {
    AgentId::ALL
        .into_iter()
        .filter_map(|agent| {
            let ids = provider(agent).catalogue().fixed_model_ids();
            let ids: Vec<String> = ids.iter().map(|id| format!("`{id}`")).collect();
            (!ids.is_empty())
                .then(|| format!(" {}'s models are always {}.", agent.label(), ids.join(", ")))
        })
        .collect()
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

/// The `git_workflow` argument: the five git workflow settings, application-wide or as a project's
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
             given no branch or worktree rule), `branch` (on a branch of their own in the project directory, which \
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
            "squash_before_delivery": {
                "type": nullable(json!("boolean")),
                "description": format!(
                    "Whether to squash the branch's commits into one commit for the feature the \
                     branch delivers, with its tests, documentation and the fixes made while \
                     reviewing or checking it, keeping a separate commit only for a bug fix \
                     unrelated to that feature, before delivering finished work by merge, push or \
                     pull request. Use the common ancestor of the current branch and its target \
                     branch as the base, and rewrite only the current branch's commits. Preserve \
                     existing `Reverts:` lines marking deliberate reverts in the original commit \
                     messages. Squash before any requested rebase. \
                     Never rewrite a pushed branch that other people or sessions also use. \
                     Applies only while `development` is not `in_place`. Default off. {RULE_REACHES}"
                ),
            },
            "rebase_before_delivery": {
                "type": nullable(json!("boolean")),
                "description": format!(
                    "Whether to rebase a branch onto its target branch before delivering finished \
                     work (merge, push or pull request), after any requested squash. The key keeps \
                     its earlier spelling for compatibility. Never rewrite a pushed branch \
                     that other people or sessions also use. Applies only while `development` is \
                     not `in_place`. {RULE_REACHES}"
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
