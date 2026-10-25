> Severity: Major

# A message queued during a fullscreen Claude Code session's startup is written before Claude Code reads input

## Symptom

In a Claude Code session using the fullscreen renderer, a message Octoboard queued while the session was starting is
written as soon as the session's first hook report arrives, before Claude Code has started reading its input: it is
never sent, and behind a launch prompt it disappears without any notice.

## Reproduction steps

1. Use Claude Code 2.1.310 with the fullscreen renderer: `"tui": "fullscreen"` in the account's `settings.json`.
2. Start a session and have a message queued for it before its first hook report (`SessionStart`) arrives:
   - a. `open_session` with no task, then an owner's `send_message` straight away (it answers "Queued: this session
     cannot take a message right now…"); or
   - b. an owner's `new_session` with a brief (the brief becomes the launch prompt), then `send_message` straight away;
     or
   - c. `reopen_session` with an instruction, for an archived session that has no conversation (fresh launch, the
     instruction is queued).
3. Wait about 12 seconds, then read the session's terminal, its transcript, the notices the daemon sent and the
   session's `writes_held` / `queued_messages`.

Measured with an isolated daemon built from main, a fake key and a local API stub: 0/10 (a, plus 0/25 more), 0/10 (b)
and 0/10 (c) messages sent. With the default inline renderer the same steps sent 10/10, 10/10 and 5/5.

## Expected vs. actual

- Expected: a message queued during startup is written once the session can take it, and starts its own turn
  (`docs/product/message-delivery.md`: "Messages held until a session can take them", where a queued message "will be
  delivered as soon as it can", and "A message written but not sent").
- Actual: in a and c, the message's text is left in the input box unsent, and `message_not_sent` is reported (with
  `writes_held` `unsent_message`). In b, the text is nowhere — not in the input box, the transcript or any API request.
  No notice is sent, `writes_held` is null, `queued_messages` is 0, and the message counts as delivered.

## Environment

- Claude Code 2.1.310 (`~/.local/bin/claude`), with `"tui": "fullscreen"` set in the config directory's settings. Not
  measured: `CLAUDE_CODE_NO_FLICKER=1`, and other Claude Code versions.
- macOS, local daemon.
- The default inline renderer is unaffected.

## Scope of impact

This affects every Claude Code session using the fullscreen renderer that is sent anything during its startup:
- an owner's message;
- the instruction `reopen_session` queues;
- messages that reach a session launched with a brief (shown silently lost);
- probably the messages forwarded to a handoff's receiving session (not tested).

In case b the message is lost and its sender is told nothing, so there is no workaround. In cases a and c the user can
press Enter in the session's terminal to send the text left in the input box.

## Leads

- Verified (timing): the daemon's write lands between 2.6 and 21 ms after `SessionStart`.
  - In fullscreen, Claude Code enables bracketed paste (`ESC[?2004h`, with `?1004h` and `?2031h`) 28–325 ms after
    `SessionStart`, so the write comes before it.
  - Inline enables it before `SessionStart`, and the write comes after it.
  - In a raw probe with no daemon, a paste+CR before `?2004h` was lost 20 times out of 55, and every one after it
    (6 of 6) was sent.
- Verified (from the 2.1.310 JavaScript):
  - At start, the CLI captures stdin itself (`startCapturingEarlyInput`), dropping `ESC[` sequences and turning CR into
    a newline.
  - When the REPL mounts, it seeds the prompt with that text.
  - Ink's first raw-mode enable stops the capture and then writes `?2004h`.
- Inferred: in case b, submitting the launch prompt clears the seeded text. This was not traced.
- Inferred: the trust confirmation is also an Ink input, so `?2004h` may appear before it is answered.
- The daemon already tracks bracketed-paste mode (`apps/daemon/src/terminal_snapshot.rs`, alacritty
  `TermMode::BRACKETED_PASTE`).
- The check after a write counts the launch prompt's `UserPromptSubmit` (about 110 ms in) as the message having been
  sent. This is why case b raises no notice. Verified by the timings; the docs say any prompt but a task notification
  counts.

## Acceptance criteria

- [ ] With the fullscreen renderer, a message queued during startup in cases a and c is sent as its own prompt (a
      `UserPromptSubmit` carrying its text) in at least 10 launches of each, with no `message_not_sent`.
- [ ] With the fullscreen renderer, in case b the message reaches the agent as a prompt of its own after the launch
      prompt, in at least 10 launches.
- [ ] With the inline renderer, the same three cases still deliver every message, with no new delay before the first
      write.
- [ ] A Claude Code session's trust confirmation still holds what Octoboard writes until it is answered.
