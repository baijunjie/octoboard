# 04 Final confirmation

> Goal: confirm, in real sessions launched by a build of this branch, what the unit tests could not reach.
> Completion criteria: every item below has been observed in a real session (or fixed where it turned out wrong), on the
> host it names.

The implementation and its unit tests are complete; these items need a real launch of the daemon and the agents.

- [ ] macOS, Claude Code: with the switch on, a newly launched project session's `Edit`/`Write` into another
      registered project is refused with the edit-layer reason, while its own project, its own repository's linked
      worktrees and a scratch or temp path pass; with the switch off, nothing is injected.
- [ ] macOS, Codex: the same for `apply_patch`; a remote resume (private app server) raises no hook-review prompt for
      the guard.
- [ ] macOS, Grok Build: the same for its edit tools, through the hooks file in the per-session Grok home.
- [ ] Windows, Codex and Grok Build: the per-session `guard-hook.cmd` runs through PowerShell (Codex) and as a bare path
      (Grok), refuses an edit into another project, and a project name or path with non-ASCII characters reaches the
      model intact.
- [ ] Windows: the guard script and list are readable only by their owner, as `docs/product/files-octoboard-owns.md`
      states.
- [ ] Git layer in a real session: a commit on an existing branch in another registered project's repository is
      refused with the git-layer message naming the working tree; the session's own repository, an unregistered
      temporary repository and `REVERT_GATE_SKIP=1` behave as `docs/product/keeping-sessions-to-their-own-project.md`
      says; turning the switch off lets the next commit through in the same session.
- [ ] A project on an unreachable mount does not hold up a launch beyond the 5 s look, and a failure to write the list
      launches the session unguarded with a log line.
