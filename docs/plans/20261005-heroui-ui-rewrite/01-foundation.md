# 01 Foundation

> Goal: a HeroUI-based UI package that starts in a plain browser, connects to a running daemon and holds its state, with
> native capabilities behind the adapter.
> Completion criteria: the package type-checks and builds; opened in a browser against a daemon started by hand it
> connects, receives the snapshot and reflects live state changes; the adapter has both implementations.

## Technical design

- [ ] The package scaffold with HeroUI set up, with a build that a browser can load and a shell can embed.
- [ ] The platform adapter's interface and its two implementations.
- [ ] The daemon connection and request layer, with the reconnect and queued-request behavior the current one has.
- [ ] The application state fed by the daemon's snapshot and events.
- [ ] Locating the daemon: handed-in address, or same-origin.

## Implementation plan

- [ ] Inventory every direct use of a Tauri API in the current UI; each becomes an adapter member.
- [ ] Carry over the connection layer and the state, keeping their observable behavior.

## Notes for the developer

- **Reusable capabilities**: the current daemon client and the protocol types; the startup screen's handling of a daemon
  that is missing or failed.
- **Development notes**: the exit flow is delicate (the daemon must be shut down in order), so its adapter is a move, not
  a rewrite.
- **Reference docs**: `daemon/PROTOCOL.md`, `docs/product/application-lifecycle.md`, `docs/memory/verifying-the-desktop-ui.md`.
