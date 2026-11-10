# 01a Linux holders and restart verification

> Goal: verify Linux holders, launch containment, restart handover and Codex companion lifetime before the Linux form is released.
> Completion criteria: the Linux portions of the requirements below are implemented and verified, including the systemd service cases and their prerequisites.

## Dependencies and current scope

- Depends on milestone 01's Linux build, command surface and launch cgroup support. The holder, handover and companion implementations already exist; Linux verification remains outstanding.
- Blocked for the systemd cases until the daemon runs as a systemd user service, each holder has its own delegated user unit, and the service successor's key can reach clients without being journalled or put in its owner record, arguments or environment.
- Linux verification is deferred by the user because Linux is not released. No container, test suite, clippy run or hands-on verification is to be resumed as part of this documentation move.
- Mixed-platform wording is preserved verbatim below; only its Linux portion belongs here. The macOS and Windows portions remain in the runtime-hardening plan. References in transferred text to holder/systemd prerequisites mean the requirements collected in this milestone. In particular, "milestone 02's systemd parts" denotes the transferred holder-service requirement, not this topic's UI milestone 02.

## Holder requirements

> Completion criteria:
> - Kill the daemon outright and start it again. Every running session stays running and attachable, and its screen
>   is rebuilt, repainted and marked inexact.
> - The adopted sessions' hooks, MCP tools and git gate work with the new daemon, and a hook report made while no
>   daemon was running reaches the new one.
> - A holder of a build outside the compatible range is not adopted: it is ended with its programs, and its sessions
>   are interrupted.
> - A holder of another build within the range receives no new terminals and exits once its last terminal ends.
> - A git gate call made while no daemon runs fails once its deadline passes.
> - A terminal the holder runs but the daemon has no record of is ended.
> - A session no holder runs is interrupted.
> - A holder that runs but refuses connections or keeps dropping the link is not ended: its sessions stay running and
>   claimed, shown as pending, and are taken over once it accepts a link again.
> - A custom shell name is kept with the terminal identity.
> - After a Linux host shutdown and the next start, every session is interrupted, and its preview is its last screen
>   from before the shutdown.

### Technical design

- [ ] On Linux, as a systemd user service that kills only its main process when stopped. The unit delegates its
        cgroup subtree, and the holder makes a group per launch inside it, falling back to the process ledger where no
        writable subtree is available. The daemon's unit is ordered to stop before the holders' units, so at a host
        shutdown the daemon makes its ordinary stop, freezing each preview, while the shells are still running. A
        daemon not run by a systemd user manager detaches its holders as on macOS, with the process ledger alone.
- [ ] Linux and Windows: verify the same crash-hook boundary.

The transferred Linux crash-hook boundary is: the daemon's crash hook no longer reaches programs a holder runs; a holder's own crash still ends them. The Windows portion stays with the Windows verification.

### Implementation plan

- [ ] The same on Linux and Windows, with the crash hook (see the open technical-design items).

### Development notes

- The Linux detach and the stop order, with the Linux shutdown criterion, need the daemon running on Linux as a
    systemd user service, which does not exist yet. The rest can be done and verified on macOS, on Linux without
    systemd once the Linux build exists, and on Windows.

### Remaining real runs (original wording)

2. Linux without systemd, blocked on Docker, which is off: with the Linux build in a container, the Unix detach, and a
   held launch joining the cgroup group the daemon made for it below the holder's own cgroup (a writable cgroup v2
   subtree, as Linux daemon run milestone 01's notes describe).
3. Linux with systemd, blocked on the daemon running on Linux as a systemd user service, which does not exist yet:
   the holder as a systemd user service with `Delegate=yes` and a group per launch, the daemon's unit stopping before
   the holders' units, and the Linux host shutdown criterion.

The Docker-off statement above records the former blocker verbatim. Docker was available on 2026-11-10; the current blocker is the user's deferral of Linux verification. The systemd prerequisites remain open.

## Planned restart requirements

> Completion criteria:
> - Request a planned restart of a daemon started by hand, not following an application, while sessions write output:
>   - every session keeps running, and its screen is exactly as it was, not marked inexact, except a screen whose
>     width changed while a program showed the alternate screen, which is marked inexact by design (see "Design
>     decisions");
>   - the output written during the restart appears once;
>   - the daemon answers the queries in that output.
> - A plain stop still ends every session, as today.
> - After a planned-stop request, a daemon started by hand next restores every screen exactly.
> - A daemon that follows an application refuses the planned-restart request.
> - Under a systemd user service, the planned restart's successor is started by systemd and the sessions survive.

### Technical design

- [ ] Under systemd the daemon does not start the successor: after its planned stop it exits with an agreed status,
      and the unit's restart rule restarts it on that status, so the successor runs in the unit. A daemon with no
      service manager starts its successor itself.

### Design decisions

- What a saved screen reproduces exactly: every state the checkpoint below covers, including the normal screen
  behind an alternate one, scrollback included, which the save reads by swapping the screens through
  `alacritty_terminal`'s public `swap_alt` and putting the alternate grid back with `grid_mut`
  (`TerminalSnapshot::normal_behind_alternate`). The one case left out, by design, is a width change while the
  alternate screen is shown, until the program enters it again: the reflow may move the hidden cursor away from the
  one entering saved, which entering again in the successor cannot rebuild, and reading it would overwrite the live
  saved cursor. Such a screen is saved marked inexact, restored with the tail since the mark and redrawn (own choice:
  its content beats a rebuild from the tail). A client's first paint reads the hidden normal screen the same way
  (`TerminalSnapshot::paint`), under the same width-change limit. Milestone 04's exact-screen claim inherits it.
- Lossless handover is bounded by the tail: more than 1 MiB written after the mark (2 MiB with a welcome pin) loses
  exact eligibility. Unconditional no-loss would need a disk-backed post-mark spool with acknowledged offsets.

### Handoff

- The systemd case (the successor started by the unit and the sessions surviving) is blocked: the daemon exits with
  status 75 after its planned stop and the unit restarts it on that status (`RestartForceExitStatus=75`), but holders
  still run inside the daemon unit's cgroup, which systemd stops with the main process, until milestone 02's systemd
  parts make each holder a user unit of its own.

- Leaving the restart to a service manager needs evidence: `SYSTEMD_EXEC_PID` equal to the daemon's pid with a
  well-formed `INVOCATION_ID`, or the explicit `--service-managed` option the maintained unit passes. An inherited
  `INVOCATION_ID` alone no longer counts. The policy is resolved once at start and the refusal and the successor
  choice read the same value. A custom unit passing `--service-managed` must restart on status 75.

**Remaining, a real run blocked on the daemon running on Linux as a systemd user service, which does not exist yet**
(see Handoff): under the systemd user service, `octoboardd restart` with sessions writing output; the daemon exits with
status 75, the unit starts the successor, and every session survives with its exact screen. It also needs the service
key delivery, a TODO of the unit: a successor started by the manager makes a key no client can obtain until then. Never
journal the key, or put it in `daemon.owner`, the arguments or the environment.

## Companion requirements

> Completion criteria:
> - Launch a Codex 0.166.0 or later session with holders installed: `codex app-server` is a child of the holder, in the
>   launch's containment, leading a session of its own; the terminal attaches to it with `--remote`; automatic messages
>   go into Codex's queue as they do today.
> - Kill the daemon outright (`kill -9`) and let it start again: the session keeps running, a message sent to the
>   adopted session goes into Codex's queue, not the terminal, and after a cancel the messages Octoboard queued before
>   the restart are started as they would have been without it.
> - `octoboardd restart` with such a session working: the same, with the exact screen; messages left in the outbox by
>   the planned stop are delivered by the successor through the queue.
> - Exit Codex in the terminal: the holder ends the server within a few seconds and nothing of the launch is left. Kill
>   the server: the terminal is ended and the session goes through its exit path, whether or not a daemon is linked.
> - A launch in a holder whose welcome does not advertise companions runs the ordinary terminal launch, with guarded
>   terminal delivery, and no `--remote`.
> - A server that does not come up leaves no companion in the holder; the terminal starts unbound, as the ordinary
>   launch.

### Technical design

- [ ] How the holder starts a companion: as a plain child with no PTY, standard input closed, output piped and read to
      its end only for `PortAfter` and discarded otherwise; leading a session of its own (`Reach::Session`: on Windows
      a job of its own nested in the launch's) inside the launch's containment opened as the terminal's is
      (`Containment::join_launch`), and in the holder's own crash table, so the holder's own crash ends it as it ends
      its programs. The start and its readiness wait run off the runtime, as a spawn does; the daemon waits for the
      answer or the end of the connection, as for a spawn.
- [ ] Lifetime, enforced by the holder whether or not a daemon is linked:
  - Once a terminal's exit is published, its companion is ended: a polite request to its process group, a short grace,
    then a kill and a sweep of what it left, and its endpoint directory removed. A terminal the holder ends or
    releases ends its companion the same way.
  - A companion that exits by itself while its terminal runs has `CompanionEnded` sent to the linked daemon, and the
    terminal is ended as the daemon ends it today when the server dies: a polite terminate, then a kill after the
    polite lead the terminal reports. A daemon that is linked does the same through `server_gone`; both are harmless
    together.
  - A companion no terminal has bound yet is ended when the connection it was asked over ends (whatever the ending),
    on `RevokeLaunch` or `EndCompanion`, and after a bind deadline in any case; it is never listed in a welcome.
  - A companion awaiting its terminal counts as something to serve: the holder's exit rule treats it as a pending
    spawn. A draining holder refuses `OpenEndpoint` and `StartCompanion` as it refuses a spawn.

- [ ] The daemon's crash table never holds a held companion, as it never holds a held terminal.

- [ ] The daemon connects to the companion's endpoint itself, not through the holder: on macOS and Linux a socket in
      a directory only the user can enter, as today, now made and removed by the holder (a holder killed outright
      leaves the directory, as a killed daemon did); on Windows the loopback port guarded by the token above.

The shared endpoint item was checked for macOS; Linux verification stays open. Its Windows token reference means the holder-issued token delivered over the authenticated holder link, whose server receives only its SHA-256.

## Desktop shutdown boundary

The desktop update and application-shutdown requirements remain macOS/Windows work. Linux has no desktop application in this plan; its host-shutdown, preview and service-order requirements are the holder requirements above.

## Mid-task handoff

All Linux verification was stopped at the user's request on 2026-11-10. No holder detach, launch membership in a writable cgroup, crash-hook boundary, restart handover or companion-lifetime result was established before stopping. The two verification containers were removed; the reusable cg-* volumes and images were retained. Linux test run not done (user deferred).
