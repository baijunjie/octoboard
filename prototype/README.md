# Validation prototype (throwaway)

This directory exists only to settle the investigation items of milestone 00 (technical validation). It is **not**
production code and carries no backward-compatibility obligation.

TODO(milestone 01): delete this whole directory. Milestone 01 writes the real `octoboardd` and the real desktop
application from scratch, guided by the conclusions recorded in `docs/plans/20261001-octoboard-mvp/00-technical-validation.md`.

## What it has to demonstrate

One chain, end to end:

> the daemon starts an agent over a PTY → hooks report status → a message reaches a running session → an MCP tool gets
> called → the frontend renders the terminal over WebSocket

and enough instrumentation to answer the terminal/architecture items T1-T4.

## Layout

| Path | Role |
|---|---|
| `daemon/` | Rust binary `obd-proto`: PTY management, WebSocket server, hook callback endpoint, stdio MCP server mode, per-agent launch argument assembly |
| `ui/` | Vite + TypeScript + `xterm.js`, talks to the daemon over WebSocket only |
| `tauri/` | Tauri 2 shell that serves `ui/` and runs the daemon as a sidecar |
| `bench/` | Node scripts measuring throughput and round-trip latency (T2) and reconnect replay (T3) |

## Rules the prototype obeys

These are the constraints the real product has, and the prototype has to prove they hold:

- The UI and the daemon interact **only** over the WebSocket protocol. No Tauri IPC, no shared state.
- Agents are launched with an environment snapshotted from the user's **login + interactive** shell, with the project
  directory as cwd — the agent binary is spawned directly, not run inside that shell. The snapshot is filtered: the
  snapshot shell's own `TERM`, and any marker identifying the daemon's own agent session, must not reach the agent.
- Injection happens through launch arguments and environment variables only. The prototype must never write to project
  files nor to the user's global agent configuration (`~/.claude`, `~/.grok`, ...).
