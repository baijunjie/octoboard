# 03 Terminal stream flow control

> Goal: a slow terminal client falls back to a fresh screen instead of being disconnected.
> Completion criteria: a client stops reading while a program floods output, and stays connected. When it resumes, it
> receives a fresh paint, not the backlog. Other clients of the same terminal are unaffected.

## Technical design

- [ ] A credit window per attached terminal stream. The client acknowledges the bytes it has consumed, and the daemon
      sends no more than the credit allows.
- [ ] A backlog cap per stream. Past it, the backlog is discarded and replaced with a fresh first paint without
      history.
- [ ] A finishing terminal closes its stream only after the pending output is delivered.

## Implementation plan

- [ ] Add the acknowledgement frame to the protocol and to the web client's terminal.
- [ ] Replace the grace-window disconnect with the cap-and-repaint rule.

## Notes for the developer

- **Reusable capabilities**: the first paint from milestone 02, and the existing per-client backpressure on the
  terminal stream.
- **Development notes**: tune the starting window and the cap on WebKit and on a browser over a slow link. They are in
  the order of a few hundred KiB and a few MiB.
- **Reference docs**: `apps/daemon/PROTOCOL.md` ("terminal stream").
