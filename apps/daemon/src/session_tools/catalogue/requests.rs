//! The tools that ask the user for a session with no owner, new or resumed.

use serde_json::{json, Value};

use super::schemas::{object_schema, open_session_schema, NAMING_LANGUAGE};
use super::ToolDef;

/// How every session, of whatever kind, has a session started that is nobody's: through a
/// confirmation the user gives. The brief and the overrides are `open_session`'s, and are checked
/// the same way before the user is asked.
pub(super) const REQUEST_SESSION: ToolDef = ToolDef {
    name: "request_session",
    description: "Ask the user to start a session that is nobody's — neither yours nor your \
                  owner's: it reports to the user, and you and it can only exchange information \
                  (`inform_session`). It can be a project session in any project of any console, \
                  or a console session. A session you want to drive yourself is one `open_session` \
                  starts, where you have it. The user is asked first, and the call waits for their \
                  answer, for several minutes at most — unless the user has chosen to have such \
                  requests approved automatically, in which case the result says so and the call \
                  does not wait. On approval Octoboard starts it with your brief as its opening \
                  prompt; follow the user's direction on whether your own work ends once the \
                  work is passed on. Use this for a peer to continue your current work in the \
                  next session. If the user only wants notes for the next session and an end, \
                  leave the notes and end without starting a session. If their intent is \
                  unclear, ask before acting. A refusal, or no answer in time, comes back as an \
                  error and changes nothing. When \
                  transferring work on a branch you are responsible for, pass it in `branches`: \
                  the user is shown them, and on approval the new session becomes responsible for \
                  them in your place. Leave `branches` out when you are only working on whatever \
                  branch is checked out. `title` names the session in the user's language. In your \
                  own console it runs on your own agent, or on the project's default agent when a \
                  project is named from a console session. For a session in another console, the \
                  default agent is the one that console (and its project) sets. A left-out account \
                  is selected by the server according to the configured policy. Automatic \
                  selection may choose another agent when neither agent nor account is named; a \
                  named agent limits selection to it, and a named account preserves both. \
                  Unspecified model and effort follow the final agent. Pass an account only when \
                  the user asked for one, and an agent only when the user asked for one or its \
                  tags (`list_agents`) fit the work. Read the result for the actual session \
                  started; a refusal explains why nothing was started.",
    schema: request_session_schema,
};

/// How every session, of whatever kind, has a conversation resumed that it does not own: through a
/// confirmation the user gives, with what the new-session dialog's Resume conversation tab gives
/// the user and the same limits.
pub(super) const REQUEST_RESUME_SESSION: ToolDef = ToolDef {
    name: "request_resume_session",
    description: "Ask the user to resume a conversation by its id — the agent's own conversation \
                  id (a UUID, in any spelling), not an Octoboard session id — as the user does in \
                  the new-session dialog's Resume conversation tab, with the same limits. If a \
                  session of the project holds the conversation, it is brought back: an archived \
                  session is reopened (and renamed, with `title`), and an interrupted one resumed, \
                  continuing its conversation, with the owner it has (it may come back with its \
                  archived owners). It stays that owner's, or the user's: you do not own it and \
                  cannot drive it. If a session that is already running holds it there is nothing \
                  to resume, and the call is refused. If no session holds the conversation but an \
                  agent's own record of it is on this machine, a new session that is nobody's is \
                  opened on it, on the agent and account that hold the record, and it reports to \
                  the user. A conversation held by a session of another project, or by a console \
                  session, is refused. Where you have `resume_session`, it resumes only sessions \
                  that are yours and needs no approval; this can resume any session. No text can \
                  be passed along, so afterwards `inform_session` tells the session what you need. \
                  The user is asked first, and the call waits for their answer, for several \
                  minutes at most — unless the user has chosen to have such requests approved \
                  automatically, in which case the result says so and the call does not wait. Use \
                  it only when the user asked for that conversation to be resumed, or you have \
                  been told to carry on work whose conversation they named. A refusal, or no \
                  answer in time, comes back as an error and changes nothing. If their intent is \
                  unclear, ask before acting.",
    schema: request_resume_session_schema,
};

/// The schema of `request_session`: `open_session`'s, and where the new session is to be.
pub(super) fn request_session_schema() -> Value {
    let mut schema = open_session_schema(false);
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
         project, and the agent the other console (and its project) sets for one elsewhere. Pass \
         one only when the user asked for it, or when its tags (`list_agents`) fit the work."
    );
    schema["properties"]["account"]["description"] = json!(
        "The account the session runs on: an account's id or name, or `default` for the agent's \
         default account. It must belong to the session's agent. Left out, the server selects it \
         according to the configured policy. An explicit agent limits selection to it; an explicit \
         account preserves both. A project session reads its own project's in `get_project`'s \
         `effective.accounts`. Pass one only when the user asked for it."
    );
    schema["properties"]["model"]["description"] = json!(
        "Overrides the model for this one session; blank is the default model \
         (`list_agent_models` names it). \
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
    schema["properties"]["compatibility"] = super::schemas::compatibility_schema(
        "this session",
        "the project's, else the console's, else the application-wide value (a project session \
         reads its own project's in `get_project`'s `effective.launch`)",
    );
    schema["properties"]["branches"] = json!({
        "type": "array",
        "items": { "type": "string" },
        "description": "For `project`: local branches of the project's repository to transfer \
                        to the new session, by name. Each must be a branch you are responsible \
                        for: one you created or were handed. Pass them when you are transferring \
                        the work on those branches; leave this out when you are only working on \
                        the checked-out branch, or when there is nothing to transfer.",
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
                                session: overrides its model; blank is the default model \
                                (`list_agent_models` names it and lists the ones the agent \
                                offers). A \
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
            "compatibility": super::schemas::compatibility_schema(
                "this session",
                "the project's, else the console's, else the application-wide value (only when no \
                 session holds the conversation, which opens a new session; one that holds it \
                 keeps its own, and the call is refused if a switch is held here)",
            ),
        }),
        &["conversation"],
    )
}
