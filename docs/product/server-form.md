# The server form

Octoboard comes in two forms. This document describes the **server form**: the daemon alone, on a Linux server
without a display, reached by its owner from a browser on another machine over SSH. What it runs on, and how it is
published, installed and upgraded, is in `docs/product/installing-the-server-form.md`; how it updates itself, in
`docs/product/server-form-updates.md`. The daemon's command line, which also acts on the desktop application's daemon
within the limits each command states, is in `docs/product/daemon-command-line.md`; the planned restart and planned
stop, which the desktop form's restart into an update and a crash of the application also rely on, are in
`docs/product/planned-restart-and-planned-stop.md`. The desktop form, and what the server form changes in starting up,
quitting and crashes, are in `docs/product/application-lifecycle.md`; who can reach the daemon, and how another machine
reaches it over SSH, is in `docs/product/who-can-reach-the-daemon.md`.

## The daemon on its own

In the server form there is no application: the daemon is the whole of Octoboard on the server, and its lifetime is
for whoever runs the server to manage. What follows is what the daemon does when no application started it, whether
started by hand, by `octoboardd start` or by a service manager.

- **Starting and stopping are the administrator's.** Nothing starts the daemon but the person or the service manager
  told to, and nothing stops it but a termination signal — `SIGTERM`; `SIGINT`, which `Ctrl+C` in the terminal it was
  started from sends; or `SIGHUP`, which that terminal sends as it closes. The daemon answers any of them exactly as the
  desktop form's quit does once confirmed (see "Quitting" in `docs/product/application-lifecycle.md`): every session's
  process is ended and every one of those sessions is left *interrupted*, never archived, every shell terminal ends, and
  nobody is asked first; it then exits with status 0. On Windows, which has no signals, the same stop follows `Ctrl+C`
  or `Ctrl+Break` in the console window the daemon was started in, and closing that window; after a close, Windows ends
  the daemon a few seconds later whatever it is doing, so the stop has those seconds; `octoboardd stop` asks for it
  there through the daemon's loopback address (see "Running in the background from the command line" in
  `docs/product/daemon-command-line.md`). A sign-out or a shutdown is not among these: Windows ends the daemon without
  telling it, as "Crashes and forced termination" in `docs/product/application-lifecycle.md` describes. Besides those, a planned restart or a planned stop, asked for on
  purpose, stops it, and is the one stop that leaves the sessions running (see
  `docs/product/planned-restart-and-planned-stop.md`). Started without an application to follow, the daemon runs until
  it is stopped; exiting with the application is a watch the desktop form asks for and the server form never does.
  Nothing in the daemon starts it again when it dies: that is the desktop application's (see "Starting up" in
  `docs/product/application-lifecycle.md`), or a service manager's (see "Running as a systemd user service" below). The
  command line can start it in the background, stop it, restart it, say where it listens and print the address to open
  it at with its key (see "Running in the background from the command line" in `docs/product/daemon-command-line.md`).
- **A browser is not the application.** Closing a tab, or every tab, ends nothing: the daemon, every session and its
  agent process, and every shell terminal run on until the daemon is stopped, and a browser opened later finds them
  where they were. There is no
  quit gesture in a browser, no confirmation, and no background switch (see "Closing the window" in
  `docs/product/application-lifecycle.md`).
- **A stop and a start interrupt every session; a planned restart does not.** A daemon stopped by a termination
  signal ends every session's process, and what "Starting up" in `docs/product/application-lifecycle.md` says of a
  daemon start holds unchanged, whichever browser or device a session was opened from; each resumes with a click, as
  after any quit. A planned restart (`octoboardd restart`) instead leaves every session and shell terminal running in
  its process holder and has the next daemon take them over with their screens as they were (see
  `docs/product/planned-restart-and-planned-stop.md`). A daemon that goes down without stopping — killed outright, say —
  leaves its sessions and shell terminals running in their process holders for the next start to take over too, as
  "Crashes and forced termination" in `docs/product/application-lifecycle.md` describes, their screens rebuilt rather
  than kept.
- **One daemon per data directory, and the data directory is the owner's.** The lock in "Starting up" in
  `docs/product/application-lifecycle.md` is per `~/.octoboard`, so per Unix user on the server; the daemon's consoles,
  projects and sessions are that user's, and its agents are launched under that user's own agent accounts, with that
  user's logins and keys — every client of the daemon acts as that user.

## The UI the daemon serves

**A server build of the daemon carries the UI inside its binary**, so that one file is the whole of Octoboard on the
server, and serves it at its own address, beside the protocol the UI speaks to it. Opening the address
`octoboardd address` prints, through the owner's SSH port forward (see `docs/product/who-can-reach-the-daemon.md`),
loads the full UI, which finds the daemon at the address it was loaded from and takes the key from that address. The
daemon the desktop application bundles carries no UI and serves none: the application's window loads the UI the
application itself bundles.

**Who may load it.** The UI's files are the published application and hold nothing secret, and a page's requests for
its scripts, stylesheets and fonts carry no key, so reading them needs no key: the page itself at `/`, and each other
file of the UI at its exact path, read (`GET` or `HEAD`) only. They still pass the check on where a request comes
from, and a page of another site, or a request naming the daemon by anything but a loopback address, is refused with
`403` (see `docs/product/who-can-reach-the-daemon.md`). Everything else needs the key: every request of the protocol,
any other path and any other method is refused with `401` without it. No other path gives the page, so an address with
any path but `/` does not open the UI; given the key, such a path is answered `404`.

**In the browser.** The page comes from `localhost` through the forward, which a browser treats as a secure context, so
copying to the clipboard works and the browser's notifications are available, a session's raised hand among them once
the browser has allowed them (see `docs/product/notification-settings.md`). Consoles, projects and their sessions
work as in the desktop application, every path, directory and account being the server's: Browse uses the application's
directory browser, which lists the server's directories (see "Browsing directories" in `docs/product/projects.md`).
What only the desktop application provides is absent, as each feature's own doc says.

**How long an address keeps opening the daemon.** The key in the address `octoboardd address` prints is the owner
key wherever the daemon holds one (see "The owner key" in `docs/product/who-can-reach-the-daemon.md`), and it stays the
same across a fresh start, a planned restart and a service manager's restart of the daemon, until the owner rotates it
by hand with `octoboardd address --rotate`; so a bookmarked address keeps opening the daemon, and a page open on it
reconnects after any of them as after any dropped connection. A page holds the key it was opened with: once the owner
key is rotated, a page already open with the old one stays connected until its connection drops, and is refused from
then on. An address that carries a launch key instead — the one `address` prints for a daemon that holds no owner key
— stops opening the daemon once any daemon other than a next one it started itself has taken its place.

**A page whose key the daemon refuses.** Each time the connection of a page the daemon served is lost, or an attempt
to connect fails, the page asks the daemon whether it still accepts the page's key (`GET /key-check`), allowing it
**5 seconds** to answer. Only an answer that the key is refused changes anything: the page then stops reconnecting for
good, with no Retry, and the whole window shows the screen for an address that does not open the daemon, saying that
this address no longer opens the daemon, that its key has changed, and to run `octoboardd address` on the server and
open the address it prints (see "The window chrome and the content panel" in `docs/product/window-layout.md`). Nothing
the page had shown stays. Reloading the page meets the same refusal; the way back is the address `address` prints. Any
other outcome — the key accepted, no answer, a daemon of an earlier build that has no such check — leaves the page
losing its connection as any other does: the connection banner and its Retry (see "Losing the daemon connection" in
`docs/product/application-lifecycle.md`), and its open terminals reconnecting (see "Reconnecting" in
`docs/product/session-terminal.md`). A page opened at the daemon's address with no key in it shows the same screen at
once, saying instead that this address does not open the daemon because it carries no key.

## Choosing the port

`octoboardd --port <port>` runs the daemon in the foreground listening on `127.0.0.1:<port>`, the same port on every
start given that option, so an address can be bookmarked and an SSH port forward
(`ssh -L <port>:127.0.0.1:<port> <server>`) written once. The port is a number from 1 to 65535; anything else, 0
included, is a usage error. Without the option the foreground daemon listens on a port the operating system assigns,
as in the desktop form, whose application never gives one. Either way the startup line and the port file name the port
(see "Starting up" in `docs/product/application-lifecycle.md`).

**The default server port** is **3369** (`defaultServerPort` in `config/app.json`, fixed when the daemon is built).
`octoboardd start` listens on it when given no `--port` (see "Running in the background from the command line" in
`docs/product/daemon-command-line.md`), and the install script's service and the example systemd unit use it unless told
otherwise (see `docs/product/installing-the-server-form.md` and "Running as a systemd user service" below), so the
forward `ssh -L 3369:127.0.0.1:3369 <server>` reaches a daemon started any of those ways.

**A port already in use fails the start.** The daemon exits with a non-zero status and an error saying that port is
already in use, before it touches any session: the sessions of the previous run stay as they were stored. A daemon
already running against the same data directory is refused by the lock before the port is tried (see "Starting up"
in `docs/product/application-lifecycle.md`).

The daemon's own options, `--port`, `--parent-pid` and `--service-managed` (see
`docs/product/planned-restart-and-planned-stop.md`), are not accepted together with a mode (`start`, `stop`, `restart`,
`status`, `address`, `update` or the helper modes the daemon starts for its agents): such a command line is a usage
error.

## Running as a systemd user service

The repository carries an example systemd user unit, `apps/daemon/dist/systemd/octoboardd.service`, for running the
daemon on a Linux server from boot, and every Linux release archive carries it as `octoboardd.service`. Its binary path
(`%h/.local/bin/octoboardd`) and port (3369, the default server port) are examples, and its comments say how to install
and enable it by hand; the install script's `--service` installs it with the path and port filled in (see "Installing
it as a service" in `docs/product/installing-the-server-form.md`). What it sets:

- It runs the daemon **in the foreground** with `--port`, as the user who owns the unit, so that user's `~/.octoboard`,
  agent accounts and keys are the daemon's, and restarts it after a failure but not after a clean stop. It passes
  `--service-managed` and restarts the daemon after exit status 75 too, the status a planned restart then ends with, so
  that the next daemon runs in the unit (see `docs/product/planned-restart-and-planned-stop.md`), and records that
  status as a success, not as a failure of the unit; a copy that drops the restart on 75 must drop the option with it.
- **Standard output is discarded**, so the startup line, which carries the key, never reaches the journal, where
  whoever may read the journal could read it. The error output, which carries the log and never the key, goes to the
  journal, as plain text: the daemon colours its error output only when it is a terminal.
- **Stopping the unit** (`systemctl --user stop`, or `restart`, which stops it first) sends `SIGTERM` to the daemon
  alone, so that it makes an ordinary stop — every session's process ended and the session left *interrupted*, every
  shell terminal ended (see "The daemon on its own" above) — and allows **90 seconds** for it; systemd then kills
  whatever is still in the daemon's unit, which holds the holders and what they run only where they could not be started
  as units of their own (see the next bullets). The kept background processes and the processes left behind in a project
  (see `docs/product/background-processes.md`) are in the cgroup of the holder unit that ran their launch, not in the
  daemon's unit, and outlive both the holder's exit and the daemon's stop, as they do with a daemon started by hand; the
  next daemon takes them in as usual. Only those of a holder that runs in the daemon's unit are killed with it.
  `octoboardd stop` and `octoboardd status` work on a daemon the unit runs as on any other, and the unit does not start
  a daemon stopped that way again; a `stop` that meets a planned restart follows the next daemon the unit starts (see
  `octoboardd stop` in "Running in the background from the command line" in `docs/product/daemon-command-line.md`).
- **The process holders run as systemd user units of their own**, outside the daemon's unit, as the holders of any
  daemon a systemd user manager runs as a service do (see "How a holder is detached" in
  `docs/product/process-holders.md`). Nothing that ends the daemon's unit ends them, so the sessions survive what leaves
  the programs running elsewhere: a planned restart, after which the unit starts the next daemon and it takes every
  session and shell terminal over with its screen as it was, and a daemon that crashes or is killed outright, which
  the unit restarts and whose successor takes them over with their screens rebuilt (see "What a daemon start takes
  over" in `docs/product/process-holders.md`). An ordinary stop still ends every session, as above.
- **At a host shutdown**, or when the user's systemd manager stops, the daemon's unit is stopped before the holders'
  units, so the daemon makes its ordinary stop while the programs still run: every session is left *interrupted* with
  its preview frozen, every shell terminal ends, and the holders, left with nothing to run, exit. The stop of the
  user's manager then ends whatever is left, kept processes included.
- **Where a holder cannot be started as a unit of its own** (see "How a holder is detached" in
  `docs/product/process-holders.md`), it runs in the daemon's unit and ends with it, with the programs it keeps: a
  daemon that crashes or is killed outright then takes those sessions' processes with it, and so does a planned
  restart, since systemd ends whatever is in the unit when the daemon exits, before it starts the next daemon, which
  finds nothing of theirs to take over.
- **`Delegate=yes`** gives the daemon a cgroup subtree it may write. Each agent launch is held in a group of its own
  below the cgroup of the holder that runs it (see "Which processes belong to a launch" in
  `docs/product/background-processes.md`), which for a holder unit is that unit's own cgroup; the daemon's subtree
  serves the holders that run in the daemon's unit.
- A user unit runs only while its user is logged in unless **lingering** is enabled for that user
  (`loginctl enable-linger <user>`); without it the daemon stops at logout and does not start at boot.

**The owner gets the address of a daemon the unit runs from `octoboardd address`**, run on the server as the unit's
user (see "Running in the background from the command line" in `docs/product/daemon-command-line.md`): the startup line,
which carries the launch key, is discarded. The address carries the owner key, which the next daemon the unit starts —
after a planned restart or a failure — accepts as well, so the same address keeps opening the daemon; the new launch key
that next daemon makes is recorded in turn for the commands (see "The UI the daemon serves" above).
