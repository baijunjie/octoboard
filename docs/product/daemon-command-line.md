# Running in the background from the command line

Seven commands act on the daemon running against the data directory of the user who runs them (`~/.octoboard`), whoever
started that daemon, the desktop application included: `start`, `stop`, `restart`, `status` and `address` below, `browser` (see "Browser access" below), and
`proxy` (see "Another device reaching the daemon over SSH" below). Whether a daemon runs is told by `daemon.lock` alone; which
process it is and where it listens, by `daemon.owner`, and its key by `daemon.key` (see
`docs/product/files-octoboard-owns.md`). Checking takes the lock for an instant, as a shared lock, so that commands
checking at the same moment do not take each other for a daemon, and a daemon starting at that very instant waits up to
a second for it rather than refusing to start. All seven work on Windows too; where Windows differs, the command says so
below.

**A planned restart caught half-way is waited out.** The daemon of a planned restart removes `daemon.owner` before it
releases the lock, and the next daemon records itself only once it holds the lock, so for a moment the lock is held with
no record, and for another it is free. Before it removes the record, the restarting daemon writes `daemon.handover`,
which the next daemon removes once it has recorded itself. So before judging, every command waits while the lock is free
and `daemon.handover` names a daemon whose next one may still take the lock: as long as that daemon's process still
runs, since one that starts its next daemon itself waits for that one to report listening, and, once it has ended, for
as long as a service manager may still start its next daemon (see "Whether a service manager may still start the next
daemon" below). `start`, `stop` and `restart` also wait while a daemon holds the lock without a usable `daemon.owner`
naming it; `status` and `address` do so only once they have seen a handover pending, and only while its
`daemon.handover` is still there, for the next daemon to record itself: the file is removed after the record is written,
so a daemon that holds the lock with no record and no file will not record, and they never wait on one that cannot
record itself. The wait lasts up to **90 seconds**. The daemon that then holds the lock and has recorded itself is the
one the command acts on, with one exception for `stop`: where the wait went through a handover, a daemon whose
`daemon.owner` does not name the handing-over daemon as the one it took over from (see "Which daemon took over" below)
is not that daemon's next one — one `start`, a user or the desktop application started in the free lock, say, or one
whose record does not say. `stop` waits for the handing-over daemon to end, within the 90 seconds, so that its pid is
the one it reports stopped; if it never ends within the wait, `stop` says it is still stopping, adds that another daemon
holds the lock and is left running, with its pid and address, and exits with status 1. Otherwise `stop` leaves the
daemon that is not the next one running, saying, as below, that the daemon stopped but another one has started since,
and exits with status 1. A daemon that starts its next daemon itself and could not tell its own start time writes
neither record nor `daemon.handover`, so the free lock of its restart cannot be told apart from no daemon at all.

**Which daemon took over.** Every daemon records in `daemon.owner` which daemon it took over from, or that it took over
from none, or does not say (for which it names, and when it does not say, see `daemon.owner` in
`docs/product/files-octoboard-owns.md`). `stop` and `restart` recognise the next daemon of the daemon they asked by its
record naming that daemon. Where the next daemon's record does not say, they compare keys instead, both read from
`daemon.key`, the asked daemon's before it was asked: a daemon that starts its next daemon itself hands it its key, and
every other start makes a key of its own, so the same key shows the next daemon and another key shows another daemon.
Where a key could not be read, for either daemon, nothing shows either way: `restart` then takes that daemon for the
next one once the restarted daemon has ended, and `stop` never follows it.

**Whether a service manager may still start the next daemon.** A daemon not recorded as service-managed that has ended
with the lock free has no next daemon on its way: one that starts its next daemon itself has seen that one report
listening, which it does only once it has recorded itself. For a daemon recorded as service-managed that has ended with
the lock free, a command asks systemd about the unit its `daemon.owner` names (`systemctl --user show`), whether or not
the daemon noted a handover: while the unit is activating or deactivating, or active with a main process, a daemon may
still come, and the command goes on waiting, whatever `RestartSec=` the unit sets or however slow the host is; once the
unit is inactive or failed, asked twice 100 milliseconds apart, none will, and the command judges at once. Only where
systemd cannot be asked — the record names no unit (the daemon runs in a system unit, under a systemd older than 248 or
under another service manager), or `systemctl` is missing, fails, does not know the unit, or takes more than **2
seconds** to answer, and one that could not be started or did not answer in time is not asked again by that command —
does a fixed wait stand in: a daemon that noted a handover, or that ended leaving its `daemon.owner` behind as a daemon
that crashed or was killed does, may still be followed for **3 seconds**, counted from the later of when the note was
written and when the command, having seen the daemon running, saw it end; any other daemon that has ended with the lock
free is gone at once.

- **`octoboardd start [--port <port>]`** starts the daemon in the background on that port, with the same range as the
  foreground daemon's `--port`, or, without `--port`, on a port the system assigns (see "Choosing the port" in
  `docs/product/server-form.md`; the browser port, 3369 by default, is another, see "Browser access" below). While a daemon already runs it refuses, saying so with that daemon's pid and address
  where they are known — a daemon still starting or restarting is named once it has recorded itself, and one still not
  recorded after the wait above is said to run at an address not known — and exits with status 1. Otherwise the daemon
  is started, with `--port <port>` where it was given, in a session of its own, with no controlling terminal and the data directory as its
  working directory, so closing the terminal or the login that ran `start` sends it nothing and it runs until it is
  stopped. On Windows it is started instead with a hidden console of its own, in a process group of its own and outside
  the job `start` runs in, so closing the console window that ran `start` does not end it. The daemon keeps none of the
  standard handles `start` was given, and on every host inherits nothing of `start`'s beyond its three standard handles
  or descriptors, the pipes it reports over, so a `start` whose output is captured or piped (a command substitution, a
  PowerShell assignment, `| more`, an SSH session) returns once it has printed, not when the daemon ends. Where that job
  does not let a process leave it, as an SSH session's does not, the daemon is started inside the job and ends when the
  job does — for an SSH session, at logout; `start` then succeeds as below and adds, on its error output after its usual
  lines, a note saying so. `start` waits up to **30 seconds** for the daemon's startup line and prints it unchanged —
  **the key included**, so whoever sees that output, or a log of it, holds the key — followed by a line with the
  daemon's pid and its address, `http://127.0.0.1:<port>` (the port the system assigned, where none was given), and
  exits with status 0. A daemon that ends before reporting (its port taken, say, or another daemon took the lock first) makes `start` say that it ended while starting, with its
  exit status, followed by what it wrote on its error output (the first 16 KiB), and exit with status 1; one that has
  reported nothing after 30 seconds is killed, with the same report. Once `start` has returned, what the daemon writes
  on its standard output and error output goes nowhere; its log is still `~/.octoboard/logs/daemon.log`. On a host whose
  logind is configured with `KillUserProcesses=yes`, the system ends every process of a login when the login closes, a
  daemon `start` started included; run it as a service there (see "Running as a systemd user service" in
  `docs/product/server-form.md`).
- **`octoboardd stop [--force]`** first asks the running daemon (`POST /stop-check`) whether stopping it would interrupt
  work, counted as the application's quit confirmation counts it: a session that is starting, working, waiting for the
  user, dormant, or idle with work left running in the background (which covers one paused with such work), and a shell
  terminal that is running and busy. Where it would, `stop` says what runs (up to ten sessions and terminals by name,
  then how many more), stops nothing and exits with status 1; `--force` skips the check and stops regardless. A daemon
  that cannot be asked (an earlier build, or one whose key is not recorded) is taken as one that may be running work and
  needs `--force` too, and so is one that answers the check with an error, its sessions unreadable (status 500). A stop by signal, a service manager's included, is not asked about, so a script that runs
  `octoboardd stop` over running work now needs `--force`. A stop that goes ahead is as follows, and on the desktop
  application's own copy as in "The application's copy of `octoboardd`" below. With no daemon running, `stop` says so and exits with status 0. When it found a daemon holding the lock
  without a record, or a planned restart half-way, and that came to no daemon running by the end of the wait above, it
  says that the daemon was already stopping, as below, and exits with status 0. Otherwise it sends `SIGTERM` to the
  process `daemon.owner` names — that process, checked by its start time, never another that has taken its pid since —
  which stops as a termination signal has it stop (see "The daemon on its own" in `docs/product/server-form.md`): every
  session interrupted, every shell terminal ended. On Windows, which has no signal for it, `stop` instead sends the
  shutdown request (`POST /shutdown`) to the port `daemon.owner` records, presenting the key `daemon.key` holds for that
  daemon; the key goes only into that loopback request, never into a command line or an environment variable, and the
  daemon stops the same way. A daemon whose key `daemon.key` does not hold — one of an earlier build, or one that could
  not write the record — is not stopped there: `stop` says so, with its pid and port, and exits with status 1. On every
  host `stop` reads that key before it asks, to recognise the daemon's successor below. Once a daemon has taken the
  request (the signal reached it, or it answered the request), `stop` waits up to **60 seconds** for it to be gone — the
  lock free, its process ended, and no next daemon on its way (see "Whether a service manager may still start the next
  daemon" above) — and exits with status 0, but within **90 seconds** of the command's start, the wait above for a
  daemon to be found included; a daemon that has ended with the lock free when the wait runs out is gone, whatever a
  service manager may still start. On Windows the daemons it goes on to follow share the first 60 seconds, counted from
  the first request a daemon took, while on Unix each daemon it follows gets 60 seconds of its own, counted from its own
  request, within the 90 seconds overall. A daemon still running when the wait ends makes it say so, with the pid, and
  exit with status 1, while the stop goes on. A daemon that is already gone when signalled, or no longer accepts the
  connection (refuses it, or resets it unanswered), has ended, or is ending, or is in a planned stop or restart (see "An
  ordinary stop during a planned stop" in `docs/product/planned-restart-and-planned-stop.md`): `stop` then watches
  `daemon.lock` and `daemon.owner` for up to **90 seconds** from its start, the wait above for a daemon to be found
  included, so that a stop that waited then takes the request has what is left of them. Either way, once another daemon
  holds the lock and shows itself the next daemon of the one asked (see "Which daemon took over" above), `stop` asks that
  one instead, the same way, at once, and follows it in turn; one that does not take the request is waited for within
  the same 90 seconds. A daemon that has already settled on starting its next daemon ignores the signal, and that next
  daemon is then stopped instead. So is the daemon a service manager starts after a service-managed daemon that crashed or was killed,
  which names that daemon in its record. Any other daemon that holds the lock once the daemon asked has ended is not
  stopped: one the desktop application or a user started since; one a service manager started after a stop, as a unit
  that restarts the daemon after every exit (`Restart=always`) starts one after an ordinary stop too, and a unit with
  `Restart=on-failure` after a daemon that removed its record and exited with a failure status; or one for which
  nothing shows either way. `stop` then says that the daemon stopped but another one has started since and is left
  running, with its pid and address, that nothing shows the stopped one handed over to it, and exits with status 1. A
  daemon not followed is reported only if it already holds the lock when `stop` looks, while it is still waiting for the
  daemon asked to be gone. On Windows the request sent to a daemon in a planned restart can reach its next daemon
  instead, which listens on the same port with the same key once the first has let go of them; that one then no longer
  accepts the request `stop` sends it on following it, and `stop` still reports it stopped. Once the lock is free and
  the process the record named has ended with no daemon recorded after it and none on its way, where no daemon this
  `stop` asked took the request (on Unix a delivered signal counts only for the daemon it reached, not for one followed
  after it), nothing is left to stop: `stop` says that the daemon was already stopping, and that if that was a planned
  stop, it left its sessions and shell terminals running in their holders for the next daemon to take over, and exits
  with status 0. A daemon that has come to none of these by then makes `stop` say that it is still stopping after the
  seconds it waited in all, with the pid of the last daemon it followed, and exit with status 1. Any other failure of the
  request is reported as an error and nothing is waited for. A running daemon that no usable `daemon.owner` names by the
  end of the wait above — it is still starting or stopping, or could not write the record — is not stopped: `stop` says
  so, with the seconds it waited and, on Linux, the pid of the process that holds the lock where exactly one does, and
  exits with status 1. A desktop application whose daemon is stopped this way does not start it again and says the
  daemon exited, as for any daemon that ends with status 0 (see "Starting up" in
  `docs/product/application-lifecycle.md`).
- **`octoboardd restart [--yes | --if-safe]`** asks the daemon for a planned restart (see
  `docs/product/planned-restart-and-planned-stop.md`). With no daemon running by the end of the wait
  above it says so and exits with status 1. It also exits with status 1, restarting nothing, for a running daemon that
  no usable `daemon.owner` names by then (as `stop` does); for one that follows the desktop application, saying that the
  application restarts it itself; for one that could not install its handler for the request, whose record does not say
  whether it follows an application and which the request would end rather than restart, saying to stop and start it
  instead.

  Before the request, where its standard input is a terminal, it asks the daemon (`POST /restart-check`) what a restart
  into the build it runs itself would interrupt — the build of the file it was run from, which it takes for the one the
  daemon starts its next daemon from, as `octoboardd update` does — judged by that build's version and holder floor, as
  the in-app restart judges it (see "Restarting into the update" in `docs/product/server-form-updates.md`). Where nothing
  that asks would end it says that every session keeps running and goes on; where sessions or shell terminals running a
  command would end, or may (the daemon cannot tell, as one from before the check cannot), it lists them and asks
  `Restart now? [y/N]`, and anything but a yes leaves the daemon running, saying `not restarted`, with status 0. A
  restart the daemon would refuse is not asked about: where it would interrupt work, or may, it is not requested
  either, the command saying the daemon's reason and exiting with status 1; where it would interrupt nothing, the
  request below says the daemon's reason. A stop or restart already under way is not added to: the command says that
  Octoboard is already stopping or restarting, requests nothing, and follows it as it follows a restart it requested
  (below), exiting 0 once a daemon runs in its place; where none does (the stop was an ordinary one, say), it says
  that the daemon was already stopping and has stopped and that no daemon runs in its place, advising `octoboardd
  start` only for a daemon no service manager runs, and exits with status 1. Where the daemon has ended by the time the
  user has answered (stopped, or restarted from elsewhere), it is not asked again: the command finds the daemon as
  above, waiting out a restart half-way, and says what it found, exiting 0 where another daemon runs in its place, and
  1 where none does or one has not recorded itself by the end of the wait (advising `octoboardd start` only for a
  daemon no service manager runs). Run with no terminal (a script, a pipe), it asks nothing and
  restarts regardless, as it always has. **`--yes`** restarts regardless, asking nothing and checking nothing;
  **`--if-safe`** checks in any case and restarts only where nothing would end, otherwise listing what would (or may)
  end, saying that it is not restarting as the restart would (or may) interrupt that work, and exiting with status
  **4** without restarting and without naming a command to run next: what the install script runs after an upgrade,
  which names it. The two cannot be given together.

  Otherwise, wherever `daemon.key` holds the daemon's key, on every host, it sends the planned-restart request
  (`POST /restart`) over loopback to the port `daemon.owner` records, with that key, which goes only into that loopback
  request, as `stop` sends it on Windows, so that a refusal comes back at once: a daemon that refuses the request (see
  `docs/product/planned-restart-and-planned-stop.md` for when it does) makes `restart` say that it refused the restart,
  with the daemon's own reason, and exit with status 1, and any other failure of the request is reported as an error.
  Only for a daemon whose key `daemon.key` does not hold, as one of an earlier build, does it send the request as the
  signal `SIGUSR1` instead, to the process `daemon.owner` names, checked by its start time as `stop` checks it; a
  daemon that follows an application refuses the signal too, and logs that it did. On Windows, which has no signal for
  it, such a daemon is not restarted: `restart` says that its key is not recorded, as `stop` does, and exits with
  status 1. A daemon that no longer accepts the connection (refuses it, or resets it unanswered), or is gone when
  signalled, is not reported stopped at once: one in a planned stop of its own resets the request while it ends, and a
  service manager may be restarting one that has ended, so `restart` goes on waiting for the next daemon as below, and
  judges the daemon as below once it has ended with the lock free. The key `restart` reads before it asks also serves to
  recognise the next daemon. It then waits for the next daemon, up to **90 seconds** from the start of the command, the wait above
  included, to hold `daemon.lock` and record itself in `daemon.owner`, prints a line with the new daemon's pid and its
  address, `http://127.0.0.1:<port>`, and exits with status 0. The next daemon is recognised as "Which daemon took
  over" above has it: one that shows itself the next daemon is taken at once, even while the restarted daemon still
  waits for it to report listening; one for which nothing shows either way, once the restarted daemon has ended. Any
  other daemon that holds the lock once the restarted daemon has ended makes `restart` say that the daemon stopped to
  restart but another one has started since and is left running, with its pid and address, that nothing shows the
  restarted daemon handed over to it, and exit with status 1. It prints no key: the next daemon keeps the launch key
  where the daemon starts it itself, and a successor under systemd makes a new one (see
  `docs/product/planned-restart-and-planned-stop.md`), while the owner key stays the same either way, so an address
  `octoboardd address` printed with it opens the new daemon as it did the old (see "The UI the daemon serves" in
  `docs/product/server-form.md`). Once the restarted daemon has ended with the lock free and no next daemon on its way
  (see "Whether a service manager may still start the next daemon" above), no daemon will take over: it crashed or was
  stopped, a stop asked for meanwhile cut the restart short, or the next daemon did not start. `restart` then says that
  the daemon stopped to restart, with its pid, and that no daemon runs in its place, for one of those reasons, which the
  log tells apart, and exits with status 1. For a daemon whose record does not say whether a service manager starts its
  next daemon, as an earlier build's does not — such a daemon may run under a service manager and exit for it to start
  the next one, leaving neither `daemon.handover` nor its record — `restart` says so only once **3 seconds** have passed
  since it saw the daemon end. When no daemon has taken over within the 90 seconds, it says that the daemon is still
  stopping, with its pid, or that it stopped but no daemon took over, that the daemon's log says why and that the
  sessions are kept for the next start, and exits with status 1. A daemon that was gone before the request reached it,
  because it had crashed or been stopped, is judged the same way, without waiting for the 90 seconds: once the lock is
  free, the daemon has ended and no next daemon is on its way, `restart` says that it was no longer accepting the
  restart and has stopped, with no daemon in its place, and exits with status 1. A daemon that took over from such a
  daemon is judged as for any other: one that shows itself its successor — as the daemon a service manager starts after
  a service-managed daemon that crashed does, since it names that daemon in its record — or for which nothing shows
  either way is reported as restarted, with its pid and address, and exit status 0; any other daemon that holds the lock
  is reported as another daemon that has started since, left running, and exit status 1. Where no usable `daemon.owner`
  names the running daemon by the end of the wait above, `restart` names, on Linux, the pid of the process that holds
  the lock where exactly one does, as `stop` does.
- **`octoboardd status`** says that the daemon is running, with its pid and its address, `http://127.0.0.1:<port>`, or,
  when no usable `daemon.owner` names it, that it is running at an address not known, and exits with status 0; with none
  running it says so and exits with status **3**. With the lock free in the middle of a planned restart it waits as
  above and reports the next daemon rather than none. A user for whom no daemon has ever run has no data directory, and
  `status` creates none.
- **`octoboardd address`** prints the address to open the running daemon at in a browser, its key included, as a single
  line on standard output and nothing else there, `http://127.0.0.1:<port>/?key=<key>`, and exits with status 0.
  `<port>` is the port the browser's machine forwards to, the same at every start, since the daemon's own port is one
  the system assigns at each start (on both forms): the browser port the daemon serves at while it is open (on Windows
  and macOS the desktop application's daemon while browser access is on, and the server form once `octoboardd browser on`
  opened one), else the default browser port, 3369, which the socket's forward names (see "Remote access to the desktop application's
  daemon" in `docs/product/who-can-reach-the-daemon.md`, and "The browser socket" and "Browser access" in
  `docs/product/server-form.md`). For a daemon that accepts no owner key it is the daemon's own port, whose address
  carries the launch key and is for programs. The output is meant to be read by a program as well, such as one run over SSH. The host
  is `127.0.0.1`, a loopback address, which a browser treats as a secure context and which the SSH forward below binds
  on the owner's own machine. The key is the owner key where the running daemon's `daemon.owner` says it accepts one
  and `owner.key` holds a key it trusts; otherwise it is the daemon's launch key, as for a daemon that serves no browser
  or is of an earlier build (see "The owner key" in `docs/product/who-can-reach-the-daemon.md`). Where the record says
  the daemon accepts an owner key but `owner.key` is missing, cannot be read or holds no key it trusts, it prints the
  launch key's address and adds on its error output that this address lasts only until the daemon restarts, and that
  `address --rotate` makes a lasting one. For the desktop application's daemon while neither its browser access nor its
  remote access is on there is no address: it says so on its error output, naming Allow browser access in the
  application's Settings and `octoboardd browser on`, and exits with status 1 with nothing on standard output; on Windows,
  where the owner pipe is no browser's way in, so does a daemon with remote access on alone. Its options are `--rotate` (below) and `--json` (see "Another device
  reaching the daemon over SSH" below).

  On its error output it also says how the address is opened. For a daemon that accepts the owner key on its socket and not at a browser port
  (see "The browser socket" in `docs/product/server-form.md`), it prints the SSH command that
  forwards the socket to the owner's machine, with the default browser port, the socket's real path and `<user>@<server>`,
  `<user>` being the login name the system records for the user (just `<server>` where it records none) and `<server>`
  left for the owner to fill in, and says to run it there and open the address there. Before that, it asks the daemon on
  its socket whether it accepts the key in the address, allowing it **5 seconds** to take the connection and as long to
  answer, and adds a warning when the daemon
  refuses it — `owner.key` changed since the daemon started, say, for which `address --rotate` makes an address it
  accepts — or when the socket cannot be asked, naming the socket and why. For the desktop application's daemon on
  Windows or macOS while its browser access is on, or the server form's daemon while browser access is on, which takes the owner key at its browser port, it prints instead the command that forwards that
  port (a browser on the same computer opens the address as it is, with no forward),
  `ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:<port>:127.0.0.1:<port> <user>@<server>`, after asking the daemon at
  that port, as above, whether it accepts the key, and warning when it refuses it (`owner.key` changed, or another
  program holds the port) or nothing answers there. For a macOS or server-form daemon whose browser port could not be
  opened while remote access serves a socket (macOS), the output is that of the socket. For a daemon that does not accept the owner key, on Linux and macOS,
  it says instead that the daemon serves no browser page — it is an earlier build, its build carries no UI, or its data
  directory is not private to its user or its socket could not be opened, as its log says — and that the address is for
  programs that take its launch key. Neither changes the status.

  With no daemon running it says so on its error output and exits with status **3**, creating no data directory, and
  waits through a planned restart, as `status` does. A running daemon that no usable `daemon.owner` names — it is still
  starting or already stopping, or could not write the record — makes it say that the address is not known, and exit
  with status 1. So does a daemon whose launch key `daemon.key` does not hold, whether or not it accepts an owner key,
  naming its pid and port: one of an earlier build, which records no key until a daemon of this build takes its place,
  or one that could not write the record, as its log says. Both keys are read from files only the daemon's user can
  read, so another user of the machine cannot obtain them this way: their own `address` looks at their own data
  directory, where no daemon of that owner's runs.
- **`octoboardd address --rotate`** replaces the owner key, so that every address carrying the old one stops opening
  the daemon, and prints the address with the new one where the running daemon accepts it. It finds the daemon as
  `address` does, waiting through a planned restart, and like it exits with status 1, rotating nothing, for a running
  daemon that no usable `daemon.owner` names or whose launch key `daemon.key` does not hold. Otherwise:
  - **A running daemon that accepts the owner key** is asked to replace it, by a request over loopback (`POST
    /owner-key/rotate`) carrying the launch key from `daemon.key`, which goes only into that request, as `stop` sends it
    on Windows. The daemon writes the new key to `owner.key`, refuses the old one from its next request on, and at once
    closes every connection the old one opened, while those the launch key opened stay (see "The owner key" in
    `docs/product/who-can-reach-the-daemon.md`); `address --rotate` prints the address with the new key, with the same
    notes on its error output as `address`, and exits with status 0. A daemon that refuses the request, fails to write
    the new key (the old one then stands, on disk and in the daemon) or no longer accepts the connection makes it say on
    its error output that the owner key was not rotated, and why, and exit with status 1, with nothing on standard
    output.
  - **The desktop application's daemon while neither its remote access nor its browser access is on**: `owner.key` is
    replaced directly, for when one is turned on, and it says so on the error output, with the note that neither is on,
    and exits with status 0 with nothing on standard output.
  - **Any other running daemon that does not accept the owner key** — it is of an earlier build, serves no browser, or
    could not read or write `owner.key` as it started — is left as it is: `owner.key` is replaced directly, for the next
    daemon that serves the browser, and the address printed is the running daemon's launch key's, with a note on the
    error output saying so; status 0.
  - **With no daemon running**, `owner.key` is replaced where the data directory exists, for the next daemon that
    serves the browser, and the error output says so; where there is no data directory, nothing is
    created and it says only that no daemon runs. Either way it exits with status **3**, with nothing on standard
    output. A daemon that starts while the file is being replaced is waited for until it has recorded itself, and the
    key is then rotated through it as above.

`octoboardd update` acts on the installed executable rather than on the running daemon: it checks for, downloads and
puts in place a newer release, or puts the previous one back, then restarts a running daemon into it as `octoboardd
restart` does, with the same check, leaving the restart for later where it would interrupt work and there is no terminal
to ask in (see "The `octoboardd update` command" in `docs/product/server-form-updates.md`).

## The application's copy of `octoboardd`

On macOS the application bundles the same `octoboardd` the server form ships (`Octoboard.app/Contents/MacOS/octoboardd`),
and the copy knows it is the application's (as `proxy` does, which exits with status 6 for it). Its daemon is the
application's, started and stopped with it, so that copy's `start` and `stop` act on the application and never start a
daemon of their own, which the application opened afterwards would find holding its data directory:

- **`start`**, with no daemon running, launches the application in the background through the system (`open -g`, with
  the argument `--start-in-background`): no window, the application out of the Dock behind its menu bar icon, whatever
  **Keep running in the background** says. It waits up to **30 seconds** for the application's daemon to record itself, then prints what
  the server form's `start` prints, the startup line (the launch key included) and the line with the daemon's pid and
  address, and exits with status 0; a daemon that has not recorded itself by then makes it say so and exit with status
  1. `--port` is refused, saying that the application picks its own port, with status 1 and nothing launched: the
  application's daemon takes a port the system assigns. With the daemon already running it refuses, saying so, as the
  server form's does.
- **`stop`**, once the check above has let it go ahead, asks the application to quit by sending it `SIGUSR1` (the
  application is the daemon's parent; where the daemon's parent is not this installation's application, `stop` sends
  nothing, says so and exits with status 1), which quits as a confirmed quit does, with no quit confirmation shown, and
  stops its daemon as a quit does (every session ended). It waits up to **60 seconds** for the application and its
  daemon to end and prints that the application quit. `SIGUSR1` is a quit that asks nothing; no other signal is.
- **`restart` and `update`** stay refused for the application's daemon, which the application restarts itself.

**The command-line tool.** macOS puts nothing inside an application bundle on the PATH, so Settings, under General,
Command line, offers to link the bundled `octoboardd` into it (see "Command-line tool" in
`docs/product/general-settings.md`).

## Browser access

**`octoboardd browser on [--port <port>]`, `browser off` and `browser status`** change and report the daemon's browser
access setting through the running daemon: they open a control connection to it with the launch key from its owner-only
record and send the request Settings sends (`set_browser_access` and, with `--port`, `set_remote_access_port`; see
"Remote access" in `apps/daemon/PROTOCOL.md`), so the setting is the one the daemon keeps, applies at once and keeps
across restarts, whichever of the two changed it. Settings does not run the command. On both forms the setting is Allow
browser access and the browser port alone, apart from the desktop application's Allow remote access (see "Remote access
to the desktop application's daemon" in `docs/product/who-can-reach-the-daemon.md`): `browser on` serves the browser at
its loopback port (3369 until `--port` or Settings sets another, kept across restarts), and `browser off` closes the port
and every connection made through it, and never closes the socket, the owner pipe or the connections made through them:
on the desktop application's daemon it says that Allow remote access is a setting of its own and was not changed, on the
server form that the socket keeps serving. On the server form the browser socket always serves and the port is off until
`browser on` (see "Browser access" in `docs/product/server-form.md`). `browser on` asks nothing; it says where the
browser is served, that `octoboardd address` prints the address that opens it, and what it costs, in the words of the
Settings confirmation: on a computer other people also log in to, one of them can take the browser port while the daemon
is not running or is restarting, and a browser that reconnects then sends them the key. A port that cannot be listened
on makes it say so, name `--port` as the way to choose another, and exit with status 1; so does a port equal to the
daemon's own (`octoboardd status` names it), which it refuses naming both ports before changing anything. `browser
status` says whether browser access is on and at which port, or that it is off, and exits with status 0 for any state it
could read.

With no daemon running, each says so and that nothing was changed, and exits with status **3**; a daemon that no usable
record names, or whose key is not recorded, is reported as `address` reports it, with status 1. A daemon that serves no
browser (its build embeds no UI, or it could not open its browser socket) has no setting, which is said, with status 1;
so does a daemon with a socket and no browser port (a Linux desktop's).
A change a connection opened by the owner key asks for is refused by the daemon, so a browser cannot turn its own
access on or move it.

## Another device reaching the daemon over SSH

Two modes of `octoboardd` are for a program run on the daemon's machine as the daemon's user — the desktop application
on another device, running them over SSH (see `docs/product/remote-daemons.md`). They reach the running daemon at its
owner endpoint: its browser socket on Linux and macOS, and on Windows the owner pipe of the desktop application's daemon
(see "Remote access to the desktop application's daemon" in `docs/product/who-can-reach-the-daemon.md`). Neither takes
a key on its command line. They are a contract between releases, since the device's application and the machine's
`octoboardd` are installed and updated apart: what they print only grows.

- **`octoboardd address --json`** (which takes no `--rotate`) prints one line on standard output, a JSON object, and
  nothing else there: `{"owner_key": "<64 hex>", "generation": <number>, "daemon_version": "<release version>"}` — the
  owner key, and the protocol generation and release version the daemon greets a connection with. It is printed only
  once the daemon, asked on its owner endpoint with that key, has accepted it. A reader ignores a field it does not know.
- **`octoboardd proxy`** connects to the owner endpoint and carries bytes both ways between it and its own standard
  input and output until either side closes, writing nothing else on standard output (its messages go to its error
  output). It reads no key: the program speaks the daemon's protocol over it, sending the owner key in its own requests
  as a browser does. On Linux and macOS, once its input ends it carries on until the daemon closes, so the answer to a
  last request is not lost; on Windows, whose pipe cannot be half-closed, it carries what the daemon still sends until
  the daemon has been silent for **2 seconds**, then ends the connection.

Both find the running daemon as `address` does, waiting through a planned restart, and exit with a status that says
why they could not reach it, the reason on their error output:

| Status | Meaning |
|---|---|
| `0` | `address --json` printed its line; `proxy`'s connection ended, by either side |
| `1` | Anything else that failed: `owner.key` is missing or unusable, or the daemon refuses its key; the daemon's answer could not be read (`address --json` then prints nothing on standard output); the daemon runs but its record is not yet readable; `proxy`'s connection failed in the middle |
| `3` | No daemon runs |
| `4` | The running daemon has no owner endpoint: it takes no owner key (an earlier build, a build without the UI, or a way in that could not be opened), or it is the desktop application's daemon with remote access off (browser access alone opens no owner endpoint) |
| `5` | The daemon has an owner endpoint, but it cannot be connected to |
| `6` | No daemon runs, and this `octoboardd` is the desktop application's own copy, whose daemon runs only while the application does: the application is not running |

A status a program does not know is a failure. `3` and `6` differ only in what can be done: a server form's daemon is
started on the machine (`octoboardd start`), the application's by opening the application there.

## Printing the connection details

**`octoboardd connection [--host <host>] [--ssh-port <port>] [--text]`** prints what another device of the owner's sets
itself up from to reach this machine's daemon over SSH (see "The macOS and Windows applications" in
`docs/product/who-can-reach-the-daemon.md`), and exits with status 0. It touches nothing: it asks no daemon, takes no
lock and creates no data directory, so it prints the same whether a daemon runs or not. The details carry no secret:

- **The host and SSH port**: those given with `--host` and `--ssh-port`; else the server address and port of the SSH
  session the command runs in, read from `SSH_CONNECTION`, which is what that session's client reached the machine at;
  else the name the machine announces on its network, at port 22 (on macOS its Bonjour name, `<LocalHostName>.local`; on
  Linux its host name and on Windows its computer name, each as the system reports it: the same name the Access
  section's SSH forward offers first). `--host` with no `--ssh-port` keeps the session's port where there is one. On its
  error output it says where the host came from — the SSH session, or the machine's own name, which other devices may
  not know — and that `--host` and `--ssh-port` correct it; with a host given it says nothing of it. Where neither a
  host is given, nor a session nor the machine's name can be read, it says to give one with `--host` and exits with
  status 1.
- **The user**: the login name the system records for the user it runs as. Where it records none the command says so
  and exits with status 1.
- **The host key fingerprint**, `SHA256:<base64>` without padding, as `ssh-keygen -l` prints it, of the first of the SSH
  server's public host key files that reads, in the order the application's SSH client prefers their algorithms:
  `ssh_host_ed25519_key.pub`, `ssh_host_ecdsa_key.pub`, `ssh_host_rsa_key.pub`, in `/etc/ssh` (on Windows, `ssh` under
  `%ProgramData%`). With none readable it is left out, and the error output says that the application shows the host
  key at the first connection for the owner to confirm.
- **The path of `octoboardd`**: the path it was invoked by, found through `PATH` for a bare name, its final symlink
  unresolved (the path `start` launches the daemon from). Where it cannot be told it is left out, and the error output
  says that the application looks in the places Octoboard installs to.
- **A name**: the name the machine announces on its network, the same name as the default host (on macOS its Bonjour
  name, `<LocalHostName>.local`), for the entry's name, left out where it is the host.

They are one line of text, a URI, with the fields left out that are missing:
`octoboard+ssh://<user>@<host>:<port>?fingerprint=<fingerprint>&octoboardd=<path>&name=<name>`. The user, host and values are percent-encoded as UTF-8, except for the
characters a URI never reserves and, in a value, `:`, `/` and `@` (so a fingerprint's `+` reads `%2B`); a host with a
`:`, an IPv6 address, is put in brackets with its zone's `%` written `%25`. A client reads fields it does not know as
none, so a later field does not break an earlier client. On standard output the command prints a QR code of that
line, with nothing added, then an empty line and the line itself; with `--text`, the line alone. The QR code is drawn
with block characters, two rows of modules to a character, with a quiet zone of two modules; where standard output is a
terminal each of its lines is set to black on bright white, so it reads the same on a dark terminal. Its error output
ends with a note that the line is pasted into the Connection details field when adding a remote daemon in the
application, and that the QR code holds the same line. No client scans the QR code yet: it is for a mobile client.

The desktop application's daemon gives the same line, and its QR code, to its Access section while remote access
is served (see "Connection details" in `docs/product/remote-access-settings.md`). What the application does
with a line pasted into the form that adds or edits a remote daemon is in "Saved remote daemons" in
`docs/product/remote-daemons.md`.

## Printing the version

`octoboardd --version`, or `-V`, prints one line on standard output, `octoboardd <version>`, and exits with status 0. It
touches nothing: it creates no data directory, takes no lock and makes no request, whatever daemon runs. It is an
option of `octoboardd` alone: none of the commands above takes it. The server form's update runs a new build this way
before putting it in place (see "Downloading and replacing the executable" in `docs/product/server-form-updates.md`).
