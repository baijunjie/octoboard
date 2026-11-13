# Session git hooks

How the daemon learns which session's git created a branch or moved a ref, and how a push or branch move is run past the
revert gate. Applies to project sessions only.

## How it works

- The daemon writes a directory of hook dispatchers under its data directory: one copy of `dispatcher.sh` per hook name,
  named by a hash of its content and left read-only, shared by every session. A project session's agent is launched with
  `core.hooksPath` pointed at it through `GIT_CONFIG_PARAMETERS`, the variable `git -c` uses, on the agent's process
  alone: the entry goes after the user's own entries there, takes precedence over the user's `GIT_CONFIG_COUNT` entries
  (left untouched), and yields to a `git -c core.hooksPath=…`. Nothing else (no repository, no configuration file) is
  touched. Not `GIT_CONFIG_COUNT`: an agent that strips variables named like secrets (`*KEY*`, `*SECRET*`, `*TOKEN*`;
  Codex's shell environment policy does where a user has turned its default excludes on, with
  `ignore_default_excludes = false`) would take `GIT_CONFIG_KEY_<n>` and leave the count, which fails every git; no
  variable set here is named so.
- Each dispatcher first takes Octoboard's own entry out of the environment, so the user's hook and every git it runs see
  the user's configuration alone, then runs the hook the user's own configuration names. A `reference-transaction` then
  reports created branches, and whether `HEAD`, a local branch or a remote-tracking branch moved, by starting
  `octoboardd git-ref` in the background (the daemon re-reads the repository's projects,
  `apps/daemon/src/project_git/ref_moves.rs`, and records branch origins, `apps/daemon/src/branch_origin/`). A prepared
  transaction that moves an existing branch, and `pre-push`, start `octoboardd gate-hook` in the foreground; only its
  exit status 3 refuses the update.
- `gate-hook` asks the daemon at run time (`POST /revert-gate/:session`, answered by `gate_inputs.rs`) whether the gate
  applies and with which branches, then runs the gate in-process (`apps/daemon/src/revert_gate/`). The reply can carry,
  instead of the inputs, the verdict that the repository belongs to another registered project than the session's
  (`apps/daemon/src/cross_project_guard/repository.rs`, decided first, while the setting that keeps sessions to their own
  project is on): at a prepared `reference-transaction` the hook refuses it with exit status 3, and it asks the daemon
  for it even under `REVERT_GATE_SKIP`, which skips only the gate. Anything but an answer lets git through, so a daemon
  that is down never blocks the user's git.
- The two modes are given no key. They first take the launch's route (`route.rs`): the port in
  `OCTOBOARD_DAEMON_PORT`, with the launch key from the owner-only key file that `OCTOBOARD_DAEMON_CREDENTIAL_PATH`
  names (the key itself is never in the environment). Where that route is gone — no port, a key file missing, unreadable
  or empty, a 401 or 403, nothing listening — they go to the daemon its owner record names, with the key its key record
  holds, as `octoboardd address` finds it; never on a holder's 503 or 502 or a timeout. Both records are read from the
  data directory the dispatcher is under (`<data>/githooks/<hash>/`), which it passes as `--data-dir` (without it, the
  helper's own: the home in its environment comes from the session's shell and may be another daemon's). So a job a
  session left running after it ended still reports and is gated. A dispatcher treats a git whose environment has no
  `OCTOBOARD_SESSION_ID` as not a session's (no report, no gate).
- The dispatchers start the installation's own entry point (`AppState::install_exe`), which an update replaces in place,
  not the copy of the build a holder runs from, which is removed once no holder runs from it. A newer build's modes may
  therefore serve dispatchers an older one wrote: their arguments and variables stay accepted across versions, a command
  line without a later argument (`--data-dir`) included.
- Nothing is injected when the user's `git` is older than `MIN_GIT`, the user's `GIT_CONFIG_PARAMETERS` is one git
  rejects, or the directory cannot be written; the session then works as it would without Octoboard.

Files: `mod.rs` the dispatcher directory and the variables, `dispatcher.sh` the script, `ref_mode.rs` and `gate_mode.rs`
the two CLI modes, `route.rs` the route they take to the daemon, `gate_inputs.rs` the daemon side of the gate's inputs,
`fixture.rs` and `tests*` test support.

## External interfaces

- The directory of dispatchers and the environment that points a session's agent at it (`mod.rs`).
- `octoboardd git-ref` (`ref_mode.rs`) — posts created branches and moved refs to `POST /git-ref/:session`.
- `octoboardd gate-hook` (`gate_mode.rs`) — asks for the gate's inputs and the verdict on the repository, refuses a foreign repository, runs the gate, exits with its status.
- `gate_inputs.rs` — answers `POST /revert-gate/:session` with the gate's inputs or the verdict on the repository.
