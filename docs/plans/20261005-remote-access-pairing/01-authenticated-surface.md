# 01 Authenticated surface

> Goal: the daemon has a separate external surface where nothing works without authentication, and the agent-facing
> endpoints are unreachable from outside the machine.
> Completion criteria: with the external surface on, an unauthenticated request or a socket from a foreign origin is
> refused on every route; the hook and MCP endpoints do not answer on the external surface; with it off, nothing listens
> beyond loopback.

## Technical design

- [ ] The router and listener are split: internal (loopback; hook and MCP) and external (everything a client uses).
- [ ] Authentication on every external route, including static files and WebSocket upgrades.
- [ ] Origin check on WebSocket upgrades.
- [ ] The setting that turns the external surface on, off by default.

## Implementation plan

- [ ] Split the surfaces first with no behavior change, then add the authentication layer to the external one.

## Notes for the developer

- **Reusable capabilities**: the MCP endpoint's per-launch token, which refuses an unknown token without detail.
- **Development notes**: default-deny; refusals carry no detail.
- **Reference docs**: `apps/daemon/PROTOCOL.md`, `docs/architecture.md`.
