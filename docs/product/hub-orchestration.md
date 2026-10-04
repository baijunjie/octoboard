# Hub orchestration

A console's **hub session** is the one the user brings a request to. It does not change project code
itself: it works out which project a request belongs to, starts sessions there, follows them up, and
summarizes what they came back with. The sessions it starts are ordinary project sessions, described
in `docs/product/sessions.md`. Besides its terminal the hub has one surface of its own for showing
the user something — the report panel, described in `docs/product/report-panel.md`.

The hub drives Octoboard through tools Octoboard injects into the session; a project session gets one
tool back the other way. **Which tools a session sees follows from its role alone**, so a project
session cannot start or archive sessions, and the hub cannot report to itself.

## One live hub per console

A console has at most one hub session that is not archived. Opening a second one is refused, and so
is reopening an archived hub while a live one exists; the refusal names the existing hub's session
id. Archiving the live hub is what frees the slot.

A hub session belongs to no project, so archived hubs are listed in the console's own "Archived
hubs" group rather than in any project's Archive.

## The hub's tools

| Tool | Arguments | What it does |
|---|---|---|
| `list_projects` | — | The console's projects: id, name, host, directory, default agent, and the sessions currently running in each. |
| `add_project` | `source` (`local` / `parent` / `github`), `path?`, `remote_url?`, `name?`, `default_agent?` | Associates one or more projects with this console, under the same rules as the user's own form (see "Associating a project" in `docs/product/consoles-and-projects.md`). Answers with the projects it added. |
| `start_session` | `project`, `brief`, `agent?` | Starts a session in one of this console's projects and hands it the brief as its opening prompt. `agent` overrides the agent for that one session. Answers with the new session's id and its agent. |
| `send_message` | `session`, `text` | Appends an instruction to a running session. Answers with whether it was written or queued. |
| `get_session` | `session` | The session's record — status, title, agent, project, whether it is part of the orchestration, timestamps — plus a tail of what it has printed. |
| `archive_session` | `session` | Ends the session's process and archives it. |
| `list_archived` | `project` | The archived sessions of one project. |
| `reopen_session` | `session`, `text?` | Relaunches an archived or interrupted session, continuing its conversation, and optionally hands it an instruction, delivered once the relaunched session can take one. |
| `show_page` | `html` | Pushes an HTML page to the console's report panel, beside the hub's own terminal. Answers with the new page's id. See `docs/product/report-panel.md`. |

A project is named either by its id or by its name where that name is unambiguous within the
console; an ambiguous name is refused and asks for the id.

`get_session` returns at most 8 KiB of the session's most recent output, with terminal escape
sequences and cursor-control bytes stripped and runs of blank lines collapsed, so it is readable
prose rather than a rendered frame. A session that is waiting for the user carries a note saying so.

The hub reaches only its own console: a session or project id from another console is refused. So is
`send_message` or `archive_session` aimed at a hub session.

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
`## Constraints`. A field the hub left out or left blank is omitted entirely rather than sent as an
empty heading.

A session the hub starts is **titled from its goal** rather than from its project — the goal's first
non-blank line, shortened to roughly 48 characters on a word boundary with an ellipsis — so several
sessions dispatched into one project can be told apart in the menu.

## Reporting

A project session reports a round of work with `report`:

| Argument | Required | Contents |
|---|---|---|
| `summary` | yes | What was done and what came of it, in natural language. |
| `status` | yes | One of `done`, `failed`, `needs_decision`. No other value is accepted. |
| `open_items` | no | Strings naming what is left unfinished. Empty when nothing is. |

The report is written into the console's hub session as a user message naming the reporting
session's id, its title and project, the status, the open items, and then the summary. The reporting
session is told either that it was delivered or that it was accepted and will reach the hub as soon
as the hub can take a message.

Reporting fails, and leaves the session exactly as it was, when:

- the session is outside the hub's orchestration, so there is nobody to report to;
- the console has no hub session on record;
- the session has already been wrapped up — a session archived by its own `done` report cannot
  report a second time.

### When a session does not report

**Reporting is not forced.** When a session that reports to the hub ends a turn without having
called `report`, Octoboard delivers that turn's last assistant message as the report instead. The
message says plainly that the session stopped without reporting and that the status is Octoboard's
guess rather than its own word. The status is `needs_decision`, or `failed` when the turn ended in
an error, and the open-items list is empty. A session whose turn produced no message at all is
reported as such, with the suggestion that the hub check it with `get_session`.

A synthesised report never archives the session: only a session's own `done` report does that.

Grok Build reports no turn end at all for some turns, and the backstop Octoboard falls back on
there fires about a minute after the turn ends, so a synthesised report for one of those turns
arrives that much later. Sessions outside the orchestration are never reported on.

### Automatic archiving

A report with `status: done` and **no** open items wraps the session up: once the report has been
accepted for the hub, the session's process is ended and the session is archived. "Accepted" rather
than "read" — a hub that is merely busy has the report queued for it, and the session is archived
anyway rather than being left alive until the hub gets round to it.

Anything else — open items, `failed`, `needs_decision` — leaves the session running and awaiting
instructions, for the hub to continue with `send_message` or to archive explicitly.

## Which sessions the hub drives

A session the hub started always reports to it. A session **the user opens by hand does not**,
unless they check "Include in hub" in the session dialog; the box is unchecked by default and the
choice is fixed for that session's lifetime. Every session record the hub reads carries that flag,
and a session outside the orchestration is the user's — the hub is told to leave it alone.

A hub session itself never reports anywhere.

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
the session can take one, oldest first. The hub is told the message was queued and that it must not
send it again.

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

The loss is reported rather than passed off as delivered, but not to everyone: the sender whose own
call ran into it is told (a report fails, and its session is *not* archived), while a sender whose
message was already queued behind it is not. What covers those is the message the user gets, which
names the session and tells them to look at it before anything else is sent.
