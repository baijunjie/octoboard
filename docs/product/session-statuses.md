# Session statuses

| Status | Wire value | Glyph | Meaning |
|---|---|---|---|
| Starting | `starting` | a muted arc turning | A newly opened session — from the dialog, by an agent's `new_session`, or by an approved session request — whose agent is still being launched; it has no process yet. It becomes *awaiting instructions*, or *working* for a session started with a brief, once the process is up, and is removed again if the launch fails (see "Starting the session" in `docs/product/opening-sessions.md`). |
| Working | `working` | an accent-coloured dot pulsing a fading copy of itself outward | The agent is executing a turn. |
| Waiting for the user | `waiting_user` | a raised hand that waves now and then | The agent is waiting on a permission decision, including its folder-trust confirmation, or has asked the user a question through its own ask-the-user tool; or Octoboard is asking the user on the agent's behalf while the agent's call waits, which is a session request (see "Waiting for the answer" in `docs/product/session-requests.md`). |
| Awaiting instructions | `idle` | a green speech bubble | The process is running and sitting at its prompt. |
| Working in background | `idle`, with a non-empty `background_tasks` | the *working* glyph as a hollow green ring: a green ring pulsing a fading copy of itself outward | The process is sitting at its prompt, but its last turn ended with work still running in the background. Claude Code sessions only; see "Working in background" below. |
| Switching account | any status except `archived`, with `switching_account` true | the *starting* arc | An account switch holds the session, or, in the window that confirmed the switch, the session waits for its turn in it (see "While a session is switching" in `docs/product/switching-session-accounts.md`). Shown in place of the session's own status, which it does not change and which the order of sessions still goes by. |
| Interrupted | `interrupted` | a pause sign | No process is running, and it did not end by being archived. The session stays in its project's list (a console session, in the console sessions section) and can be resumed. |
| Archived | `archived` | an archive box | Ended by being archived (see "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`). No longer listed among its project's sessions (a console session, no longer in the console sessions section); reached through the archive (see "Archived sessions" in `docs/product/archiving-and-resuming-sessions.md`) and can be reopened. |

*Working*, *waiting for the user*, *awaiting instructions* and *working in background* mean a process is running, and
*starting* means one is about to be; the last two mean none is, and both can be resumed.

The glyph stands for the status on the sidebar's rows, focus mode's cards, the top bar's breadcrumb and a confirmation's
list of the sessions it affects. In a confirmation, archived sessions omit the status glyph; the warning identifies them
as archived. The waving hand and the outward pulse stand still where the system asks for reduced motion, leaving the
working glyph a plain accent-coloured dot and the *working in background* glyph a plain hollow green ring; working and
awaiting instructions still differ by shape as well as by colour. The starting arc, which *switching account* shares,
stands still there too and differs from the rest by shape. *Working in background* is the *working* glyph drawn as a
hollow ring in *awaiting instructions*' green: it is told apart from *working*'s filled dot by shape as well as by
colour, and from *awaiting instructions* by shape.

**How the status reaches a user who does not read the glyph** differs by placement:

- On a **sidebar row** and on a **focus mode's card** the glyph adds nothing of its own for assistive technology: the
  row's own accessible name says the status, and a card shows it in words beside the glyph.
- On the **top bar's breadcrumb** and in a **confirmation's list of the sessions it affects** nothing around the glyph
  says the status, so the glyph itself is named after it.
- Every placement where no status text sits beside the glyph gives it a **tooltip** saying the status in the same words
  — the **sidebar's rows**, the **top bar's breadcrumb** and a **confirmation's list** (see "Tooltips on indicators" in
  `docs/product/labels-and-tooltips.md`). Focus mode's cards say the status in words already and carry none.

Transitions:

- Opening a session puts it in *starting*; once its process is up it moves to *working* when it is handed an opening
  prompt (a session started with `new_session`, or on another session's request that the user approved), and to
  *awaiting instructions* otherwise, unless the agent's own first report has already moved it on (see "Starting the
  session" in `docs/product/opening-sessions.md`). A launch that fails removes the session.
- While the process runs, reports from the agent move the session between *working*, *waiting for the user* and
  *awaiting instructions* — except while a folder-trust confirmation, Codex action prompt, a screen of the agent's that
  only the user can answer or a session request holds it at *waiting for the user* (see "What the statuses are derived
  from" below). A Claude Code session that would be *awaiting instructions* reads as *working in background* while it
  has work left running there (see "Working in background" below).
- The process ending for any reason other than archiving — the agent exiting on its own, a crash, the application
  quitting — leaves the session *interrupted*.
- Archiving leaves the session *archived*. User-initiated archiving of an owner also ends and archives the sessions
  under it; agent-initiated archiving is refused while any descendant has a process running (see "Archiving,
  interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`).
- Resuming an *interrupted* or *archived* session puts it in *awaiting instructions*, whichever of the two it came from.
  A resume that is refused or fails to launch leaves the session exactly where it was, archived included. Resuming a
  session whose owner is *archived* first does the same to its archived owners, the topmost first.
- Switching a session to another account of its agent ends and relaunches its process without archiving it: it reads as
  *interrupted* while the process is down and comes back in *awaiting instructions* — working again where it is sent the
  prompt to continue its turn (see "Continuing after a switch" in `docs/product/switching-session-accounts.md`) — and a
  switch that fails leaves it *interrupted* on the account it had (see `docs/product/switching-session-accounts.md`).
- Changing a running session's model or reasoning effort shows it as *starting* until its restart finishes, then as
  *awaiting instructions*, or *working* when it receives a prompt to continue. A failed relaunch leaves it
  *interrupted* with its earlier launch choices; changing an interrupted session's choices leaves it interrupted (see
  "Changing a session's model and effort" in `docs/product/model-and-reasoning-effort.md`).
- An agent's report never moves a session out of *interrupted* or *archived*: whether a stopped session is one or the
  other is Octoboard's own record, not the agent's.

## What the statuses are derived from

Status comes from hook events Octoboard injects into each agent per launch, with the exceptions below. Only the
session's own events count: those of a subagent it runs never change its status, on any agent, and on Claude Code
neither do those of its internal helper agents. Seven things besides those events bear on a running session's status,
and only these seven.

- While a **Claude Code** session is *working* or *waiting for the user*, Octoboard also reads that session's own
  transcript file — the machine-readable record the agent keeps of the conversation, still not its rendered output —
  and, while it is *working*, the activity glyph its terminal title starts with, because neither a turn the user cancels
  nor a declined prompt is reported by any hook event; see "Cancelling a Claude Code turn" and "Declining a Claude Code
  prompt or question" in `docs/product/turns-hooks-do-not-report.md`.
- While a **Grok Build** session is *working*, Octoboard also reads the spinner its terminal title starts with, because
  a turn cancelled before its first token is reported by no hook event; see "Cancelling a Grok Build turn before its
  first token" in `docs/product/turns-hooks-do-not-report.md`.
- While a **Codex** session has a turn open, Octoboard also reads that session's own rollout file — the record Codex
  keeps of the session, again not its rendered output — because a turn that ends in an error is reported by no hook
  event at all; see "A Codex turn that ends in an error" in `docs/product/turns-hooks-do-not-report.md`.
- A **Codex** terminal title showing `Action Required` holds the session at *waiting for the user*, including a prompt
  for one of Codex's subagents. Both phases of its blinking indicator keep the hand up; other hook reports cannot lower
  it while that title remains. See "The raised hand" below for its limits.
- A recognised folder-trust confirmation that needs the user holds its session at *waiting for the user*, even before
  the agent's first hook report. Its raised hand follows the confirmation rather than the dialog; see "What happens when
  a confirmation comes up" in `docs/product/folder-trust.md`.
- A screen of the agent's own that only the user can answer and that no hook reports — such as Claude Code's "Resume
  this conversation?" or a view of its background agents, Codex's sign-in screen, or Grok Build's usage-limit screen —
  holds its session at *waiting for the user* for as long as the session's rendered screen shows it, whatever the
  agent's hook reports say meanwhile; once it is gone the session shows the agent's last reported status again. Which
  screens are recognised is in "Messages held until a session can take them" in `docs/product/message-delivery.md`.
- A session request — a `request_session` or `request_resume_session` call, or an owner's `switch_session_account` or
  `handoff_session` call the user is asked to approve — waiting for the user's answer holds its session at *waiting for
  the user* for the whole wait, whatever the agent's own events say meanwhile; when the request ends the session goes
  back to where its agent was last seen to be (see "Waiting for the answer" in `docs/product/session-requests.md`).

A session's end is not taken from a hook either: the process is observed directly.

Where an agent reports nothing, the status simply stays at its last reported value. The known cases, which are
limitations of what the agents expose rather than of this one:

- A turn that ends with the agent asking a question **as plain prose** is indistinguishable from a finished turn on all
  three agents, so such a session reads as *awaiting instructions* rather than waiting for the user.
- Grok Build's bash mode (`!`) fires no tool or turn events, so work done through it never shows in the status; Grok's
  turn-end backstop, which was seen to follow about a minute later in one run (see "When a session does not report" in
  `docs/product/reporting-to-owners.md`), finds the session already awaiting instructions.
- A question the agent asks through **its own ask-the-user tool** is reported by Claude Code and Grok Build but not by
  Codex, which has no such event; a Codex session asking that way reads as *working* unless its terminal title signals
  `Action Required`.
- Work left running in the background is followed for **Claude Code only**. A Grok Build or Codex session whose turn
  ends with work still running reads as *awaiting instructions*, never *working in background*.

Part of the Grok Build case above is Octoboard's own attribution rule rather than a limit of what the agent exposes.
Grok Build's turn-end backstop names no turn, so Octoboard attributes it by the clock: a turn that ended on its own, or
was cancelled, makes Octoboard expect one backstop, and the next backstop to arrive is taken as that one and discarded,
whether or not a turn is open by then. When that turn's own backstop never comes, a later turn whose only ending is its
backstop has that backstop discarded instead: the session keeps reading as *working* and, for a session bound to an
owner, no report is synthesized for that turn. Whether Grok produces that sequence in practice is not known.

## Working in background

A Claude Code turn can end while work it started is still running in the background — a shell command sent there (by the
user, or by Claude Code when it runs past the tool's time limit), a Monitor, an MCP tool call still waiting, or a
background agent. Claude Code lists that work as the turn ends, and as its background subagents start and stop between
turns, and the session then reads as *working in background* instead of *awaiting instructions* until none of it is
left. On the wire it is still `idle`; what sets it apart is the session's `background_tasks`, one entry per piece of
work: `id`, and `type` and `description` where Claude Code gives them.

**How the list is kept:**

- Every turn end Claude Code reports carries the complete list of what is still running, and replaces the one before; a
  turn end listing nothing empties it. A turn that ends in an error empties it too.
- When a piece of work finishes, Claude Code tells the session so in a turn of its own, naming it; that work leaves the
  list, and the session reads as *working* for that turn as for any other.
- Between turns, while the session is at its prompt, a background subagent's own starts and stops keep the list current,
  since a subagent can wake to its own background work without the session taking a turn. A subagent that starts or
  resumes joins the list; one that stops leaves it, and the rest of the list becomes everything Claude Code reports as
  still running across the whole session at that moment. A subagent's activity never changes the session's status: while
  one works between turns the session keeps reading as *working in background*, not *working*. Inside a turn these
  starts and stops change nothing, the turn's end listing everything. Only a named subagent counts; Claude Code's
  internal helper agents are never listed. A subagent listed because it started carries its description only if an
  earlier report had named it.
- A subagent stop that would leave nothing running takes effect only after about **2 seconds**, so one subagent handing
  over to the next does not make the session flash to *awaiting instructions* in between; a turn end, a subagent
  starting, or another subagent stop within that time decides the list instead.
- The list goes when the session's process ends — archived, interrupted, switched to another account or handed off — and
  the work goes with the process, as it does when the agent exits on its own (see "Archiving, interruption and resuming"
  in `docs/product/archiving-and-resuming-sessions.md`).
- It is given only while the session is at its prompt: in any other status the session shows that status and no list,
  and what was last listed shows again if it comes back to its prompt without a new turn end.
- Clearing the conversation with `/clear` does not end work running in the background (measured on Claude Code 2.1.303),
  so the session keeps reading as *working in background*.

**In every other respect it is a session awaiting instructions.** It takes a message straight away (see "Messages held
until a session can take them" in `docs/product/message-delivery.md`), and it can be handed off (see "What can be handed
off" in `docs/product/handing-off-sessions.md`). Unlike one awaiting instructions with nothing left running, archiving
it asks for confirmation first (see "Archiving, interruption and resuming" in
`docs/product/archiving-and-resuming-sessions.md`), quitting the application asks too (see "Quitting" in
`docs/product/application-lifecycle.md`), and a turn that ends this way is not reported to its owner (see "When a
session does not report" in `docs/product/reporting-to-owners.md`).

**It has its own glyph and name** wherever a session's status is shown or said — its row, focus mode's card, the top
bar's breadcrumb, a confirmation's list of sessions, the row's accessible name and the glyph's tooltip. A console's
avatar distinguishes it from working too, using the badge and selection rules in "The console switcher" in
`docs/product/console-switcher.md`. **For activity summaries, it counts as working**: a collapsed project's activity
marker (see "Project rows" in `docs/product/sidebar-project-rows.md`) and a switch strip chip (see "The switch strip"
in `docs/product/focus-mode.md`). Project and session lists ranked by status give it a separate place, following
"Order of projects and sessions" in `docs/product/order-of-projects-and-sessions.md`. The menu bar icon's menu lists it
in a group of its own (see "The icon's menu" in `docs/product/menu-bar-icon.md`).

Every session record the session tools give carries the list (see "Session records" in
`docs/product/what-every-session-reads.md`).

## The raised hand

*Waiting for the user* is the raised hand, and it is made findable rather than left on the session's own row:

- The session's row shows the raised hand. Its project row shows one while the project is collapsed, and its console's
  avatar on the rail carries one unless the session chosen for the badge is switching account. The rail shows every
  console at once, so a waiting session can be found whatever the sidebar is showing (see "The console switcher" in
  `docs/product/console-switcher.md` and "Project rows" in `docs/product/sidebar-project-rows.md`). In a console
  session's focus mode, a console session's chip in the switch strip shows one too while it or a session bound to it is
  waiting (see "The switch strip" in `docs/product/focus-mode.md`).
- A system notification fires once as a session enters that state, naming the session by its title and the project it
  runs in — or the console whose console session it is. A session that is answered and later waits again notifies again.
  The macOS application asks for no notification permission, and macOS showed no prompt for one. The banner has been
  seen while Octoboard was not the frontmost application, and not while it was. A notification that cannot be shown is
  not reported as having failed — the sidebar's own marker carries the same signal.
- The Dock badge carries how many sessions are waiting, counted across every console, and clears when none is. While the
  window is closed into the background Octoboard has no Dock icon; the count is still kept and shows once it returns to
  the Dock (see "Closing the window" in `docs/product/application-lifecycle.md`). The menu bar icon's menu lists the
  waiting sessions too (see "The icon's menu" in `docs/product/menu-bar-icon.md`), and the icon itself carries an orange
  dot while any is waiting (see "The icon's look" in `docs/product/menu-bar-icon.md`).
- The rail carries the same count, as a raised hand and the number, shown only while at least one session is waiting.
  Pressing it selects the next waiting session after the selected one — console by console in the order the consoles
  were created, its console sessions before its projects' sessions, and both in the sidebar's order (see "Order of
  projects and sessions" in `docs/product/order-of-projects-and-sessions.md`) — and wraps from the last back to the
  first; when the selected session is not waiting, it selects the first. The sidebar follows the session it selects (see
  "Selecting a session" in `docs/product/sidebar.md`).

**Where the hand is the agent's own, the user answers in the session's terminal**, and the status leaves *waiting for
the user* on the agent's next event, when its Codex action title clears or when the screen that held it is no longer
shown — or, where the answer was a decline and no event follows, on the decline showing up in the agent's own record of
the conversation, which only Claude Code sessions are read for (see "Declining a Claude Code prompt or question" in
`docs/product/turns-hooks-do-not-report.md`). Nobody can answer for them: a session driving others — a console session,
or a project session that started sessions — is told to leave such a session alone, a message addressed to it is held
until the user is done — see "Messages held until a session can take them" in `docs/product/message-delivery.md` — and
an interrupt of its turn is refused (see "When a key is sent" in `docs/product/interrupting-turns.md`).

**A granted tool permission prompt reported through hooks lowers the hand only once the call it let through has
finished.** The user answering *yes* is itself reported by no event, so the next event the session produces is normally
the end of that tool call — the hand stays up from the moment the prompt appeared until the call is done, which is a
moment for a file write and minutes for a build or a test run. For that whole time the session reads as *waiting for the
user* everywhere the hand is shown, it counts in the rail's waiting count and the Dock badge, and a message addressed to
it goes on being held. There is no earlier signal to read: the delay is in what the agents report, not in how often
Octoboard looks. Measured on Codex 0.160.0 and on Claude Code 2.1.274; Grok Build was not measured. A prompt the user
*declines* is not delayed this way: the decline ends the turn, and the hand comes down right after the answer.

A hand raised for a folder-trust confirmation can be answered in its trust dialog or the terminal; see "What happens
when a confirmation comes up" in `docs/product/folder-trust.md`.

A hand Octoboard raised for a waiting session request is answered in that request's dialog instead, so a message
addressed to such a session is not held (see "Waiting for the answer" in `docs/product/session-requests.md`).

Where Codex's effective approval mode enables **automatic approval review** (`auto_review`) — from the explicit choice
made for Codex or the account's own configuration (see "Approval modes" in `docs/product/per-agent-settings.md`) — a
permission event alone raises no hand: requests Codex resolves itself keep the session reading as *working*. If a manual
prompt does reach the terminal, its `Action Required` title raises the hand, including a prompt labelled as belonging to
a Codex subagent. Messages addressed to the session are held while that signal is present. When Codex changes the title
back after the answer, the session returns to its last hook-reported status and queued messages are released if that
status permits delivery, without waiting for another hook event.

The title does not override a hook-reported wait after it clears: ordinary approvals whose last hook status is still
*waiting for the user* keep the hand up until the tool finishes, as described above. This detection relies on Codex's
activity title, which every launch keeps on: each launch sets the items of Codex's terminal title for that launch,
overriding the user's and the project's own `tui.terminal_title` inside Octoboard's terminal, so the signal does not
depend on how the user has configured their titles (see "Codex" in `docs/product/per-agent-launch-specifics.md`). The
user's Codex configuration files are not changed.
