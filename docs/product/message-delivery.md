# Message delivery

How Octoboard writes into a running session everything it has to put there — an instruction from an owner, a report,
information shared by another session, a report panel form submission — and when it holds one back.

## Messages held until a session can take them

A session that is working, awaiting instructions or working in background takes a message straight away, unless one
of the holds below applies to it, or it is a Claude Code session held behind a message its agent did not send (see "A
message written but not sent" below). A message written mid-turn is taken by the agent: Claude Code and Grok Build
queue it and run it once the turn ends, though Claude Code can also take it into the turn still running. Codex's
independent queue runs it after the active turn. What becomes of such messages when an owner interrupts the turn is
in "Messages during an interrupt" in `docs/product/interrupting-turns.md`.

**Draft preservation has a bounded wait.** On supported launches, Octoboard first tries to deliver the automatic
message separately while preserving the user's draft, without waiting for the user to finish typing:

- **Claude Code 2.1.274 or later and Grok Build 1.0.70 or later:** when Octoboard recognises a nonempty composer and
  can safely use the agent's native stash, it saves the draft, confirms the stash and an empty composer, then sends
  the automatic message. The agent restores the draft automatically after submission; Octoboard does not toggle the
  stash a second time. An existing stash of the user's is never overwritten by another stash operation.
  Claude Code additionally requires a verifiable Ctrl-S stash binding in the configuration this launch uses; an
  overridden binding or an unreadable or unrecognised keybindings file disables this path.
- **Codex 0.163.0:** automatic messages use its independent queue and leave the composer untouched, while idle,
  working or after cancellation, including before the user has submitted a first prompt. Delivery requires exactly
  one identifiable conversation in the session's private server, matching the resumed or previously identified
  conversation when there is one. Until that identity is unambiguous the message stays queued. This path is enabled
  only for that verified version; an unknown or different version retains the guarded terminal delivery below.

During a native stash and restoration, the user's continued typing is temporarily retained and replayed in order
after the draft's restoration is confirmed, or released as described below if confirmation times out. Output and
terminal responses continue meanwhile. If a dialog is shown during recovery, earlier retained keystrokes wait until
it is gone; newly typed answers can answer the dialog once restoration is confirmed or the protection times out,
and are not added to the draft's tracked edits. Successful restoration preserves multiline text and long
collapsed pastes. Claude Code and Codex retain the cursor position; Grok Build restores the cursor at the end.
Attachment restoration has not been verified and is not guaranteed.

Saving and restoring a native stash share one three-second confirmation window. If saving is not confirmed in time,
Octoboard shows an error and writes the message into the current composer with Enter, which may submit unfinished
user text with it; it does not send a second stash key. If the complete message was written but restoration is not
confirmed in time, Octoboard shows an error and releases retained typing into the current input without repeating
the message. Either timeout disables native stashing for that running process. A dialog still holds earlier retained
keys until it closes; after a timeout they can be released without a stash marker confirming restoration. If the
process ends before replay, Octoboard reports that retained input cannot be replayed into a resumed session.

**When separate delivery cannot be used, delivery takes priority after three seconds.** Each message gets a draft
protection grace period from its first delivery attempt; retries and continued typing do not reset it. During the
grace period, ordinary terminal delivery requires a recognised empty composer and confirmation that preceding user
edits were submitted. A delayed report for an earlier prompt, an automatic message or a background completion does
not confirm a newer draft. An unsupported stash, an existing stash, an unrecognised composer, unconfirmed edits or
an incomplete input sequence can hold delivery temporarily. Once the grace period has elapsed, the next delivery
attempt writes the message into the current input and submits it, even if unfinished user text is included. It does
not start another native stash wait then. Draft protection therefore does not wait indefinitely for an ordinary
prompt or a new process. This grace period does not raise the session's hand.

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
submission, information shared by another session, the instruction `reopen_session` hands over, the prompt to continue
after an account switch — because all of them can be model-authored and none is reviewed first.

**An instruction `reopen_session` hands a Claude Code session that resumes a conversation is not written at all**, nor
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
another session, a report panel form submission, the instruction `reopen_session` hands over or the prompt to continue
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
in `docs/product/session-statuses.md`); the moment its screen no longer shows one, the queue is written, oldest first,
and the hand comes down. Octoboard never answers such a screen: the user does, in the session's terminal. A client's
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
are dropped rather than delivered spoiled.

Octoboard does not try to repair the line. The damage is expected to be bounded on its own — the next message written
into the session closes the leftover fragment and goes in with it — so the cost would be one spoiled message rather than
a session that stays unusable, and that is the better of the two: refusing to write until something proved the line was
clean would stall a console's whole orchestration on a signal nothing can give. A partial write has never been seen to
happen, so neither has this bound been seen to hold.

The loss is reported rather than passed off as delivered, but not to everyone: the sender whose own
call ran into it is told (a report fails, and its session is *not* archived), while a sender whose
message was already queued behind it is not. What covers those is the message the user gets, which
names the session and tells them to look at it before anything else is sent.

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
| `submission_uncertain` | Codex's independent queue did not confirm accepting a message; later messages wait up to three seconds or until another submitted prompt, whichever comes first (see "A message whose acceptance is uncertain" below). |
| `unsent_message` | A message written into it was left unsent (see "A message written but not sent" below). |

An input-box or unconfirmed-edit hold is not included in `writes_held`. When no other listed hold applies, that field
can be `null` while `queued_messages` still counts messages waiting for safe draft preservation or confirmed edits.

## A message whose acceptance is uncertain

If Codex's independent queue may have accepted an automatic message but its response or the start of that message
cannot be confirmed, Octoboard reports an error and never sends that message again automatically. The user's draft
stays in the composer. Later messages remain queued for up to three seconds or until the agent reports another
submitted prompt, whichever comes first; the hold is reported as `submission_uncertain`. The user should check the
conversation before retrying manually, since the message may already be queued or running. This uncertainty does not
discard the later messages and is distinct from a partial terminal write, which drops the affected queue as described
above.

## A message written but not sent

Claude Code can take a message Octoboard writes and still not send it: the text stays in its input box and no turn
starts. Invisible characters are one cause, which their removal above is there to prevent; anything else that leaves a
message unsent is caught the same way.

**The check.** When Octoboard writes a message into a Claude Code session whose agent is at its prompt — awaiting
instructions or working in background, not mid-turn — it waits up to about three seconds for the agent's hooks to report
a prompt sent (`UserPromptSubmit`). This applies to every message Octoboard writes: an owner's instruction, a report,
shared information, a report panel form submission, the instruction `reopen_session` hands over or the prompt to
continue after an account switch when it is written rather than given as the launch prompt, the messages of a
handover or a handoff, and a client's own `send_message` request. A message that was queued is checked the same way
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
written during the wait nor lifts the hold below. Any other prompt counts, whoever sends it.

**What a message left unsent does:**

- Its text is not written again or put back in the queue: it is already in the session's input box.
- Every client is sent the `message_not_sent` notice about that session, naming its agent, provided the session's
  process that was written to is still running when the check ends. The window shows it as an error alert (see
  `docs/product/error-alerts.md`) titled "Message not sent", naming the session and where it is, and saying the agent
  has not confirmed sending the message, and that Octoboard will retry its submit key once after three seconds,
  without pasting the text again. The user is told to answer pending dialogs first and check the automatic retry; if
  the message still waits in the composer, press Enter without pasting it again. The notice carries
  `submit_retry: "once"`; a notice without that value retains the manual-send guidance for compatibility.
- Later messages wait until the agent reports another submitted prompt or the single submit-key retry is attempted.
  If a prompt has already been reported, Octoboard skips the retry. Otherwise, three seconds after the message was
  marked unsent, it presses Enter once into the current input, which may include the user's later edits. A recognised
  dialog or trust confirmation must clear first. Once the attempt is made, the hold clears even without another
  submitted-prompt report, and later messages can proceed under the other holds above. A failed retry raises an error
  asking the user to check the input; it does not repeat the message text.
- The hold belongs to the session's process that was written to and ends with it, as its queue does: a session resumed
  later starts without one.

**What the sender is told.** A sender whose own message was left unsent is told it was written into the session's input
but its agent did not send it to the conversation, so the session has not read it; that the user has been told; and that
it must not send it again. The same is said of a message a view of background agents took (see "A message a dialog
discarded or a view took" below), and the tools' wording names both: the message is in the input box, or went to a view
of the agent's background agents. How each tool words this, and what follows from it, is with the tool: the owner's
`instruct_session` in "The console session's tools" in `docs/product/hub-orchestration.md`, `report` in
`docs/product/reporting-to-owners.md`, the `send_message` tool in `docs/product/information-between-sessions.md`,
`transfer_session` in "Handing a session to another owner", `release_session` in "Releasing a session from its owner"
and `bind_to_session` in "Setting a parent for an independent session", all in
`docs/product/changing-session-owners.md`. A sender whose message arrives while the session is held behind an unsent one
is told it was queued, as for a session that cannot take a message. A sender already told its message was queued learns
nothing more if that message is later left unsent; the notice is what reports it.

**A client's `send_message` request** (the daemon's client protocol, not the sessions' tool of that name) to a Claude
Code session at its prompt is answered once the check has ended — the second one too, for a message a dialog discarded
and that is written again in the same delivery (see "A message a dialog discarded or a view took" below) — not as soon
as the message is written. A message left unsent is not refused: the client is still answered with success, and, like
every client, gets the `message_not_sent` notice.

**What is not checked:**

- A message written while the agent is mid-turn. The agent queues it and sends it only when the turn ends, which no
  short wait can see, so a mid-turn message Claude Code holds back is not detected and counts as delivered.
- Codex and Grok Build sessions: terminal delivery counts as delivered once written; Codex's independent queue has
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
