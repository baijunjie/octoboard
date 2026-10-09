# Writing daemon code

## Live state the daemon derives goes on `AppState`, never as a field on a stored record

A piece of per-entity state the daemon reads off the outside world and does not persist — a project's git status, a
repository's branch, anything recomputed rather than written by the user — is held in its own map on `AppState`
(`apps/daemon/src/state.rs`), broadcast by an event of its own carrying the whole derived record, and replayed inside
`snapshot` so a reconnecting client never has to ask for it separately. Do not add it as a field on `Console`,
`Project` or `Session`.

Those three are SQLite rows, and every `*_upserted` event carries the whole record (`apps/daemon/PROTOCOL.md`,
section "Daemon to client"), so a field that is not in the row has to be refilled on every path that writes that
record, and is blanked in the client the moment any unrelated upsert is applied. For state that changes on a schedule
of its own, that is every write path in the coordinator.

Keeping it off the record costs two cleanups that both have to be written, because the derived state gets no deletion
event of its own: the daemon drops its entry wherever the record itself is removed, and the client's reducer drops it
on that record's `*_deleted` event, cascading from a console down to each of its projects. The git statuses are the
worked example of the whole shape; the trust prompts are the same idea with the repeat sent after each `snapshot`
rather than inside it.

## Time a repeating refresh from the client, and bound it in the daemon

Work that only matters while a client is showing something — the projects of the console in the sidebar, a list in
view — gets a one-shot request in the protocol and no timer in the daemon, which cannot know what any window is
showing. The client owns the interval for as long as it keeps showing that thing, and asks once more on every fresh
`snapshot`, since the daemon may have restarted and lost whatever it was holding in memory.

The daemon then has to bound that work itself, and an in-flight claim is not enough. Several clients can be connected
at once — another window, the UI in a plain browser, any local program — each running the same interval on its own
phase, so their requests interleave rather than overlap and the claim never fires; N clients turn one unit of work
per interval into N. Add a floor per entity on how recently the work last completed, short enough that a refresh the
user's own action asks for still does something.

A claim like that is a guard released on `Drop`, never a `begin`/`end` pair: the work runs on a blocking task that
can panic, and an entity whose claim is never released is excluded from every later refresh for the rest of the
daemon's life.

## Every `git` subprocess the daemon spawns needs a non-interactive environment and a deadline

`git` and `ssh` ask for a missing credential, an unknown host key or a locked key's passphrase on the controlling
terminal, not on stdin, so giving the child a null stdin does nothing to stop it. Nobody can answer that prompt
whatever triggered the call — the daemon has no terminal the user is looking at — and a daemon that does have a
controlling terminal (every dev run, and any launch from a shell) then blocks on it forever, with nothing on screen
to say which call is stuck.

So on every `git` command the daemon builds: set `GIT_TERMINAL_PROMPT=0` and `GIT_SSH_COMMAND=ssh -oBatchMode=yes`,
remove `GIT_ASKPASS` and `SSH_ASKPASS` (the command runs with the user's own shell environment copied in, which may
define them), and run it through `subprocess::run_with_timeout` — or `subprocess::run_bounded` when its output also
needs a ceiling or it must be cancellable — rather than `Command::output`. The deadline matters on its own: it kills
the whole process group, not the direct child, which is what also reaps the `ssh` that `git` forked.

## A `git` read in a user's repository must be kept from rewriting the index

Porcelain `git status` and `git diff` are not read-only: when a tracked file's timestamps moved but its content did
not, both refresh the stat information and rewrite `.git/index`, holding `index.lock` while they do. Against a
repository an agent is working in, that makes the agent's own `git add` or `git commit` fail with "index.lock
exists". `GIT_OPTIONAL_LOCKS=0` stops `git status` from writing but not `git diff`; `git diff` also needs
`-c diff.autoRefreshIndex=false`. Set both on any `git` the daemon runs only to read. Browse reads get them from
`GitEnv::command` in `apps/daemon/src/browse/git.rs`; a `git` read built anywhere else has to set them itself.

A test claiming a read writes nothing to the repository proves it only if the fixture first leaves a tracked file
with a new modification time and unchanged content; without that, nothing triggers the refresh and the test passes
whatever the command does. Compare every file under `.git` (bytes and modification time) before and after, and when
unsure what a `git` command writes, try it in a scratch repository outside the project first.

## Type-check Linux-only code in a scratch crate, since no check on macOS compiles it

Applies to daemon code under `#[cfg(target_os = "linux")]` (the daemon is meant to run on a Linux host too) when you
are developing on macOS. `cargo check`, `cargo clippy` and the test suite on macOS skip that code entirely, so a clean
run says nothing about it. Checking the daemon itself with `--target x86_64-unknown-linux-gnu` does not get there
either: rusqlite's `bundled` feature compiles SQLite from C in a build script, which fails without a Linux C
cross-compiler (`x86_64-linux-gnu-gcc`). So copy the Linux-only functions into a scratch crate outside the repository
that depends only on what they use (typically `libc`), and run `cargo check --target x86_64-unknown-linux-gnu` there
after `rustup target add x86_64-unknown-linux-gnu`. That proves it compiles, not how it behaves; say so when
reporting the change.
