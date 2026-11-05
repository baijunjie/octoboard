# Agent-launch process ledger

`apps/daemon/src/processes/ledger/` owns attribution, persistence and cleanup of the processes an agent launch starts,
including descendants that leave the agent's terminal session. It reads the system's process table through
`apps/daemon/src/platform/proc_table/` (the macOS, Linux and Windows readers, shared with the project shell terminals'
busy check) and builds process trees for the request layer in `apps/daemon/src/coordinator/processes.rs`; the wire
interface belongs to [`apps/daemon/PROTOCOL.md`](../../../PROTOCOL.md).

The ledger is held on `AppState`. Launch setup enters through `apps/daemon/src/coordinator/launch.rs`; the carry-over of
a restart (an account switch, a model change, a change of an account's sharing) is driven by
`apps/daemon/src/coordinator/restart.rs`, which the account switch, model change and
`apps/daemon/src/coordinator/account_sharing.rs` run through; the handoff's carry-over goes through
`apps/daemon/src/coordinator/handoff.rs`, environment tagging through `apps/daemon/src/session/term/mod.rs`,
process-end cleanup and shutdown through `apps/daemon/src/coordinator/exit_watch.rs` and
`apps/daemon/src/state/process_exit.rs`, startup reconciliation through `apps/daemon/src/main.rs`, and the owner's moves
of trees through `apps/daemon/src/mcp/tools/transfer/processes.rs`.

Where things are: `mod.rs` the types, the launch lifecycle, reconciliation and `ForgetLaunch`, `attribute.rs` the rules
that attribute processes to launches, `scan.rs` the scan of the table, the scanner thread and the stop of what ended
launches left and what has worked in a removed directory, `looks.rs` the deadline-bounded looks at working directories
and project directories, `trees.rs` listings and stop, keep and move by tree, `carry.rs` carry-over between launches,
`persist.rs` what is kept on disk (`Persisted::from_book`, `read_back`), `stop.rs` stopping by identity and the rounds of
signals that follow what started meanwhile, and `table.rs` the table as the ledger reads it, over
`platform::proc_table`.

## External interfaces

- `ProcessLedger::new`, `ProcessLedger::start` — construct the ledger for a daemon's data directory and start its
  process-table scanner and reconciliation.
- `forget_inherited_tag` — remove a parent launch's tag from the daemon's own environment at startup.
- `ProcessLedger::begin_launch`, `PendingLaunch`, `ProcessTag`, `TAG_VARIABLE` — reserve and carry the attribution tag
  and the `platform::proc::Containment` used when launching an agent. Where the containment lists its members, the
  ledger attributes them to the launch; otherwise it finds them by the tag in their environment.
- `ProcessLedger::launch_started` — associate a reserved launch with its running agent process (a `platform::proc::ChildHandle`).
- `ProcessLedger::scan_before_ending`, `ProcessLedger::end_launch` — observe and clean up the processes of a launch
  being ended.
- `carry.rs`: `ProcessLedger::mark_carry_over`, `ProcessLedger::withdraw_carry_over`, `ProcessLedger::hold_for_launch`,
  `HeldForLaunch` — carry a launch's processes over to another session's launch when it ends, instead of stopping them
  (a handoff, an account switch, a change of a session's model, a change of an account's sharing), holding them while
  the receiving session has no launch running.
- `carry.rs`: `ProcessLedger::close`, `ProcessLedger::end_held_launches` — carry nothing over once the daemon is
  shutting down, and stop what was held.
- `ProcessLedger::move_trees`, `MoveRefused` — move chosen trees of one session's running launch to another's (the
  `transfer_processes` tool).
- `ProcessLedger::reconcile` — reconcile persisted attribution with the running process table.
- `ProcessLedger::trees_of_sessions`, `ProcessLedger::trees_of_project` — produce the process-tree
  listings consumed by session and project requests.
- `ProcessLedger::trees_still_running` — which of the trees a session's earlier listing named still run, for what a
  restart ended.
- `ProcessLedger::trees_counted` — count the trees of every project and console session, without
  describing them, for the request layer's global count.
- `ProcessLedger::listed_lately`, `ProcessLedger::note_listed` — share recent listings between client requests; a
  listing is noted only with the generation read before it.
- `ProcessLedger::counted_lately`, `ProcessLedger::note_counted`, `ProcessLedger::claim_counting`,
  `ProcessLedger::lists_generation` — share the global count between client requests: the last counts, the claim that
  makes overlapping requests wait for one computation, and the generation that keeps a read made before a stop, a keep
  or the end of a launch from being noted after it.
- `ProcessLedger::stop_trees`, `ProcessLedger::keep_trees` — apply the user's process-tree choices; a kept tree is
  recorded with the project it is listed under, and `trees_of_project` lists it among the project processes, the trees
  of the project no session owns (kept, and left behind). `stop_trees` answers `TreesStopped`: the trees it stopped,
  and the processes still running once it is through (the system refused to kill them, or they outlived the kill),
  each with the tree it was stopped with and why; `stop.rs` traces a process a round's scan finds back to the target of
  the first round it descends from, so a follow-up is told against the tree that was named. `keep_trees` refuses, with nothing kept, a request naming a
  tree with a process attached to its agent's own console (`SharesAgentConsole`), and the listings mark such a tree
  as not keepable.
- `ProcessLedger::project_processes`, `ProcessLedger::stop_project_processes` — list and stop a project's processes,
  for its removal.
- `carry.rs`: `HeldForLaunch::keep_what_is_held` — keep what a failed restart (an account switch, a model change, a
  change of an account's sharing) still holds for the session, under its project.

## Known limits

- The looks at a working directory or a project directory run as a pass bounded by a deadline (`looks.rs`), and one that
  outruns it is skipped while it is stuck. Only a directory whose kernel-reported text may lie under a project root is
  looked at; on macOS the report of the working directory is itself read in such a bounded pass, since it may stat the
  vnode.
- A process whose environment cannot be read (an Apple platform binary on macOS with SIP on, such as `/bin/sleep`)
  that a shell left running in the background is never attributed when that shell exits within one scan interval: its
  chain no longer reaches the launch, the shell's terminal session was never recorded, and it has no readable tag. It is
  neither listed nor stopped with the launch.
- A helper of a kept process that both double-forks and starts a terminal session of its own between two scans escapes
  the kept tree, and is stopped with the launch whose tag it carries.
- On Windows no process's environment is read, so no process is attributed by its tag: a launch's processes are found
  by their parent chain (rule 3 of `attribute.rs`) and by membership of the launch's job (rule 5). A child of a kept process
  whose parent exits within one scan interval loses the chain to the kept process, and the job it is still a member of
  pulls it back to the launch, so it is stopped with the launch instead of being kept.
- On Windows a process has no terminal session, so the rule that leaves out what sits in an agent's own session (rule 1)
  finds nothing: the agent's own internals (its MCP servers and other helpers it starts as children) reach the agent by
  their chain and are listed among the launch's process trees, where the user can see and stop them, and keep those
  not on the agent's own console (next item). Accepted as it is.
- On Windows the agent runs on a pseudoconsole, and ending its launch closes it, which ends every process attached to
  that console whatever keeps it. So a tree with a process attached to the agent's own console
  (`platform::proc::shares_console`, compared with the launch's `agent`) is listed as not keepable
  (`unkeepable_reason` `agent_console`), and a keep naming it is refused; a process whose console cannot be told is
  taken not to share it. Only what the user starts on a console of its own can be kept from a launch there.
- On Windows a Store `pwsh` that an agent's own tool starts (an app-execution alias, which the agent, not the daemon,
  starts) leaves the launch's job with everything it runs: such a process gets a job of its own on start. Its tree is
  attributed to the launch only through its parent chain (rule 3 of `attribute.rs`), once a scan has seen the chain
  reach the agent; attribution then sticks after the chain breaks. An attributed tree is stopped with the launch
  (`ProcessLedger::end_launch` stops everything attributed, not only the job's members) and, after a daemon crash, by
  the next start's `ProcessLedger::reconcile`, which stops the attributed processes the saved records name. Only the
  in-process crash kill (`platform::proc::crash`) is limited to the agent itself and the job's members on the agent's
  console, so it does not reach such a tree. A tree whose chain to the agent broke before any scan saw it belongs to no
  launch: it is neither listed nor stopped. Accepted as it is: which PowerShell runs a tool call is the agent's choice.
