# Verifying changes for Windows

## Cross-check Rust changes for Windows yourself, and report Windows-only code as unverified until it ran on Windows

Applies when you change Rust code in either crate on a Mac. `pnpm verify`, `pnpm verify --quick` and the pre-push
hook build and test for the host they run on only; none of them cross-checks Windows, so a change that breaks the
Windows build passes all of them, lands on main, and surfaces only when the release check (`pnpm verify --release`,
which does run `pnpm check:windows`) refuses to tag a version. Run `pnpm check:windows` (through `cargo xwin`; a
machine missing its toolchain is told what to install; `pnpm check:windows daemon` or `desktop` checks one crate)
before handing the change over.

It compiles both crates and links the daemon's executable, nothing more. Windows-only code, and the tests that run
only on Windows, never run on the Mac: they run on the `windows-check` job of `.github/workflows/quality.yml`, which
runs only when started by hand, or on a Windows machine.
Until one of those has run them, say when reporting that such code is compile-checked only. A Windows machine needs no
Rust toolchain to run them: build the test binary on the Mac with
`cargo xwin test --target x86_64-pc-windows-msvc -p octoboardd --no-run`, copy the `octoboardd-<hash>.exe` it names
under `target/x86_64-pc-windows-msvc/debug/deps/` across, and run it there with a test-path filter.

The desktop crate is compile-checked only: nothing on the Mac links its executable, so a change that can affect its
link — linker arguments or `cargo:rustc-link-*` lines in its `build.rs`, linker flags, embedded resources, a new native
library — passes `pnpm check:windows` and the release check unverified; report it as such until it was built on
Windows (the `windows-bundle` job of `.github/workflows/quality.yml` builds the installer). For such a change to the
daemon, the link `pnpm check:windows` runs leaves `octoboardd.exe` under `target/x86_64-pc-windows-msvc/debug/`; check
what landed in it with `llvm-readobj --coff-resources`. `cargo xwin` links with an `lld-link` that has neither libxml2
nor `mt.exe`, so anything that asks the linker to merge manifests fails there while `link.exe` on Windows accepts it.

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
- An interactive scheduled task (`schtasks ... /IT`) does run on the user's desktop, but a daemon started from one
  may not see the desktop app's process containment. Seen once, on Windows 11 ARM64 with the x64 build emulated, for a
  task created with `/IT /RU <user>` on an administrator account with UAC off: the daemon's log said its job does not
  let a process break away, so its holders' terminals end with that job. Whether other tasks or setups do the same is
  unverified; confirm process containment from the desktop app instead.

Building and running the test suite over ssh is fine; a behaviour that any of the above can change is confirmed in the
setup above. What each of these does to the daemon is in `docs/architecture.md`, "Known pitfalls of the Tauri / Rust
approach".

## Learn what Windows itself does by calling its own API on Windows, not from documentation or ReactOS

Applies when daemon code reproduces something Windows does itself, such as building a logon's environment from the
registry. Microsoft's documentation leaves the precedence and expansion order unstated, and ReactOS's `userenv`, the
readable stand-in, expands in a different order from Windows 11, so code written from either can be wrong in ways
its own tests then pin. Call the Windows function on a Windows machine and compare with it: for a logon's environment,
`CreateEnvironmentBlock` (`userenv.dll`) with the process's own token and inheritance off, from PowerShell through an
`Add-Type` P/Invoke, with a probe value set in `HKCU\Environment` only for the duration of the call. That answers it
without signing out, which on a shared machine also ends whatever runs in that desktop session.

## On a Windows runner, call System32's `tar.exe` by its full path

Applies to a script that extracts an archive with `tar` and can run in a GitHub Actions Windows job. Those jobs here run
in Git Bash (`shell: bash`), which puts its own GNU tar first on the PATH at every start: it reads no zip and takes the
`D:` of an absolute path for a remote host, so a bare `tar` that works on a Windows machine and on macOS fails there.
`scripts/fetch-conpty.mjs` names `%SystemRoot%\System32\tar.exe` for this reason, as `update-archive.mjs` names
`/usr/bin/tar` on macOS. Adding a directory with `$GITHUB_PATH` does not get ahead of Git Bash's own: Git Bash puts its
directories first at every start, after the runner has set the PATH.

## Prove what another Windows user can see as that user, through its own logon

Applies when a Windows check has to show that another standard user cannot get a key the daemon or a session's helper
accepts, from a command line, an environment or a file.

- Run the scan as the second user in a logon of its own: over SSH with that user's password, or in a scheduled task run
  as that user (which needs the batch-logon right, "Log on as a batch job"). Never through `Start-Process -Credential`
  or `runas` from the first user's desktop: such a process also carries the caller's logon SID, which the first user's
  processes grant access to, so it opens what the second user cannot and reports a leak that is not there.
- `Get-CimInstance Win32_Process` run as a non-administrator gives a null `CommandLine` for every other user's process,
  which proves neither that command lines are hidden nor that they are exposed. To know what the command lines held,
  turn on process-creation auditing with command lines and read event 4688 as an administrator; to see whether the
  second user can read a process's environment, have it call `NtQueryInformationProcess` and read the PEB.
