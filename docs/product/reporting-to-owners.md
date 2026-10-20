# Reporting to an owner

A project session bound to an owner (see `docs/product/hub-orchestration.md`) reports a round of work with `report`:

| Argument | Required | Contents |
|---|---|---|
| `summary` | yes | What was done and what came of it, in natural language. |
| `status` | yes | One of `done`, `failed`, `needs_decision`. No other value is accepted. |
| `open_items` | no | Strings naming what is left unfinished. Empty when nothing is. |

The report is written into the session the reporting session is **bound to**, its **owner** — a console session
or a project session — named by its own binding rather than looked up, since a console may
hold several console sessions. It arrives as a user message naming the reporting session's id, its title and project,
the status, the open items, and then the summary. The reporting session is told that it was delivered, that it was
accepted and will reach its owner as soon as the owner can take a message, or that it was written into the owner's input
but the owner's agent did not send it, so the owner has not read it, the user has been told, and it must not be sent
again (see "A message written but not sent" in `docs/product/message-delivery.md`). The note calls the owner a console
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

## When a session does not report

**Reporting is not forced.** When a bound session ends a turn without having called `report`, Octoboard delivers
that turn's last assistant message to its owner as the report instead.
The message says plainly that the session stopped without reporting and that the status is
Octoboard's guess rather than its own word. The status is `needs_decision`, or `failed` when the turn
ended in an error, and the open-items list is empty. A session whose turn produced no message at all
is reported as such, with the suggestion that the owner check it with `get_session`.

A synthesised report never archives the session: only a session's own `done` report does that.

**No report is synthesised for a Claude Code turn that ends with work still running in the background** — the session
is then running in background (see "Running in background" in `docs/product/sessions.md`): its work is not over. The
turn in which Claude Code tells the session that work has finished is judged like any other: reported on, unless it
too ends with work still running.

**No report is synthesised for a lead session's turn while any session bound to it has a process** — running, or being
launched or resumed: it has stopped to wait for its sessions, not stopped working. With none of them running, its turn
is reported on as any bound session's is.

Grok Build reports no turn end at all for some turns, and the backstop Octoboard falls back on
there fires about a minute after the turn ends, so a synthesised report for one of those turns
arrives that much later. Sessions outside the orchestration are never reported on.

## Automatic archiving

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

**A report left unsent archives nothing.** A report that would have archived the session, written into the owner's
input but not sent by the owner's agent (see "A message written but not sent" in `docs/product/message-delivery.md`),
leaves the reporting session running and awaiting instructions, and its answer adds that the session was not archived
because its owner has not read the report. The report still counts as the session's own for that turn, so no report is
synthesised when the turn ends. Nothing archives the session later by itself, and it was told not to send the report
again: a later qualifying report or an explicit archive, by its owner or the user, must do so. A report that was only
queued is different: it archives the session on acceptance, as above, so if it is later written and left unsent, the
session is already archived, and only the notice the user gets tells anyone.

Anything else — open items, `failed`, `needs_decision` — leaves the session running and awaiting
instructions, for its owner to continue with `send_message` or to archive explicitly.

Automatic archiving archives the reporting session and nothing else. It never archives the owner the session is
bound to, and does not touch the other sessions bound to it: archiving an owner, and what that takes with it, is
covered in "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`.
