# Deleting sessions

How a session is deleted for good, rather than archived: an archived session, one at a time or all at once, an
archived session whose agent's conversation is gone, deleted automatically, and a session that has never been used.
Archiving itself, and where archived sessions are listed, are in "Archiving, interruption and resuming" and "Archived
sessions" in `docs/product/archiving-and-resuming-sessions.md`.

Every deletion is subject to "Locking a session" in `docs/product/sessions.md`, including unused sessions,
archived descendants and bulk deletion. A locked session blocks the whole operation before any session is removed.

## Deleting archived sessions

An archived session can be deleted: one at a time, or at once every archived session of a project, every
archived session under a console session, or every archived console session of a console. Deleting all at once always
asks for confirmation. Deleting one also asks, except for an unused session (see "Deleting an unused session"
below); a used session still asks even when its agent records are absent.

**Deleting an archived owner deletes the archived sessions bound to it, at both levels**: an archived coordinator among
them takes its own archived sessions along, so deleting an archived console session or an archived coordinator leaves no
archived session under it. Left behind, reopening one of them would have no owner to come back with, and its binding
would point at nothing. The confirmation says how many that is, counting both levels, before anything is deleted.
Deleting every archived console session of a console, and deleting every archived session under a console session, take
the archived sessions under them in the same way, and the confirmation says how many. A coordinator that is not archived
keeps its own sessions. Deleting an archived reporting session on its own leaves its owner alone, and neither deleting a
project's archived sessions nor deleting the archived sessions under a console session touches a console session.

- **Deleting removes Octoboard's record of the session**, for good, the last output Octoboard kept for it, the
  report panel pages it pushed (see `docs/product/report-panel.md`), and, for a session that received a handoff, its
  handoff package (see "The handoff package" in `docs/product/handing-off-sessions.md`). The session disappears from
  every client, and the orchestration tools' `list_archived_sessions` and `resume_session` no longer find it. The
  project's directory is never touched. The agent's own record of the conversation, in the agent's own configuration
  directory, is handled as described in "Moving the agent's own records to the Trash" below.
- Deleting a session that is neither archived nor unused (see "Deleting an unused session" below) is refused, saying
  that only an archived session, or one that has never been used, can be deleted, and not while it is starting. So is
  deleting an archived session that is being resumed at that moment. An archived session whose process is still on its
  way out can be deleted.
- Deleting everything at once skips a session that stopped being archived meanwhile, because a resume got there first,
  rather than failing.
- **Deleting also removes the git branches the deleted sessions were working on**, and the linked worktrees that have
  them checked out, where git can do so safely. The confirmation, for one session or all at once, lists what git
  refuses, and locked worktrees, with the choice of taking them along; the rules are in
  `docs/product/branch-and-worktree-cleanup.md`.

Removing a project or deleting a console also deletes its archived sessions along with every other session record
(see `docs/product/removing-projects.md` and "Deleting a console" in `docs/product/consoles.md`).

## Deleting archived sessions whose conversation is gone

Octoboard deletes on its own the archived sessions whose conversation their agent no longer has, since such a session
could only be refused if reopened. Only a session judged unresumable is deleted — never one whose conversation is
resumable or unknown (see "When a conversation can no longer be resumed" in
`docs/product/archiving-and-resuming-sessions.md`).

- **When**: when the daemon starts, which covers any time it was not running, and then whenever a day of wall-clock time
  has passed since the last check, time the machine spent asleep included. The check for whether a day has passed runs
  every hour.
- **Pinned and locked sessions are never deleted this way.**
- **An archived owner goes only together with the archived sessions bound to it, at both levels, and only when every
  one of them is unresumable.** If any member of that group is resumable or unknown, pinned, locked, still being
  launched or with a process still exiting, or if any session bound to the owner at either level is not archived, the
  whole group is kept for the user. An unresumable archived session whose owner is not archived is deleted on its own.
- A session that changed while it was being checked — reopened, pinned, locked, bound or independent — is kept, with its
  group, until a later check.
- **A deletion is the same as the user's with the confirmation's cleanup option left unticked** (see "Deleting
  archived sessions" above): the session disappears from every client, and its bound branches and worktrees are removed
  with nothing forced. What is left of its agent's own records — a Claude Code subagents' directory left without its
  transcript, say — stays in the agent's directory, in every account that holds it (see "Moving the agent's own records
  to the Trash" below).
- **Nobody is told**: no confirmation, alert or notice is shown. Each deletion, and any branch or worktree that could
  not be removed, is only written to the daemon's log.
- **It can be turned off** with the switch in Settings' Cleanup section (see "Automatically delete archived sessions
  whose conversation is gone" in `docs/product/settings.md`), which is on unless the user has turned it off. While it
  is off, a check deletes nothing. The switch is read at every check and again right before each deletion, so turning
  it off while a check is running stops that check before its next deletion; what it had already deleted stays
  deleted. Turning it on starts no check of its own: deleting resumes at the next check on the schedule above, at most
  about a day later, or when the daemon next starts. The switch does not affect reopening: a session whose
  conversation is gone is refused when reopened either way (see "When a conversation can no longer be resumed" in
  `docs/product/archiving-and-resuming-sessions.md`).

## Deleting an unused session

A session that has **never been used** is deleted rather than archived: its action menu offers **Delete** in place of
**Archive** — on its sidebar row and on its card in a focus mode, a console session's own card in its focus mode
included. Delete is disabled while locked.
Its row and card offer a Delete shortcut under the same conditions (see "Session rows" in
`docs/product/sidebar-session-rows.md`); this also applies to archived preview rows in focus mode.
A session is unused, whatever its status, while all of these hold:

- **It has no conversation yet.** The first prompt its agent reports as submitted makes it used for good, whether the
  user typed it or Octoboard wrote it in, and so does a prompt sent to it that the agent's own record of the
  conversation holds when no report arrives; a session opened to resume a conversation is used from the moment it is
  opened (see `docs/product/resuming-a-conversation-by-its-session-id.md`). When a session has a conversation is set in
  "Done, awaiting instructions and child sessions working" in `docs/product/session-statuses.md`.
- **It is bound to no owner, or the user opened it.** A session the user opened bound to an owner — choosing a
  console session under "Report to" in the session dialog, or opening it from a console session's focus mode (see
  "A console session's focus mode" in `docs/product/focus-mode.md`) — can be unused. A session an agent started with
  `open_session`, a console session's or a project session's, keeps Archive even before its first prompt lands: its
  owner handed it work and expects it to exist.
- **No session is bound to it.** A live owner with any session under it, in any status, keeps Archive.

A session still *starting* (see `docs/product/session-statuses.md`) is offered **Archive**, not Delete, even while all
of these hold: deleting it is refused until its launch has settled, while archiving it goes through.

**Deleting an unused session needs no confirmation**, including when it is archived. Whether its agent has created
records does not affect the shortcut or add a confirmation: Claude Code, Codex and Grok Build follow the same rule.
Its agent records are automatically moved to the system Trash under the rules in "Moving the agent's own records to
the Trash" below, unless they turn out to hold a prompt sent to it or may hold one. The session must still be unused
when deletion takes place; if it no longer qualifies, deletion is refused and an error is shown.

Deleting removes the session's record as described in "Deleting archived sessions" above, its bound branches and
worktrees included, with nothing forced since nothing is asked. Its process is stopped, and nothing is left in the
archive. A trust confirmation or session request of the session's that is still waiting is
dropped with it (see "How a request ends" in `docs/product/session-requests.md`).

**Deleting an unused session bound to an owner tells the owner nothing.** The session simply disappears: it is no
longer among the sessions the owner lists or reads, and the owner's later tool calls naming its id are refused as an
unknown session.

- The check is made at the moment of the delete: a session that has become used by then — a prompt submitted at the
  same moment wins — is refused as in "Deleting archived sessions" above, and keeps its record and its process. So is a
  session being launched, resumed or switched to another account at that moment.
- A session whose first prompt reached its agent's record with no report counts as unused until Octoboard next reads
  that record, up to about a second (see "Done, awaiting instructions and child sessions working" in
  `docs/product/session-statuses.md`), until a later prompt when the first is a line Octoboard cannot place, and for
  good when Octoboard has no id of its conversation. Deleted meanwhile, it goes with no confirmation; its agent records
  stay where they are (see "Moving the agent's own records to the Trash" below).
- An unused session can stop existing while a session is being bound to it. Opening a session bound to an owner that
  was deleted meanwhile is refused, with nothing launched; binding or handing a session to an owner that was deleted
  meanwhile is refused, and changes nothing (see
  `docs/product/changing-session-owners.md`).

## Moving the agent's own records to the Trash

**Deleting one session automatically moves the records of every removed session with no conversation to the Trash**,
including such archived descendants of a deleted owner, unless those records hold, or may hold, a prompt sent to the
session (see below). This applies even when the confirmation's cleanup option is left unticked. Deleting all archived
sessions at once always requires opting in to record cleanup.

**Why deleting one session moves some records without asking.** A session has no conversation until its agent
reports a submitted prompt or its agent's own record shows one, unless it was opened to resume a conversation (see
"Done, awaiting instructions and child sessions working" in `docs/product/session-statuses.md`). The records of such a
session hold no conversation of the user's, so deleting the session takes them too. Claude Code and Codex write no
record of a session until something is typed into it, a local command such as `/clear` included (Claude Code 2.1.330,
Codex 0.167.1). Grok Build writes the session's directory on launch, but Octoboard finds that directory only by a file
Grok Build does not write before the first prompt (Grok Build 1.0.92), so the directory of a Grok Build session never
prompted is neither read nor moved: it stays where it is. A used session's records hold the user's conversation, and
move only when the user ticks the confirmation's option.

**Before moving a session's records on the ground that it has no conversation, the deletion reads them.** Every copy
found (in every account looked in, as below) is read from its start until its first prompt sent to the session, told as
in "Done, awaiting instructions and child sessions working" in `docs/product/session-statuses.md`. If any copy holds
one, or one cannot be read through, or holds before any prompt a line Octoboard cannot place (one longer than 8 MiB,
Grok Build's list of attached images on a line of its own and the others that section lists), or Octoboard cannot tell
a sent prompt in that agent's records, the session's records are all left where they are. The session is deleted all
the same, and nothing tells the user that its records were kept. This covers every deletion that moves records without
the user's tick: the user's own with the option left unticked, the unconfirmed deletion of an unused session, and the
automatic deletion. With the option ticked,
the records move with no such reading. A session Octoboard has no agent id of the conversation for (see "Done, awaiting
instructions and child sessions working" in `docs/product/session-statuses.md`) has no records found, so none are moved.

**No deletion the user did not confirm with that option ticked moves a used session's records**, even when the agent
itself no longer has the conversation. So:

- the automatic deletion of archived sessions whose conversation is gone (see "Deleting archived sessions whose
  conversation is gone" above) moves records as a deletion with the option left unticked does; since every session it
  deletes had a conversation (a session with none is always resumable, see "When a conversation can no longer be
  resumed" in `docs/product/archiving-and-resuming-sessions.md`), it leaves every deleted session's records where they
  are;
- reopening a session whose archived owner's conversation is gone (see "Archiving, interruption and resuming" in
  `docs/product/archiving-and-resuming-sessions.md`) moves nothing: what is left of the owner's old conversation stays
  in the agent's directory. The owner no longer names that conversation once relaunched into a fresh one, so no later
  deletion of the owner moves those leftovers either, with the option ticked or not; they are the agent's own from then
  on.

For a deletion that asks for confirmation, Octoboard checks the agent's own files across the accounts described
below for **every session the deletion removes**, including archived descendants. The result is **present** when any
record is found, **absent** only when the whole scope is proved to have no records, and **unknown** when that cannot
be established.
A missing or invalid agent session ID, an account location that cannot be determined, an unreadable location or a
symbolic link can leave the result unknown. Having no conversation does not itself prove that no files exist.

The confirmation — for one session or all archived sessions at once — hides its cleanup option when the result is
absent. Otherwise it offers an **unchecked by default** checkbox to also move the agent's own session records to the
Trash, including while the check is pending or its result is unknown. It is a plain block, not inside an alert, and its
hint says that the session, or the sessions, can then no longer be resumed in the agent itself and that the records can
be recovered from the Trash. While it is ticked, a line in the confirmation's consequences, the body text after its
options and before its buttons, says that the agent's own records of the session — and of the sessions deleted with it,
or of all the sessions deleted at once — are also moved to the Trash; it comes after the other consequence lines (see
"The confirmation" in `docs/product/branch-and-worktree-cleanup.md` for how the consequences hold their list and their
lines). Unticked, used sessions' agent records are kept; for a bulk deletion, every session's agent records are kept.
Either way the confirmation's message says that the project's files are kept. How ticking the option changes the confirm
button's label is set in "What a confirmation's confirm button names" in `docs/product/dialogs.md`. While a deletion
runs, the button shows as pending and the confirmation stays open until it has finished, including moving the records
when the option is ticked; the option cannot be changed meanwhile.

- **The records are moved to the system Trash, never deleted outright**, after the sessions themselves are deleted.
  For each agent they are:

  | Agent | What is moved |
  |---|---|
  | Claude Code | the session's transcript, in every project directory it appears in, with the directory of its subagents' transcripts beside it |
  | Codex | the session's rollout files: the thread's first rollout and every later history segment, compressed or not (see "Agent session data" in `docs/product/launching-agents.md`) |
  | Grok Build | the session's directory |

- **They are looked for wherever this machine's accounts of that agent could hold them**: the account the session last
  ran under and every account of that agent known to Octoboard, since a session switched to another account leaves its
  original copy in the account it came from (see "Carrying the conversation" in
  `docs/product/switching-session-accounts.md`). The agent's default account (see "Agent config directories" in
  `docs/product/consoles.md`) is looked in only for a session on the default account, or on an account that shares the
  default configuration with its last change of it complete, whose conversations are kept there (see
  `docs/product/sharing-the-default-configuration.md`). A session on any other account never has anything removed from
  the default account's directory, however that directory is spelled or reached through links — a copy an earlier
  switch left there included. The default account is skipped when where it lives cannot be read from the user's shell
  environment.
- **Only the session's own records are taken.** A record is found by the agent's own id of the session, which has to be
  a canonical UUID, at the exact place the agent's layout puts it in the account's directory; a record under a
  symbolically linked directory is left, and nothing else in the account's directory is touched.
- **A record another remaining session still refers to is left**: when a session that is not deleted runs the same
  agent on the same agent conversation, that conversation's records are not moved.
- A record that is missing is skipped.
- **The deletion succeeds whatever happens to the records.** A record that cannot be moved stays where it is, and the
  failure is only logged; the sessions are deleted all the same.
- An unused session's process is stopped by the deletion; if it writes to its transcript once more after the move, a
  small new transcript can be left in the agent's directory.
