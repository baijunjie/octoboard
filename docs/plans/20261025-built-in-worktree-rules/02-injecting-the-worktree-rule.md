# 02 Injecting the worktree rule

> Goal: a project session launched with worktrees on is told the default worktree rule, shaped by the project's
> effective settings.
> Completion criteria: role-text tests cover worktrees on and off, each delivery value and both rebase values, for
> bound and unbound project sessions and for all three agents, and show console sessions unchanged; a real session in a
> scratch repository creates its worktree under `.worktrees/`, and the branch it recreates from its remote branch after
> a PR is checked for being recorded as its branch.

## Handoff

- The settings tools' text (`worktrees_schema` and the `update_project` / `update_settings` descriptions in
  `apps/daemon/src/mcp/mod.rs`) does not yet say when a worktree setting change takes effect, and `update_project`'s
  generic sentence (a change reaches sessions opened or resumed afterwards) is wrong for the rule, which reaches only
  sessions begun after it: say so for use worktrees, rebase before merging and delivery.

## Technical design

- [ ] The rule text from the overview's five points, opening with the project's own rules taking precedence; rebase
      and delivery change only the sentences that name them.
- [ ] The role description takes the project's effective worktree settings; with worktrees off, no rule is added.
- [ ] Console sessions get no rule.

## Implementation plan

- [ ] Resolve the project's effective settings when the launch is assembled and pass them to the role description
      every agent's adapter already builds.
- [ ] Settle the Open point on recreated branches; if the branch is not recorded, adjust the rule's wording and report
      the gap.

## Notes for the developer

- **Reusable capabilities**: the per-agent qualified tool names in the role text; the responsible-session record of
  branches a session's git creates, and the cleanup bound to it.
- **Development notes**: the role text holds for a session's whole life (recorded once by Claude Code and Grok), so
  the rule must stand without later corrections; keep it short.
- **Reference docs**: `docs/product/launching-agents.md`, `docs/product/branch-responsible-session.md`,
  `docs/product/branch-and-worktree-cleanup.md`, the `git-worktree` skill of this repository (the source of the
  cleanup rules).
