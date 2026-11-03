//! What the descriptions and schemas of several tool families share.

use serde_json::{json, Value};

use crate::protocol::Agent;

/// Which language a session's name is written in, said on every argument that names one. Agents
/// otherwise take the language of what they read most — the project, its docs and code, the brief —
/// while the name is for the user, who may speak another. The text stays static: the user's
/// language is the one a section of the session's instructions names (see
/// `role::user_language_guidance`) where there is one, else the one the user writes in.
pub(super) const NAMING_LANGUAGE: &str =
    "Write it in the user's language: the one your instructions name \
     under \"The user's language\" where they have that section, else the one the user converses \
     with you in. Not that of the project, its docs and code, or a brief. Where your instructions \
     name none and you take work from an owner session rather than from the user directly, use the \
     user's language as your owner passes it on, else the language of your owner's messages.";

/// What every default agent and account setting says of the sessions a session starts itself:
/// which follow the starting session, and which the project's default agent. A macro so the read
/// tools' `&'static str` descriptions can take it with `concat!`.
macro_rules! started_by_a_session {
    () => {
        "A session a project session starts itself (`open_session`, or `request_session` in its \
         own console) runs on that session's own agent unless it chooses another, and one a \
         console session starts for a project (`open_session` or `request_session`) on the \
         project's default agent; a console session's request for a console session runs on the \
         requester's own agent. A left-out account is selected by the server according to the \
         configured policy. Automatic selection may choose another agent when neither agent nor \
         account is named; an explicit agent limits selection to it, and an explicit account \
         preserves both. Unspecified model and effort follow the final agent. A console session \
         names another account only when the user asked for it, and another agent only when the \
         user asked for it or its tags (`list_agents`) fit the work."
    };
}

pub(super) use started_by_a_session;

pub(super) const STARTED_BY_A_SESSION: &str = started_by_a_session!();

/// The schema of `open_session`. `with_project` is for the console session, which chooses a
/// project; a project session always starts in its own, so it is not asked.
pub(super) fn open_session_schema(with_project: bool) -> Value {
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
            "enum": agent_enum(),
            "description": if with_project {
                "The agent the session runs. Left out, it is the project's default agent. Pass \
                 another only when the user asked for it, or when its tags (`list_agents`) fit the \
                 work."
            } else {
                "The agent the session runs. Left out, it is your own agent. Pass another only \
                 when the user asked for it or when you judge it fits the work better, weighing \
                 the agents' tags in `list_agents`, and an account of it can open a session."
            },
        },
        "account": {
            "type": "string",
            "description": format!(
                "The account the session runs on: an account's id or name{pick_from}, or \
                 `default` for the agent's default account. It must belong to the session's \
                 agent. Left out, the server selects it according to the configured policy \
                 for the agent{pointer}. An explicit agent limits selection to it; an explicit \
                 account preserves both. Unspecified model and effort follow the final agent. \
                 Pass one only when the user asked for it{other_agent}.",
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
            "description": "Local branches of this project's repository to transfer to the new \
                            session, by name. Each must be a branch you are responsible for: one \
                            you created or were handed. They move to it right after it starts, \
                            and it becomes responsible for them in your place. Leave this out \
                            when there is nothing to transfer.",
        });
        return object_schema(properties, &["brief"]);
    }
    properties["project"] = json!({
        "type": "string",
        "description": "The project's id, or its name when that is unambiguous.",
    });
    object_schema(properties, &["project", "brief"])
}

/// The `enum` of an argument that names an agent, in [`Agent::ALL`] order.
pub(super) fn agent_enum() -> Value {
    Value::Array(
        Agent::ALL
            .iter()
            .map(|agent| json!(agent.wire_name()))
            .collect(),
    )
}

/// The `enum` of an argument that names an agent or is `null`.
pub(super) fn agent_enum_or_null() -> Value {
    let mut values = agent_enum();
    values
        .as_array_mut()
        .expect("agent_enum is an array")
        .push(Value::Null);
    values
}

/// Wraps a property map as a tool input schema. Every tool takes an object, so the envelope is the
/// same each time and only the properties and the required list differ.
pub(super) fn object_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}
