# Octoboard desktop application

The Tauri 2 shell of the macOS desktop application: the native window and menu bar, the `octoboardd` sidecar, the exit
flow, and keeping the application running in the background behind a menu bar icon once its window is closed, when the
user has chosen that. It contains no UI of its own: the window loads the UI in
[`../../packages/ui/`](../../packages/ui/README.md), bundled into the application at build time (not served by the
daemon), which provides the console/project/session menu, the `xterm.js` terminal, console and project management, the
report panel, the project file browser and the exit-flow screens.

The UI talks to `octoboardd` (see [`../daemon/`](../daemon/README.md)) only over the WebSocket/HTTP protocol in
[`../daemon/PROTOCOL.md`](../daemon/PROTOCOL.md) — no Tauri IPC command carries daemon traffic, and the shell never asks
the daemon anything. IPC carries only bare signals, display text or opaque ids that the UI words from what it already
shows (the Dock badge's count, the menu bar icon's menu), the macOS privacy grants, which are the system's to answer,
and the text of a file the user saves or opens through the system's own dialog. This is an architectural rule, not an
implementation detail: the already-decided remote-host feature depends on the UI never distinguishing a local daemon
from a remote one, which only holds if the daemon is reachable exclusively through that one protocol.

## Development

This is the `@octoboard/desktop` package of the repository's pnpm workspace. Its dependencies install from the
workspace root with `pnpm install` (there is no per-package install), and its scripts run from this directory as
`pnpm <script>`. It has no `typecheck` or `build` script, so the root's `pnpm typecheck` and `pnpm build` cover
`packages/ui` and `apps/web`; the Rust side is checked with `cargo`, which must be reachable on `PATH`.

`src-tauri/` is a member of the repository's root Cargo workspace, alongside `apps/daemon/`: the lockfile and the
build directory are the root's `Cargo.lock` and `target/`, and `cargo build` / `cargo test` from the root cover both
crates (`-p octoboard` for this one alone).

`src-tauri/tauri.conf.json` declares `octoboardd` as an `externalBin`, which Tauri resolves at **compile** time, not
at launch. So `pnpm build:daemon` (builds `apps/daemon/` in release and copies the binary into
`src-tauri/binaries/octoboardd-<target-triple>`, see `scripts/build-daemon.mjs`) must have been run at least once
before `pnpm tauri dev`, a `cargo build` in `src-tauri/` or a workspace-wide `cargo build`/`cargo test` from the root
will succeed — none of them runs it for you, and skipping it fails at compile time with no obvious cause. `pnpm build:tauri` (what `tauri build`
uses) runs it automatically; rerun `build:daemon` by hand whenever `apps/daemon/` changes during development.

`pnpm tauri dev` starts the `packages/ui` dev server itself (`beforeDevCommand` in `src-tauri/tauri.conf.json`, on
port 5174, which `devUrl` and the debug-build window URL in `src-tauri/src/lib.rs` both name) and opens the window on
it. `pnpm build:tauri` builds the daemon sidecar and then `packages/ui`, whose `dist/` is the `frontendDist` the
application bundles. Running the UI against a daemon started by hand, without the Tauri shell, is covered in
[`../../packages/ui/README.md`](../../packages/ui/README.md).

### Local installation

`pnpm build:install` (`scripts/install-app.mjs`) builds, signs and installs the macOS `.app` using a certificate
already in the keychain. The command is available from both the repository root and `apps/desktop/`. Its local
configuration is the gitignored repo-root `.env.install.local`, copied from
[`.env.install.example`](../../.env.install.example): `OCTOBOARD_INSTALL_DIR` names an absolute installation
directory, and `OCTOBOARD_INSTALL_SIGNING_IDENTITY` names a valid keychain signing certificate. Environment variables
take precedence over file values. The command takes no arguments.

This path builds only the `.app`, removes inherited `APPLE_*` variables and supplies only the configured signing
identity. It never reads `.env.secret` or submits notarization. It verifies the built signature and a staged copy
before replacing the installed application, keeping the previous bundle available for rollback if replacement fails.
It refuses to replace a running Octoboard; quit it before installation, which ends its active sessions.

### Release builds

`pnpm release` (`scripts/release.mjs`) runs `tauri build` — whose own `beforeBuildCommand` builds the daemon
sidecar and the frontend first, then, per `bundle.targets` in `tauri.conf.json`, produces both the `.app` and a
`.dmg` around it — and then checks what the build actually produced (`codesign`/`spctl`/`xcrun stapler`) rather than
assuming signing and notarization happened just because the right environment variables were set. In notarized mode
that step is not read-only: the `.dmg` also gets submitted for a second notarization and stapled (`tauri build` only
notarizes the `.app` inside it, so the disk image still trips Gatekeeper on a downloaded copy without this), a
network round trip of minutes that `tauri build` itself does not do.

The bundle is arm64-only: `scripts/build-daemon.mjs` builds the daemon for the host machine's own target triple, and
`scripts/release.mjs` passes no `--target` to `tauri build` either, so `pnpm release` only ever produces a bundle
for the architecture it runs on. There is no universal-binary build.

Signing and notarization are both driven by environment variables the bundled Tauri CLI reads directly (`pnpm release`
loads them from the gitignored repo-root `.env.secret`, see `.env.secret.example`; a variable already in the
environment wins, and `pnpm build:app` never reads the file); this script does not forward anything to it, it only
checks ahead of time that what is set is enough to produce something Gatekeeper will actually accept, and otherwise
fails before spending the minutes a full build takes. An App Store Connect API key is the only notarization route this
project supports, so it also refuses to start while `APPLE_ID` or `APPLE_PASSWORD` is in the environment (even empty
and with no signing identity): the Tauri CLI reads those as a second credential set, and the `.app` and the `.dmg`
could be notarized under different identities.

- Nothing set: builds unsigned. This is the only mode that runs on a machine with no Apple Developer ID
  certificate installed, and is announced as such — the signing-dependent verification steps are skipped, not
  silently assumed to have passed.
- `APPLE_SIGNING_IDENTITY` set (a **"Developer ID Application"** certificate's identity — *not* "Apple Development",
  which Gatekeeper does not accept for distribution outside the App Store) plus an App Store Connect API key,
  `APPLE_API_KEY` + `APPLE_API_ISSUER` + `APPLE_API_KEY_PATH`: signed, notarized, and stapled — the actual release
  artifact.
- `APPLE_SIGNING_IDENTITY` set without a complete API key trio: refused outright. A signed-but-not-notarized `.dmg`
  still trips Gatekeeper on a downloaded copy, so this is not a state worth building in.

Obtaining that **"Developer ID Application"** certificate is a web-only task: `POST /v1/certificates` with
`DEVELOPER_ID_APPLICATION_G2` answers `403 FORBIDDEN_ERROR`, "This operation can only be performed by the Account
Holder", and an App Store Connect team key cannot satisfy that however much access it is given. In the developer
portal's form, the Sub-CA selector defaults to **Previous Sub-CA**, whose certificates all expire on 2027-02-01, so
**G2 Sub-CA** has to be picked by hand. Afterwards, importing the certificate and its private key into the login
keychain is not enough on its own — `security find-identity -v -p codesigning` keeps reporting `0 valid identities
found` until the Developer ID G2 intermediate CA (`https://www.apple.com/certificateauthority/DeveloperIDG2CA.cer`)
is imported too, since without it the chain cannot be built.

A real Gatekeeper check (`spctl -a`) needs a copy that carries the quarantine attribute a download adds — a `.dmg`
built and opened locally on the same machine has none, and passes trivially regardless of whether the build inside
it is actually signed. To test for real, move the built `.dmg` through something that quarantines it (another
machine, a browser download, `xattr -w com.apple.quarantine ...`) before running `spctl` on what comes out of it.

To build the app only to check a change in it, run `pnpm build:app` (`scripts/build-app.mjs`; extra arguments go to
`tauri build`, e.g. `pnpm build:app --bundles app` to skip the `.dmg`). It runs the same `tauri build` with every
`APPLE_*` variable removed from the child's environment, then fails if the `.app` it produced carries a Developer ID
authority, so it cannot sign or notarize whatever the calling shell exports. The arguments known to move the output
out of `target/release/bundle` are refused up front; anything else that moves it is caught after the build, when the
bundle's age shows it is a leftover rather than one this build produced. Three accidental Developer ID-signed,
notarization-submitted builds came from a shell that happened to carry the credentials, which is why this path strips
them itself instead of asking the reader to remember.

## External interfaces

`src-tauri/` (the Rust side) defines exactly eleven of its own Tauri IPC commands: three bare signals, one carrying a
single boolean, two carrying the text of a menu, three about the macOS privacy permissions, and two that save and open
the configuration file:

- `frontend_exit_heartbeat` — marks the webview as the one handling the exit flow, so a quit is no longer let through
  unconfirmed; also doubles as a liveness ping, re-invoked on every quit gesture
  `packages/ui/src/lifecycle/useAppExit.ts` handles, which is what clears the Rust side's force-quit debounce
  (`FORCE_QUIT_WINDOW` in `src-tauri/src/exit.rs`) for a webview that is actually still answering.
- `confirm_quit` — marks a pending quit as user-confirmed and asks Tauri to actually exit.
- `set_menu_labels` (`src-tauri/src/menu.rs`, permitted by `permissions/menu-labels.toml`) — takes the menu's labels
  from the UI (keyed by its `menu.*` message keys, in the language the UI renders) and rebuilds the menu bar with
  them; a label it is not given keeps its English default, which is also what the menu built at startup shows. It
  carries only that text.
- `set_tray_menu` (`src-tauri/src/tray.rs`, permitted by `permissions/tray.toml`) — takes the menu bar icon's menu from
  the UI: sections of lines under headings, each line a label with, for one that can be chosen, the opaque id of the
  session it stands for, plus the Open and Quit labels; all already worded in the UI's language. Lines and headings
  also name a picture (an agent's mark, a status glyph) by key in an `icons` table of base64 PNGs, which the UI draws
  (`packages/ui/src/lifecycle/statusItemIcons.ts`) so the shell needs nothing per agent. While any menu of the
  app is open it only holds the latest one and applies it once the menu closes, because replacing an open menu closes it.
- `set_run_in_background` (`src-tauri/src/background.rs`, permitted by `permissions/run-in-background.toml`) — one
  boolean, the UI's Settings choice of whether the close button keeps the application running in the background; the UI
  sends it at startup and on every change, and until it does the shell takes it as off.
- `bring_to_front` (`src-tauri/src/background.rs`, permitted by `permissions/tray.toml`) — a bare signal that shows the
  window again if it was hidden by the close button, restores it if minimized, and activates the app.
- `macos_permissions_status`, `macos_request_permission` and `macos_open_privacy_settings`
  (`src-tauri/src/permissions.rs`, permitted by `permissions/macos-permissions.toml`) — the Settings dialog's
  Permissions section, which exists because a privacy request raised by an agent's child process in an embedded
  terminal is attributed by macOS to Octoboard itself, mid-task, with no explanation. They read this process's own
  Screen Recording and Accessibility grants (never prompting), raise the one system prompt for a permission the user
  asked for, and open that permission's System Settings pane. They answer the system, not the daemon, and `status`
  answers nothing off macOS. `packages/ui/src/platform/tauri.ts` exposes them as the `macosPermissions` capability.
- `save_configuration_file` and `open_configuration_file` (`src-tauri/src/configuration_file.rs`, permitted by
  `permissions/configuration-file.toml`) — the configuration export and import, which move the document as a file the
  user names. Each raises the system's save or open dialog, filtered to JSON (through `tauri-plugin-dialog`, which the
  page is given no command of), and then writes or reads the file the user picked, so the page never holds a path it
  could write elsewhere; the file name it suggests is cut to its last component, without separators or control
  characters. `save_configuration_file` takes the suggested file name and the text and answers the name saved under,
  `open_configuration_file` answers the picked file's name and its UTF-8 text, and both answer nothing when the dialog
  is dismissed. A failure is a `{ kind, detail? }`: `too_large` (a file over 64 MiB) and `not_text` for the UI to
  word, and `io` with the operating system's own text. The text is the configuration document the UI has already had
  from the daemon; the shell reads none of it. `packages/ui/src/platform/tauri.ts` exposes them as the
  `configurationFile` capability.

Beyond those, the shell emits two events to the webview. `settings-requested` fires when the application menu's
**Settings…** item (Cmd+,) is chosen; it carries no payload, and `packages/ui/src/platform/tauri.ts` exposes it as the
platform adapter's `appMenu` capability (whose `setLabels` is the other direction). `tray-session-chosen` fires when a
session line of the menu bar icon's menu is chosen, after the shell has brought the window back, with that session's id
as its payload; the platform adapter exposes it, with `set_tray_menu`, as the `statusItem` capability. The menu bar
icon's other items (Open, Quit) are answered in the shell: Open shows the window, and Quit takes the same exit flow
as Cmd+Q. Any click on the icon, left or right, opens the menu.

The window's close button is answered in the shell too (`src-tauri/src/background.rs`): the UI registers no close
listener. Cmd+W and the Window menu's Close Window item are the same gesture to it (the menu builds that item with
Tauri's predefined `close_window_with_text`, which carries ⌘W and arrives as the same `CloseRequested`). What the
gesture does is the user's choice (`set_run_in_background`, off by default). With the choice on it hides the window and
takes the application out of the Dock, leaving it reachable through the menu bar icon, the Dock icon, a reopen from the
Finder or Spotlight, or activation by a system notification. With it off it is an ordinary quit gesture, taking the same
exit flow as Cmd+Q: a live session gets the quit confirmation, and cancelling it leaves the window open. It differs
from Cmd+Q in one deliberate way: it goes through `request_quit_from_window`, which never arms the `FORCE_QUIT_WINDOW`
debounce, so a repeated close press (a double click) never forces the quit through the way a double Cmd+Q does; a
webview that stopped answering therefore cannot be quit through the close button alone. When the daemon
is not running (it failed to start or has exited, tracked by `DaemonState` in `src-tauri/src/sidecar.rs`) there is
nothing to keep in the background, so it quits whatever the choice, and a quit gesture needs no confirmation.

The window has no native titlebar
background or title text: it is created with an overlay titlebar and a hidden title on macOS, and the traffic lights
float over the UI's own top bar (`TitleBar` in `packages/ui`), centred in it by `TRAFFIC_LIGHT_X` /
`TRAFFIC_LIGHT_Y` in `src-tauri/src/lib.rs`. The window is transparent too, with a sidebar-material visual effect
behind the webview (Tauri's `effects`, which needs `macos-private-api`, see `Cargo.toml` and `app.macOSPrivateApi` in
`tauri.conf.json`; the private API rules out Mac App Store distribution). The effect follows the appearance the UI
pushes, and the window stays hidden until it has. The UI leaves its window chrome (the top bar and the left rail) clear
over the material and paints the content panel opaquely (`packages/ui/src/style.css`; the UI marks the page
translucent through the platform adapter's `translucentWindow`).

`src-tauri/capabilities/default.json` also allowlists the `notification` plugin's commands (used by
`packages/ui/src/lifecycle/useWaitingNotifications.ts` for the raised-hand system notification), the
`clipboard-manager` plugin's write-text command (the platform adapter's `clipboard` capability), the `shell` plugin's
open command (`shell:allow-open`, the platform adapter's `links` capability: a link in a rendered Markdown file opens in
the system's browser; the plugin also spawns the daemon sidecar, but disables `open` unless `plugins.shell.open` is set
in `tauri.conf.json`, which it is),
`core:window|set_badge_count` (the Dock badge), `core:window|set_theme` (the window's native appearance following the
in-app theme choice, used by `packages/ui/src/lifecycle/useNativeWindowTheme.ts`; the permission identifier in
`capabilities/default.json` is `core:window:allow-set-theme`) and `core:window|show` (reveals the window the shell
creates hidden, see `open_main_window` in `src-tauri/src/lib.rs`, called once from `packages/ui/src/main.tsx` after
it has pushed the theme; permission identifier `core:window:allow-show`). All of these are still within the
architectural rule above: they carry no daemon traffic or session state — only a count, a text, a theme string the
frontend has already derived or chosen, or no theme at all meaning follow the OS, or a bare reveal with no payload
at all, or the bare start-of-drag and zoom of the top bar's drag region (`core:window:allow-start-dragging` and
`core:window:allow-internal-toggle-maximize`), or the bare raise that brings the window to the front before the exit
confirmation asks or when the daemon's exit needs to be seen (`bring_to_front`, called through
`nativeWindow.bringToFront` by `packages/ui/src/lifecycle/useAppExit.ts`).

The window remembers its size, position and maximized state across launches (`src-tauri/src/window_state.rs`; the
frontend never calls it, so it needs no capability). What is stored is the window's *normal* frame —
its outer top-left and inner size while it is neither maximized, fullscreen nor minimized — plus whether it is
maximized, all in logical points in the desktop's global coordinate space, never physical pixels: a pixel means
something different on each display of a mixed-scale setup, a point does not. The frame is followed from the window's
move and resize events, skipping them while it is hidden (before the UI reveals it, only the shell's own placement
moves it), minimized, maximized or in native fullscreen, and discarding frames that a zoom reports on its way to
maximized (so none of those pollutes the normal frame, and fullscreen and minimized are never restored), and written
to `window-state.json` in the application's config directory (`~/Library/Application Support/dev.octoboard.app/`, not
under `~/.octoboard`: it belongs to the shell, which the daemon's data directory does not) on `RunEvent::Exit`, which
every quit path reaches.

On launch the saved state is validated against the displays connected at that moment, each display's work area
converted to points with its own scale factor, *before* the window is created, and the result is handed to the window
builder (position, inner size), so the window — still created hidden and revealed by the UI — is never seen moving into
place. A saved maximized state is applied right after the window is built rather than through the builder: AppKit
first puts a new window on the main display and the builder's position only lands afterwards, and maximizing behind
that move is what makes the window zoom on its own display and un-zoom back to the saved frame. The decision:

- no saved state, or an unreadable or malformed one: 1200×760, placed (centred) by the OS — the first-launch behaviour;
- the saved frame's top 40 points (the UI's top bar, where the window is grabbed) overlap some connected display by at
  least 200×20 points: restored, held inside the bounding box of all the connected displays' work areas — size first
  (never below the 1148×600 minimum), then position — so a frame spanning displays that are all still connected comes
  back unchanged, and one that reached onto a display since unplugged is pulled back onto the ones left;
- otherwise (its display is gone, the bar is off every display, or the frame held inside the box no longer has its bar
  on a display): 1200×760, centred on the main display's work area.

A saved maximized state is kept in both of the last two cases.

Everything else `src-tauri/` does is internal: it starts `octoboardd` as a sidecar process and bakes the port it
printed into the window's URL (`?port=`) before the window is created, so the frontend can locate the daemon without
any IPC call for it. The system's preferred languages travel the same way (`&languages=`, from
`NSLocale.preferredLanguages` at launch), because WKWebView's own `navigator.languages` holds only the first one.

## Layout

| Path | Role |
|---|---|
| `src-tauri/build.rs` | Exposes the app name from the repo-root `config/app.json` as `OCTOBOARD_APP_NAME` (read by `menu.rs`), and fails the build when `productName` in `tauri.conf.json` or the workspace `repository` in the root `Cargo.toml` no longer matches that file |
| `src-tauri/src/lib.rs` | `run()`: builds the Tauri app (taking the launch-time permission snapshot first), wires the menu/exit-flow entry points to `exit`/`menu`/`tray`, the window's close request and the reopen event to `background`, opens the main window; window-creation helpers; `MAIN_WINDOW_LABEL` |
| `src-tauri/src/exit.rs` | The exit-confirmation flow: `ExitState`, the two exit-flow IPC commands above, `request_quit` (what a quit menu item does) and `request_quit_from_window` (what the window's close does when it quits rather than hides: the same, except that it skips the `FORCE_QUIT_WINDOW` debounce), the decision the quit gestures share, and the `applicationShouldTerminate:` override onto AppKit's own delegate — `unsafe`, for catching the Dock icon's own Quit (and a system-initiated logout/restart/shutdown, which arrives the same way) |
| `src-tauri/src/page_isolation.rs` | macOS only: the window's `WKWebViewConfiguration` carrying a `WKContentRuleList` that blocks every `http(s)` request the webview would make, so a report page that got past the UI's sanitizer still cannot load from the web or preconnect / `dns-prefetch` (WebRTC is backed up by the page's nonce-only `script-src` instead); built by `lib.rs` before the window is created |
| `src-tauri/src/sidecar.rs` | Spawns `octoboardd`, parses its startup port line, reports how it terminated, and keeps `DaemonState` (whether the daemon is gone), which `exit.rs` and `background.rs` consult |
| `src-tauri/src/background.rs` | Running in the background: `set_run_in_background` (the UI's choice), the window's close button hiding it and leaving the Dock when the choice is on (an ordinary quit when it is off, an immediate one when the daemon is gone), `show_main_window` and the `bring_to_front` command bring it back, and the Dock badge, the app's activation and `hidden_by_close` state that go with it |
| `src-tauri/src/tray.rs` | The menu bar icon: its menu (rebuilt from the UI's `set_tray_menu`, held back while a menu is open (`TrayState`, following AppKit's menu-tracking notifications on macOS); Open, Quit and the session lines, with their pictures), and the `tray-session-chosen` event |
| `src-tauri/src/menu.rs` | Builds the native macOS menu bar from the labels the UI sends (`set_menu_labels`), including the Settings… item that `lib.rs` turns into the `settings-requested` event |
| `src-tauri/src/permissions.rs` | The macOS privacy permissions (Screen Recording, Accessibility): the three IPC commands above over hand-declared CoreGraphics / ApplicationServices calls and an `NSWorkspace` deep link, and `capture_launch_state()`, which records the Screen Recording grant the process started with, the only one it can use, so the UI can tell a grant made since from one it can act on |
| `src-tauri/src/configuration_file.rs` | The two configuration-file IPC commands above: the system's save and open dialogs, and the write or read of the file the user picked |
| `src-tauri/src/window_state.rs` | Remembers the window's frame and maximized state: decides the initial frame from the saved one and the connected displays, follows it from window events, saves it on exit |
| `src-tauri/capabilities/default.json` | Allowlists the IPC commands above (through `permissions/*.toml`) plus the notification, clipboard write-text, configuration-file, shell open, Dock-badge, window-theme, window-reveal and window-drag/zoom commands |
| `src-tauri/permissions/` | The app-defined permissions the capability names: `exit-lifecycle.toml`, `macos-permissions.toml`, `menu-labels.toml`, `run-in-background.toml` (`set_run_in_background`), `configuration-file.toml` (`save_configuration_file` and `open_configuration_file`), and `tray.toml` (`set_tray_menu` and `bring_to_front`) |
| `src-tauri/icons/tray-template.png` | The menu bar icon's monochrome template image, derived by hand from the app icon's silhouette with the eyes cut out; no script regenerates it |
| `src-tauri/tauri.conf.json` | Where the window's UI comes from (`frontendDist` is `packages/ui/dist`; `devUrl` and `beforeDevCommand` are that package's dev server), the `octoboardd` `externalBin`, the shell plugin's `open` flag, and the bundle targets; its `productName` mirrors `config/app.json`'s `name`, which `build.rs` checks |
| `src-tauri/Info.plist` | Merged by Tauri into the bundle's generated `Info.plist`; declares the 17 UI languages as `CFBundleLocalizations`, which is what makes VoiceOver speak the controls' roles in the system's language instead of English |
| `scripts/build-daemon.mjs` | Builds `octoboardd` in release mode and copies it into `src-tauri/binaries/` under the target-triple name Tauri's `externalBin` requires |
| `scripts/build-app.mjs` | Builds the app for local verification with every `APPLE_*` variable stripped, and fails if the result carries a Developer ID authority; see "Release builds" above |
| `scripts/install-app.mjs` | `pnpm build:install`: signs the local app and installs it using the repo-root `.env.install.local`; see "Local installation" above |
| `scripts/bundle.mjs` | Where `tauri build` puts its output, and the lookup for the bundle file a build produced; shared by the build scripts |
| `scripts/release.mjs` | Builds the release `.app`/`.dmg` and verifies the result; see "Release builds" above |
