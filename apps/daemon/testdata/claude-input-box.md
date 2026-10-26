# Claude Code input box captures

Captured on 2026-10-26 from Claude Code 2.1.310 running interactively in a 120×40 PTY against the real API. Each file
is the visible screen as a terminal emulator rendered it at that moment, trailing spaces removed and the working
directory in the banner shortened.

- `claude_input_restored.txt`: Esc pressed while the model was still thinking: Claude Code put the prompt's text back
  in its input box.
- `claude_input_restored_lines.txt`: the same for a prompt of three lines.
- `claude_input_one_ctrl_u.txt`: that three-line prompt after one Ctrl-U, which cleared its last line and left the
  line empty; the next Ctrl-U removed the empty line.
- `claude_input_cleared.txt`: a restored prompt after Ctrl-U: the input box is empty.
- `claude_input_draft.txt`: the near miss — text typed into the input box while the model was writing, still there
  after the Esc that interrupted the turn.

Raw output, replayed into the daemon's own screen by the tests, captured on 2026-10-26 from Claude Code 2.1.310 in a
120×40 PTY with a real account (the working directory in the banner overwritten with a string of the same length):

- `claude_input_placeholder.bin`: a fresh session at its prompt: the empty box shows the placeholder `❯ Try "write a
  test for <filepath>"`, written faint (SGR 2). It was not seen mid-turn, after a turn, or after an Esc in the same
  session.
- `claude_input_restored.bin`: the same session after its second prompt was cancelled with Esc before any output: the
  prompt is back in the box, written plain.
