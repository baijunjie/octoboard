# Octoboard desktop application

A Tauri 2 + React + TypeScript desktop application: the console/project/session menu, the `xterm.js` terminal, console
and project management, manual sessions, archiving and reopening, and the exit flow.

The UI talks to `octoboardd` (see [`../daemon/`](../daemon/README.md)) only over the WebSocket/HTTP protocol in
[`../daemon/PROTOCOL.md`](../daemon/PROTOCOL.md) — no Tauri IPC command carries daemon traffic or session state. This
is an architectural rule, not an implementation detail: the already-decided remote-host feature depends on the UI
never distinguishing a local daemon from a remote one, which only holds if the daemon is reachable exclusively through
that one protocol.

## Development

`src-tauri/tauri.conf.json` declares `octoboardd` as an `externalBin`, which Tauri resolves at **compile** time, not
at launch. So `npm run build:daemon` (builds `daemon/` in release and copies the binary into
`src-tauri/binaries/octoboardd-<target-triple>`, see `scripts/build-daemon.mjs`) must have been run at least once
before `npm run tauri dev` or a bare `cargo build` in `src-tauri/` will succeed — neither of those two commands runs
it for you, and skipping it fails at compile time with no obvious cause. `npm run build:tauri` (what `tauri build`
uses) runs it automatically; rerun `build:daemon` by hand whenever `daemon/` changes during development.

Running `vite dev` directly against a daemon started by hand, bypassing the Tauri shell, needs `VITE_DAEMON_PORT` set
to that daemon's port — see `src/daemon.ts` for where to read it from.

### Release builds

`npm run release` (`scripts/release.mjs`) runs `tauri build` — whose own `beforeBuildCommand` builds the daemon
sidecar and the frontend first, then, per `bundle.targets` in `tauri.conf.json`, produces both the `.app` and a
`.dmg` around it — and then checks what the build actually produced (`codesign`/`spctl`/`xcrun stapler`) rather than
assuming signing and notarization happened just because the right environment variables were set. In notarized mode
that step is not read-only: the `.dmg` also gets submitted for a second notarization and stapled (`tauri build` only
notarizes the `.app` inside it, so the disk image still trips Gatekeeper on a downloaded copy without this), a
network round trip of minutes that `tauri build` itself does not do.

The bundle is arm64-only: `scripts/build-daemon.mjs` builds the daemon for the host machine's own target triple, and
`scripts/release.mjs` passes no `--target` to `tauri build` either, so `npm run release` only ever produces a bundle
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
  unconfirmed; also doubles as a liveness ping, re-invoked on every quit gesture `useAppExit.ts` handles, which is
  what clears the Rust side's force-quit debounce (`FORCE_QUIT_WINDOW` in `src-tauri/src/exit.rs`) for a webview that
  is actually still answering.
- `confirm_quit` — marks a pending quit as user-confirmed and asks Tauri to actually exit.

Beyond those two, `src-tauri/capabilities/default.json` also allowlists the `notification` plugin's commands (used by
`src/lifecycle/useWaitingNotifications.ts` for the raised-hand system notification) and `core:window|set_badge_count`
(the Dock badge). Both are still within the architectural rule above: they carry no daemon traffic or session state,
only a count and a text the frontend has already derived from it.

Everything else `src-tauri/` does is internal: it starts `octoboardd` as a sidecar process and bakes the port it
printed into the window's URL (`?port=`) before the window is created, so the frontend can locate the daemon without
any IPC call for it.

## Layout

| Path | Role |
|---|---|
| `src-tauri/src/lib.rs` | `run()`: builds the Tauri app, wires the menu/exit-flow entry points to `exit`/`menu`, opens the main window; window-creation helpers |
| `src-tauri/src/exit.rs` | The exit-confirmation flow: `ExitState`, the two IPC commands above, the decision all three quit gestures share, and the `applicationShouldTerminate:` override onto AppKit's own delegate — the only `unsafe` code in `app/`, for catching the Dock icon's own Quit (and a system-initiated logout/restart/shutdown, which arrives the same way) |
| `src-tauri/src/sidecar.rs` | Spawns `octoboardd`, parses its startup port line, reports how it terminated |
| `src-tauri/src/menu.rs` | Builds the native macOS menu bar |
| `src-tauri/capabilities/default.json` | Allowlists the two IPC commands above plus the notification and Dock-badge commands |
| `index.html` | The window's only Content Security Policy, delivered as a `<meta>` tag rather than through `app.security.csp` in `src-tauri/tauri.conf.json` — the file's own comment has the reasoning |
| `src/daemon.ts` | Locates the daemon's port (`?port=` query param from the Tauri shell, or `VITE_DAEMON_PORT` for `vite dev` against a hand-started daemon) |
| `src/daemon-client.ts` | WebSocket client for `GET /ws/control`: request/reply correlation, reconnect, event dispatch |
| `src/protocol.ts` | Hand-written TypeScript mirror of `daemon/src/protocol.rs` / `PROTOCOL.md` |
| `src/store.tsx` | React context holding the daemon connection and the console/project/session state derived from its events, including each console's report panel pages the queue of Claude Code trust prompts awaiting the user's answer, and the trusted folders |
| `src/App.tsx` | Top-level layout: sidebar, terminal pane, the report panel (hub sessions only), dialogs |
| `src/components/` | Menu, dialogs (console/project/session create-edit, confirm, directory picker), the sidebar's trusted-folders list (`TrustedFolders.tsx`) and small UI primitives |
| `src/components/ReportPanel.tsx` | The report panel: lists a console's pushed pages, pages back through them, and renders the current one in a sandboxed iframe with a `postMessage` bridge for form submissions |
| `src/terminal/` | `TerminalController` (owns `xterm.js`, the session's `GET /ws/term/:session` socket, connection status and focus as one unit) and the `TerminalPane` component wrapping it |
| `src/lifecycle/useAppExit.ts` | Drives the exit-confirmation flow from the frontend side, calling the two Tauri commands above |
| `src/lifecycle/useWaitingNotifications.ts` | Fires the system notification and sets the Dock badge count when a session raises its hand |
| `src/sessionLabel.ts` | Where to tell the user a session is (its project, or its console's hub), since the daemon's `Session` record itself only carries ids |
| `src/agents.ts` | Display labels for the three supported agents |
| `src/main.tsx`, `src/StartupScreen.tsx`, `src/ErrorBoundary.tsx` | Startup sequencing and the error/retry screens shown before the daemon connection is ready |
| `scripts/build-daemon.mjs` | Builds `octoboardd` in release mode and copies it into `src-tauri/binaries/` under the target-triple name Tauri's `externalBin` requires |
| `scripts/release.mjs` | Builds the release `.app`/`.dmg` and verifies the result; see "Release builds" above |
