# `octoboardd`

A headless Rust binary that plays two roles in one process:

- **Host role**: owns PTYs and agent processes and receives their hook callbacks; works on the machine and its
  repositories on behalf of clients (directory listings, repository discovery, clone, remote probing, git status,
  push, branch switch, restore, opening a project in the system terminal or file manager, agent and model detection);
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
- `octoboardd mcp --session <id> --role <role> [--bound] --port <port> --token <token>` — CLI mode that is the MCP
  server each adapter registers for a session; speaks MCP on stdin/stdout and forwards tool calls to
  `POST /mcp/:token`.

## Layout

| Path | Role |
|---|---|
| `build.rs` | Exposes the app name from the repo-root `config/app.json` as `OCTOBOARD_APP_NAME` |
| `src/main.rs` | CLI entry point: parses the four modes, opens the store, binds the port, prints the handshake line the application waits for, runs the server |
| `src/server.rs` | The HTTP/WebSocket router described in `PROTOCOL.md`, including `POST /mcp/:token`, `POST /git-ref/:session` and `POST /hook/:session` |
| `src/access.rs` | The middleware every route sits behind: refuses with `403` a request whose `Host` or `Origin` is not a local client's or the application's own UI's |
| `src/protocol.rs` | Rust types for the wire protocol (kept in sync by hand with `PROTOCOL.md` and `packages/ui/src/protocol.ts`), the console session colours, and the one mapping from a failure to an `error` reply |
| `src/coordinator.rs` | Coordinator role: what each control-socket request does to the stored consoles, projects, sessions, pages and accounts, and which host-role work it triggers (session open/launch, binding and transfer, archive/reopen/delete cascades, account resolution, launch settings, console and project association and edits) |
| `src/session_request/` | `request_session`: a session asking the user to approve a session with no owner, with the pending requests, the wait for the answer and the carrying out of an approval |
| `src/reporting.rs` | The channel between an owner and the sessions bound to it: briefs, messages into a running session, reports reaching the owner or synthesised when a session stops, automatic archiving, and report-panel form submissions |
| `src/sharing.rs` | Information one session shares with another, delivered through `reporting.rs`'s message writing |
| `src/relocate.rs` | Copying a session's conversation record between account configuration directories for an account switch |
| `src/outbox.rs` | The per-session ordered queue every message Octoboard writes into an agent passes through |
| `src/store.rs` | The coordinator's SQLite storage: consoles, projects, sessions, pages, agent accounts, branch origins, trusted folders, user settings and the host table |
| `src/state.rs` | Shared daemon state: session status and turn bookkeeping, record-watch bookkeeping, each project's live git status and check claim, pending `request_session` requests, agent availability, the model cache and the hook queues |
| `src/launch_settings.rs`, `src/agent_models.rs` | Normalising and checking a model and reasoning effort (`LaunchCheck`), and listing the models an agent offers for an account (`ModelCache`); fixtures in `testdata/` |
| `src/availability.rs` | Working out, once per daemon start, which agents are available and each one's default account |
| `src/config_export.rs`, `src/config_import.rs` | Building the configuration document `export_configuration` answers with, and reading one back (`inspect` and `apply`) |
| `src/git_env.rs` | The base every `git` the daemon runs is built from (non-interactive, read-only, variable filtering) |
| `src/git_status.rs`, `src/git_push.rs`, `src/git_switch.rs`, `src/git_restore.rs`, `src/git_remote.rs`, `src/git_watch.rs` | Per-project git: status check against the remote and fast-forward, push of the current branch, switch to a local branch, restore of tracked changes, the recorded remote URL, and the ref watch that republishes the branch header |
| `src/branch_origin/` | Which session is responsible for a git branch: `ref_created`, `annotate`, `hand_over`, `project_deleted`, plus the git hook dispatchers (`git_hooks.rs`, `dispatcher.sh`) that report created branches |
| `src/session.rs`, `src/term.rs`, `src/ptyio.rs`, `src/ringbuf.rs` | One running agent process and its PTY: launch, output fan-out and replay buffer, non-blocking PTY I/O, writing messages into a session |
| `src/terminal_snapshot.rs`, `src/terminal_title.rs`, `src/saved_output.rs` | Terminal state: parsed snapshots and dormant previews, OSC title parsing, and previews persisted per ended session |
| `src/crash_cleanup.rs` | Ending every agent process when the daemon goes down on an internal failure |
| `src/trust/` | Each agent's folder-trust confirmation: the permission decision and prompt, recognising each agent's screen, and pressing it |
| `src/env_shell/` | The user's real shell environment every agent is launched with (`snapshot`, `launch_snapshot`, `cached_snapshot`) and resolving a binary on it |
| `src/hostfs.rs`, `src/open_directory.rs`, `src/open_terminal.rs` | Host filesystem work (browsing, Trash, repository discovery, clone, remote probing, agent detection) and launching the system terminal, file manager or default application |
| `src/subprocess.rs` | Running a spawned process to completion within bounds (`run_bounded`, `run_with_timeout`) and detached spawning |
| `src/hooks.rs`, `src/hook_queue.rs`, `src/hook_mode.rs` | Reading what an agent's hook payload says, the per-session ordered queue of unapplied hook events, and the `octoboardd hook` mode |
| `src/transcript.rs`, `src/rollout.rs`, `src/record_watch.rs`, `src/line_file.rs` | Watching an agent's own session record for status changes hooks never report (Claude Code transcript, Codex rollout), over a shared offset-tracked line reader |
| `src/agent_records.rs`, `src/record_titles.rs`, `src/session_title.rs`, `src/usage.rs` | Where each agent keeps its on-disk session records, the titles they give a conversation, the title a session shows, and native token statistics |
| `src/git_ref_mode.rs`, `src/loopback.rs` | The `octoboardd git-ref` mode, and the minimal HTTP client for the daemon's own loopback address shared by the CLI modes |
| `src/instance_lock.rs`, `src/paths.rs` | One daemon per data directory, and where Octoboard keeps its files under `~/.octoboard` |
| `src/test_support.rs` | Test-only fixtures shared by the modules' unit tests (scratch directories, stored state, stand-in sessions, git repositories) |
| `src/mcp/` | The Octoboard MCP server (below) |
| `src/browse/` | Project-browsing reads over the control socket (below) |
| `src/adapter/` | One adapter per agent CLI (below) |

### `src/mcp/`

The tool surface a session drives Octoboard with: orchestration tools for console sessions, a narrower set for project
sessions that can own sessions, the reporting tool for bound sessions, settings tools, information sharing between
sessions, and `request_session`. Which set a session sees follows its role and its current binding, which the daemon
checks on every call. The catalogue is shared by both sides of the stdio bridge. The wire contract is in
[`PROTOCOL.md`](PROTOCOL.md).

| File | Role |
|---|---|
| `mod.rs` | The tool catalogue, and which tools a session may see by role and binding |
| `role.rs`, `console_instructions.rs` | The role description injected at launch, and the console session's instruction file written into its working directory |
| `exec.rs` | Runs one tool call through the same coordinator and reporting functions the control socket uses |
| `bindings.rs` | `transfer_session` and `bind_to_session`, and what they tell the sessions concerned |
| `args.rs` | Reading a tool call's arguments and finding the records they name |
| `settings.rs` | The settings tools for a console, its projects and the application, and a project session's read of its own project |
| `stdio.rs` | `octoboardd mcp` itself: the stdio child forwarding calls to the daemon over loopback |

### `src/browse/`

Serves the project-browsing control-socket requests (`get_project_source`, `list_project_dir`, `search_project_files`,
`read_project_file`, `list_project_changes`, `read_project_change`, `list_project_branches`,
`compare_project_branches`, `read_project_comparison_change` and the `…_change_bodies` requests): bounded reads from
the disk, the index, a branch or a commit, a worktree's uncommitted changes, and the changes one branch has made since
it forked from another. The wire contract and budgets are in "Browsing a project" in [`PROTOCOL.md`](PROTOCOL.md).
`mod.rs`'s `serve` is the entry point, called from `server.rs`.

| File | Role |
|---|---|
| `mod.rs` | `serve`, `locate` (repository and worktree for a Git read), `resolve_project_path` |
| `source.rs` | Resolving a project to its root, repository, worktree and sibling worktrees, and the identities clients hold |
| `wire_path.rs` | `RelPath`, the canonical wire form of a path |
| `live.rs` | Reading and listing files on disk, and the bounded file-name search `search_files` |
| `blob.rs` | Reading blobs from the index or a commit, and resolving branches and commits |
| `git.rs` | The one way browse runs `git`: isolated, bounded, never writing to the repository |
| `changes.rs` | A worktree's uncommitted changes: `list`, `read`, `read_between` and the bodies reads |
| `compare.rs` | Local branches and the comparison of two of them: `branches`, `list`, `read`, `read_bodies` |
| `budget.rs`, `lane.rs` | The limits every read is held to, and the per-connection lane (outstanding-request bound, superseding, retained-bytes reservation) |
| `tests.rs`, `change_tests.rs`, `compare_tests.rs`, `nested_tests.rs`, `body_tests.rs` | Test-only end-to-end tests over real repositories |

### `src/adapter/`

A single adapter interface (launch, session id, capability injection, status reporting, resume), so the rest of the
daemon is agnostic to which agent it drives. Status hooks, the Octoboard MCP server and a role description are
injected when a session starts.

| File | Agent |
|---|---|
| `mod.rs` | The adapter trait and the agent-agnostic launch/resume flow shared by all three, and `config_dir_var` |
| `claude.rs` | Claude Code |
| `codex.rs` | Codex |
| `grok.rs` | Grok Build |
