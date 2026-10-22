# `octoboardd`

A headless Rust binary that plays two roles in one process:

- **Host role**: owns PTYs, agent processes and the plain shell terminals of projects, and receives the agents' hook
  callbacks; works on the machine and its
  repositories on behalf of clients (directory listings, repository discovery, clone, remote probing, git status,
  push, branch switch, restore, opening a project in the system terminal or file manager, agent and model detection,
  reading each account's plan quota);
  and serves bounded, source-aware reads of a project's files and changes.
- **Coordinator role**: stores consoles, projects, sessions, report panel pages and the agent accounts in SQLite,
  routes requests to the host role, and carries the stored configuration between devices as a document it exports and
  reads back.

The desktop application (`apps/desktop/`) is purely a client of this process; everything it can do goes through the
external interface below.

## Development

This crate is a member of the repository's root Cargo workspace alongside the desktop shell's Tauri crate (shared root
`Cargo.lock` and `target/`). `cargo build` and `cargo test` from the repository root cover both crates; add
`-p octoboardd` to narrow either to this one. The Tauri crate resolves its `octoboardd` sidecar at compile time, so on
a fresh checkout a workspace-wide build or test fails until `pnpm --filter @octoboard/desktop build:daemon` has run
once.

## External interfaces

- WebSocket/HTTP server, bound to `127.0.0.1` on an OS-assigned port — the only way a client talks to the daemon.
  Specified in [`PROTOCOL.md`](PROTOCOL.md), which is authoritative.
- `octoboardd hook --session <id> --port <port>` — CLI mode run by the per-session hook script for agents whose hooks
  use the command form; posts the payload from stdin to the running daemon's `POST /hook/:session`.
- `octoboardd git-ref --session <id> --port <port> -- <ref> <oid>...` — CLI mode started by a project session's
  `reference-transaction` git hook when branches were created; posts them to `POST /git-ref/:session`.
- `octoboardd status-line --session <id> --port <port> --cwd <dir> --config-dir <dir>` — CLI mode that is the status
  line the Claude Code adapter injects; posts the context window from stdin to `POST /context-window/:session` and
  runs the user's own status line with the same input, passing its output and exit status on.
- `octoboardd mcp --session <id> --role <role> [--bound] --port <port> --token <token>` — CLI mode that is the MCP
  server each adapter registers for a session; speaks MCP on stdin/stdout and forwards tool calls to
  `POST /mcp/:token`.

## Layout

| Path | Role |
|---|---|
| `build.rs` | Exposes the app name from the repo-root `config/app.json` as `OCTOBOARD_APP_NAME` |
| `src/main.rs` | CLI entry point: parses the five modes, opens the store, binds the port, prints the handshake line the application waits for, runs the server |
| `src/server.rs` | The HTTP/WebSocket router described in `PROTOCOL.md`, including `POST /mcp/:token`, `POST /git-ref/:session`, `POST /context-window/:session` and `POST /hook/:session` |
| `src/access.rs` | The middleware every route sits behind: refuses with `403` a request whose `Host` or `Origin` is not a local client's or the application's own UI's |
| `src/protocol.rs` | Rust types for the wire protocol (kept in sync by hand with `PROTOCOL.md` and `packages/ui/src/protocol.ts`), the console session colours, and the one mapping from a failure to an `error` reply |
| `src/coordinator.rs` | Coordinator role: what each control-socket request does to the stored consoles, projects, sessions, pages and accounts, and which host-role work it triggers (session open/launch, binding, transfer and release, with the owner-change notice a session not running is owed delivered at its next launch or resume, archive/reopen/delete cascades, the resumability check (`check_can_resume`, also what `request_resume_session` refuses with before asking) and `codex unarchive` before a reopen launches, `locate_conversation` (where a conversation is held, from the sessions that record it or the agents' own records: what the new-session dialog's Resume tab and `request_resume_session` both ask), account resolution, the added agents list (`add_agent`, `remove_agent`, `reorder_agents`), each agent's default account's name (`set_default_account_name`), starting and cancelling an account's login (`start_account_login`, `cancel_account_login`), an account's choice to share the default account's configuration (`set_account_sharing`, carried out through `shared_config/`), launch settings, the per-account and default-account launch override of each agent's approval handling — Codex's reviewer, Claude Code's permission mode — resolved at every launch, refusing to open or switch a session onto an account whose quota is used up (`refuse_if_account_quota_exhausted`), console and project association and edits, and the shell terminal requests, with a project's or console's deletion ending its shells) |
| `src/session_request/` | A session asking the user to approve something, for `request_session` (a new session with no owner) and `request_resume_session` (resuming a conversation: bringing back the session that holds it, or opening one on an agent's own record of it): `mod.rs` keeps the pending requests, each carrying its action (`Action`), the wait for the answer and the carrying out of an approval; `outcome.rs` holds what the caller is told of an approved request, in the call's result and in the message written into its session; `tests.rs` holds the tests |
| `src/reporting.rs` | The channel between an owner and the sessions bound to it: briefs, messages into a running session, reports reaching the owner or synthesised when a session stops, automatic archiving, and report-panel form submissions |
| `src/sharing.rs` | Information one session shares with another, delivered through `reporting.rs`'s message writing, and the wording helpers that name a session and its place; `reports_to` (`ReportsTo`) tells whom a session reports to |
| `src/archive_cleanup.rs` | The daemon task that deletes archived sessions whose conversation the agent can no longer resume (`spawn`, started from `main.rs`): it scans at start and then whenever a day of wall-clock time has passed, judges each with `agent_records.rs`, and deletes only the proven-unresumable ones, whole groups included, through the coordinator's delete path, trashing leftover agent records and removing branches and worktrees with only log output |
| `src/relocate.rs` | Copying a session's conversation record, with what the agent keeps beside it, between account configuration directories for an account switch, and `locate`, which finds an agent's record of a conversation in a configuration directory (also what `locate_conversation_to_resume` searches with) |
| `src/account_dirs.rs` | An account's configuration directory as text, no file system access: `normalize_account_dir` (how what the user typed is stored) and `account_dir_key` (how two stored directories are compared) |
| `src/shared_items.rs` | The catalog, per agent, of the entries of an account's configuration directory that may be shared with the default account's configuration directory (`shared_items`, of `SharedItem`s): each is a file or a directory, and configuration or conversation history; credentials and per-machine state are absent |
| `src/shared_config/` | An account sharing its agent's default configuration: its choice, changing it at once under the account's lock with progress, linking the catalog's items into the default directory and undoing that, and the hooks the coordinator calls around launches, account switches and deletions (below) |
| `src/outbox.rs` | The per-session ordered queue every message Octoboard writes into an agent passes through, including what each drain does when a dialog of the agent's is up (`agent_dialogs.rs`'s `WrittenMessage`: kept as a draft, discarded, or diverted) |
| `src/agent_dialogs.rs` | Dialogs and views an agent raises that only a person can answer and no hook announces (Claude Code's resume dialogs, Enter notices and background-agent views; Codex's sign-in, resume working-directory chooser, model migration and rate-limit nudge; Grok Build's usage-limit modal): `recognise` finds one on a live session's rendered screen, `watch` is the per-session task that holds the session's writes and reads as waiting for the user while one is up and lets go when the screen shows none, and `WrittenMessage` classifies what a message written into it becomes |
| `src/store.rs` | The coordinator's SQLite storage: consoles, projects, sessions, pages, agent accounts (with their sharing choice and whether the last change of it is incomplete), each agent's default account name, the added agents list, each session's owed owner-change notice, branch origins, trusted folders, user settings and the host table |
| `src/state.rs` | Shared daemon state: session status and turn bookkeeping, the shell terminal registry, record-watch bookkeeping, each project's live git status and check claim, pending `request_session` and `request_resume_session` requests, the kind of claim held on a session (`claim_kind`, `ClaimKind`, `is_held_by_launch`: a launch or resume of its process, or an account switch, handoff, handover of branches or binding change), agent availability, the model cache, each session's default effort (`set_default_effort`, filled into the wire record by `wire_session`), the context window each Claude Code session's status line last reported, the account quotas, the account logins and the hook queues; `write_hold` (`WriteHold`) answers why a running session's writes are held, and `shown_status` is the status a client is shown, waiting for the user while the process is held up by something no hook reports |
| `src/launch_settings.rs`, `src/agent_models.rs` | Normalising and checking a model and reasoning effort (`LaunchCheck`), and listing the models an agent offers for an account (`ModelCache`); fixtures in `testdata/` |
| `src/account_quotas/` | Each account's plan quota from its agent's own CLI: `mod.rs` keeps the readings (`AccountQuotas`) and runs the background poller and the on-request refresh, `probe.rs` holds the three conversations and their reply parsers, `kick.rs` the per-agent minimal request (`kick`) that starts a usage window that has reset but not begun counting, which `mod.rs` sends once per window episode, while the `auto_start_quota_windows` setting is on, and follows with a re-read (`AccountQuotas::stop_kicks` cancels running kicks at shutdown); an account's missing directory and its not being signed in are told apart from a failure there; `AccountQuotas::exhausted` (`exhausted_in`, `Exhausted`) judges from the held reading alone whether an account's quota is used up, and `held_and_exhausted` gives the reading with that judgement under one lock |
| `src/rfc3339.rs` | Reading the RFC 3339 timestamps the agents write into milliseconds since the epoch (`parse_rfc3339_millis`), and writing the compact UTC stamp backups are named with (`compact_utc_stamp`); shared by `account_quotas/probe.rs` and `transcript.rs` |
| `src/account_login/` | Signing an account in with its agent's own CLI in the background: `mod.rs` keeps the logins running (`AccountLogins`: one per account, a timeout, cancel, the end of each, the kill on shutdown), `flows.rs` drives the three CLIs and reads what they print |
| `src/availability.rs` | Working out, once per daemon start, which agents are available and each one's default account, then filling the added agents once |
| `src/config_export.rs`, `src/config_import.rs` | Building the configuration document `export_configuration` answers with, and reading one back (`inspect` and `apply`) |
| `src/config_upgrade.rs` | Upgrading a configuration document of an older version, step by step, before an import reads it |
| `src/git_env.rs` | The base every `git` the daemon runs is built from (non-interactive, read-only, variable filtering) |
| `src/git_status.rs`, `src/git_push.rs`, `src/git_switch.rs`, `src/git_restore.rs`, `src/git_remote.rs`, `src/git_watch.rs` | Per-project git: status check against the remote and fast-forward, push of the current branch, switch to a local branch, restore of tracked changes, the recorded remote URL, and the ref watch that republishes the branch header |
| `src/branch_origin/` | Which session is responsible for a git branch: `ref_created`, `annotate`, `hand_over`, `project_deleted`, plus the git hook dispatchers (`git_hooks.rs`, `dispatcher.sh`) that report created branches |
| `src/branch_origin/bound.rs`, `src/branch_origin/git.rs`, `src/branch_origin/session_branches.rs` | The reads the branch consumers share: which branches sessions are bound to, grouped by repository (`bound_in`); the user's `git` and the worktree and uncommitted-changes listings built on it (`Git`, `worktrees`, `changes`); and the git state of the branches one session is responsible for (`of_session`: worktree, head, commits and files against the branch it was cut from, uncommitted changes) or just where named branches are checked out and whether that worktree has uncommitted changes (`worktrees_of`) |
| `src/branch_origin/handover.rs` | The building blocks for moving branch responsibility between sessions, shared by `request_session`, `new_session`, `handover_branches` and the handoff: the checked branch set (`Handover`), the move and its outcome (`move_branches`, `Handed`), and the wording of branch names (`branch_name`, `named_branches`, `branch_noun_and_pronoun`) |
| `src/branch_origin/cleanup.rs`, `src/branch_origin/cleanup/project.rs` | Removing the branches and linked worktrees bound to sessions that are archived or deleted (`inspect`, `remove_in_background`, `remove_logging_left`, `will_be_left`, `left_by` and the wording of what stays), and, for a project removed with its folder moved to the Trash, the worktrees and branches that go along (`plan`, `carry_out`); `cleanup/tests.rs` holds their tests |
| `src/handoff/` | The handoff package, built by program alone for a session another is to take over: identity and brief, the conversation read out of the agent's own record (`record.rs`, one reader per agent: user messages, replies, compaction summary, latest todo list, files changed), the branches it is responsible for and the sessions it owns, rendered as markdown and written under the data directory's `handoff/` (`build`, `prepare`, `remove`, and `sweep`, which removes packages of no session at startup); `operation.rs` is the `hand_off_session` operation: it checks the refusals, starts the receiving session with the package named in its brief, moves the owner binding, the branches and the owned sessions to it, tells the sessions concerned and archives the original; `tests.rs` and `operation/tests.rs` hold the tests |
| `src/session_history.rs` | `read_session_history`: an archived session's history for the UI, read from the agent's own record through the handoff reader (`handoff/record.rs`): the user's messages, the messages Octoboard wrote into the session told apart by how they begin, and the final reply of each turn; `agent_records::history_branch` finds the record, a Codex rollout in `archived_sessions/` included |
| `src/host_tools.rs` | `HostTools` on `AppState`: the user's `git` as the cleanup runs it, the move to the Trash, and the shell and environment a shell terminal starts with, held behind a seam so a test substitutes stand-ins |
| `src/pty.rs`, `src/ptyio.rs`, `src/ringbuf.rs` | The PTY core every process the daemon runs on a PTY shares, an agent or a shell (`PtyCore`): spawn, output fan-out to attached terminal clients over a replay ring and a parsed screen, input, resize, stop with escalation to a kill, killing what an exited agent left in its session, hang-up, the foreground process group, the path of the terminal's slave side, and its entry in the crash cleanup; non-blocking PTY I/O; the replay buffer |
| `src/session.rs`, `src/term.rs`, `src/term/` | One running agent process (`LiveSession`, built on `PtyCore`) and what is the agent's own on top of it — the agent, its title, its folder-trust watch, the count of writes into its input, what its screen last showed of the agent's own dialogs (`agent_dialogs.rs`) and its hold after an agent error; launching an agent in a PTY and writing messages into a running session (`term/invisible.rs` removes from each message the characters Claude Code's prompt treats as invisible, which would otherwise make it hold the message back unsent) |
| `src/shell_terminal.rs`, `src/shell_terminal/` | Shell terminals: a project's plain login shell on a PTY, held in memory with its record beside the agent sessions (`ShellTerminals` on `AppState`). `create`, `rename`, `close`, and `close_project` / `close_projects` / `close_all` for a project's removal and the daemon's exit; a watcher per terminal for exit and for whether the shell is busy (`busy.rs`: a descendant of the shell holding the terminal open); title tracking and environment assembly. The terminal socket resolves a session or a shell terminal in `server.rs`; the wire contract is "Shell terminals" in [`PROTOCOL.md`](PROTOCOL.md). `tests.rs` holds the tests |
| `src/terminal_snapshot.rs`, `src/terminal_title.rs`, `src/saved_output.rs` | Terminal state: parsed snapshots (including the visible rows as plain text, which `agent_dialogs.rs` recognises from) and dormant previews, OSC title parsing, and previews persisted per ended session |
| `src/crash_cleanup.rs` | Ending every process on a PTY, agent or shell, when the daemon goes down on an internal failure; an agent's whole session, not just its process group |
| `src/trust/` | Each agent's folder-trust confirmation: the permission decision and prompt, recognising each agent's screen, and pressing it; also the hold on a session's writes while its start is unconfirmed, including Grok Build's release when no hook comes |
| `src/env_shell/` | The user's real shell environment every agent is launched with (`snapshot`, `launch_snapshot`, `cached_snapshot`) and resolving a binary on it |
| `src/hostfs.rs`, `src/open_directory.rs`, `src/open_terminal.rs` | Host filesystem work (browsing, Trash, repository discovery, clone, remote probing, agent detection) and launching the system terminal, file manager or default application |
| `src/path_links.rs` | File paths a session's terminal shows, checked and opened on the daemon's host (`probe` for the `probe_path_links` request, whose answers also say how a file or folder may be viewed, and `open` for `open_path_link`): a path is reachable only inside the session's project, its worktrees or a system temp root, and what could be launched rather than viewed is refused; `path_links/tests.rs` holds the tests |
| `src/subprocess.rs` | Running a spawned process to completion within bounds (`run_bounded`, `run_with_timeout`, `run_agent_with_timeout`), exchanging lines with one that answers on stdout while stdin stays open (`run_exchange`), and detached spawning; a stop kills the process's group, or for an agent CLI its whole session (`Reach`) |
| `src/process_session.rs` | Starting a process as the leader of a session of its own (`lead_session`), telling whether it is running or has exited, and with what success, without reaping it (`unreaped_state`, `exited_unreaped`), and ending a process group (`signal_group`, `kill_group`; `group_has_members` tells whether anything but the leader is left in it) and what it left in its session (`kill_members`). Agent CLIs run by `subprocess.rs` and the account logins use all of it, and `spawn_detached` uses `lead_session` to detach an application; an agent on a PTY, which `portable-pty` already makes a session leader, uses `exited_unreaped` and `kill_members`; `env_shell/snapshot.rs` uses the group functions and `unreaped_state` on the login shell of an environment capture, which it leads as a group of its own |
| `src/hooks.rs`, `src/hook_queue.rs`, `src/hook_mode.rs` | Reading what an agent's hook payload says, the per-session ordered queue of unapplied hook events, and the `octoboardd hook` mode |
| `src/transcript.rs`, `src/rollout.rs`, `src/record_watch.rs`, `src/line_file.rs` | Watching an agent's own session record for status changes hooks never report (Claude Code transcript, Codex rollout), over a shared offset-tracked line reader whose `open` is the one way to open an agent's record, since Codex may have compressed it (zstd) |
| `src/agent_records.rs`, `src/record_titles.rs`, `src/record_observed.rs`, `src/default_effort.rs`, `src/record_context.rs`, `src/session_title.rs`, `src/record_collector.rs` | Where each agent keeps its on-disk session records (and which file holds a session's conversation, `conversation_record`, and whether that conversation can still be resumed, `resumability` / `judge`, judged against the session's own account; `removable_record_paths`, which of a session's records may be deleted, with the default directory's kept for a sharing account; `conversation_files_in`, every file one conversation has under a given directory), the titles they give a conversation, the model and reasoning effort they show a session running with, the effort Codex runs a session at when its records name the model but no effort (`DefaultEfforts`, from the account's model catalogue; held in memory by `state.rs` and sent only as `Session.default_effort`), the context size they show it at (`record_context.rs`, per agent; Claude Code's records give no window, so it is the one its status line last reported, `ReportedWindows`, held in memory by `state.rs`), the title a session shows, and the one background collector that reads them incrementally (`record_collector.rs`) |
| `src/git_ref_mode.rs`, `src/status_line_mode.rs`, `src/loopback.rs` | The `octoboardd git-ref` mode, the `octoboardd status-line` mode (Claude Code's context window, and the user's own status line run in its place), and the minimal HTTP client for the daemon's own loopback address shared by the CLI modes |
| `src/instance_lock.rs`, `src/paths.rs` | One daemon per data directory, and where Octoboard keeps its files under `~/.octoboard` (including `handoff_dir`) |
| `src/test_support.rs` | Test-only fixtures shared by the modules' unit tests (scratch directories, stored state, stand-in sessions, console, project and session records, fake agent CLIs, git repositories) |
| `src/mcp/` | The Octoboard MCP server (below) |
| `src/browse/` | Project-browsing reads over the control socket (below) |
| `src/adapter/` | One adapter per agent CLI (below) |

### `src/shared_config/`

Each account keeps whether it shares the default account's configuration directory and whether the last change toward
that choice is incomplete. A change runs at once or is refused: refused while sessions of the account work or it is being
signed in, it ends the account's idle sessions first and resumes them afterwards, waits only for short agent CLI reads
(quota, models), locks the account from the check through the resumes, and tells its progress phase by phase and entry
by entry as `account_sharing`. Nothing carries a change out later; a failed one stays incomplete for the user to retry.
Before every launch of a sharing account's session the links an agent replaced are repaired, unless the account is
incomplete. The wire contract is "Sharing the default configuration" in [`PROTOCOL.md`](PROTOCOL.md); the items that
may be shared are the catalog in `src/shared_items.rs`.

External interfaces (re-exported from `mod.rs`, defined where each says):

- `set` (`change.rs`) — refuses or carries out a sharing change at once, answering when it has ended.
- `before_delete`, `before_repoint` (`change.rs`) — before an account is deleted or pointed at another directory: turn
  its sharing off first, at once under the account's lock and with its idle sessions restarted, refusing while its
  sessions work or it is being signed in.
- `before_launch` (`launch.rs`) — before a session launches or resumes: refuses while its account's change runs, and
  repairs links; a repair that cannot fix something becomes a notice rather than a refusal.
- `refuse_if_changing`, `before_switch_copy` (`mod.rs`) — refuse opening a session, a handoff or an account switch
  while a change of the account involved runs; the latter also takes the agent's lock for the switch's copy.
- `publish`, `note_session`, `states` (`status.rs`) — send each owned account's sharing state when it changed, and list them for a
  `snapshot`; `AppState` calls `publish` when a session's claim is taken or let go, its process ends or its account
  changes, and `note_session` when its record is published or a message for it is queued or written, which publishes
  only when that moved the session between working and idle.
- `Sharing` (`mod.rs`) — what `AppState` holds: the per-agent locks (`locks.rs`) over changing and repairing sharing,
  and each account's progress and last failure (`status.rs`).
- `reading_account`, `running_long` (`cli_reads.rs`) — taken by agent CLI runs outside a session: short reads (quota
  probes, model-list reads), which a change waits for, and long runs (logins), which refuse one; both wait for a
  change in progress.

The lock itself lives on `AppState` (`lock_account_sharing`), under the same lock as the sessions' launch claims, which
is what makes the in-use check and the lock one step: `begin_launch` refuses a session of a locked account. It also
tells working sessions from idle ones (`Running`, `activity_of`), claims the idle ones for the change to restart once it
no longer waits (`AccountSharingLock::claim_idle`, which also holds their outboxes until each is relaunched), and has
`shown_status` show such a session `starting` (to clients and to the agents' session tools alike) while its process is
down. A session's record being published sends the sharing state only when it moved the session between working and idle
(`note_session`).

| File | Role |
|---|---|
| `mod.rs` | The module's contract, the refusals while a change runs, what runs under an account, the error builders |
| `change.rs` | Locking an account and running a change with its progress, ending and resuming its idle sessions around it, and turning sharing off before a removal or repoint |
| `launch.rs` | What a launch takes from its account's sharing: the shared directory and the repair of replaced links |
| `locks.rs` | The per-agent lock over the default directory |
| `status.rs` | The accounts' sharing state as `account_sharing` sends it: what runs under each directory, progress, last failure |
| `link.rs` | Turning sharing on and off item by item over the catalog, telling each entry reached, and the launch-time repair of links an agent replaced |
| `moves.rs` | File-move primitives (across volumes too), backup naming, merging a directory into another |
| `take_back.rs` | On turning sharing off, moving the account's own sessions' conversations (archived ones included) back out of the default directory |
| `codex_state.rs` | Carrying, rewriting and forgetting the rows of Codex's thread state database so shared rollouts stay listed and resumable |
| `registries.rs` | Rewriting the paths plugin registries record when an account stops sharing its plugin directory |
| `cli_reads.rs` | The registry of agent CLI runs under account directories, short and long, and the hold a change takes over them |
| `tests.rs` | Tests over real directories |

### `src/mcp/`

The tool surface a session drives Octoboard with: orchestration tools for console sessions, a narrower set for project
sessions that can own sessions, the reporting tool for bound sessions, settings tools, information sharing between
sessions, the read of the accounts' plan quotas, and `request_session` and `request_resume_session`. Which set a session sees follows its role and
its current binding, which the daemon checks on every call. The catalogue is shared by both sides of the stdio bridge.
The wire contract is in [`PROTOCOL.md`](PROTOCOL.md).

| File | Role |
|---|---|
| `mod.rs` | The tool catalogue, and which tools a session may see by role and binding |
| `role.rs`, `console_instructions.rs` | The role description injected at launch, and the console session's instruction file written into its working directory |
| `exec.rs` | Runs one tool call through the same coordinator and reporting functions the control socket uses |
| `handoff.rs` | `handoff_session`: an owner hands a session of its own off to another agent, through the `hand_off_session` operation, and is not sent the message that tells an owner of it |
| `handover.rs` | `handover_branches`: an owner moves the responsibility for git branches between itself and a session of its own, or between two sessions of its own, and tells the sessions concerned; also holds the checks on the branches a call names (`branch_names`, `held_by`), which `new_session` and `request_session` use too |
| `resume_request.rs` | `request_resume_session`: a session asks the user to approve resuming a conversation by its agent conversation id, with the Resume conversation tab's capability and limits; reads the arguments, locates the conversation, plans what an approval does and refuses up front what it could never carry out, then asks through `session_request/` |
| `account_switch.rs` | `switch_session_account`: an owner moves a session of its own to another account of its agent, through the `switch_session_account` operation, keeping its conversation |
| `quotas.rs` | `get_account_quotas`: every account of every agent a session can be opened on with the plan quota reading the daemon holds for it, read-only |
| `bindings.rs` | `transfer_session`, `release_session` and `bind_to_session`, and what they tell the sessions concerned; also the helpers the other owner tools share to tell a session its owner changed now or leave the notice owed for its next start (`is_running`, `running_or_leave`, `tell_now_or_leave`, `owner_notice`) |
| `args.rs` | Reading a tool call's arguments and finding the records they name |
| `settings.rs` | The settings tools for a console, its projects and the application, and a project session's read of its own project |
| `stdio.rs` | `octoboardd mcp` itself: the stdio child forwarding calls to the daemon over loopback |

### `src/browse/`

Serves the project-browsing control-socket requests (`get_project_source`, `list_project_dir`, `search_project_files`,
`read_project_file`, `list_project_changes`, `read_project_change`, `list_project_branches`,
`compare_project_branches`, `read_project_comparison_change`, `list_project_commits`, `read_project_commit_change` and
the `…_change_bodies` requests): bounded reads from the disk, the index, a branch or a commit, a worktree's uncommitted
changes, the changes one branch has made since it forked from another, and the commits it made since, each with its
changes. The wire contract and budgets are in "Browsing a project" in [`PROTOCOL.md`](PROTOCOL.md).
`mod.rs`'s `serve` is the entry point, called from `server.rs`.

| File | Role |
|---|---|
| `mod.rs` | `serve`, `locate` (repository and worktree for a Git read), `resolve_project_path`, `project_live_roots` (the directories of a project and its worktrees that path links resolve against) |
| `source.rs` | Resolving a project to its root, repository, worktree and sibling worktrees, and the identities clients hold |
| `wire_path.rs` | `RelPath`, the canonical wire form of a path |
| `live.rs` | Reading and listing files on disk, and the bounded file-name search `search_files` |
| `blob.rs` | Reading blobs from the index or a commit, and resolving branches and commits |
| `git.rs` | The one way browse runs `git`: isolated, bounded, never writing to the repository |
| `changes.rs` | A worktree's uncommitted changes: `list`, `read`, `read_between` and the bodies reads |
| `compare.rs` | Local branches, each with the remote branch to compare it with when it is the main working copy's, and the comparison of a branch with a local branch, a remote-tracking branch or the empty tree: `branches`, `list`, `read`, `read_bodies` |
| `commits.rs` | The commits a branch made since it forked from another, or its whole history from the empty tree: `list`, `read`, `read_bodies` |
| `tree_diff.rs` | Reading `git`'s raw tree-diff output, and the stdout budget it is read under, shared by `compare.rs` and `commits.rs` |
| `line_counts.rs` | The lines each listed change adds and removes (`LineStats`), read from `git`'s numstat output beside the listing's own read under its own short deadline and output budget, with untracked files counted from disk; advisory, so a failed or over-budget run leaves a listing without counts. Used by `changes.rs`, `compare.rs` and `commits.rs`: `wanted`, `read_diff`, `read_log`, `apply`, `count_file` |
| `budget.rs`, `lane.rs` | The limits every read is held to, and the per-connection lane (outstanding-request bound, superseding, retained-bytes reservation) |
| `tests.rs`, `change_tests.rs`, `compare_tests.rs`, `commits_tests.rs`, `line_count_tests.rs`, `nested_tests.rs`, `body_tests.rs` | Test-only end-to-end tests over real repositories |

### `src/adapter/`

A single adapter interface (launch, session id, capability injection, status reporting, resume), so the rest of the
daemon is agnostic to which agent it drives. Status hooks, the Octoboard MCP server and a role description are
injected when a session starts, along with the account's approval override where the agent has one (Codex's reviewer,
Claude Code's permission mode).

| File | Agent |
|---|---|
| `mod.rs` | The adapter trait and the agent-agnostic launch/resume flow shared by all three, `config_dir_var`, and `account_command` (the command for a short query of an agent's CLI as an account would run it; `resolve_account_binary` and `account_command_of` split it so a caller making several commands runs the one binary it resolved once) |
| `claude/mod.rs` | Claude Code |
| `claude/settings.rs` | The Claude Code settings files a launch reads: the user's and the project's, with the `statusLine` they give, the `allowedHttpHookUrls` that decide between HTTP and command hooks, and the trust state in the global config file |
| `codex.rs` | Codex, and `unarchive`, which brings an archived rollout back before a resume |
| `grok.rs` | Grok Build |
