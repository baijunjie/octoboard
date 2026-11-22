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
- **P9 Leave the user's own system as it was.** What Octoboard gives an agent (hooks, its MCP server, the role text,
  settings) reaches it per launch, through an argument, an environment variable or a file Octoboard keeps in its own
  directories, never by editing a file the user or an agent keeps and never as a hook left in the user's settings. A
  write has one of two sources: the application's own code, or an agent doing what the user told it; the application
  never drives an agent into a write (a background run of a CLI that changes its configuration, a confirmation answered
  for the user that the user did not agree to in the window; the trust confirmation for a directory of Octoboard's own,
  a console's, is Octoboard's to answer). What an agent writes for itself (its records and caches, a
  credential at a login, the trust it records when its confirmation is answered) is the agent's. What is done inside a
  project the user added and its repository (its files, git operations, branches and worktrees) is development work
  and outside this rule. The application's own
  writes outside its own directories (the user's files, system settings, a remote host) are forbidden
  unless the write is a registered exception, which the product owner approves one by one and which is all of: needed by
  a capability that cannot be had without it; agreed to by the user in the window, which says what is changed and where
  (a mention in the docs is not agreement); undone when the feature is turned off, except where undoing would lose the
  user's data; visible in the application, which lists the exceptions in effect and has one action that clears them
  all, for the moment before an uninstall and nothing else (a single one is undone at its own feature's switch, never
  there); and listed in the product docs
  (`docs/product/launching-agents.md`, "What Octoboard never modifies"). The registered-writes check
  (`scripts/verify/registered-writes.mjs`, part of `pnpm verify`) holds the application's own code to this: a write
  call outside a file its crate's registration names fails it. What the clearing and the uninstall restore is only what
  exists for Octoboard alone and would otherwise be left as a dead setting, a dead file or a side effect once Octoboard
  is gone. Nothing relies on an uninstall hook: once the
  application is deleted, only static files in its own directories may remain.
