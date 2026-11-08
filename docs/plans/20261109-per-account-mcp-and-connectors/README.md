# Per-Account MCP Servers and Connectors Development Plan

## Problem and approach

A user with several accounts of one agent cannot tell which MCP tools each account actually has, and sharing the default
configuration does not carry them over as expected:

- **Platform-hosted connectors are bound to the platform account, server side.** A Codex plugin such as Jira, Slack or
  GitHub ships only a manifest and a reference to a ChatGPT App id; the OAuth connection is stored on the ChatGPT
  account and reached through the account's own sign-in. claude.ai connectors work the same way for Claude Code. Sharing
  `plugins/` and `config.toml` shares "installed", never "connected", so an account whose platform account never
  connected the app cannot use it, and sharing the sign-in would merge the accounts.
- **Codex's locally configured MCP servers share one OAuth token across accounts.** With the default credential store
  on macOS, Codex keeps the token in the keychain under a key computed from the server's name and URL alone, not from
  `CODEX_HOME`. Since sharing makes `config.toml` common to the accounts, every sharing account reads and writes the
  same entry: they share one identity at the server, and their token refreshes are not coordinated.

Octoboard does not reimplement OAuth, connector catalogues or plugin markets. Per account it shows what the agent itself
reports, links to the platform for what only the platform manages, runs the agent's own login commands for what the
agent manages locally, and keeps the local tokens of non-default Codex accounts apart.

## Key design decisions

- **Two classes of MCP tools per account.**
  - *Platform-hosted connectors* (ChatGPT Apps, claude.ai connectors): connected on the platform's website under the
    platform account; Octoboard shows them read-only with their state and offers a link to the platform's connector
    settings, telling the user which account to sign in with there. Connecting one in a browser is enough for the CLI:
    Codex reaches Apps through its built-in `codex_apps` MCP server with the home's ChatGPT sign-in, without the desktop
    app.
  - *Locally configured MCP servers* (`config.toml`, Claude Code's MCP configuration): the definition can be shared
    with the configuration, the credential stays the account's. Octoboard shows each with its sign-in state; for Codex
    it can sign in and out by running `codex mcp login` / `codex mcp logout` under the account's home.
- **Reading goes through the agents' own interfaces.**
  - Codex: the app server's `app/list` and `app/installed` give the Apps and whether each is accessible and callable
    (an App has no separate OAuth state; "connected" is told by those flags); `mcpServerStatus/list` gives every MCP
    server, plugins' and `codex_apps` included, with its auth status (`unknown`, `unsupported`, `notLoggedIn`,
    `bearerToken`, `oAuth`), runtime status and tools. `codex mcp list` covers only configured servers and no Apps, so
    it is not used. These methods are marked experimental by Codex.
  - Claude Code: `claude mcp list` under the account's configuration directory lists claude.ai connectors as
    `claude.ai <name>` and every configured server, each Connected, Needs authentication or Failed to connect. It has no
    JSON output, so its text is parsed, and it connects to every server to check its health.
- **The default Claude Code account is read with `CLAUDE_CONFIG_DIR` unset.** Naming `~/.claude` explicitly is not the
  default account: Claude Code keys its keychain credentials by the configuration directory, and such a run is signed
  out.
- **Only non-default Codex accounts get a file credential store.** Every Codex start Octoboard makes for a non-default
  account — sessions, and the MCP login and logout it runs — carries `-c mcp_oauth_credentials_store="file"`, so the
  account's MCP tokens land in its own `$CODEX_HOME/.credentials.json`. The default account keeps Codex's default store,
  matching `codex` run in a terminal. Settled by the user, with its cost: `codex` run by hand in a terminal under a
  non-default home does not carry the override and reads the default account's keychain entry.
- **The Codex reading starts Octoboard's own app server for the account**, as quota readings do, not the app-server
  daemon Codex keeps per home. That start leaves Apps and plugins on (the other app-server starts switch them off to
  stay quiet); the plugin catalogue sync and hooks stay off.
- **The list lives in each account's details dialog** (Settings → Agent accounts), next to the account's other
  per-account readings: read when the dialog opens, refreshable by hand. Settled by the user.
- **Codex's App catalogue is eventually consistent.** A first read after a change can miss it, and Codex caches the
  catalogue per home for an hour; a manual refresh asks Codex to refetch rather than trusting the cache.

## Milestones

1. 01 — Separate MCP credentials for non-default Codex accounts (closed)
2. [02 — Reading an account's MCP servers and connectors](02-reading-account-mcp-inventory.md)
3. 03 — MCP servers and connectors in the account details dialog (closed)
4. [04 — Signing a Codex account in and out of an MCP server](04-codex-mcp-sign-in.md)

## Open

- Grok Build: whether it has per-account MCP servers or connectors worth showing; until settled, its accounts show no
  such section.
- Sharing the `mcpServers` definitions of Claude Code's `.claude.json` between accounts, which today are not shared
  because that file also holds the account's identity. Not part of this plan.
