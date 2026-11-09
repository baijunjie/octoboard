# 01 Size rule

> Goal: several clients operate at once, in sync, with the terminal sized predictably.
> Completion criteria: with two clients attached to one session, input from either reaches the agent, status and records
> stay in sync on both, and the size follows the most recently active client without leaving the other garbled once it
> becomes active again. Requires a second kind of client to test with.

## Technical design

- [x] Per-connection activity tracking for terminal connections.
- [x] The size rule applied in the daemon.
- [x] Size applied after the attach replay.

## Implementation plan

- [ ] Verify with two real clients against an agent that redraws on resize.

## Mid-task handoff

- **State**: the size rule, with explicit activation, is implemented and committed but has **not been compiled or
  run**: the machine was short on memory, so every Rust build and test was deferred to one run at the end. Only
  `packages/ui/src/terminal/TerminalController.test.ts` has been run, and passes. The design checkboxes are ticked for
  the implementation; the completion criteria and the real-client verification remain.
- **What was built**: `terminal/activity.rs` ranks clients: an attach takes precedence until a deadline 30 seconds on
  (`ATTACH_CLAIM`), input and an `activate` frame never lapse. The fan keeps each client's latest requested size
  (`Fan::request_size`) and, on activation, a size request, leaving and `Fan::settle_activity`, brings the size and
  the display up to date together under its lock (`State::reconcile`). The attach returns its deadline
  (`Answering::precedence_lapses`); the terminal socket's `Registration` settles at it in a task of its own and lets
  the client go on every path after the attach. The web client sends `activate` when a session attaches by the user's
  doing in a window they are in, when they press on the terminal or come back to its window while it is shown, and
  right after a resize in the window they work in, so the activation applies the new size; it forgets the links, the
  hover and the program's copy at a fresh paint mid-stream.
- **Not done, by decision**: the review's suggestion to settle a lapsed display before parsing a chunk on the reader
  path. The reader has no way to resize the PTY, and settling only the display there would let the size and the
  display follow different clients; the settle task runs at the deadline itself, so queries parsed between the
  deadline and the task's run are answered with the previous display.
- **To run** (all uncompiled so far): `cargo test -p octoboardd terminal`, which takes the new and changed tests
  (`terminal::activity`, `terminal::fan`,
  `server::terminal::tests::the_terminal_takes_the_size_of_the_client_that_typed_last` and
  `server::terminal::tests::a_registration_settles_at_its_deadline_and_lets_its_client_go`) and the existing
  `terminal::shell` and `server::terminal` tests, `cargo test -p octoboardd protocol::bindings` for the hand-written
  `activate` member of `TermControl` in `protocol.generated.ts`, then the project's push checks (`pnpm verify`),
  clippy included.
- **Still to verify**: the completion criteria with two real clients (the macOS application and a browser) against an
  agent that redraws on resize: input from either reaches the agent, state stays in sync, the size follows the client
  used last, switching back to the other (clicking it, or coming back to its window) restores its size and colours
  with nothing typed, a resize in a background window does not take the size, and a client that attaches and stays
  quiet gives the size back after 30 seconds.

## Notes for the developer

- **Reusable capabilities**: control-channel broadcasts and the full snapshot on connect (state sync); the terminal stream
  already supports several attached clients with per-client backpressure.
- **Development notes**: the activity tracking is `terminal/activity.rs`, kept in the fan (`Fan::attach`,
  `Fan::activate`).
- **Reference docs**: `apps/daemon/PROTOCOL.md` (terminal stream), `docs/product/sessions.md`, `docs/product/session-terminal.md`.
