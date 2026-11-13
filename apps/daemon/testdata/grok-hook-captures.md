# Grok Build hook captures

The first two captures were taken on 2026-11-10 from Grok Build 1.0.77 running interactively in a PTY with a scratch
`GROK_HOME` and a fake key, its model routed through a `[model.<name>]` entry (`api_backend = "messages"`) to a local
stand-in for the Messages API, and a command hook in `$GROK_HOME/hooks/` logging every event. The stand-in answered the
prompt with one model response holding two tool calls. The third was taken on 2026-11-13 from a headless run of the
same version against its live model, with a command hook logging every event. Each line is the JSON payload of one hook
run, as Grok passed it on stdin, in the order the hook processes started.

- `grok_hooks_question_first_beside_shell.jsonl`, Ask mode (`--permission-mode default`): an `ask_user_question` and
  then a `run_terminal_command` running `sleep 6`, which needs no permission prompt. `PreToolUse` of the question,
  `PreToolUse` of the shell command, `Notification` `elicitation_dialog`, then the shell command's `PostToolUse` about
  6 s later while the question was still on screen. The question was answered about 8 s after that; its
  `PostToolUse` and the turn's `Stop` follow.
- `grok_hooks_background_subagent_beside_question.jsonl`, Always-approve mode (`--permission-mode
  bypassPermissions`): a background `spawn_subagent` and then an `ask_user_question`. `PreToolUse` of each, then the
  spawn's `PostToolUse`, `SubagentStart` and `elicitation_dialog` within 3 ms (Grok's own `timestamp`
  puts the notification first; the hook processes start concurrently, so they arrive in either order). The
  subagent's own events follow while the question waits, each carrying `subagentType`. The question's `PostToolUse`
  and the turn's `Stop` come once it was answered, about 16 s after it appeared.
- `grok_hooks_background_work_turns.jsonl`, a headless run (`grok -p --trust`): one turn that started a background
  shell command (`sleep 20`) and a background subagent (`sleep 25`) and ended with the text `STARTED`. In order: the
  session's prompt; the subagent's `SubagentStart` and its own `UserPromptSubmit` (each carrying `subagentType`); the
  turn's `Stop` (`reason: "end_turn"`) listing both in `backgroundTasks`; the shell command's completion waking the
  session with a `UserPromptSubmit` whose prompt is the `<system-reminder>` completion notice, and that turn's `Stop`
  listing the subagent; the subagent's `SubagentStop`; the subagent's completion waking the session again, and that
  turn's `Stop` listing nothing; and the teardown `Stop` (`reason: "shutdown"`, no `backgroundTasks`), kept as the last
  line.

The `SessionStart` line and the teardown `Stop` and `SessionEnd` (`reason: "shutdown"`) are left out of the first two
captures; the third has no `SessionStart` or `SessionEnd` line. `sessionId`, `session_id`, `transcriptPath`,
`transcript_path`, `cwd` and `workspaceRoot` are removed, and the scratch directory in the paths left inside tool
results of the first two is replaced with `/tmp/grok-probe/`, plain and URL-encoded. Every other field is as Grok sent
it.
