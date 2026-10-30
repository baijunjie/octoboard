# Reporting to a parent session

A project session with a parent reports a round of work with `report`, whether it is a coordinator or a worker
(see `docs/product/hub-orchestration.md`). The parent is also called the `owner` in the API:

| Argument | Required | Contents |
|---|---|---|
| `summary` | yes | What was done and what came of it, in natural language. |
| `status` | yes | One of `done`, `failed`, `needs_decision`. No other value is accepted. |
| `open_items` | no | Strings naming what is left unfinished. Empty when nothing is. |

The report is written into its **parent session** — a console session or project coordinator — named by the
reporting relationship rather than looked up, since a console may
hold several console sessions. It arrives as a user message naming the reporting session's id, its title and project,
the status, the open items, and then the summary. The reporting session is told that it was delivered, that it was
accepted and will reach its owner as soon as the owner can take a message, that it was kept for its owner, which is
interrupted, and is delivered once the owner is resumed, so it must not be sent again (see "Reports and notices for an
interrupted owner" below), or that it was written into the owner's input but the owner's agent did not send it to the
conversation, so the owner has not read it, the user has been told, and it must not be sent again (see "A message
written but not sent" and "A message a dialog discarded or a view took" in `docs/product/message-delivery.md`). The
note calls the owner a console session or a project session according to which it is.

Reporting is refused, or fails without accepting the report, when:

- the session is independent, so there is nobody to report to;
- its owner is no longer on record;
- its owner is archived. Nothing is kept for it: the reason names the owner and tells the session to tell the user in
  its terminal what the report says, since the owner will not read it. The binding names the owner by its id and
  outlives the archive, so once the owner is reopened the session's reports reach it again. A session archived along
  with its owner is told instead that there is nothing further to report;
- its owner still has no process running at the end of the wait in "Reports and notices for an interrupted owner"
  below — it is being started, resumed, switched to another account or handed off, or having its branches handed over
  or its owner changed. The reason names the owner and tells the session to report again once its owner is running.
  An owner that is simply interrupted does not fail a report: it has the report kept;
- a message queued for its owner could not be written in full, so it and everything queued behind it, the report
  included, were dropped (see "Messages held until a session can take them" in `docs/product/message-delivery.md`).
  The reason names the owner, says the user has been told, and tells the session to tell the user in its terminal what
  the report says rather than report again;
- the session has already been wrapped up — a session archived by its own `done` report cannot
  report a second time;
- the session is a coordinator with a parent, the report has `status: done` and no open items, and a session bound to it
  is not archived, unless a lock blocks automatic archiving (see "Automatic archiving" below). The reason names those
  sessions. Its other reports go through, and none of them archives it.

If a Codex owner's independent queue may have accepted the report but cannot confirm it, reporting returns an error
and the reporting session is not archived. The report still counts as that session having reported for the turn:
Octoboard does not synthesise a second report when it stops, since that could duplicate one already accepted. It does
not resend the original report automatically. The delivery hold and recovery rules are in
`docs/product/message-delivery.md`'s "A message whose acceptance is uncertain" section.

## When a session does not report

**Reporting is not forced.** When a reporting session ends a turn without having called `report`, Octoboard delivers
that turn's last assistant message to its owner as the report instead.
The message says plainly that the session stopped without reporting and that the status is
Octoboard's guess rather than its own word. The status is `needs_decision`, or `failed` when the turn
ended in an error, and the open-items list is empty. A session whose turn produced no message at all
is reported as such, with the suggestion that the owner check it with `get_session`.

A synthesised report never archives the session: only a session's own `done` report does that.

**No report is synthesised for a Claude Code turn that ends with work still running in the background** — the session is
then working in background (see "Working in background" in `docs/product/session-statuses.md`): its work is not over.
The turn in which Claude Code tells the session that work has finished is judged like any other: reported on, unless it
too ends with work still running.

**No report is synthesised for a coordinator's turn while any session bound to it has a process** — running, or being
launched or resumed: it has stopped to wait for its sessions, not stopped working. With none of them running, its turn
is reported on as any reporting session's is.

**A failed turn's report waits for the automatic account switch.** While **Automatically switch accounts at the usage
limit** is on, the report owed for a turn that ended in an error is held while that switch judges the turn (see "When a
session is switched" in `docs/product/automatic-account-switch.md`). When the switch moves the session to another
account, or resumes it on its own account because its quota has recovered, no report is sent: the owner gets the
switch's message instead, which says whether the session was sent a prompt to continue. Otherwise — the turn did not
stop on the usage limit, the session is left alone, it is held for a reset due within 5 minutes, no account qualifies,
or the switch fails, even when the session is brought back up on the account it had — the report is sent once the switch
has decided. Since the switch can read the session's record again for up to 4 seconds and wait up to 15 seconds for
another operation's hold on the session to end, a failed turn's report can arrive that much later while the setting is
on, whatever the error was. With the setting off, nothing waits.

**No report is synthesised for a turn that was cancelled** — by the user in the terminal, or by its owner's
`interrupt_session` (see `docs/product/interrupting-turns.md`): the session is back at its prompt, awaiting
instructions. How a cancel that fires no hook event is recognised is in `docs/product/turns-hooks-do-not-report.md`.

Grok Build reports no turn end at all for some turns, and the backstop Octoboard falls back on
there fires about a minute after the turn ends, so a synthesised report for one of those turns
arrives that much later. Independent sessions are never reported on.

## Notices to an owner

Besides reports, Octoboard itself writes a few notices into an owner about a session bound to it, where no report would
say what happened. Each is written through the delivery every message takes, queued while the owner cannot take one
(see "Messages held until a session can take them" in `docs/product/message-delivery.md`), and kept for an owner that
is interrupted (see "Reports and notices for an interrupted owner" below). An archived owner, or one with no process
running for another reason once the wait there is over, is not told, then or later.

- **A session ended on its own.** When the process of a reporting session ends without anyone having asked for it — it
  crashed, or the user closed it in its terminal — its owner is told that the session ended on its own although nobody
  archived it, interrupted it or moved it to another account, that it is recorded as interrupted and the work it was
  doing stopped with it, and to look at it with `get_session` and relaunch it with `reopen_session` if the work should
  go on. An end somebody asked for is not told: an archive, the daemon shutting down, or an operation that stops the
  process itself, such as an account switch or a change of sharing. A process that ends by itself while such an
  operation holds the session is told all the same, unless that operation started it and reports its outcome (see
  "When the switch fails" in `docs/product/automatic-account-switch.md`).
- **A session is waiting for the user.** When a reporting session comes to wait for the user — at a permission prompt, a
  question, a trust confirmation, a dialog of its agent, Codex's action title or a request of its own, by whichever
  route it is seen — its owner is told that the session is waiting for the user and only they can clear it, not to
  chase it or send it anything to get it going, a message to it being held until they are done, and to carry on with
  the rest of its work. It is told once per turn, however often the session goes in and out of waiting in that turn,
  whether or not the session has reported in it, and once for a wait before the session's first turn; the next turn
  tells it again. Nothing is said when the session is no longer shown as waiting by the time the notice would be
  written. No report is synthesised for a turn while the session waits, so this notice is how the owner learns of it.
  Grok Build's usage-limit screen is the exception: the session stopped on its limit with nothing open for the user to
  answer, so its owner is not told, though the session still shows as *waiting for the user* (see "What the statuses
  are derived from" in `docs/product/session-statuses.md`).
- **Octoboard switched a session's account by itself**, or tried to and could not bring it back up: see "When the
  switch succeeds" and "When the switch fails" in `docs/product/automatic-account-switch.md`.

## Reports and notices for an interrupted owner

An owner that is **interrupted** — its process ended without it being archived — has what comes for it about its
sessions **kept** for its resume, rather than refused or dropped:

- a session's own report, and the first report of a session binding itself to it (see "Setting a parent for an
  independent session" in `docs/product/changing-session-owners.md`);
- a synthesised report (see "When a session does not report" above);
- every notice in "Notices to an owner" above;
- the notice of a handoff of one of its sessions (see "Who is told" in `docs/product/handing-off-sessions.md`) and the
  notice to the new owner of a session handed to it with `transfer_session` (see "Handing a session to another owner"
  in `docs/product/changing-session-owners.md`).

The notice that the user released a session from its owner is the exception: an owner with no process running is not
told it, then or later (see "Releasing a session from its owner" in `docs/product/changing-session-owners.md`).

What is kept is held in the same store, and under the same rules, as information shared with an interrupted session:
it survives a restart of Octoboard, is written into the owner once its process is next started, in the order it came
among everything kept for it, shared information included, is dropped when the owner is archived or deleted, is passed
on when the owner is handed off, and is counted in the owner's `queued_messages` (see "Information for an interrupted
session" in `docs/product/information-between-sessions.md`).

- **Each kept report and notice is dated.** It carries a line saying it was kept for the owner while it was
  interrupted, the minute it dates from (in UTC), that what it says of the session may no longer hold, and to check the
  session with `get_session` before acting on it. In a report the line comes among the lines that open it, before the
  open items and the summary.
- **A kept wait gives way to what came after it.** A kept notice that a session is waiting for the user is dropped once
  any later report or notice about the same session is kept for that owner, another wait included; information shared
  with the owner does not drop it. At the resume, a kept wait is also dropped when that session no longer reads as
  waiting for the user.
- **A `done` report still archives the reporter.** A report kept this way counts as accepted (see "Automatic archiving"
  below): a `done` report with no open items, with no lock blocking it, archives the reporting session right away, and
  its answer says the report was kept and that the session is now archived. Any other report leaves the reporting
  session as it is.
- **An owner in the middle of an operation is waited for.** An owner with no process running that is being started,
  resumed, switched to another account or handed off, or having its branches handed over or its owner changed, can
  neither take a message nor have one kept, so a report or notice for it waits up to **15 seconds** for that to finish
  and then goes by how the owner stands: written into it once it is running, kept while it is interrupted. One still
  without a process after the wait is refused, for a report (see the refusals at the top of this document), and not
  told, for a notice.
- **A report, or a notice in "Notices to an owner" above, that waited for an owner which a handoff archived
  meanwhile** goes to the session that took the owner's place (see `docs/product/handing-off-sessions.md`).

An owner is told this in its role description — a console session in its instruction file, a project coordinator in its
own text: reports and Octoboard's notices about its sessions that come while it is interrupted are kept and arrive once
it is resumed, each saying when it dates from, so it checks the session with `get_session` before acting on one, and the
notice that the user released a session from it is not kept.

## Automatic archiving

Locks follow "Locking a session" in `docs/product/sessions.md`. If the reporting session or any session under it
is locked, its report is delivered normally, including `done` with no open items, but it is not archived. The
unfinished-session refusal for a coordinator's completion report does not apply in that case. Unlocking does not
retroactively archive it; a later qualifying report or an explicit archive must do so.

Locks can be changed while a `done` report is on its way to its owner, and what decided the archive is checked again
once the report has been accepted. If by then the reporting session or a session under it has been locked, or a
session that is not archived has been bound to it, the report stays delivered but the session is not archived, and the
answer adds that it was not archived because one of those changed while the report was delivered. A session someone
else archived meanwhile is answered as archived.

A report with `status: done` and **no** open items, with no lock blocking it, wraps the session up: once the report
has been accepted for its owner — delivered, queued, or kept for an interrupted owner — the session's process is ended
and the session is archived. The git branches it was working on, and the linked worktrees that have them checked out,
are then removed where git can do so without force, and the report's answer names those that will stay (see "Archives
made by an agent" in `docs/product/branch-and-worktree-cleanup.md`).
"Accepted" rather than "read" — an owner that is merely busy has the report queued for it,
and the session is archived anyway rather than being left alive until the owner gets round
to it.

**A report left unsent archives nothing.** A report that would have archived the session, written into the owner's
input but not sent to its conversation by the owner's agent — left in the input box or taken by a view of its background
agents (see "A message written but not sent" in `docs/product/message-delivery.md`) —
leaves the reporting session running and awaiting instructions, and its answer adds that the session was not archived
because its owner has not read the report. The report still counts as the session's own for that turn, so no report is
synthesised when the turn ends. Nothing archives the session later by itself, and it was told not to send the report
again: a later qualifying report or an explicit archive, by its owner or the user, must do so. A report that was only
queued is different: it archives the session on acceptance, as above, so if it is later written and left unsent, the
session is already archived, and only the notice the user gets tells anyone.

The one-time automatic submit-key retry in `docs/product/message-delivery.md`'s "A message written but not sent"
section does not archive a reporting session later by itself either. A report whose independent-queue acceptance is
uncertain likewise archives nothing, as described above.

Anything else — open items, `failed`, `needs_decision` — leaves the session running and awaiting
instructions, for its owner to continue with `instruct_session` or to archive explicitly.

Automatic archiving archives the reporting session and nothing else. It never archives the owner the session is
bound to, and does not touch the other sessions bound to it: archiving an owner, and what that takes with it, is
covered in "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`.
