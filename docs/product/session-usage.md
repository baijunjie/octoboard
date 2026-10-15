# Session usage

## Native session usage

Focus mode's live session cards, its archive preview rows and the full archive view show the session's token total
only when its agent has recorded a positive total. The visible total is a compact, localized number without a unit
label; its tooltip and accessible description identify it as token usage and provide the exact total, input and
output counts, plus cache-read and cache-write input counts when available. A recorded zero remains valid saved
usage, but zero and missing usage both omit the indicator and its usage-specific separator.

These are native token counts, not a price estimate, context-window size or elapsed duration. Input includes cached
input; the separate cache counts are subsets of it. The agent's recorded total is authoritative when it provides
one, and cache counts are never added to that total again. Different agents' accounting can differ:

- **Claude Code**: input and output usage across the session and its own native subagents, counting a shared native
  message only once. Input includes ordinary input, cache reads and cache writes; total is input plus output.
- **Codex**: the session's native cumulative token usage, including its recorded total and cached-input count.
- **Grok Build**: the native session usage ledger, including inherited history when that ledger includes it.

Usage refreshes approximately once a second while the agent process is running, subject to when the agent writes
its record. A final refresh follows the process stopping. Previously saved usage remains available after archiving
and restarting Octoboard. Older sessions can acquire usage when their native records are still available; missing
or unreadable records do not create a zero estimate or erase the last saved counts. Switching accounts while keeping
the same native conversation preserves its usage; moving to a different native conversation clears the previous
conversation's counts until the new one supplies usage.

For where the live-card total appears, see "What both views show" in `docs/product/focus-mode.md`. Archive dates and
the archive metadata block are described in "The archive view" in `docs/product/sidebar.md`.
