# Sessions

A **session** is one agent CLI process, running in a console's or a project's working directory and rendered in a
terminal the user types into directly. Octoboard is not an agent and not a terminal of its own: it starts the agent's
own binary and shows it as it is. What it adds to each launch, and what it guarantees it does not change, is in
`docs/product/launching-agents.md`.

## Console sessions and project sessions

| | Working directory | Where it appears |
|---|---|---|
| Console session | the console's working directory | the console sessions section |
| Project session | the project's directory | under that project |

**A console may hold any number of console sessions running at once.** Opening, resuming or reopening
one never checks what else is running in the console, and the sidebar lists every one of them that is
not archived, each as its own row, in the console sessions section above the project list (see "The
console sessions section and the project list" in `docs/product/sidebar.md`).

A console session's row follows a session row exactly — the same status glyph, its agent's icon, its
title, a pin button when pinned, and selecting it resumes it when it is interrupted, as selecting an
interrupted session's row does. Its action menu offers **Pin**/**Unpin**, **Rename**, **Focus mode** (see
`docs/product/focus-mode.md`), **Switch account** and **Archive** — **Delete** in its place while the console session
has never been used (see "Deleting an unused session" below); there is no Resume item, for the same reason. A
console session's title is renameable, like a project session's; its default is under **Name** in "Opening a session"
below.

The console session cannot archive itself (see "The console session's tools" in
`docs/product/hub-orchestration.md`), so the user archives it from its own row's menu, as any session
is archived. Archiving it moves it among the console's archived console sessions; these are not reached
from any one row, but from the console's own menu, which offers **Archived console sessions** — a
submenu of the console's newest archived console sessions, each of which can be selected, and the way
to the archive view of all of them (see "The console header" in `docs/product/sidebar.md`). The console sessions
section's own **New console session** button opens the session dialog described in "Opening a session" below — whereas resuming or reopening a console session keeps its
recorded agent and account; only switching accounts changes its account (see "Agent config directories" in
`docs/product/consoles-and-projects.md` and "Switching a session's account" below).

A console session carries no project; it runs in the console's working directory. It is given
Octoboard's orchestration tools and dispatches work to sessions in the console's projects — see
`docs/product/hub-orchestration.md`. Because it belongs to no project, archived console sessions
belong to the console rather than to any project.

A project session opened bound to a console session carries that console session's **binding badge**
wherever it is listed, and the console session's own row shows the same colour (see "The binding
badge" in `docs/product/sidebar.md`).

An **unbound** project session — one the user opened by hand without choosing a console session under **Report to**
(see "Opening a session" below) — is given a narrower set of the orchestration tools, scoped to its own project: it can
start sessions there, which are bound to it and report to it, and drive them (see "The unbound project session's
tools" in `docs/product/hub-orchestration.md`). A session it starts carries no binding badge and cannot start sessions
of its own. A session's **owner** is the session it is bound to: a console session, or the project session that started
it (an unbound session's own owner is the user). A project session opened unbound can later become bound to a console
session while keeping the sessions it started, which makes it a **lead session**: it asks for a console session, and
once the user approves, Octoboard starts one and binds it there (see `docs/product/requesting-a-console-session.md`);
orchestration is at most two levels deep (see "Lead sessions" in `docs/product/hub-orchestration.md`). Every project session, bound or not, can also list the other
running sessions of its project and share information with them; it takes work only from its owner, and what comes from
another session is information to weigh (see "Information between sessions of a project" in
`docs/product/hub-orchestration.md`).

Selecting a session also fills the window's right pane, beside the terminal: with a console session's own **report
panel** (see `docs/product/report-panel.md`), or with a project session's project's **project pane**, its files (see
`docs/product/project-pane.md`). When the right pane shows something else is in "What the right pane shows" in
`docs/product/window-layout.md`.

## Where sessions are listed

Sessions are listed in the sidebar, one console at a time: its console sessions in the console
sessions section, a project session under its project. Archived sessions are not listed among the
others; they are reached from their project's or, for a console session, the section's own action
menu, and from the archive view. Everything about the sidebar — its rows and menus, the order sessions
are listed in, pinning, the archive view and how selecting a session works — is in
`docs/product/sidebar.md`; focus mode is in `docs/product/focus-mode.md`.

## Opening a session

New console session from the console sessions section, including its empty row, and the terminal's offer of the
same action open a dialog before anything is launched. New session under a project opens the same dialog for that
project. Both offer these fields in this order:

- **Name** (optional) — the empty field shows the project's name for a project session, or the console's name for a
  console session, as its placeholder. Leaving it empty uses that parent's current name when the session is created,
  with no numeric suffix; multiple sessions can have the same name. The field stays empty until the user enters a
  custom name. The dialog removes surrounding whitespace from an entered name and uses the parent name when nothing
  remains. Renaming a console changes the default for future console sessions; existing sessions keep their created
  names.
- **Agent** — an agent picker joined to an account picker, as in the console and project dialogs. The agent picker
  lists all three agents with their icons and names. An agent Octoboard has determined unavailable (its binary does
  not resolve on the user's login shell `PATH`) is marked as not installed and cannot be selected; before that
  determination has landed, each agent remains selectable. The field opens on a selectable agent whenever there is
  one, starting from the default resolved in "Which agent a session uses" below.
  The account picker offers only that agent's accounts, with its default account first, and is omitted when the agent
  has no named accounts. It opens on the account resolved from the project and console for that agent (see "Which
  account a session uses" below); a console session starts from its console's account for the selected agent.
  There is no inheritance entry: both pickers settle concrete choices for this session.
  Switching agents retains each agent's account choice separately. If a remembered account is removed while the dialog
  is open, that agent stays selected and its account falls back to its project's and console's current resolution,
  or its console's alone for a console session.
  Switching agents returns focus to the Agent picker even when the account picker appears or disappears. If the last
  named account is removed while focus is on the account picker or one of its open options, focus returns to Agent.
- **Report to** (project sessions only; a choice, "No console session" by default) — one of the console's console
  sessions that are not archived, the most recently started first, each shown beside its colour, the same one the session's binding badge
  will carry (see "The binding badge" in `docs/product/sidebar.md`), or none. A chosen console session receives this
  session's reports, instead of the session staying outside the orchestration. A console with no console session that
  is not archived shows no choice at all, and the session is unbound. A binding, once set, is never changed or undone.
  A session the console session itself starts is always bound to it, and one an unbound project session
  starts is bound to that project session; see "Which sessions an owner drives" in
  `docs/product/hub-orchestration.md`. The choice is offered from the project list and the terminal's **New session** offer outside focus
  mode. Opened from a project's focus mode the dialog has no such field and the session is always unbound; opened from a console session's focus mode it
  has none either, and shows a line saying the session reports to that console session instead, which is the binding
  it gets (see `docs/product/focus-mode.md`).

After these fields, the collapsed **Advanced** section offers the model and reasoning effort for this session. It
initially shows the selected agent's project defaults, or the console's console-session defaults for a console
session; untouched values follow that parent, and changes apply only to this session, including when resumed.
The choices, clearing and account-specific lists are described in "The Advanced section" in `docs/product/model-and-reasoning-effort.md`.

A console session has no project or Report to field and is never bound to another session. Opening it with the daemon
protocol's `open_session` accepts an explicit agent and account as the dialog does. Its `title` is stored trimmed, and
one that is absent, `null` or blank uses the console's current name, as the dialog's empty name does.

The dialog takes no task: a session the user opens by hand starts in *awaiting instructions*, sitting at the agent's
prompt, and is given its work by typing into its terminal. Only a session started with `start_session` — by a console
session or a project session — is handed an opening prompt, the brief it is started with (see "Handing out a
task: the brief" in `docs/product/hub-orchestration.md`); it starts in *working*.

A session in a folder its agent has not been told to trust can first stop on that agent's own trust confirmation,
before it takes up its task or reaches its prompt. A resumed session can stop on it as well; since a session left
interrupted is relaunched only when it is selected or resumed, that is when its confirmation appears. Octoboard presses
that confirmation for the user once they have given permission — for that project, or for a folder its directory lies
under, whichever agent asked — or asks them first; see `docs/product/folder-trust.md`.

If the launch itself fails — the directory cannot be reached, the agent binary is not on the user's `PATH`, the user's
shell environment could not be captured (see "The launch environment" in `docs/product/launching-agents.md`), or, for
Grok Build only, the account the session would hold pins a directory that is not an initialized Grok home (see "Agent
config directories" in `docs/product/consoles-and-projects.md`) — no session appears in the sidebar and the failure is
reported. A missing config directory does not fail a brand new session for the other two agents: it has no conversation
to lose, so Claude Code creates the directory itself and Octoboard creates it for Codex.

**With every agent determined unavailable, opening a session is refused outright, before any of that**: the dialog
cannot be submitted, the console sessions section's own new-session action is disabled, and a console session's
`start_session` tool is refused with a reason it can report (see "The console session's tools" in
`docs/product/hub-orchestration.md`). The dialog and the tool's refusal say which agents Octoboard supports and that one
of them has to be on the user's `PATH`, without saying how to install one; the console sessions section, with room for
less, says only that no agent was found on the user's `PATH` (see "The console sessions section and the project list" in
`docs/product/sidebar.md`). This refusal only ever fires once availability has actually been determined and found none;
while that determination is still pending, nothing is refused on its account and a session opens or fails exactly as
described above.

### Which agent a session uses

In descending priority:

1. the agent chosen for this session when it was opened;
2. the project's default agent;
3. the console's default agent.

A console session has no project, so it uses the agent chosen for that session, or the console's default agent when
none is given. Console sessions of different agents can run alongside each other in the same console. Their
instruction files are described in "The console session's instruction file" in `docs/product/launching-agents.md`.

A session's agent is fixed for its lifetime. Resuming or reopening a session always relaunches the same agent —
session records belong to a specific agent and cannot be moved across agents.

### Which account a session uses

The account of the session's agent, in descending priority:

1. the account chosen for this session when it was opened;
2. the project's account for that agent, where the project does not leave it to its console — a project set to **the
   agent's default account** ends the chain there, whatever the console names;
3. the console's account for that agent;
4. that agent's default account.

A console session carries no project, so after its own account choice its chain goes straight to the console's
reference. A project leaves the account to the console for every agent it inherits for, which is how every project starts (see "The project's account, model and
reasoning effort" in `docs/product/consoles-and-projects.md`). A session opened without changing the account picker
therefore runs under the project's account for its agent, or the console's, or the agent's default account when
neither pins anything. The session's account is identified by its agent icon's tooltip and its accessible session
name, without an account suffix in visible session metadata (see "Session rows" in `docs/product/sidebar.md`).

A session that makes no choice of its own — one a console session or a project session starts with its own
`start_session` tool — takes the project's setting. The dialog that opens a session by hand always makes a choice,
opening on the account the rest of the chain resolves to (the project's setting for the agent, else the console's, else
the agent's default account) and sending that as the session's own choice, so a session opened there runs under that
account unless the user picks another in the account picker.

### Which model and reasoning effort a session uses

A session uses its own model and reasoning effort choices, each independently, or inherits the current defaults for
its agent from its project (its console for a console session). The inheritance, explicit clearing and persistence
rules are in `docs/product/model-and-reasoning-effort.md`. How the resolved values reach the agent is in "The model
and reasoning effort" in `docs/product/launching-agents.md`.

## Session statuses

| Status | Wire value | Glyph | Meaning |
|---|---|---|---|
| Working | `working` | an accent-coloured dot pulsing a fading copy of itself outward | The agent is executing a turn. |
| Waiting for the user | `waiting_user` | a raised hand that waves now and then | The agent is waiting on a permission decision or has asked the user a question through its own ask-the-user tool; or Octoboard is asking the user on the agent's behalf while the agent's call waits, which is a request for a console session (see "The call" in `docs/product/requesting-a-console-session.md`). |
| Awaiting instructions | `idle` | a green speech bubble | The process is running and sitting at its prompt. |
| Interrupted | `interrupted` | a pause sign | No process is running, and it did not end by being archived. The session stays in its project's list (a console session, in the console sessions section) and can be resumed. |
| Archived | `archived` | an archive box | Ended by being archived (see "Archiving, interruption and resuming"). No longer listed among its project's sessions (a console session, no longer in the console sessions section); reached through the archive (see "Archived sessions" below) and can be reopened. |

The first three mean a process is running; the last two mean none is, and both can be resumed.

The glyph stands for the status wherever one is shown — the sidebar's rows, focus mode's cards, the top bar's
breadcrumb and a confirmation's list of the sessions it affects. In a confirmation, archived sessions omit the
status glyph; the warning identifies them as archived. The waving hand and the outward pulse stand still
where the system asks for reduced motion, leaving the working glyph a plain accent-coloured dot; working and awaiting
instructions still differ by shape as well as by colour.

**How the status reaches a user who does not read the glyph** differs by placement:

- On a **sidebar row** and on a **focus mode's card** the glyph adds nothing of its own for assistive technology: the
  row's own accessible name says the status, and a card shows it in words beside the glyph.
- On the **top bar's breadcrumb** and in a **confirmation's list of the sessions it affects** nothing around the glyph
  says the status, so the glyph itself is named after it.
- Every placement where no status text sits beside the glyph gives it a **tooltip** saying the status in the same
  words — the **sidebar's rows**, the **top bar's breadcrumb** and a **confirmation's list** (see "Tooltips on
  indicators" in `docs/product/labels-and-tooltips.md`). Focus mode's cards say the status in words already and
  carry none.

Transitions:

- Opening a session puts it in *working* when it is handed an opening prompt (a session started with `start_session`),
  and in *awaiting instructions* otherwise (see "Opening a session").
- While the process runs, reports from the agent move the session between *working*, *waiting for the user* and
  *awaiting instructions* — except while a request for a console session holds it at *waiting for the user* (see
  "What the statuses are derived from" below).
- The process ending for any reason other than archiving — the agent exiting on its own, a crash, the application
  quitting — leaves the session *interrupted*.
- Archiving leaves the session *archived*. Archiving an owner also archives the *interrupted* sessions under it, and
  is refused while any session under it has a process running (see "Archiving, interruption and resuming").
- Resuming an *interrupted* or *archived* session puts it in *awaiting instructions*, whichever of the two it came
  from. A resume that fails to launch leaves the session exactly where it was, archived included. Resuming a session
  whose owner is *archived* first does the same to its archived owners, the topmost first.
- Switching a session to another account of its agent ends and relaunches its process without archiving it: it reads
  as *interrupted* while the process is down and comes back in *awaiting instructions*, and a switch that fails leaves
  it *interrupted* on the account it had (see "Switching a session's account").
- An agent's report never moves a session out of *interrupted* or *archived*: whether a stopped session is one or the
  other is Octoboard's own record, not the agent's.

### What the statuses are derived from

Status comes from hook events Octoboard injects into each agent per launch, never from reading the terminal's rendered
output. Three things besides those events bear on a running session's status, and only these three.

- While a **Claude Code** session is *waiting for the user*, Octoboard also reads that session's own transcript file —
  the machine-readable record the agent keeps of the conversation, still not its rendered output — because a declined
  prompt is reported by no hook event at all; see "Declining a Claude Code prompt or question" below.
- While a **Codex** session has a turn open, Octoboard also reads that session's own rollout file — the record Codex
  keeps of the session, again not its rendered output — because a turn that ends in an error is reported by no hook
  event at all; see "A Codex turn that ends in an error" below.
- A `request_console_session` call waiting for the user's answer holds its session at *waiting for the user* for the
  whole wait, whatever the agent's own events say meanwhile; when the request ends the session goes back to where its
  agent was last seen to be (see "The call" in `docs/product/requesting-a-console-session.md`).

A session's end is not taken from a hook either: the process is observed directly.

Where an agent reports nothing, the status simply stays at its last reported value. The known cases, which are
limitations of what the agents expose rather than of this one:

- A turn that ends with the agent asking a question **as plain prose** is indistinguishable from a finished turn on
  all three agents, so such a session reads as *awaiting instructions* rather than waiting for the user.
- When the user cancels an in-flight turn in Claude Code, nothing is reported at all; a session that was *working*
  keeps reading as working until the next prompt is submitted. A session that was *waiting for the user* falls into
  the same silence, but there the hand would be left up with nothing to lower it, so that one case is recovered
  from the transcript instead — see "Declining a Claude Code prompt or question" below.
- No agent reports anything while it is on its own folder-trust confirmation, so a session sitting there keeps the
  status its launch gave it — *working* or *awaiting instructions* — and raises no hand, although it is waiting for a
  person.
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
if it is bound to an owner, no report would be synthesized for that turn. Whether Grok produces that sequence in
practice was not checked (in one run an ending followed within a second by a new prompt got no backstop).

### The raised hand

*Waiting for the user* is the raised hand, and it is made findable rather than left on the session's
own row:

- The session's row shows the raised hand. Its project row shows one while the project is collapsed,
  and its console's avatar on the rail carries one, the rail showing every console at once, so a
  waiting session can be found whatever the sidebar is showing (see "The console switcher" and
  "Project rows" in `docs/product/sidebar.md`). In a console session's focus mode, a console session's
  chip in the switch strip shows one too while it or a session bound to it is waiting (see "The switch
  strip" in `docs/product/focus-mode.md`).
- A system notification fires once as a session enters that state, naming the session by its title
  and the project it runs in — or the console whose console session it is. A session that is answered and later
  waits again notifies again. No notification permission is asked for: the application posts the
  notification directly, and macOS showed no permission prompt. The banner was seen while Octoboard
  was not the frontmost application; while it was frontmost, no banner was seen. A notification that
  cannot be shown is not reported as having failed — the sidebar's own marker carries the same signal.
- The Dock badge carries how many sessions are waiting, counted across every console, and clears
  when none is. While the window is closed into the background Octoboard has no Dock icon; the count
  is still kept and shows once it returns to the Dock (see "Closing the window" in
  `docs/product/application-lifecycle.md`). The menu bar icon's menu lists the waiting sessions too
  (see "The icon's menu" in `docs/product/menu-bar-icon.md`).
- The rail carries the same count, as a raised hand and the number, shown only while at least one
  session is waiting. Pressing it selects the next waiting session after the selected one — console by
  console in the order the consoles were created, its console sessions before its projects' sessions,
  and both in the sidebar's order (see "Order of projects and sessions" in `docs/product/sidebar.md`) —
  and wraps from the last back to the first; when the selected session is not waiting, it selects the
  first. The sidebar follows the session it selects (see "Selecting a session" in
  `docs/product/sidebar.md`).

**Where the hand is the agent's own, the user answers in the session's terminal**, and the status leaves *waiting for
the user* on the agent's next event — or, where the answer was a decline and no event follows, on the decline showing
up in the agent's own record of the conversation, which only Claude Code sessions are read for (see "Declining a
Claude Code prompt or question" below). Nobody can answer for them: a session driving others — a console session, or a
project session that started sessions — is told to leave such a session alone, and a message addressed to it is held
until the user is done — see "Messages held until a session can take them" in `docs/product/hub-orchestration.md`.

**A granted permission prompt lowers the hand only once the call it let through has finished.** The user answering *yes*
is itself reported by no event, so the next event the session produces is normally the end of that tool call — the hand
stays up from the moment the prompt appeared until the call is done, which is a moment for a file write and minutes for
a build or a test run. For that whole time the session reads as *waiting for the user* everywhere the hand is shown, it
counts in the rail's waiting count and the Dock badge, and a message addressed to it goes on being held. There is no
earlier signal to read: the delay is in what the agents report, not in how often Octoboard looks. Measured on Codex
0.160.0 and on Claude Code 2.1.274; Grok Build was not measured. A prompt the user *declines* is not delayed this way:
the decline ends the turn, and the hand comes down right after the answer.

A hand Octoboard raised for a waiting request for a console session is answered in that request's dialog instead, so a
message addressed to such a session is not held (see "The call" in `docs/product/requesting-a-console-session.md`).

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

What follows from that move is nothing special to this path: the raised hand comes down wherever the
sidebar showed it, the Dock badge count drops, and a message queued
for the session while its hand was up is released (see "Messages held until a session can take them"
in `docs/product/hub-orchestration.md`). A later prompt or question in the same session raises the
hand and notifies afresh, as any other does.

- The agent prints that the turn was interrupted, but its process is still running, so the session is
  *awaiting instructions* and not *interrupted*.
- No turn end was reported, so no report is synthesised for the session's owner for that turn either (see "When a
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
- A decline is noticed the same way while a request for a console session holds that session's hand
  up (see "The call" in `docs/product/requesting-a-console-session.md`). The hand stays up until the
  request ends, since it is the request's own; the session is then *awaiting instructions*, where the
  decline left its agent.

**What this costs to keep working.** The transcript is a file format Claude Code owns and rewrites on
upgrade, and a decline is recognised by two fixed marker strings inside it; the behaviour was
measured against Claude Code 2.1.274 and 2.1.286, and the hand was seen coming down in the running
application against 2.1.289. A release that renames those markers breaks this
silently and completely — there is no error, nothing is reported as having failed, and the only
symptom is a declined prompt leaving the raised hand up with nothing to lower it. Nothing Octoboard
can observe by itself tells that apart from a user who simply has not answered yet.

### A Codex turn that ends in an error

A Codex turn that **ends in an error** — the account's usage limit reached, for one — is reported by
no hook event at all: Codex has no failure event and no notification event, so nothing follows the
turn's prompt submission, while the agent is already back at its prompt. So for this one case
Octoboard reads the session's own rollout file, the record Codex keeps of the session, and moves the
session to *awaiting instructions* once the turn appears there as ended with an error — normally
within about a second of the prompt, and so up to about half a second behind Codex's own return to
its prompt. Every turn of a session is covered, its first included.

- The agent's process is still running, so the session is *awaiting instructions* and not
  *interrupted*.
- The turn is reported on: a session bound to an owner has a report synthesised for it with status
  *failed*, carrying Codex's own error text and the usual line saying the session stopped without
  reporting (see "When a session does not report" in `docs/product/hub-orchestration.md`).
- **Codex sessions only.** Claude Code and Grok Build report a failed turn through their own hook
  events, and the file read here is Codex's own format; neither is watched this way.
- A turn Codex ends normally, and one the user cancels, are reported by Codex's own hook events, and
  nothing is read from the rollout for them.
- If the record cannot be read — the prompt submission named no rollout file or no turn, or the
  turn's end never appears there — nothing moves the session, and it keeps reading *working* until
  something else does.

**What this costs to keep working.** The rollout is a file format Codex owns and rewrites on upgrade,
and a failed turn is recognised by the shape of the record Codex writes for the turn's end; the
behaviour was measured against Codex 0.160.0. A release that changes that shape breaks this silently
— there is no error and nothing is reported as having failed — and the only symptom is a session
reading *working* while Codex sits at its prompt.

## Archiving, interruption and resuming

**Archiving** ends the agent's process and keeps the session and its record. The agent is asked to
exit first and is killed only if it does not; a kill takes the agent's tool subprocesses with it.
Archiving is available for any session that is not already archived, including an interrupted one;
a console session is archived from its own row's menu, like any other (see "Console sessions and
project sessions" above). The user archives a session from its row's menu. A session at its prompt or
interrupted, with no unarchived session under it, is archived at once, since nothing is cut short; one at work,
waiting for the user, being resumed, or with an unarchived session under it asks for confirmation first. A session that has never been used offers **Delete** there instead (see "Deleting an unused session" below).
An archived session keeps its pin, if it had one (see "Order of projects and sessions" in `docs/product/sidebar.md`).

Besides the user, two things archive a session: its owner, explicitly, and a project session's own report saying the
work is finished with nothing left open (see "Automatic archiving" in `docs/product/hub-orchestration.md`). Both
archive that one project session and nothing else, except that an owner archiving a lead session archives it as an
owner, under the rules below.

**Archiving an owner — a console session, or a project session that has started sessions — goes along its bindings,
and is decided by whether a process is running — not by the status.** It reaches every session **under** it: the
sessions bound to it and, for a console session, the sessions bound to a lead session bound to it (see "Lead sessions"
in `docs/product/hub-orchestration.md`). An interrupted session has no process but is not archived, and a session at
its prompt, at work or waiting for the user does have one.

- **While any session under it has a process running, archiving the owner is refused**, whatever that session's
  status and at whichever level, and nothing is changed. The refusal says how many sessions it is and names them; the
  user archives those sessions, or waits for them to finish, and tries again. A session being launched, resumed or
  switched at that moment counts as running.
- **With none running, archiving the owner archives the sessions under it too** — every one that is not archived yet,
  which can only be interrupted ones — so nothing under it is left outside the archive. The confirmation says how many
  will be archived with it.
- Sessions that are not under it, among them sessions bound to another session of the same console, are not touched.
- Ending an owner's process for a switch of its account is not archiving, and archives nothing (see "Switching a
  session's account" below).

**Resuming** an interrupted session happens by selecting it, or by pressing "Resume" on its terminal's card (see
"Interrupted, archived and dormant sessions" in `docs/product/session-terminal.md`, which also names the two ways of showing an interrupted session that do not resume it).
**Reopening** an archived session is the same relaunch, but selecting an archived session does not do it:
the session is shown, still archived, and is reopened by typing into its terminal, by pressing "Reopen" on its
terminal's card, or by pressing "Reopen" in the archive view (see "The archive view" in
`docs/product/sidebar.md`). Either relaunches the same agent in the same directory and reassembles everything
Octoboard injects.

- Where the agent has a stored conversation, that conversation is resumed.
- A session that **nobody ever typed into** has no stored conversation on the agent's side; resuming it opens a fresh
  conversation in its place, in the same project and under the same session.
- A resume carries no opening prompt, so the session comes up at the agent's prompt; an archived session reopened by
  typing into its terminal is then handed what was typed (see "Interrupted, archived and dormant sessions" in
  `docs/product/session-terminal.md`).
- A session relaunches with the account and the config directory it last recorded for its agent — the one it was
  opened with until the user switches it (see "Switching a session's account" below), not the console's current
  reference — and is refused if that directory no longer exists, but only when the session has a conversation to
  resume; one that was opened and never typed into launches into the missing directory instead (see "Agent config
  directories" in `docs/product/consoles-and-projects.md`).
- **Reopening a session whose owner is archived reopens its archived owners first, level by level from the topmost
  down**, then the session itself, so what the session reports reaches a running owner: a session bound to an archived
  lead session whose console session is archived too brings back the console session, then the lead session, then
  itself. The walk goes up through the owners that are archived and stops at the first that is not; an owner that is
  only interrupted is not relaunched by this. If an owner cannot be reopened, the whole reopen fails with its reason:
  the owners already brought back stay reopened, and the rest, the session included, stay as they were. Likewise, if
  the owners came back and it is the session's own relaunch that then fails, they stay reopened.
- **Reopening an owner on its own reopens nothing bound to it**, and reopening a session reopens none of the other
  sessions bound to the same owners. The group comes back one session at a time, from the session the user asked for.
- Resuming a session whose process is already running is refused. The refusal a double-click produces is not surfaced
  to the user.

An interrupted session and an archived session are relaunched the same way; the difference is how the session got
there, where it is listed, and that selecting an archived one does not relaunch it.

### Switching a session's account

A session can be moved to another account of its own agent from its row's **Switch account** submenu (see "Session
rows" in `docs/product/sidebar.md`); a console session is switched from its own row, among that session's own agent's
accounts. The submenu is offered only when the agent has more than one account, and the user is asked
to confirm. It is the user's action alone: Octoboard never switches a session on its own, and reads no sign that an
account's usage has run out.

The confirmation has an **unchecked by default** checkbox to switch every session of the same agent in that console
to the selected account. It includes console sessions, project sessions and bound sessions, across all projects in
the console; archived sessions and sessions already on the target account are excluded. The confirmation shows how
many sessions are eligible. Leaving the checkbox unchecked switches only the selected session. Neither choice
changes the console's or projects' account defaults for future sessions.

Confirming fixes the set of sessions to process and locks the checkbox. Sessions opened afterwards do not join that
set, including on a retry. Sessions are switched one at a time; one that has been removed, archived or moved to the
target account before its turn is skipped. A progress bar shows the number processed out of the fixed total, counting
failed and skipped sessions as processed, and the title of the session currently being switched is shown and
announced to assistive technology. A failure does not stop the remaining sessions: the dialog lists each failed
session and its reason, then offers Retry for those failures alone. Successful switches are kept and never repeated
by Retry. The confirmation closes when the pass finishes without failures.

A switch ends the session's process the way archiving does, copies the session's conversation into the target
account's config directory, records the new account and directory on the session, and relaunches it as a resume does,
with everything Octoboard injects reassembled. It archives nothing: while the process is down the session reads as
*interrupted*, and it comes back in *awaiting instructions* like any resumed session. Ending an owner's process this
way does not itself switch or archive the sessions bound to it; the console-wide choice switches each eligible bound
session separately.

- **The conversation continues** where the agent has one stored: the session's own record is copied — added to the
  target directory, or replacing an earlier copy there — and the original stays in the account it came from, so a
  failed switch loses nothing and switching back needs no second copy. A session nobody ever typed into has no
  conversation to copy; the switch records the account and relaunches into a fresh conversation, as a resume of such a
  session does. The default account is a target like any other: the directory it resolves to at that moment is where
  the conversation is copied, and the relaunch runs against that same resolution.
- **The session reads the target account's configuration and keeps its explicit model and reasoning effort choices.**
  An account's directory holds the agent's global configuration as well as its login. Settings kept only in the old
  account's directory do not follow the session; values explicitly supplied by Octoboard still apply according to
  `docs/product/model-and-reasoning-effort.md`. The same conversation can come up with different account defaults.
- **A switch that cannot be made leaves the session on the account it had, and says why.** It is refused, with nothing
  recorded, when the session is being launched or resumed at that moment, when it is archived, when it is on that
  account already, when the target is a Grok Build directory that is not an initialized Grok home, when the user's
  shell environment could not be captured — which is read whenever either side of the switch is the default account,
  since that is what resolves its directory (see "The launch environment" in `docs/product/launching-agents.md`) — and
  when the conversation record is not where it should be; the session's process is then left running. The one further
  refusal comes after the process has been asked to end: a process that has not gone within 30 seconds stops the
  switch, with nothing copied and nothing recorded, and that process may still be running. Once the process has been
  ended, a copy that does not complete, a relaunch the launch rules refuse and a relaunch whose process ends at once
  are each reported as a failed switch, not as a success: the session is left *interrupted* on its old account,
  resumable as before, and already reads so everywhere by the time the failure is shown. Octoboard tells that last
  case from a success by the process ending within a few seconds of the relaunch, never by reading what the agent
  printed. An account that is not signed in is therefore not a failed switch: every agent, Grok Build included, comes
  up on its own sign-in rather than exiting, so the switch succeeds and the session's terminal shows that agent's
  sign-in, which may start its sign-in flow.
- **It can take a few seconds**, the time the agent is given to exit and the time the relaunched process is watched; the
  confirmation stays open, its button reading "Switching account…" and the session's terminal "Resuming session…",
  until the result is known, and shows a failure in place. The confirmation cannot be dismissed meanwhile, and the
  session cannot be resumed or switched again.

### Archived sessions

A project's archived sessions, and a console's archived console sessions, stay until the user deletes them, the
project is removed or the console is deleted. They are listed most recently archived first, and reached:

- from the project's "View archive" submenu, or for console sessions the console's "Archived console
  sessions" submenu, with the newest five;
- from the project's focus mode, with the newest ten, bound sessions included; from a console session's focus mode,
  the archived sessions under it, again the newest ten;
- from the archive view, with all of them.

How each of those looks and behaves is in "Project rows" and "The archive view" in `docs/product/sidebar.md`, and in
`docs/product/focus-mode.md`.

### Deleting archived sessions

An archived session can be deleted: one at a time, or at once every archived session of a project, every
archived session under a console session, or every archived console session of a console. The user is asked to
confirm either way.

**Deleting an archived owner deletes the archived sessions bound to it, at both levels**: an archived lead session
among them takes its own archived sessions along, so deleting an archived console session or an archived lead session
leaves no archived session under it. Left behind, reopening one of them would have no owner to come back with, and its
binding would point at nothing. The confirmation says how many that is, counting both levels, before anything is
deleted. Deleting every archived console session of a console, and deleting every archived session under a console
session, take the archived sessions under them in the same way, and the confirmation says how many. A lead session that
is not archived keeps its own sessions. Deleting an archived bound session on its own leaves its owner alone, and
neither deleting a project's archived sessions nor deleting the archived sessions under a console session touches a
console session.

- **Deleting removes only Octoboard's record of the session**, for good, and the last output Octoboard kept for it. The
  session disappears from every client, and the orchestration tools' `list_archived` and `reopen_session` no longer
  find it. The agent's own record of the conversation, in the agent's own configuration directory, and the project's
  directory are never touched.
- Deleting a session that is neither archived nor unused (see "Deleting an unused session" below) is refused, saying
  that only an archived session, or one that has never been used, can be deleted, and not while it is starting. So is
  deleting an archived session that is being resumed at that moment. An archived session whose process is still on its
  way out can be deleted.
- Deleting everything at once skips a session that stopped being archived meanwhile, because a resume got there first,
  rather than failing.

Removing a project or deleting a console also deletes its archived sessions along with every other session record
(see "Removing a project" and "Deleting a console" in `docs/product/consoles-and-projects.md`).

### Deleting an unused session

A session that has **never been used** is deleted rather than archived: its action menu offers **Delete** in place of
**Archive** — on its sidebar row, on its card in a focus mode and, for a console session, in its focus mode's header.
A session is unused, whatever its status, while all of these hold:

- **It has had no turn yet.** The first prompt its agent reports as submitted makes it used for good, whether the user
  typed it or Octoboard wrote it in.
- **It is bound to no owner.** A session another session started, or one opened to report to a console session, keeps
  Archive even before its first prompt lands, since it reports to that owner.
- **No session is bound to it.** An owner with any session under it, in any status, keeps Archive.

Deleting one asks for the same confirmation as deleting an archived session, and removes the session's record exactly
as described in "Deleting archived sessions" above. Its process is stopped, and nothing is left in the archive. A trust
confirmation or console-session request of the session's that is still waiting is dropped with it.

- The check is made at the moment of the delete: a session that has become used by then — a prompt submitted at the
  same moment wins — is refused as in "Deleting archived sessions" above, and keeps its record and its process. So is a
  session being launched, resumed or switched to another account at that moment.
- An unused session can stop existing while a session is being bound to it. Opening a session bound to an owner that
  was deleted meanwhile is refused, with nothing launched; an approved console-session request whose new console
  session was deleted before the caller could be bound fails as described in "On approval" in
  `docs/product/requesting-a-console-session.md`.

## Renaming a session

A session's created name can be changed at any time, archived sessions included. The rename field contains that
name alone; an empty name is rejected. An agent's terminal title remains separate (see "Displayed names" in
`docs/product/session-titles.md`).

## The terminal

The selected session's terminal — what its area shows with no session selected, typing into it, its output and
scrollback, reconnecting and loading, a dormant session's saved preview, links in its output, and what it offers while
a console has nothing in it — is described in `docs/product/session-terminal.md`.
