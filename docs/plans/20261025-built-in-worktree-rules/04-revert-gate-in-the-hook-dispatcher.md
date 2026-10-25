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

## Handoff

- The gate is `octoboardd revert-gate <state> [--guard <branch>]... [--target <branch> <target-branch>]...
  [--fork-point <branch> <commit>]...`, with the hook's ref update lines on stdin: `--guard` names a resident guarded
  branch, each `--target` names a branch and its recorded target branch (every target named is guarded), each
  `--fork-point` a branch and its recorded fork point. It exits 3 to reject, with the message for the agent on stderr,
  and 0 otherwise, including when it fails itself; treat only 3 as a rejection. Only `prepared` is checked.
  `REVERT_GATE_SKIP=1` skips it, as in the per-clone gate.
- Only the `reference-transaction` path was ported: the per-clone gate's `pre-push` check of a branch pushed for a PR
  against its target branch's remote tip has no built-in counterpart.
- Each `git` it runs goes through the daemon's bounded run, which adds about 10 ms of polling per call (around 0.1 s
  for a rejecting move in a small repository); measure it on a real repository when wiring it in.

## Notes for the developer

- **Reusable capabilities**: the hook dispatchers and the `GIT_CONFIG_COUNT` slot they are injected through; the
  branch-origin reporting and its state directory between the prepared and committed calls; the effective settings
  from 01.
- **Development notes**: the dispatcher already forwards to the user's and the project's own hooks; the gate runs
  alongside, never instead of them. Nothing is written to the repository's configuration.
- **Reference docs**: `docs/product/branch-responsible-session.md`, `docs/product/launching-agents.md`'s "The launch
  environment" section, `.githooks/hook.sh` (the shell-side filter it uses).
