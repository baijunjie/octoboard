# 01 Linux build and background run

> Goal: the daemon builds and runs on Linux and can be started, stopped and queried from the command line.
> Completion criteria: on a Linux machine without a display the daemon builds, starts detached on the port it was given
> and again on the same port after a restart, runs agent sessions, reports its status and address, and stops cleanly,
> leaving every session interrupted; macOS behavior is unchanged.

## Technical design

- [ ] The daemon and its agent adapters build and run on Linux.
- [ ] start / stop / status commands; start takes the port, detaches and prints the address.
- [x] The port file is written per user, under the daemon's data directory, not in the shared temporary directory.
- [ ] A running daemon is found through the single-instance mechanism.
- [ ] An example service definition for running it at boot as the owner's user; a user service runs only while that
      user has a login session unless lingering is enabled for the user, and nobody logs in on the server.
- [ ] Each agent launch in a cgroup v2 group of its own where the host allows it — the service definition delegating a
      subtree to the daemon (`Delegate=yes`) is the natural way — so that ending the launch removes the whole group and
      nothing it started can escape, however it detached. Where no writable subtree is available, the daemon keeps to
      the per-launch process ledger it uses on macOS (tag, lineage and session-id attribution), which already works
      on Linux; the cgroup is a strengthening on top of it, not a replacement.

## Implementation plan

- [ ] Bring the Linux build up and record what differs from macOS in launching agents and locating directories.
- [ ] Add the command surface on top of the existing lifecycle code.

## Notes for the developer

- **Reusable capabilities**: the single-instance lock, the port file, the optional parent-process watch, and the
  shell-environment snapshot used to launch agents.
- **Development notes**:
  - Anything macOS-specific surfaces here; fix it or record it, do not paper over it.
  - Agent processes are planned to move later under separate holder processes, each a systemd user service of its
    own, so that they outlive a daemon restart. Each holder's unit will then delegate its own cgroup subtree and make
    the per-launch groups inside it, so keep the per-launch cgroup handling from depending on agents living inside the
    daemon's own service.
- **Testing the cgroup groups**: the plain `docker run --init` container the Linux tests use has no systemd and a
  read-only `/sys/fs/cgroup`, so it only exercises the fallback to the ledger. Testing the groups themselves needs a
  container with a writable cgroup v2 subtree (`--privileged`, or a private cgroup namespace mounted read-write), or
  one with systemd as PID 1 and a user manager for the `Delegate=yes` path.
- **Reference docs**: `apps/daemon/README.md`, `docs/product/application-lifecycle.md`.

## Mid-task handoff

- **How far**: everything in this milestone is implemented: `--port` on the daemon, `start --port` / `stop` /
  `status` (`apps/daemon/src/cli/background.rs`), the owner record `daemon.owner` beside `daemon.lock` that the
  commands find a running daemon by, the example unit `apps/daemon/dist/systemd/octoboardd.service`, and per-launch
  cgroup v2 groups (`platform/proc/cgroup.rs`, used by the Unix `Containment` and the process ledger). On 2026-11-10,
  on macOS, real `octoboardd start --port`, `restart` and `stop` runs found the running daemon through its owner
  record, kept the port and ended every session on `stop`; `status` was not run there (evidence on the verification Mac:
  `/tmp/octoboard-manual-20261110-67e2/REPORT.md`, `/tmp/octoboard-gui-20261110-67e2/REPORT.md`). The scripted suites
  (`instance_lock`, `cli::`, `platform::proc`, `processes::ledger`, the layering tests) run at push; the Windows side
  of the change builds (a native `cargo build -p octoboardd --target x86_64-pc-windows-msvc` on the Windows VM
  succeeded on 2026-11-10), and `pnpm check:windows` runs at release.
- **Already observed on Linux** (Docker, aarch64, before three rounds of review fixes changed the code): start
  detached and surviving the closed login, status, refusal while running, stop leaving no record behind, restart on
  the same port, a taken port failing clearly; with a writable cgroup subtree, a launch's double-forked orphan listed
  and killed with its group; the full daemon suite passing in both a privileged and a plain container apart from four
  `accounts::shared_config` tests that fail only because the container runs as root.
- **Remaining, each a real run on Linux, blocked on Docker, which is off on the verification Mac** (each with
  `CARGO_BUILD_JOBS=2`, one at a time):
  - A plain `docker run --init` container: `cargo clippy -p octoboardd --all-targets -- -D warnings` and
    `cargo test -p octoboardd` (the cgroup tests skip; the ledger's fallback is exercised).
  - A `--privileged --cgroupns=private` container with cgroup2 mounted read-write: `cargo test -p octoboardd`
    (the contract tests `a_launch_s_group_ends_…`, `what_a_launch_s_group_spares_…`,
    `a_leftover_group_is_found_again_and_removed_once_empty` and the ledger test
    `ending_a_launch_removes_its_group_and_spares_what_was_kept`).
  - End to end with the built binary and a scratch `HOME`: start / status / stop / restart on the same port, an
    agent session run through it (a Codex session stops at its sign-in screen with no login), and after `stop` every
    session interrupted. This settles the command and single-instance items above
    with the build item.
  - Optional: a container with systemd as PID 1 and a user manager, running the example unit with `Delegate=yes`.
- **Trade-offs made**: shell terminals get no group (their background jobs outlive them by design); the group kill
  comes after the ledger's polite rounds and is skipped when a table read failed; a process the agent forked before
  it was moved into its group can escape the group (portable-pty has no pre-exec hook) and is left to the ledger's
  other rules; groups are made under a `Subtree` base, by default the starting process's own cgroup, with no check
  that it is delegated; `start` relays the handshake line, key included, to the owner's terminal; Windows has no
  `start` / `stop`. What remains of macOS-specific behavior: nothing found that breaks on Linux in locating
  directories; real agent CLIs' launches on Linux are what the end-to-end run above checks.
