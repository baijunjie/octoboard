# Draft preservation

How Octoboard keeps a message it writes into a running session's terminal — an instruction from an owner, a report,
information shared by another session, a report panel form submission, a notice (see `docs/product/message-delivery.md`)
— from being sent together with text the user has in the session's input box (the composer), what it tells the
user when it has to set that text aside or clear it, and what it does when the text cannot be cleared: delivering the
message comes first, so a message does not wait without end on text Octoboard cannot get out of its way, but goes in
with it as the last resort.

A slash command or a message with images a client sends is written under the same rules, its several pastes and its
Enter all within one draft-preserving step, so a stashed draft comes back only after its Enter (see
`docs/product/slash-commands-and-image-messages.md`).

## Draft preservation has a bounded wait

A plain-text message sent through Pi’s extension or OpenCode’s private server can leave the terminal composer untouched. On the terminal path, including slash commands and image messages, the usual draft rules apply.

On supported launches, Octoboard first tries to deliver the automatic message separately while preserving the user's
draft, without waiting for the user to finish typing:

- **Claude Code 2.1.274 or later and Grok Build 1.0.70 or later:** when Octoboard recognises a nonempty composer and
  can safely use the agent's native stash, it saves the draft, confirms the stash and an empty composer, then sends
  the automatic message. The agent restores the draft automatically after submission; Octoboard does not toggle the
  stash a second time. An existing stash of the user's is never overwritten by another stash operation. Octoboard
  sees a stash by the agent's own indicator on the screen: on Claude Code, the word `stashed` in the row just above the
  input box's top rule (`› stashed` up to 2.1.314, `draft stashed` in 2.1.329, after the effort indicator while that
  is still shown); on Grok Build, `Stashed` in the input box's top border. The stash key
  is pressed only on a composer read off a whole frame (the first condition under "Keys that leave the composer empty
  are confirmed by the screen" below): Octoboard waits up to half a second for one, and a composer that shows text off
  a frame still being drawn — as at the start of a turn, when the box can be read before it is drawn again — holds the
  message for a later attempt rather than have the key land on a box that may be empty.
  Claude Code additionally requires a verifiable Ctrl-S stash binding in the configuration this launch uses; an
  overridden binding or an unreadable or unrecognised keybindings file disables this path.
- **GitHub Copilot CLI 1.0.114 or later:** Ctrl-S saves a draft. The footer marks it as `stashed`. A second Ctrl-S
  restores it once the automatic message has left the composer.
- **Codex 0.166.0 or later, on a launch with its private app server** (see "Codex's private app server" in
  `docs/product/per-agent-launch-specifics.md`): automatic messages go into Codex's own queue and leave the composer
  untouched, so none of the composer checks, the native stash or the three-second grace period below applies to them
  (see "Messages through Codex's own queue" in `docs/product/message-delivery.md`).
- **Any other Codex launch:** Octoboard uses no stash of Codex's, so its automatic messages take the guarded terminal
  delivery below.

Octoboard reads the composer off the session's screen with faint text left out, so Claude Code's hints in an empty
box — a fresh session's placeholder, and "Press up to edit queued messages" while a message waits for the turn to
end — read as an empty composer; text typed there replaces the hint and reads as a draft. An agent whose composer can
only be found by something it draws faint, such as the rules framing it, has its composer read off the screen as
drawn instead, and the placeholder it shows in an empty box is then read as an empty composer on every host. Every
reading of what the composer holds — the checks for a draft, the stash, the clearing below, the interrupt's (see "The
input box afterwards" in `docs/product/interrupting-turns.md`) — reads it off the same screen. A message Octoboard has
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

Where the stash key emptied the composer but no stash indicator Octoboard recognises showed within the confirmation
window, nor right after it, the agent draws its indicator in a way Octoboard cannot read, so it could not see a stash
the user already holds either, and a further stash key could overwrite it. Octoboard then stops using the agent's native
stash for the rest of the agent's process: the `draft_save_unconfirmed` notice is still sent for that attempt, and every
later message takes the guarded terminal delivery below, also after a prompt is sent. A daemon that restarts and adopts
the running session uses the native stash again, until the next such attempt. Where the composer still shows the draft
at the timeout, only the hold above applies: no stash key until a prompt is sent.

## The draft and typing notices

Each is sent to every client about the session, naming its agent (`agent`); `typing_lost` also carries `bytes`, the
amount of retained typing lost, `draft_discarded` the text the draft held, as `detail`, and `cleared: "part"` when
the composer did not empty, and `message_cleared_after_cancel` likewise the text of the message cleared, as `detail`,
and `cleared: "part"` when the composer did not empty (see "Clearing a draft past the grace period" below);
`message_cleared_after_cancel` is sent in place of `draft_discarded`, never beside it. `message_sent_with_draft`
carries the text the composer held as `detail` (see "When the composer cannot be cleared" below), absent where it
showed none or could not be read; `held: "message"`, with no `detail`, where that text was only the message Octoboard
wrote last, and `held: "message_and_draft"`, with `detail`, where it was that message with text typed after it. Of
`draft_save_unconfirmed`, `draft_restore_unconfirmed` and `typing_held`, one stash and its recovery send only the first
that applies; `draft_discarded`, `message_cleared_after_cancel`, `message_sent_with_draft`, `typing_blocked` and
`typing_lost` are sent whenever they apply, and `message_waits_on_input` once a message. The window shows each as an error alert (see `docs/product/error-alerts.md`)
naming the session and where it is: a known failure with no remedy beyond its own text, not an unexpected one.

| Notice | Alert title | What the alert says |
|---|---|---|
| `draft_save_unconfirmed` | "Draft save not confirmed" | The agent did not confirm setting the draft aside, so Octoboard did not write its message there; the message waits until the input box is empty, and the user is to send or clear the draft, or Octoboard clears it shortly and shows what it held. |
| `draft_discarded` | "Draft cleared" | Octoboard cleared the draft from the input box so that its message would not be sent together with it; to keep the draft, the user copies it from the block below the text and pastes it back into the input box. |
| `draft_discarded` with `cleared: "part"` | "Draft partly cleared" | Octoboard cleared only part of the draft so that its message would not be sent together with it; the message waits until the input box is empty, and Octoboard tries again once the box changes. To keep the draft, the user copies it from the block below; part of it may still be in the input box. |
| `message_cleared_after_cancel` | "Message not carried out" | When the turn was interrupted, the agent put a message Octoboard had written for the session, which it was holding, back in the input box unsent, and Octoboard cleared it from the input box and could not write it again, so it was not carried out; to have it carried out, the user copies it from the block below the text and sends it again. Sent only where the message could not be written again (see "A message given back at a cancel" under "Clearing a draft past the grace period" below); the alert does not say why. |
| `message_cleared_after_cancel` with `cleared: "part"` | "Message not carried out" | As above, under the same title, which says what did not get done, but Octoboard cleared only part of the message; part of it may still be in the input box. The alert does not say what became of the next message, which waits for the box to empty or, its wait over, went in with what was left. |
| `message_dropped_at_cancel` | "Message not carried out" | When the turn was cancelled, the agent dropped a message Octoboard had written for the session, which it was holding, and Octoboard could not write it again, so it was not carried out; to have it carried out, the user copies it from the block below the text and sends it again. Nothing was put back in the input box and nothing was cleared from it. Sent only for an agent whose provider declares that a cancel drops the messages it holds, and only where the message could not be written again (see "Messages held until a session can take them" in `docs/product/message-delivery.md`); the alert does not say why. |
| `message_sent_with_draft` | "Message sent with the draft" | Octoboard could not clear the draft from the input box, so its message was sent together with the draft; the user is to check the session, and can copy the draft from the block below the text. |
| `message_sent_with_draft` with neither `detail` nor `held` | "Message sent with the input box's content" | Octoboard could not clear the input box, so its message was sent together with whatever the box held; the user is to check the session. |
| `message_sent_with_draft` with `held: "message"` | "Message sent with the previous one" | The input box still held Octoboard's previous message, unsent, so the new message was sent together with it; the user is to check the session. |
| `message_sent_with_draft` with `held: "message_and_draft"` | "Message sent with the previous one and the draft" | The input box still held Octoboard's previous message, unsent, with text typed after it, so the new message was sent together with both; the user is to check the session, and can copy what the box held from the block below the text. |
| `message_waits_on_input` | "Message waiting to go in" | A message from Octoboard is waiting because the session's input box cannot be read, and a list or another view may be covering it; the user is to close the list or view, or send or clear what the input box holds, so the message can go in. |
| `draft_restore_unconfirmed` | "Draft restore not confirmed" | The agent did not confirm putting the draft back after the message was sent; the user is to check the input box before sending what it holds. Its details add that what was typed while the draft was set aside goes into the input box as it is now, and that the message is not sent again. |
| `typing_held` | "Typing held back" | Some of what was typed while Octoboard wrote a message has not reached the agent yet; the user is to answer any dialog the session shows. Its details add that the agent is showing a dialog or not taking input, and that what was held back is typed in, in order, once it takes input again. |
| `typing_blocked` | "Message partly written" | Only part of a message Octoboard wrote reached the input box, and it will not be sent again; the user is to check the input box and remove what is left of the message before sending a prompt. Its details add that what was typed while the message was written is discarded on the next key, that typing goes straight to the agent from then on, and that Octoboard's other messages wait for a prompt to be sent, each for about a minute at most, after which each is written into the input box together with what the box holds. |
| `typing_lost` | "Typing lost" | The session ended before the agent confirmed putting its draft back, so what was typed meanwhile is lost and is not typed in when the session is resumed. |

The text a `draft_discarded`, `message_cleared_after_cancel`, `message_dropped_at_cancel` or `message_sent_with_draft`
alert hands back is shown in
a block of its own after the alert's text, with a Copy button, under a label naming it as the draft's content (as the
message's content for `message_cleared_after_cancel` and `message_dropped_at_cancel`, and as the input box's content for
`held: "message_and_draft"`): for `message_dropped_at_cancel`, the message's own text, as Octoboard wrote it;
otherwise the text as the input box showed it before the first clearing key, or, for a
`message_sent_with_draft` with no clearing in the same attempt, before the message's write; its rows joined by line
breaks, in its own direction, its long lines wrapped. A long paste the agent shows collapsed (Claude Code's
`[Pasted text #1 +24 lines]`, Codex's `[Pasted Content 2400 chars]`) is only that placeholder, so its text cannot be
got back from the alert. The exception is a `message_cleared_after_cancel` sent because an interrupt cleared such a
placeholder (see "Messages held until a session can take them" in `docs/product/message-delivery.md`): its block holds
the message's own text, as Octoboard wrote it. How the block is shown is in "Names and diagnostic details" in
`docs/product/error-alerts.md`.

## The draft grace period

**When separate delivery cannot be used, delivery takes priority after three seconds.** Each message gets a draft
protection grace period from its first delivery attempt; retries and continued typing do not reset it. The exception is
a message refused while the session's input is held for draft preservation — a draft being stashed or restored, or a
partly written message (see "A message only partly written while the draft is stashed" below): its grace period starts
again from its first attempt after the hold ends, or, once the hold has lasted 30 seconds, from its first attempt after
that (see "When the composer cannot be cleared" below). So does a message a Codex session's private app server held back
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
is not written together with text the composer shows while that text may yet be set aside or cleared: a composer that
reads empty takes the message directly, one that shows text has it stashed around the message where the agent's native
stash can be used, and where it cannot, the composer is cleared first; where it cannot be cleared either, the message
waits 30 seconds and then goes in with that text (see "Clearing a draft past the grace period" and "When the composer
cannot be cleared" below). Such a
write still waits while a key the user typed less than half a second ago may have put text into the composer that is
not drawn yet.

A composer read empty off a frame not drawn whole (see the first condition under "Keys that leave the composer empty
are confirmed by the screen" below) holds the message for 2 seconds of retries in all, since an agent that keeps
redrawing, as one mid-turn can, may never show a whole frame: the retries refused for that reason form a run, which goes
on while each comes within 10 seconds of the one before, and ends once a message is written, the composer is read off a
whole frame or it shows text. Once the run has lasted 2 seconds, the message is written on such a reading, unless
Octoboard's record of the typing knows of text typed into the composer that nothing since removed: such a frame may hide
that text, so the message keeps waiting for a whole frame, and the reading neither starts the run nor ends it; that wait
is one for text that cannot be cleared, and ends as "When the composer cannot be cleared" below says. A retry
held for another reason — a key typed less than half a second ago, or an unreadable composer (below) — neither starts
the run nor ends it.

A composer that cannot be read at all, while any key typed into it is not yet confirmed (whatever it typed or removed)
or a key that can put unknown text into it (see below) still holds, holds the message too, past the grace period as
well, until the composer can be read again or a typed prompt is matched: an unreadable composer cannot show that the
keys left it empty, and a view that hides the composer, such as a picker, would take the message's keys. Where no key
it waits on can put unknown text into the composer (keys that typed text, a bracketed paste included, removed it or moved
the cursor), the wait is one for text that cannot be cleared, and ends as "When the composer cannot be cleared" below
says. Claude Code's
composer is read while it shows a prompt recalled from history under its `History` rule.

Draft protection therefore does not wait indefinitely for an ordinary prompt or a new process, nor for text in the
composer that cannot be cleared: past 30 seconds of that the message goes in with the text (see "When the composer cannot
be cleared" below). Delivery still waits, with no bound, on an unreadable composer after a key that can put unknown text
into it, since a view such as a picker may be hiding the composer and would take the message's keys; once a message has
waited so for 30 seconds, every client is sent the `message_waits_on_input` notice, once for that message.

**What still goes in with text the screen does not show.** Besides the end of the wait in "When the composer cannot be
cleared" below, past the grace period a composer that cannot be read while every key typed into it is confirmed takes
the message as it is, and so does a composer read empty off frames not drawn whole once the 2-second run is over, when
Octoboard's record of the typing knows of no text left in it. Text in such a composer that the screen did not show —
drawn late, or hidden by a frame read in part — is then sent together with the message.

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
launch, a stash already shown, a stash left unsettled, a stash turned off after an indicator Octoboard could not read
(see "Draft preservation has a bounded wait" above) — Octoboard
clears the composer before it writes the message. On Claude Code, Grok Build and Codex, Ctrl-U is
measured to delete one line of the composer a press and to do nothing else, mid-turn included. On an agent with no
measured clearing key, a composer that shows text holds the message as "When the composer cannot be cleared" below says.

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
  press that did not show at all sends none. Where the message then goes in as the composer stands, the text is handed
  back by `message_sent_with_draft` instead (see "When the composer cannot be cleared" below); either way, text the
  presses removed is handed back once. A message the agent gave back at a cancel is not told as a draft: it is written
  again, or, where it cannot be, told by `message_cleared_after_cancel` in place of `draft_discarded` (below).
- **The message.** It is written once a whole frame shows the composer empty and nothing else has written into the
  session since the last press; the keys typed before then count as confirmed, so later messages no longer wait on
  them. If the composer is not seen empty, or something wrote into the session after the last press, nothing of the
  message is written while its wait in "When the composer cannot be cleared" below is not over: it stays queued and is
  tried again. Once that wait is over, it goes in within the same attempt, as the composer stands.
- **A press nobody saw.** After any press that did not show, whatever the presses before it did, no further press goes
  in until the composer shows something else, the agent reports a prompt sent, it ends a turn, or 30 seconds have
  passed, when one more press is allowed; so a composer that ignores Ctrl-U is not pressed into again and again.
- **A message left in the composer is no draft.** A composer that holds only the message Octoboard wrote last, with
  nothing typed since (a message left unsent), is not cleared: it holds that message if the composer shows nothing but
  its text (compared without whitespace, so wrapping does not count) or a single placeholder of a long paste (Claude
  Code's `[Pasted text #1 +24 lines]`, Codex's `[Pasted Content 2400 chars]`). The queued message waits as on any
  composer that cannot be cleared (below). A draft the agent put back from its native stash counts as typed for this, so
  it is never taken for that message. A message written at the prompt and left unsent, and one written while a turn ran
  into the composer of an agent that does not give messages back at a cancel, are never cleared; the exception is the
  next bullet.
- **A message given back at a cancel.** Some agents hold a message written into their composer while a turn runs for
  the end of the turn, take it out of the composer, and put it back there, unsent, when the turn is cancelled; Claude
  Code, Codex and Grok Build do not. On such an agent, a composer that shows nothing but the message Octoboard wrote
  last, written while a turn ran, with nothing typed since, holds no message left unsent: left there, it would hold the
  next message back for 30 seconds and then be sent joined to it. So once the next message is past its grace, the
  composer is cleared as a draft is, with the same presses and bounds, and the next message written once it shows empty.
  The message is not lost: it went in, and its sender was answered that it was delivered, so it is written again. It is
  put back at the front of the session's queue, as it was written (its header with it), and written again right after
  the message it was cleared for, still behind that message if that one has to wait. This holds whoever sent it and
  whatever has become of the sender since — a report from a session archived since, an instruction, shared information,
  a notice of Octoboard's own — and nobody is told: no alert to the user, no message to the sender. The bounds and the
  cases where it is not written again, when the user is sent `message_cleared_after_cancel` with its text instead, are in
  "Messages held until a session can take them" in `docs/product/message-delivery.md`; that notice is sent in place of
  `draft_discarded`, with `cleared: "part"` where the composer did not empty. Several messages given back at once are
  told apart the same way: a composer that begins with the latest messages Octoboard wrote while the turn ran, one
  after another in their order (compared without whitespace, so neither wrapping nor an empty row between two counts),
  holds those messages, and each is written again, in that order. Text that follows them, set off by whitespace, is the
  user's: it is cleared with them and handed back by `draft_discarded`, alone. A composer where the user's text comes
  first, or that only begins as a message does, is a draft, cleared and told whole (`draft_discarded`), and what it
  held is not written again; so is one that shows messages of a turn before the last. A cancel the user types in the terminal is a key typed after the message, but the composer it leaves,
  showing that message's own text and nothing else, is still the message given back and is handled as one. A clear that
  leaves part of the message in the composer still has it written again, once: what is left of it there is neither that
  message nor a draft, and a later clear of it tells nobody. Where such a clear leaves text and the next message's
  30-second wait (below) is over, the next message goes in as the composer stands, with `message_sent_with_draft`
  carrying what was left, and the message given back is written again after it. An interrupt made with
  `interrupt_session` clears what the agent gives back right after its key instead, where it can (see "The input box
  afterwards" in `docs/product/interrupting-turns.md`); the message is then written again only after the next message
  written into the session, as "Messages held until a session can take them" in `docs/product/message-delivery.md`
  says. The messages are told apart there as here, and text the user typed after them is cleared with them and handed
  back by `draft_discarded`, alone.

## When the composer cannot be cleared

A message past its grace that finds a draft in the composer waits for it to go, but only for **30 seconds**, counted
from its first attempt that found one, however much the clears take out of it meanwhile; every attempt still tries the
stash and the clear first. The wait is the message's own, kept under the queue's identity for it, so a later message
with the same text waits its own while; a message that goes in ends it, and so does a composer seen empty off a whole
frame, so a draft typed after that starts a wait of its own. Once the wait is over, the message goes in as
the composer stands where the draft still cannot be got out of its way: at once where no key may clear it now, and
within the same attempt where a clear leaves text behind. The draft cannot be got out of the way where it is:

- text on an agent with no measured clearing key;
- text a clearing press was not seen to change, while the press holds further presses ("A press nobody saw" above);
- text a clearing left behind (a partial clear, a clearing that a key the user typed ended, a composer not seen empty
  off a whole frame after it);
- text the composer shows only off frames still being drawn, a composer read empty off such frames while Octoboard's
  record of the typing knows of text typed into it, and an unreadable composer while no key it waits on can put unknown
  text into it;
- only the message Octoboard wrote last, left unsent ("A message left in the composer is no draft" above), not one the
  agent gave back at a cancel ("A message given back at a cancel" above).

A message left unsent that waits for the user to send it — its submit key not retried because something was typed
after it or the composer held more than it, or a Codex message left in its input box on a Windows host (see "A message
written but not sent" in `docs/product/message-delivery.md`) — holds later messages for 30 seconds from then too; after
that the next message goes in as the composer stands, without clearing it or waiting out its own grace period, so both
messages, and anything typed after the first, go in as one prompt.

A hold on the session's input that does not end — a message only partly written while the draft was stashed, typing
retained during a stash that is not replayed (see "A message only partly written while the draft is stashed" below and
"Draft preservation has a bounded wait" above) — keeps each message out until the hold has lasted 30 seconds and the
message, past its own grace, has waited 30 seconds more on it, after which it goes in as the composer stands, as above;
a message within its grace waits for the hold to end and then takes the stash or the clear. The hold's time starts
again with each message that goes in past it, so the next one waits it out afresh, and a hold for a partly written
message ends once the agent reports a prompt that holds the message that went in past it, since the fragment went with
it. A key sequence the user typed that never gets the rest of its bytes — a bracketed paste never closed, the `ESC [` an
Alt-[ leaves — no longer keeps out a message written as the composer stands once its last byte is 30 seconds old; no
stash or clearing key goes in on it.

Once the 30 seconds are over, the message is written into the composer as it stands, followed by its Enter, so it may
be sent together with that text; Octoboard's record of the typing then counts that text as gone with it, so later
messages do not wait on it. A slash command a client sent is the exception: it is never written beside other text, and is
refused with `message_not_written` instead (see "Sending a slash command" in
`docs/product/slash-commands-and-image-messages.md`). For a message written so, every client is sent the
`message_sent_with_draft` notice:

- with the text the composer showed just before the write, or no text where it could not be read or showed none;
- where a clear ran in the same attempt and left text, with the draft as read before that clear, what the clear took
  out included; no `draft_discarded` is sent for that clear, unless the write is then refused, so what the clear removed
  is handed back once and never lost unseen;
- where the composer held only the message Octoboard wrote last, saying so (`held: "message"`): the two messages then go
  in as one prompt, each under its own header;
- where it held that message with text typed after it, saying so (`held: "message_and_draft"`) and carrying what the
  composer showed.

A composer seen empty off a whole frame by then takes the message as any empty composer does, whatever Octoboard's
record of the typing still held, which is told the composer is empty: no `message_sent_with_draft` is sent, and where
the presses of a clear in the same attempt turn out to have emptied it, `draft_discarded` hands back what they removed.
The write still never goes in while a dialog, a trust confirmation or a wait for the user's action is up, while a key
the user typed is incomplete (within the 30 seconds above), less than half a second after a key that may have put text
in the composer not drawn yet, or into a composer that cannot be read after a key that can put unknown text into it,
which waits for the user as "The draft grace period" above says, the `message_waits_on_input` notice going out once the
message has waited 30 seconds; it waits for those as any write does, and goes in once they have cleared. What follows
the write is as for any message written (see "A message written but not sent" in `docs/product/message-delivery.md`).

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
  itself, as a retry or from its own queue. Messages are the exception to how long it holds: once the hold has lasted
  30 seconds and a message past its own grace period has waited 30 seconds on it, that message goes into the input as
  it stands, with whatever is left of the lost one, and the hold ends once the agent reports a prompt that holds it
  (see "When the composer cannot be cleared" above).
- Messages queued behind the lost one stay queued, with any sent meanwhile, and are written in order once the hold
  ends or as the bound above lets them past it. Nothing is dropped with it, so no `queued_messages_dropped` notice is
  sent: `typing_blocked` tells the user instead.
- The sender whose own message it was is answered `queued_messages_lost` with `fragment: "held"`: the message could
  not be written in full into the session, so it was dropped; messages queued behind it stay queued; the session's
  input box may hold part of it, and messages written into the session wait for a prompt to be sent, each for about a
  minute at most, after which Octoboard writes each into the input box together with what the box holds. A sender whose message was queued behind it is told it was queued.

A partial write outside a native stash is not covered by this: it drops the message and its queue (see "Messages
held until a session can take them" in `docs/product/message-delivery.md`).
