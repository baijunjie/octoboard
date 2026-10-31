# 01 Process holders

> Goal: agent processes and shell terminals survive the daemon going down, and keep working with the daemon that
> starts next.
> Completion criteria:
> - Kill the daemon outright and start it again. Every running session stays running and attachable, and its screen
>   is rebuilt, repainted and marked inexact.
> - The adopted sessions' hooks, MCP tools and git gate work with the new daemon.
> - A terminal the holder runs but the daemon has no record of is ended.
> - A session no holder runs is interrupted.
> - After a Linux host shutdown and the next start, every session is interrupted, and its preview is its last screen
>   from before the shutdown.

## Technical design

- [ ] A holder process, a mode of the daemon binary:
  - It owns the PTYs and is the parent of every program it starts.
  - It reads output and forwards it, keeps a raw tail per terminal, writes input, resizes and signals.
  - It does not parse output.
  - Each program's input is written separately, so a program that stops reading blocks only itself.
- [ ] A local socket per holder and a small framed protocol between daemon and holder:
  - From the daemon: spawn, input, resize, signal and release.
  - From the holder: spawned, spawn failed, output, exited and its tails.
  - A program's exit is reported only after its last output has been forwarded.
- [ ] The handshake on connecting:
  - The holder answers with an instance id, its build, how it last parted from a daemon (handover or crash) and the
    terminals it runs.
  - Then it sends one tail per terminal.
  - A newer daemon connection takes the holder over from an older one.
- [ ] Adoption on start:
  - Terminals are matched with their sessions and shell terminals.
  - Terminals with no record are ended.
  - Sessions with no terminal are interrupted.
- [ ] After a crash, each screen is rebuilt from its tail and marked inexact. The program is resized by one row and
      back so a full-screen program repaints.
- [ ] A spawn the holder has not confirmed stays claimed and unavailable, and is never started a second time on a
      guess. A late failure clears the claim, and the next start reconciles it against the holder's list.
- [ ] A holder whose build differs from the daemon's receives no new terminals and exits when its last one ends. New
      terminals go to a holder of the current build.
- [ ] Holders are detached from the daemon:
  - On macOS, in a session of their own.
  - On Linux, as a systemd user service that kills only its main process when stopped. The daemon's unit is ordered
    to stop before the holders' units, so at a host shutdown the daemon makes its ordinary stop, freezing each
    preview, while the shells are still running.
  - Either way, a holder outlives the daemon's process group.
- [ ] Hooks, the MCP bridge and the git gate of an adopted session reach the new daemon. How is open (see the
      overview).

## Implementation plan

- [ ] Move PTY ownership behind the holder protocol, with the daemon still ending everything on exit, so behavior is
      unchanged.
- [ ] Make the helpers' way of reaching the daemon survive a restart.
- [ ] Then let holders outlive the daemon, and add adoption and crash recovery.
- [ ] Then add the build check and draining.

## Notes for the developer

- **Reusable capabilities**:
  - The launch tag in each launch's environment.
- **Development notes**:
  - Builds on the daemon's first paint on attach, which is planned separately and has to exist first.
  - The Linux detach and the stop order, with the Linux shutdown criterion, need the daemon running on Linux as a
    systemd user service, which does not exist yet. Until then this milestone can be done and verified on macOS alone.
  - The saved process records are used at start to stop what the previous run left behind. That cleanup has to leave
    alone the launches of adopted sessions.
  - This changes the start-up rules that no process from the previous run is adopted and that sessions' scratch space
    is cleared. It also changes "Agent processes do not outlive the daemon" and the same statement for shell
    terminals. The application-lifecycle, background-process and shell-terminal product docs change with it.
- **Reference docs**: `docs/architecture.md` ("Why the daemon is split out"),
  `docs/product/application-lifecycle.md` ("Starting up", "Crashes and forced termination"),
  `docs/product/background-processes.md`, `docs/product/shell-terminals.md`, `docs/product/launching-agents.md`.
