# Codex prompt captures (interactions, slash popup, image placeholders)

Captured from Codex 0.167.1 in a 120x40 PTY on macOS, in Octoboard's arrangement (private app server, `--remote`
terminal, asking mode with `approval_policy="on-request"`, `approvals_reviewer="user"`), against a local stand-in for the
Responses API, from a scratch home. A `.bin` is the raw output from the launch on, replayed into the daemon's own screen;
a `.txt` is the rendered screen of a larger capture (kept as text for size).
The probe directory's machine-local path is rewritten in every file to `/private/tmp/probe-dir/...`, segment by
segment at the same length, so the byte offsets and the screen columns are those of the capture.

- `codex_interaction_command.bin`: a command approval (`touch probe_cmd_out.txt && echo created`), three options.
- `codex_interaction_command_granted.bin`: the same after `1`: the composer is back (`Gone`).
- `codex_interaction_patch.bin`: an `apply_patch` outside the workspace; `Destination:` is cut mid-word across rows.
- `codex_interaction_mcp.bin`: an MCP tool approval form (`stubmcp`, `probe_tool`), four options without key letters.
- `codex_interaction_mcp_granted.bin`: after "Always allow": the composer is back.
- `codex_interaction_elicitation.txt`: an MCP elicitation form; no hook fires for it.
- `codex_interaction_question.bin`: a `request_user_input` question, one question, two options and the terminal's own
  "None of the above" (captured with the `default_mode_request_user_input` flag; the same page as in Plan mode).
- `codex_interaction_notes_open.txt`, `codex_interaction_notes_pasted.txt`: the notes field open under "None of the above",
  empty and holding a pasted note.
- `codex_interaction_question2.txt`, `codex_interaction_question3.txt`: pages 2 and 3 of a three-question set.
- `codex_interaction_question1_partly_answered.txt`: page 1 of a three-question set gone back to (Left) after question 1 was
  answered: `(2 unanswered)`, the highlight on option 2. Not the whole set to answer, so it is not published.
- `codex_interaction_on_none.txt`: the one-question page with the highlight moved to "None of the above", notes closed.
- `codex_interaction_unanswered.txt`: "Submit with unanswered questions?".
- `codex_interaction_slash_partial.txt`, `codex_interaction_slash_bare.txt`: the slash popup after `/sta` and after `/`.
- `codex_interaction_slash_status.bin`: `/status` pasted (raw, 12 KB): the popup lists `/status` (highlighted) and
  `/statusline`. The daemon reads the screen with faint cells blanked, so this one is replayed through
  `visible_text_undimmed`, which is how the completion is read in production.
- `codex_interaction_slash_none.bin`, `.txt`: after `/status` ran: the composer back with no popup.
- `codex_interaction_images.txt`: two pasted image paths and text in the composer (`[Image #1] [Image #2]`).
- `codex_interaction_hooks.jsonl`: the `PreToolUse`, `PermissionRequest` and `PostToolUse` payloads of the calls above
  (session, turn, transcript and cwd fields removed). The `request_user_input` records are: `call_resp_…782186`, the
  call rejected in Default mode (`PreToolUse` only, no `PostToolUse`), `call_resp_…813291`, the one-question call asked
  in Plan mode (`PreToolUse` and `PostToolUse`), and `call_resp_…848607`, the three-question Plan-mode call.

Not measured: a digit on the last page of several questions (Enter submitting all is; the digit is expected to), a digit
on a middle page (Enter is), patch cancel, digit `2` on command and patch prompts (the letters `p` and `a` are pressed
instead), a notes field on a page before the last, wrapped option
rows, MCP server names with characters outside letters and digits.
