# Running in the background from the command line

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
  foreground daemon's `--port`, or, without `--port`, on the default server port (see "Choosing the port" in
  `docs/product/server-form.md`). While a daemon already runs it refuses, saying so with that daemon's pid and address
  where they are known — a daemon still starting or restarting is named once it has recorded itself, and one still not
  recorded after the wait above is said to run at an address not known — and exits with status 1. Otherwise the daemon
  is started with `--port <port>` in a session of its own, with no controlling terminal and the data directory as its
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
  daemon's pid and its address, `http://127.0.0.1:<port>`, and exits with status 0. A daemon that ends before reporting
  (its port taken, say, or another daemon took the lock first) makes `start` say that it ended while starting, with its
  exit status, followed by what it wrote on its error output (the first 16 KiB), and exit with status 1; one that has
  reported nothing after 30 seconds is killed, with the same report. Once `start` has returned, what the daemon writes
  on its standard output and error output goes nowhere; its log is still `~/.octoboard/logs/daemon.log`. On a host whose
  logind is configured with `KillUserProcesses=yes`, the system ends every process of a login when the login closes, a
  daemon `start` started included; run it as a service there (see "Running as a systemd user service" in
  `docs/product/server-form.md`).
- **`octoboardd stop`** with no daemon running says so and exits with status 0. When it found a daemon holding the lock
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
- **`octoboardd restart`** asks the daemon for a planned restart (see
  `docs/product/planned-restart-and-planned-stop.md`) and takes no option. With no daemon running by the end of the wait
  above it says so and exits with status 1. It also exits with status 1, restarting nothing, for a running daemon that
  no usable `daemon.owner` names by then (as `stop` does); for one that follows the desktop application, saying that the
  application restarts it itself; for one that could not install its handler for the request, whose record does not say
  whether it follows an application and which the request would end rather than restart, saying to stop and start it
  instead. Otherwise, wherever `daemon.key` holds the daemon's key, on every host, it sends the planned-restart request
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
    connected until its connection drops (see "The UI the daemon serves" in `docs/product/server-form.md`); to cut such
    pages off at once, run `octoboardd restart` after rotating, which drops every page's connection while the sessions
    keep running. A daemon that refuses the request, fails to write the new key (the old one then stands, on disk and in
    the daemon) or no longer accepts the connection makes it say on its error output that the owner key was not rotated,
    and why, and exit with status 1, with nothing on standard output.
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

## Printing the version

`octoboardd --version`, or `-V`, prints one line on standard output, `octoboardd <version>`, and exits with status 0. It
touches nothing: it creates no data directory, takes no lock and makes no request, whatever daemon runs. It is an
option of `octoboardd` alone: none of the commands above takes it. The server form's update runs a new build this way
before putting it in place (see "Downloading and replacing the executable" in `docs/product/server-form-updates.md`).
