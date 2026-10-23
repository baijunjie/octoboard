# Title captures

Records from which an agent's own title for a conversation is read, taken from sessions driven by hand, with nothing personal kept: on 2026-10-16 in scratch directories unless a row gives another date.

| Capture | Native source and version | Retained |
| --- | --- | --- |
| `titles_claude_2.1.301.jsonl` | Claude Code 2.1.301 transcript | The `mode`, `last-prompt` and `ai-title` lines of the session, with the session and leaf ids replaced. The `custom-title` line is not captured: it has the shape Claude Code writes for a rename (`customTitle`, `sessionId`) with a made-up title. |
| `titles_claude_2.1.308_command.jsonl` | Claude Code 2.1.308 transcript, taken on 2026-10-22 | The first five `user` lines of a session that ran `/clear` and then a plugin skill's slash command, which Claude Code gives no `ai-title`: the local-command caveat, the `/clear` line (no `promptId`), the command as typed, the skill body Claude Code adds as a meta line, and the first tool result. Ids, paths and the skill's name are replaced; the skill body and the tool output are reduced to placeholders, and fields that carry nothing for title reading are dropped from the tool result line. |
| `titles_codex_0.163.0_rollout.jsonl` | Codex 0.163.0 rollout | The session metadata reduced to its id and version, the two user-role response items (the injected `AGENTS.md` and environment context reduced to placeholders, and the typed prompt), and the `item_completed` event of the `UserMessage` item. Codex 0.163.0 writes no `user_message` event. |
| `titles_codex_0.142.5_rollout.jsonl` | Codex 0.142.5 rollout | The `user_message` event older Codex writes, with the shape of a real one and a made-up message. |
| `titles_codex_0.163.0_index.jsonl` | Codex 0.163.0 `session_index.jsonl` | Two lines of named threads. The third line, renaming the first thread, is made up in the shape `/rename` appends. |
| `titles_grok_1.0.60_summary.json` | Grok Build 1.0.60, the installed binary when the session was run | The session's `summary.json` with the identities, request ids and paths replaced. |

The Claude `last-prompt` line and the Codex response items are the near-misses: they carry the prompt, yet are not where a title is read from. In the Claude command capture, the `/clear` line is written in the same tags as the skill command, yet is a local command and not a prompt.
