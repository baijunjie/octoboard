# Grok Build slash-command guard captures

Captured on 2026-11-20 from Grok Build 1.0.89 (`--no-leader`) running interactively in a 120×40 PTY against a local
stand-in for its inference API, from a scratch home with a fake API key. Each file is the raw output from the launch
on, replayed into the daemon's own screen by the tests.

- `grok_slash_zwsp_guarded.bin`: `/session-info` pasted after a zero-width space (U+200B). The composer shows
  `/session-info` and no command is offered. Sent with Enter, the text reached the model as an ordinary prompt (the
  `UserPromptSubmit` hook reported it with the zero-width space in front); the same held for a custom command with
  arguments.
- `grok_slash_space_guarded.bin`: the composer cleared with Ctrl-U, then `/session-info` pasted after a single space.
  Grok offers the `/session-info` command above the composer; sent with Enter, it ran the command and nothing reached
  the model. A line break or a no-break space before the slash ran the command too.
