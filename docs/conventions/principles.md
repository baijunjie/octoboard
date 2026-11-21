# General principles

The principles every convention document builds on. A case no convention writes down is judged by these; each document
opens with its own domain principles and states only the rules these do not already settle.

- **P1 Follow the surrounding code.** Naming, structure, comment density and error handling follow the habit of the
  code around the change, and a change clears out the dead code and stale comments it leaves behind: code that reads as
  one hand is cheaper to read and review.
- **P2 Reuse before writing.** Go through the shared entry point or component the structure doc lists for the job;
  where it cannot hold the case, extend it with a parameter or a kind, never a second implementation, and a missing
  one is added to the structure doc's list in the same change: two implementations drift apart.
- **P3 One source per fact.** A value is defined once and everything else derives from it; a copy another toolchain
  forces is pinned to its source by a check (a parity test, a case table or a typed record), so the copies cannot drift.
- **P4 Complete the set in the same change.** Whatever enumerates a kind of thing gets the new member in the change
  that adds it (a setting: its MCP tools, configuration export, search index and field versions; a code: its UI record
  and words; an account refusal reason: every picker; a region: F6): a member missed in one list is a silent gap.
- **P5 Agent differences live in the provider.** Generic code never branches on which agent it is
  ([provider-neutral core](provider-neutral-core.md)).
- **P6 Unknown is not "no".** An unrecognised value, an unreadable file or an unresolvable path takes the safe reading:
  it destroys nothing, offers nothing and counts nothing, since a guess can do damage a refusal cannot.
- **P7 Facts about outside programs come from real runs.** What an agent CLI, git, a daemon or WKWebView does is taken
  from a capture or a run against the real thing, never from memory or documentation alone.
- **P8 A failing check is fixed at its cause.** A check is never bypassed, loosened or retried until it passes; the
  failure is traced to what made it fail.
