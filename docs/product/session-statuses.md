# Session statuses

| Status | Wire value | Glyph | Meaning |
|---|---|---|---|
| Starting | `starting` | a grey arc turning | A newly opened session — from the dialog, by an agent's `new_session`, or by an approved session request — whose agent is still being launched; it has no process yet. Once the process is up it becomes *working* for a session started with a brief, *done* for one opened on an existing conversation, and *awaiting instructions* otherwise; it is removed again if the launch fails (see "Starting the session" in `docs/product/opening-sessions.md`). |
| Working | `working` | a solid blue (accent) dot pinging a fading copy of itself outward | The agent is executing a turn. |
| Waiting for the user | `waiting_user` | an orange (warning) raised hand that waves now and then | The agent is waiting on a permission decision, including its folder-trust confirmation, or has asked the user a question through its own ask-the-user tool; or Octoboard is asking the user on the agent's behalf while the agent's call waits, which is a session request (see "Waiting for the answer" in `docs/product/session-requests.md`). |
| Dormant | `dormant` | an orange (warning) "zz", two Zs rising | The terminal of a running Claude Code session shows one of its views of its background agents, and the session is held there until the user leaves it. See "Dormant" below. |
| Paused | `idle`, with `paused` true | a green pause sign in a circle | The process is sitting at its prompt after a turn that ended abnormally — cancelled, ended by a declined prompt, stopped on a usage limit, failed, or cut short by the session's process ending. See "Paused" below. |
| Done | `idle`, with `has_conversation` true | a green circle with a check mark | The process is running and sitting at its prompt with nothing left running in the background and no delegated work in progress, and the session has a conversation. See "Done, awaiting instructions and child sessions working" below. |
| Awaiting instructions | `idle`, with `has_conversation` false | a green speech bubble | The process is running and sitting at its prompt, and the session has never had a conversation: it waits for its first instruction. |
| Child sessions working | `idle`, while a session under it is starting, working, waiting for the user, dormant or has work left running in the background | a hollow green ring with a filled green dot in its centre, pinging outward | The process is sitting at its prompt with nothing of its own left running, while work it delegated is in progress in the sessions under it. See "Done, awaiting instructions and child sessions working" below. |
| Working in background | `idle`, with a non-empty `background_tasks` | a hollow green ring, pinging outward | The process is sitting at its prompt, but its last turn ended with work still running in the background. Claude Code sessions only; see "Working in background" below. |
| Switching account | any status except `archived`, with `switching_account` true | the *starting* arc | An account switch holds the session, or, in the window that confirmed the switch, the session waits for its turn in it (see "While a session is switching" in `docs/product/switching-session-accounts.md`). Shown in place of the session's own status, which it does not change; lists ranked by status sort it by the status it will have once the switch is over (see "The status order" in `docs/product/order-of-projects-and-sessions.md`). |
| Interrupted | `interrupted` | a grey pause sign, with no circle | No process is running, and it did not end by being archived. The session stays in its project's list (a console session, in the console sessions section) and can be resumed. |
| Archived | `archived` | an archive box | Ended by being archived (see "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`). No longer listed among its project's sessions (a console session, no longer in the console sessions section); reached through the archive (see "Archived sessions" in `docs/product/archiving-and-resuming-sessions.md`) and can be reopened. |

*Working*, *waiting for the user* and *dormant* mean a process is running, and so do the five statuses of a session **at
its prompt** — *paused*, *done*, *awaiting instructions*, *child sessions working* and *working in background*, all
`idle` on the wire. *Starting* means a process is about to be; *interrupted* and *archived* mean none is, and both can
be resumed.

*Paused*, *done*, *awaiting instructions*, *child sessions working* and *working in background* are told apart by the
client alone: the daemon's `status` is `idle` for all five, and so is the `status` of every session record the session
tools give (see "Session records" in `docs/product/what-every-session-reads.md`), which carry `paused` and the other
facts the client reads.

## Which status a session shows

A session shows the first of these that applies:

1. *Switching account*, while an account switch holds it, over every status but *archived*.
2. Its wire status, when that is not `idle` — *dormant* included, which the daemon works out (see "Dormant" below).
3. *Paused*, while `paused` is true.
4. *Working in background*, while it has work left running in the background (Claude Code only; for the other agents
   this step never applies).
5. *Child sessions working*, while a session under it is starting, working, waiting for the user, dormant or has work
   left running in the background.
6. *Done*, when it has a conversation.
7. *Awaiting instructions*.

## Status glyphs

The glyph stands for the status on the sidebar's rows, focus mode's cards, the top bar's breadcrumb and a confirmation's
list of the sessions it affects. In a confirmation, archived sessions omit the status glyph; the warning identifies them
as archived. The same glyphs mark the top status of a group of sessions (see "The top status" below), and the menu bar
icon's menu draws them still (see "The icon's menu" in `docs/product/menu-bar-icon.md`).

**Colour says what the session asks of the person; form progresses gradually along the status order** (see "The status
order" in `docs/product/order-of-projects-and-sessions.md`), so statuses next to each other look related. Orange is a
session that needs the person: the hand of *waiting for the user*, waiting for an answer, and the "zz" of *dormant*,
held in a view until the person leaves it. Green is the session at its prompt, where it can be talked to: the circled
pause of *paused*, the check mark of *done*, the speech bubble of *awaiting instructions*, the ring with a centre dot of
*child sessions working* and the hollow ring of *working in background*. Blue is the session itself busy: the solid dot
of *working*. The muted grey is a session that cannot be talked to now: the bare pause of *interrupted*, and the turning
arc that *starting* and *switching account* share. *Archived* is an archive box. The ring of *child sessions working* is
larger than the other dots, and its ping reaches less far, ending within the glyph's own slot.

**Every glyph sits in a box of one size, whatever the status**: 16 px on rows, cards, the breadcrumb and a
confirmation's list, 14 px in a status marker (see "The top status" below), so glyphs in a column share one centre line.
The raised hand and the speech bubble, whose ink leans to one side, are moved slightly within the box so that they read
centred; the turning arc turns about the box's centre.

**Dormant's "zz"** is a large Z at the bottom left with a smaller one stepping up to its top right. Where motion is
allowed, Zs drift up that diagonal instead, shrinking as they rise, a new one entering at the bottom left as the top one
leaves at the top right; each takes about 2.4 seconds to cross, the next a third of that behind, and none is drawn
outside the glyph's box.

The waving hand, the pings, the turning arc and the rising Zs stand still where the system asks for reduced motion.
Every status still differs from the rest by shape, so colour is never the only cue: the filled dot of *working* from the
hollow ring of *working in background*, that ring from the larger ring of *child sessions working* by the latter's centre
dot, the rings from the speech bubble and the check mark, the "zz" from the hand, and the circled pause of *paused*
from the bare pause of *interrupted*.

**How the status reaches a user who does not read the glyph** differs by placement:

- On a **sidebar row** and on a **focus mode's card** the glyph adds nothing of its own for assistive technology: the
  row's own accessible name says the status, and a card shows it in words beside the glyph.
- On the **top bar's breadcrumb** and in a **confirmation's list of the sessions it affects** nothing around the glyph
  says the status, so the glyph itself is named after it.
- Every placement where no status text sits beside the glyph gives it a **tooltip** saying the status in the same words
  — the **sidebar's rows**, the **top bar's breadcrumb**, a **confirmation's list** and a **status marker** (see
  "Tooltips on indicators" in `docs/product/labels-and-tooltips.md`). Focus mode's cards say the status in words already
  and carry none.

## Transitions

- Opening a session puts it in *starting*; once its process is up it moves to *working* when it is handed an opening
  prompt (a session started with `new_session`, or on another session's request that the user approved), to *done*
  when it was opened on an existing conversation (see "Opening a new session on a found conversation" in
  `docs/product/resuming-a-conversation-by-its-session-id.md`), and to *awaiting instructions* otherwise, unless the
  agent's own first report has already moved it on (see "Starting the session" in `docs/product/opening-sessions.md`).
  A launch that fails removes the session.
- While the process runs, reports from the agent move the session between *working*, *waiting for the user* and its
  prompt — except while a folder-trust confirmation, Codex action prompt, a screen of the agent's that only the user can
  answer or a session request holds it at *waiting for the user* (see "What the statuses are derived from" below), and
  while a view of its background agents shows it *dormant* (see "Dormant" above). At its prompt it shows the status
  "Which status a session shows" above gives it: a session started with a brief is *working*, then *done* once that turn
  is over; one opened by hand is *awaiting instructions* until its first turn; a turn that ends abnormally leaves it
  *paused* until the next one (see "Paused" above).
- The process ending for any reason other than archiving — the agent exiting on its own, a crash, the application
  quitting — leaves the session *interrupted*. A turn it was in the middle of then counts as having ended abnormally
  (see "Paused" below).
- Archiving leaves the session *archived*. User-initiated archiving of an owner also ends and archives the sessions
  under it; agent-initiated archiving is refused while any descendant has a process running (see "Archiving,
  interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`).
- Resuming an *interrupted* or *archived* session brings it back at its prompt, whichever of the two it came from:
  *paused* where its last turn ended abnormally, *done* where it has a conversation otherwise, *awaiting instructions*
  where it has none yet, and *working* once it takes up an instruction the resume hands it. A resume that is refused or fails to launch leaves the session exactly where it was,
  archived included. Resuming a session whose owner is *archived* first does the same to its archived owners, the
  topmost first.
- Switching a session to another account of its agent ends and relaunches its process without archiving it: it reads as
  *interrupted* while the process is down and comes back at its prompt like a resumed session — *working* again where it
  is sent the prompt to continue its turn (see "Continuing after a switch" in
  `docs/product/switching-session-accounts.md`) — and a switch that fails leaves it *interrupted* on the account it had
  (see `docs/product/switching-session-accounts.md`).
- Changing a running session's model or reasoning effort shows it as *starting* until its restart finishes, then at its
  prompt like a resumed session, or *working* when it receives a prompt to continue. A failed relaunch leaves it
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
  this conversation?", Codex's sign-in screen, or Grok Build's usage-limit screen — holds its session at *waiting for
  the user* for as long as the session's rendered screen shows it, whatever the agent's hook reports say meanwhile; once
  it is gone the session shows the agent's last reported status again. Claude Code's views of its background agents are
  recognised the same way but show the session *dormant* instead (see "Dormant" above). Which screens are recognised is
  in "Messages held until a session can take them" in `docs/product/message-delivery.md`.
- A session request — a `request_session` or `request_resume_session` call, or an owner's `switch_session_account` or
  `handoff_session` call the user is asked to approve — waiting for the user's answer holds its session at *waiting for
  the user* for the whole wait, whatever the agent's own events say meanwhile; when the request ends the session goes
  back to where its agent was last seen to be (see "Waiting for the answer" in `docs/product/session-requests.md`).

A session's end is not taken from a hook either: the process is observed directly.

Where an agent reports nothing, the status simply stays at its last reported value. The known cases, which are
limitations of what the agents expose rather than of this one:

- A turn that ends with the agent asking a question **as plain prose** is indistinguishable from a finished turn on all
  three agents, so such a session reads as at its prompt — *done*, normally — rather than waiting for the user.
- Grok Build's bash mode (`!`) fires no tool or turn events, so work done through it never shows in the status; Grok's
  turn-end backstop, which was seen to follow about a minute later in one run (see "When a session does not report" in
  `docs/product/reporting-to-owners.md`), finds the session already at its prompt.
- A question the agent asks through **its own ask-the-user tool** is reported by Claude Code and Grok Build but not by
  Codex, which has no such event; a Codex session asking that way reads as *working* unless its terminal title signals
  `Action Required`.
- For status reporting, work left running in the background is followed for **Claude Code only**. A Grok Build or Codex
  session whose turn ends with work still running reads as *done* (or another status at its prompt), never *working in
  background*.
  Process cleanup follows every agent's launches separately (see "Automatic stopping and its boundaries" in
  `docs/product/background-processes.md`).

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
turns, and the session then reads as *working in background* instead of *done* or any other status at its prompt
until none of it is left. On the wire it is still `idle`; what sets it apart is the session's `background_tasks`, one entry per piece of
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
  over to the next does not make the session flash to another status at its prompt in between; a turn end, a subagent
  starting, or another subagent stop within that time decides the list instead.
- The list goes when the session's process ends — archived, interrupted, switched to another account or handed off.
  The launch's background work is stopped except for processes the user kept, under "Automatic stopping and its
  boundaries" in `docs/product/background-processes.md`.
- It is given only while the session is at its prompt: in any other status the session shows that status and no list,
  and what was last listed shows again if it comes back to its prompt without a new turn end.
- Clearing the conversation with `/clear` does not end work running in the background (measured on Claude Code 2.1.303),
  so the session keeps reading as *working in background*.

**In every other respect it is like any other session at its prompt.** It takes a message straight away (see "Messages held
until a session can take them" in `docs/product/message-delivery.md`), and it can be handed off (see "What can be handed
off" in `docs/product/handing-off-sessions.md`). The session it is bound to reads as *child sessions working* (see "Done,
awaiting instructions and child sessions working" below). Unlike one *done* or *awaiting instructions* with no session under it,
archiving it asks for confirmation first (see "Archiving, interruption and resuming" in
`docs/product/archiving-and-resuming-sessions.md`), quitting the application asks too (see "Quitting" in
`docs/product/application-lifecycle.md`), and a turn that ends this way is not reported to its owner (see "When a
session does not report" in `docs/product/reporting-to-owners.md`).

**It has its own glyph and name** wherever a session's status is shown or said, as every status does (see "The top
status" below), and lists ranked by status give it a place of its own (see "The status order" in
`docs/product/order-of-projects-and-sessions.md`).

Every session record the session tools give carries the list (see "Session records" in
`docs/product/what-every-session-reads.md`).

## Done, awaiting instructions and child sessions working

**Done and awaiting instructions are told apart by whether the session has a conversation** (`has_conversation`). A
session has one from the moment its agent reports the first prompt submitted, or from the moment it is opened or
resumed on an existing conversation (see "Opening a new session on a found conversation" in
`docs/product/resuming-a-conversation-by-its-session-id.md`), and keeps it: clearing the conversation with `/clear`
does not take it away. A session with a conversation that is at its prompt, with nothing left running in the background
and no delegated work in progress, is *done*; one that has never had a conversation is *awaiting instructions*. So:

- a session opened by hand with no brief is *awaiting instructions* until its first turn, then *done* after it;
- a session started with a brief is *working*, then *done* once its turn is over;
- a session opened on a found conversation is *done* as soon as it is up;
- resuming an *interrupted* or *archived* session that has a conversation, and the relaunch after an account switch or a
  change of model and effort, bring it back *done* — or *working*, when it is sent a prompt to continue, and *paused*
  when its last turn ended abnormally (see "Paused" below).

**Child sessions working** is a session at its prompt, with nothing of its own left running in the background, while a
session bound to it — or, at any depth, bound to one of those — is *starting*, *working*, *waiting for the user* or
*dormant*, or is at its prompt with work left running in the background, whether it shows *working in background* or
*paused*, since that work is still going. Sessions under it that are at their prompt with nothing left running in the
background, *interrupted* or *archived* do not count. It takes precedence over *done* and *awaiting instructions*, and gives way to *paused* and
*working in background* (see "Which status a session shows" above).

**Done and child sessions working are sessions at their prompt in every respect but their glyph and name.** Each
takes a message straight away (see
"Messages held until a session can take them" in `docs/product/message-delivery.md`) and can be handed off (see "What
can be handed off" in `docs/product/handing-off-sessions.md`). Neither *done* nor *child sessions working* holds up a
quit by itself; for *child sessions working*, the busy sessions under it do (see "Quitting" in
`docs/product/application-lifecycle.md`). Archiving one with an unarchived session under it asks for confirmation, as
for any owner (see "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`).

Each has its own glyph and name wherever a session's status is shown or said, as every status does (see "The top
status" below), and lists ranked by status give each a place of its own (see "The status order" in
`docs/product/order-of-projects-and-sessions.md`). The sessions under one with child sessions working count by their own
status, wherever they are.

## Paused

**Paused is a session at its prompt whose last turn ended abnormally**: cancelled — by the user in the terminal, or by an
owner's `interrupt_session` (see `docs/product/interrupting-turns.md`) — ended by a declined permission prompt or
question, stopped on a usage limit, failed, such as on an overloaded model or a network or API error, or cut short by
the session's process ending while the turn was in progress. On the wire it
is `idle` with `paused: true`; the flag names no reason, and neither does the window: the glyph's tooltip, a focus
mode's card and every accessible name say only that it is paused.

**What sets it:**

- the end of the turn as the agent reports it — Claude Code's `StopFailure` hook event, Codex's `Interrupt`, Grok
  Build's `StopCancelled` and `StopFailure` — or as Octoboard recovers it where no hook reports it: a Claude Code
  decline or cancel, a Grok Build cancel before its first token, and a Codex turn that ended in an error (see
  `docs/product/turns-hooks-do-not-report.md`);
- the session losing its process while its agent was last reported *working* or *waiting for the user*: the agent exiting or
  crashing, the session being interrupted or archived, the application quitting, or the daemon going down. A session
  force-quit in the middle of a turn reads as *interrupted* and, once resumed, comes back *paused* rather than *done*.
  A process that ends while the session is at its prompt — the agent quit from its prompt, say — leaves the flag as the
  last turn left it.

  Whether a turn was in progress is judged on the agent's own status: a waiting session request holding the session at
  *waiting for the user* over an agent at its prompt is not a turn in progress, so its process ending leaves the flag
  as the last turn left it. The one exception is a daemon that went down without stopping its sessions: what the
  request held is gone with it, so the status last recorded decides, and such a session comes back paused.

  Hook events the agent had already delivered are applied before the end of its process is recorded — waited on for
  up to 2 seconds when a process exits, and for up to half a second across all sessions when the application quits —
  so a turn that ended normally just before the end reads as having ended normally.

**What clears it:**

- the next turn beginning, or anything else that moves the agent's own status off its prompt, such as a permission
  prompt its hooks report. A waiting session request holding the session at *waiting for the user* is not its agent's
  own status and does not clear it (see "Waiting for the answer" in `docs/product/session-requests.md`): where the
  agent is still at its prompt when the request ends, the session reads as paused again;
- a turn ending normally, whether or not it leaves work running in the background;
- a Claude Code session beginning a fresh conversation: a fresh start, or `/clear`. Resuming or compacting the
  conversation, `/compact` included, leaves it;
- the agent no longer having the session's conversation, so that its next launch opens a fresh one.

Grok Build's turn-end backstop and its turn end at shutdown neither set nor clear it.

**A turn's own end can arrive after its process ended.** Codex does not wait for any of its hooks, nor Claude Code for
hooks given in the command form (see "What is injected on every launch" in `docs/product/launching-agents.md`), so
the end of a turn they reported just before the process ended can reach Octoboard only after that end has marked the
turn as cut short. Such a late end is still taken for that turn when it arrives within 10 seconds of the process
ending and names the turn — Claude Code's `prompt_id`, Codex's `turn_id`, Grok Build's `promptId`:

- a normal end clears the flag, so the session comes back *done* rather than *paused* — only while the session has
  been neither resumed nor ended again since;
- an abnormal end leaves it set;
- an end that names no turn, or another turn, never clears it.

Nothing of such a late event reaches a process started in the session since. A session whose process Octoboard stops
itself in the middle of a turn — archiving, interrupting, quitting — stays paused: stopped so, Codex reports nothing, Claude Code
only the end of its session, and Grok Build only its turn end at shutdown, which names no turn.

**It is kept with the session, not with its process**, so it outlives the process ending: a resume, an account switch,
a change of model and effort or of its account's sharing that brings the session back at its prompt brings it back
paused when its last turn ended abnormally. A session that is sent a prompt to continue its turn on the relaunch reads
as *working* instead, and the turn that follows decides the flag afresh.

**It shows only while the session would otherwise be at its prompt.** A wait that only the session's screen shows, or a
view that shows it *dormant*, takes its place without clearing it, so the session reads as paused again once that is
gone.

**In every other respect it is a session at its prompt**: it takes a message straight away (see "Messages held until a
session can take them" in `docs/product/message-delivery.md`), can be handed off (see "What can be handed off" in
`docs/product/handing-off-sessions.md`), is archived at once when nothing else asks for a confirmation (see "Archiving,
interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`), and holds up no quit by itself (see
"Quitting" in `docs/product/application-lifecycle.md`). A paused session that still has work running in the background is
the exception: quitting asks for confirmation for it, as for *working in background*, and its owner reads as *child
sessions working*.

## Dormant

**Dormant is a running Claude Code session whose terminal shows one of its views of its background agents**: a
background subagent's view (`claude_subagent_view`), its agents view (`claude_agents_view`), or its list of background
agents with the focus moved onto it (`claude_agent_list_focus`) — the screens of those codes in "Messages held until a
session can take them" in `docs/product/message-delivery.md`. The person is looking at the agents rather than being
asked anything, and the session cannot take a message until they leave the view. On the wire it is `dormant`, worked out
by the daemon from the session's screen and never stored: the agent's own last reported status is kept beneath it and
shows again the moment the view is gone.

- **It shows over *working* and over every status at its prompt.** It does not show over a wait for the user: a hand
  the agent's hooks or a session request raised, a folder-trust confirmation, Codex's `Action Required` title and every
  other recognised screen of the agent's — Grok Build's usage-limit screen included — still read as *waiting for the
  user*. Nor does it show over *starting*, *interrupted* or *archived*; *switching account* shows over it as over any
  status.
- **What is written into it is held** as for any recognised screen, its `writes_held` reason `agent_dialog` with the
  view's code, and written once the view is gone.
- **Nobody acts on it in the user's place.** An owner's `interrupt_session` and `set_session_model` are refused with
  `session_waiting_for_user`, under a message that names the view (see "When a key is sent" in
  `docs/product/interrupting-turns.md` and "Changing a session's model and effort" in
  `docs/product/model-and-reasoning-effort.md`); `get_session` carries a note telling the caller to leave it be. An
  automatic account switch leaves it alone (see `docs/product/automatic-account-switch.md`), and a change of its
  account's sharing is refused while it lasts (see "When a change is carried out" in
  `docs/product/sharing-the-default-configuration.md`). The user can still change its model from the window, where the
  dialog warns that restarting closes the view.
- **It raises no hand**: no system notification, no count in the Dock badge or the rail's waiting count, and its owner
  is not told that it waits (see "Notices to an owner" in `docs/product/reporting-to-owners.md`). The menu
  bar icon's orange dot does light for it (see "The icon's look" in `docs/product/menu-bar-icon.md`).
- **It counts as busy**: quitting asks for confirmation while one is dormant (see "Quitting" in
  `docs/product/application-lifecycle.md`), archiving it asks first (see "Archiving, interruption and resuming" in
  `docs/product/archiving-and-resuming-sessions.md`), and the session it is bound to reads as *child sessions working*.

## The top status

A collapsed project's row and a console session's focus-mode switch-strip chip mark several sessions with their
**top status**: the shown status, as "Which status a session shows" above gives it, of the session that comes first
in the status order (see "The status order" in `docs/product/order-of-projects-and-sessions.md`), the most recently
started breaking a tie. Archived sessions are left out. A session starting or switching account is ranked with the
status it will have once that is over, but marked as starting or switching account. The choice does not depend on
pins, nor on whether the sidebar is ordered by status.

Every status but *archived* has its own glyph there, as "Status glyphs" above describes, so the mark always says exactly
which status it is; there is a mark whenever any of the sessions is not archived. The accessible name of the row or
chip is its own name followed by that status ("Name, Working"; for a pinned project, "Name, pinned, Working"). Where
each is drawn is in "Project rows" in `docs/product/sidebar-project-rows.md` and "The switch strip" in
`docs/product/focus-mode.md`.

**A console's avatar on the rail selects only among sessions whose shown status is waiting for the user, paused, done or
awaiting instructions**, in that priority order, with the most recently started breaking a tie. Other shown statuses
are excluded before choosing the top status, including starting and switching account. With no eligible session, it
has no badge and its accessible name and tooltip contain only the console's name. Otherwise they include the chosen
status. The badges are described in "The console switcher" in `docs/product/console-switcher.md`.

The menu bar icon's session list uses the same four eligible statuses and their priority (see "The icon's menu" in
`docs/product/menu-bar-icon.md`). This filtering does not change session rows, collapsed-project markers or focus-mode
switch-strip chips.

## The raised hand

*Waiting for the user* is the raised hand, and it is made findable rather than left on the session's own row:

- The session's row shows the raised hand. Its project row shows one while the project is collapsed, and its console's
  avatar on the rail carries one when a session shows that waiting status. The rail shows every console at once, so a
  waiting session can be found whatever the sidebar is showing (see "The console switcher" in
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
  dot while any is waiting or dormant (see "The icon's look" in `docs/product/menu-bar-icon.md`).
- The rail carries the same count, as a raised hand and the number, shown only while at least one session is waiting.
  Pressing it selects the next waiting session after the selected one — console by console in the order the consoles
  were created, its console sessions before its projects' sessions, and both in the sidebar's order (see "Order of
  projects and sessions" in `docs/product/order-of-projects-and-sessions.md`) — and wraps from the last back to the
  first; when the selected session is not waiting, it selects the first. The sidebar follows the session it selects (see
  "Selecting a session" in `docs/product/sidebar.md`).

A *dormant* session raises none of this: it notifies nobody and is in neither count; only the menu bar icon's dot lights
for it (see "Dormant" above).

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
