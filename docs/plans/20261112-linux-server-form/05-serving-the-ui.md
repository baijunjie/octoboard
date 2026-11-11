# 05 Serving the UI

> Goal: opening the daemon's address in a browser shows the full UI, served by the daemon itself.
> Completion criteria: on a Linux server without a display, the owner opens the address from milestone 02 through an
> SSH port forward on their own machine and runs a console session and project sessions entirely in the browser, the
> raised hand's notification reaching them; the daemon binary contains the UI.

## Technical design

- [ ] The UI build is embedded into the daemon at build time and served same-origin with the protocol endpoints.
- [ ] Verified with the embedded build: the UI finds the daemon at its own origin with the key from its address, and
      what only a desktop shell provides is absent.

## Implementation plan

- [ ] Make `apps/daemon` a pnpm workspace package wrapping its cargo commands, depending on the UI package's build.
- [ ] Add the embedding to the daemon's build, ordered after the UI build through that dependency.

## Notes for the developer

- **Reusable capabilities**: the shared UI package and its browser host facts, which already describe a browser with
  none of a shell's features; the UI's existing lookup of the daemon at its own origin and of a key passed in its
  address.
- **Development notes**: a browser grants the Notifications and Clipboard APIs only to a secure context, which over
  plain `http` is a loopback address alone; the forward gives the page one.
- **Reference docs**: `apps/daemon/README.md`, `apps/daemon/PROTOCOL.md`.
