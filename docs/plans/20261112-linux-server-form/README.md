# Linux Server Form Development Plan

## Problem and approach

Octoboard has two deployment forms (see "Two deployment forms" in `docs/architecture.md`). The desktop form exists; this
plan completes the server form: the daemon alone on a Linux server without a display, kept running in the background,
reached by its owner from a browser on their own machine. One `octoboardd` binary is the whole deployment: it runs the
sessions and serves the UI itself.

The daemon listens on the server's loopback only, and the owner reaches it through an SSH port forward
(`ssh -L <port>:127.0.0.1:<port> <server>`), opening `http://localhost:<port>` in their browser. This is the server
form's lasting way in, not a stopgap: it needs no exposure beyond loopback and no TLS, and the page it gives is a secure
context, so notifications and the clipboard work.

Out of scope: the macOS or Windows application connecting to a remote daemon, and any way in that does not go
through SSH.

## Key design decisions

- **Loopback only, reached through an SSH port forward, for good.** `localhost` is what today's `Host` / `Origin`
  check admits and what a browser treats as a secure context.
- **The daemon serves the UI itself**, same-origin with its protocol endpoints, the UI build embedded in the binary,
  so the deployment is one file.
- **The owner gets the browser address from the server.** Logged in over SSH as the daemon's user, the owner runs one
  command that prints the address with the daemon's key, and opens it through the forward. The key still keeps out
  pages served from `localhost` on the owner's machine; only the daemon's own Unix user can obtain it through the
  command. It is never journalled, nor put in the owner record, the
  daemon's arguments or its environment. The same delivery serves a daemon started by hand and one run by systemd,
  including the successor of a planned restart, so it is built once, before the milestones that need it.
- **Command-line surface**: start (detaches and keeps running), stop, status, restart.
- **Lifecycle**: a daemon with a parent application follows it (desktop form); one without keeps running until stopped.
- **A stable address.** The daemon keeps the port it is given at start, so the forward command is written down once;
  the operating-system-assigned port stays the desktop form's. The port file lives in the daemon's data directory, not
  the shared temporary directory, since on Linux that is one `/tmp` for every user and two owners' daemons on one
  machine would write one file.
- **The daemon runs as the owner's own Unix user**, not a system account: its sessions run as that user, with that
  user's `~/.octoboard`, agent accounts and keys.
- **The key reaches only the daemon's own Unix user.** Other users of a shared server reach the same loopback, and
  the key is what keeps them out; it is therefore never put where they can read it, such as a process's arguments,
  which any user can list. Today the `octoboardd` helpers a session runs carry it there; that is fixed in every form,
  since a Mac or Windows machine can have other users too.
- **`apps/daemon` becomes a pnpm workspace package**, a `package.json` wrapping its cargo commands, so the workspace
  orders "build the UI, then embed it in the daemon" instead of that order being remembered by hand.
- **Containment on Linux** is a cgroup v2 group per launch where a writable subtree is delegated, on top of the
  process ledger, which stays the fallback where none is.

## Milestones

- [01 Linux build and command verification](01-linux-build-and-command-verification.md)
- [02 Owner access link](02-owner-access-link.md) — needs 01
- [03 Keeping the key from other users](03-key-kept-from-other-users.md) — needs 01
- [04 Holders as systemd user services](04-holders-as-user-services.md) — needs 01; its planned-restart case needs 02
- [05 Serving the UI](05-serving-the-ui.md) — needs 02

## Open

- How the Linux build is distributed.
- Whether the daemon-served UI is also what the desktop window loads.
- A fresh start, or a successor started by systemd, makes a new key, so a bookmarked address stops working then; whether
  to give the browser something that survives a restart.
