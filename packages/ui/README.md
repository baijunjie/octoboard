# Octoboard UI

`@octoboard/ui`: a HeroUI 3 + React 19 + Tailwind 4 + Vite application, the UI of the desktop application. The macOS
shell in [`../../apps/desktop/`](../../apps/desktop/README.md) loads this package's `dist/` (bundled into the
application, not served by the daemon), and the same build also runs in a plain browser. It has the sidebar tree, the
terminal pane, the dialogs, the hub's report panel, toasts and the failure screens.

It talks to `octoboardd` (see [`../../daemon/`](../../daemon/README.md)) only over the WebSocket protocol in
[`../../daemon/PROTOCOL.md`](../../daemon/PROTOCOL.md), and reaches anything native (quit flow, system notifications,
Dock badge, the native window's own theme) only through the platform adapter, which is chosen once at startup.

## Development

This is a package of the repository's pnpm workspace: install from the workspace root with `pnpm install`, then run
the scripts from this directory.

- `pnpm dev` — the Vite dev server on port 5174, which is also what `pnpm tauri dev` in `apps/desktop/` starts and
  opens its window on.
- `pnpm typecheck` — `tsc --noEmit`.
- `pnpm build` — typechecks, then writes `dist/` with relative asset paths, so a static server or a shell can serve it
  from any path. Opening `dist/index.html` over `file://` does not work: Chrome refuses module scripts from a `null`
  origin.

### Pointing it at a daemon

The daemon binds an OS-assigned port. Start `octoboardd` and read the port from the first line it prints on stdout, or
from `$TMPDIR/octoboardd.port`. Then either:

- open `http://localhost:5174/?port=<port>`, or set `VITE_DAEMON_PORT`; or
- run `OCTOBOARD_DAEMON_PORT=<port> pnpm dev` and open the plain URL, which reaches the daemon through a same-origin
  `/ws` proxy (`vite.config.ts`).

With none of these the page shows a "no daemon address" screen. A daemon started by hand for development should get an
isolated `HOME` and `TMPDIR`: its instance lock (`daemon.lock`) and database live under `$HOME/.octoboard`, and the
port file under the temp directory, so the defaults would touch the real data and collide with a running instance.

## Layout

| Path | Role |
|---|---|
| `index.html` | The window's only Content Security Policy, delivered as a `<meta>` tag, and the inline bootstrap script that resolves and applies the theme class before the first paint; the file's own comment has the reasoning for both |
| `src/main.tsx` | Startup: picks the platform, locates the daemon, creates the connection, pushes the resolved theme and reveals the native window, and renders either the app or `StartupScreen` |
| `src/platform/` | The `PlatformAdapter` interface (optional `exit`, `notifications`, `badge`, `nativeWindow` capabilities; an absent one means the feature is absent) and its two implementations, `tauri.ts` and `browser.ts`; `react.tsx` exposes it to components. Only `tauri.ts` may import `@tauri-apps/*`, and only dynamically, so the browser path never loads them |
| `src/daemon.ts` | The rules for locating the daemon (`?port=`, `VITE_DAEMON_PORT`, or the page's own origin), and the `?error=` startup message |
| `src/daemon-client.ts` | WebSocket client for `GET /ws/control`: request/reply correlation, reconnect, event dispatch |
| `src/protocol.ts` | Hand-written TypeScript mirror of `daemon/src/protocol.rs` / `PROTOCOL.md` |
| `src/store.ts` | The Zustand store holding the connection and the console/project/session state derived from daemon events; `createDaemon(origin)` builds it and exposes the request function and `terminalUrl(session)`, `useDaemon` / `useDaemonStore` read it |
| `src/theme.tsx` | `ThemeProvider` / `useOctoboardTheme`: the user's light/dark/system choice and what "system" currently resolves to, wrapping HeroUI's own `useTheme` rather than tracking it separately. Exactly one instance, mounted in `main.tsx` |
| `src/layout.ts` | The `docked` breakpoint (read back out of `style.css`'s own CSS variable): `useIsNarrow()` for the one piece of narrow-mode behaviour that needs it in JS, and `drawerClass()`, the geometry the sidebar's and the report panel's narrow-mode drawers share |
| `src/lifecycle/` | `useAppExit` (the exit flow, through the platform's `exit` capability), `useWaitingNotifications` (system notification and badge when a session raises its hand) and `useNativeWindowTheme` (pushes later in-app theme changes onto the platform's `nativeWindow` capability; the startup push and the window's own reveal are done imperatively from `main.tsx` instead, so they also cover the error screen) |
| `src/focusGuard.ts` | A workaround, imported by `main.tsx` so it is registered before the first press, for react-aria throwing when a press on a `preventFocusOnPress` control takes focus out of an iframe; the file's own comment has the reasoning |
| `src/agents.ts` | The three agents' display labels, the option list built from them, and each agent's config-directory console field and placeholder |
| `src/sessionLabel.ts` | Where to tell the user a session is, and the status labels |
| `src/StartupScreen.tsx`, `src/ErrorBoundary.tsx` | The screens shown when there is no daemon connection, or the UI itself crashed |
| `src/App.tsx` | The screen: sidebar and terminal pane, session selection and resume, the toast stack and connection banner, the dialog layer, and — below the `docked` breakpoint — which of the sidebar/report-panel drawers is open and the shared Escape handling that closes them |
| `src/components/` | The sidebar tree (`Sidebar`), its action menu (`ActionMenu`), `StatusIcon` and `AgentBadge`, the trusted-folders list (`TrustedFolders`), the Light/Dark/System switch (`ThemeSwitcher`), the toast stack (`Toasts`), the connection banner (`ConnectionBanner`), the sidebar row that asks for notification permission from a user gesture (`NotificationsPrompt`), the labelled-control wrapper (`TitledControl`) and the dimming layer behind a narrow-mode drawer (`Scrim`) |
| `src/terminal/` | `TerminalController` (the xterm.js instance, a session's socket, status and focus), `TerminalPane` (its React boundary, reconnect backoff, the Resume affordance) and `xtermThemes.ts` (the light/dark xterm palettes the controller switches between) |
| `src/report/` | The hub's report panel: `ReportPanel` (paging through a console's pages, the `octoboard.submit` listener) and `pageDocument` (the sandboxed frame's `srcdoc`: the page's own CSP, the bridge, the history lock) |
| `src/dialogs/` | The dialogs (console, project, session, rename, confirmation, directory picker, workspace-trust prompt), on a shared `Dialog` frame over HeroUI's `Modal` with its `useDialogAction` and `useRefocusIfLost` hooks, plus the form controls they share (`TextInput`, `OptionSelect`); `dialogRequest.ts` is the request the sidebar's menus raise, which `App` holds and `RequestedDialog` renders. Escape and an outside click close one, and closing returns focus to where a menu took it from |
