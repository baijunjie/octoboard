# Project Map

Navigation for the code tree: one line per module, linking to that module's own doc for how it works internally.

- [`daemon/`](../daemon/README.md) — `octoboardd`, the Rust daemon: host role (PTYs, agent processes, directories,
  repositories) and coordinator role (console/project/session data in SQLite), reachable only through the
  WebSocket/HTTP protocol in [`daemon/PROTOCOL.md`](../daemon/PROTOCOL.md).
- [`app/`](../app/README.md) — the Tauri 2 + React + TypeScript desktop application: the console/project/session UI
  and terminal, a client of `daemon/` over WebSocket only.
