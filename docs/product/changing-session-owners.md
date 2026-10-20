# Changing a session's owner

A binding can change after a session was opened in two ways that sessions make themselves, neither of which asks the
user: an owner hands a session of its own to another owner with `transfer_session`, and an unbound project session
binds itself to an owner with `bind_to_session`. The rules and messages below are theirs. The one other change is a
handoff to another agent — made by the user, or by an owner with `handoff_session` — which binds every session the
original owns to the receiving session (see `docs/product/handing-off-sessions.md`). A binding is never cleared: a
bound session does not become unbound again.
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
retrying. Both tools are also refused, changing nothing, while the session being rebound, or an owner involved — the
owner it is to be bound to, or a handover's earlier or new owner — is being started, resumed, handed off, switched to
another account or having its owner changed; the reason says to look again once that is done.

## Handing a session to another owner

`transfer_session` is offered to every session that can own sessions: a console session, an unbound project session and
a lead session.

| Argument | Required | Contents |
|---|---|---|
| `session` | yes | The session to hand over: one of the caller's (`yours: true`). |
| `to` | yes | The new owner's session id. |
| `note` | yes | The handover, for the new owner: what the session is doing, where it stands and what is left. |

Besides the rules above, it is refused for a session that is not the caller's (as for `send_message`), for one with no
process running — it is to be told of its new owner, so it has to be reopened first — and for handing a session to the
caller itself.

Once handed over, the session is bound to the new owner and reports to it, and the caller no longer drives it. Nothing
else is moved: the sessions bound to it stay bound to it. A lead session handed to a console session stays a lead
session; handed to a project session it has the role of a bound one. An ordinary bound session keeps that role
wherever it goes.

Both sessions concerned are told, by messages written through the delivery every message takes (queued while one cannot
take a message; see "Messages held until a session can take them" in `docs/product/message-delivery.md`):

- **The session** is told its owner has changed — which session handed it over to which, by kind, id and title — that it
  takes work only from the new owner, or from the user typing in its terminal, and reports to it with `report`, that
  this replaces what it was told of its owner when it was started, and that the earlier owner no longer drives it. A
  session that has lost the tools for owning sessions is also told it can no longer start or drive sessions, with the
  list of the Octoboard tools it has left.
- **The new owner** is told which session handed which session over to it, by id, title and project, that the session
  is its own from now on and is driven with `send_message`, `archive_session` and `reopen_session`, and, for a lead
  session, that it keeps its own sessions, which the new owner can read but not drive; then the caller's `note`, every
  line quoted, followed by a closing line.

Neither message spares the turn it starts a synthesised report: a session that carries on and stops without reporting is
reported on to its owner as usual (see "When a session does not report" in
`docs/product/reporting-to-owners.md`).

The call returns `session`, `owner` (the new owner's id), `owner_kind` (`console` or `project`) and a `note` saying the
session now reports to the new owner and is no longer the caller's, and whether each of the two was told, will be told
as soon as it can take a message, could not be told, with the reason, or was not told because the message was written
into its input but its agent did not send it, the user having been told to check it (see "A message written but not
sent" in `docs/product/message-delivery.md`). When the new owner could not be told, the note says to tell it of the
handover with `share_info`. Once the binding has changed the handover stands, whatever became of the messages.

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
`docs/product/reporting-to-owners.md`), with the status `needs_decision` and no open items. The report's summary
opens by saying the project session bound itself to the owner, is the owner's to drive from now on and reports to it
— and, for a lead session, that it keeps the sessions it started and drives them itself, which the owner can read but not drive — then gives the caller's
`summary`. It counts as the caller's report for its current turn, so that turn ending without a `report` produces no
synthesised report (see "When a session does not report" in
`docs/product/reporting-to-owners.md`).

The call returns `owner`, `owner_kind` and a `note` telling the caller that it is now bound to that owner (as its lead
session, where it is one) and whether its summary was sent as its first report — or was written into the owner's input
as its first report but not sent by the owner's agent, so the owner has not read it, the user has been told, and it is
not to be sent again (see "A message written but not sent" in `docs/product/message-delivery.md`); that from now on the
owner is the one it takes work from, or the user typing in its terminal, and it reports to it with `report` once per
round of work, which replaces what its role description says about being unbound and not calling `report`; that `done`
with no open items archives it, subject to "Automatic archiving" in `docs/product/reporting-to-owners.md` — refused for
a lead session while a session it started is not archived and no lock blocks automatic archiving — while
`needs_decision` or open items leave it running; that a lead session keeps driving its own sessions with `new_session`
and the tools that go with it, and that any other can no longer start or drive sessions, with the list of the Octoboard
tools it has left; and not to call `bind_to_session` again. Once the caller is bound, the binding stands: a summary that
could not be delivered is reported in the note, with the reason, and the caller is told to send it with `report` itself.

## When a binding changes while a session runs

A session's role description is written when it launches, from who it reports to then, and two of the agents keep what
a conversation was first given on every resume (see "What is injected on every launch" in
`docs/product/launching-agents.md`). So a change of owner is told to the session in prose instead — in the message a
handover or a handoff writes into it, or in the result of its own `bind_to_session` call — and its role description
says that what
Octoboard tells it of a later change replaces what it says of its owner.

The tools follow the binding as it stands. Octoboard checks every call against it, so a tool the session no longer has
is refused even where its agent still lists it. The session's tool list itself is updated with the answer to its next
call of any Octoboard tool, and its agent is told the list has changed (the MCP `tools/list_changed` notice); whether an
agent reads the list again on that notice is the agent's own. Only two changes take tools away — binding to a project
session, and a lead session handed to one — and they leave the bound session's set: `report`, `list_sessions`,
`share_info`, `get_project` and `request_session`.
