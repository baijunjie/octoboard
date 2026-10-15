# Native usage captures

Usage metadata extracted on 2026-10-15 from existing local CLI session records; no model call was made to produce these fixtures.

| Capture | Native source and version | Retained fields |
| --- | --- | --- |
| `usage_claude_2.1.296.jsonl` | Claude Code 2.1.296, version recorded on the original transcript entries | Five assistant content-block records with two message IDs and their complete native usage objects. IDs are replaced consistently. |
| `usage_codex_0.162.1.jsonl` | Codex 0.162.1, version in the rollout session metadata and installed `codex --version` | One `event_msg` / `token_count` with its complete `info` object. |
| `usage_grok_1.0.52.json` | Grok Build 1.0.52, installed binary and native session usage ledger | Complete native ledger; only `sessionId` is replaced. |

Transcript content, file paths, session/request identities, timestamps and unrelated record metadata were omitted from the JSONL captures. Grok's ledger contains usage metadata only. Token values and nested usage structures are unchanged. The latest available Claude transcript was produced by 2.1.296; the installed executable had since advanced to 2.1.300, which was not run against a model to create a new capture.

Claude repeats the same final per-message usage in multiple content-block records; the reader must deduplicate `message.id`. Its input excludes cache read/write input, so the normalized input adds those native categories once. Codex and Grok publish cumulative session totals directly; cache and reasoning values are subsets of those totals. The existing `rollout_running_turn.jsonl` capture is replayed as a native near-miss: `token_count` with `info: null` reports no known usage.
