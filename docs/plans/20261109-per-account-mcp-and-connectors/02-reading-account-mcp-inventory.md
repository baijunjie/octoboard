# 02 — Reading an account's MCP servers and connectors

> Goal: the daemon can read, for one Claude Code or Codex account, its platform-hosted connectors and its locally
> configured MCP servers with their states, and hand them to clients.
> Completion criteria: a protocol request returns the inventory of a given account; parsing and mapping are covered by
> unit tests on recorded Codex app-server replies and recorded `claude mcp list` output (connected, needs
> authentication, failed, none configured, signed out); a manual read against a real Codex and Claude Code account
> returns what the agents report.

## Technical design

- [x] An account inventory: the account, when it was read, and two lists — platform connectors and local MCP servers —
      or a reason the reading failed (agent not signed in, CLI missing, timed out, unsupported agent).
- [x] A platform connector entry: id, display name, state (connected / not connected / unknown), and for Codex the
      plugins that bring it.
- [x] A local MCP server entry: name, transport origin (the URL of a remote server, none for a local command), whether
      a plugin brings it, sign-in state (signed in / needs sign-in / no sign-in needed / unknown), runtime state
      (running / failed with its message), and tool count where the agent reports it.
- [x] The agent's own state values map onto these states in one place per agent; a value Octoboard does not know maps
      to unknown rather than failing the reading.
- [x] The request takes a refresh flag: off reads what the agent has cached, on asks the agent to refetch.

## Implementation plan

- [x] Codex reading: open an app-server conversation for the account's home, call `app/list` and `app/installed` for
      the Apps and `mcpServerStatus/list` for the MCP servers, and build the inventory; the refresh flag passes Codex's
      refetch options. Apps come from the `codex_apps` server, which is not listed as a local server of its own.
- [x] Claude Code reading: run `claude mcp list` under the account's configuration directory (unset for the default
      account), split `claude.ai <name>` lines into platform connectors and the rest into local servers, and map the
      three markers to states.
- [x] The protocol request and its reply type, with errors carried as the inventory's failure reason; one reading per
      account at a time, a second request joining the running one.

## Notes for the developer

- **Reusable capabilities**: the shared Codex app-server conversation (start for an account outside a session,
  `initialize` handshake) used by quota readings, reset credits and sign-in; the account directory resolution used by
  launches and quota readings; the daemon's error code scheme.
- **Development notes**: the quiet overrides Octoboard applies when starting an app server switch Apps off, so this
  reading must not inherit that one (see the Open item in the overview). The Codex methods are experimental: guard
  against missing fields and an unknown method. `claude mcp list` connects to every server and may take seconds: run it
  off the request path with a timeout. A reading must not write the account's configuration.
- **Handoff**: `mcpServerStatus/list` reports each server's auth status, which Codex reads from its MCP credential
  store, so the app server this reading starts for a non-default Codex account carries
  `-c mcp_oauth_credentials_store="file"` (`codex::MCP_FILE_CREDENTIALS_STORE` in `apps/daemon/src/adapter/codex/mod.rs`),
  and none for the default account, as session launches do; otherwise it would report the default account's keychain
  sign-ins. Cover it with a launch-argument test for both kinds of account.
- **Reference docs**: `docs/product/account-quota-readings.md` (how Octoboard reads through an agent's CLI),
  `docs/agent-cli-reference.md`, `docs/memory/probing-agent-clis.md`, `docs/memory/adding-a-daemon-error-code.md`,
  `apps/daemon/PROTOCOL.md`.
- **Verification state**: everything but the manual read is done and reviewed. The manual read against a real Codex and
  Claude Code account is still open (unattended runs could not read without touching the user's real homes); settle it
  before closing the milestone.
