# Pending interactions

While a session waits on the user, its agent shows a prompt in its terminal: a tool permission prompt, or the questions
of its own ask-the-user tool. The daemon reads that prompt off the agent's screen, publishes it to its clients as the
session's `interaction`, and takes the user's answer from a client (`answer_interaction`), which it presses into the
agent's terminal key by key. A prompt it does not publish, and any prompt a client offers no form for, is answered in
the session's terminal as before. Octoboard's own window offers no answer form today: it answers in the terminal, and
shows the failure notice below.

## Which agents publish their prompts

Each provider declares which kinds of prompt it publishes and answers, in its descriptor's
`capabilities.interactions`: `permission` (tool permission prompts) and `question` (its ask-the-user tool's
questions). Claude Code, Codex, Grok Build, Antigravity CLI, GitHub Copilot CLI, OpenCode and Qwen Code declare both. Pi declares neither. It has no native approval or ask-the-user tool, and its extension dialogs are reported as blocked without a tool call. Answer those dialogs in Pi’s terminal. An `answer_interaction` for a session of a provider that
declares neither is refused with `provider_unsupported` (operation `interactions`), and nothing is pressed.

## When a prompt is published

A prompt is published as `Session.interaction` only while all of these hold:

- the session is shown *waiting for the user* (`status: "waiting_user"`); the field is absent at any other status;
- the provider declares the prompt's kind;
- the provider recognises the prompt on the agent's screen, which is read about every quarter of a second while the
  session waits, **and** matches it to a tool call the agent's hooks reported starting in the current turn (the turn's
  last 16 calls are kept; a new prompt the agent is sent starts a new turn, and so does a `working` report for an agent
  whose status comes from its program's reports, which also start the watch of the screen when they say the agent
  waits on the user);
- the prompt is as it first appears: nothing picked, ticked or typed in it and no text field open, and for questions,
  their first page. A prompt the user has begun answering in the terminal is left to the terminal.

So nothing is published for a prompt no hook reported a call for (Codex's MCP elicitations, a prompt that was already
up when the daemon started), for one in a layout the provider does not know, or for one that would take more than
32 KiB as published.

Each prompt published gets a new `id`, which an answer has to name. A prompt that changes on screen is published again
under a new `id`; so is one still up and unchanged after anything was typed into the session's terminal since it was
published: from then on the earlier `id` is refused. A prompt the user is part way through answering in the terminal
can stay published meanwhile, but an answer to it is refused all the same. For an agent whose prompt shows what is
selected in colour alone, so that a key typed into it cannot be read back off the screen, a prompt anything was typed
into since the daemon first saw it up is not published at all, and one published is withdrawn once anything is typed
into it and not published again while it stays up. The interaction is withdrawn once the prompt
is gone from the screen or no longer recognised there, the session no longer reads as waiting for the user, or the
session's process ends. Clients receive it,
added, replaced or withdrawn, on `session_upserted` like any field of the session, and in the snapshot.

## What a published prompt holds

```
PendingInteraction { id,
                     call_id?,          // the tool call it belongs to, as the conversation feed names it
                     kind: "permission", tool, category?, title?, detail?, input, input_truncated?: true,
                       choices: { id, label, effect: "allow"|"allow_remember"|"deny"|"deny_remember"|"other",
                                  text?: "optional" }[]
                   | kind: "question",
                       questions: { text, header?, multi_select, options: { label, description? }[],
                                    free_text?: "instead"|"with" }[],
                       dismissible }
```

- **A permission prompt** carries the tool's name as the agent calls it, its category and one-line title as the live
  conversation feed classes the same call (see "Tool calls" in `docs/product/live-conversation-feed.md`), the agent's
  own description where the call has one, and the call's input, each string cut at 4 KiB (`input_truncated` is then
  `true`). Its `choices` are the options the prompt offers whose effect the daemon can tell, each with the agent's own
  label: allowing the call (`allow`), allowing it and adding a rule so the agent does not ask again (`allow_remember`),
  declining it (`deny`), declining it and adding a rule (`deny_remember`); `other` is for an option of none of these
  effects, which none of the agents publishes today. An option that has the agent record a permission rule, whether
  for the rest of the session or in its settings, is `allow_remember` or `deny_remember`, published with the agent's
  label. **An option that switches the agent's permission mode is never published**: it is answered only in the
  terminal. A
  choice with `text: "optional"` can carry text for the agent with the answer.
- **Questions** carry each question's text, its short header where it has one, its options, whether several options
  may be chosen, and whether a written answer is taken `instead` of an option or `with` the options chosen (absent
  where the question takes none), and whether the questions can be dismissed.

## Answering

`answer_interaction` names the session and the interaction's `id`, with:

- `choice`, a choice's `id`, for a permission prompt, and `text` for a choice that takes it;
- `answers`, one `{selected: [option indexes], text?}` per question in order, for questions: one option at most for
  a question that is not multi-select, a written answer only where the question takes one, and not beside an option
  where it takes it `instead`; every question needs an answer;
- or `dismiss: true`, to decline questions that can be dismissed.

Text is taken as the agent would read it: control characters and the characters Claude Code treats as invisible are
removed (see "Messages held until a session can take them" in `docs/product/message-delivery.md`), and text left blank
counts as none. An answer that does not fit the prompt is refused before anything is pressed: with `field_required`
for a missing field and `unreadable_request` for one that does not fit, each naming the `field`.

The daemon then presses the answer into the agent's terminal key by key. Each key is chosen from the screen as it
stands, written only while the screen still shows what the key was chosen for and nothing has been typed into the
terminal since the prompt was published, and waited on, for up to 30 seconds, to change the screen before the next is
chosen. Where the screen tells nothing between keys (a selection drawn in colour alone), the provider chooses the keys
together, and they are written as one write under the same rules and waited on as one key; each counts towards the 64
keys an answer may take, and keys that would take it past that are not written; the only text written is the user's own, pasted into a field the prompt opened for it. While an answer is being
pressed, nothing Octoboard queued for the session is written into it, and a second answer for the session is refused.
The request is answered with `ack` once the prompt is seen gone.

- **`interaction_not_waiting`** (`session`): the prompt named is no longer up — answered already, in the terminal or by
  another client, replaced by another prompt, or gone with the session's process — or something was typed into the
  terminal since it was published, which leaves the prompt to the person typing; also for a session with no process
  and while another answer is being pressed. Nothing is pressed after that is seen. A client drops its answer form and
  shows the session as it now is.
- **`interaction_answer_failed`** (`agent`, `detail`): the answer could not be pressed — the screen was not as the next
  key needed, a key did not take, the prompt was still up after 64 keys, or the terminal could not be written. Keys may
  have been pressed before it stopped, and the prompt is left to the terminal. Every client is also sent the
  `interaction_answer_failed` notice about the session; the window shows it as an error alert titled "Could not answer
  the prompt", saying Octoboard could not enter the answer in the agent and to answer the prompt in the terminal, with
  the reason (`detail`, English, shown as it is) under its details.
- `unknown_session` for a session the daemon does not know, and `provider_unsupported` as above.

## When the prompt leaves the screen

Once a published prompt is seen gone from the screen — the agent's input box back on two looks in a row — whoever
answered it, the user in the terminal or a client, the interaction is withdrawn and the session's hand comes down: the
session is put back to *working*, and what is queued for it may be written. The answer itself is reported by no hook
event, and the next one is normally the end of the call the prompt let through, which can be minutes away for a build
or a test run; so the hand does not wait for it. What the agent does next moves the session on from *working*: the
tool's end, or a decline that ends the turn. Claude Code reports a plain decline by no event at all; it is still found
in its transcript after the hand has come down this way, and puts the session at its prompt (see "Declining a Claude
Code prompt or question" in `docs/product/turns-hooks-do-not-report.md`).

- The hand stays up while another call the agent's hooks reported still waits on the user, until that call's own
  prompt leaves or the call ends (see "The raised hand" in `docs/product/session-statuses.md`).
- A hand a session request raised stays up until the request ends; the prompt leaving only moves where the agent is
  remembered to be beneath it, to *working* (see "Waiting for the answer" in `docs/product/session-requests.md`).
- A prompt that was never published lowers the hand only as the agent's hook events do, as "The raised hand" in
  `docs/product/session-statuses.md` describes.

## Claude Code

Measured on 2.1.328.

- **Permission prompts** of Bash, Write, Edit and WebFetch, and of any other tool whose prompt has the same layout,
  matched to a call by the command, the file or the address the prompt shows. Published choices: `Yes` (`allow`),
  `Yes, and …` with a rule not to ask again (`allow_remember`), `No` or `No, and tell Claude what to do differently`
  (`deny`). Where the prompt offers to amend the answer (Bash's, Write's and Edit's do, WebFetch's does not), its first
  option, `Yes`, and its last, the `No` one, take optional text, which reaches Claude with the answer. The options that
  switch the permission mode (to auto mode, or to accepting edits for the session) are not published.
- **`AskUserQuestion`**: one question or several, single-select or multi-select. A single-select question takes a
  written answer `instead` of an option, a multi-select one `with` its options. Several questions are answered in
  turn and submitted from the review page the agent shows last. Its `Chat about this` option is not published.
  Dismissing the questions presses Esc, which the agent records as a decline.

## Codex

Measured on 0.167.1, as Octoboard launches it (private app server, `--remote` terminal).

- **Command and patch approvals**: `Yes, proceed` (`allow`), `Yes, and don't ask again for …` (`allow_remember`; it
  adds a rule) and `No, and tell Codex what to do differently` (`deny`, which also interrupts the turn). None takes
  text: Codex offers no field for it.
- **MCP tool approvals**: `Allow` (`allow`), `Allow for this session` and `Always allow` (`allow_remember`), `Cancel`
  (`deny`, which cancels the call, not the turn).
- **MCP elicitations**, a form an MCP server raises, fire no hook and are never published.
- **`request_user_input` questions**, which Codex asks only in Plan mode: single-select, each taking a written note
  `with` its option; a note alone stands for Codex's own `None of the above`. Dismissing the questions presses Esc,
  which interrupts the turn.

A question raises the session's hand from its `PreToolUse` hook event, since no permission event precedes it. Codex
fires the same event for the tool in Default mode, where it then rejects the call at once, and the two cannot be told
apart: such a call raises the hand briefly, until the turn's next event, and a session bound to an owner may send its
owner a notice that it waits for the user, at most once per turn (see "Notices to an owner" in
`docs/product/reporting-to-owners.md`).

## Grok Build

Measured on 1.0.89.

- **Permission cards**: `Yes` or `Yes, proceed` (`allow`), `Yes, allow edits in … for this session` and `Always allow:
  …` (`allow_remember`), `No, reject` (`deny`, taking optional text sent to the agent as feedback) and `Never allow …`
  (`deny_remember`). The card's last, unnumbered option, which switches the session to approving everything and
  rewrites the user's Grok configuration, is never published.
- **`ask_user_question`**: one question or several, shown one at a time, single-select or multi-select. Only the last
  question takes a written answer, `instead` of an option. Dismissing the questions presses Shift+X, after which the
  turn carries on.
- A card whose keyboard focus the user moved off it with Esc is still the same prompt: it stays published, and an
  answer first brings the focus back.
