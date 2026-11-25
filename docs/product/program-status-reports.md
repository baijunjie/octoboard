# Program status reports

How the program running in a session's terminal can report its own status through OSC 7501, the Program Status
Protocol, revision 0.4 (<https://www.superlogical.com/rex/docs/build/program-status>), how such reports tell whether a
turn is running, and when they become the session's status. What the statuses mean, and where they come from for the
other agents, is in `docs/product/session-statuses.md`.

## Where the protocol is supported and where reports give the status

- **Every session's terminal supports the protocol, whichever its agent**: it keeps the reports as records (see
  "Reports and records" below), takes them out of what the clients receive, and answers the support query (see "The
  support query" below). Supporting the protocol says that the terminal accepts reports, not that they become the
  session's status.
- **Reports give a session its status only when its agent has no status integration of its own**, no hooks or
  signals that supply the full status. Antigravity CLI and Pi use reports as the source of status. Claude Code, Codex, Grok Build, GitHub Copilot CLI, OpenCode and Qwen Code have status integrations. Their reports still describe terminal activity. Qwen Code also uses reports for waits and cancellations alongside its hooks.
- **One launch of an agent that has a status integration can be without it**, where the agent lets the user switch
  it off and Octoboard does not override that. The agent's provider says so for the launch, and that session then
  takes its whole status from reports, turns opened and closed included, as a session of an agent with no integration
  does, for as long as its process lives, across a daemon restart too. Qwen Code does this when its hooks are disabled in its configuration. The launch shows a notice.
- **A shell terminal of the Terminal panel** (see `docs/product/terminal-panel.md`) does not support the protocol:
  reports reach the clients as ordinary output, and so does the support query, which no client answers.

"From the root record to the session's status" and "Reports at the end of the process" below apply only to a session
whose status reports give; "What reports can say beside an agent's hooks" only to a session whose agent's hooks give its
status and declare it; "Reports as the sign that a turn is running" applies to every session.

## The support query

- **A program asks whether the protocol is supported with `OSC 7501 ; ? ST`**, where `ST` is `ESC \` or `BEL`.
- **Octoboard answers it in every session's terminal, at once, whether or not any client is attached, on every host** —
  on Windows also in a terminal running on the system's own console host — and takes it out of the output, as it does
  the queries it answers itself (see "Queries Octoboard answers itself" in `docs/product/terminal-queries.md`). The
  exception is a query the program asks while no daemon is connected to the process holder its terminal runs in: the
  holder answers only the device attributes, so the support query goes unanswered (see "Terminal queries while no
  daemon is connected" in `docs/product/process-holders.md`).
- **The answer** is `OSC 7501 ; ? : states=idle,working,done,blocked,error : kinds=permission,question,auth ST`, written
  without spaces and ended with the same terminator the query used: every state and kind of revision 0.4 is supported.

## Reports and records

- **A report is `OSC 7501 ; key=value:key=value… ST`**, where `ST` is `ESC \` or `BEL`. Reports are taken out of the
  output, so no client sees them.
- **The fields**:

  | Key | Value |
  |---|---|
  | `state` | Required: `idle`, `working`, `done`, `blocked` or `error`, or `clear` (see below). |
  | `id` | The record the report addresses: 1 to 8 `/`-separated segments, each 1 to 32 bytes of `A-Z a-z 0-9 _ . + -`, at most 128 bytes in all. Without it, the report addresses the root record. |
  | `kind` | What a `blocked` program waits on: `permission`, `question` or `auth`. |
  | `progress` | 0 to 100, with `working` or `blocked` only; absent means indeterminate. |
  | `app` | A stable identifier of the program, at most 32 bytes. |
  | `title` | Base64 UTF-8 text, at most 192 bytes decoded (256 encoded). |
  | `msg` | Base64 UTF-8 text, at most 2048 bytes decoded (2732 encoded). |

- **A report is discarded whole** when it names no state or an unknown one (states are lower case), when its `id`
  breaks the rules above, when `app`, `title` or `msg` is too long, when `title` or `msg` is not valid base64 (padding
  may be left out, but padding given must be right) or decodes to text that is not UTF-8 or holds a control character,
  or when a key of lower-case letters is longer than 16 bytes.
- **A sequence longer than 4096 bytes**, counted from its `ESC` to the end of its `ST`, **or holding a control
  character other than a tab**, is not read as a report: it reaches the clients as ordinary output.
- **What is skipped rather than discarded**: a malformed pair — no `=`, an empty key, a key that is not all lower-case
  ASCII letters, or a value with a byte outside `A-Z a-z 0-9 _ . , + / = -` — is skipped and the rest of the report is
  read. Spaces and tabs around a key or value are ignored, an unknown key is ignored, and of a key given twice the last
  value counts. A `kind` or `progress` that is unknown, out of range or given with a state it does not apply to is
  dropped from the record, and the report is otherwise kept.
- **Records are kept by id.** Each report replaces its record whole, keeping nothing of what the record said before.
  `clear` with an `id` removes that record and every record under it (`a` removes `a` and `a/b`, not `ab`); `clear`
  without an `id` removes every record.
- **At most 256 records** are kept per terminal: a report for a new id when 256 are kept removes the least recently
  updated record to make room, the root record included.
- **Only the root record's state bears on the session**, and the `msg` of the root's `done` or `error` report, which
  is what the turn it ends said unless the agent's provider gives that message itself (see "From the root record to
  the session's status" below). The other records, and the `kind`, `progress`,
  `app` and `title` of any record, are kept but shown nowhere.

## How long records last

- **A shell prompt starting forgets every `working` and `blocked` record**, whatever its id, and keeps the `done`,
  `error` and `idle` ones: a prompt start is `OSC 133 ; A ST`, with or without options (`OSC 133 ; A ; … ST`), and the
  prompt itself still reaches the clients. A prompt start longer than 128 bytes, not counting its `ST`, is not
  recognised and forgets nothing.
- **The end of the session's process forgets every `working` and `blocked` record** too, keeping the others; this moves
  no status, since the end of the process settles the status itself (see "Reports at the end of the process" below).
- **A terminal reset (RIS, `ESC c`) forgets every record. A soft reset (DECSTR, `CSI ! p`) forgets none.**

## Reports as the sign that a turn is running

An agent's terminal title has shown whether one of its turns is running, by the activity glyph it starts the title
with. An agent that reports through OSC 7501 once the support query is answered can drop those glyphs (Claude Code
2.1.322 does), so in every session, whichever its agent and whether or not reports give its status, the root record's
reports are read as that sign too:

- **`working` is a turn running. Every other state — `idle`, `done`, `error`, `blocked` — and the root record going
  away** (cleared, removed to make room, or forgotten at a shell prompt or a reset) **is the program back at its
  prompt.** `blocked` is a turn waiting on the user, which the title showed as at the prompt too;
  where hooks give the status, they move the session to *waiting for the user* then, or the report itself does where
  the agent's provider declares it (see "What reports can say beside an agent's hooks" below).
- **The first report of the root record takes over from the title for as long as the session's process runs**: from
  then on the title's activity glyphs are ignored, whatever they show.
- **A reported `working` stays current until the next report.** A program reports only when its state changes, so the
  rule that a running title counts only while the agent keeps sending it (see "When a key is sent" in
  `docs/product/interrupting-turns.md`) applies to titles alone.
- **Before any report**, the title alone says whether a turn runs, as for an agent that never reports.
- **After a daemon restart that adopts the session's terminal** (see "What a daemon start takes over" in
  `docs/product/process-holders.md`), the root record the terminal was rebuilt with — from the saved screen of a
  planned restart, or from the holder's tail of the output — is taken as the program's last report, since the program
  does not report it again. A tail that starts after the program's last report rebuilds no root record; the turn then
  reads as not running until the program reports again, unless the title shows it running.

This reading, never a status, is what the following go by: whether `interrupt_session` may send a key, its watch for
the prompt the agent puts back in its input box, and when a prompt after the key counts as the turn's end (see "When a
key is sent", "The input box afterwards" and "After the keys" in `docs/product/interrupting-turns.md`); and the watch
that moves a Claude Code session to its prompt after a cancel no hook reports (see "Cancelling a Claude Code turn" in
`docs/product/turns-hooks-do-not-report.md`).

This applies to every agent that reports, not only to Claude Code. Codex on Windows has been seen reporting `idle`,
`working` and `done` while keeping the spinner in its title, so from its first report its turn is read off the reports
too; whether it reports leaving `working` when an Esc cancels its turn is not measured. Its cancel still reaches the
session through its `Interrupt` hook, and a key goes in only while the hooks read *working* as well.

## From the root record to the session's status

- **The root record's state is the session's status**:

  | Root record | Session status |
  |---|---|
  | `working` | *working* |
  | `blocked` | *waiting for the user* (for the kind `auth`, once it has stood for 3 seconds; see below) |
  | `idle` from `working` or `blocked` while a turn is open: the turn was cancelled (see below) | at its prompt (`idle`), paused |
  | `done` from `working` or `blocked` while a turn is open: the turn ended normally | at its prompt (`idle`), not paused |
  | any other `idle`, or `done`: no turn ends (see below) | at its prompt (`idle`), paused or not as before |
  | `error` | at its prompt (`idle`), paused |
  | none, gone while it read `working` or `blocked` with a turn open: the turn ended normally (see below) | at its prompt (`idle`), not paused |
  | none otherwise — never reported, cleared, removed to make room, or forgotten by a prompt start or a reset | at its prompt (`idle`), paused or not as before |

  The pause records how the last turn ended, so only a turn's end sets or clears it, `error` setting it whether or not
  a turn was open, and a new turn starting clears it (see "Paused" in `docs/product/session-statuses.md`). A move to
  the prompt that ends no turn leaves it as it was.

  At its prompt, the session shows the status "Which status a session shows" in `docs/product/session-statuses.md`
  gives it, so where not paused, `done` and `idle` both read as *done* once the session has a conversation and as
  *awaiting instructions* before. *Dormant* and *working in background* never come from reports.
- **A `blocked` of kind `auth` counts only once it has stood for 3 seconds.** A program may report it while it checks
  a sign-in that is there, as it starts and as it resumes, and go on to `idle` without the user having had anything to
  do (a program may do so at every launch, for a moment). Where the next report replaces it within the
  3 seconds it is passed over whole: the session is never shown as *waiting for the user* and no owner is told. A
  `blocked` of another kind that replaces it is such a report, and is shown and told at once. One that stays, a sign-in the user really has to make, is shown and told to an owner 3
  seconds after it was reported. A `blocked` of kind `permission` or `question`, or one that names no kind, counts
  from the moment it is reported.
- **An `idle` reported before the turn of a launch's own prompt has started counts only once it has stood for 5
  seconds.** A session launched with a prompt on the agent's command line reads *working* from its launch, and a
  program may report `idle` once it is up and `working` only when it takes the prompt, a moment later. Where the next
  report replaces that `idle` within the 5 seconds it is passed over whole: the session is never shown at its prompt
  and the messages queued for it are not released into a turn about to start. One that stays is an agent that did not
  take the prompt, and the session is then shown at its prompt. This holds until the reports show the launch's turn
  start or such an `idle` has stood, and only for the daemon that launched the session; an `idle` that ends a turn is
  never held. The turn activity read from the reports (see "Reports as the sign that a turn is
  running" above) follows every report at once, this one included.
- **A `done` straight after `working` may be held as a gap inside the turn, where the agent's provider says so.** An
  agent may report `done` for a moment between two model calls of one turn and `working` again at the next call.
  Where its provider declares that, and how long such a gap lasts at most, such a `done` with a turn open is held for
  that long: a `working` within it passes the `done` over whole, so the turn goes on, nothing is reported, the session
  is never shown at its prompt and no queued message is released into the running turn. Any other report, the bound
  running out, the end of the process or the daemon beginning to stop have the `done` count, ending the turn as it
  says; the turn's end then reaches the board and an owner up to that bound later. Without the declaration a `done`
  ends the turn at once. A known limit: a `working` that starts a new turn within the bound cannot be told from the
  next model call, so the first turn's end is never reported and the owner hears of one turn end for two turns. Qwen
  Code runs a message queued with Ctrl+Q as a turn of its own right after the turn ends, and that turn's `working`
  comes in the same read as the first turn's `done` (measured on Qwen Code 0.28.3), so the two turns read as one.
  A daemon that begins to stop applies a held `done` at once, also in a
  planned stop where the agent lives on: a stop inside a real gap reports a turn end the agent did not reach.
- **A turn starts when the root record goes to `working` with no turn open**, and the session then has a
  conversation. A move between `working` and `blocked` stays within the turn: a blocked program is held up within its
  turn, as an agent asking for a permission is. A `blocked` reported with no turn open is a wait outside one, a screen
  the program shows as it starts (a folder to trust, a sign-in): the session is shown as *waiting for the user* and an
  owner is told, but no turn starts, so going back to `idle` from it ends none and nothing is reported; going on to
  `working` starts the turn there. A known limit: a `blocked` with no turn open that is followed by `done` or `error`
  with no `working` between ends no turn either, so that report's `msg` is dropped and a failure is not reported to an
  owner, though `error` still leaves the session paused.
- **An `idle` with no turn open opens and closes nothing**: the one a program reports as it starts, before its first
  turn, or one after a `blocked` reported with no turn open. It tells nobody anything and puts the session at its
  prompt with the pause as it was: a session whose last turn ended abnormally stays paused when its program starts
  again. A `blocked` that was shown as *waiting for the user* has already cleared the pause, as any move off the prompt
  does.
- **A turn ends when the root record leaves both `working` and `blocked`**, in one of three ways:
  - **On `done` or `error`**, the turn ended normally, or failed for `error`. For a session bound to an owner, it is
    reported as a turn that ended without the session reporting (see "When a session does not report" in
    `docs/product/reporting-to-owners.md`). What it said is the message the agent's provider gave for it, where it gives
    one (see the next bullet), and otherwise the `msg` of the report, where it carries one that is not blank: the reply
    for `done`, the failure's text for `error`. A turn with neither is reported as one that produced no message.
  - **On `idle`**, with no `done` or `error`, the turn was cancelled: a program reports `idle` when its turn is
    cancelled. It is closed with no report to an owner, as a cancel an agent's hook reports is (see "When a session
    does not report" in `docs/product/reporting-to-owners.md`), takes no message, and leaves the session at its prompt,
    paused (see "Paused" in `docs/product/session-statuses.md`).
  - **With the root record gone** while it read `working` or `blocked` — cleared, removed to make room, or forgotten at
    a shell prompt or a reset (see "How long records last" above) — the turn ended normally, and is reported as one
    that produced no message.
- **An agent's provider can give a turn's message from a source of its own**, such as a hook payload or the agent's
  transcript, since a program may cut the `msg` it reports short, or report none. That message replaces the `msg` of
  the `done` or `error` report that ends the turn; the turn is still closed once, by the report. When the message has
  not arrived by the time that report is applied, the turn end waits up to **2 seconds** for it, the session reading
  *working* meanwhile, and after that the report's own `msg` stands. The wait ends early once the session's process has
  ended or the daemon is stopping, and at once when the provider says the turn had no message. A blank message counts
  as none. A message that arrives after its turn was closed is dropped and never given to a later turn, and a cancelled
  turn takes none. A message the provider has to read from a file is read without holding up the session's hook
  events. A provider that gives no messages has its turn ends wait for nothing; no agent supported today gives them.
- **A provider can say that its program's `msg` is not the turn's message** (a program that reports the session's
  name there, say). A turn of such an agent that its provider gave no message for, in time or at all, is then reported
  as one that produced no message, whatever the `msg` of its `done` or `error` report holds. No agent supported today
  says so.
- **After a daemon restart that adopts the session's terminal**, the session keeps the status it had before the
  restart; the root record the terminal was rebuilt with (see "Reports as the sign that a turn is running" above) is
  not applied again, but is taken as the state the next report moves from, and a record of `working`, or of `blocked`
  on a `permission` or a `question`, reopens the turn it was in, since the daemon that had it open kept it only in
  memory. So a `done` that ends a turn running across the restart closes and reports that turn, as it would have
  without the restart, and a `blocked` in between opens no other. A `blocked` that names no kind reopens no turn, as
  one reported with no turn open starts none. A `blocked` of kind `auth` is taken as just reported: it has to stand for
  its 3 seconds again before the session is shown as *waiting for the user*, and is passed over if the next report
  replaces it, so a daemon that stopped while a launch's passing sign-in check was up leaves no wait behind. A turn
  that was running when the program reported that `auth` is not reopened either: if the program then goes straight to
  `done` or `error` without reporting `working` again, that report ends no turn.
- **A `blocked` of kind `auth` still within its 3 seconds does not hold up what waits for the reports to be applied**:
  the end of the process, a report's delivery, and a daemon's planned stop, which drops it for the next daemon to take
  up as above. Within those 3 seconds the session does not yet read as *waiting for the user*, so a message
  queued for it then can be written while the program shows its sign-in; at a launch the hold on a starting session's
  messages covers this, and for a sign-in that comes up mid-session nothing does.
- **Going to *waiting for the user* tells an owner** that the session waits for the user, as any route to that status
  does (see "Notices to an owner" in `docs/product/reporting-to-owners.md`), and messages held for the session are
  released once a report brings it to its prompt, as they are on a hook's report.
- **A session request holds the session at *waiting for the user*** for the whole wait whatever the reports say, as it
  does whatever an agent's hooks say (see "Waiting for the answer" in `docs/product/session-requests.md`).

## Reports at the end of the process

- **Reports read before the session's process ended are applied before its end is recorded**, waited on for up to 2
  seconds after the agent's hook events are (see "Paused" in `docs/product/session-statuses.md`). So a final `done`
  decides that the turn ended normally, a final `error` that it failed, and a final `idle` that it was cancelled,
  rather than the turn reading as cut short by the end of the process. The status after the end is still
  *interrupted*, or *archived* where the session was archived.
- **When the application quits**, reports already read are applied before the sessions are marked interrupted, within
  the half second the agents' hook events are waited on across all sessions.
- **Once the end is recorded, reports change nothing**: one applied after it, still on its way or not waited for,
  neither moves the status nor opens a turn.
- **A session's own `report` call** (see `docs/product/reporting-to-owners.md`) first waits, within the same 2 seconds,
  for the reports already read to be applied, so a turn those reports start does not take the report's credit away.

## What reports can say beside an agent's hooks

Where an agent's hooks give its status, its provider can declare that the program's reports say two things beside
them: when it waits on the user, and when a turn was cancelled. The hooks stay the source of everything else, and only
what the provider declares is read. Qwen Code declares both signals. Other agents with an active status integration use reports only for terminal activity.

- **The wait on the user.** A `blocked` root record raises the session's hand, as a terminal title that says only the
  user can act does: the session reads *waiting for the user*, its owner is told as on any route to that status (see
  "Notices to an owner" in `docs/product/reporting-to-owners.md`), messages for it are held, with the `writes_held`
  reason `agent_action` (see "Messages held until a session can take them" in `docs/product/message-delivery.md`), and
  `interrupt_session` is refused with `session_waiting_for_user` (see "When a key is sent" in
  `docs/product/interrupting-turns.md`). The hand goes up at once for the kind `permission` or `question`, or for a
  `blocked` that names no kind, and for the kind `auth` only once it has stood for 3 seconds: one the next report
  replaces within them is passed over, as in "From the root record to the session's status" above. The report that
  replaces the `blocked` lowers the hand, and messages held for the session are released.
  - The hand is held over whatever the hooks say meanwhile: a hook reporting *working* does not lower it. A hand the
    hooks raised themselves stays theirs to lower; the report that lowers its own leaves theirs up.
  - From the program's first report of the root record on, its terminal title no longer says whether the agent waits
    on the user. Before any report, the title does, as for an agent that declares nothing.
  - After a daemon restart that adopts the session's terminal, a `blocked` root record the terminal was rebuilt with
    (see "Reports as the sign that a turn is running" above) raises the hand again, under the same rules, since the
    program does not report it again.
- **The cancel.** A root record that goes from `working` straight to `idle` is a cancelled turn: such a program reports
  `done` where a turn ends and `idle` where it is cancelled. Once the hook events already posted for the session have
  been applied — what they say of the session wins — and only if the session still reads *working* and the reports
  have shown no turn running since, the session is put at its prompt, paused (see "Paused" in
  `docs/product/session-statuses.md`), its turn is closed with no report to an owner, and messages held for it are
  released. This happens at once, without the second a terminal title back at the prompt has to stand for (see
  "Cancelling a Claude Code turn" in `docs/product/turns-hooks-do-not-report.md`). A turn end the agent's own hook still
  reports afterwards closes that turn as its real end. A `done` closes nothing by this route, nor does an `idle` after
  any state but `working`.
