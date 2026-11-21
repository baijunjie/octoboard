# Running git from the daemon

The rules (every `git` through `git/run.rs` with a family `Runner`, the pace, remote commands with an
`SshConfiguration`, the deadline, browse decorations, `--ignore-submodules`, `<revision>:<path>` operands, full ref
names, one Unicode form) are in `docs/conventions/daemon-processes-and-io.md`, "Git from the
daemon"; the fixture rules (stale stat with a positive control, Git 2.36 commands) in `docs/conventions/tests.md`,
"Daemon tests". What they leave out:

## A status-only `git diff --quiet` runs through `read`

A command that only reads the repository runs through `read`, a status-only `git diff --quiet` included: `read` adds the
index guard.

## Symptoms of a `git` run outside `git::run` turn up far from the call

- A daemon that hangs with nothing on screen saying which call is stuck: `git` and `ssh` read a missing credential, an
  unknown host key or a locked key's passphrase from the controlling terminal, not stdin, so a null stdin does not stop
  them, and a daemon launched from a shell has a terminal to block on.
- An agent's `git add` failing on `index.lock`: `git status` and `git diff` rewrite `.git/index` for a tracked file
  whose timestamps moved while its content did not, so a read is not read-only.

## Check git fixtures by removing what they depend on

- A stale-stat fixture: delete the backdating step and confirm the positive control fails. Backdating placed before a
  `commit` or `checkout` the fixture runs is undone by their index refresh, yet the test still passes alone (the index
  is left racily clean and re-verified anyway) and turns intermittent under a parallel suite's load.
  `reading_the_branch_header_leaves_the_repository_untouched` in `apps/daemon/src/project_git/header.rs`, with the
  control after it, is the worked pair.
- A comparison of paths `git` prints with names read from disk: Linux never shows the mismatch, and on macOS only a
  fixture holding a name stored decomposed does.

## A git write made for the user pins its outcome, not how their own git shapes it

The user's configuration reaches every `git` the daemon writes with. Pin on the command line each option that config
could turn into another outcome than the one the person asked for: for a merge commit `--commit --no-ff --no-squash`
(against `merge.ff` and `branch.<name>.mergeOptions`) and `--no-log` where the message already holds `merge.log`'s
shortlog. Leave alone what only shapes the result as their own run of that command would: `commit.cleanup` and the
hooks. Never `--cleanup=strip` to drop lines of the daemon's own: it also strips what a `prepare-commit-msg` hook wrote
(a hook prepending `#42 ` fails the merge with "Empty commit message"); filter the daemon's input instead. A commit the
daemon makes is compared in a test with what the plain command writes in the same repository, as
`apps/daemon/src/project_git/pull/tests.rs` does for the manual pull's merge.

## Abort only a git operation the daemon can prove it started

Agents and the user run `git` in the same working tree, so a merge (or rebase, cherry-pick) under way after a daemon
write failed may be theirs. Abort only when the operation's state names exactly what the daemon started (`MERGE_HEAD`
equal to the commit it merged), and leave the tree alone when that state cannot be read. Refuse to start when an
operation is already under way. This needs the operation started on a resolved object id, not `@{upstream}` or a branch
name; a merge of an object id is titled `Merge commit '<oid>'`, so take the message a plain merge of the branch would
write from `git fmt-merge-msg`, as `apps/daemon/src/project_git/pull.rs` does, never hand-written.
