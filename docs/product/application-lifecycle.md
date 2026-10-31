# Application lifecycle

Octoboard comes in two forms. This document describes the **desktop form**, the macOS application, in full: the agent
processes are owned by a background daemon that starts and stops with it; background processes the user chose to keep
are managed by the user instead (see "Automatic stopping and its boundaries" in `docs/product/background-processes.md`).
Closing the window quits the application unless the user has turned on keeping it running in the background, a
setting that is off to begin with (see "Closing the window" below). What the **server form** — the daemon alone, on a
Linux server without a display, reached by its owner from a browser on another machine — changes is said where it
applies, and in "The daemon on its own" below. That form is not built yet, so those passages state what already follows
from it and what the daemon does today, not a finished behaviour.

## System requirements and distribution

- **macOS 11 or later**, on Apple Silicon or Intel — the release build is one universal bundle holding both
  architectures, the bundled daemon included.
- Octoboard is distributed as a `.dmg` disk image. It cannot be distributed through the Mac App Store: its translucent
  window (see "The window chrome and the content panel" in `docs/product/window-layout.md`) relies on a private macOS
  interface, which the store does not admit. The release build, the bundled daemon included, is signed with a Developer
  ID and notarized, so Gatekeeper admits the `.app` and the `.dmg` as built, a copy carrying the quarantine attribute
  included; that has been confirmed only on the machine that built it, not yet by installing on a clean one.
- Once installed, the application updates itself in place from the project's releases, on the user's request (see
  `docs/product/in-app-updates.md`).
- The server form has no requirements or distribution settled: the daemon is a single binary meant to be put on the
  server as it is, and its Linux build is not established yet.

## Starting up

In the desktop form the daemon is started as part of the application. Whoever starts it, it listens on `127.0.0.1`
only, on a port the operating system assigns, makes a new key for this launch (see "Who can reach the daemon" below),
and prints both on its standard output as its first line, `octoboardd listening on 127.0.0.1:<port> with key <key>`:
the application reads the port and the key from there, and neither is ever fixed or guessed. The daemon also writes the
port, without the key, to a file in the temporary directory, for a developer attaching to a daemon started by hand, who
takes the key from that startup line (see "Files Octoboard owns" below). Neither fits the server form: nothing off the
server can reach loopback, and a port that changes on every start cannot be bookmarked or written into a tunnel
command; the address the daemon listens on there does not exist yet.

A page of the UI given no key — or no port, where its own address cannot be the daemon's — connects to nothing and
shows a screen saying there is no daemon address or key, and how to give them to a page run against a daemon started by hand:
`?port=` and `?key=` in the page's address, `VITE_DAEMON_PORT` and `VITE_DAEMON_KEY` for the dev server, or
`OCTOBOARD_DAEMON_PORT` to have the dev server proxy to the daemon, the key still given by `?key=` or `VITE_DAEMON_KEY`.
The desktop application always gives its window both.

**At launch, starting the daemon is tried up to 3 times.** An attempt fails when the daemon cannot be started, ends before it has
reported its port and key, or has not reported them within **10 seconds**, in which case it is stopped. The next attempt
follows **1 second** after the first failure and **2 seconds** after the second. The application stays responsive
meanwhile, and its window opens once the daemon has started or the last attempt has failed; a quit before then is
immediate and also stops a daemon still being started. When every attempt failed, the window opens on the screen for a
daemon that could not start, its reason saying how many attempts were made and what the last one did, the daemon's
own error output included where it ended early.

On every daemon start:

- **Every session a previous run left behind becomes *interrupted*.** No process from that run is adopted as a running
  session, whatever the stored status said. Archived sessions are left as they are.
- Per-session scratch space from previous runs is cleared.
- Saved background-process records are reconciled under "Automatic stopping and its boundaries" in
  `docs/product/background-processes.md`.

**One Octoboard at a time.** A second daemon running against the same `~/.octoboard` would own a second set of agent
processes while sharing one database, so it refuses to start, on every attempt. The application then opens its window
anyway, on a screen that says the daemon failed to start — a failed start never leaves a window with no explanation.
The screen says what the daemon is and that opening Octoboard again tries once more, with the reason under its
explanation; no error alert opens over it. It is a known failure, not an unexpected one: it asks for a report only if
those steps do not resolve it (the screen is described in "The window chrome and the content panel" in
`docs/product/window-layout.md`). It has no Quit button; it quits like any window with no daemon running, at once and
asking nothing, and with nothing to keep running, closing the window quits whatever that setting says (see "Closing
the window" and "Quitting" below). The lock is per data directory, so on a server it is one daemon per Unix user,
whose `~/.octoboard` is the daemon's (see "The daemon on its own" below).

**A daemon that dies later is started again.** When the daemon has started and then ends on its own, with a non-zero
exit status or by a signal, the application starts a new one after **1**, then **2**, then **4 seconds**, at most **3
times in any 10 minutes**; a relaunch that fails counts too, and the next one follows. Once one succeeds, the window is
reloaded onto the new daemon with no notice of its own (until then it has lost its connection to the daemon, see
"Losing the daemon connection" below), and a window closed into the background stays there. The daemon's end has
interrupted every session as any daemon end does, and each resumes with a click (see the list above).
While the daemon is down there is nothing to keep running, so a close or quit meanwhile behaves as with no daemon (see
"Closing the window" and "Quitting" below). Nothing is started again once a quit or a restart is under way, and
there is no button to start the daemon by hand.

When the daemon is not started again — those 3 relaunches in the last 10 minutes are spent, or it ended with exit
status 0, as on a requested shutdown or `SIGTERM` — the application says so in an error alert titled "The daemon exited
unexpectedly", which says that the daemon runs the user's sessions and terminals, that Octoboard cannot work without
it, and asks the user to restart Octoboard; it does not silently keep showing a stale session list. The alert treats it
as an unexpected failure, offering **Report issue**, with the detail behind **Error details** rather than in its
sentence (see "Recovery and unexpected errors" in `docs/product/error-alerts.md`): how the daemon ended (its exit
status, or a signal, or why the last relaunch failed) and, when the relaunches are spent, that it was restarted that
many times in the last ten minutes and is not restarted again. A daemon stopped by the application's own quit or
restart raises no alert (see "Quitting" below).

**The window appears already themed.** It is created hidden and shown only once the UI has applied the chosen appearance
(see "Light, dark and follow the system" in `docs/product/appearance.md`), so a launch never flashes the operating
system's own appearance first. The window therefore appears a fraction of a second later than it otherwise would. Should
the UI never get that far at all — its bundle failing to load, say — the shell shows the window regardless **4 seconds**
after creating it, so a failed start never leaves a running application with no window on screen. A window the user has
by then closed into the background stays closed.

**Closing the window during a launch quits.** The shell starts every launch with keeping the application running in
the background off, whatever the user last chose: the choice is kept by the window's UI and handed over once it has
loaded (see "Closing the window" below), so a close in the first moments of a launch — and one on the screen shown
when the daemon failed to start, which never gets a choice to hand over — quits.

## Who can reach the daemon

The daemon has no login and no notion of who is connected: whoever reaches it with its key is let in, and every client
is the same client. The key keeps web pages out, not programs: it is in the command lines and environment of the agents
and helpers the daemon starts, where any program on this machine can read it. In the desktop form that means **any
program running on this machine can connect to the daemon and do everything the application can**: create consoles and
sessions, start agents, type into their terminals, and open a shell terminal and run any command in it (see
`docs/product/shell-terminals.md`). It is safe there only because the daemon listens on loopback, so whoever can reach
it already has the machine.

In the server form the same sentence reads **anyone who can reach the port can do everything** — start agents, type into
terminals and run any command in a shell terminal on the server, as the Unix user the daemon runs as — which is no
access model for a machine on a network. So the daemon as it is today must not be put beyond loopback, and nothing in it
lets it be: it binds to loopback and nowhere else, and it refuses any request whose `Host` names it by anything but a
loopback address. A reverse proxy in front of it does not change that: the daemon also refuses a request whose `Origin`
— which a browser sends on every WebSocket upgrade — is neither the application's, nor loopback, nor the same as `Host`,
so a proxy would have to rewrite both headers, and the daemon behind it would be open to whatever the proxy lets through
with nothing of its own to tell it apart but the key, which is one for every client, changes on every start and is
readable by any process on the server. What the form still lacks is the whole of an access model: a credential that
lasts and is kept like one, checked on every route, the UI and the WebSocket upgrades included; one credential per
device of the owner's, so that a lost or retired device can be revoked without re-issuing the rest; and TLS, which a
browser needs before it will show a notification or touch the clipboard from a page at a network address at all. None
of that exists yet.

The server form is the same user reaching their own daemon from another machine, not a daemon shared by several people.
The daemon will not tell clients apart even then: whoever holds a credential is taken for the owner and can do
everything the owner can, so a credential is to be kept like a password. That is this version's design boundary, not a
gap left open.

The one way a browser on another machine reaches today's daemon is a tunnel that ends on the server's loopback — an
SSH port forward. The page is then at `http://localhost:<port>`, a loopback address, which the checks below admit once
the page carries the key of the daemon's current launch, and which a browser treats as a secure context, so
notifications and the clipboard work through it.

**A web page open in a browser cannot, unless it is Octoboard's own.** Every request, on every route and every
WebSocket upgrade, passes two checks in turn:

- **Where it comes from.** When a browser makes a request on behalf of a page, the daemon answers `403` unless that page
  is the application's own window or is served over `http` or `https` from this machine's loopback address, on any
  port. A site that makes its own name resolve to `127.0.0.1` is refused as well. Programs that are not browsers, such
  as scripts, are not subject to this check, and neither is the application's own window.
- **The key.** A request that passed the first check is answered `401` unless it carries, as its `key` query parameter,
  the key the daemon made for this launch. This is what keeps out the pages the first check lets through: any page on a
  local dev server, and any other Tauri application, all of which share the application window's origin. The desktop
  application hands the key to its own window, and to the window it reloads onto a daemon started again (see "Starting
  up" above); the daemon hands it to the agents' helpers it starts, beside the port. A key is good only for the launch
  that made it: after any daemon start, a page or program holding an earlier one is refused until it is given the new
  one.

## Losing the daemon connection

A dropped connection is retried automatically a couple of times with a short backoff. While that is happening a
banner says so. Once the attempts are spent, the banner offers a Retry button; nothing retries forever on its own.
Each new connection re-reads the whole state, so nothing has to be replayed by hand. A request still waiting for its
reply when the connection closes, or when it is reopened, is never sent again on the new connection: it fails, saying
that the connection closed, or that it was reopened before the reply arrived.

**Retry with a dialog or a menu open.** Nothing drawn over the content panel — a dialog, a menu, a popover — covers
the banner: a press where Retry is drawn retries the connection, and what was open stays open, so a pointer never has
to close anything first to get back out of a lost connection (see "What an open dialog, menu or popover covers" in
`docs/product/window-layout.md`). Retry is also reachable by keyboard with a modal dialog open; the focus cycle and
the restriction while a menu is open are in "Moving focus between regions with F6" in
`docs/product/moving-focus-between-regions.md`.

Failures a user has to know about use the acknowledgement dialog described in "Error alerts" in
`docs/product/error-alerts.md`, including a daemon process that exits and is not started again, a failed report-page submission, and
the failure of a dialog's own action. Session notices that are not failures use toasts; see `docs/product/toasts.md`.
What is wrong with one field is said under that field instead (see "Errors under a field" in
`docs/product/error-alerts.md`).

## Closing the window

This section and "Quitting" below are the desktop form's. A browser tab has no close that Octoboard sees, and nothing
in a browser quits anything (see "The daemon on its own" below).

**The window's close button, `Cmd+W` and the Window menu's Close Window item are one gesture.** All three do the same
thing, deliberately, and what they do is the user's choice: the **Keep running in the background when the window is
closed** switch in Settings' General section, which is **off to begin with** (see "Keep running in the background" in
`docs/product/settings.md`).

**With the switch off, closing the window quits.** It is an ordinary quit gesture, taking exactly the path `Cmd+Q` takes
(see "Quitting" below): with a session starting, working, waiting for the user or working in background, or a busy shell
terminal, it asks the same confirmation, where Cancel leaves the window open and everything running, and otherwise it
quits at once. The sessions a confirmed close ends are left *interrupted* and resumable, just as any confirmed quit
leaves them (see "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`).

**With the switch on, closing the window closes it into the background**: the window is hidden, Octoboard leaves the
Dock, and the focus passes to the application used before it. The daemon, every session and its agent process keep
running, and so does the UI behind the hidden window: a session that raises its hand still notifies (see "The raised
hand" in `docs/product/session-statuses.md`), the Dock badge's count is kept up to date for when Octoboard returns to
the Dock, and the menu bar icon's menu keeps following the sessions (see `docs/product/menu-bar-icon.md`).

- **In native fullscreen** the window first leaves fullscreen and is hidden once that is done, about 0.8 seconds later,
  so no empty fullscreen space is left behind. Brought back within that time, it is not hidden.
- **When the daemon is not running** — it failed to start, or has exited, or is being started again — there is nothing
  to keep running, so closing the window quits at once and asks nothing, whatever the switch says.
- Hide Octoboard (`Cmd+H`) is not a close: it hides the application as macOS always does, and Octoboard stays in the
  Dock.
- The menu bar icon is there whatever the switch says (see `docs/product/menu-bar-icon.md`).
- Closing the window is never a way to force a quit through: see "Quitting when the window has stopped responding"
  below.

**Where the choice is stored.** It is kept **per client**, in that client's own browser storage, under the same rules
as the appearance and language choices (see "Where the choice is stored" in `docs/product/appearance.md`): the daemon
neither stores it nor carries it, and it is not among the files under `~/.octoboard` (see "Files Octoboard owns"
below). So it does not travel between a desktop window and a browser tab, or between two machines reaching the same
daemon, and a client whose storage has been cleared is back to off. Each launch starts from off until the window's UI
hands the choice over (see "Starting up" above).

**Bringing a window closed into the background back.** Any of these brings the window back where it was, at the size
and position it had:

- the menu bar icon's menu: its Open Octoboard or one of its sessions (see "Clicking the icon" and "The icon's menu"
  in `docs/product/menu-bar-icon.md`);
- opening Octoboard again while it runs — from the Finder, Spotlight or `open`;
- anything else that activates Octoboard, such as clicking one of its notifications;
- a quit that has to ask for confirmation (see "Quitting" below);
- the daemon exiting and not being started again: the window comes back showing the notice that it exited (see
  "Starting up" above). A daemon that exits while the window is shown only shows the notice, and does not take the
  focus from whatever the user is doing. A daemon that is started again brings nothing back.

Octoboard then returns to the Dock, comes to the front, and its Dock badge shows the current waiting count again.

## Quitting

Every way of ending the application behaves the same: `Cmd+Q`, the application menu's Quit, the menu bar icon's Quit
Octoboard (see "Quit Octoboard" in `docs/product/menu-bar-icon.md`), the Dock icon's own Quit, a system-initiated
termination — logging out, restarting or shutting down the machine — and, while keeping Octoboard running in the
background is off, closing the window: the close button, `Cmd+W` and the Window menu's Close Window item (see "Closing
the window" above). The one difference is the escape hatch in "Quitting when the window has stopped responding" below,
which the close gestures have no part in.

- **While any session is *starting*, *working*, *waiting for the user* or *working in background*, quitting asks for
  confirmation.** One working in background counts because the quit would also end the work it left running (see
  "Working in background" in `docs/product/session-statuses.md`). A session *awaiting instructions* does not count: it
  is in the middle of nothing that is reported, and the quit still leaves it interrupted and resumable like the others.
  The message says that quitting interrupts those sessions and that each stays resumable next time.
- **While any shell terminal is busy, quitting asks for confirmation too** (see "Busy shells" in
  `docs/product/shell-terminals.md`); an idle shell terminal, or one whose shell has exited, does not count. With only
  busy shell terminals holding the quit, the message says some terminals are still running something and quitting ends
  what they run; with sessions and busy shell terminals both, it says both are still running, that quitting interrupts
  the sessions, each resumable next time, and ends what the terminals run.
- Asking brings Octoboard to the front first, restoring its window if it was minimized and bringing it back if it was
  closed into the background, so a quit from outside the window — the Dock icon's own Quit while another application
  covers it, the menu bar icon's Quit or a logout while the window is closed — does not leave the question unseen.
- Once confirmed, every session process is ended and every one of those sessions is left **interrupted**, never
  archived. Clicking one next time resumes it (see "Archiving, interruption and resuming" in
  `docs/product/archiving-and-resuming-sessions.md`). Every shell terminal ends too (see "Closing a shell terminal" in
  `docs/product/shell-terminals.md`).
- **Once the daemon has begun to shut down, nothing new starts.** From then on — a confirmed quit or restart, or a
  termination signal (see "The daemon on its own" below) — no session is launched, resumed, reopened, switched to
  another account or handed off, and nothing else that needs a session to itself (a handover of its branches, a change
  of its owner) is begun: each is refused with `daemon_shutting_down`. A launch that was already past that check when
  the shutdown began is ended as soon as its process comes up, leaving the session *interrupted* like the rest. Where a
  window still shows such a refusal, it is an error alert saying that the daemon is shutting down, so nothing can be
  started now, with no remedy and no **Report issue**. An automatic account switch, or its restore of the session, that
  the shutdown cuts short tells nobody (see "When the switch fails" in `docs/product/automatic-account-switch.md`).
- With no session *starting*, *working*, *waiting for the user* or *working in background* — none at all, or only ones
  *awaiting instructions* — and no busy shell terminal, quitting is immediate and asks nothing, and a window closed into
  the background is not brought back for it.
- While the daemon is not running — it failed to start, or has exited, or is being started again — every quit is
  immediate and asks nothing, and no daemon is started again after it.
- On the screen for a UI that failed (see "The window chrome and the content panel" in
  `docs/product/window-layout.md`), every quit is immediate and asks nothing, even while sessions are running or a
  shell terminal is busy, since the window no longer knows them; the daemon is not asked to shut down first. It exits
  once the application is gone, leaving the sessions as a crash does (see "Crashes and forced termination" below).
- The daemon stopping as part of a quit is not reported as the daemon exiting: no notice is shown and a closed window is
  not brought back for it.
- Quitting does not depend on the daemon answering: if it does not, the application exits anyway after a short wait.
- If the application cannot end itself once the quit is under way, an error alert titled "Could not quit" says
  Octoboard kept running instead of ending and asks the user to try quitting it again, and the window stays open.
  The alert treats it as an unexpected failure, offering **Report issue**, with the reason behind **Error details**.
- Until the window has loaded far enough to be able to ask, a quit is never held back — a window that never gets as
  far as showing the confirmation can still be quit.

Because a logout, restart or shutdown is answered exactly like any other quit, **Octoboard can hold up a logout,
restart or shutdown until the user answers the confirmation dialog.** That is deliberate, not an oversight.

### Restarting

Octoboard restarts itself only to apply an installed update, when the user presses **Restart now** (see "Restarting to
apply an update" in `docs/product/in-app-updates.md`). A restart is a quit followed by a fresh launch, and follows the
rules above:

- It asks for confirmation exactly when a quit would — a session *starting*, *working*, *waiting for the user* or
  *working in background*, or a busy shell terminal — and otherwise restarts at once. The confirmation is titled as a
  restart, its message says that restarting interrupts those sessions, each resumable after the restart, and ends what
  the terminals run, as the quit's message would, and its confirming button reads Restart. Cancel leaves everything
  running.
- A quit gesture arriving while the restart's confirmation is open turns it into the quit's confirmation.
- Once confirmed, everything ends as in a confirmed quit — every session left *interrupted* and resumable, every shell
  terminal ended, the window's size and position saved — and Octoboard starts again. The daemon is stopped before the
  new launch, since the new launch's daemon would otherwise refuse to start while the old one holds the data directory
  (see "Starting up" above): the old daemon is first asked to shut down, as in any quit; one still running **2
  seconds** later is sent a termination signal, which has it end its sessions' processes first, and one still running
  **5 seconds** after that is killed.
- If the application cannot restart, an error alert titled "Could not restart" says Octoboard kept running instead of
  relaunching and asks the user to quit it and open it again. It is treated as an unexpected failure, as a failed quit
  is.

### Quitting when the window has stopped responding

A confirmation dialog that can no longer be shown or answered — the window's process died, or its page hung — would
otherwise leave no way out short of a force quit. So: **when a quit gesture has been deferred to the confirmation
dialog and a second quit gesture arrives within 2 seconds, that second gesture quits immediately and asks nothing.**
The sessions are left exactly as a crash leaves them (see "Crashes and forced termination" below).

A window that is still answering resets that 2-second window every time it handles a quit gesture, so pressing `Cmd+Q`
twice in quick succession on a working Octoboard does **not** skip the confirmation. The escape hatch opens only when
nothing answered the first gesture. It is known to hold for scripted quits; with the page frozen, two `Cmd+Q` key
presses have been seen not to take it, for a reason not established, so the menu's Quit is the surer way out.

**Closing the window has no part in this, even when it is a quit** (see "Closing the window" above). A double press
on a close button is an ordinary gesture and must not end a live session unasked, so the close button, `Cmd+W` and
Close Window never take this escape hatch however often they are repeated, and never open it for the gesture after
them either. `Cmd+Q`, the application menu's Quit, the menu bar icon's Quit or the Dock icon's Quit — twice — is the
way out of that state.

## Crashes and forced termination

A crash of the application, or killing it outright, leaves **the same state** as a confirmed quit: every session
interrupted and resumable.

- The daemon exits by itself once the application is gone, whether or not it was asked to. That is a watch the
  application asks for when it starts the daemon; a daemon started on its own has no application to follow (see "The
  daemon on its own" below).
- Agent processes do not outlive the daemon: it ends them on its way out, including when it is signalled to stop and
  when it goes down on an internal failure.
- A normal process exit stops the launch's attributed background processes, subject to the user's keeps and the
  boundaries in "Automatic stopping and its boundaries" in `docs/product/background-processes.md`. On an internal
  failure the daemon also kills every process in each agent's terminal session, helpers in process groups of their
  own included. At the next daemon start, saved process records allow cleanup of attributed processes that survived
  outside those terminal sessions, while preserving kept processes.
- Shell terminals do not outlive the daemon either (see "Closing a shell terminal" in
  `docs/product/shell-terminals.md`).
- Should a session's process nonetheless survive, the next start still marks every non-archived session interrupted
  (see "Starting up"), so the session list never claims a session is running that this Octoboard does not own.

## The daemon on its own

In the server form there is no application: the daemon is the whole of Octoboard on the server, and its lifetime is
for whoever runs the server to manage. What follows is what the daemon does today when it is started by hand, with
nothing that the form will need and does not have yet claimed to exist.

- **Starting and stopping are the administrator's.** Nothing starts the daemon but the person or the service manager
  told to, and nothing stops it but a termination signal — `SIGTERM`, or `Ctrl+C` in the terminal it was started
  from. The daemon answers that exactly as the desktop form's quit does once confirmed: every session's process is
  ended and every one of those sessions is left *interrupted*, never archived, every shell terminal ends, and nobody
  is asked first. Started
  without an application to follow, the daemon runs until it is stopped; exiting with the application is a watch the
  desktop form asks for and the server form never does. Nothing starts it again when it dies: that is the desktop
  application's (see "Starting up" above). A command-line surface for starting it in the background,
  stopping it and asking where it listens does not exist yet.
- **A browser is not the application.** Closing a tab, or every tab, ends nothing: the daemon, every session and its
  agent process, and every shell terminal run on until the daemon is stopped, and a browser opened later finds them
  where they were. There is no
  quit gesture in a browser, no confirmation, and no background switch (see "Closing the window" above).
- **A restart interrupts every session.** What "Starting up" says of a daemon start holds unchanged, whichever browser
  or device a session was opened from; each resumes with a click, as after any quit.
- **One daemon per data directory, and the data directory is the owner's.** The lock in "Starting up" is per
  `~/.octoboard`, so per Unix user on the server; the daemon's consoles, projects and sessions are that user's, and its
  agents are launched under that user's own agent accounts, with that user's logins and keys — every client of the
  daemon acts as that user.

## Files Octoboard owns

Everything Octoboard writes for itself lives under `~/.octoboard` — on the machine the daemon runs on, which in the
server form is the server — with two exceptions, the window's state and the port file, described after the table:

| Path | Contents |
|---|---|
| `~/.octoboard/octoboard.db` | Consoles, projects, session records, the report panel pages every session pushed, each project's trust permission and the trusted folders (see "The trust permission" in `docs/product/folder-trust.md`), and the settings the daemon keeps for every client (see "Git" and "General" in `docs/product/settings.md`). The database records the version of its schema, and the daemon decides what to do with the file when it opens it, on every start: **a database at the version this build reads** is used as it is; **a file with no tables in it** is given the current schema in place; **a database at an older schema version** (this build reads version 1, and has no upgrade from an older one, so the only older state is a database written before versions were recorded) is moved aside: the daemon renames it, beside itself, to `octoboard.db.superseded-<Unix time in seconds>`, logs a warning naming both files, and starts a fresh database at the usual path. Nothing in a database moved aside is read back into the new one: the user re-enters their consoles, projects and accounts by hand, and the old file stays on disk, never deleted. **A database at a newer version** is refused and left untouched, and so is **a file that is not a SQLite database**: the daemon fails to start, with a reason that names the file (for a database at a version this build does not read, its version and that renaming the named file moves it aside, which deletes nothing and has this build start a fresh database), shown as described in "Starting up" above. |
| `~/.octoboard/consoles/<console id>/` | A console's working directory, where its console sessions run, including the console session instruction file Octoboard writes there, `AGENTS.md`, and `CLAUDE.md`, a link to it (see "The console session's instruction file" in `docs/product/launching-agents.md`). Removed when the console is deleted. |
| `~/.octoboard/run/<session id>/` | Per-session scratch space for what a launch injects. Removed when the session's process is gone, and cleared wholesale on daemon start. |
| `~/.octoboard/output/<session id>` | The saved terminal preview shown for an interrupted session (see "Interrupted, archived and dormant sessions" in `docs/product/session-terminal.md` for its capture rules and limits). Written when the process ends, including when the daemon stops it on its way out, but not when the daemon itself goes down on an internal failure. Starting a new process discards it. Removed with the session's record; on daemon start, any left for a session that no longer exists is removed. |
| `~/.octoboard/handoff/<session id>.md` | The handoff package a session read when it took over another session's place (see "The handoff package" in `docs/product/handing-off-sessions.md`), named for the receiving session. Removed with that session's record; on daemon start, any left for a session that no longer exists, and any half-written one, is removed. |
| `~/.octoboard/daemon.lock` | Enforces one daemon per data directory. |
| `~/.octoboard/logs/daemon.log` | The daemon's log, at the level of information and above, the panics of any of its threads included. Only the daemon that holds `daemon.lock` writes it, so a second daemon refused at start leaves it alone. An entry that would take the file past **5 MiB** first rotates it: `daemon.log` becomes `daemon.log.1`, the one before that `daemon.log.2`, then `daemon.log.3`, and the oldest beyond that is dropped. |
| `~/.octoboard/logs/desktop.log` | The macOS application's own log: each attempt to start the daemon or start it again and how it went, and the failures the application meets itself, each line with a UTC timestamp. A file larger than **1 MiB** when the application first writes to it in a launch is moved to `desktop.log.1`, replacing the one there. |
| `~/.octoboard/process-ledger.json` | The launch identities, attributed processes and user keeps used for background-process cleanup across daemon restarts (see "Automatic stopping and its boundaries" in `docs/product/background-processes.md`). It records the boot ID of the system it was written in (on Linux), and records from before the system last started are ignored. An unreadable file is moved aside as `process-ledger.json.unreadable`; processes identified only by their launch tag are kept rather than stopped without knowing the user's choices. A file whose boot ID cannot be compared with the running system's (it records none, written before boot IDs were recorded, on a system that has one, or the daemon cannot read the system's) is treated as unreadable but not moved aside. |

The macOS application keeps the window's size, position and maximized state in
`~/Library/Application Support/dev.octoboard.desktop/window-state.json` (see "The window's size and position across
launches" in `docs/product/window-size-and-position.md`). It belongs to the application's window rather than to the
daemon, so it is not under `~/.octoboard`. Deleting it makes the next launch open the window as a first launch does.

The daemon writes the port it listens on to `octoboardd.port` in the temporary directory, for a developer attaching to
a daemon started by hand (see "Starting up"), and removes it on its way out. The file holds the port alone, never the
daemon's key. On macOS that directory is the user's own
`$TMPDIR`; on Linux, where `TMPDIR` is usually unset, it is the shared `/tmp`, so two daemons on one server — each a
different owner's, each single-user — would write the same file.

What Octoboard writes inside a project's repository, and what it writes into the user's own agent configuration, is
in "What Octoboard never modifies" in `docs/product/launching-agents.md`.
