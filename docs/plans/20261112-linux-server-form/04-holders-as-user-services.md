# 04 Holders as systemd user services

> Goal: on Linux, sessions outlive a daemon crash and a planned restart, under systemd and without it, as they already
> do on macOS and Windows.
> Completion criteria: every criterion below holds on Linux without systemd and under the systemd user service.

The holder, handover and companion implementations exist and were verified on macOS and Windows; what remains is the
Linux side: holders as systemd user units, and verifying the rest on Linux.

## Criteria

**Holders**

- Kill the daemon outright and start it again: every running session stays running and attachable, and its screen is
  rebuilt, repainted and marked inexact.
- The adopted sessions' hooks, MCP tools and git gate work with the new daemon; a hook report made while no daemon ran
  reaches the new one; a git gate call made while no daemon runs fails once its deadline passes.
- A holder of a build outside the compatible range is ended with its programs and its sessions are interrupted; one of
  another build within the range receives no new terminals and exits once its last terminal ends.
- A terminal the holder runs but the daemon has no record of is ended; a session no holder runs is interrupted.
- A holder that refuses connections or keeps dropping the link is not ended: its sessions stay running and claimed,
  shown as pending, and are taken over once it accepts a link again.
- A custom shell name is kept with the terminal identity.
- The daemon's crash hook does not reach programs a holder runs; a holder's own crash ends them; the daemon's crash
  table never holds a held terminal or companion.
- After a host shutdown and the next start, every session is interrupted, and its preview is its last screen from
  before the shutdown.

**Planned restart** (daemon started by hand, sessions writing output)

- Every session keeps running with its screen exactly as it was, except a screen whose width changed while a program
  showed the alternate screen, which is marked inexact by design; output written during the restart appears once; the
  daemon answers the queries in it.
- A plain stop still ends every session; after a planned-stop request the next daemon restores every screen exactly.
- Under the systemd user service, the successor is started by systemd and every session survives with its exact
  screen.

**Codex companion** (Codex 0.166.0 or later, holders installed)

- `codex app-server` is a child of the holder, inside the launch's containment, leading a session of its own; the
  terminal attaches with `--remote`; automatic messages go into Codex's queue.
- After `kill -9` of the daemon and a new start, a message to the adopted session goes into Codex's queue, and after a
  cancel the messages queued before the restart start as they would have; `octoboardd restart` the same, with the exact
  screen and the outbox delivered by the successor.
- Exiting Codex ends the server within a few seconds, leaving nothing of the launch; killing the server ends the
  terminal through its exit path, with or without a linked daemon.
- A holder whose welcome does not advertise companions runs the ordinary terminal launch; a server that does not come
  up leaves no companion and the terminal starts unbound.
- The companion's endpoint is a socket in a directory only the user can enter, made and removed by the holder.

## Technical design

- [ ] Each holder, when the daemon runs under a systemd user manager, runs as a systemd user unit of its own that kills
      only its main process when stopped, delegates its own cgroup subtree, and makes a group per launch inside it,
      falling back to the process ledger where no writable subtree is available.
- [ ] The daemon's unit is ordered to stop before the holders' units, so at a host shutdown the daemon makes its
      ordinary stop, freezing each preview, while the shells still run.
- [ ] A daemon not run by a systemd user manager detaches its holders as on macOS, with the process ledger alone.

## Implementation plan

- [ ] Verify on Linux without systemd: the Unix detach, a held launch joining the group the daemon made for it below
      the holder's cgroup, the crash-hook boundary, the planned restart and the companion criteria.
- [ ] Make holders user units under systemd, with the stop order.
- [ ] Verify under systemd in a container with systemd as PID 1 and a user manager: `Delegate=yes` and a group per
      launch, the stop order and the host-shutdown criterion, `octoboardd restart` with sessions writing output.

## Notes for the developer

- **Reusable capabilities**: the holder protocol and adoption, the planned-stop checkpoint and tail, the companion
  start and lifetime rules, `Containment::join_launch`, the per-launch cgroup groups, the process ledger.
- **Development notes**:
  - Leaving the restart to a service manager needs evidence: `SYSTEMD_EXEC_PID` equal to the daemon's pid with a
    well-formed `INVOCATION_ID`, or `--service-managed`, which the example unit passes; an inherited `INVOCATION_ID`
    alone does not count. A custom unit passing `--service-managed` must restart on status 75.
  - Under systemd the daemon exits with status 75 after a planned stop and the unit restarts it on that status; the
    sessions survive once the holders are outside the daemon unit's cgroup.
  - The successor's key reaches the owner through milestone 02.
  - Keep the per-launch cgroup handling independent of agents living inside the daemon's own service.
  - Lossless handover is bounded by the tail: more than 1 MiB written after the mark (2 MiB with a welcome pin) loses
    exact eligibility.
- **Reference docs**: `docs/product/application-lifecycle.md` ("Planned restart and planned stop", "Running as a
  systemd user service"), `apps/daemon/README.md`.
