# Remote daemons

The macOS and Windows applications show, besides their own daemon, the daemons of other machines the owner reaches over
SSH: a Linux server's (see `docs/product/server-form.md`) or another of the owner's desktops' (see "Remote access to
the desktop application's daemon" in `docs/product/who-can-reach-the-daemon.md`). Each such **remote daemon** is shown
in a window of its own, a **remote window**, used as the local daemon's window is; the local daemon's window is the
**main window**. The remote daemons the owner has saved are listed, added and connected to on the main window's
**Remote devices page** (see `docs/product/remote-devices-page.md`). What reaching a daemon over SSH means for its
security is in `docs/product/who-can-reach-the-daemon.md`. A browser has none of this.

## One window per daemon

Every daemon the application shows has one window, and no daemon has two: the main window for the local daemon, and one
remote window for each connected remote daemon. Two saved remote daemons that name the same SSH destination (the same
user, host and SSH port) are the same daemon. Each window shows only its own daemon's consoles, projects, sessions and
notifications, and is independent of the others.

What a window remembers of its daemon — the selected console, the sidebar's view, the file browsers' state, the right
pane's panels, the shell terminals' tabs — is kept per daemon, under the SSH destination for a remote one, so one
window neither changes nor removes another's. A remote daemon's window first opens with the layout the main window has,
and keeps its own from then on. What is the application's is shared by every window, and a change in one reaches the
others at once: the appearance, the terminal font size and the keyboard shortcuts. The language is the daemon's: each
window shows the one its own daemon holds (see `docs/product/language.md`). Each window's size and position are
remembered on their own (see "The window's size and position across launches" in
`docs/product/window-size-and-position.md`).

## Where remote daemons are reached from

The Remote devices page, and the saved remote daemons, are offered in these places, each listing the same items in
the same order: **Remote Devices**; then each saved remote daemon by its name, in the order they were saved, or, with
none saved, a line that cannot be chosen saying there are none; then **Open This Mac's Window** (on Windows, this
PC's).

- **On macOS, the Remote menu** of the application menu, between View and Window, and the **Dock icon's menu**, below
  the open windows the system lists there (macOS places an application's own Dock menu items there).
- **On Windows, the system menu of every window** (the menu the title bar's icon and `Alt+Space` open), after the
  system's own items and a separator, and the **taskbar's jump list**: Remote devices and this PC's window as its
  tasks, and the saved remote daemons in a category of their own named after the Remote menu. Windows labels them in
  sentence case, as its own menus are. A jump list item starts the application when it is not running, and reaches the
  running one otherwise. Once the clean-up before uninstalling has removed the jump list, the application does not
  write it again until it next starts (see `docs/product/clean-up-before-uninstalling.md`).

They are worded in the main window's language and follow a change of it, and of the saved remote daemons, at once.

- **Remote Devices** shows and focuses the main window, bringing it back from the background or from being hidden,
  with its Remote devices page shown (see `docs/product/remote-devices-page.md`).
- **A saved remote daemon** brings forward its window where it has one, or opens the system's browser at it again where
  its connection is held for the browser (see "Opening a remote daemon in the browser" below). Otherwise it connects to
  that daemon at once, as Connect on the Remote devices page does, and the daemon's window opens once it is connected.
  Nothing is shown while it connects. Where the connection has something to ask or report — a password, a host key to
  confirm, a failure — the main window comes forward and its Remote devices page shows it (see "What is handed to the
  page" in `docs/product/remote-devices-page.md`). Where the daemon is no longer saved, or the saved ones cannot be
  read, the Remote devices page is shown.
- **Open This Mac's Window** shows and focuses the main window, bringing it back from the background or from being
  hidden.

**The main window's rail** has a **Remote devices** button (see "The rail" in `docs/product/window-layout.md`), with
the number of connections held — in a remote window or for the browser — in a badge, which opens the Remote devices
page (see "Opening and leaving the page" in `docs/product/remote-devices-page.md`). A remote window, and a browser, have
no such button.

The menu bar icon's menu has a tab for each connected remote daemon (see `docs/product/menu-bar-icon.md`); it offers no
way to connect, add, edit or remove one, nor shows the Remote devices page.

## Saved remote daemons

The application keeps the remote daemons the owner has added, for each: a name, the SSH host, the SSH port, the user,
how to authenticate, the host key fingerprint pinned for it, the path of `octoboardd` on that machine where it is
not found by itself, and what it knows of this computer's key there: whether the owner said no to it or turned it off,
and, for a key it installed, the file its line was added to and what the line is marked with (see "Device keys"
below). They are this computer's, in `remote-daemons.json` beside the window state (see
`docs/product/files-octoboard-owns.md`); no daemon holds them, and no configuration export carries them. **No password
or passphrase is ever stored**, there or anywhere: one is asked for when a connection needs it, and kept in memory only
for that connection's reconnections.

A new remote daemon and a saved one are filled in on the same form, on the Remote devices page (see "The form" in
`docs/product/remote-devices-page.md`), whose fields are, in this order:

- **Connection details**, where the line `octoboardd connection` prints on the server (see "Printing the connection
  details" in `docs/product/daemon-command-line.md`), or the one the Access section of another desktop shows (see
  `docs/product/remote-access-settings.md`), is pasted, by hand or with the field's Paste button. Spaces around it are
  ignored. Such a line fills the host, the SSH port (22 where it names none) and the user, sets the path of
  `octoboardd` (emptying it where the line has none), sets the name where the line carries one, and pins the host key
  fingerprint where it carries one; a line that carries no fingerprint leaves a fingerprint already pinned as it is.
  Fields of the line the application does not know are skipped. Text that is not such a line fills nothing, and saving
  is refused under the field. The field itself is not saved.
- **Name**: what the remote window, its tab, the menus and the Remote devices page show for it. Left blank, it is the
  host.
- **User**, **Host** and **SSH port** (1 to 65535, 22 to begin with), each required; a host or user with a space in it
  is refused.
- **Authentication**: **Password**, the remote user's password, asked for at each connection; or **Key file**, a
  private key file on this computer, whose path is then asked for (`~` standing for the home folder) and whose
  passphrase is asked for at a connection only where the key has one. A daemon whose host has this computer's key
  added signs in with that key instead: the form says so in place of the choice, which comes back once the key is
  turned off (see "Device keys" below). The key is never chosen on the form.
- **Path of octoboardd**, empty to begin with, which leaves it to be found by itself (see "Connecting" below).
- **Pinned host key**, shown for a saved daemon and for a new one once a fingerprint is pinned: the fingerprint, with
  **Clear**, which unpins it so that the host key shown at the next connection is confirmed again; or, with none
  pinned, a line saying it is pinned when the host key is confirmed at the first connection. A fingerprint is never
  typed in.

A saved daemon whose host, port or user is changed is a new SSH destination. Where this computer's key is added for
it, the form warns of this before the save (see "The form" in `docs/product/remote-devices-page.md`), and saving the
change first takes the key back from the host it was added on (see "Taking the key back" below); the daemon then signs
in with a password, and its next connection asks about the key for the new destination.

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
   daemon of another protocol generation than the application's is refused here, before any window opens (see "A
   version mismatch" below).
5. **This computer's key**, after a login with the password or a key file: the owner is asked whether to add it, the
   connection ending with the question, and it is added only where they agree (see "Asking before the key is added"
   below).
6. **Showing the daemon**: the application listens on a loopback port of its own, which the system assigns, and opens
   the daemon's window there with the owner key; each connection the window opens through that port is carried to the
   daemon by one run of `octoboardd proxy` over the SSH connection. The daemon never listens beyond its own machine's
   loopback and owner endpoint for this.

A host key confirmed at the first connection is pinned once the connection succeeds.

### What a connection asks or reports

Every question and failure of a connection is shown on the Remote devices page of the main window, as a dialog over
the window, wherever the connection was asked from: the page's own Connect or Open, or a menu's item for a saved
daemon. The page shows for it, and for a connection asked for from a menu the main window comes forward (see
"Connecting from the page" and "What is handed to the page" in `docs/product/remote-devices-page.md`); no remote window
shows one, and none is open for that daemon yet. A failure is an error alert (see "The error alert" in
`docs/product/error-alerts.md`). What the host or `octoboardd` said is under the alert's **Error details**, with
anything shaped like a key taken out.

Two kinds of connection ask and report nothing: those a launch makes (see "Bringing remote windows back at launch"
below), and the reconnection of a window whose connection dropped (see "A dropped connection" below).

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
  longer accepts this computer's key and to sign in with the password instead. Once signed in, the owner is asked
  about the key again, as at a first connection; it is not added again without asking.
- **This computer's key could be added**, after a sign-in with the password or a key file: the question described in
  "Asking before the key is added" below.
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

The password, the passphrase, a fingerprint just confirmed and the answer about this computer's key are kept for the
retries of that one connection, and dropped when it ends.

### A version mismatch

A remote daemon of another protocol generation is refused before its window opens: the dialog, on the Remote devices
page, names the daemon and the two release versions, the daemon's and this application's, and says which side is
behind. Neither side is updated from here: updating is done on each machine itself (see "No updates in a remote window"
below).

- **The daemon is behind**: it says the two cannot work together until the daemon is updated, on the server itself.
  Its footer holds **Cancel** and **Open in browser**.
- **This application is behind**: it says they cannot work together until this application is updated. Its footer holds
  **Go to Updates**, which closes the dialog and opens Settings' Updates section in the main window, and **Open in
  browser**; the dialog's close button dismisses it.

Either way it says that the daemon's own web interface, which matches its version, can be opened in the browser
instead. **Open in browser** connects again, holding the connection for the browser (see below).

A remote daemon of the same protocol generation is connected whatever its release version. What a newer one reports
that this application does not know is handled as `docs/product/values-a-newer-daemon-sends.md` says.

## Opening a remote daemon in the browser

**Open in browser** holds the connection without a window: the application listens on its loopback port as for a window,
and opens the system's default browser at that port with the owner key, where the daemon serves its own UI, of its own
version. The address, which carries the owner key, is opened by the application itself and never given to a page. Such a
connection is listed with the connected remote daemons, its state **Opened in browser**; choosing it in a menu, or Open
on the Remote devices page, opens the browser at it again. It is ended by **Disconnect** (below) and
by quitting the application, and is not brought back at the next launch. While it is held, the owner key is read again
whenever the daemon's key changes, so the address it opens follows a rotation. Nothing is forwarded for it: a loopback
address its page shows opens as written, on this computer (see "Loopback addresses of the remote machine" in
`docs/product/links-and-files-in-a-remote-window.md`).

## Device keys

**The application can sign in to a saved remote daemon with a key of this computer's own**, so later connections sign
in without asking for a password or passphrase. It is added to the remote host only where the owner agrees to it, the device's page shows whether it is
there (see "This computer's key" in `docs/product/remote-devices-page.md`), and it is taken back from the host and from
this computer when the owner turns it off or removes the saved daemon. Adding and removing this key's line is the only
change the application makes to a remote host's files.

- The key is an Ed25519 key pair made for that saved daemon. The private key is kept in the system's credential store —
  the login keychain on macOS, the Credential Manager on Windows — and never leaves the computer; only the public key is
  sent. Nothing else of the remote daemons is kept there.
- Its line is the public key, marked `Octoboard on <this computer's name>`, so it can be told apart there. It goes in
  the remote user's authorized keys: `~/.ssh/authorized_keys` on a Linux or macOS host; on a Windows host the file that
  user's login reads, for an administrator `%ProgramData%\ssh\administrators_authorized_keys`, and for any other user
  `~\.ssh\authorized_keys`.

A saved daemon's key is in one of three states, which the interface words in terms of adding the key; below, a key
that is added is also called installed:

- **Added**: its line is on the host, and the saved daemon signs in with it.
- **Not added, asked for at the next connection**: what a new saved daemon starts as, and what turning the key on
  leads to.
- **Off**: the owner said no when asked, or turned it off. Nothing is added and nothing is asked until it is turned
  on again.

### Asking before the key is added

After a sign-in with the remote user's password or a key file, once the daemon has been reached and read, a saved
daemon whose key is neither installed nor off adds nothing: the connection ends with a question, shown as any question
of a connection is (see "What a connection asks or reports" above), and nothing has been written on the host. The
question, **Add this computer's key**, names the daemon and says:

- what is added: one line, this computer's public key, with its mark shown as a monospace literal (the text the owner
  would look for in the file), and nothing else in the file changed;
- where: the file, for that user on that host — `~/.ssh/authorized_keys` on a Linux or macOS host, and on a Windows
  host the full path of the file that user's login reads, which the host is asked for without anything being written;
- what it does: later connections from this computer sign in with it without asking for a password or passphrase;
- that the private key is kept in the system's credential store, and that removing the remote daemon or turning its key
  off removes the line and the private key;
- that without it connections keep signing in as they do, the question is not asked again, and the key can be turned on
  later on the daemon's page.

**Add key** and **Do not add** each connect again with that answer, without asking for anything already given; the
close button ends the connection with nothing installed, and the next connection asks again. That holds for the
question that follows a key the host stopped accepting too (see "Installing the key" below): closing it ends a
connection the owner had already signed in to with the password, and connecting again asks for the password and then
the question again. The question comes after
the seconds a connection takes, so it opens with focus on the dialog itself rather than on Add key (see "Where keyboard
focus starts in a confirmation" in `docs/product/dialogs.md`).

- **Add key** installs the key, and the connection goes on (see "Installing the key" below).
- **Do not add** installs nothing, and the connection goes on with what the owner signed in with. The answer is
  remembered for that saved daemon, whose key is off from then on. It is noted before anything else — before the host
  is spoken to again, and even where a window already shows that daemon — so it stands even where that reconnection
  fails, and noting it changes nothing else. Where this computer still holds a key for it (one the host stopped
  accepting, say), a connection takes that key back: its line is taken out of the host's file where it is still there,
  and the private key is removed from the credential store. A saved daemon whose key is installed goes on being shown
  as installed until then, and signs in with a password afterwards. The connection made with the answer does it; where
  that connection fails, the next sign-in with the password does it, asking nothing.
- **A host that cannot say which file its keys are in** (a Windows host that does not answer the request for it) is not
  asked about: the connection goes on with what the owner signed in with, and the next one asks again.

Connections made at launch ask nothing about the key and add none (see "Bringing remote windows back at launch"
below), nor does the reconnection of a window (see "A dropped connection" below). A connection asked for from a menu
hands the question to the Remote devices page, as any of its questions (see "What is handed to the page" in
`docs/product/remote-devices-page.md`).

### Installing the key

- The line is appended as `ssh-copy-id` does: the file and, on a Linux or macOS host, `~/.ssh` are created private
  where they do not exist, the lines already there are left as they are, and nothing is added where the key is already
  there. On a Windows host, an administrator's file created here is given the access rights the SSH server requires
  (Administrators and SYSTEM only); an existing file's access rights are left as they are.
- The key is used only once a connection made with it has been accepted; the saved daemon then signs in with it, and
  keeps the file and the mark of its line.
- **An install that fails leaves nothing behind**: a key the host does not accept is taken out of its file again, and
  a key pair made for that attempt is removed from the credential store. A key installed for a saved daemon that can
  no longer record it — one removed or pointed elsewhere meanwhile, or whose saved remote daemons cannot be written —
  is taken back at once, over the connection still open. Either way the window opens all the same, and the next
  connection asks again.
- **A key the host no longer accepts** (its line was removed there): a connection already open lasts until it drops.
  The next connection reports that the server no longer accepts this computer's key and asks for the password; once
  signed in, the owner is asked about the key again, as at a first connection. A saved daemon whose private key this
  computer no longer holds asks for the password, and then about the key, in the same way.

### Turning the key off and on

The device's page turns the key off or on (see "This computer's key" in `docs/product/remote-devices-page.md`).

- **Off, where the key is installed**: the key is taken back (see "Taking the key back" below) and the saved daemon is
  kept. It then signs in with a password, asked for at its next connection, unless the owner chooses a key file on the
  form, and it is not asked about the key.
- **A window already open on a daemon whose key was taken back** — turned off, the saved daemon removed or pointed
  elsewhere — keeps its connection until it drops. After that its reconnections are refused: a reconnection signs in
  with what the connection was made with, the key that is gone, and asks nothing (see "A dropped connection" below).
  The way back is to close the window, or choose Disconnect, and connect again where the daemon is still saved, which
  asks for the password.
- **Off, where nothing is installed**: nothing changes on the host; the answer is only remembered.
- **On**: installs nothing by itself; the next connection asks before adding the key.

### Taking the key back

The key is taken back when the saved daemon is removed, when its key is turned off, and when a save stops it signing
in with the key — a change of host, port or user — in which case it is taken back from the host it was installed on,
before the change is saved. Taking it back:

- connects to the host with the key itself and takes out exactly the line that carries this key, keeping every byte
  of every other line: lines in another encoding, with a NUL or with CRLF endings stay as they were, a last line with
  no line ending keeps having none, and a byte order mark stays. The file is rewritten in place, never replaced: on a
  Linux or macOS host it stays the same file with its owner, permissions and links, and on a Windows host it keeps its
  access rights; it stays, empty, where that line was its only one. Nothing of the application's is left on the host;
- then removes the private key from the credential store.

Removing a saved daemon, saving it and turning its key off wait for a connection being made to that daemon to end,
and a connection asked for meanwhile waits for them, so a key being installed is recorded before the daemon is removed
or pointed elsewhere, and is then taken back.

**A host that cannot be reached, that refuses the key, or whose file does not hold the line** — the host reads its
keys from another file, or the line was moved — and a key this computer no longer holds, do not hold up the removal,
the save or the switch: it goes on, and the private key is removed all the same. The line is never reported as
removed. The application cannot tell these cases apart for certain, so an error alert, **Could not remove this
computer's key from the server**, says only that this computer's key could not be removed from the host, so the line
with its mark (shown as a monospace literal) may still be in the file for the user there — in the user's authorized
keys, where the application does not know the file — and that, if it is, the owner deletes it on the server to remove
the key. It adds that the private key was removed from this computer only where it was. What the host said is under
its Error details.

**A credential store that does not remove the private key** (a locked keychain, say) does not hold them up either. A
second error alert, **Could not remove this computer's private key**, says the credential store did not remove the
private key kept for the user on the host, and names its item — its service and account — for the owner to remove it
there; what the store said is under its Error details. It comes on its own, or after the first.

**A rewrite of the host's file cut short** — the write failed and the file's content could not be put back — may leave
the user's authorized keys incomplete, which does not hold the removal, the save or the switch up either, and the
private key is removed all the same. Instead of the first alert, an error alert titled as the server's authorized keys
possibly being incomplete says that removing this computer's key from the host was cut short while the user's
authorized keys were being rewritten, so keys may be missing from them; it names the copy of their earlier content the
host keeps, to restore the file from, since until then the keys it held may not sign in. What the host said is under
its Error details.

A key an earlier version of the application installed — a saved daemon that signs in with this computer's key but
holds no record of the file and the mark — counts as installed and is taken back in the same way. Where such a key is
left on its host, the alert names the mark this computer gives its keys and the user's authorized keys, without a file.

### Taking every key back at once

The application can take back every installed key in one go, speaking to the hosts side by side, so that one that
does not answer holds up no other. It acts on the saved daemons whose key is installed: it takes each key back and
turns that daemon's key off. It also acts on a saved daemon that no longer signs in with the key but whose private key
the credential store still holds, because the store did not remove it when the key was taken back: it removes that
private key, and changes nothing on the host or in the saved daemon. It reports, for each saved daemon it acted on,
what became of its line and its private key (as above). Any other saved daemon is left as it was: one never asked is
still asked at its next connection. It is reached only through **Remove all** in the clean-up before uninstalling, in
the main window's Settings, whose confirmation lists a private key left that way as an item of its own, marked as the
private key only, and whose result shows, under each key not removed, what became of it (see `docs/product/clean-up-before-uninstalling.md`).
The Windows uninstaller's clean-up takes every key back the same way, with nothing listed first, and names what stays
at its end ("The Windows uninstaller's clean-up" there).

## The remote window

A remote window is opened once its daemon is connected, never before, and is used as the main window is, with these
differences:

- **Its title** is the daemon's name followed by the application's name. **Its top bar** shows the daemon's name in a
  chip at the bar's start, before the breadcrumb, with a server glyph, read by assistive technology as the daemon on
  that name; the chip keeps its width while the breadcrumb beside it is cut short, and a name too long for it fades,
  with the whole name in a tooltip (see "The top bar" in `docs/product/window-layout.md`). The chip is there on the
  screens shown before the daemon's state has arrived too.
- **Settings are the remote daemon's.** Every setting the daemon keeps is that daemon's, the language included, so the
  window shows, and its Settings change, the language the remote daemon holds (see "The language the daemon holds" in
  `docs/product/language.md`). What is the application's own is not offered there: Launch at login, Keep running in the
  background and Show menu bar icon in General (see `docs/product/general-settings.md`), the Permissions section, the
  Access section, and the application's updates (below). Cleanup's Clean up before uninstalling row has no action there and
  only says where the clean-up is done (see "Which installation it speaks for" in
  `docs/product/clean-up-before-uninstalling.md`). Appearance, the terminal font size and the keyboard
  shortcuts are the application's, shared with every window.
- **Nothing is opened on the remote machine, and nothing of this computer is offered for the daemon's files**: no File
  manager, Default application or System terminal under Open in (see `docs/product/sidebar-project-rows.md` and "A row's
  actions" in `docs/product/project-pane.md`), no File manager or Default application in the file viewer's header, which
  has **Download and open** in their place, opening a copy of the file on this computer (see "Files opened on this
  computer" in `docs/product/links-and-files-in-a-remote-window.md`), no Open in in the terminal's editing menu, and
  Browse uses the application's directory browser, which lists the remote machine's directories, instead of the system's
  picker (see "Browsing directories" in `docs/product/projects.md`). **File paths in a terminal are links** all the
  same, checked on the remote machine by the rules of any window: a click opens a file in the file viewer, which reads
  it through the remote daemon, and a folder is a link only when it has a README or index file to show there (see "File
  paths" in `docs/product/terminal-links.md`).
- **Web addresses open in this computer's browser**, one on the remote machine's own loopback through a forwarded port
  (see "Loopback addresses of the remote machine" in `docs/product/links-and-files-in-a-remote-window.md`).
- **It neither quits the application nor stops its daemon.** Quitting is the main window's (see "Quitting" in
  `docs/product/application-lifecycle.md`); the remote daemon and its sessions run on whatever the application does.
- **Its notifications and waiting count are its own daemon's**: a session waiting for the user in that daemon notifies
  through the application, a click on the notification brings this window forward and opens that session, and its waiting sessions count in
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
connection" in `docs/product/application-lifecycle.md`), and the daemon is shown **Reconnecting** on the Remote
devices page until the SSH connection is made again. The application makes it again by itself, after pauses that grow
between failed attempts, 1 second, then 2, 4, 8 and 16, up to **30 seconds**, for as long as the window is open, and as
long as a connection is held for the browser (see "Opening a remote daemon in the browser" above). Every connection
made to the daemon's loopback port meanwhile — the window's own, a terminal's, a file it loads, the browser's —
also connects over SSH again, without waiting for that pause; connections that come while an attempt is under way wait
for it, and fail with it. A connection through a forwarded port fails while a pause runs instead (see below).

**The window retries until it connects or is closed**, with no number of attempts after which it gives up, its own
pauses growing in the same way, so that an attempt it makes once the remote machine can be reached again connects.
Once the SSH connection is made again, by whichever connection, the window is told and connects to its daemon at once,
its pauses started over, so that it and the Remote devices page stop saying it is reconnecting within moments of each
other.
Its connection banner says it is reconnecting all the while, with **Try again**, which attempts at once, after an
attempt still under way has ended, starts the window's pauses over from 1 second, and shows as busy until its attempt
connects or fails.

**A connection that would meet the same answer again** — the remote machine refuses the password or the key, or
shows a host key other than the one pinned, or the key file cannot be read or needs a passphrase — is tried again only
after **5 minutes**, whatever connection asks, so that a password the machine refuses is not sent again and again (a
host may ban a computer that does so). Try again tries it at once all the same.

The owner key is read again over each new SSH connection, and whenever the window has no connection open, so a key
rotated meanwhile is followed: the window is pointed at the new key only where it changed.

**A reconnection asks nothing and reports nothing outside its window**: it signs in with what the connection was made
with, kept in memory, and a failed attempt is tried again by itself after its pause — one that would meet the same
answer only after the long pause above, or at once on Try again. No dialog opens for it, in the remote window or on
the Remote devices page. To start over, with another password say, the owner
closes the window or chooses Disconnect, and connects again.

The ports forwarded for the window's loopback addresses (see "Loopback addresses of the remote machine" in
`docs/product/links-and-files-in-a-remote-window.md`) stay the application's meanwhile: a connection through one fails
until the SSH connection is back, and then works again with no new opening. An address opened while it is down raises
an error alert at once rather than waiting for the reconnection.

## Disconnecting and closing

**Closing a remote window disconnects it**: the loopback port and the ports forwarded for its loopback addresses are let
go and the SSH connection ended, and nothing is
asked of, or stopped on, the remote daemon, whose sessions run on. The window closes for good; which window then shows,
or whether the application goes into the background or quits, follows "Closing the window" in
`docs/product/application-lifecycle.md`.

**Disconnect**, on a remote daemon's tab in the menu bar icon's menu (see "Disconnecting" in
`docs/product/menu-bar-icon.md`), ends that daemon's connection: it closes its window, or lets go of the connection held
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
owner connects it. One that would be asked about this computer's key goes on without it, adding nothing and leaving
the question for a connection the owner makes. A launch at login that keeps the application in the background brings
none back. None of these
connections shows the Remote devices page.

**Where the main window was hidden at quit**, it is held hidden while the remote windows are connected, and stays hidden
once at least one of them is back, until something shows it. When none has come back within **15 seconds**, it is shown
as at any launch; a remote window that connects later still opens.
