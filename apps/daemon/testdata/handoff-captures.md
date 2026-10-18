# Handoff record fixtures

Conversation records for the readers that turn a record into a handoff package (`src/handoff/record.rs`), replayed
through the real readers. No model call was made to produce them.

Each line is a real line from a record on the development machine, taken for its shape and scrubbed: every key not
needed to tell the line apart is dropped, ids, paths and timestamps are gone, and its text is replaced with neutral text
(the key set, nesting and discriminating values such as `type`, `origin.kind` and `synthetic_reason` are the real ones).
Each file holds two or more user messages, the replies, todo updates and edit calls, beside near-miss lines a reader must
drop: tool output, reasoning, injected context, a subagent's or stand-in line. Those carry `TOOL-OUTPUT`, `REASONING`,
`INJECTED` or `SIDECHAIN` in their text, which the tests assert never reach the conversation.

| Capture | Source version | Near-misses it holds |
| --- | --- | --- |
| `handoff_claude_2.1.302.jsonl` | Claude Code 2.1.302 | A message typed mid-turn, recorded only as a `queued_command` attachment with `origin.kind` `human` (a string `prompt` and a list one); a background task's notification as a user line (with `origin.kind` `task-notification`), as a `queued_command` attachment, and a `peer` user line; a meta line; a compact summary; a subagent's (`isSidechain`) reply; a `<synthetic>` stand-in reply; a tool result. |
| `handoff_codex_0.162.1_rollout.jsonl` | Codex 0.162.1 (the `user_message` line is from 0.142.5) | The injected `AGENTS.md` user message beside the typed one; reasoning, command and function-output items; a `FileChange` item that failed and one carrying a `move_path`. |
| `handoff_grok_1.0.62_chat_history.jsonl` | Grok Build 1.0.62 | System, project-instruction and system-reminder lines (with `synthetic_reason`), `<user_info>` and `<image_files>` user lines, a `<user_query>`-wrapped prompt; a compaction: the `compaction_meta` line restating the rules, and the one carrying the summary. |

Synthesized, because no record on the machine holds a sample, from the tools' published input shapes:

- Claude Code: the `TodoWrite` and `NotebookEdit` tool calls. No transcript held either.
- Claude Code: the older `<task-notification>` user line with no `origin`.
- Grok Build: a `<system-reminder>` user line with no `synthetic_reason`.
- Codex: the `update_plan` function call and the `apply_patch` custom tool call (with its `*** Move to:` line). Codex
  0.162.1 runs its tools through a code-mode `exec`, so none of its rollouts held either as a call of its own.
- The `todo_write` calls and the `search_replace` / `write` edits of the Grok capture have the real argument shapes with
  neutral content, put into the real assistant line shape.
