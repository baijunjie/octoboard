# Sessions

A **session** is one agent CLI process, running with a project's directory as its working directory and rendered in a
terminal the user types into directly. Octoboard is not an agent and not a terminal of its own: it starts the agent's
own binary and shows it as it is. What it adds to each launch, and what it guarantees it does not change, is in
`docs/product/launching-agents.md`.

## Hub sessions and project sessions

| | Working directory | Where it appears |
|---|---|---|
| Hub session | the console's working directory | the console's "Hub" row |
| Project session | the project's directory | under that project |

**A console has at most one hub session whose process is running**, enforced by the daemon: opening
a hub, or reopening an interrupted or archived one, while another hub of the console is running is
refused, and the refusal names the running hub's session id.

**The Hub row** holds the console's newest hub session that is not archived, whether running or
interrupted. Clicking it starts a new hub when the row is empty and selects the hub in it otherwise,
which resumes an interrupted hub as selecting any interrupted session does. While the row holds a hub
it has an action menu:

- **Archive** — always offered. It asks for confirmation, then archives the hub as described in
  "Archiving, interruption and resuming" below.
- **Resume** — offered only while the hub is interrupted.

There is no Rename on the Hub row; its label is always "Hub". The hub cannot archive itself (see
"The hub's tools" in `docs/product/hub-orchestration.md`), so the user archives it from this menu.
Archiving the hub moves it to the console's "Archived hubs" group and leaves the Hub row empty, so the
next click on the row starts a fresh hub. That fresh hub is opened with the console's current hub
agent and agent config directory, whereas resuming or reopening a hub keeps the ones it was opened
with (see "Agent config directories" in `docs/product/consoles-and-projects.md`).

Should a console ever have more than one hub that is not archived, the ones not in the Hub row are
listed directly below it as ordinary session rows.

A hub session carries no project; it runs in the console's working directory. It is given Octoboard's
orchestration tools and dispatches work to sessions in the console's projects — see
`docs/product/hub-orchestration.md`. Because it belongs to no project, archived hubs are listed in
the console's own "Archived hubs" group rather than under any project.

Selecting a hub session also shows its console's **report panel** beside the terminal, described in
`docs/product/report-panel.md`; a project session's terminal has the pane to itself.

## The console → project → session menu

The left-hand tree has three levels: console → project → session. Console and project rows expand and collapse;
collapse state is per window and is not stored.

Under each project, sessions whose process is running and sessions that were interrupted are listed directly;
archived sessions are grouped under an "Archive (n)" row that expands on its own. The console itself
carries an "Archived hubs (n)" group below its Hub row, for the hub sessions that have been archived.
Each session row shows its status, its title, and a badge naming its agent (Claude Code, Codex, Grok
Build).

Below the tree, a collapsible "Trusted folders (n)" block appears while at least one folder is trusted
for Claude Code's workspace-trust prompt; see "Trusted folders" in `docs/product/launching-agents.md`.

Selecting a session shows its terminal. Clicking a row deliberately does not move keyboard focus away from the
terminal; a row reached with Tab can be activated with Enter or Space.

A session is selected automatically only when this application is the one that opened it — through the Hub row or the
new-session dialog. A session that appears any other way, such as one the hub started or one another client of the
daemon opened, is added to the tree unselected and is put on screen by the user selecting it.

## Opening a session

A session is opened under a project with:

- **Agent** — defaulted as below, overridable for this session only.
- **Title** (optional) — defaults to the project's name. A hub session's title defaults to "Hub".
- **Initial task** (optional) — handed to the agent as its initial prompt.
- **Include in hub** (a checkbox, off by default) — makes this session report its results to the
  console's hub instead of staying outside the orchestration. The choice is fixed for the session's
  lifetime. A session the hub itself starts always reports to it; see "Which sessions the hub drives"
  in `docs/product/hub-orchestration.md`.

A session opened **with** a task starts in *working*. A session opened **without** one starts in *awaiting
instructions*: it is sitting at the agent's prompt.

A Claude Code session in a directory Claude Code has not been trusted with first stops on Claude
Code's own workspace-trust prompt, before it takes up its task or reaches its prompt. A resumed
session can stop on it as well; since a session left interrupted is relaunched only when it is
selected or resumed, that is when its prompt appears. Octoboard answers that prompt for the user once
they have agreed — for that project, or for a folder its directory lies under — or asks them first;
see "Claude Code's workspace-trust prompt" in `docs/product/launching-agents.md`.

If the launch itself fails — the directory cannot be reached, the agent binary is not on the user's `PATH`, the
user's shell environment could not be captured (see "The launch environment" in
`docs/product/launching-agents.md`), the config directory the session would hold for its agent no longer exists (see
"Agent config directories" in `docs/product/consoles-and-projects.md`) — no session appears in the tree and the
failure is reported.

### Which agent a session uses

In descending priority:

1. the agent chosen for this session when it was opened;
2. the project's default agent;
3. the console's default agent.

A hub session uses the console's hub agent instead.

A session's agent is fixed for its lifetime. Resuming or reopening a session always relaunches the same agent —
session records belong to a specific agent and cannot be moved across agents.

## Session statuses

| Status | Wire value | Meaning |
|---|---|---|
| Working | `working` | The agent is executing a turn. |
| Waiting for the user | `waiting_user` | The agent is waiting on a permission decision or has asked the user a question through its own ask-the-user tool. |
| Awaiting instructions | `idle` | The process is running and sitting at its prompt. |
| Interrupted | `interrupted` | No process is running, and it did not end by being archived. The session stays in its project's list (a hub, in its console's Hub row) and can be resumed. |
| Archived | `archived` | Ended by being archived (see "Archiving, interruption and resuming"). Listed in the project's Archive group (a hub, in its console's "Archived hubs" group) and can be reopened. |

The first three mean a process is running; the last two mean none is, and both can be resumed.

Transitions:

- Opening a session puts it in *working* or *awaiting instructions*, per the initial task.
- While the process runs, reports from the agent move the session between *working*, *waiting for the user* and
  *awaiting instructions*.
- The process ending for any reason other than archiving — the agent exiting on its own, a crash, the application
  quitting — leaves the session *interrupted*.
- Archiving leaves the session *archived*.
- Resuming an *interrupted* or *archived* session puts it in *awaiting instructions*, whichever of the two it came
  from. A resume that fails to launch leaves the session exactly where it was, archived included.
- An agent's report never moves a session out of *interrupted* or *archived*: whether a stopped session is one or the
  other is Octoboard's own record, not the agent's.

### What the statuses are derived from

Status comes from hook events Octoboard injects into each agent per launch, never from reading the terminal's rendered
output. There is one exception, and only one: while a **Claude Code** session is *waiting for the user*, Octoboard also
reads that session's own transcript file — the machine-readable record the agent keeps of the conversation, still not
its rendered output — because a declined prompt is reported by no hook event at all; see "Declining a Claude Code
prompt or question" below. A session's end is not taken from a hook either: the process is observed directly.

Where an agent reports nothing, the status simply stays at its last reported value. The known cases, which are
limitations of what the agents expose rather than of this one:

- A turn that ends with the agent asking a question **as plain prose** is indistinguishable from a finished turn on
  all three agents, so such a session reads as *awaiting instructions* rather than waiting for the user.
- When the user cancels an in-flight turn in Claude Code, nothing is reported at all; a session that was *working*
  keeps reading as working until the next prompt is submitted. A session that was *waiting for the user* falls into
  the same silence, but there the hand would be left up with nothing to lower it, so that one case is recovered
  from the transcript instead — see "Declining a Claude Code prompt or question" below.
- Claude Code reports nothing while it is on its workspace-trust prompt, so a session sitting there keeps the status its
  launch gave it — *working* or *awaiting instructions* — and raises no hand, although it is waiting for a person.
- Grok Build's bash mode (`!`) fires no tool or turn events, so work done through it never shows in the status; Grok's
  turn-end backstop was seen to follow about a minute later in one run (see "When a session does not report" in
  `docs/product/hub-orchestration.md`), but it finds the session already awaiting instructions.
- A question the agent asks through **its own ask-the-user tool** is reported by Claude Code and Grok
  Build but not by Codex, which has no such event; a Codex session asking that way reads as *working*.

Part of the Grok Build case above is Octoboard's own attribution rule rather than a limit of what the agent exposes.
Grok Build's turn-end backstop carries no turn id, so the daemon attributes it by clock. An ending that names its own
turn, or a cancellation, arms one expected backstop (a flag, not a count), and the next clock-attributed signal spends
it whether or not a turn is open. When an ending's own backstop never arrives, the flag stays armed, so a later turn
whose only ending is its backstop would have that backstop discarded: the session would keep reading as working and,
if it reports to the hub, no report would be synthesized for that turn. Whether Grok produces that sequence in
practice was not checked (in one run an ending followed within a second by a new prompt got no backstop).

### The raised hand

*Waiting for the user* is the raised hand, and it is made findable rather than left on the session's
own row:

- The session's row shows a raised-hand icon, and so do its project row and its console row, so a
  waiting session can be found with the tree collapsed.
- A system notification fires once as a session enters that state, naming the session by its title
  and the project it runs in — or the console whose hub it is. A session that is answered and later
  waits again notifies again. No notification permission is asked for: the application posts the
  notification directly, and macOS showed no permission prompt. The banner was seen while Octoboard
  was not the frontmost application; while it was frontmost, no banner was seen. A notification that
  cannot be shown is not reported as having failed — the tree's own marker carries the same signal.
- The Dock badge carries how many sessions are waiting, counted across every console, and clears
  when none is.

**The user answers in the session's terminal**, and the status leaves *waiting for the user* on the
agent's next event — or, where the answer was a decline and no event follows, on the decline showing
up in the agent's own record of the conversation, which only Claude Code sessions are read for (see
"Declining a Claude Code prompt or question" below). Nobody can answer for them: the hub is told to
leave such a session alone, and a message addressed to it is held until the user is done — see
"Messages held until a session can take them" in `docs/product/hub-orchestration.md`.

Where the user's own Codex configuration **resolves approval requests by itself**, Octoboard raises
no hand at all: the permission event still fires, but Codex resolves the request, no dialog ever
reaches the user and the tool proceeds — a hand there would ask them to answer something they never
see. Those sessions keep reading as *working*.

### Declining a Claude Code prompt or question

A Claude Code session with its hand up that is then **declined** — the permission prompt answered No
or dismissed with Esc, or the agent's own ask-the-user question cancelled — reports nothing at all:
no hook event of any kind follows, while the agent is already back at an empty prompt. So for this
one case Octoboard reads the session's own transcript file, the record Claude Code keeps of the
conversation, and moves the session to *awaiting instructions* once the decline appears there,
normally within about a second of the user answering. Both ways a Claude Code session raises its hand
are covered: a permission prompt and its own ask-the-user question.

What follows from that move is nothing special to this path: the raised hand comes down on the
session row, its project row and its console row, the Dock badge count drops, and a message queued
for the session while its hand was up is released (see "Messages held until a session can take them"
in `docs/product/hub-orchestration.md`). A later prompt or question in the same session raises the
hand and notifies afresh, as any other does.

- The agent prints that the turn was interrupted, but its process is still running, so the session is
  *awaiting instructions* and not *interrupted*.
- No turn end was reported, so no report is synthesised for the hub for that turn either (see "When a
  session does not report" in `docs/product/hub-orchestration.md`).
- **Claude Code sessions only.** Codex and Grok Build report a decline through their own hook events,
  and the transcript read here is Claude Code's own format; neither is watched this way.
- Only a hand that is currently up is recovered. A cancelled in-flight turn in a session that reads
  as *working* is still reported by nothing — see the limitation list in "What the statuses are
  derived from" above.
- Where an ask-the-user question is left not by declining it but by asking to chat about it instead,
  the turn in fact carries on; Octoboard reads that as a decline too, so the session reads as
  *awaiting instructions* for a moment until the agent's next event puts it back to *working*.
- If the record cannot be read — the pending-decision event named no transcript file, or the file is
  unreadable — nothing lowers the hand, and the session keeps it up until something else moves its
  status.

**What this costs to keep working.** The transcript is a file format Claude Code owns and rewrites on
upgrade, and a decline is recognised by two fixed marker strings inside it; the behaviour was
measured against Claude Code 2.1.274 and 2.1.286, and the hand was seen coming down in the running
application against 2.1.289. A release that renames those markers breaks this
silently and completely — there is no error, nothing is reported as having failed, and the only
symptom is a declined prompt leaving the raised hand up with nothing to lower it. Nothing Octoboard
can observe by itself tells that apart from a user who simply has not answered yet.

## Archiving, interruption and resuming

**Archiving** ends the agent's process and keeps the session and its record. The agent is asked to
exit first and is killed only if it does not; a kill takes the agent's tool subprocesses with it.
Archiving is available for any session that is not already archived, including an interrupted one;
for the hub it is on the Hub row's menu (see "Hub sessions and project sessions" above). The user
archives a session from its row's menu, and is asked to confirm first.

Besides the user, two things archive a session: the hub, explicitly, and a project session's own
report saying the work is finished with nothing left open (see "Automatic archiving" in
`docs/product/hub-orchestration.md`).

**Resuming** happens by selecting an interrupted or archived session, or through its "Resume" action. It relaunches
the same agent in the same directory and reassembles everything Octoboard injects.

- Where the agent has a stored conversation, that conversation is resumed.
- A session that **nobody ever typed into** has no stored conversation on the agent's side; resuming it opens a fresh
  conversation in its place, in the same project and under the same session.
- A resume carries no initial task, so the session comes up at the agent's prompt.
- A session relaunches with the config directory it was opened with for its agent, not the console's current setting,
  and is refused if that directory no longer exists (see "Agent config directories" in
  `docs/product/consoles-and-projects.md`).
- Resuming a session whose process is already running is refused. The refusal a double-click produces is not surfaced
  to the user.

An interrupted session and an archived session are resumed the same way; the difference is only how the session got
there, and where it appears in the tree.

## Renaming a session

A session's title can be changed at any time, archived sessions included. An empty title is rejected.

## The terminal

The selected session's terminal is live: keystrokes go straight to the agent, exactly as in a system terminal. Mouse
reporting works, so an agent's own mouse-driven TUI is usable. `Ctrl+C` reaches the agent.

- Attaching to a session replays the **most recent 2 MiB** of that session's output, then follows live output. The
  terminal itself keeps 10,000 lines of scrollback.
- A reattach always redraws from the replay rather than appending to what is on screen, so scrollback older than the
  replay window is lost on every reconnect.
- Resizing the window resizes the agent's terminal. The window has a minimum size, sized so that the
  sidebar, the terminal and the report panel all stay usable side by side — see "The window's minimum
  size" in `docs/product/window-layout.md`.
- The terminal's colours follow the window's light or dark appearance — see "What follows the choice"
  in `docs/product/appearance.md`.
- A client that stops draining output for more than a few seconds is dropped by the daemon rather than letting output
  buffer without bound. The application then reconnects by itself, with a backoff, up to five times; while a running
  session's terminal is disconnected a "Reconnect" button is available as well. A reconnect in the background never
  steals keyboard focus.
- Selecting an interrupted or archived session resumes it rather than attaching; until a process is running, the
  terminal reads "Not running" and offers "Resume".
- **A session's last output stays on screen once its process has ended**, under that "Not running" status and beside
  "Resume", so what the agent printed last — why it stopped, what it was waiting for — can still be read. The kept
  screen belongs to that one session: selecting a different dormant session clears the screen rather than showing the
  previous session's output, and resuming re-attaches and redraws from the daemon's replay rather than appending to
  what was kept.
