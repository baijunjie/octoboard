# Claude Code hook captures

`claude_hooks_question_beside_late_call.jsonl` was captured on 2026-11-10 from Claude Code 2.1.319 running
interactively in a PTY, in auto mode (`permissions.defaultMode: "auto"`), against a local stand-in for the Messages API
with HTTP hooks posting to the same stand-in. The stand-in answered the prompt with one assistant message holding two
tool calls, a background `Agent` and then, two seconds later, an `AskUserQuestion`, and gave the auto-mode safety check
its verdict three seconds after asking. Each line is the JSON body of one hook post, in the order received.

The `Agent` call's safety check was only asked for once the whole message had streamed, so its `PostToolUse` came
while the question was already up: `PreToolUse` `Agent`, `PreToolUse` `AskUserQuestion`, `PermissionRequest`
`AskUserQuestion` (with no `tool_use_id`), `SubagentStart`, then the `Agent`'s `PostToolUse` about 3 s after the
question's dialog appeared. The question was answered about 28 s later; its `PostToolUse`, the `PostToolBatch`, the
background agent's `<task-notification>` prompt and the turn's `Stop` follow.

The `SessionStart` line is left out. `session_id`, `transcript_path`, `cwd` and `agent_transcript_path` are removed,
and the scratch directory in the paths left inside prompts and tool results is replaced with `/tmp/claude-probe/`.
Every other field is as Claude Code sent it.
