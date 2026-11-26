# GitHub Copilot CLI captures

Test data for the Copilot adapter (`apps/daemon/src/adapter/copilot/`), taken from GitHub Copilot CLI 1.0.114 on macOS
on 2026-11-22. What they show is written up in `docs/agent-cli-reference.md`, "GitHub Copilot CLI".

How they were taken. Copilot ran in a pseudo-terminal of 40 rows, in a scratch home (`HOME`) with no sign-in, pointed
at a local stand-in for the model (`COPILOT_OFFLINE=true`, `COPILOT_PROVIDER_BASE_URL`, `COPILOT_MODEL=standin`): an
OpenAI-compatible server that answers a prompt with scripted text or tool calls, so that each kind of turn could be
provoked exactly and without a plan's spend. Everything Copilot itself wrote is as it wrote it: the terminal bytes, the
hook payloads its hooks received on stdin, and the lines of its `events.jsonl`. Only the tool call ids
(`call_<8 hex>`), the prompts (`CMD:…`, the script's commands) and the model's texts (`ok`, `done`, `slow slow …`) are
the stand-in's. A run against the hosted model with a signed-in account gave payloads of the same fields, with the
model's own call ids. The probe's scratch paths are rewritten to `/tmp/probe/…`; nothing else is edited.

## Screens

Taken in `/private/tmp/copilot-probe/w120` and `…/w80`, folders Copilot had not trusted, at 120 and at 80 columns
(the number in each name).

Terminal bytes, as the pseudo-terminal gave them:

- `copilot_trust_screen_<w>.bin`: from the start to the folder-trust prompt fully drawn, the cursor `❯` on "1. Yes".
- `copilot_trust_after_down_<w>.bin`: what one Down added: the cursor redrawn on "2. Yes, and remember this folder for
  future sessions".
- `copilot_trust_dismissed_<w>.bin`: what Enter there added over the next five seconds: the prompt gone, "Folder … has
  been added to trusted folders." and the composer drawn for the first time.
- `copilot_trust_on_resume_1.0.114_120.bin`: a 30-row resume on macOS with the folder-trust modal open. The previous
  prompt is drawn after the modal with the same `❯` glyph. The visible modal still has its cursor on "1. Yes".
- `copilot_permission_prompt_<w>.bin`: everything after that up to a permission prompt fully drawn ("Do you want to run
  this command?" with its three options, the cursor on "1. Yes"). It is the near miss of the trust prompt. On the way
  it holds the drafts typed for the composer captures below and a turn cancelled with Ctrl-C.

The composer, as terminal bytes too, taken in the same two folders once Copilot trusted them, so that a test renders
them as production does, with the faint cells blanked (`TerminalSnapshot::visible_text_undimmed`, what a draft is read
off) and as drawn (`visible_text`, what the start signal reads). `copilot_composer_<w>_<nn>_<state>.bin` is what
Copilot wrote between the state before it and that state, so the screen at a state is the files up to it fed in
order, in a terminal of 40 rows and `<w>` columns:

| File | The screen then |
|---|---|
| `01_empty` | From the start: the composer empty, `❯` alone between its two rules, the footer on the last row. |
| `02_one_line` | `one line draft` typed. |
| `03_wrapped` | After Ctrl-U: a first line longer than the window, a line break (Ctrl-J) and `second line`. The wrapped rows and the second line are indented two columns. |
| `04_empty_line` | After clearing: `above the gap`, two line breaks, `below the gap`: a draft with an empty line in it. |
| `05_cleared` | The draft cleared again (Ctrl-U and Backspace by turns): the composer empty. |
| `06_during_turn` | `CMD:slow` sent and, while the turn runs (`◉ Working` in the footer), `typed during the turn` typed. The prompt sent is in the timeline as ` ❯ CMD:slow`, indented. |
| `07_after_cancel` | Ctrl-C: the turn is cancelled and the draft is still in the composer. |
| `08_permission_prompt` | After Ctrl-U, a turn whose shell command raises a permission prompt: no composer is drawn; the menu is inside a box, its cursor row `│ ❯ 1. Yes`. |
| `09_ask_user_form` | After Esc, a turn whose `ask_user` form is up: no composer is drawn; the form sits between two rules, its cursor row ` ❯ alpha` indented. |
| `10_back_at_prompt` | After Esc there: the composer back, empty. |

Both of the composer's rules are drawn faint (SGR 2), so with faint cells blanked they are empty rows; the mark `❯`,
the draft and the footer are plain. The form's rules are plain.

The composer with the sidebar open, taken the same way on 2026-11-23 in `/private/tmp/copilot-probe/s120` and `…/s80`,
folders Copilot already trusted. `copilot_sidebar_<w>_<nn>_<state>.bin` is what Copilot wrote between the state before
it and that state:

| File | The screen then |
|---|---|
| `01_open` | From the start, then Left at the empty prompt: the sidebar open, and the whole view drawn to the right of its border, the composer `│❯` between its two rules, each of which starts at the border too. |
| `02_one_line` | `one line draft` typed. |
| `03_wrapped` | After Ctrl-U: the long first line of the captures above, a line break and `second line`; the wrapped rows and the second line are indented two columns from the border. |
| `04_cleared` | The draft cleared again: the composer empty. |

The sidebar's border is drawn in a colour (SGR 90), not faint, so it is there in both renderings. The sidebar's own
column holds text on some rows (the session's name and folder), none of it a border.

### The form

An MCP server's form, taken on 2026-11-23 the same way (a scratch home with no sign-in, the stand-in, 40 rows), in
`/private/tmp/copilot-elicit/w120` and `…/w80`, folders Copilot had not trusted, with a stub MCP server `stub` whose
tool `elicit` answers a call with `elicitation/create`: the message `Which fruit should the stub use?` and two fields,
`fruit` (required, `apple` or `pear`) and `note` (text). `copilot_elicitation_<w>_<nn>_<state>.bin` is what Copilot
wrote between the state before it and that state:

| File | The screen then |
|---|---|
| `01_start` | From the start to the folder-trust prompt. |
| `02_trusted` | Enter: the prompt answered, the composer drawn. |
| `03_form` | `CMD:elicit` sent; the stand-in calls `stub-elicit` and the form is drawn between two rules: `stub needs information.`, the message, the fields as tabs (`[fruit]  note`), the focused field's choices with `❯ apple`, and the footer `↑/↓ select · enter accept · tab next · ctrl+d decline · esc cancel`. Copilot reports `state=blocked:kind=question` over OSC 7501. |
| `04_escaped` | Esc: the form erased, the server answered `action: "cancel"`, and the turn ends with `done`. |

`copilot_elicitation_fields_120_<nn>_<state>.bin` is a second session at 120 columns, its first three files as above:

| File | The screen then |
|---|---|
| `04_tab_refused` | Tab with no choice made: ` * This field is required`, the focus kept. |
| `05_paste_and_enter` | A bracketed paste of `typed message` and Enter: the paste is nowhere, the Enter accepts `apple` and moves the focus to `note`, whose footer is `enter accept · shift+tab prev · ctrl+d decline · esc cancel`. |

`ask_user`'s question is the same form titled `Copilot needs information.` (`copilot_composer_<w>_09_ask_user_form.bin`
above), with the footer `↑/↓ select · enter accept · ctrl+d decline · esc cancel`.

### The draft stash

Taken on 2026-11-23 with 1.0.114 in a scratch `COPILOT_HOME` signed in through the GitHub CLI's token (no stand-in,
no prompt sent), in a pseudo-terminal of 40 rows at 120 and 80 columns, the folder trusted for the session with Enter.
`copilot_stash_<w>_<nn>_<state>.bin` is what Copilot wrote between the state before it and that state, fed in order as
the composer captures are; the scratch folder's path is rewritten to `/tmp/probe/w<w>`.

| File | The screen then |
|---|---|
| `01_draft` | From the start: `keep this draft` typed into the composer. |
| `02_stashed` | Ctrl-S: the composer empty, the footer ending `stashed · Copilot Fusion`. |
| `03_restored` | Ctrl-S again: the draft back, `stashed` gone from the footer. |

## Hook payloads

`copilot_hook_payloads.jsonl`: one object a line, `{"scene", "event", "payload"}`: `payload` is the hook's stdin as it
came, `event` the name its hook entry was registered under (what Octoboard's entries pass as `--event`), and `scene`
the session it is from. Every native event was registered, the two Octoboard does not register (`permissionRequest`,
`errorOccurred`) and `sessionEnd` included. The scenes, each one session in a trusted folder with a stub MCP server
named `octoboard` allowed by `--allow-tool=octoboard`:

| Scene | What happens |
|---|---|
| `prompt` | A prompt answered with text. |
| `shell-approved` | The model runs `touch`; the permission prompt is answered "1. Yes". |
| `create-approved`, `edit-approved` | The model calls `create`, then `edit` in a session of its own (`toolArgs` an object naming `path`); each prompt is approved. |
| `apply-patch-approved` | With the model named as a GPT one (`COPILOT_PROVIDER_MODEL_ID=gpt-5.4`), the model calls `apply_patch`, whose `toolArgs` is the patch as a string; approved. |
| `mcp-call-allowed` | The model calls `octoboard-report`, which the allow rule lets run with no prompt; `permissionRequest` runs all the same. |
| `ask-user-answered` | The model calls `ask_user`; the form's first choice is accepted. |
| `subagent-shell-approved` | The model starts a subagent (`task`) whose own turn runs `touch`: the subagent's `userPromptSubmitted`, `preToolUse`, `permissionRequest`, `postToolUse` and `agentStop` carry `agentId`; the `permission_prompt` notification of its prompt does not. |
| `detached-shell-completes` | The model starts `sleep 3` detached; three seconds after the turn ends, the completion arrives as a prompt wrapped in `<system_notification>` with a `shell_detached_completed` notification. |
| `model-error` | The model request is answered with status 400: one `errorOccurred`, no `agentStop`. |
| `permission-refused-with-feedback` | The permission prompt is answered "3. No, and tell Copilot what to do differently" with a line of text: no post event for the call; the turn carries on to its `agentStop`. |
| `two-calls-one-prompt` | One model message calls `bash` (a `touch`, which prompts) and `view`: both `preToolUse`, then `view`'s `postToolUse`, and only then the prompt's `permissionRequest` and notification. |
| `subagent-fails` | The subagent's model request fails: its `errorOccurred` carries `agentId`; the session's `task` call ends with `postToolUseFailure`. |
| `cancelled-esc-twice` | Esc twice while the model writes: no hook after `userPromptSubmitted`. |
| `permission-cancelled-esc` | Esc at the permission prompt: no hook after the notification. |

| `hosted-apply-patch-denied` | The one scene against the hosted model (a signed-in Copilot Free account, where Copilot Fusion chose MAI Code 2 Flash), in `/tmp/copilot-probe/own` with Octoboard's cross-project guard as a second `preToolUse` entry and `--allow-all`: asked to create a file in another project, the model calls `apply_patch` (its call id `custom_call_…`), the guard denies it, and no post event runs. |

| `message-during-shell` | With `--allow-all`, the model runs `sleep 6` and a message is sent while it runs: the command is moved to the background and its call ends at once, the message joins the turn (no `userPromptSubmitted` runs for it), and the command's completion arrives later as a prompt wrapped in `<system_notification>` with a `shell_completed` notification. |
| `subagent-ask-user-unavailable` | A subagent's model calls `ask_user`: a subagent has no such tool, and the call ends with `postToolUseFailure` ("Tool 'ask_user' does not exist."), with no form and no notification. |

Each session's last line is the `sessionEnd` its stop ran.

## Records

`copilot_events.jsonl`: lines of the sessions' `events.jsonl`, whole and in order, of these types only: `user.message`,
`assistant.turn_start`, `assistant.message`, `assistant.turn_end`, `tool.execution_start`, `tool.execution_complete`,
`permission.requested`, `permission.completed`, `abort`, `session.error`, `subagent.started`, `subagent.completed`,
`subagent.failed`. The other lines (the session's start, its system prompt, each hook's start and end, usage) are left
out. A subagent's lines carry `agentId` beside `type`, not inside `data`; the `permission.completed`
of a subagent's permission prompt is one of them (line 22, `approved`). The sessions, by line:

| Lines | Session |
|---|---|
| 1-11 | `shell-approved`: a message that only requests a tool (`content` empty), the permission approved, then the turn's text. |
| 12-33 | `subagent-shell-approved`: the subagent's own prompt, messages and turn end among the session's. |
| 34-48 | `subagent-fails`: the subagent's `session.error` and `subagent.failed`; the session's turn carries on. |
| 49-58 | A subagent running when Esc is pressed twice: the session's own `abort` (`user_abort`). |
| 59-62 | `cancelled-esc-twice`: `abort` with `reason: "user_abort"`. |
| 63-66 | The same turn cancelled with Ctrl-C: `abort` with `reason: "user_initiated"`. |
| 67-74 | `permission-cancelled-esc`: `permission.completed` with `result.kind: "cancelled"`, then `abort`. |
| 75-85 | `permission-refused-with-feedback`: `permission.completed` with `result.kind: "denied-interactively-by-user"`; no `abort`. |
| 86-89 | `model-error`: `session.error` after the turn's end. |
| 90-94 | The model answers with no text: an `assistant.message` whose `content` is one space, then `session.error` ("No response was returned…"); this turn did run `agentStop`. |
| 95-103 | A turn whose first message only requests a tool and whose last has a `content` of one space: the turn ends with no text of its own. |

## Background work

Taken on 2026-11-23 the same way (a scratch home, the stand-in, 120 columns), each scene one session in a trusted
folder with the stub MCP server, which here has a tool `slow` that sleeps for the seconds it is given.

`copilot_background_hook_payloads.jsonl` has the shape of the hook payloads above, in the order the hooks reached their
log. `copilot_background_events.jsonl` holds the same sessions' record lines, one session after another, of these types:
`user.message`, `assistant.message`, `system.notification`, `tool.execution_start`, `tool.execution_complete`,
`subagent.started`, `subagent.completed`, `permission.requested`, `permission.completed`, `session.shutdown`,
`session.resume`, and the `hook.start` lines of `agentStop` alone.

| Scene | Record lines | What happens |
|---|---|---|
| `two-detached-shells` | 1-21 | With `--allow-all`, one model message starts `sleep 4` and `sleep 9` detached (`mode: "async"`, `detach: true`). Both calls end at once (lines 5 and 6, `shellId` 0 and 1) and the turn ends (8). Each completion is a `system.notification` of kind `shell_detached_completed` (9, 15) and a turn of its own, which ends at lines 14 and 20. |
| `background-shell-completes` | 22-34 | `sleep 5` started with `mode: "async"` and not detached: its call ends at once (25), the turn ends (27), and the completion is a `system.notification` of kind `shell_completed` (28). |
| `background-agent-idle` | 35-55 | A `task` call with `mode: "background"`: `subagent.started` with `executionMode: "background"` (38), the session's turn ends (41) while the agent runs a foreground `sleep 6` of its own (44-45), then `subagent.completed` (48) and a `system.notification` of kind `agent_idle` (49). |
| `detached-shell-stopped` | 56-65 | A detached `sleep 30` (59) that the model stops with `stop_bash` (61-62): no notification follows. |
| `slow-call-ends-at-prompt` | 66-76 | Manual approval. One model message calls `bash` (a `touch`, which prompts) and `octoboard-slow` for 6 seconds: the permission is requested (70), the slow call ends while the prompt is still up (71, its `postToolUse` 6 s after the notification), and the prompt is answered three seconds later (72). |
| `notification-after-turn-end` | 77-85 | The same `touch` with the notification hook's delivery held back four seconds: the prompt is answered at once (80-81), the call ends and the turn ends (84), and only then does the `permission_prompt` notification reach the log, as the payload file's order shows. |
| `idle-agent-written-to` | 86-124 | A background agent as above (`subagent.started` 89, `subagent.completed` 99, `agent_idle` 100), then a second prompt whose model message calls `write_agent` with the agent's id (108): the agent works again from a `user.message` of its own (112) with no `subagent.started`, and ends with a second `agent_idle` notification (118) and no `subagent.completed`. Its hooks the second time: its own `userPromptSubmitted`, its `agentStop` and a `subagentStop`, no `subagentStart`. |
| `resumed-after-kill` | 125-137 | One model message starts `sleep 90` detached and `sleep 91` in the background (129-130), the turn ends (132), and the CLI's process is killed with `kill -9`: no `session.shutdown`. The session resumed with the same `--session-id` writes `session.resume` (133) and takes a prompt (134-136). |

## Captures of 2026-11-23 against a stand-in

Taken with 1.0.114 in a scratch `HOME` with no sign-in, pointed at a local OpenAI-compatible stand-in
(`COPILOT_OFFLINE=true`, `COPILOT_PROVIDER_BASE_URL`, `COPILOT_MODEL=standin`) that answers `CMD:tool <name> <json>`
with that tool call, in a pseudo-terminal of 40 rows and 120 columns. Each series is one session, its files the bytes
Copilot wrote since the file before, fed in order; the first file of each series holds everything from the start. The
scratch folder's path appears as Copilot wrote it.

### Interactions

| File | The screen then |
|---|---|
| `copilot_permission_120_01_permission` | The permission prompt for `touch made-by-probe`, the cursor on `1. Yes`. |
| `copilot_permission_120_02_on_third` | Two Downs: the cursor on `3. No, and tell Copilot what to do differently (Esc to stop)`. An Enter here changed nothing. |
| `copilot_permission_120_03_feedback_typed` | `use echo` typed: the row reads `3. use echo (Esc to stop)`. |
| `copilot_permission_120_04_denied` | Enter: the call refused with the feedback, the turn answered `done`, the composer back. |
| `copilot_question_120_01_form` | An `ask_user` form of two fields, `choice` (`alpha`, `beta`) and `note` (a string), the cursor on `alpha`. |
| `copilot_question_120_02_other` | Three Downs: the cursor on `Other (type your answer below)` (Down stops there), a text field open below the form. |
| `copilot_question_120_03_other_typed` | `zeta` typed into that field. |
| `copilot_question_120_04_text_field` | Enter: the `note` field has the focus, its text field empty. |
| `copilot_question_120_05_text_typed` | `hello` typed. |
| `copilot_question_120_06_answered` | Enter: the form sent (`choice: zeta`, `note: hello`), the composer back. |

### Images

| File | The screen then |
|---|---|
| `copilot_images_120_01_one_image` | A PNG's path pasted alone: `[image: red-one.png]` in the composer. |
| `copilot_images_120_02_two_images` | A second pasted: `[image: red-one.png][image: blue-two.png]`. |
| `copilot_images_120_03_with_text` | `what colours` pasted after them. Enter then sent both images to the model. |

### Slash commands

| File | The screen then |
|---|---|
| `copilot_commands_120_01_idle` | The idle composer. |
| `copilot_commands_120_02_slash` | `/` typed: the list, `❯ /add-dir` highlighted, the composer showing `/add-dir`. |
| `copilot_commands_120_03_partial` | `he` typed: `❯ /help` highlighted, the composer showing `/help`. |
| `copilot_commands_120_04_ran` | Enter: `/help`'s view. |
| `copilot_commands_120_05_with_argument` | Esc, Ctrl-U and `/model ` typed: the command's forms, `❯ /model` highlighted, an argument hint in the composer. |


## Captures of 2026-11-24 against a stand-in

Taken the same way as the 2026-11-23 series (1.0.114, a scratch `HOME` with no sign-in, the stand-in answering
`CMD:tool <name> <json>` with that call, 40 rows and 120 columns, the stub MCP server `octoboard` given through
`--additional-mcp-config`), each series one session from its start, each file the bytes written since the one before.
The working directory is `/private/tmp/nap-r5-cop/work`, as Copilot wrote it.

| File | The screen then |
|---|---|
| `copilot_permission_create_120_01_prompt` | `create` of `made.txt`: `Create file`, the diff, `Do you want to create <path>?`, three rows. |
| `copilot_permission_edit_120_01_prompt` | `edit` of `existing.txt` (`hi` to `ho`): the diff, `Do you want to edit <path>?`, three rows. |
| `copilot_permission_view_120_01_prompt` | `view` of `/etc/hosts`: `Allow path access`, the path and its directory in boxes, `Do you want to allow this?`, four rows ending `4. No (Esc)`. |
| `copilot_permission_view_120_02_on_no` | Three Downs: the cursor on `4. No (Esc)`. |
| `copilot_permission_view_120_03_chosen` | Enter: the call refused ("The user rejected this tool call."), the turn cancelled, the composer back. |
| `copilot_permission_mcp_120_01_prompt` | `octoboard-slow` with `{"seconds": 0}`: `slow`, the arguments as indented JSON in a box, `Do you want to use this tool?`, three rows. |
| `copilot_permission_mcp_120_02_refused` | Esc: the call and the turn cancelled, the composer back. |
| `copilot_permission_long_120_01_prompt` | `touch` with 40 arguments (845 characters): the command wraps in its box at spaces. |
| `copilot_permission_huge_120_01_prompt` | `touch` with 200 arguments (4,205 characters): only the start of the box, no bottom side, the hint with `pgup/pgdn to scroll`. |
| `copilot_feedback_wrap_120_01_prompt` | The prompt for `touch made-by-probe`. |
| `copilot_feedback_wrap_120_02_on_third` | Two Downs: the cursor on the feedback row. |
| `copilot_feedback_wrap_120_03_wrapped` | A feedback of 173 characters pasted: `3. <text>` wrapping onto an indented row, `(Esc to stop)` on a row of its own. |
| `copilot_feedback_wrap_120_04_sent` | Enter: the call refused with the whole feedback, the turn answered `done`, the composer back. |
| `copilot_question_wrap_120_01_form` | An `ask_user` form of two string fields whose message wraps onto a second row; the fields' row after a blank one. |
| `copilot_question_wrap_120_02_typed` | 176 characters typed in the first field: the text wraps onto a second row; the footer ends `esc clear`. |
| `copilot_question_wrap_120_03_ctrl_d` | Ctrl-D: nothing changes while the field holds text. |
| `copilot_question_wrap_120_04_esc` | Esc: the field emptied, the footer ending `esc cancel` again. |
| `copilot_question_wrap_120_05_declined` | Ctrl-D: the form declined, the turn going on (`done`), the composer back. |

## Exits at start

Taken on 2026-11-26 from GitHub Copilot CLI 1.0.114 (macOS, the npm package `@github/copilot`), each a launch in a
pseudo-terminal of 40 rows and 120 columns, in an empty working directory, with `COPILOT_HOME` an empty scratch
directory and no sign-in. Each file is every byte Copilot wrote, from its start to its end; nothing is edited. Neither
run sent a prompt.

| File | The launch, and what it wrote |
|---|---|
| `copilot_exit_unknown_option.bin` | `copilot --octoboard-unknown-option`: `error: unexpected argument '--octoboard-unknown-option' found`, the usage line and the hint to try `--help`, then the sequences that leave the terminal as found (the alternate screen switched on and straight off, mouse reporting, bracketed paste and focus reporting off, the cursor shown). Status 1, within 0.6 s. `--octoboard-unknown-option=1` wrote the same. |
| `copilot_exit_resume_not_found.bin` | The near miss, a fast exit that refuses no option: `copilot --resume=<a uuid of no session>`: a `Resuming session` spinner for about 7 s, then `Error: No session, task, or name matched '<id>'.` with the ways to resume or start a session, and the same leaving sequences. Status 1. |
