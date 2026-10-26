# Grok Build composer captures

Captured on 2026-10-26 from Grok Build 1.0.70 running interactively in a 120×40 PTY against a local stand-in for its
inference API. Each file is the visible screen as a terminal emulator rendered it at that moment, trailing spaces
removed.

- `grok_composer_restored.txt`: Ctrl-C pressed before the first token arrived: Grok put the prompt back in its
  composer.
- `grok_composer_restored_lines.txt`: the same for a prompt of three lines.
- `grok_composer_restored_wrapped.txt`: the same for one long line, wrapped over two rows; one Ctrl-U cleared it.
- `grok_composer_one_ctrl_u.txt`: the three-line prompt after one Ctrl-U, which cleared its last line and left the
  line empty; the next Ctrl-U removed the empty line, and four more emptied the composer.
- `grok_composer_cleared.txt`: the composer emptied by Ctrl-U ("Input cleared · ctrl+z to undo").
- `grok_composer_working.txt`: a turn running, with the prompt shown above an empty composer.
- `grok_composer_draft.txt`: the near miss — text typed into the composer while a turn ran.

Raw output, replayed into the daemon's own screen by the tests, from a session in a 120×40 PTY against the same
stand-in:

- `grok_composer_draft.bin`: "draft text" typed into the composer while a turn waited for its first token.
- `grok_composer_restored.bin`: the same session after Ctrl-U and Ctrl-C: the prompt is back in the composer. Neither
  writes the composer's text faint.
