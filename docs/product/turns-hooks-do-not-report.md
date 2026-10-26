# Turns the agents' hooks do not report

A session's status follows the hook events its agent reports (see "What the statuses are derived from" in
`docs/product/session-statuses.md`). A few ways a turn ends, or a raised hand comes down, are reported by no hook event,
and Octoboard reads them from somewhere else instead: the agent's own record of the conversation, or its terminal title.
This doc covers each of those cases, what Octoboard reads for it, and where that reading fails. They apply whoever
caused them: a cancel the user makes in the terminal and one an owner's `interrupt_session` makes (see
`docs/product/interrupting-turns.md`) are recovered the same way.

## Declining a Claude Code prompt or question

A Claude Code session with its hand up that is then **declined** — the permission prompt answered No or dismissed with
Esc, or the agent's own ask-the-user question cancelled — reports nothing at all: no hook event of any kind follows,
while the agent is already back at an empty prompt. So for this one case Octoboard reads the session's own transcript
file, the record Claude Code keeps of the conversation, and moves the session to *awaiting instructions* once the
decline appears there, normally within about a second of the user answering. Both ways a Claude Code session raises its
hand are covered: a permission prompt and its own ask-the-user question.

What follows from that move is nothing special to this path: the raised hand comes down wherever the sidebar showed it,
the Dock badge count drops, and a message queued for the session while its hand was up is released (see "Messages held
until a session can take them" in `docs/product/message-delivery.md`). A later prompt or question in the same session
raises the hand and notifies afresh, as any other does.

- The agent prints that the turn was interrupted, but its process is still running, so the session is *awaiting
  instructions* and not *interrupted*.
- No turn end was reported, so no report is synthesised for the session's owner for that turn either (see "When a
  session does not report" in `docs/product/reporting-to-owners.md`).
- **Claude Code sessions only.** Codex and Grok Build report a decline through their own hook events, and the transcript
  read here is Claude Code's own format; neither is watched this way.
- Only a hand that is currently up is recovered this way. A turn cancelled while the session reads as *working* is
  recovered by its own watch — see "Cancelling a Claude Code turn" below.
- Only the decline Claude Code records as the user's own entry counts. The transcript also logs each request sent to the
  model, replaying the conversation so far — earlier declines included; a decline replayed inside one does not lower the
  hand.
- Where an ask-the-user question is left not by declining it but by asking to chat about it instead, the turn in fact
  carries on; Octoboard reads that as a decline too, so the session reads as *awaiting instructions* for a moment until
  the agent's next event puts it back to *working*.
- If the record cannot be read — the pending-decision event named no transcript file, or the file is unreadable —
  nothing lowers the hand, and the session keeps it up until something else moves its status.
- A decline is noticed the same way while a session request holds that session's hand up (see "Waiting for the answer"
  in `docs/product/session-requests.md`). The hand stays up until the request ends, since it is the request's own; the
  session is then *awaiting instructions*, where the decline left its agent.

**What this costs to keep working.** The transcript is a file format Claude Code owns and rewrites on upgrade, and a
decline is recognised by two fixed marker strings inside it; the behaviour was measured against Claude Code 2.1.274 and
2.1.286, and seen working in the application against 2.1.289. A release that renames those markers breaks this silently
and completely — there is no error, nothing is reported as having failed, and the only symptom is a declined prompt
leaving the raised hand up with nothing to lower it. Nothing Octoboard can observe by itself tells that apart from a
user who simply has not answered yet.

## Cancelling a Claude Code turn

A Claude Code session reading *working* whose turn the user **cancels** — Esc pressed while the request waits for the
model, or while the model is thinking, writing or running a tool — reports nothing: the cancelled turn fires no hook
event, while the agent is already back at its prompt. So while the session works Octoboard watches two things, and moves
the session to *awaiting instructions* as soon as either shows the cancel, normally within about a second of it:

- **The transcript.** Once the model has shown output, Claude Code records the cancel in the session's own transcript
  file, as a user entry reading `[Request interrupted by user]` (cancelled while the model writes) or `[Request
  interrupted by user for tool use]` (cancelled while a tool runs). The session moves once that entry appears.
- **The terminal title.** Claude Code starts its terminal title with a turning half-filled circle (`◐`, `◑`) while a
  turn runs, and with `✳` when it is back at its prompt. A cancel before the model has shown anything — the request
  still waiting for its response, or the model still thinking — writes nothing to the transcript, and the title is its
  only trace. The session moves once three things hold: the title has shown the prompt for about a second after showing
  the turn running, Octoboard has applied every hook event it has received for the session, and the session still reads
  *working*. The same `✳` comes up when a turn ends normally and when a permission prompt or question waits on the user,
  but the hook events that come with those have moved the session by then, and a session they moved stays where they put
  it.

After a cancel before any output, Claude Code puts the prompt's text back in its input box. A message Octoboard writes
into the session from then on goes in after that text and is sent together with it, whether the session reads *working*
or *awaiting instructions* at the time. A cancel made by `interrupt_session` clears that text first (see "The input box
afterwards" in `docs/product/interrupting-turns.md`); a cancel the user makes in the terminal is left as the agent
leaves it.

- **When the watches run.** Both start each time a hook event moves the session into *working* — a submitted prompt, or
  any other event that finds it not already working, such as the hand coming down on an answered prompt — and stop once
  the session leaves *working* some other way, raises its hand, or its process ends. While a session request holds the
  session's hand up (see "Waiting for the answer" in `docs/product/session-requests.md`), the title watch waits: a
  cancel the title showed meanwhile moves the session once the request ends, if it is then back at *working*.
- **Only this turn's cancel counts.** Only transcript entries written after the watch started are read, and an interrupt
  counts only if it is timestamped no earlier than a few seconds before the watch started, so an earlier turn's
  interrupt, and one in the history of a resumed or copied conversation, does not. An interrupt replayed inside a logged
  request does not count either, for the reason given in "Declining a Claude Code prompt or question" above. A `✳`
  counts only after a running glyph the title showed when the watch started or later, so the `✳` the previous turn left
  on the title does not end this one. Each submitted prompt retires the previous turn's watches before its own turn
  opens, so a cancel found late never ends the turn after it.
- **Not while a screen of the agent's own is up.** While one of the Claude Code screens that hold messages is on the
  session's screen (see "Messages held until a session can take them" in `docs/product/message-delivery.md`), a `✳` is
  the turn waiting on the user and is not counted.
- The agent's process is still running, so the session is *awaiting instructions* and not *interrupted*.
- No turn end was reported, so no report is synthesised for the session's owner for that turn (see "When a session does
  not report" in `docs/product/reporting-to-owners.md`).
- **Claude Code sessions only.** Codex reports a cancelled turn through its own hook event, and Grok Build through its
  own once the model has shown output; the transcript and title glyphs read here are Claude Code's own. A Grok Build
  turn cancelled before its first token is read from Grok's own title (see "Cancelling a Grok Build turn before its
  first token" below).
- A cancel while the session's hand is up is a decline, covered by "Declining a Claude Code prompt or question" above.

Where it is not recovered, the session keeps reading *working* until its next prompt is submitted:

- the hook event that moved the session into *working* named no transcript file, or the file could not be read when the
  watch started — the title is then not watched either;
- the cancel came before Octoboard applied the hook event that moved the session into *working* — hook events are
  delivered asynchronously — as can happen with a cancel very soon after the prompt was submitted; its interrupt entry
  and its `✳` can then both be missed;
- an event from the cancelled turn reaches Octoboard after the move, reporting the session as *working* again;
- for a cancel before any output, which only the title shows: Claude Code has been told not to set its terminal title,
  or the title showing the turn running and the `✳` after it reached Octoboard in one piece of output, of which only the
  last title is seen (rare);
- for a cancel after output: the interrupt entry carries no timestamp Octoboard can read, and the title misses it too.

**What this costs to keep working.** As with a decline, the transcript is a file format Claude Code owns and rewrites on
upgrade, and a cancel is recognised there by the two fixed interrupt strings above; they were seen in real transcripts
of Claude Code 2.1.296, 2.1.302 and 2.1.303, where a cancel while the model thinks wrote `[Request interrupted by user]`
too, and of 2.1.310 for cancels after output. The title is read by its fixed glyphs, `◐`/`◑` and `✳`, measured on Claude
Code 2.1.310. A release that renames the strings or changes the glyphs breaks this silently — there is no error and
nothing is reported as having failed — and the only symptom is a cancelled turn reading *working* until the next prompt.

## Cancelling a Grok Build turn before its first token

Grok Build stops a turn on Ctrl-C (Esc does nothing there). Once the model has started answering, the cancel fires
Grok's own `StopCancelled` hook event and the session moves to *awaiting instructions* on it. A Ctrl-C **before the
first token arrives** fires no hook event at all — neither `StopCancelled` nor a turn end — and Grok puts the prompt
back in its composer. Left at that, the session would read *working* until Grok's turn-end backstop about a minute
later, which would then find the turn open and quiet and synthesise a report saying it ended with nothing to say. So
while the session works Octoboard watches its terminal title:

- **The terminal title.** Grok Build starts its title with a braille spinner while a turn runs, keeps it through tool
  calls, and drops it within a few tens of milliseconds of the cancel. The session moves once three things hold: the
  title has shown no spinner for about a second after showing one, Octoboard has applied every hook event it has
  received for the session, and the session still reads *working*. A turn that ends normally drops the spinner too, but
  its hook event has moved the session by then, and a session it moved stays where it put it.

What follows the move:

- The turn counts as cancelled: no report is synthesised for the session's owner for it, and the backstop that comes
  about a minute later finds no turn to end (see "When a session does not report" in
  `docs/product/reporting-to-owners.md`). A message held while the turn ran is released.
- Should Grok still report a turn end for that turn afterwards, that turn end is taken as how the turn really ended, and
  the turn is reported on as any other is.
- The prompt Grok put back in its composer stays there, and a message Octoboard writes from then on goes in after it. A
  cancel made by `interrupt_session` clears it first (see "The input box afterwards" in
  `docs/product/interrupting-turns.md`); a cancel the user makes in the terminal is left as Grok leaves it.
- The agent's process is still running, so the session is *awaiting instructions* and not *interrupted*.

How the watch is kept to the right turn:

- **When the watch runs.** It starts when a submitted prompt moves the session into *working*, and again whenever a hook
  event brings the session back to *working* from waiting for the user, so what the title showed while a card waited on
  the user is not counted against the turn that carries on after it. Each submitted prompt retires the previous turn's
  watch before its own turn opens. While a session request holds the session's hand up, it waits as the Claude Code
  title watch does (see "Cancelling a Claude Code turn" above).
- **Not while a screen of the agent's own is up.** While one of Grok's screens that hold messages is on the session's
  screen (see "Messages held until a session can take them" in `docs/product/message-delivery.md`), a title without a
  spinner is not counted.
- **Only this turn's cancel counts.** A title without a spinner counts only after a spinner the title showed when the
  watch started or later.

Where it is not recovered, the session keeps reading *working* until Grok's backstop or its next prompt: the cancel came
before Octoboard applied the hook event that moved the session into *working*, or the title showing the turn running and
the title after it reached Octoboard in one piece of output, of which only the last title is seen.

**What this costs to keep working.** The title is read by Grok's braille spinner, and the cancel's lack of any hook
event was measured on Grok Build 1.0.70. A release that changes the title breaks this silently — there is no error — and
the symptom is the one above: a cancelled turn reading *working* until the backstop, and a report synthesised for it.

## A Codex turn that ends in an error

A Codex turn that **ends in an error** — the account's usage limit reached, for one — is reported by no hook event at
all: Codex has no failure event and no notification event, so nothing follows the turn's prompt submission, while the
agent is already back at its prompt. So for this one case Octoboard reads the session's own rollout file, the record
Codex keeps of the session, and moves the session to *awaiting instructions* once the turn appears there as ended with
an error — normally within about a second of the prompt, and so up to about half a second behind Codex's own return to
its prompt. Every turn of a session is covered, its first included.

- The agent's process is still running, so the session is *awaiting instructions* and not *interrupted*.
- The turn is reported on: a session bound to an owner has a report synthesised for it with status *failed*, carrying
  Codex's own error text and the usual line saying the session stopped without reporting (see "When a session does not
  report" in `docs/product/reporting-to-owners.md`).
- **Codex sessions only.** Claude Code and Grok Build report a failed turn through their own hook events, and the file
  read here is Codex's own format; neither is watched this way.
- A turn Codex ends normally, and one the user cancels, are reported by Codex's own hook events, and nothing is read
  from the rollout for them.
- If the record cannot be read — the prompt submission named no rollout file or no turn, or the turn's end never appears
  there — nothing moves the session, and it keeps reading *working* until something else does.

**What this costs to keep working.** The rollout is a file format Codex owns and rewrites on upgrade, and a failed turn
is recognised by the shape of the record Codex writes for the turn's end; the behaviour was measured against Codex
0.160.0. A release that changes that shape breaks this silently — there is no error and nothing is reported as having
failed — and the only symptom is a session reading *working* while Codex sits at its prompt.
