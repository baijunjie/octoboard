# 01 Linux build and command verification

> Goal: the daemon builds on Linux, and what is already built for the server form is verified there.
> Completion criteria: on a Linux machine without a display, the daemon builds and its suite and clippy pass; it starts
> detached on the port it was given and again on the same port after a restart, runs an agent session, reports its
> status and address, refuses a second start while running, fails clearly on a taken port, and stops cleanly, leaving
> every session interrupted; with a writable cgroup subtree, a launch's group is removed with everything in it when the
> launch ends; macOS behavior is unchanged.

## Technical design

- [ ] The Linux build compiles (the per-launch cgroup group name does not compile on Linux today).
- [ ] The daemon suite and clippy pass on Linux, with the cgroup tests skipped where no writable subtree exists and the
      ledger fallback exercised.
- [ ] The cgroup contract tests pass with a writable cgroup v2 subtree.
- [ ] start / status / stop / restart, the single-instance refusal and the stable port work end to end with the built
      binary.
- [ ] An agent session runs through a daemon started this way, and `stop` leaves it interrupted.

## Implementation plan

- [ ] Reapply the compilation fix and build.
- [ ] Rerun `holder::server::tests::input_reaches_the_program_and_a_resize_reaches_its_terminal` first; fix it if it
      still fails.
- [ ] Plain `docker run --init` container: `cargo clippy -p octoboardd --all-targets -- -D warnings` and
      `cargo test -p octoboardd`.
- [ ] `--privileged --cgroupns=private` container with cgroup2 mounted read-write: `cargo test -p octoboardd`,
      covering `a_launch_s_group_ends_…`, `what_a_launch_s_group_spares_…`,
      `a_leftover_group_is_found_again_and_removed_once_empty` and the ledger test
      `ending_a_launch_removes_its_group_and_spares_what_was_kept`.
- [ ] End to end with the built binary and a scratch `HOME`: start / status / stop / restart on the same port, a
      second start refused, a taken port, an agent session (a Codex session stops at its sign-in screen with no
      login), and every session interrupted after `stop`.

## Notes for the developer

- **Reusable capabilities**: the single-instance lock and owner record, the port file, the optional parent-process
  watch, the shell-environment snapshot used to launch agents, the process ledger, the per-launch cgroup groups.
- **Development notes**:
  - Already built, and verified on macOS: `--port`, `start --port` / `stop` / `status` / `restart`, the owner record
    beside the single-instance lock, the per-user port file, the example systemd unit, the per-launch cgroup groups,
    and the process holders. On Linux they were observed only on an earlier scratch build.
  - Run Linux builds one at a time with `CARGO_BUILD_JOBS=2`.
  - The plain container has no systemd and a read-only `/sys/fs/cgroup`, so it only exercises the ledger fallback.
  - Four `accounts::shared_config` tests fail only when the container runs as root; run as a normal user or account
    for it.
  - Anything macOS-specific that surfaces is fixed or recorded, not papered over. Nothing found so far breaks on Linux
    in locating directories; real agent CLIs' launches are what the end-to-end run checks.
  - Settled trade-offs of the built code, not to be reopened here: shell terminals get no group (their background jobs
    outlive them by design); the group kill comes after the ledger's polite rounds and is skipped when a table read
    failed; a process an agent forks before it is moved into its group can escape it (portable-pty has no pre-exec
    hook) and is left to the ledger; groups are made under the starting process's own cgroup by default, with no check
    that it is delegated; Windows has no `start` / `stop`.
- **Reference docs**: `apps/daemon/README.md`, `docs/product/application-lifecycle.md` ("The daemon on its own").
