# Message delivery

How Octoboard writes into a running session everything it has to put there — an instruction from an owner, a report,
information shared by another session, a report panel form submission — and when it holds one back.

## Messages held until a session can take them

A session that is working or at its prompt (paused, done, awaiting instructions, with child sessions working, or working
in background) takes a message straight away, unless one of the holds below applies to it, or it is held behind a
message its agent did not send (see "A message written but not sent" below). A message written mid-turn is taken by
the agent: Claude Code and Grok Build queue it and run it once the turn ends, though Claude Code can also take it into
the turn still running, and Codex steers one written into its terminal into the running turn. Codex's own queue (see
"Messages through Codex's own queue" below) runs it as a turn of its own after the running one. What becomes of such
messages when an owner interrupts the turn is in "Messages during an interrupt" in `docs/product/interrupting-turns.md`.

**Draft preservation has a bounded wait.** On supported launches, Octoboard first tries to deliver the automatic
message separately while preserving the user's draft, without waiting for the user to finish typing:

- **Claude Code 2.1.274 or later and Grok Build 1.0.70 or later:** when Octoboard recognises a nonempty composer and
  can safely use the agent's native stash, it saves the draft, confirms the stash and an empty composer, then sends
  the automatic message. The agent restores the draft automatically after submission; Octoboard does not toggle the
  stash a second time. An existing stash of the user's is never overwritten by another stash operation.
  Claude Code additionally requires a verifiable Ctrl-S stash binding in the configuration this launch uses; an
  overridden binding or an unreadable or unrecognised keybindings file disables this path.
- **Codex 0.166.0 or later, on a launch with its private app server** (see "Codex's private app server" in
  `docs/product/per-agent-launch-specifics.md`): automatic messages go into Codex's own queue and leave the composer
  untouched, so none of the composer checks, the native stash or the three-second grace period below applies to them
  (see "Messages through Codex's own queue" below).
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
Octoboard sends the `draft_save_unconfirmed` notice and writes the message into the current composer with Enter,
which may submit unfinished user text with it; it does not send a second stash key. If the complete message was
written but restoration is not confirmed in time, Octoboard sends the `draft_restore_unconfirmed` notice and releases
retained typing into the current input without repeating the message. Either timeout disables native stashing for
that running process. A dialog still holds earlier retained keys until it closes; after a timeout they can be
released without a stash marker confirming restoration. While retained typing cannot all be replayed yet — a dialog
is up or the terminal is not taking it, also once the confirmation window has run out with the stash unsettled —
Octoboard sends the `typing_held` notice. If the process ends before restoration is confirmed, Octoboard sends the
`typing_lost` notice: the retained typing is not replayed into a resumed session.

**The draft and typing notices.** Each is sent to every client about the session, naming its agent (`agent`);
`typing_lost` also carries `bytes`, the amount of retained typing lost. Of `draft_save_unconfirmed`,
`draft_restore_unconfirmed` and `typing_held`, one stash and its recovery send only the first that applies;
`typing_blocked` and `typing_lost` are sent whenever they apply. The window shows each as an error alert (see
`docs/product/error-alerts.md`) naming the session and where it is: a known failure with no remedy beyond its own
text, not an unexpected one.

| Notice | Alert title | What the alert says |
|---|---|---|
| `draft_save_unconfirmed` | "Draft save not confirmed" | The agent did not confirm saving the draft, so Octoboard wrote its message into the input box as it was; the user is to check the conversation, since the message may have been sent together with the draft. |
| `draft_restore_unconfirmed` | "Draft restore not confirmed" | The agent did not confirm putting the draft back after the message was sent; the user is to check the input box before sending what it holds. Its details add that what was typed while the draft was set aside goes into the input box as it is now, and that the message is not sent again. |
| `typing_held` | "Typing held back" | Some of what was typed while Octoboard wrote a message has not reached the agent yet; the user is to answer any dialog the session shows. Its details add that the agent is showing a dialog or not taking input, and that what was held back is typed in, in order, once it takes input again. |
| `typing_blocked` | "Message partly written" | Only part of a message Octoboard wrote reached the input box, and it will not be sent again; the user is to check the input box and remove what is left of the message before sending a prompt. Its details add that what was typed while the message was written is discarded on the next key, that typing goes straight to the agent from then on, and that Octoboard's other messages wait until a prompt is sent. |
| `typing_lost` | "Typing lost" | The session ended before the agent confirmed putting its draft back, so what was typed meanwhile is lost and is not typed in when the session is resumed. |

**When separate delivery cannot be used, delivery takes priority after three seconds.** Each message gets a draft
protection grace period from its first delivery attempt; retries and continued typing do not reset it. The exception is
a message refused while the session's input is held for draft preservation — a draft being stashed or restored, or a
partly written message (see "A message only partly written while the draft is stashed" below): its grace period starts
again from its first attempt after the hold ends. So does a message a Codex session's private app server held back
before the terminal took it (see "Messages through Codex's own queue" below): its grace period starts again from its
first attempt after the server first held it back. It starts again that way once per message only, so time the server
holds the message back after that counts toward the grace period. During the grace period, ordinary terminal delivery requires a
recognised empty composer and confirmation that preceding user edits were submitted. A delayed report for an earlier
prompt, an automatic message or a background completion does not confirm a newer draft. An unsupported stash, an
existing stash, an unrecognised composer, unconfirmed edits or an incomplete input sequence can hold delivery
temporarily. Once the grace period has elapsed, the next delivery attempt writes the message into the current input and
submits it, even if unfinished user text is included. It does not start another native stash wait then. Draft protection
therefore does not wait indefinitely for an ordinary prompt or a new process. This grace period does not raise the
session's hand.

The deadline never answers a recognised dialog or trust confirmation, overrides an agent's permission wait, or writes
into an ended or frozen session. Such holds must clear first. A partial terminal write remains a delivery failure;
Octoboard does not repair a broken paste or replay retained typing into it (see below).

Octoboard retries queued messages automatically when the session can take them, including after its input box is
redrawn. A sender told its message was queued must not send it again. The ordinary queue lifetime below still applies:
relaunching a session does not preserve messages held only in its running process's queue.

**A message is not written verbatim.** Its text is reshaped in the steps below, in this order:

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
4. For a Claude Code session only: a line, or a part of one starting at a space right before `/` or a Windows drive path
   (`X:\`), that ends in an image extension (`.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`) has its last word wrapped in
   backticks, so ``- /a/b.png`` arrives as ``- `/a/b.png` ``. Claude Code reads such a piece as an image to attach and,
   in doing so, loses the Enter written after the message, which would then sit unsent in its input box, one written
   mid-turn included (see "A message written but not sent" below). The path stays readable text, unless it contains a
   space, which the backticks then split; a path line inside a fenced code block is wrapped as well. A path followed by
   punctuation or already in backticks is not touched. Codex and Grok Build have not been measured for this.

This holds for everything Octoboard writes into a running session — an instruction, a report, a report panel form
submission, information shared by another session, the instruction `resume_session` hands over, the prompt to continue
after an account switch — because all of them can be model-authored and none is reviewed first.

**An instruction `resume_session` hands a Claude Code session that resumes a conversation is not written at all**, nor
is the prompt to continue that an account switch sends one (see "Continuing after a switch" in
`docs/product/switching-session-accounts.md`): it is given to the relaunched agent as the prompt its launch starts
with, since Claude Code can ask "Resume this conversation?" at a resume launched without one, and a message written
into that dialog would be thrown away (see the screens that hold messages, below). The prompt is reshaped by the
first three steps, and one that would then begin with `-` also gets a single leading space, so it is not read as a
command-line option; one that is nothing but whitespace once reshaped is dropped, and the session resumes with no
prompt. Such a prompt is neither queued nor checked as below. For any other session, Claude Code's included when it has
no conversation to resume, the instruction or prompt is queued and written like any other message.

**A blank message is refused.** A message passed to a client's `send_message` request or to an owner's
`instruct_session` tool that is nothing but whitespace once its control characters and invisible characters are
removed is not written: a client is refused with `field_required` (field `text`), an owner's tool with that reason in
prose.

An instruction, a report or information shared by another session, for a session that **cannot** take one right now — it
is waiting for the user at a permission prompt or a question — is queued rather than dropped, and delivered as soon as
the session can take one, oldest first. A session that sent an instruction or shared information is told it was queued
and that it must not send it again. The wait can outlast the user's answer: a permission prompt reported through hooks
keeps the session's hand up until the call it let through has finished. A Codex action title also holds messages; when
it clears, delivery resumes if the last hook-reported status permits it (see "The raised hand" in
`docs/product/session-statuses.md`).

A message for a session with no process running is refused outright: a resume starts the agent at
its prompt and replays nothing. The exceptions are information shared with an interrupted session, and the reports and
notices for an interrupted owner, which are kept until the session is resumed (see "Information for an interrupted
session" in `docs/product/information-between-sessions.md` and "Reports and notices for an interrupted owner" in
`docs/product/reporting-to-owners.md`). Anything still queued when a session's process ends goes with it, unless the
session is being handed off to another agent, whose queue is kept for the session that takes its place (see "Messages
for the original during a handoff" in `docs/product/handing-off-sessions.md`), or it is kept for an interrupted
session, which stays kept for the next resume.

**Messages are also held while a session's trust confirmation may be up.** A message's trailing Enter would answer
the agent's own folder-trust confirmation (see `docs/product/folder-trust.md`) — accepting it, and so trusting the
folder, at Codex's and Grok Build's, and declining it, which exits the agent, at Claude Code's. So from every launch of
a session, a resume included, everything Octoboard writes into it — an instruction, a report, information shared by
another session, a report panel form submission, the instruction `resume_session` hands over or the prompt to continue
after an account switch when it is not the launch prompt — is queued as above until the confirmation can no longer be
on screen and, for Claude Code, until the agent is reading its input:

- for Claude Code, until both of two things have happened, whichever comes last releasing the queue: the session's
  first hook report has arrived, which Claude Code sends only once it is past the confirmation — with none to answer,
  within a couple of seconds of starting, a launch or a resume with no prompt included — and the session's terminal
  has bracketed paste on, which Claude Code turns on, and keeps on, once it starts reading its input. Bracketed paste
  has to be on when the first hook report arrives, or come on after it: bracketed paste turned on and off again
  before the report does not count. That covers Claude Code's probe of the terminal as it starts, which turns
  bracketed paste on and straight off again long before it reads its input, and the paste mode of its trust
  confirmation. From the first hook report on, whether Claude Code turns bracketed paste off again does not matter.
  With Claude Code's default renderer bracketed paste comes on before the first hook report, so the report releases
  the queue; with its fullscreen renderer (`"tui": "fullscreen"`) the report comes first. A message written before
  Claude Code reads its input would be left unsent in its input box, or wiped when a launch prompt is submitted. A
  Claude Code session whose hook reports never arrive, or whose terminal never has bracketed paste on at or after the
  first one, holds its messages for as long as it runs: they are never let through on time alone;
- for Grok Build, until its first hook report, which it too sends only once it is past the confirmation. The one
  exception: a Grok Build session that has sent none 30 seconds after its launch — a managed policy that allows only
  managed hooks keeps every hook of Octoboard's from running — has its messages let through anyway, provided its
  confirmation has not been recognised. A confirmation that has been, before those 30 seconds or after them, holds
  its messages until Grok's screen no longer shows it, or until a hook report arrives;
- for Codex, which reports nothing until its first prompt, until Octoboard's watch for the confirmation ends with none
  shown — at the first terminal title Codex sets, once its session has started; when that watch ends is in "How a
  confirmation is pressed" in `docs/product/folder-trust.md`. Once a confirmation is shown, they are held until
  Octoboard's press of it succeeds, a hook report arrives, or the person's own answer in the terminal is seen to
  replace it on screen. A Codex session that neither sets a terminal title nor sends a hook report holds its messages
  for as long as it runs: they are never let through on time alone.

These startup and trust holds do not prevent the user from answering in the terminal. The temporary retention of
typing during native draft preservation is described above. A client's direct request to write into a session
(`send_message` in the daemon's client protocol) is refused instead of queued while the trust hold lasts, with the
error code `session_trust_pending`, saying to send it once the confirmation has been answered, or once the session
has finished starting if none appears.

**Messages are also held while the agent shows a screen only the user can answer.** Some of the agents' own screens
run no hook and set no terminal title, and a message's paste and Enter would answer them: the message is thrown away
and the Enter picks whatever option is highlighted, or the text is sent somewhere other than the conversation.
Octoboard recognises these screens on the session's rendered screen, and while one is up everything it writes into
the session is queued as above. Each has a code, which the owners' tools report (see the end of this section):

| Agent | Screen | Code |
|---|---|---|
| Claude Code | "Resume this conversation?" at a launch that resumes a conversation with no prompt | `claude_resume_at_launch` |
| Claude Code | "Resume this conversation?" coming up by itself in an idle session | `claude_resume_idle` |
| Claude Code | "Resume from summary", offered at a resume | `claude_resume_from_summary` |
| Claude Code | A notice that waits for Enter, ending "Enter to continue · Esc to cancel" | `claude_notice` |
| Claude Code | "Make auto mode your default permission mode?", offered once when the account's settings name a default permission mode other than auto, after the prompt box at a launch or once a turn ends; its focused answer, picked by a message's Enter, would make auto the account's default | `claude_auto_mode_default` |
| Claude Code | Its list of background agents under the prompt box, with the focus moved onto it | `claude_agent_list_focus` |
| Claude Code | A background subagent's view, showing that subagent's transcript in place of the conversation | `claude_subagent_view` |
| Claude Code | Its agents view, where what is typed starts another background session | `claude_agents_view` |
| Codex | Its sign-in screen | `codex_sign_in` |
| Codex | The choice between the session's and the current working directory, at a resume | `codex_resume_cwd` |
| Codex | The prompt to try a newer model | `codex_model_migration` |
| Codex | The nudge to switch to another model as a rate limit nears | `codex_rate_limit_nudge` |
| Grok Build | Its usage-limit screen, offering to upgrade, buy credits or try again | `grok_usage_limit` |

Every Codex launch keeps its last three screens away as far as it can (see "Codex" in
`docs/product/per-agent-launch-specifics.md`), so they hold messages only when one comes up all the same. While such a
screen is up the session reads as *waiting for the user* and raises its hand (see "What the statuses are derived from"
in `docs/product/session-statuses.md`) — except Claude Code's three views of its background agents
(`claude_agent_list_focus`, `claude_subagent_view`, `claude_agents_view`), which show it *dormant* and raise no hand (see
"Dormant" in `docs/product/session-statuses.md`); the moment its screen no longer shows one, the queue is written,
oldest first, and the hand comes down. Octoboard never answers such a screen: the user does, in the session's terminal. A client's
`send_message` request meanwhile is queued like any other message, not refused.

**And while an interrupt is typing into the session.** While `interrupt_session` sends its keys into a Claude Code or
Grok Build session whose input box it finds on the screen, and clears what the agent puts back in its input box,
everything Octoboard has to write into the session is queued, so that nothing joins the restored prompt, and the queue
is written as soon as the interrupt is done (see "The input box afterwards" in `docs/product/interrupting-turns.md`). It
lasts no longer than the interrupt takes to answer, raises no hand, and is not reported in `writes_held`.

**And for a moment after a Grok Build error.** When a Grok Build session's hooks report that its agent stopped on an
error (`StopFailure`, or a notification of type `agent_error`), what Octoboard writes into it is held for 2 seconds, or
until a prompt is sent there if that comes sooner, since Grok may be drawing its usage-limit screen. After that the
screen holds the messages if it is up, and they go in otherwise. This hold alone raises no hand. An error of a
subagent's, or of another agent, holds nothing.

There is a second way a message Octoboard accepted does not arrive. If the agent stops taking input
part way through a write, part of that message is left sitting in the session's input line. Whatever
Octoboard writes next would be run together with it, so that message and everything queued behind it
are dropped rather than delivered spoiled. A message written while the draft is stashed is the exception: the session
then holds Octoboard's later writes, so only that message is lost (see "A message only partly written while the draft
is stashed" below), and nothing below applies to it.

Octoboard does not try to repair the line. The damage is expected to be bounded on its own — the next message written
into the session closes the leftover fragment and goes in with it — so the cost would be one spoiled message rather than
a session that stays unusable, and that is the better of the two: refusing to write until something proved the line was
clean would stall a console's whole orchestration on a signal nothing can give. A partial write has never been seen to
happen, so neither has this bound been seen to hold.

The loss is reported rather than passed off as delivered, but not to everyone: the sender whose own
call ran into it is told (a report fails, and its session is *not* archived), while a sender whose
message was already queued behind it is not. What covers those is the `queued_messages_dropped` notice the user gets,
which names the session and tells them to look at it before anything else is sent.

**What the owners' tools report.** Every session record the session tools give (see "Session records" in
`docs/product/what-every-session-reads.md`) carries `queued_messages`, the number of messages queued for the session and
not yet written — what is kept for an interrupted session until its resume included (information, and for an owner
the reports and notices), each piece counted once — and
`writes_held`: `null`, or, for a running session that holds what is written to it whatever its `status` reads, an object
with `reason`, an `explanation` in prose, and, for `agent_dialog`, the screen's code in `dialog`. A session held only
because its agent reported it is waiting for the user — at a permission prompt or a question — carries `null`: its
`status` says so. When several holds apply, the first of these is reported:

| `reason` | The hold |
|---|---|
| `starting` | The agent has not yet signalled that it is past its startup screens, so its trust confirmation may still come — or, for Claude Code, has sent its first hook report but does not yet have bracketed paste on (see above). |
| `trust_confirmation` | Its folder-trust confirmation is up and has not been answered. |
| `agent_dialog` | It is showing a screen only the user can answer (above). |
| `agent_action` | Codex's terminal title says it is waiting on the user. |
| `agent_error` | The moment after a Grok Build error (above). |
| `sharing_change` | It is being restarted for a change of its account's sharing of the default configuration, and is resumed on its conversation afterwards; what is queued for it is written into the resumed process (see "When a change is carried out" in `docs/product/sharing-the-default-configuration.md`). |
| `handoff` | It is being handed off to another agent. |
| `submission_uncertain` | Codex's own queue did not confirm accepting a message; later messages wait up to three seconds or until another submitted prompt, whichever comes first (see "A message whose acceptance is uncertain" below). |
| `unsent_message` | A message written into it was left unsent (see "A message written but not sent" below). |

An input-box, unconfirmed-edit or draft-preservation hold — a draft being stashed or restored, or a partly written
message — is not included in `writes_held`. When no other listed hold applies, that field can be `null` while
`queued_messages` still counts messages waiting for safe draft preservation, confirmed edits or the end of such a hold,
or, on a Codex session whose messages go into Codex's own queue, for Codex to take them (see "Messages through Codex's
own queue" below).

## A message only partly written while the draft is stashed

If only part of a message reaches the input after Octoboard pressed the stash key, whether or not saving was confirmed,
the message is lost: it is not sent again, and what the input holds is unknown — a paste left open, the message's text
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
held until a session can take them" above).

## Messages through Codex's own queue

On a Codex launch with its private app server (see "Codex's private app server" in
`docs/product/per-agent-launch-specifics.md`), Octoboard submits each message to Codex's own queue for the session's
conversation instead of typing it into the terminal. The composer is not read or touched: what the user has in it —
multiline text and long collapsed pastes included — and the cursor stay as they are, and nothing of it is submitted
with the message, whether the session is idle, mid-turn or past a cancelled turn, and before the user has submitted a
first prompt too.

**When it runs.** In an idle session the message runs at once, as a turn. Mid-turn, Codex queues it and runs it as a
turn of its own after the running one, unlike a message written into its terminal, which Codex steers into the running
turn. A message never overtakes anything queued before it. Codex itself starts an item added to an idle conversation
only when its queue was empty; one added behind items a cancel left queued waits there (measured on Codex 0.166.0). So
after adding a message to a queue that holds items ahead of it, Octoboard starts the queue's first item when that item
is one of its own — Codex then runs the rest in order, one turn each, the new message among them — and starts nothing
when the first item is someone else's, which leaves the message waiting behind it. Octoboard's own messages keep the
order they were sent in.

**After a cancelled turn.** Codex leaves what its queue holds there past a cancel, and does not run it by itself. After
every cancel that stops a turn — an owner's `interrupt_session` (see "Messages during an interrupt" in
`docs/product/interrupting-turns.md`) or the user's own cancel in the terminal (Esc, which Codex reports with its
`Interrupt` hook event) — Octoboard looks at the queue of the session's own conversation and, when the first message
waiting there is one of its own, starts it. Codex runs what follows it in the queue after that turn, as after any turn,
so they run as turns of their own, one after another. A message whose acceptance was not confirmed (see "A message
whose acceptance is uncertain" below) counts as one of Octoboard's own there. Octoboard starts nothing when none of its
own messages wait there, and nothing when a message someone else put into the queue is ahead of its own: it never
starts a message it did not queue, nor its own ahead of one. The cancel's hook event and an owner's interrupt each
start the queue. A second start within about 10 seconds of one reads whether the conversation is running a turn: while
it is, or when that cannot be read, it is the same cancel's second start and starts nothing more; a conversation idle
again has had a cancel of its own since, and its queue is started as after any cancel. Codex's terminal does not put the user's own mid-turn input
into this queue (measured on Codex 0.166.0: Enter steers it into the running turn, Tab queues it inside the terminal),
so in practice only Octoboard's messages are there. The interrupt key of an owner's `interrupt_session` is never
delayed by any of this: the queue is looked at only once the key is in.

**What still holds it.** The holds in "Messages held until a session can take them" above apply as they do to the
terminal: such a message waits while the session is being stopped, while its trust confirmation may be up, while it
shows a screen only the user can answer, and under the other holds listed there. The draft-preservation checks and
their grace period do not apply, and nor do the checks of "A message written but not sent" below.

**Which conversation it goes to.** Always the session's own, identified once and kept while its process runs: on a
resume, the resumed conversation; on a new session, the first conversation of the user's that the server reports
starting. That can come before the user's first submission, so an automatic message can be the session's first prompt.
Codex's ephemeral conversation for generating a title, its sub-agents' conversations, and further tasks the user opens
or creates in Codex's agent command center never receive the session's messages: even while the user has switched to
another task there, they go to the original conversation.

**Before the session's own conversation is known:**

- While the server has loaded no conversation the session could own — its trust confirmation still up, the terminal
  still starting, or the agent command center open with no task — the message waits in the session's queue, and is
  tried again every 2 seconds, and as soon as the server reports the session's conversation started.
- The message goes through the guarded terminal delivery instead, with its draft checks and grace period (restarted
  once after the server held it back, as "Messages held until a session can take them" above says), when the server cannot be reached, a request to it fails, or
  the session's conversation cannot be told apart: several conversations the user started are loaded, or
  conversations are loaded none of which the user started. Each message is judged on its own, so the next one tries
  the queue again.

**Once the session's own conversation is known**, a message never goes into the terminal, which may be showing another
task of the agent command center. Codex's server unloads the original conversation a while after the terminal has
switched to another task (two to three minutes, measured on Codex 0.166.0). A message for a conversation that is not
loaded has the server load it again (`thread/resume`, naming only the conversation, so its settings stay as they are),
which also runs what its queue already holds. When that fails, or the server cannot be reached or does not answer,
the message waits in the session's queue and is tried again every 2 seconds. So does a message the server answers is
overloaded and to retry later ("Server overloaded; retry later." in Codex 0.166.0): that answer takes nothing in, and
the message does not fail. The same goes for a message Codex took into its queue but whose start it answered with
that reply (see "A message whose acceptance is uncertain" below): it waits there and is tried again every 2 seconds;
tried again, it is started where it waits in Codex's queue rather than added a second time, and once it is no longer
there, having run meanwhile, it counts as delivered. When Codex otherwise refuses to take the message into its queue,
the message fails instead of waiting (see "A message Codex's own queue refuses" below).

**A server that has exited** keeps the message queued, whether or not the conversation is known: the session's
terminal ends with its server (see "Codex's private app server" in `docs/product/per-agent-launch-specifics.md`).

**Messages that wait.** While a message waits for Codex to take it — the session has no conversation yet, its
conversation could not be loaded again, or the server could not be reached, did not answer or answered that it is
overloaded, adding the message or starting it — its sender is told it
was queued, `queued_messages` counts it, and `writes_held` reports nothing for it. Once messages have waited about
30 seconds, every client is sent the `provider.codex.messages_held` notice about the session, once per wait. The window
shows it as a toast (see `docs/product/toasts.md`): messages for the session are waiting because Codex is not taking
them yet, and Octoboard sends them once it does and types nothing into the terminal meanwhile; the toast names no
cause. When a message goes in, the terminal takes it, or Codex refuses it or does not confirm taking it, the wait ends
and the notice is withdrawn (`session_notice_withdrawn` with that code), as it is when the session's server stops,
which it does when the session ends; the window leaves a toast it is showing as it
is.

## A message whose acceptance is uncertain

Octoboard adds each message to Codex's queue and then, while the message is still queued and the queue's first item is
one of Octoboard's own, asks Codex to start that first item (see "Messages through Codex's own queue" above). A message
gone from the queue by then, or waiting behind an item someone else queued, counts as accepted. Codex refuses to start
an item it has already started or will run after the running turn, so a refused start is judged from Codex's queue, not
from the refusal's wording: the message gone from the queue, or still in it behind a running turn, counts as accepted; a
conversation the server has unloaded meanwhile is loaded again, which runs its queue, and judged once more; still in
it with no turn running, or a queue that cannot be read then, leaves its start unconfirmed — unless Codex refused the
start as overloaded ("Server overloaded; retry later."), in which case the message is not uncertain but waits in
Codex's queue to be started again (see "Messages through Codex's own queue" above).

If Codex's own queue may have accepted a message but that cannot be confirmed — the connection to the server lost after
the request went out, an acknowledgement without the queued message's id, or a start left unconfirmed as above —
Octoboard never sends that message again automatically, through the queue or the terminal. Nothing is written into
the composer. Later messages remain queued for three seconds, and are then written, or until the agent reports another
submitted prompt, whichever comes first; the hold is reported as `submission_uncertain`. The user should check the
conversation before retrying manually, since the message may already be queued or running. This uncertainty does not
discard the later messages and is distinct from a partial terminal write, which drops the affected queue as described
above.

The sender whose own message it was — a client's `send_message` request or report panel form submission
(`submit_page`) included — is answered with the error `message_acceptance_unconfirmed`, naming its agent
(`agent`): the agent did not confirm accepting the message, which may already be queued or running and will not be
sent again, so it is not to be sent again before the conversation is checked. A sender whose message was queued behind
it is told it was queued. What a reporting session is told is in `docs/product/reporting-to-owners.md`.

A message Octoboard sends on its own account is taken as possibly delivered when it ends this way, not as undelivered:

- a message forwarded during a handoff, or the handoff's first work prompt, raises no `handoff_messages_dropped`
  notice (see "Messages for the original during a handoff" and "When a handoff fails" in
  `docs/product/handing-off-sessions.md`);
- a message whose turn is spared a synthesised report — the outcome message of a session request, the message telling
  a relaunched session which processes its restart ended, the message of an automatic account switch — still spares
  that turn, so a report for it is not synthesised twice;
- a session being told of a change of its owner is not owed the notice again at its next launch (see "A change of
  owner told at the next launch" in `docs/product/changing-session-owners.md`);
- a session being told of a branch or process transfer is not named in the transfer's `note` as one the caller is to
  tell itself (see "Transferring branches between sessions of an owner" in
  `docs/product/branch-responsible-session.md`);
- the summary `bind_to_session` delivers as the caller's first report counts as that report, and the caller is told
  not to report it again (see "Setting a parent for an independent session" in
  `docs/product/changing-session-owners.md`);
- an automatic retry of a turn stopped at its model's capacity counts as sent, so the wait for it is not ended with
  the held report (see "When the selected model is at capacity" in `docs/product/turns-hooks-do-not-report.md`).

Every client is sent the `message_acceptance_unconfirmed` notice about that session, naming its agent (`agent`),
provided the session's process that was written to is still running. The window shows it as an error alert (see
`docs/product/error-alerts.md`) titled "Message not confirmed", naming the session and where it is: the agent did not
confirm accepting a message Octoboard wrote into the session, and the user is to check the conversation before deciding
whether to send it again. Its details add that the message may already be queued and will not be sent again, and that
later messages are held for at most three seconds or until another prompt is sent. The alert, and the error a client's
request is answered with, is a known failure with no remedy beyond its own text, not an unexpected one.

## A message Codex's own queue refuses

Once the session's own conversation is known, Octoboard asks Codex to add each message to that conversation's queue.
Codex takes a message even for a conversation it has unloaded (measured on Codex 0.166.0), so a refusal of that request
is about the message itself. Such a message fails:

- It is not sent by any route: not tried again through the queue, and not written into the terminal. Nothing is
  written into the composer.
- The messages queued after it go on as usual; nothing behind it is held or dropped.
- The sender whose own message it was — a client's `send_message` request or report panel form submission
  (`submit_page`) included — is answered with the error `message_refused`, naming its agent (`agent`) and
  giving Codex's reason (`detail`): the agent refused the message, so it was not sent. A sender whose message was
  queued at the time is told it was queued and learns nothing more from its own call; the notice reports the refusal.
- Every client is sent the `message_refused` notice about that session, with the same `agent` and `detail`. The window
  shows it as an error alert (see `docs/product/error-alerts.md`) titled "Message refused", naming the session and
  where it is: the agent refused a message Octoboard wrote for the session, so it was not sent. Its details add that
  later messages are still sent, followed by Codex's reason, shown as raw text under **Error details** (see "Names and diagnostic details" in `docs/product/error-alerts.md`). The
  alert, and the error a client's request is answered with, is a known failure with no remedy beyond its own text, not
  an unexpected one.

A message Octoboard sends on its own account that ends this way is undelivered, as for any other failed write, except
that a message forwarded during a handoff, or the handoff's first work prompt, raises no `handoff_messages_dropped`
notice, since the refusal has already been shown as `message_refused` (see "Messages for the original during a
handoff" and "When a handoff fails" in `docs/product/handing-off-sessions.md`). What a reporting session is told is in
`docs/product/reporting-to-owners.md`.

## A message written but not sent

Claude Code can take a message Octoboard writes and still not send it: the text stays in its input box and no turn
starts. Invisible characters are one cause, which their removal above is there to prevent; anything else that leaves a
message unsent is caught the same way. Codex on a Windows daemon host can leave a message in its input box too (see
"Codex on a Windows daemon host" below).

**The check.** When Octoboard writes a message into a Claude Code session whose agent is at its prompt — whichever
status it shows there, not mid-turn — it waits up to about three seconds for the agent's hooks to report
a prompt sent (`UserPromptSubmit`). On a Windows daemon host the first message of a launch — a new session's, a resumed
one's or a relaunch's, written before the agent has reported any prompt in that launch — is given three times as long,
about nine seconds, since the launch's first hook starts cold there; later messages get the usual wait. This applies to every message Octoboard writes: an owner's instruction, a report,
shared information, a report panel form submission, the instruction `resume_session` hands over or the prompt to
continue after an account switch when it is written rather than given as the launch prompt, the messages of a
transfer or a handoff, and a client's own `send_message` request. A message that was queued is checked the same way
when it is finally written. When several queued messages are written in one go, each is checked on its own, against the
agent's status at the moment it is written: once the first is sent the agent reads as working, so the rest are written
unchecked, as mid-turn messages; if the agent is at its prompt again by then — after a very short turn, say — the next
one is checked too.

- With the report in time, the message counts as delivered.
- Without it, the message is taken to be waiting in the session's input box: it is **left unsent** — unless one of the
  screens that hold messages came up while it was written, which is told apart as in "A message a dialog discarded or a
  view took" below.

A background task's completion does not count as a prompt sent. Claude Code reports a finished background task to
itself as a `UserPromptSubmit` whose prompt contains `<task-notification>`; that prompt neither confirms a message
written during the wait nor lifts the hold below. Neither does the report a subagent resumed with `SendMessage` hands
back, which Claude Code submits to itself as a prompt holding an `<agent-message from="…">` block. Any other prompt
counts, whoever sends it.

**Codex on a Windows daemon host.** There the terminal hands Codex a paste as typed keys, and Codex reads an Enter
that arrives in the same burst as a line break in the text, so a message written together with its Enter would stay in
the input box. So Octoboard writes the message's paste alone, and its Enter only once Codex has caught up with the
paste: its input box shows the whole message (compared without whitespace, so wrapping does not count; for a message
too long for the box, its last 40 characters or more; for a large paste, Codex's `[Pasted Content <n> chars]` naming
the message's length; and nothing else, where the message went into a box found empty, so text the user typed
meanwhile holds the Enter back), and after that the box has held still for the settle window (250 milliseconds). Only
the box counts, not the rest of the screen: mid-turn, Codex redraws its spinner and status line all the time. It waits
up to **15 seconds** for that, not counting the time no input box is recognised on the screen at all. Octoboard reads
the box above each footer Codex draws in its ordinary states with its default keys and status line, down to very
narrow terminals, including the "tab to queue message" footer Codex shows while a turn runs a tool with the message in
its box; the uncounted time is a fallback for a screen whose footer Octoboard does not know. That allowance is only
for a message written into a box found empty; its unrecognised time has a cap of its own, **10 minutes** in all,
during which the session's queue waits and the session does not count as idle, and the wait ends at once if the
session ends, is being stopped or is archived. If the box never shows the message within those caps, no Enter is
pressed and the message is **left unsent**, as below. After the Enter, an input box that empties counts as the message
sent, as does one no longer recognised on the screen, so no Enter goes into whatever took its place. Enter is pressed
once more only when the first became a line break: the box's rows changed, end in an empty row below the whole
message, and the box held still so. A box the Enter did not change is waited on, not pressed into again. If neither
comes within 15 seconds of an Enter, or the box still holds the message after the second, the message is left unsent,
but its Enter has had its chance: no further Enter is pressed for it. This applies to every message Octoboard writes
into such a session's terminal, mid-turn or at its prompt; a message that goes into Codex's own queue (see "Messages
through Codex's own queue" above) is not written into the terminal, so none of this applies to it.

**What a message left unsent does:**

- Its text is not written again or put back in the queue: it is already in the session's input box.
- Every client is sent the `message_not_sent` notice about that session, naming its agent, provided the session's
  process that was written to is still running when the check ends. The window shows it as an error alert (see
  `docs/product/error-alerts.md`) titled "Message not sent", naming the session and where it is:
  - For a message the check above left unsent the notice carries `submit_retry: "once"`, and the alert says the agent
    has not confirmed sending the message, and that Octoboard will retry its submit key once after three seconds,
    without pasting the text again. The user is told to answer pending dialogs first and check the automatic retry; if
    the message still waits in the composer, press Enter without pasting it again.
  - For a Codex message left in its input box on a Windows host (its Enter held back, or pressed and not taken) the
    notice carries no `submit_retry`, and the alert says the agent did not send the message, so it is waiting in the
    session's input box, and that Octoboard writes nothing more into the session until a prompt is sent. The user is
    told to open the session, check the message waiting in its input box and press Enter to send it.
- **The notice is withdrawn** once the agent reports a prompt sent after the message the notice was about — the
  message's own report arriving late, the submit-key retry having worked, or the user pressing Enter: every client is
  sent `session_notice_withdrawn` naming the session and the code `message_not_sent`, and the window closes that
  session's "Message not sent" alert, or drops it from the alerts waiting to be shown. A client not showing the notice
  ignores the withdrawal; later prompts withdraw nothing more.
- Later messages wait until the agent reports another submitted prompt or, for a message the check left unsent, the
  single submit-key retry is attempted; behind a Codex message left in its input box on a Windows host, only a prompt
  the agent reports releases them. If a prompt has already been reported, Octoboard skips the retry. Otherwise, three
  seconds after the message was
  marked unsent, it presses Enter once into the current input, which may include the user's later edits. A recognised
  dialog or trust confirmation must clear first. Once the attempt is made, the hold clears even without another
  submitted-prompt report, and later messages can proceed under the other holds above. If writing the submit key fails,
  every client is sent the `submit_retry_failed` notice about that session, naming its agent and carrying the failed
  write's reason. The window shows it as an error alert titled "Submit not confirmed", saying the agent has not confirmed
  sending the message and Octoboard could not press Enter again, and telling the user to check the input box and, if the
  message is still there, press Enter without pasting it again. The key is not retried again and the message is not
  written again.
- A message left unsent after its draft was put in the agent's native stash (see "Draft preservation has a bounded
  wait" above) still counts as sent for the draft's protection: restoring the stash is the agent's, and the user's own
  send of the message, which brings the draft back, ends the protection. No agent both uses a native stash and has its
  submit key checked as above today, so this case is not reached yet.
- The hold belongs to the session's process that was written to and ends with it, as its queue does: a session resumed
  later starts without one.

**What the sender is told.** A sender whose own message was left unsent is told it was written into the session's input
but its agent did not send it to the conversation, so the session has not read it; that the user has been told; and that
it must not send it again. The same is said of a message a view of background agents took (see "A message a dialog
discarded or a view took" below), and the tools' wording names both: the message is in the input box, or went to a view
of the agent's background agents. How each tool words this, and what follows from it, is with the tool: the owner's
`instruct_session` in "The console session's tools" in `docs/product/hub-orchestration.md`, `report` in
`docs/product/reporting-to-owners.md`, the `inform_session` tool in `docs/product/information-between-sessions.md`,
`transfer_session` in "Handing a session to another owner", `release_session` in "Releasing a session from its owner"
and `bind_to_session` in "Setting a parent for an independent session", all in
`docs/product/changing-session-owners.md`. A sender whose message arrives while the session is held behind an unsent one
is told it was queued, as for a session that cannot take a message. A sender already told its message was queued learns
nothing more if that message is later left unsent; the notice is what reports it.

**A client's `send_message` request** (the daemon's client protocol) to a Claude
Code session at its prompt is answered once the check has ended — the second one too, for a message a dialog discarded
and that is written again in the same delivery (see "A message a dialog discarded or a view took" below) — not as soon
as the message is written. A message left unsent is not refused: the client is still answered with success, and, like
every client, gets the `message_not_sent` notice.

**What is not checked:**

- A message written while the agent is mid-turn. The agent queues it and sends it only when the turn ends, which no
  short wait can see, so a mid-turn message Claude Code holds back is not detected and counts as delivered.
- Grok Build sessions, and Codex sessions outside a Windows daemon host: a message counts as delivered once it has been
  written. A Codex message written into the terminal on a Windows daemon host is checked only as "Codex on a Windows
  daemon host" above says. A message Codex's own queue takes counts as delivered once the queue has accepted it, with
  the acceptance rule in "A message whose acceptance is uncertain" above.
- A message written through a native stash whose input recovery remains pending: the recovery rules above apply,
  and the message is not reported as an unsent draft or retried.
- Which prompt was sent. The prompt's text is not matched against the message: a prompt the user sends within those
  seconds is taken as the message sent.

**What the check gets wrong.** In two cases a message the agent has in fact queued is reported not sent, with the
notice and the hold that follow, and a `done` report written then archives nothing (see "Automatic archiving" in
`docs/product/reporting-to-owners.md`). The hold can lift when the agent sends the queued message at the end of its
turn, or through the one-time submit-key retry above.

- After "Chat about this" is chosen on a question, Octoboard reads the Claude Code session from its transcript as back
  at its prompt while its agent is in fact still working, so a message written then is checked.
- A background task's completion can arrive during the few seconds' wait. The agent starts a turn on it and queues the
  message just written until that turn ends, and the completion does not count as a prompt sent. This case is inferred
  from how the check works and has not been observed.

## A message a dialog discarded or a view took

While a checked message is written and waited on, Octoboard also watches the session's screen for the screens that
hold messages (see "Messages held until a session can take them" above), so that one which comes up as the message
goes in is seen even if the message's own Enter has taken it down again by the end of the wait. When the agent has not
reported the prompt sent in time and such a screen came up meanwhile, what became of the message depends on the
screen:

- **A dialog that throws the message away** — "Resume this conversation?" at a launch, "Resume from summary", a notice
  that waits for Enter, "Make auto mode your default permission mode?", or the focused list of background agents.
  Nothing of the message is in the input box. It goes back to the front of the session's queue and is written again once
  the dialog is gone, and, being written again, it is checked again. No unsent hold is set. When the message's own Enter
  has already closed the dialog, it is written again straight away, within the same delivery — at most once per
  delivery, so a dialog that every write brings back cannot keep the sender waiting; a dialog still up, or one that
  comes back, leaves the message queued until it is gone. Every client is sent the `message_discarded_by_dialog` notice
  about the session, naming its agent, provided the process written to is still running; the window shows it as a toast
  (see `docs/product/toasts.md`), not an error alert, titled with where the session is, saying the agent showed a dialog
  while Octoboard wrote a message into the session and discarded it, and that the message is written again once the
  dialog has been answered. A sender is told what its message came to: when it was written again within the same
  delivery, what that second write came to — delivered, left unsent, or queued again; otherwise that it was queued, as
  for a session that cannot take one, which is also what the sender of any message waiting behind it is told.
- **A view of background agents** — a subagent's view or the agents view. The message has gone to the subagent, or
  started another background session, so it is not written again: that would deliver it twice. No unsent hold is set;
  the rest of the queue waits until the view is left, as it does behind any screen that holds messages. Every client is
  sent the `message_diverted_by_view` notice about the session, naming its agent, provided the process written to is
  still running; the window shows it as a toast titled with where the session is, saying the agent was showing a view
  of its background agents, so the message went there rather than to the conversation, and asking the user to check
  the session. It does not say the message will be written again. Its sender is told what the sender of an unsent
  message is told (see "What the sender is told" above); a sender whose message was waiting behind it is told it was
  queued.
- **"Resume this conversation?" coming up by itself in an idle session** keeps the message as a draft in the input box
  and starts no turn: that is a message left unsent, as above.

Only a message the check covers — one written into a Claude Code session at its prompt — is told apart this way. Codex
and Grok Build screens, and anything written mid-turn, are only held for before the write, never checked after it.
