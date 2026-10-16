# Deleting sessions

How a session is deleted for good, rather than archived: an archived session, one at a time or all at once, and a
session that has never been used. Archiving itself, and where archived sessions are listed, are in "Archiving,
interruption and resuming" and "Archived sessions" in `docs/product/sessions.md`.

## Deleting archived sessions

An archived session can be deleted: one at a time, or at once every archived session of a project, every
archived session under a console session, or every archived console session of a console. Deleting all at once always
asks for confirmation. Deleting one also asks, except for an unused session whose agent records are proved absent
(see "Deleting an unused session" below); a used session still asks even when its agent records are absent.

**Deleting an archived owner deletes the archived sessions bound to it, at both levels**: an archived lead session
among them takes its own archived sessions along, so deleting an archived console session or an archived lead session
leaves no archived session under it. Left behind, reopening one of them would have no owner to come back with, and its
binding would point at nothing. The confirmation says how many that is, counting both levels, before anything is
deleted. Deleting every archived console session of a console, and deleting every archived session under a console
session, take the archived sessions under them in the same way, and the confirmation says how many. A lead session that
is not archived keeps its own sessions. Deleting an archived bound session on its own leaves its owner alone, and
neither deleting a project's archived sessions nor deleting the archived sessions under a console session touches a
console session.

- **Deleting removes Octoboard's record of the session**, for good, and the last output Octoboard kept for it. The
  session disappears from every client, and the orchestration tools' `list_archived` and `reopen_session` no longer
  find it. The project's directory is never touched. The agent's own record of the conversation, in the agent's own
  configuration directory, is kept too, unless the confirmation's Trash option is ticked (see "Moving the agent's own
  records to the Trash" below).
- Deleting a session that is neither archived nor unused (see "Deleting an unused session" below) is refused, saying
  that only an archived session, or one that has never been used, can be deleted, and not while it is starting. So is
  deleting an archived session that is being resumed at that moment. An archived session whose process is still on its
  way out can be deleted.
- Deleting everything at once skips a session that stopped being archived meanwhile, because a resume got there first,
  rather than failing.

Removing a project or deleting a console also deletes its archived sessions along with every other session record
(see "Removing a project" and "Deleting a console" in `docs/product/consoles-and-projects.md`).

## Deleting an unused session

A session that has **never been used** is deleted rather than archived: its action menu offers **Delete** in place of
**Archive** — on its sidebar row, on its card in a focus mode and, for a console session, in its focus mode's header.
Its row and card also offer a Delete shortcut (see "Session rows" in `docs/product/sidebar.md`).
A session is unused, whatever its status, while all of these hold:

- **It has no conversation yet.** The first prompt its agent reports as submitted makes it used for good, whether the
  user typed it or Octoboard wrote it in; a session opened to resume a conversation is used from the moment it is
  opened (see "Resuming a conversation started outside Octoboard" in `docs/product/sessions.md`).
- **It is bound to no owner.** A live session another session started, or one opened to report to a console session,
  keeps Archive even before its first prompt lands, since it reports to that owner.
- **No session is bound to it.** A live owner with any session under it, in any status, keeps Archive.

A session still *starting* (see "Session statuses" in `docs/product/sessions.md`) is offered **Archive**, not Delete,
even while all of these hold: deleting it is refused until its launch has settled, while archiving it goes through.

**Deleting first checks for the agent's own records**, including when the unused session is archived. While the check
is pending, no confirmation is shown. When their absence is proved, deletion goes through without confirmation and
without moving any agent records to the Trash. The session is checked again at deletion: it must still be the same
unused session and its agent records must still be proved absent. If that guarded deletion is refused, the records
are checked again and a confirmation is offered. A check that finds records or cannot prove their absence also
offers confirmation (see "Moving the agent's own records to the Trash" below).

Deleting removes the session's record as described in "Deleting archived sessions" above. Its process is stopped,
and nothing is left in the archive. A trust confirmation or session request of the session's that is still waiting is
dropped with it (see "How a request ends" in `docs/product/session-requests.md`).

- The check is made at the moment of the delete: a session that has become used by then — a prompt submitted at the
  same moment wins — is refused as in "Deleting archived sessions" above, and keeps its record and its process. So is a
  session being launched, resumed or switched to another account at that moment.
- An unused session can stop existing while a session is being bound to it. Opening a session bound to an owner that
  was deleted meanwhile is refused, with nothing launched; binding or handing a session to an owner that was deleted
  meanwhile is refused, and changes nothing (see "Changing a session's owner" in
  `docs/product/hub-orchestration.md`).

## Moving the agent's own records to the Trash

For a deletion, Octoboard checks the agent's own files across the accounts described below for **every
session the deletion removes**, including archived descendants. The result is **present** when any record is found,
**absent** only when the whole scope is proved to have no records, and **unknown** when that cannot be established.
A missing or invalid agent session ID, an account location that cannot be determined, an unreadable location or a
symbolic link can leave the result unknown. Having no conversation does not itself prove that no files exist.

The confirmation — for one session or all archived sessions at once — hides its cleanup option when the result is
absent. Otherwise it offers an **unchecked by default** checkbox to also move the agent's own session records to the
Trash, including while the check is pending or its result is unknown. Its hint says that the
agent's transcript goes, wherever this machine keeps it, that the session can then no longer be resumed in the agent
itself, and that it can be recovered from the Trash; when archived sessions are deleted along, the hint covers their
transcripts too. Unticked, only Octoboard's own record goes. Either way the confirmation's message says that the
project's files are kept. While a deletion with the option ticked runs, the button says it is moving to the Trash,
and the confirmation stays open until the move has finished; while any deletion runs, the option cannot be changed.

- **The records are moved to the system Trash, never deleted outright**, after the sessions themselves are deleted.
  For each agent they are:

  | Agent | What is moved |
  |---|---|
  | Claude Code | the session's transcript, in every project directory it appears in, with the directory of its subagents' transcripts beside it |
  | Codex | the session's rollout files |
  | Grok Build | the session's directory |

- **They are looked for wherever this machine's accounts of that agent could hold them**: the account the session last
  ran under, every account of that agent known to Octoboard, and the agent's default account (see "Agent config
  directories" in `docs/product/consoles-and-projects.md`), since a session switched to another account leaves its
  original copy in the account it came from (see "Switching a session's account" in `docs/product/sessions.md`). The
  default account is skipped when where it lives cannot be read from the user's shell environment.
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
