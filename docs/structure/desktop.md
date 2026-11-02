# Desktop shell structure (`apps/desktop/`)

`@octoboard/desktop`: the Tauri 2 shell of the macOS desktop application. Its Rust crate `octoboard`
(`apps/desktop/src-tauri/`, library `app_lib`, Tauri `~2.12` with the tray-icon and macOS private-API features, plus the
shell, notification, clipboard, dialog and updater plugins) is a member of the root Cargo workspace beside
`apps/daemon/`. The npm package holds only the build, install, release and release-publishing scripts. The shell has no
UI of its own: the window loads the UI of `packages/ui/`, bundled at build time, and starts `octoboardd` as a sidecar.

The shell never carries daemon traffic. The UI talks to the daemon over the WebSocket/HTTP protocol
([`PROTOCOL.md`](../../apps/daemon/PROTOCOL.md)); the shell bakes the daemon's port and per-launch key and the system's
preferred languages into the window's URL, restarts a daemon that ends unexpectedly and points the window at it, and
passes only bare signals, display values, opaque ids, macOS privacy grants, update information and the text of a file
the user saves or opens through the system dialog over IPC.

Paths in this doc that do not start with `apps/` or `packages/` are relative to `apps/desktop/`.

## Directory map

### `src-tauri/`

| Path | Role |
|---|---|
| `build.rs` | Exposes the app name and repository address from `config/app.json`; fails the build when `tauri.conf.json`, the workspace `repository`/`license` or the two version values disagree (the version lives in `apps/desktop/package.json` and the root `Cargo.toml`) |
| `src/lib.rs`, `src/main.rs` | `run()` builds the Tauri app, wires the menu, exit flow, close request and reopen event, and opens the hidden main window; `main.rs` is the binary entry |
| `src/sidecar.rs`, `src/recovery.rs` | Starting `octoboardd`, parsing its handshake line into the daemon's address, keeping `DaemonState` and a tail of its stderr; restarting a daemon that ended, with a budget and backoff, and telling the window when the budget is spent or when an installed update is waiting for its restart (the daemon is then left alone) |
| `src/exit.rs`, `src/background.rs` | The exit-confirmation flow and the `applicationShouldTerminate:` override; running in the background behind the menu bar icon, `bring_to_front` |
| `src/menu.rs`, `src/tray/` | The native menu bar built from the UI's labels; the menu bar icon: `mod.rs` creation and `set_tray_menu`, `model.rs` the menu as the UI sends it, `items.rs` native items, `menu_tracking.rs`, `finish.rs`, `status_item.rs`, `icon.rs`, `lines.rs` the custom session and meter rows, `openers.rs` the quota openers (the rows that open a quota submenu). The model carries no agent knowledge: the UI sends each window's and meter's fill colour |
| `src/permissions.rs`, `src/notification_settings.rs`, `src/configuration_file.rs`, `src/updater.rs` | The IPC commands for macOS privacy grants, the Notifications settings pane, the configuration file dialogs, and checking for and installing updates |
| `src/system_settings.rs` | The one way the shell opens a System Settings pane (`open_system_settings`), and the address tables of the privacy and notification panes; `permissions.rs` and `notification_settings.rs` go through it |
| `src/window_state.rs`, `src/page_isolation.rs`, `src/diagnostics.rs` | Remembering and validating the window frame; blocking every `http(s)` request the page would make; the `diagnostic!` log to stderr and `desktop.log` |
| `capabilities/default.json`, `permissions/*.toml` | The IPC allowlist and the app-defined permission files it names |
| `tauri.conf.json`, `Info.plist`, `Entitlements.plist`, `icons/`, `images/` | Where the window's UI comes from (`index.html` with the window's query, which Tauri resolves against `devUrl` under `tauri dev` and against the bundled assets otherwise), the `octoboardd` `externalBin`, bundle targets, the public key that verifies the update archive (`createUpdaterArtifacts` is off: the scripts make and sign the archive); bundle metadata and the privacy usage descriptions (`Info.plist`); the entitlements for the privacy grants (Apple Events, camera, audio input, contacts, calendars, photos, location; `Entitlements.plist`, wired by `bundle.macOS.entitlements`); app and tray icons; the DMG background. `icons/AppIcon.icon/` is the macOS 26 layered icon (an Icon Composer package listed in `bundle.icon`, compiled by Xcode 26's `actool`); its `Assets/octopus.png` is derived by hand from `assets/branding/logo.png` and `icons/tray-template.png` by hand from the app icon, and no script regenerates either |

### [`scripts/`](../../apps/desktop/scripts/README.md) (`apps/desktop/scripts/`)

The sidecar build, local build, local install, release, update-signing and release-publishing scripts, with their shared
`bundle.mjs`, `update-archive.mjs` (packs and signs the update archive), the testable `release-manifest.mjs` and the
temporary `actool/` shim. Its README holds their interfaces, how the release
is built, published and tagged, and the rule that the version lives in two places.

## IPC surface

Commands permitted through `capabilities/default.json` and `permissions/*.toml`: the exit flow
(`frontend_exit_heartbeat`, `confirm_quit`, `daemon_exit_detail`), `set_menu_labels`, `set_tray_menu`,
`set_run_in_background`, `bring_to_front`, `macos_permissions_status`, `macos_request_permission`,
`macos_open_privacy_settings`, `macos_open_notification_settings`, `save_configuration_file`, `open_configuration_file`,
`check_for_update` and `install_update`. Events to the webview: `settings-requested`, `about-requested`,
`tray-session-chosen`, `tray-menu-opened`, `exit-requested` and `daemon-exited`. `packages/ui/src/platform/tauri.ts`
maps commands and events onto the platform adapter's capabilities; it is the UI's only counterpart of this surface.

## Dependency direction

- The shell depends on the daemon only as a sidecar executable and a handshake line; the UI depends on the shell only
  through the platform adapter. Neither the Rust crate nor the scripts import daemon code.
- `src-tauri/build.rs` reads the repo-root `config/app.json`; the scripts use `scripts/lib/env.mjs` and `scripts/lib/git.mjs` at
  the repository root.

## Where new code goes

- A new native capability the UI needs: a module in `src-tauri/src/`, its command registered in `lib.rs`, a permission
  file in `permissions/` and its entry in `capabilities/default.json`, and the adapter capability in
  `packages/ui/src/platform/`. A command that carries daemon data does not belong here.
- Menu bar icon work stays inside `src/tray/`: parse in `model.rs`, build in `items.rs`, native-only finishing in
  `finish.rs`, `status_item.rs`, `icon.rs`, `lines.rs` and `openers.rs`.
- A build or release step is a script in `scripts/` sharing `bundle.mjs`; the pure part of a publishing decision goes in
  `release-manifest.mjs` so it can be tested without GitHub.

## Shared entry points

| Need | Entry point |
|---|---|
| A diagnostic line | the `diagnostic!` macro (`src/diagnostics.rs`) |
| Stop the daemon before a restart or quit | `sidecar::stop_daemon` |
| Open a System Settings pane | `system_settings::open_system_settings` |
| Paths, `tauri build`, bundle checks in a script | `apps/desktop/scripts/bundle.mjs` |
| Pack and sign the update archive | `apps/desktop/scripts/update-archive.mjs` |
| Environment files and `git` in a script | `scripts/lib/env.mjs` (`loadEnv`) and `scripts/lib/git.mjs` at the repository root |

Consistency checks that hold structure: `src-tauri/build.rs` (app name, repository, license and the two version values),
`pnpm test` in this package (the release scripts and the update archive), and Rust format, clippy and tests through the repository checks.
