# Orchestration

Orchestration is one session starting other sessions, handing them work, following them up and taking their reports.
The session doing it is the **owner** of the sessions bound to it; every session it starts is bound to it and reports
to it. Three kinds of session can be an owner:

- A **console session** — a session the user brings a request to; a console may run several of them at once (see
  "Several console sessions per console" below). It works out which of its console's projects a request belongs to,
  starts sessions there, follows them up, and summarizes what they came back with. **It changes no project itself** —
  one of its console's or another console's, any worktree of it included: neither a file in a project, nor a command
  that creates, edits, moves or deletes anything in one or changes its repository. Every change to a project of its
  console goes through a session it starts; for one of another console's it asks the user or calls `request_session`
  there. Reading a project is legitimate, and is how a console session checks what a session reports. Work that touches
  no project — installing or removing software on the machine, say — it carries out itself, within the permissions it
  has, without starting a session in an unrelated project or adding a project for it; any part of such work that does
  touch a project still goes the way above. It keeps what such work makes out of the console's working directory, which
  is removed with the console. Unlike the rule that it changes no project itself, this rule about work that touches no
  project can be overridden by the user's console session instructions (see "Console session instructions" in
  `docs/product/consoles.md`). Besides its terminal it has one surface of its own for showing the user something — the
  report panel, described in `docs/product/report-panel.md`.
- An **unbound project session** — a project session bound to no owner: one the user opened by hand without binding it
  to a console session, or one started on another session's request (see `docs/product/session-requests.md`). It
  works in its own project and can also start sessions in that project and drive them (see "The unbound project
  session's tools" below).
- A **lead session** — a project session that was unbound and bound itself to a console session of its console,
  keeping the sessions it had started (see "Lead sessions" below).

Every session can also ask the user to start a session that is nobody's — in any project of any console, or a console
session — which reports to the user and which the caller does not drive (see `docs/product/session-requests.md`), and
ask the user to resume a conversation by its id, which brings back the session that holds it whoever owns it, or opens
a session that is nobody's on the agent's record of it (see `docs/product/requests-to-resume-a-conversation.md`).

The sessions an owner starts are ordinary project sessions, described in `docs/product/sessions.md`. Archiving,
resuming and deleting an owner follow the same rules whichever kind it is (see "Archiving, interruption and resuming"
in `docs/product/archiving-and-resuming-sessions.md` and "Deleting archived sessions" in `docs/product/deleting-sessions.md`).

Octoboard injects the tools into the session. A console session gets the full set (see "The console session's tools"
below); a project session that can own sessions — an unbound one or a lead session — a narrower set, scoped to its own
project for what it starts and drives (see "The unbound project session's tools" below); and a project session with
the role of a bound one — bound to an owner and not a lead session — gets one tool back the other way, `report`, and
cannot start or drive sessions. Every session, whatever its kind, also gets the two tools for sharing information with
every other running session (see `docs/product/information-between-sessions.md`), `request_session` and
`request_resume_session`; every project session also gets `get_project`, a read-only view of its own project's settings
(see `docs/product/agent-settings-tools.md`); and every session gets `get_account_quotas`, a read of each account's plan
quota (see "Reading quotas from a session" in `docs/product/account-quotas.md`), and `get_session_branches`, a read of
the git branches a session is responsible for with their worktrees (see "Reading the branches a session is responsible
for" in `docs/product/branch-responsible-session.md`).
**Which tools a session sees follows from its role, fixed for its lifetime, and from its binding as it stands**, which
can change while the session runs (see `docs/product/changing-session-owners.md`). A console session cannot report, and
orchestration is at most two levels deep: a console session, a lead session bound to it, and the lead session's own
sessions.

A session is offered the tools of the Octoboard that launched it, for as long as that agent process runs: a tool
Octoboard gains later reaches a running session only once it is next launched — resumed, reopened or moved to another
account.

## Several console sessions per console

A console may run any number of console sessions at once, each with its own sessions reporting to it
(see `docs/product/reporting-to-owners.md`): a session reports to the console session that is named in its own binding,
not to "the" console session of its console. How a console session is archived, and where archived
console sessions are listed, are in "Console sessions and project sessions" in
`docs/product/sessions.md`.

## The console session's tools

| Tool | Arguments | What it does |
|---|---|---|
| `list_projects` | — | The console's projects, each with its settings (see "A project's settings" in `docs/product/agent-settings-tools.md`) and the sessions currently running in it — every session of the console, whichever session owns it. |
| `add_project` | `source` (`local` / `parent` / `git`), `path?`, `remote_url?`, `name?`, `description?`, `default_agent?`, `tags?`, `accounts?`, `launch?`, `pinned?`, `worktrees?` | Associates one or more projects with this console, under the same rules as the user's own form (see "Associating a project" in `docs/product/projects.md`) and a stricter one for a URL to clone (see "Adding a project with its settings" in `docs/product/agent-settings-tools.md`). Answers with the settings of the projects it added. |
| `update_project` | `project`, `name?`, `description?`, `tags?`, `add_tags?`, `remove_tags?`, `pinned?`, `default_agent?`, `accounts?`, `launch?`, `worktrees?` | Changes one of this console's projects' settings. See "Changing a project: `update_project`" in `docs/product/agent-settings-tools.md`. |
| `get_console`, `update_console` | — / `name?`, `default_agent?`, `project_session_agent?`, `accounts?`, `launch?`, `project_session_launch?` | Reads or changes the calling console session's own console. See "The console's own settings: `get_console` and `update_console`" in `docs/product/agent-settings-tools.md`. |
| `get_settings`, `update_settings` | — / `auto_pull_repositories?`, `auto_delete_unresumable_archives?`, `auto_select_account?`, `sidebar_order_by_status?`, `default_clone_dir?`, `worktrees?` | Reads or changes the application-wide settings. See "Application settings: `get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md`. |
| `new_session` | `project`, `brief`, `title?`, `agent?`, `account?`, `model?`, `effort?` | Starts a session in one of this console's projects, bound to the calling console session, and hands it the brief as its opening prompt. Needs no approval. `title` names the session (see "Naming the sessions a session starts" below). `agent`, `account`, `model` and `effort` choose those for that one session, as the session dialog's choices do (see "Opening a session" in `docs/product/sessions.md`): `account` is an account by id or name, or `default` for the agent's default account, and must belong to the agent the session will run; a blank `model` or `effort` is the agent's own default, and any other must be one the agent offers (see "The Advanced section" in `docs/product/model-and-reasoning-effort.md`). Left out, `agent` is the project's default agent, not the caller's own; a left-out `account` is chosen as "The agent and account of a session a session starts" below describes. Answers with the new session's id and its agent. Refused, with the reason in prose, when the session's resolved agent has been determined unavailable on this machine — never while that determination is still pending — or is not added, or when the account it would run on is turned off or has used up its quota (see "Opening a session" in `docs/product/sessions.md`). |
| `send_message` | `session`, `text` | Appends an instruction to a running session of the caller's. Answers with `delivered`, true only when it was written and not left unsent, and a `note` saying it was delivered, that it was queued, or that it was written into the session's input but its agent did not send it to the conversation — it is in the input box, or went to a view of the agent's background agents (see "A message written but not sent" and "A message a dialog discarded or a view took" in `docs/product/message-delivery.md`); the last two each say not to send it again. A session that a change of its account's sharing is restarting accepts it too, and holds it for the resumed process (see "When a change is carried out" in `docs/product/sharing-the-default-configuration.md`). |
| `get_session` | `session` | The record of any session, of any console — status, title, agent, whether it is a console session, its console and project, which session owns it and whether that owner is a console session or a project session (or that it is unbound), whether it is the caller's, timestamps, its account, the model and effort it is seen running with, whether it is pinned or locked, and the names of the branches it is responsible for — plus a tail of what it has printed. |
| `rename_session` | `session`, `title` | Changes the title of one of the caller's sessions, under the rules of "Renaming a session" in `docs/product/sessions.md`. Answers with the session's record. |
| `set_session_pinned` | `session`, `pinned` | Pins or unpins one of the caller's sessions (see "Pinning" in `docs/product/order-of-projects-and-sessions.md`). Answers with the session's record. |
| `archive_session` | `session` | Ends the process of one of the caller's sessions and archives it. Once the process has ended, the git branches the archived sessions were working on, and the linked worktrees that have them checked out, are removed where git can do so without force; the answer waits for that and names in `branches_left` what stays, and why (see "Archives made by an agent" in `docs/product/branch-and-worktree-cleanup.md`). |
| `archive_self` | — | Ends the caller's own process and archives the caller, once every session under it is archived, removing its branches and worktrees as `archive_session` does. See "A session archiving itself" below. |
| `list_archived` | `project` | The archived sessions of one of this console's projects, whoever owns them, each with its owner. |
| `reopen_session` | `session`, `text?` | Relaunches an archived or interrupted session of the caller's, continuing its conversation, and optionally hands it an instruction — its next task. A Claude Code session that resumes a conversation is launched with the instruction as its prompt; any other has it delivered once the relaunched session can take one (see "Messages held until a session can take them" in `docs/product/message-delivery.md`). The caller is the console session the session is bound to and is running, so reopening one never brings a console session back. |
| `handoff_session` | `session`, `agent`, `account?`, `title?`, `model?`, `effort?`, `note?` | Hands one of the caller's sessions off to a new session of another agent, which takes its place and stays the caller's, and archives the original, once the user approves, unless they have chosen to have handoffs approved without asking. See "Handing off from an owner's tool" in `docs/product/handing-off-sessions.md`. |
| `handover_branches` | `branches`, `from?`, `to?` | Moves the responsibility for git branches from one session of the caller's own to another, without the user and without replacing any session. A console session is responsible for no branch, so it names both `from` and `to`. See "Handing branches between sessions of an owner" in `docs/product/branch-responsible-session.md`. |
| `switch_session_account` | `session`, `account` | Moves one of the caller's sessions to another account of its own agent, keeping its conversation, once the user approves, unless they have chosen to have switches approved without asking. See "Switching from an owner's tool" in `docs/product/switching-session-accounts.md`. |
| `transfer_session` | `session`, `to`, `note` | Hands one of the caller's sessions to another owner, without the user. See "Handing a session to another owner" in `docs/product/changing-session-owners.md`. |
| `release_session` | `session`, `note?` | Releases one of the caller's sessions from its binding, without the user: it becomes the user's own, unbound session, and is passed the optional `note` as its former owner's parting note. See "Releasing a session from its owner" in `docs/product/changing-session-owners.md`. |
| `request_session` | `kind`, `project?`, `console?`, `brief`, `title?`, `agent?`, `account?`, `model?`, `effort?`, `branches?` | Asks the user to start a session that is nobody's. See `docs/product/session-requests.md`. |
| `request_resume_session` | `conversation`, `project?`, `title?`, `model?`, `effort?` | Asks the user to resume a conversation by its id, as the session dialog's Resume conversation tab does; a console session has to name the project. See `docs/product/requests-to-resume-a-conversation.md`. |
| `list_sessions`, `share_info` | see `docs/product/information-between-sessions.md` | As for every session. |
| `get_session_branches` | `session?` | The git branches any session is responsible for, each with its project and worktree, read-only, as for every session. See "Reading the branches a session is responsible for" in `docs/product/branch-responsible-session.md`. |
| `get_account_quotas` | `agent?` | Each account's plan quota, read-only, as for every session. See "Reading quotas from a session" in `docs/product/account-quotas.md`. |
| `show_page` | `html` | Pushes an HTML page to the calling console session's report panel, beside the console session's own terminal. Answers with the new page's id. See `docs/product/report-panel.md`. |

A project is named either by its id or by its name where that name is unambiguous within the
console; an ambiguous name is refused and asks for the id. A project of another console is refused, except by
`request_session` and `request_resume_session`, which may name any project.

`get_session` returns at most 8 KiB of the session's most recent output, with terminal escape
sequences and cursor-control bytes stripped and runs of blank lines collapsed, so it is readable
prose rather than a rendered frame. For a session whose process has ended, an archived or
interrupted one, that is the output the daemon kept when the process ended, so the field reads the
same whether the session is running or not. It is `null` only where the daemon never kept any
output — no process of the session has printed anything yet, it ended with the daemon killed
outright before the output was kept, or a new process has since taken the session over and
discarded it (see "`GET /ws/term/:session` — terminal stream" in `apps/daemon/PROTOCOL.md`). A
session that is waiting for the user carries a note saying so.

Every session record these tools give — `list_projects`' sessions, `get_session`, `list_sessions`, `list_archived`,
and the record `rename_session` and `set_session_pinned` answer with — carries `background_tasks`: for a Claude Code
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
which say how many messages are queued for the session and what keeps them from being written whatever its `status`
reads (see "Messages held until a session can take them" in `docs/product/message-delivery.md`).

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

**Reads are wide, writes are the caller's own.** `list_projects` and `list_archived` cover the sessions of the
console's own projects, whoever owns them; `get_session` reads any session of any console, `get_session_branches`
the branches of any session, and `list_sessions` lists every running one. Each record carries `owner` (the id of the
session it is bound to, or null when it is unbound — always null for a console session, which is never bound),
`owner_kind` (`console` or `project`, the kind of session that owner
is; null when it is unbound) and `yours` (whether that owner is the caller). `send_message`, `rename_session`,
`set_session_pinned`, `archive_session`, `reopen_session`, `handoff_session`, `switch_session_account`,
`transfer_session` and `release_session` act only on a session bound to the caller; for any other — unbound, bound to another console
session or to a project session, a console session, or the caller itself — they refuse, with a reason naming who owns
it, and change nothing.
`archive_session` aimed at the caller itself is refused pointing to `archive_self` instead. `new_session` always binds the new
session to the caller, so a console session cannot start one on another's behalf: it can hand one over afterwards with
`transfer_session`, or ask the user for one that is nobody's with `request_session`. `request_resume_session` can bring
back a session that is not the caller's, but only with the user's approval, and it stays whoever's it was (see
`docs/product/requests-to-resume-a-conversation.md`). The reads are not narrowed because
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
| `new_session` | `brief`, `title?`, `agent?`, `account?`, `model?`, `effort?`, `branches?` | Starts a session in the caller's own project, bound to the caller, and hands it the brief as its opening prompt, exactly as the console session's `new_session` does, with the same overrides (see "Handing out a task: the brief" below). Needs no approval. There is no `project` argument; a call that names another project anyway is refused before anything is launched. `agent` may name any agent; left out, `agent` follows the caller's own; a left-out `account` is chosen as "The agent and account of a session a session starts" below describes. `branches` hands the new session branches the caller is responsible for, which its opening prompt then names (see "Handing branches to a session the caller starts" in `docs/product/branch-responsible-session.md`). Answers with the new session's id and its agent, and, where `branches` names a branch, `branches` (the ones moved) and a `note` on what became of them. Refused, with the reason in prose, when the session's resolved agent has been determined unavailable on this machine — never while that determination is still pending — or is not added, or when the account it would run on is turned off or has used up its quota (see "Opening a session" in `docs/product/sessions.md`). |
| `send_message` | `session`, `text` | As the console session's. |
| `get_session` | `session` | As the console session's: any session, of any project or console. |
| `archive_session` | `session` | As the console session's. |
| `archive_self` | — | As the console session's. Offered to a lead session too, but refused for it: it wraps up with `report` instead (see "A session archiving itself" below). |
| `reopen_session` | `session`, `text?` | As the console session's. |
| `rename_session` | `session`, `title` | As the console session's. |
| `set_session_pinned` | `session`, `pinned` | As the console session's. |
| `handoff_session` | `session`, `agent`, `account?`, `title?`, `model?`, `effort?`, `note?` | As the console session's. |
| `handover_branches` | `branches`, `from?`, `to?` | As the console session's, except that the caller can be responsible for branches itself: `from` or `to` left out is the caller, so it can also give its own branches to a session of its own or take one of theirs. |
| `switch_session_account` | `session`, `account` | As the console session's. |
| `transfer_session` | `session`, `to`, `note` | As the console session's. See "Handing a session to another owner" in `docs/product/changing-session-owners.md`. |
| `release_session` | `session`, `note?` | As the console session's. See "Releasing a session from its owner" in `docs/product/changing-session-owners.md`. |
| `bind_to_session` | `owner`, `summary` | Binds the caller, while it is unbound, to an owner, without the user. Refused for a lead session, which is bound already. See "Binding an unbound session to an owner" in `docs/product/changing-session-owners.md`. |
| `request_session` | as the console session's | Asks the user to start a session that is nobody's. See `docs/product/session-requests.md`. |
| `request_resume_session` | `conversation`, `project?`, `title?`, `model?`, `effort?` | As the console session's; `project` defaults to the caller's own. See `docs/product/requests-to-resume-a-conversation.md`. |
| `report` | as in `docs/product/reporting-to-owners.md` | Offered, but refused while the session is unbound: it has nobody to report to. A lead session's `report` is accepted and goes to its console session (see "Lead sessions" below). |
| `list_sessions`, `share_info` | see `docs/product/information-between-sessions.md` | As for every session. |
| `get_session_branches` | `session?` | The git branches any session is responsible for, each with its project and worktree, read-only, as for every session. See "Reading the branches a session is responsible for" in `docs/product/branch-responsible-session.md`. |
| `get_account_quotas` | `agent?` | Each account's plan quota, read-only, as for every session. See "Reading quotas from a session" in `docs/product/account-quotas.md`. |
| `get_project` | — | Its own project's settings, read-only, as for every project session (see "A project's settings" in `docs/product/agent-settings-tools.md`). |

It has no `list_projects`, `list_archived`, `add_project` or `show_page`, and none of the tools that change a setting:
`update_project`, `get_console`, `update_console`, `get_settings` and `update_settings` are the console session's alone.

**Reads are wide, writes are the caller's own** — the console session's rule. `get_session` reads any session of any
project or console, carrying `owner`, `owner_kind` and `yours` as above. `send_message`, `rename_session`,
`set_session_pinned`, `archive_session`, `reopen_session`, `handoff_session`, `switch_session_account`,
`transfer_session` and `release_session` act only on a session bound to the caller, and refuse, with a reason naming who owns it, and change
nothing, for any other: an unbound session, one bound to a console session or to another project session, the caller
itself, or a console session. For an unbound project session, `archive_session` aimed at itself is refused pointing to
`archive_self` instead.

A session a project session starts is bound to it, so it has the role of a bound one: it is offered `report`,
`list_sessions`, `get_session_branches`, `share_info`, `get_project`, `get_account_quotas`, `request_session` and
`request_resume_session`, and
cannot start or drive
sessions of its own, nor read one with `get_session`. It reports to the project session that started it exactly as a
console session's sessions report to their console session: see `docs/product/reporting-to-owners.md`.

## Lead sessions

A **lead session** is a project session bound to a console session of its console that can still own sessions of its
own project. A session becomes one by binding itself, while unbound, to a running console session with
`bind_to_session` (see "Binding an unbound session to an owner" in `docs/product/changing-session-owners.md`). One with no console session to bind to can
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
  is not blocked by a lock (see "Automatic archiving" in `docs/product/reporting-to-owners.md`), and no report is
  synthesised for it while they run (see `docs/product/reporting-to-owners.md`).
- **Its console session can hand it on.** Handed to another console session of the console it stays a lead session,
  keeping its sessions. It can be handed to a project session only while it has no sessions of its own, archived ones
  included; it then has the role of a bound session and can own none (see "Handing a session to another owner" in
  `docs/product/changing-session-owners.md`).
- **It can be released.** Released by its console session or by the user, it is an unbound project session again and
  keeps its sessions (see "Releasing a session from its owner" in `docs/product/changing-session-owners.md`).
- **Its sessions stay ordinary bound sessions.** A session bound to a lead session has the role of a bound one and can
  never be an owner: naming one as the owner of a new session, or binding or handing a session to one, is refused.
  There is no third level and no cycle.

A session is **under** an owner when it is bound to it, or bound to a lead session that is bound to it. Archiving,
reopening and deleting follow the bindings across both levels; see "Archiving, interruption and resuming" in
`docs/product/archiving-and-resuming-sessions.md` and "Deleting archived sessions" in `docs/product/deleting-sessions.md`.

**The console session sees the lead session's sessions but does not drive them.** A lead session's `owner_kind` reads
`console`; its own sessions read `project`, with the lead session as their `owner`. They appear in the console
session's `list_projects`, `get_session` and `list_sessions`, not as its own; `send_message`, `rename_session`,
`set_session_pinned`, `archive_session`, `reopen_session`, `handoff_session`, `switch_session_account`,
`transfer_session` and `release_session` aimed at one are refused, naming the lead session as its owner (see "Which sessions an owner
drives" below). The lead session itself is bound to the console session and is driven by it like any other session of
its own.

## Handing out a task: the brief

`new_session` takes a `brief` of four natural-language fields:

| Field | Required | Contents |
|---|---|---|
| `goal` | yes | The outcome to achieve. |
| `context` | no | Background and relevant leads. The field asks the caller to include which language the user converses with it in, so that the session can name the sessions it starts in that language (see "Naming the sessions a session starts" below). |
| `acceptance` | no | Criteria for being done. |
| `constraints` | no | What must not be touched, whether committing or pushing is allowed, and so on. |

Octoboard renders the brief into the session's opening prompt from a **fixed template** — one
section per field, in the order above, headed `## Goal`, `## Context`, `## Acceptance`,
`## Constraints`. A field the caller left out or left blank is omitted entirely rather than
sent as an empty heading.

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

## Naming the sessions a session starts

`new_session` and `request_session` take an optional `title`, the started session's name in the session list (its
created name; see `docs/product/session-titles.md`), stored trimmed. Left out or blank, the session is **titled from
its goal** rather than from its project — the goal's first non-blank line, shortened to roughly 48 characters on a word
boundary with an ellipsis — so several sessions dispatched into one project can be told apart in the menu.
`handoff_session`'s `title` names the receiving session the same way; its default is described in "What the receiving
session takes over" in `docs/product/handing-off-sessions.md`. `rename_session`'s `title` is a session's new name, and
`request_resume_session`'s names the session it opens or renames the archived session it reopens (see "The call" in
`docs/product/requests-to-resume-a-conversation.md`).

Every one of these `title` arguments tells the agent to write the name in **the language the user converses with it
in**, not that of the project, its docs and code, or a brief. A session that takes its work from an owner session
rather than from the user uses the user's language as its owner passes it on — the brief's `context` asks for it (see
"Handing out a task: the brief" above) — else the language of its owner's messages. The language is the agent's to
choose: Octoboard does not check it, and the daemon does not know the language of the user's interface.

## The agent and account of a session a session starts

A session that a session starts itself — with `new_session`, or with `request_session` — runs on the agent and account
below unless the call names others. In the caller's own console the caller's own agent takes the place of the default
the project and console set (see "Which agent a session uses" in `docs/product/sessions.md`), which applies only where
the caller's own cannot — except that a session a console session starts in a project always takes that default. The
account does not always follow the caller.

- **The agent.** Left out, `agent` is settled by who calls and where the session is:
  - A project session's `new_session`, and its `request_session` in its own console, run the caller's own agent,
    unless that agent has been determined unavailable on this machine or is not on the list of added agents; then it
    is settled as for a session opened without a choice.
  - A console session's `new_session`, and its `request_session` for a project session, run the project's default
    agent — the project's own, else its console's default agent for project sessions — as for a session opened without
    a choice, and never follow the caller's own agent.
  - A console session's `request_session` for a console session in its own console runs the caller's own agent, with
    the same fallback as a project session's.
  - In another console a left-out `agent` is that console's and its project's, as for a session opened without a
    choice.
- **The account.** An `account` the call names is used as given, including `default`, which chooses the agent's default
  account and does not fall through to a project pin. Left out or blank, while **Automatically select an account** is
  on (see that section in `docs/product/settings.md`), it is the account of the session's agent whose weekly quota
  resets soonest, among accounts that can still open a session — the same pick the new-session dialog makes, ranked as
  in "Which account a session uses" in `docs/product/sessions.md`. That pick applies in the caller's own console and in
  another. When the setting is off, or no account ranks: in the caller's own console, it is the caller's own account,
  the agent's default account included, when the session is of the caller's own agent — named, or settled on, as when a
  project's default agent is the console session's own — and that account can still open a
  session. The caller's account is passed over, silently, when it no longer exists, is turned off (see "Turning an
  account off" in `docs/product/agent-accounts.md`) or is used up by the reading the daemon holds (see "A used-up
  account" in `docs/product/account-quotas.md`). Then, and for a session of another agent, the account is the
  project's for that agent, else the console's. In another console, when none ranks, the account is the project's,
  else the console's, and never the caller's. The call is refused only when the account it settles on is turned off or
  used up as well (see "Opening a session" in `docs/product/sessions.md`).
- **Named ones win.** An `agent` or `account` the call names is used as given. A console session's tools, and every
  session's `request_session`, tell the caller to name either only when the user asked for it. A project session's
  `new_session` also lets it name another agent unasked when it judges that agent fits the work better and an account
  of it can open a session, and another account when that is the way to reach that agent (see "Choosing the session a
  task goes to" below).
- **The model and reasoning effort** are not the caller's: left out, they are inherited as for any new session of that
  agent (see "Which model and reasoning effort a session uses" in `docs/product/sessions.md`).

A `request_session` for a session in **another console** — a console session there, or a session in one of its
projects — takes its agent from that console and its project, and nothing of the caller's agent. Its account, when
left out, is the pick above while automatic account selection is on, and the project's, else the console's, when the
setting is off or none ranks. `handoff_session` always names its receiving agent; a left-out account follows the
account rule above as in the caller's own console, the session it hands off being always in it (see "Handing off from
an owner's tool" in `docs/product/handing-off-sessions.md`).

## Which sessions an owner drives

A session started with `new_session` is always bound to the session that started it — a console session, or a
project session that is unbound or a lead session — so it reports to it, and it is that owner's while it stays bound
to it:
the owner instructs it, archives it once it is done and reopens it when it has another task — for a project session
that owns it, another closely related task with room for it (see "Choosing the session a task goes to" below). A
session started on a
`request_session`, once approved, is bound to nobody: it reports to the user, and neither the session that
asked for it nor that session's owner drives it (see `docs/product/session-requests.md`). A session **the user opens by
hand is not bound**, unless they choose a console session under "Report to" in the session dialog (see "Opening a
session" in `docs/product/sessions.md`); the choice is none by default and offers console sessions only. A binding
changes afterwards only when its owner hands the session over, when an unbound session binds itself, when its owner
is handed off to another agent and the session moves to the one that takes the owner's place, or when its owner or the
user releases the session, which leaves it unbound (see `docs/product/changing-session-owners.md`). A session the user
opened bound to an owner can still be deleted by the user while it is unused, and the owner is not told (see "Deleting an unused session" in
`docs/product/deleting-sessions.md`); one started with `new_session` cannot.

An owner drives only the sessions bound to it. Every session record it reads names that session's owner, and a session
that is unbound, or bound to any other session, is not this one's to drive: it can read such a session, but is told
to leave it alone and is refused if it tries to act on it.

A console session itself never reports anywhere, and neither does an unbound project session.

## Choosing the session a task goes to

The role description of a project session that can own sessions — an unbound one or a lead session — tells it when to
start sessions of its own and which session a piece of work goes to. A console session is not told: it works out which
project a request belongs to and does not concern itself with how a project's work is split among sessions. A project
session with the role of a bound one owns none and is not told either. This is guidance to the agent; Octoboard does not
enforce it:

- An ordinary task needs no sessions of its own: the project session does it itself, with subagents where they help.
- It starts sessions only for a large requirement that can be developed in parallel on several branches, which it
  splits across them and coordinates, or for a kind of work another agent handles better than its own, when an account
  of that agent can open a session — `get_account_quotas` shows which can. The text names no agent.
- Weigh how related a piece of work is to what a session already holds, and how much of its context is used —
  `context` in `get_session` and `list_sessions` (see "The console session's tools" above), judged by `tokens` when the
  window is not known.
- Give a piece to an existing session only when it is closely related to that session's work and the session still has
  room. Reopening an archived session for another task is held to the same condition, which the role description's line
  on `new_session` and the tool's description repeat.
- Start a new session for an unrelated piece, or when the related session's context is heavily used: about 60% of its
  window.
- Split independent pieces of work across sessions rather than queuing them on one.
- Wrap up and archive a session near a full context rather than handing it more.

For work another agent handles better, it may name that agent in `new_session` without the user having asked (see "The
agent and account of a session a session starts" above).

## A session archiving itself

A session that reports to nobody — a console session or an unbound project session — wraps itself up with
`archive_self`, which takes no arguments: it ends the caller's own process and archives the caller. A session bound
to an owner does not archive itself this way; it wraps up with a `done` report with no open items (see "Automatic
archiving" in `docs/product/reporting-to-owners.md`). A lead session is offered `archive_self` with the rest of the unbound project session's tools, but
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
  in `docs/product/archiving-and-resuming-sessions.md`).

Otherwise the call is answered first — with the caller's id, the status `archived` and a note that its process will
now end — and then the caller's process is ended and the caller is archived, as an owner's explicit archive does.
Only the caller is archived; nothing under it is touched. When the removal following the archive will leave some of
the branches and worktrees bound to the caller, judged before the archive, the answer names them in `branches_left`
(see "Archives made by an agent" in `docs/product/branch-and-worktree-cleanup.md`).
