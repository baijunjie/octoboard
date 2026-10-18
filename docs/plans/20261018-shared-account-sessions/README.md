# Complete account migration

## Problem

Moving a session to another account ("Switch account") copies that conversation's record, and what the agent keeps
beside it, from the old account's config directory into the new one, then resumes it there. What is left:

- **Codex history segments are left behind.** A thread's later history segments (`rollout-…-<id>_<x>.jsonl[.zst]`,
  written when a turn is reverted, each continuing the one before; the last segment of the chain is the conversation's
  head) are not carried, so a switch of a reverted thread resumes a stale branch, and Octoboard's record lookup reads
  the stale first file for usage and titles.
- **The old account keeps a stale copy**, which its own terminal resume picker still lists.

## Plan outline

1. Make the existing migration complete and correct.
2. Measure whether accounts can share session directories. The measurements showed sharing unsafe for Codex and Claude
   Code, and Grok Build unguarded against two homes running one session (recorded in `docs/agent-cli-reference.md`,
   "Sharing a session directory between accounts"); the user dropped sharing, and switching keeps copying.
3. Carry and read Codex's whole history-segment chain, which the measurements showed the migration still misses.

## Milestones

1. 01 Complete account migration (closed)
2. 02 Measure cross-account sharing (closed)
3. [02a Carry Codex history segments](02a-carry-codex-history-segments.md)

## Open

- Whether the stale copy left in the old account after a switch is removed, kept, or left to the user.
