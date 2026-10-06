# Daemon Messages as Codes

> Goal: every error and notice the daemon sends for the user to read carries a stable code and its parameters, and
> the UI renders it from the catalog in the current language.
> Completion criteria: the protocol document lists every code with its parameters; every user-facing error and notice
> the daemon can send uses one; the UI shows each in the current language (English until translations land); the
> daemon's protocol tests and the UI's typecheck pass.

### Technical design

- [ ] The protocol's `error` and `session_notice` events and request replies that report a failure carry a code and
  named parameters for anything user-facing, documented in the protocol document and mirrored in the UI's protocol
  types.
- [ ] Each existing user-facing message in the daemon is given a code and its parameters (for instance "this console
  already has a hub session" with the hub as a parameter).
- [ ] The UI renders a code through the catalog, filling in the parameters (a session or console parameter rendered as
  its name, never an internal identifier), and handles an unknown code as decided in the overview's "Open".

## Notes for the developer

- **Reusable capabilities**: the existing error and notice paths in the daemon and the UI's toast stream; the
  protocol's own contract tests.
- **Development notes**: the protocol is a contract shared by every client; change the protocol document, the daemon
  and the UI's mirror of it together.
- **Reference docs**: `apps/daemon/PROTOCOL.md`, `docs/architecture.md`, `packages/ui/README.md`.
