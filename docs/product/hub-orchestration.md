# Console session orchestration

A **console session** is the one the user brings a request to. It does not change project
code itself: it works out which project a request belongs to, starts sessions there, follows them up,
and summarizes what they came back with. The sessions it starts are ordinary project sessions,
described in `docs/product/sessions.md`. Besides its terminal the console session has one surface of
its own for showing the user something — the report panel, described in `docs/product/report-panel.md`.

The console session drives Octoboard through tools Octoboard injects into the session; a project
session gets one tool back the other way. **Which tools a session sees follows from its role alone**,
so a project session cannot start or archive sessions, and the console session cannot report to
itself.

## Several console sessions per console

A console may run any number of console sessions at once, each with its own sessions reporting to it
(see "Reporting" below): a session reports to the console session that is named in its own binding,
not to "the" console session of its console. How a console session is archived, and where archived
console sessions are listed, are in "Console sessions and project sessions" in
`docs/product/sessions.md`.

## The console session's tools

| Tool | Arguments | What it does |
|---|---|---|
| `list_projects` | — | The console's projects: id, name, host, directory, default agent, and the sessions currently running in each. |
| `add_project` | `source` (`local` / `parent` / `github`), `path?`, `remote_url?`, `name?`, `default_agent?` | Associates one or more projects with this console, under the same rules as the user's own form (see "Associating a project" in `docs/product/consoles-and-projects.md`). Answers with the projects it added. |
| `start_session` | `project`, `brief`, `agent?` | Starts a session in one of this console's projects and hands it the brief as its opening prompt. `agent` overrides the agent for that one session. Answers with the new session's id and its agent. Refused, with the reason in prose, when the session's resolved agent has been determined unavailable on this machine — never while that determination is still pending. |
| `send_message` | `session`, `text` | Appends an instruction to a running session. Answers with whether it was written or queued. |
| `get_session` | `session` | The session's record — status, title, agent, project, whether it is part of the orchestration, timestamps — plus a tail of what it has printed. |
| `archive_session` | `session` | Ends the session's process and archives it. |
| `list_archived` | `project` | The archived sessions of one project. |
| `reopen_session` | `session`, `text?` | Relaunches an archived or interrupted session, continuing its conversation, and optionally hands it an instruction, delivered once the relaunched session can take one. |
| `show_page` | `html` | Pushes an HTML page to the calling console session's report panel, beside the console session's own terminal. Answers with the new page's id. See `docs/product/report-panel.md`. |

A project is named either by its id or by its name where that name is unambiguous within the
console; an ambiguous name is refused and asks for the id.

`get_session` returns at most 8 KiB of the session's most recent output, with terminal escape
sequences and cursor-control bytes stripped and runs of blank lines collapsed, so it is readable
prose rather than a rendered frame. A session that is waiting for the user carries a note saying so.

The console session reaches only its own console: a session or project id from another console is
refused. So is `send_message` or `archive_session` aimed at a console session.

A refused call comes back to the agent as a **tool error carrying the reason in prose**, not as a
transport failure — "this session is waiting for the user" is advice the model is meant to act on.

## Handing out a task: the brief

`start_session` takes a `brief` of four natural-language fields:

| Field | Required | Contents |
|---|---|---|
| `goal` | yes | The outcome to achieve. |
| `context` | no | Background and relevant leads. |
| `acceptance` | no | Criteria for being done. |
| `constraints` | no | What must not be touched, whether committing or pushing is allowed, and so on. |

Octoboard renders the brief into the session's opening prompt from a **fixed template** — one
section per field, in the order above, headed `## Goal`, `## Context`, `## Acceptance`,
`## Constraints`. A field the console session left out or left blank is omitted entirely rather than
sent as an empty heading.

A session the console session starts is **titled from its goal** rather than from its project — the
goal's first non-blank line, shortened to roughly 48 characters on a word boundary with an ellipsis —
so several sessions dispatched into one project can be told apart in the menu.

A Claude Code session the console session starts in a directory Claude Code has not been trusted with
first stops on Claude Code's workspace-trust prompt. When the user has given that project their
consent, or has trusted a folder its directory lies under, Octoboard answers the prompt and the
session carries on without them; otherwise the user is asked in a dialog — again after the application
reconnects, if the prompt is still waiting — and the session waits on the prompt until it is answered,
reading as *working* meanwhile. A trusted folder covers the projects the console session itself
associates in it, a repository it clones there included: their sessions are answered without the user
being asked, and the permission rules and hooks in those repositories' `.claude/settings.json` then
apply without asking. See "Claude Code's workspace-trust prompt" and "Trusted folders" in
`docs/product/launching-agents.md`.

## Reporting

A project session reports a round of work with `report`:

| Argument | Required | Contents |
|---|---|---|
| `summary` | yes | What was done and what came of it, in natural language. |
| `status` | yes | One of `done`, `failed`, `needs_decision`. No other value is accepted. |
| `open_items` | no | Strings naming what is left unfinished. Empty when nothing is. |

The report is written into the console session the reporting session is **bound to** — the one named
by its own binding, not "the" console session of its console, since a console may hold several — as
a user message naming the reporting session's id, its title and project, the status, the open items,
and then the summary. The reporting session is told either that it was delivered or that it was
accepted and will reach its console session as soon as that console session can take a message.

Reporting fails, and leaves the session exactly as it was, when:

- the session is unbound, so there is nobody to report to;
- the console session it is bound to is no longer on record;
- the session has already been wrapped up — a session archived by its own `done` report cannot
  report a second time.

### When a session does not report

**Reporting is not forced.** When a session that reports to the console session ends a turn without
having called `report`, Octoboard delivers that turn's last assistant message as the report instead.
The message says plainly that the session stopped without reporting and that the status is
Octoboard's guess rather than its own word. The status is `needs_decision`, or `failed` when the turn
ended in an error, and the open-items list is empty. A session whose turn produced no message at all
is reported as such, with the suggestion that the console session check it with `get_session`.

A synthesised report never archives the session: only a session's own `done` report does that.

Grok Build reports no turn end at all for some turns, and the backstop Octoboard falls back on
there fires about a minute after the turn ends, so a synthesised report for one of those turns
arrives that much later. Sessions outside the orchestration are never reported on.

### Automatic archiving

A report with `status: done` and **no** open items wraps the session up: once the report has been
accepted for the console session, the session's process is ended and the session is archived.
"Accepted" rather than "read" — a console session that is merely busy has the report queued for it,
and the session is archived anyway rather than being left alive until the console session gets round
to it.

Anything else — open items, `failed`, `needs_decision` — leaves the session running and awaiting
instructions, for the console session to continue with `send_message` or to archive explicitly.

## Which sessions the console session drives

A session the console session started is always bound to it, so it always reports to it. A session
**the user opens by hand is not**, unless they check "Report to console session" in the session
dialog; the box is unchecked by default and the binding is fixed for that session's lifetime once
set. Every session record the console session reads carries its binding, and a session that is
unbound, or bound to a different console session, is not this one's to drive — the console session
is told to leave it alone.

A console session itself never reports anywhere.

## Messages held until a session can take them

A session that is working or awaiting instructions takes a message straight away; every agent queues
one written mid-turn and consumes it when the turn ends.

**A message is not written verbatim.** Every control character except newline and tab is removed from
its text, and a message that would otherwise begin with `/` is written with a single leading space, so
it reaches the model as text instead of being run as one of the agent's own slash commands. This holds
for everything Octoboard writes into a running session — an instruction, a report, a report panel form
submission — because all three can be model-authored and none is reviewed first.

An instruction or a report for a session that **cannot** take one right now — it is waiting for the
user at a permission prompt or a question — is queued rather than dropped, and delivered as soon as
the session can take one, oldest first. The console session is told the message was queued and that it
must not send it again.

A message for a session with no process running is refused outright: a resume starts the agent at
its prompt and replays nothing. Anything still queued when a session's process ends goes with it.

There is a second way a message Octoboard accepted does not arrive. If the agent stops taking input
part way through a write, part of that message is left sitting in the session's input line. Whatever
Octoboard writes next would be run together with it, so that message and everything queued behind it
are dropped rather than delivered spoiled.

Octoboard does not try to repair the line. The damage is bounded on its own — the next message written
into the session closes the leftover fragment and goes in with it — so the cost is one spoiled message
rather than a session that stays unusable, and that is the better of the two: refusing to write until
something proved the line was clean would stall a console's whole orchestration on a signal nothing
can give.

A partial write has **not been observed** in any attempt, so none of this has been seen to happen. The one try, on
Claude Code only, sent an 8,822-byte `send_message` into a session sitting at its folder-trust dialog: the call answered
`delivered`, the dialog stayed untouched and no text appeared in the terminal. What became of the text was not
established, and it is unexplained: it does not match the documented effect of a write at Claude Code's trust dialog,
where a trailing CR exits the session (see "Writing into a running session" in `docs/agent-cli-reference.md`). The
assumption above that the next write closes the leftover fragment, bounding the damage to one spoiled message, has
likewise never been seen to hold.

The loss is reported rather than passed off as delivered, but not to everyone: the sender whose own
call ran into it is told (a report fails, and its session is *not* archived), while a sender whose
message was already queued behind it is not. What covers those is the message the user gets, which
names the session and tells them to look at it before anything else is sent.
