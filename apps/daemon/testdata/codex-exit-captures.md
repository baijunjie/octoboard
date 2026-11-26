# Codex captures of an exit at start

Measured on Codex 0.167.1 (`codex-cli 0.167.1`, standalone release, macOS arm64) on 2026-11-26, each from the start
of a launch on a 120x40 pseudoterminal with a scratch `CODEX_HOME` and working directory, the terminal's queries
answered and no key typed. Nothing reached a model: no prompt was sent. Each `.bin` is the raw output, to be made plain
text as the early exit makes it (`crate::terminal::plain_text::strip_escapes`). What each one shows is set out in
`docs/agent-cli-reference.md`, "Codex", "What Codex does with an option or a setting it does not take".

- `codex_exit_unknown_option.bin`: `codex --definitely-not-an-option`. Argument parsing refuses it before anything is
  drawn: `error: unexpected argument '--definitely-not-an-option' found`, a tip, the usage, exit status 2. A release
  without an option Octoboard passes (`--dangerously-bypass-hook-trust`, `--remote`) would refuse it the same way.
- `codex_exit_unknown_variant.bin`: `codex -c approvals_reviewer="bogus"`, a value of a setting the release does not
  know. Refused while the configuration loads, before anything is drawn: `Error: unknown variant `bogus`, expected one
  of …` and `in `approvals_reviewer``, exit status 1.
- `codex_exit_unknown_variant_mcp.bin`: the same for `mcp_servers.octoboard.default_tools_approval_mode="bogus"`
  (with `mcp_servers.octoboard.command="true"`), a key under a table: the line names the whole key path.
- `codex_exit_unknown_profile.bin`: `codex -c default_permissions=":bogus"`. Refused only after the terminal took
  bracketed paste and the alternate screen: `Error loading configuration: default_permissions refers to unknown
  built-in profile `:bogus``, exit status 1, still before anything is drawn: an early exit, whose refusal names
  `default_permissions`, so the reduced launch leaves out the permission preset and keeps the hooks.
- `codex_exit_home_missing.bin` (the near-miss): `CODEX_HOME` naming a directory that does not exist (rewritten to
  `/scratch/missing-home`). A fast exit at start, exit status 1, that names no option or setting.

Keys a release does not know at all are passed over without a word, the launch going on to draw its screen: an unknown
top-level key, a hook event (`hooks.BogusEvent`), a field of a hook entry or of an MCP server, a `tui.`, `notice.` or
`features.` key, and an unknown terminal title item.
