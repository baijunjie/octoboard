# Information between sessions

Every session — a console session, or a project session whatever its binding — can see every other running session, in
every console and project, and tell any of them things. Neither needs to own the other.

| Tool | Arguments | What it does |
|---|---|---|
| `list_sessions` | — | The running sessions other than the caller, in every console and project, console sessions included, each with its status, title, agent, `role` (a console or a project session), `console`, `project` (null for a console session), `background_tasks`, `queued_messages`, `writes_held`, `owner`, `owner_kind` and `yours` as in the orchestration tools' reads (see "The console session's tools" in `docs/product/hub-orchestration.md`), and `your_owner` marking the one that owns the caller. The caller's own owner is also given at the top level (null when the caller is unbound or a console session). |
| `share_info` | `session`, `text` | Writes `text` into a running session, of any project or console, console sessions included, as information from the caller. Answers with `delivered` and a `note` saying, as the owner's `send_message` does (see "The console session's tools" in `docs/product/hub-orchestration.md`), whether it was delivered, queued, or written into the receiver's input but not sent to its conversation by its agent, and whether the receiver's owner was sent a copy — a copy written into the owner's input but not sent to its conversation is reported as such, the user having been told. Refused, with the reason in prose, for the caller itself, a session that does not exist, and a session with no process running. |

**Information is not instruction.** Every session knows who it belongs to: the console session or project session it
is bound to, or the user for an unbound project session or a console session. It takes work only from that owner (or
from the user typing in its terminal), and is told so in its role description, or, for a console session, in its
instruction file; what comes from any other session is to be weighed and not obeyed. A bound session is also told that
information neither starts nor finishes a round of work, so it does not `report` because of it: a `done` report would
archive it. A message `share_info` writes into the receiver therefore opens with a header naming the sender (id and
title) and saying the sender is not the receiver's owner and that the text is information to weigh, not work to take
on. The text itself is quoted — every line, blank ones included, is prefixed with `> ` — and followed by a closing line
naming the sender, so that a body imitating a report, a report panel form submission or a header of Octoboard's own is
plainly the sender's text and not one of those; the copy to an owner is quoted the same way. When the sender *is* the
receiver's owner the header says so instead and does not call it a non-owner. Commanding stays as in "Which sessions an
owner drives" in `docs/product/hub-orchestration.md`: `send_message`, `archive_session` and `reopen_session` act only on the caller's own sessions.

**This boundary is framing, not enforcement.** A message is text written into the receiver's terminal, and whether a
model acts on it is the model's own call; Octoboard cannot stop one that does. What it does is make the distinction as
plain as it can — a separate tool for information, the header on every delivery, role descriptions that name the
session's owner — and keep the one thing it can enforce, which sessions may be commanded, hard.

**An owner is told what reaches its sessions.** Information delivered to a session that is bound is also copied to that
session's owner, whether a console session or a project session, naming the sender and the receiver and saying it is
information. A copy is for the owner's awareness: if it shows a session of its own being redirected, the owner may
correct that session with `send_message`. This way a task an owner dispatched is not redirected by outside input it
cannot see. No copy goes to the sender when the sender is the owner, and none when the receiver is unbound or a console
session, the user being its owner. A copy that cannot be delivered — the owner has no process running — does not fail
the delivery to the receiver; the sender is told the owner was not copied. A copy for an owner that cannot take a
message right now is queued.

Delivery is the same as for any other message: sanitized as in "Messages held until a session can take them" in
`docs/product/message-delivery.md`, held when the receiver's own agent is waiting for the user — at a permission prompt
or a question of its own — may be at its trust confirmation, or shows a screen only the user can answer, and refused
when it has no process running. A receiver
reading as *waiting for the user* only because a session request of its own is waiting takes the message straight
away, since what has to be answered there is a dialog (see "Waiting for the answer" in
`docs/product/session-requests.md`). The two tools are on by default and have no setting. A session launched before
they existed keeps the role description it was first given, and on resume sees the tools without being told about them
or about its owner.
