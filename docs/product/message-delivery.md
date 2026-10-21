# Message delivery

How Octoboard writes into a running session everything it has to put there — an instruction from an owner, a report,
information shared by another session, a report panel form submission — and when it holds one back.

## Messages held until a session can take them

A session that is working, awaiting instructions or working in background takes a message straight away, unless it is
a Claude Code session held behind a message its agent did not send (see "A message written but not sent" below); every
agent queues one written mid-turn and consumes it when the turn ends.

**A message is not written verbatim.** Its text is reshaped in three steps, in this order:

1. Every control character except newline and tab is removed.
2. Every character Claude Code treats as invisible is removed. Claude Code does not send a prompt that holds one: it
   removes them, leaves the cleaned text in its input box for the user to review and send, and starts no turn, so the
   Enter Octoboard writes after the message would be spent on that hold and the message would sit unsent, one written
   mid-turn included (see "A message written but not sent" below). The usual carriers are the bidirectional
   isolates (U+2068, U+2069) the UI wraps values in, which agents copy from what the UI shows. The set follows Claude
   Code 2.1.304: U+00AD, U+034F, U+061C, U+115F, U+1160, U+17B4, U+17B5, U+180B–U+180F, U+200B–U+200F, U+2028–U+202E,
   U+2060–U+206F, U+3164, U+FE00–U+FE0F, U+FEFF, U+FFA0, U+FFF0–U+FFFB, U+1107F, U+13430–U+1343F, U+16FE4,
   U+1BCA0–U+1BCA3, U+1D173–U+1D17A and U+E0000–U+E0FFF. The line and paragraph separators (U+2028, U+2029) become
   newlines instead. Inside emoji the characters Claude Code keeps are kept: a zero-width joiner between two
   pictographs, a text or emoji presentation selector (U+FE0E, U+FE0F) right after an emoji, and one between a digit,
   `#` or `*` and the keycap mark U+20E3. Claude Code's exceptions that depend on the surrounding script — a joiner
   among Arabic or Indic letters, a zero-width space in Thai and similar scripts, a direction mark in a right-to-left
   line — are not kept: those characters are removed. Nor is its exception for the England, Scotland and Wales tag
   flags: their tag characters are removed, so each arrives as a plain black flag. Private-use and unassigned
   characters, and format characters outside the set, pass through. The removal applies whatever the session's agent;
   Codex and Grok Build have not been measured for such a hold.
3. A message that would otherwise begin with `/` is written with a single leading space, so it reaches the model as
   text instead of being run as one of the agent's own slash commands. The check is made on the text left after the
   removals, so a removed character in front of a slash command does not keep it from being guarded.

This holds for everything Octoboard writes into a running session — an instruction, a report, a report panel form
submission, information shared by another session, the instruction `reopen_session` hands over — because all of them
can be model-authored and none is reviewed first.

**A blank message is refused.** A message passed to `send_message` — a client's request or an owner's tool — that is
nothing but whitespace once its control characters and invisible characters are removed is not written: a client is
refused with `field_required` (field `text`), an owner's tool with that reason in prose.

An instruction, a report or information shared by another session, for a session that **cannot** take one right now — it
is waiting for the user at a permission prompt or a question — is queued rather than dropped, and delivered as soon as
the session can take one, oldest first. A session that sent an instruction or shared information is told it was queued
and that it must not send it again. The wait can outlast the user's answer: a permission prompt reported through hooks
keeps the session's hand up until the call it let through has finished. A Codex action title also holds messages;
when it clears, delivery resumes if the last hook-reported status permits it (see "The raised hand" in
`docs/product/sessions.md`).

A message for a session with no process running is refused outright: a resume starts the agent at
its prompt and replays nothing. Anything still queued when a session's process ends goes with it, unless the session
is being handed off to another agent, whose queue is kept for the session that takes its place (see
"Messages for the original during a handoff" in `docs/product/handing-off-sessions.md`).

**Messages are also held while a session's trust confirmation may be up.** A message's trailing Enter would answer
the agent's own folder-trust confirmation (see `docs/product/folder-trust.md`) — accepting it, and so trusting the
folder, at Codex's and Grok Build's, and declining it, which exits the agent, at Claude Code's. So from every launch of
a session, a resume included, everything Octoboard writes into it — an instruction, a report, information shared by
another session, a report panel form submission, the instruction `reopen_session` hands over — is queued as above
until the confirmation can no longer be on screen:

- for Claude Code and Grok Build, until the session's first hook report, which each sends only once it is past the
  confirmation (and, for Claude Code, past any other startup screen that follows it). A session whose hook reports
  never arrive holds its messages for as long as it runs;
- for Codex, which reports nothing until its first prompt, until Octoboard's watch for the confirmation ends with none
  shown — normally at the first terminal title Codex sets, once its session has started; when that watch ends is in
  "Recognition is limited to the start of a session" in `docs/product/folder-trust.md`. Once a confirmation is shown,
  they are held until Octoboard's press of it succeeds, a hook report arrives, or the person's own answer in the
  terminal is seen to replace it on screen. A session whose terminal title is left on but which never sets one nor
  sends a hook report holds its messages for as long as it runs.

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

## A message written but not sent

Claude Code can take a message Octoboard writes and still not send it: the text stays in its input box and no turn
starts. Invisible characters are one cause, which their removal above is there to prevent; anything else that leaves a
message unsent is caught the same way.

**The check.** When Octoboard writes a message into a Claude Code session whose agent is at its prompt — awaiting
instructions or working in background, not mid-turn — it waits up to about three seconds for the agent's hooks to report
a prompt sent (`UserPromptSubmit`). This applies to every message Octoboard writes: an owner's instruction, a report,
shared information, a report panel form submission, the instruction `reopen_session` hands over, the messages of a
handover or a handoff, and a client's own `send_message`. A message that was queued is checked the same way when it is
finally written. When several queued messages are written in one go, each is checked on its own, against the agent's
status at the moment it is written: once the first is sent the agent reads as working, so the rest are written
unchecked, as mid-turn messages; if the agent is at its prompt again by then — after a very short turn, say — the next
one is checked too.

- With the report in time, the message counts as delivered.
- Without it, the message is taken to be waiting in the session's input box: it is **left unsent**.

A background task's completion does not count as a prompt sent. Claude Code reports a finished background task to
itself as a `UserPromptSubmit` whose prompt contains `<task-notification>`; that prompt neither confirms a message
written during the wait nor lifts the hold below. Any other prompt counts, whoever sends it.

**What a message left unsent does:**

- It is not written again, put back in the queue or changed: its text is already in the session's input box.
- Every client is sent the `message_not_sent` notice about that session, naming its agent, provided the session's
  process that was written to is still running when the check ends. The window shows it as an error alert (see
  `docs/product/error-alerts.md`) titled with where the session is, saying the agent did not send a message Octoboard
  wrote into the session, that the message is waiting in its input box, and that nothing more is written into the
  session until a prompt is sent; the remedy is to open the session, check the message in its input box and press Enter
  to send it.
- Nothing more is written into the session until its agent next reports a prompt sent, whoever sends it: the user
  pressing Enter on the waiting message, or typing and sending something else. Messages arriving meanwhile are queued,
  and are then delivered in order, oldest first.
- The hold belongs to the session's process that was written to and ends with it, as its queue does: a session resumed
  later starts without one.

**What the sender is told.** A sender whose own message was left unsent is told it was written into the session's input
but its agent did not send it, so the session has not read it; that the user has been told; and that it must not send
it again. How each tool words this, and what follows from it, is with the tool: the owner's `send_message` in "The
console session's tools" in `docs/product/hub-orchestration.md`, `report` in `docs/product/reporting-to-owners.md`,
`share_info` in `docs/product/information-between-sessions.md`, `transfer_session` in "Handing a session to another
owner" and `bind_to_session` in "Binding an unbound session to an owner", both in
`docs/product/changing-session-owners.md`. A sender whose message arrives while the session is held behind an unsent
one is told it was queued, as for a session that cannot take a message. A sender already told its message was queued
learns nothing more if that message is later left unsent; the notice is what reports it.

**A client's `send_message`** to a Claude Code session at its prompt is answered once the check has ended, not as soon
as the message is written. A message left unsent is not refused: the client is still answered with success, and, like
every client, gets the `message_not_sent` notice.

**What is not checked:**

- A message written while the agent is mid-turn. The agent queues it and sends it only when the turn ends, which no
  short wait can see, so a mid-turn message Claude Code holds back is not detected and counts as delivered.
- Codex and Grok Build sessions: a message counts as delivered once it has been written.
- Which prompt was sent. The prompt's text is not matched against the message: a prompt the user sends within those
  seconds is taken as the message sent.

**What the check gets wrong.** In two cases a message the agent has in fact queued is reported not sent, with the
notice and the hold that follow, and a `done` report written then archives nothing (see "Automatic archiving" in
`docs/product/reporting-to-owners.md`). In both, the hold lifts when the agent sends the queued message at the end of
its turn.

- After "Chat about this" is chosen on a question, Octoboard reads the Claude Code session from its transcript as back
  at its prompt while its agent is in fact still working, so a message written then is checked.
- A background task's completion can arrive during the few seconds' wait. The agent starts a turn on it and queues the
  message just written until that turn ends, and the completion does not count as a prompt sent. This case is inferred
  from how the check works and has not been observed.
