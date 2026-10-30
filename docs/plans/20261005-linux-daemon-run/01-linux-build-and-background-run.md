# 01 Linux build and background run

> Goal: the daemon builds and runs on Linux and can be started, stopped and queried from the command line.
> Completion criteria: on a Linux machine without a display the daemon builds, starts detached on the port it was given
> and again on the same port after a restart, runs agent sessions, reports its status and address, and stops cleanly,
> leaving every session interrupted; macOS behavior is unchanged.

## Technical design

- [ ] The daemon and its agent adapters build and run on Linux.
- [ ] start / stop / status commands; start takes the port, detaches and prints the address.
- [ ] The port file is written per user, under the daemon's data directory, not in the shared temporary directory.
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
- **Development notes**: anything macOS-specific surfaces here; fix it or record it, do not paper over it.
- **Testing the cgroup groups**: the plain `docker run --init` container the Linux tests use has no systemd and a
  read-only `/sys/fs/cgroup`, so it only exercises the fallback to the ledger. Testing the groups themselves needs a
  container with a writable cgroup v2 subtree (`--privileged`, or a private cgroup namespace mounted read-write), or
  one with systemd as PID 1 and a user manager for the `Delegate=yes` path.
- **Reference docs**: `apps/daemon/README.md`, `docs/product/application-lifecycle.md`.
