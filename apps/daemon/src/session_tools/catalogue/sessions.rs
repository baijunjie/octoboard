//! The tools that start a session and drive one through its life: instruct, interrupt, rename,
//! pin, archive and resume.

use serde_json::json;

use super::schemas::{object_schema, open_session_schema, NAMING_LANGUAGE};
use super::ToolDef;

/// `open_session` as a console session has it: the session starts in whichever of the console's
/// projects it names.
pub(super) const CONSOLE_OPEN_SESSION: ToolDef = ToolDef {
    name: "open_session",
    description: "Start a session in one of this console's projects and hand it a task. It is \
                  yours for its whole life: it reports to this console session, its owner, and \
                  you instruct it with `instruct_session`, archive it once it is done and \
                  reopen it with `resume_session` when it has another task. Needs no approval. \
                  Returns the new session's id, its agent, the account it opened on \
                  (`account`, `account_name`) and the `model` and `effort` it was launched \
                  with (`null` is unset: the default model, which `list_agent_models` names, and \
                  the agent's own default effort). The brief is rendered into the \
                  session's opening prompt; `title` names the session in the user's language. \
                  It prefers the project's default agent. A left-out account is selected by \
                  the server according to the configured policy. Automatic selection may \
                  choose another agent when neither agent nor account is named; an explicit \
                  agent limits selection to it, and an explicit account preserves both. \
                  Unspecified model and effort follow the final agent. A refusal explains why \
                  the session could not start. Name another account only when the user asked \
                  for it. Name another agent only when the user asked for it or when its tags \
                  (`list_agents`) fit the work. Use this when you should keep directing the \
                  session and receiving its reports. To pass your own work on to a peer in the \
                  next session, use `request_session`; if the user only wants notes for the next \
                  session and an end, leave the notes and end. Ask the user if their intent is \
                  unclear.",
    schema: || open_session_schema(true),
};

/// `open_session` as a project session that can own sessions has it: the session starts in the
/// caller's own project.
pub(super) const PROJECT_OPEN_SESSION: ToolDef = ToolDef {
    name: "open_session",
    description: "Start a session in this project and hand it a task. It is yours for its \
                  whole life: it reports to this session, its owner, and you instruct it with \
                  `instruct_session`, archive it once it is done and reopen it with \
                  `resume_session` when it has another closely related task and room for it. \
                  Needs no approval. It prefers your own agent. A left-out account is selected \
                  by the server according to the configured policy. Automatic selection may \
                  choose another agent when neither agent nor account is named; an explicit \
                  agent limits selection to it, and an explicit account preserves both. \
                  Unspecified model and effort follow the final agent. A refusal explains why \
                  the session could not start. Name another account only when the user asked \
                  for it, or when it is the way to reach another agent. Name another agent \
                  only when the user asked for it or when you judge it fits the work better, \
                  weighing the agents' tags in `list_agents`, and an account of it can open a \
                  session. Returns the new session's id, its agent, the account it opened on \
                  (`account`, `account_name`) and the `model` and `effort` it was launched \
                  with (`null` is unset: the default model, which `list_agent_models` names, and \
                  the agent's own default effort), and with `branches` the ones that \
                  moved and a `note` on how the transfer went. The brief is rendered into the \
                  session's opening prompt; `title` names the session in the user's language. \
                  A session started this way cannot start sessions of its own, and only this \
                  project is available; a session elsewhere, or one that should not report to \
                  you, is one `request_session` asks the user for. To give the new session a \
                  branch you are responsible for, pass it in `branches`; `transfer_branches` \
                  moves the branches of any session of yours later. Use this only when you \
                  should keep directing the session and receiving its reports. To pass your \
                  own work on to a peer in the next session, use `request_session`; if the user \
                  only wants notes for the next session and an end, leave the notes and end. \
                  Ask the user if their intent is unclear.",
    schema: || open_session_schema(false),
};

pub(super) const INSTRUCT_SESSION: ToolDef = ToolDef {
    name: "instruct_session",
    description: "Append an instruction to a running session of yours: it directs the session; \
                  to tell a session something without instructing it, use `inform_session`. \
                  Delivered straight away when the session is idle or mid-turn; held until the \
                  user is done when the session is waiting for them. The answer says if it was \
                  queued, or written but left unsent by the session's agent. Refused for a \
                  session that is not yours (`yours: false`): it is somebody else's, to be left \
                  alone. Refused for an interrupted or archived session of yours, which has no \
                  process to take it: `resume_session` with `text` relaunches it and delivers \
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

pub(super) const INTERRUPT_SESSION: ToolDef = ToolDef {
    name: "interrupt_session",
    description: "Stop the turn a running session of yours is in the middle of, as the user \
                  pressing its interrupt key would. The session keeps its conversation and \
                  goes back to awaiting instructions; redirect it with `instruct_session`. Use it \
                  when the session is working on the wrong thing or should stop now, not to \
                  hurry it along. The answer says whether a turn was stopped (`interrupted`), \
                  the session's `status` afterwards, and in `note` what became of the messages \
                  you sent it during that turn. A session not in a turn is left alone. Refused \
                  for a session that is waiting for the user or `dormant`, not running or still \
                  starting, or being switched, handed off or restarted, and for a session that is \
                  not yours (`yours: false`): it is somebody else's, to be left alone.",
    schema: || {
        object_schema(
            json!({
                "session": { "type": "string", "description": "The session's id." },
            }),
            &["session"],
        )
    },
};

pub(super) const ARCHIVE_SESSION: ToolDef = ToolDef {
    name: "archive_session",
    description: "End a session's process and archive it. Use it to wrap a session up \
                  explicitly; a session that finishes cleanly with nothing left open is \
                  archived without being asked. Refused for a session that is not yours \
                  (`yours: false`): it is somebody else's, to be left alone, and for a session \
                  that is `locked` (see `get_session`). The git branches the sessions archived \
                  created, and the linked worktrees that have them checked out, are removed \
                  once their processes have ended, wherever git can do it without force; \
                  `branches_left` in the answer names any that stay, and why (`archive` in \
                  `list_session_branches` says so beforehand, for each branch). With `keep` \
                  nothing is removed and nothing is waited for: they stay bound to their \
                  sessions and come back with them when those are reopened (a coordinator's \
                  child session is reopened by that coordinator), the answer carries \
                  `branches_kept: true`, and its `note` names the branches of the sessions \
                  archived with it, a coordinator's child sessions (`list_session_branches` \
                  lists the branches of the session itself). To move them to a session of yours \
                  now, use `transfer_branches` with `from` the session you archived; the \
                  branches of the sessions archived with it are not yours to name as `from`, so \
                  `resume_session` the session you archived with a `text` asking it to \
                  `transfer_branches` them from each of those to itself, then hand them on with \
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

pub(super) const ARCHIVE_SELF: ToolDef = ToolDef {
    name: "archive_self",
    description: "End this session's own process and archive it, once its work is finished. \
                  Meant for a session that reports to nobody; one that reports to a parent \
                  (worker or coordinator) wraps up with `report` instead and is refused here. \
                  Refused while any session under this one is not archived, running or \
                  interrupted: an owner archives its sessions once their tasks are done, so an \
                  unarchived one means its task is not finished. Finish it and archive it with \
                  `archive_session` first; for a coordinator's child, wait for that coordinator \
                  to finish it or archive the coordinator with `archive_session`. Refused while \
                  this session or a session under it is locked. The call is answered before the \
                  process ends. The git branches this session created, and the linked worktrees \
                  that have them checked out, are removed once it has ended, wherever git can \
                  do it without force; `branches_left` in the answer names any that will stay, \
                  and why.",
    schema: || object_schema(json!({}), &[]),
};

pub(super) const RENAME_SESSION: ToolDef = ToolDef {
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

pub(super) const SET_SESSION_PINNED: ToolDef = ToolDef {
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

pub(super) const RESUME_SESSION: ToolDef = ToolDef {
    name: "resume_session",
    description: "Relaunch an archived or interrupted session of yours, continuing its \
                  conversation, and optionally hand it the next instruction or relaunch it with \
                  another model or reasoning effort (it stays on its agent and its account; to \
                  move it to another account, use `switch_session_account`). A `model` or \
                  `effort` the agent does not offer is refused, with nothing relaunched. Returns \
                  the session's id, its account (`account`, `account_name`) and the `model` \
                  and `effort` it was launched with (`null` is unset: the default model, which \
                  `list_agent_models` names, and the agent's own default effort). \
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
                    "description": "Resume it with this model, which then becomes the \
                                    session's own; blank is the default model \
                                    (`list_agent_models` names it). \
                                    `list_agent_models` lists the ones its agent offers \
                                    under its account. Left out, it keeps the model it had.",
                },
                "effort": {
                    "type": "string",
                    "description": "Resume it with this reasoning effort, which then becomes \
                                    the session's own; blank is the agent's own default. \
                                    `list_agent_models` lists the levels each model offers. \
                                    Left out, it keeps the effort it had.",
                },
            }),
            &["session"],
        )
    },
};
