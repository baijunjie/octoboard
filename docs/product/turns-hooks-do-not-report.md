# Turns the agents' hooks do not report

A session's status follows the hook events its agent reports (see "What the statuses are derived from" in
`docs/product/session-statuses.md`). A few ways a turn ends, or a raised hand comes down, are reported by no hook event,
and Octoboard reads them from somewhere else instead: the agent's own record of the conversation, or its terminal title
or the status it reports in its terminal.
This doc covers each of those cases, what Octoboard reads for it, and where that reading fails. They apply whoever
caused them: a cancel the user makes in the terminal and one an owner's `interrupt_session` makes (see
`docs/product/interrupting-turns.md`) are recovered the same way. Each ending recovered here leaves the session *paused*
(see "Paused" in `docs/product/session-statuses.md`).

## Declining a Claude Code prompt or question

A Claude Code session with its hand up that is then **declined** — the permission prompt answered No or dismissed with
Esc, or the agent's own ask-the-user question cancelled — reports nothing at all: no hook event of any kind follows,
while the agent is already back at an empty prompt. So for this one case Octoboard reads the session's own transcript
file, the record Claude Code keeps of the conversation, and moves the session to its prompt once the
decline appears there, normally within about a second of the user answering. Both ways a Claude Code session raises its
hand are covered: a permission prompt and its own ask-the-user question.

What follows from that move is nothing special to this path: the raised hand comes down wherever the sidebar showed it,
the Dock badge count drops, and a message queued for the session while its hand was up is released (see "Messages held
until a session can take them" in `docs/product/message-delivery.md`). A later prompt or question in the same session
raises the hand and notifies afresh, as any other does.

- The agent prints that the turn was interrupted, but its process is still running, so the session is at its prompt,
  *paused*, and not *interrupted*.
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
  carries on; Octoboard reads that as a decline too, so the session reads as at its prompt for a moment until
  the agent's next event puts it back to *working*.
- If the record cannot be read — the pending-decision event named no transcript file, or the file is unreadable —
  nothing lowers the hand, and the session keeps it up until something else moves its status.
- A decline is noticed the same way while a session request holds that session's hand up (see "Waiting for the answer"
  in `docs/product/session-requests.md`). The hand stays up until the request ends, since it is the request's own; the
  session is then at its prompt, *paused*, where the decline left its agent.

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
the session to its prompt as soon as either shows the cancel, normally within about a second of it:

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

  Once the terminal answers its OSC 7501 support query, Claude Code 2.1.322 drops these glyphs from its title and
  reports its state instead: `working` when a turn starts, `idle` when it is cancelled (about 80 ms after the Esc,
  before output or after it), `done` when it ends normally. From its first report on, the title watch reads the
  reports instead of the title's glyphs: `working` stands for the turning circle, and any other state, or the record
  gone, for `✳`, under the same three conditions (see "Reports as the sign that a turn is running" in
  `docs/product/program-status-reports.md`). Should it report `blocked`, a turn waiting on the user (not measured), that
  counts as `✳` does; the hook event that raises the hand moves the session out of *working* itself.

After a cancel before any output, Claude Code puts the prompt's text back in its input box. For a message Octoboard
has for the session from then on, whether the session reads *working* or at its prompt at the time, that text is a
draft like one the user typed (see "Messages held until a session can take them" in
`docs/product/message-delivery.md`): where the agent's native stash is used, it is set aside around the message and
put back after; otherwise the message waits out the draft protection grace period and then goes in after that text
and is sent together with it. A cancel made by `interrupt_session` clears that text first (see "The input box
afterwards" in `docs/product/interrupting-turns.md`); a cancel the user makes in the terminal is left as the agent
leaves it.

- **When the watches run.** Both start each time a hook event moves the session into *working* — a submitted prompt, or
  any other event that finds it not already working, such as the hand coming down on an answered prompt — and stop once
  the session leaves *working* some other way, raises its hand, or its process ends. While a session request holds the
  session's hand up (see "Waiting for the answer" in `docs/product/session-requests.md`), the title watch waits: a
  cancel the title showed meanwhile moves the session once the request ends, if it is then back at *working*.
- **A cancel Claude Code carries straight on from leaves the session *working*.** Esc pressed while the user has a
  message waiting in Claude Code's queue cancels the turn and runs that message as the next one, with no hook for it.
  When the cancel's interrupt is read with the queue handing the message over beside it (within a second), or with the
  message's own prompt entry written under the interrupt as a new turn, the cancelled turn counts as ended but the
  session stays *working*, and the watch carries on for the turn now running: that turn's own cancel still moves the
  session, and its end leaves the session idle. A message taken into the running turn at a tool boundary starts no
  turn, and a cancel after it is an ordinary one.
- **Only this turn's cancel counts.** Claude Code stamps every user entry of a turn, its interrupt included, with the
  turn's id (`promptId`), and passes the same id in its hook events (`prompt_id`). An interrupt counts only if it names
  the turn of the hook event that started the watch, or, once the watch carries on for a turn it ran straight on to,
  that turn. Where the hook event or the entry names no turn, only the rules that follow apply. Only transcript entries written after the watch started are read, and an interrupt
  counts only if it is timestamped no earlier than a few seconds before the watch started, so an earlier turn's
  interrupt, and one in the history of a resumed or copied conversation, does not. An interrupt replayed inside a logged
  request does not count either, for the reason given in "Declining a Claude Code prompt or question" above. A `✳`
  counts only after a running glyph the title showed when the watch started or later, so the `✳` the previous turn left
  on the title does not end this one. Each submitted prompt retires the previous turn's watches before its own turn
  opens, so a cancel found late never ends the turn after it.
- **Not while a screen of the agent's own is up.** While one of the Claude Code screens that hold messages is on the
  session's screen (see "Messages held until a session can take them" in `docs/product/message-delivery.md`), a `✳` is
  the turn waiting on the user and is not counted.
- The agent's process is still running, so the session is at its prompt, *paused*, and not *interrupted*.
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
- for a cancel before any output, which only the title or the reports show: Claude Code has been told not to set its
  terminal title and reports no status, or the title showing the turn running and the `✳` after it reached Octoboard in
  one piece of output, of which only the last title is seen (rare);
- for a cancel after output: the interrupt entry carries no timestamp Octoboard can read, and the title misses it too.

**What this costs to keep working.** As with a decline, the transcript is a file format Claude Code owns and rewrites on
upgrade, and a cancel is recognised there by the two fixed interrupt strings above; they were seen in real transcripts
of Claude Code 2.1.296, 2.1.302 and 2.1.303, where a cancel while the model thinks wrote `[Request interrupted by user]`
too, and of 2.1.310 for cancels after output. The title is read by its fixed glyphs, `◐`/`◑` and `✳`, measured on Claude
Code 2.1.310, and the reports that replace them by their states, measured on Claude Code 2.1.322. A release that
renames the strings, or changes the glyphs or the reports, breaks this silently — there is no error and
nothing is reported as having failed — and the only symptom is a cancelled turn reading *working* until the next prompt.

## Cancelling a Grok Build turn before its first token

Grok Build stops a turn on Ctrl-C (Esc does nothing there). Once the model has started answering, the cancel fires
Grok's own `StopCancelled` hook event and the session moves to its prompt on it. A Ctrl-C **before the
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
- The agent's process is still running, so the session is at its prompt, *paused*, and not *interrupted*.

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
Codex keeps of the session, and moves the session to its prompt once the turn appears there as ended with
an error — normally within about a second of the prompt, and so up to about half a second behind Codex's own return to
its prompt. Every turn of a session is covered, its first included.

- The agent's process is still running, so the session is at its prompt, *paused*, and not *interrupted*.
- The turn is reported on: a session bound to an owner has a report synthesised for it with status *failed*, carrying
  Codex's own error text and the usual line saying the session stopped without reporting (see "When a session does not
  report" in `docs/product/reporting-to-owners.md`). A turn that stopped because the selected model was at capacity is
  retried first, and its report held, as described under "When the selected model is at capacity" below.
- **Codex sessions only.** Claude Code and Grok Build report a failed turn through their own hook events, and the file
  read here is Codex's own format; neither is watched this way.
- A turn Codex ends normally, and one the user cancels, are reported by Codex's own hook events, and nothing is read
  from the rollout for them.
- If the record cannot be read — the prompt submission named no rollout file or no turn, or the turn's end never appears
  there — nothing moves the session, and it keeps reading *working* until something else does.

### When the selected model is at capacity

A Codex turn that failed because **the selected model was at capacity** — the rollout records the turn's error with
`codex_error_info` `server_overloaded`, and Codex's own text says the selected model is at capacity and to try a
different model — is retried automatically instead of being reported at once. The failure can come after a turn that
has already been working for a long time. This is always on, with no setting, and only for Codex; every other error is
reported at once as above.

- **The retries.** There are at most 3, sent 30 seconds, 60 seconds and 120 seconds after the failure they retry. Each
  is one Octoboard message written into the same running process, saying that the selected model was at capacity so the
  last turn stopped, that Octoboard is retrying automatically (retry *n* of 3), and asking it to continue its task.
  Nothing else is done: the session is never relaunched, its model, reasoning effort and account are never changed, and
  the automatic account switch does not judge the turn (see "When a session is switched" in
  `docs/product/automatic-account-switch.md`). Turning that setting on or off does not affect a retry.
- **While a retry waits**, the session is at its prompt, *paused*, as any failed turn leaves it. Its owner — when it is
  bound to one and had not reported in the failed turn — is not given the failed report yet: it is told instead that the
  session's model was at capacity so its turn stopped, that Octoboard will retry automatically in the given number of
  seconds (retry *n* of 3) in the same session, and not to send it a continue prompt itself. The failed report is held.
  A session that had already reported in that turn, or has no owner, is retried all the same, with nobody told.
- **Failures in a row are counted.** The count starts again only when a turn of the session ends without a capacity
  failure, or when a wait is ended in a way that gives the owner the held report (below). The fourth capacity failure in
  a row is not retried: it is reported as any failed turn is, and the session is left
  *paused*; the report adds that Octoboard retried automatically 3 times and the model was still at capacity, and that
  the user can retry later or switch the session's model manually.
- **The turn a retry starts** is reported on like any other: when it ends without the session calling `report`, its
  owner gets the usual synthesised report (see "When a session does not report" in
  `docs/product/reporting-to-owners.md`).

**Ending a wait without a word.** A turn of the session starting — the user submitting a prompt in its terminal, say —
or a message being queued for it, such as an owner's instruction, ends the wait: no retry is sent and the owner hears
nothing more of that failure. The session is at work again, and that turn is reported on as usual; the count stays for
that turn's end to settle.

**Ending a wait with the held report.** The wait also ends, with no retry sent, on any of these:

- the turn being cancelled — by an owner's `interrupt_session` or a cancel Codex reports;
- the session being resumed or restarted, switched to another account, or having its model or reasoning effort changed;
- the session found, when the wait is up, no longer as the failure left it: its process replaced, anything typed or
  written into its terminal, another turn ended, its account, model, effort or conversation changed, or it is *waiting
  for the user* or *dormant* rather than at its prompt;
- the retry message being refused because the session cannot take a message at that moment, or failing to be
  written — Codex's own queue refusing it included (see "A message Codex's own queue refuses" in
  `docs/product/message-delivery.md`). A retry message that Codex's own queue may have taken without confirming it
  counts as sent and does not end the wait (see "A message whose acceptance is uncertain" in
  `docs/product/message-delivery.md`).

The owner is then given the failed report it was told would not come, carrying Codex's error text and saying that
Octoboard's automatic retry did not go ahead and that the user can retry later or switch the session's model manually.
That report never reaches the owner before the notice that a retry was coming. A resume, a restart, an account switch
or a change of model or effort ends the wait as it starts, and the report is delivered only while the session still has
its process when the report is delivered: one whose relaunch has already ended the process by then is dropped, as on an
exit, and that operation's own result or notice tells the owner what became of the session. No report follows for a session that has
been archived or deleted, which ends its wait too. When the session's process exits during a wait, the wait ends and the
report cannot be delivered; the owner is told instead that the session ended on its own (see "Notices to an owner" in
`docs/product/reporting-to-owners.md`).

Waits are held only in the daemon's memory and do not survive a restart of the daemon.

**What this costs to keep working.** The rollout is a file format Codex owns and rewrites on upgrade, and a failed turn
is recognised by the shape of the record Codex writes for the turn's end; the behaviour was measured against Codex
0.160.0, and the capacity failure's `server_overloaded` on rollouts of Codex 0.162. A release that renames that value
breaks only the retry: such a turn is then reported at once as any other failed turn. A release that changes that shape breaks this silently — there is no error and nothing is reported as having
failed — and the only symptom is a session reading *working* while Codex sits at its prompt.
