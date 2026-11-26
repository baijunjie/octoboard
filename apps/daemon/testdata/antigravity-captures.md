# Antigravity CLI captures

Measured on Antigravity CLI 1.5.14 (macOS, a 120x40 PTY unless a file's name ends in another width, a scratch `HOME`
and working directory, `AGY_CLI_OSC7501=on`, and a local stand-in for the Gemini API: `"modelProvider": "gemini"` in
`--settings`, a fake `GEMINI_API_KEY` and `GOOGLE_GEMINI_BASE_URL`, and `cache/onboarding.json` marking the wizard
done; the stand-in has to treat a `model` content holding a `functionResponse` as the tool's result, or a scripted tool
call loops). Paths are rewritten to `/scratch/...`
(`/scratch/home`, `/scratch/project`, and `/scratch/session` for a launch's own directory), inside the base64 `msg` of
a status report as well; the hook payloads and the terminal output are otherwise as captured. What each capture
shows is described in `docs/agent-cli-reference.md`, section "Antigravity CLI".

Every screen is a `.bin` of the terminal's raw output, to be replayed through the terminal snapshot: the screen a
reader is given has its faint cells blanked, which no transcription shows. The one thing Antigravity CLI draws faint
(SGR 2) in any capture is the model name at the right of the footer (`Gemini 3.1 Pro · low`); the rules, the footer's
`? for shortcuts` and the mode hint of an empty box are grey (SGR 90), the prompt mark is blue (SGR 94), and a draft
is plain. Antigravity CLI sets no terminal title, so no capture holds one.

## The trust screen

Two launches in a folder not yet trusted, the launch's own directory given with `--gemini_dir`.

- `antigravity_trust_screen.bin`: from the start of the launch to the first drawing of the trust screen. It turns
  bracketed paste on before it draws, and reports `blocked` / `permission` ("Trust this folder?").
- `antigravity_trust_cursor_moved.bin`: what one Down wrote, which puts the cursor (`>`) on "No, exit".
- `antigravity_trust_accepted.bin`: from Enter on "Yes, I trust this folder" to the input box, 3 s later, with the
  `idle` report.
- `antigravity_trust_declined.bin`: the other launch, from Enter on "No, exit" to the end of the process (status 0),
  with an `idle` report.

## Screens, each from the launch

Each file is the terminal output **from the start of a launch** to the state its name says, so it is replayed from an
empty terminal of its own size. All but the last were taken in a trusted folder, in the CLI's default permission
mode with its feedback survey left on.

- `antigravity_screen_input_empty.bin`, `antigravity_screen_input_draft.bin`,
  `antigravity_screen_input_draft_two_rows.bin`: the input box empty, holding `a draft`, and holding that and a second
  row (`second row`, after Ctrl-J).
- `antigravity_screen_input_empty_80.bin`, `antigravity_screen_input_draft_80.bin`,
  `antigravity_screen_input_empty_60.bin`, `antigravity_screen_input_draft_60.bin`: the empty box and the draft
  `a draft` on an 80x40 and a 60x40 PTY. The footer stays one row at both widths; at 60 columns the rule that parts
  two turns is as wide as the box's own.
- `antigravity_screen_input_after_ctrl_u.bin`, `antigravity_screen_input_after_ctrl_u_twice.bin`: the two-row draft
  after one Ctrl-U (the second row emptied) and after a second (the row gone, the first row left): Ctrl-U does not
  clear the box.
- `antigravity_screen_input_during_turn.bin`: the box while a turn runs (`SLOW please`, its model request hanging),
  under the turn's `Generating...` row, with `esc to cancel` as its footer.
- `antigravity_screen_permission_command.bin`, `antigravity_screen_permission_command_on_no.bin`,
  `antigravity_screen_permission_write.bin`, `antigravity_screen_permission_edit.bin`,
  `antigravity_screen_question.bin`: the permission prompt of a command (cursor on "Yes, run command"; then, in
  another session, on "No, cancel"), of a file's creation and of a file's edit, and a question. None has an input
  box.
- `antigravity_screen_after_failed_turn.bin`: the screen after a failed turn where the CLI drew no survey, the input
  box in place. It is the last file of its session, so a long reply's turn is in it before the failure.
- `antigravity_screen_survey.bin`: the feedback survey (`How's the CLI experience so far?`, `[1] Good  [2] Fine  [3]
  Bad  [0] Skip`), which takes the input box's place; it came after the session's tenth turn.
- `antigravity_screen_input_empty_auto_mode.bin`: a launch with the user's signed-in account (nothing sent, the
  address replaced with `user@example.com`), from its start to the empty input box, which shows the hint of its
  mode, `Auto mode: edits & safe commands auto-approved (shift+tab to cycle)`, in grey, under the one-time notice
  `Auto is now the default agent mode`. The one screen not taken with the stand-in.

## Attachments, the completion and the task panel

Taken on 2026-11-23 against the stand-in in a trusted folder, each from the start of a launch, in one session with
`--dangerously-skip-permissions`; the scratch paths are rewritten to `/scratch/project` and `/scratch/img`.

- `antigravity_screen_image_one.bin`, `antigravity_screen_image_two.bin`: a 64x64 PNG's path pasted on its own, once
  and twice: the row above the box reads `▸ 📎 1 media attached (file, 168 B, image/png)  (ctrl+o to expand)`, then
  `▸ 📎 2 media attached  (ctrl+o to expand)`, and the box stays empty.
- `antigravity_screen_image_two_text.bin`: the same with `with text` pasted after them, in the box.
- `antigravity_screen_completion_prefix.bin`: `/cha` pasted into the empty box: the completion under it highlights
  `> /changelog  Show release notes and changes` above its hint `↑/↓ Navigate · enter Select · tab Complete`.
- `antigravity_screen_completion_none.bin`: `/nosuchcmd`: the completion reads `No matches`.
- `antigravity_screen_background_task.bin`: after a turn whose `run_command` (`sleep 8`, `WaitMsBeforeAsync` 500)
  went on as a task: the panel `● [22:53:30] sleep 8 running` between the box and a rule of its own, the footer
  ending `1 task(s) · /tasks`.
- `antigravity_screen_background_task_draft.bin`: the same with `a draft` in the box.

## The subagent's row in the panel

Taken on 2026-11-24 against the stand-in in a trusted folder, in the default permission mode, each from the start of
one launch; the scratch paths are rewritten to `/scratch/project`. One answer called `define_subagent` and
`invoke_subagent` (role `Probe Sleeper`), the subagent's one model request taking 12 s.

- `antigravity_screen_subagent_panel.bin`: 3 s on, the session's turn over: the empty box, and under it the panel
  `● Agent(sleeper: Probe Sleeper) · Gemini 3.1 Pro  Working · 1s` between two rules, the footer ending
  `1 subagent(s)`.
- `antigravity_screen_subagent_panel_draft.bin`: the same with `a draft` pasted into the box.
- `antigravity_screen_subagent_panel_permission.bin`: after Ctrl-U and a `run_command` (`echo hi`) sent while the
  subagent still ran: the command's permission prompt in place of the box, the panel hidden, the footer still ending
  `1 subagent(s)`.

## The timer's and the cron job's rows in the panel

Taken on 2026-11-24 against the stand-in in a trusted folder, in the default permission mode, each from the start of
one launch; the scratch paths are rewritten to `/scratch/project`. One answer called `schedule` twice: a timer
(`DurationSeconds` 900, `Prompt` `timer fired`) and a cron job (`0 3 * * *`, `MaxIterations` 2, `Prompt` `cron fired`).

- `antigravity_screen_schedule_panel.bin`: 4 s on, the turn over: the empty box, and under it the panel
  `● [09:09:16] cron (daily at 03:00) running` above `● [09:09:16] 15m timer running` between two rules, the footer
  ending `2 task(s) · /tasks`.
- `antigravity_screen_schedule_panel_draft.bin`: the same with `a draft` pasted into the box.

## Held messages and a cancel

Taken on 2026-11-23 through a launch's own directory, each from the start of the launch.

- `antigravity_screen_two_messages_held.bin`: a turn whose model request hangs (`SLOW first`) with two messages
  pasted into it (`held message one`, `held message two`), held above the empty input box.
- `antigravity_screen_cancel_gave_two_back.bin`: the same after Esc: both messages in the input box, a row each.
- `antigravity_screen_cancel_gave_one_back.bin`: another session, one message held, after Esc.
- `antigravity_screen_given_back_cleared.bin`: the two-row box after three presses of Ctrl-U, empty.

## Prompts answered by digits

Taken on 2026-11-23 against the stand-in in a trusted folder, in the default permission mode with the survey off, each
from the start of a launch; the scratch paths are rewritten to `/scratch/project`.

- `antigravity_screen_permission_command_all_rows.bin`: the command prompt after one Down: the highlight on row 2,
  and all six rows drawn (`5. No, and always deny … in this conversation`, `6. No, and always deny … (Persist to
  settings.json)` under the four drawn at first).
- `antigravity_screen_permission_mcp.bin`: the prompt of a tool of an MCP server of the launch's (`probe/slow`):
  `Allow calling this tool?` and six rows, from `1. Yes, allow tool call` to `6. No, and always deny tool
  'probe/slow' (Persist to settings.json)`.
- `antigravity_screen_question_multi.bin`: `ask_question` with two questions, the first of several choices: `Question
  1/2: Pick colors?`, `> 1. [ ] Red` to `3. [ ] Blue`, `4. Write-in...`, hint `↑/↓ Navigate · → Next · space Toggle ·
  enter Submit · esc Skip`.
- `antigravity_screen_question_multi_ticked.bin`, `antigravity_screen_question_last.bin`,
  `antigravity_screen_question_write_in.bin`: one form of three questions (`Which target?`, `Pick colors?` of several
  choices, `Name?`), after `2` (the second question on the screen, its first answered `Beta`), then `1` and `3`
  (`[x] Red`, `[x] Blue`); after Enter (`Question 3/3: Name?`, hint `… enter Submit All · esc Skip`); after `3` and a
  paste (`Your answer:` and `custom answer` under the rows, hint `enter Submit All · esc Back`). Enter then sent `A1:
  Beta`, `A2: Red, Blue`, `A3: custom answer` to the model.

## Turns

The terminal output of one turn each, from the paste of its prompt to the settled screen, with the OSC 7501 reports
as the CLI wrote them. All but the last come from the session of the two accepted trust captures, in the default
permission mode with `permissions.allow` holding `mcp(octoboard/*)`.

- `antigravity_turn_answer.bin`: a prompt that is only answered. `working`, `done` (with the reply). A draft left in
  the box went out with it, so the prompt reads `a drafthi`.
- `antigravity_turn_cancel_esc.bin`: a prompt whose model request hangs, cancelled with Esc after 3 s. `working`,
  `idle`.
- `antigravity_turn_permission_approved.bin`: a command that needs permission, approved with Enter. `working`,
  `blocked` / `permission` ("Approve Bash: …"), `working`, `done`.
- `antigravity_turn_permission_declined.bin`: the same declined on "No, cancel". `working`, `blocked` /
  `permission`, `idle`.
- `antigravity_turn_write_file.bin`, `antigravity_turn_edit_file.bin`: a file created and edited, each approved.
  `working`, `blocked` / `permission` ("Approve Edit: …"), `working`, `done`.
- `antigravity_turn_question.bin`: `ask_question`, answered with Enter. `working`, `blocked` / `question`, `working`,
  `done`.
- `antigravity_turn_mcp_tool.bin`: a tool of the `octoboard` server, allowed by the launch's rule. It starts with an
  `idle` (the end of the turn before), then `working`, `done`, with no prompt.
- `antigravity_turn_failed.bin`: a model request answered with HTTP 402. `working`, `error` (with the error's text).
- `antigravity_turn_long_reply.bin` (another session): a turn whose reply is 5,010 characters (`BEGIN`, the ten
  digits 500 times, `END`). Its `done` report carries the reply's first 199 characters and `…` as its `msg` (202
  bytes decoded).
- `antigravity_exit.bin`: Ctrl-D twice at the prompt, to the end of the process (status 0), with the lines that name
  the conversation to resume.

## Hook payloads and the model list

- `antigravity_hooks.jsonl`: one hook payload a line, as the CLI wrote it to a command hook's stdin, with every event
  registered, in the order of the turns above.
  1. `SessionStart`, 2. `PreInvocation`, 3. `PostInvocation`, 4. `Stop` (`NO_TOOL_CALL`) of the answered turn.
  5. `PreInvocation` of the cancelled turn: nothing follows it.
  6. to 12. the approved command: `PreInvocation`, `PreToolUse` and `PostToolUse` of `run_command`,
     `PostInvocation`, the second model call's pair, `Stop`.
  13., 14. the declined command: `PreInvocation`, `PreToolUse`; no `PostToolUse` and no `Stop` follow.
  15. to 21. `write_file`; 22. to 28. `edit_file`; 29. to 35. `ask_question`; 36. to 42. `call_mcp_tool`: each a
     `PreInvocation`, the tool's `PreToolUse` and `PostToolUse`, `PostInvocation`, the second call's pair and `Stop`.
  43., 44. the failed turn: `PreInvocation`, then `Stop` with `terminationReason` `ERROR` and the error's text.
- `antigravity_hooks_background_command.jsonl`: the hooks of a turn whose `run_command` (`sleep 6; echo
  finished-bg`, `WaitMsBeforeAsync` 500) went on as a background task: `SessionStart`, `PreInvocation`, the command's
  `PreToolUse`, `PostInvocation`, the second model call's pair, `Stop` with `fullyIdle: false`; 5 s later the
  command's `PostToolUse` (the same call id), the pair of the CLI's own turn on the task's result and `Stop` with
  `fullyIdle: true`.
- `antigravity_hooks_subagent.jsonl`: the hooks of a turn that called `define_subagent` and `invoke_subagent` (role
  `Probe Sleeper`) in one answer, the subagent's one model request taking 8 s: the session's events, and the
  subagent's (`SessionStart`, its model call's pair, `Stop` with `fullyIdle: true`) under its own `conversationId`
  with `lastUserInput` empty, interleaved as they came; the session's first `Stop` says `fullyIdle: false`, its last,
  after a turn of its own on the subagent's message, `fullyIdle: true`.
- `antigravity_hooks_schedule_timer.jsonl` and `antigravity_hooks_schedule_cron.jsonl` (2026-11-24): the hooks of a
  `schedule` call, taken with `hooks.json` shaped as the launch registers Octoboard's hooks (`SessionStart` and `Stop`
  as plain entries, `PreToolUse` and `PostToolUse` under `"matcher": "*"`, beside the guard's `write_file|edit_file`
  entry), each payload as Octoboard's entry received it. The timer (`DurationSeconds` 4, `Prompt` `check the build`):
  `SessionStart`, the call's `PreToolUse`, `Stop` with `fullyIdle: false`; 4 s later its `PostToolUse` (the same call
  id), then the `Stop` of the CLI's turn on it, `fullyIdle: true`. The cron job (`* * * * *`, `MaxIterations` 2,
  `Prompt` `cron fired`): the same, with a second `Stop` with `fullyIdle: false` after its first run, and its
  `PostToolUse` at its second.
- `antigravity_hooks_schedule_cancelled.jsonl` (2026-11-24): the hooks of the launch above, taken the same way: both
  calls' `PreToolUse` and the turn's `Stop` with `fullyIdle: false`; then, after Ctrl-U, a turn calling `manage_task`
  (`Action` `kill`, `TaskId` the timer's task id from its call's result): its `PreToolUse` and `PostToolUse`, the
  timer's `PostToolUse` (its call id) right after, and the turn's `Stop` with `fullyIdle: false`. The screen drew
  `Killed 1 task (task-2)`, the timer's row was gone and the footer said `1 task(s)`.
- `antigravity_models.txt`: what `agy models` printed for the user's signed-in account (a personal Google account on
  the free plan).
- `antigravity_transcript_full.jsonl`: the `transcript_full.jsonl` of one conversation (2026-11-23): a turn whose
  reply is 5,010 characters, a turn that calls a tool of an MCP server and then answers, a turn cancelled with Esc
  and a turn that failed.
- `antigravity_transcript_held_messages.jsonl`: the `transcript_full.jsonl` of another conversation (2026-11-23): a
  turn whose model request takes 6 s (`SLOW6 first`) with two messages pasted into it (`held message one`, `held
  message two`), which the CLI sent as one message of two rows when the turn ended, and that message's own turn. The
  `Stop` of the first turn named `lastUserInput` `SLOW6 first`, and the third entry was appended 0.5 s after that
  hook ended.
- `antigravity_transcript_system_lines.jsonl`: **not a capture.** Entries written by hand in the shape of the two
  kinds a real model's conversations hold beside the ones above (`source` `SYSTEM`, `type` `SYSTEM_MESSAGE` with
  `content`, and `type` `ERROR_MESSAGE` with `error`), in the places they were seen in: a note after the user's
  message, between two answers, after the last answer and as a transcript's last line, and an error between the
  user's message and the answer. The field names and kinds are those of real conversations; every text is made up.


## Records and quota

- `antigravity_transcript_rewind.jsonl` is a 1.5.14 interactive transcript from the local stand-in.
  Three prompts wrote steps 0 through 6. Rewinding to the third prompt and replacing it appends
  another 5 and 6. Scratch paths are rewritten as above.
- `antigravity_quota.jsonl` holds the fields Octoboard consumes from the measured 1.5.14 free account
  `/usage` JSON reply, with the two weekly buckets. It excludes unrelated reply and diagnostic data.

## A launch that exits as it starts

Measured on Antigravity CLI 1.5.14 (macOS, 2026-11-26), each an interactive launch in a 120x40 PTY with a scratch
`HOME`, working directory and `--gemini_dir` directory (its `config/.migrated` in place), `AGY_CLI_OSC7501=on` and
`AGY_CLI_DISABLE_AUTO_UPDATE=true`; no prompt was given and nothing was sent. Each file is the whole terminal output
of the launch, which ended by itself before it drew anything. Paths are rewritten to `/scratch/...`.

- `antigravity_exit_settings_unknown_key.bin`: `--settings` naming a file of `{"bogusKey": true}`. Status 1 after
  0.15 s: `Error: --settings: unknown setting "bogusKey"`.
- `antigravity_exit_settings_invalid_value.bin`: `--settings` naming a file of `{"showFeedbackSurvey": "maybe"}`, a
  key a launch of Octoboard's gives with a value the CLI does not take. Status 1 after 0.15 s: `Error: --settings:
  invalid value for setting "showFeedbackSurvey": showFeedbackSurvey: invalid value "maybe"`.
- `antigravity_exit_settings_file_missing.bin`, a near-miss: `--settings` naming a file that is not there. Status 1
  after 0.13 s, as fast as a refused setting, with no setting named: `Error: reading --settings file: open
  /scratch/nope.json: no such file or directory`.
- `antigravity_exit_unknown_option.bin`, a near-miss: `--bogus-option`. Status 2 after 0.7 s: `flags provided but
  not defined: -bogus-option`, then the whole usage text.

A file of `{"permissions": {"allow": [], "bogusList": []}}` was taken: the launch went on to its sign-in screen (the
scratch `HOME` had no sign-in) and was stopped after 8 s. So the CLI checks the top-level keys of `--settings` only,
and a refusal always names one of them.
