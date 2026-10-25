# 03 Revert gate engine

> Goal: the revert gate's decision runs as a daemon subcommand, with the inputs it reads from git configuration today
> passed in instead.
> Completion criteria: tests in scratch repositories cover each rule of the gate (non-fast-forward with only unpushed
> commits dropped, aligning with a remote-tracking branch, a squash with a changed base, a conflict resolution keeping
> only the local side, a purely additive commit wiped out, a declared `Reverts:`), allowing and rejecting as
> `revert-gate.py` does; nothing calls it yet.

## Technical design

- [ ] A daemon subcommand taking the hook's input (stage and ref updates) and the guarded branches with each one's
      target branch and fork point, returning allow or reject with a message for the agent.
- [ ] The same rules and window as `revert-gate.py`, including the declared-revert exemption.

## Implementation plan

- [ ] Port the rules from `revert-gate.py`, whose header documents them, keeping its known gaps as they are.
- [ ] Write the tests from those rules first: the script has no test suite to migrate.

## Notes for the developer

- **Reusable capabilities**: the daemon's git invocation that drops inherited `GIT_CONFIG_*`; the existing
  `octoboardd` subcommand the hook dispatchers already start.
- **Development notes**: the decision must be fast on the common path, since it runs synchronously inside git.
- **Reference docs**: `.githooks/revert-gate.py`, `.githooks/hook.sh`.
