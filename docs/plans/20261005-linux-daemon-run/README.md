# Linux Daemon Run Development Plan

## Problem and approach

On Linux, Octoboard has no application: the daemon is started from the command line, keeps running in the background, and
serves the same UI to a browser. This plan makes the daemon build and run on Linux, gives it a command-line surface for
running in the background, and has it serve the shared desktop UI itself so that one binary is the whole Linux
deployment.

Serving to other machines needs authentication and is a separate plan; here the daemon stays on loopback.

## Key design decisions

- **The daemon serves the UI itself**, same-origin with its protocol endpoints, with the UI build embedded in the binary.
- **Command-line surface**: start (detaches and keeps running), stop, status; status prints the address again.
- **Lifecycle**: a daemon with a parent application still follows it (macOS); one without keeps running until stopped.
- **Loopback only here.**

## Milestones

- [01 Linux build and background run](01-linux-build-and-background-run.md)
- [02 Serving the UI](02-serving-the-ui.md) — needs the UI to run outside the Tauri shell

## Open

- How the Linux build is distributed.
- Whether the daemon-served UI is also what the macOS window loads.
