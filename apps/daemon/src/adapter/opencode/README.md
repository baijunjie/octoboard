# OpenCode adapter

`OpenCodeProvider` implements the shared agent interface for both OpenCode generations. The descriptor's supported
version list gates launch. `version.rs` chooses the generation and checks whether OpenCode must migrate legacy
credentials before launch. Keep generation-specific configuration and event handling in this adapter.

- `OpenCodeProvider` in `mod.rs` assembles the launch plan and wires the record store and account operations.
  `plugin.js` and `plugin-v2.js` forward native events. `tui.mjs` and `tui-v2.js` handle events that originate
  in the attached terminal.
- `OpenCodeServer` and its `MessageTransport` in `server.rs` start the private server, attach the terminal and
  deliver prompts through the selected generation's API. `adopt` reconnects to a server kept by a holder. The first session creation
  or v2 lookup has a 45-second bootstrap budget, extended to four minutes on Windows for first-run setup such as a
  ripgrep download. Steady-state requests keep the 10-second budget.
- `OpenCodeRecords` in `records.rs` implements the shared `RowRecords` interface. The v1 reader uses native CLI
  queries. The v2 reader uses projected session records and waits for legacy history import when required.
  Keep the feed's record sequence stable while the assistant's message is still streaming.
- `accounts.rs` wires the default account's sign-in and inventory readings. `mcp_inventory.rs` reads the merged
  MCP configuration through the native CLI.
