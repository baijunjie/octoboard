# Access settings

Settings' **Access** section (see "Access" in `docs/product/settings.md`) is
where the owner opens the desktop application's own daemon to others: to a browser on this computer, at the loopback
browser port; to a browser on another computer, and to another computer's application (see
`docs/product/remote-daemons.md`), both through an SSH login to this computer. It holds two switches, **Allow browser
access** and **Allow remote access**, which are independent: either can be on without the other. What turning each on
opens, and to whom, is in "Remote access to the desktop application's daemon" in
`docs/product/who-can-reach-the-daemon.md`.

**Only the main window of the macOS and Windows applications has the section**, for the daemon the application runs
itself. A browser never has it, since it would be changing the very access it came in through, and the daemon refuses
such a change from a browser anyway; nor does a remote window, whose daemon is another machine's.

Its opening paragraph says that the section lets the owner use this computer's application in a browser on this
computer, or from another computer through an SSH connection to this one, in a browser or in that computer's own
application, and that it works only while the application runs here: quitting it ends that use. Where the daemon has no
browser port (it is the macOS and Windows applications' that has one), the paragraph speaks of another computer's
application alone and does not mention the browser, and the section has no This computer's browser group and no tabs below.

The rows are in three groups, by where they are used (headings as in "Settings groups" in
`docs/conventions/ui-standard.md`): **This computer's browser**, **Other computers** and **Key**. Searching in Settings
finds each row under the section's name and its group's heading.

## This computer's browser

Only where the daemon has a browser port. In order: the Allow browser access switch, why the port could not be opened
(while it could not), the browser port, and, while the port is open, the local address.

### Allow browser access

A switch, **off to begin with** (on Windows, a store from before the setting began with the value remote access had, so
an update changes nothing a user had on). While it is off its description says what turning it on allows, that a
browser on this computer can open the application at the browser port with no SSH connection; while it is on, that a
browser that reaches the port with the key can do everything the owner can on this computer. It opens the loopback
browser port and nothing else.

**Turning it on asks first**, in a confirmation saying that whoever reaches the address with its key can do everything
the owner can on this computer, running any command included; that the key lasts until it is rotated and travels with
the browser's history, bookmarks and sync; and that, on a computer other people also log in to, one of them can take the
browser port while the application is not running or is restarting, and a browser that reconnects then sends them the
key. **Allow** turns it on; Cancel leaves it off. Turning it off asks nothing, and closes the port and the connections
made through it, leaving the socket or pipe as it is. A change that fails is an error alert. The daemon keeps the
setting like remote access's, and it is in no export and no settings tool; `octoboardd browser on` and `off` change it
too (see `docs/product/daemon-command-line.md`).

While it is on but the daemon could not open the port, the section shows an error under the switch, titled that browser
access is on but the browser port is not open, with the reason in the application's words and the system's own behind
its details:

- the browser port is already in use, by another program or another user of the computer, and is to be changed to a
  free one;
- the application could not listen on the browser port for another reason;
- the daemon has no interface to serve, or the key that signs a browser in could not be created or read.

The error is read out when it appears while the section is shown, not when the section opens on it. Remote access, where
it serves, goes on meanwhile. The section offers no forward to the socket meanwhile: while the port is not open it shows
no address and no steps.

### Browser port

A row giving the loopback port the daemon serves the browser at, the default browser port, 3369, to begin with, whatever
the switch's state, with **Change port**. It is the user's setting whether or not the daemon serves. Its description
says that it is the port a browser on this computer, and an SSH forward from another computer, reach the application
at, and that changing it disconnects the pages open at the old port. Change port opens a dialog with one field, a whole
number from 1 to 65535 that no other program uses; anything else is refused under the field. The change is sent on Save
and takes effect at once: while browser access is on, the old port is let go, every page connected through it is
disconnected, and the new port is listened on. A port already in use is no failure of the change: the section then
shows the error above. The port, like the switches, is in no export and no settings tool.

### Local address

Where the browser port is open (not while another program holds it, which the error above says), the **Local address**
row: the address `http://127.0.0.1:<port>/?key=<owner key>` in a field that carries two icon buttons at its end, **Open
in browser**, which opens it in the user's default browser as every link the application shows is opened, and Copy. Its
description says to open the address in a browser on this computer and that the key in it signs the browser in.

## Other computers

The SSH server comes first, above the tabs, since both ways in from another computer log in over it. Under it, where
the daemon has a browser port, two tabs, one per way in, in the underline strip every Settings page's tabs take (see
"Within a Settings page" in `docs/conventions/ui-standard.md`): **Octoboard app**, selected to begin with, and
**Browser**. The tab chosen is not remembered when Settings is opened again. Where the daemon has no browser port there
are no tabs, just the application's rows. A search in Settings that finds rows in one tab only shows that tab, whichever
was selected, and the other tab cannot be chosen until the search is over or finds something in both; while it finds
only the SSH server or rows of other groups, there are no tabs.

The **Octoboard app** tab holds the Allow remote access switch, why remote access could not be served (while it could
not), and, while remote access serves, the connection details. The **Browser** tab holds the steps for a browser on
another computer, or the line to turn browser access on (see "The Browser tab").

### The SSH server

The first row shows the status of this computer's SSH server, **Remote Login** on macOS and **OpenSSH Server** on
Windows. Its description explains that access from another computer through the application requires an SSH connection
to this one; where the daemon has a browser port, that access through the application or a browser does (on Windows,
that both another computer's application and a browser log in to this one over SSH).
When the server is off or its status is unknown, the status and directions appear above the description.

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

### Allow remote access

A switch, **off to begin with**. While it is off its description says what turning it on allows, that another computer's
application can use the application through an SSH connection to this one; while it is on, that such an application can
do everything the owner can on this computer. It opens the socket on macOS and Linux and, on Windows, the named pipe; it
opens no browser port, whatever that setting is. The application no longer offers a browser the SSH forward to the
socket: a browser is let in through Allow browser access. The socket itself still serves, and a browser that reaches it
by the owner's own forward still works (see "Remote access to the desktop application's daemon" in
`docs/product/who-can-reach-the-daemon.md`).

**Turning it on asks first**, in a confirmation saying that whoever reaches the address with its key can do everything
the owner can on this computer, running any command included; that the key lasts until it is rotated; and that reaching
it from another computer goes through an SSH login to this one. It says nothing of the browser, its history, bookmarks
and sync, or its port: those are the Allow browser access confirmation's. **Allow** turns it on; Cancel leaves it off.
Turning it off asks nothing, and closes the socket or pipe and the connections made through it, leaving whatever runs
through the browser port as it is. The switch shows the daemon's own state throughout, and a change that fails is an
error alert.

The daemon keeps the setting, so it holds from the daemon's start, and takes effect at once in both directions. It is
offered neither to the agents' settings tools (see `docs/product/agent-settings-tools.md`) nor in a configuration
export (see "Which settings travel" in `docs/product/settings-in-a-configuration.md`).

### When remote access cannot be served

While remote access is on but the daemon could not open its way in, the section shows an error under the switch, titled
that remote access is on but this computer cannot be reached, with the reason in the application's words and the
system's own words behind its details, all worded for another computer's application and never for a browser:

- the daemon has no interface to serve;
- the folder that holds the application's data is not private to the user, so the socket another computer's application
  connects through was not opened (macOS; on Linux, nothing is served);
- that socket could not be opened (macOS);
- the key that signs another computer's application in could not be created or read;
- the named pipe other computers connect through could not be opened, another program or user possibly holding its
  name (Windows).

The error is read out when it appears while the section is shown, not when the section opens on it. Nothing of remote
access is served meanwhile; the rest of the daemon, browser access included, runs as usual.

### Connection details

One row, while remote access serves: the line another computer's application is set up from, as `octoboardd connection`
prints it (see "Printing the connection details" in `docs/product/daemon-command-line.md`), with a copy action, and its
QR code below it. The host is the name this computer announces on its network (on macOS its Bonjour name,
`<LocalHostName>.local`; on Linux its host name and on Windows its computer name, as the system reports them), the port
the default SSH port, the fingerprint that of this computer's SSH host key where it can be read, and the path of
`octoboardd` the application's own. Its description says that on the other computer **Add remote daemon** is chosen and
the details pasted into its Connection details field (see "Saved remote daemons" in `docs/product/remote-daemons.md`),
and that the host is to be corrected there if the other computer cannot reach this one by that name.

### The Browser tab

Only where the daemon has a browser port. It depends on Allow browser access:

- **While the browser port is open**, two steps:
  - **Step 1: SSH forward**: the command that forwards a port on the other computer to the browser port, with a copy
    action, `ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:<port>:127.0.0.1:<port> <user>@<server>`, `<user>` being
    this computer's login name; its description says to run it on the other computer and leave it running while the
    browser is in use, and that it connects a port there to this computer over SSH. It names no other tool, and reaching
    the port with a tool such as `tailscale serve` is described on the documentation site. Where the daemon lists the
    names and addresses another computer can reach this one by (this computer's name first, its IPv4 and then its IPv6
    addresses after; loopback, unspecified, multicast and link-local addresses, `localhost` names, deprecated,
    tentative and failed addresses, and on Linux temporary IPv6 addresses are left out), the `<server>` in the command
    is a pressable token showing the first of them, so that the command is ready to copy. Pressing it opens a menu of
    those and a last item, **Enter the address yourself**, which puts `<server>` back for the owner to replace; what is
    chosen is in the command and in what the copy action copies. With none listed the command is plain text and its
    description says to replace `<server>` with this computer's address.
  - **Step 2: Open the address**: it does not repeat the address, and says to open the address above, the local
    address of This computer's browser, in a browser on the other computer once step 1 is running.
- **While Allow browser access is off**, one row, **Browser access is off**, saying to turn on Allow browser access above
  to open the application in a browser on another computer.
- **While it is on but the port could not be opened**, one row, **The browser port is not open**, saying that the alert
  under Allow browser access above says why; the tab can be chosen. The section offers no forward to the socket Allow
  remote access opens, on either system.

Running the command needs this computer's SSH server on, the first row of the group.

## Key

**Rotate key**, a button shown while either way in serves (Allow remote access or Allow browser access): the daemon
holds one owner key for both. Its description says that browser access and remote access share one key, that a new key
disconnects the browser pages and the other computers' applications that use the old one, that the applications
reconnect with the new key by themselves, and that the pages have to be opened again at the new address. It asks first,
in a confirmation saying the same; **Rotate** replaces the key, as `octoboardd address --rotate` does (see "The owner
key" in `docs/product/who-can-reach-the-daemon.md`), cutting off every connection the old key opened (see "Revoking it"
there), and the local address shows the new one. A rotation that fails is an error alert.
