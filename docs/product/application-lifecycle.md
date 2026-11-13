# Application lifecycle

Octoboard comes in two forms. This document describes the **desktop form**, the macOS or Windows application, in
full: the agent processes are owned by a background daemon that starts and stops with it, and run in process holders
that keep them running through a daemon that goes down without stopping (see `docs/product/process-holders.md`);
background processes the user chose to keep are managed by the user instead (see "Automatic stopping and its
boundaries" in `docs/product/background-processes.md`). Closing the window quits the application unless the user has
turned on keeping it running in the background, a setting that is off to begin with (see "Closing the window" below). What the **server form** — the daemon alone, on a
Linux server without a display, reached by its owner from a browser on another machine over SSH — changes is said where
it applies, and in `docs/product/server-form.md`.

Who can reach the daemon, in either form, is in `docs/product/who-can-reach-the-daemon.md`, and the files Octoboard
writes for itself are in `docs/product/files-octoboard-owns.md`. What the desktop application runs on and how it is
installed is in `docs/product/installing-the-desktop-app.md`.

## Starting up

In the desktop form the daemon is started as part of the application. Whoever starts it, it listens on `127.0.0.1`
only, makes a new key for this launch, its launch key (see `docs/product/who-can-reach-the-daemon.md`), and prints the port and the key
on its standard output as its first line, `octoboardd listening on 127.0.0.1:<port> with key <key>`. The port is one the
operating system assigns, unless the daemon was started with `--port` (see "Choosing the port" in
`docs/product/server-form.md`); the desktop application never gives one, reads the port and the key from that line, and
neither is ever fixed or guessed. The daemon also writes the port, without the key, to a file in its data directory, for
a developer attaching to a daemon started by hand, who takes the key from that startup line, and records the key in a
file of its own that only its user can read, from which `octoboardd address` gives it to that user where the daemon
holds no owner key (see "Running in the background from the command line" in `docs/product/daemon-command-line.md`, and
`docs/product/files-octoboard-owns.md`). Nothing
off the machine reaches loopback except through a tunnel that ends on it, such as an SSH port forward; for the server
form, `--port` keeps the port the same across starts, and the owner key keeps the key the same (see "The owner key" in
`docs/product/who-can-reach-the-daemon.md`), so an address can be bookmarked and a tunnel command written once.

A page of the UI given no key — or no port, where its own address cannot be the daemon's — connects to nothing and
shows a screen saying there is no daemon address or key, and how to give them to a page run against a daemon started by hand:
`?port=<port>&key=<key>` in the page's address, `VITE_DAEMON_PORT` and `VITE_DAEMON_KEY` for the dev server, or
`OCTOBOARD_DAEMON_PORT` to have the dev server proxy to the daemon, the key still given by `?key=` or `VITE_DAEMON_KEY`.
The desktop application always gives its window both. The page the server form's daemon serves, opened at the daemon's
address with neither a key nor a port, shows the screen for an address that does not open the daemon instead, which
tells its user to take the address from `octoboardd address` (see "The UI the daemon serves" in
`docs/product/server-form.md`); a page on a dev server still shows the screen above.

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
whose `~/.octoboard` is the daemon's (see "The daemon on its own" in `docs/product/server-form.md`).

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
planned stop (see "The daemon on its own" in `docs/product/server-form.md`) — the application says so in an error alert
titled "The daemon exited unexpectedly", which says that the daemon runs the user's sessions and terminals, that
Octoboard cannot work without it, and asks the user to restart Octoboard; it does not silently keep showing a stale
session list. The alert treats it as an unexpected failure, offering **Report issue**, with the detail behind **Error
details** rather than in its sentence (see "Recovery and unexpected errors" in `docs/product/error-alerts.md`): how the
daemon ended (its exit status, or a signal, or why the last relaunch failed) and, when the relaunches are spent, that it
was restarted that many times in the last 10 minutes and is not restarted again, or, when an update is installed
(macOS), that the update is not applied until Octoboard restarts and the daemon is not restarted before then. A daemon
stopped by the application's own quit or restart raises no alert (see "Quitting" below).

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

## Losing the daemon connection

A dropped connection is retried automatically a couple of times with a short backoff. While that is happening a
banner says so. Once the attempts are spent, the banner offers a Retry button; nothing retries forever on its own.
Pressing Retry turns the banner back at once to saying it is reconnecting, without the button; a connection that
fails again is retried automatically as before, and Retry is offered again once those attempts are spent. Each new connection re-reads the whole state, so nothing has to be replayed by hand. A request still waiting for its
reply when the connection closes, or when it is reopened, is never sent again on the new connection: it fails, saying
that the connection closed, or that it was reopened before the reply arrived.

**A key the daemon refuses.** A page the server form's daemon served asks the daemon, each time its connection is lost
or an attempt fails, whether it still accepts the page's key. Once the daemon says it does not, the page stops
reconnecting for good and shows the screen for an address that does not open the daemon in place of the banner, with no
Retry (see "The UI the daemon serves" in `docs/product/server-form.md`). Any other answer, or none, leaves everything
here as it is.

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
SSH port forward described in `docs/product/who-can-reach-the-daemon.md`, whose network path can drop with no close
reaching either end.

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
in a browser quits anything (see "The daemon on its own" in `docs/product/server-form.md`).

**The window's close button, `Cmd+W` and the Window menu's Close Window item are one gesture**, and so, in the Windows
application, which has no application menu, are the close button of the system's title bar and `Alt+F4`. They all do
the same thing, deliberately, and what they do is the user's choice: the **Keep running in the background when the
window is closed** switch in Settings' General section, which is **off to begin with** (see "Keep running in the
background" in `docs/product/general-settings.md`).

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
state, together with whether the menu bar icon is shown (see `docs/product/files-octoboard-owns.md`, which says what
happens when the file is missing, unreadable or cannot be written): the daemon neither stores it nor carries it, and it
is not among the files under `~/.octoboard`. The application reads it as it starts, before the window's UI has loaded,
so it holds from the first moment of a launch; turning the switch takes effect at once and is written straight away. It
does not travel between two machines reaching the same daemon, and clearing the window's browser storage leaves it as it
is. A configuration export that includes the General section carries the exporting application's choice, and an
import asked to apply that section sets it on the application that runs the import, as turning the switch would; a
browser, which has no such choice, neither writes nor applies it (see "The settings a client holds" in
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
  termination signal (see "The daemon on its own" in `docs/product/server-form.md`), or a planned stop (see "Planned
  restart and planned stop" in `docs/product/planned-restart-and-planned-stop.md`) — no session is launched, resumed,
  reopened, switched to another account or handed off, and nothing else that needs a session to itself (a transfer of
  its branches, a change of its owner) is begun, and no update of an agent's CLI is started (see "Updating" in
  `docs/product/agent-cli-updates.md`): each is refused with `daemon_shutting_down`. A launch that was already past that check when
  the shutdown began is ended as soon as its process comes up, leaving the session *interrupted* like the rest. Where a
  window still shows such a refusal, it is an error alert saying that the daemon is stopping and to try again after it
  restarts, with no remedy and no **Report issue**; the wording names no operation, since a planned stop refuses more
  than a launch (see "While the terminals are handed over" in `docs/product/planned-restart-and-planned-stop.md`). An
  automatic account switch, or its restore of the session, that the shutdown cuts short tells nobody (see "When the
  switch fails" in `docs/product/automatic-account-switch.md`).
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
short (see "An ordinary stop during a planned stop" in `docs/product/planned-restart-and-planned-stop.md`), and a
planned stop asked for after the note is made ordinary too. It is the one word that reaches a daemon in a planned stop
on every host, since the daemon accepts no new connection by then.

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
  planned stop" in `docs/product/planned-restart-and-planned-stop.md`). It does not wait for the planned stop: within
  the same seconds it sends the daemon the termination signal (macOS; on Windows the note is the one notice) and the
  shutdown request, and kills a daemon still running at the end. Once the end of the session has begun, a restart that
  has not yet started the update's installer or the new launch starts neither, and nor does a quit installing a
  downloaded update (Windows).

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
  and planned stop" in `docs/product/planned-restart-and-planned-stop.md`), and Octoboard starts again; the window's
  size and position are saved. The daemon is stopped before the new launch, since the new launch's daemon would
  otherwise refuse to start while the old one holds the data directory (see "Starting up" above): a daemon still running
  **2 seconds** later is sent the planned-stop request directly, which is sent once more a second later when it could
  not be sent, and never replaced by a termination signal, which would make an ordinary stop; no such request is sent
  once the system has begun ending the user's session (see "When the system ends the session" above). A daemon still
  running **30 seconds** after that is killed; its sessions and shell terminals are then left running in their holders
  as after a daemon crash, and the new version's daemon takes them over with their screens rebuilt and marked inexact
  (see "The screen of a terminal taken over" in `docs/product/process-holders.md`).
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
stop" in `docs/product/planned-restart-and-planned-stop.md`): every session and shell terminal is handed over to its
process holder with its screen saved, and the next launch takes every one of them over, with its screen exactly as it
was. Until then they run on in their holders.

- The daemon stops by itself once the application is gone, whether or not it was asked to: with its ordinary stop
  after the quit note, with a planned stop without it. That is a watch the application asks for when it starts the
  daemon; a daemon started on its own has no application to follow (see "The daemon on its own" in
  `docs/product/server-form.md`).
- **Agent processes and shell terminals outlive a daemon that goes down without stopping.** They run in process
  holders, not in the daemon: a daemon that crashes, goes down on an internal failure or is killed outright leaves them
  running there, and the next daemon start takes them over (see "Which daemon ends leave the programs running" and
  "What a daemon start takes over" in `docs/product/process-holders.md`). Only an ordinary stop — a quit, the daemon's
  shutdown request, a termination signal, or the daemon finding the application gone after its quit note — ends them,
  as "Quitting" above describes; a planned stop hands them to the next daemon instead (see "Planned restart and planned
  stop" in `docs/product/planned-restart-and-planned-stop.md`).
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
