# `octoboardd`

The Octoboard daemon: a headless Rust binary that runs the agent sessions and keeps all of Octoboard's data. It owns the
terminals and agent processes, receives the agents' hook callbacks, works on your repositories on behalf of the window,
and stores consoles, projects, sessions and agent accounts in SQLite under `~/.octoboard`.

Every client — the desktop application, or the UI in a browser — reaches it only over the WebSocket/HTTP protocol in
[`PROTOCOL.md`](PROTOCOL.md), on `127.0.0.1` with a key that changes at every start. In the desktop application it runs
as a sidecar the shell starts for you; you build and run it by hand to work on the daemon itself, or to run it on a
server, where it serves the UI to your browser itself.

## Build and run

This crate is a member of the repository's root Cargo workspace, alongside the desktop shell's Tauri crate. From the
repository root:

```sh
cargo build -p octoboardd           # Build the daemon.
cargo run -p octoboardd             # Start it; it prints `octoboardd listening on 127.0.0.1:<port> with key <key>`.
pnpm test:rust                      # Run the Rust tests of both crates.
```

For a server, build the binary with the UI inside it, so that a browser reaching the daemon's port loads the UI from
the daemon itself:

```sh
pnpm --filter @octoboard/daemon... build   # Build the UI, then the release daemon embedding it.
```

The `...` builds the UI package first; the binary is `target/release/octoboardd`. A plain `cargo build -p octoboardd`
embeds nothing and serves no UI, as the desktop application's sidecar does: `pnpm --filter @octoboard/desktop
build:daemon` writes the same `target/release/octoboardd` without it, so copy a server build away before building the
application. `cargo build -p octoboardd --features embedded-ui` embeds whatever `packages/ui/dist` holds and fails when
it holds no build. A build embedding the UI refuses while `VITE_DAEMON_KEY` or `VITE_DAEMON_PORT` is set in its
environment, since the UI build would bake it into the page the daemon serves; it cannot tell whether a `packages/ui/dist`
built earlier holds one.

A daemon started by hand uses your real `~/.octoboard` and refuses to start while another daemon holds it, such as the
installed application's. To point the UI's dev server at it, see [`packages/ui/README.md`](../../packages/ui/README.md).

A workspace-wide `cargo build` or `cargo test` also builds the Tauri crate, which needs the daemon sidecar built once
first: `pnpm --filter @octoboard/desktop build:daemon`.

## Running in the background (Linux)

On a headless server the daemon runs on its own, as the owner's user, and listens on `127.0.0.1` only. Install the
build with the UI inside it (see "Build and run" above):

```sh
octoboardd start --port 7531        # Start it in the background and return.
octoboardd status                   # Print its pid and address; exits 3 when none runs.
octoboardd address                  # Print the browser address with its key; exits 3 when none runs.
octoboardd restart                  # Restart it on the same address, keeping its sessions running.
octoboardd stop                     # Stop it, which leaves every session interrupted.
octoboardd --port 7531              # Run it in the foreground on that port, as a service does.
```

`start` refuses while a daemon already runs against the same `~/.octoboard`. Reach the daemon from your own machine
through an SSH port forward, which puts it at `localhost:7531` there:

```sh
ssh -L 7531:127.0.0.1:7531 server
```

and open the address `octoboardd address` prints on the server, `http://localhost:7531/?key=<key>`. The key changes
whenever the daemon starts afresh, including when systemd starts the next one after a planned restart, so run the
command again then; it works however the daemon was started, and only as the daemon's own user: the key comes from an
owner-only file in `~/.octoboard`. A program can run it over SSH and read the port and key from that one line.

To start it at boot instead, install the example systemd user unit
[`dist/systemd/octoboardd.service`](dist/systemd/octoboardd.service); its comments say how, and why the user needs
lingering enabled. Use the unit as well on a host where logind kills a user's processes at logout
(`KillUserProcesses=yes`), which would end a daemon started by `start` with the login. Under the unit, the sessions
run in user units of their own (`octoboard-holder-*.service`), so `restart`, and a restart of the unit after a crash,
keep them running.

## Read next

- [The daemon on its own: its command line, planned restart and running as a
  service](../../docs/product/server-form.md), [who can reach it](../../docs/product/who-can-reach-the-daemon.md) and
  [the files it owns](../../docs/product/files-octoboard-owns.md)
- [The daemon's structure: modules, CLI modes, layering and shared helpers](../../docs/structure/daemon.md)
- [Architecture](../../docs/architecture.md) and the [documentation index](../../docs/README.md)
