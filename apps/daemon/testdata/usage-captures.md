# Native usage captures

Usage metadata extracted on 2026-10-15 from existing local CLI session records; no model call was made to produce these fixtures.

| Capture | Native source and version | Retained fields |
| --- | --- | --- |
| `usage_claude_2.1.296.jsonl` | Claude Code 2.1.296, version recorded on the original transcript entries | Five assistant content-block records with two message IDs and their complete native usage objects. IDs are replaced consistently. |
| `usage_codex_0.162.1.jsonl` | Codex 0.162.1, version in the rollout session metadata and installed `codex --version` | One `event_msg` / `token_count` with its complete `info` object. |
| `usage_grok_1.0.52.json` | Grok Build 1.0.52, installed binary and native session usage ledger | Complete native ledger; only `sessionId` is replaced. |
| `observed_claude.jsonl` | Claude Code 2.1.296 and 2.1.302, version on the original entries; installed `claude` is 2.1.302 | Four assistant records cut from real transcripts, in order: a main-transcript turn (2.1.296, `claude-opus-5`, effort `high`), a later main-transcript turn (2.1.302, `claude-opus-5-5`, `medium`), a sidechain turn from a `subagents/agent-*.jsonl` (2.1.302, `claude-sonnet-5-5`, `high`) and a main-transcript `<synthetic>` notice (2.1.302, no effort). Only `type`, `isSidechain`, `message.id` (replaced), `message.model`, the complete native `message.usage`, the top-level `effort` and `version` are kept. |
| `observed_codex_0.162.1.jsonl` | Codex 0.162.1, `cli_version` in the rollout session metadata and installed `codex --version` | Two `turn_context` records from two rollouts (`gpt-6.1-sol` with effort `medium`, then `codex-auto-review` with `low`); only `type`, `payload.model` and `payload.effort` are kept. |

The Grok model and effort replay through the existing `titles_grok_1.0.60_summary.json` capture (`current_model_id`, `reasoning_effort`).

Transcript content, file paths, session/request identities, timestamps and unrelated record metadata were omitted from the JSONL captures. Grok's ledger contains usage metadata only. Token values and nested usage structures are unchanged. When `usage_claude_2.1.296.jsonl` was captured, the latest available Claude transcript was produced by 2.1.296; the installed executable had since advanced to 2.1.300, which was not run against a model to create a new capture.

Claude repeats the same final per-message usage in multiple content-block records; the reader must deduplicate `message.id`. Its input excludes cache read/write input, so the normalized input adds those native categories once. Codex and Grok publish cumulative session totals directly; cache and reasoning values are subsets of those totals. The existing `rollout_running_turn.jsonl` capture is replayed as a native near-miss: `token_count` with `info: null` reports no known usage.
