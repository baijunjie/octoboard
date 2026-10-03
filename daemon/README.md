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
- `octoboardd hook --session <id> --port <port>` — a second CLI mode, not the server. This is what the per-session hook
  script execs on every agent hook event; it reads the hook's payload from stdin and posts it to the running daemon's
  `POST /hook/:session`.

## Layout

| Path | Role |
|---|---|
| `src/main.rs` | CLI entry point: parses the two modes above, opens the store, binds the port, prints the handshake line the application waits for, runs the server until shutdown |
| `src/server.rs` | The HTTP/WebSocket router described in `PROTOCOL.md` |
| `src/protocol.rs` | Rust types for the wire protocol; kept in sync with `PROTOCOL.md` and with `app/src/protocol.ts` by hand |
| `src/coordinator.rs` | Coordinator role: what each control-socket request does to the stored consoles/projects/sessions, and which host-role work it triggers |
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
| `src/instance_lock.rs` | Enforces one daemon per data directory |
| `src/paths.rs` | Where Octoboard keeps its own files, under `~/.octoboard` |
| `src/adapter/` | One adapter per agent CLI — see below |

### `src/adapter/`

A single adapter interface (launch, pre-allocate/obtain a session id, inject capabilities, report status, resume), so
the rest of the daemon is agnostic to which agent it is driving. What is injected in this milestone is the status
hooks only; the MCP server and the role description belong to orchestration and are not implemented yet.

| File | Agent |
|---|---|
| `mod.rs` | The adapter trait and the agent-agnostic launch/resume flow shared by all three |
| `claude.rs` | Claude Code — injects by CLI flags only |
| `codex.rs` | Codex — injects through repeated `-c` overrides, re-passed on every resume |
| `grok.rs` | Grok Build — no flag for hooks or MCP; injects through a per-session `GROK_HOME` overlay |

The design constraints each adapter has to satisfy (which flags must never be passed, workspace-trust gating, how a
message is written into a running session, resume semantics) are documented in each file's own doc comment and in
"Agent adapters" in `docs/mvp.md`; they are not repeated here.
