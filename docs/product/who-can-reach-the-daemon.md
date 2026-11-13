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
and nowhere else — beside the socket in its data directory on which the server form serves the browser, which only its
own user can reach (see "The browser socket" in `docs/product/server-form.md`) — and nothing in it lets it be put
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
password.

- **A browser** reaches the server form's daemon through the owner's own SSH forward of the daemon's browser socket to
  a port on their own machine, `ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:<port>:<socket path> <user>@<server>`,
  and opens `http://127.0.0.1:<port>/?key=<owner key>` (see "The browser socket" in `docs/product/server-form.md`), never
  through the daemon's TCP port. That is a loopback address, which the checks below admit once the page carries a key
  the daemon accepts, and which a browser treats as a secure context, so notifications and the clipboard work through
  it with no TLS. The owner gets that address with the key, and the forward with the real socket path and login name,
  by running `octoboardd address` on the server as the daemon's user, however the daemon was started (see "Running in
  the background from the command line" in `docs/product/daemon-command-line.md`). A server build of the daemon serves
  the UI at that address itself (see "The UI the daemon serves" in `docs/product/server-form.md`).
- **The macOS and Windows applications** — Status: not implemented. The application will reach a remote daemon, a
  Linux server's or another of the owner's desktops', through an SSH tunnel it builds itself. Setting a device up is
  helped by a QR code that carries the remote machine's host, SSH port, user and host key fingerprint, and no secret. A
  desktop that is to be reached this way needs its SSH server on — Remote Login on macOS, OpenSSH Server on Windows —
  and the application will guide the owner to turn it on. Today the application only ever runs its own local daemon.

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

**Keeping a key from other users also means keeping it off a port they could take.** Any user of the machine can
listen on a loopback port the daemon is not holding, and the daemon lets its port go for a moment at a planned restart
and while a service manager restarts it; whatever a client sends to that port meanwhile reaches whoever took it. A
browser reconnecting through the owner's forward would send the owner key there, so the owner key is never accepted on
the TCP port: the browser reaches the daemon only through its socket, which no other user but the superuser can
create, replace or connect to, nor forward through an SSH login of their own (see "The browser socket" in
`docs/product/server-form.md`). The launch key is still sent to the TCP port, by the clients that carry it, and keeps a
narrower exposure there: at a planned restart in which the daemon starts its next daemon itself, the next daemon keeps
the same launch key, so a client that sends it to the port during that gap could give it to another user who took the
port, and that user could then open the next daemon with it. Where a service manager starts the next daemon, that one
makes a new launch key, and the old one opens nothing.

**A web page open in a browser cannot, unless it is Octoboard's own.** Every request, on every route and every
WebSocket upgrade, passes two checks in turn — except a read of a file of the UI the daemon serves, which passes the
first alone (see "The UI the daemon serves" in `docs/product/server-form.md`):

- **Where it comes from.** When a browser makes a request on behalf of a page, the daemon answers `403` unless that page
  is the application's own window or is served over `http` or `https` from this machine's loopback address, on any
  port. A site that makes its own name resolve to `127.0.0.1` is refused as well. Programs that are not browsers, such
  as scripts, are not subject to this check, and neither is the application's own window.
- **The key.** A request that passed the first check is answered `401` unless it carries, as its `key` query parameter,
  the key the daemon made for this launch — its **launch key** — or, on the browser socket of a daemon that serves the
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

A daemon that serves the browser on its socket (see "The browser socket" in `docs/product/server-form.md`) accepts a
second key beside its launch key, on that socket alone: the **owner key**, the key of the address the owner opens in a
browser. It is kept in `~/.octoboard/owner.key`, which only the daemon's user can read, and is trusted only as a file
of that user's that nobody else could have read or written (see `docs/product/files-octoboard-owns.md`). Such a daemon
reads it as it starts, and makes a new one and writes it there when the file is missing or holds no key it trusts; one
that cannot read the file, or cannot write a new key, logs a warning, accepts no owner key and serves no browser, its
launch key alone opening it. A daemon that serves no browser — the one the desktop application starts, one whose build
carries no UI, one on Windows, one whose socket could not be opened — never reads, makes or accepts an owner key.

The owner key is the browser's alone. On the browser socket it is accepted on every request, a WebSocket included, as
the launch key is, and so opens everything the launch key opens, except replacing the owner key itself, which needs the
launch key. The TCP port never accepts it, refusing it as any wrong key (see "Other users of the same machine" above).
It is in no helper's key file, settings file or transcript, no holder's record, no command line, environment, log or
startup line, and not in `daemon.key`: the launch key keeps every other use it has. `octoboardd address` prints the
address with the owner key wherever the running daemon accepts one (see "Running in the background from the command
line" in `docs/product/daemon-command-line.md`).

**It lasts until the owner rotates it.** The owner key stays the same across every start of a daemon that serves the
browser, a planned restart and a service manager's restart included, so an address bookmarked in a browser keeps
opening the daemon; nothing replaces it but `octoboardd address --rotate`. This is a trade-off the server form makes
for an address that keeps working: the key in the browser's address no longer expires with the daemon, and the browser
keeps the address, the key with it, in its history, its bookmarks and its open tabs. A browser that syncs those across
devices carries the key to every other device signed into it, and to the sync provider. The key is still of use only
to whoever can also reach the daemon's browser socket, which only the daemon's user (and the superuser) can open on the
server — through an SSH login as that user, say.

**Revoking it** takes `octoboardd address --rotate` alone: the daemon refuses the old key on every new request and
connection from then on, and at once closes every connection the old key opened, every control connection and every
terminal connection of every page that holds it; the connections opened with the launch key stay. The sessions keep
running. A page so cut off finds its key refused as it tries to reconnect and says so (see "The UI the daemon serves" in
`docs/product/server-form.md`); every page then has to be opened again from the new address.
