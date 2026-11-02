//! The tools that move a session or what it holds: handoff, handover of branches and
//! processes, account and model changes, and changes of owner.

use serde_json::json;

use super::schemas::{agent_enum, object_schema, NAMING_LANGUAGE};
use super::ToolDef;

/// How an owner moves the responsibility for git branches between sessions, without the user: the
/// branch records change hands, and no session is replaced.
pub(super) const HANDOVER_BRANCHES: ToolDef = ToolDef {
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

/// How an owner moves process trees between sessions, without the user: which session's end
/// stops them changes, and they keep running.
pub(super) const HANDOVER_PROCESSES: ToolDef = ToolDef {
    name: "handover_processes",
    description: "Move process trees a session started (a dev server, a preview, a watcher) from \
                  one session to another, so that they are stopped when the receiving session's \
                  process ends (its archive, an interrupt, its agent exiting) instead of the \
                  giving one's; they keep running meanwhile, and nothing else changes. Use it \
                  before archiving a session whose server another session of yours still needs, \
                  say. `from` and `to` are each this session or a session of yours (`yours: \
                  true`); leave either out for this session. Both must have a process running \
                  and neither may be archived: an archive stops what a session started, and what \
                  the user kept is the user's. Name the trees by the `id` that `processes` in \
                  `get_session` gives for `from`; each must be one `from` holds now. A tree no \
                  session holds (one the user kept, or one left behind in a project) cannot be \
                  handed to a session. Needs no approval. \
                  Refused, changing nothing, while `from` is another session that is working, \
                  starting or waiting for the user. The answer lists the trees moved as `to` now \
                  holds them; the sessions concerned other than this one are told. A handoff, an \
                  account switch, a model change and a change of account sharing carry every \
                  process over by themselves.",
    schema: || {
        object_schema(
            json!({
                "processes": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "The ids of the process trees to move, as `processes` in \
                                    `get_session` gives them. Each must be one `from` holds.",
                },
                "from": {
                    "type": "string",
                    "description": "The session handing the processes over: this session (the \
                                    default) or a session of yours with a process running.",
                },
                "to": {
                    "type": "string",
                    "description": "The session taking the processes: this session (the \
                                    default) or a session of yours with a process running.",
                },
            }),
            &["processes"],
        )
    },
};

/// How an owner moves a session of its own to another agent, with the user's approval: the
/// receiving session takes the original's whole place and stays the owner's, and the original is
/// archived.
pub(super) const HANDOFF_SESSION: ToolDef = ToolDef {
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
                    "enum": agent_enum(),
                    "description": "The agent the new session runs on; not the session's own.",
                },
                "account": {
                    "type": "string",
                    "description": "The new session's account: an account's id or name, or \
                                    `default` for the agent's default account. It must belong to \
                                    `agent`. Left out, the server selects it according to the \
                                    configured policy, staying on `agent`. A project session reads \
                                    its project's account settings in `get_project`'s \
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
pub(super) const SWITCH_SESSION_ACCOUNT: ToolDef = ToolDef {
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
                  against `list_agents`.",
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

/// How an owner changes the model and reasoning effort of a session of its own, keeping its
/// conversation and its account, as the user's Change model does, without approval.
pub(super) const SET_SESSION_MODEL: ToolDef = ToolDef {
    name: "set_session_model",
    description: "Change the model or reasoning effort of one of your sessions (`yours: true`), \
                  which then become the session's own. It stays on its agent and its account, \
                  so no approval is needed. A running session is restarted to take them: its \
                  process is ended and relaunched resuming its conversation, which cuts off a \
                  turn in progress. It is not sent a message for this: it comes back awaiting \
                  instructions, so send it the next step with `instruct_session` — except where \
                  the result's `note` says it was sent a prompt to continue its task (a turn the \
                  restart cut off or a usage limit had stopped), which you must not repeat. The \
                  call waits a few seconds for the relaunch to come up; should the call be cut \
                  off before then, the change still runs to its end and its outcome is written \
                  into your session. An interrupted session is not started: it takes the new \
                  values when it is next resumed. Values it already has succeed, changing \
                  nothing, unless a change of its model is under way, which refuses the call. A \
                  `model` or `effort` the agent does not offer under the session's \
                  account is refused, changing nothing; `list_agent_models` lists them. Refused \
                  too for an archived session (`reopen_session` takes a model and effort), one \
                  that is starting or being switched, handed off or otherwise changed, and one \
                  waiting for the user or `dormant`, which the restart would take its prompt or \
                  view from — a usage-limit dialog alone is not such a wait: the restart ends \
                  it, and the turn it stopped is continued. Returns the session, whether it was \
                  `restarted`, and its own `model` and `effort` (`null` is the agent's own \
                  default; absent where it follows its project's or console's).",
    schema: || {
        object_schema(
            json!({
                "session": {
                    "type": "string",
                    "description": "The id of the session of yours to change.",
                },
                "model": {
                    "type": "string",
                    "description": "The model it runs with from now on; blank is the agent's \
                                    own default. `list_agent_models` lists the ones its agent \
                                    offers under its account. Left out, it keeps the model it \
                                    has.",
                },
                "effort": {
                    "type": "string",
                    "description": "The reasoning effort it runs with from now on; blank is the \
                                    agent's own default. `list_agent_models` lists the levels \
                                    each model offers. Left out, it keeps the effort it has.",
                },
            }),
            &["session"],
        )
    },
};

/// How an owner — a console session, or a project session that can own sessions — passes one of
/// its sessions to another owner, without the user: the session is its to pass, and the rules a
/// binding keeps are checked as for any binding.
pub(super) const TRANSFER_SESSION: ToolDef = ToolDef {
    name: "transfer_session",
    description: "Hand one of your child sessions (`yours: true`) to another parent session, to \
                  pass its task over: from then on it reports to that parent, which drives it, and \
                  it is no longer yours. Needs no approval. The new parent is a console session of \
                  this console, or a coordinator of the session's own project. A coordinator with \
                  child sessions, archived ones included, can go only to a console session. The \
                  new parent session must be running. The session is told who its parent session \
                  now is, when it next starts if it is not running, and the new parent session is \
                  sent `note`. A coordinator handed to a project session becomes a worker and \
                  loses `show_page` and the other tools for the report panel, and every page it \
                  pushed is withdrawn.",
    schema: || {
        object_schema(
            json!({
                "session": {
                    "type": "string",
                    "description": "The id of the session of yours to hand over.",
                },
                "to": { "type": "string", "description": "The new parent session's session id." },
                "note": {
                    "type": "string",
                    "description": "The handover, in natural language, for the new parent session: \
                                    what the session is doing, where it stands and what is left.",
                },
            }),
            &["session", "to", "note"],
        )
    },
};

/// How an owner — a console session, or a project session that can own sessions — lets one of its
/// sessions go, without the user: the session is its to release, and becomes the user's own.
pub(super) const RELEASE_SESSION: ToolDef = ToolDef {
    name: "release_session",
    description:
        "Release one of your sessions (`yours: true`) from its binding to you, when it is \
                  not yours to drive any more: it has no parent session afterwards and is the \
                  user's own, taking work only from the user in its terminal. You can no longer \
                  instruct or drive it, but `send_message` still lets either of you tell the other \
                  something. Needs no approval. It stops reporting to anyone, becomes an \
                  independent coordinator that can create and manage child sessions, and is no \
                  longer archived with you or by its own `done` report. A coordinator keeps the \
                  sessions it started, and the branches the session is responsible for stay with \
                  it. A session is told it has no parent session now, when it next starts if it is \
                  not running, which replaces what it was told of its parent session when it was \
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
                    "description": "Optional. A parting note for the session, passed on as yours \
                                    when it is told it has no parent session now: what it should \
                                    know to carry on without you. A blank note is ignored, and one \
                                    over 4000 characters is refused.",
                },
            }),
            &["session"],
        )
    },
};

/// How an unbound project session becomes one of another session's, without the user: it puts
/// itself under that owner, and the rules a binding keeps are checked as for any binding.
pub(super) const BIND_TO_SESSION: ToolDef = ToolDef {
    name: "bind_to_session",
    description: "Set a parent for this independent coordinator: from then on you report to it \
                  with `report` and take work from it. Needs no approval. A console parent of this \
                  console preserves your coordinator role and child sessions. A project \
                  coordinator of this project can be your parent only if you have no child \
                  sessions, archived ones included; you become a worker and can no longer start or \
                  manage child sessions, and lose `show_page` and the other tools for the report \
                  panel: every page you pushed there is withdrawn when you bind (binding to a \
                  console session keeps them). The parent session must be running, and is sent \
                  `summary` as your first report. The result says which tools you have from then \
                  on. Refused once this session has a parent. To tell a session something without \
                  changing the reporting relationship, use `send_message`.",
    schema: || {
        object_schema(
            json!({
                "owner": { "type": "string", "description": "The parent session's session id." },
                "summary": {
                    "type": "string",
                    "description": "What you are doing and what you need from the parent session, \
                                    in natural language. It reaches the parent session as your \
                                    first report.",
                },
            }),
            &["owner", "summary"],
        )
    },
};
