# Archiving and resuming sessions

How a session's process is ended by archiving or by an interruption, how a dormant session — interrupted or archived —
is resumed or reopened, when its conversation can no longer be resumed, and how long archived sessions are kept.
Sessions themselves and locking are in `docs/product/sessions.md`, their statuses in `docs/product/session-statuses.md`;
deleting sessions for good is in `docs/product/deleting-sessions.md`.

## Archiving, interruption and resuming

**Archiving** ends the agent's process and keeps the session and its record. The agent is asked to
exit first (`SIGTERM`) and is killed only if it is still there 3 seconds later.

**Nothing an agent started outlives its process.** However an agent's process ends — it exits on its own, exits when
asked, or is killed — once it has exited every process still in its terminal session is killed outright (`SIGKILL`):
its tool subprocesses, including helpers that moved to a process group of their own or ignore the hangup, such as
background tasks, MCP or REPL helpers and dev servers it started. Work an agent sent off to run in the background
therefore ends with it. A process that detached into a session of its own (`setsid`) is out of reach and keeps
running. Shell terminals do not work this way (see "Closing a shell terminal" in `docs/product/shell-terminals.md`);
a daemon going down on an internal failure is in "Crashes and forced termination" in
`docs/product/application-lifecycle.md`.

Archiving is available for a session that is not already archived, including an interrupted one, subject to "Locking a
session" in `docs/product/sessions.md`. A console session is archived from its own row's menu, like any other (see
"Console sessions and project sessions" in `docs/product/sessions.md`). The user archives a session from its row's menu.
A session at its prompt or interrupted, with no unarchived session under it, is archived at once, since nothing is cut
short, unless a branch or worktree bound to it needs the user's word; one starting, at work, waiting for the user,
working in background, being resumed, or with an unarchived session under it asks for confirmation first. For a session
working in background the confirmation says that its process ends and the work it is running in the background is
stopped, and that it can be reopened later; that message follows the session while the confirmation is open, as its
background work starts or finishes (see "Working in background" in `docs/product/session-statuses.md`). A session that
has never been used offers **Delete** there instead, unless it is still starting (see "Deleting an unused session" in
`docs/product/deleting-sessions.md`). An archive confirmed there that fails is reported in an error alert titled "Could
not archive the session", and the confirmation stays open. Once the processes have ended, an archive removes the git
branches the archived sessions were working on, and the linked worktrees that have them checked out, where git can do so
safely; its confirmation lists what git refuses, and locked worktrees, with the choice of taking them along (see
`docs/product/branch-and-worktree-cleanup.md`). Archiving a session that is still *starting* stands: it is archived at
once, and its agent's process is ended as soon as it comes up. An archived session keeps its pin, if it had one (see
"Order of projects and sessions" in `docs/product/order-of-projects-and-sessions.md`).

Besides the user, four things archive a session: its owner, explicitly; a project session's own report saying the
work is finished with nothing left open (see "Automatic archiving" in `docs/product/reporting-to-owners.md`); a
console session or unbound project session archiving itself, which it can do only once every session under it is
archived (see "A session archiving itself" in `docs/product/hub-orchestration.md`); and a handoff to another agent,
which archives the original once its place has moved (see `docs/product/handing-off-sessions.md`). Each archives that
one session and nothing else, except that an owner archiving a lead session archives it as an owner, under the rules
below.

**Archiving an owner — a console session, or a project session that has started sessions — follows its bindings.**
It reaches every session **under** it: the sessions bound directly to it and, for a console session, those under a
bound lead session, including descendants of an already archived lead (see "Lead sessions" in
`docs/product/hub-orchestration.md`). This includes sessions the user opened and later bound to the owner, as well as
sessions started by the owner.

- **The user's archive confirmation lists every unarchived session under the owner**, with its current status,
  and says that their processes will end and they will be archived together. The list follows the current bindings
  while the confirmation is open. Confirming ends and archives the whole group, even if descendants are starting,
  working, waiting for the user or at their prompt. It does not require archiving them separately first.
- **Agent tools and automatic reports retain the running-session safeguard.** If any descendant has a process
  running, including a launch, resume or account switch in progress, archiving the owner is refused and nothing
  changes. The refusal counts and names those sessions. With none running, the owner and its remaining unarchived
  descendants are archived together.
- All paths check the owner and every descendant for a lock before ending any process or archiving any session;
  an already archived descendant's lock also blocks the operation (see "Locking a session" in
  `docs/product/sessions.md`).
- Sessions outside those bindings are not touched. Ending an owner's process for an account switch archives
  nothing (see `docs/product/switching-session-accounts.md`).

**Resuming** an interrupted session happens by selecting it, or by pressing "Resume" on its terminal's card (see
"Interrupted, archived and dormant sessions" in `docs/product/session-terminal.md`, which also names the two ways of
showing an interrupted session that do not resume it), or, for a project session, by giving its conversation's session
ID in the session dialog's **Resume conversation** tab (see "Resuming or going to a session of this project from the
tab" in `docs/product/resuming-a-conversation-by-its-session-id.md`). **Reopening** an archived session is the same
relaunch, but selecting an archived session does not do it: the session is shown, still archived, with its history in
place of its terminal (see `docs/product/session-history.md`), and is reopened by pressing "Reopen" on its terminal's
card, in a focus mode's archived-row action menu (see "What both views show" in `docs/product/focus-mode.md`), in the
archive view (see "The archive view" in `docs/product/archive-view.md`), or, for a project session, by giving its
conversation's session ID in the session dialog's **Resume conversation** tab (see "Reopening an archived session from
the tab" in `docs/product/resuming-a-conversation-by-its-session-id.md`). Each relaunches the same agent in the same
directory and reassembles everything Octoboard injects. An owner session resumes or reopens a session of its own with
`reopen_session` (see "The console session's tools" in `docs/product/hub-orchestration.md`); any other session can ask
the user to resume or reopen a project session, by its conversation's session ID, with `request_resume_session` (see
`docs/product/requests-to-resume-a-conversation.md`). Both relaunch it as above.

- Where the agent has a stored conversation, that conversation is resumed.
- A session that **nobody ever typed into** has no stored conversation on the agent's side; resuming it opens a fresh
  conversation in its place, in the same project and under the same session. A session opened to resume a conversation
  is the exception: it has that conversation from the start (see
  `docs/product/resuming-a-conversation-by-its-session-id.md`).
- A resume carries no opening prompt, so the session comes up at the agent's prompt.
- A resume or reopen the user starts from the window on an account whose last change of sharing is incomplete asks
  the user first, and nothing is relaunched when that is cancelled (see "Starting a session on an account whose last
  change is incomplete" in `docs/product/sharing-the-default-configuration.md`).
- A session relaunches with the account and the config directory it last recorded for its agent — the one it was opened
  with until it is switched to another account (see `docs/product/switching-session-accounts.md`), not the console's
  current reference — and is refused if that directory no longer exists, but only when the session has a conversation to
  resume; one with none yet, opened and never typed into without resuming a conversation, launches into the missing
  directory instead (see "Agent config directories" in `docs/product/consoles.md`).
- **A session whose conversation its agent no longer has is not reopened.** Before anything is relaunched, the session
  is looked up in its agent's own records (see "When a conversation can no longer be resumed" below). When its
  conversation is proven gone, the resume or reopen is refused with `conversation_unresumable` and nothing is
  relaunched, its archived owners included: the session stays archived or interrupted, as it was. Its error alert is
  an actionable failure, not an unexpected one (see "Recovery and unexpected errors" in
  `docs/product/error-alerts.md`), whose next step is to archive the session if it is not archived already, delete it,
  and start a new session to carry on the work. Anything short of proof launches as before, with the launch's own
  refusals: a pinned account directory that no longer exists is still refused as in the bullet above
  (`config_dir_unreachable`).
- **A Codex conversation that Codex itself archived is brought back first.** When the session's rollout is only in the
  account's `archived_sessions/`, Octoboard runs `codex unarchive` for it under the session's account, then resumes it.
  If that fails, the reopen is refused with `codex_unarchive_failed`, carrying Codex's own message, and the session is
  left as it was. Its error alert is an actionable failure too, whose next step is to check that Codex runs in the
  login shell and can read its account directory, then reopen the session, and if it still fails, delete it and start
  a new one.
- **Reopening a session whose owner is archived reopens its archived owners first, level by level from the topmost
  down**, then the session itself, so what the session reports reaches a running owner: a session bound to an archived
  lead session whose console session is archived too brings back the console session, then the lead session, then
  itself. The walk goes up through the owners that are archived and stops at the first that is not; an owner that is
  only interrupted is not relaunched by this. If an owner cannot be reopened, the whole reopen fails with its reason:
  the owners already brought back stay reopened, and the rest, the session included, stay as they were. Likewise, if
  the owners came back and it is the session's own relaunch that then fails, they stay reopened.
  - The session and every owner about to be reopened are looked up in their agents' records before the first owner is
    relaunched, so a session refused with `conversation_unresumable` leaves its owners archived too. The session's own
    `codex unarchive` also runs before any owner is relaunched, so its failure leaves the whole group as it was; an
    archived Codex owner's runs just before that owner's own relaunch, and its failure is that owner's refusal.
  - **An archived owner whose conversation is proven gone does not stop the reopen**: what is left of its old
    conversation's records is moved to the Trash, under the rules in "Moving the agent's own records to the Trash" in
    `docs/product/deleting-sessions.md`, and the owner is relaunched into a fresh conversation, without the context of
    its earlier one, as a session nobody typed into would be. The session is then resumed as usual.
- **Reopening an owner on its own reopens nothing bound to it**, and reopening a session reopens none of the other
  sessions bound to the same owners. The group comes back one session at a time, from the session the user asked for.
- Resuming a session whose process is already running is refused. The refusal a double-click produces is not surfaced
  to the user.

An interrupted session and an archived session are relaunched the same way; the difference is how the session got
there, where it is listed, and that selecting an archived one does not relaunch it.

### When a conversation can no longer be resumed

Agents expire their own conversation records by their own rules, so the conversation of a dormant session can be gone
by the time it is reopened. Whether it can still be resumed is judged from the agent's own records, **looking only at
the account the session is on** — the account directory it last recorded, or the agent's default account — and only
for the agent's id of the session's current conversation. A copy left in another account by an earlier account switch
does not count. The answer is one of three:

- **Resumable**: the record is there. A session with no conversation yet (see the bullet on a session nobody ever
  typed into, above) is always resumable, since it relaunches into a fresh conversation.
- **Unresumable**: the record is proven gone from the account.

  | Agent | Unresumable when |
  |---|---|
  | Claude Code | no project directory of the account holds the transcript `<id>.jsonl`; a directory of the session's subagents' transcripts left without it does not count |
  | Codex | no rollout of the conversation, plain (`.jsonl`) or compressed (`.jsonl.zst`), is under the account's `sessions/` or `archived_sessions/`; a thread that had a turn reverted is one conversation, so any of its files (the first rollout or a later history segment) keeps it from being unresumable |
  | Grok Build | never: a session directory missing locally is unknown, since Grok Build restores sessions from its own remote registry |

- **Unknown**: neither can be proven — the account directory cannot be read (missing, unmounted, permission denied),
  or holds none of the agent's records directories at all; the default account's location cannot be determined; the
  session's agent id is missing or is not a UUID; or the record could be behind a symbolic link or a directory that
  cannot be searched in full.

  A session of an account whose directory shares the default configuration (see
  `docs/product/sharing-the-default-configuration.md`) reaches its records through links, so it is always judged
  unknown: it is never deleted automatically as a session whose conversation is gone, and never refused with
  `conversation_unresumable`. Once the account's sharing has been turned off, its sessions' conversations are back in
  the account's own directories and are judged as usual.

Only an unresumable answer is acted on: it refuses the session's reopen and relaunches an archived owner into a fresh
conversation (see "Archiving, interruption and resuming" above), and it lets an archived session be deleted
automatically (see "Deleting archived sessions whose conversation is gone" in `docs/product/deleting-sessions.md`).
An unknown answer is treated as resumable.

### Switching a session's account

A session can be moved to another account of its own agent, ending and relaunching its process with its conversation
carried along. The user does it from the session's **Switch account** submenu, and an owner with the
`switch_session_account` tool for a session of its own; how, and what is carried, is in
`docs/product/switching-session-accounts.md`.

### Handing a session off to another agent

A session's work can be moved to a new session of another agent, which takes its place — its owner, its branches and
the sessions it owns — and reads a package built from its conversation, while the original is archived. The user
starts one from the session's **Hand off** menu item, and an owner with the `handoff_session` tool for a session of its
own; see `docs/product/handing-off-sessions.md`.

### Archived sessions

A project's archived sessions, and a console's archived console sessions, stay until the user deletes them (see
"Deleting archived sessions" in `docs/product/deleting-sessions.md`), the project is removed or the console is deleted,
or until Octoboard deletes them once their agent's conversation is proven gone (see "Deleting archived sessions whose
conversation is gone" in `docs/product/deleting-sessions.md`). They are listed most recently archived first, and
reached:

- from the project's or console's "View archive" submenu, as the archive preview described in "The archive view" in
  `docs/product/archive-view.md`;
- from either focus mode, as that same preview, within that focus mode's scope;
- from the archive view, with all of them.

How each of those looks and behaves is in "Project rows" in `docs/product/sidebar-project-rows.md`, "The archive view"
in `docs/product/archive-view.md`, and `docs/product/focus-mode.md`.
