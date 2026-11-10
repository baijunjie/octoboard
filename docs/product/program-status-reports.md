# Program status reports

How the program running in a session's terminal can report its own status through OSC 7501, the Program Status
Protocol, revision 0.4 (<https://www.superlogical.com/rex/docs/build/program-status>), and when such reports become the
session's status. What the statuses mean, and where they come from for the other agents, is in
`docs/product/session-statuses.md`.

## Where the protocol is supported and where reports give the status

- **Every session's terminal supports the protocol, whichever its agent**: it keeps the reports as records (see
  "Reports and records" below), takes them out of what the clients receive, and answers the support query (see "The
  support query" below). Supporting the protocol says that the terminal accepts reports, not that they become the
  session's status.
- **Reports give a session its status only when its agent has no status integration of its own**, no hooks or
  signals of the agent that give its status. Claude Code, Codex and Grok Build each have one (see "What the statuses
  are derived from" in `docs/product/session-statuses.md`), and in their sessions reports move nothing: no status, no
  turn, no pause and no notice to an owner. Every agent supported today has one, so in this build no session takes its
  status from reports.
- **A shell terminal of the Terminal panel** (see `docs/product/terminal-panel.md`) does not support the protocol:
  reports reach the clients as ordinary output, and so does the support query, which no client answers.

"From the root record to the session's status" and "Reports at the end of the process" below apply only to a session
whose status reports give.

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
- **Only the root record's state bears on the session.** The other records, and the `kind`, `progress`, `app`, `title`
  and `msg` of any record, are kept but shown nowhere.

## How long records last

- **A shell prompt starting forgets every `working` and `blocked` record**, whatever its id, and keeps the `done`,
  `error` and `idle` ones: a prompt start is `OSC 133 ; A ST`, with or without options (`OSC 133 ; A ; … ST`), and the
  prompt itself still reaches the clients. A prompt start longer than 128 bytes, not counting its `ST`, is not
  recognised and forgets nothing.
- **The end of the session's process forgets every `working` and `blocked` record** too, keeping the others; this moves
  no status, since the end of the process settles the status itself (see "Reports at the end of the process" below).
- **A terminal reset (RIS, `ESC c`) forgets every record. A soft reset (DECSTR, `CSI ! p`) forgets none.**

## From the root record to the session's status

- **The root record's state is the session's status**:

  | Root record | Session status |
  |---|---|
  | `working` | *working* |
  | `blocked` | *waiting for the user* |
  | `idle` or `done` | at its prompt (`idle`), not paused |
  | `error` | at its prompt (`idle`), paused |
  | none — never reported, cleared, removed to make room, or forgotten by a prompt start or a reset | at its prompt (`idle`), not paused |

  At its prompt, the session shows the status "Which status a session shows" in `docs/product/session-statuses.md`
  gives it, so `done` and `idle` both read as *done* once the session has a conversation and as *awaiting instructions*
  before. *Dormant* and *working in background* never come from reports.
- **A turn starts when the root record goes to `working` or `blocked` from neither**, including from no record, and
  the session then has a conversation. A move between `working` and `blocked` stays within the turn: a blocked program
  is held up within its turn, as an agent asking for a permission is.
- **A turn ends when the root record leaves both `working` and `blocked`.** It counts as failed when the root record
  goes to `error`, and as ended normally otherwise, a `working` or `blocked` root forgotten at a shell prompt included
  (see "How long records last" above). For a session bound to an owner, the turn is reported as a turn that ended
  without the session reporting, one that produced no message: a report's `msg` is never passed on (see "When a session
  does not report" in `docs/product/reporting-to-owners.md`).
- **Going to *waiting for the user* tells an owner** that the session waits for the user, as any route to that status
  does (see "Notices to an owner" in `docs/product/reporting-to-owners.md`), and messages held for the session are
  released once a report brings it to its prompt, as they are on a hook's report.
- **A session request holds the session at *waiting for the user*** for the whole wait whatever the reports say, as it
  does whatever an agent's hooks say (see "Waiting for the answer" in `docs/product/session-requests.md`).

## Reports at the end of the process

- **Reports read before the session's process ended are applied before its end is recorded**, waited on for up to 2
  seconds after the agent's hook events are (see "Paused" in `docs/product/session-statuses.md`). So a final `done` or
  `idle` decides that the turn ended normally, and a final `error` that it failed, rather than the turn reading as cut
  short by the end of the process. The status after the end is still *interrupted*, or *archived* where the session was
  archived.
- **When the application quits**, reports already read are applied before the sessions are marked interrupted, within
  the half second the agents' hook events are waited on across all sessions.
- **Once the end is recorded, reports change nothing**: one applied after it, still on its way or not waited for,
  neither moves the status nor opens a turn.
- **A session's own `report` call** (see `docs/product/reporting-to-owners.md`) first waits, within the same 2 seconds,
  for the reports already read to be applied, so a turn those reports start does not take the report's credit away.
