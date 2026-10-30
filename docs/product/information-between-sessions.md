# Information between sessions

Every session — a console session, or a project session whatever its binding — can see every other running session, in
every console and project, and tell any of them things — an interrupted one too, which takes it once it is resumed.
Neither needs to own the other.

| Tool | Arguments | What it does |
|---|---|---|
| `list_sessions` | `console?`, `project?`, `owner?`, `yours?`, `kind?`, `branch?`, `status?` | The running sessions other than the caller (or, with `status`, the interrupted ones too or instead; see "Narrowing the list of sessions" below), in every console and project, console sessions included — one that a change of its account's sharing has ended and is about to resume included, as `starting` (see "When a change is carried out" in `docs/product/sharing-the-default-configuration.md`) — each with its status, title, agent, `role` (a console or a project session), `orchestration_role` (`coordinator` or `worker`), `console`, `project` (null for a console session), `background_tasks`, `context`, `queued_messages`, `writes_held`, `account`, `account_name`, `observed_model`, `observed_effort`, `default_effort`, `pinned`, `locked`, `branches`, `owner`, `owner_kind` and `yours` as in the orchestration tools' reads (see "The console session's tools" in `docs/product/hub-orchestration.md`), and `your_owner` marking the one that owns the caller. The caller's own owner is also given at the top level (null when the caller is independent or a console session). See "Narrowing the list of sessions" below for its arguments. |
| `send_message` | `session`, `text` | Writes `text` into another session, of any project or console, console sessions included, as information from the caller; an interrupted session has it kept and written into it once it is resumed (see "Information for an interrupted session" below). Answers with `delivered` and a `note` saying, as the owner's `instruct_session` does (see "The console session's tools" in `docs/product/hub-orchestration.md`), whether it was delivered, queued, kept for an interrupted session's resume, or written into the receiver's input but not sent to its conversation by its agent, and whether the receiver's owner was sent a copy — a copy written into the owner's input but not sent to its conversation is reported as such, the user having been told; one kept for an interrupted owner is reported as kept. `delivered` is `false` for information kept for a resume. Refused, with the reason in prose, for the caller itself, a session that does not exist, an archived session, which takes nothing until it is reopened, and any other session with no process running, such as one whose launch has not brought its process up yet — except one a change of its account's sharing of the default configuration is restarting, which accepts it and holds it for the resumed process (see "When a change is carried out" in `docs/product/sharing-the-default-configuration.md`). |

**Information is not instruction.** Every session knows who it belongs to: the console session or project session it is
bound to, or the user for an independent project session or a console session. It takes work only from that owner (or
from the user typing in its terminal), and is told so in its role description, or, for a console session, in its
instruction file; what comes from any other session is to be weighed and not obeyed. A reporting session is also told
that information neither starts nor finishes a round of work, so it does not `report` because of it: a `done` report
would archive it. A message `send_message` writes into the receiver therefore opens with a header naming the sender (id
and title) and saying the sender is not the receiver's owner and that the text is information to weigh, not work to take
on. The text itself is quoted — every line, blank ones included, is prefixed with `> ` — and followed by a closing line
naming the sender, so that a body imitating a report, a report panel form submission or a header of Octoboard's own is
plainly the sender's text and not one of those; the copy to an owner is quoted the same way. When the sender *is* the
receiver's owner the header says so instead and does not call it a non-owner. Commanding stays as in "Which sessions an
owner drives" in `docs/product/hub-orchestration.md`: `instruct_session`, `archive_session` and `reopen_session` act
only on the caller's own sessions.

**This boundary is framing, not enforcement.** A message is text written into the receiver's terminal, and whether a
model acts on it is the model's own call; Octoboard cannot stop one that does. What it does is make the distinction as
plain as it can — a separate tool for information, the header on every delivery, role descriptions that name the
session's owner — and keep the one thing it can enforce, which sessions may be commanded, hard.

**An owner is told what reaches its sessions.** Information delivered to a session that is bound is also copied to that
session's owner, whether a console session or a project session, naming the sender and the receiver and saying it is
information. A copy is for the owner's awareness: if it shows a session of its own being redirected, the owner may
correct that session with `instruct_session`. This way a task an owner dispatched is not redirected by outside input it
cannot see. No copy goes to the sender when the sender is the owner, and none when the receiver is independent or a
console session, the user being its owner. A copy for an owner that cannot take a message right now is queued, and one
for an interrupted owner is kept for its resume as the information itself would be (see "Information for an interrupted
session" below). A copy that cannot be delivered — the owner is archived, or has no process running for another reason —
does not fail the delivery to the receiver; the sender is told the owner was not copied.

Delivery is the same as for any other message, including its draft protection and other holds: see "Messages held
until a session can take them" in `docs/product/message-delivery.md`. A receiver with no process running is the
exception: an interrupted one has the information kept for its resume (see "Information for an interrupted session"
below), one a change of its account's sharing is restarting has it held for the resumed process (see "When a change is
carried out" in `docs/product/sharing-the-default-configuration.md`), and any other refuses it. A receiver reading as
*waiting for the user* only because a session request of its own is waiting takes the message subject to those holds,
since what has to be answered there is a dialog (see "Waiting for the answer" in `docs/product/session-requests.md`).
The two tools are on by default and have no setting.

## Narrowing the list of sessions

`list_sessions` takes seven optional arguments, the same for every caller whatever its role. `status` picks which
sessions are listed, and an archived session never is, whatever `status` is — archived sessions are `list_archived`'s
(see "Listing archived sessions" in `docs/product/what-every-session-reads.md`):

- `live`, the default: the sessions with a process — a session a change of its account's sharing is restarting
  included;
- `interrupted`: the sessions whose process ended without their being archived, each reading `interrupted`; an owner
  resumes one of its own with `reopen_session` (see "The console session's tools" in
  `docs/product/hub-orchestration.md`);
- `any`: both.

Each of the other six, given, narrows the list, and a session is listed only when it matches every one:

- `console`: only the sessions of that console, named by its id or by a name that names one console (`list_consoles`
  lists them).
- `project`: only the sessions of that project, named by its id, in any console, or by its name — looked for among the
  caller's own console's projects first, else among every console's. Given with `console`, the project is looked for
  among that console's projects only, by id or by name, and one that is not among them is refused. A console session
  is in no project, so it is never listed under one.
- `owner`: only the sessions bound to that session, named by its id.
- `yours`: `true` lists only the sessions bound to the caller, `false` only those that are not.
- `kind`: `console` for console sessions only, `project` for project sessions only.
- `branch`: only the sessions responsible for the local branch of that name, as their `branches` name it (see "Session
  records" in `docs/product/what-every-session-reads.md`).

A `console`, `project` or `owner` that names nothing, a name that names more than one console or project, a `yours` that
is not `true` or `false`, a `kind` or a `status` of any other value, and `kind: console` together with a `project` are
refused, with the reason in prose, rather than read as a filter that matches nothing.

## Information for an interrupted session

`send_message` to an **interrupted** session — one whose process has ended without it being archived — is not refused:
the information is kept in Octoboard's store and written into the session when its process is next started, by a
resume, a reopen or any other relaunch, such as one for a switch of account. The caller is told it was queued, that it
is kept until the session is resumed, and not to send it again. The same holds for the copy for an owner that is
interrupted (see "An owner is told what reaches its sessions" above). The same store also keeps the reports and
notices for an interrupted owner, under the rules below (which ones, and how they read, is in "Reports and notices for
an interrupted owner" in `docs/product/reporting-to-owners.md`).

- **It survives until it is written.** What is kept stays in the store across a restart of Octoboard and a relaunch
  that fails, and through a relaunch that succeeds until it has been written: if the process ends, or Octoboard exits,
  before then, whatever was not yet written is delivered at the next resume instead, so a piece may arrive twice but
  is not lost. A kept piece of information is gone only
  once its message has been written into the session — a message left unsent in the agent's input box, or taken by a
  view of its background agents, counts as written (see "A message written but not sent" and "A message a dialog
  discarded or a view took" in `docs/product/message-delivery.md`) — or when the queue it was on is lost to a partial
  write, which the user is told of as for any message (see "Messages held until a session can take them" there).
- **Delivered in the order it arrived**, among everything kept for the session, as messages queued for the resumed
  session, held like any other until the session can take one (see "Messages held until a session can take them" in
  `docs/product/message-delivery.md`). When the relaunch carries an instruction of its own — an owner's
  `reopen_session` text, or a notice of a change of owner (see "A change of owner told at the next launch" in
  `docs/product/changing-session-owners.md`) — a Claude Code session that resumes a conversation has that instruction
  as its launch prompt and the information written after it; any other session has the information written first and
  the instruction after it.
- **Archiving drops it.** Archiving the session discards what was kept for it, so a later reopen is not handed it, and
  deleting the session deletes it with the session. `send_message` to an archived session is refused outright, so that
  nothing piles up for a session that may never be reopened.
- **A handoff passes it on.** When an interrupted session is handed off to another agent, what was kept for it is
  forwarded to the receiving session with the rest of its queue, each piece once, and then no longer kept for the
  original (see "Messages for the original during a handoff" in `docs/product/handing-off-sessions.md`).
- **It is counted as waiting.** The `queued_messages` of the session's record counts everything kept for it —
  information, and for an owner the reports and notices — each piece once, whether or not a resume has queued it yet
  (see "Session records" in `docs/product/what-every-session-reads.md`).

The information is kept in the store rather than in memory because quitting the application is one of the most common
ways a session becomes interrupted; a queue held in memory would be lost with it.
