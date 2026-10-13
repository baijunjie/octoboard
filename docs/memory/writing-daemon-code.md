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

## A store write outside a transaction is a journalled pair of fsyncs under the one lock

`Store::open` (`apps/daemon/src/store.rs`) sets only `foreign_keys`: `journal_mode` and `synchronous` keep SQLite's
defaults, and the store is a single `Connection` behind a mutex that every method takes. So each statement run outside
a transaction commits on its own — a rollback journal written and fsynced, the pages written and fsynced, the journal
removed — while every other caller of the store waits on the mutex for it. A loop calling a one-row method is
therefore the wrong shape wherever the number of rows follows a repository, a listing or anything else rather than
the single thing the user did: reconciling a repository's branch records writes up to a thousand rows, from a hook
callback and from inside a browse slot.

So a store method that can write more than one row opens one `conn.transaction()` for the lot. When those writes
follow from a read of the same rows, or the caller needs what the rows hold afterwards, do both reads inside that
transaction instead of around the call: a method of this kind is reached from a hook callback and from the repeating
refresh each connected client runs on its own phase, so a read-modify-write split across two takes of the mutex gets
another caller's writes in between and one of the two is lost. `Store::apply_branch_origins` is the shape to copy.

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

## Cap how many blocking threads a per-entity pass starts at once

Applies to daemon work that runs one blocking task per project, per directory or per session — a sweep over a console's
projects, a reconciliation at daemon start, a read of every directory under a parent being associated. One
`tokio::task::spawn_blocking` per item is wrong however cheap the item looks: that pool is one pool for the whole
daemon, and every user-visible path shares it — opening a session, answering a trust prompt, a browse read — so
associating a parent directory that holds a few hundred repositories fills it and the user's next action waits behind
the slowest item. Such a task usually also waits on `env_shell::cached_snapshot()`'s single lock, held for a whole
login-shell fork while the cache is cold, so the threads past the first few are not even doing work in parallel.

So run the items through a bounded runner instead — a `JoinSet` held at a fixed width, starting the next item as soon
as any one finishes — and give each item a deadline of its own, so one that stalls holds a single place rather than
the pass. Four at a time is this daemon's figure everywhere it does this.

## Build every `git` the daemon spawns from `git_env.rs`, and give it a deadline

`apps/daemon/src/git_env.rs` assembles every `git` command the daemon runs: `non_interactive` is the base, and a
command that only reads a repository adds `read_only` on top of it, before the subcommand is pushed, since it
contributes a `-c` argument. Start from there rather than from `Command::new`, and set none of those variables at a
call site; the module header carries the reason for each one, and a call site that assembles its own takes some of
them and misses the rest.

The environment is part of that, for a command that writes as much as for one that reads: `non_interactive` takes it
whole, so a snapshot of the user's login shell is passed through `git_env::user_configuration_only` on the way in. That
filter keeps what says which of the user's own material to use and how to reach a remote, and drops anything that can
change which repository a command looks at, set an arbitrary configuration item, or weaken verification — and both
sides of that line fail where the user can see it. Too wide, and a `GIT_DIR` out of the user's rc file points the
command at another repository: what a read returns lands as persistent wrong data wherever it is stored and broadcast
rather than shown for a moment, and a fast-forward merge writes files into a worktree that is not the project's. Too
narrow, and a command that works from the user's own shell fails here for good — a fetch missing a certificate or
proxy setting the user exported leaves its error in the project's status on every later check, with nothing in the
interface pointing at the cause.

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

## Name a diff's two versions `<revision>:<path>`, and end the arguments with `--`

Applies wherever daemon code wants the diff `git` computes between two specific versions of a path, rather than a diff
over pathspecs. Write each operand as `<revision>:<path>` — a commit, or `:0` for the index — even when the two object
ids are already in hand, and close the argument list with a `--`.

`git` looks a path's attributes up by its path, so two bare object ids leave it none: a path the repository marks
`-diff` is diffed as text instead of reported as `Binary files … differ`, and the section's `a/` and `b/` names come
out as the object ids. Those attributes are the user's, and losing them silently changes what the client is shown.
The `--` matters because an operand of that form is also a valid path, so a worktree file happening to be named like
one makes `git` refuse the whole command as an ambiguous argument — a condition that depends on what the user has on
disk, which no fixture without such a file reproduces.

## A test that a `git` read leaves the repository alone needs a fixture whose stat information is stale

A tracked file whose modification time moved while its content did not is the only thing that makes a read want to
refresh the index. Against a freshly built repository the read writes nothing whatever its environment says, so a test
over that fixture passes with the guard removed as well, and proves nothing about the guard. The fixture has to produce
the stale state, and the test needs a positive control beside it — the same read with the guard off, asserted to write.
`apps/daemon/src/git_status.rs`'s `reading_the_branch_header_leaves_the_repository_untouched`, with the control that
follows it, is the worked pair. Build both halves from `apps/daemon/src/test_support.rs` — `move_modification_time_back`
for the stale stat and `git_dir_state` for the before-and-after comparison — rather than a copy of your own.

The backdating goes after every `git` the fixture runs: `commit` and `checkout` refresh the whole index, so a step
placed before them is recorded as the index entry's own stat data and leaves the fixture not stale at all. A fixture
ordered that way still passes, because its last write leaves the index's entries racily clean and git re-verifies and
rewrites the index on the next read regardless — which satisfies the control while the read under test never meets a
stale stat, and stops happening once a parallel suite's load widens the gap between the worktree write and git's own
index write, so the test turns intermittent. Check the fixture by deleting the backdating step: the control has to fail
without it.

## Lint the Rust crates with `cargo clippy --all-targets`

Plain `cargo clippy` checks only the lib and bin targets, and every test in these crates sits in a `#[cfg(test)]`
module (there is no `tests/` directory), so it lints none of the test code — and most changes here add more test code
than production code. A clean run without `--all-targets` is therefore no evidence about the tests just written, and a
warning in them surfaces much later, whenever something else makes that target rebuild. Do not read that delay as
cargo withholding cached warnings: warnings are replayed for an unchanged target, so a second run does repeat them,
and neither `cargo clean` nor touching a file is the fix.

## Run Linux-only code's tests in a Linux container, since no check on macOS compiles it

Applies to daemon code under `#[cfg(target_os = "linux")]` (the daemon is meant to run on a Linux host too) when you
are developing on macOS. `cargo check`, `cargo clippy` and the test suite on macOS skip that code entirely, so a clean
run says nothing about it, and `--target x86_64-unknown-linux-gnu` from macOS fails in rusqlite's `bundled` build
script without a Linux C cross-compiler. So run `cargo test -p octoboardd` inside a Linux container (Docker).

To exercise a fallback for a refused system call (the browse reads' `openat2` refused as `ENOSYS` or `EPERM`), run
the container with a seccomp profile that returns that errno for the call.

Without a container runtime, the fallback is a compile check only: copy the Linux-only functions into a scratch crate
outside the repository that depends only on what they use (typically `libc`), and run
`cargo check --target x86_64-unknown-linux-gnu` there after `rustup target add x86_64-unknown-linux-gnu`; say when
reporting that it proves it compiles, not how it behaves.

## Build daemon test fixtures from Git 2.36's commands

The daemon supports Git 2.36 and later (`apps/daemon/PROTOCOL.md`, "Browse"), but the Mac it is developed on has a
newer git, so a fixture that needs a newer one passes every local run and fails only on an older host. The Debian
bookworm Rust image (git 2.39) is the oldest run available, and it checks 2.39, not the floor. So a fixture uses only
commands and options Git 2.36 has: an empty-branch worktree, for one, is `worktree add --detach` then
`checkout --orphan`, not `worktree add --orphan` (2.42).

A test of behavior that only a newer git has — telling a broken branch from an unborn one needs `show-ref --exists`,
2.43 — runs that part only where `test_support::git_has_show_ref_exists` says the git has it, since the older git's
documented fallback gives a different answer.
