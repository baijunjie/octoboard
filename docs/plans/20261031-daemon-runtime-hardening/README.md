# Daemon Runtime Hardening Development Plan

## Problem and approach

Several parts of the daemon's runtime hold up on a single machine with one client and a daemon that lives exactly as
long as the application. They fall short of the forms planned next: a Linux server reached from browsers, several
clients at once, and a daemon that can restart without ending the work it carries.

- **Agent processes die with the daemon.** A daemon crash or restart interrupts every session, and so do an
  application crash and restarting the desktop application to apply an update.
- **A session's status comes from hooks and from signals specific to each supported agent**, so an agent with no such
  integration has no status.
- **Concurrent writes from several clients overwrite each other** without notice.

The terminal stream's own improvements (an exact first paint on attach, flow control, and terminal queries answered by
the daemon) and the protocol's compatibility guard are planned separately, not here. This plan relies on the first
paint and on daemon-side query answering where it says so: a screen rebuilt or restored after a restart reaches
clients through the first paint.

## Key design decisions

- **A separate holder process owns the PTYs** and is the parent of every agent process, so the daemon can stop and
  start while programs keep running. The holder keeps no authoritative screen; it keeps a raw tail per terminal, and
  runs a bounded streaming query recogniser on the output so that, while no daemon is linked, it answers the fixed
  queries whose answer needs no screen with the daemon's own answer (in the first phase the primary device attributes
  only; the cursor position and other screen-dependent queries go unanswered, a stated limit). On start the daemon
  reconnects to every holder; a holder it cannot link to yet is never ended for that, and what it runs stays claimed
  until it is linked to or known to be gone. The handshake says whether the last parting was a planned handover or a
  crash, and lists the terminals the holder runs. This holds on macOS, Linux and Windows: on Windows a pseudoconsole
  created by a detached holder outlives the daemon with either console host, and an update replaces the installation
  while holders run from their build's copy (see "Process holders on Windows" in `docs/architecture.md`).
- **On Linux each holder is a systemd user service that delegates its cgroup subtree** (`Delegate=yes`), and makes a
  group per launch inside it. Where no writable subtree is available, or no systemd user manager runs the daemon,
  holders are detached processes as on macOS and the per-launch process ledger (tag, lineage and session-id
  attribution) remains the fallback.
- **Holders, not a handover of PTY handles from one daemon to the next.** A handle handover in the manner of herdr
  (the old daemon passes its PTY masters to its successor over a Unix socket and exits) was weighed and not taken:
  - Holders are one mechanism for a crash, a planned restart and an update alike. A handover covers only the planned
    moment: when the daemon crashes, the master closes and the programs end, so crash survival would need holders as
    a second mechanism anyway.
  - On Windows a pseudoconsole lives as long as the process that created it, which a holder is; a handover would have
    to move an opaque console handle to another process, which the system's API does not offer.
  - An update moves no running state: the new daemon connects to the old build's holder.
  - The cost is a holder protocol that daemons of later builds must keep speaking, and one more local hop per byte.
- **A session that outlives its daemon keeps working**: its hooks, its MCP bridge and the git gate reach the restarted
  daemon. A session that is only running, with its status and tools lost, does not count.
- **Helpers reach the daemon through their holder.** A session's hooks, MCP bridge and git gate connect to their
  holder's address, which does not change while the holder lives, and the holder authenticates them and relays to the
  current daemon. While no daemon is connected the holder keeps their requests and hands them over once one is.
  Requests that only report, such as hook reports, wait until a daemon connects; a request its caller waits on, such
  as the git gate or an MCP call, waits only up to a deadline that covers an ordinary restart, then fails as it does
  today with no daemon running.
  This was preferred over persisting the daemon's port, key and tokens and binding the same port again: requests made
  while the daemon restarts are not lost, there is no port another program can take meanwhile, and since a helper and
  its holder come from the same build, the only interface that crosses builds is the one between holder and daemon.
- **Each build's holders and helpers run from a copy of that build** kept outside the installation location, and
  removed once that build's last holder exits. Helpers are started afresh on every call, so without the copy an
  update replacing the installation would run the new build's helpers against an old holder. On Windows the copy also
  leaves the installation folder free for the installer.
- **A daemon adopts holders from the previous stable release up to its own build.** Beta builds between two stable
  releases stay compatible with each other throughout. A holder outside that range, a newer one included, is not
  adopted: the daemon ends it with its programs, and their sessions are interrupted, as every session is today.
  Adopting a session whose helpers cannot reach the new daemon would leave its status and tools silently broken.
- **A planned restart hands over exactly, a crash recovers approximately.** Before a planned restart the daemon saves
  each screen; its successor restores it and replays only the output written since. After a crash the screen is
  rebuilt from the raw tail, marked inexact, and the program is made to repaint by a one-row resize there and back.
- **Of the daemon's stops, only an ordinary stop ends sessions; a planned stop hands them over exactly.** A plain stop,
  a termination signal or the service manager stopping the daemon is an ordinary stop and ends every session, as today.
  A planned stop is made by the `restart` command, beside start, stop and status, through a planned-restart request that
  also brings up the successor on the same address (a daemon run by systemd exits with an agreed status after its
  planned stop, and the unit restarts on that status, so the successor starts in the unit; a daemon with no service
  manager starts the successor itself); by the desktop update restart, through a planned-stop request with no successor,
  since the new application starts its own daemon; and by the daemon finding the desktop application gone. A daemon that
  follows an application refuses the planned-restart request.
- **A daemon of another build gives an old holder no new terminals.** It still adopts the terminals that holder runs;
  the old holder exits once its last one ends, while new terminals go to a holder of the current build.
- **A desktop update keeps sessions running, on macOS and Windows.** Restarting the application to apply an update
  makes a planned stop, and the new version's daemon adopts the holders, the previous build's included. The restart
  asks for confirmation only when some session would be interrupted (a holder outside the compatible range), and
  names those sessions.
- **An application crash keeps sessions running too**: the daemon finding the application gone makes a planned stop,
  and the next launch adopts the holders.
- **A plain quit and a system shutdown still interrupt every session.** Every quit counts, the ones that do not wait
  for the daemon included (the quit of a window that stopped responding, the quit from the failed-UI screen, a quit
  whose stop request the daemon does not answer): the application tells the daemon it is quitting before it exits,
  without waiting, and the daemon that finds the application gone after that makes an ordinary stop. A shutdown,
  restart or logout of the system asks for no confirmation: the daemon makes its ordinary stop, freezing each
  session's preview, and the system goes on. On Windows, where the system ends the daemon with no notice, the
  application passes the system's end-session notice on, and the stop is only as complete as the time the system
  allows.
- **Programs may report their own status** through OSC 7501, the Program Status Protocol. Supporting the protocol and
  taking the status from it are separate: every session's terminal receives the reports and answers the support
  query, while only an agent with no other integration takes its status from them. Where hooks or an agent's own
  signals give a status, they win over OSC 7501.
- **A write several clients can make carries the version it was read at.** A stale write is refused as a conflict
  with the current version, and the client reads again before retrying. Versions are kept per field, so two
  clients changing different fields do not conflict. Every record a client edits through a form is checked:
  settings, project settings, consoles, account configuration. A change made in one action, such as a rename or a
  pin, keeps the last write.
- **A heartbeat on the control channel** lets a client notice a connection that has silently died, which matters
  once clients reach the daemon over a network.
- **A Codex session's private app server runs in its holder, as the terminal's companion.** The holder starts it
  before the terminal under the terminal's id, from a command line the daemon sends, ends it once the terminal exits
  and ends the terminal when it dies, whether or not a daemon is linked, and lists its endpoint in every welcome. The
  daemon connects to that endpoint directly, as it did to its own server, so the queue logic and its tests stand, and
  a daemon that adopts the terminal links to the server again and re-identifies the conversation from the session
  record. A holder that does not advertise companions gets the ordinary Codex launch, and a daemon without holders
  keeps the server as its own child.
- **Remote hosts, once they are planned, are reached over SSH.** The protocol runs over an SSH session's standard
  input and output, with no port opened and authentication left to SSH. The daemon installs or updates its own binary
  on the remote machine, reconnects with backoff, treats a silent line as lost, and folds the remote machine's events
  into the local event stream. No milestone here builds it.

## Milestones

- 01 Windows holder feasibility (closed)
- [02 Process holders](02-process-holders.md) — its Linux systemd parts need the daemon running on Linux as a
  systemd user service, which does not exist yet
- [03 Planned restart handover](03-planned-restart-handover.md) — needs 02, and the daemon-side answering of
  terminal queries that multi-client terminal milestone 04 brought; its `restart` command sits beside the
  command-line `start`, `stop` and `status` of Linux daemon run milestone 01, none of which runs a daemon in the
  background on Windows; its systemd case needs the daemon running on Linux as a systemd user service, which does
  not exist yet
- [04 Desktop updates keep sessions running](04-desktop-update-handover.md) — needs 03
- [05 Program status reports](05-program-status-reports.md)
- [06 Conflict-checked writes](06-conflict-checked-writes.md)
- 07 Control-channel heartbeat (closed)
- [08 Codex app server in the holder](08-codex-server-in-holder.md) — needs 02 and 03, and Codex's private app
  server, which is in place
