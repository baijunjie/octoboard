# Octoboard desktop application

The Tauri 2 shell of the macOS desktop application: the native window and menu bar, the `octoboardd` sidecar, the exit
flow, and keeping the application running in the background behind a menu bar icon once its window is closed, when the
user has chosen that. It contains no UI of its own: the window loads the UI in
[`../../packages/ui/`](../../packages/ui/README.md), bundled into the application at build time.

The UI talks to `octoboardd` (see [`../daemon/`](../daemon/README.md)) only over the WebSocket/HTTP protocol in
[`../daemon/PROTOCOL.md`](../daemon/PROTOCOL.md) — no Tauri IPC command carries daemon traffic, and the shell never asks
the daemon anything. IPC carries only bare signals, display text or opaque ids the UI words from what it already shows,
the macOS privacy grants, and the text of a file the user saves or opens through the system's own dialog. The shell
starts `octoboardd` as a sidecar and bakes the port it printed (`?port=`) and the system's preferred languages
(`&languages=`) into the window's URL.

## Development

This is the `@octoboard/desktop` package of the repository's pnpm workspace: install from the workspace root with
`pnpm install`, then run its scripts from this directory. It has no `typecheck` or `build` script; the Rust side is
checked with `cargo`. `src-tauri/` is a member of the root Cargo workspace alongside `apps/daemon/` (`cargo build` /
`cargo test` from the root cover both; `-p octoboard` narrows to this crate).

- `pnpm build:daemon` (`scripts/build-daemon.mjs`) — builds `apps/daemon/` in release mode and copies the binary into
  `src-tauri/binaries/`. Run it once before `pnpm tauri dev` or any `cargo build` / `cargo test` that includes `src-tauri/`
  (including a workspace-wide one from the root), and again when `apps/daemon/` changes.
- `pnpm tauri dev` — starts the `packages/ui` dev server (port 5174) and opens the window on it.
- `pnpm build:tauri` — builds the daemon sidecar, then `packages/ui`, whose `dist/` the application bundles.
- `pnpm build:app` — builds the app without signing, to check a change (below).
- `pnpm build:install` — builds, signs and installs the app locally (below).
- `pnpm release` — builds the distributable `.app` and `.dmg` (below).

### Local installation

`pnpm build:install` (`scripts/install-app.mjs`, also available from the repository root) builds, signs and installs
the `.app`:

1. Put `APPLE_SIGNING_IDENTITY` (a valid signing certificate in the keychain) in the gitignored repo-root `.env.secret`.
2. Copy [`.env.install.example`](../../.env.install.example) to `.env.install.local` and set `OCTOBOARD_INSTALL_DIR`
   to an absolute installation directory. Environment variables take precedence over file values.
3. Quit any running Octoboard, then run `pnpm build:install` (it takes no arguments).

It builds only the `.app`, never notarizes, and replaces the installed application only after verifying the built
signature, keeping the previous bundle for rollback if replacement fails.

### Release builds

`pnpm release` (`scripts/release.mjs`) runs `tauri build` (which builds the daemon sidecar and frontend first), producing
the `.app` and a `.dmg` for the host's own architecture only, then verifies the result and, when notarizing, submits and
staples the `.dmg` too. Signing and notarization read `APPLE_*` variables from the gitignored repo-root `.env.secret`
(template `.env.secret.example`; a variable already in the environment wins). Notarization uses an App Store Connect
API key only, so `APPLE_ID` / `APPLE_PASSWORD` in the environment is refused.

- Nothing set: an unsigned build, with the signing-dependent verification skipped.
- `APPLE_SIGNING_IDENTITY` (a "Developer ID Application" identity) plus `APPLE_API_KEY`, `APPLE_API_ISSUER` and
  `APPLE_API_KEY_PATH`: signed, notarized and stapled.
- `APPLE_SIGNING_IDENTITY` without the full API key trio: refused.

`pnpm build:app` (`scripts/build-app.mjs`) runs the same `tauri build` with every `APPLE_*` variable removed from the
child's environment, so it can neither sign nor notarize, and fails if the `.app` carries a Developer ID authority.
Extra arguments go to `tauri build` (for example `pnpm build:app --bundles app` to skip the `.dmg`).

## External interfaces

The shell defines eleven Tauri IPC commands, permitted through `src-tauri/capabilities/default.json` and
`src-tauri/permissions/*.toml`:

- `frontend_exit_heartbeat` — marks the webview as the one handling the exit flow, and is re-invoked on every quit gesture.
- `confirm_quit` — marks a pending quit as user-confirmed and exits.
- `set_menu_labels` (`menu.rs`) — takes the menu's labels from the UI and rebuilds the native menu bar.
- `set_tray_menu` (`tray.rs`) — takes the menu bar icon's menu (worded lines, session ids, quota submenus of headers, window rows and notes, picture keys (the submenus' opening lines carry an agent mark too), and the `attention` flag that raises the icon's hand) from the UI.
- `set_run_in_background` (`background.rs`) — the UI's choice of whether closing the window keeps the app running.
- `bring_to_front` (`background.rs`) — shows, restores and activates the window.
- `macos_permissions_status`, `macos_request_permission`, `macos_open_privacy_settings` (`permissions.rs`) — read, request
  and open the pane of the Screen Recording and Accessibility grants.
- `save_configuration_file`, `open_configuration_file` (`configuration_file.rs`) — move the configuration document
  through the system's save and open dialogs.

Events the shell emits to the webview: `settings-requested` (the menu's Settings… item), `tray-session-chosen` (a session
line of the menu bar icon's menu, carrying the session id), `tray-menu-opened` (that menu begins to open), `exit-requested` (a quit gesture needing the UI's exit
flow) and `daemon-exited` (the sidecar ended). `packages/ui/src/platform/tauri.ts` maps commands and events onto the
platform adapter's capabilities. `capabilities/default.json` also allowlists the notification, clipboard read and write text,
shell open, Dock badge, window theme, window show and window drag/zoom commands the UI uses.

## Layout

| Path | Role |
|---|---|
| `src-tauri/build.rs` | Exposes the app name from the repo-root `config/app.json` as `OCTOBOARD_APP_NAME`, and fails the build when `productName` in `tauri.conf.json` or the workspace `repository` no longer matches that file |
| `src-tauri/src/lib.rs` | `run()`: builds the Tauri app, wires the menu, exit flow, close request and reopen event, and opens the main window (overlay titlebar, translucent material, hidden until the UI reveals it) |
| `src-tauri/src/exit.rs` | The exit-confirmation flow: `ExitState`, the two exit-flow commands, `request_quit` and `request_quit_from_window`, and the `applicationShouldTerminate:` override for Dock Quit and system shutdown |
| `src-tauri/src/background.rs` | Running in the background: `set_run_in_background`, the close button hiding the window when chosen, and `bring_to_front` |
| `src-tauri/src/tray.rs` | The menu bar icon: its menu rebuilt from `set_tray_menu`, its switch between the two icon images together with the orange raised-hand overlay view, and the `tray-session-chosen` and `tray-menu-opened` events |
| `src-tauri/src/tray_lines.rs` | The rows of the menu bar icon's menus, each a menu item with a view of its own: the sessions' section headings, an account's header, a note, and a quota window's name, detail and bar; none of them dims or highlights |
| `src-tauri/src/menu.rs` | The native macOS menu bar built from the UI's labels, including the Settings… item |
| `src-tauri/src/sidecar.rs` | Spawns `octoboardd`, parses its startup port line, reports how it terminated, and keeps `DaemonState` |
| `src-tauri/src/permissions.rs` | The macOS privacy permission commands and `capture_launch_state()` |
| `src-tauri/src/configuration_file.rs` | The two configuration-file commands |
| `src-tauri/src/window_state.rs` | Remembers the window's frame and maximized state across launches and validates it against the connected displays |
| `src-tauri/src/page_isolation.rs` | macOS only: the webview configuration that blocks every `http(s)` request the page would make |
| `src-tauri/capabilities/default.json`, `src-tauri/permissions/` | The IPC allowlist and the app-defined permission files it names |
| `src-tauri/tauri.conf.json` | Where the window's UI comes from, the `octoboardd` `externalBin`, and the bundle targets |
| `src-tauri/Info.plist` | Merged into the bundle's `Info.plist`; declares the UI languages as `CFBundleLocalizations` |
| `src-tauri/icons/tray-template.png` | The menu bar icon's monochrome template image, derived by hand from the app icon; no script regenerates it |
| `src-tauri/icons/tray-attention-template.png` | The octopus with the corner cut out where the raised hand goes and no hand, a template image shown while a session waits for the user; drawn from `tray-template.png` by a one-off script not kept in the repo; no script regenerates it |
| `src-tauri/icons/tray-attention-hand-template.png` | The raised hand alone on the same canvas, black for its alpha; `tray.rs` tints it the system orange in a view over the status item's button; drawn from lucide's `Hand` by the same one-off script; no script regenerates it |
| `scripts/build-daemon.mjs`, `scripts/build-app.mjs`, `scripts/install-app.mjs`, `scripts/release.mjs` | The build, local-verification, local-install and release scripts above |
| `scripts/bundle.mjs` | Where `tauri build` puts its output, and the lookup for the bundle it produced; shared by the build scripts |
