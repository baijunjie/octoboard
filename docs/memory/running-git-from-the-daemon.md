# Running git from the daemon

The rules (every `git` through `git/run.rs` with a family `Runner`, the pace, remote commands with an
`SshConfiguration`, the deadline, browse decorations, `--ignore-submodules`, `<revision>:<path>` operands, full ref
names, one Unicode form, a `-c` in a write `Policy`) are in `docs/conventions/daemon-processes-and-io.md`, "Git from the
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
