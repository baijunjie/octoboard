# 04 — Signing a Codex account in and out of an MCP server

> Goal: from a Codex account's details dialog, the user signs that account in to or out of a locally configured MCP
> server that uses OAuth, without a terminal, and the token lands in that account's own store.
> Completion criteria: Sign in runs `codex mcp login <name>` under the account's home and the browser OAuth completes
> there; Sign out runs `codex mcp logout <name>`; the inventory rereads afterwards and shows the new state; for a
> non-default account the run carries `-c mcp_oauth_credentials_store="file"`, verified in an isolated scratch home.

## Technical design

- [x] Sign in and Sign out actions on a local MCP server entry of a Codex account whose sign-in state allows it; none on
      platform connectors, plugin servers without OAuth or Claude Code entries.
- [x] A protocol request per action naming the account and the server; it ends with success, cancellation by the user,
      or a failure carrying the CLI's message.
- [x] One sign-in per account and server at a time; while it runs the entry shows it is waiting for the browser.

## Implementation plan

- [x] Run the CLI command for the account's home with the launch environment Octoboard uses for that account, plus
      `-c mcp_oauth_credentials_store="file"` for a non-default account (and none for the default account); let Codex
      open the browser itself.
- [x] Reread the account's inventory when the command ends, and surface a failure through the UI's error alert.
- [x] UI controls, copy and gallery scenarios for the waiting, success and failure states.

## Notes for the developer

- **Reusable capabilities**: the account sign-in flow Octoboard already runs for Codex (starting a CLI under an
  account's home and following it to completion); milestone 02's reading for the reread.
- **Development notes**: never run these commands against the user's real homes in tests; build a scratch home as the
  probe memory describes. The default account's sign-in writes Codex's default store, as `codex mcp login` in a terminal
  would.
- **Handoff**: the override is `codex::MCP_FILE_CREDENTIALS_STORE` in `apps/daemon/src/adapter/codex/mod.rs`, which
  session launches already pass for a non-default account (decided by the account's directory being pinned, whatever
  its sharing state); the login and logout runs take the same constant and the same rule.
  The dialog's section is `packages/ui/src/settings/accounts/AccountMcpSection.tsx`: a server entry is `ServerEntry`,
  whose state badges are where the actions go, and `useAccountMcpInventory`'s `refresh` rereads the inventory (one
  reading at a time; it keeps the shown reading until the answer). Its gallery scenarios are under
  `packages/ui/src/gallery/fixtures/` with the account details scenarios, and the fixture daemon stages the inventory
  per scenario (`Fixture.accountMcpInventory`).
- **Reference docs**: `docs/product/logging-accounts-in.md`, `docs/memory/isolating-agent-cli-probes.md`.
- **Verification state**: the daemon's runs are covered by fake-CLI tests (argv and `CODEX_HOME` for both kinds of
  account, success, failure with the CLI's stderr, cancellation, timeout, joining, the busy refusal, account removal,
  shutdown). Codex 0.166.0 itself was run in a scratch home against a local fake OAuth server: with the file store
  override, `codex mcp login` wrote the token to the scratch `$CODEX_HOME/.credentials.json` and `codex mcp logout`
  removed it; that run used `--no-browser` to keep the browser closed, so the path where Codex opens the browser
  itself, and a sign-in through the daemon against a real provider, are still unverified (unattended runs cannot
  complete a browser OAuth). Settle that before closing the milestone.
