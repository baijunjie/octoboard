# Agent-launch process ledger

`apps/daemon/src/process_ledger/` owns attribution, persistence and cleanup of the processes an agent launch starts,
including descendants that leave the agent's terminal session. It reads the native macOS or Linux process table and
builds process trees for the request layer in `apps/daemon/src/process_requests.rs`; the wire interface belongs to
[`apps/daemon/PROTOCOL.md`](../../PROTOCOL.md).

The ledger is held on `AppState`. Launch setup and the carry-over of a restart (an account switch, a model change, a
change of an account's sharing) enter through `apps/daemon/src/coordinator.rs` and
`apps/daemon/src/shared_config/change.rs`, the handoff's carry-over through `apps/daemon/src/handoff/operation.rs`,
environment tagging through `apps/daemon/src/term.rs`, process-end cleanup and shutdown through
`apps/daemon/src/state.rs`, startup reconciliation through `apps/daemon/src/main.rs`, and the owner's moves of trees
through `apps/daemon/src/mcp/process_handover.rs`.

Where things are: `mod.rs` the ledger and launch lifecycle, `attribute.rs` the rules that attribute processes to launches,
`trees.rs` listings and stop, keep and move by tree, `carry.rs` carry-over between launches, `persist.rs` what is kept on
disk, `stop.rs` stopping by identity, and `table.rs` with `table/macos.rs` and `table/linux.rs` the process-table readers.

## External interfaces

- `ProcessLedger::new`, `ProcessLedger::start` — construct the ledger for a daemon's data directory and start its
  process-table scanner and reconciliation.
- `forget_inherited_tag` — remove a parent launch's tag from the daemon's own environment at startup.
- `ProcessLedger::begin_launch`, `PendingLaunch`, `ProcessTag`, `TAG_VARIABLE` — reserve and carry the attribution tag
  used when launching an agent.
- `ProcessLedger::launch_started` — associate a reserved launch with its running agent process.
- `ProcessLedger::scan_before_ending`, `ProcessLedger::end_launch` — observe and clean up the processes of a launch
  being ended.
- `carry.rs`: `ProcessLedger::hand_over`, `ProcessLedger::withdraw_hand_over`, `ProcessLedger::hold_for_launch`,
  `HeldForLaunch` — carry a launch's processes over to another session's launch when it ends, instead of stopping them
  (a handoff, an account switch, a change of a session's model, a change of an account's sharing), holding them while
  the receiving session has no launch running.
- `carry.rs`: `ProcessLedger::close`, `ProcessLedger::end_held_launches` — carry nothing over once the daemon is
  shutting down, and stop what was held.
- `ProcessLedger::move_trees`, `MoveRefused` — move chosen trees of one session's running launch to another's (the
  `handover_processes` tool).
- `ProcessLedger::reconcile` — reconcile persisted attribution with the running process table.
- `ProcessLedger::trees_of_sessions`, `ProcessLedger::trees_of_project`, `ProcessTree` — produce the process-tree
  listings consumed by session and project requests.
- `ProcessLedger::listed_lately`, `ProcessLedger::note_listed` — share recent listings between client requests.
- `ProcessLedger::stop_trees`, `ProcessLedger::keep_trees` — apply the user's process-tree choices; a kept tree is
  recorded with the project it is listed under, and `trees_of_project` lists it among the project processes, the trees
  of the project no session owns (kept, and left behind).
- `ProcessLedger::project_processes`, `ProcessLedger::stop_project_processes` — list and stop a project's processes, for
  its removal.
- `carry.rs`: `HeldForLaunch::keep_what_is_held` — keep what a failed restart (an account switch, a model change, a
  change of an account's sharing) still holds for the session, under its project.

## Known limits

- A process whose environment cannot be read (an Apple platform binary on macOS, such as `/bin/sleep`) that a shell
  left running in the background is never attributed when that shell exits within one scan interval: its chain no
  longer reaches the launch, the shell's terminal session was never recorded, and it has no readable tag. It is neither
  listed nor stopped with the launch.
- A helper of a kept process that both double-forks and starts a terminal session of its own between two scans escapes
  the kept tree, and is stopped with the launch whose tag it carries.
