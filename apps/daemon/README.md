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
  Every route requires the per-launch key the daemon prints with its port on its first stdout line, which each CLI mode
  below takes as `--key`. Specified in [`PROTOCOL.md`](PROTOCOL.md), which is authoritative.
- `octoboardd hook --session <id> --port <port> --key <key>` — CLI mode run by the per-session hook script for agents whose hooks
  use the command form; posts the payload from stdin to the running daemon's `POST /hook/:session`.
- `octoboardd git-ref --session <id> --port <port> --key <key> [--head <ref>] -- <ref> <oid>...` — CLI mode started by a project
  session's `reference-transaction` git hook when branches were created; posts them to `POST /git-ref/:session`.
- `octoboardd gate-hook --session <id> --port <port> --key <key> reference-transaction <state>|pre-push` — CLI mode run in the
  foreground by a project session's `reference-transaction` and `pre-push` git hook dispatchers when an existing branch
  moves or branches are pushed; asks `POST /revert-gate/:session` whether the revert gate applies and with what, runs
  it in-process over the hook's stdin and exits with its status (3 rejects); anything but an answer lets git through.
- `octoboardd revert-gate <state> [--guard <branch>]... [--target <branch> <target-branch>]... [--fork-point <branch> <commit>]...`
  and `octoboardd revert-gate pre-push <remote> <url> [the same options]` — CLI modes that run the revert gate directly
  with the guarded branches, target branches and fork points given as arguments (the session's hook dispatchers use
  `gate-hook` instead); read the hook's ref updates, or the refs of a push, from stdin and exit 3 to reject one that
  undoes changes already merged into a guarded branch (reason on stderr), 0 otherwise, including when the check itself
  fails.
- `octoboardd status-line --session <id> --port <port> --key <key> --cwd <dir> --config-dir <dir>` — CLI mode that is the status
  line the Claude Code adapter injects; posts the context window from stdin to `POST /context-window/:session` and
  runs the user's own status line with the same input, passing its output and exit status on.
- `octoboardd mcp --session <id> --role <role> [--worker] --port <port> --key <key> --token <token>` — CLI mode that is the MCP
  server each adapter registers for a session; speaks MCP on stdin/stdout and forwards tool calls to
  `POST /mcp/:token`.

## Layout

`src/` is grouped into one directory per domain; each row below groups the files of one responsibility. A few
single-file helpers sit at its root: `main.rs`, `blocking_pass.rs`, `git_workflow.rs`, `host_tools.rs`,
`instance_lock.rs`, `interface_language.rs`, `log_file.rs`, `paths.rs`, `project_trash.rs`, `rfc3339.rs`,
`trash_safety.rs` and `wire_path.rs`. Test-only files (`tests.rs`, `*_tests.rs`, `test_support.rs`) sit beside the code
they test, and `testdata/` holds the captured agent screens, terminal titles and session records the tests replay.

### Entry and wire

| Path | Role |
|---|---|
| `build.rs` | Exposes the app name from the repo-root `config/app.json` as `OCTOBOARD_APP_NAME` |
| `src/main.rs` | Entry point: hands a named CLI mode to `cli::run`; otherwise takes the instance lock, opens the store, binds the port, generates the launch key, prints the handshake line (port and key) the application waits for, and runs the server |
| `src/cli/` | The command line: `mod.rs` the clap `Cli` and `Command` and `run`, which dispatches the modes other processes start (`hook`, `git-ref`, `gate-hook`, `revert-gate`, `status-line`, `mcp`); `hook.rs` and `status_line.rs` the `hook` and `status-line` modes, and `loopback.rs` the minimal HTTP client for the daemon's own loopback address that the modes calling the running daemon share. The other modes' code is `src/session_git_hooks/ref_mode.rs` and `gate_mode.rs` (`git-ref`, `gate-hook`), `src/mcp/stdio.rs` (`mcp`) and `src/revert_gate/` (`revert-gate`) |
| `src/server/` | The HTTP/WebSocket router described in `PROTOCOL.md`: `mod.rs` the route table and guard, `control.rs` the control socket (the snapshot a client opens with, the broadcasts, the requests and the one writer), `terminal.rs` the terminal socket (the replay, the live output and terminal input dispatch through one background writer per connection with bounded deferred input and priority for query replies), `callbacks.rs` the hook, git-ref, revert-gate, context-window and MCP routes, and `access.rs` the middleware refusing with `403` a request whose `Host` or `Origin` is not a local client's or the application's own UI's and with `401` one without the launch key (`generate_key`) |
| `src/protocol/` | Rust types for the wire protocol (described in `PROTOCOL.md`; the UI's hand-written `packages/ui/src/protocol.ts` is checked against them through `bindings.rs`), re-exported by `mod.rs` as the one `crate::protocol` namespace from a file per concern: `requests.rs` and `browse.rs` (what a client sends), `events.rs` (what the daemon sends), `records.rs` (the records clients hold, the console session colours, `Session::orchestration_role` and `Session::is_worker`, the session's stored coordinator or worker capability), `settings.rs`, `agents.rs`, `models.rs`, `quotas.rs`, `accounts.rs`, `history.rs`, `session_branches.rs`, `path_links.rs` and `terminal.rs` (one section of `PROTOCOL.md` each), `fields.rs` (serde helpers), `errors.rs` (the one mapping from a failure to an `error` reply), and `codes.rs` (the `Code` registry, constructible only there) |
| `src/protocol/bindings.rs` | Test-only contract check: renders the wire types as TypeScript into the committed `packages/ui/src/protocol.generated.ts` and fails when that file is stale; the same test regenerates it when run with `OCTOBOARD_WRITE_BINDINGS=1` |

### Coordinator role

| Path | Role |
|---|---|
| `src/coordinator/` | What each control-socket request does to the stored consoles, projects, sessions, pages and accounts, and which host-role work it triggers; `mod.rs` is the dispatcher (`handle_inner`, which also carries the shell terminal requests) and the submodules carry the requests out: `consoles.rs`, `projects.rs`, `open.rs` (session open), `resume.rs`, `launch.rs` (with `SWITCH_STOP_TIMEOUT`, how long a restart waits for a process it ended to be recorded down), `restart.rs` (what a switch of account, a change of model and a change of sharing share when they restart a session), `bindings.rs` (parent assignment, transfer, release), `lifecycle.rs` (archive and delete cascades), `account_switch.rs`, `model_change.rs`, `account_sharing.rs` (a change of an account's sharing of the default configuration as an operation: `set`, `before_delete`, `before_repoint`, which end the account's idle sessions and resume them around what `src/accounts/shared_config/` does to the directories), `exit_watch.rs` (not a request handler: `watch_exit`, the exit watcher every launch starts, which records how a session's process ended and tells its owner of an end nobody asked for), `archive_cleanup.rs` (not a request handler: `spawn`, the daemon task that deletes archived sessions whose conversation the agent can no longer resume), `pages.rs` (withdrawing a worker's report-panel pages, `withdraw_pages`, when a session becomes a worker, the daemon-start sweep of pages left on workers, and telling the panel which pages are gone), `account_choice.rs` (account resolution), `accounts.rs` (agent list and account edits), `settings.rs`, `messages.rs` and `validate.rs`; `set_session_model` (`model_change.rs`) is the model-and-effort change shared by the control socket and MCP; `check_can_resume` (also what `request_resume_session` refuses with) and `locate_conversation` (where a conversation is held, for the Resume tab and `request_resume_session`), both in `resume.rs`, are the entry points the resume paths share |
| `src/session_request/` | A session asking the user to approve something: `request_session`, `request_resume_session`, and the `switch_session_account` and `handoff_session` calls the user is asked to approve; keeps the pending requests, the wait for the answer and the carrying out of an approval |
| `src/messaging/` | The messages Octoboard writes into sessions: `delivery.rs` the daemon-wide API for writing a message into a session (the gate on what it can take, the queue, what became of each message), including writing to an owner, which waits out a claim on it, follows a session handed to another owner, and keeps the message for an owner that is interrupted until it resumes, and `Delivery::note`, the one wording of what became of a delivery for every tool and notice that tells a session so; `outbox.rs` the per-session ordered queue every such message passes through, stable message identities, and drain outcomes for dialogs, partial writes, pending draft recovery and uncertain native submissions; `reporting.rs` and `sharing.rs` the reports a bound session sends to its owner (delivered into it, with automatic archiving, and the report Octoboard synthesises when a session stops without one) and information one session shares with another; `owner_notices.rs` every notice to an owner about its sessions (one ended on its own, waits for the user, was switched to another account at its usage limit or resumed once its quota recovered, or could not be) and the parent-change notices to a session whose owner changed or to the owner that lost it, with the sentences that tell a session its role after such a change, which `bind_to_session` and a handoff use too; `texts.rs` the fixed texts and message openings written in Octoboard's own name, including the opening prompts of a session started at another session's request, a report panel form submission and the brief |
| `src/handoff/` | The handoff package built by program alone for a session another is to take over (identity, the conversation read from the agent's own record, branches and child sessions, written under the data directory), and the `hand_off_session` operation that carries a handoff out; where a package's file is and its removal are `package_file.rs`'s, which depends on nothing but `std` so that the removal of a session's record (`src/state/forget.rs`) uses it too |
| `src/branch_origin/` | Which session is responsible for a git branch: recording created branches and their target branch and fork point, handing responsibility over, listing a session's branches, what is bound to a session (`bound.rs`), and reading the reflog files the attribution leans on (`git_files.rs`) |
| `src/branch_cleanup/` | Cleaning up the branches and worktrees bound to archived or deleted sessions: `survey.rs` (what is bound and where it stands), `remove.rs` (the removal and its consents), `report.rs` (its wire, MCP and text presentation), `project.rs`, and the per-repository gates (`Cleanups`) in `mod.rs` |
| `src/session_git_hooks/` | The git hooks of a project session: the directory of hook dispatchers (`mod.rs`, `dispatcher.sh`) that `core.hooksPath` is pointed at through the agent's environment to report created branches and start `octoboardd gate-hook`, `gate_inputs.rs`, the daemon side of `POST /revert-gate/:session`, and the `git-ref` and `gate-hook` CLI modes (`ref_mode.rs`, `gate_mode.rs`) |
| `src/revert_gate/` | The revert gate engine behind `octoboardd revert-gate` and run in-process by `octoboardd gate-hook` (`check`): the local `reference-transaction` path, the `pre-push` path and the content rules |
| `src/git_workflow.rs` | The git workflow settings: the application-wide values, a project's override, and `effective`, the one function that combines them |
| `src/interface_language.rs` | The languages the interface is offered in as the daemon knows them, mirroring the UI's `i18n/languages.ts` |

### Storage, state and configuration

| Path | Role |
|---|---|
| `src/store/` | The coordinator's SQLite storage, `Store` (its `require_project`, `require_console`, `require_account` and `require_session` answer a known record or its `unknown_*` error): `mod.rs` the struct, `schema.rs` the schema and what opening an existing file does, `rows.rs` what the table files share, and a file per table family: `hosts.rs`, `consoles.rs`, `accounts.rs`, `projects.rs` (with trusted folders), `settings.rs`, `agents.rs` (the added agents list), `kept_messages.rs` (messages kept for interrupted sessions: shared information, and the reports and notices for a parent session), `pages.rs`, `sessions.rs`, `session_links.rs` (binding to an owner, lock, owed parent-change notices) and `branch_origins.rs` |
| `src/state/` | Shared daemon state (`AppState` in `mod.rs`, with `broadcast_settings`) with a file per concern holding its methods: the session records' writes (`sessions.rs`), `status.rs` (status transitions; `shown_status` is the status a client is shown) and `titles.rs`; `live.rs` (running sessions), `claims.rs` (launch and exclusive claims), `sharing_lock.rs`, `sharing_status.rs` (each account's sharing state as clients are told it), `process_exit.rs` (the stop of every session at shutdown and the output saved as each one ends); `mcp_tokens.rs`, `turns.rs` (turn bookkeeping), `left_for_limit.rs`; `write_queue.rs` (the facade over `src/messaging/outbox.rs`); `requests.rs` (pending session requests), `background.rs`, `record_watch.rs`, `git_status.rs` (each project's live git status), `agent_availability.rs`, `stalled.rs`, and `forget.rs` (what is dropped where a record is removed); the shell terminal registry, the model cache, account quotas and logins, pending quota recovery waits and the hook queues are fields of `AppState` owned by their own modules |
| `src/config_document/` | The configuration document that carries the stored configuration between devices: `export.rs` building what `export_configuration` answers with, and `import/` reading one back (`inspect` and `apply`; `mod.rs` for the document, projects and settings, `accounts.rs` for its accounts) |
| `src/agents/` | What the daemon knows about each agent CLI apart from launching it: `availability.rs` works out, once per daemon start, which agents are available and each one's default account; `models.rs` lists the models an agent offers for an account (`ModelCache`); `launch_settings.rs` normalises and checks a model and reasoning effort; `codex_app_server.rs` is what every conversation with `codex app-server` shares: the quiet start for an account outside a session (`CODEX_APP_SERVER_ARGS`, `CODEX_QUIET_FEATURES`) and the `initialize` handshake a session's private server opens with too; `layout.rs` is where each agent keeps its files on disk, one `AgentLayout` per agent (`layout`): the variable naming its configuration directory and the directory used without one, the directory and depth of a session's records, and the files kept beside them; names only one agent's own logic uses (Codex's rollout naming, Grok Build's session marker) are constants beside its descriptor. The record code, the account switch's copy, the sharing catalog, the quota and login code and the adapters read these names from here; a project's own `.claude/` or `.codex/` is not the agent's directory, so a name read from it stays where it is read. Agent-generic code reads the descriptor's fields, code inside one agent's branch the named constants |
| `src/paths.rs`, `src/instance_lock.rs` | Where Octoboard keeps its files under `~/.octoboard`, and one daemon per data directory |
| `src/log_file.rs` | The size-rotated `daemon.log` under `paths::logs_dir()` that the daemon's logging writes to besides stderr, enabled only once the instance lock is held |
| `src/rfc3339.rs` | Reading the timestamps the agents write, and writing the compact UTC stamp backups are named with |

### Agent accounts

| Path | Role |
|---|---|
| `src/accounts/` | The agent accounts; `mod.rs` holds how two account names are compared (`account_name_key`, and `clears_default_account_name` for the default account's), and the files below the rest |
| `src/accounts/quotas/` | Each account's plan quota read from its agent's own CLI, and the email it is signed in with (`email.rs`, read from the CLI's local sign-in record and carried on each reading): the held readings (`mod.rs`), the background poller and the on-request and on-demand refreshes, with what makes an account due and when a kick is scheduled (`poll.rs`), what the CLI is asked and how a reply is read (`probe.rs`), the minimal request that starts a reset usage window (`kick.rs`), a Codex account's usage limit reset credits carried on its reading and spent through `use_reset_credit` (`use_credit.rs` holds the function and its per-account guard; `reset_credit.rs` asks `codex app-server` to consume one), the used-up judgement (`judgement.rs`, behind `AccountQuotas::exhausted`), and verified availability for automatic recovery (`AccountQuotas::verified_available`); completed readings notify `auto_switch` |
| `src/accounts/login/` | Signing an account in with its agent's own CLI in the background (`AccountLogins`: one per account, timeout, cancel) |
| `src/accounts/cli_run.rs` | Running an agent's CLI as an account for a short query outside a session (a quota probe or kick, a reset credit's use, a model listing): `admitted_command`, the command admitted under the account's directory for the whole run (`shared_config::admitted_reading`); such a run is held to `processes::subprocess::Bounds::agent_query` |
| `src/accounts/dirs.rs`, `src/accounts/relocate.rs` | An account's configuration directory as text (`normalize_account_dir`, `account_dir_key`), and copying a session's conversation record between configuration directories for an account switch (`locate` finds a conversation record) |
| `src/accounts/shared_config/` | An account sharing its agent's default configuration (below); `items.rs` is the catalog of entries that may be shared |
| `src/accounts/trash.rs` | Moving a removed account's config directory to the Trash (`delete_account` with `trash_directory`, `inspect_account_trash`): refused while its sessions run, and on `trash_safety`'s rules with the other accounts', the agents' default and the projects' directories in the way; judged before and again under the account's lock, the sign-in ended first |
| `src/accounts/switch_continue.rs` | `should_continue`: whether a session an account switch relaunches is sent `CONTINUE_PROMPT` to carry on with its turn; `stopped_on_usage_limit`: whether a conversation's last turn stopped on the plan's usage limit |
| `src/accounts/auto_switch.rs` | Automatic continuation of turns stopped by a usage limit: `after_failed_turn` starts account switching through the coordinator; `RecoveryState` owns pending recovery waits, and `after_quota_refresh` reconsiders them after quota readings |

### Host work: git and filesystem

| Path | Role |
|---|---|
| `src/git/` | The daemon's one layer for running `git` and reading a repository's layout from disk: `env.rs` (what every `git` shares: non-interactive, read-only, variable filtering), `run.rs` (the one runner, `UserGit` with a call-site family's `Policy`, `run` / `run_checked`, `GitError`; the revert gate's own `revert_gate/git.rs`, which runs inside the user's git hook, is the exception), `layout.rs` (a worktree's git directory and common directory, repository discovery, from git's own files without running `git`), `repo_files.rs` (what a repository's other administrative files say), `worktree_list.rs` (the parsed `git worktree list`), `isolated.rs` (`IsolatedGit`, the isolated invocation browse reads use), and `branch_git.rs` (`BranchGit`, the `git` and worktree reads behind sessions' branches, which is `HostTools.git`) |
| `src/project_git/` | Git work on a project's own repository: the runners the rest share (`mod.rs`), the branch header (`header.rs`), status against the remote and fast-forward (`status.rs`), push, switch to a local branch, restore of tracked changes, the recorded remote URL (`remote.rs`) and what a remote URL says (`remote_url.rs`), the ref watch that republishes the branch header (`watch.rs`), and clone and remote probing (`clone.rs`) |
| `src/blocking_pass.rs` | `each_blocking` and `BLOCKING_WIDTH`, the width of a pass of blocking per-entity work and the runner that holds a pass to it |
| `src/browse/` | Project-browsing reads, over the control socket and for the MCP tools that look at a project's changes (below) |
| `src/wire_path.rs` | `RelPath`, the canonical wire form of a path or branch name, used crate-wide |
| `src/hostfs/` | Host filesystem work: `mod.rs` (expanding typed paths, telling "not found" from other errors, requiring a directory, and bounded reads: `read_bounded` reads any source within a bound with an optional cancel, which `browse::live` reads a project's files through too, and `open_regular` / `read_regular` read a file the project does not own without blocking and only if it is a regular file, with the text forms `read_regular_text` and `utf8_text`), `identity.rs` and `place.rs` (telling directories apart by device and inode, and where a directory really is), `listing.rs` (directory listing, repository discovery, agent detection), `replace.rs` (`replace_file`, the one way a file is replaced whole: staged beside it, flushed and renamed over it, with the permissions the caller asks for) and `trash.rs` (`move_to_trash`, which reports a neutral `TrashFailure` for each caller to word) |
| `src/host_open/`, `src/path_links/mod.rs`, `src/host_tools.rs` | Launching the system file manager, default application or terminal (`host_open/paths.rs`, `host_open/terminal.rs`, the shared launcher start in `mod.rs`), probing and opening the file paths a terminal shows, and `HostTools`, the seam a test substitutes stand-ins behind (the user's `git` for the branch cleanup, the Trash, the shell a shell terminal runs) |
| `src/trash_safety.rs` | Whether a directory may be moved to the Trash, or a linked worktree removed by the branch cleanup (`Worktrees`), without taking away what belongs to something else: protected directories (the home directory, a disk's root, Octoboard's data), and paths that must keep their files (`Overlap`), told apart by device and inode, a path that cannot be resolved counting as in the way; each caller words its own refusals |
| `src/project_trash.rs` | Which folder removing a project may move to the Trash, on `trash_safety`'s rules: the project refusals and their wording; an inspection's refusal that a path that could not be resolved decided, or the worktrees outside the folder it held back, is kept under a token, for the removal confirmed after it to repeat |
| `src/env_shell/` | The user's real shell environment every agent is launched with, and resolving a binary on it |

### Running sessions

| Path | Role |
|---|---|
| `src/terminal/` | Processes on a PTY and what clients see of them: `pty.rs`, `ptyio.rs` and `ringbuf.rs` the PTY core every process on a PTY shares (`PtyCore`: spawn, output fan-out over a replay ring and a parsed screen, input, resize, stop with escalation, and non-blocking PTY I/O); `snapshot.rs`, `title.rs` and `saved_output.rs` parsed terminal snapshots and dormant previews, OSC title parsing, and previews persisted per ended session; `reply_route.rs` which answers to a terminal program's queries reach it (with several clients attached to one terminal, each runs an emulator that answers, so the daemon lets one client's answer for a point in the output through and drops the rest and the stale; also tells a client attaching how much of its replay is settled); `shell/` shell terminals, a project's plain login shell on a PTY (`ShellTerminals` on `AppState`) with create, rename, close and a watcher for exit and busyness (`busy.rs`); the wire contract is "Shell terminals" in [`PROTOCOL.md`](PROTOCOL.md) |
| `src/session/` | One running agent process (`LiveSession` in `mod.rs`), terminal input and message-write gates; `term/` launches agents with their owned sidecars and routes automatic messages to checked PTY writes or native submission (`stash.rs` checks whether a launch supports the native stash binding); `draft.rs` tracks user edits, input boundaries and submitted prompts reported by hooks; `swap.rs` owns native composer-stash transactions, retained keyboard input and recovery for the same launch; `input_box.rs` reads what an agent's input box holds off its screen; `dialogs.rs` recognises dialogs an agent raises that only a person can answer, and holds the session's writes while one is up and retries pending message delivery; `trust/` each agent's folder-trust confirmation (recognising the screen, pressing it, and holding a session's writes meanwhile); `interrupt/` `interrupt_session`, shared by the control request and the MCP tool, which stops the turn a running session is in with the agent's own interrupt key |
| `src/titles/` | The title a session shows (`mod.rs`), put together from the agent's own records, the terminal title and the first prompt; `activity.rs` and `interrupt.rs` what an agent's terminal title says about its turn, and closing a turn from the title when a cancel leaves nothing in the record or hooks |
| [`src/processes/ledger/`](src/processes/ledger/README.md) | The agent-launch process ledger: attributing the processes an agent launch started, wherever they went, from the native process table; listing them as trees, stopping, keeping or moving them, carrying them over to another launch, and stopping what a launch left when it ends |
| `src/processes/` | The rest: `requests/mod.rs` the client-facing process requests (selects the sessions, the projects and their local directories for `list`, `stop`, `keep` and an archive inspection's `long_running`, and runs the ledger's blocking work off the async runtime), `subprocess.rs` and `leader.rs` (running a spawned process to completion within bounds, `Bounds::agent_query` being those of an agent CLI's short query, and starting, observing and ending a process as the leader of a session of its own), `pids.rs` (`all`, every process id on the system, which the session sweeps, the ledger's process table and a shell terminal's busy check filter), and `crash_cleanup.rs` (ending every process on a PTY when the daemon goes down on an internal failure) |

### Agent records, hooks and adapters

| Path | Role |
|---|---|
| `src/hooks/` | Reading what an agent's hook payload says (`mod.rs`), the per-session ordered queue of unapplied hook events (`queue.rs`), and what applying an event does to the session's status, turns, background tasks and record watches (`apply.rs`) |
| `src/records/` | An agent's own on-disk session records: `mod.rs` finds them by the names in `src/agents/layout.rs` and tells whether a conversation can still be resumed; `collector.rs` is the one background collector that persists what the records show of a session, from `titles.rs` (the titles), `observed.rs` (model and effort), `default_effort.rs` (the effort a session runs at when nothing sets one) and `context.rs` (context size); `conversation.rs` reads a conversation record into one sequence for every agent, and `history.rs` is `read_session_history`, a session's history for the UI's history view and the MCP tool of the same name; `transcript.rs` (Claude Code) and `rollout.rs` (Codex) watch the record for status changes hooks never report, over `watch.rs`; every pass over a record, incremental or one-off, goes through `line_file.rs`, the bounded line reader (it also opens a record, decompressing one Codex compressed) |
| `src/adapter/` | One adapter per agent CLI (below) |
| `src/mcp/` | The Octoboard MCP server (below) |

### `src/accounts/shared_config/`

Each account keeps whether it shares the default account's configuration directory and whether the last change toward
that choice is incomplete. A change runs at once or is refused, ends the account's idle sessions first and resumes them
afterwards, and reports its progress as `account_sharing`; running a change as an operation (`set`, and `before_delete`
and `before_repoint`, which turn sharing off before an account is deleted or pointed at another directory) is
`src/coordinator/account_sharing.rs`'s, and this module holds what it does to the directories, the locks it takes, and
when it tells its progress. The wire contract is "Sharing the default configuration" in
[`PROTOCOL.md`](PROTOCOL.md); the items that may be shared are the catalog in `src/accounts/shared_config/items.rs`.

External interfaces (re-exported from `mod.rs`, defined where each says):

- `lock_account`, `Locked`, `Refused`, `Changing`, `carry_out` (`change.rs`) — lock an account for a change of its
  sharing, keep its progress until the change ends, and carry the change out on the account's directory; for
  `src/coordinator/account_sharing.rs`.
- Also for `src/coordinator/account_sharing.rs`: `in_use` and `default_dir` (`mod.rs`), what runs under an account's
  directory and where the default one is; `account_in_use`, `still_sharing`, `not_shareable`, `sharing_in_progress`
  and `sharing_failed` (`mod.rs`), its refusals; `lock_agent` and `try_lock_agent` (`mod.rs`), the agent's lock;
  `is_same_directory` (`mod.rs`); `entries` (`link.rs`), how many entries a change goes through;
  `own_conversations` (`take_back.rs`), the conversations turning sharing off takes back; and `FailureCause`.
- `before_launch` (`launch.rs`) — before a session launches or resumes: refuses while its account's change runs, and
  repairs links.
- `refuse_if_changing`, `before_switch_copy` (`mod.rs`) — refuse opening a session, a handoff or an account switch
  while a change of the account involved runs.
- `Sharing` (`mod.rs`) — what `AppState` holds: the per-agent locks (`locks.rs`).
- `admitted_reading`, `running_long` (`cli_reads.rs`) — taken by agent CLI runs outside a session, which a change, or
  the move of a removed account's directory to the Trash, waits for or refuses.
- `hold` (`cli_reads.rs`) — taken by a sharing change and by an account's removal (`src/accounts/trash.rs`) for as
  long as it moves files under the directory; no CLI run starts there meanwhile.

None of these is what clients are told of each account's sharing: `AppState` publishes it
(`src/state/sharing_status.rs`: `publish_account_sharing` wherever what runs under an account's directory may have
changed, `account_sharing_states` for a `snapshot`, and `set_sharing_progress`, `start_sharing_change` and
`end_sharing_change`, the progress and failure a change tells), reading what runs there through `in_use`. The lock over
an account's change is not in this module either: it lives on `AppState` (`lock_account_sharing`), under the same lock
as the sessions' launch claims, and `begin_launch` refuses a session of a locked account.

Internals, by file:

| File | Role |
|---|---|
| `link.rs`, `moves.rs` | Turning sharing on and off item by item over the catalog and repairing links an agent replaced, and the file-move primitives it uses (across volumes, backup naming, merging directories) |
| `take_back.rs` | On turning sharing off, moving the account's own sessions' conversations back out of the default directory |
| `codex_state.rs`, `registries.rs` | Keeping Codex's thread state database and the plugin registries' recorded paths consistent with shared or unshared items |

### `src/mcp/`

The tool surface a session drives Octoboard with. Every session has the same reads (sessions, histories, branches,
consoles, projects, archives, models, plan quotas, and a project's changes and branches); console sessions add
orchestration and settings tools, project coordinators a narrower orchestration set, and project sessions with a
parent the reporting tool. Every session has information sharing, `request_session` and `request_resume_session`.
Which tools a session sees follows its kind, coordinator or worker capability, and parent relationship, which the
daemon checks on every call. The wire contract is in [`PROTOCOL.md`](PROTOCOL.md).

| File | Role |
|---|---|
| `mod.rs` | The tool catalogue, and which tools a session may see by kind, coordinator or worker capability, and parent relationship |
| `role.rs`, `console_instructions.rs` | The role description injected at launch, and the console session's instruction file |
| `exec/`, `args.rs` | Dispatching one tool call through the same coordinator and reporting functions the control socket uses (`exec/mod.rs`: the call, its caller and the dispatch; the tools by family in `projects.rs`, `sessions.rs`, `messaging.rs` and `archive.rs`; `resolve.rs` finding the session a tool acts on), and reading a call's arguments |
| `reads.rs`, `models.rs`, `history.rs`, `quotas.rs`, `session_branches.rs` | The read-only tools every role has: `get_session`, `list_sessions`, `list_projects`, `list_consoles`, `list_archived`, `list_agent_models`, `read_session_history`, `get_account_quotas`, `get_session_branches` |
| `project_changes.rs`, `project_branches.rs`, `project_browse.rs` | The read-only tools every role has for looking at a project's repository: `list_project_changes` (a worktree's uncommitted changes, a branch against a base, by default its recorded target branch, with its commits, or one commit) and `read_project_diff` (one file's diff, in windows) in `project_changes.rs`, `list_project_branches` (a repository's branches with the session recorded as responsible for each) in `project_branches.rs`; `project_browse.rs` is what they share, one `browse::serve` request run under the daemon-wide gate with the requests the window sends |
| `handoff.rs`, `handover.rs`, `process_handover.rs`, `bindings.rs`, `account_switch.rs` | The parent-relationship and child-session management tools: `handoff_session`, `handover_branches`, `handover_processes`, `transfer_session`, `release_session` and `bind_to_session`, `switch_session_account`; `handover.rs` also holds the branch checks (`branch_names`, `held_by`) that `new_session` and `request_session` use and the skeleton `handover_processes` shares (`Parties`, the two sessions under their claims, and telling them what moved), and `bindings.rs` reads these tools' arguments and tells the caller what became of each session concerned, the notices themselves being `src/messaging/owner_notices.rs`'s |
| `session_model.rs` | `set_session_model`: changes the model and reasoning effort of a session the caller owns through the coordinator |
| `resume_request.rs` | `request_resume_session` |
| `pages.rs`, `settings.rs` | A session's report panel pages (`show_page`, `list_pages`, `delete_pages`), for console sessions and project coordinators, (withdrawing a worker's pages is `src/coordinator/pages.rs`), and the settings tools (a project is described with its stored settings and what they resolve to, `effective`) |
| `stdio.rs` | `octoboardd mcp` itself: the stdio child forwarding calls to the daemon over loopback, including a tool of the role's other binding catalogue, which the daemon then answers or refuses |

### `src/browse/`

Serves the project-browsing control-socket requests (`get_project_source`, `list_project_dir`, `search_project_files`,
`read_project_file`, the change, branch-comparison and commit listings and reads, and the `…_change_bodies` requests):
bounded reads from the disk, the index, a branch or a commit. The wire contract and budgets are in "Browsing a project"
in [`PROTOCOL.md`](PROTOCOL.md). `mod.rs`'s `serve` is the entry point, called from `server/control.rs` for the control
socket and from `mcp/project_browse.rs` for the MCP tools of the same reads.

| File | Role |
|---|---|
| `mod.rs`, `source.rs` | `serve`, and resolving a project to its root, repository and worktrees |
| `live.rs`, `blob.rs`, `ignored.rs` | Reading and listing files on disk and the file-name search, reading blobs from the index or a commit, and which entries Git ignores |
| `changes.rs`, `compare.rs`, `commits.rs`, `tree_diff.rs`, `line_counts.rs` | A worktree's uncommitted changes, branch comparisons, the commits a branch made since it forked, and the shared reading of `git`'s tree-diff and line-count output |
| `budget.rs`, `lane.rs` | The limits every read is held to, and the per-connection lane, with `Gate::admit`, which admits an MCP tool's read to the same budget outside any connection's lane |

### `src/adapter/`

A single adapter interface (launch, session id, capability injection, status reporting, resume). It is not the only
per-agent code: where each agent keeps its files is `src/agents/layout.rs`, and what differs in how the agents behave
is known where it is used, among them their screens and dialogs in `src/session/` and `src/titles/`, their hook
payloads in `src/hooks/`, their records in `src/records/`, their sign-in, quota and account-switching protocols in
`src/accounts/`, tool naming, the instruction file and role text in `src/mcp/`, launch and account switch in
`src/coordinator/`, model catalogues in `src/agents/models.rs` and detecting a project's agent in
`src/hostfs/listing.rs`. Status hooks, the Octoboard MCP server and a role description are injected when a session
starts, along with the agent's approval override where the agent has one.

| File | Agent |
|---|---|
| `mod.rs` | The adapter trait and the agent-agnostic launch/resume flow shared by all three, `account_dir`, the directory an account's session sees (its pinned one, else the agent's default), and `account_command`, the command for a short query of an agent's CLI as an account would run it |
| `claude/` | Claude Code, with `settings.rs` reading the settings files a launch depends on |
| `codex/` | Codex: `mod.rs` the adapter, `private_server.rs` `PrivateServerPlan` for the app server and remote TUI launch, `transport.rs` (`CodexTransport::start`, `endpoint`, `send` and `stop`: owns a launch's private Codex app server, provides its remote TUI endpoint, and submits messages through the Unix WebSocket queue independently of the composer), `unarchive.rs` which brings an archived rollout back before a resume, `config.rs` reading the Codex home's `config.toml` and models cache and a project's `.codex/config.toml` server names, `claude_compatibility.rs` the developer-instructions text (`claude_compatibility.md`) and `reserved_env`, the environment policy for Claude Code's MCP servers |
| `grok/` | Grok Build: `mod.rs` the adapter and launch plan, `sandbox.rs` the sandbox refusal, `home.rs` the per-session `GROK_HOME` farm, `writeback.rs` the shared-config writeback |
| `claude_mcp.rs` | Claude compatibility for Codex: the MCP servers Claude Code would load, turned into Codex `-c mcp_servers.*` overrides, secrets carried in the environment |
| `files.rs` | Bounded, non-blocking text-file readers: `read_bounded_text` for project-scoped adapter configuration, and `src/hostfs/`'s `read_regular_text` re-exported for agent-owned configuration, caches and install manifests |
