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

## Running in the background (Linux)

On a headless server the daemon runs on its own, as the owner's user, and listens on `127.0.0.1` only:

```sh
octoboardd start --port 7531        # Start it in the background and return.
octoboardd status                   # Print its pid and address; exits 3 when none runs.
octoboardd stop                     # Stop it, which leaves every session interrupted.
octoboardd --port 7531              # Run it in the foreground on that port, as a service does.
```

`start` refuses while a daemon already runs against the same `~/.octoboard`. Reach the daemon from your own machine
through an SSH port forward, which puts it at `127.0.0.1:7531` there for a client with the key `start` printed:

```sh
ssh -L 7531:127.0.0.1:7531 server
```

To start it at boot instead, install the example systemd user unit
[`dist/systemd/octoboardd.service`](dist/systemd/octoboardd.service); its comments say how, and why the user needs
lingering enabled. Use the unit as well on a host where logind kills a user's processes at logout
(`KillUserProcesses=yes`), which would end a daemon started by `start` with the login.

## Read next

- [What the daemon does and the files it owns](../../docs/product/application-lifecycle.md)
- [The daemon's structure: modules, CLI modes, layering and shared helpers](../../docs/structure/daemon.md)
- [Architecture](../../docs/architecture.md) and the [documentation index](../../docs/README.md)
