# Running git from the daemon

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

## A `git` that only adds to a browse reply gets a short deadline and a budget of its own, and failing leaves it out

Applies to a `git` run that decorates a browse reply the daemon has already read — which of a listing's entries are
ignored, a change listing's line counts — and above all to one behind a reply the client asks for again while it is on
screen (a folder in view in the Files tab is relisted every few seconds). Give it a deadline of a few seconds defined in
`apps/daemon/src/browse/budget.rs`, as `LINE_COUNT_TIMEOUT` and `IGNORE_CHECK_TIMEOUT` are, never `GIT_READ_TIMEOUT`'s
30 seconds, and take a run that fails, overruns or prints past its ceiling as "no addition" rather than as a failed
reply; where the same answer is asked for again and again, remember it while what decides it is unchanged rather than
start a `git` each time. Every buffer the run holds — its stdout ceiling, what is built from it — is a constant in that
file and is added into the reply's reservation there, or the per-connection retained-bytes bound no longer covers the
reply (`apps/daemon/PROTOCOL.md`, "Browse budgets").

## A `git` that compares with the worktree passes `--ignore-submodules`

Applies to a daemon `git status`, or a `git diff` without `--cached`, especially on a path that repeats — the Git
mode's change list refreshes every 10 seconds while it is on screen, and a sweep runs per project. Without the flag,
`git` checks every checked-out submodule for modified content and untracked files by running a `git status` inside
it, so each submodule adds a whole status to every run, and no fixture without submodules shows the cost. Pass
`--ignore-submodules=dirty` where a submodule moved to another commit still has to show (the change listing's
`git status`), and `--ignore-submodules=all` where submodules are not wanted at all (the line counts' `git diff`).

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

## Name a ref to `git` by its full name or `@{upstream}`, never by a short name

Applies wherever daemon code passes a branch, an upstream or a remote-tracking branch back to `git` as a revision
(`merge`, `rev-list`, `log`, `rev-parse`). `git` resolves a short name by trying `$GIT_DIR/<name>`, `refs/<name>`,
`refs/tags/<name>`, `refs/heads/<name>` and `refs/remotes/<name>` in that order, so a tag named like a branch, or a
local branch named `origin/main`, takes the name, and the command quietly acts on that other commit: a fast-forward
onto `origin/main` lands on the local branch. Anyone working in the repository, an agent included, can create such a
ref. Write `refs/heads/<branch>` or `refs/remotes/<remote>/<branch>`, take an upstream from `%(upstream)`, which prints
it in full, or name it `@{upstream}`; a short name such as the branch header's `upstream` is for showing.

## Compare the paths `git` prints with names read from disk only in one Unicode form

Applies to daemon code on macOS that matches paths a `git` prints against names it read from the filesystem itself — a
listing's entries, a search's traversal. There `git` normally runs with `core.precomposeUnicode` on and prints a name
stored decomposed (as macOS applications commonly write an accented name) precomposed, while the directory read returns
the bytes as stored, so a byte comparison of the two misses exactly those names. Settle the form before writing the
comparison: run that `git` with `-c core.precomposeUnicode=false`, so it prints names as stored — what the Files tab's
ignored lookups do, at the cost `apps/daemon/PROTOCOL.md`'s "Ignored entries" gives — or normalise both sides. Linux
never shows the mismatch, and on macOS only a fixture holding a name stored decomposed does.

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

## Build daemon test fixtures from Git 2.36's commands

The daemon supports Git 2.36 and later (`apps/daemon/PROTOCOL.md`, "Git invocation"), but the Mac it is developed on
has a newer git, so a fixture that needs a newer one passes every local run and fails only on an older host. The Debian
bookworm Rust image (git 2.39) is the oldest run available, and it checks 2.39, not the floor. So a fixture uses only
commands and options Git 2.36 has: an empty-branch worktree, for one, is `worktree add --detach` then
`checkout --orphan`, not `worktree add --orphan` (2.42).

A test of behavior that only a newer git has — telling a broken branch from an unborn one needs `show-ref --exists`,
2.43 — runs that part only where `test_support::git_has_show_ref_exists` says the git has it, since the older git's
documented fallback gives a different answer.
