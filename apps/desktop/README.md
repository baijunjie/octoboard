# Octoboard desktop application

The Tauri 2 shell of the macOS desktop application: the native window and menu bar, the `octoboardd` sidecar, the exit
flow, and keeping the application running in the background behind a menu bar icon once its window is closed, when the
user has chosen that. It contains no UI of its own: the window loads the UI in
[`../../packages/ui/`](../../packages/ui/README.md), bundled into the application at build time.

The UI talks to `octoboardd` (see [`../daemon/`](../daemon/README.md)) only over the WebSocket/HTTP protocol in
[`../daemon/PROTOCOL.md`](../daemon/PROTOCOL.md) — no Tauri IPC command carries daemon traffic, and the shell never
asks the daemon anything. IPC carries only bare signals, display values or opaque ids the UI derives from what it
already shows, the macOS privacy grants, an update's channel, version and notes (and the download progress
`install_update` streams), and the text of a file the user saves or opens through the system's own dialog. The shell
starts `octoboardd` as a sidecar and bakes the port it printed (`?port=`) and the system's preferred languages
(`&languages=`) into the window's URL.

## Development

This is the `@octoboard/desktop` package of the repository's pnpm workspace: install from the workspace root with
`pnpm install`, then run its scripts from this directory. It has no `typecheck` or `build` script; the Rust side is
checked with `cargo`. `src-tauri/` is a member of the root Cargo workspace alongside `apps/daemon/` (`cargo build` /
`cargo test` from the root cover both; `-p octoboard` narrows to this crate).

- `pnpm build:daemon` (`scripts/build-daemon.mjs`) — builds `apps/daemon/` in release mode and copies the binary into
  `src-tauri/binaries/`. Run it once before `pnpm tauri dev` or any `cargo build` that includes `src-tauri/` (including a
  workspace-wide one from the root), and again when `apps/daemon/` changes. Without it the build fails at compile time,
  saying the `binaries/octoboardd-<target triple>` resource does not exist. `pnpm build:daemon --universal` builds
  the sidecars of a universal release (below) ahead of time, as `pnpm release` does through Tauri.
- `pnpm test:rust` (from the repository root, `scripts/test-rust.mjs`) — runs `cargo test --workspace`, the tests of
  both crates in one invocation, so their shared dependencies build once. It sets `TAURI_CONFIG` to empty
  `bundle.externalBin`, which the tests can do because they never start the sidecar, so it needs no daemon build; a
  plain `cargo test` that includes `src-tauri/` still needs one. Switching between the two rebuilds this crate.
- `pnpm tauri dev` — starts the `packages/ui` dev server (port 5174) and opens the window on it.
- `pnpm build:tauri` — builds the daemon sidecar, then `packages/ui`, whose `dist/` the application bundles.
- `pnpm build:app` — builds the app without signing, to check a change (below).
- `pnpm build:install` — builds, signs and installs the app locally (below).
- `pnpm release` — builds the distributable `.app`, `.dmg` and signed update archive (below).
- `pnpm publish-release` — publishes what `pnpm release` built for the in-app updates to find (below).
- `pnpm test` — the Node tests of the release scripts (`scripts/*.test.mjs`).

### Local installation

`pnpm build:install` (`scripts/install-app.mjs`, also available from the repository root) builds, signs and installs
the `.app`:

1. Put `APPLE_SIGNING_IDENTITY` (a valid signing certificate in the keychain) in the gitignored repo-root `.env.secret`.
2. Copy [`.env.install.example`](../../.env.install.example) to `.env.install.local` and set `OCTOBOARD_INSTALL_DIR`
   to an absolute installation directory. Environment variables take precedence over file values.
3. Quit any running Octoboard, then run `pnpm build:install` (it takes no arguments).

It builds only the `.app`, never notarizes, and replaces the installed application only after verifying the built
signature, keeping the previous bundle for rollback if replacement fails.

It needs Xcode 26's `actool` as the selected developer directory (`xcode-select` or `DEVELOPER_DIR`), and fails when the
built `.app` lacks the layered app icon (`Assets.car` and `CFBundleIconName`).

### Release builds

`pnpm release` (`scripts/release.mjs`) runs `tauri build --target universal-apple-darwin` (which builds the daemon sidecar
and frontend first), producing one universal `.app` and `.dmg` (both Mac architectures, under
`target/universal-apple-darwin/release/bundle`), then verifies the result and, when notarizing, submits and
staples the `.dmg` too. Signing and notarization read `APPLE_*` variables from the gitignored repo-root `.env.secret`
(template `.env.secret.example`; a variable already in the environment wins). Notarization uses an App Store Connect
API key only, so `APPLE_ID` / `APPLE_PASSWORD` in the environment is refused.

- Nothing set: an unsigned build, with the signing-dependent verification skipped. It makes no update archive and
  needs no update key.
- `APPLE_SIGNING_IDENTITY` (a "Developer ID Application" identity) plus `APPLE_API_KEY`, `APPLE_API_ISSUER` and
  `APPLE_API_KEY_PATH`: signed, notarized and stapled, and, with the update key below, an update archive too.
- `APPLE_SIGNING_IDENTITY` without the full API key trio: refused.

A signed build also needs the update signing key, `TAURI_SIGNING_PRIVATE_KEY` (the key file's path or the key itself;
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` for a key made with a password) in `.env.secret`, and is refused without it.
It is a different key from Apple's: it signs `Octoboard.app.tar.gz`, the archive the in-app update downloads, and the
application checks that signature against `plugins.updater.pubkey` in `tauri.conf.json` before it installs anything. The
key pair comes from `pnpm tauri signer generate -w ~/.tauri/octoboard-updater.key` (empty password allowed); the public
half is the file's `.pub`. Never commit the private key, and keep a backup: with the key lost, no installed copy can ever
be updated again, and with it leaked, anyone can ship an update to every one of them. After the build the script checks
that `Octoboard.app.tar.gz` and its `.sig` exist next to the `.app`; the build first deletes the ones an earlier build left, so
they are always this build's.

A universal build needs both Rust targets (`rustup target add aarch64-apple-darwin x86_64-apple-darwin`). For it
`scripts/build-daemon.mjs`, which Tauri runs as `beforeBuildCommand`, sees `TAURI_ENV_TARGET_TRIPLE` and builds the daemon
for both architectures into `src-tauri/binaries/` (`octoboardd-<triple>` for each, which each per-architecture build
resolves, and their `lipo` join `octoboardd-universal-apple-darwin`, which the bundler wants). `pnpm build:app` and
`pnpm build:install` stay on the host's architecture.

Like `pnpm build:install`, it needs Xcode 26's `actool` as the selected developer directory and fails when the `.app` lacks
the layered app icon.

`pnpm build:app` (`scripts/build-app.mjs`) runs the same `tauri build` with every `APPLE_*` variable removed from the
child's environment, so it can neither sign nor notarize, and fails if the `.app` carries a Developer ID authority.
Extra arguments go to `tauri build` (for example `pnpm build:app --bundles app` to skip the `.dmg`).
A missing layered app icon only produces a warning there. `pnpm build:app` and `pnpm build:install` also drop the
update key variables and turn the update archive off (`createUpdaterArtifacts` in `tauri.conf.json`), so neither needs
the key nor signs anything with it.

### In-app updates and publishing a release

The application looks for a newer version a short while after launch and then every six hours, and on request from
Settings' About section, which also shows the running version and a **channel** choice. A found update is announced in a notice that
never installs by itself: the user presses Download and install, and once the new `.app` has replaced the old one in
place, the notice says it takes effect after a restart (Restart now goes through the exit confirmation, since a restart
ends the running sessions; Later only hides it). The channel is kept by the UI in `localStorage`
(`updateChannel` in `packages/ui/src/preferenceKeys.ts`) and is **beta** by default.

Each channel is a static manifest, in Tauri's updater format, on one fixed-tag release of the public repository
(`repositoryUrl` in `config/app.json`): the release `updater` holds `beta.json` and `stable.json`
(`.../releases/download/updater/<channel>.json`). Every release is written to `beta.json`, and one without a semver
prerelease suffix (`0.2.0`, not `0.2.0-beta.1`) to `stable.json` too, so beta users are offered stable releases as well.
A channel with no manifest yet (the manifest's address answers 404) means "no update"; any other answer of the server,
or a request that fails, is a failed check.

`pnpm --filter @octoboard/desktop publish-release (--notes "text" | --notes-file path) [--dry-run]`
(`scripts/publish-release.mjs`) publishes what the last signed `pnpm release` built, for the version in `tauri.conf.json`:

1. It refuses unless `gh` is logged in, the working tree is clean and the universal build in
   `target/universal-apple-darwin/release/bundle` is this version's, signed and notarized: the `.app` says it is
   `<version>`, the `.dmg` is named for it and passes `stapler validate`, and the update archive and its signature are
   no older than the `.app` (`pnpm release` deletes the earlier ones first, since their names carry no version). A release
   `v<version>` that already exists with exactly this build's assets, the same archive size and the same signature is an
   earlier run that failed part-way, and this run resumes from step 3; any other (other assets, a rebuild of the same
   version, a draft) is refused, since a published version is never replaced: raise the version instead.
2. It creates the release `v<version>` on the current commit (which must be pushed), marked prerelease when the version
   has a suffix, with the `.dmg`, and the update archive and its signature named
   `Octoboard_<version>_universal.app.tar.gz` and `.sig`.
3. It creates the `updater` release if it is missing (as a prerelease, so it is never the repository's latest), then
   uploads `beta.json`, and `stable.json` when the version has no suffix, to it with `--clobber`. A manifest holds the
   version, the notes, the date and, under both `darwin-aarch64` and `darwin-x86_64`, the one universal archive's address
   and signature. A manifest that already announces a newer version (semver order, a prerelease before its release) is
   left alone and reported, so a late release of an older version never moves a channel backwards. A manifest that is not
   valid is refused, with nothing created yet. Immutable releases must stay off for the repository: the `updater`
   release is rewritten by every release (the script refuses when it is immutable). The script reads a release's
   `isImmutable` field, which needs a `gh` recent enough to report it (2.86 does).

`--dry-run` prints the plan and the manifest without calling GitHub. The pure parts (which manifests a
version goes to, names, addresses, the manifest) are `scripts/release-manifest.mjs`, tested by `pnpm test`.

## External interfaces

The shell defines thirteen Tauri IPC commands, permitted through `src-tauri/capabilities/default.json` and
`src-tauri/permissions/*.toml`:

- `frontend_exit_heartbeat` — marks the webview as the one handling the exit flow, and is re-invoked on every quit gesture.
- `confirm_quit` — marks a pending quit as user-confirmed and exits, or, with `restart`, relaunches the application instead
  (how an installed update takes effect), after making sure the daemon has exited (SIGTERM after two seconds, then a kill after
  five more), off the main thread, since the new instance's daemon cannot start while the old one holds its instance lock.
- `set_menu_labels` (`menu.rs`) — takes the menu's labels and the Open settings accelerator the user has chosen from the UI and rebuilds the native menu bar.
- `set_tray_menu` (`tray.rs`) — takes the menu bar icon's menu, attention state, selected-session quota indicator and console-account quota indicators for submenu openers from the UI.
- `set_run_in_background` (`background.rs`) — the UI's choice of whether closing the window keeps the app running.
- `bring_to_front` (`background.rs`) — shows, restores and activates the window.
- `macos_permissions_status`, `macos_request_permission`, `macos_open_privacy_settings` (`permissions.rs`) — read, request
  and open the pane of the Screen Recording and Accessibility grants.
- `save_configuration_file`, `open_configuration_file` (`configuration_file.rs`) — move the configuration document
  through the system's save and open dialogs.
- `check_for_update`, `install_update` (`updater.rs`) — look for a newer version on a channel (`stable` or `beta`), answering
  its version and notes, and download and install what the check found, reporting progress through a channel. They refuse to
  overlap, and once an update is installed nothing more is offered until the restart.

Events the shell emits to the webview: `settings-requested` (the menu's Settings… item), `about-requested` (the menu's
About item, which the UI answers by opening Settings on its About section; there is no native About panel),
`tray-session-chosen` (a session line of the menu bar icon's menu, carrying the session id), `tray-menu-opened` (that
menu begins to open), `exit-requested` (a quit gesture needing the UI's exit flow) and `daemon-exited` (the sidecar
ended). `packages/ui/src/platform/tauri.ts` maps commands and events onto the platform adapter's capabilities.
`capabilities/default.json` also allowlists the notification, clipboard read and write text, shell open, Dock badge,
window theme, window show and window drag/zoom commands the UI uses.

## Layout

| Path | Role |
|---|---|
| `src-tauri/build.rs` | Exposes the app name and the repository address from the repo-root `config/app.json` as `OCTOBOARD_APP_NAME` and `OCTOBOARD_REPOSITORY_URL` (where the update manifests are), and fails the build when `productName` in `tauri.conf.json` or the workspace `repository` or `license` no longer matches that file |
| `src-tauri/src/lib.rs` | `run()`: builds the Tauri app, wires the menu, exit flow, close request and reopen event, and opens the main window (overlay titlebar, translucent material, hidden until the UI reveals it) |
| `src-tauri/src/exit.rs` | The exit-confirmation flow: `ExitState`, the two exit-flow commands, `request_quit` and `request_quit_from_window`, and the `applicationShouldTerminate:` override for Dock Quit and system shutdown |
| `src-tauri/src/background.rs` | Running in the background: `set_run_in_background`, the close button hiding the window when chosen, and `bring_to_front` |
| `src-tauri/src/tray.rs` | The menu bar icon: its menu rebuilt from `set_tray_menu`, its template image and orange attention-dot overlay, selected-session quota label and icon updates, and the `tray-session-chosen` and `tray-menu-opened` events |
| `src-tauri/src/tray_icon.rs` | Composes the menu bar icon's template image and attention-dot layer with an optional two-layer quota capsule (a main bar and, when given, a thinner weekly bar) |
| `src-tauri/src/tray_lines.rs` | The menu bar icon's custom menu rows (session headings, account headers, notes, quota windows and the two-layer 5-hour/weekly `meter` row), plus `dress_quota_openers`, which attaches quota bars to native submenu titles while preserving AppKit's submenu handling |
| `src-tauri/src/menu.rs` | The native macOS menu bar built from the UI's labels, including the About item (a plain item that emits `about-requested`, not the native panel) and the Settings… item, whose accelerator the UI sets at runtime |
| `src-tauri/src/sidecar.rs` | Spawns `octoboardd`, parses its startup port line, reports how it terminated, keeps `DaemonState` and the sidecar's child, and `stop_daemon`, which a restart waits on |
| `src-tauri/src/permissions.rs` | The macOS privacy permission commands and `capture_launch_state()` |
| `src-tauri/src/configuration_file.rs` | The two configuration-file commands |
| `src-tauri/src/updater.rs` | The two update commands, the channel-to-manifest mapping and the state that keeps a check and an install from overlapping |
| `src-tauri/src/window_state.rs` | Remembers the window's frame and maximized state across launches and validates it against the connected displays |
| `src-tauri/src/page_isolation.rs` | macOS only: the webview configuration that blocks every `http(s)` request the page would make |
| `src-tauri/capabilities/default.json`, `src-tauri/permissions/` | The IPC allowlist and the app-defined permission files it names |
| `src-tauri/tauri.conf.json` | Where the window's UI comes from, the `octoboardd` `externalBin`, the bundle targets, and the update archive and the public key that verifies it |
| `src-tauri/Info.plist` | Merged into the bundle's `Info.plist`; declares the UI languages as `CFBundleLocalizations` |
| `src-tauri/icons/AppIcon.icon/` | The macOS 26 layered app icon (an Icon Composer package: `icon.json` and `Assets/octopus.png`), listed in `bundle.icon` of `tauri.conf.json`; the bundler compiles it with Xcode 26's `actool` into the `.app`'s `Assets.car`. `Assets/octopus.png` is derived by hand from `assets/branding/logo.png` (trimmed to its alpha bounds, centered on a 1024 px canvas); no script regenerates it. `icons/icon.icns` stays as the fallback for earlier macOS |
| `src-tauri/icons/tray-template.png` | The menu bar icon's monochrome template image, derived by hand from the app icon; no script regenerates it |
| `scripts/build-daemon.mjs`, `scripts/build-app.mjs`, `scripts/install-app.mjs`, `scripts/release.mjs`, `scripts/publish-release.mjs` | The build, local-verification, local-install, release and release-publishing scripts above |
| `scripts/release-manifest.mjs` | What `publish-release.mjs` decides without GitHub: manifests per version, asset names and addresses, the manifest itself (tested by `scripts/release-manifest.test.mjs`) |
| `scripts/bundle.mjs` | Shared by the build scripts: `tauriBuild` (runs `tauri build` in this directory with `scripts/actool/` first on the `PATH`), where its output lands, `universalBundleDir` (where a release's output lands), the lookup for the bundle it produced, `appVersion` (the version an `.app` says it is), `removeUpdateArchives` (clears earlier builds' update archives), `assertLayeredIcon` (checks the `.app` for the compiled layered icon) and `withoutUpdaterArtifacts` (the environment of a build that makes no update archive) |
| `scripts/actool/actool` | A shim for Xcode's `actool` that runs it with stdin on `/dev/null`, working around the Node-hosted Tauri CLI starting it with stdin closed; temporary, to be removed once the upstream fix ships |
