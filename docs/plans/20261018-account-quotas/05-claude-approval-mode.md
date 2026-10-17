# 05 Claude Code approval mode

> Goal: Claude Code accounts, and the default Claude Code account, have an approval mode choice — Follow account
> configuration / Automatic / Ask me / Approve everything — passed to Claude Code on every launch and resume.
> Completion criteria: daemon tests show the right `--permission-mode` argument (or none) for each choice on fresh
> launches and resumes; the Settings selector appears for Claude accounts; a real session launched with each mode
> behaves as that mode; the Open items about first-use confirmations are measured and recorded.

## Technical design

- [ ] The per-account approval setting and the default-account setting extended to Claude Code, with values
      follow / auto / manual / bypass; stored with a migration.
- [ ] The launch spec carries the Claude mode; the Claude adapter adds `--permission-mode <mode>` when one is set.

## Implementation plan

- [ ] Generalise the coordinator's per-session and per-account approval handling, which currently accepts only Codex.
- [ ] Show the approval selector for Claude Code accounts with Claude's four choices; Codex keeps its own three.
- [ ] Measure the first-use behaviour of auto and bypass on a fresh account and record it in the agent CLI reference;
      report a blocking confirmation to the user before going further.
- [ ] Strings, gallery scenarios through the gallery skill, protocol doc.

## Notes for the developer

- **Reusable capabilities**: the Codex approval reviewer path end to end (protocol, store, coordinator, launch spec,
  Settings selector); the shell snapshot that keeps `--permission-mode` from being ignored under inherited Claude
  session variables.
- **Development notes**: the value stays local and is never exported, like Codex's.
- **Reference docs**: `docs/agent-cli-reference.md`, `docs/product/settings.md` ("Agent accounts"),
  `docs/product/launching-agents.md`, `docs/product/configuration-export-and-import.md`.
