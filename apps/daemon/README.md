# `octoboardd`

The Octoboard daemon: a headless Rust binary that runs the agent sessions and keeps all of Octoboard's data. It owns the
terminals and agent processes, receives the agents' hook callbacks, works on your repositories on behalf of the window,
and stores consoles, projects, sessions and agent accounts in SQLite under `~/.octoboard`.

Every client — the desktop application, or the UI in a browser — reaches it only over the WebSocket/HTTP protocol in
[`PROTOCOL.md`](PROTOCOL.md), on `127.0.0.1` with a key that changes at every start. In the desktop application it runs
as a sidecar the shell starts for you; you only build and run it by hand to work on the daemon itself.

## Build and run

This crate is a member of the repository's root Cargo workspace, alongside the desktop shell's Tauri crate. From the
repository root:

```sh
cargo build -p octoboardd           # Build the daemon.
cargo run -p octoboardd             # Start it; it prints `octoboardd listening on 127.0.0.1:<port> with key <key>`.
pnpm test:rust                      # Run the Rust tests of both crates.
```

A daemon started by hand uses your real `~/.octoboard` and refuses to start while another daemon holds it, such as the
installed application's. To point the UI's dev server at it, see [`packages/ui/README.md`](../../packages/ui/README.md).

A workspace-wide `cargo build` or `cargo test` also builds the Tauri crate, which needs the daemon sidecar built once
first: `pnpm --filter @octoboard/desktop build:daemon`.

## Read next

- [What the daemon does and the files it owns](../../docs/product/application-lifecycle.md)
- [The daemon's structure: modules, CLI modes, layering and shared helpers](../../docs/structure/daemon.md)
- [Architecture](../../docs/architecture.md) and the [documentation index](../../docs/README.md)
