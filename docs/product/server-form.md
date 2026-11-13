# The server form

Octoboard comes in two forms. This document describes the **server form**: the daemon alone, on a Linux server
without a display, reached by its owner from a browser on another machine over SSH. What it runs on, and how it is
published, installed and upgraded, is in `docs/product/installing-the-server-form.md`; how it updates itself, in
`docs/product/server-form-updates.md`. The command line described here also
acts on the desktop application's daemon, within the limits each command states, and the planned stop is also what the
desktop form's restart into an update and a crash of the application rely on. The desktop form, and what the server form
changes in starting up, quitting and crashes, are in `docs/product/application-lifecycle.md`; who can reach the daemon,
and how another machine reaches it over SSH, is in `docs/product/who-can-reach-the-daemon.md`.

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
  there through the daemon's loopback address (see "Running in the background from the command line" below). A sign-out
  or a shutdown is not among these: Windows ends the daemon without telling it, as "Crashes and forced termination" in
  `docs/product/application-lifecycle.md` describes. Besides those, a planned restart or a planned stop, asked for on
  purpose, stops it, and is the one stop that leaves the sessions running (see "Planned restart and planned stop"
  below). Started without an application to follow, the daemon runs until it is stopped; exiting with the application is
  a watch the desktop form asks for and the server form never does. Nothing in the daemon starts it again when it dies:
  that is the desktop application's (see "Starting up" in `docs/product/application-lifecycle.md`), or a service
  manager's (see "Running as a systemd user service" below). The command line can start it in the background, stop it,
  restart it, say where it listens and print the address to open it at with its key (see "Running in the background from
  the command line" below).
- **A browser is not the application.** Closing a tab, or every tab, ends nothing: the daemon, every session and its
  agent process, and every shell terminal run on until the daemon is stopped, and a browser opened later finds them
  where they were. There is no
  quit gesture in a browser, no confirmation, and no background switch (see "Closing the window" in
  `docs/product/application-lifecycle.md`).
- **A stop and a start interrupt every session; a planned restart does not.** A daemon stopped by a termination
  signal ends every session's process, and what "Starting up" in `docs/product/application-lifecycle.md` says of a
  daemon start holds unchanged, whichever browser or device a session was opened from; each resumes with a click, as
  after any quit. A planned restart (`octoboardd restart`) instead leaves every session and shell terminal running in
  its process holder and has the next daemon take them over with their screens as they were (see "Planned restart and
  planned stop" below). A daemon that goes down without stopping — killed outright, say — leaves its sessions and shell
  terminals running in their process holders for the next start to take over too, as "Crashes and forced termination" in
  `docs/product/application-lifecycle.md` describes, their screens rebuilt rather than kept.
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
`octoboardd start` listens on it when given no `--port` (see "Running in the background from the command line"
below), and the install script's service and the example systemd unit use it unless told otherwise (see
`docs/product/installing-the-server-form.md` and "Running as a systemd user service" below), so the forward
`ssh -L 3369:127.0.0.1:3369 <server>` reaches a daemon started any of those ways.

**A port already in use fails the start.** The daemon exits with a non-zero status and an error saying that port is
already in use, before it touches any session: the sessions of the previous run stay as they were stored. A daemon
already running against the same data directory is refused by the lock before the port is tried (see "Starting up"
in `docs/product/application-lifecycle.md`).

The daemon's own options, `--port`, `--parent-pid` and `--service-managed` (see "Planned restart and planned stop"
below), are not accepted together with a mode (`start`, `stop`, `restart`, `status`, `address`, `update`
or the helper modes the daemon starts for its agents): such a command line is a usage error.

## Running in the background from the command line

Five commands act on the daemon running against the data directory of the user who runs them (`~/.octoboard`), whoever
started that daemon, the desktop application included. Whether a daemon runs is told by `daemon.lock` alone; which
process it is and where it listens, by `daemon.owner`, and its key by `daemon.key` (see
`docs/product/files-octoboard-owns.md`). Checking takes the lock for an instant, as a shared lock, so that commands
checking at the same moment do not take each other for a daemon, and a daemon starting at that very instant waits up to
a second for it rather than refusing to start. All five work on Windows too; where Windows differs, the command says so
below.

**A planned restart caught half-way is waited out.** The daemon of a planned restart removes `daemon.owner` before it
releases the lock, and the next daemon records itself only once it holds the lock, so for a moment the lock is held with
no record, and for another it is free. Before it removes the record, the restarting daemon writes `daemon.handover`,
which the next daemon removes once it has recorded itself. So before judging, every command waits while the lock is free
and `daemon.handover` names a daemon whose next one may still take the lock: as long as that daemon's process still
runs, since one that starts its next daemon itself waits for that one to report listening, and, for one whose next
daemon a service manager starts, for **3 seconds** after it ended, counted from the later of when the command saw it end
and when the file was written. `start`, `stop` and `restart` also wait while a daemon holds the lock without a usable
`daemon.owner` naming it; `status` and `address` do so only once they have seen a handover pending, and only while its
`daemon.handover` is still there, for the next daemon to record itself: the file is removed after the record is written,
so a daemon that holds the lock with no record and no file will not record, and they never wait on one that cannot
record itself. The wait lasts up to **90 seconds**. The daemon that then holds the lock and has recorded itself is the
one the command acts on, with one exception for `stop`: where the wait went through the handover of a daemon whose
`daemon.owner` (as `daemon.handover` repeats it) said a service manager starts its next daemon, a daemon not recorded as
service-managed is not that next daemon, and neither is one that follows the desktop application, since the restarting
daemon starts its next one without it — one `start`, a user or the desktop application started in the free lock — and
neither is any daemon that holds the lock before the handing-over daemon has ended, which a service manager waits for;
`stop` waits for that daemon to end, within the 90 seconds, so that its pid is the one it reports stopped, and then
treats the daemon it found as not the next one whatever its record says; if that daemon never ends within the wait,
`stop` says it is still stopping, adds that another daemon holds the lock and is left running, with its pid and address,
and exits with status 1. `stop` leaves the daemon that is not the next one running, saying, as below, that the daemon
stopped but another one has started since, and exits with status 1. After the handover of a daemon that starts its next
daemon itself, whatever daemon holds the lock next, but one that follows the desktop application, is taken for that next
daemon: the key that would show it is removed with the restarting daemon's records, and that daemon starts its next one
as soon as it releases the lock, so that only a start racing it could come first. A daemon that starts its next daemon
itself and could not tell its own start time writes neither record nor `daemon.handover`, so the free lock of its
restart cannot be told apart from no daemon at all.

- **`octoboardd start [--port <port>]`** starts the daemon in the background on that port, with the same range as above,
  or, without `--port`, on the default server port (see "Choosing the port" above). While a daemon already runs it refuses, saying so with that daemon's pid and address where they
  are known — a daemon still starting or restarting is named once it has recorded itself, and one still not recorded
  after the wait above is said to run at an address not known — and exits with status 1. Otherwise the daemon is started
  with `--port <port>` in a session of its own, with no controlling terminal and the data directory as its working
  directory, so closing the terminal or the login that ran `start` sends it nothing and it runs until it is stopped. On
  Windows it is started instead with a hidden console of its own, in a process group of its own and outside the job
  `start` runs in, so closing the console window that ran `start` does not end it. The daemon keeps none of the standard
  handles `start` was given, and on every host inherits nothing of `start`'s beyond its three standard handles or
  descriptors, the pipes it reports over, so a `start` whose output is captured or piped (a command substitution, a
  PowerShell assignment, `| more`, an SSH session) returns once it has printed, not when the daemon ends. Where that job
  does not let a process leave it, as an SSH session's does not, the daemon is started inside the job and ends when the
  job does — for an SSH session, at logout; `start` then succeeds as below and adds, on its error output after its usual
  lines, a note saying so. `start` waits up to **30 seconds** for the daemon's startup line and prints it unchanged —
  **the key included**, so whoever sees that output, or a log of it, holds the key — followed by a line with the
  daemon's pid and its address, `http://127.0.0.1:<port>`, and exits with status 0. A daemon that ends before reporting
  (its port taken, say, or another daemon took the lock first) makes `start` say that it ended while starting, with its
  exit status, followed by what it wrote on its error output (the first 16 KiB), and exit with status 1; one that has
  reported nothing after 30 seconds is killed, with the same report. Once `start` has returned, what the daemon writes
  on its standard output and error output goes nowhere; its log is still `~/.octoboard/logs/daemon.log`. On a host whose
  logind is configured with `KillUserProcesses=yes`, the system ends every process of a login when the login closes, a
  daemon `start` started included; run it as a service there (see "Running as a systemd user service" below).
- **`octoboardd stop`** with no daemon running says so and exits with status 0. When it found a daemon holding the lock
  without a record, or a planned restart half-way, and that came to no daemon running by the end of the wait above, it
  says that the daemon was already stopping, as below, and exits with status 0. Otherwise it sends `SIGTERM` to the
  process `daemon.owner` names — that process, checked by its start time, never another that has taken its pid since —
  which stops as a termination signal has it stop (see "The daemon on its own" above): every session interrupted, every
  shell terminal ended. On Windows, which has no signal for it, `stop` instead sends the shutdown request (`POST
  /shutdown`) to the port `daemon.owner` records, presenting the key `daemon.key` holds for that daemon; the key goes
  only into that loopback request, never into a command line or an environment variable, and the daemon stops the same
  way. A daemon whose key `daemon.key` does not hold — one of an earlier build, or one that could not write the record —
  is not stopped there: `stop` says so, with its pid and port, and exits with status 1. On every host `stop` reads that
  key before it asks, to recognise the daemon's successor below. Once a daemon has taken the request (the signal reached
  it, or it answered the request), `stop` waits up to **60 seconds** for it to be gone — the lock free and its process
  ended — and exits with status 0, but within **90 seconds** of the command's start, the wait above for a daemon to be
  found included; on Windows the daemons it goes on to follow share the first 60 seconds, counted from the first request
  a daemon took, while on Unix each daemon it follows gets 60 seconds of its own, counted from its own request, within
  the 90 seconds overall. A daemon still running when the wait ends makes it say so, with the pid, and exit with status
  1, while the stop goes on. A daemon that is already gone when signalled, or no longer accepts the connection (refuses
  it, or resets it unanswered), has ended, or is ending, or is in a planned stop or restart (see "An ordinary stop
  during a planned stop" below): `stop` then watches `daemon.lock` and `daemon.owner` for up to **90 seconds** from its
  start, the wait above for a daemon to be found included, so that a stop that waited then takes the request has what is
  left of them. Either way, once another daemon holds the lock and shows itself the next daemon of a planned restart,
  `stop` asks that one instead, the same way, and follows it in turn; one that does not take the request is waited for
  within the same 90 seconds. A daemon that starts the next daemon itself hands it its key, so that one shows itself by
  having recorded the very key `stop` read for the daemon it asked (a daemon that has already settled on starting it
  ignores the signal). The next daemon a service manager starts makes a new key instead, so where `daemon.owner`
  recorded the daemon asked as one whose next daemon a service manager starts, a daemon recorded as service-managed
  shows itself the next daemon once the one asked has ended. For such a daemon `stop` goes on watching for **3 seconds**
  once the lock is free and the daemon has ended, the time for the service manager to start the next one (systemd starts
  it 100 milliseconds after the exit unless the unit says otherwise), so stopping it is reported that much later; a next
  daemon started later than that is neither stopped nor reported. One `stop` follows at most one next daemon a service
  manager starts: a second is reported and left running as below, since a unit that restarts the daemon after every exit
  starts one after each stop too (with `Restart=always`, `stop` therefore follows one plain systemd restart and stops
  it, then reports the next one as started since; the shipped unit uses `Restart=on-failure` and is not affected). Where
  the key could not be read but `daemon.handover` names a daemon asked that starts its next daemon itself as handing
  over, its key went with its records, and `stop` follows the daemon that holds the lock next as after a handover it
  waited through (see above), but not one that follows the desktop application. Any other daemon that holds the lock
  once the daemon asked has ended is not stopped: one the desktop application or a user started since, or any daemon
  when neither the key nor a service-manager succession can be shown — the key could not be read, and the record does
  not say whether a service manager starts its next daemon, as a record an earlier build wrote does not. `stop` then
  says that the daemon stopped but another one has started since and is left running, with its pid and address, and
  exits with status 1. A daemon not followed is reported only if it already holds the lock when `stop` looks. On Windows
  the request sent to a daemon in a planned restart can reach its next daemon instead, which listens on the same port
  with the same key once the first has let go of them; that one then no longer accepts the request `stop` sends it on
  following it, and `stop` still reports it stopped. Once the lock is free and the process the record named has ended
  with no daemon recorded after it, where no daemon this `stop` asked took the request (on Unix a delivered signal
  counts only for the daemon it reached, not for one followed after it), nothing is left to stop: `stop` says that the
  daemon was already stopping, and that if that was a planned stop, it left its sessions and shell terminals running in
  their holders for the next daemon to take over, and exits with status 0. A daemon that has come to none of these by
  then makes `stop` say that it is still stopping after the seconds it waited in all, with the pid of the last daemon it
  followed, and exit with status 1. Any other failure of the request is reported as an error and nothing is waited for.
  A running daemon that no usable `daemon.owner` names by the end of the wait above — it is still starting or stopping,
  or could not write the record — is not stopped: `stop` says so, with the seconds it waited, and exits with status 1. A
  desktop application whose daemon is stopped this way does not start it again and says the daemon exited, as for any
  daemon that ends with status 0 (see "Starting up" in `docs/product/application-lifecycle.md`).
- **`octoboardd restart`** asks the daemon for a planned restart (see "Planned restart and planned stop" below) and
  takes no option. With no daemon running by the end of the wait above it says so and exits with status 1. It also exits
  with status 1, restarting nothing, for a running daemon that no usable `daemon.owner` names by then (as `stop` does);
  for one that follows the desktop application, saying that the application restarts it itself; for one that could not
  install its handler for the request, whose record does not say whether it follows an application and which the request
  would end rather than restart, saying to stop and start it instead. Otherwise it sends the request to the process
  `daemon.owner` names, checked by its start time as `stop` checks it, as the signal `SIGUSR1`, so that the command line
  never holds the daemon's key; a daemon that follows an application refuses the signal too, and logs that it did. On
  Windows, which has no signal for it, it sends the planned-restart request (`POST /restart`) over loopback instead,
  with the key from `daemon.key`, as `stop` does: a daemon whose key is not recorded is not restarted (status 1, saying
  so as `stop` does), and a daemon that refuses the request makes `restart` say that it refused the restart, with the
  daemon's own reason, and exit with status 1. A daemon that no longer accepts the connection (refuses it, or resets it
  unanswered), or is gone when signalled, is not reported stopped at once: one in a planned stop of its own resets the
  request while it ends, and a service manager may be restarting one that has ended, so `restart` goes on waiting for
  the next daemon as below, and judges the daemon as below once it has ended with the lock free. On every host `restart`
  reads that key before it asks, to recognise the next daemon. It then waits for the next daemon, up to **90 seconds**
  from the start of the command, the wait above included, to hold `daemon.lock` and record itself in `daemon.owner`,
  prints a line with the new daemon's pid and its address, `http://127.0.0.1:<port>`, and exits with status 0. The next
  daemon is recognised as `stop` recognises the one it follows: where `daemon.owner` recorded the daemon as one that
  starts the next daemon itself, only a daemon that recorded its key, even while the restarted daemon still waits for it
  to report listening; where it recorded a service manager starting it, a daemon recorded as service-managed, once the
  restarted daemon has ended. Where the record does not say, any daemon that holds the lock and has recorded itself is
  taken for the next one, as is any such daemon where the record says the restarted daemon starts the next one itself
  and the key proves nothing because it could not be read, for the restarted daemon or for the next one. Any other
  daemon that holds the lock once the restarted daemon has ended makes `restart` say that the daemon stopped to restart
  but another one has started since and is left running, with its pid and address, that nothing shows the restarted
  daemon handed over to it, and exit with status 1. It prints no key: the next daemon keeps the launch key where the
  daemon starts it itself, and a successor under systemd makes a new one (see below), while the owner key stays the same
  either way, so an address `octoboardd address` printed with it opens the new daemon as it did the old (see "The UI
  the daemon serves" above). Once the restarted daemon has ended, the lock is free and the `daemon.handover` it writes as it hands
  over is not pending, no daemon is on its way: a stop asked for meanwhile cut the restart short, or the next daemon did
  not start, or it has been stopped again since, or the daemon exited for a service manager and none started the next
  one within the 3 seconds above. `restart` then says that the daemon stopped to restart, with its pid, and that no
  daemon runs in its place, for one of those reasons, which the log tells apart, and exits with status 1. It says so at
  once, except for a daemon recorded as service-managed that ended without a note of a handover, which it waits 3
  seconds for before judging, so that the service manager has its time to start the next one. When no daemon has taken
  over within the 90 seconds, it says that the daemon is still stopping, with its pid, or that it stopped but no daemon
  took over, that the daemon's log says why and that the sessions are kept for the next start, and exits with status 1.
  A daemon that was gone before the request reached it, because it had crashed or been stopped, is judged without
  waiting for the 90 seconds, and without a `daemon.handover` to wait for: once the lock is free and the daemon has
  ended, or after 3 seconds for a service-managed one, `restart` says that it was no longer accepting the restart and
  has stopped, with no daemon in its place, and exits with status 1. A daemon that took over
  from such a daemon is judged as for any other: the successor (a daemon that a service manager started after a
  service-managed one, or any daemon that holds the lock where the record proves nothing) is reported as restarted,
  with its pid and address, and exit status 0; any other daemon that holds the lock is reported as another daemon that
  has started since, left running, and exit status 1.
- **`octoboardd status`** says that the daemon is running, with its pid and its address, `http://127.0.0.1:<port>`, or,
  when no usable `daemon.owner` names it, that it is running at an address not known, and exits with status 0; with none
  running it says so and exits with status **3**. With the lock free in the middle of a planned restart it waits as
  above and reports the next daemon rather than none. A user for whom no daemon has ever run has no data directory, and
  `status` creates none.
- **`octoboardd address`** prints the address to open the running daemon at in a browser, its key included, as a single
  line on standard output and nothing else there, `http://localhost:<port>/?key=<key>`, and exits with status 0; the
  output is meant to be read by a program as well, such as one run over SSH. The host is `localhost`, which a browser
  treats as a secure context and which the owner's SSH port forward opens on their own machine (see
  `docs/product/who-can-reach-the-daemon.md`). The key is the owner key where the running daemon's `daemon.owner` says
  it accepts one and `owner.key` holds a key; otherwise it is the daemon's launch key, as for a daemon that follows the
  desktop application or is of an earlier build (see "The owner key" in `docs/product/who-can-reach-the-daemon.md`).
  Where the record says the daemon accepts an owner key but `owner.key` is missing, cannot be read or holds no key, it
  prints the launch key's address and adds on its error output that this address lasts only until the daemon restarts,
  and that `address --rotate` makes a lasting one. Its one option is `--rotate` (below). With no daemon running it says
  so on its error output and exits with status **3**, creating no data directory, and waits through a planned restart,
  as `status` does. A running daemon that no usable `daemon.owner` names — it is still starting or already stopping, or
  could not write the record — makes it say that the address is not known, and exit with status 1. So does a daemon
  whose launch key `daemon.key` does not hold, whether or not it accepts an owner key, naming its pid and port: one of
  an earlier build, which records no key until a daemon of this build takes its place, or one that could not write the
  record, as its log says. Both keys are read from files only the daemon's user can read, so another user of the
  machine cannot obtain them this way: their own `address` looks at their own data directory, where no daemon of that
  owner's runs.
- **`octoboardd address --rotate`** replaces the owner key, so that every address carrying the old one stops opening
  the daemon, and prints the address with the new one where the running daemon accepts it. It finds the daemon as
  `address` does, waiting through a planned restart, and like it exits with status 1, rotating nothing, for a running
  daemon that no usable `daemon.owner` names or whose launch key `daemon.key` does not hold. Otherwise:
  - **A running daemon that accepts the owner key** is asked to replace it, by a request over loopback (`POST
    /owner-key/rotate`) carrying the launch key from `daemon.key`, which goes only into that request, as `stop` sends it
    on Windows. The daemon writes the new key to `owner.key` and refuses the old one from its next request on; `address
    --rotate` prints the address with the new key and exits with status 0. A page already open with the old key stays
    connected until its connection drops (see "The UI the daemon serves" above); to cut such pages off at once, run
    `octoboardd restart` after rotating, which drops every page's connection while the sessions keep running. A daemon
    that refuses the request, fails to write the new key (the old one then stands, on disk and in the daemon) or no
    longer accepts the connection makes it say on its error output that the owner key was not rotated, and why, and
    exit with status 1, with nothing on standard output.
  - **A running daemon that does not accept the owner key** — it follows the desktop application, is of an earlier
    build, or could not read or write `owner.key` as it started — is left as it is: `owner.key` is replaced directly,
    for the next daemon started without the desktop application, and the address printed is the running daemon's
    launch key's, with a note on the error output saying so; status 0.
  - **With no daemon running**, `owner.key` is replaced where the data directory exists, for the next daemon started
    without the desktop application, and the error output says so; where there is no data directory, nothing is
    created and it says only that no daemon runs. Either way it exits with status **3**, with nothing on standard
    output. A daemon that starts while the file is being replaced is waited for until it has recorded itself, and the
    key is then rotated through it as above.

`octoboardd update` acts on the installed executable rather than on the running daemon: it checks for, downloads and
puts in place a newer release, or puts the previous one back, and leaves the restart to `octoboardd restart` (see "The
`octoboardd update` command" in `docs/product/server-form-updates.md`).

## Planned restart and planned stop

A **planned restart** stops the daemon without ending anything: every session and shell terminal is handed over to the
process holder it runs in, with its screen saved, and the next daemon comes up on the same port and takes them over
with their screens as they were, exactly wherever the saved screen can reproduce them (see "The screen of a terminal
handed over" in `docs/product/process-holders.md`). A **planned stop** is the same handover with no next daemon: the
daemon exits with status 0, and the next daemon started — by hand, by `octoboardd start` or by the desktop application —
takes everything over as exactly.

**Asking for one.** A planned restart is asked for by `octoboardd restart` (see "Running in the background from the
command line" above), by the signal `SIGUSR1` it sends, or by the control route `POST /restart`, which answers `202` at
once and which `octoboardd restart` sends on Windows. A page served by a daemon that updates itself asks for one with
**Restart now** once an update is installed, which, unlike the others, is refused while a download of the update runs or
while any stop or restart is already asked for (see "Restarting into the update" in
`docs/product/server-form-updates.md`). A planned stop is asked for by the control route
`POST /planned-stop` or the WebSocket request `planned_stop`; both answer at once and neither is ever refused. The
desktop application asks for one to restart into an update (see "Restarting" in
`docs/product/application-lifecycle.md`), and a daemon that follows the application makes one itself when it finds the
application gone with no quit note (see "Crashes and forced termination" in `docs/product/application-lifecycle.md`). A
planned restart is refused, and nothing is stopped, for a daemon that follows the desktop application, which starts the
daemon it talks to itself (`POST /restart` answers `409` with the reason `follows_application`). A planned stop or
restart asked for while an ordinary stop (the shutdown request, a termination signal) is already asked for does not
replace it, and a planned stop asked for after the desktop application's quit note is made ordinary (see "The quit note"
in `docs/product/application-lifecycle.md`); an ordinary stop asked for before a planned one has begun replaces it, and
one asked for once it has begun cuts it short (see "An ordinary stop during a planned stop" below).

**Who starts the next daemon.** A daemon no service manager restarts starts the next one itself once it has handed
everything over and let go of `daemon.lock`: in the background, as `octoboardd start` does, on the same port and with
the same launch key, so a page or program that held the old address and key reaches the new daemon unchanged. The key goes to
the next daemon on its standard input, never on its command line or in its environment, where other users of the
machine could find it (see `docs/product/who-can-reach-the-daemon.md`). On Windows, where
the job the daemon runs in does not let a process leave it, the next daemon is started inside that job, as
`octoboardd start` starts it, and ends when the job does; the daemon logs a warning saying so. A window open on it loses
its connection meanwhile and reconnects as after any dropped connection (see "Losing the daemon connection" in
`docs/product/application-lifecycle.md`). The next daemon's startup line, which carries the key, is read by the stopping
daemon and neither shown nor logged. The next daemon is started from the path the daemon itself was started by — the
command's own path, made absolute (looked up in `PATH` for a bare name), its last symbolic link not followed — when that
path led to the very file the daemon was running as it started, and otherwise from the file it runs; `octoboardd start`
starts its daemon from such a path too. So a symbolic link pointed at a newer build since, or a newer file put in place
at the same path, restarts into that build, and a path that no longer leads to an executable fails the restart. A next
daemon that has not reported listening within **30 seconds** is killed; whenever the next daemon does not come up, the
old one logs why and exits with status 1, leaving the sessions and shell terminals running in their holders for the next
start.

A daemon leaves the next start to a service manager only on evidence, decided once as it starts: the option
`--service-managed`, which the example systemd unit passes, or systemd naming the daemon's own process as the one it
started for a unit (`SYSTEMD_EXEC_PID` equal to the daemon's pid, with an `INVOCATION_ID`). A daemon started by hand
from a terminal that itself runs under systemd has neither. Such a daemon starts no next daemon: it exits with status
**75** after its planned stop, and the service is to start the next one (see "Running as a systemd user service"
below), which makes a new launch key. A service that passes `--service-managed` must restart the daemon on status 75. On
Windows `--service-managed` is accepted and ignored.

**What a planned stop keeps.** No session is marked *interrupted* and no session record changes; no shell terminal is
closed, and each keeps its record and the name the user gave it; nothing a session started is stopped, and no terminal
preview is saved (see `docs/product/files-octoboard-owns.md`); the sessions claimed for a holder that cannot be
connected to stay claimed. The account quota runs and the logins under way are stopped, and the hook events and program
status reports already received are applied, as an ordinary stop does. The messages queued for a running session and not
yet written into it are carried over to the next daemon (see "Messages held until a session can take them" in
`docs/product/message-delivery.md`). A holder that does not answer the handover within **5 seconds**, or one of a build
that takes no handover, has the terminals it had not marked by then left running as after a crash (those marked before
keep their saved screens); a link being opened again is waited for first, up to about **15 seconds**. The next daemon
takes them over with their screens rebuilt (see "The screen of a terminal taken over" in
`docs/product/process-holders.md`). Where holders cannot be set up and the terminals run in the daemon itself, nothing
can be handed over: a planned stop then ends every session as an ordinary stop does, and a planned restart still starts
the next daemon.

### While the terminals are handed over

From the moment a planned stop begins, the connections already open are served until the daemon exits, but nothing new
starts, as once any shutdown has begun (see "Quitting" in `docs/product/application-lifecycle.md`), and nothing that
would change or end a terminal, or remove what one works in, is begun either, whichever client, agent tool or report
asks for it. Each of these is refused with `daemon_shutting_down`, with nothing changed:

- archiving or deleting sessions, an owner's archive tool included, and a `done` report that would archive its session,
  which is refused whole, so nothing in it is delivered;
- removing a project or a console;
- restarting a session for a change of its account, its model or its account's sharing of the default configuration,
  and handing a session off;
- stopping processes on request, and interrupting a turn;
- answering a session's folder-trust confirmation;
- starting or closing a shell terminal;
- a message for a session, once the messages still queued have been saved for the next daemon (see "Messages held until
  a session can take them" in `docs/product/message-delivery.md`).

Requests that only read are answered as ever, and the background clean-up of archived sessions does not run. An
operation already under way when the planned stop begins is waited for, up to **5 seconds**, with the processes it ends
and the branch removals it starts, before the terminals are handed over. One still running when that wait runs out goes
on without being waited for, but no later step of it that would start or end a terminal or a process, remove a branch
or move a folder to the Trash is taken: that step is refused with `daemon_shutting_down`, and what it would have ended
is left to the next daemon (see "Moving the project folder to the Trash" in `docs/product/removing-projects.md`).

On every terminal, from just before it is handed over, the clients no longer change its size, nor what the program is
told of their display or focus; the next daemon takes the clients' size when they reconnect. Until the
daemon parts from the holders, the answers to the queries in the output it read before the handover, its own and those
it left to the clients, still go into the program. Typing into a terminal once it is handed over is dropped: every
client is sent the error `daemon_shutting_down`, with no request id and no params, once for each terminal connection the
typing came over, in place of `terminal_input_lost` (see "Reconnecting" in `docs/product/session-terminal.md`), and the
window shows it as an error alert. A paste the handover cut part-way is lost typing, reported as `terminal_input_lost`.

The daemon removes `octoboardd.port`, `daemon.owner` and `daemon.key` as it exits, as on any stop.

### An ordinary stop during a planned stop

Anything that asks for an ordinary stop while a planned stop is asked for or under way makes the stop ordinary: before
the planned stop has begun it takes its place, and once it has begun it cuts it short. What still reaches a daemon in a
planned stop: a termination signal (`octoboardd stop` sends one outside Windows), or on Windows what stands for one
(see "The daemon on its own" above); the desktop application's quit note (see "The quit note" in
`docs/product/application-lifecycle.md`); and the shutdown request on a connection already open. No new connection is
accepted by then, so a `POST /shutdown` sent on one does not arrive.

The planned stop stops where it stands: in its wait for the operations already under way (which is cut short, and what
is refused meanwhile is still refused), before any terminal is handed over (none is), or after (no further screen is
saved, and the screens saved by then are still written for the next start). The holders' marking of the terminals and a
parting already begun are finished first. Then the daemon stops as an ordinary stop does, leaving nothing for a next
daemon: every session's preview is kept first — the screen as it is now for a terminal still the daemon's, the screen up
to the handover for one handed over, since its holder is ended with what it runs and the output since the handover goes
with it — then every session is left *interrupted*, every shell terminal ends, and the holders that had terminals
handed over, that the daemon parted from or that it left to the next daemon are ended with everything they run. No next daemon is started, a
planned restart's included, and the daemon exits with status 0.

`octoboardd stop` on Windows, which sends the shutdown request, does not cut a planned stop short: it waits for the
handover to end (see `octoboardd stop` in "Running in the background from the command line" above). When the next
daemon of a planned restart, which the daemon started itself with its own key, has taken over by then, `stop` stops that
one, which ends every session and shell terminal as an ordinary stop does; a daemon that took over without that key is
left running, and `stop` says so; when the daemon has ended with none taking over, as after a planned stop, `stop` says
the daemon stopped and exits with status 0; if that was a planned stop, where holders run the sessions and shell
terminals, they are kept for the next start (an ordinary stop already under way has ended them). Outside Windows, the
termination signal `octoboardd stop` sends reaches the daemon in its planned stop and makes the stop ordinary, as above,
unless the daemon has already settled on its successor: it then starts that one all the same, or exits for the service
manager to start it, and `stop` follows and stops it as on Windows, a daemon a service manager started included (see
`octoboardd stop` in "Running in the background from the command line" above).

## Running as a systemd user service

The repository carries an example systemd user unit, `apps/daemon/dist/systemd/octoboardd.service`, for running the
daemon on a Linux server from boot, and every Linux release archive carries it as `octoboardd.service`. Its binary path
(`%h/.local/bin/octoboardd`) and port (3369, the default server port) are examples, and its comments say how to install
and enable it by hand; the install script's `--service` installs it with the path and port filled in (see "Installing
it as a service" in `docs/product/installing-the-server-form.md`). What it sets:

- It runs the daemon **in the foreground** with `--port`, as the user who owns the unit, so that user's `~/.octoboard`,
  agent accounts and keys are the daemon's, and restarts it after a failure but not after a clean stop. It passes
  `--service-managed` and restarts the daemon after exit status 75 too, the status a planned restart then ends with, so
  that the next daemon runs in the unit (see "Planned restart and planned stop" above), and records that status as a
  success, not as a failure of the unit; a copy that drops the restart on 75 must drop the option with it.
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
  a daemon stopped that way again; a `stop` that meets a planned restart follows the next daemon the unit starts, as
  above.
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
user (see "Running in the background from the command line" above): the startup line, which carries the launch key, is
discarded. The address carries the owner key, which the next daemon the unit starts — after a planned restart or a
failure — accepts as well, so the same address keeps opening the daemon; the new launch key that next daemon makes is
recorded in turn for the commands (see "The UI the daemon serves" above).
