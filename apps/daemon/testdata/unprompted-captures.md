# Records written with no prompt sent

What each agent's CLI writes of a session before the user has sent it a prompt, for the tests of whether a record holds
a prompt of the user's (`src/records/user_prompt.rs`, `RecordStore::is_user_prompt`). Each CLI was started on a
pseudo-terminal in a scratch home of its own, with a fake API key so that no account was used, left at its input box
and ended. No model turn was taken: the one request a capture shows was refused for its key.

Each file is what the CLI wrote, scrubbed: ids, paths, timestamps and request ids are replaced with neutral ones, and
the text Grok Build wrote (its system prompt, its reminder) with neutral text. The keys, the nesting and every value a
reader tells a line by (`type`, `isMeta`, `promptId`, `synthetic_reason`, the tags a line's text opens with) are the
real ones.

| Capture | Source version | What was done | What it shows |
| --- | --- | --- | --- |
| `unprompted_claude_2.1.330_clear.jsonl` | Claude Code 2.1.330 | `/clear` typed into a session never prompted | The transcript Claude Code starts for the new conversation: the caveat line (`isMeta`), the `/clear` command line with no `promptId`, and a `system` line for its output. No line is a prompt. |
| `unprompted_claude_2.1.330_shell.jsonl` | Claude Code 2.1.330 | `!echo probe` typed into a session never prompted | The `<bash-input>` and `<bash-stdout>` user lines, both with a `promptId`, and the stand-in reply to the request Claude Code then made (refused, the key being fake). The lines between them (four `attachment` lines and the four `api-request` lines holding the request) are left out. This version asks the model about shell input; whether one that carries a `promptId` is a prompt cannot be told from the line. |
| `unprompted_grok_1.0.92_chat_history.jsonl` | Grok Build 1.0.92 | Started with `--session-id`, nothing typed | The `chat_history.jsonl` Grok writes on launch: its system prompt and one reminder marked `synthetic_reason: system_reminder`. |

Measured in the same runs, with nothing to capture:

- **Claude Code 2.1.330 writes no transcript until something is typed.** Started with `--session-id` and left at its
  input box, its config directory holds no `projects/` directory at all.
- **Codex 0.167.1 writes no rollout until the first prompt.** Left at its input box, its home holds no `sessions/`
  directory. So no capture of a Codex rollout with no prompt exists: the tests take the lines a rollout opens with
  (the session's metadata and the instructions Codex adds) from `titles_codex_0.163.0_rollout.jsonl`.
- **Grok Build 1.0.92 writes no `usage.json` before the first prompt.** The session directory holds
  `chat_history.jsonl`, `summary.json`, `system_prompt.txt`, `prompt_context.json`, `events.jsonl` and
  `title_refresh_idx`, and not the marker Octoboard finds a session directory by, so the directory of a session never
  prompted is not found.
