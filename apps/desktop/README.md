# Octoboard desktop application

The Tauri 2 shell of the macOS desktop application: the native window and menu bar, the `octoboardd` sidecar, and the
exit flow. It contains no UI of its own: the window loads the UI in [`../../packages/ui/`](../../packages/ui/README.md),
bundled into the application at build time (not served by the daemon), which provides the console/project/session
menu, the `xterm.js` terminal, console and project management, the report panel and the exit-flow screens.

The UI talks to `octoboardd` (see [`../../daemon/`](../../daemon/README.md)) only over the WebSocket/HTTP protocol in
[`../../daemon/PROTOCOL.md`](../../daemon/PROTOCOL.md) — no Tauri IPC command carries daemon traffic or session state. This
is an architectural rule, not an implementation detail: the already-decided remote-host feature depends on the UI
never distinguishing a local daemon from a remote one, which only holds if the daemon is reachable exclusively through
that one protocol.

## Development

This is the `@octoboard/desktop` package of the repository's pnpm workspace. Its dependencies install from the
workspace root with `pnpm install` (there is no per-package install), and its scripts run from this directory as
`pnpm <script>`. It has no `typecheck` or `build` script, so the root's `pnpm typecheck` and `pnpm build` cover only
`packages/ui`; the Rust side is checked with `cargo` in `src-tauri/`, which must be reachable on `PATH`.

`src-tauri/tauri.conf.json` declares `octoboardd` as an `externalBin`, which Tauri resolves at **compile** time, not
at launch. So `pnpm build:daemon` (builds `daemon/` in release and copies the binary into
`src-tauri/binaries/octoboardd-<target-triple>`, see `scripts/build-daemon.mjs`) must have been run at least once
before `pnpm tauri dev` or a bare `cargo build` in `src-tauri/` will succeed — neither of those two commands runs
it for you, and skipping it fails at compile time with no obvious cause. `pnpm build:tauri` (what `tauri build`
uses) runs it automatically; rerun `build:daemon` by hand whenever `daemon/` changes during development.

`pnpm tauri dev` starts the `packages/ui` dev server itself (`beforeDevCommand` in `src-tauri/tauri.conf.json`, on
port 5174, which `devUrl` and the debug-build window URL in `src-tauri/src/lib.rs` both name) and opens the window on
it. `pnpm build:tauri` builds the daemon sidecar and then `packages/ui`, whose `dist/` is the `frontendDist` the
application bundles. Running the UI against a daemon started by hand, without the Tauri shell, is covered in
[`../../packages/ui/README.md`](../../packages/ui/README.md).

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

Signing and notarization are both driven by environment variables the bundled Tauri CLI reads directly; this script
does not forward anything to it, it only checks ahead of time that what is set is enough to produce something
Gatekeeper will actually accept, and otherwise fails before spending the minutes a full build takes:

- Nothing set: builds unsigned. This is the only mode that runs on a machine with no Apple Developer ID
  certificate installed, and is announced as such — the signing-dependent verification steps are skipped, not
  silently assumed to have passed.
- `APPLE_SIGNING_IDENTITY` set (a **"Developer ID Application"** certificate's identity — *not* "Apple Development",
  which Gatekeeper does not accept for distribution outside the App Store) plus either `APPLE_ID` + `APPLE_PASSWORD`
  + `APPLE_TEAM_ID` (the 10-character Team ID, from the Developer account's membership page) or `APPLE_API_KEY` +
  `APPLE_API_ISSUER` + `APPLE_API_KEY_PATH`: signed, notarized, and stapled — the actual release artifact.
- `APPLE_SIGNING_IDENTITY` set without a complete notarization credential trio: refused outright. A signed-but-not-
  notarized `.dmg` still trips Gatekeeper on a downloaded copy, so this is not a state worth building in.

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

## External interfaces

`src-tauri/` (the Rust side) defines exactly two of its own Tauri IPC commands, both bare exit-flow signals with no
daemon traffic or session data in them:

- `frontend_exit_heartbeat` — marks the webview as the one handling the exit flow, so a quit is no longer let through
  unconfirmed; also doubles as a liveness ping, re-invoked on every quit gesture `packages/ui/src/lifecycle/useAppExit.ts` handles, which is
  what clears the Rust side's force-quit debounce (`FORCE_QUIT_WINDOW` in `src-tauri/src/exit.rs`) for a webview that
  is actually still answering.
- `confirm_quit` — marks a pending quit as user-confirmed and asks Tauri to actually exit.

Beyond those two, the shell emits one event to the webview, `settings-requested`, when the application menu's
**Settings…** item (Cmd+,) is chosen; it carries no payload, and `packages/ui/src/platform/tauri.ts` exposes it as the
platform adapter's `appMenu` capability. The window has no native titlebar background or title text: it is created
with an overlay titlebar and a hidden title on macOS, and the traffic lights float over the UI's own top bar
(`TitleBar` in `packages/ui`), centred in it by `TRAFFIC_LIGHT_X` / `TRAFFIC_LIGHT_Y` in `src-tauri/src/lib.rs`.

`src-tauri/capabilities/default.json` also allowlists the `notification` plugin's commands (used by
`packages/ui/src/lifecycle/useWaitingNotifications.ts` for the raised-hand system notification),
`core:window|set_badge_count` (the Dock badge), `core:window|set_theme` (the window's native appearance following the
in-app theme choice, used by `packages/ui/src/lifecycle/useNativeWindowTheme.ts`; the permission identifier in
`capabilities/default.json` is `core:window:allow-set-theme`) and `core:window|show` (reveals the window the shell
creates hidden, see `open_main_window` in `src-tauri/src/lib.rs`, called once from `packages/ui/src/main.tsx` after
it has pushed the theme; permission identifier `core:window:allow-show`). All of these are still within the
architectural rule above: they carry no daemon traffic or session state — only a count, a text, a theme string the
frontend has already derived or chosen, or no theme at all meaning follow the OS, or a bare reveal with no payload
at all, or the bare start-of-drag and zoom of the top bar's drag region (`core:window:allow-start-dragging` and
`core:window:allow-internal-toggle-maximize`).

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
  (never below the 1100×600 minimum), then position — so a frame spanning displays that are all still connected comes
  back unchanged, and one that reached onto a display since unplugged is pulled back onto the ones left;
- otherwise (its display is gone, the bar is off every display, or the frame held inside the box no longer has its bar
  on a display): 1200×760, centred on the main display's work area.

A saved maximized state is kept in both of the last two cases.

Everything else `src-tauri/` does is internal: it starts `octoboardd` as a sidecar process and bakes the port it
printed into the window's URL (`?port=`) before the window is created, so the frontend can locate the daemon without
any IPC call for it.

## Layout

| Path | Role |
|---|---|
| `src-tauri/src/lib.rs` | `run()`: builds the Tauri app, wires the menu/exit-flow entry points to `exit`/`menu`, opens the main window; window-creation helpers |
| `src-tauri/src/exit.rs` | The exit-confirmation flow: `ExitState`, the two IPC commands above, the decision all three quit gestures share, and the `applicationShouldTerminate:` override onto AppKit's own delegate — the only `unsafe` code in `apps/desktop/`, for catching the Dock icon's own Quit (and a system-initiated logout/restart/shutdown, which arrives the same way) |
| `src-tauri/src/sidecar.rs` | Spawns `octoboardd`, parses its startup port line, reports how it terminated |
| `src-tauri/src/menu.rs` | Builds the native macOS menu bar, including the Settings… item that `lib.rs` turns into the `settings-requested` event |
| `src-tauri/src/window_state.rs` | Remembers the window's frame and maximized state: decides the initial frame from the saved one and the connected displays, follows it from window events, saves it on exit |
| `src-tauri/capabilities/default.json` | Allowlists the two IPC commands above plus the notification, Dock-badge, window-theme, window-reveal and window-drag/zoom commands |
| `src-tauri/tauri.conf.json` | Where the window's UI comes from (`frontendDist` is `packages/ui/dist`; `devUrl` and `beforeDevCommand` are that package's dev server), the `octoboardd` `externalBin`, and the bundle targets |
| `scripts/build-daemon.mjs` | Builds `octoboardd` in release mode and copies it into `src-tauri/binaries/` under the target-triple name Tauri's `externalBin` requires |
| `scripts/release.mjs` | Builds the release `.app`/`.dmg` and verifies the result; see "Release builds" above |
