# `octoboardd`

A headless Rust binary that plays two roles in one process:

- **Host role**: owns PTYs and agent processes, receives hook callbacks, lists directories, finds the git repositories
  under a parent directory, clones a repository, probes a git remote and detects which agent a project is set up for,
  checks a project's git status against its remote, and serves bounded, source-aware reads of a project's files (on
  disk, in the index, in a branch or commit).
- **Coordinator role**: stores consoles, projects, sessions, report panel pages and the agent accounts in SQLite, and
  routes requests to the host role.

The desktop application (`apps/desktop/`) is purely a client of this process. It never shares state or an IPC channel with it —
everything it can do goes through the external interface below.

## Development

This crate is a member of the repository's root Cargo workspace, alongside the desktop shell's Tauri crate: the
lockfile and the build directory are the root's `Cargo.lock` and `target/`. `cargo build` and `cargo test` run from
the repository root and cover both crates; `-p octoboardd` narrows either to this one. The Tauri crate resolves its
`octoboardd` sidecar at compile time, so on a fresh checkout a workspace-wide build or test fails until
`pnpm --filter @octoboard/desktop build:daemon` has run once.

## External interfaces

- WebSocket/HTTP server, bound to `127.0.0.1` on an OS-assigned port — the only way the desktop application (or a
  future remote-host connection) talks to the daemon. Specified in [`PROTOCOL.md`](PROTOCOL.md), which is
  authoritative; this doc does not restate it.
- `octoboardd hook --session <id> --port <port>` — one of two other CLI modes, not the server. This is what the
  per-session hook script execs on every agent hook event; it reads the hook's payload from stdin and posts it to the
  running daemon's `POST /hook/:session`.
- `octoboardd mcp --session <id> --role <role> [--bound] --port <port> --token <token>` — the other CLI mode. This is
  the MCP server each adapter registers as a session's `command`-type MCP server (see `src/mcp/` below); it speaks MCP
  on stdin/stdout and forwards each tool call to the running daemon's `POST /mcp/:token`. Both are specified in
  [`PROTOCOL.md`](PROTOCOL.md).

## Layout

| Path | Role |
|---|---|
| `build.rs` | Exposes the app name from the repo-root `config/app.json` as `OCTOBOARD_APP_NAME`, for the wording the daemon itself shows the user |
| `src/main.rs` | CLI entry point: parses the three modes above, opens the store, binds the port, prints the handshake line the application waits for, runs the server until shutdown |
| `src/access.rs` | The middleware every route sits behind: turns away, with `403`, a request whose `Host` or `Origin` is not one a local client or the application's own UI would send; the rule is in `PROTOCOL.md` |
| `src/server.rs` | The HTTP/WebSocket router described in `PROTOCOL.md`, including `POST /mcp/:token` |
| `src/protocol.rs` | Rust types for the wire protocol; kept in sync with `PROTOCOL.md` and with `packages/ui/src/protocol.ts` by hand |
| `src/coordinator.rs` | Coordinator role: what each control-socket request does to the stored consoles/projects/sessions/pages/accounts, and which host-role work it triggers, including the launch flow a resume and a switch of a session's account share, the binding of an unbound project session to a console session as a lead session (`bind_to_console_session`), and the archive, reopen and delete cascades over the sessions bound under a session, down to a lead session's own; projects are stored with absolute, lexically normalised paths |
| `src/console_request.rs` | A project session's request for a console session (`request_console_session`): the requests waiting for the user's answer, held in memory and put to every client again after a snapshot, the call that waits for the answer up to a time limit of Octoboard's own, its withdrawal when the caller's process ends, the call's connection is dropped or the caller asks again, and carrying out an approval — a console session started as one opened by hand, the caller bound to it as a lead session through `coordinator::bind_to_console_session`, its request delivered as its first report and the outcome also written into the caller's session, with the turns that outcome and a backgrounded call's result start spared a synthesised report |
| `src/reporting.rs` | The channel between an owner (a console session, or an unbound or lead project session that started sessions) and the project sessions bound to it: the brief a task is handed over as, writing a message into a running session, a report reaching its owner (a lead session's final report is held back until its sessions are archived), the report synthesised when a session stops without sending one (withheld for a lead session while its sessions have a process), `close_turn_and_report`, the one path that closes a turn and has that report made, shared by the hook handler and the record watches, automatic archiving, and rendering a report panel form submission into the console session's message |
| `src/sharing.rs` | Information one project session shares with another: the quoted, framed message the receiver reads, and the copy sent to the receiver's owner, delivered through `reporting.rs`'s message writing |
| `src/relocate.rs` | Copying one session's conversation record from one account's config directory into another's, for a switch of the session's account: finding the record by name under the agent's root, and copying it to the same path relative to the directory, staged and checked before it replaces anything |
| `src/outbox.rs` | The per-session queue every message Octoboard writes into an agent passes through: order-preserving, one drainer per session, and what happens to a message the session only partly accepted |
| `src/store.rs` | Coordinator's SQLite storage for consoles, projects, sessions, pages, agent accounts, the trusted folders, the user settings and the host table |
| `src/state.rs` | Shared daemon state: the session status transitions, the turn bookkeeping a synthesised report rests on (including the credits that make a turn started by Octoboard's own message, or by the result of a call the agent moved to the background, count as reported, each told apart by the prompt it opens with where the agent's prompt is read), the record watches' bookkeeping (which watch is current for a session, and how far into its record file the carried-offset watches have read), each project's live git status and check claim, the requests for a console session waiting for an answer, and every agent's current availability and resolved default account |
| `src/availability.rs` | Working out, once per daemon start, which agents are available and what each one's default account resolves to |
| `src/git_env.rs` | The base every `git` the daemon runs in a user's repository or against a user's remote is built from: `non_interactive`, a command that cannot prompt, and `read_only`, which keeps a read from rewriting the repository's index; the git command builders of `hostfs.rs`, `git_status.rs` and `browse/git.rs` start from it |
| `src/git_status.rs` | Checks one project's git status against its remote and fast-forwards it: automatically when auto-sync is on, and for one project right now on the manual `sync_project_git` request (`sync_project_now`, which ignores the check floor and the setting; a request that finds a check in flight is owed one forced rerun, recorded in the claim map in `state.rs`) — see `PROTOCOL.md`'s "Daemon behaviour, per project" |
| `src/session.rs` | One running agent process: its PTY, its output fan-out, how it is stopped |
| `src/crash_cleanup.rs` | Ending every agent process when the daemon goes down on an internal failure — a panic on the main thread, or any abort — and leaving them running after a panic the daemon survives; the running agents are held in a lock-free table a signal handler can read |
| `src/trust/` | Each agent's own folder-trust confirmation: `mod.rs` decides whether the user has given the one permission all agents share (per project or through a trusted parent folder; console sessions are pressed without asking), asks the application through `trust_prompt` / `confirm_trust` and reports failures; `screen.rs` recognises each agent's screen in a session's terminal output and keeps the session's watch, including holding Octoboard's own messages while a screen may be up; `press.rs` presses it — the only code that types keys into a session on its own; `carry.rs` carries the entry Grok Build writes into its per-session trust store over to the user's own |
| `src/term.rs` | Launching an agent in a PTY, and writing messages into a running one |
| `src/ptyio.rs` | Non-blocking read/write on a PTY master fd (a blocking write can park forever behind a modal dialog) |
| `src/ringbuf.rs` | Fixed-capacity ring buffer holding a session's recent terminal output, replayed to a client that attaches or reconnects |
| `src/saved_output.rs` | The ring buffer's contents kept as one file per session once its process ends (`paths::saved_output_dir`), replayed by the terminal socket for a session with no process; written atomically, removed with the session's record, and swept at startup |
| `src/hostfs.rs` | Host role's filesystem work: browsing directories, finding git repositories under a parent directory, cloning one, probing a remote, detecting which agent a directory or remote is set up for, lexical path normalisation |
| `src/env_shell.rs` | Captures the user's real shell environment (`$SHELL -l -i -c 'env -0 && printf <marker>'`) that every agent is launched with; also a cached variant for a caller on its own repeating schedule (`cached_snapshot`, used by `git_status.rs`) and resolving a binary on that environment's `PATH` |
| `src/subprocess.rs` | Running one subprocess the daemon spawned directly to completion within bounds — a deadline, a ceiling on its output, a cancel, the whole process group killed on any stop: `run_bounded` (output bounded while it is read) and `run_with_timeout` (a thin wrapper over it); used by `hostfs.rs`, `git_status.rs` and `browse/` |
| `src/hooks.rs` | Reads what one agent's hook event payload says — the session's status, its turn boundaries, the prompt a turn was started with, and the tool calls it moved to the background; each agent's events and payload shape differ |
| `src/transcript.rs` | Claude Code's side of watching an agent's own session record: recognises, in the session's transcript JSONL, the one status change its hooks never report — a declined permission prompt or `AskUserQuestion` — arms the watch when a hook raises the hand, and lowers it when the rejection is found |
| `src/rollout.rs` | Codex's side of the same: recognises, in the session's rollout file, the end of a turn that ended in an error — which Codex reports through no hook at all — arms the watch from that turn's `UserPromptSubmit`, and closes the turn as failed |
| `src/record_watch.rs` | The mechanism `transcript.rs` and `rollout.rs` share: an offset-tracked line reader (`scan_lines`) and the poll loop around it (`spawn`), parameterised by poll interval, start offset (at the file's end, or carried across turns) and the caller's per-line check and action |
| `src/hook_mode.rs` | The `octoboardd hook` CLI mode itself |
| `src/loopback.rs` | A minimal HTTP client for the daemon's own loopback address, shared by `hook_mode.rs` and `mcp/stdio.rs` — the two CLI modes that call the running daemon from a separate process |
| `src/mcp/` | The Octoboard MCP server — see below |
| `src/browse/` | Bounded, source-aware reads of a project's files over the control socket — see below |
| `src/instance_lock.rs` | Enforces one daemon per data directory |
| `src/paths.rs` | Where Octoboard keeps its own files, under `~/.octoboard`, and the host's home directory when it is really known (`known_home_dir`), as reported to clients |
| `src/adapter/` | One adapter per agent CLI — see below |
| `src/test_support.rs` | Test-only (`#[cfg(test)]`, not part of the binary): the fixtures the modules' unit tests share — a self-removing scratch directory and file, an `AppState` over a fresh store in one, a stand-in live session on a PTY that ends itself, the helpers for tests that run a fixture shell and check the processes it left (`env_shell` and `subprocess` tests), and the git fixtures: `isolated_git_env` (the process environment without any `GIT_*` variable and with no global or system git configuration, which the `git_status.rs` and `browse/` tests share), `git_command` (the system git, `SYSTEM_GIT`, in a directory in that environment, which the other git fixtures are built on), `git` / `git_bytes` (plain `git` for fixture setup, failing the test on a non-zero exit), `git_stdin` (the same with input piped in) and `repo_with` (a repository in a scratch directory with one commit) |

### `src/mcp/`

The Octoboard MCP server: the orchestration tools the console session drives Octoboard with, the narrower set an
unbound project session, or a lead session, drives its own project with and asks for a console session through
(`request_console_session`, carried out by `src/console_request.rs`), the reporting tool a bound project session answers
through, and the two tools every project session shares information with its project's other sessions through.
Which set a session sees follows from its role and from whether it has the role of a bound session (`--bound`: bound to an owner and not a lead session), both fixed for its lifetime; a lead session, bound only after it was launched unbound, keeps the unbound set.
The tool catalogue is shared by both sides of the stdio bridge, so the child process, the daemon, and the role
descriptions cannot drift apart. The wire-level `POST /mcp/:token` contract is in `PROTOCOL.md`; "The console
session's tools", "The unbound project session's tools", "Information between sessions of a project" and "Reporting"
in `docs/product/hub-orchestration.md` list the tools and what each one does.

| File | Role |
|---|---|
| `mod.rs` | The tool catalogue, and which tools a session may see, by its role (`console` / `project`) and whether it has the bound role |
| `role.rs` | The role description injected at launch, and the console session instruction file written into a console's working directory |
| `exec.rs` | Runs one tool call against the real consoles, projects and sessions, through the same coordinator/reporting functions the control socket uses; the project-scoped reads and `share_info` resolve their targets within the caller's own project |
| `stdio.rs` | `octoboardd mcp` itself: the stdio child process each adapter registers, forwarding every call to the daemon over loopback, and closing the connection of a `request_console_session` call the agent cancels |

### `src/browse/`

Serves the project-browsing requests of the control socket (`get_project_source`, `list_project_dir`,
`read_project_file`, `list_project_changes`, `read_project_change`, `list_project_branches`, `compare_project_branches`,
`read_project_comparison_change`, and the two `…_change_bodies` requests that read a diff's whole file bodies): where a
project's files live (the directory itself, its repository and worktree, the same place in the repository's other
worktrees), and then a bounded read of a directory listing or one file from the disk, the index, a branch or a commit,
or of a worktree's uncommitted changes to the project and one change's diff, the repository's local branches, and the
changes between two branches' tips and one of them as a diff. A request is re-resolved from the store and the repository
each time; nothing a client names is used as a directory. The wire contract, the identities handed to clients, the Git
invocation rules and the budget values are in "Browsing a project" in [`PROTOCOL.md`](PROTOCOL.md) and are not restated
here. `mod.rs`'s `serve` is the entry point, called from `server.rs`, which also owns the connection's lane wiring, the
retained-bytes reservation each request makes and the writer that puts control events before browse replies; `state.rs`
holds the daemon-wide `Gate` (concurrent reads and retained bytes) the lanes draw on.

| File | Role |
|---|---|
| `mod.rs` | `serve`: turns one request into a reply or a coded error, and the marker error a cancelled read ends with; `locate`, the repository and the worktree (the project's own, or another of the same repository, checked now) a Git read works in |
| `source.rs` | Resolving a project to its root, repository, worktree and sibling worktrees; minting and re-checking the identities clients hold; repository discovery that stops below the home directory |
| `wire_path.rs` | The canonical text form a path takes on the wire so no byte of a file name is lost (`RelPath`) |
| `live.rs` | Reading and listing files on disk under a scope, bounded while reading, refusing symlink escapes, links in a path's components and non-regular files |
| `blob.rs` | Reading a file's blob from the index or a commit, and resolving and verifying a branch (a broken one, naming no commit, refused) or commit, and checking a branch name without looking it up (`branch_refname`); the exact-path lookups of the index's entries and of a commit's tree entry, and a blob read by object id, that a change's two sides are read through |
| `git.rs` | The one way browse runs `git`: isolated from the environment and configuration, bounded, output kept as bytes, and never writing to the repository (`GitEnv`); its exact-path runs (`run_exact`) and the predicate for paths that nest (`paths_nest`) |
| `changes.rs` | A worktree's uncommitted changes scoped to a project and one change's diff: `list` (one `git status` of the whole worktree, kept to the entries with a side inside the project, a side outside it named by its repository path alone) and `read` (the `Reader`: one change read afresh at the paths it names from its group's two sources, its index entry and disk file re-checked after the patch is made, the change refused as `source_changed` when they moved; a change with a side outside the project gets no patch), `read_between`, the same read of a change between two commits, which nothing is re-checked after since neither moves, and `read_bodies` / `read_bodies_between`, the same read run without inline bodies and its sides compared with the versions a client names before each file's whole body is read from the matched blob or file version |
| `compare.rs` | A repository's local branches (`branches`, one `for-each-ref`, broken branches listed as they are and cut at a budget) and the comparison of two of them: `list` (the changes between the two tips, one `diff-tree` of the whole tree kept to what touches the project like a worktree's change listing, falling back to the project's own directory when the output overflows for a project below the repository's root) and `read` (one change, read through `changes.rs`'s `read_between` from the two commits the comparison resolved, never from the branches again) and `read_bodies` (the same for a change's bodies) |
| `compare_tests.rs` | End-to-end tests of the three branch requests over real repositories (test-only; shares `tests.rs`'s `Fixture`) |
| `body_tests.rs` | End-to-end tests of the two `…_change_bodies` requests over real repositories (test-only; shares `tests.rs`'s `Fixture`) |
| `change_tests.rs` | End-to-end tests of the two change requests over real repositories and worktrees (test-only; shares `tests.rs`'s `Fixture`) |
| `budget.rs` | The limits every browse read is held to, in one place, with the change listing's, diff's, branch listing's and comparison's budgets and each request kind's reservation |
| `lane.rs` | Per-connection lane: the bound on outstanding requests, slots whose newer request supersedes the older, and the reservation of retained bytes against the daemon-wide `Gate` |
| `tests.rs` | End-to-end tests over real directories and repositories (test-only) |

### `src/adapter/`

A single adapter interface (launch, pre-allocate/obtain a session id, inject capabilities, report status, resume), so
the rest of the daemon is agnostic to which agent it is driving. Three things are injected when a session starts: the
status hooks, the Octoboard MCP server, and a role description ("What is injected on every launch" in
`docs/product/launching-agents.md`).

| File | Agent |
|---|---|
| `mod.rs` | The adapter trait and the agent-agnostic launch/resume flow shared by all three |
| `claude.rs` | Claude Code — injects by CLI flags only |
| `codex.rs` | Codex — injects through repeated `-c` overrides, re-passed on every resume |
| `grok.rs` | Grok Build — no flag for hooks or MCP; injects through a per-session `GROK_HOME` overlay |

The design constraints each adapter has to satisfy (which flags must never be passed, workspace-trust gating, how a
message is written into a running session, resume semantics) are documented in each file's own doc comment and in
"Injecting Octoboard into each agent" in `docs/agent-cli-reference.md`; they are not repeated here.
