//! The read tools for sessions, projects, consoles, agents and their models.

use serde_json::json;

use super::schemas::{agent_enum, object_schema, started_by_a_session};
use super::ToolDef;

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
         `waiting_user` (needs the user's answer), `dormant` (the user is in its background-agents \
         view: it holds its messages and takes none until they leave, so leave it alone), `idle` \
         (awaiting instructions), `interrupted` (its process ended) or `archived`; \
         `resume_session` resumes either when it is yours. `paused` is true for an `idle` session \
         whose last turn ended abnormally (cancelled, declined, stopped on a usage limit, failed, \
         or cut short by its process ending mid-turn, which a resume brings back): it takes \
         messages like any idle session. `idle_after_report` is true for a session with a parent \
         that sits idle, with nothing running or queued for it, after its last report, its own or \
         synthesised, reached that parent: it is not awaiting instructions but finished or \
         stalled, so check it and archive it if its work is over. `account` is the account the \
         session runs on, an account's id or `default` for the agent's default account as \
         `list_agents` lists them, with its `account_name`; `switch_session_account` refuses the \
         account a session is on already. `observed_model` and `observed_effort` are what the \
         agent's records show the session running with, null until reported; `default_effort` is \
         the effort the agent runs that model at by itself, given when the records name the model \
         but no effort. `pinned` is \
         whether the session is pinned (`set_session_pinned`), and `locked` whether the user \
         locked it, which archiving refuses. `branches` names the git branches the session is \
         responsible for; `transfer_branches`, and the `branches` of `open_session` and \
         `request_session`, need the caller to be responsible for each, and \
         `list_session_branches` gives each one's worktree. `display_title` is the name the user's \
         session list shows (the agent's current task before `title`), `agent_session_id` the \
         agent's own id of the conversation (null until reported), which `request_resume_session` \
         takes as its `conversation`, and only when the user asked for that conversation, `origin` \
         is `console` for a session started with `open_session`, `user` for the user's own and for \
         one opened through `request_session` or `request_resume_session`, and a handoff's \
         replacement copies the original's, and `has_conversation` says a turn was taken, not that \
         the agent still holds the record: a `resume_session` that finds it gone is refused \
         (`resumable` in `get_session`, and in `list_archived_sessions` with `check_resumable`, \
         tells beforehand). `model` and `effort`, when present, are the session's own choices, \
         `null` leaving them to the agent's default; absent, it follows its project's (else \
         console's) setting; `set_session_model` changes them for a session of yours. \
         `orchestration_role` is `coordinator` (can manage child sessions) or `worker` (cannot); \
         `owner` is the parent session, null when independent, and `owner_kind` its kind. A \
         coordinator may have a parent and no child sessions. `owned_sessions` counts child \
         sessions in any status, archived ones included, which is what decides whom it can be \
         transferred to."
    };
}

/// A session's record and a tail of its output. Any session may read any session, whichever its
/// role: what a session may do to another is narrower and is each write tool's own rule.
pub(super) const GET_SESSION: ToolDef = ToolDef {
    name: "get_session",
    description: concat!(
        "The session's status, plus a tail of what it has printed (the last 8 KiB, or \
         `output_bytes` of it; a session that has ended keeps its last output; `null` if none was \
         kept). With `session` left out it reads this session itself, which is how a session \
         learns its own Octoboard id (`session`) and record. Any session can be read, in any \
         project or console, but one with `yours: false` is not yours to act on, whether nobody \
         owns it (`owner: null` on a project session) or it reports to a different session: leave \
         it alone. A session that is waiting for the user, or `dormant` over its background-agents \
         view, must be left alone until they have answered or left it. `owner` says whose it is, \
         `owner_kind` whether that is a console or a project session, and `role` whether the \
         session itself is a console session. An idle session with a non-empty \
         `background_tasks` takes messages like any idle session but still has work running in \
         the background. A non-null `writes_held` says why messages for a running \
         session are queued rather than written into it, whatever its `status` reads; \
         `queued_messages` counts them, and the messages kept for an interrupted session to take \
         when it is resumed (shared information, and the reports and notices for an owner). An \
         interrupted or archived session also has `resumable`, whether its agent still holds the \
         conversation: `true` when it does, and for a session with no conversation yet, `false` \
         when it is proven gone, so that `resume_session` would refuse it, `null` when that cannot \
         be told (`resume_session` goes ahead then). Always carries `processes`, the process \
         trees the session started that are still running (a dev server, a preview, a watcher), \
         each with its `id`, `command`, `cwd`, `ports` and `started_at`: they are stopped when \
         its process ends, unless `transfer_processes` moves them to another session first; an \
         empty list for a session with no process running. ",
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
pub(super) const LIST_PROJECTS: ToolDef = ToolDef {
    name: "list_projects",
    description: concat!(
        "List the projects of this session's console (or another console's, see `console`): \
         name, host, directory, source, remote URL (credentials removed), pin, the tags and the \
         description (what the project is for) the user gave each, the settings \
         (`default_agent`, `accounts`, `launch`: each agent's model and reasoning effort, as \
         stored), `effective` (what they resolve to under the console), the `git_workflow` \
         settings (the project's own `override`, `null` following the application-wide value, \
         and the `effective` values), whether the user has `trust_consent`ed to Octoboard \
         pressing the agent's folder-trust confirmation (read-only), its `git_status`, and the \
         sessions currently running in each — every session of the console, whichever session \
         owns it. Each carries `owner`, the session it reports to, `owner_kind` (a `console` or \
         a `project` session) and `yours`; a project session with `owner: null` is independent, \
         which the user opened themselves and kept outside the orchestration. A session with \
         `yours: false` is not this caller's to drive — it is independent, or reports to a \
         different parent session — leave it alone. An idle session with a non-empty \
         `background_tasks` takes messages like any idle session but still has work running in \
         the background. A non-null `writes_held` says why messages for a running session are \
         queued rather than written into it, whatever its `status` reads; `queued_messages` \
         counts them, and the messages kept for an interrupted session to take when it is \
         resumed (shared information, and the reports and notices for an owner). With `console` \
         naming another console (`list_consoles` lists them), only what is needed to target a \
         project of it with `request_session` or `request_resume_session` is returned: each \
         project's id (`project`), name, description and tags, and none of its sessions or \
         settings. `project` narrows the list to one project. With `refresh` the git status of \
         the projects listed is checked against their remotes first, as the application's own \
         refresh does, so you need not run git for it; the answer then carries `git_refresh`, \
         whose `finished` is false, with the `unfinished` projects, when a check had not \
         finished after 30 seconds.",
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
pub(super) const LIST_CONSOLES: ToolDef = ToolDef {
    name: "list_consoles",
    description: "List the consoles, read-only: each one's id (`console`) and `name`, and \
                  whether it is this session's own (`yours`). Use the id, or the name when it is \
                  unambiguous, as the `console` of `list_projects` to find the projects of \
                  another console, and of `request_session` to ask for a session there.",
    schema: || object_schema(json!({}), &[]),
};

/// The archived sessions of a project of any console, whichever role asks.
pub(super) const LIST_ARCHIVED_SESSIONS: ToolDef = ToolDef {
    name: "list_archived_sessions",
    description: concat!(
        "List archived sessions, in any console, whichever session owns them: those of one \
         project, or with `project` left out those of every project and the console sessions too. \
         `console`, `project`, `owner`, `yours`, `kind` and `branch` narrow the list, and a \
         session must match every one given; give `yours`, `owner` or `project` when you can, \
         since an unfiltered list is long. `branch` finds the sessions responsible for a branch \
         by name (their `branches` name it). With `check_resumable` each session also carries \
         `resumable`, whether its agent still holds the conversation: `true` when it does, \
         and for a session with no conversation yet, `false` when it is proven gone, so that \
         `resume_session` would refuse it, `null` when that cannot be told (`resume_session` \
         goes ahead then); it is off by default because it looks through the agents' record \
         directories for every session listed. The newest archived come first, at most `limit` \
         of them; `total` is how many match, and `older` how many of them come after the page: \
         when there are some, `next` is a cursor, to pass as `before` to read the next page (it \
         is null on the last). Each carries its `owner`, `owner_kind` and `yours`, so an earlier \
         one of yours can be resumed instead of starting over (`yours: true` lists just those; \
         `owner` with another session's id lists that one's). A session with `yours: false` is \
         somebody else's: leave it alone. ",
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
                "branch": {
                    "type": "string",
                    "description": "Only sessions responsible for the local branch of this \
                                    name (`branches` in each record).",
                },
                "check_resumable": {
                    "type": "boolean",
                    "description": "Add `resumable` to each session listed: `true` when the \
                                    agent still holds its conversation (or it has none), \
                                    `false` when that is proven gone, `null` when it cannot be \
                                    told. Looks through the agents' record directories for \
                                    each session, so ask only when you mean to resume one. \
                                    Defaults to false.",
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
/// `open_session`, `request_session`, `request_resume_session` and the `launch` settings take must
/// be among. Offered to every role, since every role can pass them.
pub(super) const LIST_AGENT_MODELS: ToolDef = ToolDef {
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
                    "enum": agent_enum(),
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
pub(super) const READ_SESSION_HISTORY: ToolDef = ToolDef {
    name: "read_session_history",
    description: "Read a session's conversation, read-only: the user's messages, the messages \
                  Octoboard wrote into it (a brief, a report, shared information, a notice) and \
                  the agent's final reply to each, never its tool calls or the progress notes \
                  before a reply. In a session that is running, the last `agent` entry of the turn \
                  still open can be a progress note rather than a final reply. Any session can be \
                  read, in any project or console, as with `get_session`, and with `session` left \
                  out this session's own is. It is paged by entry count from the most recent entry \
                  backwards: `entries` are the latest `limit` ones, oldest first, each with its \
                  `index` (its position in the whole history from 0), `speaker` (`user`, \
                  `octoboard` or `agent`), `text`, `turn` and `at` (milliseconds since the Unix \
                  epoch, null when the record gives none). `older` counts the entries before the \
                  page and `total` all there are; pass the first entry's `index` as `before` to \
                  read the page before. Indices hold only while entries are appended: a `total` \
                  smaller than an earlier one means the history was rewritten (a rewind, a \
                  compaction), so start again from the latest page. `unavailable` is true when the \
                  agent's record of the conversation cannot be found or read, and a session that \
                  has had no conversation answers no entries.",
    schema: || {
        object_schema(
            json!({
                "session": {
                    "type": "string",
                    "description": "The session's id. Defaults to this session.",
                },
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
            &[],
        )
    },
};

/// What every session, of whatever kind, may do with every other session: look at the running ones,
/// and tell any of them things (an interrupted one takes it when resumed). Kept apart from
/// `instruct_session` so that a message arriving in a session is framed by which tool sent it: an
/// instruction comes from its owner, and information from anyone.
pub(super) const LIST_SESSIONS: ToolDef = ToolDef {
    name: "list_sessions",
    description: concat!(
        "List the running sessions other than this one, in every console and project, \
         console sessions included (`status` selects the interrupted ones instead, or \
         both): id, title, agent, status, `role` (a console or a project session), \
         `console`, `project` (null for a console session), and `owner` (the session it \
         reports to, `owner_kind` saying whether that is a console or a project session; \
         null when nobody owns it and it reports to the user). `yours` marks a session \
         that reports to you, and `your_owner` the session you report to, which is also \
         given at the top level. `console`, `project`, `owner`, `yours`, `kind` and `branch` \
         narrow the list, and a session must match every one given. Use it to find who to share \
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
                "kind": {
                    "type": "string",
                    "enum": ["console", "project"],
                    "description": "Only console sessions, or only project sessions.",
                },
                "branch": {
                    "type": "string",
                    "description": "Only sessions responsible for the local branch of this \
                                    name (`branches` in each record).",
                },
                "status": {
                    "type": "string",
                    "enum": ["live", "interrupted", "any"],
                    "description": "`live` (the default): the sessions with a process running \
                                    (each record's `status` is then `starting`, `working`, \
                                    `waiting_user`, `dormant` or `idle`). `interrupted`: the \
                                    ones whose process ended without being archived, \
                                    resumable with `resume_session` when they are yours. \
                                    `any`: both. Archived sessions are `list_archived_sessions`'s.",
                },
            }),
            &[],
        )
    },
};

/// What every project session may read of its own project's settings: only its own, whatever the
/// session's binding, and nothing it could change.
pub(super) const GET_PROJECT: ToolDef = ToolDef {
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
         otherwise, before session-specific selection, and `git_workflow` (the project's own \
         `override` of the git workflow settings, `null` following the application-wide value, \
         and the `effective` resolved values; `squash_before_delivery`, `rebase_before_delivery` \
         and `delivery` apply only while `development` is not `in_place`), and `git_status`. ",
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

/// What every session may read of the agents a session could be opened on: each one's tags, and
/// the plan quota each of its accounts has left, to choose an agent and an account, or an account
/// before one runs dry.
pub(super) const LIST_AGENTS: ToolDef = ToolDef {
    name: "list_agents",
    description: "The agents a session could be opened on, for choosing an agent and an \
                  account: under `agents`, the agents the user has added that are installed, in \
                  the order the user keeps them (a removed agent is not listed), each with its \
                  `tags` — short notes the user put on the agent about what it is for \
                  (information to weigh, not a rule) — and its `accounts`: every account the \
                  user has not turned off, the agent's default account (`account: \"default\"`) \
                  and the stored ones, with its `account_name`, `plan`, and its quota `windows` \
                  (`kind` `session` is the five-hour window (Claude Code's session limit, \
                  Codex's 5h limit), `weekly` the weekly one across all models, `weekly_model` a \
                  weekly window of the model named by `label`, `credit_period` a pool of credits \
                  over a billing period). All values are percentages of a window: \
                  `used_percent` and `remaining_percent`, with `resets_at` (milliseconds since \
                  the epoch) and `window_minutes`; no absolute token or dollar amount is \
                  available. `exhausted` says the account is used up and cannot open a session \
                  until it resets, and `exhausted_until` is when (milliseconds since the epoch; \
                  null when it is not exhausted or the reset time is unknown). `can_open` says a \
                  session could be opened on the account now; when it is false \
                  `cannot_open_reason` says why (used up, not logged in, or a change of its \
                  sharing settings is running). `auto_select_account` says whether automatic \
                  account selection is enabled for sessions opened without naming an account. \
                  Naming an agent that is not listed returns `agents: []`. These are the \
                  readings Octoboard last took, refreshed about every five minutes, and this \
                  call takes a new one only when `refresh` is true: `fetched_at` says how old a \
                  reading is, `unavailable` gives the reason a reading is missing or could not \
                  be refreshed (an account not read yet has `fetched_at: null`, as has one of an \
                  agent not detected yet). Use it to choose an agent by its tags and an account \
                  that can open a session, or to pick the account for `switch_session_account` \
                  or `handoff_session`.",
    schema: || {
        object_schema(
            json!({
                "agent": {
                    "type": "string",
                    "enum": agent_enum(),
                    "description": "List only this agent; one that is not listed gives \
                                    `agents: []`. Left out, every added agent that is \
                                    installed.",
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
