# 03 Terminal stream flow control

> Goal: a slow terminal client falls back to a fresh screen instead of being disconnected.
> Completion criteria: a client stops reading while a program floods output, and stays connected. When it resumes, it
> receives a fresh paint, not the backlog. Other clients of the same terminal are unaffected.

## Technical design

- [x] A credit window per attached terminal stream. The client acknowledges the bytes it has consumed, and the daemon
      sends no more than the credit allows.
- [x] A backlog cap per stream. Past it, the backlog is discarded and replaced with a fresh first paint without
      history.
- [x] A finishing terminal closes its stream only after the pending output is delivered.

## Implementation plan

- [x] Add the acknowledgement frame to the protocol and to the web client's terminal.
- [x] Replace the grace-window disconnect with the cap-and-repaint rule.

## Notes for the developer

- **Reusable capabilities**: the first paint (`TerminalSnapshot::paint`, taken under the fan lock in `Fan::attach`).
- **Development notes**: tune the starting window and the cap on WebKit and on a browser over a slow link. They are in
  the order of a few hundred KiB and a few MiB; the code starts with a 512 KiB window and a 4 MiB cap
  (`CREDIT_WINDOW`, `BACKLOG_CAP` in `apps/daemon/src/terminal/fan.rs`) and the web client acknowledges every 64 KiB
  (`ACK_BYTES` in `packages/ui/src/terminal/TerminalController.ts`), none of them tuned yet.
- **Not yet compiled or run**: the daemon side was written without compiling. The tests for the completion criteria are
  `a_client_that_falls_past_the_cap_gets_a_fresh_paint_and_the_others_every_byte` (fan) and
  `a_client_that_stops_reading_through_a_flood_stays_connected_and_resumes_on_a_fresh_paint` (server), beside the
  other new fan tests; the milestone closes once they pass.
- **Reference docs**: `apps/daemon/PROTOCOL.md` ("terminal stream").
