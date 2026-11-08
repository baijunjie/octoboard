# 07 Control-channel heartbeat

> Goal: a client notices a control connection that has silently died, and reconnects.
> Completion criteria: silently drop a connected client's traffic, with no close on either side. Within the heartbeat
> timeout the client treats the connection as lost and takes its existing lost-connection path, and the daemon drops
> the connection.

## Technical design

- [ ] A heartbeat on the control channel in both directions, with a timeout after which the connection counts as
      lost.
- [ ] A client whose heartbeat times out takes the existing path for losing the daemon connection, reconnects and
      reads state again.
- [ ] The daemon drops a connection whose heartbeat times out, releasing what that client held.

## Notes for the developer

- **Development notes**:
  - A protocol change follows the protocol's generation and the "What every client implements" rules.
  - Useful on loopback too, so it can be built and verified before the server form exists.
- **Reference docs**: `apps/daemon/PROTOCOL.md`, `docs/product/application-lifecycle.md` ("Losing the daemon
  connection").
