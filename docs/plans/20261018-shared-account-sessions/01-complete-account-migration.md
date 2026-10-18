# 01 Complete account migration

> Goal: switching a session's account carries the whole conversation for every agent, including compressed Codex
> rollouts, and Octoboard's record index finds compressed rollouts too.
> Completion criteria: daemon tests cover a compressed Codex rollout being located, indexed and switched, Claude Code
> sidecars and rewind checkpoints being carried, and Codex subagent rollouts being carried; a real Codex session idle
> past the compression age switches and resumes.

## Implementation plan

- [ ] Record lookup and the record index recognise `rollout-…-<id>.jsonl.zst` alongside `.jsonl` for Codex; usage,
      titles and trash-on-delete see compressed rollouts.
- [ ] The switch copies a compressed rollout as it is (Codex resumes it from its own database's recorded path —
      verify which path Codex records after compression and that resume under the target home succeeds).
- [ ] The switch carries Claude Code's `projects/<slug>/<id>/` directory and `file-history/<id>/` with the transcript.
- [ ] The switch carries Codex's subagent rollouts (`rollout-…-<parent>_<child>.jsonl`, compressed or not).
- [ ] All carried items go through the existing staged, size-verified copy, and roll back together with it.

## Notes for the developer

- **Reusable capabilities**: the daemon's relocation step used by the account switch (staging directory, per-file
  verification, same-file no-op); the agent record index.
- **Development notes**: the copy stays a copy (the original remains in the old account); the old-account stale copy
  question is in the overview's "Open".
- **Reference docs**: `docs/agent-cli-reference.md` ("Moving a conversation to another config directory"),
  `docs/product/sessions.md` ("Switching a session's account").
