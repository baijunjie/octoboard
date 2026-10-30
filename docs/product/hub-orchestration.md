# Orchestration

Orchestration is one session starting other sessions, handing them work, following them up and taking their reports.
The session directing another is its **parent session**; the session reporting to it is its **child session**.
The API also calls the parent the `owner`. An independent session answers directly to the user.

Project sessions have two separate axes: their **role** controls whether they can manage children; their **reporting
relationship** says whether they answer directly to the user or report to a parent session.

| Role | Reporting relationship | Capabilities |
|---|---|---|
| Coordinator | Independent | Works in its project, creates and manages child sessions in that project, and has a report panel. |
| Coordinator | Reports to a console session | Keeps those capabilities and reports its work to that console session. |
| Worker | Reports to a console or project session | Works in its project and reports to its parent; cannot create or manage child sessions and has no report panel tools. |

A coordinator does not need to have children. Having a parent alone does not make a session a worker. A worker always
has a parent; releasing it makes it an independent coordinator. A console session is a separate kind of session, with
coordination capabilities across its own console's projects.

The sessions that can manage children are:

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
- A **project coordinator** — it works in its own project and can create and manage child sessions there, whether
  independent or reporting to a console session (see "Project coordinators' tools" and "Coordinators with a parent
  session" below). It has its own report panel (see `docs/product/report-panel.md`).

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
change follows its kind and current role. A console session gets the full set of tools that act (see "The console
session's tools" below); a project coordinator gets a narrower set, scoped to its own project for what it starts and
drives (see "Project coordinators' tools" below); a worker gets `report` and cannot create or manage child sessions.
Every session also gets `send_message`, for sharing information with every other running session (see
`docs/product/information-between-sessions.md`), `request_session` and `request_resume_session`; every project session
also gets `get_project`, a read-only view of its own project's settings (see `docs/product/agent-settings-tools.md`).

**Tools follow the current role and reporting relationship.** These can change while the session runs (see
`docs/product/changing-session-owners.md`). A console session cannot report. The reporting hierarchy has at most two
edges: a console session, a project coordinator reporting to it, and a worker reporting to that coordinator.

**What no tool does, by design.** No session is offered a tool for any of these:

- **Opening a file or a diff in the window for the user.** Besides the sessions' own terminals, the report panel of a
  console session or project coordinator is the only place a session shows the user anything.
- **Deleting a session.** Deleting is destructive and cannot be undone, so it stays with the user (see
  `docs/product/deleting-sessions.md`); an owner archives the sessions it is done with instead.
- **Working independently by itself.** A session with a parent cannot release itself: the binding is the owner's to end,
  so only its owner or the user can release it (see "Releasing a session from its owner" in
  `docs/product/changing-session-owners.md`).
- **Locking or unlocking a session.** The lock is the user's protection against archiving or deleting a session, so
  only the user sets or clears it (see "Locking a session" in `docs/product/sessions.md`).
- **Renaming itself.** A session's title is for the user and its owner to set, so `rename_session` refuses the caller
  itself (see "The console session's tools" below).
- **Forcing the removal of branches or worktrees.** An agent's archive removes only what git removes without force; a
  worktree with changes, an unmerged branch or a locked worktree would lose work if forced, so taking it along stays with
  the user's archive confirmation, and `archive_session` has no `force_remove` (see "Archives made by an agent" in
  `docs/product/branch-and-worktree-cleanup.md`). Its `keep` goes the other way, removing nothing.
- **Taking over the sessions of an interrupted owner.** A session interrupted under an owner that is itself interrupted
  stays bound to that owner; the work is taken up again by resuming that owner, which then drives its sessions again,
  not by a new console session taking them over.
- **Changing a console session's colour.** The colour (only a console session has one) is cosmetic and the user's
  to choose, so there is nothing for a session to gain from a tool.
- **The project Git actions: pushing, switching branch, restoring changes, pulling.** A session runs git itself, under
  its own agent's permission prompts, so the project pane's actions are not duplicated for it; restoring changes
  discards work, and that stays with the user's confirmation. (`list_projects` and `get_project` with `refresh` only
  check the git status, and fast-forward a branch under the user's `auto_pull_repositories` setting.)
- **Shell terminals.** A session has its own terminal and runs commands through its agent, under that agent's
  permission prompts; the project pane's shell terminals are the user's.
- **Exporting the configuration.** An export carries what the settings tools deliberately do not expose (a console's
  avatar and console session instructions, and application settings that stay the user's own; see
  `docs/product/agent-settings-tools.md`), so no tool writes it out; it stays with the user's **Export…** button (see
  `docs/product/configuration-export-and-import.md`).

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
| `get_settings`, `update_settings` | — / `auto_pull_repositories?`, `auto_delete_unresumable_archives?`, `auto_select_account?`, `sidebar_order_by_status?`, `default_clone_dir?`, `git_workflow?`, `claude_compatibility?` | Reads or changes the application-wide settings. See "Application settings: `get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md`. |
| `new_session` | `project`, `brief`, `title?`, `agent?`, `account?`, `model?`, `effort?` | Starts a session in one of this console's projects, bound to the calling console session, and hands it the brief as its opening prompt. Needs no approval. `title` names the session (see "Naming the sessions a session starts" below). `agent`, `account`, `model` and `effort` choose those for that one session, as the session dialog's choices do (see "The session dialog" in `docs/product/opening-sessions.md`): `account` is an account by id or name, or `default` for the agent's default account, and must belong to the agent the session will run; a blank `model` or `effort` is the agent's own default, and any other must be one the agent offers (see "The Advanced section" in `docs/product/model-and-reasoning-effort.md`). Left out, `agent` is the project's default agent, not the caller's own; a left-out `account` is chosen as "The agent and account of a session a session starts" below describes. Answers with the new session's id, its agent, the account it opened on (`account`, the id or `default`, and `account_name`) and the `model` and `effort` it was launched with (`null` for the agent's own default), since the account and the launch settings are settled when the session opens rather than by the call. Refused, with the reason in prose, when the session's resolved agent has been determined unavailable on this machine — never while that determination is still pending — or is not added, or when the account it would run on is turned off, has used up its quota or is not logged in (see "When the agent or the account cannot open a session" in `docs/product/opening-sessions.md`). |
| `instruct_session` | `session`, `text` | Appends an instruction to a running session of the caller's. Answers with `delivered`, true only when it was written and not left unsent, and a `note` saying it was delivered, that it was queued, or that it was written into the session's input but its agent did not send it to the conversation — it is in the input box, or went to a view of the agent's background agents (see "A message written but not sent" and "A message a dialog discarded or a view took" in `docs/product/message-delivery.md`); the last two each say not to send it again. A session that a change of its account's sharing is restarting accepts it too, and holds it for the resumed process (see "When a change is carried out" in `docs/product/sharing-the-default-configuration.md`). Refused for an interrupted or archived session of the caller's, which has no process to take it, with a reason saying which it is and that `reopen_session` with `text` relaunches it and delivers the instruction. |
| `interrupt_session` | `session` | Stops the turn a running session of the caller's is in the middle of, as the user pressing its agent's interrupt key would; the session keeps its conversation and goes back to awaiting instructions. Answers with `interrupted`, the session's `status` afterwards and a `note`. A session not in a turn is left alone. See `docs/product/interrupting-turns.md`. |
| `rename_session` | `session`, `title` | Changes the title of one of the caller's sessions, under the rules of "Renaming a session" in `docs/product/sessions.md`. Answers with the session's record. |
| `set_session_pinned` | `session`, `pinned` | Pins or unpins one of the caller's sessions (see "Pinning" in `docs/product/order-of-projects-and-sessions.md`). Answers with the session's record. |
| `archive_session` | `session`, `keep?` | Ends the process of one of the caller's sessions and archives it. Once the process has ended, the git branches the archived sessions were working on, and the linked worktrees that have them checked out, are removed where git can do so without force; the answer waits for that and names in `branches_left` what stays, and why. With `keep` nothing is removed and nothing is waited for (see "Archives made by an agent" in `docs/product/branch-and-worktree-cleanup.md`). |
| `archive_self` | — | Ends the caller's own process and archives the caller, once every session under it is archived, removing its branches and worktrees as `archive_session` does. See "A session archiving itself" below. |
| `reopen_session` | `session`, `text?`, `model?`, `effort?` | Relaunches an archived or interrupted session of the caller's, continuing its conversation, and optionally hands it an instruction — its next task. A Claude Code session that resumes a conversation is launched with the instruction as its prompt; any other has it delivered once the relaunched session can take one (see "Messages held until a session can take them" in `docs/product/message-delivery.md`). `model` and `effort` reopen it with another model or reasoning effort, which become the session's own choices, its later launches included; a blank one is the agent's own default, and one left out keeps what the session has. Each is checked, before anything is relaunched, against what the session's agent offers under the account the session runs on, as `list_agent_models` lists it (see "The Advanced section" in `docs/product/model-and-reasoning-effort.md`); a value it does not offer is refused (`invalid_launch_setting`), with nothing reopened or recorded. A reopen whose relaunch fails puts the session's earlier model and effort back. The session stays on its agent and its account (`switch_session_account` moves it to another account), and an archived owner that comes back with it keeps its own settings. Answers with the session's id, its account (`account`, the id or `default`, and `account_name`) and the `model` and `effort` the relaunch used — the session's own, else its project's or console's — `null` for the agent's own default. The caller is the console session the session is bound to and is running, so reopening one never brings a console session back. |
| `handoff_session` | `session`, `agent`, `account?`, `title?`, `model?`, `effort?`, `note?` | Hands one of the caller's sessions off to a new session of another agent, which takes its place and stays the caller's, and archives the original, once the user approves, unless they have chosen to have handoffs approved without asking. See "Handing off from an owner's tool" in `docs/product/handing-off-sessions.md`. |
| `handover_branches` | `branches`, `from?`, `to?` | Moves the responsibility for git branches from one session of the caller's own to another, without the user and without replacing any session. A console session is responsible for no branch, so it names both `from` and `to`. See "Handing branches between sessions of an owner" in `docs/product/branch-responsible-session.md`. |
| `handover_processes` | `processes`, `from?`, `to?` | Moves process trees (a dev server, a preview, a watcher) from one running session of the caller's own to another, without the user: they keep running and are stopped when the receiving session's process ends instead of the giving one's. A project process, which no session holds, cannot be handed to a session. See "Moving processes between sessions" in `docs/product/background-processes.md`. |
| `switch_session_account` | `session`, `account` | Moves one of the caller's sessions to another account of its own agent, keeping its conversation, once the user approves, unless they have chosen to have switches approved without asking. The session comes back awaiting its next step from the caller, unless its `note` says it was sent a prompt to continue a turn the switch cut off or a usage limit had stopped, which the caller does not repeat. See "Switching from an owner's tool" and "Continuing after a switch" in `docs/product/switching-session-accounts.md`. |
| `set_session_model` | `session`, `model?`, `effort?` | Changes the model or reasoning effort of a session of the caller's on the same agent and account, without approval. See "Changing a session's model and effort" in `docs/product/model-and-reasoning-effort.md`. |
| `transfer_session` | `session`, `to`, `note` | Hands one of the caller's sessions to another owner, without the user. See "Handing a session to another owner" in `docs/product/changing-session-owners.md`. |
| `release_session` | `session`, `note?` | Releases one of the caller's sessions from its binding, without the user: it becomes the user's own, independent session, and is passed the optional `note` as its former owner's parting note. See "Releasing a session from its owner" in `docs/product/changing-session-owners.md`. |
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
`archive_session`, `reopen_session`, `handoff_session`, `switch_session_account`, `set_session_model`,
`transfer_session` and `release_session` act only on a session bound to the caller; for any other — independent, bound
to another console session or to a project session, a console session, or the caller itself — they refuse, with a reason
naming who owns it, and change nothing. The reason says why it is not the caller's to do — only its owner instructs or
drives it, or it is the user's — and, unless the session is archived, that the caller can still tell it something with
`send_message`, which it weighs rather than obeys, adding for a reporting session that its owner is sent a copy (see
`docs/product/information-between-sessions.md`); for the caller itself it says to say it to the user instead.
`archive_session` aimed at the caller itself is refused pointing to `archive_self` instead. `new_session` always binds
the new session to the caller, so a console session cannot start one on another's behalf: it can hand one over
afterwards with `transfer_session`, or ask the user for one that is nobody's with `request_session`.
`request_resume_session` can bring back a session that is not the caller's, but only with the user's approval, and it
stays whoever's it was (see `docs/product/requests-to-resume-a-conversation.md`). The settings tools change only the
caller's own console, its projects and the application-wide settings (see `docs/product/agent-settings-tools.md`).

A refused call comes back to the agent as a **tool error carrying the reason in prose**, not as a
transport failure — "this session is waiting for the user" is advice the model is meant to act on.

## Project coordinators' tools

A project coordinator can create and manage child sessions in its own project, whether independent or reporting to
a console session. Besides the reads in `docs/product/what-every-session-reads.md`:

| Tool | Arguments | What it does |
|---|---|---|
| `new_session` | `brief`, `title?`, `agent?`, `account?`, `model?`, `effort?`, `branches?` | Starts a session in the caller's own project, bound to the caller, and hands it the brief as its opening prompt, exactly as the console session's `new_session` does, with the same overrides and the same answer (see "Handing out a task: the brief" below). Needs no approval. There is no `project` argument; a call that names another project anyway is refused before anything is launched. `agent` may name any agent; left out, `agent` follows the caller's own; a left-out `account` is chosen as "The agent and account of a session a session starts" below describes. `branches` hands the new session branches the caller is responsible for, which its opening prompt then names (see "Handing branches to a session the caller starts" in `docs/product/branch-responsible-session.md`). Answers as the console session's does, and, where `branches` names a branch, also with `branches` (the ones moved) and a `note` on what became of them. Refused, with the reason in prose, when the session's resolved agent has been determined unavailable on this machine — never while that determination is still pending — or is not added, or when the account it would run on is turned off, has used up its quota or is not logged in (see "When the agent or the account cannot open a session" in `docs/product/opening-sessions.md`). |
| `instruct_session` | `session`, `text` | As the console session's. |
| `interrupt_session` | `session` | As the console session's. |
| `archive_session` | `session`, `keep?` | As the console session's. |
| `archive_self` | — | As the console session's. Offered to a coordinator with a parent too, but refused for it: it wraps up with `report` instead (see "A session archiving itself" below). |
| `reopen_session` | `session`, `text?`, `model?`, `effort?` | As the console session's. |
| `rename_session` | `session`, `title` | As the console session's. |
| `set_session_pinned` | `session`, `pinned` | As the console session's. |
| `handoff_session` | `session`, `agent`, `account?`, `title?`, `model?`, `effort?`, `note?` | As the console session's. |
| `handover_branches` | `branches`, `from?`, `to?` | As the console session's, except that the caller can be responsible for branches itself: `from` or `to` left out is the caller, so it can also give its own branches to a session of its own or take one of theirs. |
| `handover_processes` | `processes`, `from?`, `to?` | As the console session's; `from` or `to` left out is the caller, so it can also give the processes it started to a session of its own or take one of theirs. |
| `switch_session_account` | `session`, `account` | As the console session's. |
| `set_session_model` | `session`, `model?`, `effort?` | As the console session's. |
| `transfer_session` | `session`, `to`, `note` | As the console session's. See "Handing a session to another owner" in `docs/product/changing-session-owners.md`. |
| `release_session` | `session`, `note?` | As the console session's. See "Releasing a session from its owner" in `docs/product/changing-session-owners.md`. |
| `bind_to_session` | `owner`, `summary` | Binds the caller, while it is independent, to an owner, without the user. Refused for a coordinator with a parent, which is bound already. See "Setting a parent for an independent session" in `docs/product/changing-session-owners.md`. |
| `request_session` | as the console session's | Asks the user to start a session that is nobody's. See `docs/product/session-requests.md`. |
| `request_resume_session` | `conversation`, `project?`, `title?`, `model?`, `effort?` | As the console session's; `project` defaults to the caller's own. See `docs/product/requests-to-resume-a-conversation.md`. |
| `report` | as in `docs/product/reporting-to-owners.md` | Offered, but refused while the session is independent: it has nobody to report to. A coordinator with a parent can report to that console session (see "Coordinators with a parent session" below). |
| `send_message` | see `docs/product/information-between-sessions.md` | As for every session. |
| `get_project` | `refresh?` | Its own project's settings, read-only, as for every project session (see "A project's settings" in `docs/product/agent-settings-tools.md`), optionally with its git status checked first (see "Reading the status from a session" in `docs/product/project-git-status.md`). |
| `show_page`, `list_pages`, `delete_pages` | as the console session's | As the console session's: it is the project session that faces the user, and its pages show in its own report panel in its project's right pane (see `docs/product/report-panel.md`). |

It has no `add_project`, and none of the tools that change a setting: `update_project`, `get_console`,
`update_console`, `get_settings` and `update_settings` are the console session's alone.

**Writes are the caller's own** — the console session's rule. `instruct_session`, `interrupt_session`, `rename_session`,
`set_session_pinned`, `archive_session`, `reopen_session`, `handoff_session`, `switch_session_account`,
`set_session_model`, `transfer_session` and `release_session` act only on a session bound to the caller, and refuse,
with a reason naming who owns it, and change nothing, for any other: an independent session, one bound to a console
session or to another project session, the caller itself, or a console session. For an independent project session,
`archive_session` aimed at itself is refused pointing to `archive_self` instead.

A child session created with `new_session` is a worker: it is offered `report`, the reads in
`docs/product/what-every-session-reads.md`, `send_message`, `get_project`, `request_session` and
`request_resume_session`, and cannot create or manage child sessions. It has no report panel tools. It reports to the
session that started it; see `docs/product/reporting-to-owners.md`.

## Coordinators with a parent session

An independent project coordinator can set a running console session of its console as its parent with
`bind_to_session` (see "Setting a parent for an independent session" in `docs/product/changing-session-owners.md`).
It keeps its coordinator role, including when it has no child sessions. One without a console session to report to
can first ask the user for one with `request_session`, of kind `console`, and set it as its parent once started
(see `docs/product/session-requests.md`). A handoff preserves the role and reporting relationship (see
`docs/product/handing-off-sessions.md`). A session opened with a console parent in the session dialog, or created
by that console session with `new_session`, starts as a worker.

- **It keeps its child sessions and report panel.** Existing children continue reporting to it, and it can create
  more in its own project with "Project coordinators' tools" above.
- **It keeps its launch description.** Every launch — a resume, a reopen, a switch of account — gives it the
  independent coordinator's role description and tool list. The result that sets its parent, and later notices of
  reporting changes, replace the description's statement that it is independent.
- **It reports to its console parent.** Its `report` is accepted and delivered there. A `done` report with no open
  items is refused while its children are unfinished and automatic archiving is not blocked by a lock (see
  "Automatic archiving" in `docs/product/reporting-to-owners.md`). No report is synthesised while a child has a
  process (see "When a session does not report" in that document).
- **Its console parent can transfer it.** Transfer to another console session of the same console preserves its
  role and children. Transfer to a project coordinator is allowed only when it has no children, archived ones
  included; it becomes a worker, loses the tools for managing children, and its report panel pages are withdrawn
  (see "Handing a session to another owner" in `docs/product/changing-session-owners.md` and "When a session becomes
  a worker" in `docs/product/report-panel.md`).
- **It can be released.** Its console parent or the user can release it. It becomes independent, retaining its
  coordinator role and children (see "Releasing a session from its owner" in `docs/product/changing-session-owners.md`).
- **Its children are workers.** They cannot be parents: creating a child under one, or transferring or setting a
  parent to one, is refused. There is no third reporting edge and no cycle.

A session is **under** a parent when it reports directly to it, or reports to a project coordinator that reports to
it. Archiving, reopening and deleting follow both reporting edges; see "Archiving, interruption and resuming" in
`docs/product/archiving-and-resuming-sessions.md` and "Deleting archived sessions" in
`docs/product/deleting-sessions.md`.

**A console session sees a coordinator's children but does not manage them.** The coordinator's `owner_kind` reads
`console`; its children read `project`, with the coordinator as their `owner`. They appear in the console session's
`list_projects`, `get_session` and `list_sessions`, with `yours: false`. `instruct_session`, `interrupt_session`,
`rename_session`, `set_session_pinned`, `archive_session`, `reopen_session`, `handoff_session`,
`switch_session_account`, `set_session_model`, `transfer_session` and `release_session` aimed at one are refused, naming
the coordinator as its parent (see "Which sessions an owner drives" below). The console session manages the coordinator
itself as its direct child.

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

A session started with `new_session` or `request_session` begins with an agent preference based on its destination
and caller. An explicit agent or account wins; automatic selection can change an unnamed choice as described below.

- **The initial agent.** Left out, `agent` is settled by who calls and where the session is:

  - A project session's `new_session`, and its `request_session` in its own console, prefer the caller's own agent.
  - A console session's `new_session`, and its `request_session` for a project session, prefer the project's default
    agent — the project's own, else the console's project-session default — as in "Which agent a session uses" in
    `docs/product/opening-sessions.md`.
  - A console session's `request_session` for a console session in its own console prefers the caller's own agent.
  - In another console, the initial agent comes from that console and its project; the caller's agent is not inherited.

  With automatic selection off, an inherited caller's agent that is unavailable or not added falls back to the
  destination's default. With automatic selection on, the usable-account search below also skips unavailable or
  unadded agents.
- **Automatic selection.** While **Automatically select an account** is on and `account` is left out or blank, each
  candidate agent uses this account preference, stopping at the first usable choice:

  1. Its usable account whose weekly quota resets soonest, ranked as in "Which account a session uses" in
     `docs/product/opening-sessions.md`.
  2. The caller's own account, including the agent's default account, only in the caller's own console and only when
     this candidate is the caller's agent.
  3. The project's inherited account for this agent, else the console's, else the agent's default account.
  4. The first usable account in this agent's saved display order.

  Removed accounts, turned-off accounts, accounts found not logged in, used-up accounts and accounts whose sharing of
  the default configuration is being changed are passed over. An unknown or unavailable quota reading alone does not
  exclude an account. If no account of the initial agent is usable and `agent` was not named, the other added,
  installed agents are tried in added-agent order, each with the same preferences. A fallback to the caller's agent
  within the same console still considers the caller's account; in another console it never does. The resulting
  agent and account are fixed when the call is made, including for a request that waits for approval. If no added agent
  has a usable account, the call is refused on the initial agent's account that becomes usable soonest, as set out in
  "Which account a session uses" in `docs/product/opening-sessions.md`; when no such account qualifies, the ordinary
  caller or project/console inheritance rules apply.
- **With automatic selection off**, a left-out or blank account is the caller's own account when the destination is
  in the same console, uses the same agent and that account can still open a session. A removed, turned-off, used-up
  or not-logged-in caller account is passed over. Otherwise the account comes from the project, else the console,
  else the agent's default account. No search of other accounts or agents is added for unavailable accounts.
- **Named ones win.** A named `agent` restricts automatic selection to that agent. A named `account` is used as given,
  including `default`, which selects the agent's default account outright. A console session's tools, and every
  session's `request_session`, tell the caller to name either only when the user asked for it. A project session's
  `new_session` also lets it name another agent unasked when that agent fits the work better and an account of it can
  open a session, and another account when that is the way to reach that agent (see "Choosing the session a task goes
  to" below).
- **The model and reasoning effort** are not the caller's. Left out, they are inherited for the final selected agent
  from the destination project or console (see "Which model and reasoning effort a session uses" in
  `docs/product/opening-sessions.md`). Explicit values are checked against that agent's offered choices.

`handoff_session` always names its receiving agent and keeps it. With automatic selection on, its left-out account is
chosen as above for a session of that agent the caller starts where the handed-off session is, and is never another
agent's: the receiving session is always in the caller's own console, so the caller's own account takes its place in
that order when the receiving agent is the caller's. With it off, its left-out account follows "Handing off from an
owner's tool" in `docs/product/handing-off-sessions.md`. Resuming a conversation and switching a session's account
likewise keep their specified agent.

## Which sessions an owner drives

A session started with `new_session` is always bound to the session that started it — a console session, or a project
coordinator — so it reports to it, and it is that owner's while it stays bound to it: the owner instructs it, archives
it once it is done and reopens it when it has another task — for a project session that owns it, another closely related
task with room for it (see "Choosing the session a task goes to" below). A session started on a `request_session`, once
approved, is bound to nobody: it reports to the user, and neither the session that asked for it nor that session's owner
drives it (see `docs/product/session-requests.md`). A session **the user opens by hand is independent**, unless they
choose a console session under "Report to" in the session dialog (see "The session dialog" in
`docs/product/opening-sessions.md`); the choice is none by default and offers console sessions only. A binding changes
afterwards only when its owner hands the session over, when an independent session binds itself, when its owner is
handed off to another agent and the session moves to the one that takes the owner's place, or when its owner or the user
releases the session, which leaves it independent (see `docs/product/changing-session-owners.md`). A session the user
opened bound to an owner can still be deleted by the user while it is unused, and the owner is not told (see "Deleting
an unused session" in `docs/product/deleting-sessions.md`); one started with `new_session` cannot.

An owner drives only the sessions bound to it. Every session record it reads names that session's owner, and a session
that is independent, or bound to any other session, is not this one's to drive: it can read such a session, but is told
to leave it alone and is refused if it tries to act on it.

A console session itself never reports anywhere, and neither does an independent project session.

## Choosing the session a task goes to

The role description of a project coordinator tells it when to
start sessions of its own and which session a piece of work goes to. A console session is not told: it works out which
project a request belongs to and does not concern itself with how a project's work is split among sessions. A project
session with the worker role owns none and is not told either. This is guidance to the agent; Octoboard does not
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

A session that reports to nobody — a console session or an independent project session — wraps itself up with
`archive_self`, which takes no arguments: it ends the caller's own process and archives the caller. A session bound to
an owner does not archive itself this way; it wraps up with a `done` report with no open items (see "Automatic
archiving" in `docs/product/reporting-to-owners.md`). A coordinator with a parent is offered `archive_self` with the
rest of the project coordinator's tools, but the call is refused, telling it to report `done` instead.

An owner archives each of its sessions once its task is done (see "Which sessions an owner drives" above), so a
session under the caller that is not archived means unfinished work. The call is refused, with the reason in prose,
and changes nothing:

- **while the caller, or any session under it, is locked** — a session under it counts even when it is archived
  already. The refusal names the locked sessions under it, which the user has to unlock (see "Locking a session" in
  `docs/product/sessions.md`). Locks are checked before what follows.
- **while any session under it is not archived**, running or interrupted alike. The refusal names each such session by
  id and title. Those bound to the caller come with the advice to finish them and archive them with `archive_session`
  first. Those bound to a coordinator reporting to the caller are not the caller's to archive: they are named
  grouped under their coordinator, with the advice to wait for the coordinator to finish them — its own `done` report
  archives it — or to archive the coordinator with `archive_session`, which takes along its sessions that have no
  process running and is refused while any of them has one (see "Archiving, interruption and resuming" in
  `docs/product/archiving-and-resuming-sessions.md`).

Otherwise the call is answered first — with the caller's id, the status `archived` and a note that its process will
now end — and then the caller's process is ended and the caller is archived, as an owner's explicit archive does.
Only the caller is archived; nothing under it is touched. When the removal following the archive will leave some of
the branches and worktrees bound to the caller, judged before the archive, the answer names them in `branches_left`
(see "Archives made by an agent" in `docs/product/branch-and-worktree-cleanup.md`).
