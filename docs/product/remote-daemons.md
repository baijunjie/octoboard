# Remote daemons

The macOS and Windows applications show, besides their own daemon, the daemons of other machines the owner reaches over
SSH: a Linux server's (see `docs/product/server-form.md`) or another of the owner's desktops' (see "Remote access to
the desktop application's daemon" in `docs/product/who-can-reach-the-daemon.md`). Each such **remote daemon** is shown
in a window of its own, a **remote window**, used as the local daemon's window is; the local daemon's window is the
**main window**. What reaching a daemon over SSH means for its security is in `docs/product/who-can-reach-the-daemon.md`.
A browser has none of this.

## One window per daemon

Every daemon the application shows has one window, and no daemon has two: the main window for the local daemon, and one
remote window for each connected remote daemon. Two saved remote daemons that name the same SSH destination (the same
user, host and SSH port) are the same daemon. Each window shows only its own daemon's consoles, projects, sessions and
notifications, and is independent of the others.

What a window remembers of its daemon — the selected console, the sidebar's view, the file browsers' state, the right
pane's panels, the shell terminals' tabs — is kept per daemon, under the SSH destination for a remote one, so one
window neither changes nor removes another's. A remote daemon's window first opens with the layout the main window has,
and keeps its own from then on. What is the application's is shared by every window, and a change in one reaches the
others at once: the interface language (see `docs/product/language.md`), the appearance, the terminal font size and
the keyboard shortcuts. Each window's size and position are remembered on their own (see "The window's size and
position across launches" in `docs/product/window-size-and-position.md`).

## Where remote daemons are reached from

The saved remote daemons, and a new remote window, are offered in these places, each listing the same items in the same
order: **New Remote Window**; then each saved remote daemon by its name, in the order they were saved, or, with none
saved, a line that cannot be chosen saying there are none; then **Open This Mac's Window** (on Windows, this PC's).

- **On macOS, the Remote menu** of the application menu, between View and Window, and the **Dock icon's menu**, above
  the open windows the system lists there.
- **On Windows, the system menu of every window** (the menu the title bar's icon and `Alt+Space` open), after the
  system's own items and a separator, and the **taskbar's jump list**: New remote window and this PC's window as its
  tasks, and the saved remote daemons in a category of their own named after the Remote menu. Windows labels them in
  sentence case, as its own menus are. A jump list item starts the application when it is not running, and reaches the
  running one otherwise.

They are worded in the interface language and follow a change of it, and of the saved remote daemons, at once.

- **New Remote Window** opens a new window on the connect page (see "The connect page" below).
- **A saved remote daemon** brings forward its window where it has one, or opens the system's browser at it again where
  its connection is held for the browser (see "Opening a remote daemon in the browser" below). Otherwise it opens a new
  window on the connect page, which starts connecting to that daemon at once, as if Connect had been pressed there.
- **Open This Mac's Window** shows and focuses the main window, bringing it back from the background or from being
  hidden.

The main window's rail lists the connected remote daemons (see "The rail" in `docs/product/window-layout.md`), and the
menu bar icon's menu has a tab for each (see `docs/product/menu-bar-icon.md`); neither offers a way to connect, add,
edit or remove one.

## The connect page

A new remote window opens on the connect page, which belongs to the application and shows no daemon. It lists the
saved remote daemons, each with its name, the badge of its connection where it is connected (**Connected**,
**Reconnecting**, or **Opened in browser** for one held for the browser) and its SSH destination as
`user@host:port`, with three buttons: **Connect** (**Open** for one that is connected), **Edit** and **Remove**.
**Add remote daemon** is under the list, or, with none saved, the empty list's own action.

- **Connect** connects to that daemon (see "Connecting" below). Only one connection is made at a time from a page: the
  pressed row's button shows that it is under way, and the other rows' Connect is disabled meanwhile.
- **Open** brings forward the window that shows that daemon, or opens the browser at it again, and asks for nothing.
- **Remove**, after a confirmation saying the daemon is removed from the list and the server and its daemon stay as they
  are, removes the saved daemon and the device key kept for it (see "Device keys" below). A window open on it stays
  until it is closed.

**Once connected, the window that asked becomes the daemon's window**: the connect page gives way to that daemon, in
the same window. When that daemon already has a window, that one is brought forward and the window that asked closes;
when the connection was asked to open in the browser, the browser opens and the window that asked closes. A second
request for a daemon a connection is already being made to waits for that one, and then finds its window.

## Saved remote daemons

The application keeps the remote daemons the owner has added, for each: a name, the SSH host, the SSH port, the user,
how to authenticate, the host key fingerprint pinned for it, and the path of `octoboardd` on that machine where it is
not found by itself. They are this computer's, in `remote-daemons.json` beside the window state (see
`docs/product/files-octoboard-owns.md`); no daemon holds them, and no configuration export carries them. **No password
or passphrase is ever stored**, there or anywhere: one is asked for when a connection needs it, and kept in memory only
for that connection's reconnections.

**Add remote daemon** and **Edit** open the same form, in this order:

- **Connection details**, where the line `octoboardd connection` prints on the server (see "Printing the connection
  details" in `docs/product/daemon-command-line.md`), or the one the Remote access section of another desktop shows (see
  `docs/product/remote-access-settings.md`), is pasted. Spaces around it are ignored. Such a line fills the host, the
  SSH port (22 where it names none) and the user, sets the path of `octoboardd` (emptying it where the line has none),
  sets the name where the line carries one, and pins the host key fingerprint where it carries one; a line that carries
  no fingerprint leaves a fingerprint already pinned as it is. Fields of the line the application does not know are
  skipped. Text that is not such a line fills nothing, and saving is refused under the field. The field itself is not
  saved.
- **Name**: what the remote window, its tab and the menus show for it. Left blank, it is the host.
- **Host**, **SSH port** (1 to 65535, 22 to begin with) and **User**, each required; a host or user with a space in it
  is refused.
- **Authentication**: **Password**, the remote user's password, asked for at each connection; **Key file**, a private
  key file on this computer, whose path is then asked for (`~` standing for the home folder) and whose passphrase is
  asked for at a connection only where the key has one; and, only for a daemon this computer's own key has already been
  installed for, **This computer's key** (see "Device keys" below).
- **Path of octoboardd**, empty to begin with, which leaves it to be found by itself (see "Connecting" below).
- **Pinned host key**, shown for a saved daemon and for a new one once a fingerprint is pinned: the fingerprint, with
  **Clear**, which unpins it so that the host key shown at the next connection is confirmed again; or, with none
  pinned, a line saying it is pinned when the host key is confirmed at the first connection. A fingerprint is never
  typed in.

A saved daemon whose host, port or user is changed is a new SSH destination: the device key kept for it is dropped,
and one that authenticated with This computer's key authenticates with a password until a key is installed there again.

## Connecting

A connection to a remote daemon is made in this order, each step's failure ending it as "What a connection asks or
reports" below says:

1. **The password**, for a daemon that authenticates with one, is asked for first, unless its connection is already held
   (Open), naming the daemon and the user, and saying it is used for this connection only and not stored.
2. **The SSH connection** the application makes itself, to the host and port, verifying the host's key against the
   pinned fingerprint, and authenticating with the password (answering a keyboard-interactive login with it where the
   host asks for one), the key file or this computer's key. A host that stops answering fails the connection rather than
   holding it.
3. **Finding `octoboardd`** on the remote machine: the path the saved daemon names, else, on a Linux or macOS host, the
   remote user's `PATH` as a command run over SSH gets it, `~/.local/bin`, `/usr/local/bin`, and the application's own
   files in `/Applications` and `~/Applications`; on a Windows host, the application's per-user install and its
   per-machine one.
4. **Reading the daemon** through `octoboardd address --json` (see "Another device reaching the daemon over SSH" in
   `docs/product/daemon-command-line.md`): its owner key, the protocol generation it speaks and its release version. A
   daemon of another protocol generation than the application's is refused here, before any window loads (see "A version
   mismatch" below).
5. **Installing this computer's key**, after a login with the password or a key file (see "Device keys" below).
6. **Showing the daemon**: the application listens on a loopback port of its own, which the system assigns, and opens
   the window there with the owner key; each connection the window opens through that port is carried to the daemon by
   one run of `octoboardd proxy` over the SSH connection. The daemon never listens beyond its own machine's loopback and
   owner endpoint for this.

A host key confirmed at the first connection is pinned once the connection succeeds.

### What a connection asks or reports

Every question and failure is shown in the window that asked, over the connect page or the window, as a dialog; a
failure is an error alert (see "The error alert" in `docs/product/error-alerts.md`). What the host or `octoboardd` said
is under the alert's **Error details**, with anything shaped like a key taken out.

- **A password or a key's passphrase is needed**: the password dialog, or one asking for the key file's passphrase.
- **A host with no pinned fingerprint**: a confirmation naming the daemon and the fingerprint of the key the host
  presented, asking to compare it with the server's own (giving a command that prints it there) and to connect only if
  they are the same. **Connect** connects again with that fingerprint confirmed, which is pinned once the connection
  succeeds.
- **A host whose key does not match the pinned fingerprint**: refused, in an alert saying that the server presented a
  different host key than the one pinned and the connection was refused, and that, if the server's key has really
  changed, the saved daemon is edited, its pinned host key cleared and the daemon connected again; both fingerprints
  are under Error details.
- **No SSH server answers** at the host and port: an alert naming the address, asking to check the host, the port and
  that the server is running.
- **The sign-in is refused**: an alert asking to check the user and the password or key file.
- **The key file cannot be read or decoded, or its passphrase is wrong**: an alert saying so.
- **This computer's key is no longer accepted** (it was removed on the host): the password dialog, saying the server no
  longer accepts this computer's key and that signing in with the password adds it again.
- **`octoboardd` is not found**: a dialog listing where it was looked for and asking for its path on the server;
  **Save and connect** saves the path with the daemon and connects again.
- **No daemon is running** on a server: a confirmation saying so, whose **Start daemon** runs `octoboardd start` on the
  server and connects again. A start that fails is an alert saying the server did not start its daemon.
- **The host's `octoboardd` is a desktop application's, and the application is not running there**: a confirmation
  saying that its daemon runs only while the application does, to open the application on that computer and try
  again, with **Try again**.
- **The daemon offers no way in**: an alert saying that the daemon cannot be connected to — a desktop application's
  daemon needs remote access turned on (see "Remote access to the desktop application's daemon" in
  `docs/product/who-can-reach-the-daemon.md`), and one without the web interface cannot be connected to.
- **The daemon's way in does not answer**: an alert saying the server answered but its daemon did not, which may have
  just stopped.
- **A daemon of another version**: see "A version mismatch" below.
- **Any other failure**, one the application does not recognise included, is unexpected: its alert says something
  unexpected went wrong and offers **Report issue**.

The password, the passphrase and a fingerprint just confirmed are kept for the retries of that one connection, and
dropped when it ends.

### A version mismatch

A remote daemon of another protocol generation is refused before any window loads: the dialog names the daemon and the
two release versions, the daemon's and this application's, and says which side is behind. Neither side is updated from
here: updating is done on each machine itself (see "No updates in a remote window" below).

- **The daemon is behind**: it says the two cannot work together until the daemon is updated, on the server itself.
  Its footer holds **Cancel** and **Open in browser**.
- **This application is behind**: it says they cannot work together until this application is updated. Its footer holds
  **Go to Updates**, which closes the dialog and opens Settings' Updates section in the main window, and **Open in
  browser**; the dialog's close button dismisses it.

Either way it says that the daemon's own web interface, which matches its version, can be opened in the browser
instead. **Open in browser** connects again, holding the connection for the browser (see below).

## Opening a remote daemon in the browser

**Open in browser** holds the connection without a window: the application listens on its loopback port as for a
window, and opens the system's default browser at that port with the owner key, where the daemon serves its own UI, of
its own version. The address, which carries the owner key, is opened by the application itself and never given to a
page. Such a connection is listed with the connected remote daemons, its state **Opened in browser**; choosing it in a
menu, on the rail or on the connect page opens the browser at it again. It is ended by **Disconnect** (below) and by
quitting the application, and is not brought back at the next launch. While it is held, the owner key is read again
whenever the daemon's key changes, so the address it opens follows a rotation.

## Device keys

**After the first login with the remote user's password or a key file, the application installs a key of this
computer's own and signs in with it from then on**, so later connections ask for nothing.

- The key is an Ed25519 key pair made for that saved daemon. The private key is kept in the system's credential store —
  the login keychain on macOS, the Credential Manager on Windows — and never leaves the computer; only the public key is
  sent. Nothing else of the remote daemons is kept there.
- The public key is appended to the remote user's authorized keys, as `ssh-copy-id` does: `~/.ssh/authorized_keys` on a
  Linux or macOS host, created private where it does not exist, the entries already there left as they are, and
  nothing added where the key is already there. On a Windows host it is the file that user's login reads: for an
  administrator `%ProgramData%\ssh\administrators_authorized_keys`, given, where it is created, the access rights the
  SSH server requires (Administrators and SYSTEM only), and for any other user `~\.ssh\authorized_keys`. The entry is
  marked `Octoboard on <this computer's name>`, so it can be told apart and removed there.
- The key is used only once a connection made with it has been accepted; the saved daemon then authenticates with
  **This computer's key**. Where installing it fails, the window opens all the same, and the next connection asks for
  the password again and tries again.
- **Revoking a device** is removing its line from the authorized keys on the remote machine; a connection already open
  lasts until it drops. The application then reports that the server no longer accepts this computer's key and asks for
  the password, which installs it again. Listing or removing installed keys from within the application is not offered.
- The private key is removed from the credential store when the saved daemon is removed, or when its host, port or user
  is changed.

## The remote window

A remote window is used as the main window is, with these differences:

- **Its title** is the daemon's name followed by the application's name. **Its top bar** shows the daemon's name in a
  chip at the bar's start, before the breadcrumb, with a server glyph, read by assistive technology as the daemon on
  that name; the chip keeps its width while the breadcrumb beside it is cut short, and a name too long for it fades,
  with the whole name in a tooltip (see "The top bar" in `docs/product/window-layout.md`). The chip is there on the
  screens shown before the daemon's state has arrived too.
- **Settings are the remote daemon's.** Every setting the daemon keeps is that daemon's, the agent language included
  (see "The agent language" in `docs/product/language.md`). What is the application's own is not offered there: Launch
  at login, Keep running in the background and Show menu bar icon in General (see `docs/product/general-settings.md`),
  the Permissions section, the Remote access section, and the application's updates (below). Appearance, the interface
  language, the terminal font size and the keyboard shortcuts are the application's, shared with every window.
- **Nothing of this computer is offered for the daemon's files**: no File manager, Default application or System
  terminal under Open in (see `docs/product/sidebar-project-rows.md` and "A row's actions" in
  `docs/product/project-pane.md`), no File manager or Default application in the file viewer's header (see "The
  header" in `docs/product/file-viewer.md`), file paths in the terminal stay plain text (see "File paths" in
  `docs/product/terminal-links.md`), and Browse uses the application's directory browser, which lists the remote
  machine's directories, instead of the system's picker (see "Browsing directories" in `docs/product/projects.md`).
  Web addresses still open in this computer's browser.
- **It neither quits the application nor stops its daemon.** Quitting is the main window's (see "Quitting" in
  `docs/product/application-lifecycle.md`); the remote daemon and its sessions run on whatever the application does.
- **Its notifications and waiting count are its own daemon's**: a session waiting for the user in that daemon notifies
  through the application, a click on the notification brings this window forward, and its waiting sessions count in
  the Dock badge (see "The raised hand" in `docs/product/session-statuses.md`). The menu bar icon's menu has a tab for
  its daemon (see `docs/product/menu-bar-icon.md`).
- On macOS, Settings… and About in the application menu open in the window focused last, so in a remote window they
  open that window's Settings.

### No updates in a remote window

The application is updated only on its own machine, and a remote daemon only on its machine. A remote window shows no
update notice, no update strip, no "updated" notice, and no Version row, and makes no update check. Its Settings has an
Updates section only where its daemon updates itself (see "Which daemons update themselves" in
`docs/product/server-form-updates.md`), and it then holds only that daemon's **Check for updates automatically** and
**Update channel** rows, the first worded for the daemon, which checks in the background and is updated on the
computer it runs on (see `docs/product/update-settings.md`). A browser open on a daemon keeps its update features.

## A dropped connection

The loopback port a remote window is opened on is the application's from before the window opens until it closes,
reconnections included, so that the owner key is never sent to a port another user of this computer could have taken.
When the SSH connection drops, the window loses its daemon connection as any window does (see "Losing the daemon
connection" in `docs/product/application-lifecycle.md`), and the daemon is shown **Reconnecting** on the rail and the
connect page; the next connection the window makes connects over SSH again, with pauses that grow between failed
attempts, up to **30 seconds**. The owner key is read again over each new SSH connection, and whenever the window has no
connection open, so a key rotated meanwhile is followed: the window is pointed at the new key only where it changed.

## Disconnecting and closing

**Closing a remote window disconnects it**: the loopback port is let go and the SSH connection ended, and nothing is
asked of, or stopped on, the remote daemon, whose sessions run on. The window closes for good; which window then shows,
or whether the application goes into the background or quits, follows "Closing the window" in
`docs/product/application-lifecycle.md`.

**Disconnect** — on a remote daemon's tile on the rail (see "The rail" in `docs/product/window-layout.md`) and on its
tab in the menu bar icon's menu — ends that daemon's connection: it closes its window, or lets go of the connection held
for the browser. It never quits the application: with no other window left on screen, the application goes into the
background where the user keeps it running there and its own daemon runs, and otherwise the main window comes back.

**Quitting the application** ends every remote connection, asking nothing about the remote daemons, and stops only the
local daemon.

## Bringing remote windows back at launch

At every quit the application remembers which remote windows were open, in the order they were opened, and whether the
main window was hidden at that moment (closed while a remote window stayed on screen). A remote window closed by the
very gesture that quits the application counts as open; one held for the browser does not count.

At the next launch they are all connected again at once, asking nothing, and each window opens where it was left. One
whose connection needs something the application cannot supply without asking — a password, a passphrase, a host key
to confirm — or that fails, a version mismatch included, opens no dialog and no window, and stays unconnected until the
owner connects it. A launch at login that keeps the application in the background brings none back.

**Where the main window was hidden at quit**, it is held hidden while the remote windows are connected, and stays hidden
once at least one of them is back, until something shows it. When none has come back within **15 seconds**, it is shown
as at any launch; a remote window that connects later still opens.
