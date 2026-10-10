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

## Build every `git` the daemon spawns from `git_env.rs`, and give it a deadline

`apps/daemon/src/git_env.rs` assembles every `git` command the daemon runs: `non_interactive` is the base, and a
command that only reads a repository adds `read_only` on top of it, before the subcommand is pushed, since it
contributes a `-c` argument. Start from there rather than from `Command::new`, and set none of those variables at a
call site; the module header carries the reason for each one, and a call site that assembles its own takes some of
them and misses the rest.

Two of those reasons are worth having in advance, because the symptom turns up nowhere near the call. A `git` child
that can prompt hangs the daemon with nothing on screen to say which call is stuck: `git` and `ssh` read a missing
credential, an unknown host key or a locked key's passphrase from the controlling terminal rather than stdin, so a
null stdin does not stop them, and a daemon launched from a shell has a terminal to block on. And a read is not
read-only — `git status` and `git diff` rewrite `.git/index` for a tracked file whose timestamps moved while its
content did not, and the `index.lock` they hold is what fails the `git add` an agent is running in that same
repository.

The deadline stays the call site's own: run the command through `subprocess::run_with_timeout`, or
`subprocess::run_bounded` when its output also needs a ceiling or it must be cancellable, rather than
`Command::output`. It kills the whole process group rather than the direct child, which is what also reaps the
`ssh` that `git` forked.

## A test that a `git` read leaves the repository alone needs a fixture whose stat information is stale

A tracked file whose modification time moved while its content did not is the only thing that makes a read want to
refresh the index. Against a freshly built repository the read writes nothing whatever its environment says, so a
test over that fixture passes with the guard removed as well, and proves nothing about the guard. The fixture has
to produce the stale state, and the test needs a positive control beside it — the same read with the guard off,
asserted to write. `apps/daemon/src/git_status.rs`'s
`reading_the_branch_header_leaves_the_repository_untouched`, with the control that follows it, is the worked pair.

## Run Linux-only code's tests in a Linux container, since no check on macOS compiles it

Applies to daemon code under `#[cfg(target_os = "linux")]` (the daemon is meant to run on a Linux host too) when you
are developing on macOS. `cargo check`, `cargo clippy` and the test suite on macOS skip that code entirely, so a clean
run says nothing about it, and `--target x86_64-unknown-linux-gnu` from macOS fails in rusqlite's `bundled` build
script without a Linux C cross-compiler. So run `cargo test -p octoboardd` inside a Linux container (Docker).

A stock Rust image on Debian bookworm fails some browse tests for reasons unrelated to the code, which reads as a
Linux bug in the daemon: its git (2.39) is too old, and at least one of them needs `git worktree add --orphan` (git
2.42 or later). Use an image whose git is new enough (Debian trixie's is). To exercise a fallback for a refused system call (the browse
reads' `openat2` refused as `ENOSYS` or `EPERM`), run the container with a seccomp profile that returns that errno for
the call.

Without a container runtime, the fallback is a compile check only: copy the Linux-only functions into a scratch crate
outside the repository that depends only on what they use (typically `libc`), and run
`cargo check --target x86_64-unknown-linux-gnu` there after `rustup target add x86_64-unknown-linux-gnu`; say when
reporting that it proves it compiles, not how it behaves.
