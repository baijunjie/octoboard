# Claude Code activity title captures

Captured on 2026-10-25 from Claude Code 2.1.310 running interactively in a PTY against a local stand-in for the
Messages API, with HTTP hooks posting to the same stand-in. Only complete OSC title sequences are retained, in the
order they were sent; screen output is excluded.

- `claude_title_idle.bin`: the title at launch, back at the prompt (`✳`).
- `claude_title_cancel.bin`: a turn whose request was never answered, from its first running frame (`◐`, `◑`
  alternating) to the `✳` sent 70 ms after Esc was pressed. No hook fired and no transcript entry was written for the
  cancel.
- `claude_title_permission.bin`: the near miss — a turn's running frame and the `✳` sent 43 ms after its
  `PermissionRequest` hook was posted, while the permission dialog waited on the user.
