# Verifying changes for Windows

## Cross-check Rust changes for Windows yourself, and report Windows-only code as unverified until it ran on Windows

Applies when you change Rust code in either crate on a Mac. `pnpm verify`, the pre-push hook and the release check
build and test for the host they run on only; none of them cross-checks Windows, so a change that breaks the Windows
build passes all of them and surfaces only when a release builds the Windows installer. Run `pnpm check:windows`
(through `cargo xwin`, which has to be installed; `pnpm check:windows daemon` or `desktop` checks one crate) before
handing the change over.

It only compiles. Windows-only code, and the tests that run only on Windows, never run on the Mac: they run on the
`windows-check` job of `.github/workflows/quality.yml`, which runs only when started by hand, or on a Windows machine.
Until one of those has run them, say when reporting that such code is compile-checked only.

## Confirm Windows behaviour as a standard user on that user's own desktop, not over ssh or as an administrator

Applies when a change's behaviour on Windows is confirmed by running it, in a virtual machine or on hardware. Run the
daemon and the app as a standard (non-administrator) user whose name, and so home directory, holds non-ASCII
characters, with Developer Mode off and nothing elevated, from that user's own desktop session. That is what most users
have, and the opposite setup hides defects:

- An administrator's or an elevated process can make file symlinks, so every path for a host without file links
  (configuration sharing reported unavailable, a console's `CLAUDE.md` written as a plain file) never runs, and Codex
  behaves differently when run as administrator.
- A session over OpenSSH runs at High integrity, inside a kill-on-close job, and passes the inherited "ignore Ctrl-C"
  flag to everything it starts, so what it shows about elevation, Ctrl-C and process containment is not what the
  desktop app's daemon sees.

Building and running the test suite over ssh is fine; a behaviour that any of the above can change is confirmed in the
setup above. What each of these does to the daemon is in `docs/architecture.md`, "Known pitfalls of the Tauri / Rust
approach".

## On a Windows runner, call System32's `tar.exe` by its full path

Applies to a script that extracts an archive with `tar` and can run in a GitHub Actions Windows job. Those jobs here run
in Git Bash (`shell: bash`), which puts its own GNU tar first on the PATH at every start: it reads no zip and takes the
`D:` of an absolute path for a remote host, so a bare `tar` that works on a Windows machine and on macOS fails there.
`scripts/fetch-conpty.mjs` names `%SystemRoot%\System32\tar.exe` for this reason, as `update-archive.mjs` names
`/usr/bin/tar` on macOS. Adding a directory with `$GITHUB_PATH` does not get ahead of Git Bash's own: Git Bash puts its
directories first at every start, after the runner has set the PATH.
