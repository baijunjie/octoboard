# 03 Scoped verification

> Goal: a verification runs only the checks the branch's changes can affect, in a fail-fast order, and a tree that
> has passed is not verified again.
> Completion criteria: one command verifies a branch against its target branch; a UI-only branch runs no Rust step, a
> docs-only branch runs only lint and format checks, and a branch touching shared configuration runs everything; a
> second run on the same tree with the same lockfile and toolchain reports the earlier pass instead of re-running;
> the full suite still runs on request; the workflow text agents follow points to the command.

## Technical design

- [ ] Scope rules, from the paths changed since the merge base with the target branch:
  - Rust steps: Rust sources, Cargo manifests and lockfile, the toolchain file, the daemon's test data, the desktop
    shell, the shared app configuration.
  - Desktop tests: the desktop shell, root Cargo files, the shared app configuration, the toolchain file (the desktop
    crate does not depend on the daemon crate).
  - Web type check: the web app, the UI language matcher it imports, the shared app configuration, the lockfile.
  - UI type check and tests: the UI package, the lockfile, the shared app configuration.
  - Documentation only: lint and format checks only.
- [ ] Order: format, lint and type checks first; tests and gallery sweep after; stop at the first failure.
- [ ] Verify-once stamp: the commit's tree, the lockfile and the toolchain identify a pass; a matching stamp skips the
      run.
- [ ] A flag to force the full suite.

## Implementation plan

- [ ] Write the command and its scope rules.
- [ ] Point the verification steps agents follow (pre-merge verification, delivery static checks, the change-check
      loop's re-runs, the gallery writer's baseline) at the command, per the overview's "Open" item on where that text
      lives.

## Notes for the developer

- **Reusable capabilities**: the flow scripts under `.githooks/` and the target branch and fork point each worktree
  branch records.
- **Development notes**: keep the gallery sweep to the touched scenarios, as the gallery skill already does.
- **Reference docs**: `.claude/skills/git-worktree/SKILL.md` "Merging", `CLAUDE.md` "Before delivery".
