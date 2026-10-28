# Interrupting a session's turn

An owner can stop the turn a session it owns is in the middle of, as the user pressing the agent's interrupt key in the
terminal would. The session keeps its process and its conversation and goes back to *awaiting instructions*, for the
owner to redirect it with `instruct_session`. This is not the *interrupted* status: that is a session whose process has
ended (see `docs/product/session-statuses.md`), and an interrupted turn leaves the process running.

## Who can interrupt a turn

- **An owner, through its `interrupt_session` tool.** A console session and a project session that can own sessions — a
  coordinator — get the tool (see "The console session's tools" and "Project coordinators' tools" in
  `docs/product/hub-orchestration.md`); a project session with the worker role does not. It acts only on a session bound
  to the caller: for any other — independent, bound to another session, a console session, or the caller itself — it is
  refused in prose naming who owns it, as `instruct_session` is, and nothing is sent.
- **A client, through the daemon's `interrupt_session` request.** It checks no ownership: any session can be named.
- **The window has no control for it.** The user interrupts a turn by pressing the agent's own key in the session's
  terminal.

## The call

The tool and the request take one argument, `session`, the session's id, and run the same interrupt. The answer
carries:

| Field | Meaning |
|---|---|
| `interrupted` | `true` when the interrupt key was written and the turn ended within the wait (see "After the keys" below); `false` otherwise, including when there was nothing to interrupt. |
| `status` | The session's status once the interrupt has settled. It can already read *working* again when a message its agent had queued started the next turn at once. |
| `note` | Prose saying what the caller is to know (see "What the note says" below). |

Over the control protocol the request is `{"type": "interrupt_session", "id": …, "session": …}` and the reply is
`session_interrupted`, with `id`, `session`, `interrupted`, `status` and `note`. A refusal is an error with one of the
codes below; through the tool it reaches the model as its reason in prose.

## When a key is sent

What happens depends on where the session stands when the call is made:

| The session | What happens |
|---|---|
| In a turn: *working* by its hooks | The agent's interrupt key is sent (see "The keys each agent gets" below). |
| At its prompt — *awaiting instructions* or *working in background*, including one holding messages after a Grok Build error or behind a message left unsent | Nothing is sent; answered with `interrupted: false` and a note that there was nothing to interrupt. |
| Already being interrupted by another `interrupt_session` | Nothing is sent; answered with `interrupted: false`. |
| Waiting for the user: its agent's permission prompt or question, a screen of the agent's own that only the user can answer, Codex's `Action Required` title, or a session request of its own waiting for the user's answer | Refused with `session_waiting_for_user`: a key would answer it in the user's place. |
| Showing its folder-trust confirmation | Refused with `session_trust_pending`. |
| Archived or *interrupted*, with no process, still being launched, or not yet past its startup screens | Refused with `session_not_running`. |
| Being switched to another account, handed off, having its branches handed over, given another owner, or restarted for a change of its account's sharing | Refused with `session_busy`. |
| An id that names no session | Refused with `unknown_session` (through the tool, in prose). |

A key is written only while **both** of these hold, checked right before each key: the session reads *working* by its
hooks, and its terminal title shows a turn running — a running indicator the agent has sent within the last few
seconds (Claude Code's turning half-filled circle, Codex's and Grok Build's braille spinner). When either fails at
that moment — the turn has just ended, or the title does not show it running — nothing is sent and the answer is
`interrupted: false`, with a note saying which. The same goes for a screen of the agent's own coming up just then, and
for anything else being written into the session's terminal between the check and the key. None of these is an
error.

## The keys each agent gets

Each key is a write of its own, never joined to a message, and an interrupt never sends a second interrupt key.

| Agent | Keys |
|---|---|
| Claude Code | One Esc. |
| Codex | One Esc. Never Ctrl-C, which at Codex's prompt exits it. |
| Grok Build | Ctrl-U, then Ctrl-C. Grok ignores Esc, and with text in its composer its Ctrl-C clears the text instead of cancelling the turn, so the composer is emptied first. |

**A draft in Grok Build's composer is cleared before the Ctrl-C.** Octoboard reads the composer off the screen and,
while it holds text, presses Ctrl-U one at a time, each press seen on the screen, until it shows empty; an empty
composer, or one that cannot be read, gets a single Ctrl-U. The note says a draft was cleared, and that Grok may let
the user undo the clearing with ctrl+z. When the draft does not clear, or its text cannot be read back to clear it, no
Ctrl-C is sent, and the note says which keys went in.

## The input box afterwards

**Claude Code and Grok Build put the interrupted prompt back in their input box** when the key lands before the
model has shown anything; a message written after that would be sent together with it. So once the title shows the
turn has stopped, Octoboard clears that text with Ctrl-U, one press at a time while the screen shows text in the box,
and confirms on the screen that the box is empty. It does so only when the box was empty before the interrupt key and
nothing else has been written into the terminal meanwhile.

- **Text that was already in Claude Code's input box before the Esc** — typed by the user, or a message left unsent
  (see "A message written but not sent" in `docs/product/message-delivery.md`) — is never touched.
- **Text left in the box afterwards** — a restored prompt that could not be cleared, or text that was there before —
  is reported in the note, which asks for the user to clear the input box before anything more is sent.
- The input box is read with faint text left out, so the placeholder Claude Code shows in a fresh session's empty box
  does not count as text.
- Codex leaves its composer empty after an Esc, and nothing is cleared there.

A cancel the user makes in the terminal is left as the agent leaves it; only an interrupt made with
`interrupt_session` clears the input box.

## After the keys

The answer waits up to **five seconds** for the turn to end. A turn that ended counts even when a message the agent
had queued starts the next one at once. A session that still reads *working* after the wait is answered with
`interrupted: false` and a note saying so, to be checked with `get_session` before it is sent anything.

The session leaves *working* the way it does when the user presses the key: on Codex's own `Interrupt` hook event, on
Grok Build's `StopCancelled`, or, for the cancels that fire no hook event, from Claude Code's transcript and terminal
title and from Grok Build's terminal title (see "Cancelling a Claude Code turn" and "Cancelling a Grok Build turn
before its first token" in `docs/product/turns-hooks-do-not-report.md`). Either way:

- the session is *awaiting instructions*, its process and conversation intact;
- the interrupted turn is not reported on: no report is synthesised for the owner for it (see "When a session does not
  report" in `docs/product/reporting-to-owners.md`);
- messages Octoboard holds for the session are written once the interrupt is done (see "Messages during an interrupt"
  below).

## Messages during an interrupt

**For a Claude Code or Grok Build session, messages are held while the keys go in**, so none joins the prompt the
agent puts back in its input box, and written right afterwards (see "Messages held until a session can take them" in
`docs/product/message-delivery.md`).

**What becomes of the messages written during the cancelled turn** differs by agent, and the note says it, with about
how many messages Octoboard wrote into the session while that turn ran (approximate: a message written just as the
turn began or ended can fall on either side):

- **Claude Code** may already have taken some into the turn it cancelled; the rest stay queued in it and run next, as
  a new turn.
- **Grok Build** keeps them queued and runs them next, as a new turn.
- **Codex** steers a message written mid-turn into the running turn, which the cancel throws away with it: unless the
  turn had already acted on them, they were not acted on and are to be sent again.

## What the note says

The note is prose for the caller, and says whichever of these apply:

- that there was nothing to interrupt, or that an interrupt of the session is already under way;
- that no key was sent, and why: the terminal does not show a turn running, the agent is showing something only the
  user can answer, something else was written into the terminal just then, or a Grok Build draft could not be read;
- which keys went in when not all did, and why the rest did not;
- that the session still reads *working* after the wait;
- that the turn was stopped and the session awaits its next instruction by `instruct_session`;
- what may have become of the messages written during the cancelled turn, with their approximate count (see
  "Messages during an interrupt" above);
- that a draft in Grok Build's composer was cleared, which Grok may let the user undo with ctrl+z;
- that the input box still holds text after the interrupt, which the user should clear before more is sent.
