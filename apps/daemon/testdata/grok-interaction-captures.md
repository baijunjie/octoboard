# Grok Build pending-interaction captures

Captured on 2026-11-20 from Grok Build 1.0.89 (`--no-leader --permission-mode default`) running interactively in a
120x40 PTY against a local stand-in for its inference API, from a scratch home with a fake API key; its hooks were
logged beside the screens. Each file replays into the daemon's own screen (`TerminalSnapshot::new(40, 120)`). The
measurements are summarised in `docs/agent-cli-reference.md`, "Grok Build".
The probe directory's machine-local path is rewritten in every file to `/private/tmp/probe-dir/...`, segment by
segment at the same length, so the byte offsets and the screen columns are those of the capture.

Two are the raw output from the launch on:

- `grok_interaction_shell_card.bin`: a shell permission card (`2 (●) Yes, proceed`, `3 (○) No, reject (type to add
  feedback)`, `4 (○) Never allow: printf`, and the unnumbered always-approve row), footer `2/4:select`.
- `grok_interaction_mcp_card.bin`: an MCP card, `Allow (Probemcp) Echo?` with its arguments behind `│`, rows 2 to 5.

The others are the screen of a capture of the same session, which holds 1 to 2 MB of spinner frames each (Grok draws
only the differences, with no full redraw to cut the output at), re-emitted as the 40 rows it shows (cursor
positioned, no attributes); each was checked once, when written, to replay to the same rows as the full capture read
with its faint cells blanked (`visible_text_undimmed`, which is how the reader is given the screen). They therefore lack
the faint footer separators and the background-only highlights of the originals: nothing in them tests a row's focus by
colour (a permission card's focus is read from its `(●)`; a question card's cursor is background-only and is not read).

- `grok_interaction_edit_card.bin`, `grok_interaction_write_card.bin`: `Allow Edit to notes.txt?`, and the same for
  `write` with the path cut in the middle; `2 Yes, allow edits in work/ for this session`, `3 (●) Yes`, `4 No, reject`.
- `grok_interaction_shell_parked.bin`: Esc pressed on the shell card, footer `Tab:permission`, the card still drawn.
- `grok_interaction_shell_typing.bin`, `grok_interaction_shell_typed.bin`: the No row reached and a message started
  (`3 (●) ❯ use`, then a pasted one), footer `Enter:send │ Esc:back`.
- `grok_interaction_shell_on_always.bin`: the highlight on the always-approve row (the scope widened).
- `grok_interaction_idle.bin`, `grok_interaction_dismissed.bin`: the composer back after a card was answered or
  dismissed with Shift+X.
- `grok_interaction_single.bin`, `grok_interaction_single_parked.bin`, `grok_interaction_single_field.bin`,
  `grok_interaction_single_typed.bin`: one `ask_user_question` question with three options and the `z` row; the same
  parked (`Tab:question`); the `z` row opened (`z (●) ❯`); and with a pasted answer in it.
- `grok_interaction_multi.bin`, `grok_interaction_multi_toggled.bin`, `grok_interaction_multi_typed.bin`: a
  multi-select question (`[ ]`), options 1 and 3 ticked, and a written answer beside them.
- `grok_interaction_three_q1.bin`, `_q2.bin`, `_q3.bin`, `_q3_typed.bin`, `_back_q1.bin`: three questions
  (single, multi, single), shown one at a time with `[k/3]` on the hint row; the last with a pasted answer; and back at
  the first after answering, with the earlier pick highlighted.
- `grok_interaction_slash_partial.bin`, `grok_interaction_slash_completed.bin`: `/probe` pasted, with the completion
  `❯ /probecmd <words> …` between two rules above the composer and the rest of the name (`cmd`) drawn in a dim colour in the box, not as SGR 2 faint, so blanking faint cells does not remove it; then
  after Enter, which accepted `/probecmd <words>` into the box without sending it.
- `grok_interaction_two_images.bin`: two image paths pasted one after the other, `❯ [Image #1] [Image #2]`.

`grok_interaction_hooks.jsonl` holds the `PreToolUse` lines of the same probe, as Grok sent them (the session's paths
and ids dropped): a shell command, an edit, a write, an MCP call, and three `ask_user_question` calls (single,
multi-select, three questions). The probe sent none from a subagent.

Not measured: whether Enter in the `z` field on a question before the last advances or submits all the questions (the
reader presses a written answer on the last question only); whether a digit on the last of several single-select
questions submits; Tab wrapping from the unnumbered always-approve row to the first row; why a pasted `/sess` ran and a
pasted `/probe` only completed.
