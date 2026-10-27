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
once; `pnpm test:rust` from the root runs both crates' tests without it.

## External interfaces

- WebSocket/HTTP server, bound to `127.0.0.1` on an OS-assigned port — the only way a client talks to the daemon.
  Specified in [`PROTOCOL.md`](PROTOCOL.md), which is authoritative.
- `octoboardd hook --session <id> --port <port>` — CLI mode run by the per-session hook script for agents whose hooks
  use the command form; posts the payload from stdin to the running daemon's `POST /hook/:session`.
- `octoboardd git-ref --session <id> --port <port> [--head <ref>] -- <ref> <oid>...` — CLI mode started by a project
  session's `reference-transaction` git hook when branches were created; posts them to `POST /git-ref/:session`.
- `octoboardd gate-hook --session <id> --port <port> reference-transaction <state>|pre-push` — CLI mode run in the
  foreground by a project session's `reference-transaction` and `pre-push` git hook dispatchers when an existing branch
  moves or branches are pushed; asks `POST /revert-gate/:session` whether the revert gate applies and with what, runs
  it in-process over the hook's stdin and exits with its status (3 rejects); anything but an answer lets git through.
- `octoboardd revert-gate <state> [--guard <branch>]... [--target <branch> <target-branch>]... [--fork-point <branch> <commit>]...`
  and `octoboardd revert-gate pre-push <remote> <url> [the same options]` — CLI modes that run the revert gate directly
  with the guarded branches, target branches and fork points given as arguments (the session's hook dispatchers use
  `gate-hook` instead); read the hook's ref updates, or the refs of a push, from stdin and exit 3 to reject one that
  undoes changes already merged into a guarded branch (reason on stderr), 0 otherwise, including when the check itself
  fails.
- `octoboardd status-line --session <id> --port <port> --cwd <dir> --config-dir <dir>` — CLI mode that is the status
  line the Claude Code adapter injects; posts the context window from stdin to `POST /context-window/:session` and
  runs the user's own status line with the same input, passing its output and exit status on.
- `octoboardd mcp --session <id> --role <role> [--bound] --port <port> --token <token>` — CLI mode that is the MCP
  server each adapter registers for a session; speaks MCP on stdin/stdout and forwards tool calls to
  `POST /mcp/:token`.

## Layout

The crate is one flat `src/` of modules; each row groups the files of one responsibility. Test-only files
(`tests.rs`, `*_tests.rs`, `test_support.rs`) sit beside the code they test, and `testdata/` holds the captured agent
screens, terminal titles and session records the tests replay.

### Entry and wire

| Path | Role |
|---|---|
| `build.rs` | Exposes the app name from the repo-root `config/app.json` as `OCTOBOARD_APP_NAME` |
| `src/main.rs` | CLI entry point: parses the seven modes, opens the store, binds the port, prints the handshake line the application waits for, runs the server |
| `src/server.rs`, `src/access.rs` | The HTTP/WebSocket router described in `PROTOCOL.md` (the control, terminal, MCP, hook, git-ref, revert-gate and context-window routes), and the middleware refusing with `403` a request whose `Host` or `Origin` is not a local client's or the application's own UI's |
| `src/protocol.rs` | Rust types for the wire protocol (kept in sync by hand with `PROTOCOL.md` and `packages/ui/src/protocol.ts`), the console session colours, and the one mapping from a failure to an `error` reply |
| `src/git_ref_mode.rs`, `src/gate_hook_mode.rs`, `src/status_line_mode.rs`, `src/hook_mode.rs`, `src/loopback.rs` | The `git-ref`, `gate-hook`, `status-line` and `hook` CLI modes, and the minimal HTTP client for the daemon's own loopback address they share (`mcp` is `src/mcp/stdio.rs`, `revert-gate` is `src/revert_gate/`) |

### Coordinator role

| Path | Role |
|---|---|
| `src/coordinator.rs` | What each control-socket request does to the stored consoles, projects, sessions, pages and accounts, and which host-role work it triggers: session open, launch, resume, binding, transfer, release, archive and delete cascades, account resolution, agent list and account edits, launch settings, and the shell terminal requests; `check_can_resume` (also what `request_resume_session` refuses with) and `locate_conversation` (where a conversation is held, for the Resume tab and `request_resume_session`) are the entry points the resume paths share |
| `src/session_request/` | A session asking the user to approve something: `request_session`, `request_resume_session`, and the `switch_session_account` and `handoff_session` calls the user is asked to approve; keeps the pending requests, the wait for the answer and the carrying out of an approval |
| `src/reporting.rs`, `src/sharing.rs` | The channel between an owner and the sessions bound to it (briefs, messages into a running session, reports, automatic archiving, report-panel form submissions), and information one session shares with another |
| `src/handoff/` | The handoff package built by program alone for a session another is to take over (identity, the conversation read from the agent's own record, branches and owned sessions, written under the data directory), and the `hand_off_session` operation that carries a handoff out |
| `src/switch_continue.rs` | `should_continue`: whether a session an account switch relaunches is sent `CONTINUE_PROMPT` to carry on with its turn |
| `src/session_history.rs` | `read_session_history`: a session's history for the UI's history view and the MCP tool of the same name, read from the agent's own record |
| `src/archive_cleanup.rs` | The daemon task (`spawn`) that deletes archived sessions whose conversation the agent can no longer resume |
| `src/branch_origin/` | Which session is responsible for a git branch: recording created branches and their target branch and fork point, handing responsibility over, cleaning up the branches and worktrees of archived or deleted sessions, and the git hook dispatchers (`git_hooks.rs`, `dispatcher.sh`) that report created branches and start `octoboardd gate-hook`; `revert_gate_inputs.rs` is the daemon side of `POST /revert-gate/:session` |
| `src/revert_gate/` | The revert gate engine behind `octoboardd revert-gate` and run in-process by `octoboardd gate-hook` (`check`): the local `reference-transaction` path, the `pre-push` path and the content rules |
| `src/git_workflow.rs` | The git workflow settings: the application-wide values, a project's override, and `effective`, the one function that combines them |
| `src/interface_language.rs` | The languages the interface is offered in as the daemon knows them, mirroring the UI's `i18n/languages.ts` |

### Storage, state and configuration

| Path | Role |
|---|---|
| `src/store.rs` | The coordinator's SQLite storage: consoles, projects, sessions, pages, agent accounts and their choices, the added agents list, owed owner-change notices, kept information for interrupted sessions (`pending_info`), branch origins, trusted folders, user settings and the host table |
| `src/state.rs` | Shared daemon state (`AppState`): session status and turn bookkeeping, the shell terminal registry, each project's live git status, pending session requests, launch claims, agent availability, the model cache, account quotas and logins, and the hook queues; `shown_status` is the status a client is shown |
| `src/outbox.rs` | The per-session ordered queue every message Octoboard writes into an agent passes through, and what a drain does when a dialog of the agent's is up |
| `src/config_export.rs`, `src/config_import.rs`, `src/config_upgrade.rs` | Building the configuration document `export_configuration` answers with, reading one back (`inspect` and `apply`), and upgrading an older document before an import reads it |
| `src/launch_settings.rs`, `src/agent_models.rs` | Normalising and checking a model and reasoning effort, and listing the models an agent offers for an account (`ModelCache`) |
| `src/paths.rs`, `src/instance_lock.rs` | Where Octoboard keeps its files under `~/.octoboard`, and one daemon per data directory |
| `src/availability.rs` | Working out, once per daemon start, which agents are available and each one's default account |

### Agent accounts

| Path | Role |
|---|---|
| `src/account_quotas/` | Each account's plan quota read from its agent's own CLI: the held readings and background poller, the on-request refresh, the minimal request that starts a reset usage window, and the used-up judgement (`AccountQuotas::exhausted`) |
| `src/account_login/` | Signing an account in with its agent's own CLI in the background (`AccountLogins`: one per account, timeout, cancel) |
| `src/account_dirs.rs`, `src/shared_items.rs`, `src/relocate.rs` | An account's configuration directory as text (`normalize_account_dir`, `account_dir_key`), the catalog of entries that may be shared with the default account's directory, and copying a session's conversation record between configuration directories for an account switch (`locate` finds a conversation record) |
| `src/shared_config/` | An account sharing its agent's default configuration (below) |
| `src/rfc3339.rs` | Reading the timestamps the agents write, and writing the compact UTC stamp backups are named with |

### Host work: git and filesystem

| Path | Role |
|---|---|
| `src/git_env.rs` | The base every `git` the daemon runs is built from (non-interactive, read-only, variable filtering) |
| `src/git_status.rs`, `src/git_push.rs`, `src/git_switch.rs`, `src/git_restore.rs`, `src/git_remote.rs`, `src/git_watch.rs` | Per-project git: status against the remote and fast-forward, push, switch to a local branch, restore of tracked changes, the recorded remote URL, and the ref watch that republishes the branch header |
| `src/browse/` | Project-browsing reads over the control socket (below) |
| `src/hostfs.rs`, `src/open_directory.rs`, `src/open_terminal.rs`, `src/path_links.rs`, `src/host_tools.rs` | Host filesystem work (browsing, Trash, repository discovery, clone, remote probing, agent detection), launching the system terminal, file manager or default application, probing and opening the file paths a terminal shows, and `HostTools`, the seam a test substitutes stand-ins behind |
| `src/subprocess.rs`, `src/process_session.rs` | Running a spawned process to completion within bounds, and starting, observing and ending a process as the leader of a session of its own |
| `src/env_shell/` | The user's real shell environment every agent is launched with, and resolving a binary on it |

### Running sessions

| Path | Role |
|---|---|
| `src/pty.rs`, `src/ptyio.rs`, `src/ringbuf.rs` | The PTY core every process on a PTY shares (`PtyCore`): spawn, output fan-out over a replay ring and a parsed screen, input, resize, stop with escalation, and non-blocking PTY I/O |
| `src/session.rs`, `src/term.rs`, `src/term/` | One running agent process (`LiveSession`) and what is the agent's own on top of the PTY: launching an agent and writing messages into a running session |
| `src/shell_terminal.rs`, `src/shell_terminal/` | Shell terminals: a project's plain login shell on a PTY (`ShellTerminals` on `AppState`) with create, rename, close and a watcher for exit and busyness; the wire contract is "Shell terminals" in [`PROTOCOL.md`](PROTOCOL.md) |
| `src/terminal_snapshot.rs`, `src/terminal_title.rs`, `src/saved_output.rs` | Parsed terminal snapshots and dormant previews, OSC title parsing, and previews persisted per ended session |
| `src/crash_cleanup.rs` | Ending every process on a PTY when the daemon goes down on an internal failure |
| `src/trust/` | Each agent's folder-trust confirmation: recognising the screen, pressing it, and holding a session's writes meanwhile |
| `src/agent_dialogs.rs` | Recognising dialogs an agent raises that only a person can answer, and the per-session watch that holds writes while one is up |
| `src/interrupt.rs`, `src/interrupt/` | `interrupt_session`, shared by the control request and the MCP tool: stops the turn a running session is in with the agent's own interrupt key |
| `src/title_activity.rs`, `src/title_interrupt.rs` | What an agent's terminal title says about its turn, and closing a turn from the title when a cancel leaves nothing in the record or hooks |

### Agent records, hooks and adapters

| Path | Role |
|---|---|
| `src/hooks.rs`, `src/hook_queue.rs` | Reading what an agent's hook payload says, and the per-session ordered queue of unapplied hook events |
| `src/transcript.rs`, `src/rollout.rs`, `src/record_watch.rs`, `src/line_file.rs` | Watching an agent's own session record for status changes hooks never report (Claude Code transcript, Codex rollout), over a shared offset-tracked line reader |
| `src/agent_records.rs`, `src/record_titles.rs`, `src/record_observed.rs`, `src/default_effort.rs`, `src/record_context.rs`, `src/session_title.rs`, `src/record_collector.rs` | Where each agent keeps its on-disk session records, whether a conversation can still be resumed, and what the records show of a session (titles, model and effort, context size), gathered by the one background collector |
| `src/adapter/` | One adapter per agent CLI (below) |
| `src/mcp/` | The Octoboard MCP server (below) |

### `src/shared_config/`

Each account keeps whether it shares the default account's configuration directory and whether the last change toward
that choice is incomplete. A change runs at once or is refused, ends the account's idle sessions first and resumes them
afterwards, and reports its progress as `account_sharing`. The wire contract is "Sharing the default configuration" in
[`PROTOCOL.md`](PROTOCOL.md); the items that may be shared are the catalog in `src/shared_items.rs`.

External interfaces (re-exported from `mod.rs`, defined where each says):

- `set` (`change.rs`) — refuses or carries out a sharing change at once, answering when it has ended.
- `before_delete`, `before_repoint` (`change.rs`) — turn sharing off before an account is deleted or pointed at
  another directory.
- `before_launch` (`launch.rs`) — before a session launches or resumes: refuses while its account's change runs, and
  repairs links.
- `refuse_if_changing`, `before_switch_copy` (`mod.rs`) — refuse opening a session, a handoff or an account switch
  while a change of the account involved runs.
- `publish`, `note_session`, `states` (`status.rs`) — send each owned account's sharing state when it changed, and
  list them for a `snapshot`.
- `Sharing` (`mod.rs`) — what `AppState` holds: the per-agent locks (`locks.rs`) and each account's progress and last
  failure.
- `reading_account`, `running_long` (`cli_reads.rs`) — taken by agent CLI runs outside a session, which a change waits
  for or refuses.

The lock over an account's change is not in this module: it lives on `AppState` (`lock_account_sharing`), under the
same lock as the sessions' launch claims, and `begin_launch` refuses a session of a locked account.

Internals, by file:

| File | Role |
|---|---|
| `link.rs`, `moves.rs` | Turning sharing on and off item by item over the catalog and repairing links an agent replaced, and the file-move primitives it uses (across volumes, backup naming, merging directories) |
| `take_back.rs` | On turning sharing off, moving the account's own sessions' conversations back out of the default directory |
| `codex_state.rs`, `registries.rs` | Keeping Codex's thread state database and the plugin registries' recorded paths consistent with shared or unshared items |

### `src/mcp/`

The tool surface a session drives Octoboard with. Every session has the same reads (sessions, histories, branches,
consoles, projects, archives, models and plan quotas); console sessions add orchestration and settings tools, project
sessions that can own sessions a narrower orchestration set, bound sessions the reporting tool, and every session has
information sharing, `request_session` and `request_resume_session`. Which tools a session sees follows its role and
binding, which the daemon checks on every call. The wire contract is in [`PROTOCOL.md`](PROTOCOL.md).

| File | Role |
|---|---|
| `mod.rs` | The tool catalogue, and which tools a session may see by role and binding |
| `role.rs`, `console_instructions.rs` | The role description injected at launch, and the console session's instruction file |
| `exec.rs`, `args.rs` | Dispatching one tool call through the same coordinator and reporting functions the control socket uses, and reading a call's arguments |
| `reads.rs`, `models.rs`, `history.rs`, `quotas.rs`, `session_branches.rs` | The read-only tools every role has: `get_session`, `list_sessions`, `list_projects`, `list_consoles`, `list_archived`, `list_agent_models`, `read_session_history`, `get_account_quotas`, `get_session_branches` |
| `handoff.rs`, `handover.rs`, `bindings.rs`, `account_switch.rs` | The owner tools: `handoff_session`, `handover_branches`, `transfer_session`, `release_session` and `bind_to_session`, `switch_session_account`; `handover.rs` also holds the branch checks (`branch_names`, `held_by`) that `new_session` and `request_session` use, and `bindings.rs` the owner-notice helpers the owner tools share (`running_or_leave`, `tell_now_or_leave`, `owner_notice`) |
| `resume_request.rs` | `request_resume_session` |
| `pages.rs`, `settings.rs` | A session's report panel pages (`show_page`, `list_pages`, `delete_pages`), for a console session and a project session that faces the user, and the settings tools |
| `stdio.rs` | `octoboardd mcp` itself: the stdio child forwarding calls to the daemon over loopback |

### `src/browse/`

Serves the project-browsing control-socket requests (`get_project_source`, `list_project_dir`, `search_project_files`,
`read_project_file`, the change, branch-comparison and commit listings and reads, and the `…_change_bodies` requests):
bounded reads from the disk, the index, a branch or a commit. The wire contract and budgets are in "Browsing a project"
in [`PROTOCOL.md`](PROTOCOL.md). `mod.rs`'s `serve` is the entry point, called from `server.rs`.

| File | Role |
|---|---|
| `mod.rs`, `source.rs`, `wire_path.rs` | `serve`, resolving a project to its root, repository and worktrees, and `RelPath`, the canonical wire form of a path |
| `live.rs`, `blob.rs`, `ignored.rs` | Reading and listing files on disk and the file-name search, reading blobs from the index or a commit, and which entries Git ignores |
| `changes.rs`, `compare.rs`, `commits.rs`, `tree_diff.rs`, `line_counts.rs` | A worktree's uncommitted changes, branch comparisons, the commits a branch made since it forked, and the shared reading of `git`'s tree-diff and line-count output |
| `git.rs`, `budget.rs`, `lane.rs` | The one way browse runs `git` (isolated, bounded, never writing), the limits every read is held to, and the per-connection lane |

### `src/adapter/`

A single adapter interface (launch, session id, capability injection, status reporting, resume), so the rest of the
daemon is agnostic to which agent it drives. Status hooks, the Octoboard MCP server and a role description are
injected when a session starts, along with the agent's approval override where the agent has one.

| File | Agent |
|---|---|
| `mod.rs` | The adapter trait and the agent-agnostic launch/resume flow shared by all three, and `account_command`, the command for a short query of an agent's CLI as an account would run it |
| `claude/` | Claude Code, with `settings.rs` reading the settings files a launch depends on |
| `codex.rs` | Codex, and `unarchive`, which brings an archived rollout back before a resume |
| `grok.rs` | Grok Build |
