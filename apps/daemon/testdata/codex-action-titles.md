# Codex action title captures

Captured on 2026-10-17 from an existing local Codex session's retained PTY output through a read-only
terminal-stream connection. No input, resize or model turn was sent. Only complete OSC title sequences
are retained; command output and conversation text are excluded.

The installed CLI at capture time was Codex 0.162.1. The running session's executable version was not
independently queried. These title forms also match the observations for Codex 0.163.0 recorded in
`docs/agent-cli-reference.md`, "Across the three agents".

- `codex_approval_title.bin`: the actual `Action Required` title, terminated by BEL.
- `codex_working_title.bin`: the ordinary spinning title from the same session, the near miss that
  must not raise a hand and the transition that releases one.

The `[ . ]` blink phase exercised separately in tests is based on Codex's title format, rather than
another captured frame. Tests for task names containing the marker are constructed negative cases.
