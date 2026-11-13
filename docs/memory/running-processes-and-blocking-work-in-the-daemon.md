# Running processes and blocking work in the daemon

## Cap how many blocking threads a per-entity pass starts at once

Applies to daemon work that runs one blocking task per project, per directory or per session — a sweep over a console's
projects, a reconciliation at daemon start, a read of every directory under a parent being associated. One
`tokio::task::spawn_blocking` per item is wrong however cheap the item looks: that pool is one pool for the whole
daemon, and every user-visible path shares it — opening a session, answering a trust prompt, a browse read — so
associating a parent directory that holds a few hundred repositories fills it and the user's next action waits behind
the slowest item. Such a task usually also waits on `env_shell::cached_snapshot()`'s single lock, held for a whole
login-shell fork while the cache is cold, so the threads past the first few are not even doing work in parallel.

So run the items through `blocking_pass::each_blocking` (`apps/daemon/src/blocking_pass/mod.rs`) instead, which starts
the next item as soon as any one finishes, and give each item a deadline of its own, so one that stalls holds a single
place rather than the pass. A pass that needs another shape (a semaphore, the directory looks in `hostfs::place`) is
held to the same `blocking_pass::BLOCKING_WIDTH` rather than a figure of its own.

## Keep process starts off every path that runs per launch, per hook event or per sweep item

Applies to daemon code that would start a process — a `--version` or `--help` probe, a login shell, a hook forwarder,
a `git` per project — on a path that runs for every session launch, every hook event, or every item of a periodic pass.
On a Mac running endpoint-security software, as managed machines do, each exec can stall for seconds (and a login-shell
snapshot is a serial chain of them), so a process that costs milliseconds on a development
machine is what the user waits on, multiplied by the number of sessions.

So look for a route with no process first: read what is wanted from where it is installed or stored (as
`AgentProvider::installed_version` reads an agent's version without running it), serve a cached result (the
`env_shell` snapshots, never a fresh login shell per call), or let the other side call the daemon (a Claude Code that
takes HTTP hooks posts to the hook route with no forwarder process behind it). Where a process cannot be avoided, run
it off the path the user waits on, cache what it returns, and bound how many run at once.

## Ending everything an agent CLI started takes its session, not its process group

Applies to daemon code that runs an agent CLI and has to end whatever it left running — when it exits, when it is
stopped, when the daemon goes down. Agent CLIs move some helpers (a REPL or code-execution host, a background shell
task) into process groups of their own, so the process-group stop other subprocesses get leaves them running; they
keep the CLI's session. Do not write a sweep of your own: run the CLI through `apps/daemon/src/processes/subprocess.rs`
with `Reach::Session` (`Bounds::agent_query` for a short query through `run_bounded` — an account's query through
`adapter::account_cli`, never by hand, see `docs/structure/daemon.md`, "Shared entry points" — or
`Bounds { reach: Reach::Session, .. }` for `run_exchange`), and where it
cannot go through there (it runs on a PTY, or a flow reads it while it runs), build on
`apps/daemon/src/platform/proc/` (`lead_session`, `kill_group`, `kill_members`). Anything that is not an agent CLI, such as `git`, keeps `Reach::Group`
(`run_with_timeout`): a session sweep lists every process on the system.

`Reach::Session` sweeps the session when the CLI ends by itself too. Where the CLI can start something the user goes
on using — a login that opens a browser, which `xdg-open` on a Linux desktop starts straight into the CLI's session —
sweep only when the daemon ends the CLI (cancel, timeout, daemon exit) and just reap one that exits by itself, as the
account login flows in `apps/daemon/src/accounts/login/flows.rs` do.


## A process that may have a controlling terminal never takes a login-shell snapshot

Applies to code an `octoboardd` subcommand runs when a person starts it from a terminal (`apps/daemon/src/cli/`, such
as `octoboardd update`), and to every shared helper such code reaches. `env_shell`'s snapshots (`snapshot`,
`launch_snapshot`, `cached_snapshot`) start `$SHELL -l -i -c` in a process group of its own; from a process with a
controlling terminal, that interactive shell is stopped by `SIGTTOU`/`SIGTTIN` on the terminal it shares, and the
command hangs. The daemon has no controlling terminal and never meets this, so its own paths and tests do not show it.
Such a process already runs in the user's shell environment: read what it needs (a proxy variable, `PATH`) from its
own environment (`std::env`), and where a shared helper takes a snapshot, give it a way to be handed the process's own
environment instead, as `UreqFetcher::command_line` (`apps/daemon/src/self_update/fetch.rs`) does.

## Long work that must not hold the daemon's stop runs on a thread of its own, not the blocking pool

Applies to daemon work that can run for minutes — a download, a wait on the network or on another process — which the
daemon's stop or a planned restart should not wait for. Once `run_daemon` returns, `main` (`apps/daemon/src/main.rs`)
shuts the runtime down waiting up to `RUNTIME_SHUTDOWN_WAIT` (5 s) for the `spawn_blocking` work still running, then
exits with that work cut off wherever it stands, its child processes with it under the systemd unit
(`KillMode=mixed`). So long work on the blocking pool both holds the exit for those seconds and is cut short at an
arbitrary step. Start it with `std::thread::spawn`, and check `AppState::stop_asked` right before each step that cannot
be undone (replacing a file, writing a record), refusing it once the stop has begun.

The one exception is an agent CLI update (`agents::cli`): the updater's tool must not be cut off mid-install, so a
planned restart is refused while one runs (`RestartRefusal::AgentUpdateRunning`), no update is claimed once a restart
was accepted, and a stop the daemon is asked for waits for it up to its own deadline (`AgentClis::wait_for_updates`,
called by `main` before the 5 s runtime shutdown, so an update that ends early frees the exit at once). A stop a service
manager makes is cut at the unit's `TimeoutStopSec` instead, which kills the update. Do not add more long
`spawn_blocking` work that the stop must wait for; give it a thread of its own as above.

The same bound means that whatever the stop has to finish — a record the next daemon reads, the handover — must be
awaited by `run_daemon` itself: blocking work started and left (its `JoinHandle` dropped) is not waited for past the
bound.

That check cannot interrupt a blocking read. Where a read can stall, run it on a reader thread of its own with an idle
deadline, so the work gives up and moves on while the abandoned reader ends only with its request's own deadline, and
cap how many abandoned readers may be alive before no new request is made, or a network that stalls every time leaves
one behind per attempt. `pump` in `apps/daemon/src/self_update/fetch.rs` is the shape.
