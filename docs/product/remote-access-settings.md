# Remote access settings

Settings' **Remote access** section (see "Remote access" in `docs/product/settings.md`) is where the owner opens the
desktop application's own daemon to their other devices: a browser on another computer, or another computer's
application (see `docs/product/remote-daemons.md`), each reaching it through an SSH login to this computer. What turning
it on opens, and to whom, is in "Remote access to the desktop application's daemon" in
`docs/product/who-can-reach-the-daemon.md`.

**Only the main window of the macOS and Windows applications has the section**, for the daemon the application runs
itself. A browser never has it, since it would be changing the very access it came in through, and the daemon refuses
such a change from a browser anyway; nor does a remote window, whose daemon is another machine's.

Its opening paragraph says that the section lets the owner use the application from a browser on another computer
through an SSH connection to this one, and that it works only while the application runs here: quitting it ends remote
use.

## Allow remote access

A switch, **off to begin with**. While it is off its description says what turning it on allows; while it is on, that a
browser that reaches the application with its key can do everything the owner can on this computer.

**Turning it on asks first**, in a confirmation saying that whoever reaches the address with its key can do everything
the owner can on this computer, running any command included; that the key lasts until it is rotated and travels with
the browser's history, bookmarks and sync; and that reaching it from another computer goes through an SSH login to
this one. On Windows it adds that, on a computer other people also log in to, one of them can take the browser port
while the application is not running or is restarting, and a browser that reconnects then sends them the key.
**Allow** turns it on; Cancel leaves it off. Turning it off asks nothing. The switch shows the daemon's own state
throughout, and a change that fails is an error alert.

The daemon keeps the setting, so it holds from the daemon's start, and takes effect at once in both directions. It is
offered neither to the agents' settings tools (see `docs/product/agent-settings-tools.md`) nor in a configuration
export (see "Which settings travel" in `docs/product/settings-in-a-configuration.md`).

## When the daemon cannot serve

While remote access is on but the daemon could not open its way in, the section shows an error under the switch, titled
that remote access is on but no browser can be served, with the reason in the application's words and the system's own
words behind its details:

- the daemon has no interface to serve;
- the folder that holds the application's data is not private to the user, so the socket a browser connects through was
  not opened (macOS);
- the socket could not be opened (macOS);
- the key that signs a browser in could not be created or read;
- the browser port is already in use, by another program or another user of the computer, and is to be changed to a
  free one (Windows);
- the application could not listen on the browser port for another reason (Windows);
- the named pipe other computers connect through could not be opened, another program or user possibly holding its
  name (Windows).

The error is read out when it appears while the section is shown, not when the section opens on it. Nothing is served
meanwhile; the rest of the daemon runs as usual.

## Browser port

On Windows alone, a row giving the loopback port the daemon serves the browser at, the server form's default port to
begin with, whatever the switch's state, with **Change port**. Its description says that it is the port on this computer
the SSH forward reaches the application at, and that changing it disconnects the pages open at the old port. Change
port opens a dialog with one field, a whole number from 1 to 65535 that no other program uses; anything else is
refused under the field. The change is sent on Save and takes effect at once: while remote access is on, the old port is
let go, every page the owner key opened is disconnected, and the new port is listened on. A port already in use is no
failure of the change: the section then shows the error above. The port, like the switch, is in no export and no
settings tool.

## While remote access is served

While the daemon serves, the section shows four more rows:

- **Browser address**: the address a browser opens, `http://127.0.0.1:<port>/?key=<owner key>`, with a copy action. Its
  description says to open it in a browser on the other computer once the forward below is running, and that the key in
  it signs the browser in. `<port>` is the server form's default port on macOS, the same at every launch, and the browser
  port on Windows.
- **SSH forward**: the command that forwards that port on the other computer to this one's way in, with a copy action;
  its description says to run it on the other computer with `<server>` replaced by this computer's address, and to leave
  it running while the browser is in use. On macOS it forwards to the daemon's socket,
  `ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:<port>:<socket path> <user>@<server>`; on Windows, whose SSH server
  cannot forward to a socket, to the browser port, `ssh -N -o ExitOnForwardFailure=yes -L
  127.0.0.1:<port>:127.0.0.1:<port> <user>@<server>`, `<user>` being this computer's login name.
- **Connection details**: the line another computer's application is set up from, as `octoboardd connection` prints it
  (see "Printing the connection details" in `docs/product/daemon-command-line.md`), with a copy action, and its QR code
  below it. The host is this computer's own name, the port the default SSH port, the fingerprint that of this computer's
  SSH host key where it can be read, and the path of `octoboardd` the application's own. Its description says that the
  other computer's application connects through this computer's SSH server: there, **Add remote daemon** is chosen and
  the details pasted into its Connection details field (see "Saved remote daemons" in
  `docs/product/remote-daemons.md`); that the host is to be corrected there if the other computer cannot reach this one
  by that name; and that the application has to keep running here.
- **Rotate key**, a button, whose description says that it gives the address a new key and disconnects the pages open
  with the old one. It asks first, in a confirmation saying that the pages open with the old address are disconnected
  and have to be opened again with the new one; **Rotate** replaces the key, as `octoboardd address --rotate` does (see
  "The owner key" in `docs/product/who-can-reach-the-daemon.md`), and the address above shows the new one. A rotation
  that fails is an error alert.

## The SSH server

The last row says whether this computer's SSH server is on, since the forward and another computer's application both
log in through it: **Remote Login** on macOS, **OpenSSH Server** on Windows, its description saying that another
computer reaches this one over SSH, which the system calls by that name.

- **On** (on Windows, running): a badge saying so.
- **Off** (on Windows, not running or not installed): a line saying how to turn it on in the system's settings — on
  macOS under General, then Sharing, then Remote Login; on Windows by adding OpenSSH Server under System, then Optional
  features, then starting the OpenSSH SSH Server service in Services and setting it to start automatically — with a
  button that opens the system's settings there.
- **Not known**, where it cannot be read: a line saying the application could not tell, where to check, and the same
  button.

Turning it on needs an administrator, which the application never asks for itself. It is read when the section opens
and again whenever the window gets the focus back, and a change of it is read out. A system settings app that does not
open is an error alert saying where to go by hand.
