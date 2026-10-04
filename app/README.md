# Octoboard desktop application

A Tauri 2 + React + TypeScript desktop application: the console/project/session menu, the `xterm.js` terminal, console
and project management, manual sessions, archiving and reopening, and the exit flow.

The UI talks to `octoboardd` (see [`../daemon/`](../daemon/README.md)) only over the WebSocket/HTTP protocol in
[`../daemon/PROTOCOL.md`](../daemon/PROTOCOL.md) — no Tauri IPC command carries daemon traffic or session state. This
is an architectural rule, not an implementation detail: the already-decided remote-host feature depends on the UI
never distinguishing a local daemon from a remote one, which only holds if the daemon is reachable exclusively through
that one protocol.

## Development

`src-tauri/tauri.conf.json` declares `octoboardd` as an `externalBin`, which Tauri resolves at **compile** time, not
at launch. So `npm run build:daemon` (builds `daemon/` in release and copies the binary into
`src-tauri/binaries/octoboardd-<target-triple>`, see `scripts/build-daemon.mjs`) must have been run at least once
before `npm run tauri dev` or a bare `cargo build` in `src-tauri/` will succeed — neither of those two commands runs
it for you, and skipping it fails at compile time with no obvious cause. `npm run build:tauri` (what `tauri build`
uses) runs it automatically; rerun `build:daemon` by hand whenever `daemon/` changes during development.

Running `vite dev` directly against a daemon started by hand, bypassing the Tauri shell, needs `VITE_DAEMON_PORT` set
to that daemon's port — see `src/daemon.ts` for where to read it from.

## External interfaces

`src-tauri/` (the Rust side) defines exactly two of its own Tauri IPC commands, both bare exit-flow signals with no
daemon traffic or session data in them:

- `frontend_handles_exit` — marks the webview as the one handling the exit flow, so a quit is no longer let through
  unconfirmed.
- `confirm_quit` — marks a pending quit as user-confirmed and asks Tauri to actually exit.

Beyond those two, `src-tauri/capabilities/default.json` also allowlists the `notification` plugin's commands (used by
`src/lifecycle/useWaitingNotifications.ts` for the raised-hand system notification) and `core:window|set_badge_count`
(the Dock badge). Both are still within the architectural rule above: they carry no daemon traffic or session state,
only a count and a text the frontend has already derived from it.

Everything else `src-tauri/` does is internal: it starts `octoboardd` as a sidecar process and bakes the port it
printed into the window's URL (`?port=`) before the window is created, so the frontend can locate the daemon without
any IPC call for it.

## Layout

| Path | Role |
|---|---|
| `src-tauri/` | Rust shell: sidecar lifecycle for `octoboardd`, window creation, the exit-confirmation flow and native menu, the two IPC commands above, and the capabilities file allowlisting those plus the notification and Dock-badge commands. Nothing else. |
| `src/daemon.ts` | Locates the daemon's port (`?port=` query param from the Tauri shell, or `VITE_DAEMON_PORT` for `vite dev` against a hand-started daemon) |
| `src/daemon-client.ts` | WebSocket client for `GET /ws/control`: request/reply correlation, reconnect, event dispatch |
| `src/protocol.ts` | Hand-written TypeScript mirror of `daemon/src/protocol.rs` / `PROTOCOL.md` |
| `src/store.tsx` | React context holding the daemon connection and the console/project/session state derived from its events |
| `src/App.tsx` | Top-level layout: sidebar, terminal pane, dialogs |
| `src/components/` | Menu, dialogs (console/project/session create-edit, confirm, directory picker) and small UI primitives |
| `src/terminal/` | `TerminalController` (owns `xterm.js`, the session's `GET /ws/term/:session` socket, connection status and focus as one unit) and the `TerminalPane` component wrapping it |
| `src/lifecycle/useAppExit.ts` | Drives the exit-confirmation flow from the frontend side, calling the two Tauri commands above |
| `src/lifecycle/useWaitingNotifications.ts` | Fires the system notification and sets the Dock badge count when a session raises its hand |
| `src/sessionLabel.ts` | Where to tell the user a session is (its project, or its console's hub), since the daemon's `Session` record itself only carries ids |
| `src/agents.ts` | Display labels for the three agents the MVP supports |
| `src/main.tsx`, `src/StartupScreen.tsx`, `src/ErrorBoundary.tsx` | Startup sequencing and the error/retry screens shown before the daemon connection is ready |
| `scripts/build-daemon.mjs` | Builds `octoboardd` in release mode and copies it into `src-tauri/binaries/` under the target-triple name Tauri's `externalBin` requires |
