# OpenCode captures

Real output of OpenCode 1.19.6 on macOS, replayed by the tests of `adapter/opencode/accounts.rs` and
`adapter/opencode/models.rs`. Each was taken in a scratch `HOME` and scratch XDG directories under `/tmp/probe`, in an
empty working directory, with the environment cleared (`env -i`) down to `PATH`, `HOME` and the four XDG variables.
The data directory in the output was rewritten to `/tmp/probe/data/opencode/auth.json`; nothing else was changed.
The files hold standard output as it came down a pipe, colour escapes included. In every `auth list` run standard
error held only a lone reset escape (`ESC [ 0 m`), and every run exited with status 0.

| File | Command | State |
|---|---|---|
| `opencode_auth_list_signed_out_1.19.6.txt` | `opencode auth list` | no `auth.json`, no key in the environment: `0 credentials` |
| `opencode_auth_list_environment_1.19.6.txt` | `opencode auth list` | `ANTHROPIC_API_KEY` set: `0 credentials`, then an `Environment` block ending `1 environment variable` |
| `opencode_auth_list_stored_1.19.6.txt` | `opencode auth list` | `auth.json` holding one `anthropic` API-key entry: `1 credentials` |
| `opencode_auth_list_stored_and_environment_1.19.6.txt` | `opencode auth list` | the stored entry and `ANTHROPIC_API_KEY` and `OPENAI_API_KEY` set: `1 credentials`, `2 environment variables` |
| `opencode_models_signed_out_1.19.6.txt` | `opencode models` | signed out: the twelve free `opencode/…` models |
| `opencode_models_environment_1.19.6.txt` | `opencode models` | `ANTHROPIC_API_KEY` set: the free models, then the `anthropic/…` ones |

The credential in the stored runs was a made-up key (`sk-ant-fake`) in the scratch directory, which no request used.
The count is not pluralised (`1 credentials`).

## Screens

`opencode_screen_*_1.19.6.txt` are screens of OpenCode 1.19.6 as a terminal drew them: the visible rows of the
screen, one per line, each cut at its last visible cell, replayed by the tests of `adapter/opencode/screen.rs`. They
were taken by running `opencode` in a PTY (answering its terminal queries), writing the keys given, rendering what it
drew with `@xterm/xterm` at the size in the name (columns by rows), and rewriting the working directory it prints to
`/tmp/probe/cwd` (padded, so no column moves). No capture is hand-built. The scratch `HOME`, XDG directories and working
directory were under `/tmp/probe`, the environment was cleared down to `PATH`, `HOME`, the XDG variables, `TERM` and
`LANG`.

A turn needs a model, so the in-session screens were taken with a provider of `OPENCODE_CONFIG_CONTENT` (an
`@ai-sdk/openai-compatible` provider, `stub/m`) pointing at a local server that answers every request with `Done.`;
that is why the model reads `M` and the provider `Stub` on the agent row. The two prompts first captured here were
taken again as raw bytes ("Raw screens of review round 6", below).

| File | Keys written, then the screen |
|---|---|
| `start_120x40`, `start_200x40` | none: the start screen, an empty box with its hint (the example differs per run) |
| `draft_one_row_120x40` | `fix the build` |
| `draft_two_rows_120x40` | `first line`, Ctrl-J, `second line` |
| `draft_wide_120x40` | `你好，世界 ok` |
| `session_empty_120x40`, `session_empty_200x40` | `say hi`, Enter, and the turn's end: an empty box in a session; at 200 columns a sidebar is drawn |
| `session_draft_120x40` | the same, then `next step` |
| `session_draft_two_rows_200x40` | the same, then `next step`, Ctrl-J, `and more`, with the sidebar |
| `session_wide_draft_sidebar_200x16` | the same, then `你好，世界啊`, Ctrl-J, `ok`, in a terminal 16 rows high so the sidebar's words (`LSP`, `LSPs are disabled`) fall on the box's own rows |
| `session_queued_120x16` | `a SLOW one`, Enter, and while that turn runs `second message`, Enter, in a terminal 16 rows high so the conversation reaches down to the box: the prompt marked `QUEUED` ends with a row of its bar, and an empty row stands between it and the box |

The box rows that the tests build by hand are the negative cases of `what_is_not_framed_as_a_box_is_none` (a rule of 19
cells, a block with a row missing or filled): variations of a real box's rows, written inline in the test, since
OpenCode draws no such screen.

The queued screen was taken later by the same method with one difference: the stand-in holds its answer to a prompt
containing `SLOW` for twenty seconds, so the turn is still running when the second prompt is written.

## Hook payloads

`opencode_hook_payloads_1.19.6.jsonl` holds what Octoboard's plugin (`adapter/opencode/plugin.js`) handed the session's
hook script on its standard input, one payload a line, in the order they came: the plugin as committed, loaded by
OpenCode 1.19.6 through `OPENCODE_CONFIG_CONTENT`, with a hook script that appended its standard input to a file. The
model was the same stand-in as for the screens (`stub/m`), so the answers read `Final answer from stub.`; the session,
message and request ids are OpenCode's own. Nothing was edited. Each scenario is one launch, a new session each (its
`session.created` starts it), ended with `SIGTERM`:

| Order | Scenario | Payloads |
|---|---|---|
| 1 | `--prompt=hello`, answered | `session.created`, `turn.started`, `session.idle` with the answer |
| 2 | a `bash` call under `permission: {"bash": "ask"}`, allowed once with Enter | `permission.asked` (`tool` is `bash`), `permission.replied`, `session.idle` |
| 3 | the same, rejected with Esc | `permission.asked`, `permission.replied`, `session.idle` with no answer |
| 4 | the `question` tool, answered with Enter | `question.asked` (`tool` is `question`), `question.replied`, `session.idle` |
| 5 | the `task` tool: a subagent in a child session | the child's `session.created`, `turn.started` and `session.idle` carry `subagent: true` |
| 6 | a turn interrupted with Esc pressed twice, a second prompt having been submitted meanwhile | `turn.started`, `turn.cancelled` |
| 7 | the stand-in answers 401 | `turn.started`, `session.error` with the answer's message |

## Sessions and prompts

Four more launches, each a file of its own, taken on 2026-11-23 by the same method with the plugin as committed then
(it tells of a prompt only once it has stayed open, and forwards the events of the conversation a prompt was last
submitted in). Nothing was edited.

| File | Scenario | Payloads |
|---|---|---|
| `opencode_hook_payloads_auto_1.19.6.jsonl` | `--auto` under `permission: {"bash": "ask"}`, two `bash` calls in one step, each asked and answered by OpenCode itself | no `permission.asked`; two `permission.replied`, then the idle |
| `opencode_hook_payloads_two_prompts_1.19.6.jsonl` | a `bash` call and the `question` tool in one step, each answered with Enter | `question.asked`, `permission.asked`, `permission.replied`, `question.replied` |
| `opencode_hook_payloads_new_session_1.19.6.jsonl` | a turn held nine seconds that then asks for `bash`; meanwhile `/new` and a prompt there; back through the session list, the prompt allowed, a third prompt | the second session's turn; nothing of the first one's prompt or idle, which came while the second was shown; the third prompt's turn under the first session's id |
| `opencode_hook_payloads_back_in_a_session_1.19.6.jsonl` | a turn held fourteen seconds; `/new` and a prompt there; back through the session list and a prompt that joins the running turn | a second `turn.started` for the first session, under its turn's own id, with the joining prompt; then its idle |

## Lists and dialogs

`opencode_screen_dialogs_120x40_1.19.6.bin` holds the bytes OpenCode 1.19.6 wrote to a 120x40 PTY in one session (the
stand-in model, no plugin, working directory `/private/tmp/probe/cwd`), from its launch with `--prompt=hello` to its
end. Unlike the screens above it is not rendered: the tests replay a prefix of it through the daemon's own terminal.
Each offset is the count of bytes written when the screen named stood, half a second after the keys:

| Offset | Keys written, then the screen |
|---|---|
| 42367 | none: the answer and an empty box |
| 45588 | `/`: the command list above the box (then Ctrl-U) |
| 47792 | `draft @`: the mention list above the box (then Esc, Ctrl-U) |
| 59425, 78260, 97977, 115937, 135735, 152896, 170002, 188909, 207170, 224329 | ctrl+p, then `/sessions`, `/models`, `/agents`, `/themes`, `/help`, `/status`, `/connect`, `/export`, `/timeline` each with Enter: the dialog (each then Esc, Ctrl-U) |
| 243273 | `keep me`, ctrl+p: the command list over a box holding a draft |
| 251110 | Esc: the draft in the box |

`opencode_screen_dialogs_80x40_1.19.6.bin` and `opencode_screen_dialogs_40x40_1.19.6.bin` hold the same session at 80
and 40 columns (40 rows), without the two lists above the box and the draft: each dialog opened and closed in turn, as
above. Their offsets:

| Screen | 80x40 | 40x40 |
|---|---|---|
| the answer and an empty box | 60640 | 95810 |
| the ten dialogs, in the order above | 70165, 86479, 102768, 117218, 133588, 147321, 160999, 176478, 191311, 205018 | 103615, 116937, 129945, 140927, 153845, 164321, 174615, 186682, 198243, 208579 |
| the last dialog closed | 211095 | 213007 |

At 40 columns each dialog spans the terminal but for a margin, its title row is the narrowest of the captures (nine
blanks between `Connect a provider` and its `esc`), and the key hints at the foot of the session and model lists are
split into columns that wrap. `opencode_screen_dialogs_30x40_1.19.6.bin` holds the same session at 30 columns, for a
dialog that is up and not recognised: the agent list stood open at byte 63793, five blanks before its `esc`. The same
session was run at 60, 24, 20 and 16 columns and not kept: at 60 every title row has 29 blanks or more; at 30, 24 and
20 OpenCode still draws the dialogs, with fewer blanks than nine, a
longer title running into its `esc` (`Connect a providersc` at 30) and, at 20, wrapped; at 16 it fails as it starts
(Bun reports a segmentation fault).

## Raw screens of review round 6

Captures taken on 2026-11-24 for the sixth review round, in place of screens rendered by the Python terminal emulator
`pyte` earlier: each file holds the bytes OpenCode 1.19.6 wrote to a 120x40 PTY in one launch, from its start to the
last screen named, and the tests replay a prefix of it through the daemon's own terminal (`TerminalSnapshot`), read as
the production caller reads it: `visible_text()` for a prompt, `visible_text_undimmed()` for the input box and the
list above it. The scratch `HOME`, XDG directories and working directory were under `/private/tmp/nap-r6-oc`, the
environment cleared down to `PATH`, `HOME`, `USER`, the XDG variables, `TERM` and `LANG`, the terminal's queries
answered; the model was the stand-in (`stub/m`, answering `Final answer from stub.` and scripted by a word of the
prompt to call a tool), the configuration `permission: {"bash": "ask", "edit": "ask"}` for the prompts, and the plugin
as committed with this change. The working directory, `/private/tmp/nap-r6-oc/cwd`, was rewritten to `/tmp/probe/cwd`
padded with spaces to the same length, so no byte moves; nothing else was edited. Each offset is the count of bytes
written when the screen named stood, 1.5 s after its keys (0.8 s in the command session, 2.5 s after a turn's end).

### Prompts

Replayed by the tests of `adapter/opencode/interaction.rs` (and, for the outside directory and the question, by those
of `adapter/opencode/screen.rs`), each launched with `--prompt=please <word> now`:

| File | The call | Offset: keys written, then the screen |
|---|---|---|
| `prompt_bash` | `bash` of `echo hi` | 32737: none, the prompt as it opens; 33588: `l` and Enter written together, the `Always allow` step (`- echo *`); 38453: Enter on it, and the turn's end: the box back |
| `prompt_edit` | `write` of `made.txt` in the working directory (permission `edit`) | 33504: none; 35004: `l` and Enter together, the `Always allow` step ("This will allow edit until OpenCode is restarted.") |
| `prompt_external` | `read` of `/etc/hosts` | 34338: none, `← Access external directory /etc` and its pattern |
| `prompt_question` | one question that takes one answer | 31878: none; 32666: `3`, its own answer's field open; 32792: `my own answer` pasted |
| `prompt_tabs` | two questions | 33149: none; 33563: `1`; 34705: `2`, the review |
| `prompt_several` | one question that takes several | 39580: none; 39642: `1` |
| `prompt_question_alpha` | one question, `Which one?` with `Alpha` and `Beta` | 37498: none |
| `question_wrapped` | one question of two rows, its first option's label and description of two rows each | 42940: none |
| `question_multiple_wrapped` | one question that takes several, of two rows with its suffix, its first label of one long row | 42616: none |
| `permission_bash_wrapped` | a `bash` command of three `echo`s over two rows | 33319: none |
| `permission_edit_subdirectory` | a `write` of `sub/dir/made.txt`: `→ Edit sub/dir/made.txt` | 33329: none |

`opencode_hook_payloads_prompts_1.19.6.jsonl` holds the `permission.asked` and `question.asked` payloads the plugin
forwarded in the `prompt_bash`, `prompt_edit`, `prompt_question`, `prompt_tabs` and `prompt_several` launches, in that
order; `opencode_hook_payloads_wrapped_1.19.6.jsonl` those of the four launches of what wraps and then that of
`prompt_bash` again. The working directory was rewritten to `/tmp/probe/cwd` in them too (`tmp/probe/cwd` in a
pattern, which is relative to `/` outside a repository). An `edit` call carries its file as the prompt draws it
(`drawn_path`: `made.txt`, `sub/dir/made.txt`) and its `always` (`*`).

### The command list and the space

`opencode_screen_commands_120x40_1.19.6.bin`, replayed by the tests of `adapter/opencode/screen.rs`: one session with
a custom command `probecmd` (description `probe`, template `say PROBED`) in the inline configuration and no plugin.
Its offsets:

| Offset | Keys written, then the screen |
|---|---|
| 22271 | none: the start screen |
| 22657 | `/probecmd` pasted: its row above the box |
| 22840 | a space: the list closed, the box holding `/probecmd` |
| 23330 | Ctrl-U, `/zzz` pasted: `No matching items` above the box |
| 23513 | a space: the list closed, the box holding `/zzz` |
| 26582 | Ctrl-U, then `/` typed: the list of every command, several rows |
| 39573 | Ctrl-U, `first` pasted and Enter, the turn's end: a session at rest, no list |
| 39941 | `/probecmd` pasted: its row above the box, in the session |
| 40164 | a space: the list closed |
| 41631 | Enter: the command ran and the box is empty |
| 41992 | `/zzz` pasted: `No matching items` |
| 42215 | a space: the list closed, the box holding `/zzz` |
| 42291 | Ctrl-U: the box empty |

`No matching items` is drawn in a grey of the palette (`ESC [ 38;5;8 m`), not faint (SGR 2), so it stays on the screen
the input box is read from. The stand-in agent of `a_lone_command_runs_once_its_list_is_read_closed_after_the_space`
writes the bytes up to 39573, then those up to 39941 at the paste, up to 40164 at the space, and up to 41631 at the
Enter; that of `a_lone_command_with_no_matching_items_is_refused_and_cleared` writes the bytes up to 41631, then those
up to 41992 at the paste (`No matching items`), and up to 42291 at a Ctrl-U (the box empty).

### Images

`opencode_screen_images_120x40_1.19.6.bin`: at the start screen (offset 20933), a PNG's path pasted on its own twice
(21110, 21232), then `look at these` pasted (21319): `[Image 1] [Image 2] look at these` in the box.

## Row records and MCP inventory

Captured from OpenCode 1.19.6 on macOS in scratch HOME and XDG directories, with a local inference stand-in and no
real model turn. `opencode_db_rows_1.19.6.json` is the stdout of the provider's session/message/part SELECT after
one completed turn. `opencode_mcp_list_1.19.6.txt` is the stdout of `opencode mcp list` with two local stand-ins
using `/usr/bin/false`, one disabled and one that fails the MCP connection. The final blank line is omitted.
