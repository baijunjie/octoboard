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
  there. Reading a project — opening a file, searching it, looking at its git history — is legitimate, and is how a
  console session checks what a session reports; `git status` and `git diff` are not reading, as they rewrite the
  repository's index and can fail a command the session working there is running, so it sees what a session has changed
  with `list_project_changes` and `read_project_diff` instead (see "Reading a project's changes" in
  `docs/product/what-every-session-reads.md`). Work that touches no project — installing or removing software on the
  machine, say — it carries out itself, within the permissions it has, without starting a session in an unrelated
  project or adding a project for it; any part of such work that does touch a project still goes the way above. It keeps
  what such work makes out of the console's working directory, which is removed with the console. Unlike the rule that
  it changes no project itself, this rule about work that touches no project can be overridden by the user's console
  session instructions (see "Console session instructions" in `docs/product/consoles.md`). Besides its terminal it has
  one surface of its own for showing the user something — the report panel, described in `docs/product/report-panel.md`.
- An **unbound project session** — a project session bound to no owner: one the user opened by hand without binding it
  to a console session, or one started on another session's request (see `docs/product/session-requests.md`). It works
  in its own project and can also start sessions in that project and drive them (see "The unbound project session's
  tools" below). Like a console session, it has a report panel for showing the user something (see
  `docs/product/report-panel.md`).
- A **lead session** — a project session that was unbound and bound itself to a console session of its console,
  keeping the sessions it had started and its report panel (see "Lead sessions" below).

Every session can also ask the user to start a session that is nobody's — in any project of any console, or a console
session — which reports to the user and which the caller does not drive (see `docs/product/session-requests.md`), and
ask the user to resume a conversation by its id, which brings back the session that holds it whoever owns it, or opens
a session that is nobody's on the agent's record of it (see `docs/product/requests-to-resume-a-conversation.md`).

The sessions an owner starts are ordinary project sessions, described in `docs/product/sessions.md`. Archiving, resuming
and deleting an owner follow the same rules whichever kind it is (see "Archiving, interruption and resuming" in
`docs/product/archiving-and-resuming-sessions.md` and "Deleting archived sessions" in
`docs/product/deleting-sessions.md`).

Octoboard injects the tools into the session. **What a session reads is the same whatever its kind**: every session
gets the same read-only tools, with the same scope (see `docs/product/what-every-session-reads.md`). What it can
change follows its kind. A console session gets the full set of tools that act (see "The console session's tools"
below); a project session that can own sessions — an unbound one or a lead session — a narrower set, scoped to its own
project for what it starts and drives (see "The unbound project session's tools" below); and a project session with
the role of a bound one — bound to an owner and not a lead session — gets one tool back the other way, `report`, and
cannot start or drive sessions. Every session, whatever its kind, also gets `send_message`, for sharing information with
every other running session (see `docs/product/information-between-sessions.md`), `request_session` and
`request_resume_session`; every project session also gets `get_project`, a read-only view of its own project's settings
(see `docs/product/agent-settings-tools.md`).
**Which tools a session sees follows from its role, fixed for its lifetime, and from its binding as it stands**, which
can change while the session runs (see `docs/product/changing-session-owners.md`). A console session cannot report, and
orchestration is at most two levels deep: a console session, a lead session bound to it, and the lead session's own
sessions.

**What no tool does, by design.** No session is offered a tool for any of these:

- **Opening a file or a diff in the window for the user.** Besides the sessions' own terminals, the report panel of a
  console session, an unbound project session or a lead session is the only place a session shows the user anything.
- **Deleting a session.** Deleting is destructive and cannot be undone, so it stays with the user (see
  `docs/product/deleting-sessions.md`); an owner archives the sessions it is done with instead.
- **Unbinding itself.** A bound session cannot release itself from its owner: the binding is the owner's to end, so
  only its owner or the user can release it (see "Releasing a session from its owner" in
  `docs/product/changing-session-owners.md`).
- **Renaming itself.** A session's title is for the user and its owner to set, so `rename_session` refuses the caller
  itself (see "The console session's tools" below).
- **Forcing the removal of branches or worktrees.** An agent's archive removes only what git removes without force; a
  worktree with changes, an unmerged branch or a locked worktree would lose work if forced, so taking it along stays with
  the user's archive confirmation, and `archive_session` has no `force_remove` (see "Archives made by an agent" in
  `docs/product/branch-and-worktree-cleanup.md`). Its `keep` goes the other way, removing nothing.
- **Taking over the sessions of an interrupted owner.** A session interrupted under an owner that is itself interrupted
  stays bound to that owner; the work is taken up again by resuming that owner, which then drives its sessions again,
  not by a new console session taking them over.

**Only an owner can interrupt a turn.** An owner can stop the turn of a session it owns with `interrupt_session` (see
`docs/product/interrupting-turns.md`); no other session can interrupt another's turn, and what any session writes into
a session mid-turn goes to that turn's agent as described in "Messages held until a session can take them" in
`docs/product/message-delivery.md`.

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

Besides the reads in `docs/product/what-every-session-reads.md`:

| Tool | Arguments | What it does |
|---|---|---|
| `add_project` | `source` (`local` / `parent` / `git`), `path?`, `remote_url?`, `name?`, `description?`, `default_agent?`, `tags?`, `accounts?`, `launch?`, `pinned?`, `git_workflow?` | Associates one or more projects with this console, under the same rules as the user's own form (see "Associating a project" in `docs/product/projects.md`) and a stricter one for a URL to clone (see "Adding a project with its settings" in `docs/product/agent-settings-tools.md`). Answers with the settings of the projects it added. |
| `update_project` | `project`, `name?`, `description?`, `tags?`, `add_tags?`, `remove_tags?`, `pinned?`, `default_agent?`, `accounts?`, `launch?`, `git_workflow?` | Changes one of this console's projects' settings. See "Changing a project: `update_project`" in `docs/product/agent-settings-tools.md`. |
| `get_console`, `update_console` | — / `name?`, `default_agent?`, `project_session_agent?`, `accounts?`, `launch?`, `project_session_launch?` | Reads or changes the calling console session's own console. See "The console's own settings: `get_console` and `update_console`" in `docs/product/agent-settings-tools.md`. |
| `get_settings`, `update_settings` | — / `auto_pull_repositories?`, `auto_delete_unresumable_archives?`, `auto_select_account?`, `sidebar_order_by_status?`, `default_clone_dir?`, `git_workflow?` | Reads or changes the application-wide settings. See "Application settings: `get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md`. |
| `new_session` | `project`, `brief`, `title?`, `agent?`, `account?`, `model?`, `effort?` | Starts a session in one of this console's projects, bound to the calling console session, and hands it the brief as its opening prompt. Needs no approval. `title` names the session (see "Naming the sessions a session starts" below). `agent`, `account`, `model` and `effort` choose those for that one session, as the session dialog's choices do (see "The session dialog" in `docs/product/opening-sessions.md`): `account` is an account by id or name, or `default` for the agent's default account, and must belong to the agent the session will run; a blank `model` or `effort` is the agent's own default, and any other must be one the agent offers (see "The Advanced section" in `docs/product/model-and-reasoning-effort.md`). Left out, `agent` is the project's default agent, not the caller's own; a left-out `account` is chosen as "The agent and account of a session a session starts" below describes. Answers with the new session's id, its agent, the account it opened on (`account`, the id or `default`, and `account_name`) and the `model` and `effort` it was launched with (`null` for the agent's own default), since the account and the launch settings are settled when the session opens rather than by the call. Refused, with the reason in prose, when the session's resolved agent has been determined unavailable on this machine — never while that determination is still pending — or is not added, or when the account it would run on is turned off, has used up its quota or is not logged in (see "When the agent or the account cannot open a session" in `docs/product/opening-sessions.md`). |
| `instruct_session` | `session`, `text` | Appends an instruction to a running session of the caller's. Answers with `delivered`, true only when it was written and not left unsent, and a `note` saying it was delivered, that it was queued, or that it was written into the session's input but its agent did not send it to the conversation — it is in the input box, or went to a view of the agent's background agents (see "A message written but not sent" and "A message a dialog discarded or a view took" in `docs/product/message-delivery.md`); the last two each say not to send it again. A session that a change of its account's sharing is restarting accepts it too, and holds it for the resumed process (see "When a change is carried out" in `docs/product/sharing-the-default-configuration.md`). |
| `interrupt_session` | `session` | Stops the turn a running session of the caller's is in the middle of, as the user pressing its agent's interrupt key would; the session keeps its conversation and goes back to awaiting instructions. Answers with `interrupted`, the session's `status` afterwards and a `note`. A session not in a turn is left alone. See `docs/product/interrupting-turns.md`. |
| `rename_session` | `session`, `title` | Changes the title of one of the caller's sessions, under the rules of "Renaming a session" in `docs/product/sessions.md`. Answers with the session's record. |
| `set_session_pinned` | `session`, `pinned` | Pins or unpins one of the caller's sessions (see "Pinning" in `docs/product/order-of-projects-and-sessions.md`). Answers with the session's record. |
| `archive_session` | `session`, `keep?` | Ends the process of one of the caller's sessions and archives it. Once the process has ended, the git branches the archived sessions were working on, and the linked worktrees that have them checked out, are removed where git can do so without force; the answer waits for that and names in `branches_left` what stays, and why. With `keep` nothing is removed and nothing is waited for (see "Archives made by an agent" in `docs/product/branch-and-worktree-cleanup.md`). |
| `archive_self` | — | Ends the caller's own process and archives the caller, once every session under it is archived, removing its branches and worktrees as `archive_session` does. See "A session archiving itself" below. |
| `reopen_session` | `session`, `text?`, `model?`, `effort?` | Relaunches an archived or interrupted session of the caller's, continuing its conversation, and optionally hands it an instruction — its next task. A Claude Code session that resumes a conversation is launched with the instruction as its prompt; any other has it delivered once the relaunched session can take one (see "Messages held until a session can take them" in `docs/product/message-delivery.md`). `model` and `effort` reopen it with another model or reasoning effort, which become the session's own choices, its later launches included; a blank one is the agent's own default, and one left out keeps what the session has. Each is checked, before anything is relaunched, against what the session's agent offers under the account the session runs on, as `list_agent_models` lists it (see "The Advanced section" in `docs/product/model-and-reasoning-effort.md`); a value it does not offer is refused (`invalid_launch_setting`), with nothing reopened or recorded. A reopen whose relaunch fails puts the session's earlier model and effort back. The session stays on its agent and its account (`switch_session_account` moves it to another account), and an archived owner that comes back with it keeps its own settings. Answers with the session's id, its account (`account`, the id or `default`, and `account_name`) and the `model` and `effort` the relaunch used — the session's own, else its project's or console's — `null` for the agent's own default. The caller is the console session the session is bound to and is running, so reopening one never brings a console session back. |
| `handoff_session` | `session`, `agent`, `account?`, `title?`, `model?`, `effort?`, `note?` | Hands one of the caller's sessions off to a new session of another agent, which takes its place and stays the caller's, and archives the original, once the user approves, unless they have chosen to have handoffs approved without asking. See "Handing off from an owner's tool" in `docs/product/handing-off-sessions.md`. |
| `handover_branches` | `branches`, `from?`, `to?` | Moves the responsibility for git branches from one session of the caller's own to another, without the user and without replacing any session. A console session is responsible for no branch, so it names both `from` and `to`. See "Handing branches between sessions of an owner" in `docs/product/branch-responsible-session.md`. |
| `switch_session_account` | `session`, `account` | Moves one of the caller's sessions to another account of its own agent, keeping its conversation, once the user approves, unless they have chosen to have switches approved without asking. The session comes back awaiting its next step from the caller, unless its `note` says it was sent a prompt to continue a turn the switch cut off or a usage limit had stopped, which the caller does not repeat. See "Switching from an owner's tool" and "Continuing after a switch" in `docs/product/switching-session-accounts.md`. |
| `transfer_session` | `session`, `to`, `note` | Hands one of the caller's sessions to another owner, without the user. See "Handing a session to another owner" in `docs/product/changing-session-owners.md`. |
| `release_session` | `session`, `note?` | Releases one of the caller's sessions from its binding, without the user: it becomes the user's own, unbound session, and is passed the optional `note` as its former owner's parting note. See "Releasing a session from its owner" in `docs/product/changing-session-owners.md`. |
| `request_session` | `kind`, `project?`, `console?`, `brief`, `title?`, `agent?`, `account?`, `model?`, `effort?`, `branches?` | Asks the user to start a session that is nobody's. See `docs/product/session-requests.md`. |
| `request_resume_session` | `conversation`, `project?`, `title?`, `model?`, `effort?` | Asks the user to resume a conversation by its id, as the session dialog's Resume conversation tab does; a console session has to name the project. See `docs/product/requests-to-resume-a-conversation.md`. |
| `send_message` | see `docs/product/information-between-sessions.md` | As for every session. |
| `show_page` | `html` | Pushes an HTML page to the caller's report panel, beside its own terminal. Answers with the new page's id. See "Pushing a page" in `docs/product/report-panel.md`. |
| `list_pages` | `page?` | Lists the pages the caller pushed, or reads one of them back in full. See "Reading back the pages pushed" in `docs/product/report-panel.md`. |
| `delete_pages` | `pages?`, `all?` | Withdraws pages the caller pushed, for good: the ids named, or every one. Answers with the ids deleted. See "Withdrawing pages" in `docs/product/report-panel.md`. |

A project is named either by its id or by its name where that name is unambiguous within the
console; an ambiguous name is refused and asks for the id. A project of another console is refused, except by
`request_session` and `request_resume_session`, which may name any project, and by the reads `list_sessions` and
`list_archived` (see `docs/product/what-every-session-reads.md`).

**Writes are the caller's own.** `instruct_session`, `interrupt_session`, `rename_session`, `set_session_pinned`,
`archive_session`, `reopen_session`, `handoff_session`, `switch_session_account`, `transfer_session` and
`release_session` act only on a session bound to the caller; for any other — unbound, bound to another console session
or to a project session, a console session, or the caller itself — they refuse, with a reason naming who owns it, and
change nothing. The reason says why it is not the caller's to do — only its owner instructs or drives it, or it is the
user's — and, unless the session is archived, that the caller can still tell it something with `send_message`, which it
weighs rather than obeys, adding for a bound session that its owner is sent a copy (see
`docs/product/information-between-sessions.md`); for the caller itself it says to say it to the user instead.
`archive_session` aimed at the caller itself is refused pointing to `archive_self` instead.
`new_session` always binds the new session to the caller, so a console session cannot start one on another's behalf: it
can hand one over afterwards with `transfer_session`, or ask the user for one that is nobody's with `request_session`.
`request_resume_session` can bring back a session that is not the caller's, but only with the user's approval, and it
stays whoever's it was (see `docs/product/requests-to-resume-a-conversation.md`). The settings tools change only the
caller's own console, its projects and the application-wide settings (see `docs/product/agent-settings-tools.md`).

A refused call comes back to the agent as a **tool error carrying the reason in prose**, not as a
transport failure — "this session is waiting for the user" is advice the model is meant to act on.

## The unbound project session's tools

A project session that can own sessions — an **unbound project session**, one the user opened by hand without choosing a
console session under "Report to" (see "The session dialog" in `docs/product/opening-sessions.md`) or one started on
another session's request (see `docs/product/session-requests.md`), and a **lead session** — can start sessions in its
own project and drive them, the way a console session drives its console's. Besides the reads in
`docs/product/what-every-session-reads.md`:

| Tool | Arguments | What it does |
|---|---|---|
| `new_session` | `brief`, `title?`, `agent?`, `account?`, `model?`, `effort?`, `branches?` | Starts a session in the caller's own project, bound to the caller, and hands it the brief as its opening prompt, exactly as the console session's `new_session` does, with the same overrides and the same answer (see "Handing out a task: the brief" below). Needs no approval. There is no `project` argument; a call that names another project anyway is refused before anything is launched. `agent` may name any agent; left out, `agent` follows the caller's own; a left-out `account` is chosen as "The agent and account of a session a session starts" below describes. `branches` hands the new session branches the caller is responsible for, which its opening prompt then names (see "Handing branches to a session the caller starts" in `docs/product/branch-responsible-session.md`). Answers as the console session's does, and, where `branches` names a branch, also with `branches` (the ones moved) and a `note` on what became of them. Refused, with the reason in prose, when the session's resolved agent has been determined unavailable on this machine — never while that determination is still pending — or is not added, or when the account it would run on is turned off, has used up its quota or is not logged in (see "When the agent or the account cannot open a session" in `docs/product/opening-sessions.md`). |
| `instruct_session` | `session`, `text` | As the console session's. |
| `interrupt_session` | `session` | As the console session's. |
| `archive_session` | `session`, `keep?` | As the console session's. |
| `archive_self` | — | As the console session's. Offered to a lead session too, but refused for it: it wraps up with `report` instead (see "A session archiving itself" below). |
| `reopen_session` | `session`, `text?`, `model?`, `effort?` | As the console session's. |
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
| `send_message` | see `docs/product/information-between-sessions.md` | As for every session. |
| `get_project` | `refresh?` | Its own project's settings, read-only, as for every project session (see "A project's settings" in `docs/product/agent-settings-tools.md`), optionally with its git status checked first (see "Reading the status from a session" in `docs/product/project-git-status.md`). |
| `show_page`, `list_pages`, `delete_pages` | as the console session's | As the console session's: it is the project session that faces the user, and its pages show in its own report panel in its project's right pane (see `docs/product/report-panel.md`). |

It has no `add_project`, and none of the tools that change a setting: `update_project`, `get_console`,
`update_console`, `get_settings` and `update_settings` are the console session's alone.

**Writes are the caller's own** — the console session's rule. `instruct_session`, `interrupt_session`, `rename_session`,
`set_session_pinned`, `archive_session`, `reopen_session`, `handoff_session`, `switch_session_account`,
`transfer_session` and `release_session` act only on a session bound to the caller, and refuse, with a reason naming who
owns it, and change nothing, for any other: an unbound session, one bound to a console session or to another project
session, the caller itself, or a console session. For an unbound project session, `archive_session` aimed at itself is
refused pointing to `archive_self` instead.

A session a project session starts is bound to it, so it has the role of a bound one: it is offered `report`, the reads
in `docs/product/what-every-session-reads.md`, `send_message`, `get_project`, `request_session` and
`request_resume_session`, and cannot start or drive sessions of its own. It has no report panel tools either. It reports
to the project session that started it exactly as a console session's sessions report to their console session: see
`docs/product/reporting-to-owners.md`.

## Lead sessions

A **lead session** is a project session bound to a console session of its console that can still own sessions of its own
project. A session becomes one by binding itself, while unbound, to a running console session with `bind_to_session`
(see "Binding an unbound session to an owner" in `docs/product/changing-session-owners.md`). One with no console session
to bind to can first ask the user for one with `request_session`, of kind `console`, and bind to it once it has been
started (see `docs/product/session-requests.md`). A session that takes a lead session's place in a handoff is one too
(see `docs/product/handing-off-sessions.md`). A session opened bound to a console session, or started by one, is not a
lead session: it has the role of a bound one.

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
`docs/product/archiving-and-resuming-sessions.md` and "Deleting archived sessions" in
`docs/product/deleting-sessions.md`.

**The console session sees the lead session's sessions but does not drive them.** A lead session's `owner_kind` reads
`console`; its own sessions read `project`, with the lead session as their `owner`. They appear in the console session's
`list_projects`, `get_session` and `list_sessions`, not as its own; `instruct_session`, `interrupt_session`,
`rename_session`, `set_session_pinned`, `archive_session`, `reopen_session`, `handoff_session`,
`switch_session_account`, `transfer_session` and `release_session` aimed at one are refused, naming the lead session as
its owner (see "Which sessions an owner drives" below). The lead session itself is bound to the console session and is
driven by it like any other session of its own.

## Handing out a task: the brief

`new_session` takes a `brief` of four natural-language fields:

| Field | Required | Contents |
|---|---|---|
| `goal` | yes | The outcome to achieve. |
| `context` | no | Background and relevant leads. The field asks the caller to include the user's language, so that the session can name the sessions it starts in it (see "Naming the sessions a session starts" below). |
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

Every one of these `title` arguments tells the agent to write the name in **the user's language**: the language its
instructions name under "The user's language" where they have that section (see "The guidance on the user's language"
in `docs/product/launching-agents.md`), else the language the user converses with it in — not that of the project, its
docs and code, or a brief. Where its instructions name none, a session that takes its work from an owner session rather
than from the user uses the user's language as its owner passes it on — the brief's `context` asks for it (see
"Handing out a task: the brief" above) — else the language of its owner's messages. The tools' descriptions say this
the same way for every session, whatever its instructions hold. The language is the agent's to choose: Octoboard does
not check the name's language.

## The agent and account of a session a session starts

A session that a session starts itself — with `new_session`, or with `request_session` — runs on the agent and account
below unless the call names others. In the caller's own console the caller's own agent takes the place of the default
the project and console set (see "Which agent a session uses" in `docs/product/opening-sessions.md`), which applies only
where the caller's own cannot — except that a session a console session starts in a project always takes that default.
The account does not always follow the caller.

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
  account and does not fall through to a project pin. Left out or blank, while **Automatically select an account** is on
  (see that section in `docs/product/settings.md`), it is the account of the session's agent whose weekly quota resets
  soonest, among accounts that can still open a session — the same pick the new-session dialog makes, ranked as in
  "Which account a session uses" in `docs/product/opening-sessions.md`. That pick applies in the caller's own console
  and in another. When the setting is off, or no account ranks: in the caller's own console, it is the caller's own
  account, the agent's default account included, when the session is of the caller's own agent — named, or settled on,
  as when a project's default agent is the console session's own — and that account can still open a session. The
  caller's account is passed over, silently, when it no longer exists, is turned off (see "Turning an account off" in
  `docs/product/agent-accounts.md`), is used up by the reading the daemon holds (see "A used-up account" in
  `docs/product/account-quotas.md`) or that reading found it not logged in (see "An account that is not logged in"
  there). Then, and for a session of another agent, the account is the project's for that
  agent, else the console's. In another console, when none ranks, the account is the project's, else the console's, and
  never the caller's. The call is refused only when the account it settles on is turned off, used up or not logged in
  as well (see "When the agent or the account cannot open a session" in `docs/product/opening-sessions.md`).
- **Named ones win.** An `agent` or `account` the call names is used as given. A console session's tools, and every
  session's `request_session`, tell the caller to name either only when the user asked for it. A project session's
  `new_session` also lets it name another agent unasked when it judges that agent fits the work better and an account
  of it can open a session, and another account when that is the way to reach that agent (see "Choosing the session a
  task goes to" below).
- **The model and reasoning effort** are not the caller's: left out, they are inherited as for any new session of that
  agent (see "Which model and reasoning effort a session uses" in `docs/product/opening-sessions.md`).

A `request_session` for a session in **another console** — a console session there, or a session in one of its
projects — takes its agent from that console and its project, and nothing of the caller's agent. Its account, when
left out, is the pick above while automatic account selection is on, and the project's, else the console's, when the
setting is off or none ranks. `handoff_session` always names its receiving agent; a left-out account follows the
account rule above as in the caller's own console, the session it hands off being always in it (see "Handing off from
an owner's tool" in `docs/product/handing-off-sessions.md`).

## Which sessions an owner drives

A session started with `new_session` is always bound to the session that started it — a console session, or a project
session that is unbound or a lead session — so it reports to it, and it is that owner's while it stays bound to it: the
owner instructs it, archives it once it is done and reopens it when it has another task — for a project session that
owns it, another closely related task with room for it (see "Choosing the session a task goes to" below). A session
started on a `request_session`, once approved, is bound to nobody: it reports to the user, and neither the session that
asked for it nor that session's owner drives it (see `docs/product/session-requests.md`). A session **the user opens by
hand is not bound**, unless they choose a console session under "Report to" in the session dialog (see "The session
dialog" in `docs/product/opening-sessions.md`); the choice is none by default and offers console sessions only. A
binding changes afterwards only when its owner hands the session over, when an unbound session binds itself, when its
owner is handed off to another agent and the session moves to the one that takes the owner's place, or when its owner or
the user releases the session, which leaves it unbound (see `docs/product/changing-session-owners.md`). A session the
user opened bound to an owner can still be deleted by the user while it is unused, and the owner is not told (see
"Deleting an unused session" in `docs/product/deleting-sessions.md`); one started with `new_session` cannot.

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
  `context` in `get_session` and `list_sessions` (see "Session records" in
  `docs/product/what-every-session-reads.md`), judged by `tokens` when the window is not known.
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
`archive_self`, which takes no arguments: it ends the caller's own process and archives the caller. A session bound to
an owner does not archive itself this way; it wraps up with a `done` report with no open items (see "Automatic
archiving" in `docs/product/reporting-to-owners.md`). A lead session is offered `archive_self` with the rest of the
unbound project session's tools, but the call is refused, telling it to report `done` instead.

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
