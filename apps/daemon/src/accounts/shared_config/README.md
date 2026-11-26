# Sharing the default configuration

An account can share its agent's default configuration directory instead of keeping a copy of its own. This module does
what a change of that choice does to the directories on disk. The wire contract is "Sharing the default configuration"
in [`PROTOCOL.md`](../../../PROTOCOL.md); the items that may be shared are each provider's own catalog
(`AccountOps::shared_items`, in `adapter/<provider>/shared_config.rs`), the shape of an item being `items.rs`'s.

## How it works

Each account stores whether it shares and whether the last change toward that choice is incomplete. A change runs at
once or is refused: it takes the agent's lock, ends the account's idle sessions, moves or links the catalog items one by
one, then resumes those sessions, and reports its progress as `account_sharing`. A change that fails part-way leaves the
account marked incomplete, with the failing items, until it is tried again. Once the daemon is stopping no change begins
on disk, and the stop waits for one already at work there (`on_disk.rs`), so that no stop leaves an entry half-changed.

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
own conversations back out of the default directory when sharing is turned off; `registries.rs` keeps the plugin
registries' recorded paths consistent with shared or unshared items, and `keep.rs` keeps the default directory free of
paths through a sharing account's directory (the registries pointed back after each of the account's sessions, before
its launches, at stop and at start), and a provider's thread database is kept consistent through its `ThreadRegistry`
(`apps/daemon/src/accounts/thread_registry.rs`; Codex's is `adapter/codex/threads.rs`), which the account switch's copy
uses too; `cli_reads.rs` is the admission of agent CLI runs outside a session against a running change. `link.rs` also
holds the constructors of `SharingCause` and `SharingItemFailure` (defined in `protocol/accounts.rs`).

## External interfaces

Exported from `mod.rs` (defined there unless a file is named):

- `lock_account`, `Locked`, `Refused`, `Changing`, `carry_out` (`change.rs`) — lock an account for a change of its
  sharing, keep its progress until the change ends, and carry the change out on the account's directory.
- `in_use`, `runs_under`, `default_dir`, `lock_agent`, `try_lock_agent`, and the refusals `account_in_use`,
  `still_sharing`, `not_shareable`, `sharing_in_progress` and `sharing_failed` — what the coordinator's operation and
  the state layer need.
- `entries` (`link.rs`) — how many entries a change goes through.
- `own_conversations` (`take_back.rs`) — the conversations turning sharing off takes back.
- `before_launch` (`launch.rs`) — before a session launches or resumes: refuses while its account's change runs, and
  repairs links.
- `keep_registries_after_exit`, `keep_registries_at_stop`, `keep_registries_at_start` (`keep.rs`) — point the plugin
  registries back at the default directory after a sharing account's session ends, as the daemon stops and as it starts.
- `refuse_if_changing`, `before_switch_copy` — refuse opening a session, a handoff or an account switch while a change
  of the account involved runs.
- `Sharing` — what `AppState` holds: the per-agent locks and the changes at work on disk.
- `begin_on_disk`, `close_changes_on_disk`, `changes_on_disk_ended` (`on_disk.rs`) — a change's admission to the
  account's directory, refused once the daemon is stopping; and the daemon's stop closing that admission and waiting for
  the changes already at work there.
- `admitted_reading`, `running_long`, `hold`, `is_missing`, `Run` (`cli_reads.rs`) — taken by agent CLI runs outside a
  session (a change, or the move of a removed account's directory to the Trash, waits for or refuses them), and by a
  change or an account's removal while it moves files under the directory, so that no CLI run starts there meanwhile.
