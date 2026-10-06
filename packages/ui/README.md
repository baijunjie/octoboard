# Octoboard UI

`@octoboard/ui`: a HeroUI 3 + React 19 + Tailwind 4 + Vite application, the UI of the desktop application. The macOS
shell in [`../../apps/desktop/`](../../apps/desktop/README.md) loads this package's `dist/` (bundled into the
application, not served by the daemon), and the same build also runs in a plain browser. It has the sidebar tree, the
terminal pane, the window-wide top bar, the dialogs and the settings dialog, the hub's report panel, toasts and the
failure screens.

It talks to `octoboardd` (see [`../../daemon/`](../../daemon/README.md)) only over the WebSocket protocol in
[`../../daemon/PROTOCOL.md`](../../daemon/PROTOCOL.md), and reaches anything native (quit flow, system notifications,
Dock badge, the native window's own theme, the window chrome, the app menu) only through the platform adapter, which is
chosen once at startup.

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
| `src/platform/` | The `PlatformAdapter` interface (optional `exit`, `notifications`, `badge`, `nativeWindow`, `windowChrome` (macOS only: the window has no native titlebar, so the top bar is its titlebar; how much of its leading edge the traffic lights cover right now, none in fullscreen) and `appMenu` (the native menu's Settings… item) capabilities; an absent one means the feature is absent) and its two implementations, `tauri.ts` and `browser.ts`; `react.tsx` exposes it to components. Only `tauri.ts` may import `@tauri-apps/*`, and only dynamically, so the browser path never loads them |
| `src/daemon.ts` | The rules for locating the daemon (`?port=`, `VITE_DAEMON_PORT`, or the page's own origin), and the `?error=` startup message |
| `src/daemon-client.ts` | WebSocket client for `GET /ws/control`: request/reply correlation, reconnect, event dispatch |
| `src/protocol.ts` | Hand-written TypeScript mirror of `daemon/src/protocol.rs` / `PROTOCOL.md` |
| `src/store.ts` | The Zustand store holding the connection and the console/project/session state derived from daemon events; `createDaemon(origin)` builds it and exposes the request function, `terminalUrl(session)` and `onToast` (the daemon's errors and notices and `toastError`'s messages as a stream of toast requests, which the store itself does not keep), `useDaemon` / `useDaemonStore` read it |
| `src/theme.tsx` | `ThemeProvider` / `useOctoboardTheme`: the user's light/dark/system choice and what "system" currently resolves to, wrapping HeroUI's own `useTheme` rather than tracking it separately. Exactly one instance, mounted in `main.tsx` |
| `src/layout/breakpoint.ts` | The `docked` breakpoint (read back out of `style.css`'s own CSS variable): `useIsNarrow()`, which closes the drawers when the window widens and tells the top bar's toggles whether they drive a drawer or a docked pane |
| `src/layout/paneOverlay.tsx` | The overlay geometry the sidebar and the report panel share: `drawerClass(side, mode, …)`, in the `drawer` form (a closed-by-default overlay below the breakpoint, a row sibling above it) or the `floating` form (the user-hidden docked pane that floats in on hover, at every width), and `PeekHotZone`, the window-edge strip that floats a hidden pane in |
| `src/layout/paneWidth.ts` | `usePaneWidth(side, docked)`: the sidebar's or the report panel's docked-mode width, the persisted user choice (one entry per side) clamped at render time so the terminal's floor holds, with the report panel giving up width first down to its own floor; it re-renders its caller only when the clamp's result changes, not on every window resize. The sidebar's `width` is what anything aligned with its edge (a top-bar segment) should follow; `App.tsx` owns the two calls |
| `src/layout/panelVisibility.ts` | `useDockedPanelVisible(panel)`: whether the docked sidebar / report panel is shown (the user can hide either from the top bar), persisted in `localStorage`; below the `docked` breakpoint the drawers' open state in `usePaneToggles` applies instead |
| `src/layout/usePaneToggles.ts` | The top bar's two panel toggles: the drawers' open state and the docked panes' visibility, the hidden docked sidebar or report panel floating in on hover from its window edge or toggle (`sidebarPeek`, `reportPeek`, one mechanism per side, never both at once), the Escape handling that closes every open drawer or floating pane (`dismissOverlays`, which the report page's relayed Escape reaches too), and handing focus to the terminal when the pane holding it is hidden |
| `src/layout/persistedPreference.ts` | `createPersistedPreference(key, parse, serialize)`: a user preference kept in `localStorage` (every access guarded) and shared module-wide, the store under `panelVisibility.ts` and `paneWidth.ts` |
| `src/waiting.ts` | The sessions waiting for the user in sidebar-tree order, and the one after the current selection (the top bar's waiting count button) |
| `src/lifecycle/` | `useAppExit` (the exit flow, through the platform's `exit` capability), `useWaitingNotifications` (system notification and badge when a session raises its hand), `useNotificationPermission` (the permission as the platform reports it and how to ask for it, shared by the sidebar prompt and the settings) and `useNativeWindowTheme` (pushes later in-app theme changes onto the platform's `nativeWindow` capability; the startup push and the window's own reveal are done imperatively from `main.tsx` instead, so they also cover the error screen) |
| `src/focusGuard.ts` | A workaround, imported by `main.tsx` so it is registered before the first press, for react-aria throwing when a press on a `preventFocusOnPress` control takes focus out of an iframe; the file's own comment has the reasoning |
| `src/agents.ts` | The three agents' display labels, the option list built from them, and each agent's config-directory console field and placeholder |
| `src/sessionLabel.ts` | Where to tell the user a session is, and the status labels |
| `src/StartupScreen.tsx`, `src/ErrorBoundary.tsx` | The screens shown when there is no daemon connection, or the UI itself crashed |
| `src/App.tsx` | The screen: the top bar, sidebar and terminal pane, session selection and resume, the toast stack and connection banner and the dialog layer, with the pane toggles in `usePaneToggles` (above) and the settings dialog's state in `useSettingsDialog` (`src/settings/`) |
| `src/components/` | `TitleBar` (the bar across the whole window: sidebar toggle and New console in a left segment as wide as the docked sidebar, the selected session's breadcrumb, the waiting count, one chip for connection trouble that is empty while the daemon and the selected terminal are healthy (with the terminal's Reconnect once automatic attempts are spent), the report panel toggle and Settings; `BareTitleBar` is the empty version for the screens without a session UI), the sidebar tree (`Sidebar`), its action menu (`ActionMenu`), `StatusIcon` and `AgentBadge`, the toast stack (`Toasts`: HeroUI's own `Toast.Provider` fed by `Daemon.onToast`, always at the bottom right; toasts dismiss themselves, pausing while hovered or focused, and identical ones merge), the connection banner (`ConnectionBanner`), the sidebar row that asks for notification permission from a user gesture (`NotificationsPrompt`), the panes' drag handle (`PaneResizeHandle`, per side, running as far up as its pane's edge — the sidebar's from the window top, the report panel's from below the top chrome — rendered by `App` next to its pane), `FadeOverflow` (content that fades out at a clipped edge instead of an ellipsis, cut at its end or, for a path, at its start, with one shared `ResizeObserver`), the labelled-control wrapper (`TitledControl`) and the dimming layer behind a narrow-mode drawer (`Scrim`) |
| `src/settings/` | The settings modal (`SettingsDialog`: a section list on the left, the selected section's rows on the right, over the window with everything under it left mounted), `useSettingsDialog` (whether it is open, including from the app menu's Settings… item (ignored while another modal or an action menu is open, or before the first snapshot), and returning focus to the terminal on close), and its sections: `AppearanceSection` (Light/Dark/System), `TrustedFoldersSection`, `NotificationsSection`, on the shared `SettingRow`; a section that can unmount a focused control calls `useSectionRefocus` (`useSectionRefocus.ts`) so focus returns to the dialog |
| `src/terminal/` | `TerminalController` (the xterm.js instance, a session's socket, status and focus), `TerminalPane` (its React boundary, reconnect backoff, reporting connection trouble to the top bar, the Not running overlay with Resume) and `xtermThemes.ts` (the light/dark xterm palettes the controller switches between) |
| `src/report/` | The hub's report panel: `ReportPanel` (paging through a console's pages, the `octoboard.submit` and Escape-relay listener) and `pageDocument` (the sandboxed frame's `srcdoc`: the page's own CSP, the bridge with its Escape relay, the history lock) |
| `src/dialogs/` | The dialogs (console, project, session, rename, confirmation, directory picker, workspace-trust prompt), on a shared `Dialog` frame over HeroUI's `Modal` with its `useDialogAction` and `useRefocusIfLost` hooks, plus the form controls they share (`TextInput`, `OptionSelect`); `dialogRequest.ts` is the request the sidebar's menus and the top bar's New console raise, which `App` holds and `RequestedDialog` renders. Escape and an outside click close one, and closing returns focus to where a menu took it from |
