# `octoboardd`

A headless Rust binary that plays two roles in one process:

- **Host role**: owns PTYs and agent processes, receives hook callbacks, lists directories, finds the git repositories
  under a parent directory, clones a repository, and checks a project's git status against its remote.
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
- `octoboardd mcp --session <id> --role <role> --port <port> --token <token>` — the other CLI mode. This is the
  MCP server each adapter registers as a session's `command`-type MCP server (see `src/mcp/` below); it speaks MCP on
  stdin/stdout and forwards each tool call to the running daemon's `POST /mcp/:token`. Both are specified in
  [`PROTOCOL.md`](PROTOCOL.md).

## Layout

| Path | Role |
|---|---|
| `build.rs` | Exposes the app name from the repo-root `config/app.json` as `OCTOBOARD_APP_NAME`, for the wording the daemon itself shows the user |
| `src/main.rs` | CLI entry point: parses the three modes above, opens the store, binds the port, prints the handshake line the application waits for, runs the server until shutdown |
| `src/access.rs` | The middleware every route sits behind: turns away, with `403`, a request whose `Host` or `Origin` is not one a local client or the application's own UI would send; the rule is in `PROTOCOL.md` |
| `src/server.rs` | The HTTP/WebSocket router described in `PROTOCOL.md`, including `POST /mcp/:token` |
| `src/protocol.rs` | Rust types for the wire protocol; kept in sync with `PROTOCOL.md` and with `packages/ui/src/protocol.ts` by hand |
| `src/coordinator.rs` | Coordinator role: what each control-socket request does to the stored consoles/projects/sessions/pages/accounts, and which host-role work it triggers, including the launch flow a resume and a switch of a session's account share; projects are stored with absolute, lexically normalised paths |
| `src/reporting.rs` | The channel between a console session and its project sessions: the brief a task is handed over as, writing a message into a running session, a report reaching the console session, the report synthesised when a session stops without sending one, automatic archiving, and rendering a report panel form submission into the console session's message |
| `src/relocate.rs` | Copying one session's conversation record from one account's config directory into another's, for a switch of the session's account: finding the record by name under the agent's root, and copying it to the same path relative to the directory, staged and checked before it replaces anything |
| `src/outbox.rs` | The per-session queue every message Octoboard writes into an agent passes through: order-preserving, one drainer per session, and what happens to a message the session only partly accepted |
| `src/store.rs` | Coordinator's SQLite storage for consoles, projects, sessions, pages, agent accounts, the trusted folders, the user settings and the host table |
| `src/state.rs` | Shared daemon state: the session status transitions, each project's live git status and check claim, and every agent's current availability and resolved default account |
| `src/availability.rs` | Working out, once per daemon start, which agents are available and what each one's default account resolves to |
| `src/git_status.rs` | Checks one project's git status against its remote and, with auto-sync on, fast-forwards it — see `PROTOCOL.md`'s "Daemon behaviour, per project" |
| `src/session.rs` | One running agent process: its PTY, its output fan-out, how it is stopped |
| `src/trust.rs` | Recognising Claude Code's workspace-trust screen in a Claude session's terminal output, deciding whether the user has consented (per project or through a trusted parent folder; console sessions are answered without asking), asking the application through `claude_trust_prompt` / `confirm_claude_trust`, and answering the screen — the only code that types keys into a session on its own |
| `src/term.rs` | Launching an agent in a PTY, and writing messages into a running one |
| `src/ptyio.rs` | Non-blocking read/write on a PTY master fd (a blocking write can park forever behind a modal dialog) |
| `src/ringbuf.rs` | Fixed-capacity ring buffer holding a session's recent terminal output, replayed to a client that attaches or reconnects |
| `src/saved_output.rs` | The ring buffer's contents kept as one file per session once its process ends (`paths::saved_output_dir`), replayed by the terminal socket for a session with no process; written atomically, removed with the session's record, and swept at startup |
| `src/hostfs.rs` | Host role's filesystem work: browsing directories, finding git repositories under a parent directory, cloning one, lexical path normalisation |
| `src/env_shell.rs` | Captures the user's real shell environment (`$SHELL -l -i -c 'env -0 && printf <marker>'`) that every agent is launched with; also a cached variant for a caller on its own repeating schedule (`cached_snapshot`) and a generic timeout-bounded subprocess runner (`run_with_timeout`), both used by `git_status.rs` |
| `src/hooks.rs` | Turns one agent's hook event payload into a session status; each agent's events and payload shape differ |
| `src/transcript.rs` | Watches a Claude Code session's own transcript JSONL for the one status change its hooks never report — a declined permission prompt or `AskUserQuestion` — and lowers the raised hand when found; the only place a session's status comes from something other than a hook event |
| `src/hook_mode.rs` | The `octoboardd hook` CLI mode itself |
| `src/loopback.rs` | A minimal HTTP client for the daemon's own loopback address, shared by `hook_mode.rs` and `mcp/stdio.rs` — the two CLI modes that call the running daemon from a separate process |
| `src/mcp/` | The Octoboard MCP server — see below |
| `src/instance_lock.rs` | Enforces one daemon per data directory |
| `src/paths.rs` | Where Octoboard keeps its own files, under `~/.octoboard`, and the host's home directory when it is really known (`known_home_dir`), as reported to clients |
| `src/adapter/` | One adapter per agent CLI — see below |
| `src/test_support.rs` | Test-only (`#[cfg(test)]`, not part of the binary): the fixtures the modules' unit tests share — a self-removing scratch directory and file, an `AppState` over a fresh store in one, and a stand-in live session on a PTY that ends itself |

### `src/mcp/`

The Octoboard MCP server: the orchestration tools the console session drives Octoboard with, and the one reporting tool a project
session answers through. The tool catalogue is shared by both sides of the stdio bridge, so the child process, the
daemon, and the role descriptions cannot drift apart. The wire-level `POST /mcp/:token` contract is in
`PROTOCOL.md`; "The console session's tools" and "Reporting" in `docs/product/hub-orchestration.md` list the tools and what each
one does.

| File | Role |
|---|---|
| `mod.rs` | The tool catalogue, and which tools each session role (`console` / `project`) may see |
| `role.rs` | The role description injected at launch, and the console session instruction file written into a console's working directory |
| `exec.rs` | Runs one tool call against the real consoles, projects and sessions, through the same coordinator/reporting functions the control socket uses |
| `stdio.rs` | `octoboardd mcp` itself: the stdio child process each adapter registers, forwarding every call to the daemon over loopback |

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
