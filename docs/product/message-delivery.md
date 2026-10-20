# Message delivery

How Octoboard writes into a running session everything it has to put there — an instruction from an owner, a report,
information shared by another session, a report panel form submission — and when it holds one back.

## Messages held until a session can take them

A session that is working, awaiting instructions or running in background takes a message straight away; every agent
queues one written mid-turn and consumes it when the turn ends.

**A message is not written verbatim.** Its text is reshaped in three steps, in this order:

1. Every control character except newline and tab is removed.
2. Every character Claude Code treats as invisible is removed. Claude Code does not send a prompt that holds one: it
   removes them, leaves the cleaned text in its input box for the user to review and send, and starts no turn, so the
   Enter Octoboard writes after the message would be spent on that hold and the message would sit unsent — one written
   mid-turn included — while its sender was told it had been delivered. The usual carriers are the bidirectional
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
can be model-authored and none is reviewed first. Octoboard does not check that the agent submitted what it wrote: a
message counts as delivered once it has been written.

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
