# 04 Revert gate in the hook dispatcher

> Goal: with the revert gate on, git run inside a project session refuses a branch update that undoes merged content.
> Completion criteria: in a session's environment, a squash with a changed base onto a guarded branch is rejected with
> the gate's message, ordinary commits, rebases and merges pass, a project with its own gate installed is checked only
> by its own, turning the switch off lets the same update through without relaunching the session, and an error inside
> the gate lets the update through.

## Technical design

- [ ] When a session's git creates a branch, Octoboard records its target branch (the branch it was created from) and
      its fork point beside the responsible-session record.
- [ ] Resident guarded branches: the project's default branch; guarded branches are those plus every recorded target
      branch.
- [ ] The `reference-transaction` dispatcher, at the prepared stage, filters in the shell for a guarded branch moving
      before starting the gate, and rejects the transaction when the gate rejects.
- [ ] The gate is skipped when the project's effective revert gate setting is off, read at run time, or when the
      project has its own gate installed.
- [ ] Any failure to reach or run the gate lets the update through.

## Implementation plan

- [ ] Record target branch and fork point where branch creation is already reported.
- [ ] Add the prepared-stage call to the dispatcher, then the skip conditions, then the fail-open paths.
- [ ] Settle the Open point on recognizing a project's own gate.

## Notes for the developer

- **Reusable capabilities**: the hook dispatchers and the `GIT_CONFIG_COUNT` slot they are injected through; the
  branch-origin reporting and its state directory between the prepared and committed calls; the effective settings
  from 01.
- **Development notes**: the dispatcher already forwards to the user's and the project's own hooks; the gate runs
  alongside, never instead of them. Nothing is written to the repository's configuration.
- **Reference docs**: `docs/product/branch-responsible-session.md`, `docs/product/launching-agents.md`'s "The launch
  environment" section, `.githooks/hook.sh` (the shell-side filter it uses).
