# Codex composer captures and footer shapes

The captures below were taken from Codex 0.163.0 in a 45-row, 140-column PTY, with an isolated configuration
directory, no initial task and a local model provider. Each binary capture contains all raw output from startup. The
footer shapes Codex 0.166.0's source gives are recorded after them.

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

## Footer shapes from Codex 0.166.0's source

Read from the `rust-v0.166.0` tag of `openai/codex` (`codex-rs/tui/src`) and its insta snapshots, where no capture
shows them. The composer reader (`src/adapter/codex/screen.rs`) takes them with Codex's default keys and default
status line items (`model-with-reasoning`, `current-dir`, `thread-name`; `chatwidget.rs`). Where the host's paths
are Unix ones it still rejects a thread's name on the status line, a directory from the home directory (`~/…`) and
the two-row footer, so on macOS it gains only one-row footers, before the thread is named, outside the home directory.

- Two layouts. With the fullscreen transcript (`tui.fullscreen_transcript`, on by default, and the alternate screen),
  the footer is two rows: the status line, then a hint row (`chatwidget/rendering.rs` sets `separate_status_line`;
  `chat_composer/status_surface.rs`). In the terminal's own scrollback it is one row (the captures above).
- A status line wider than its row is cut at its end with `…` (`truncate_line_with_ellipsis_if_overflow`), so the
  thread's name goes first, then the directory, the separator and the model. No item is left out whole.
- The hint row holds `? for shortcuts` at an empty composer (`← for agents · ? for shortcuts` with agents to switch
  to), nothing at an idle draft, `tab to queue message` while a turn runs and the composer holds a draft, and
  `esc esc to edit previous message`, `esc again to edit previous message` or `ctrl+c again to quit` (`⌃c` on macOS,
  `^c` on Linux; Ctrl-D alike) for a moment after those keys (`bottom_pane/footer.rs`). The hint is clipped at the
  row's edge, not cut with `…`; the reader takes a clipped hint of at least three characters.
- The warnings stand at the right, one column from the edge, only in a passive footer (an empty composer, or a draft
  while no turn runs): `⚠ N warnings · f2 to view` (`⚠ 1 warning · …`) where that takes at most half the row, else
  `⚠ N · f2`, which happens under 53 columns (`chat_composer/warning_notice.rs`). The hint is clipped two columns
  before them.
- A one-row footer with no status line (configured off, or while a turn runs and the composer holds a draft) has the
  hint at the left and the context window left (`N% context left`, or `12K used` without a known window) at the right
  (`single_line_footer_layout`; snapshots `footer_collapse_queue_*`, `footer_collapse_empty_*`). As the row narrows,
  the queue hint is shortened to `tab to queue` beside the context; once even that does not fit, the context goes and
  the whole `tab to queue message` comes back, down to about 22 columns, below which only `tab to queue` is left. The
  shortcuts hint goes before the context.
- The collaboration mode (`Plan mode`, `Plan mode (⇧tab to cycle)` on macOS, `Plan mode (shift+tab to cycle)`
  elsewhere), the goal's status (`Pursuing goal`, `Goal paused (/goal resume)`, `Goal unmet (…)`, …) and
  `IDE context` stand right-aligned on the status line's row (`status_line_right_indicator_line`).
- Not footers: `1 background terminal running · /ps to view · /stop to close` and `esc to interrupt` are drawn above
  the composer (`unified_exec_footer.rs`, the status indicator).

The reader still gives no composer for:

- where the host's paths are Unix ones, a two-row footer, a `~/…` directory or a thread's name;
- a remapped key, or a status line of the user's own;
- the vim mode indicator, or the collaboration mode on the left of a row without the status line;
- a flash or a hint override in the hint row, the transcript's hints while it is scrolled back (`New activity`,
  `↓ …`), the shortcut overlay, or history search;
- `? for shortcuts` alone on one row, which Codex draws only while the context window is pending, and which may as
  well be the second row of an unknown footer;
- a model cut to one word, or, on one row, a status line cut before the model's separator.
