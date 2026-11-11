# Messages held after a usage limit

When a session's turn stops because its account reached its usage limit, every message Octoboard wrote into the
session afterwards would start another turn that fails the same way. So from such a turn's end Octoboard holds what it
writes into the session until the account can run turns again. This applies to Claude Code, Codex and Grok Build
sessions alike, and whether or not **Automatically switch accounts at the usage limit** is on. How the hold sits among
the other reasons a message waits is in "Messages held until a session can take them" in
`docs/product/message-delivery.md`.

## When the hold starts

**At the end of a turn that failed** — Claude Code's and Grok Build's `StopFailure` hook event, or a Codex turn its
rollout shows as failed — Octoboard holds the session's writes at once and reads the session's conversation record for
whether the usage limit stopped the turn. It is the same reading the automatic account switch makes, the record read
again for up to 4 seconds when it does not say so yet (see "When a session is switched" in
`docs/product/automatic-account-switch.md`).

- **The usage limit stopped it:** the hold stays, as described below.
- **Anything else, or a record that cannot be read** (a session with no conversation included): the hold ends, and
  what was queued meanwhile is written.
- **This judging lasts at most 10 seconds.** Past that, what is queued is written as if nothing held it; a verdict
  that the usage limit stopped the turn, arriving later, holds what is written from then on. While the turn is being
  judged the hold is not reported in `writes_held`, though `queued_messages` counts what waits.
- **A turn that starts while the failed one is still being judged** takes the verdict's place: the session is held, and
  reported held, until that turn ends, and how it ends decides, as in "When the hold ends" below.
- A failure reported while no turn of the session is open holds nothing.
- A Codex turn that failed at its model's capacity and that Octoboard will retry (see "When the selected model is at
  capacity" in `docs/product/turns-hooks-do-not-report.md`) is not judged.

## While the hold lasts

- **Everything Octoboard writes into the session is queued**, never refused: an owner's instruction, information
  shared by another session, a report, Octoboard's notices to the session as an owner, a report panel form submission,
  the retry of a turn stopped at its model's capacity. Each sender is told its message was queued, as for any session
  that cannot take one, and must not send it again. The queue keeps the order messages were sent in.
- **Neither the session's status nor its hand changes.** The hold is no wait for the user.
- **Owners see it** in the session tools' records as `writes_held` with the reason `usage_limit`, whose `explanation`
  says the last turn stopped on the account's usage limit, what lets the messages go, that they are then written in
  the order they were sent, and not to send them again. A hold listed before it in "What the owners' tools report" in
  `docs/product/message-delivery.md` is reported in its place.
- **An owner's `interrupt_session` still works**: the session is treated as at its prompt or in a turn like any other
  (see "When a key is sent" in `docs/product/interrupting-turns.md`).
- **A queued message does not end the automatic account switch's wait for quota** (see "When no account qualifies" in
  `docs/product/automatic-account-switch.md`), since it starts no turn.
- **Messages Octoboard had already put into Codex's own queue** before the turn failed (see "Messages through Codex's
  own queue" in `docs/product/message-delivery.md`) are Codex's to run: Octoboard does not take them back.

## The user's own messages

A message the user sends from a client (`send_message` in the daemon's client protocol) is not held by this hold: it is
written ahead of the messages the hold keeps back, which stay queued in their order behind it, and the client is
answered as for any message written into the session. The turn it starts decides the hold by how it ends (see "When the
hold ends" below).

- The session's other holds still apply to it: while the agent shows a screen only the user can answer it is queued,
  and while the session is starting or its trust confirmation may be up it is refused with `session_trust_pending`
  (see "Messages held until a session can take them" in `docs/product/message-delivery.md`).
- It is still refused while the session is waiting for the user.
- Like any new input, it ends the automatic account switch's wait for quota.

## When the hold ends

Once the hold ends, what it kept back is written, oldest first, under whatever other holds apply. It ends on the first
of these:

- **The account has recovered.** A successful quota reading of the account the session is on, begun after the failed
  turn ended, shows it by the rule in "When no account qualifies" in `docs/product/automatic-account-switch.md`: at
  least one account-wide window, and a finite used percentage below 100% in every account-wide window. A reading of
  another account, an older reading, a failed or unavailable one, or merely passing a reset time does not end it.
  Octoboard uses the readings it takes anyway (see "When quotas are read" in `docs/product/account-quota-readings.md`)
  and adds none for this. When the automatic account switch acts on the same reading, it acts first: a session it
  restarts takes what was queued into its relaunched process (see "What is queued crosses an account switch or a model
  change" in `docs/product/message-delivery.md`), so each message is written once.
- **The session has a new process.** The hold belongs to the process: an account switch, a change of model or effort, a
  restart for a change of its account's sharing, a handoff or a resume starts the session without it.
- **A turn the held messages did not start ends without failing.** Such a turn — the user's prompt typed in the
  terminal, the user's own message from a client, a turn the agent starts by itself, one from Codex's own queue
  included — is judged when it ends. Any end other than a failure ends the hold, a cancel the agent reports included; a
  failed end is judged as a failed turn is (see "When the hold starts" above), and holds again when the usage limit
  stopped it. A cancel Octoboard only infers from the terminal title (see "Cancelling a Claude Code turn" and
  "Cancelling a Grok Build turn before its first token" in `docs/product/turns-hooks-do-not-report.md`) does not end
  it; the turn's real end, when it comes, does.

**The hold is kept in memory only.** A daemon that takes the session over after a restart does not hold it, so what was
saved for the session is written as "What is queued crosses a planned restart" in `docs/product/message-delivery.md`
says.
