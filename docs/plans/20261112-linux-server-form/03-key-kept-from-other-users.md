# 03 Keeping the key from other users

> Goal: no other Unix user of the machine can obtain a key that lets them reach the daemon. Completion criteria: with
> sessions of every agent running, including their hooks, MCP server, status line and git hooks, no key the daemon
> accepts appears in any process's arguments, nor in a process environment or a file another user can read; what the key
> reached before still works; checked as a second, unprivileged user listing every process's arguments and environment,
> on Linux, macOS and Windows.

## Technical design

- [ ] The `octoboardd` helpers (`hook`, `mcp`, `status-line`, `git-ref`, `gate-hook`) take the key from a place only
      the daemon's user can read — their environment, or an owner-only file — instead of a `--key` argument.
- [ ] Every agent configuration the daemon writes or passes that carries a key (hook commands and URLs, the MCP
      server registration) carries it only where other users cannot read it.
- [ ] The scoped keys a holder issues for its sessions follow the same rule.

## Notes for the developer

- **Reusable capabilities**: the environment the daemon already gives a project session (`OCTOBOARD_DAEMON_KEY`
  beside `OCTOBOARD_DAEMON_PORT`), the owner-only files of the data directory, the access guard's key check.
- **Development notes**: an agent may pass its hook or MCP command through a shell or its own launcher, so check what
  each agent CLI actually hands the helper before relying on its environment. The protocol document's "Who may
  connect" states where the key lives today and is updated with the change.
- **Reference docs**: `apps/daemon/PROTOCOL.md` ("Who may connect", "The key"), `docs/architecture.md` ("Known gap:
  other users of the same machine share its loopback").
