# Session git hooks

How the daemon learns which session's git created a branch or moved a ref, and how a push or branch move is run past the
revert gate. Applies to project sessions only.

## How it works

- The daemon writes a directory of hook dispatchers under its data directory: one copy of `dispatcher.sh` per hook name,
  named by a hash of its content and left read-only, shared by every session. A project session's agent is launched with
  `core.hooksPath` pointed at it through `GIT_CONFIG_COUNT` variables on the agent's process alone; nothing else (no
  repository, no configuration file) is touched, and the user's own `GIT_CONFIG_*` entries keep their place.
- Each dispatcher first runs the hook the user's own configuration names. A `reference-transaction` then reports created
  branches, and whether `HEAD`, a local branch or a remote-tracking branch moved, by starting `octoboardd git-ref` in
  the background (the daemon re-reads the repository's projects, `apps/daemon/src/project_git/ref_moves.rs`, and records
  branch origins, `apps/daemon/src/branch_origin/`). A prepared transaction that moves an existing branch, and
  `pre-push`, start `octoboardd gate-hook` in the foreground; only its exit status 3 refuses the update.
- `gate-hook` asks the daemon at run time (`POST /revert-gate/:session`, answered by `gate_inputs.rs`) whether the gate
  applies and with which branches, then runs the gate in-process (`apps/daemon/src/revert_gate/`). Anything but an
  answer lets git through, so a daemon that is down never blocks the user's git.
- Nothing is injected when the user's `git` is older than `MIN_GIT`, `GIT_CONFIG_COUNT` is unusable, or the directory
  cannot be written; the session then works as it would without Octoboard.

Files: `mod.rs` the dispatcher directory and the variables, `dispatcher.sh` the script, `ref_mode.rs` and `gate_mode.rs`
the two CLI modes, `gate_inputs.rs` the daemon side of the gate's inputs, `fixture.rs` and `tests*` test support.

## External interfaces

- The directory of dispatchers and the environment that points a session's agent at it (`mod.rs`).
- `octoboardd git-ref` (`ref_mode.rs`) — posts created branches and moved refs to `POST /git-ref/:session`.
- `octoboardd gate-hook` (`gate_mode.rs`) — asks for the gate's inputs, runs the gate, exits with its status.
- `gate_inputs.rs` — answers `POST /revert-gate/:session`.
