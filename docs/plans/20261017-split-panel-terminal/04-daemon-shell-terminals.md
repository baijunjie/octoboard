# Shell terminals in the daemon

> Goal: the daemon can start, list, stream and end plain shell terminals that belong to a project, independent of any
> agent session.
> Completion criteria: a client can create a terminal for a project, attach to its output and input over the terminal
> socket, resize it, find the project's terminals with their scrollback intact after reconnecting, and close it; every
> client sees terminals appear and end, including a shell exiting on its own; removing the project or stopping the
> daemon ends its terminals; agent sessions behave as before. Verified by daemon tests for the lifecycle and the
> protocol, and by attaching a test client.

## Technical design

- [ ] A PTY core shared by agent sessions and shell terminals: output fan-out, the ring buffer, the reader thread,
      resize, terminate and crash cleanup. Agent-specific parts (agent, role, MCP token, trust watching, title
      observing) stay with the agent session.
- [ ] A shell terminal record held as live in-memory state: terminal id, project id, working directory, title, and its
      PTY core; what it keeps after its shell exits follows the overview's Open item on exited shells.
- [ ] The shell is the user's login shell, started with the shell environment snapshot the daemon already builds; the
      per-session variables Octoboard sets for an agent are not set.
- [ ] Shell terminals reach clients the way other records do: carried in the snapshot and kept current by upserted and
      deleted broadcasts. Control requests create and close a project's terminals.
- [ ] The terminal socket accepts a shell terminal's id as it does a session's, with the same binary frames and resize
      frame.

## Implementation plan

- [ ] Extract the PTY core from the agent session, with agent sessions moved onto it and their tests passing unchanged.
- [ ] Add the shell terminal record and its registry on the daemon state.
- [ ] Add the snapshot field, the broadcasts and the control requests, changing the protocol doc and both protocol
      definitions together.
- [ ] Let the terminal socket resolve either kind.
- [ ] End a project's terminals when the project is removed, and all of them on daemon exit.

## Notes for the developer

- **Reusable capabilities**: the PTY launch, fan-out, ring buffer and non-blocking IO with per-client backpressure;
  the shell environment snapshot; the crash cleanup table; the control socket's snapshot, broadcast and request
  plumbing.
- **Development notes**:
  - Agents are deliberately not spawned inside a shell; a shell terminal is the opposite case and spawns the shell
    itself.
  - A lagging client gets a fresh snapshot in place of the events it missed, so terminal state must be recoverable
    from the snapshot alone.
  - The protocol doc and the Rust and TypeScript protocol definitions are kept in sync by hand.
  - The server form constraint in the overview applies here.
  - The "Open terminal" action that opens Terminal.app is a separate feature and stays.
- **Reference docs**: `apps/daemon/PROTOCOL.md`, `docs/architecture.md`, `docs/memory/writing-daemon-code.md`,
  `docs/product/project-terminal.md`, `docs/product/application-lifecycle.md`.
