# Grok Build hook captures

Captured on 2026-11-10 from Grok Build 1.0.77 running interactively in a PTY with a scratch `GROK_HOME` and a fake
key, its model routed through a `[model.<name>]` entry (`api_backend = "messages"`) to a local stand-in for the
Messages API, and a command hook in `$GROK_HOME/hooks/` logging every event. The stand-in answered the prompt with one
model response holding two tool calls. Each line is the JSON payload of one hook run, as Grok passed it on stdin, in
the order the hook processes started.

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

The `SessionStart` line and the teardown `Stop` and `SessionEnd` (`reason: "shutdown"`) are left out. `sessionId`,
`session_id`, `transcriptPath`, `transcript_path`, `cwd` and `workspaceRoot` are removed, and the scratch directory in
the paths left inside tool results is replaced with `/tmp/grok-probe/`, plain and URL-encoded. Every other field is
as Grok sent it.
