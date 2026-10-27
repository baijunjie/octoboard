# Changing a session's owner

A binding can change after a session was opened in three ways that sessions make themselves, none of which asks the
user: an owner hands a session of its own to another owner with `transfer_session`, an owner releases a session of its
own with `release_session`, and an unbound project session binds itself to an owner with `bind_to_session`. The user
can release any bound session as well. The rules and messages below are theirs. The one other change is a handoff to
another agent — made by the user, or by an owner with `handoff_session` — which binds every session the original owns
to the receiving session (see `docs/product/handing-off-sessions.md`). A released session is unbound, and can be bound
again like any unbound project session.
Every connected window shows the change at once, the session moving to its new place in the sidebar and focus modes.

## Who can own a session

Every binding made this way keeps the same rules. The new owner must be:

- a **console session of the session's own console**, or a **project session of the session's own project that can own
  sessions** — an unbound one or a lead session. A project session with the role of a bound one owns none, so
  orchestration stays at most two levels deep, and no binding can close a cycle;
- **running** — it has a process, and is neither interrupted nor archived;
- **not the session itself.**

A session that has sessions of its own — archived ones included, since its owner can reopen them — can only be bound to
a console session: bound to a project session it could own none. A call that breaks a rule is refused, with the reason
in prose, and changes nothing; so is one whose binding changed while it was being made — the session bound elsewhere
meanwhile, it gained sessions of its own, or the owner can no longer own sessions — which says to look again before
retrying. `transfer_session` and `bind_to_session` are also refused, changing nothing, while the session being rebound,
or an owner involved — the owner it is to be bound to, or a handover's earlier or new owner — is being started, resumed,
handed off, switched to another account, having its branches handed over or having its owner changed; the reason says to
look again once that is done.

## Handing a session to another owner

`transfer_session` is offered to every session that can own sessions: a console session, an unbound project session and
a lead session.

| Argument | Required | Contents |
|---|---|---|
| `session` | yes | The session to hand over: one of the caller's (`yours: true`). |
| `to` | yes | The new owner's session id. |
| `note` | yes | The handover, for the new owner: what the session is doing, where it stands and what is left. |

Besides the rules above, it is refused for a session that is not the caller's (as for `send_message`) and for handing
a session to the caller itself. The session itself may be running or not; only the new owner must be running. One with
no process running is told of its new owner when it next starts (see "A change of owner told at the next launch"
below).

Once handed over, the session is bound to the new owner and reports to it, and the caller no longer drives it. Nothing
else is moved: the sessions bound to it stay bound to it. A lead session handed to a console session stays a lead
session; handed to a project session it has the role of a bound one. An ordinary bound session keeps that role
wherever it goes.

Both sessions concerned are told, by messages written through the delivery every message takes (queued while one cannot
take a message; see "Messages held until a session can take them" in `docs/product/message-delivery.md`):

- **The session**, when it is running, is told its owner has changed — which session handed it over to which, by kind,
  id and title — that it takes work only from the new owner, or from the user typing in its terminal, and reports to it
  with `report`, that this replaces what it was told of its owner when it was started, and that the earlier owner no
  longer drives it. A session that has lost the tools for owning sessions is also told it can no longer start or drive
  sessions, with the list of the Octoboard tools it has left.
- **The new owner** is told which session handed which session over to it, by id, title and project, that the session
  is its own from now on and is driven with `send_message`, `archive_session` and `reopen_session`, and, for a lead
  session, that it keeps its own sessions, which the new owner can read but not drive; then the caller's `note`, every
  line quoted, followed by a closing line.

Neither message spares the turn it starts a synthesised report: a session that carries on and stops without reporting is
reported on to its owner as usual (see "When a session does not report" in
`docs/product/reporting-to-owners.md`).

The call returns `session`, `owner` (the new owner's id), `owner_kind` (`console` or `project`) and a `note` saying the
session now reports to the new owner and is no longer the caller's, and whether each of the two was told, will be told
as soon as it can take a message, is not running and will be told when it next starts (the session only), could not be
told, with the reason, or was not told because the message was written
into its input but its agent did not send it to the conversation — it is in the input box, or went to a view of the
agent's background agents — the user having been told to check the session (see "A message written but not
sent" in `docs/product/message-delivery.md`). When the new owner could not be told, the note says to tell it of the
handover with `share_info`. Once the binding has changed the handover stands, whatever became of the messages.

## Releasing a session from its owner

`release_session` is offered to every session that can own sessions: a console session, an unbound project session and
a lead session. It needs no approval.

| Argument | Required | Contents |
|---|---|---|
| `session` | yes | The session to release: one of the caller's (`yours: true`). |
| `note` | no | A parting note for the released session: what it should know to carry on without its owner. Trimmed; a blank note is ignored, and one longer than 4000 characters is refused. |

The user can release any bound session, whoever its owner is, with **Unbind** in the sidebar's session menu, which
asks for confirmation first and takes no note (see "Unbinding a session from the window" below).

A released session is an **ordinary unbound project session**, the user's own:

- its binding is cleared, and a lead session is no longer one;
- it takes work only from the user typing in its terminal, and `report` is refused for it, since it has nobody to
  report to; no report is synthesised for it either;
- it has the unbound project session's tools, so it can start and drive sessions of its own (see "The unbound project
  session's tools" in `docs/product/hub-orchestration.md`), and it can be bound again with `bind_to_session`;
- what it had stays with it: a released lead session keeps the sessions bound to it and goes on driving them, and the
  branches the session is responsible for stay its own;
- its former owner no longer instructs or drives it: archiving that owner no longer takes it along, and nothing archives
  it on a `done` report. Releasing cuts no communication: either of the two can still tell the other something with
  `share_info`, which the receiver weighs rather than obeys (see `docs/product/information-between-sessions.md`).

A session can be released whatever its state — running, interrupted or archived. The call is refused and changes
nothing:

| Refused | Code |
|---|---|
| A console session, which is never bound | `console_session_never_bound` |
| A session that is not bound | `session_not_bound` |
| A session that is not the caller's, or a `note` longer than 4000 characters (the tool only) | — |
| The session is being started, resumed, handed off, switched to another account, having its branches handed over or having its owner changed; the reason says to look again once that is done | `session_changing` |
| The owner it is bound to is being started, resumed, handed off, switched to another account or having its owner changed; the reason says to look again once that is done | `session_owner_changing` (names the `owner`) |
| Its binding changed while it was being released — it was bound elsewhere meanwhile; the reason says to look again before retrying | `session_binding_changed` |

The window's request is answered with these codes (an id that names no session is `unknown_session`); through the
tool, every refusal reaches the model as its reason in prose.

**The released session is told**, by a message written through the delivery every message takes (queued while it
cannot take one; see "Messages held until a session can take them" in `docs/product/message-delivery.md`), that it has
no owner now, having been released from the session it reported to, and is the user's own session; that it takes work
only from the user typing in its terminal and is not to call `report`, which is refused for a session with no owner;
that this replaces what it was told of its owner when it was started; and that it can start and drive sessions of its
own with `new_session` and the tools that go with it, with the list of the Octoboard tools it now has. A release made
with a `note` ends that message with the note, marked as its former owner's, every line quoted, followed by a closing
line. One with no process running is told when it next starts instead, and so is one whose message could not be
written; the note is kept with what it is owed (see "A change of owner told at the next launch" below).

**The former owner is told only of the user's release**: an owner that releases a session itself is sent nothing, since
it made the call. When the user releases a session, its former owner, if it has a process running, is told by a message
written through the same delivery (queued while it cannot take one) which session the user released from it — by
kind, id and title — that the session has no owner now and answers only to the user, so it can no longer instruct,
drive or archive it, and the session does not report to it; that it can still tell the session something with
`share_info`, which the session weighs rather than obeys; that it should no longer count on the session; and that any
work it handed the session is left with the user. A former owner with no process running is not told,
then or later; a later attempt by it to drive the session is refused as for any session that is not its own.

The call returns `session` and a `note` saying that the session has no owner now, is no longer the caller's, is the
user's own session, is no longer instructed or driven by the caller and no longer reports to it, though `share_info` can
still tell it something, and whether it was told, will be told as soon as it can take a
message, is not running and will be told when it next starts, or was not told because the message was written into its
input but its agent did not send it to the conversation, the user having been told to check the session (see "A
message written but not sent" in `docs/product/message-delivery.md`). Once the binding is cleared the release stands,
whatever became of the message.

## Unbinding a session from the window

**Unbind** in a bound session's menu (see "Session rows" in `docs/product/sidebar-session-rows.md`) releases nothing
by itself: it opens a confirmation, **Unbind session**, a warning modal: the warning icon leads its header (see
"Destructive confirmations" in `docs/product/dialogs.md`), though the release is not destructive. Its body names the
session and its owner by their titles, each once, in a first paragraph saying that the session will no longer report to
that owner and will answer only to the user, and that from then on only the session itself can bind to an owner again
(with `bind_to_session`) — Octoboard cannot do it for the user. A consequence line follows: the owner will no longer be
able to instruct it, archive it or give it to another owner, archiving the owner will not archive it, and the two can
still share information with each other. **Cancel** closes the confirmation and releases nothing. **Unbind**, the
primary button, which has keyboard focus when the confirmation opens, releases the session as "Releasing a session
from its owner" above describes, with no note, and closes the confirmation.

The confirmation reads the session and its owner as they stand while it is open:

- **The binding is gone.** When the session is released some other way, archived or deleted, or its owner no longer
  exists, the confirmation closes without a word. So does a confirmed release refused as `session_not_bound`, or as
  `session_binding_changed` once the session reads unbound.
- **The binding moved to another owner.** The confirmation names the session's owner as it is now. A confirmed release
  refused as `session_binding_changed` while the session is still bound opens an error alert saying the session's
  owner changed while it was being released, with a line saying the confirmation now names its current owner and to
  unbind again if the user still wants to; the confirmation stays open.
- **The session or its owner is busy.** A confirmed release refused as `session_changing` opens an error alert saying
  the session is being started, resumed, handed off or switched to another account, is having its branches handed
  over or is being given another owner, so its owner cannot change now, with a line saying to wait for that to finish
  before trying again. One refused as `session_owner_changing` says the owner, named, is being started, resumed, handed
  off, switched to another account or given another owner, so sessions cannot be bound to or moved from it now, with a
  line saying to wait for that to finish, then try again. The
  confirmation stays open.

Every error alert a confirmed release opens is titled that the session could not be unbound, and leaves the
confirmation open for another try (see "The error alert" in `docs/product/error-alerts.md`).

## Binding an unbound session to an owner

`bind_to_session` is offered to a project session that can own sessions, and refused, with the reason in prose, once
the caller is bound: a lead session, bound already, cannot call it to move to another owner.

| Argument | Required | Contents |
|---|---|---|
| `owner` | yes | The owner's session id. |
| `summary` | yes | What the caller is doing and what it needs from the owner, which reaches the owner as its first report. |

Bound to a **console session**, the caller becomes a **lead session**: it keeps the sessions it started and goes on
driving them (see "Lead sessions" in `docs/product/hub-orchestration.md`). Bound to a **project session** — which
only a session with no sessions of its own may — it becomes an ordinary bound session, with the role of a bound one:
it can no longer start or drive sessions.

**The summary reaches the owner as the caller's first report**, through the ordinary reporting path (see
`docs/product/reporting-to-owners.md`), with the status `needs_decision` and no open items. The report's summary opens
by saying the project session bound itself to the owner, is the owner's to drive from now on and reports to it — and,
for a lead session, that it keeps the sessions it started and drives them itself, which the owner can read but not drive
— then gives the caller's `summary`. It counts as the caller's report for its current turn, so that turn ending without
a `report` produces no synthesised report (see "When a session does not report" in
`docs/product/reporting-to-owners.md`).

The call returns `owner`, `owner_kind` and a `note` telling the caller that it is now bound to that owner (as its lead
session, where it is one) and whether its summary was sent as its first report — or was written into the owner's input
as its first report but not sent to its conversation by the owner's agent, so the owner has not read it, the user has
been told, and it is not to be sent again (see "A message written but not sent" in `docs/product/message-delivery.md`);
that from now on the owner is the one it takes work from, or the user typing in its terminal, and it reports to it with
`report` once per round of work, which replaces what its role description says about being unbound and not calling
`report`; that `done` with no open items archives it, subject to "Automatic archiving" in
`docs/product/reporting-to-owners.md` — refused for a lead session while a session it started is not archived and no
lock blocks automatic archiving — while `needs_decision` or open items leave it running; that a lead session keeps
driving its own sessions with `new_session` and the tools that go with it, and that any other can no longer start or
drive sessions, with the list of the Octoboard tools it has left; and not to call `bind_to_session` again. Once the
caller is bound, the binding stands: a summary that could not be delivered is reported in the note, with the reason, and
the caller is told to send it with `report` itself.

## When a binding changes while a session runs

A session's role description is written when it launches, from who it reports to then, and two of the agents keep what
a conversation was first given on every resume (see "What is injected on every launch" in
`docs/product/launching-agents.md`). So a change of owner is told to the session in prose instead — in the message a
handover, a release or a handoff writes into it, or in the result of its own `bind_to_session` call, or, for a session
that was not running, at its next launch (see "A change of owner told at the next launch" below) — and its role
description says that what Octoboard tells it of a later change replaces what it says of its owner.

The tools follow the binding as it stands. Octoboard checks every call against it, so a tool the session no longer has
is refused even where its agent still lists it. The session's tool list itself is updated with the answer to its next
call of any Octoboard tool, and its agent is told the list has changed (the MCP `tools/list_changed` notice); whether an
agent reads the list again on that notice is the agent's own. Only two changes take tools away — binding to a project
session, and a lead session handed to one — and they leave the bound session's set: `report`, the reads every session
has (see `docs/product/what-every-session-reads.md`), `share_info`, `get_project`,
`request_session` and `request_resume_session`. A release gives a session that had the role of a bound one the
unbound project session's tools.

## A change of owner told at the next launch

A session whose owner changes — by a release, a `transfer_session`, or a handoff that moves the sessions of the
original (see "Who is told" in `docs/product/handing-off-sessions.md`) — while it has no process running, or whose
message about the change could not be written, is **owed a notice** of it. The notice is kept with the session, across
a restart of Octoboard, and is not part of the session record clients read.

It is delivered when the session's process is next started — a resume, a reopen, or any other relaunch, such as one for
a switch of account — ahead of the launch's own instruction, in the same text as that instruction where there is one (an
owner's `reopen_session` text, say): a Claude Code session resuming a conversation has it in its launch prompt, any
other session as a message delivered once it can take one (see "Messages held until a session can take them" in
`docs/product/message-delivery.md`). Where it stands among the information kept for an interrupted session is in
"Information for an interrupted session" in `docs/product/information-between-sessions.md`.

The notice is built at delivery, from the binding as it stands then, so a later change replaces an earlier one still
owed and several changes end in one notice. A session that has an owner is told that its owner changed while it was
not running, which session its owner is now, that it takes work only from it or from the user typing in its terminal
and reports to it with `report`, and that this replaces what it was told of its owner when it was started; one with
the role of a bound one is also told it can no longer start or drive sessions, with the list of the Octoboard tools it
has. A session with no owner is told what a released session is told (see "Releasing a session from its owner" above),
with the parting note of the release that left it unbound, where that release was made with one and is still the
latest change of the session's owner: a later change of owner — the session binding itself to an owner, say — drops
the note, and a note kept for a message that could not be written is kept only while the session is still unbound.

The notice, and any note kept with it, is no longer owed once the launch has started the session's process. A launch
that fails leaves it owed for the next one, so a session may be told of a change twice, but never not at all.
