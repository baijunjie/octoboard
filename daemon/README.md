# `octoboardd`

A headless Rust binary that plays two roles in one process:

- **Host role**: owns PTYs and agent processes, receives hook callbacks, lists directories, finds the git repositories
  under a parent directory, and clones a repository.
- **Coordinator role**: stores consoles, projects and sessions in SQLite, and routes requests to the host role.

The desktop application (`app/`) is purely a client of this process. It never shares state or an IPC channel with it —
everything it can do goes through the external interface below.

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
| `src/main.rs` | CLI entry point: parses the three modes above, opens the store, binds the port, prints the handshake line the application waits for, runs the server until shutdown |
| `src/server.rs` | The HTTP/WebSocket router described in `PROTOCOL.md`, including `POST /mcp/:token` |
| `src/protocol.rs` | Rust types for the wire protocol; kept in sync with `PROTOCOL.md` and with `app/src/protocol.ts` by hand |
| `src/coordinator.rs` | Coordinator role: what each control-socket request does to the stored consoles/projects/sessions, and which host-role work it triggers |
| `src/reporting.rs` | The channel between a console's hub and its project sessions: the brief a task is handed over as, writing a message into a running session, a report reaching the hub, the report synthesised when a session stops without sending one, and automatic archiving |
| `src/outbox.rs` | The per-session queue every message Octoboard writes into an agent passes through: order-preserving, one drainer per session, and what happens to a message the session only partly accepted |
| `src/store.rs` | Coordinator's SQLite storage for consoles, projects, sessions and the host table |
| `src/state.rs` | Shared daemon state and the session status transitions |
| `src/session.rs` | One running agent process: its PTY, its output fan-out, how it is stopped |
| `src/term.rs` | Launching an agent in a PTY, and writing messages into a running one |
| `src/ptyio.rs` | Non-blocking read/write on a PTY master fd (a blocking write can park forever behind a modal dialog) |
| `src/ringbuf.rs` | Fixed-capacity ring buffer holding a session's recent terminal output, replayed to a client that attaches or reconnects |
| `src/hostfs.rs` | Host role's filesystem work: browsing directories, finding git repositories under a parent directory, cloning one |
| `src/env_shell.rs` | Captures the user's real shell environment (`$SHELL -l -i -c 'env -0'`) that every agent is launched with |
| `src/hooks.rs` | Turns one agent's hook event payload into a session status; each agent's events and payload shape differ |
| `src/hook_mode.rs` | The `octoboardd hook` CLI mode itself |
| `src/loopback.rs` | A minimal HTTP client for the daemon's own loopback address, shared by `hook_mode.rs` and `mcp/stdio.rs` — the two CLI modes that call the running daemon from a separate process |
| `src/mcp/` | The Octoboard MCP server — see below |
| `src/instance_lock.rs` | Enforces one daemon per data directory |
| `src/paths.rs` | Where Octoboard keeps its own files, under `~/.octoboard` |
| `src/adapter/` | One adapter per agent CLI — see below |

### `src/mcp/`

The Octoboard MCP server: the orchestration tools the hub drives Octoboard with, and the one reporting tool a project
session answers through. The tool catalogue is shared by both sides of the stdio bridge, so the child process, the
daemon, and the role descriptions cannot drift apart. The wire-level `POST /mcp/:token` contract is in
`PROTOCOL.md`; "Octoboard MCP tools" in `docs/mvp.md` section 5.2 lists the tools and what each one does.

| File | Role |
|---|---|
| `mod.rs` | The tool catalogue, and which tools each session role (`hub` / `worker`) may see |
| `role.rs` | The role description injected at launch, and the hub instruction file written into a console's working directory |
| `exec.rs` | Runs one tool call against the real consoles, projects and sessions, through the same coordinator/reporting functions the control socket uses |
| `stdio.rs` | `octoboardd mcp` itself: the stdio child process each adapter registers, forwarding every call to the daemon over loopback |

### `src/adapter/`

A single adapter interface (launch, pre-allocate/obtain a session id, inject capabilities, report status, resume), so
the rest of the daemon is agnostic to which agent it is driving. Three things are injected when a session starts: the
status hooks, the Octoboard MCP server, and a role description (`docs/mvp.md` section 5.1).

| File | Agent |
|---|---|
| `mod.rs` | The adapter trait and the agent-agnostic launch/resume flow shared by all three |
| `claude.rs` | Claude Code — injects by CLI flags only |
| `codex.rs` | Codex — injects through repeated `-c` overrides, re-passed on every resume |
| `grok.rs` | Grok Build — no flag for hooks or MCP; injects through a per-session `GROK_HOME` overlay |

The design constraints each adapter has to satisfy (which flags must never be passed, workspace-trust gating, how a
message is written into a running session, resume semantics) are documented in each file's own doc comment and in
"Agent adapters" in `docs/mvp.md`; they are not repeated here.
