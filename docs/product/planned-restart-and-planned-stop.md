# Planned restart and planned stop

A **planned restart** stops the daemon without ending anything: every session and shell terminal is handed over to the
process holder it runs in, with its screen saved, and the next daemon comes up on the same port and takes them over
with their screens as they were, exactly wherever the saved screen can reproduce them (see "The screen of a terminal
handed over" in `docs/product/process-holders.md`). A **planned stop** is the same handover with no next daemon: the
daemon exits with status 0, and the next daemon started — by hand, by `octoboardd start` or by the desktop application —
takes everything over as exactly.

**Asking for one.** A planned restart is asked for by `octoboardd restart` (see "Running in the background from the
command line" in `docs/product/daemon-command-line.md`), by the control route `POST /restart`, which answers `202` at once
and which `octoboardd restart` sends wherever the daemon's key is recorded, or by the signal `SIGUSR1`, which it sends
outside Windows to a daemon whose key is not. A page served by a daemon that
updates itself asks for one with **Restart now** once an update is installed, which is refused, with an alert saying
why, wherever a planned restart is refused below and, unlike the others, also while a download of the update runs or
while any stop or restart is already asked for (see "Restarting into the update" in
`docs/product/server-form-updates.md`). A planned stop is asked for by the control route `POST /planned-stop` or the
WebSocket request `planned_stop`; both answer at once. They are refused, and nothing is stopped, only for a daemon a
service manager runs (see below) while a process holder it started runs inside the systemd unit the daemon itself runs
in, on the same terms as a planned restart: the daemon exits with status 0 after the stop, the unit stops, and that
holder ends with every session and shell terminal it runs, which a planned stop promises to keep (`POST /planned-stop`
answers `409` with the reason `holders_in_service`, and the `planned_stop` request fails with that code); a caller that
wants those sessions ended uses the ordinary stop. No new program is admitted in the unit from then on either, as with a
planned restart. The daemon's own planned stop, made when the application it follows is gone, is never refused. The
desktop application asks for one to restart into an update (see "Restarting" in
`docs/product/application-lifecycle.md`), and a daemon that follows the application makes one itself when it finds the
application gone with no quit note (see "Crashes and forced termination" in `docs/product/application-lifecycle.md`). A
planned restart is refused, and nothing is stopped, for a daemon that follows the desktop application, which starts the
daemon it talks to itself (`POST /restart` answers `409` with the reason `follows_application`). It is refused as well,
and nothing is stopped, for a daemon a service manager restarts (see below) while a process holder it started runs
inside the systemd unit the daemon itself runs in, having no unit of its own (see "How a holder is detached" in
`docs/product/process-holders.md`): the unit's stop at the restart would end that holder, and every session and shell
terminal it runs, so such a holder counts from its start, and then while it runs a session or shell terminal of the
daemon, until it is lost or its start fails (`POST /restart` answers `409` with the reason `holders_in_service`, whose
message says that sessions and terminals run inside the daemon's systemd unit, so stopping or restarting the daemon
would end them, and to stop or restart the unit with `systemctl` on the server to do it anyway). That decision is made
together with admitting new programs there: once the restart is accepted, a session or terminal that would run inside
the unit (a holder started there, or a program added to a holder already there) is refused until the restart completes.
A planned restart is refused too, and nothing is stopped, while an update of an agent's CLI runs (`POST /restart`
answers `409` with the reason `agent_update_running`, and **Restart now** fails with that code), since the exit would
cut the update off mid-install, and an update asked for once a restart has been accepted is refused as the daemon
shutting down; a planned stop with no successor is never refused for an update. A
planned stop or restart asked for while an ordinary stop (the shutdown request, a termination signal) is already asked
for does not replace it, and a planned stop asked for after the desktop application's quit note is made ordinary (see
"The quit note" in `docs/product/application-lifecycle.md`); an ordinary stop asked for before a planned one has begun
replaces it, and one asked for once it has begun cuts it short (see "An ordinary stop during a planned stop" below).

**Who starts the next daemon.** A daemon no service manager restarts starts the next one itself once it has handed
everything over and let go of `daemon.lock`: in the background, as `octoboardd start` does, on the same port and with
the same launch key, so a page or program that held the old address and key reaches the new daemon unchanged. The key goes to
the next daemon on its standard input, never on its command line or in its environment, where other users of the
machine could find it (see `docs/product/who-can-reach-the-daemon.md`); a second line there names the stopping daemon,
which the next daemon records as the daemon it took over from (see `daemon.owner` in
`docs/product/files-octoboard-owns.md`), and a next daemon of an earlier build reads the key alone. On Windows, where
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
start. A daemon that updates itself first falls back, where it can, when the next daemon was of another build put at the
path — by an update, or a rollback made under it — and its own build is kept beside it as `octoboardd.previous`: it puts
its own build back and starts that instead (see "When the update does not start" in
`docs/product/server-form-updates.md`).

A daemon leaves the next start to a service manager only on evidence, decided once as it starts: the option
`--service-managed`, which the example systemd unit passes, or systemd naming the daemon's own process as the one it
started for a unit (`SYSTEMD_EXEC_PID` equal to the daemon's pid, with an `INVOCATION_ID`), which systemd sets from
version 248, so that on an earlier systemd the option is the only evidence. A daemon started by hand from a terminal
that itself runs under systemd has neither. Such a daemon starts no next daemon: it exits with status **75** after its
planned stop, and the service is to start the next one (see "Running as a systemd user service" in
`docs/product/server-form.md`), which makes a new launch key. A service that passes `--service-managed` must restart
the daemon on status 75. On Windows `--service-managed` is accepted and ignored.

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

## While the terminals are handed over

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

## An ordinary stop during a planned stop

Anything that asks for an ordinary stop while a planned stop is asked for or under way makes the stop ordinary: before
the planned stop has begun it takes its place, and once it has begun it cuts it short. What still reaches a daemon in a
planned stop: a termination signal (`octoboardd stop` sends one outside Windows), or on Windows what stands for one
(see "The daemon on its own" in `docs/product/server-form.md`); the desktop application's quit note (see "The quit note"
in `docs/product/application-lifecycle.md`); and the shutdown request on a connection already open. No new connection is
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
handover to end (see `octoboardd stop` in "Running in the background from the command line" in
`docs/product/daemon-command-line.md`). When the next daemon of a planned restart, which the daemon started itself, has
taken over by then, `stop` stops that one, which ends every session and shell terminal as an ordinary stop does; a
daemon that does not show itself that next daemon is left running, and `stop` says so; when the daemon has ended with
none taking over, as after a planned stop, `stop` says the daemon stopped and exits with status 0; if that was a planned
stop, where holders run the sessions and shell terminals, they are kept for the next start (an ordinary stop already
under way has ended them). Outside Windows, the termination signal `octoboardd stop` sends reaches the daemon in its
planned stop and makes the stop ordinary, as above, unless the daemon has already settled on its successor: it then
starts that one all the same, or exits for the service manager to start it, and `stop` follows and stops it as on
Windows, a daemon a service manager started included (see `octoboardd stop` in "Running in the background from the
command line" in `docs/product/daemon-command-line.md`).
