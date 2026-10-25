# What every session reads

The read-only tools of Octoboard's MCP server (see `docs/product/hub-orchestration.md`). Every session — a console
session, a project session that can own sessions and a project session with the role of a bound one alike — is offered
the same ones, and each reads the same for every caller: any session, of any project or console, and the projects and
archives of any console.

| Tool | Arguments | What it does |
|---|---|---|
| `get_session` | `session`, `output_bytes?` | The record of any session, of any console — status, title, agent, whether it is a console session, its console and project, which session owns it and whether that owner is a console session or a project session (or that it is unbound), whether it is the caller's, timestamps, its account, the model and effort it is seen running with, whether it is pinned or locked, and the names of the branches it is responsible for — plus a tail of what it has printed (see "Session records" below). |
| `list_sessions` | `console?`, `project?`, `owner?`, `yours?` | The running sessions other than the caller, in every console and project, optionally narrowed. See `docs/product/information-between-sessions.md`. |
| `read_session_history` | `session`, `limit?`, `before?` | What any session's conversation held, a page at a time from the latest. See "Reading a session's history" below. |
| `get_session_branches` | `session?` | The git branches any session is responsible for, each with its project and worktree. See "Reading the branches a session is responsible for" in `docs/product/branch-responsible-session.md`. |
| `list_agent_models` | `agent`, `account?` | The models an agent offers and the reasoning-effort levels of each. See "Reading the models an agent offers" below. |
| `list_consoles` | — | Every console, each as `console` (its id), `name` and `yours` (whether it is the caller's own console). The id, or a name that names one console, is what a `console` argument takes. |
| `list_projects` | `console?` | The projects of a console: the caller's own in full, another's only as far as needed to target one of its projects. See "Listing a console's projects" below. |
| `list_archived` | `project` | The archived sessions of any project of any console, whoever owns them, each with its owner. The project is named by its id, in any console, or by its name — looked for among the caller's own console's projects first, else among every console's; a name that names more than one project is refused. |
| `get_account_quotas` | `agent?`, `refresh?` | Each account's plan quota, optionally read again first. See "Reading quotas from a session" in `docs/product/account-quotas.md`. |

The reads are not narrowed by role because sessions of any kind can work in the same project, and so the same working
directory, at once — two console sessions can dispatch into it, say — and one that could not see the others' sessions
would collide with them. That holds for a bound session as much as for one that can own sessions, so a bound session
is offered every read too, at the cost of a few more tool descriptions in its context. What stays narrower is a read of
settings — `get_project` is a project session's read of its own project (see `docs/product/agent-settings-tools.md`),
and `get_console` and `get_settings` are the console session's (see "The console session's tools" in
`docs/product/hub-orchestration.md`) — and `list_pages`, a console session's read of the report panel pages it pushed
itself (see "Reading back the pages pushed" in `docs/product/report-panel.md`). What a session may do to another is
narrower too (see "Which sessions an owner drives" in `docs/product/hub-orchestration.md`). The tool descriptions, the
console session's instructions and every project session's role description tell it that a session it does not own is
somebody else's and is to be left alone, whatever it can read of it.

## Listing a console's projects

`list_projects` takes an optional `console`, by id or by a name that names one console (`list_consoles` lists them);
a name that names none, or more than one, is refused. Left out, it is the caller's own console — for a project session,
the console its project is in. Both answers carry `console`, the id of the console listed, and `projects`:

- **The caller's own console**: each project with its settings (see "A project's settings" in
  `docs/product/agent-settings-tools.md`) and the sessions currently running in it — every session of the console,
  whichever session owns it — each a session record as in "Session records" below.
- **Another console**: each project only as `project` (its id), `name`, `description` and `tags` — what
  `request_session` and `request_resume_session` need to target one of its projects — with no sessions and no
  settings.

## Reading a session's history

`read_session_history` reads the conversation of any session, running, interrupted or archived, from the agent's own
record — the same record the history view reads (see `docs/product/session-history.md`), read afresh on each call. It
holds the user's messages, the messages Octoboard wrote into the session — a brief, a report, shared information, a
notice — and the agent's final reply to each turn. Unlike the view, it hides none of Octoboard's messages, marking them
as Octoboard's instead; everything else the view leaves out (see "What the history lists" there), tool calls among it,
is left out here too.

It takes `session` and two optional arguments, which page the history by entry count from the most recent entry
backwards:

- `limit`, how many entries to return: a whole number from 1 to 100, 20 when left out;
- `before`, an entry's `index`: only the entries before it are returned. A whole number, 0 or more, and not past the
  number of entries the history has. Left out, the latest entries are returned.

Anything else, and a session that does not exist, is refused. The answer is:

| Field | Contents |
|---|---|
| `session` | The session's id. |
| `unavailable` | `true` when the agent's record of the conversation cannot be found or read; `total` and `older` are then 0 and `entries` empty. A session that has had no conversation answers `false` with no entries. |
| `total` | How many entries the whole history has. |
| `older` | How many entries come before the page. |
| `entries` | The page, oldest first. Each entry is `index` (its position in the whole history, from 0), `speaker` (`user`, `octoboard` or `agent`), `text`, `turn` and `at` (milliseconds since the epoch, `null` where the record gives no time). A `user` or `octoboard` entry opens a turn, numbered from 1, and the `agent` reply after it shares its number; a turn with no reply has no `agent` entry. |

The first entry's `index`, passed as `before`, reads the page before. **Indices hold only while entries are
appended.** A compacted Grok session shows only the turns since the compaction, and a Claude Code rewind can replace
entries, so a `total` smaller than one read earlier means the history was rewritten, and the reader starts again from
the latest page. In a running session, the last `agent` entry of the turn still open can be a progress note rather than
a final reply.

## Reading the models an agent offers

`list_agent_models` takes `agent` (`claude`, `codex` or `grok`) and an optional `account`: an account of that agent by
id or by name, or `default`; left out, the agent's default account. An account that does not exist, or belongs to
another agent, is refused. It lists what the model pickers list for that account (see "The models offered" and "The
reasoning effort offered" in `docs/product/model-and-reasoning-effort.md`), from the same answers the pickers reuse, so
a call runs nothing while an answer is fresh. The answer is `agent`, `account` (the account's id, or `default`),
`models` and `unavailable`:

- each model is `id` — what a `model` argument takes — `name` and `efforts`, the levels an `effort` argument takes for
  that model;
- `unavailable` is `null`, or, when the agent cannot be asked, `code` and `message` saying why (as in "While the list
  is being fetched, and when there is none" there), with `models` empty.

The description of every `model` and `effort` a tool takes — `new_session`, `request_session`,
`request_resume_session`, `handoff_session`, and the `launch` settings of `add_project`, `update_project` and
`update_console` — points to it: a value given there must be one it lists, under the rules in "The Advanced section"
in `docs/product/model-and-reasoning-effort.md`.

## Session records

`get_session` returns the session's most recent output in `recent_output`, with terminal escape
sequences and cursor-control bytes stripped and runs of blank lines collapsed, so it is readable
prose rather than a rendered frame. How much is set by `output_bytes`, a whole number from 1 to 65536 (64 KiB),
counted in bytes of the raw output before anything is stripped; left out, it is 8192 (8 KiB). A value outside those
bounds, or one that is not a whole number, is refused rather than clamped, so a caller is never left believing it read
more than it did. Output shorter than what is asked for is returned whole. For a session whose process has ended, an archived or
interrupted one, that is the output the daemon kept when the process ended, so the field reads the
same whether the session is running or not. It is `null` only where the daemon never kept any
output — no process of the session has printed anything yet, it ended with the daemon killed
outright before the output was kept, or a new process has since taken the session over and
discarded it (see "`GET /ws/term/:session` — terminal stream" in `apps/daemon/PROTOCOL.md`). A
session that is waiting for the user carries a note saying so.

Every session record the session tools give — `list_projects`' sessions, `get_session`, `list_sessions`,
`list_archived`, and the record `rename_session` and `set_session_pinned` answer with — carries `owner` (the id of the
session it is bound to, or null when it is unbound — always null for a console session, which is never bound),
`owner_kind` (`console` or `project`, the kind of session that owner is; null when it is unbound) and `yours` (whether
that owner is the caller).

Each record also carries `background_tasks`: for a Claude Code
session working in background, the work it left running, each entry with `id` and, where Claude Code gives them,
`type` and `description`; an empty list for every other session. Such a session's `status` is `idle`: it takes a
message like any session awaiting instructions, but its work is not over (see "Working in background" in
`docs/product/sessions.md`).

Each of these records also carries `context`, the session's context size as the window shows it (see "Session context
size" in `docs/product/session-context.md`): an object with `tokens`, plus `window` and `used_percent` — the share of
the window in use as a whole percentage, by the agent's own accounting as in "The context-size chip" there — only once
a context window is known, both keys left out otherwise. It is `null` while the context size is unknown: before the
agent's records show one, and for Claude Code after a compaction until the next request.

```json
"context": {"tokens": 120000, "window": 200000, "used_percent": 60}
```

The `status` these records carry is the one the window shows: a running session reads `waiting_user` while a
folder-trust confirmation that needs the user, a Codex action title or a screen only the user can answer holds it there,
whatever its agent last reported (see "What the statuses are derived from" in `docs/product/sessions.md`), and one that
a change of its account's sharing has ended and is about to resume reads `starting` (see "When a change is carried out"
in `docs/product/sharing-the-default-configuration.md`). Each record also carries `queued_messages` and `writes_held`,
which say how many messages are waiting to be written into the session — information kept for an interrupted session
until its resume included — and what keeps them from being written whatever its `status` reads (see "Messages held until a session can take them" in `docs/product/message-delivery.md`).

Each record also carries the facts the tools that act on a session depend on, so a caller can check them beforehand:

- `account`, the account the session runs on — the account's id, or `default` for its agent's default account, as
  `get_account_quotas` and `switch_session_account` name accounts — and `account_name`, that account's name: for the
  default account the name the user gave it, else `Default` (see "The default account" in
  `docs/product/agent-accounts.md`); for an account removed since, its id. The account's directory is never given.
  `switch_session_account` refuses the account the session is on already.
- `observed_model` and `observed_effort`, the model and reasoning effort the agent's records show the session running
  with, each `null` until the records show one, and `default_effort`, the level the agent runs that model at by itself:
  given only where the records name the model but no effort and that level can be worked out, else `null` (see "The
  model and effort a session is shown running with" in `docs/product/model-and-reasoning-effort.md`).
- `pinned`, whether the session is pinned (see "Pinning" in `docs/product/order-of-projects-and-sessions.md`), and
  `locked`, whether the user locked it (see "Locking a session" in `docs/product/sessions.md`); `archive_session`
  refuses a locked session.
- `branches`, the names of the git branches the session is responsible for (see "The responsible session" in
  `docs/product/branch-responsible-session.md`): an empty list when it is responsible for none, and always for a
  console session. These are the branches `handover_branches`, and the `branches` of `new_session` and
  `request_session`, need the caller to be responsible for; where each is checked out is `get_session_branches`'
  answer. Reading a record runs no `git`.
