> Severity: Major

# A message answers Claude Code's "Make auto mode your default permission mode?" screen

## Symptom

A message Octoboard writes into a Claude Code session showing "Make auto mode your default permission mode?" answers
that screen with Yes, changing the account's `defaultMode` to `auto`, and the message itself is never sent.

## Reproduction steps

1. Pick a Claude Code account whose configuration has never shown the auto mode default prompt (a fresh account
   directory, for instance) and whose `defaultMode` is not `auto`.
2. Start a project session on that account with the Auto approval mode, so Claude Code is launched with
   `--permission-mode auto`.
3. While the "Make auto mode your default permission mode?" screen is up, have a message written into the session (an
   owner's `send_message`, or a message that was queued during startup and is written once the session is past it).
4. Read the account's Claude Code settings afterwards, and the notices the daemon sent.

Observed once, in a real-session check with an isolated daemon and a scratch repository; how often it recurs on the
same account is unknown (the screen is presumably shown only until it is answered).

## Expected vs. actual

- Expected: the screen holds what Octoboard writes, as the other Claude Code screens only the user can answer do, and
  Octoboard never writes the user's agent configuration (`docs/product/message-delivery.md` "Messages are also held
  while the agent shows a screen only the user can answer"; `docs/product/launching-agents.md` "What Octoboard never
  modifies").
- Actual: the message's Enter picked Yes, the account's `defaultMode` became `auto`, and the daemon reported
  `message_not_sent`.

## Environment

- Claude Code; the session used the `sonnet` model. Version unknown (2.1.310 is installed on the machine where the
  ticket was filed).
- macOS, local daemon.
- Precondition: an account that has not answered this screen before, launched with the Auto approval mode.

## Scope of impact

Any session started in Auto mode on such an account that is sent a message while the screen is up: the user's own
agent configuration is changed without their word, and the message is lost to the conversation. Workaround: answer
the screen in the session's terminal before anything is sent to it, and restore `defaultMode` by hand if it was
changed.

## Leads

- Verified (observed in the check): the `message_not_sent` notice came, and `defaultMode` was `auto` afterwards.
- Inferred: the screen is not among the Claude Code screens Octoboard recognises on the rendered screen (it is not in
  the table of screens that hold messages), so nothing holds the write.
- Inferred: the screen runs no hook, so the `starting` hold may already have lifted by the time it is up.

## Acceptance criteria

- [ ] While the auto mode default prompt is up, nothing Octoboard writes reaches the session; it is queued and written
      once the screen is gone.
- [ ] The owners' tools report the session's `writes_held` as `agent_dialog` with a code for this screen while it is
      up, and the session reads as waiting for the user.
- [ ] Following the reproduction steps leaves the account's `defaultMode` unchanged unless the user answers Yes
      themselves.
