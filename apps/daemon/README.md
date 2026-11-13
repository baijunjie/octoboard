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

The release build for a Linux server, statically linked against musl so that it runs on any distribution, is
`pnpm --filter @octoboard/desktop release --no-sign-update` on a Linux host of the architecture to build (it needs
`musl-gcc`, Debian and Ubuntu's `musl-tools`); it writes the archive every release publishes to
`target/linux-release/`. The [desktop scripts README](../desktop/scripts/README.md) describes it.

## Running in the background (Linux)

On a headless server the daemon runs on its own, as the owner's user, and listens on `127.0.0.1` only. Install the
released build, which carries the UI inside it, for your user:

```sh
curl --proto '=https' --tlsv1.2 -fsSL https://octoboard.dev/install.sh | sh             # Into ~/.local/bin; run again to upgrade.
curl --proto '=https' --tlsv1.2 -fsSL https://octoboard.dev/install.sh | sh -s -- --service  # Also install and start the user unit (port 3369, or --port N).
```

or put a build of your own on the server (see "Build and run" above). Then:

```sh
octoboardd start                    # Start it in the background on port 3369 (or --port N) and return.
octoboardd status                   # Print its pid and address; exits 3 when none runs.
octoboardd address                  # Print the browser address with its key; exits 3 when none runs.
octoboardd restart                  # Restart it on the same address, keeping its sessions running.
octoboardd stop                     # Stop it, which leaves every session interrupted.
octoboardd --port 3369              # Run it in the foreground on that port, as a service does.
```

`start` refuses while a daemon already runs against the same `~/.octoboard`. A browser reaches the daemon through a
socket only its user can open, `~/.octoboard/browser.sock`, not through its port. Forward that socket from your own
machine, which puts the daemon at `127.0.0.1:3369` there (`octoboardd address` prints this command with the real path):

```sh
ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:3369:/home/you/.octoboard/browser.sock you@server
```

and open the address `octoboardd address` prints on the server, `http://127.0.0.1:3369/?key=<key>`. On a server the
key is the owner key, which stays the same across restarts, so the address can be bookmarked; `octoboardd address
--rotate` replaces it and disconnects every page that used the old one. The command works however the daemon was
started, and only as the daemon's own user: the key comes from an owner-only file in `~/.octoboard`. A program can run
it over SSH and read the port and key from the one line it prints on standard output.

To start it at boot instead, install the example systemd user unit
[`dist/systemd/octoboardd.service`](dist/systemd/octoboardd.service), which the install script's `--service` does for
you and every release archive carries; its comments say how, and why the user needs lingering enabled. After upgrading
the installed file, `octoboardd restart` moves a running daemon to it, under the unit or not. Use the unit as well on a
host where logind kills a user's processes at logout (`KillUserProcesses=yes`), which would end a daemon started by
`start` with the login. Under the unit, on systemd 236 or later, the sessions run in user units of their own
(`octoboard-holder-*.service`), so `restart`, and a restart of the unit after a crash, keep them running; where a holder
could not be started as a unit of its own, the daemon refuses `restart` while it runs a session, since the restart would
end it, and a restart it accepts refuses a new session there until it completes.

## Read next

- [The daemon on its own: its port and running as a service](../../docs/product/server-form.md),
  [its command line](../../docs/product/daemon-command-line.md),
  [planned restart and planned stop](../../docs/product/planned-restart-and-planned-stop.md),
  [who can reach it](../../docs/product/who-can-reach-the-daemon.md) and
  [the files it owns](../../docs/product/files-octoboard-owns.md)
- [The daemon's structure: modules, CLI modes, layering and shared helpers](../../docs/structure/daemon.md)
- [Architecture](../../docs/architecture.md) and the [documentation index](../../docs/README.md)
