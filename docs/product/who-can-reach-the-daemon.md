# Who can reach the daemon

The daemon has no login and no notion of who is connected: whoever reaches it with its key is let in, and every client
is the same client. The key keeps web pages out, not programs. Where the daemon's terminals run in process holders (see
`docs/product/process-holders.md`), each holder's registry file holds the secret of the holder's connection to the
daemon, which controls every terminal that holder runs; the file is the user's alone, so any program running as the
same user can read it. Each agent and its helpers carry the key the holder issued for that launch (where it is kept is
under "Other users of the same machine" below). That key is accepted by the holder's routes for that session's helpers,
and by its MCP route for that launch's own MCP token alone, so it reaches the daemon's MCP tools as that session and no
other. Where holders cannot be set up (see "When holders cannot be set up" in `docs/product/process-holders.md`), they
carry the daemon's own key instead, kept in the same places. Either way, in the desktop form (the macOS or Windows
application, see `docs/product/application-lifecycle.md`) **any program running on this machine as the same user can do
everything the application can**: create consoles and sessions, start agents, type into their terminals, and open a
shell terminal and run any command in it (see `docs/product/shell-terminals.md`). It is safe there only because the
daemon listens on loopback, so whoever can reach it already has the machine.

In the server form (the daemon alone on a Linux server, see `docs/product/server-form.md`) the same sentence reads
**anyone who reaches the daemon with its key can do everything** — start agents, type into terminals and run any command
in a shell terminal on the server, as the Unix user the daemon runs as. So in both forms the daemon listens on loopback
and nowhere else — beside the socket in its data directory on which the server form, and the desktop application's
daemon while remote access is on, serve the browser, which only its own user can reach (see "The browser socket" in
`docs/product/server-form.md`), and on Windows the desktop application's named pipe (see "Remote access to the desktop
application's daemon" below) — and nothing in it lets it be put
beyond loopback: it binds to loopback only, and it refuses any request whose `Host` names it by anything but a loopback
address. A reverse proxy in front of it does not change that:
the daemon also refuses a request whose `Origin` — which a browser sends on every WebSocket upgrade — is neither the
application's, nor loopback, nor the same as `Host`, so a proxy would have to rewrite both headers, and the daemon
behind it would be open to whatever the proxy lets through with nothing of its own to tell it apart but the key, which
is one for every client, and which, for the browser, lasts until the owner rotates it (see "The owner key" below). As in the desktop form, any program running on the server as that
user can do everything too, through the keys and the holder secrets above.

**Another machine reaches the daemon through SSH, and only through SSH.** SSH authenticates and encrypts; the daemon
has no listener beyond loopback, no TLS and no credential of its own for a device. Whoever can log in over SSH as the
daemon's Unix user can reach it, so that login is the credential: a device of the owner's is let in by its SSH key and
revoked by removing that key — a connection already open lasts until it drops — and the login is to be kept like a
password. The desktop application installs a key of its own on the remote machine after its first login there (see
"Device keys" in `docs/product/remote-daemons.md`).

- **A browser** reaches the server form's daemon through the owner's own SSH forward of the daemon's browser socket to
  a port on their own machine, `ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:<port>:<socket path> <user>@<server>`,
  and opens `http://127.0.0.1:<port>/?key=<owner key>` (see "The browser socket" in `docs/product/server-form.md`), never
  through the daemon's TCP port. That is a loopback address, which the checks below admit once the page carries a key
  the daemon accepts, and which a browser treats as a secure context, so notifications and the clipboard work through
  it with no TLS. The owner gets that address with the key, and the forward with the real socket path and login name,
  by running `octoboardd address` on the server as the daemon's user, however the daemon was started (see "Running in
  the background from the command line" in `docs/product/daemon-command-line.md`). A server build of the daemon serves
  the UI at that address itself (see "The UI the daemon serves" in `docs/product/server-form.md`). A browser reaches
  the desktop application's daemon the same way on macOS while its remote access is on, or at its browser port while its
  browser access is on, with the address and the forward its Settings show (see "Remote access to the desktop application's daemon" below).
- **The macOS and Windows applications** reach a remote daemon — a Linux server's, or another of the owner's desktops'
  while its remote access is on — through an SSH connection they make themselves, verifying the host key against a
  pinned fingerprint, and running `octoboardd proxy` on the remote machine, as the daemon's user, for each connection
  the window opens (see `docs/product/remote-daemons.md`). Setting a device up is helped by the connection details
  `octoboardd connection` prints on the remote machine, or a desktop's Access section shows, as a line of text
  the application takes pasted and as a QR code of that line, carrying the host, SSH port, user, host key fingerprint
  and the path of `octoboardd`, and no secret (see "Printing the connection details" in
  `docs/product/daemon-command-line.md`). The same connection also carries, for a remote window, connections to the
  loopback ports of the remote machine whose addresses the owner opens there (see "Loopback addresses of the remote
  machine" in `docs/product/links-and-files-in-a-remote-window.md`).

The server form is the same user reaching their own daemon from another machine, not a daemon shared by several people.
The daemon does not tell clients apart: whoever reaches it with its key is taken for the owner and can do everything the
owner can. That is this version's design boundary, not a gap left open.

**Other users of the same machine share its loopback, and the key is what keeps their processes out**, with, for the
browser, a socket only the daemon's user can reach (below). This holds in both forms: a shared Linux server has other
users, and so can a macOS or Windows machine. So no key the daemon or a holder accepts is on the command line of any
process Octoboard starts, since every user of the machine can list those, the daemon's own included. A key is kept only
where the daemon's user alone can read it:

- **A session's helpers** — its hooks, the MCP bridge and the status line, and in a project session Octoboard's git
  hooks — read it from the file `key` in the session's scratch directory, `~/.octoboard/run/<session id>/key`. The
  file's path is all the helpers' command lines name; the git hooks, which the git the agent runs starts, get it from
  `OCTOBOARD_DAEMON_CREDENTIAL_PATH` in the agent's environment (see "The session's git hooks" in
  `docs/product/branch-responsible-session.md`). A file that cannot be read, or holds no key, is a key the daemon
  refuses: the hooks post nothing, the status line still shows the user's own but reports nothing, and the MCP bridge's
  calls are refused. The git hooks alone then go to the daemon by the records it keeps for `octoboardd address` (its
  owner record and its key record, see `docs/product/files-octoboard-owns.md`), which is how a job a session left
  running after it ended still reports and is gated; they do so too when the session's port refuses the key or nothing
  listens on it, never with the daemon's key to the port the environment names, and with neither route they report
  nothing while the revert gate lets the git through.
- **Claude Code's HTTP hooks** carry it in their address, which is in the settings file Claude Code is given (see "How
  each agent is given the injection" in `docs/product/per-agent-launch-specifics.md`). Claude Code also writes that
  address into the conversation's transcript under `~/.claude/projects`, a file only its user can read, where it
  outlives the session; the key in it is revoked when the session's launch is released, except where holders cannot be
  set up and it is the daemon's own key.

No agent's environment holds a key, a project session's or a console session's: an agent may copy its whole
environment into files of its own, which other users can read (Codex writes every exported variable into its shell
snapshot files), so an environment is not a place a key can be kept. Nor does any variable Octoboard gives a session's
git hooks have `KEY`, `SECRET` or `TOKEN` in its name: an agent that strips such variables from the commands it runs
(Codex's shell environment policy does so only where the user has set `ignore_default_excludes = false`) would
otherwise break the hooks, or git itself.

One secret is in an environment: on a Windows daemon host, the capability token of a Codex session's server is in its
terminal's environment, as `OCTOBOARD_CODEX_REMOTE_TOKEN`, since Codex's terminal reads it from a variable alone (see
"Where the server listens" in `docs/product/per-agent-launch-specifics.md`). It opens only that session's server, never
the daemon. The server, which runs the agent's commands and writes Codex's shell snapshots, is not given it, and Windows
lets no other non-administrator user read a process's environment. This has been checked only with a session whose
turn the API refused, and with shell commands run through Codex's `!`: the token was in no file under `CODEX_HOME` or
`%TEMP%`, not in the server's environment, and not readable by a second non-administrator user. It has not yet been
checked after a real turn that ran commands.

Every file that holds a key or a holder's secret is its user's alone from its first byte (on Windows through the data
directory's access control list); which files those are is in `docs/product/files-octoboard-owns.md`. On Linux and macOS
`~/.octoboard` itself is closed to every other user (see `docs/product/files-octoboard-owns.md`); under a permissive
umask, 0002 say, some of its subdirectories and files are created readable by others all the same, but none of those
holds a key. The MCP bridge's command line does name its session's MCP token, which only says which session a call is
for and is of no use without the key. A session taken over from an earlier build keeps the key where that build put it
until it ends: on its helpers' command lines, or in its agent's environment.

**Keeping a key from other users also means keeping it off a port they could take.** Any user of the machine can listen
on a loopback port the daemon is not holding, and the daemon lets its port go for a moment while a service manager
restarts it, and at some planned restarts (below); whatever a client sends to that port meanwhile reaches whoever took
it. A browser reconnecting through the owner's forward would send the owner key there, so the owner key is never
accepted on the daemon's TCP port: the browser reaches the daemon only through its socket, which no other user but the
superuser can create, replace or connect to, nor forward through an SSH login of their own (see "The browser socket" in
`docs/product/server-form.md`). The exceptions are a browser port of the daemon's own, which takes the owner key where
the owner chose it with Allow browser access: the desktop application's daemon on Windows and macOS, and the server
form's daemon once `octoboardd browser on` opened one, at the cost said under "Remote access to the
desktop application's daemon" below, which is the same on each. The launch key is still sent to the TCP port, by the clients that carry it.
The desktop application's remote window is opened on a loopback port the application itself holds from before the
window opens until it closes, its reconnections included, for the same reason (see "A dropped connection" in
`docs/product/remote-daemons.md`). At a planned
restart in which the daemon starts its next daemon itself, the next daemon keeps the same launch key, so outside Windows
the daemon hands its listening socket to the next daemon and the port is never free between the two (see "The port
between the two daemons" in `docs/product/planned-restart-and-planned-stop.md`). The launch key keeps a narrower
exposure only where the port is free at such a restart all the same — on Windows, where nothing is handed over, and when
the build at the daemon's path is one from before the socket was handed over, which refuses it, so that the daemon
starts it once more binding the port itself: a client that sends the key to the port during that gap could give it to
another user who took the port, and that user could then open the next daemon with it. Where a service manager starts
the next daemon, that one makes a new launch key, and the old one opens nothing.

**A web page open in a browser cannot, unless it is Octoboard's own.** Every request, on every route and every
WebSocket upgrade, passes two checks in turn — except a read of a file of the UI the daemon serves, which passes the
first alone (see "The UI the daemon serves" in `docs/product/server-form.md`):

- **Where it comes from.** When a browser makes a request on behalf of a page, the daemon answers `403` unless that page
  is the application's own window or is served over `http` or `https` from this machine's loopback address, on any
  port. A site that makes its own name resolve to `127.0.0.1` is refused as well. Programs that are not browsers, such
  as scripts, are not subject to this check, and neither is the application's own window.
- **The key.** A request that passed the first check is answered `401` unless it carries, as its `key` query parameter,
  the key the daemon made for this launch — its **launch key** — or, on the browser's way in of a daemon that serves the
  browser, the owner key (see "The owner key" below). This is what keeps out the pages the first check lets through: any
  page on a local dev server, and any other Tauri application, all of which share the application window's origin. The
  desktop application hands the key to its own window, and to the window it reloads onto a daemon started again (see
  "Starting up" in `docs/product/application-lifecycle.md`). The helpers of an agent a process holder runs carry a key
  of their own launch, issued by that holder, instead (see "Session helpers and the holder" in
  `docs/product/process-holders.md`); only where the terminals run in the daemon itself does it hand its own key to the
  agents' helpers, beside the port. A launch key is good only for the daemon that made it and for a next daemon that the
  daemon starts itself at a planned restart, which keeps it (see "Planned restart and planned stop" in
  `docs/product/planned-restart-and-planned-stop.md`): after any other daemon start, a page or program holding an
  earlier launch key is refused until it is given the new one. A page of the UI carries the key in its own address, and
  never sends that address as a referrer, so the key does not travel with a link it opens or a request it makes.

## The owner key

A daemon that serves the browser (see "The browser socket" in `docs/product/server-form.md`) accepts a second key
beside its launch key, on the browser's way in alone: the **owner key**, the key of the address the owner opens in a
browser. Those daemons are the server form's, and the desktop application's while its remote access or its browser access
is on (see "Remote access to the desktop application's daemon" below); the way in is the browser socket, the browser port
where one is open, and for the desktop application's daemon on Windows its owner pipe. The key is kept in `~/.octoboard/owner.key`, which only the
daemon's user can read, and is trusted only as a file of that user's that nobody else could have read or written (see
`docs/product/files-octoboard-owns.md`). Such a daemon reads it as it starts serving the browser, and makes a new one and
writes it there when the file is missing or holds no key it trusts; one that cannot read the file, or cannot write a new
key, logs a warning, accepts no owner key and serves no browser, its launch key alone opening it. A daemon that serves
no browser — the desktop application's while both settings are off, one whose build carries no UI, a server-form daemon
on Windows, one whose way in could not be opened — never reads, makes or accepts an owner key.

The owner key is the browser's alone. On the way in it is accepted on every request, a WebSocket included, as the launch
key is, and so opens everything the launch key opens, except replacing the owner key itself, which needs the launch key,
and changing the desktop application's Allow remote access or Allow browser access, which only the owner can do, in the
application's window or, for browser access, with `octoboardd browser` and the launch key; a connection the owner key
opened can change neither.
The daemon's TCP port never accepts it, refusing it as any wrong key (see "Other users of the same machine" above). It
is in no helper's key file, settings file or transcript, no holder's record, no command line, environment, log or
startup line, and not in `daemon.key`: the launch key keeps every other use it has. `octoboardd address` prints the
address with the owner key wherever the running daemon accepts one (see "Running in the background from the command
line" in `docs/product/daemon-command-line.md`), and `octoboardd address --json` gives it to another device's
application over SSH (see "Another device reaching the daemon over SSH" in the same doc).

**It lasts until the owner rotates it.** The owner key stays the same across every start of a daemon that serves the
browser, a planned restart and a service manager's restart included, so an address bookmarked in a browser keeps
opening the daemon; nothing replaces it but `octoboardd address --rotate` and, for the desktop application's daemon,
Rotate key in its Access section (see `docs/product/remote-access-settings.md`). This is a trade-off made for an
address that keeps working: the key in the browser's address no longer expires with the daemon, and the browser keeps
the address, the key with it, in its history, its bookmarks and its open tabs. A browser that syncs those across
devices carries the key to every other device signed into it, and to the sync provider. The key is still of use only
to whoever can also reach the daemon's way in, which only the daemon's user (and the superuser) can open on its machine
— through an SSH login as that user, say — except for the browser port below.

**Revoking it** takes a rotation alone: the daemon refuses the old key on every new request and connection from then
on, and at once closes every connection the old key opened, every control connection and every terminal connection of
every page that holds it; the connections opened with the launch key stay. The sessions keep running. A page so cut off
finds its key refused as it tries to reconnect and says so (see "The UI the daemon serves" in
`docs/product/server-form.md`); every page then has to be opened again from the new address. A remote window of another
computer's application reads the new key itself and carries on (see "A dropped connection" in
`docs/product/remote-daemons.md`).

## Remote access to the desktop application's daemon

The daemon the macOS or Windows application runs serves no browser and takes no owner key until the owner turns on
one of two settings in its Settings, both in the Access section (see
`docs/product/remote-access-settings.md`) and off to begin with: **Allow remote access**, for another computer reaching this one over SSH, and **Allow browser access**, for a
browser at the daemon's loopback browser port. The daemon keeps both, and applies them at its start and at once whenever
one changes. Either can be on without the other:

- **Allow remote access**, turned on, opens the daemon's socket on macOS and Linux (`~/.octoboard/browser.sock`, under
  the same conditions as the server form's, see "The browser socket" in `docs/product/server-form.md`), and on Windows,
  whose SSH server cannot forward to a socket or a named pipe, its owner pipe, and serves the UI it carries on the socket.
  Another of the owner's computers reaches either through its application and `octoboardd proxy` (see
  `docs/product/remote-daemons.md`), and a browser on another machine through the owner's own SSH forward to the socket
  (Settings does not offer that forward; Windows has none). It opens no TCP port.
- **Allow browser access**, turned on (macOS, Windows), opens the browser port, below, which a browser on the same
  computer opens at `http://127.0.0.1:<port>/?key=<owner key>` with nothing to run first, and which another machine
  reaches through something that carries the loopback port over an encrypted channel: an SSH forward to the port, or a
  tool such as `tailscale serve` (see the documentation site). The Access section gives the SSH forward alone.
  It lets no other computer's application in, and opens no socket or pipe.
- **Either turned off** closes its own way in and the connections made through it, leaving the other serving. The owner
  key is held and accepted while either way in is open, and refused, with every connection it opened closed as a
  rotation does, once both are off. `octoboardd address` then says neither is on (see "Running in the background from the
  command line" in `docs/product/daemon-command-line.md`).
- One of the two failing to open (a socket refused, a port taken) leaves the other serving, and Settings says which.

Only what runs as the owner can turn either on, in the application's own window or, for browser access, with
`octoboardd browser on` in a terminal: they are offered to no agent's settings tool, no configuration document carries
or applies them, and the daemon refuses a change of either, or a rotation of the owner key, on a connection the owner key
opened, so a browser cannot turn its own access on or move it. The command asks the running daemon, with the launch key,
the same request Settings sends (see "Browser access" in `docs/product/daemon-command-line.md`); anything that runs
commands as the owner, an agent included, can run it, which the owner accepts, since an agent running as the owner can
already read the owner key with `octoboardd address`. The command changes browser access alone and never remote access.
The daemon's own TCP port serves no UI and refuses the owner key throughout, on every system. The application's daemon
runs only while the application does, so remote use needs the application kept running on that computer: quitting it ends
every remote connection to its daemon.

**The browser port** is a loopback port of the daemon's own, apart from its TCP port, which serves the UI and takes the
owner key. It is the default browser port, 3369, until the owner changes it in Settings or with `octoboardd browser
on --port`, which takes effect at once: the old port is let go, every connection made through it is closed and the new
one is listened on. A port that cannot be listened on (taken by another program or user) is said in Settings, which asks
for another port. **This is the one place a TCP port accepts the owner key, and it costs this**: any other user of the
computer can take that port whenever the daemon is not holding it — while the application is not running, or while its
daemon is restarted — and a browser that reconnects meanwhile sends them the key. The confirmation that turns browser
access on says so, as does `octoboardd browser on`. The macOS and Windows daemons take this cost on for a browser that
needs no forward, with no further protection. **The server form's daemon takes it on only while browser access is on**,
which it is not until the owner runs `octoboardd browser on`: a server is more often shared by several users, so there the
port is off to begin with, the socket serves the browser alone, and `browser off` closes the port again, with every
connection made through it, leaving the socket as it is (see "Browser access" in `docs/product/server-form.md`). The
same port may never be the daemon's own TCP port, which the daemon assigns itself and which never accepts the owner key.
The application itself never sends the key to this port.

**On Windows**, the owner pipe is the way in Allow remote access opens, as the socket is elsewhere:

- **The owner pipe**, `\\.\pipe\octoboard-owner-<hash of the data directory>`, which another computer's application
  reaches through `octoboardd proxy` run over SSH. The daemon creates it refusing remote clients, owned by and open only
  to its own user (where the name is taken, it joins only a pipe of its own user's), and `octoboardd proxy` and
  `address --json` refuse a pipe of that name owned by anyone else. A change of the browser port, or browser access
  turned on or off, leaves it open; a pipe whose name another program or user holds is not opened.

**Another computer reaches it only while its SSH server is on**: Remote Login on macOS, OpenSSH Server on Windows. The
application says in its Access section whether it is, and how to turn it on (see "The SSH server" in
`docs/product/remote-access-settings.md`); it never turns it on itself.
