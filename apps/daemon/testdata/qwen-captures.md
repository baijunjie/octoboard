# Qwen Code captures

Measured on Qwen Code 0.28.0 unless a file's entry names another version (macOS, 120x40 PTY, scratch `HOME` and
working directory, a local stand-in for an OpenAI-compatible inference API). Paths are rewritten to `/scratch/...`;
the hook payloads and the terminal output are otherwise as captured, and the input box files are the rows of the
rendered screen, transcribed. What each capture shows is described in `docs/agent-cli-reference.md`, section "Qwen
Code".

- `qwen_hooks.jsonl`: one hook payload a line, as Qwen Code wrote it to a command hook's stdin, in the order below.
  1. `SessionStart` of a new session.
  2. `UserPromptSubmit` of a typed prompt.
  3. `Stop` of a turn that only answered.
  4. `Notification` (`idle_prompt`) after that `Stop`, with no `prompt_id`.
  5. `PreToolUse` of a shell command that needed no permission.
  6. `Notification` (`idle_prompt`) right after that `PreToolUse`, inside the turn.
  7. `PostToolUse` of the same call.
  8. `UserPromptSubmit` with an empty `prompt`: the model called back with the tool's result, inside the turn.
  9. `PermissionRequest` of a shell command that needed permission; it names no call id.
  10. `Notification` (`permission_prompt`) for the same prompt.
  11. `Notification` (`idle_prompt`) after that prompt was declined with Esc: the only event a decline fires.
  12. `PreToolUse` of `ask_user_question`.
  13. `PostToolUse` of the answered question; it carries no `prompt_id`.
  14. `StopFailure` of a turn whose model request failed with HTTP 500 after Qwen Code's own retries.
  15. `SessionEnd` at exit (Ctrl-C twice).
  16. `PreToolUse` of an MCP tool called through `tool_call`: `tool_name` is the tool itself.
  17. `PostToolUse` of the same call.
  18. `SubagentStart` of a background agent.
  19. `SubagentStop` of the same agent, which its own `background_tasks` still lists as running.
  20. `Stop` of the turn that launched it, once it had finished: the task is listed as `completed`.
  21. `Stop` of the turn Qwen Code started itself on the agent's completion; no `UserPromptSubmit` came before it.
  22. `Stop` of a turn that left a background agent running: the task is listed as `running`.
- `qwen_trust_screen.bin`: the terminal output of a launch with folder trust turned on, in a folder not yet trusted,
  up to the first drawing of the trust dialog.
- `qwen_trust_accepted.bin`, `qwen_trust_declined.bin`, `qwen_trust_cursor_moved.bin` (0.28.3, 2026-11-22): the
  terminal output of such a launch from its start, the launch's four `ui` settings in the system settings file. The
  first goes on through Enter at the dialog (written 9 s after the launch) to the input box of the fresh start, 7 s
  later; the second through Esc to the input box, 5 s later; the third through one Down, which leaves the cursor
  (`›`) on "2. Trust parent folder", 2 s later.
- `qwen_turn_cancel_esc.bin`, `qwen_turn_cancel_ctrl_c.bin`, `qwen_turn_permission_declined.bin`,
  `qwen_turn_permission_approved.bin`, `qwen_turn_two_model_calls.bin` (0.28.3, 2026-11-22): the terminal output of
  one turn each, cut from one session in `--approval-mode default` with the launch's four `ui` settings, from just
  before the prompt was typed to the screen having settled after the turn. Each holds the OSC 7501 reports and the
  titles as Qwen Code wrote them, a report right before its title:
  1. a prompt whose model request hangs, cancelled with Esc after 1.2 s: `working`, `◐`; `idle`, plain.
  2. the same cancelled with Ctrl-C: `working`, `◐`; `idle`, plain.
  3. a shell command that needs permission, declined with Esc after 2 s: `working`, `◐`; `done`, plain; `working`,
     `◐`; `blocked:kind=permission`, `✳`; `done`, plain.
  4. the same approved with Enter: `working`, `◐`; `done`, plain; `working`, `◐`; `blocked:kind=permission`, `✳`;
     `working`, `◐`; `done`, plain.
  5. a shell command that needs none, so two model calls: `working`, `◐`; `done`, plain (the gap); `working`, `◐`;
     `done`, plain.
  Each starts with a redraw of the whole frame, the logo first, as the prompt is typed; the screen before it is not
  in the file.
- `qwen_input_placeholder.txt`, `qwen_input_draft.txt`, `qwen_input_pasted.txt`, `qwen_input_yolo.txt`: the bottom of
  the rendered screen with the input box empty, holding a two-line draft, holding a long paste, and empty in YOLO
  mode, whose prompt mark is `*`.

- `qwen_hooks_steered.jsonl`, `qwen_hooks_queued.jsonl` (0.28.3, 2026-11-23, YOLO mode against the stand-in): the
  hook payloads of a prompt whose tool call runs 8 s (`perl -e 'sleep 8'`), with "held message one" submitted while
  it ran, by Enter in the first and by Ctrl+Q in the second, through to the `Stop` of the turn that took the
  message. The session ids and the paths are rewritten as in `qwen_hooks.jsonl`.

- `qwen_input_given_back.txt` (0.28.3, 2026-11-23): the bottom of the rendered screen after a prompt whose model
  request hangs, a message submitted with Enter while it ran ("steered message") and Esc: the turn reads `Request
  cancelled.` and the message is back in the input box, followed by a space and the zero-width space (U+200B) Qwen
  Code draws at the cursor. The rules are cut to 100 columns.

- `qwen_hooks_guard.jsonl` (0.28.3, 2026-11-23, `--approval-mode default` against the stand-in): the `PreToolUse`
  payloads of three file-writing calls, a `write_file` to a folder outside the working directory (which a probe hook
  refused), a `write_file` inside it and an `edit`, each as the hook's stdin; paths rewritten to `/scratch/...`.
- `qwen_prompt_shell.bin`, `qwen_prompt_shell_cursor_moved.bin`, `qwen_prompt_answered.bin`,
  `qwen_prompt_write_file.bin`, `qwen_prompt_edit.bin`, `qwen_prompt_web_fetch.bin`, `qwen_prompt_mcp.bin`,
  `qwen_prompt_notebook.bin`, `qwen_prompt_shell_multiline.bin` (0.28.3, 2026-11-23, `--approval-mode default`, a
  120x40 terminal, the stand-in's MCP server not trusted): the terminal output of one session, each cut from just
  before its turn's prompt was typed (where Qwen Code redraws the whole frame) to the screen named: a permission prompt
  for `mkdir probe-dir` (`run_shell_command`), the same with the cursor moved down three times onto "No, suggest
  changes", and the screen after `1` answered it; a prompt for a `write_file` of `ok.txt` and an `edit` of it; a
  `web_fetch` of `http://127.0.0.1:9/page`; the MCP tool `mcp__octoboard__nap`; a `notebook_edit` of a cell of
  `nb.ipynb` (after a `read_file` of it); and a command of two lines (`mkdir m1 &&` / `mkdir m2`), whose call row
  wraps. A prompt is drawn in place of the input box, from the call's row (`? Shell mkdir probe-dir (Make a
  directory) ←`) to `Waiting for user confirmation...`; while the history scrolls, a scrollbar (`█`, `│`) is drawn in
  the last column beside it. The calls, as their `PreToolUse` reported them, are the first seven lines of
  `qwen_hooks_prompts.jsonl`, in that order.
- `qwen_question_first_of_two.bin`, `qwen_question_cursor_moved.bin`, `qwen_question_second_multi.bin`,
  `qwen_question_submit.bin`, `qwen_question_typed.bin`, `qwen_question_multi_typed.bin`, `qwen_question_single.bin`,
  `qwen_question_single_multi.bin` (0.28.3, 2026-11-23, as above): an `ask_user_question` form, each cut from just
  before its turn's prompt: of two questions (a single choice "Which colour do you prefer?" with Red and Blue, then a
  multiple one "Which sizes fit?" with Small, Large and Huge) as drawn, with the cursor moved down, on the second
  question after `1` answered the first, on the `Submit` tab after Space ticked Small and Enter; in a second form, the
  first question's field written ("purple"); in a third, the second question's field written ("my own size") with
  Small ticked; and a single question, a single choice and a multiple one ("Pick one fruit?" / "Pick fruits?", Apple
  and Pear), which has no tabs.
- `qwen_question_0.28.4.bin` and `qwen_question_0.28.4_hook.json` (0.28.4, macOS, 120x40): a single-choice
  question and its `PreToolUse` payload, from a scratch launch with a loopback model and hooks enabled.
  Both 0.28.3 and 0.28.4 drew the same form and reported its wait as OSC 7501 `blocked:kind=question`.
  In both versions the hook supplied the tool and its arguments before the wait. The output also captures
  the status transitions before the form.
- `qwen_input_attachments.bin`, `qwen_input_two_attachments.bin`, `qwen_input_slash_completion.bin` (0.28.3,
  2026-11-23): the terminal output of one session, each cut from just before its first paste, after an image's path
  was pasted and `hello` written; after two images' paths were pasted with nothing written (each listed on the
  `Attachments:` row above the box, which holds only the text; Enter with the box otherwise empty sent nothing); and
  with `/sta` then pasted beside those two (the completion list under the box, the highlighted row marked `>`).
- `qwen_hooks_prompts.jsonl` (0.28.3, 2026-11-23): one hook payload a line, as Qwen Code wrote it to a command hook's
  stdin: lines 1 to 7 are the `PreToolUse` of the calls of the `qwen_prompt_*.bin` captures, in the order listed
  above; lines 8 to 11 the `UserPromptSubmit` of a message written behind the slash guard (`\u200b/stats probe
  guarded`), of one of two lines behind it, of `hello` sent with an image attached, and of a guarded message sent with
  an image. Paths are rewritten to `/scratch/q26-run/...` and the session ids replaced.

In the `.bin` captures the scratch folder's path is rewritten to `/scratch/q26-run/`, of the same length, so the
frame is drawn as captured.

The terminal titles seen, each padded with spaces to 80 columns: `Qwen - <folder>` at the prompt, `◐︎ Qwen - <folder>`
while a turn runs and `✳︎ Qwen - <folder>` while a permission prompt or a question waits (each glyph followed by
U+FE0E). Inside a turn the title goes back to the plain form for a moment between model calls (0.1 s to 0.6 s measured).

- `qwen_record_rewind.jsonl` (0.28.3, 2026-11-24): the chat record after two prompts and a
  conversation-only rewind of the second. Qwen Code appended a rewind linking to the first
  branch and a checkpoint after it. The scratch paths were rewritten.
- `qwen_record_tool.jsonl` (0.28.3, 2026-11-24): the chat record of one headless prompt against
  the stand-in, with a completed `write_file` call and a reply. The scratch paths were rewritten.
- `qwen_record_manual_title.json` (0.28.3, 2026-11-24): the title sidecar after
  `/rename Explicit Probe Title`.
- `qwen_record_prompt_title.json` and `qwen_record_auto_title.json` (0.28.3, 2026-11-24): title
  sidecars from the first prompt and the fast model respectively.
