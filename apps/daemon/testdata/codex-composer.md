# Message delivery captures

Captured from Codex 0.163.0 in a 45-row, 140-column PTY, with an isolated configuration directory,
no initial task and a local model provider. Each binary capture contains all raw output from startup.

- `codex_fresh_empty.bin`: ready at its first empty composer; the placeholder is faint.
- `codex_hooked_empty.bin`: empty after a submitted turn under command-line overrides and injected hooks; its footer includes warnings.
- `codex_fresh_draft.bin`: two pasted lines, `FRESH_DRAFT_ONE` and `FRESH_DRAFT_TWO`, without Enter.
- `codex_blank_first_draft.bin`: a draft with an empty first line and text on the following two lines.
- `codex_blank_first_no_status_draft.bin`: three empty leading rows and two text rows with `tui.status_line=[]`; the context and warnings footer remains.

These captures check the empty-composer gate before PTY message delivery.

`grok_user_prompt_submit.json` captures a Grok Build 1.0.70 `UserPromptSubmit` hook for a 181-line prompt.
Its `prompt` is plain text, with all 180 LF separators preserved.

`codex_user_prompt_submit.json` captures a Codex 0.163.0 `UserPromptSubmit` hook for a long multiline
paste under command-line configuration overrides. Its `prompt` trims leading and trailing whitespace
and preserves internal line breaks.
