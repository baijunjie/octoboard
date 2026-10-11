# Application lifecycle

Octoboard is a macOS desktop application. The agent processes are owned by a background daemon that starts and stops
with it; nothing keeps running once the application is gone. Closing the window quits the application unless the user
has turned on keeping it running in the background, a setting that is off to begin with (see "Closing the window"
below).

## System requirements and distribution

- **macOS 11 or later**, on **Apple Silicon only** — the release build is a single-architecture `arm64` bundle, and
  there is no Intel or universal build.
- Octoboard is distributed as a `.dmg` disk image. It cannot be distributed through the Mac App Store: its translucent
  window (see "The window chrome and the content panel" in `docs/product/window-layout.md`) relies on a private macOS
  interface, which the store does not admit. The release build is signed with a Developer ID and notarized, so
  Gatekeeper admits it as built: `spctl` reports `Notarized Developer ID` for the `.app` and the `.dmg`, including a
  copy carrying the quarantine attribute, and `codesign --verify --deep --strict` passes on the bundled daemon. That was
  verified on the machine that built it; installing and running it on a clean machine has not been tried yet.

## Starting up

The daemon is started as part of the application and listens on `127.0.0.1` only, on a port the operating system
assigns. The application is handed that port by the process that started the daemon; it is never fixed and never
guessed.

Two things happen on every daemon start:

- **Every session a previous run left behind becomes *interrupted*.** None of those processes survived, whatever the
  stored status said. Archived sessions are left as they are.
- Per-session scratch space from previous runs is cleared.

**One Octoboard at a time.** A second daemon running against the same `~/.octoboard` would own a second set of agent
processes while sharing one database, so it refuses to start. The application then opens its window anyway, on a
screen that states why the daemon could not start and offers to quit — a failed start never leaves a window with no
explanation. With no daemon running there is nothing to keep running, so closing the window quits whatever that
setting says (see "Closing the window" below).

If the daemon starts but dies later, the application says so and asks the user to restart Octoboard; it does not
silently keep showing a stale session list.

**The window appears already themed.** It is created hidden and shown only once the UI has applied the chosen
appearance (see "Light, dark and follow the system" in `docs/product/appearance.md`), so a launch never flashes the
operating system's own appearance first. The window therefore appears a fraction of a second later than it otherwise
would — measured at 140-220 ms after it is created. Should the UI never get that far at all — its bundle failing to
load, say — the shell shows the window regardless **4 seconds** after creating it, so a failed start never leaves a
running application with no window on screen. A window the user has by then closed into the background stays closed.

**Closing the window during a launch quits.** The shell starts every launch with keeping the application running in
the background off, whatever the user last chose: the choice is kept by the window's UI and handed over once it has
loaded (see "Closing the window" below), so a close in the first moments of a launch — and one on the screen shown
when the daemon failed to start, which never gets a choice to hand over — quits.

## Who can reach the daemon

The daemon has no login or token. **Any program running on this machine can connect to it and do everything the
application can**: create consoles and sessions, start agents, type into their terminals.

**A web page open in a browser cannot.** When a browser makes a request on behalf of a page, the daemon answers `403`
unless that page is the application's own window or is served over `http` or `https` from this machine's loopback
address, on any port. A site that makes its own name resolve to `127.0.0.1` is refused as well. Programs that are not
browsers, such as scripts, are not subject to this check, and neither is the application's own window.

## Losing the daemon connection

A dropped connection is retried automatically a couple of times with a short backoff. While that is happening a
banner says so. Once the attempts are spent, the banner offers a Retry button; nothing retries forever on its own.
Each new connection re-reads the whole state, so nothing has to be replayed by hand.

**Retry with a dialog or a menu open.** Nothing drawn over the content panel — a dialog, a menu, a popover — covers
the banner: a press where Retry is drawn retries the connection, and what was open stays open, so a pointer never has
to close anything first to get back out of a lost connection (see "What an open dialog, menu or popover covers" in
`docs/product/window-layout.md`). By keyboard it is the other way round — `F6` does not leave an open dialog or menu,
so it has to be dismissed before `F6` reaches the banner (see "Moving focus between regions with F6" in
`docs/product/moving-focus-between-regions.md`).

**Observed:** after the daemon alone was killed with `SIGTERM` while the application stayed open, the banner read
"Disconnected from the daemon." with its Retry button up within moments (the two automatic retries fail at once against
a daemon that is gone), together with a toast "The daemon process exited unexpectedly (exit code 0). Restart Octoboard
to continue."

Failures a user has to know about are shown as toasts (see `docs/product/toasts.md`), and so are notices about a session
that are not failures. Every such notice names where its session is — the project it runs in, or the console whose
console session it is — and so does a failure that is about one particular session, since what the daemon reports refers
to the session it is about only as "this session". A failed report-page submission is one such failure (see "Submitting
a form back to the console session" in `docs/product/report-panel.md`). A failure raised by a dialog's own action is
shown in that dialog instead — except in the trust dialog, which closes and reports most failures as a toast (see
"The trust dialog" in `docs/product/folder-trust.md`).

A dialog has two places to say what went wrong. **The failure of the dialog's own action** — what the daemon
rejected, or an image the window could not read as an avatar — is a line at the foot of the dialog. **What is wrong
with one field**, such as a required field left blank, is said under that field instead, and the field is marked
invalid. A field is never marked before the user has tried to submit: the message appears on the first attempt —
which sends nothing while any field is still wanting — and from then on follows what is typed, so it goes as soon as
the field is corrected and comes back if the field is emptied again.

## Closing the window

**The window's close button, `Cmd+W` and the Window menu's Close Window item are one gesture.** All three do the same
thing, deliberately, and what they do is the user's choice: the **Keep running in the background when the window is
closed** switch in Settings' General section, which is **off to begin with** (see "Keep running in the background" in
`docs/product/settings.md`).

**With the switch off, closing the window quits.** It is an ordinary quit gesture, taking exactly the path `Cmd+Q`
takes (see "Quitting" below): with a session running it asks the same confirmation, where Cancel leaves the window
open and everything running, and with none it quits at once. The sessions a confirmed close ends are left
*interrupted* and resumable, just as any confirmed quit leaves them (see "Archiving, interruption and resuming" in
`docs/product/sessions.md`).

**With the switch on, closing the window closes it into the background**: the window is hidden, Octoboard leaves the
Dock, and the focus passes to the application used before it. The daemon, every session and its agent process keep
running, and so does the UI behind the hidden window: a session that raises its hand still notifies (see "The raised
hand" in `docs/product/sessions.md`), the Dock badge's count is kept up to date for when Octoboard returns to the
Dock, and the menu bar icon's menu keeps following the sessions (see `docs/product/menu-bar-icon.md`).

- **In native fullscreen** the window first leaves fullscreen and is hidden once that is done, about 0.8 seconds later,
  so no empty fullscreen space is left behind. Brought back within that time, it is not hidden.
- **When the daemon is not running** — it failed to start, or has exited — there is nothing to keep running, so
  closing the window quits at once and asks nothing, whatever the switch says.
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

- a left click on the menu bar icon, or its menu's Open Octoboard or one of its sessions (see "Clicking the icon" and
  "The icon's menu" in `docs/product/menu-bar-icon.md`);
- opening Octoboard again while it runs — from the Finder, Spotlight or `open`;
- anything else that activates Octoboard, such as clicking one of its notifications;
- a quit that has to ask for confirmation (see "Quitting" below);
- the daemon exiting unexpectedly: the window comes back showing the notice that it exited (see "Losing the daemon
  connection" above). A daemon that exits while the window is shown only shows the notice, and does not take the
  focus from whatever the user is doing.

Octoboard then returns to the Dock, comes to the front, and its Dock badge shows the current waiting count again.

**Observed** in the packaged application, under Simplified Chinese.

**With the switch off:**

- on a launch with no choice stored yet, Settings' General section showed the row last, below Default clone
  directory, with its switch off;
- with a session running, the close button, `Cmd+W` and the Window menu's Close Window item each raised the ordinary
  quit confirmation, with the application, the daemon and the session's agent process all still alive; Cancel left
  the window open and all three running as the same processes; confirming ended the application about 0.14 seconds
  later, the daemon and the agent process following it;
- with no session running, the close button quit within about 0.15 seconds, asking nothing.

**With the switch on:**

- the close button and `Cmd+W` closing the window into the background: the application left running with the daemon
  and the agent process alive and no window, Octoboard out of the Dock and inactive, with the application used
  before it frontmost again; a left click on the menu bar icon then brought the window back at the same size and
  position, the session still live and Octoboard back in the Dock and frontmost;
- a close from native fullscreen hiding the window with no empty fullscreen space left behind, and the window
  staying on screen when brought back during that transition, by the menu bar icon's Open Octoboard or by opening
  Octoboard again;
- closing the window again within a second of bringing it back, with the Dock icon ending right and never duplicated;
- `Cmd+H` working after a close and a return;
- the window coming back by opening Octoboard again while it ran (for the menu bar icon, see "What has been tried by
  hand" in `docs/product/menu-bar-icon.md`), at the same size and position, and opening Octoboard again while the
  window was on screen only bringing it to the front;
- activating Octoboard while the window was closed bringing back the window, the Dock icon and the badge — tried by
  activating the application directly — while a notification firing meanwhile did not bring the window back;
- the page behind the hidden window staying live, and the Dock badge coming back with the right count, including a
  count that changed while the window was closed;
- the daemon killed while the window was closed bringing it back with the notice within about 0.2 seconds, and
  killed while the window was shown and another application frontmost showing the notice only, with the focus left
  where it was.

**Whatever the switch says:** the close button quit within about 0.2 seconds, asking nothing, with the daemon gone.

**The switch itself.** Turning it on took effect without a relaunch — the next close hid the window — and survived
one: after a quit and a fresh launch the switch still read on and a close still hid the window. Turning it off in
that launch made the close quit, and after a further launch the switch read off and the close asked for confirmation
again; the stored choice followed each change. The row's description differed between the two states and fitted its
row in Simplified Chinese in the dark theme.

**Not yet tried by hand:** the row's English description and its absence in a browser, neither seen in the
application itself; a close from native fullscreen with the switch off; and a real click on one of Octoboard's
notifications bringing the window back.

## Quitting

Every way of ending the application behaves the same: `Cmd+Q`, the application menu's Quit, the menu bar icon's Quit
Octoboard (see "Quit Octoboard" in `docs/product/menu-bar-icon.md`), the Dock icon's own Quit, a system-initiated
termination — logging out, restarting or shutting down the machine — and, while keeping Octoboard running in the
background is off, closing the window: the close button, `Cmd+W` and the Window menu's Close Window item (see
"Closing the window" above). The one difference is the escape hatch in "Quitting when the window has stopped
responding" below, which the close gestures have no part in. `Cmd+Q` and the application menu's Quit were each seen to
ask, and Cancel left everything running; the menu bar icon's Quit was seen to ask too. The Dock icon's own Quit and a
system logout were not tried by hand; an AppleEvent quit, which goes through the same `applicationShouldTerminate:`
path, was, and asked the same way. A quit by closing the window was seen to ask while a session ran, and to end the
application on confirmation (see "Closing the window" above).

- **While any session has a running process, quitting asks for confirmation.** The message says that quitting
  interrupts those sessions and that each stays resumable next time. Asking brings Octoboard to the front first,
  restoring its window if it was minimized and bringing it back if it was closed into the background, so a quit from
  outside the window — the Dock icon's own Quit while another application covers it, the menu bar icon's Quit or a
  logout while the window is closed — does not leave the question unseen. The menu bar icon's Quit with the window
  closed was seen to bring it back and ask; the other cases were not yet tried by hand.
- Once confirmed, every session process is ended and every one of those sessions is left **interrupted**, never
  archived. Clicking one next time resumes it (see "Archiving, interruption and resuming" in
  `docs/product/sessions.md`).
- With no session running, quitting is immediate and asks nothing, and a window closed into the background is not
  brought back for it; seen with the menu bar icon's Quit.
- While the daemon is not running — it failed to start, or has exited — every quit is immediate and asks nothing.
- The daemon stopping as part of a quit is not reported as the daemon exiting: no notice is shown and a closed window
  is not brought back for it. Seen with the menu bar icon's Quit while the window was closed and no session was
  running: the application exited with no window, Dock icon or notice appearing.
- Quitting does not depend on the daemon answering: if it does not, the application exits anyway after a short wait.
- Until the window has loaded far enough to be able to ask, a quit is never held back — a window that never gets as
  far as showing the confirmation can still be quit.

Because a logout, restart or shutdown is answered exactly like any other quit, **Octoboard can hold up a logout,
restart or shutdown until the user answers the confirmation dialog.** That is deliberate, not an oversight.

### Quitting when the window has stopped responding

A confirmation dialog that can no longer be shown or answered — the window's process died, or its page hung — would
otherwise leave no way out short of a force quit. So: **when a quit gesture has been deferred to the confirmation
dialog and a second quit gesture arrives within 2 seconds, that second gesture quits immediately and asks nothing.**
The sessions are left exactly as a crash leaves them (see "Crashes and forced termination" below).

A window that is still answering resets that 2-second window every time it handles a quit gesture, so pressing
`Cmd+Q` twice in quick succession on a working Octoboard does **not** skip the confirmation. The escape hatch opens
only when nothing answered the first gesture.

**Closing the window has no part in this, even when it is a quit** (see "Closing the window" above). A double press
on a close button is an ordinary gesture and must not end a live session unasked, so the close button, `Cmd+W` and
Close Window never take this escape hatch however often they are repeated, and never open it for the gesture after
them either. `Cmd+Q`, the application menu's Quit, the menu bar icon's Quit or the Dock icon's Quit — twice — is the
way out of that state.

**Observed:** with the switch off and a session running, two presses of the close button 0.36 seconds apart, and
a pointer double-click on it about 80 ms apart, each left the confirmation up with nothing quit.

**Observed, unexplained caveat:** with the page frozen by `SIGSTOP` on the application's WebContent process, two
scripted quits back to back (AppleEvent, or the menu item through System Events) quit without asking, as described, but
two `Cmd+Q` key presses did not, even with a second between them. Whether that is WKWebView holding key equivalents
while its process is frozen, or an artifact of `SIGSTOP`, was not established, and a real page hang was not tried. The
Dock icon's Quit was not tried in that state.

## Crashes and forced termination

A crash of the application, or killing it outright, leaves **the same state** as a confirmed quit: every session
interrupted and resumable.

- The daemon exits by itself once the application is gone, whether or not it was asked to.
- Agent processes do not outlive the daemon: it ends them on its way out, including when it is signalled to stop and
  when it goes down on an internal failure.
- An agent's tool subprocesses go with the agent.
- Should a session's process nonetheless survive, the next start still marks every non-archived session interrupted
  (see "Starting up"), so the session list never claims a session is running that this Octoboard does not own.

## Files Octoboard owns

Everything Octoboard writes for itself lives under `~/.octoboard`, with one exception, the window's state, described
after the table:

| Path | Contents |
|---|---|
| `~/.octoboard/octoboard.db` | Consoles, projects, session records, the report panel pages of every console session, each project's trust permission and the trusted folders (see "The trust permission" in `docs/product/folder-trust.md`), and the settings the daemon keeps for every client (see "Git" and "General" in `docs/product/settings.md`). Octoboard has not shipped, so its schema still changes in place: a database written by an older build is not upgraded. The daemon instead moves it aside, beside itself, with a `.superseded-<timestamp>` suffix, and starts a fresh one at the usual path — the data in the old file is not read back into the new one, and the user re-enters their consoles, projects and accounts by hand. Two exceptions are changed in place instead, so the database and everything in it are kept: the default clone directory, an optional setting added later, is added to an existing settings table; and a projects table that still keeps the trust permission under its older, Claude Code-only name has that column renamed, each project keeping the permission it had. |
| `~/.octoboard/consoles/<console id>/` | A console's working directory, where its console sessions run, including the console session instruction file Octoboard generates there (see "The console session's instruction file" in `docs/product/launching-agents.md`). Removed when the console is deleted. |
| `~/.octoboard/run/<session id>/` | Per-session scratch space for what a launch injects. Removed when the session's process is gone, and cleared wholesale on daemon start. |
| `~/.octoboard/output/<session id>` | The raw terminal output a session's last process left, at most 2 MiB, shown when the session is selected with no process (see "The terminal" in `docs/product/sessions.md`). Written when the process ends, including when the daemon stops it on its way out, but not when the daemon itself goes down on an internal failure. Removed with the session's record; on daemon start, any left for a session that no longer exists is removed. |
| `~/.octoboard/daemon.lock` | Enforces one daemon per data directory. |

The macOS application keeps the window's size, position and maximized state in
`~/Library/Application Support/dev.octoboard.app/window-state.json` (see "The window's size and position across
launches" in `docs/product/window-size-and-position.md`). It belongs to the application's window rather than to the
daemon, so it is not under `~/.octoboard`. Deleting it makes the next launch open the window as a first launch does.

Nothing is written inside a project directory, and nothing is written into the user's own agent configuration beyond
two narrow exceptions — see "What Octoboard never modifies" in `docs/product/launching-agents.md`.
