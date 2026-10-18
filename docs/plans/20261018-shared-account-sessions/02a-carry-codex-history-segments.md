# 02a Carry Codex history segments

> Goal: an account switch of a Codex session carries the thread's whole history-segment chain, so the target resumes
> the current conversation, and Octoboard's record lookup reads the chain's head.
> Completion criteria: daemon tests cover a reverted thread (first rollout plus later segments, plain and compressed)
> being located by its head, read for usage and titles from the head, carried whole by a switch, and moved to the Trash
> whole; a real Codex thread with a revert, copied into scratch homes, switches through the switch code and resumes the
> current branch (its newest turn present, the reverted turn absent).

## Implementation plan

- [ ] Record lookup, the record index, usage, titles, presence and trash-on-delete treat `rollout-…-<id>.jsonl[.zst]`
      and its later segments `rollout-…-<id>_<x>.jsonl[.zst]` as one thread whose head is the last segment of the chain
      (its leaf, not the newest by mtime) (each segment carries `history_mode: "paginated"` and a `history_base` naming
      the file it continues from; follow the chain rather than trusting names alone where it matters).
- [ ] The switch carries every segment of the chain with the first rollout, through the same staged copy; a target
      holding only part of the chain is refused by Codex (`invalid paginated history lineage … missing source rollout`),
      and a target holding the first file alone silently resumes a stale branch.
- [ ] A target whose own database row names an older head (the thread was switched there before, then reverted
      elsewhere): by the measurement in `docs/agent-cli-reference.md` ("Sharing a session directory between accounts", a
      home whose row names an older segment follows its row), it is expected to resume the stale head even with the
      whole chain copied. Confirm by a real resume; if confirmed, decide with the user how the switch handles that row.
- [ ] Correct the statements that call these files subagent rollouts or say the switch carries the first file alone:
      `docs/product/switching-session-accounts.md` and its `docs/README.md` index entry, the daemon's relocate comments
      and tests, `docs/memory/probing-agent-clis.md`'s example (via memory-writer), and in `docs/agent-cli-reference.md`
      "Codex, history segments" the sentences on Octoboard's switch and on the earlier misreading.

## Notes for the developer

- **Measured facts**: `docs/agent-cli-reference.md`, "Codex, history segments" and "Sharing a session directory between
  accounts" (real subagents are named by their own id and never match the parent's).
- **Reusable capabilities**: the account switch's companion listing and staged copy (`apps/daemon/src/relocate.rs`), the
  zstd-aware record opener (`line_file::open`), the agent record index.
