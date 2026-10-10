# Orchestration

Orchestration is one session starting other sessions, handing them work, following them up and taking their reports.
The session doing it is the **owner** of the sessions it starts; every session it starts is bound to it and reports to
it. Three kinds of session can be an owner:

- A **console session** — a session the user brings a request to; a console may run several of them at once (see
  "Several console sessions per console" below). It does not change project code itself: it works out which project a
  request belongs to, starts sessions there, follows them up, and summarizes what they came back with. Besides its
  terminal it has one surface of its own for showing the user something — the report panel, described in
  `docs/product/report-panel.md`.
- An **unbound project session** — a project session the user opened by hand without binding it to a console session.
  It works in its own project and can also start sessions in that project and drive them (see "The unbound project
  session's tools" below).
- A **lead session** — a project session that was opened unbound and later became bound to a console session, at its
  own request and with the user's approval, keeping the sessions it had started (see "Lead sessions" below).

The sessions an owner starts are ordinary project sessions, described in `docs/product/sessions.md`. Archiving,
resuming and deleting an owner follow the same rules whichever kind it is (see "Archiving, interruption and resuming"
and "Deleting archived sessions" in `docs/product/sessions.md`).

Octoboard injects the tools into the session. A console session gets the full set (see "The console session's tools"
below), an unbound project session a narrower set scoped to its own project, and a session bound to an owner gets one
tool back the other way, `report`. Every project session, bound or not, also gets the two tools for sharing information
with the other sessions of its project (see "Information between sessions of a project" below). **Which tools a session
sees follows from its role and from whether it was opened bound**, both fixed for its lifetime: a session opened bound
cannot start or archive sessions, and an owner cannot report to itself. A lead session keeps the tools of the unbound
project session it was opened as. Orchestration is therefore at most two levels deep: a console session, a lead session
bound to it, and the lead session's own sessions.

## Several console sessions per console

A console may run any number of console sessions at once, each with its own sessions reporting to it
(see "Reporting" below): a session reports to the console session that is named in its own binding,
not to "the" console session of its console. How a console session is archived, and where archived
console sessions are listed, are in "Console sessions and project sessions" in
`docs/product/sessions.md`.

## The console session's tools

| Tool | Arguments | What it does |
|---|---|---|
| `list_projects` | — | The console's projects: id, name, host, directory, default agent, tags (see "Tags" in `docs/product/consoles-and-projects.md`), and the sessions currently running in each — every session of the console, whichever console session owns it. |
| `add_project` | `source` (`local` / `parent` / `git`), `path?`, `remote_url?`, `name?`, `default_agent?` | Associates one or more projects with this console, under the same rules as the user's own form (see "Associating a project" in `docs/product/consoles-and-projects.md`). Answers with the projects it added. |
| `start_session` | `project`, `brief`, `agent?` | Starts a session in one of this console's projects, bound to the calling console session, and hands it the brief as its opening prompt. `agent` overrides the agent for that one session. Answers with the new session's id and its agent. Refused, with the reason in prose, when the session's resolved agent has been determined unavailable on this machine — never while that determination is still pending. |
| `send_message` | `session`, `text` | Appends an instruction to a running session of the caller's. Answers with whether it was written or queued. |
| `get_session` | `session` | The session's record — status, title, agent, project, which session owns it and whether that owner is a console session or a project session (or that it is unbound), whether it is a console session, whether it is the caller's, timestamps — plus a tail of what it has printed. |
| `archive_session` | `session` | Ends the process of one of the caller's sessions and archives it. |
| `list_archived` | `project` | The archived sessions of one project, whoever owns them, each with its owner. |
| `reopen_session` | `session`, `text?` | Relaunches an archived or interrupted session of the caller's, continuing its conversation, and optionally hands it an instruction, delivered once the relaunched session can take one. The caller is the console session the session is bound to and is running, so reopening one never brings a console session back. |
| `show_page` | `html` | Pushes an HTML page to the calling console session's report panel, beside the console session's own terminal. Answers with the new page's id. See `docs/product/report-panel.md`. |

A project is named either by its id or by its name where that name is unambiguous within the
console; an ambiguous name is refused and asks for the id.

`get_session` returns at most 8 KiB of the session's most recent output, with terminal escape
sequences and cursor-control bytes stripped and runs of blank lines collapsed, so it is readable
prose rather than a rendered frame. A session that is waiting for the user carries a note saying so.

The console session reaches only its own console: a session or project id from another console is
refused. So is `send_message`, `archive_session` or `reopen_session` aimed at a console session.

**Reads are console-wide, writes are the caller's own.** `list_projects`, `get_session` and `list_archived`
return every session of the console, each carrying `owner` (the id of the session it is bound to, or null when it is
unbound), `owner_kind` (`console` or `project`, the kind of session that owner is; null when it is unbound) and
`yours` (whether that owner is the caller). `send_message`, `archive_session` and `reopen_session` act only on a
session bound to the caller; for any other — unbound, or bound to another console session or to a project session —
they refuse, with a reason naming who owns it, and change nothing. `start_session` always binds the new session to
the caller, so a console session cannot start one on another's behalf. The reads are not narrowed because two console
sessions can dispatch into the same project, and so the same working directory, at once; one that could not see the
other's sessions would collide with them. The tool descriptions and the console session's instructions tell it that a
session it does not own is somebody else's and is to be left alone.

A refused call comes back to the agent as a **tool error carrying the reason in prose**, not as a
transport failure — "this session is waiting for the user" is advice the model is meant to act on.

## The unbound project session's tools

An **unbound project session** — one the user opened by hand without choosing a console session under "Report to"
(see "Opening a session" in `docs/product/sessions.md`) — can start sessions in its own project and drive them, the
way a console session drives its console's:

| Tool | Arguments | What it does |
|---|---|---|
| `start_session` | `brief`, `agent?` | Starts a session in the caller's own project, bound to the caller, and hands it the brief as its opening prompt, exactly as the console session's `start_session` does (see "Handing out a task: the brief" below). There is no `project` argument; a call that names another project anyway is refused before anything is launched. `agent` may name any agent, not only the caller's own. Answers with the new session's id and its agent. Refused, with the reason in prose, when the session's resolved agent has been determined unavailable on this machine — never while that determination is still pending. |
| `send_message` | `session`, `text` | As the console session's. |
| `get_session` | `session` | As the console session's, for any session of the caller's own project. |
| `archive_session` | `session` | As the console session's. |
| `reopen_session` | `session`, `text?` | As the console session's. |
| `request_console_session` | `request`, `summary`, `open_items?` | Asks the user for a console session to reach other projects through; on approval the caller becomes its lead session. Refused for a lead session. See `docs/product/requesting-a-console-session.md`. |
| `report` | as in "Reporting" below | Offered, but refused while the session is unbound: it has nobody to report to. A lead session's `report` is accepted and goes to its console session (see "Lead sessions" below). |
| `list_project_sessions`, `share_info` | as in "Information between sessions of a project" below | As for every project session. |

It has no `list_projects`, `list_archived`, `add_project` or `show_page`.

**Reads are project-wide, writes are the caller's own** — the console session's rule, narrowed to one project.
`get_session` reads any session of the caller's own project, carrying `owner`, `owner_kind` and `yours` as above; a
session of another project, and any console session, is refused. `send_message`, `archive_session` and
`reopen_session` act only on a session bound to the caller, and refuse, with a reason naming who owns it, and change
nothing, for any other: an unbound session, one bound to a console session or to another project session, the caller
itself, or a session outside its project.

A session an unbound project session starts is bound to it, so it is offered `report` and the two information tools and
cannot start sessions of its own. It reports to the project session that started it exactly as a console session's
sessions report to their console session: see "Reporting" below.

## Lead sessions

A **lead session** is a project session that was opened unbound and was later bound to a console session of its
console. A session becomes one through its own `request_console_session` call, once the user approves it: Octoboard
starts a console session for it and binds it there (see `docs/product/requesting-a-console-session.md`).

- **Becoming bound happens once.** Only an unbound project session can become bound after it was opened, only through
  that request, only to a console session of its own console that is running, and only once. A binding, once set, is
  never changed or undone.
- **It keeps its sessions.** The sessions it had started stay bound to it, and it goes on driving them with the
  unbound project session's tools (see "The unbound project session's tools" above), starting more in its own project
  as before.
- **It keeps what it was launched with.** Every launch of it — a resume, a reopen, a switch of account — gives it the
  unbound project session's role description and tool list, as at its first launch, not those of a session bound to a
  console session.
- **It reports to its console session**, as any session bound to one does: its `report` is accepted and delivered
  there. Its `done` report with no open items is refused while its own sessions are unfinished, and no report is
  synthesised for it while they run (see "Reporting" and "When a session does not report" below).
- **Its sessions stay ordinary bound sessions.** A session bound to a lead session gets `report` and the information
  tools and no `start_session`, and can never be an owner: naming one as the owner of a new session is refused. There
  is no third level and no cycle.

A session is **under** an owner when it is bound to it, or bound to a lead session that is bound to it. Archiving,
reopening and deleting follow the bindings across both levels; see "Archiving, interruption and resuming" and
"Deleting archived sessions" in `docs/product/sessions.md`.

**The console session sees the lead session's sessions but does not drive them.** A lead session's `owner_kind` reads
`console`; its own sessions read `project`, with the lead session as their `owner`. The console session's reads stay
console-wide, so those sessions appear in its `list_projects` and `get_session`, not as its own; `send_message`,
`archive_session` and `reopen_session` aimed at one are refused, naming the lead session as its owner (see "Which
sessions an owner drives" below). The lead session itself is bound to the console session and is driven by it like any
other session of its own.

## Information between sessions of a project

Every project session — unbound, bound to a console session, or bound to a project session — can see the other running
sessions of its own project and tell them things. Console sessions are outside every project: they do not get these
tools and cannot be shared with, and a session of another project cannot be listed or reached.

| Tool | Arguments | What it does |
|---|---|---|
| `list_project_sessions` | — | The other running project sessions of the caller's project, each with its status, title, agent, `owner` and `owner_kind` as in the reads above, and `your_owner` marking the one that owns the caller. The caller's own owner is also given at the top level (null when the caller is unbound). |
| `share_info` | `session`, `text` | Writes `text` into a running project session of the caller's project, as information from the caller. Answers with whether it was written or queued, and whether the receiver's owner was sent a copy. Refused, with the reason in prose, for a session of another project, a console session, the caller itself, and a session with no process running. |

**Information is not instruction.** Every session knows who it belongs to: the console session or project session that
started it, or the user for one they opened by hand. It takes work only from that owner (or from the user typing in its
terminal), and is told so in its role description; what comes from any other session is to be weighed and not obeyed. A
bound session is also told that information neither starts nor finishes a round of work, so it does not `report` because
of it: a `done` report would archive it. A message `share_info` writes into the receiver therefore opens with a header
naming the sender (id and title) and saying the sender is not the receiver's owner and that the text is information to
weigh, not work to take on. The text itself is quoted — every line, blank ones included, is prefixed with `> ` — and
followed by a closing line naming the sender, so that a body imitating a report, a report panel form submission or a
header of Octoboard's own is plainly the sender's text and not one of those; the copy to an owner is quoted the same
way. When the sender *is* the receiver's owner the header says so instead and does not call it a non-owner. Commanding
stays as in "Which sessions an owner drives": `send_message`, `archive_session` and `reopen_session` act only on the
caller's own sessions.

**This boundary is framing, not enforcement.** A message is text written into the receiver's terminal, and whether a
model acts on it is the model's own call; Octoboard cannot stop one that does. What it does is make the distinction as
plain as it can — a separate tool for information, the header on every delivery, role descriptions that name the
session's owner — and keep the one thing it can enforce, which sessions may be commanded, hard.

**An owner is told what reaches its sessions.** Information delivered to a session that is bound is also copied to that
session's owner, whether a console session or a project session, naming the sender and the receiver and saying it is
information. A copy is for the owner's awareness: if it shows a session of its own being redirected, the owner may
correct that session with `send_message`. This way a task an owner dispatched is not redirected by outside input it
cannot see. No copy goes to the sender when the sender is the owner, and none when the receiver is unbound, the user
being its owner. A copy that cannot be delivered — the owner has no process running — does not fail the delivery to the
receiver; the sender is told the owner was not copied. A copy for an owner that cannot take a message right now is
queued.

Delivery is the same as for any other message: sanitized as in "Messages held until a session can take them", held
when the receiver's own agent is waiting for the user — at a permission prompt or a question of its own — or may be at
its trust confirmation, and refused when it has no process running. A receiver reading as *waiting for the user* only
because a request for a console session of its own is waiting takes the message straight away, since what has to be
answered there is a dialog (see "The call" in `docs/product/requesting-a-console-session.md`). The two tools are on by
default and have no setting. A session launched before they existed keeps the role description it was first given, and
on resume sees the tools without being told about them or about its owner.

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
`## Constraints`. A field the caller left out or left blank is omitted entirely rather than
sent as an empty heading.

A session started with `start_session` is **titled from its goal** rather than from its project — the
goal's first non-blank line, shortened to roughly 48 characters on a word boundary with an ellipsis —
so several sessions dispatched into one project can be told apart in the menu.

A session started with `start_session` in a folder its agent has not been told to trust can first stop
on that agent's own trust confirmation. When the user has given that project the trust permission, or
has trusted a folder its directory lies under, Octoboard presses the confirmation and the session
carries on without them; otherwise the user is asked in a dialog — again after the application
reconnects, if the confirmation is still waiting — and the session waits on it until it is answered,
reading as *working* meanwhile. A trusted folder covers the projects the console session itself
associates in it, a repository it clones there included: their sessions' confirmations are pressed
without the user being asked, whichever agent shows them, and that agent then applies the
repository's own configuration — for Claude Code, the permission rules and hooks in its
`.claude/settings.json` — without asking. See "The trust permission" and "Trusted folders" in
`docs/product/folder-trust.md`.

## Reporting

A bound project session reports a round of work with `report`:

| Argument | Required | Contents |
|---|---|---|
| `summary` | yes | What was done and what came of it, in natural language. |
| `status` | yes | One of `done`, `failed`, `needs_decision`. No other value is accepted. |
| `open_items` | no | Strings naming what is left unfinished. Empty when nothing is. |

The report is written into the session the reporting session is **bound to**, its **owner** — a console session,
or the project session that started it — named by its own binding rather than looked up, since a console may
hold several console sessions. It arrives as a user message naming the reporting session's id, its title and project,
the status, the open items, and then the summary. The reporting session is told either that it was delivered or that
it was accepted and will reach its owner as soon as the owner can take a message, the note calling the owner a console
session or a project session according to which it is.

Reporting fails, and leaves the session exactly as it was, when:

- the session is unbound, so there is nobody to report to;
- its owner is no longer on record;
- its owner has no process running — it is interrupted or archived. The binding names the owner by its id and
  outlives this, so once the owner is resumed or reopened the session's reports reach it again;
- the session has already been wrapped up — a session archived by its own `done` report cannot
  report a second time;
- the session is a lead session, the report has `status: done` and no open items, and a session bound to it is not
  archived. The reason names those sessions. Its sessions archive themselves when they report they are done, so one
  that is not archived has not finished, and neither has the lead session. Its other reports go through, and none of
  them archives it.

### When a session does not report

**Reporting is not forced.** When a bound session ends a turn without having called `report`, Octoboard delivers
that turn's last assistant message to its owner as the report instead.
The message says plainly that the session stopped without reporting and that the status is
Octoboard's guess rather than its own word. The status is `needs_decision`, or `failed` when the turn
ended in an error, and the open-items list is empty. A session whose turn produced no message at all
is reported as such, with the suggestion that the owner check it with `get_session`.

A synthesised report never archives the session: only a session's own `done` report does that.

**No report is synthesised for a lead session's turn while any session bound to it has a process** — running, or being
launched or resumed: it has stopped to wait for its sessions, not stopped working. With none of them running, its turn
is reported on as any bound session's is.

Grok Build reports no turn end at all for some turns, and the backstop Octoboard falls back on
there fires about a minute after the turn ends, so a synthesised report for one of those turns
arrives that much later. Sessions outside the orchestration are never reported on.

### Automatic archiving

A report with `status: done` and **no** open items wraps the session up: once the report has been
accepted for its owner, the session's process is ended and the session is archived.
"Accepted" rather than "read" — an owner that is merely busy has the report queued for it,
and the session is archived anyway rather than being left alive until the owner gets round
to it.

Anything else — open items, `failed`, `needs_decision` — leaves the session running and awaiting
instructions, for its owner to continue with `send_message` or to archive explicitly.

Automatic archiving archives the reporting session and nothing else. It never archives the owner the session is
bound to, and does not touch the other sessions bound to it: archiving an owner, and what that takes with it, is
covered in "Archiving, interruption and resuming" in `docs/product/sessions.md`.

## Which sessions an owner drives

A session started with `start_session` is always bound to the session that started it — a console session, or a
project session that is unbound or a lead session — so it always reports to it. A session **the user opens by hand is
not**, unless they choose a console session under "Report to" in the session dialog (see "Opening a session" in
`docs/product/sessions.md`); the choice is none by default and offers console sessions only. A binding, once set, is
never changed or undone; the one way a session opened unbound becomes bound is as a lead session, through its own
request for a console session (see "Lead sessions" above).

An owner drives only the sessions bound to it. Every session record it reads names that session's owner, and a session
that is unbound, or bound to any other session, is not this one's to drive: it can read such a session, but is told
to leave it alone and is refused if it tries to act on it.

A console session itself never reports anywhere, and neither does an unbound project session.

## Messages held until a session can take them

A session that is working or awaiting instructions takes a message straight away; every agent queues
one written mid-turn and consumes it when the turn ends.

**A message is not written verbatim.** Every control character except newline and tab is removed from its text, and a
message that would otherwise begin with `/` is written with a single leading space, so it reaches the model as text
instead of being run as one of the agent's own slash commands. This holds for everything Octoboard writes into a running
session — an instruction, a report, a report panel form submission, information shared by another session — because all
of them can be model-authored and none is reviewed first.

An instruction, a report or information shared by another session, for a session that **cannot** take one right now — it
is waiting for the user at a permission prompt or a question — is queued rather than dropped, and delivered as soon as
the session can take one, oldest first. A session that sent an instruction or shared information is told it was queued
and that it must not send it again.

A message for a session with no process running is refused outright: a resume starts the agent at
its prompt and replays nothing. Anything still queued when a session's process ends goes with it.

**Messages are also held while a session's trust confirmation may be up.** A message's trailing Enter would answer
the agent's own folder-trust confirmation (see `docs/product/folder-trust.md`) — accepting it, and so trusting the
folder, at Codex's and Grok Build's, and declining it, which exits the agent, at Claude Code's. So from every launch of
a session, a resume included, everything Octoboard writes into it — an instruction, a report, information shared by
another session, a report panel form submission, the instruction `reopen_session` hands over — is queued as above
until the confirmation can no longer be on screen:

- for Claude Code and Grok Build, until the session's first hook report, which each sends only once it is past the
  confirmation (and, for Claude Code, past any other startup screen that follows it). A session whose hook reports
  never arrive holds its messages for as long as it runs;
- for Codex, which reports nothing until its first prompt, until about 30 seconds have passed since the launch (or
  64 KiB of output has been printed) with no confirmation shown; once one is shown, until Octoboard's press of it
  succeeds, a hook report arrives, or the person's own answer in the terminal is seen to replace it on screen.

The user's own typing in the session's terminal is never held. A client's direct request to write into a session
(`send_message` in the daemon's client protocol) is refused instead of queued, with the error code
`session_trust_pending`, saying to send it once the confirmation has been answered, or once the session has finished
starting if none appears.

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
Claude Code only, sent an 8,822-byte `send_message` into a session sitting at its folder-trust dialog — a write the
hold described above now keeps back: the call answered `delivered`, the dialog stayed untouched and no text appeared
in the terminal. What became of the text was not
established, and it is unexplained: it does not match the documented effect of a write at Claude Code's trust dialog,
where a trailing CR exits the session (see "Writing into a running session" in `docs/agent-cli-reference.md`). The
assumption above that the next write closes the leftover fragment, bounding the damage to one spoiled message, has
likewise never been seen to hold.

The loss is reported rather than passed off as delivered, but not to everyone: the sender whose own
call ran into it is told (a report fails, and its session is *not* archived), while a sender whose
message was already queued behind it is not. What covers those is the message the user gets, which
names the session and tells them to look at it before anything else is sent.
