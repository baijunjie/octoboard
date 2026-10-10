# Requesting a console session from a project session

## Problem

An unbound project session can start and drive sessions only in its own project. When its work needs other projects
too, it has no way to reach them, and the user has to carry the request to a console session by hand.

Two other approaches were considered and set aside:

- **Letting a project session reach other projects directly.** This breaks assumptions that owners and their bound
  sessions share a project: removing a project would leave dangling bindings or be refused halfway, and a project's
  bulk archive delete would silently remove sessions elsewhere. Reads, discovery and UI ownership would all need
  widening. It also hands a session the user opened by hand the console's whole reach, without asking.
- **Promoting the project session itself into a console session** (moving its conversation to the console's directory
  and resuming it there). A session's role description cannot change on resume: Claude Code replays its appended
  system prompt verbatim, and Grok Build stores its `--rules` in the session record. The session would get a console
  session's tools while still being told it is a project session. Moving the transcript to another working directory
  is also unmeasured for every agent.

## Plan outline

An unbound project session that needs other projects calls `request_console_session`, handing over its request and
what it has found out. The user confirms first. Once confirmed, Octoboard starts an ordinary console session in the
caller's console and binds the caller to it; what the caller handed over reaches the console session as the caller's
first report. The project session becomes the **lead session** of its project's work: it keeps its own bound sessions
and keeps driving them, and it reports to the console session as any bound session does. The console session
dispatches into the other projects and drives the lead session like any session of its own.

## Key design decisions

### The lead session keeps its team

When the requesting session already owns sessions of its project, those sessions **stay bound to it**. They are not
re-bound to the console session.

- A session's role description is fixed at launch. The sessions it started were told their owner is the project
  session; re-binding them to the console session would leave each one with a description of its owner that is no
  longer true.
- Why those sessions were started and how far each has got is in the lead session's conversation, not the console
  session's. Re-binding would hand the console session work it has no context for.
- The lead session and its sessions are an internal team. The console session deals with the lead session only.

Orchestration becomes at most two levels deep: console session → lead session → the lead session's sessions. A
session bound to a lead session still cannot start sessions, so there is no third level and no cycle.

### The console session sees the team but does not drive it

The console session's reads stay console-wide: the lead session's sessions appear in `list_projects` and `get_session`
with the lead session as their owner. The console session cannot drive them; the existing rule (an owner drives only
the sessions bound to it) already refuses that. Hiding them would let the console session dispatch a second session
into the same project and working directory without knowing one is already working there, which is the reason reads
are console-wide in the first place.

### A team goes where its owner goes

The sessions bound to a project session follow that session wherever the views place it. A project's focus mode lists
the project's unbound sessions, each with the sessions bound to it under it. A lead session is bound to a console
session, so it leaves its project's focus mode for its console session's, and its team goes with it, under it. The
project's focus mode counts its sessions bound to each console session; that count and its chip include a lead
session's team.

### A lead session follows its team's rules

A lead session's sessions follow the same rules as a console session's: one archives itself when it reports `done`
with no open items. A session of the lead session's that is not archived has therefore not finished its work, and the
lead session has not finished either:

- A lead session's `report` with `status: done` and no open items is **refused while any session bound to it is not
  archived**, with the reason naming those sessions. Its other reports (open items, `failed`, `needs_decision`) are
  accepted as for any bound session; none of them archives it.
- When a lead session ends a turn without calling `report`, **no report is synthesised for it while any session bound
  to it is running**: it has stopped to wait for its team, not stopped working. With none running, the synthesised
  report is sent as for any bound session.

### Becoming bound happens once, by the session's own call

A project session's binding is no longer fixed from launch in every case. **An unbound project session may become
bound exactly once, and only through `request_console_session`.** No other path binds an existing session, and a
binding is never undone.

The change is caused by the session's own tool call, and the tool's result tells it that it is now bound to the
console session and must report to it with `report`. That information sits in the session's own conversation, so its
fixed role description does not have to change.

### The console session is an ordinary one; the request is a report

The console session started on approval is launched exactly as one the user opens by hand, with no opening prompt of
its own. The caller is bound to it at the same moment, so what the caller handed over — what it needs done and what it
has found out — is delivered to the console session as the lead session's first report, through the existing
reporting path.

### User confirmation is enforced by Octoboard, not by the prompt

The daemon does not act on a `request_console_session` call by itself. It asks the user in a dialog showing the request,
and **the call waits for the answer**. Only on approval does it start the console session and bind the caller. The user
dismissing the dialog without choosing counts as a refusal. A refusal comes back to the session as a tool error in fixed
prose saying the user refused the request; the user is not asked for a reason. A rule written into a prompt can be
ignored by a model; the dialog cannot. This matters because the request hands a session the user opened by hand a route
to the console's whole reach.

**The wait has a time limit, and it is Octoboard's own.** The user may be away. When the limit passes without an
answer, Octoboard closes the dialog, starts nothing, and the call comes back as a tool error saying the user did not
respond. What to do next is the agent's own call; stopping there is the expected outcome. The limit is one value set by
Octoboard, below the time limit of Octoboard's own forwarding of the call; it is not tuned to any agent's own time
limit on a tool call, so adding an agent never requires measuring or configuring one.

**A request lives until it is answered, times out or is withdrawn.** The daemon holds it; a client only shows it.

- After the application reconnects, the pending request is sent again and its dialog shown again, as the trust
  confirmation's is. With several clients, the first answer wins and the other dialogs close.
- Several pending requests are shown in order, oldest first.
- It is withdrawn — its dialog closed, nothing started — on the signals Octoboard sees for every agent alike: the
  caller's process ends (interrupted, archived, exited), the call's connection to the daemon is dropped, or the agent
  cancels the call over MCP where it does so. The caller can ask again once resumed.
- An approval given after a withdrawal starts nothing, and the user is told in a toast that the session is no longer
  waiting.
- **The fallback for an agent that gave up on its own:** an agent may end the call by its own time limit without any
  signal reaching Octoboard. Octoboard does not try to know when; if the user approves after that, the approval is
  carried out as usual, and the outcome — that the caller is now bound to the console session and reports to it with
  `report` — is also written into the caller's session as a message, through the same delivery as any other message,
  so the caller learns it even though its tool result was lost.
- Pending requests are kept in memory only and are not persisted. No caller can outlive the daemon: every agent
  process ends with it, a session comes back `interrupted`, and the call is served inside the daemon, so a restart
  leaves nothing waiting.

### Where the rules are written

- **The tool's description** carries how to use it: call it when sessions are needed in other projects, the user is
  asked first, and on approval the caller becomes bound to the new console session and reports to it with `report`.
  Tool descriptions are in front of the model on every request and are served at every launch rather than recorded
  once, so later changes reach existing sessions on resume.
- **The tool's result** tells the session that it is now bound and to whom.
- **The role description is not changed.** It would only repeat the tool's description, and only new sessions would
  ever see it, because a role description is recorded once. This holds on every later launch too: a lead session is
  resumed and reopened with the unbound project session's role description and tool list it started with, not those
  of a session bound to a console session.

The tool is announced to unbound project sessions and to lead sessions, which keep that tool list; a lead session's
call is refused. Sessions an unbound project session starts and console sessions never see it.

## Milestones

1. [01 Lead sessions in the binding model](01-lead-sessions-in-the-binding-model.md)
2. [02 The request tool and its confirmation](02-request-tool-and-confirmation.md)
3. [03 Lead sessions and their teams in the sidebar](03-lead-sessions-in-the-sidebar.md)
