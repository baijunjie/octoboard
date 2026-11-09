# 04 Terminal queries answered by the daemon

> Goal: a program's terminal queries are answered once, by the daemon, whether any client is attached or not.
> Completion criteria:
> - With no client attached, a program asking for the cursor position, its colours or the device attributes gets an
>   answer.
> - With several clients attached, each of those queries is answered exactly once, and the answered queries do not
>   reach the clients.
> - The colours a program reads match the theme of the most recently active client.
> - Clients answer only the queries the daemon passes through.

## Technical design

- [x] Answers come from the daemon's terminal emulator, written to the program as input.
- [x] Answered queries are removed from the stream clients receive. A query the emulator cannot answer passes through
      for a client to answer.
- [x] A display-facts frame from the client: foreground, background and cursor colours, and cell size, sent on attach
      and when they change.
- [x] The facts of the most recently active client set the emulator's display. A change of facts notifies programs
      that subscribed to colour-scheme changes.
- [x] Defaults for the display facts while no client has reported any.

## Implementation plan

- [x] Settle which queries the emulator answers. The daemon's current parser may not answer all of them; decide when
      starting whether to extend it or replace it.
- [x] Add display facts to the protocol and the web client, then switch answering to the daemon.
- [x] Reduce the client reply path and its rules to the queries passed through (see the overview's "Open").

## Mid-task handoff

- **Where it stands**: everything above is implemented and committed on `mc/daemon-answers-queries`, together with the
  tests for the completion criteria, but the Rust code has never been compiled: by the user's decision every compile
  and test run waits until all milestones are done. None of the completion criteria is verified yet. The UI tests of
  the terminal (`packages/ui/src/terminal/TerminalController.test.ts`, `TerminalUserInput.test.ts`) pass.
- **To continue**: compile the daemon and fix what the compiler finds, run the tests below, then close the milestone.
  - The completion criteria, end to end: `server::terminal::tests::the_daemon_answers_the_programs_queries_in_the_most_recently_active_clients_colours`
    (no client attached, two clients, the most recently active client's colours, no query reaching a client) and
    `server::tests` (the shell terminal test: no `CSI 6n` reaches a client, the reply routing of a passed-through
    answer).
  - Units: `terminal::queries`, `terminal::activity`, `terminal::display`, `terminal::snapshot::tests`
    (the tests of `take_in`, which is in `snapshot/answers.rs`), `terminal::snapshot::paint::tests`, `terminal::fan::tests`, `terminal::query_log`,
    `terminal::reply_route`, `terminal::pty::tests`, and the bindings test (`protocol::bindings`, which compares the
    hand-edited `TermControl` in `packages/ui/src/protocol.generated.ts` with the rendering).
  - `session::trust::tests`, whose `read_capture` now strips the colour queries everywhere and the device attributes
    and cursor position wherever the host does not answer those, since the daemon answers them into a stand-in that
    reads keys.
  - On Windows, the end-to-end test assumes the bundled host echoes an OSC 11 answer byte for byte and leaves the
    cursor at column 6 after `READY`.
- **Decisions made in this attempt**:
  - The parser is extended, not replaced: `terminal/queries.rs` cuts the output into plain output and whole query
    sequences (holding back one cut short), and `TerminalSnapshot::take_in` answers each from `alacritty_terminal`'s
    own answers where it has them (device attributes, status and cursor reports, mode reports, the text area, the
    dynamic colours through the display) and writes the rest itself (cell size, the colour scheme and mode 2031).
  - Mode reports are answered only for the modes the clients keep as the daemon does (not synchronised output, which
    only a client holds frames for); every other mode, and every query not listed in `PROTOCOL.md`, passes through.
  - Offsets on the terminal stream (the replay mark, a reply's offset) now count the bytes handed to clients, so they
    stay consistent once answered queries are cut out. `ReplyRoute::DAEMON` is gone: a client cannot see, and so
    cannot answer, what the daemon answered.
  - The passed-through queries keep the existing reply rules (first client's answer to a point, the offset rules, the
    5-second window). This settles the overview's open question on them.
  - Activity, shared with milestone 01, is attaching, sending input and an `activate` frame (`terminal/activity.rs`);
    a display frame, a reply, and input that is only the terminal's own traffic (focus and pointer reports,
    `is_terminal_protocol`) are not activity. An attach takes precedence for 30 seconds only (`ATTACH_CLAIM`), and
    the colours follow the same ranking as the size, brought up to date together with it.
  - Defaults: the web client's dark theme and a cell of 8 by 17 pixels.
  - Known limit, left as is by the user's decision: when a passed-through query is followed by one the daemon answers
    (a query with the device attributes as its end marker, as notcurses and neovim write them), the daemon's answer
    reaches the program before the client's. None of the agents run today does this.

## Notes for the developer

- **Development notes**:
  - Focus reports remain each client's own input.
  - **`TermFacts::host_answers_queries`** (`apps/daemon/src/platform/term/mod.rs`): where it is true (the system's
    console host on Windows) the host answers the device attributes, status and cursor reports itself, so the daemon
    leaves those to the clients. `TermFacts::host_waits_on_queries` is no longer read outside tests: the daemon answers
    promptly everywhere.
- **Reference docs**: `apps/daemon/PROTOCOL.md` ("Answers to the program's queries" and "Display" under the terminal
  stream).
