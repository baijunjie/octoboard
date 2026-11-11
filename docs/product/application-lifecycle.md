# Application lifecycle

Octoboard comes in two forms. This document describes the **desktop form**, the macOS or Windows application, in
full: the agent processes are owned by a background daemon that starts and stops with it, and run in process holders
that keep them running through a daemon that goes down without stopping (see `docs/product/process-holders.md`);
background processes the user chose to keep are managed by the user instead (see "Automatic stopping and its
boundaries" in `docs/product/background-processes.md`). Closing the window quits the application unless the user has
turned on keeping it running in the background, a setting that is off to begin with (see "Closing the window" below). What the **server form** — the daemon alone, on a
Linux server without a display, reached by its owner from a browser on another machine — changes is said where it
applies, and in "The daemon on its own" below. That form is not built yet, so those passages state what already follows
from it and what the daemon does today, not a finished behaviour.

What the desktop application runs on and how it is installed is in `docs/product/installing-the-desktop-app.md`.

## Starting up

In the desktop form the daemon is started as part of the application. Whoever starts it, it listens on `127.0.0.1`
only, makes a new key for this launch (see "Who can reach the daemon" below), and prints the port and the key on its
standard output as its first line, `octoboardd listening on 127.0.0.1:<port> with key <key>`. The port is one the
operating system assigns, unless the daemon was started with `--port` (see "Choosing the port" below); the desktop
application never gives one, reads the port and the key from that line, and neither is ever fixed or guessed. The
daemon also writes the port, without the key, to a file in its data directory, for a developer attaching to a daemon
started by hand, who takes the key from that startup line (see "Files Octoboard owns" below). Nothing off the machine
reaches loopback except through a tunnel that ends on it, such as an SSH port forward; for the server form, `--port`
keeps the port the same across starts, so an address can be bookmarked and a tunnel command written once.

A page of the UI given no key — or no port, where its own address cannot be the daemon's — connects to nothing and
shows a screen saying there is no daemon address or key, and how to give them to a page run against a daemon started by hand:
`?port=<port>&key=<key>` in the page's address, `VITE_DAEMON_PORT` and `VITE_DAEMON_KEY` for the dev server, or
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

- **What the process holders kept running is taken over first.** The sessions and shell terminals a holder still runs
  are adopted and keep running, and its other terminals are ended (see "What a daemon start takes over" in
  `docs/product/process-holders.md`). The exception is the holders a quit of the application left running: every
  start ends what they run instead (see "After a quit of the application" in the same doc).
- **Every other session a previous run left behind becomes *interrupted*.** No process from that run is adopted as a
  running session unless a holder runs it, whatever the stored status said. Archived sessions are left as they are.
- **While Resume sessions on start is on, every interrupted session is then resumed**, the ones the previous run left
  that no holder kept running and any interrupted earlier, a few at a time (see "Resuming the interrupted sessions at
  start" in `docs/product/archiving-and-resuming-sessions.md`).
- Per-session scratch space from previous runs is cleared, except the adopted sessions'.
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
"Losing the daemon connection" below), and a window closed into the background stays there. A daemon that ended this
way left its sessions and shell terminals running in their process holders, and the new daemon takes them over as it
starts; any session it does not take over is interrupted and resumes with a click (see the list above).
While the daemon is down there is nothing to keep running, so a close or quit meanwhile behaves as with no daemon (see
"Closing the window" and "Quitting" below). Nothing is started again once a quit or a restart is under way, nor, on
macOS, once an in-app update is installed and waits for the restart that applies it, including one installed while a
relaunch was pending: the installed update has already replaced the application, so a relaunch would pair the new
version's daemon with the running window of the old one. On Windows a downloaded update leaves the running version's
files in place until its installer runs as Octoboard quits, so a daemon that ends meanwhile is started again under the
rules above. There is no button to start the daemon by hand.

When the daemon is not started again — those 3 relaunches in the last 10 minutes are spent, on macOS an update is
installed and waits for its restart, or it ended with exit status 0, as on a requested shutdown, a termination signal or a
planned stop (see "The daemon on its own" below) — the application says so in an error alert titled "The daemon exited unexpectedly", which says that the daemon runs the user's sessions and terminals, that Octoboard cannot work without
it, and asks the user to restart Octoboard; it does not silently keep showing a stale session list. The alert treats it
as an unexpected failure, offering **Report issue**, with the detail behind **Error details** rather than in its
sentence (see "Recovery and unexpected errors" in `docs/product/error-alerts.md`): how the daemon ended (its exit
status, or a signal, or why the last relaunch failed) and, when the relaunches are spent, that it was restarted that
many times in the last 10 minutes and is not restarted again, or, when an update is installed (macOS), that the update is not
applied until Octoboard restarts and the daemon is not restarted before then. A daemon stopped by the application's own
quit or restart raises no alert (see "Quitting" below).

**The window appears already themed.** It is created hidden and shown only once the UI has applied the chosen appearance
(see "Light, dark and follow the system" in `docs/product/appearance.md`), so a launch never flashes the operating
system's own appearance first. The window therefore appears a fraction of a second later than it otherwise would. Should
the UI never get that far at all — its bundle failing to load, say — the shell shows the window regardless **4 seconds**
after creating it, so a failed start never leaves a running application with no window on screen. A window the user has
by then closed into the background stays closed. That fallback is not armed at a launch the system made for **Launch at
login**, and such a launch starts hidden, as if its window had been closed into the background, while keeping Octoboard
running in the background is on (see "A launch at login" in `docs/product/launch-at-login.md`).

**Closing the window during a launch follows the user's choice.** The application reads whether to keep running in the
background from its own settings file as it starts, before the window's UI has loaded (see "Where the choice is
stored" under "Closing the window" below), so a close in the first moments of a launch closes the window into the
background or quits as the switch says. A close on the screen shown when the daemon failed to start quits, as any close
does while the daemon is not running.

## Who can reach the daemon

The daemon has no login and no notion of who is connected: whoever reaches it with its key is let in, and every client
is the same client. The key keeps web pages out, not programs. Where the daemon's terminals run in process holders (see
`docs/product/process-holders.md`), each holder's registry file holds the secret of the holder's connection to the
daemon, which controls every terminal that holder runs; the file is the user's alone, so any program running as the
same user can read it. Each agent and its helpers carry the key the holder issued for that launch, in their environment
and their command lines (the MCP bridge's and the status line's among them, and Claude Code's own), which other local
users may be able to read on macOS and Linux. That key is accepted by the holder's routes for that session's helpers,
and by its MCP route for that launch's own MCP token alone, so it reaches the daemon's MCP tools as that session and no
other. Where holders cannot be set up (see "When holders cannot be set up" in `docs/product/process-holders.md`), the
daemon's own key is in the command lines and environment of the agents and helpers it starts. Either way, in the
desktop form **any program running on this machine as the same user can do everything the application can**: create
consoles and sessions, start agents, type into their terminals, and open a shell terminal and run any command in it
(see `docs/product/shell-terminals.md`). It is safe there only because the daemon listens on loopback, so whoever can
reach it already has the machine.

In the server form the same sentence reads **anyone who can reach the port can do everything** — start agents, type into
terminals and run any command in a shell terminal on the server, as the Unix user the daemon runs as — which is no
access model for a machine on a network. So the daemon as it is today must not be put beyond loopback, and nothing in it
lets it be: it binds to loopback and nowhere else, and it refuses any request whose `Host` names it by anything but a
loopback address. A reverse proxy in front of it does not change that: the daemon also refuses a request whose `Origin`
— which a browser sends on every WebSocket upgrade — is neither the application's, nor loopback, nor the same as `Host`,
so a proxy would have to rewrite both headers, and the daemon behind it would be open to whatever the proxy lets through
with nothing of its own to tell it apart but the key, which is one for every client and changes on every start, while
the credentials above lie within reach of processes on the server: each launch's key in its agent's and helpers'
environment and command lines, the secret of each holder's connection in a registry file any process of the same user
can read, and, where no holder could be set up, the daemon's own key in the command lines and environment of the agents
and helpers it starts. What the form still lacks is the whole of an access model: a credential that lasts and is kept
like one, checked on every route, the UI and the WebSocket upgrades included; one credential per device of the
owner's, so that a lost or retired device can be revoked without re-issuing the rest; and TLS, which a browser needs
before it will show a notification or touch the clipboard from a page at a network address at all. None of that exists
yet.

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
  up" above). The helpers of an agent a process holder runs carry a key of their own launch, issued by that holder,
  instead (see "Session helpers and the holder" in `docs/product/process-holders.md`); only where the terminals run in
  the daemon itself does it hand its own key to the agents' helpers, beside the port. A key is good only for the launch
  that made it: after any daemon start, a page or program holding an earlier one is refused until it is given the new
  one. A page of the UI carries the key in its own address, and never sends that address as a referrer, so the key
  does not travel with a link it opens or a request it makes.

## Losing the daemon connection

A dropped connection is retried automatically a couple of times with a short backoff. While that is happening a
banner says so. Once the attempts are spent, the banner offers a Retry button; nothing retries forever on its own.
Pressing Retry turns the banner back at once to saying it is reconnecting, without the button; a connection that
fails again is retried automatically as before, and Retry is offered again once those attempts are spent. Each new connection re-reads the whole state, so nothing has to be replayed by hand. A request still waiting for its
reply when the connection closes, or when it is reopened, is never sent again on the new connection: it fails, saying
that the connection closed, or that it was reopened before the reply arrived.

**A connection that dies silently.** A connection can die with no close on either side — a network path that drops,
a machine that sleeps — so each side watches the other for silence. While connected, the window sends the daemon a
heartbeat every **15 seconds**, which the daemon answers; once nothing at all has arrived from the daemon for **45
seconds**, the window takes the connection as lost without waiting for the socket to report a close, and everything
above follows as for a connection that closed: the banner, the automatic retries and then Retry, the waiting requests
failing as closed, and the new connection re-reading the whole state. Time during which the window's timers were held
back — a hidden window or page, a machine asleep — is not counted as silence: the 45 seconds are measured afresh from
when the timers run again, and a system clock set back is taken the same way. The daemon, for its part, pings every
client every **15 seconds** and drops a client that has sent it nothing for **45 seconds**, releasing what it held for
that connection; the browser or webview answers those pings itself, so a client that has nothing else to send stays
connected. Both run on every connection, a loopback one included; they are what notices a dead connection through the
SSH port forward described in "Who can reach the daemon" above, whose network path can drop with no close reaching
either end.

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

## A daemon of another version

The window and the daemon speak a protocol with a version of its own, which the two must share. Every connection to
the daemon opens with the daemon greeting it: the protocol version it speaks and its own release version. The window
sends nothing on a connection, and does not count it as connected, until that greeting has arrived and named the
window's own protocol version; requests made meanwhile wait as they do while connecting.

When the greeting names another protocol version, or the daemon sends its state with no greeting before it (a daemon
older than the greeting), the window reads nothing more from it on a guess: it closes the connection for good, never
reconnects, and fails any request still waiting, saying that the connection closed. In its place the whole window shows
the screen for a daemon of another version (see "The window chrome and the content panel" in
`docs/product/window-layout.md`), even when the window was already in use on an earlier connection: nothing the window
had shown stays. No connection banner shows and there is no Retry, since another attempt would meet the same daemon.
The way out is the screen's: in the desktop application, quitting and opening Octoboard again, which brings up the
daemon bundled with it; in a browser, reloading the page.

A quit while that screen is shown — its own **Quit Octoboard**, or any of the shell's quit gestures (see "Quitting"
below) — quits at once, asking nothing about running sessions or terminals: that daemon is not talked to.

## Closing the window

This section and "Quitting" below are the desktop form's. A browser tab has no close that Octoboard sees, and nothing
in a browser quits anything (see "The daemon on its own" below).

**The window's close button, `Cmd+W` and the Window menu's Close Window item are one gesture**, and so, in the Windows
application, which has no application menu, are the close button of the system's title bar and `Alt+F4`. They all do
the same thing, deliberately, and what they do is the user's choice: the **Keep running in the background when the
window is closed** switch in Settings' App behavior section, which is **off to begin with** (see "Keep running in the
background" in `docs/product/app-behavior-settings.md`).

**With the switch off, closing the window quits.** It is an ordinary quit gesture, taking exactly the path `Cmd+Q` takes
(see "Quitting" below): with a session starting, working, waiting for the user, dormant or working in background, or a
busy shell terminal, it asks the same confirmation, where Cancel leaves the window open and everything running, and
otherwise it quits at once. The sessions a confirmed close ends are left *interrupted* and resumable, just as any confirmed quit
leaves them (see "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`).

**With the switch on, closing the window closes it into the background**: the window is hidden — on macOS Octoboard
leaves the Dock and the focus passes to the application used before it; on Windows the window leaves the taskbar. The
daemon, every session and its agent process keep running, and so does the UI behind the hidden window: a session that
raises its hand still notifies (see "The raised hand" in `docs/product/session-statuses.md`), on macOS the Dock
badge's count is kept up to date for when Octoboard returns to the Dock, and the menu bar icon's menu — the
notification-area icon's on Windows — keeps following the sessions (see `docs/product/menu-bar-icon.md`).

- **In native fullscreen** on macOS the window first leaves fullscreen and is hidden once that is done, about 0.8
  seconds later, so no empty fullscreen space is left behind. Brought back within that time, it is not hidden.
- **When the daemon is not running** — it failed to start, or has exited, or is being started again — there is nothing
  to keep running, so closing the window quits at once and asks nothing, whatever the switch says.
- Hide Octoboard (`Cmd+H`) is not a close: it hides the application as macOS always does, and Octoboard stays in the
  Dock.
- The menu bar icon is there whatever the switch says, unless the user has hidden it (see "Hiding the icon" in
  `docs/product/menu-bar-icon.md`); a window closed into the background while it is hidden comes back by the other
  ways below.
- Closing the window is never a way to force a quit through: see "Quitting when the window has stopped responding"
  below.

**Where the choice is stored.** The desktop application keeps it itself, in its settings file beside the window's
state, together with whether the menu bar icon is shown (see "Files Octoboard owns" below, which says what happens when
the file is missing, unreadable or cannot be written): the daemon neither stores it nor carries it, and it is not among
the files under `~/.octoboard`. The application reads it as it starts, before the window's UI has loaded, so it holds
from the first moment of a launch; turning the switch takes effect at once and is written straight away. It does not
travel between two machines reaching the same daemon, and clearing the window's browser storage leaves it as it is. A
configuration export that includes the App behavior section carries the exporting application's choice, and an import
asked to apply that section sets it on the application that runs the import, as turning the switch would; a browser, which has
no such choice, neither writes nor applies it (see "The settings a client holds" in
`docs/product/settings-in-a-configuration.md`).

**Bringing a window closed into the background back.** Any of these brings the window back where it was, at the size
and position it had:

- the menu bar icon's menu, while the icon is shown: its entry for opening the board or one of its sessions (see
  "Clicking the icon" and "The icon's menu" in `docs/product/menu-bar-icon.md`);
- opening Octoboard again while it runs — on macOS from the Finder, Spotlight or `open`; on Windows any launch of it,
  such as from the Start menu, which starts no second Octoboard but brings the running one's window forward (once its
  window exists: during the first moments of a launch there is none yet to bring);
- anything else that activates Octoboard, such as clicking one of its notifications;
- a quit that has to ask for confirmation (see "Quitting" below);
- the daemon exiting and not being started again: the window comes back showing the notice that it exited (see
  "Starting up" above). A daemon that exits while the window is shown only shows the notice, and does not take the
  focus from whatever the user is doing. A daemon that is started again brings nothing back.

Octoboard then comes to the front — on macOS it returns to the Dock, and its Dock badge shows the current waiting count
again.

## Quitting

Every way of quitting the application behaves the same: `Cmd+Q`, the application menu's Quit, the menu bar icon's quit
entry (see "Quitting completely" in `docs/product/menu-bar-icon.md`), the Dock icon's own Quit and, while keeping
Octoboard running in the background is off, closing the window: the close button, `Cmd+W` and the Window menu's Close
Window item (see "Closing the window" above). The Windows application has no application menu and no `Cmd+Q`: it is
quit through the notification-area icon's quit entry, or by closing the window while keeping it running in the
background is off. The difference is the escape hatch in "Quitting when the window has stopped responding" below, which
the close gestures have no part in. The system logging out, restarting or shutting down is not one of these quits and
never asks (see "When the system ends the session" below). Nothing below asks while the window shows a daemon of
another version (see "A daemon of another version" above).

- **While any session is *starting*, *working*, *waiting for the user*, *dormant* or *working in background*, quitting
  asks for confirmation.** One working in background counts because the quit would also end the work it left running
  (see "Working in background" in `docs/product/session-statuses.md`), and so does a *paused* one that still has work
  running in the background; a dormant one counts because its background agents
  may be at work (see "Dormant" in `docs/product/session-statuses.md`). Any other session at its prompt — *paused*,
  *done*, *awaiting instructions* or with *child sessions working* — does not count by itself: it is in the middle of nothing that is
  reported, and the quit still leaves it interrupted and resumable like the others. One with child sessions working
  holds the quit up only through the busy sessions under it, which count by their own status.
  The message says that quitting interrupts those sessions and that each stays resumable next time.
- **While any shell terminal is busy, quitting asks for confirmation too** (see "Busy shells" in
  `docs/product/shell-terminals.md`); an idle shell terminal, or one whose shell has exited, does not count. With only
  busy shell terminals holding the quit, the message says some terminals are still running something and quitting ends
  what they run; with sessions and busy shell terminals both, it says both are still running, that quitting interrupts
  the sessions, each resumable next time, and ends what the terminals run.
- Asking brings Octoboard to the front first, restoring its window if it was minimized and bringing it back if it was
  closed into the background, so a quit from outside the window — the Dock icon's own Quit while another application
  covers it, or the menu bar icon's Quit while the window is closed — does not leave the question unseen.
- Once confirmed, every session process is ended and every one of those sessions is left **interrupted**, never
  archived. Clicking one next time resumes it (see "Archiving, interruption and resuming" in
  `docs/product/archiving-and-resuming-sessions.md`). Every shell terminal ends too (see "Closing a shell terminal" in
  `docs/product/shell-terminals.md`).
- **Once the daemon has begun to shut down, nothing new starts.** From then on — a confirmed quit or restart, or a
  termination signal (see "The daemon on its own" below), or a planned stop (see "Planned restart and planned stop"
  below) — no session is launched, resumed, reopened, switched to
  another account or handed off, and nothing else that needs a session to itself (a transfer of its branches, a change
  of its owner) is begun, and no update of an agent's CLI is started (see "Updating" in
  `docs/product/agent-cli-updates.md`): each is refused with `daemon_shutting_down`. A launch that was already past that check when
  the shutdown began is ended as soon as its process comes up, leaving the session *interrupted* like the rest. Where a
  window still shows such a refusal, it is an error alert saying that the daemon is stopping and to try again after it
  restarts, with no remedy and no **Report issue**; the wording names no operation, since a planned stop refuses more
  than a launch (see "While the terminals are handed over" below). An automatic account switch, or its restore of the session, that
  the shutdown cuts short tells nobody (see "When the switch fails" in `docs/product/automatic-account-switch.md`).
- With no session *starting*, *working*, *waiting for the user*, *dormant* or *working in background* — none at all, or
  only ones otherwise at their prompt — and no busy shell terminal, quitting is immediate and asks nothing, and a window
  closed into the background is not brought back for it.
- While the daemon is not running — it failed to start, or has exited, or is being started again — every quit is
  immediate and asks nothing, and no daemon is started again after it.
- On the screen for a UI that failed (see "The window chrome and the content panel" in
  `docs/product/window-layout.md`), every quit is immediate and asks nothing, even while sessions are running or a
  shell terminal is busy, since the window no longer knows them; the daemon is not asked to shut down first. It stops
  once the application is gone, with the same ordinary stop as a confirmed quit (see "The quit note" below). On
  Windows, while a downloaded update is held, that quit stops the daemon first and then starts the update's installer
  without relaunching Octoboard (see "Downloading and installing" in `docs/product/in-app-updates.md`).
- The daemon stopping as part of a quit is not reported as the daemon exiting: no notice is shown and a closed window is
  not brought back for it.
- Quitting does not depend on the daemon answering: if it does not, the application exits anyway after a short wait,
  and the daemon still makes its ordinary stop once the application is gone (see "The quit note" below).
- If the application cannot end itself once the quit is under way, an error alert titled "Could not quit" says
  Octoboard kept running instead of ending and asks the user to try quitting it again, and the window stays open.
  The alert treats it as an unexpected failure, offering **Report issue**, with the reason behind **Error details**.
- Until the window has loaded far enough to be able to ask, a quit is never held back — a window that never gets as
  far as showing the confirmation can still be quit.

### The quit note

**Every quit tells the daemon that the application is quitting before the application ends**, without waiting for an
answer: a confirmed quit, a quit that asks nothing because nothing holds it up, the quit from the screen for a UI that
failed, the escape hatch in "Quitting when the window has stopped responding" below, a quit whose stop the daemon does
not answer, and a quit while the daemon is not running. It does so in two steps, in this order: it leaves the quit
record (below), then writes the note to the daemon's standard input, which lands even when the daemon does not answer.
A quit from the window — a confirmed one, or one that asks nothing — takes both steps before it asks the daemon to shut
down, so a daemon that ends in between still leaves the quit behind. A daemon that finds the application gone after the
note makes its ordinary stop: every session's process ended and every one of those sessions left *interrupted*, its
preview frozen, as a confirmed quit leaves them. A daemon that finds the application gone with no note makes a planned
stop instead (see "Crashes and forced termination" below). The restart into an update writes neither the record nor the
note (see "Restarting" below). A note the daemon reads while a planned stop is asked for or under way does not wait for
the application to be gone: it asks for the ordinary stop at once, which takes the place of the planned stop or cuts it
short (see "An ordinary stop during a planned stop" below), and a planned stop asked for after the note is made
ordinary too. It is the one word that reaches a daemon in a planned stop on every host, since the daemon accepts no new
connection by then.

**The quit record** is what makes the quit hold whatever became of the note: the file `~/.octoboard/application-quit`,
written whole, whether or not a daemon is running and whether or not it takes the note. It is left whenever the
application had a daemon of its own during this run that may run process holders — one running or still starting, one
that ran earlier in this launch, or one it stopped while that still ran. A launch whose every daemon exited on its own
before it reported its port and key, as one does when another daemon holds the data directory (see "One Octoboard at a
time" above), leaves none, so its quit ends nothing of that other daemon's. Every daemon start, whether the application
started it or not, reads the record and ends the holders the quit left, with what they run, instead of taking them over,
so the quit still interrupts every session when the daemon that was told ended first, never read the note, was killed,
or was not running at all (see "After a quit of the application" in `docs/product/process-holders.md`). The system
ending the user's session leaves the record too (see "When the system ends the session" below).

### When the system ends the session

A logout, restart or shutdown of the system is not one of the quits above: **it never asks for confirmation**, whatever
is running and whatever Resume sessions on start says, and the window is not brought back for it. The daemon makes its
ordinary stop before the system goes on: every session is left *interrupted* with its preview frozen and every shell
terminal ends, as in a confirmed quit, and the sessions are resumed at the next start while Resume sessions on start is
on (see "Resuming the interrupted sessions at start" in `docs/product/archiving-and-resuming-sessions.md`).

- **On macOS** the application stops the daemon before it answers the system: the quit record, the quit note and the
  daemon's shutdown request go out at once, and a daemon still running is killed, all within about **5 seconds**. Octoboard tells such a
  quit by the reason the system gives with it; a quit request that gives none of those reasons asks as any quit does,
  and so does every quit the user makes — `Cmd+Q`, the menus' Quit, the Dock icon's Quit, closing the window.
- **On Windows** the system signing the user out or shutting down sends the daemon no notice of its own (see "Crashes
  and forced termination" below); the application passes the system's end of the session on to it, with the same stop
  within about **4 seconds**, as far as the time the system allows. It installs no downloaded update, which is offered
  again after the next start (see "Downloading and installing" in `docs/product/in-app-updates.md`).
- **During a restart into an update** the end of the session wins, and the stop is still ordinary. The application
  leaves the quit record and writes the quit note, which a restart otherwise never does, so the daemon's stop is made
  ordinary whether its planned stop is asked for, under way or still to be asked for (see "An ordinary stop during a
  planned stop" below). It does not wait for the planned stop: within the same seconds it sends the daemon the termination
  signal (macOS; on Windows the note is the one notice) and the shutdown request, and kills a daemon still running at
  the end. Once the end of the session has begun, a restart that has not yet started the update's installer or the new
  launch starts neither, and nor does a quit installing a downloaded update (Windows).

### Restarting

Octoboard restarts itself only to apply an installed update, when the user presses **Restart now** (see "Restarting to
apply an update" in `docs/product/in-app-updates.md`). **A restart is no quit: it hands every session and shell terminal
over to the new version**, which takes them over running, with their screens exactly as they were wherever the saved
screen can reproduce them (see "The screen of a terminal handed over" in `docs/product/process-holders.md`). No session
is interrupted, no preview is frozen, and nothing a session started is stopped.

- **What it would not carry over.** Before every restart, the window asks the daemon which running sessions and shell
  terminals the new version would not take over, giving it the update's holder floor and the update's version (see
  "Where updates come from" in `docs/product/in-app-updates.md`): the new version takes over the process holders whose
  version lies from the floor up to its own, both included. Not carried over are those whose holder's version lies
  outside that range or is not known, and those whose terminal runs in no holder (see "Holders of other builds" and
  "When holders cannot be set up" in `docs/product/process-holders.md`). The shell terminals of a holder the daemon
  cannot connect to at the moment (see "A holder that cannot be connected to" in the same doc) that would not be carried
  over are listed apart, as terminals that cannot be reached, since whether one is busy is not known. Without both the
  floor and the version — an update whose manifest names no floor, or one this window never saw being installed — the
  daemon cannot tell, and its answer counts as none. The daemon is given at most **2 seconds** to answer.
- **With an answer, it asks for confirmation whenever the restart would interrupt something**: any session the answer
  lists, whatever it is doing (idle, waiting for the user or working alike, and one whose holder cannot be reached),
  a listed shell terminal that is busy, a listed shell terminal the window does not know (counted as busy), or a shell
  terminal that cannot be reached. A listed shell terminal at its prompt does not ask by itself, but ends all the same.
  An answer that lists nothing restarts at once, asking nothing.
- **The confirmation** is titled as a restart and its confirming button reads Restart; Cancel leaves everything
  running. It names the sessions it would interrupt by title, up to five, or four followed by "and N more" past five;
  listed sessions the window cannot name are counted in "and N more", and when it can name none its message states how
  many sessions restarting interrupts. Its message says that restarting interrupts those sessions, each resumable after
  the restart, and, when a busy shell terminal is among what it would not carry over, that it ends what the terminals
  run; with only busy shell terminals and no session, it says some terminals are still running something and
  restarting ends what they run; with only terminals that cannot be reached, it says some terminals cannot be reached
  right now and restarting may end what they run. Otherwise, a line says that terminals that cannot be reached may end
  too, where there are any. It says that everything else keeps running only when nothing it would not carry over goes
  unmentioned: no listed shell terminal at its prompt and none that cannot be reached.
- **With no answer** — the daemon does not answer in time, the answer cannot be had, or the daemon cannot tell — it asks
  whenever a session's process is running, whatever it is doing, or a shell terminal is busy, and for the terminals
  that cannot be reached where the daemon's answer listed any. It names every session whose process is running, and
  its message says it could not check what keeps running through the update, and that those sessions may be
  interrupted and what the busy terminals run may end (with only terminals that cannot be reached, the message is the
  one above for them); it never says that everything else keeps running.
- A quit gesture arriving while the restart's confirmation is open, or while the window is still working out whether to
  ask, turns it into the quit's confirmation, or the quit.
- **Once confirmed, or at once when nothing asks**, the window asks the daemon for a planned stop (see "Planned restart
  and planned stop" below), and Octoboard starts again; the window's size and position are saved. The daemon is
  stopped before the new launch, since the new launch's daemon would otherwise refuse to start while the old one holds
  the data directory (see "Starting up" above): a daemon still running **2 seconds** later is sent the planned-stop
  request directly, which is sent once more a second later when it could not be sent, and never replaced by a
  termination signal, which would make an ordinary stop; no such request is sent once the system has begun ending the
  user's session (see "When the system ends the session" above). A daemon still running **30 seconds** after that is killed;
  its sessions and shell terminals are then left running in their holders as after a daemon crash, and the new
  version's daemon takes them over with their screens rebuilt and marked inexact (see "The screen of a terminal taken
  over" in `docs/product/process-holders.md`).
- **The new version's daemon takes everything over.** On macOS the update has already replaced the application, and
  the new application's daemon adopts the holders, those of the previous build included, within its compatible range.
  On Windows the update's installer runs only after the planned stop, while the holders run on from their copy of the
  build (see "The copy of the build holders run from" in `docs/product/process-holders.md`), and the installer starts
  the new version. What the new daemon does not take over it ends, and those sessions are left *interrupted*.
- While a confirmed restart stops the daemon and, on Windows, starts the update's installer, another restart starts
  nothing, and a quit made meanwhile through the menu, the tray or the window is held back without asking anything while
  the restart goes ahead. On macOS the Dock's Quit is turned back, and the restart goes on. The system logging out,
  restarting or shutting down meanwhile is not held back (see "When the system ends the session" above).
- If the application cannot restart, an error alert titled "Could not restart" says Octoboard kept running instead of
  relaunching and asks the user to quit it and open it again. It is treated as an unexpected failure, as a failed quit
  is.

### Quitting when the window has stopped responding

A confirmation dialog that can no longer be shown or answered — the window's process died, or its page hung — would
otherwise leave no way out short of a force quit. So: **when a quit gesture has been deferred to the confirmation
dialog and a second quit gesture arrives within 2 seconds, that second gesture quits immediately and asks nothing.**
The daemon is not asked to shut down first, but it is told of the quit and makes its ordinary stop once the
application is gone, so every session is left *interrupted*, as a confirmed quit leaves it (see "The quit note"
above). On Windows, while a
downloaded update is held, that quit stops the daemon first and then starts the update's installer without relaunching
Octoboard (see "Downloading and installing" in `docs/product/in-app-updates.md`).

A window that is still answering resets that 2-second window every time it handles a quit gesture, so pressing `Cmd+Q`
twice in quick succession on a working Octoboard does **not** skip the confirmation. The escape hatch opens only when
nothing answered the first gesture. It is known to hold for scripted quits; with the page frozen, two `Cmd+Q` key
presses have been seen not to take it, for a reason not established, so the menu's Quit is the surer way out.

**Closing the window has no part in this, even when it is a quit** (see "Closing the window" above). A double press
on a close button is an ordinary gesture and must not end a live session unasked, so the close button, `Cmd+W`, Close
Window and `Alt+F4` never take this escape hatch however often they are repeated, and never open it for the gesture
after them either. `Cmd+Q`, the application menu's Quit, the menu bar icon's Quit (the notification-area icon's on
Windows) or the Dock icon's Quit — twice — is the way out of that state.

## Crashes and forced termination

**A crash of the application, or killing it outright, leaves every session running.** The application wrote no quit
note (see "The quit note" above), so the daemon, finding it gone, makes a planned stop (see "Planned restart and planned
stop" below): every session and shell terminal is handed over to its process holder with its screen saved, and the next
launch takes every one of them over, with its screen exactly as it was. Until then they run on in their holders.

- The daemon stops by itself once the application is gone, whether or not it was asked to: with its ordinary stop
  after the quit note, with a planned stop without it. That is a watch the application asks for when it starts the
  daemon; a daemon started on its own has no application to follow (see "The daemon on its own" below).
- **Agent processes and shell terminals outlive a daemon that goes down without stopping.** They run in process
  holders, not in the daemon: a daemon that crashes, goes down on an internal failure or is killed outright leaves them
  running there, and the next daemon start takes them over (see "Which daemon ends leave the programs running" and
  "What a daemon start takes over" in `docs/product/process-holders.md`). Only an ordinary stop — a quit, the daemon's
  shutdown request, a termination signal, or the daemon finding the application gone after its quit note — ends them,
  as "Quitting" above describes; a planned stop hands them to the next daemon instead (see "Planned restart and planned
  stop" below).
  Where holders cannot be set up and the terminals run in the daemon itself, the daemon ends them on its way out,
  including when it goes down on an internal failure. An update of an agent's CLI is the exception: it runs on to
  its end however the daemon ends (see "Updating" in `docs/product/agent-cli-updates.md`).
- A normal process exit stops the launch's attributed background processes, subject to the user's keeps and the
  boundaries in "Automatic stopping and its boundaries" in `docs/product/background-processes.md`. On an internal
  failure of the process the terminals run in — a holder, or the daemon where they run in it — every process in each
  agent's terminal session is also killed, helpers in process groups of their own included; on Windows, every process
  of the launch's job object attached to the agent's console. At the next daemon start, saved process records allow
  cleanup of attributed processes that survived outside those terminal sessions, while preserving kept processes.
- **On Windows a sign-out or a shutdown ends the daemon together with the application and the user's other
  processes**, its process holders, agents and shell terminals among them. Windows sends the daemon no notice of
  either — it withholds them from a program that has loaded the system's window libraries, as the daemon has. The
  desktop application passes the system's end of the session on to the daemon it started, which makes its ordinary
  stop as far as the time the system allows (see "When the system ends the session" above); a daemon started on its
  own is told nothing, logs no stop, and does not stop its sessions in order. The next daemon start finds no holder
  left to take over: every non-archived session is marked interrupted and resumes with a click (see "Starting up"),
  and the saved background-process records are reconciled, stopping any attributed process they name that is still
  running and leaving kept ones alone (see "Automatic stopping and its boundaries" in
  `docs/product/background-processes.md`), though the system has normally ended those processes already.
- Should a session's process survive anywhere but in a holder the next start takes over, that start still marks its
  session interrupted (see "Starting up"), so the session list never claims a session is running that this Octoboard
  does not own.

## The daemon on its own

In the server form there is no application: the daemon is the whole of Octoboard on the server, and its lifetime is
for whoever runs the server to manage. What follows is what the daemon does today when it is started by hand, with
nothing that the form will need and does not have yet claimed to exist.

- **Starting and stopping are the administrator's.** Nothing starts the daemon but the person or the service manager
  told to, and nothing stops it but a termination signal — `SIGTERM`; `SIGINT`, which `Ctrl+C` in the terminal it was
  started from sends; or `SIGHUP`, which that terminal sends as it closes. The daemon answers any of them exactly as
  the desktop form's quit does once confirmed: every session's process is ended and every one of those sessions is
  left *interrupted*, never archived, every shell terminal ends, and nobody is asked first; it then exits with
  status 0. On Windows, which has no signals, the same stop follows `Ctrl+C` or `Ctrl+Break` in the console window the
  daemon was started in, and closing that window; after a close, Windows ends the daemon a few seconds later whatever
  it is doing, so the stop has those seconds. A sign-out or a shutdown is not among these: Windows ends the daemon
  without telling it, as "Crashes and forced termination" above describes. Besides those, a planned restart or a
  planned stop, asked for on purpose, stops it, and is the one stop that leaves the sessions running (see "Planned
  restart and planned stop" below).
  Started without an application to follow, the daemon runs until it is stopped; exiting with the
  application is a watch the desktop form asks for and the server form never does. Nothing in the daemon starts it
  again when it dies: that is the desktop application's (see "Starting up" above), or a service manager's (see
  "Running as a systemd user service" below). The command line can start it in the background, stop it, restart it and
  say where it listens (see "Running in the background from the command line" below).
- **A browser is not the application.** Closing a tab, or every tab, ends nothing: the daemon, every session and its
  agent process, and every shell terminal run on until the daemon is stopped, and a browser opened later finds them
  where they were. There is no
  quit gesture in a browser, no confirmation, and no background switch (see "Closing the window" above).
- **A stop and a start interrupt every session; a planned restart does not.** A daemon stopped by a termination
  signal ends every session's process, and what "Starting up" says of a daemon start holds unchanged, whichever browser
  or device a session was opened from; each resumes with a click, as after any quit. A planned restart
  (`octoboardd restart`) instead leaves every session and shell terminal running in its process holder and has the next
  daemon take them over with their screens as they were (see "Planned restart and planned stop" below), except under
  the systemd unit, where the sessions do not survive it yet (see "Running as a systemd user service" below). A daemon
  that goes down without stopping — killed outright, say — leaves its sessions and shell terminals running in their
  process holders for the next start to take over too, as "Crashes and forced termination" above describes, their
  screens rebuilt rather than kept.
- **One daemon per data directory, and the data directory is the owner's.** The lock in "Starting up" is per
  `~/.octoboard`, so per Unix user on the server; the daemon's consoles, projects and sessions are that user's, and its
  agents are launched under that user's own agent accounts, with that user's logins and keys — every client of the
  daemon acts as that user.

### Choosing the port

`octoboardd --port <port>` runs the daemon in the foreground listening on `127.0.0.1:<port>`, the same port on every
start given that option, so an address can be bookmarked and an SSH port forward
(`ssh -L <port>:127.0.0.1:<port> <server>`) written once. The port is a number from 1 to 65535; anything else, 0
included, is a usage error. Without the option the operating system assigns the port, as in the desktop form, which
never gives one. Either way the startup line and the port file name the port (see "Starting up" above).

**A port already in use fails the start.** The daemon exits with a non-zero status and an error saying that port is
already in use, before it touches any session: the sessions of the previous run stay as they were stored. A daemon
already running against the same data directory is refused by the lock before the port is tried (see "Starting up"
above).

The daemon's own options, `--port`, `--parent-pid` and `--service-managed` (see "Planned restart and planned stop"
below), are not accepted together with a mode (`start`, `stop`, `restart`, `status`
or the helper modes the daemon starts for its agents): such a command line is a usage error.

### Running in the background from the command line

Four commands act on the daemon running against the data directory of the user who runs them (`~/.octoboard`),
whoever started that daemon, the desktop application included. Whether a daemon runs is told by `daemon.lock` alone;
which process it is and where it listens, by `daemon.owner` (see "Files Octoboard owns" below). Checking takes the
lock for an instant, so a daemon starting at that very instant refuses to start. Running in the background is not
supported on Windows: there `start` refuses, saying so, with exit status 1, and so do `stop` and `restart` when a
daemon runs; `status` works.

- **`octoboardd start --port <port>`** starts the daemon in the background on that port; `--port` is required, with the
  same range as above. While a daemon already runs it refuses, saying so with that daemon's pid and address where they
  are known, and exits with status 1. Otherwise the daemon is started with `--port <port>` in a session of its own,
  with no controlling terminal and the data directory as its working directory, so closing the terminal or the login
  that ran `start` sends it nothing and it runs until it is stopped. `start` waits up to **30 seconds** for the
  daemon's startup line and prints it unchanged — **the key included**, so whoever sees that output, or a log of it,
  holds the key — followed by a line with the daemon's pid and its address, `http://127.0.0.1:<port>`, and exits with
  status 0. A daemon that ends before reporting (its port taken, say, or another daemon took the lock first) makes
  `start` say that it ended while starting, with its exit status, followed by what it wrote on its error output (the
  first 16 KiB), and exit with status 1; one that has reported nothing after 30 seconds is killed, with the same
  report. Once `start` has returned, what the daemon writes on its standard output and error output goes nowhere; its
  log is still `~/.octoboard/logs/daemon.log`. On a host whose logind is configured with `KillUserProcesses=yes`, the
  system ends every process of a login when the login closes, a daemon `start` started included; run it as a service
  there (see "Running as a systemd user service" below).
- **`octoboardd stop`** with no daemon running says so and exits with status 0. Otherwise it sends `SIGTERM` to the
  process `daemon.owner` names — that process, checked by its start time, never another that has taken its pid since —
  which stops as a termination signal has it stop (see "The daemon on its own" above): every session interrupted,
  every shell terminal ended. It then waits up to **60 seconds** for the daemon to be gone and exits with status 0; a
  daemon still running then makes it say so, with the pid, and exit with status 1, while the stop goes on. A running
  daemon that no usable `daemon.owner` names — it is still starting or already stopping, or could not write the record
  — is not stopped: `stop` says so and exits with status 1. A desktop application whose daemon is stopped this way
  does not start it again and says the daemon exited, as for any daemon that ends with status 0 (see "Starting up"
  above).
- **`octoboardd restart`** asks the daemon for a planned restart (see "Planned restart and planned stop" below) and
  takes no option. With no daemon running it says so and exits with status 1. It also exits with status 1, restarting
  nothing, for a running daemon that no usable `daemon.owner` names (as `stop` does); for one that follows the desktop
  application, saying that the application restarts it itself; for one of an earlier build, or one that could not install its handler for the request, whose record does not
  say whether it follows an application and which the request would end rather than restart, saying to stop and start it
  instead; and on Windows, saying that restarting is not supported there. Otherwise it sends the request to the
  process `daemon.owner` names, checked by its start time as `stop` checks it, as the signal `SIGUSR1`, so that the
  command line never holds the daemon's key; a daemon that follows an application refuses the signal too, and logs
  that it did. It then waits up to **90 seconds** for another daemon to hold `daemon.lock` and record itself in
  `daemon.owner`, prints a line with the new daemon's pid and its address, `http://127.0.0.1:<port>`, and exits with
  status 0. It prints no key: the next daemon keeps the old one where the daemon starts it itself, and a successor
  under systemd makes a new one (see below). When no daemon has taken over by then, it says that the daemon is still
  stopping, with its pid, or that it stopped but no daemon took over, that the daemon's log says why and that the
  sessions are kept for the next start, and exits with status 1; one that is gone before the request reaches it makes
  it say so and exit with status 1 too.
- **`octoboardd status`** says that the daemon is running, with its pid and its address, `http://127.0.0.1:<port>`, or,
  when no usable `daemon.owner` names it, that it is running at an address not known, and exits with status 0; with
  none running it says so and exits with status **3**. A user for whom no daemon has ever run has no data directory, and
  `status` creates none.

### Planned restart and planned stop

A **planned restart** stops the daemon without ending anything: every session and shell terminal is handed over to the
process holder it runs in, with its screen saved, and the next daemon comes up on the same port and takes them over
with their screens as they were, exactly wherever the saved screen can reproduce them (see "The screen of a terminal
handed over" in `docs/product/process-holders.md`). A **planned stop** is the same handover with no next daemon: the
daemon exits with status 0, and the next daemon started — by hand, by `octoboardd start` or by the desktop application —
takes everything over as exactly.

**Asking for one.** A planned restart is asked for by `octoboardd restart` (see "Running in the background from the
command line" above), by the signal `SIGUSR1` it sends, or by the control route `POST /restart`, which answers `202` at
once. A planned stop is asked for by the control route `POST /planned-stop` or the WebSocket request `planned_stop`;
both answer at once and neither is ever refused. The desktop application asks for one to restart into an update (see
"Restarting" above), and a daemon that follows the application makes one itself when it finds the application gone
with no quit note (see "Crashes and forced termination" above). A planned restart is refused, and nothing is stopped, for a daemon
that follows the desktop application, which starts the daemon it talks to itself (`POST /restart` answers `409` with
the reason `follows_application`), and where no next daemon could come up: on Windows, where the daemon cannot run in
the background (`no_successor`). A planned stop or restart asked for while an ordinary stop (the shutdown request, a
termination signal) is already asked for does not replace it, and a planned stop asked for after the desktop
application's quit note is made ordinary (see "The quit note" above); an ordinary stop asked for before a planned one has
begun replaces it, and one asked for once it has begun cuts it short (see "An ordinary stop during a planned stop"
below).

**Who starts the next daemon.** A daemon no service manager restarts starts the next one itself once it has handed
everything over and let go of `daemon.lock`: in the background, as `octoboardd start` does, on the same port and with
the same key, so a page or program that held the old address and key reaches the new daemon unchanged. A window open on
it loses its connection meanwhile and reconnects as after any dropped connection (see "Losing the daemon connection"
above). The next daemon's startup line, which carries the key, is read by the stopping daemon and neither shown nor logged.
The next daemon is started from the path the daemon itself was started by — the command's own path, made absolute
(looked up in `PATH` for a bare name), its last symbolic link not followed — when that path led to the very file the
daemon was running as it started, and otherwise from the file it runs; `octoboardd start` starts its daemon from such a
path too. So a symbolic link pointed at a newer build since, or a newer file put in place at the same path, restarts
into that build, and a path that no longer leads to an executable fails the restart. A next daemon that has not
reported listening within **30 seconds** is killed; whenever the next daemon does not come up, the old one logs why and
exits with status 1, leaving the sessions and shell terminals running in their holders for the next start.

A daemon leaves the next start to a service manager only on evidence, decided once as it starts: the option
`--service-managed`, which the example systemd unit passes, or systemd naming the daemon's own process as the one it
started for a unit (`SYSTEMD_EXEC_PID` equal to the daemon's pid, with an `INVOCATION_ID`). A daemon started by hand
from a terminal that itself runs under systemd has neither. Such a daemon starts no next daemon: it exits with status
**75** after its planned stop, and the service is to start the next one (see "Running as a systemd user service"
below), which makes a new key. A service that passes `--service-managed` must restart the daemon on status 75. On
Windows `--service-managed` is accepted and ignored.

**What a planned stop keeps.** No session is marked *interrupted* and no session record changes; no shell terminal is
closed, and each keeps its record and the name the user gave it; nothing a session started is stopped, and no terminal
preview is saved (see "Files Octoboard owns" below); the sessions claimed for a holder that cannot be connected to stay
claimed. The account quota runs and the logins under way are stopped, and the hook events and program status reports
already received are applied, as an ordinary stop does. The messages queued for a running session and not yet written
into it are carried over to the next daemon (see "Messages held until a session can take them" in
`docs/product/message-delivery.md`). A holder that does not answer the handover within **5 seconds**,
or one of a build that takes no handover, has the terminals it had not marked by then left running as after a crash
(those marked before keep their saved screens); a link being opened again is waited for first, up to about **15
seconds**. The next daemon takes them over with their screens rebuilt (see "The screen of a terminal taken over" in `docs/product/process-holders.md`).
Where holders cannot be set up and the terminals run in the daemon itself, nothing can be handed over: a planned stop
then ends every session as an ordinary stop does, and a planned restart still starts the next daemon.

#### While the terminals are handed over

From the moment a planned stop begins, the connections already open are served until the daemon exits, but nothing new
starts, as once any shutdown has begun (see "Quitting" above), and nothing that would change or end a terminal, or
remove what one works in, is begun either, whichever client, agent tool or report asks for it. Each of these is refused
with `daemon_shutting_down`, with nothing changed:

- archiving or deleting sessions, an owner's archive tool included, and a `done` report that would archive its session,
  which is refused whole, so nothing in it is delivered;
- removing a project or a console;
- restarting a session for a change of its account, its model or its account's sharing of the default configuration,
  and handing a session off;
- stopping processes on request, and interrupting a turn;
- answering a session's folder-trust confirmation;
- starting or closing a shell terminal;
- a message for a session, once the messages still queued have been saved for the next daemon (see "Messages held until
  a session can take them" in `docs/product/message-delivery.md`).

Requests that only read are answered as ever, and the background clean-up of archived sessions does not run. An
operation already under way when the planned stop begins is waited for, up to **5 seconds**, with the processes it ends
and the branch removals it starts, before the terminals are handed over. One still running when that wait runs out goes
on without being waited for, but no later step of it that would start or end a terminal or a process, remove a branch
or move a folder to the Trash is taken: that step is refused with `daemon_shutting_down`, and what it would have ended
is left to the next daemon (see "Moving the project folder to the Trash" in `docs/product/removing-projects.md`).

On every terminal, from just before it is handed over, the clients no longer change its size, nor what the program is
told of their display or focus; the next daemon takes the clients' size when they reconnect. Until the
daemon parts from the holders, the answers to the queries in the output it read before the handover, its own and those
it left to the clients, still go into the program. Typing into a terminal once it is handed over is dropped: every
client is sent the error `daemon_shutting_down`, with no request id and no params, once for each terminal connection the
typing came over, in place of `terminal_input_lost` (see "Reconnecting" in `docs/product/session-terminal.md`), and the
window shows it as an error alert. A paste the handover cut part-way is lost typing, reported as `terminal_input_lost`.

The daemon removes `octoboardd.port` and `daemon.owner` as it exits, as on any stop.

#### An ordinary stop during a planned stop

Anything that asks for an ordinary stop while a planned stop is asked for or under way makes the stop ordinary: before
the planned stop has begun it takes its place, and once it has begun it cuts it short. What still reaches a daemon in a
planned stop: a termination signal (`octoboardd stop` sends one), or on Windows what stands for one (see "The daemon on
its own" above); the desktop application's quit note (see "The quit note" above); and the shutdown request on a
connection already open. No new connection is accepted by then, so a `POST /shutdown` sent on one does not arrive.

The planned stop stops where it stands: in its wait for the operations already under way (which is cut short, and what
is refused meanwhile is still refused), before any terminal is handed over (none is), or after (no further screen is
saved, and the screens saved by then are still written for the next start). The holders' marking of the terminals and a
parting already begun are finished first. Then the daemon stops as an ordinary stop does, leaving nothing for a next
daemon: every session's preview is kept first — the screen as it is now for a terminal still the daemon's, the screen up
to the handover for one handed over, since its holder is ended with what it runs and the output since the handover goes
with it — then every session is left *interrupted*, every shell terminal ends, and the holders that had terminals
handed over, that the daemon parted from or that it left to the next daemon are ended with everything they run. No next daemon is started, a
planned restart's included, and the daemon exits with status 0.

### Running as a systemd user service

The repository carries an example systemd user unit, `apps/daemon/dist/systemd/octoboardd.service`, for running the
daemon on a Linux server from boot. Its binary path (`%h/.local/bin/octoboardd`) and port (7531) are examples, and its
comments say how to install and enable it. What it sets:

- It runs the daemon **in the foreground** with `--port`, as the user who owns the unit, so that user's `~/.octoboard`,
  agent accounts and keys are the daemon's, and restarts it after a failure but not after a clean stop. It passes
  `--service-managed` and restarts the daemon after exit status 75 too, the status a planned restart then ends with, so
  that the next daemon runs in the unit (see "Planned restart and planned stop" above); a copy that drops the restart
  on 75 must drop the option with it.
- **Standard output is discarded**, so the startup line, which carries the key, never reaches the journal, where
  whoever may read the journal could read it. The error output, which carries the log and never the key, goes to the
  journal.
- **Stopping the unit** sends `SIGTERM` to the daemon alone, so that it ends its sessions in order, and allows
  **90 seconds** for it; systemd then kills whatever is still in the unit, kept background processes and processes
  left behind in a project included. `octoboardd stop` and `octoboardd status` work on a daemon the unit runs as on any
  other, and the unit does not start a daemon stopped that way again.
- **`Delegate=yes`** gives the daemon a cgroup subtree it may write, in which it holds each agent launch in a group of
  its own (see "Which processes belong to a launch" in `docs/product/background-processes.md`).
- **The process holders run in the unit too**, so the programs they keep end with it: a daemon the unit runs that
  crashes or is killed outright takes its sessions' processes with it, unlike elsewhere (see "How a holder is
  detached" in `docs/product/process-holders.md`). **So does a planned restart, for now**: systemd ends whatever is in
  the unit when the daemon exits, the holders and their programs included, before it starts the next daemon, which then
  finds nothing to take over. Every session is interrupted, and resumed while Resume sessions on start is on, and every
  shell terminal is gone, as after a stop and a start. Sessions survive a planned restart under the unit only once the
  holders run as user units of their own, which they do not yet.
- A user unit runs only while its user is logged in unless **lingering** is enabled for that user
  (`loginctl enable-linger <user>`); without it the daemon stops at logout and does not start at boot.

**A daemon the unit runs has no way yet to hand its key to its owner.** The key is only on the startup line, which the
unit discards, and `status` does not show it, so no client that needs the key — any page of the UI — can be given it.
The next daemon a planned restart has the unit start makes a new key as well.

## Files Octoboard owns

Everything Octoboard writes for itself lives under `~/.octoboard` — on the machine the daemon runs on, which in the
server form is the server — with two exceptions, the desktop application's window state and its settings file,
described after the table. On Windows the
directory is `%USERPROFILE%\.octoboard`, and the paths below are the same under it, with Windows' separator
(`%USERPROFILE%\.octoboard\logs\daemon.log`, say). A user name and home path outside ASCII (`C:\Users\测试用户`, say)
take nothing away: the data directory, its logs, agent launches and the hooks Octoboard gives the agents work under it
as under any other.

| Path | Contents |
|---|---|
| `~/.octoboard/octoboard.db` | Consoles, projects, session records, the report panel pages every session pushed, each project's trust permission and the trusted folders (see "The trust permission" in `docs/product/folder-trust.md`), and the settings the daemon keeps for every client (see "How switches save" in `docs/product/settings.md` and `docs/product/git-settings.md`). What the daemon does with an existing file when it starts is under "The database's schema version" below. |
| `~/.octoboard/consoles/<console id>/` | A console's working directory, where its console sessions run, including the console session instruction file Octoboard writes there, `AGENTS.md`, and `CLAUDE.md`, a link to it, or a file that imports it where the host makes no file links (see "The console session's instruction file" in `docs/product/launching-agents.md`). Removed when the console is deleted. |
| `~/.octoboard/run/<session id>/` | Per-session scratch space for what a launch injects. Removed when the session's process is gone, and cleared on daemon start, except for the sessions the start adopts from process holders. |
| `~/.octoboard/output/<session id>` | The saved terminal preview shown for an interrupted session (see "Interrupted and archived sessions" in `docs/product/session-terminal.md` for its capture rules and limits). Written when the process ends while a daemon runs it, including when the daemon stops it on its way out. A daemon that goes down without stopping, or makes a planned stop, writes none, its sessions' processes running on in their process holders; a later daemon start writes it from the screen a planned stop saved, for a session whose holder turns out to be gone (see "The screen of a terminal handed over" in `docs/product/process-holders.md`). Starting a new process discards it, and so does a daemon start adopting the session's running process. Removed with the session's record; on daemon start, any left for a session that no longer exists is removed. |
| `~/.octoboard/handoff/<session id>.md` | The handoff package a session read when it took over another session's place (see "The handoff package" in `docs/product/handing-off-sessions.md`), named for the receiving session. Removed with that session's record; on daemon start, any left for a session that no longer exists, and any half-written one, is removed. |
| `~/.octoboard/daemon.lock` | Enforces one daemon per data directory, and is what alone tells `octoboardd start`, `stop` and `status` whether a daemon runs (see "Running in the background from the command line"). |
| `~/.octoboard/daemon.owner` | The running daemon's process identity (its pid and start time), the port it listens on and whether it follows the desktop application, never its key, from which `octoboardd stop`, `restart` and `status` learn which process to signal and where it listens, and `restart` whether that daemon can be restarted (a record an earlier build wrote, or one whose daemon could not install its handler for the request, does not say, and `restart` refuses it). Owner-only. Every daemon that holds `daemon.lock` writes it, in either form, once it is listening and before its startup line, replacing any earlier one whole; one that cannot write it, or cannot tell its own start time, logs a warning and starts all the same. Removed when the daemon stops, unless it names another daemon by then, but not when the daemon goes down on an internal failure; a record whose process is gone is ignored. `daemon.lock`, not this file, says whether a daemon runs. |
| `~/.octoboard/restart-screens.json` | The screens a planned stop saved for the next daemon (see "Planned restart and planned stop"), each with the handover it belongs to, and the version of the file's format. Owner-only, replaced whole. Each planned stop replaces it, carrying over only the screens still kept for holders it could not connect to, and removes it when it has none to keep. The next daemon to start reads it before it takes anything over and keeps it: each screen is taken out of it once used — when its terminal is taken over or ended, the screens of one holder's terminals together, before any of the output since the handover is taken in, and, for a holder found gone, once the previews made of its screens are written (see "The screen of a terminal handed over" in `docs/product/process-holders.md`) — the file being written again without them, and removed once empty. The screens of a holder not connected to yet stay for when it is. A file that cannot be read, or is of a format version the daemon does not read, is deleted, and so is one that cannot be written again without a screen taken out of it; the screens it held are then rebuilt. |
| `~/.octoboard/restart-outbox.json` | The messages a planned stop left queued for running sessions, carried over to the next daemon (see "Messages held until a session can take them" in `docs/product/message-delivery.md`). Owner-only, replaced whole, removed by a planned stop that leaves none. The next daemon to start reads and deletes it before it takes anything over; a file that cannot be read, or is of another format version, is deleted and its messages are lost. |
| `~/.octoboard/background-tasks.json` | The background work each running Claude Code session's agent left running (see "Working in background" in `docs/product/session-statuses.md`), each list with the process it was left by, and the version of the file's format. Owner-only, replaced whole. Every daemon writes it each time a session's list changes, a subagent stop still waiting to empty the list being written as empty, removes a session's entry once its process has ended, and removes the file when no entry is left. A daemon that goes down, planned or not, leaves it as it was. The next daemon to start reads it before it takes anything over, gives each list back to the session taken over running the process that left it, and drops the entries of sessions it neither takes over nor keeps claimed for a holder it cannot connect to yet, and that of a claimed session once its claim ends without its process taken over. A file that cannot be read, or is of another format version, is ignored and replaced at the next change. |
| `~/.octoboard/application-quit` | The quit record the desktop application leaves at every quit, and when the system ends the user's session, whenever it had a daemon of its own that may run process holders (see "The quit note"). Every daemon start reads it, with or without the application, and ends the holders the quit left instead of taking them over: it first makes the record name the holders running at that start, and removes each from it once it is confirmed ended or found gone, the file going with the last (see "After a quit of the application" in `docs/product/process-holders.md`). Its format is in "The quit record" in `apps/daemon/PROTOCOL.md`. |
| `~/.octoboard/octoboardd.port` | The port the daemon listens on, alone, never the daemon's key, for a developer attaching to a daemon started by hand (see "Starting up"). Only the daemon that holds `daemon.lock` writes it; one that cannot write it logs a warning and starts all the same. Removed when the daemon stops, unless the file names another port by then, but not when the daemon goes down on an internal failure. |
| `~/.octoboard/logs/daemon.log` | The daemon's log, at the level of information and above, the panics of any of its threads included. Only the daemon that holds `daemon.lock` writes it, so a second daemon refused at start leaves it alone. An entry that would take the file past **5 MiB** first rotates it: `daemon.log` becomes `daemon.log.1`, the one before that `daemon.log.2`, then `daemon.log.3`, and the oldest beyond that is dropped. |
| `~/.octoboard/logs/desktop.log` | The desktop application's own log: each attempt to start the daemon or start it again and how it went, and the failures the application meets itself, each line with a UTC timestamp. A file larger than **1 MiB** when the application first writes to it in a launch is moved to `desktop.log.1`, replacing the one there. |
| `~/.octoboard/holders/` | One file per running process holder, by which a daemon finds and connects to it, holding the secret that connection is opened with (see `docs/product/process-holders.md`). The directory and its files are the user's alone. A holder removes its file when it exits; a daemon start removes the file of a holder that is gone. |
| `~/.octoboard/builds/<build>/` | A copy of a build's daemon executable, which process holders and the helpers of their launches run from, so that an update can replace the installed application while they run; on Windows with `conpty.dll` and `OpenConsole.exe` beside it. `<build>` is the version and a short hash of the executable. Removed once no holder runs from it (see "The copy of the build holders run from" in `docs/product/process-holders.md`). |
| `~/.octoboard/logs/holder-<holder id>.log` | A process holder's log, at the level of information and above, rotated as `daemon.log` is. A holder starting removes the logs of holders no longer running that have not been written for **7 days**. |
| `~/.octoboard/agent-cli-updates/<agent>.log` | The output of the last update of that agent's CLI, one file per agent named by its id (see "Updating" in `docs/product/agent-cli-updates.md`). Overwritten by each update of the agent, kept across restarts. |
| `~/.octoboard/process-ledger.json` | The launch identities, attributed processes and user keeps used for background-process cleanup across daemon restarts (see "Automatic stopping and its boundaries" in `docs/product/background-processes.md`). It records the boot ID of the system it was written in (on Linux), and records from before the system last started are ignored. An unreadable file is moved aside as `process-ledger.json.unreadable`; processes identified only by their launch tag are kept rather than stopped without knowing the user's choices. A file whose boot ID cannot be compared with the running system's (it records none, written before boot IDs were recorded, on a system that has one, or the daemon cannot read the system's) is treated as unreadable but not moved aside. |

The desktop application keeps the window's size, position and maximized state in
`~/Library/Application Support/dev.octoboard.desktop/window-state.json` on macOS and in
`%APPDATA%\dev.octoboard.desktop\window-state.json` on Windows (see "The window's size and position across
launches" in `docs/product/window-size-and-position.md`). It belongs to the application's window rather than to the
daemon, so it is not under `~/.octoboard`. Deleting it makes the next launch open the window as a first launch does.

Beside it, in the same directory, `settings.json` holds the two choices the desktop application must know before the
window's UI has loaded: keeping it running in the background when the window is closed, under `run_in_background`,
`false` to begin with (see "Closing the window" above), and showing the menu bar icon, the notification-area icon on
Windows, under `show_menu_bar_icon`, `true` to begin with (see "Hiding the icon" in `docs/product/menu-bar-icon.md`).
The application reads it once as it starts, and writes it whole each time the Settings switch for either changes,
including when an import applies them.

- **No file**: both choices take their defaults.
- **A file that cannot be read, or does not hold these settings in this form**: both take their defaults, a line saying
  so is written to `desktop.log`, and the file is left as it is until the next change of either choice replaces it. A
  choice missing from an otherwise readable file takes its default, and a key this build does not know is ignored.
- **A write that fails**: the change still applies for the rest of the run, a line saying so is written to
  `desktop.log`, and the user is not told; the next launch starts from what the file last held.
- Deleting the file makes the next launch start from the defaults; clearing the window's browser storage does not touch
  it.
- **The first launch of a build that keeps this file, finding none**, takes over the two choices an earlier build kept
  in the window's browser storage: once the window's UI has loaded it hands them to the application, which applies
  them and writes them to the file, and then removes them from browser storage. Until then that launch runs on the
  defaults, so the icon shows even if the user had hidden it. Should the file not be written, the choices stay in
  browser storage and the next launch takes them over again.

What Octoboard writes inside a project's repository, and what it writes into the user's own agent configuration, is
in "What Octoboard never modifies" in `docs/product/launching-agents.md`.

### The database's schema version

The database records the version of its schema — this build's is **3** — and a mark in the file's header (its SQLite
application ID, `0x4F435442`, "OCTB") that identifies it as Octoboard's. On every start, before using an existing
`~/.octoboard/octoboard.db`, the daemon decides what to do with it:

- **A file with no tables in it** and no version is given the current schema in place.
- **A database without the mark** — one written by a build from before the first release, whatever version it records
  — is moved aside, and the daemon starts a fresh database at the usual path:
  - It is renamed, beside itself, to `octoboard.db.superseded-<Unix time in seconds>`; when that name, or the name of
    one of its sidecar files below, is already taken, a counter is added, `octoboard.db.superseded-<time>-2` (then `-3`
    and so on), so an earlier moved-aside copy is never overwritten. The daemon logs a warning naming both files.
  - The moved-aside file is a complete copy on its own: before renaming it, the daemon writes whatever its write-ahead
    log still holds back into it. Any of SQLite's sidecar files still beside it (`-wal`, `-shm`, `-journal`) move
    with it, under the new name with the same suffix, so none is left for the fresh database. While another process
    is in the middle of reading or writing the database, so that its log cannot be written back, nothing is moved:
    the daemon fails to start, with a reason that names the file and says another process is using it.
  - The file is only renamed, never deleted, nothing in it changes beyond its log being written back, and nothing in it
    is read back into the new one. To carry settings across, export the configuration with the build that wrote it
    and import it into the new one (see `docs/product/configuration-export-and-import.md`); anything else the user
    takes from the moved-aside file by hand.
- **A file carrying another program's mark** is refused and left as it is, whatever version it records: the daemon
  fails to start, with a reason that names the file and says it is not an Octoboard database. A file that is not a
  SQLite database at all is refused and left as it is too, with a reason that names the file.
- **A database at the version this build reads** is used as it is.
- **A database at an older version** is backed up, then upgraded in place. Versions change only between releases, so
  an older version is always one a release wrote.
  - The backup is a copy beside it, `octoboard.db.pre-v<its version>`, taken after its write-ahead log is written back
    into it. No earlier backup is overwritten: when that name is taken, the copy's name has the Unix time in seconds
    added, `octoboard.db.pre-v<its version>-<time>` (and a counter after it, should that be taken too). When an earlier
    backup of that version already holds exactly the file's current content, it serves as the backup and no new copy is
    made, so starting again after a failed upgrade does not pile up copies. A backup is either complete or absent: it
    is written as `<its name>.partial` and renamed once it is on disk, and a `.partial` file is never taken for a
    backup. Backups stay on disk, never deleted. A backup that cannot be made stops the start, with the database
    untouched.
  - The upgrade is all or nothing: if any part of it fails, the database is left exactly as it was, and the daemon fails
    to start, with a reason that names the database, the versions it was being taken from and to, and the backup.
- **A database at a newer version than this build reads, or at a version below 1**, and **a file with no tables that
  records a version** are refused and left untouched: the daemon fails to start, with a reason that names the file, its
  version, and that renaming the named file moves it aside, which deletes nothing and has this build start a fresh
  database.

A start that fails over the database is shown as described in "Starting up" above.
