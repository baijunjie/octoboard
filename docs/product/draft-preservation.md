# Draft preservation

How Octoboard keeps a message it writes into a running session's terminal — an instruction from an owner, a report,
information shared by another session, a report panel form submission, a notice (see `docs/product/message-delivery.md`)
— from being sent together with text the user has in the session's input box (the composer), and what it tells the
user when it has to set that text aside or clear it.

## Draft preservation has a bounded wait

On supported launches, Octoboard first tries to deliver the automatic message separately while preserving the user's
draft, without waiting for the user to finish typing:

- **Claude Code 2.1.274 or later and Grok Build 1.0.70 or later:** when Octoboard recognises a nonempty composer and
  can safely use the agent's native stash, it saves the draft, confirms the stash and an empty composer, then sends
  the automatic message. The agent restores the draft automatically after submission; Octoboard does not toggle the
  stash a second time. An existing stash of the user's is never overwritten by another stash operation. The stash key
  is pressed only on a composer read off a whole frame (the first condition under "Keys that leave the composer empty
  are confirmed by the screen" below): Octoboard waits up to half a second for one, and a composer that shows text off
  a frame still being drawn — as at the start of a turn, when the box can be read before it is drawn again — holds the
  message for a later attempt rather than have the key land on a box that may be empty.
  Claude Code additionally requires a verifiable Ctrl-S stash binding in the configuration this launch uses; an
  overridden binding or an unreadable or unrecognised keybindings file disables this path.
- **Codex 0.166.0 or later, on a launch with its private app server** (see "Codex's private app server" in
  `docs/product/per-agent-launch-specifics.md`): automatic messages go into Codex's own queue and leave the composer
  untouched, so none of the composer checks, the native stash or the three-second grace period below applies to them
  (see "Messages through Codex's own queue" in `docs/product/message-delivery.md`).
- **Any other Codex launch:** Octoboard uses no stash of Codex's, so its automatic messages take the guarded terminal
  delivery below.

Octoboard reads the composer off the session's screen with faint text left out, so Claude Code's hints in an empty
box — a fresh session's placeholder, and "Press up to edit queued messages" while a message waits for the turn to
end — read as an empty composer; text typed there replaces the hint and reads as a draft. A message Octoboard has
just written into an empty composer can still be drawn there for a moment before the agent takes it, so before the
next message is checked for a draft, Octoboard waits up to 500 ms after that write for the composer to read empty; only
text still there after that counts as a draft. Octoboard therefore does not stash a message of its own.

During a native stash and restoration, the user's continued typing is temporarily retained and replayed in order after
the draft's restoration is confirmed, released as described below if confirmation times out, or discarded if the message
reaches the input only in part (see "A message only partly written while the draft is stashed" below). Output and
terminal responses continue meanwhile. If a dialog is shown during recovery, earlier retained keystrokes wait until it
is gone; newly typed answers can answer the dialog once restoration is confirmed or the protection times out, and are
not added to the draft's tracked edits. Successful restoration preserves multiline text and long collapsed pastes.
Claude Code retains the cursor position; Grok Build restores the cursor at the end. Attachment restoration has
not been verified and is not guaranteed.

Saving and restoring a native stash share one three-second confirmation window. If saving is not confirmed in time,
Octoboard sends the `draft_save_unconfirmed` notice, releases retained typing into the current input and writes
nothing of the message there: within the message's grace period below it stays queued until the composer reads empty,
and past it the composer is cleared first (see "Clearing a draft past the grace period" below). It does not send a
second stash key. If the complete message was written but restoration is not confirmed in time, Octoboard sends the
`draft_restore_unconfirmed` notice and releases retained typing into the current input without repeating the message.
After either timeout, and after the confirmation window runs out with a stash unsettled, the stash may still hold the
draft without the screen showing it, and a second stash key could overwrite it; so no stash key is pressed for any
message until the agent reports a prompt sent, which restores what the stash holds. A refused stash key (a dialog came
up, the process ended) sends nothing, and the next attempt tries the stash again. A dialog still holds earlier retained
keys until it closes; after a timeout they can be released without a stash marker confirming restoration. While
retained typing cannot all be replayed yet — a dialog is up or the terminal is not taking it, also once the
confirmation window has run out with the stash unsettled — Octoboard sends the `typing_held` notice. If the process ends
before restoration is confirmed, Octoboard sends the `typing_lost` notice: the retained typing is not replayed into a
resumed session.

## The draft and typing notices

Each is sent to every client about the session, naming its agent (`agent`); `typing_lost` also carries `bytes`, the
amount of retained typing lost, and `draft_discarded` the text the draft held, as `detail`, and `cleared: "part"` when
the composer did not empty. Of `draft_save_unconfirmed`, `draft_restore_unconfirmed` and `typing_held`, one stash and
its recovery send only the first that applies; `draft_discarded`, `typing_blocked` and `typing_lost` are sent whenever
they apply. The window shows each as an error alert (see `docs/product/error-alerts.md`) naming the session and where it
is: a known failure with no remedy beyond its own text, not an unexpected one.

| Notice | Alert title | What the alert says |
|---|---|---|
| `draft_save_unconfirmed` | "Draft save not confirmed" | The agent did not confirm setting the draft aside, so Octoboard did not write its message there; the message waits until the input box is empty, and the user is to send or clear the draft, or Octoboard clears it shortly and shows what it held. |
| `draft_discarded` | "Draft cleared" | Octoboard cleared the draft from the input box so that its message would not be sent together with it; to keep the draft, the user copies it from the block below the text and pastes it back into the input box. |
| `draft_discarded` with `cleared: "part"` | "Draft partly cleared" | Octoboard cleared only part of the draft so that its message would not be sent together with it; the message waits until the input box is empty, and Octoboard tries again once the box changes. To keep the draft, the user copies it from the block below; part of it may still be in the input box. |
| `draft_restore_unconfirmed` | "Draft restore not confirmed" | The agent did not confirm putting the draft back after the message was sent; the user is to check the input box before sending what it holds. Its details add that what was typed while the draft was set aside goes into the input box as it is now, and that the message is not sent again. |
| `typing_held` | "Typing held back" | Some of what was typed while Octoboard wrote a message has not reached the agent yet; the user is to answer any dialog the session shows. Its details add that the agent is showing a dialog or not taking input, and that what was held back is typed in, in order, once it takes input again. |
| `typing_blocked` | "Message partly written" | Only part of a message Octoboard wrote reached the input box, and it will not be sent again; the user is to check the input box and remove what is left of the message before sending a prompt. Its details add that what was typed while the message was written is discarded on the next key, that typing goes straight to the agent from then on, and that Octoboard's other messages wait until a prompt is sent. |
| `typing_lost` | "Typing lost" | The session ended before the agent confirmed putting its draft back, so what was typed meanwhile is lost and is not typed in when the session is resumed. |

The draft a `draft_discarded` alert hands back is shown in a block of its own after the alert's text, under a label
naming it as the draft's content, with a Copy button: the text as the input box showed it before the first clearing
key, its rows joined by line breaks, in its own direction, its long lines wrapped. A long paste the agent shows
collapsed (Claude Code's `[Pasted text #1 +24 lines]`) is only that placeholder, so its text cannot be got back from
the alert. How the block is shown is in "Names and diagnostic details" in `docs/product/error-alerts.md`.

## The draft grace period

**When separate delivery cannot be used, delivery takes priority after three seconds.** Each message gets a draft
protection grace period from its first delivery attempt; retries and continued typing do not reset it. The exception is
a message refused while the session's input is held for draft preservation — a draft being stashed or restored, or a
partly written message (see "A message only partly written while the draft is stashed" below): its grace period starts
again from its first attempt after the hold ends. So does a message a Codex session's private app server held back
before the terminal took it (see "Messages through Codex's own queue" in `docs/product/message-delivery.md`): its grace
period starts again from its first attempt after the server first held it back. It starts again that way once per
message only, so time the server holds the message back after that counts toward the grace period. This grace period
does not raise the session's hand.

During the grace period, ordinary terminal delivery requires a recognised empty composer and confirmation that
preceding user edits were submitted. A delayed report for an earlier prompt, an automatic message or a background
completion does not confirm a newer draft. An unsupported stash, an existing stash, an unrecognised composer,
unconfirmed edits or an incomplete input sequence can hold delivery temporarily; an Esc typed on its own counts as a
complete key once it is 500 ms old, so it holds nothing as an incomplete sequence after that, and does not keep the
native stash from being used.

Once the grace period has elapsed, the next delivery attempt no longer waits for edits to be confirmed, but a message
is not written together with text the composer shows: a composer that reads empty takes the message directly, one that
shows text has it stashed around the message where the agent's native stash can be used, and where it cannot, the
composer is cleared first, or the message keeps waiting (see "Clearing a draft past the grace period" below). Such a
write still waits while a key the user typed less than half a second ago may have put text into the composer that is
not drawn yet.

A composer read empty off a frame not drawn whole (see the first condition under "Keys that leave the composer empty
are confirmed by the screen" below) holds the message for 2 seconds of retries in all, since an agent that keeps
redrawing, as one mid-turn can, may never show a whole frame: the retries refused for that reason form a run, which goes
on while each comes within 10 seconds of the one before, and ends once a message is written, the composer is read off a
whole frame or it shows text. Once the run has lasted 2 seconds, the message is written on such a reading, unless
Octoboard's record of the typing knows of text typed into the composer that nothing since removed: such a frame may hide
that text, so the message keeps waiting for a whole frame, and the reading neither starts the run nor ends it. A retry
held for another reason — a key typed less than half a second ago, or an unreadable composer (below) — neither starts
the run nor ends it.

A composer that cannot be read at all, while any key typed into it is not yet confirmed (whatever it typed or removed)
or a key that can put unknown text into it (see below) still holds, holds the message too, past the grace period as
well, until the composer can be read again or a typed prompt is matched: an unreadable composer cannot show that the
keys left it empty, and a view that hides the composer, such as a picker, would take the message's keys. Claude Code's
composer is read while it shows a prompt recalled from history under its `History` rule.

Draft protection therefore does not wait indefinitely for an ordinary prompt or a new process. Delivery does wait, with
no bound, where writing the message could put it together with text in the composer: a composer that shows text no stash
or clearing key can take (Codex's), one that shows text only off frames still being drawn, as an agent redrawing
without pause can keep it, one that holds only a message Octoboard wrote before (see "Clearing a draft past the grace
period" below), and an unreadable composer while keys typed into it are unconfirmed.

**What still goes in with text the screen does not show.** Past the grace period, a composer that cannot be read while
every key typed into it is confirmed takes the message as it is, and so does a composer read empty off frames not drawn
whole once the 2-second run is over, when Octoboard's record of the typing knows of no text left in it. Text in such a
composer that the screen did not show — drawn late, or hidden by a frame read in part — is then sent together with the
message.

A message queued while keys held the session is written as soon as the composer is confirmed empty, unless another
hold applies, rather than waiting out the grace period: while a message is queued, the session's screen is checked for
this about every quarter of a second.

The deadline never answers a recognised dialog or trust confirmation, overrides an agent's permission wait, or writes
into an ended or frozen session. Such holds must clear first. A partial terminal write remains a delivery failure;
Octoboard does not repair a broken paste or replay retained typing into it (see "A message only partly written while
the draft is stashed" below, and "Messages held until a session can take them" in `docs/product/message-delivery.md`
for a partial write outside a stash).

## Clearing a draft past the grace period

Past a message's grace period, where the agent's native stash cannot take the draft — no stash on the agent or on this
launch, a stash already shown, a stash left unsettled (see "Draft preservation has a bounded wait" above) — Octoboard
clears the composer before it writes the message. It does so only on Claude Code and Grok Build, whose Ctrl-U is
measured to delete one line of the composer a press and to do nothing else. On Codex, whose clearing key is not
measured, a composer that shows text holds the message until the user empties it.

- **Reading the draft.** The composer is read off a whole frame, waiting up to half a second for one, while no key the
  user typed is incomplete or so recent (under half a second) that its text may not be drawn yet, no dialog or trust
  confirmation is up and nothing else holds the session's writes. Otherwise nothing is pressed and the message stays
  queued, to be tried again.
- **The presses.** Ctrl-U is pressed one at a time, each written only while nothing else has written into the session
  since the last and no dialog is up, and each waited for up to **five seconds** to change the composer on the screen
  before the next goes in. It plans two presses for each row of the composer and one more, **40** at most, and the whole
  clearing gets **fifteen seconds**, or **a second for each press** it plans if that is more. A press not seen in time,
  the time running out, or the user typing meanwhile ends the clearing.
- **The notice.** Octoboard sends the `draft_discarded` notice, with the text it read, whenever a press removed
  something, or showed late but left the composer empty; with `cleared: "part"` where the composer did not empty. A
  press that did not show at all sends none.
- **The message.** It is written once a whole frame shows the composer empty and nothing else has written into the
  session since the last press; the keys typed before then count as confirmed, so later messages no longer wait on
  them. If the composer is not seen empty, or something wrote into the session after the last press, nothing of the
  message is written: it stays queued and is tried again.
- **A press nobody saw.** After any press that did not show, whatever the presses before it did, no further press goes
  in until the composer shows something else, the agent reports a prompt sent, it ends a turn, or 30 seconds have
  passed, when one more press is allowed; so a composer that ignores Ctrl-U is not pressed into again and again.
- **A message left in the composer is no draft.** A composer that holds only the message Octoboard wrote last, with
  nothing typed since (a message left unsent), is not cleared: it holds that message if the composer shows nothing but
  its text (compared without whitespace, so wrapping does not count) or, on Claude Code, a single placeholder of a long
  paste. The queued message waits; once a clearing has been held back on that message for 30 seconds, every client is
  sent the `message_not_sent` notice without `submit_retry` (see "A message written but not sent" in
  `docs/product/message-delivery.md`), once for that message. A draft the agent put back from its native stash counts
  as typed for this, so it is never taken for that message.

## Keys that leave the composer empty are confirmed by the screen

User edits are confirmed by the agent reporting a prompt sent that they account for, or, for keys no submitted prompt
accounts for — an Esc, Ctrl-U, Ctrl-C, text typed and cleared again, a prompt sent that no hook matched — by the
composer reading empty once all of these hold:

- the empty composer is read off a whole frame: no synchronized-output update (`ESC[?2026h` to `ESC[?2026l`, which
  Codex and Grok Build wrap each frame in) is open, the agent's output has been quiet for 50 ms, and nothing arrives
  while the screen is read. An update open for 1 second or more counts as over, so an agent that never closes one
  does not keep the screen from showing a whole frame; an update opened again before it is closed keeps the time it
  was first opened;
- the agent has printed output since the user's last key. A single Esc typed at the prompt while no other key held
  the session is the exception: it needs no output after it, since an agent can print nothing for an Esc at an empty
  prompt and such an Esc types no text. It counts as typed at the prompt only while neither the title nor the agent's
  reported status last showed a turn running, and the agent's hooks do not report one running either, as they can
  right after a prompt is sent, while the title still shows the prompt; an Esc typed during a turn cancels it, and the
  prompt the agent puts back can come after any output, so it waits for output like any other key. So does retained
  typing replayed after a native stash (see above), whose timing is not known;
- that key is at least 500 ms old;
- no input sequence the user typed is still incomplete; a single Esc counts as a complete key once it is 500 ms old,
  as above, and a key typed after that is a key of its own, not the rest of a sequence;
- Octoboard's record of the typing knows of no text typed into the composer that nothing since removed: text typed
  and left there still holds as a draft, even over a composer that reads empty;
- no key typed since the last prompt Octoboard matched to a prompt the agent reported sending is one that can put
  text into the composer that Octoboard cannot know.

## Keys that can put unknown text into the composer hold even over an empty one

The text they bring can be drawn later than the empty composer is read, and a message written meanwhile would go in
after it. Such a key holds until the agent reports sending a prompt that Octoboard matches to an Enter typed at or after
the key, Octoboard writes a message at least half a second after the key, or the grace period elapses. They are: a
prompt recalled from history (Up, Down, Ctrl-P, Ctrl-N, Ctrl-R), a completion (Tab), a suggestion accepted (Right, End,
Ctrl-E, Ctrl-F), a yank (Ctrl-Y, Alt-Y), a paste or a stash brought back with Ctrl-V or Ctrl-S, an undo, Esc pressed
twice within two seconds with no message of Octoboard's written between the presses (which can bring back an earlier
prompt to edit), Enter on text that begins with `/` or contains `@` (it can take a completion the agent's menu offers),
and any other key not known to only remove text, move the cursor back or do nothing. The keys known to do only that are
Ctrl-A, Ctrl-B, Ctrl-C, Ctrl-D, Ctrl-K, Ctrl-L, Ctrl-U, Ctrl-W, Backspace, Delete, Alt-B, Alt-D, Alt-Backspace, Left and
Home (plain or with modifiers), and a single Esc. Text pasted through the terminal's bracketed paste is text Octoboard
knows, and holds as typed text does.

## A message only partly written while the draft is stashed

If only part of a message reaches the input after Octoboard pressed the stash key and saw the draft saved, the message
is lost: it is not sent again, and what the input holds is unknown — a paste left open, the message's text
without its Enter, the draft perhaps still stashed. Native stashing is off for the rest of the process, and Octoboard
sends the `typing_blocked` notice. Then:

- Typing retained while the message was written is never replayed. The user's next key in the terminal — not what an
  attached terminal sends by itself — discards it and goes straight into the session, as every key after it does.
- Everything else Octoboard writes into the session — messages, interrupt keys, trust answers, the submit-key retry of
  a message left unsent — stays held until the agent reports a prompt sent after the user has typed again, or until
  the process ends. A prompt reported before the user types again does not end the hold, nor does one submitted
  before the message was written: a late hook report of an earlier prompt, or a prompt the agent sends again by
  itself, as a retry or from its own queue.
- Messages queued behind the lost one stay queued, with any sent meanwhile, and are written in order once the hold
  ends. Nothing is dropped with it, so no `queued_messages_dropped` notice is sent: `typing_blocked` tells the user
  instead.
- The sender whose own message it was is answered `queued_messages_lost` with `fragment: "held"`: the message could
  not be written in full into the session, so it was dropped; messages queued behind it stay queued; the session's
  input box may hold part of it, and Octoboard writes nothing more into the session until a prompt is sent, so a
  message sent now waits until then. A sender whose message was queued behind it is told it was queued.

A partial write outside a native stash is not covered by this: it drops the message and its queue (see "Messages
held until a session can take them" in `docs/product/message-delivery.md`).
