# Remote Access over SSH Development Plan

## Problem and approach

The owner wants to reach their daemon from another device: the macOS or Windows application controlling a daemon on
a Linux server (the server form, see "Two deployment forms" in `docs/architecture.md`), or another machine reaching
the daemon of their desktop form, from the application or from a browser. The daemon is a remote shell by nature — it
starts agents, types into them and lists directories — so the connection needs authentication and encryption.

SSH already provides both, and the owner already has it to their server. Another machine therefore reaches a daemon
through SSH and only through SSH: the daemon never listens beyond loopback, and Octoboard builds no network layer,
TLS, device credential or hosted service of its own. A browser goes through the owner's own SSH forward; the
application carries an SSH client and reaches the daemon through a command it runs on the remote machine. Setting a
device up is made easy with connection details shown on the remote machine, as text and as a QR code.

One user, any number of their devices. Whoever can log in over SSH as the daemon's Unix user can already control the
daemon, so that login is the credential: a device is let in by its SSH key and revoked by removing it.

## Key design decisions

- **The owner endpoint.** A daemon is opened from another machine at an endpoint only the daemon's user can reach,
  and the owner key is accepted there alone: the socket in the data directory on Linux and macOS, as the server form
  has today, and on Windows a named pipe only that user can open. The daemon's own TCP port never accepts the owner
  key; on Windows alone a separate loopback listener for the browser does (see "A browser" below). Another user of
  the machine can take a loopback port the daemon is not holding and be sent the key; nobody else can create, replace
  or connect to the endpoint.
- **Remote access to a desktop daemon is opt-in.** One Allow remote access switch, off by default and confirmed
  explicitly when turned on, makes the application's daemon open its owner endpoint, serve the UI on it and accept
  the owner key there. The daemon keeps the setting, and it takes effect at once in both directions: turning it off
  closes every connection the owner key opened, as rotating the key does. It is offered neither to the agents'
  settings tools nor in the configuration document, so that neither those tools nor an import can open a machine to
  remote use.
- **The desktop daemon carries the UI**, as the server builds do, so both forms serve a browser through one code path
  and the switch needs no restart. It costs about 7 MB of every download (the UI build is 27 MB uncompressed); the
  embedded files are stored compressed, which keeps the installed daemon small in both forms. The window keeps
  loading the UI the application bundles (settled by the user).
- **A browser reaches a daemon through the owner's own SSH forward.** On Linux and macOS that is a forward of the
  socket, as in the server form. Windows' SSH server cannot forward to a socket or a named pipe (Win32-OpenSSH 9.5
  and 9.8 both fail, and its fix has no date), so on Windows alone the daemon accepts the owner key at a fixed
  loopback TCP port, on a listener of its own, while remote access is on, as VS Code's remote server does on a
  Windows host (settled by the user). The confirmation says what that costs: on a computer other people also log in
  to, one of them can take that port whenever the daemon is not holding it — while the application is not running,
  or while its daemon is restarted — and be sent the key by a browser that reconnects.
- **The application reaches a remote daemon through a command it runs over SSH**, not through an SSH forward. It
  runs, as the daemon's user, a mode of `octoboardd` that connects to that daemon's owner endpoint and carries the
  bytes on its standard input and output, once for each connection the window opens, behind a loopback port the
  application listens on itself and holds for as long as the window lives, so that the window never sends the key to
  a port someone else could have taken. Docker (`docker system dial-stdio`) and Zed (its server's proxy mode) reach a
  per-user daemon over SSH the same way. It works on every host system, Windows included; it does not depend on the
  SSH server's forwarding settings; and the application need not know where the endpoint is.
- **The application carries its own SSH client** (russh): one connection authenticates, runs the commands and carries
  every proxied connection; a failure is told apart by kind (a wrong password, a host key that does not match, no
  daemon running); macOS and Windows behave alike; and a mobile client can reuse it. The cost is an SSH
  implementation among the application's dependencies and tunnel traffic passing through the application's process.
- **First login with the user's own SSH credentials** (a password or a key they already have). The application then
  makes a key pair of its own for that device, installs the public key for the remote user, as `ssh-copy-id` does,
  and connects with it from then on. The private key is kept in the system's credential store; a password is never
  stored.
- **The host key is pinned.** The connection details carry the remote host key's fingerprint, and the application
  refuses a host whose key does not match it. For a host typed in by hand with no fingerprint, the application shows
  the fingerprint at the first connection, connects only once the owner confirms it, and pins it from then on.
- **Revoking a device** is removing its SSH key; a connection already open lasts until it drops. Listing and removing
  device keys from within Octoboard is not part of this plan: the setup guide says how to remove one on the remote
  machine.
- **One window per daemon.** The local daemon has its window, and each connected remote daemon has one of its own,
  with its own consoles, projects, sessions and notifications, marked with the remote host's name. The UI is already
  one page for one daemon, so each window is isolated as it stands, whereas one window switched between daemons would
  leave the daemons not shown without notifications and could not ask about, or stop, the local daemon at a quit
  made while it shows a remote one. Quitting the application stops only the local daemon: a remote window only
  disconnects, and the remote daemon's sessions run on. Closing a remote window disconnects it, and the existing
  rule for closing the window (quit, or into the background) applies when the last visible window is closed.
  Application-level settings (updates, launch at login, running in the background, Permissions, Remote access) are
  offered in the local window only. Language, appearance, terminal font size and keyboard shortcuts are the
  application's, shared by every window; what a window remembers of its daemon (the selected console, the file
  browsers' state and the like) is kept per daemon.
- **One menu bar icon per daemon**, the remote ones marked, each with its own daemon's sessions, attention dot, quota
  and keep-awake eyes, gone when its daemon is disconnected (settled by the user). The Dock badge is the sum of the
  windows' counts.
- **A remote daemon of another protocol generation is refused before its window loads**, naming both versions and the
  ways out: updating the server's daemon, which the application can ask for over SSH, or updating the application.
  The screen for a daemon of another version would be a dead end there, since its way out is to restart the
  application.
- **Finding `octoboardd` on the remote machine.** A command run over SSH does not get a login shell's `PATH`, so the
  application looks in the places Octoboard installs to (the server form's install directory, the application's own
  files) and the connection details carry the path.
- **The connection details carry no secret**: host, SSH port, user, host key fingerprint and the path of
  `octoboardd`. They are shown as text and as a QR code, in the Linux terminal by `octoboardd` and in the desktop
  application. The text, pasted into the application, is what is used today; the QR code is for a mobile client,
  which does not exist yet, and is built now so that the host side needs no change then (settled by the user).
- **The desktop form can be a host too.** The application checks whether the computer's SSH server is on (macOS
  Remote Login, Windows OpenSSH Server) and guides the owner to turn it on. Everything about reaching the desktop
  daemon from elsewhere lives in one Remote access section of Settings, which the milestones that concern it add to.
  Quitting the desktop application still stops its daemon, so remote use needs the application kept running.
- **No relay.** Reaching a daemon without SSH, through a relay as a remote-desktop tool does, is a separate plan.

## Milestones

- [01 Remote access switch and browser access on macOS](01-remote-access-switch-and-browser-access-on-macos.md) —
  needs the owner key and the browser socket, which exist
- [02 Browser access on Windows](02-browser-access-on-windows.md) — needs 01
- [03 The owner endpoint command](03-owner-endpoint-command.md) — needs the browser socket of the server form,
  which exists
- [04 One window per daemon](04-one-window-per-daemon.md) — needs nothing of the others
- [05 Connecting to a remote daemon over SSH](05-remote-daemon-over-ssh.md) — needs 03 and 04
- [06 Menu bar icon, Dock badge and notifications per daemon](06-menu-bar-icon-dock-badge-and-notifications.md) —
  needs 05
- [07 Device keys](07-device-keys.md) — needs 05
- [08 Connection details and setup guide](08-connection-details-and-setup-guide.md) — needs 05
- [09 The desktop form as a host](09-desktop-form-as-host.md) — needs 01, 02, 03 and 08

## Open

- Which mobile client a phone uses; no mobile client exists yet.
- Where in the window remote daemons are listed, added and connected from.
- Whether the first version honours the owner's SSH agent and `~/.ssh/config` (host aliases, identity files), and
  jump hosts.
- Whether remote windows come back at launch, at a launch at login and after a restart into an update.
- Whether a remote window offers the remote daemon's own update, as a browser on a server form's daemon does.
- Which window Settings and About open in when no window has the focus.
- The default port of browser access on Windows.
- Whether Windows offers the application's way in (the named pipe) without also opening the browser's port.
- How the page for a refused key words its way back (today: run `octoboardd address` on the server) when the daemon
  is the desktop application's.
- How a desktop host whose application is not running is told from a server whose daemon is stopped, for which the
  application offers to start it.
