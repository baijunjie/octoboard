# Linux Daemon Run Development Plan

## Problem and approach

Linux is the server form (see "Two deployment forms" in `docs/architecture.md`): there is no application and no
display. The daemon is started on the server from the command line, keeps running in the background, and serves the
same UI to its owner's browser on another machine — the machine showing the window and the machine holding the daemon
and the projects are two different ones. This plan makes the daemon build and run on Linux, gives it a command-line
surface for running in the background, and has it serve the shared desktop UI itself so that one binary is the whole
Linux deployment.

Exposing the daemon beyond loopback needs authentication and TLS and is a separate plan; here the daemon stays on
loopback. A headless server has no browser, so that is not a state anyone can use from the server itself: what this
plan delivers is reached through a tunnel that ends on the server's loopback — an SSH port forward — by the owner,
from their own machine, with the browser at `http://localhost:<forwarded port>`. That address is what today's access
check admits and what a browser treats as a secure context, so notifications and the clipboard work through it; it is
the one arrangement that works with the daemon as it is, and it is how the milestones here are verified.

## Key design decisions

- **The daemon serves the UI itself**, same-origin with its protocol endpoints, with the UI build embedded in the
  binary.
- **Command-line surface**: start (detaches and keeps running), stop, status; status prints the address again.
- **Lifecycle**: a daemon with a parent application still follows it (macOS); one without keeps running until stopped.
- **Loopback only here**, reached from off the server through an SSH port forward; nothing else listens until a
  separate plan exposes the daemon.
- **A stable address.** The daemon takes the port it listens on at start and keeps it, so the address can be
  bookmarked and a tunnel command written down once; the operating-system-assigned port stays the desktop form's, where
  the shell is handed it. The port file leaves the shared temporary directory — on Linux `std::env::temp_dir()` is
  `/tmp` for every user, so two owners' daemons on one machine, each single-user and each its own, would write one
  file — for the daemon's own data directory.
- **The daemon runs as the owner's own Unix user**, not a system account: its sessions run as that user, with that
  user's `~/.octoboard`, agent accounts and keys.

## Milestones

- [01 Linux build and background run](01-linux-build-and-background-run.md)
- [02 Serving the UI](02-serving-the-ui.md) — needs the UI to run outside the Tauri shell

## Open

- How the Linux build is distributed.
- Whether the daemon-served UI is also what the macOS window loads.
