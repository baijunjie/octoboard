# Sharing the default configuration

An account can share its agent's default configuration directory instead of keeping a copy of its own. This module does
what a change of that choice does to the directories on disk. The wire contract is "Sharing the default configuration"
in [`PROTOCOL.md`](../../../PROTOCOL.md); the items that may be shared are the catalog in `items.rs`.

## How it works

Each account stores whether it shares and whether the last change toward that choice is incomplete. A change runs at
once or is refused: it takes the agent's lock, ends the account's idle sessions, moves or links the catalog items one by
one, then resumes those sessions, and reports its progress as `account_sharing`. A change that fails part-way leaves the
account marked incomplete, with the failing items, until it is tried again.

What lives where:

- This module holds what a change does to the directories, the locks it takes (`locks.rs`) and the failures it reports.
- Running a change as an operation (`set`, and `before_delete` and `before_repoint`, which turn sharing off before an
  account is deleted or pointed at another directory) is `apps/daemon/src/coordinator/account_sharing.rs`.
- What clients are told of each account's sharing is published by `AppState`
  (`apps/daemon/src/state/sharing_status.rs`), which reads what runs under an account's directory through `in_use`. The
  lock over an account's change lives on `AppState` (`lock_account_sharing`), under the same lock as the sessions'
  launch claims, so `begin_launch` refuses a session of a locked account.

Internals, by file: `link.rs` and `moves.rs` turn sharing on and off item by item and repair links an agent replaced
(with the file-move primitives: across volumes, backup naming, merging directories); `take_back.rs` moves the account's
own conversations back out of the default directory when sharing is turned off; `codex_state.rs` and `registries.rs`
keep Codex's thread state database and the plugin registries' recorded paths consistent with shared or unshared items;
`cli_reads.rs` is the admission of agent CLI runs outside a session against a running change.

## External interfaces

Exported from `mod.rs` (defined there unless a file is named):

- `lock_account`, `Locked`, `Refused`, `Changing`, `carry_out` (`change.rs`) — lock an account for a change of its
  sharing, keep its progress until the change ends, and carry the change out on the account's directory.
- `in_use`, `runs_under`, `default_dir`, `is_same_directory`, `lock_agent`, `try_lock_agent`, and the refusals
  `account_in_use`, `still_sharing`, `not_shareable`, `sharing_in_progress` and `sharing_failed` — what the
  coordinator's operation and the state layer need.
- `entries` (`link.rs`) — how many entries a change goes through.
- `own_conversations` (`take_back.rs`) — the conversations turning sharing off takes back.
- `FailureCause` and `FailedItem` (`link.rs`) — why an item failed, and which item.
- `before_launch` (`launch.rs`) — before a session launches or resumes: refuses while its account's change runs, and
  repairs links.
- `refuse_if_changing`, `before_switch_copy` — refuse opening a session, a handoff or an account switch while a change
  of the account involved runs.
- `Sharing` — what `AppState` holds: the per-agent locks.
- `admitted_reading`, `running_long`, `hold`, `is_missing`, `Run` (`cli_reads.rs`) — taken by agent CLI runs outside a
  session (a change, or the move of a removed account's directory to the Trash, waits for or refuses them), and by a
  change or an account's removal while it moves files under the directory, so that no CLI run starts there meanwhile.
