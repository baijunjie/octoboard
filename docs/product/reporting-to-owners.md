# Reporting to a parent session

A project session with a parent reports a round of work with `report`, whether it is a coordinator or a worker
(see `docs/product/hub-orchestration.md`). The parent is also called the `owner` in the API:

| Argument | Required | Contents |
|---|---|---|
| `summary` | yes | What was done and what came of it, in natural language. |
| `status` | yes | One of `done`, `failed`, `needs_decision`. No other value is accepted. |
| `open_items` | no | Strings naming what the reporter left unfinished. Empty when nothing is. |
| `owner_items` | no | Strings naming items handed to the owner to carry out — local testing it runs, a merge it does, a decision only it can act on. Not open items: they never keep a `done` report from archiving the reporter (see "Automatic archiving" below). |

`report`'s name and call shape stay as they are (see "The MCP server as a contract" in `apps/daemon/PROTOCOL.md`):
`owner_items` is optional, and a call without it behaves exactly as one with an empty list. It is read the same whatever
the status. A reporting session is told in its role description, and in what tells it of a new parent, to put items that
are its owner's to carry out in `owner_items` rather than `open_items`.

The report is written into its **parent session** — a console session or project coordinator — named by the reporting
relationship rather than looked up, since a console may hold several console sessions. It arrives as a user message
(see "The report as the owner reads it" below). The reporting session is told that it was delivered; that it is queued
and will reach its owner as soon as the owner can take a message; that it was kept for its owner, which is interrupted,
and is delivered once the owner is resumed (see "Reports and notices for an interrupted owner" below); or that it was
written into the owner's input but the owner's agent did not send it to the conversation, so the owner has not read it
and the user has been told to check the owner (see "A message written but not sent" and "A message a dialog discarded or
a view took" in `docs/product/message-delivery.md`). Every outcome but the first tells it not to send the report again.
The note names the owner by its id, calling it a console session or a project session according to which it is.

Reporting is refused, or fails without accepting the report, when:

- the session is independent, so there is nobody to report to;
- its owner is no longer on record;
- its owner is archived. Nothing is kept for it: the reason names the owner and tells the session to tell the user in
  its terminal what the report says, since the owner will not read it. The binding names the owner by its id and
  outlives the archive, so once the owner is reopened the session's reports reach it again. A session archived along
  with its owner is told instead that there is nothing further to report;
- its owner still has no process running at the end of the wait in "Reports and notices for an interrupted owner"
  below — it is being started, resumed, switched to another account or handed off, or having its branches transferred
  or its owner changed. The reason names the owner and tells the session to report again once its owner is running.
  An owner that is simply interrupted does not fail a report: it has the report kept;
- a message queued for its owner could not be written in full, so it and everything queued behind it, the report
  included, were dropped (see "Messages held until a session can take them" in `docs/product/message-delivery.md`).
  The reason names the owner, says the user has been told, and tells the session to tell the user in its terminal what
  the report says rather than report again. When the report itself was written only in part while the owner's draft
  was stashed, only the report was dropped, and the reason says so, adding that the owner's input box may hold part of
  it (see "A message only partly written while the draft is stashed" in `docs/product/message-delivery.md`);
- the session has already been wrapped up — a session archived by its own `done` report cannot
  report a second time;
- the session is a coordinator with a parent, the report has `status: done` and no open items (whatever owner items it
  has), and a session bound to it is not archived, unless a lock blocks automatic archiving (see "Automatic archiving"
  below). The reason names those sessions. Its other reports go through, and none of them archives it.

## The report as the owner reads it

A report reaches its owner as:

```text
Report from your child session, the project session `<id>` ("<title>") in the project "<project>"
Status: <status>
Open items:
- <open item>
For you to do:
- <owner item>

> <summary, line by line>
— end of report from session `<id>` —
```

- **The first two lines** name the reporting session — by kind, id and title, and the project it runs in, left out when
  the project's record cannot be read or the project's name is the session's title — and give the status, `done`, `failed` or `needs_decision`, on its own line.
- **Octoboard's own lines come next, unquoted**, each on its own line under the status: the line saying a report was
  synthesised (see "When a session does not report" below); the sentence saying what a session that bound itself to the
  owner now is to it, on its first report (see "Setting a parent for an independent session" in
  `docs/product/changing-session-owners.md`); and the line saying the report was kept for an interrupted owner (see
  "Reports and notices for an interrupted owner" below).
- **The open items**, then **the owner items** under a heading saying they are the owner's to do, one per line, each
  item put on one line. Each list is left out when it is empty.
- **The summary** follows after a blank line, every line of it quoted with `> `, blank ones included, and is closed by
  a line naming the session by its id. Neither the summary nor an item can pass for a line of Octoboard's own (see "The
  headers" in `docs/product/message-headers.md`). A session whose turn ended without saying anything has no summary
  to quote: in its place, Octoboard's own line says it said nothing and to check it with `get_session`, with no
  closing line, and the synthesised line then says only that the status is Octoboard's guess.

## When a session does not report

**Reporting is not forced.** When a reporting session ends a turn without having called `report`, Octoboard delivers
that turn's last assistant message to its owner as the report instead. The message says plainly that the session stopped
without reporting and that the status is Octoboard's guess rather than its own word. The status is `needs_decision`, or
`failed` when the turn ended in an error, and it has neither open items nor owner items. A session whose turn produced
no message at all is reported as such, with the suggestion that the owner check it with `get_session`.

A synthesised report never archives the session: only a session's own `done` report does that. Its owner is reminded
of the session once if it stays idle after that report (see "Notices to an owner" below).

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

**A Codex turn stopped at its model's capacity is retried before it is reported.** Its failed report is held while
Octoboard retries the turn in the same session, up to 3 times, and the owner is told of each retry instead of being
given the report; the automatic account switch does not judge a turn being retried. The report is sent once the retries
are used up or a retry does not go ahead, and says which. See "When the selected model is at capacity" in
`docs/product/turns-hooks-do-not-report.md`.

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
  doing stopped with it, and to look at it with `get_session` and relaunch it with `resume_session` if the work should
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
  are derived from" in `docs/product/session-statuses.md`). Nor is it told of a session that is *dormant*, showing a
  view of its background agents, which is no wait for the user (see "Dormant" in `docs/product/session-statuses.md`).
- **Octoboard switched a session's account by itself**, or tried to and could not bring it back up: see "When the
  switch succeeds" and "When the switch fails" in `docs/product/automatic-account-switch.md`.
- **Octoboard will retry a Codex turn stopped at its model's capacity**, in place of that turn's failed report: see
  "When the selected model is at capacity" in `docs/product/turns-hooks-do-not-report.md`.
- **A session stayed idle after its last report.** When a reporting session has been idle after its last report for **10
  minutes** without a break — `idle_after_report` true in its session record the whole time (see "Session records" in
  `docs/product/what-every-session-reads.md`) — its owner is told once for that report. Idleness is looked at every 30
  seconds, so the notice can come up to about 30 seconds after the 10 minutes are up, and any look that finds the
  session no longer idle after its report starts the 10 minutes again. The notice names the session, says for how long
  it has been idle since its last report reached the owner with nothing running, says whether that report was its own,
  which did not archive it, or one Octoboard synthesised because it stopped without reporting, and tells the owner to
  check it with `get_session` and archive it with `archive_session` if its work is over, otherwise to give it its next
  instruction with `instruct_session`, adding that it is not reminded again until the session reports again. It covers a
  session that ended a round without reporting as well as one whose own report left it running — with open items, a
  status other than `done`, a lock, or a change while the report was delivered. Nothing is said when the session is no
  longer idle after that report by the time the notice would be written. A session whose last report is followed by a
  new turn of its own, a later report, the end of its process or an archive is not told about for that report. The
  notice never archives anything: archiving stays with the owner or the user.

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
  open items, the owner items and the summary (see "The report as the owner reads it" above).
- **A kept wait gives way to what came after it.** A kept notice that a session is waiting for the user, or that it
  stayed idle after its last report, is dropped once any later report or notice about the same session is kept for
  that owner, another of the same kind included; information shared with the owner does not drop it. At the resume, a
  kept wait is also dropped when that session no longer reads as waiting for the user, and a kept notice that it stayed
  idle when it is no longer idle after its report.
- **A `done` report still archives the reporter.** A report kept this way counts as accepted (see "Automatic archiving"
  below): a `done` report with no open items, with no lock blocking it, archives the reporting session right away, and
  its answer says the report was kept and that the session is now archived. Any other report leaves the reporting
  session as it is.
- **An owner in the middle of an operation is waited for.** An owner with no process running that is being started,
  resumed, switched to another account or handed off, or having its branches transferred or its owner changed, can
  neither take a message nor have one kept, so a report or notice for it waits up to **15 seconds** for that to finish
  and then goes by how the owner stands: written into it once it is running, kept while it is interrupted. One still
  without a process after the wait is refused, for a report (see the refusals at the top of this document), and not
  told, for a notice.
- **A report, or a notice in "Notices to an owner" above, that waited for an owner which a handoff archived
  meanwhile** goes to the session that took the owner's place (see `docs/product/handing-off-sessions.md`).

An owner is told this in its role description — a console session in its instruction file, a project coordinator in its
own text: reports and Octoboard's notices about its sessions that come while it is interrupted are kept and arrive once
it is resumed, each saying when it dates from, so it checks the session with `get_session` before acting on one, and the
notice that the user released a session from it is not kept. The same text tells it that the items a report lists as
for it to do are its own to carry out, and that one of its sessions left idle after its last report shows
`idle_after_report: true` and is told of once when that has lasted 10 minutes, to archive it if its work is over or
give it its next instruction.

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

A report with `status: done` and **no** open items, with no lock blocking it, wraps the session up — owner items do not
count as open items, so a `done` report whose only items are owner items wraps it up as well: once the report
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
leaves the reporting session running at its prompt, and its answer adds that the session was not archived
because its owner has not read the report. The report still counts as the session's own for that turn, so no report is
synthesised when the turn ends. Nothing archives the session later by itself, and it was told not to send the report
again: a later qualifying report or an explicit archive, by its owner or the user, must do so. A report that was only
queued is different: it archives the session on acceptance, as above, so if it is later written and left unsent, the
session is already archived, and only the notice the user gets tells anyone.

The one-time automatic submit-key retry in `docs/product/message-delivery.md`'s "A message written but not sent"
section does not archive a reporting session later by itself either.

Anything else — open items, `failed`, `needs_decision` — leaves the session running and awaiting
instructions, for its owner to continue with `instruct_session` or to archive explicitly.

Automatic archiving archives the reporting session and nothing else. It never archives the owner the session is
bound to, and does not touch the other sessions bound to it: archiving an owner, and what that takes with it, is
covered in "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`.
