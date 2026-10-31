# 01 Size rule

> Goal: several clients operate at once, in sync, with the terminal sized predictably.
> Completion criteria: with two clients attached to one session, input from either reaches the agent, status and records
> stay in sync on both, and the size follows the most recently active client without leaving the other garbled once it
> becomes active again. Requires a second kind of client to test with.

## Technical design

- [ ] Per-connection activity tracking for terminal connections.
- [ ] The size rule applied in the daemon.
- [ ] Size applied after the attach replay.

## Implementation plan

- [ ] Verify with two real clients against an agent that redraws on resize.

## Notes for the developer

- **Reusable capabilities**: control-channel broadcasts and the full snapshot on connect (state sync); the terminal stream
  already supports several attached clients with per-client backpressure.
- **Development notes**: the per-connection activity tracking is shared with milestone 04. Whichever of the two lands
  first builds it, and the other reuses it.
- **Reference docs**: `apps/daemon/PROTOCOL.md` (terminal stream), `docs/product/sessions.md`, `docs/product/session-terminal.md`.
