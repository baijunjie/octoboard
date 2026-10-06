# 01 Linux build and background run

> Goal: the daemon builds and runs on Linux and can be started, stopped and queried from the command line.
> Completion criteria: on a Linux machine the daemon builds, starts detached, runs agent sessions, reports its status and
> address, and stops cleanly; macOS behavior is unchanged.

## Technical design

- [ ] The daemon and its agent adapters build and run on Linux.
- [ ] start / stop / status commands; start detaches and prints the address.
- [ ] A running daemon is found through the single-instance mechanism.
- [ ] An example service definition for running it at login.

## Implementation plan

- [ ] Bring the Linux build up and record what differs from macOS in launching agents and locating directories.
- [ ] Add the command surface on top of the existing lifecycle code.

## Notes for the developer

- **Reusable capabilities**: the single-instance lock, the port file, the optional parent-process watch, and the
  shell-environment snapshot used to launch agents.
- **Development notes**: anything macOS-specific surfaces here; fix it or record it, do not paper over it.
- **Reference docs**: `apps/daemon/README.md`, `docs/product/application-lifecycle.md`.
