# Daemon Runtime Hardening Development Plan

## Problem and approach

Several parts of the daemon's runtime hold up on a single machine with one client and a daemon that lives exactly as
long as the application. They fall short of the forms planned next: a Linux server reached from browsers, several
clients at once, and a daemon that can restart without ending the work it carries.

- **Agent processes die with the daemon.** A daemon crash or restart interrupts every session.
- **A session's status comes from hooks and from signals specific to each supported agent**, so an agent with no such
  integration has no status.
- **Concurrent writes from several clients overwrite each other** without notice.

The terminal stream's own improvements (an exact first paint on attach, flow control, and terminal queries answered by
the daemon) and the protocol's compatibility guard are planned separately, not here. This plan relies on the first
paint and on daemon-side query answering where it says so: a screen rebuilt or restored after a restart reaches
clients through the first paint.

## Key design decisions

- **A separate holder process owns the PTYs** and is the parent of every agent process, so the daemon can stop and
  start while programs keep running. The holder does not parse output; it keeps a raw tail per terminal. On start the
  daemon reconnects to every holder. The handshake says whether the last parting was a planned handover or a crash,
  and lists the terminals the holder runs.
- **A session that outlives its daemon keeps working**: its hooks, its MCP bridge and the git gate reach the restarted
  daemon. A session that is only running, with its status and tools lost, does not count.
- **A planned restart hands over exactly, a crash recovers approximately.** Before a planned restart the daemon saves
  each screen; its successor restores it and replays only the output written since. After a crash the screen is
  rebuilt from the raw tail, marked inexact, and the program is made to repaint by a one-row resize there and back.
- **Only an explicit restart hands sessions over.** A `restart` command, beside start, stop and status, is the planned
  restart. A plain stop, a termination signal or the service manager stopping the daemon still ends every session, as
  today.
- **A daemon of another build does not take over an old holder.** The old holder receives no new terminals and exits
  once its last one ends, while new terminals go to a holder of the current build.
- **Restarts and shutdowns here are the ones that are not a desktop quit**: the server form, and a restart of the
  daemon alone. In the desktop form an update and a system shutdown are a quit of the application, which interrupts
  every session today; whether that changes is open.
- **Programs may report their own status** through OSC 7501, the Program Status Protocol, as a source of status for
  agents with no other integration.
- **A write several clients can make carries the version it was read at.** A stale write is refused as a conflict
  with the current version, and the client reads again before retrying.

## Milestones

- [01 Process holders](01-process-holders.md) — needs the daemon's first paint on attach, which does not exist
  yet; its Linux parts also need the daemon running on Linux as a systemd user service, which does not exist yet
- [02 Planned restart handover](02-planned-restart-handover.md) — needs 01; also needs daemon-side answering of
  terminal queries and the daemon's command-line start, stop and status commands, which do not exist yet
- [03 Program status reports](03-program-status-reports.md) — needs daemon-side answering of terminal queries, which
  does not exist yet
- [04 Conflict-checked writes](04-conflict-checked-writes.md)

## Open

- Whether, once holders exist, the desktop form keeps sessions running when the application quits, updates or
  crashes. Today all three leave every session interrupted, and the daemon exits once the application is gone.
- How hooks, the MCP bridge and the git gate reach a restarted daemon. Each of these changes at every daemon start
  today:
  - the desktop daemon's port, which the operating system assigns;
  - the daemon key, which every helper carries: on its command line, in Claude Code's HTTP hook URL, in a project
    session's environment, and in the hook script in the session's scratch space, which is cleared at start;
  - the MCP token, which is issued per agent launch and kept only in the daemon's memory.
- Where the per-launch cgroup subtree is delegated from on Linux once agents run under holders rather than under the
  daemon's own service.
- Which status wins when a program reports through OSC 7501 and another source for the same session says something
  else.
- The granularity of conflict checks on settings (the whole settings record, or each field), and which other records
  get them.
- How the macOS shutdown and restart notifications are used, once the desktop daemon can outlive the application.
- A heartbeat on the control channel, so that a client in the server form notices a connection that has silently
  died.
- The transport for remote hosts, once they are planned:
  - carry the protocol over an SSH session's standard input and output, with no port opened and authentication left
    to SSH;
  - the daemon installs or updates its own binary on the remote machine;
  - it reconnects with backoff and treats a silent line as lost;
  - it folds the remote machine's events into the local event stream.
