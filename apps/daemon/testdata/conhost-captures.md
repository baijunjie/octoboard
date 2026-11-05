# Inbox console host captures

Raw output of Claude Code 2.1.319 (a native `claude.exe`, `--model haiku`) and Codex 0.166.0 (a native `codex.exe`
from its npm package, `-m gpt-5.6-luna` with low reasoning effort and no update check), each run through a Windows
pseudoconsole on 2026-11-06 in a Windows 11 23H2 (build 22631) ARM64 virtual machine, against the real APIs. The
pseudoconsole was made by `CreatePseudoConsole` of the inbox `kernel32.dll`, so its host is `C:\Windows\System32\conhost.exe`:
what the program wrote is not what comes out of the pipe, but the host's repaint of its own parsed screen, which is what
the daemon reads on Windows (`TermFacts::host_renders`). The screen was 120 columns by 40 rows, the programs started with
`TERM=xterm-256color` and `DISABLE_AUTOUPDATER=1` as the daemon sets them, and nothing outside the host answered a query.

Each file is the whole output from the start of the process up to the stage it names, so a stage is replayed alone, from
byte 0, never after another file, and on a screen of 120 columns and 40 rows (a smaller one leaves other cells of the
first frames on it). Daemon tests replay them on every OS, since they are only bytes.

The directory, folder and user names, the account's plan name, the clocks and the usage percentage of the real runs were
overwritten with text of the same length, so every cell the host skipped over with a cursor move (it draws only the cells
that changed) still holds the character it was copied from. The user name, the checkout path under it and the folder's
name (in the trust screens, the banners, the footers and Codex's titles) had their letters rotated by 13, in every place
alike (`C:\Users\oww\bpgbobneq-cbeg\z8\f1\cebw-pynhqr-vaobk`, `~\bpgbobneq-cbeg\z8\f1\cebw-pbqrk-vaobk`; Codex also draws the
path in lower case on its trust screen, and puts the folder's name in its titles); the plan name in Claude Code's account
line became "Sample Plan", every clock reads 12:00 and the usage warning 99%. The user name in the host's title from the program's path was rotated the
same way. The model's own words and the prompts are as they were.

| Capture | Shows |
| --- | --- |
| `conhost_claude_trust_screen.bin` | The folder-trust screen at the start (a fresh, untrusted folder): "No, exit" selected, with the cursor glyph U+276F before it. |
| `conhost_claude_trust_after_down.bin` | The same after a Down: the glyph on "Yes, I trust this folder". |
| `conhost_claude_input_placeholder.bin` | The fresh session after the trust screen: an empty input box whose placeholder, `Try "refactor <filepath>"` (it differs from run to run), is drawn faint (SGR 2), the idle title `✳ Claude Code`. |
| `conhost_claude_input_pasted.bin` | `say hi in two words`, pasted bracketed and not yet submitted, written plain in the box. |
| `conhost_claude_turn_running.bin` | 1.2 s after its Enter: the turn running, the spinner line and the running title (`◑ Claude Code`). |
| `conhost_claude_turn_finished.bin` | The turn finished ("Hi there!"): the box empty with no placeholder drawn, the idle title `✳ Greeting in two words`. |
| `conhost_claude_cancel_running.bin` | A second prompt (a long one) submitted, cut at its first running title, before the Esc. |
| `conhost_claude_input_restored.bin` | After the Esc: the cancelled prompt, `write a 400 word story about a lighthouse keeper`, put back in the box and written plain, the idle title. |
| `conhost_codex_trust_screen.bin` | The "Folder access" screen of a git-initialised fresh folder, drawn over Codex's loading frame: "› 1. Trust and continue" bold and inverse over the whole row, the note line showing the lower-cased path. The host's title from the program's path (`…\codex.exe`) is in the stream after the first frame. |
| `conhost_codex_trust_after_down.bin` | The same after a Down: the inverse row and the glyph on "› 2. Quit". |
| `conhost_codex_fresh_empty.bin` | The first empty composer after the trust screen (Up and Enter accepted it): the placeholder "Ask Codex to do anything" faint, a usage warning above, and a footer of two rows: the model and the directory (`~\…`), then `? for shortcuts` with `⚠ 2 warnings · f2 to view` at the right edge. |
| `conhost_codex_fresh_draft.bin` | `say hi in two words`, pasted and not submitted (the host draws it 100 to 150 ms after the paste); the footer's second row has only the warnings, the hint is gone. |
| `conhost_codex_turn_running.bin` | 1.5 s after its Enter: the "Working" status line, the title with a braille spinner glyph. |
| `conhost_codex_working_title.bin` | A second prompt submitted, cut at its first braille-spinner title, after the thread got its name (`⠼ Say hi | …`). |
| `conhost_codex_interrupted.bin` | After the Esc: "Conversation interrupted", the empty composer with its placeholder, the idle title. |
| `conhost_codex_turn_finished.bin` | The first turn finished ("Hi there", "Worked for 5s"): the empty composer, the title `Say hi | …`. |

What the stream shows that the macOS captures do not:

- Neither agent's own title is first. Claude Code's trust screen sets none; Codex's first title is the host's, the path of
  its program, which comes after the host has drawn the loading frame, and its own (the folder's name) comes later.
- Codex's footer, from 0.166.0, is two rows. Once the thread has a name the model row ends with it (`… · Say hi`).
- The host keeps the faint (SGR 2) of both placeholders, so the faint cells can be blanked, though it may draw other cells
  plain.
- A word the host does not redraw is skipped with a cursor move, so a path or a name in the raw bytes is often cut into
  pieces with the cells between them not in the bytes at all; read the parsed screen, not the bytes.

Two footers seen in the same virtual machine with Codex 0.166.0, read off the screen and not kept as files: while a
turn ran a tool and the composer held a draft, the second row was `tab to queue message` (`tab` bold) below
`GPT-6.1-Sol default · ~\code\codex-trust`; and in a terminal about 44 columns wide, an idle composer's second row was
`? for shortcuts` with `⚠ 2 · f2` at the right edge, below a model row that showed only `GPT-6.1-Sol default`.
Codex's source accounts for the queue hint and the short warnings (`codex-composer.md`, "Footer shapes from Codex
0.166.0's source"); it cuts a status line too wide for its row with `…`, so it does not explain a model row with no
directory and no `…`.
