# Verifying changes for Windows

## Cross-check Rust changes for Windows yourself, and report Windows-only code as unverified until it ran on Windows

Applies when you change Rust code in either crate on a Mac. `pnpm verify`, `pnpm verify --quick` and the pre-push
hook build and test for the host they run on only; none of them cross-checks Windows, so a change that breaks the
Windows build passes all of them, lands on main, and surfaces only when the release check (`pnpm verify --release`,
which does run `pnpm check:windows`) refuses to tag a version. Run `pnpm check:windows` (through `cargo xwin`; a
machine missing its toolchain is told what to install; `pnpm check:windows daemon` or `desktop` checks one crate)
before handing the change over.

It compiles both crates and links the daemon's executable, nothing more. It is `cargo check`, not clippy, and does not
deny warnings, while the `windows-check` job runs clippy on the daemon with `--all-targets` and `-D warnings`: treat
every warning it prints as a failure (a test helper only Unix tests use is gated to Unix, as they are), and expect
clippy-only lints in Windows-only code to surface only there. Windows-only code, and the tests that run
only on Windows, never run on the Mac: they run on the `windows-check` job of `.github/workflows/quality.yml`, which
runs only when started by hand, or on a Windows machine.
Until one of those has run them, say when reporting that such code is compile-checked only. A Windows machine needs no
Rust toolchain to run them: build the test binary on the Mac with
`cargo xwin test --target x86_64-pc-windows-msvc -p octoboardd --no-run`, copy the `octoboardd-<hash>.exe` it names
under `target/x86_64-pc-windows-msvc/debug/deps/` across, and run it there with a test-path filter. Put the x64
`conpty.dll` and `OpenConsole.exe` beside it (`node scripts/fetch-conpty.mjs --arch x64 --into <dir>`), as the
`windows-check` job does: without them the daemon runs on the inbox console host, where the Git Bash trust-screen
stand-ins fail whatever the change. Tests that read repository files through `env!("CARGO_MANIFEST_DIR")` (test data,
captured screens, the protocol files) look for them at the Mac checkout's path and fail there unless the files are
copied to that path; neither failure says anything about the change.

The desktop crate is compile-checked only by `pnpm check:windows` and the release check: neither links its
executable, so a change that can affect its link — linker arguments or `cargo:rustc-link-*` lines in its `build.rs`,
linker flags, embedded resources, a new native library — passes them unverified. It does link on the Mac by hand (the
build in the next section), which shows the link works with `lld-link`; report such a change as unverified on
`link.exe` until it was built on Windows (the `windows-bundle` job of `.github/workflows/quality.yml` builds the
installer). For such a change to the
daemon, the link `pnpm check:windows` runs leaves `octoboardd.exe` under `target/x86_64-pc-windows-msvc/debug/`; check
what landed in it with `llvm-readobj --coff-resources`. `cargo xwin` links with an `lld-link` that has neither libxml2
nor `mt.exe`, so anything that asks the linker to merge manifests fails there while `link.exe` on Windows accepts it.

## Run the app on a Windows machine from a build made on the Mac, laid out as the per-user install

Applies when the app has to run on a Windows machine or VM that cannot build it (a small VM disk is enough to rule
that out). Build `packages/ui/dist` first (both executables embed it), then cross-build on the Mac with
`cargo xwin build --release --target x86_64-pc-windows-msvc`, once with `-p octoboardd --bin octoboardd --features
embedded-ui` and once with `-p octoboard --features tauri/custom-protocol`, in the environment `windowsCheckEnv()`
(`scripts/lib/windows-toolchain.mjs`) returns plus `TAURI_CONFIG={"bundle":{"externalBin":[],"resources":[]}}`, as
`scripts/cargo.mjs` sets for the cross-check; run each under `node scripts/verify/heavy.mjs` so it waits for the
machine's heavy-step slot. Copy `Octoboard.exe`, `octoboardd.exe` and the x64 `conpty.dll` and `OpenConsole.exe`
(`scripts/fetch-conpty.mjs`) into `%LOCALAPPDATA%\Octoboard`, the per-user install's directory, not a folder of your
own: what looks for the installed daemon there, a remote host lookup included, finds it only at that path. The jump
list, the system menu, the flyout and the pipe all ran from that layout.

For arm64 (`--target aarch64-pc-windows-msvc`, with the arm64 `conpty.dll`), also put `arm64-clang-shim/` under
cargo-xwin's cache first on the PATH, as `scripts/cargo.mjs` does (any `pnpm check:windows arm64` run writes it):
without it `ring`'s build script fails, and switching cargo-xwin to its `clang` backend is no way around it. On Windows
on Arm, the x64 build cannot show an uninstall or a setup run while Octoboard runs, whatever the change: there the
uninstaller's Restart Manager check stops with "Failed to kill Octoboard"; check those cases with the arm64 build.

## Script a Windows GUI step as one task, and change appearance the way Settings does

Applies when a GUI step on Windows is driven from a script (a scheduled task on the user's desktop) and read off a
screen dump. The flyout closes as soon as it loses focus, so one script opens it and acts on it, and the screen dump
is taken while that script is still running (sleeping after the action), never from a later step.

Light or dark app mode written to the registry does not reach running apps until `WM_SETTINGCHANGE` with
`"ImmersiveColorSet"` is broadcast, so without the broadcast the app reads as ignoring the change; High Contrast can
be toggled from a script with `SystemParametersInfo(SPI_SETHIGHCONTRAST)`.

## Confirm Windows behaviour as a standard user on that user's own desktop, not over ssh or as an administrator

Applies when a change's behaviour on Windows is confirmed by running it, in a virtual machine or on hardware. Run the
daemon and the app as a standard (non-administrator) user whose name, and so home directory, holds non-ASCII
characters, with Developer Mode off and nothing elevated, from that user's own desktop session. That is what most users
have, and the opposite setup hides defects:

- An administrator's or an elevated process can make file symlinks, so every path for a host without file links
  (configuration sharing reported unavailable, a console's `CLAUDE.md` written as a plain file) never runs, and Codex
  behaves differently when run as administrator.
- A session over OpenSSH runs inside a kill-on-close job and passes the inherited "ignore Ctrl-C" flag to everything
  it starts; an administrator's runs at High integrity there (a standard user's at Medium, as on its desktop). So what
  it shows about elevation, Ctrl-C and process containment is not what the desktop app's daemon sees.
- An interactive scheduled task (`schtasks ... /IT`) does run on the user's desktop, but a daemon started from one
  may not see the desktop app's process containment. Seen once, on Windows 11 ARM64 with the x64 build emulated, for a
  task created with `/IT /RU <user>` on an administrator account with UAC off: the daemon's log said its job does not
  let a process break away, so its holders' terminals end with that job. Whether other tasks or setups do the same is
  unverified; confirm process containment from the desktop app instead.

Building and running the test suite over ssh is fine; a behaviour that any of the above can change is confirmed in the
setup above. What each of these does to the daemon is in `docs/architecture.md`, "Known pitfalls of the Tauri / Rust
approach".

## Reach a Windows VM from a Rust test on the Mac through a forwarded port

Applies when a test binary built on the Mac connects to a Windows machine on the local network itself, such as the live
Windows device-key test in `apps/desktop/src-tauri/src/remote/authorized_keys.rs` (its `OCTOBOARD_TEST_WINDOWS_SSH_*` variables
are described there). macOS local-network privacy answers such a binary's connection with "No route to host", while
`ssh` from the terminal reaches the same address, so the failure says nothing about the code. Forward a loopback port
first, `ssh -f -N -L 127.0.0.1:22922:127.0.0.1:22 <user>@<vm>`, point the test at it
(`OCTOBOARD_TEST_WINDOWS_SSH_HOST=127.0.0.1`, `OCTOBOARD_TEST_WINDOWS_SSH_PORT=22922`), and end the forward by its pid
afterwards.

## Learn what Windows itself does by calling its own API on Windows, not from documentation or ReactOS

Applies when daemon code reproduces something Windows does itself, such as building a logon's environment from the
registry. Microsoft's documentation leaves the precedence and expansion order unstated, and ReactOS's `userenv`, the
readable stand-in, expands in a different order from Windows 11, so code written from either can be wrong in ways
its own tests then pin. Call the Windows function on a Windows machine and compare with it: for a logon's environment,
`CreateEnvironmentBlock` (`userenv.dll`) with the process's own token and inheritance off, from PowerShell through an
`Add-Type` P/Invoke, with a probe value set in `HKCU\Environment` only for the duration of the call. That answers it
without signing out, which on a shared machine also ends whatever runs in that desktop session.

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
