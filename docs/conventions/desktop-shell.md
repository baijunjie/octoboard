# Desktop shell conventions

The rules for the Tauri shell of the macOS and Windows applications (`apps/desktop/`: the Rust crate in `src-tauri/` and
the scripts around it). Where a module lives, what depends on what and the shared entry points are in
`docs/structure/desktop.md`; the UI's side of the platform adapter is covered in `docs/structure/ui.md`; the
provider-neutral rule applies to the shell like to any other code. Paths below are relative to
`apps/desktop/src-tauri/src/` unless they start with `apps/` or `packages/`.

## What the shell carries

- **A command that carries daemon data does not belong in the shell.** The UI talks to the daemon over the network
  protocol alone (no Tauri IPC, no shared state), which is what lets a browser or a remote window run the same UI; the
  shell passes only bare signals, display values, opaque ids, grants, update information and user-chosen files or
  paths. Terminal output in particular never crosses Tauri IPC: the frontend connects to the daemon over WebSocket.
- **The UI reaches a native capability only through the platform adapter** (`packages/ui/src/platform/`), chosen once at
  startup, and `platform/tauri.ts` is the only counterpart of the IPC surface. In a browser a missing capability means
  the feature is absent, not that it fails.
- **A new native capability the UI needs is added in four places**: a module under `src/`, its command registered in
  `lib.rs`, a permission file in `permissions/` with its entry in `capabilities/default.json`, and the capability in
  the adapter. A command a remote window may call is listed in `capabilities/remote.json`, the remote windows' general
  allowlist (`remote-window.json` holds only port forwarding and file copies); the shell never gives a remote window
  the remote daemons' commands.
- **A difference by operating system that the UI must know is a field of `HostFacts`** (`host_facts.rs`, mirrored in
  `packages/ui/src/platform/host.ts`), never a user-agent test: the shell states the machine's facts once, per target.
- **A setting the shell must know before the page loads is a field of `AppSettings`** (`app_settings.rs`), applied in
  `set_app_settings`; the page side (`createShellPreference`) is in `docs/conventions/ui-code.md`. The shell cannot read
  the page's storage, and the tray and the background mode start before any page exists.
- **The shell depends on the daemon only as a sidecar executable and its handshake line.** Neither the crate nor the
  scripts import daemon code, so the two release independently.
- **The Windows window uses the system's title bar and frame and has no application menu**; its shortcuts are
  registered by the page, which handles them wherever there is no native menu.

## Code that differs by platform

- **Platform code goes into the concern's `macos.rs` and `windows.rs`** (or inline `mod macos` / `mod windows`) behind a
  shared surface selected in `mod.rs`, not behind `cfg(target_os)` in shared code. Shared code may carry only the
  selection of the platform half, a module that exists on one platform (`system_settings.rs`),
  `cfg_attr(..., allow(dead_code))`, `cfg(any(windows, test))` on a pure helper only the Windows half calls (so its
  tests run everywhere), the fact tables of `host_facts.rs`, and a test step for what a platform's file system lacks.
- **`windows.rs` serves every target but macOS, Linux included**: what needs a Windows API is compiled out, does
  nothing or has a development stand-in. A Linux checkout must still compile and test the shell's shared code.
  `pnpm check:windows` catches Windows compile breaks only; it does not test the code.

## Shared entry points with a rule behind them

The list of helpers is in `docs/structure/desktop.md`, "Shared entry points"; these are the ones with a prohibition.

- **Open an address only through `open_address::open` / `open_file`** (a System Settings pane, a Settings page, the
  browser, a downloaded remote file), and never repeat the address in a failure message: an address can carry a user
  name, a host or a key.
- **Write a diagnostic with `diagnostic!`**, not `eprintln!`, so it reaches `desktop.log` as well as stderr; apart from
  `diagnostic!` itself (`diagnostics.rs`), only the forwarding of the daemon's own output (`sidecar/mod.rs`) uses
  `eprintln!`, since it is not the shell's diagnostic. Write JSON under the app config directory with
  `config_dir::write_json`, which writes atomically.
- **Stop the daemon before a restart or quit only through `sidecar::stop_daemon` with a `StopKind`** (the update restart
  is the planned stop, which hands the sessions over instead of interrupting them), and once the exit flow exists leave
  or end the application only through `exit::confirm_quit`, `exit::on_exit_requested` and `exit::on_exit`. macOS Quit
  does not raise Tauri's `ExitRequested`, so a path that bypasses these skips the confirmation, the quit note and the
  update install. A startup failure before it exists may end the shell directly.

## Native callbacks and threads

- **Do not rely on `AppHandle::run_on_main_thread` to run work after the current native callback returns.** Called on
  the main thread (from a menu event, an observer, a delegate method, a window procedure) it runs the closure at once.
  Queue such work with `NSOperationQueue::mainQueue().addOperationWithBlock` and read the state it depends on inside
  the block (`tray/macos/menu_tracking.rs` does); from a background thread `run_on_main_thread` is still the way onto
  the main thread.
- **On Windows never create a window in place from a native callback or a synchronous command.** The new window stays at
  `about:blank` (the Mac shows no symptom); showing a window that already exists is not affected. Leave the callback
  first: `tauri::async_runtime::spawn` a task that calls `run_on_main_thread`, as `menu/system_menu.rs` does.
- **A state the shell announces with an event and that stays true afterwards is also kept in the shell** and readable by
  a command the page invokes once its `listen` is registered, reporting once whichever arrives first: `daemon-exited`
  with `daemon_exit_detail`. An event reaches only a page that is already listening, and the shell itself reloads the
  window when it points it at a restarted daemon. A momentary gesture (a menu item chosen) needs no command.
- **A hand-written native hook takes the AppKit object from the notification or delegate call that hands it over**
  (the `NSMenu` that is the object of `NSMenuDidBeginTrackingNotification`), rather than reading it back through
  `tauri`, `tray-icon` or `muda` internals: those accessors give up quietly after a crate update, and the hook then
  does nothing while everything still compiles. When a change moves one of those crates, find the hooks (search for
  `with_inner_tray_icon`, `ns_status_item`, `ns_window`) and confirm in the running app that each still works.
- **A hand-written `objc2` `msg_send!` declares each object answer as `Option<Retained<_>>`** and turns nil into the
  caller's "nothing there" (`apple_event.rs` is the shape); a plain `Retained<_>` only where Apple's header marks the
  answer nonnull. A nil in a plain `Retained<_>` panics, and inside an AppKit callback the panic is swallowed by the
  override's fallback with nothing logged.

## Native menus

- **A native menu line of unbounded length (a session title, a project or console name) is cut where the menu's data is
  built in `packages/ui`**, with `ellipsize` from `packages/ui/src/i18n/displayColumns.ts` to a fixed number of display
  columns; never by `length` or code points, since a CJK character is about twice as wide as a Latin one. AppKit cuts a
  too-wide item in its middle without an error.
- **Anything the shell places against a menu's width uses `NSMenu::size().width` clamped to what the data's caps keep
  the menu under**, never the reported width alone: past AppKit's own cap the reported width is wider than what is
  shown.
- **A command that is a native menu item has its binding pushed to the shell as the menu accelerator**
  (`useNativeMenuLabels`, `set_menu_labels`), and `packages/ui/src/shortcuts/validate.ts` refuses Shift with a number
  or symbol key for it: AppKit matches a menu key against the character with Shift applied, so that combination never
  fires. The window's own key listeners never see a native menu item's key.

## Tests in the shell

- **A test that starts a process holds `crate::spawning()` across the start**, code under test that starts one takes it
  under `#[cfg(test)]` (`command_line` in `remote/device_key.rs`), and a test that takes a file lock and asserts on its
  release holds the guard for the whole test. A process forked while another test has a locked file open inherits it
  until the exec, so the lock appears held and the test fails now and then.
