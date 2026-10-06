# Multi-client Terminal Development Plan

## Problem and approach

Several clients — the macOS application, a browser, later a phone — can operate at once with synchronised state. State
changes already broadcast to every client and terminal input already lands in the one PTY. What is missing is a rule for
the shared PTY's size: an agent lays its screen out for one width, so when clients of different widths are attached at the
same time one of them has to win.

An agent redraws itself when the PTY is resized, which is what makes a size change work at all; the rule is only about who
is allowed to ask. A client that does not attach a terminal — a phone showing a conversation view, say — never affects the
size.

## Key design decisions

- **The most recently active terminal client decides the size**, decided in the daemon, not by each client.
- **A client that attaches and then goes quiet does not keep claiming the size**; becoming active again re-applies its size.
- **Replay does not leave a stale layout**: the client's size is applied right after the replay it receives on attaching.

## Milestone

> Goal: several clients operate at once, in sync, with the terminal sized predictably.
> Completion criteria: with two clients attached to one session, input from either reaches the agent, status and records
> stay in sync on both, and the size follows the most recently active client without leaving the other garbled once it
> becomes active again. Requires a second kind of client to test with.

### Technical design

- [ ] Per-connection activity tracking for terminal connections.
- [ ] The size rule applied in the daemon.
- [ ] Size applied after the attach replay.

### Implementation plan

- [ ] Verify with two real clients against an agent that redraws on resize.

### Notes for the developer

- **Reusable capabilities**: control-channel broadcasts and the full snapshot on connect (state sync); the terminal stream
  already supports several attached clients with per-client backpressure.
- **Reference docs**: `apps/daemon/PROTOCOL.md` (terminal stream), `docs/product/sessions.md`.

## Open

- What counts as "active" (input only, or output viewing as well), and the idle cutoff.
