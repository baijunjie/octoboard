> Severity: Moderate

# The environment capture leaves helpers started by the user's rc file running

## Symptom

A process that the user's interactive shell rc file starts and disowns keeps running, re-parented to pid 1, after the
daemon's login-shell environment capture has finished successfully.

## Reproduction steps

1. Use a temporary `HOME` whose `.zshrc` contains
   `zsh -dfc 'exec -a gitstatusd-standin sleep 100000' </dev/null >/dev/null 2>&1 &!`
   (the way powerlevel10k starts `gitstatusd`), with zsh as the login shell.
2. Start the daemon (`octoboardd`) with that `HOME`.
3. Trigger a launch that captures the environment snapshot, for example create a shell terminal for a project
   (`create_shell_terminal` over `/ws/control`).
4. Once the launch has succeeded, run `ps -o pid,ppid,command -ax | grep gitstatusd-standin`.

## Expected vs. actual

- Expected: once the capture has returned, nothing it started is left running; the only `gitstatusd-standin` is the
  one belonging to the shell terminal's own shell (position the user confirmed when asking for this ticket).
- Actual: two `gitstatusd-standin` processes run; the one started by the capture's shell has ppid 1 and stays until
  killed by hand.

## Environment

macOS (Darwin 25), zsh as the login shell, daemon built from `main` at 75dab0dd4. Needs an rc file that starts a
background helper detached from the shell's job table (`&!` / `disown`) with its streams off the capture's pipes.

## Scope of impact

Every user whose interactive rc starts such a helper (powerlevel10k users, async prompt frameworks). Each capture the
daemon runs leaves one more helper behind for as long as it lives; agent session launches capture the environment the
same way as shell terminals do. Workaround: kill the leftover processes by hand.

## Leads

- Verified: `apps/daemon/src/env_shell/snapshot.rs` spawns the capture shell in its own process group and kills that
  group only on a timeout; on success `wait_with_timeout` returns without killing it, by design, per the comments near
  `process_group(0)` and `wait_with_timeout`.
- Inferred: a helper disowned with `&!` is still in the capture shell's process group, so signalling that group after a
  successful capture would reach it; whether the same holds for helpers that call `setsid` is unknown.
- Unknown: whether a capture through bash or fish leaves helpers the same way.

## Acceptance criteria

- [ ] With the rc file in step 1, after a successful capture no process started by the capture's shell is left running.
- [ ] The shell terminal's (or agent session's) own shell still starts its helper normally.
- [ ] A capture that times out still kills everything it started, as today.
