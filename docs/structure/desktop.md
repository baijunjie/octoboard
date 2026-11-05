# Desktop shell structure (`apps/desktop/`)

`@octoboard/desktop`: the Tauri 2 shell of the macOS and Windows desktop applications. Its Rust crate `octoboard`
(`apps/desktop/src-tauri/`, library `app_lib`, Tauri `~2.12` with the tray-icon and macOS private-API features, plus the
shell, notification, clipboard, dialog and updater plugins, and off macOS the single-instance plugin) is a member of the root Cargo workspace beside
`apps/daemon/`. The npm package holds only the build, install, release and release-publishing scripts. The shell has no
UI of its own: the window loads the UI of `packages/ui/`, bundled at build time, and starts `octoboardd` as a sidecar.

The shell never carries daemon traffic. The UI talks to the daemon over the WebSocket/HTTP protocol
([`PROTOCOL.md`](../../apps/daemon/PROTOCOL.md)); the shell bakes the daemon's port and per-launch key and the system's
preferred languages and the host's facts (`host_facts.rs`) into the window's URL, restarts a daemon that ends unexpectedly and points the window at it, and
passes only bare signals, display values, opaque ids, macOS privacy grants, update information, the text of a file
the user saves or opens through the system dialog, and the path of a directory the user chooses there over IPC.

Paths in this doc that do not start with `apps/` or `packages/` are relative to `apps/desktop/`.

## Directory map

### `src-tauri/`

A concern that differs by platform is a module with the shared surface in its `mod.rs` (or its own file) and the
implementations in `macos.rs` and `windows.rs` (a small concern keeps them as inline `mod macos` and `mod windows` in
its own file, as `languages.rs`, `notification_settings.rs`, `diagnostics.rs` and `sidecar.rs` do); `windows.rs` serves every target but macOS, Linux included, where what needs a Windows API is compiled out or does nothing.

| Path | Role |
|---|---|
| `build.rs` | On macOS targets generates the native localization resources through `scripts/native-localizations.mjs`, exposes the app name and repository address from `config/app.json`; fails the build when `tauri.conf.json`, the workspace `repository`/`license` or the two version values disagree (the version lives in `apps/desktop/package.json` and the root `Cargo.toml`) |
| `src/host_facts.rs` | The facts the UI takes from the machine instead of inferring them (system, shortcut modifier, window chrome, which native features exist, the system's words), per target OS, serialised as the `host` query parameter; mirrors `HostFacts` in `packages/ui/src/platform/host.ts` |
| `src/lib.rs`, `src/main.rs` | `run()` builds the Tauri app, wires the menu, exit flow, close request and the platform's lifecycle hooks, and opens the hidden main window once page isolation is ready, dressed by the window chrome; `main.rs` is the binary entry (a release build on Windows is a GUI subsystem program, so it opens no console window) |
| `src/chrome/`, `src/languages.rs` | The main window's frame (macOS: the overlay titlebar with the traffic lights and the translucent sidebar material; elsewhere the system's own frame) and, in `install`, what the built window's webview needs set on it (Windows: WebView2's browser accelerator keys, off in release builds and kept in debug builds, where reload and find help debugging); the system's preferred languages for the window's query (macOS; elsewhere none) |
| `src/sidecar.rs`, `src/recovery.rs` | Starting `octoboardd`, parsing its handshake line into the daemon's address, keeping `DaemonState` and a tail of its stderr; restarting a daemon that ended, with a budget and backoff, and telling the window when the budget is spent or when an update has replaced the application's files and waits for its restart (macOS; the daemon is then left alone, while a Windows update that is only downloaded still lets the daemon be restarted) |
| `src/exit.rs`, `src/background.rs`, `src/lifecycle/` | The exit-confirmation flow, and the way out that installs a downloaded update whichever way the quit got through (`on_exit_requested`, `install_on_exit`); running in the background behind the menu bar icon, `bring_to_front`; the platform's part around both (macOS: the `applicationShouldTerminate:` override, the reopen and activation, leaving fullscreen and the Dock hide and show; elsewhere empty hooks, and the single-instance plugin, registered first, whose callback shows the running window again when the app is launched a second time) |
| `src/menu/`, `src/tray/` | The native menu bar built from the UI's labels (`macos.rs`; elsewhere no menu is set and `set_menu_labels` accepts the labels and does nothing); the menu bar icon: `mod.rs` creation and `set_tray_menu`, `model.rs` the menu as the UI sends it, `items.rs` native items (a line's picture goes through the platform's `menu_picture`), `menu_tracking.rs` holding back a menu sent while one is open, `finish.rs` what each line needs finishing, `status_item.rs` which look the icon shows, `icon.rs` its pictures; `macos/` the rich status item (`mod.rs` passes `menu_picture` through, as AppKit tints template pictures itself): `finish.rs` the finishing pass, `menu_tracking.rs` following AppKit's menu tracking, `status_item.rs` the accessible name and the attention dot, `lines.rs` the custom session and meter rows, `openers.rs` the quota openers (the rows that open a quota submenu); `windows.rs` plain items with nothing to finish, single-colour template menu pictures recoloured from their alpha for the menu's theme (`menu_picture`; its `theme` module reads the app mode, `AppsUseLightTheme`, which the popup menu follows, not the taskbar's `SystemUsesLightTheme`, and high contrast) with the last menu rebuilt when the system's `ImmersiveColorSet` setting changes (`WM_SETTINGCHANGE`, `MenuTracking::rebuild`), the app icon in colour with the attention dot as its picture (no quota capsules), the label as the tooltip, the click that opens the menu reported as `tray-menu-opened`, and the menu's modal loop followed through a subclass of tray-icon's window (`WM_ENTERMENULOOP`, `WM_EXITMENULOOP`), so a menu sent while it is open is held back and applied from a timer that `WM_EXITMENULOOP` starts (the chosen item's `WM_COMMAND` is posted as the loop ends, and a timer message waits behind a posted one), which a reopened menu kills. The model carries no agent knowledge: the UI sends each window's and meter's fill colour |
| `src/permissions/`, `src/notification_settings.rs`, `src/configuration_file.rs`, `src/directory_picker.rs`, `src/updater/` | The IPC commands for macOS privacy grants (elsewhere they answer that there are none), the Notifications settings (macOS: the app's pane; Windows: the Settings page `ms-settings:notifications`, opened with `ShellExecuteW`; Linux a no-op), the configuration file dialogs, the system directory picker, and checking for and installing updates (`mod.rs` the commands, the phases and the manifest address, `macos.rs` installing in place when the download ends and naming the macOS manifests `stable.json` and `beta.json`, `windows.rs` naming the Windows manifests `stable-windows.json` and `beta-windows.json`, downloading only and starting the NSIS installer, which ends the process, from the restart (`install_for_restart`, which the exit flow's `confirm_quit` calls) and, without the relaunch, on every other way out of the application (`exit.rs`); `replaced_on_disk` says whether an update has replaced the files, which only the macOS install does before the restart) |
| `src/system_settings.rs` | macOS only: the one way the shell opens a System Settings pane (`open_system_settings`), and the address tables of the privacy and notification panes; `permissions/macos.rs` and `notification_settings.rs` go through it |
| `src/window_state.rs`, `src/page_isolation/`, `src/diagnostics.rs` | Remembering and validating the window frame (`initial_window` never fits it below `MIN_SIZE`, so on a screen smaller than that, such as 1024×768, Windows caps the width at the screen's, and the layout stays usable; what the system frame adds around the inner size is `chrome::SystemFrame`); blocking non-image `http(s)` requests in the window's webview, which on macOS is a WebKit content rule list (with a development-only localhost exception) and on Windows a WebView2 `WebResourceRequested` filter installed on the built window (`install`, answering 403; `policy.rs` decides, letting through images, the app's own `tauri.localhost` and `ipc.localhost`, and in debug builds `localhost`), and nothing on Linux; the `diagnostic!` log to stderr and `desktop.log` under `~/.octoboard/logs/`, where the home is `USERPROFILE` on Windows and `HOME` elsewhere, as the daemon reads it |
| `capabilities/default.json`, `permissions/*.toml` | The IPC allowlist and the app-defined permission files it names |
| `tauri.conf.json`, `tauri.macos.conf.json`, `tauri.windows.conf.json`, `Info.template.plist`, `Entitlements.plist`, `icons/`, `images/`, `licenses/` | Where the window's UI comes from (`index.html` with the window's query, which Tauri resolves against `devUrl` under `tauri dev` and against the bundled assets otherwise), the `octoboardd` `externalBin`, the public key that verifies the update archive (`createUpdaterArtifacts` is off: the scripts make and sign the archive); the base file holds what every target shares, and Tauri merges the file of the target OS over it (arrays replace): `tauri.macos.conf.json` the bundle targets, icons, the generated `.lproj` resources and `bundle.macOS`, `tauri.windows.conf.json` the NSIS target (unsigned, WebView2 through its bootstrapper, per-user install), the `icon.ico` among the icons, the ConPTY files `scripts/fetch-conpty.mjs --installer` lays out under `target/conpty-bundle/` (fetched by the Windows overlay's before-dev and before-build commands) and the Windows Terminal licence texts in [`licenses/windows-terminal/`](../../apps/desktop/src-tauri/licenses/windows-terminal/README.md) as bundle resources, and the updater's passive install mode; bundle metadata (`Info.template.plist`); the entitlements for the privacy grants (Apple Events, camera, audio input, contacts, calendars, photos, location; `Entitlements.plist`, wired by `bundle.macOS.entitlements`); app and tray icons; the DMG background. `icons/icon.ico` is the Windows icon, generated from `icons/icon.png` at 16 to 256 px. `icons/AppIcon.icon/` is the macOS 26 layered icon (an Icon Composer package listed in `bundle.icon`, compiled by Xcode 26's `actool`); its `Assets/octopus.png` is derived by hand from `assets/branding/logo.png` and `icons/tray-template.png` by hand from the app icon, and no script regenerates either |
| `localizations/`, `generated-localizations/`, `Info.plist` | Native permission catalogs (one JSON file per language) and their generated bundle resources; generation belongs to the scripts module below |

### [`scripts/`](../../apps/desktop/scripts/README.md) (`apps/desktop/scripts/`)

The native-resource generation, sidecar build, local build, local install, version preparation, release, update-signing
and release-publishing scripts, with their shared `bundle.mjs`, `update-archive.mjs` (packs and signs the update
archive), the testable `release-manifest.mjs` (which manifests each platform writes, the platform selection and the tag message's `Platforms:` line) and the temporary `actool/` shim. `release.mjs` dispatches on the host to
`release-macos.mjs` or `release-windows.mjs`; `windows-release.mjs` holds the Windows signing modes, the `signtool`
checks and the installer inspection, and `sign-file.mjs` is the per-file sign command of the `command` mode. `release-desktop.mjs` owns the
`release:version` command that prepares and pushes a version for CI (`--platforms` for a version of one platform); `tag-message.mjs` reads a tag message for `.github/release-notes.sh`. `publish-release.mjs` publishes the selected platforms (`--platforms`, both by default). The module README holds the scripts' interfaces,
the automated and manual release procedures, failure recovery, and the rule that the version lives in two places.

## IPC surface

Commands permitted through `capabilities/default.json` and `permissions/*.toml`: the exit flow
(`frontend_exit_heartbeat`, `confirm_quit`, `daemon_exit_detail`), `set_menu_labels`, `set_tray_menu`,
`set_run_in_background`, `bring_to_front`, `permissions_status`, `request_permission`,
`open_privacy_settings`, `open_notification_settings` (also the Windows notification settings), `save_configuration_file`, `open_configuration_file`,
`pick_directory`, `check_for_update` and `install_update`. Events to the webview: `settings-requested`,
`about-requested`, `tray-session-chosen`, `tray-updates-requested`, `tray-menu-opened`, `exit-requested` and
`daemon-exited`.
`packages/ui/src/platform/tauri.ts` maps commands and events onto the platform adapter's capabilities; it is the UI's
only counterpart of this surface.

## Dependency direction

- The shell depends on the daemon only as a sidecar executable and a handshake line; the UI depends on the shell only
  through the platform adapter. Neither the Rust crate nor the scripts import daemon code.
- `src-tauri/build.rs` reads the repo-root `config/app.json`; the scripts use `scripts/lib/env.mjs` and `scripts/lib/git.mjs` at
  the repository root.

## Where new code goes

- A new native capability the UI needs: a module in `src-tauri/src/`, its command registered in `lib.rs`, a permission
  file in `permissions/` and its entry in `capabilities/default.json`, and the adapter capability in
  `packages/ui/src/platform/`. A difference by OS the UI must know of is a field of `host_facts.rs` and `HostFacts`, not a user-agent
  test. A command that carries daemon data does not belong here.
- Menu bar icon work stays inside `src/tray/`: parse in `model.rs`, build in `items.rs`, pictures in `icon.rs`,
  native-only finishing in `macos/` (`windows.rs` for its counterpart).
- Code that differs by platform goes into the concern's `macos.rs` and `windows.rs` (or its inline `mod macos` and
  `mod windows`) behind the shared surface its `mod.rs` selects, not behind `cfg(target_os)` in shared code. What
  shared code may still carry is no behaviour of its own: the selection of the platform half; a module that exists on
  one platform only (`system_settings.rs`); lint attributes for code a platform does not reach
  (`cfg_attr(..., allow(dead_code))`); `cfg(any(windows, test))` on a pure helper only the Windows half calls, so its
  tests run everywhere; the per-target fact tables of `host_facts.rs`; and a test step for what a platform's file
  system lacks.
- A desktop build or release command belongs in this package's `scripts/` and `package.json`. Build steps share
  `bundle.mjs`; the pure part of a publishing decision goes in `release-manifest.mjs` so it can be tested without
  GitHub.

## Shared entry points

| Need | Entry point |
|---|---|
| A diagnostic line | the `diagnostic!` macro (`src/diagnostics.rs`) |
| Stop the daemon before a restart or quit | `sidecar::stop_daemon`: waits for the daemon to exit after the UI's `shutdown`, then sends `POST /shutdown` (`sidecar::request_shutdown`), or SIGTERM on macOS when the request fails (the platform half of `sidecar::terminate`; Windows has no signal), then kills (`TerminateProcess` on Windows). The sidecar is started through the shell plugin, which on Windows gives it a hidden console (`CREATE_NO_WINDOW`) |
| Relaunch, applying a downloaded update | `exit::confirm_quit` with `restart`: under the `installing` guard it shares with `quit_installing` and `install_on_exit`, it stops the daemon (`sidecar::stop_daemon`) and calls `updater::install_for_restart`, which on Windows starts the held installer with the relaunch, ending the process after saving `window_state` and removing the tray icon, and on macOS does nothing; when it returns, the guard is given back and `app.request_restart()` relaunches |
| End the application, installing a downloaded update | `exit::on_exit_requested` and `exit::install_on_exit`, which stop the daemon and call `updater::install_on_quit` (Windows; nothing is held on macOS) |
| Open a System Settings pane | `system_settings::open_system_settings` |
| Paths, `tauri build`, bundle checks in a script | `apps/desktop/scripts/bundle.mjs` |
| Pack and sign the update archive (or sign the Windows installer) | `apps/desktop/scripts/update-archive.mjs` |
| Windows code-signing mode, `signtool` checks, installer inspection | `apps/desktop/scripts/windows-release.mjs` |
| Environment files and `git` in a script | `scripts/lib/env.mjs` (`loadEnv`) and `scripts/lib/git.mjs` at the repository root |

Consistency checks that hold structure: `src-tauri/build.rs` (app name, repository, license and the two version values),
`pnpm test` in this package (the release scripts and the update archive), and Rust format, clippy and tests through the repository checks.
