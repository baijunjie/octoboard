# Orchestration

Orchestration is one session starting other sessions, handing them work, following them up and taking their reports.
The session doing it is the **owner** of the sessions bound to it; every session it starts is bound to it and reports
to it. Three kinds of session can be an owner:

- A **console session** — a session the user brings a request to; a console may run several of them at once (see
  "Several console sessions per console" below). It works out which project a request belongs to, starts sessions
  there, follows them up, and summarizes what they came back with. **It changes no project itself**: neither a file in
  a project, nor a command that creates, edits, moves or deletes anything in one or changes its repository — every
  change goes through a session it starts. Reading a project is legitimate, and is how a console session checks what a
  session reports. Besides its terminal it has one surface of its own for showing the user something — the report
  panel, described in `docs/product/report-panel.md`.
- An **unbound project session** — a project session bound to no owner: one the user opened by hand without binding it
  to a console session, or one started on another session's request (see `docs/product/session-requests.md`). It
  works in its own project and can also start sessions in that project and drive them (see "The unbound project
  session's tools" below).
- A **lead session** — a project session that was unbound and bound itself to a console session of its console,
  keeping the sessions it had started (see "Lead sessions" below).

Every session can also ask the user to start a session that is nobody's — in any project of any console, or a console
session — which reports to the user and which the caller does not drive (see `docs/product/session-requests.md`).

The sessions an owner starts are ordinary project sessions, described in `docs/product/sessions.md`. Archiving,
resuming and deleting an owner follow the same rules whichever kind it is (see "Archiving, interruption and resuming"
in `docs/product/sessions.md` and "Deleting archived sessions" in `docs/product/deleting-sessions.md`).

Octoboard injects the tools into the session. A console session gets the full set (see "The console session's tools"
below); a project session that can own sessions — an unbound one or a lead session — a narrower set, scoped to its own
project for what it starts and drives (see "The unbound project session's tools" below); and a project session with
the role of a bound one — bound to an owner and not a lead session — gets one tool back the other way, `report`, and
cannot start or drive sessions. Every session, whatever its kind, also gets the two tools for sharing information with
every other running session (see `docs/product/information-between-sessions.md`) and `request_session`; every project session
also gets `get_project`, a read-only view of its own project's settings (see `docs/product/agent-settings-tools.md`).
**Which tools a session sees follows from its role, fixed for its lifetime, and from its binding as it stands**, which
can change while the session runs (see "Changing a session's owner" below). A console session cannot report, and
orchestration is at most two levels deep: a console session, a lead session bound to it, and the lead session's own
sessions.

A session is offered the tools of the Octoboard that launched it, for as long as that agent process runs: a tool
Octoboard gains later reaches a running session only once it is next launched — resumed, reopened or moved to another
account.

## Several console sessions per console

A console may run any number of console sessions at once, each with its own sessions reporting to it
(see "Reporting" below): a session reports to the console session that is named in its own binding,
not to "the" console session of its console. How a console session is archived, and where archived
console sessions are listed, are in "Console sessions and project sessions" in
`docs/product/sessions.md`.

## The console session's tools

| Tool | Arguments | What it does |
|---|---|---|
| `list_projects` | — | The console's projects, each with its settings (see "A project's settings" in `docs/product/agent-settings-tools.md`) and the sessions currently running in it — every session of the console, whichever session owns it. |
| `add_project` | `source` (`local` / `parent` / `git`), `path?`, `remote_url?`, `name?`, `description?`, `default_agent?`, `tags?`, `accounts?`, `launch?`, `pinned?` | Associates one or more projects with this console, under the same rules as the user's own form (see "Associating a project" in `docs/product/consoles-and-projects.md`) and a stricter one for a URL to clone (see "Adding a project with its settings" in `docs/product/agent-settings-tools.md`). Answers with the settings of the projects it added. |
| `update_project` | `project`, `name?`, `description?`, `tags?`, `add_tags?`, `remove_tags?`, `pinned?`, `default_agent?`, `accounts?`, `launch?` | Changes one of this console's projects' settings. See "Changing a project: `update_project`" in `docs/product/agent-settings-tools.md`. |
| `get_console`, `update_console` | — / `name?`, `default_agent?`, `project_session_agent?`, `accounts?`, `launch?`, `project_session_launch?` | Reads or changes the calling console session's own console. See "The console's own settings: `get_console` and `update_console`" in `docs/product/agent-settings-tools.md`. |
| `get_settings`, `update_settings` | — / `auto_pull_repositories?`, `default_clone_dir?` | Reads or changes the application-wide settings. See "Application settings: `get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md`. |
| `new_session` | `project`, `brief`, `agent?`, `account?`, `model?`, `effort?` | Starts a session in one of this console's projects, bound to the calling console session for its whole life, and hands it the brief as its opening prompt. Needs no approval. `agent`, `account`, `model` and `effort` override those for that one session, as the session dialog's choices do (see "Opening a session" in `docs/product/sessions.md`): `account` is an account by id or name, or `default` for the agent's default account, and must belong to the agent the session will run; a blank `model` or `effort` is the agent's own default, and any other must be one the agent offers (see "The Advanced section" in `docs/product/model-and-reasoning-effort.md`). Answers with the new session's id and its agent. Refused, with the reason in prose, when the session's resolved agent has been determined unavailable on this machine — never while that determination is still pending. |
| `send_message` | `session`, `text` | Appends an instruction to a running session of the caller's. Answers with whether it was written or queued. |
| `get_session` | `session` | The record of any session, of any console — status, title, agent, whether it is a console session, its console and project, which session owns it and whether that owner is a console session or a project session (or that it is unbound), whether it is the caller's, timestamps — plus a tail of what it has printed. |
| `rename_session` | `session`, `title` | Changes the title of one of the caller's sessions, under the rules of "Renaming a session" in `docs/product/sessions.md`. Answers with the session's record. |
| `set_session_pinned` | `session`, `pinned` | Pins or unpins one of the caller's sessions (see "Pinning" in `docs/product/sidebar.md`). Answers with the session's record. |
| `archive_session` | `session` | Ends the process of one of the caller's sessions and archives it. Once the process has ended, the git branches the archived sessions were working on, and the linked worktrees that have them checked out, are removed where git can do so without force; the answer waits for that and names in `branches_left` what stays, and why (see "Archives made by an agent" in `docs/product/branch-and-worktree-cleanup.md`). |
| `archive_self` | — | Ends the caller's own process and archives the caller, once every session under it is archived, removing its branches and worktrees as `archive_session` does. See "A session archiving itself" below. |
| `list_archived` | `project` | The archived sessions of one of this console's projects, whoever owns them, each with its owner. |
| `reopen_session` | `session`, `text?` | Relaunches an archived or interrupted session of the caller's, continuing its conversation, and optionally hands it an instruction — its next task — delivered once the relaunched session can take one. The caller is the console session the session is bound to and is running, so reopening one never brings a console session back. |
| `transfer_session` | `session`, `to`, `note` | Hands one of the caller's running sessions to another owner, without the user. See "Handing a session to another owner" below. |
| `request_session` | `kind`, `project?`, `console?`, `brief`, `agent?`, `account?`, `model?`, `effort?`, `branches?` | Asks the user to start a session that is nobody's. See `docs/product/session-requests.md`. |
| `list_sessions`, `share_info` | see `docs/product/information-between-sessions.md` | As for every session. |
| `show_page` | `html` | Pushes an HTML page to the calling console session's report panel, beside the console session's own terminal. Answers with the new page's id. See `docs/product/report-panel.md`. |

A project is named either by its id or by its name where that name is unambiguous within the
console; an ambiguous name is refused and asks for the id. A project of another console is refused, except by
`request_session`, which may name any project.

`get_session` returns at most 8 KiB of the session's most recent output, with terminal escape
sequences and cursor-control bytes stripped and runs of blank lines collapsed, so it is readable
prose rather than a rendered frame. For a session whose process has ended, an archived or
interrupted one, that is the output the daemon kept when the process ended, so the field reads the
same whether the session is running or not. It is `null` only where the daemon never kept any
output — no process of the session has printed anything yet, it ended with the daemon killed
outright before the output was kept, or a new process has since taken the session over and
discarded it (see "`GET /ws/term/:session` — terminal stream" in `apps/daemon/PROTOCOL.md`). A
session that is waiting for the user carries a note saying so.

**Reads are wide, writes are the caller's own.** `list_projects` and `list_archived` cover the sessions of the
console's own projects, whoever owns them; `get_session` reads any session of any console, and `list_sessions` lists
every running one. Each carries `owner` (the id of the session it is bound to, or null when it is unbound — always
null for a console session, which is never bound), `owner_kind` (`console` or `project`, the kind of session that owner
is; null when it is unbound) and `yours` (whether that owner is the caller). `send_message`, `rename_session`,
`set_session_pinned`, `archive_session`, `reopen_session` and `transfer_session` act only on a session bound to the
caller; for any other — unbound, bound to another console session or to a project session, a console session, or the
caller itself — they refuse, with a reason naming who owns it, and change nothing. `archive_session` aimed at the
caller itself is refused pointing to `archive_self` instead. `new_session` always binds the new
session to the caller, so a console session cannot start one on another's behalf: it can hand one over afterwards with
`transfer_session`, or ask the user for one that is nobody's with `request_session`. The reads are not narrowed because
two console sessions can dispatch into the same project, and so the same working directory, at once; one that could not
see the other's sessions would collide with them. The tool descriptions and the console session's instructions tell it
that a session it does not own is somebody else's and is to be left alone.

A refused call comes back to the agent as a **tool error carrying the reason in prose**, not as a
transport failure — "this session is waiting for the user" is advice the model is meant to act on.

## The unbound project session's tools

A project session that can own sessions — an **unbound project session**, one the user opened by hand without
choosing a console session under "Report to" (see "Opening a session" in `docs/product/sessions.md`) or one started on
another session's request (see `docs/product/session-requests.md`), and a **lead session** — can start sessions in its
own project and drive them, the way a console session drives its console's:

| Tool | Arguments | What it does |
|---|---|---|
| `new_session` | `brief`, `agent?`, `account?`, `model?`, `effort?` | Starts a session in the caller's own project, bound to the caller for its whole life, and hands it the brief as its opening prompt, exactly as the console session's `new_session` does, with the same overrides (see "Handing out a task: the brief" below). Needs no approval. There is no `project` argument; a call that names another project anyway is refused before anything is launched. `agent` may name any agent, not only the caller's own. Answers with the new session's id and its agent. Refused, with the reason in prose, when the session's resolved agent has been determined unavailable on this machine — never while that determination is still pending. |
| `send_message` | `session`, `text` | As the console session's. |
| `get_session` | `session` | As the console session's: any session, of any project or console. |
| `archive_session` | `session` | As the console session's. |
| `archive_self` | — | As the console session's. Offered to a lead session too, but refused for it: it wraps up with `report` instead (see "A session archiving itself" below). |
| `reopen_session` | `session`, `text?` | As the console session's. |
| `rename_session` | `session`, `title` | As the console session's. |
| `set_session_pinned` | `session`, `pinned` | As the console session's. |
| `transfer_session` | `session`, `to`, `note` | As the console session's. See "Handing a session to another owner" below. |
| `bind_to_session` | `owner`, `summary` | Binds the caller, while it is unbound, to an owner, without the user. Refused for a lead session, which is bound already. See "Binding an unbound session to an owner" below. |
| `request_session` | as the console session's | Asks the user to start a session that is nobody's. See `docs/product/session-requests.md`. |
| `report` | as in "Reporting" below | Offered, but refused while the session is unbound: it has nobody to report to. A lead session's `report` is accepted and goes to its console session (see "Lead sessions" below). |
| `list_sessions`, `share_info` | see `docs/product/information-between-sessions.md` | As for every session. |
| `get_project` | — | Its own project's settings, read-only, as for every project session (see "A project's settings" in `docs/product/agent-settings-tools.md`). |

It has no `list_projects`, `list_archived`, `add_project` or `show_page`, and none of the tools that change a setting:
`update_project`, `get_console`, `update_console`, `get_settings` and `update_settings` are the console session's alone.

**Reads are wide, writes are the caller's own** — the console session's rule. `get_session` reads any session of any
project or console, carrying `owner`, `owner_kind` and `yours` as above. `send_message`, `rename_session`,
`set_session_pinned`, `archive_session`, `reopen_session` and `transfer_session` act only on a session bound to the
caller, and refuse, with a reason naming who owns it, and change nothing, for any other: an unbound session, one bound
to a console session or to another project session, the caller itself, or a console session. For an unbound project
session, `archive_session` aimed at itself is refused pointing to `archive_self` instead.

A session a project session starts is bound to it, so it has the role of a bound one: it is offered `report`,
`list_sessions`, `share_info`, `get_project` and `request_session`, and cannot start or drive sessions of its own, nor
read one with `get_session`. It reports to the project session that started it exactly as a console session's sessions
report to their console session: see "Reporting" below.

## Lead sessions

A **lead session** is a project session bound to a console session of its console that can still own sessions of its
own project. A session becomes one by binding itself, while unbound, to a running console session with
`bind_to_session` (see "Binding an unbound session to an owner" below). One with no console session to bind to can
first ask the user for one with `request_session`, of kind `console`, and bind to it once it has been started (see
`docs/product/session-requests.md`). A session that takes a lead session's place in a handoff is one too (see
`docs/product/handing-off-sessions.md`). A session opened bound to a console session, or started by one, is not a lead
session: it has the role of a bound one.

- **It keeps its sessions.** The sessions it had started stay bound to it, and it goes on driving them with the
  unbound project session's tools (see "The unbound project session's tools" above), starting more in its own project
  as before.
- **It keeps what it was launched with.** Every launch of it — a resume, a reopen, a switch of account — gives it the
  unbound project session's role description and tool list, as at its first launch, not those of a session bound to a
  console session.
- **It reports to its console session**, as any session bound to one does: its `report` is accepted and delivered
  there. Its `done` report with no open items is refused while its own sessions are unfinished and automatic archiving
  is not blocked by a lock (see "Automatic archiving" below), and no report is
  synthesised for it while they run (see "Reporting" and "When a session does not report" below).
- **Its console session can hand it on.** Handed to another console session of the console it stays a lead session,
  keeping its sessions. It can be handed to a project session only while it has no sessions of its own, archived ones
  included; it then has the role of a bound session and can own none (see "Handing a session to another owner" below).
- **Its sessions stay ordinary bound sessions.** A session bound to a lead session has the role of a bound one and can
  never be an owner: naming one as the owner of a new session, or binding or handing a session to one, is refused.
  There is no third level and no cycle.

A session is **under** an owner when it is bound to it, or bound to a lead session that is bound to it. Archiving,
reopening and deleting follow the bindings across both levels; see "Archiving, interruption and resuming" in
`docs/product/sessions.md` and "Deleting archived sessions" in `docs/product/deleting-sessions.md`.

**The console session sees the lead session's sessions but does not drive them.** A lead session's `owner_kind` reads
`console`; its own sessions read `project`, with the lead session as their `owner`. They appear in the console
session's `list_projects`, `get_session` and `list_sessions`, not as its own; `send_message`, `rename_session`,
`set_session_pinned`, `archive_session`, `reopen_session` and `transfer_session` aimed at one are refused, naming the
lead session as its owner (see "Which sessions an owner drives" below). The lead session itself is bound to the console
session and is driven by it like any other session of its own.

## Changing a session's owner

A binding can change after a session was opened in two ways that sessions make themselves, neither of which asks the
user: an owner hands a session of its own to another owner with `transfer_session`, and an unbound project session
binds itself to an owner with `bind_to_session`. The rules and messages below are theirs. The one other change is a
handoff to another agent, which binds every session the original owns to the receiving session (see
`docs/product/handing-off-sessions.md`). A binding is never cleared: a bound session does not become unbound again.
Every connected window shows the change at once, the session moving to its new place in the sidebar and focus modes.

### Who can own a session

Every binding made this way keeps the same rules. The new owner must be:

- a **console session of the session's own console**, or a **project session of the session's own project that can own
  sessions** — an unbound one or a lead session. A project session with the role of a bound one owns none, so
  orchestration stays at most two levels deep, and no binding can close a cycle;
- **running** — it has a process, and is neither interrupted nor archived;
- **not the session itself.**

A session that has sessions of its own — archived ones included, since its owner can reopen them — can only be bound to
a console session: bound to a project session it could own none. A call that breaks a rule is refused, with the reason
in prose, and changes nothing; so is one whose binding changed while it was being made — the session bound elsewhere
meanwhile, it gained sessions of its own, or the owner can no longer own sessions — which says to look again before
retrying.

### Handing a session to another owner

`transfer_session` is offered to every session that can own sessions: a console session, an unbound project session and
a lead session.

| Argument | Required | Contents |
|---|---|---|
| `session` | yes | The session to hand over: one of the caller's (`yours: true`). |
| `to` | yes | The new owner's session id. |
| `note` | yes | The handover, for the new owner: what the session is doing, where it stands and what is left. |

Besides the rules above, it is refused for a session that is not the caller's (as for `send_message`), for one with no
process running or still starting — it is to be told of its new owner, so it has to be reopened or its launch has
to settle first — and for handing a session to the
caller itself.

Once handed over, the session is bound to the new owner and reports to it, and the caller no longer drives it. Nothing
else is moved: the sessions bound to it stay bound to it. A lead session handed to a console session stays a lead
session; handed to a project session it has the role of a bound one. An ordinary bound session keeps that role
wherever it goes.

Both sessions concerned are told, by messages written through the delivery every message takes (queued while one cannot
take a message; see "Messages held until a session can take them" in `docs/product/message-delivery.md`):

- **The session** is told its owner has changed — which session handed it over to which, by kind, id and title — that it
  takes work only from the new owner, or from the user typing in its terminal, and reports to it with `report`, that
  this replaces what it was told of its owner when it was started, and that the earlier owner no longer drives it. A
  session that has lost the tools for owning sessions is also told it can no longer start or drive sessions, with the
  list of the Octoboard tools it has left.
- **The new owner** is told which session handed which session over to it, by id, title and project, that the session
  is its own from now on and is driven with `send_message`, `archive_session` and `reopen_session`, and, for a lead
  session, that it keeps its own sessions, which the new owner can read but not drive; then the caller's `note`, every
  line quoted, followed by a closing line.

Neither message spares the turn it starts a synthesised report: a session that carries on and stops without reporting is
reported on to its owner as usual (see "When a session does not report" below).

The call returns `session`, `owner` (the new owner's id), `owner_kind` (`console` or `project`) and a `note` saying the
session now reports to the new owner and is no longer the caller's, and whether each of the two was told, will be told
as soon as it can take a message, or could not be told, with the reason. When the new owner could not be told, the note
says to tell it of the handover with `share_info`. Once the binding has changed the handover stands, whatever became of
the messages.

### Binding an unbound session to an owner

`bind_to_session` is offered to a project session that can own sessions, and refused, with the reason in prose, once
the caller is bound: a lead session, bound already, cannot call it to move to another owner.

| Argument | Required | Contents |
|---|---|---|
| `owner` | yes | The owner's session id. |
| `summary` | yes | What the caller is doing and what it needs from the owner, which reaches the owner as its first report. |

Bound to a **console session**, the caller becomes a **lead session**: it keeps the sessions it started and goes on
driving them (see "Lead sessions" above). Bound to a **project session** — which only a session with no sessions of its
own may — it becomes an ordinary bound session, with the role of a bound one: it can no longer start or drive sessions.

**The summary reaches the owner as the caller's first report**, through the ordinary reporting path (see "Reporting"
below), with the status `needs_decision` and no open items. The report's summary opens by saying the project session
bound itself to the owner, is the owner's to drive from now on and reports to it — and, for a lead session, that it
keeps the sessions it started and drives them itself, which the owner can read but not drive — then gives the caller's
`summary`. It counts as the caller's report for its current turn, so that turn ending without a `report` produces no
synthesised report (see "When a session does not report" below).

The call returns `owner`, `owner_kind` and a `note` telling the caller that it is now bound to that owner (as its lead
session, where it is one) and whether its summary was sent as its first report; that from now on the owner is the one
it takes work from, or the user typing in its terminal, and it reports to it with `report` once per round of work,
which replaces what its role description says about being unbound and not calling `report`; that `done` with no open
items archives it, subject to "Automatic archiving" below — refused for a lead session while a session it started
is not archived and no lock blocks automatic archiving — while `needs_decision` or
open items leave it running; that a lead session keeps driving its own sessions with `new_session` and the tools that
go with it, and that any other can no longer start or drive sessions, with the list of the Octoboard tools it has left;
and not to call `bind_to_session` again. Once the caller is bound, the binding stands: a summary that could not be
delivered is reported in the note, with the reason, and the caller is told to send it with `report` itself.

### When a binding changes while a session runs

A session's role description is written when it launches, from who it reports to then, and two of the agents keep what
a conversation was first given on every resume (see "What is injected on every launch" in
`docs/product/launching-agents.md`). So a change of owner is told to the session in prose instead — in the message a
handover or a handoff writes into it, or in the result of its own `bind_to_session` call — and its role description
says that what
Octoboard tells it of a later change replaces what it says of its owner.

The tools follow the binding as it stands. Octoboard checks every call against it, so a tool the session no longer has
is refused even where its agent still lists it. The session's tool list itself is updated with the answer to its next
call of any Octoboard tool, and its agent is told the list has changed (the MCP `tools/list_changed` notice); whether an
agent reads the list again on that notice is the agent's own. Only two changes take tools away — binding to a project
session, and a lead session handed to one — and they leave the bound session's set: `report`, `list_sessions`,
`share_info`, `get_project` and `request_session`.

## Handing out a task: the brief

`new_session` takes a `brief` of four natural-language fields:

| Field | Required | Contents |
|---|---|---|
| `goal` | yes | The outcome to achieve. |
| `context` | no | Background and relevant leads. |
| `acceptance` | no | Criteria for being done. |
| `constraints` | no | What must not be touched, whether committing or pushing is allowed, and so on. |

Octoboard renders the brief into the session's opening prompt from a **fixed template** — one
section per field, in the order above, headed `## Goal`, `## Context`, `## Acceptance`,
`## Constraints`. A field the caller left out or left blank is omitted entirely rather than
sent as an empty heading.

A session started with `new_session` is **titled from its goal** rather than from its project — the
goal's first non-blank line, shortened to roughly 48 characters on a word boundary with an ellipsis —
so several sessions dispatched into one project can be told apart in the menu.

A session started with `new_session` in a folder its agent has not been told to trust can first stop
on that agent's own trust confirmation. When the user has given that project the trust permission, or
has trusted a folder its directory lies under, Octoboard presses the confirmation and the session
carries on without them; otherwise the user is asked in a dialog — again after the application
reconnects, if the confirmation is still waiting — and the session waits on it until it is answered,
reading as *working* meanwhile. A trusted folder covers the projects the console session itself
associates in it, a repository it clones there included: their sessions' confirmations are pressed
without the user being asked, whichever agent shows them, and that agent then applies the
repository's own configuration — for Claude Code, the permission rules and hooks in its
`.claude/settings.json` — without asking. See "The trust permission" and "Trusted folders" in
`docs/product/folder-trust.md`.

## Reporting

A bound project session reports a round of work with `report`:

| Argument | Required | Contents |
|---|---|---|
| `summary` | yes | What was done and what came of it, in natural language. |
| `status` | yes | One of `done`, `failed`, `needs_decision`. No other value is accepted. |
| `open_items` | no | Strings naming what is left unfinished. Empty when nothing is. |

The report is written into the session the reporting session is **bound to**, its **owner** — a console session
or a project session — named by its own binding rather than looked up, since a console may
hold several console sessions. It arrives as a user message naming the reporting session's id, its title and project,
the status, the open items, and then the summary. The reporting session is told either that it was delivered or that
it was accepted and will reach its owner as soon as the owner can take a message, the note calling the owner a console
session or a project session according to which it is.

Reporting fails, and leaves the session exactly as it was, when:

- the session is unbound, so there is nobody to report to;
- its owner is no longer on record;
- its owner has no process running — it is interrupted or archived. The binding names the owner by its id and
  outlives this, so once the owner is resumed or reopened the session's reports reach it again;
- the session has already been wrapped up — a session archived by its own `done` report cannot
  report a second time;
- the session is a lead session, the report has `status: done` and no open items, and a session bound to it is not
  archived, unless a lock blocks automatic archiving (see "Automatic archiving" below). The reason names those
  sessions. Its other reports go through, and none of them archives it.

### When a session does not report

**Reporting is not forced.** When a bound session ends a turn without having called `report`, Octoboard delivers
that turn's last assistant message to its owner as the report instead.
The message says plainly that the session stopped without reporting and that the status is
Octoboard's guess rather than its own word. The status is `needs_decision`, or `failed` when the turn
ended in an error, and the open-items list is empty. A session whose turn produced no message at all
is reported as such, with the suggestion that the owner check it with `get_session`.

A synthesised report never archives the session: only a session's own `done` report does that.

**No report is synthesised for a lead session's turn while any session bound to it has a process** — running, or being
launched or resumed: it has stopped to wait for its sessions, not stopped working. With none of them running, its turn
is reported on as any bound session's is.

Grok Build reports no turn end at all for some turns, and the backstop Octoboard falls back on
there fires about a minute after the turn ends, so a synthesised report for one of those turns
arrives that much later. Sessions outside the orchestration are never reported on.

### Automatic archiving

Locks follow "Locking a session" in `docs/product/sessions.md`. If the reporting session or any session under it
is locked, its report is delivered normally, including `done` with no open items, but it is not archived. The
unfinished-session refusal for a lead session's completion report does not apply in that case. Unlocking does not
retroactively archive it; a later qualifying report or an explicit archive must do so.

A report with `status: done` and **no** open items, with no lock blocking it, wraps the session up: once the report
has been accepted for its owner, the session's process is ended and the session is archived. The git branches it was
working on, and the linked worktrees that have them checked out, are then removed where git can do so without force,
and the report's answer names those that will stay (see "Archives made by an agent" in
`docs/product/branch-and-worktree-cleanup.md`).
"Accepted" rather than "read" — an owner that is merely busy has the report queued for it,
and the session is archived anyway rather than being left alive until the owner gets round
to it.

Anything else — open items, `failed`, `needs_decision` — leaves the session running and awaiting
instructions, for its owner to continue with `send_message` or to archive explicitly.

Automatic archiving archives the reporting session and nothing else. It never archives the owner the session is
bound to, and does not touch the other sessions bound to it: archiving an owner, and what that takes with it, is
covered in "Archiving, interruption and resuming" in `docs/product/sessions.md`.

## Which sessions an owner drives

A session started with `new_session` is always bound to the session that started it — a console session, or a
project session that is unbound or a lead session — so it reports to it, and it is that owner's for its whole life:
the owner instructs it, archives it once it is done and reopens it when it has another task. A session started on a
`request_session`, once the user approves, is bound to nobody: it reports to the user, and neither the session that
asked for it nor that session's owner drives it (see `docs/product/session-requests.md`). A session **the user opens by
hand is not bound**, unless they choose a console session under "Report to" in the session dialog (see "Opening a
session" in `docs/product/sessions.md`); the choice is none by default and offers console sessions only. A binding
changes afterwards only when its owner hands the session over, or when an unbound session binds itself, and is never
cleared (see "Changing a session's owner" above).

An owner drives only the sessions bound to it. Every session record it reads names that session's owner, and a session
that is unbound, or bound to any other session, is not this one's to drive: it can read such a session, but is told
to leave it alone and is refused if it tries to act on it.

A console session itself never reports anywhere, and neither does an unbound project session.

## A session archiving itself

A session that reports to nobody — a console session or an unbound project session — wraps itself up with
`archive_self`, which takes no arguments: it ends the caller's own process and archives the caller. A session bound
to an owner does not archive itself this way; it wraps up with a `done` report with no open items (see "Automatic
archiving" above). A lead session is offered `archive_self` with the rest of the unbound project session's tools, but
the call is refused, telling it to report `done` instead.

An owner archives each of its sessions once its task is done (see "Which sessions an owner drives" above), so a
session under the caller that is not archived means unfinished work. The call is refused, with the reason in prose,
and changes nothing:

- **while the caller, or any session under it, is locked** — a session under it counts even when it is archived
  already. The refusal names the locked sessions under it, which the user has to unlock (see "Locking a session" in
  `docs/product/sessions.md`). Locks are checked before what follows.
- **while any session under it is not archived**, running or interrupted alike. The refusal names each such session
  by id and title. Those bound to the caller come with the advice to finish them and archive them with
  `archive_session` first. Those bound to a lead session bound to the caller are not the caller's to archive: they are
  named grouped under their lead session, with the advice to wait for the lead session to finish them — its own
  `done` report archives it — or to archive the lead session with `archive_session`, which takes along its sessions
  that have no process running and is refused while any of them has one (see "Archiving, interruption and resuming"
  in `docs/product/sessions.md`).

Otherwise the call is answered first — with the caller's id, the status `archived` and a note that its process will
now end — and then the caller's process is ended and the caller is archived, as an owner's explicit archive does.
Only the caller is archived; nothing under it is touched. When the removal following the archive will leave some of
the branches and worktrees bound to the caller, judged before the archive, the answer names them in `branches_left`
(see "Archives made by an agent" in `docs/product/branch-and-worktree-cleanup.md`).
