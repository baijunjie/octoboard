# Octoboard UI

`@octoboard/ui`: a HeroUI 3 + React 19 + Tailwind 4 + Vite application, the UI of the desktop application. The macOS
shell in [`../../apps/desktop/`](../../apps/desktop/README.md) loads this package's `dist/` (bundled into the
application, not served by the daemon), and the same build also runs in a plain browser. It has the sidebar, the
terminal pane, the archive view, the window-wide top bar, the dialogs and the settings dialog, the hub's report panel, toasts and the
failure screens, all in the user's language (see `src/i18n/` below).

It talks to `octoboardd` (see [`../../apps/daemon/`](../../apps/daemon/README.md)) only over the WebSocket protocol in
[`../../apps/daemon/PROTOCOL.md`](../../apps/daemon/PROTOCOL.md), and reaches anything native (quit flow, system
notifications, Dock badge, the native window's own theme, the window chrome, the app menu) only through the platform
adapter, which is chosen once at startup.

## Development

This is a package of the repository's pnpm workspace: install from the workspace root with `pnpm install`, then run
the scripts from this directory.

- `pnpm dev` — the Vite dev server on port 5174, which is also what `pnpm tauri dev` in `apps/desktop/` starts and
  opens its window on.
- `pnpm gallery` — the same dev server on port 5175, opening the UI state gallery (below) in the browser.
- `pnpm typecheck` — `tsc --noEmit`.
- `pnpm test` — `vitest run`: the unit tests beside the code (`*.test.ts`), for the pure modules, plus jsdom tests for a hook (`*.test.tsx`).
- `pnpm build` — typechecks, runs the catalog check (`vitest run src/i18n`), then writes `dist/` with relative asset
  paths, so a static server or a shell can serve it from any path. Opening `dist/index.html` over `file://` does not
  work: Chrome refuses module scripts from a `null` origin.

### The UI state gallery

The dev server serves a dev-only gallery of states that are hard to reach against a live daemon at `/gallery.html` (no
daemon needed): `pnpm gallery` serves it on port 5175 and opens it, and under `pnpm dev` it is at
`http://localhost:5174/gallery.html`. Pick a scenario from the list; light/dark, any offered language (`ar` is the
right-to-left one) and the window width are controls above the window, and all of them are in the URL, so a link
reproduces a view. The window is an iframe, so its width decides the `docked` breakpoint (1100px) for real. The real
`App` renders over a fixture daemon (`src/gallery/fixtureDaemon.ts`). A scenario's page writes its `octoboard.*` and
`heroui-theme` entries into `localStorage` on every load; on port 5174 that origin is the dev app's own, so opening the
gallery there resets the dev app's preferences, which `pnpm gallery`'s port avoids. A scenario's page sets
`data-gallery-ready` on `<html>` once its steps have run, which is what automation waits on before capturing. Neither
gallery page is part of the bundle `pnpm build` writes, but `src/gallery/` is still type-checked by it, so a broken
fixture fails the build.

To add a scenario, add an entry to a group's file in `src/gallery/fixtures/` (or a new file, listed in `index.ts`): its
`state` is built with `snapshotState` from the helpers in `builders.ts`, so a protocol change breaks it at typecheck.
State that lives in a component rather than the store (a dialog, the archive view, which session is selected) is
reached with `steps`, which press controls by name as a user would (`ui.press(ui.t("titleBar.settings"))`). `preferences`
sets what the app persists (pane visibility and widths, the sidebar's console and focus project), `toasts` raises daemon
errors and notices, and `terminal: "refuse"` makes a session's terminal unreachable.

### Pointing it at a daemon

The daemon binds an OS-assigned port. Start `octoboardd` and read the port from the first line it prints on stdout, or
from `$TMPDIR/octoboardd.port`. Then either:

- open `http://localhost:5174/?port=<port>`, or set `VITE_DAEMON_PORT`; or
- run `OCTOBOARD_DAEMON_PORT=<port> pnpm dev` and open the plain URL, which reaches the daemon through a same-origin
  `/ws` proxy (`vite.config.ts`).

Both variables can also be set in the committed repo-root `.env` (commented out there) or a gitignored `.env.local`;
a variable in the shell wins over them. The dev server reads those files and a `vite build` never does. Prefer them to
a `packages/ui/.env`: that directory is vite's own `envDir`, so a `vite build` reads it too and a `VITE_DAEMON_PORT`
there goes into a release bundle, and vite expands `$VAR` in it where the root files are read literally.

With none of these the page shows a "no daemon address" screen. A daemon started by hand for development should get an
isolated `HOME` and `TMPDIR`: its instance lock (`daemon.lock`) and database live under `$HOME/.octoboard`, and the
port file under the temp directory, so the defaults would touch the real data and collide with a running instance.

## Layout

| Path | Role |
|---|---|
| `index.html` | The window's only Content Security Policy, delivered as a `<meta>` tag, and the inline bootstrap script that resolves and applies the theme class before the first paint; the file's own comment has the reasoning for both |
| `gallery.html`, `gallery-frame.html`, `src/gallery/` | The dev-only UI state gallery, not part of a build (see "The UI state gallery" above): the shell page (`gallery.html`, `shell.tsx`), the page one scenario renders in (`gallery-frame.html`, `frame.tsx`, which mirrors `main.tsx` over a fixture daemon and gets `index.html`'s Content Security Policy through `vite.config.ts`), the scenario type (`scenario.ts`), the steps' helpers (`interact.ts`), the fake terminal sockets (`fakeTerminal.ts`), the fixture daemon (`fixtureDaemon.ts`), the storage a scenario starts from (`prepare.ts`) and the scenarios (`fixtures/`, with `fixtures/builders.ts` building their state) |
| `src/preferenceKeys.ts` | The `localStorage` key of every persisted preference, apart from the modules that read them so the gallery's storage reset can name a key without loading them |
| `src/main.tsx` | Startup: picks the platform, locates the daemon, creates the connection, pushes the resolved theme and reveals the native window, and renders either the app or `StartupScreen` |
| `src/platform/` | The `PlatformAdapter` interface (optional `exit`, `notifications`, `badge`, `nativeWindow`, `windowChrome` (macOS only: the window has no native titlebar, so the top bar is its titlebar; how much of its left edge the traffic lights cover right now, none in fullscreen) and `appMenu` (the native menu's Settings… item, and the labels the UI hands the menu) capabilities; an absent one means the feature is absent) and its two implementations, `tauri.ts` and `browser.ts`; `react.tsx` exposes it to components. Only `tauri.ts` may import `@tauri-apps/*`, and only dynamically, so the browser path never loads them |
| `src/daemon.ts` | The rules for locating the daemon (`?port=`, `VITE_DAEMON_PORT`, or the page's own origin), and the `?error=` startup message |
| `src/daemon-client.ts` | WebSocket client for `GET /ws/control`: request/reply correlation, reconnect, event dispatch |
| `src/protocol.ts` | Hand-written TypeScript mirror of `apps/daemon/src/protocol.rs` / `PROTOCOL.md` |
| `src/store.ts` | The Zustand store holding the connection and the console/project/session state derived from daemon events; `createDaemon(origin)` builds it (over `createStateStore`, which the gallery's fixture daemon uses too) and exposes the request function, `terminalUrl(session)` and `onToast` (the daemon's errors and notices and `toastError`'s messages as a stream of toast requests, which the store itself does not keep), `useDaemon` / `useDaemonStore` read it |
| `src/daemonMessage.ts` | Words what the daemon reports (an `error` or a `session_notice`) from its `code` and `params` in a given language, through the catalog's `daemon.<code>` messages: a `console` / `project` / `session` param is shown as that record's name, a `reason_code` param is worded through `daemon.trust_reason.<code>` in place of `reason`, and a code the catalog lacks shows the daemon's own English `message`. `store.ts` applies it to toasts and to a failed request's `message` |
| `src/appConfig.ts` | The app-level facts from the repo-root `config/app.json` (`name`, `repositoryUrl`), the single source for them |
| `src/i18n/` | Internationalization: the offered languages (`languages.ts`), mapping the system's preferred languages onto them (`matchLanguage.ts`), the typed English catalog (`messages/en.ts`), the translated ones (`messages/zh-Hans.ts`, each registered in `CATALOGS`) and the lookup with its placeholders and plural forms (`catalog.ts`), the current language, the user's choice of it persisted and the desktop shell's `?languages=` hand-over of the system's preferred languages read (`language.ts`), the global `{appName}` placeholder every message can use (filled from `appConfig`), and its React side (`react.tsx`: `useT`, `Message` for text with markup in a placeholder, `LanguageProvider`, which also hands the locale to react-aria). Every user-facing string goes through it |
| `src/theme.tsx` | `ThemeProvider` / `useOctoboardTheme`: the user's light/dark/system choice and what "system" currently resolves to, wrapping HeroUI's own `useTheme` rather than tracking it separately. Exactly one instance, mounted in `main.tsx` |
| `src/layout/breakpoint.ts` | The `docked` breakpoint (read back out of `style.css`'s own CSS variable): `useIsNarrow()`, which closes the drawers when the window widens and tells the top bar's toggles whether they drive a drawer or a docked pane |
| `src/layout/paneOverlay.tsx` | The overlay geometry the sidebar and the report panel share: `drawerClass(side, mode, …)`, in the `drawer` form (a closed-by-default overlay below the breakpoint, a row sibling above it) or the `floating` form (the user-hidden docked pane that floats in on hover, at every width), and `PeekHotZone`, the window-edge strip that floats a hidden pane in |
| `src/layout/paneWidth.ts` | `usePaneWidth(side, docked)`: the sidebar's or the report panel's docked-mode width, the persisted user choice (one entry per side) clamped at render time so the terminal's floor holds, with the report panel giving up width first down to its own floor; it re-renders its caller only when the clamp's result changes, not on every window resize. The sidebar's `width` is what anything aligned with its edge (a top-bar segment) should follow; `App.tsx` owns the two calls |
| `src/layout/panelVisibility.ts` | `useDockedPanelVisible(panel)`: whether the docked sidebar / report panel is shown (the user can hide either from the top bar), persisted in `localStorage`; below the `docked` breakpoint the drawers' open state in `usePaneToggles` applies instead |
| `src/layout/usePaneToggles.ts` | The top bar's two panel toggles: the drawers' open state and the docked panes' visibility, the hidden docked sidebar or report panel floating in on hover from its window edge or toggle (`sidebarPeek`, `reportPeek`, one mechanism per side, never both at once), the Escape handling that closes every open drawer or floating pane (`dismissOverlays`, which the report page's relayed Escape reaches too), and handing focus to the terminal when the pane holding it is hidden |
| `src/waiting.ts` | The sessions waiting for the user in sidebar order (`sidebar/order.ts`), and the one after the current selection (the top bar's waiting count button) |
| `src/lifecycle/` | `useAppExit` (the exit flow, through the platform's `exit` capability), `useWaitingNotifications` (system notification and badge when a session raises its hand), `useNotificationPermission` (the permission as the platform reports it and how to ask for it, shared by the top bar's bell and the settings) and `useNativeMenuLabels` (pushes the native menu's labels in the current language through `appMenu`, at startup and on every language change; mounted from `main.tsx` outside the error boundary) and `useNativeWindowTheme` (pushes later in-app theme changes onto the platform's `nativeWindow` capability; the startup push and the window's own reveal are done imperatively from `main.tsx` instead, so they also cover the error screen) |
| `src/focusGuard.ts` | A workaround, imported by `main.tsx` so it is registered before the first press, for react-aria throwing when a press on a `preventFocusOnPress` control takes focus out of an iframe; the file's own comment has the reasoning |
| `src/agents.ts` | The three agents' product names (proper nouns, not in the catalog), the option list built from them, and each agent's config-directory console field and placeholder |
| `src/sessionLabel.ts` | Where to tell the user a session is, the status labels and a session row's accessible name (`sessionAriaLabel`); all take a `Translate` |
| `src/projectFiltering.ts` | The sidebar project filter's pure rules: the tags in use across projects, the picked tags narrowed to them, whether a project matches the keyword and tags, and adding or dropping a tag in the stored selection |
| `src/relativeTime.ts` | `formatRelativeTime`: "5 minutes ago" for a past instant, in a given language |
| `src/avatarImage.ts` | `imageToAvatar`: an image file as a 128 x 128 centre-cropped `data:` URL (WebP, PNG where the browser cannot encode it), the form a console's avatar is stored in |
| `src/persistedPreference.ts` | `createPersistedPreference(key, parse, serialize)`: a user preference kept in `localStorage` (every access guarded) and shared module-wide, the store under `panelVisibility.ts`, `paneWidth.ts` and the language choice in `i18n/language.ts` |
| `src/StartupScreen.tsx`, `src/ErrorBoundary.tsx` | The screens (and the two messages of the startup ones) shown when there is no daemon connection, or the UI itself crashed |
| `src/App.tsx` | The screen: the top bar, sidebar and terminal pane, session selection and resume, which archive (if any) is open over the terminal, the toast stack and connection banner and the dialog layer, with the pane toggles in `usePaneToggles` (above) and the settings dialog's state in `useSettingsDialog` (`src/settings/`) |
| `src/components/` | `TitleBar` (the bar across the whole window: sidebar toggle and New console in a left segment as wide as the docked sidebar, the selected session's breadcrumb, the waiting count, one icon for connection trouble that is empty while the daemon and the selected terminal are healthy (with the terminal's Reconnect once automatic attempts are spent), the bell that asks for notification permission from a user gesture while it is undecided, the report panel toggle and Settings; `BareTitleBar` is the empty version for the screens without a session UI), the action menu (`ActionMenu`: items with icons, submenus, separators and single-selection sections, and a custom trigger), `StatusIcon` (per-status glyphs, animated while a session works) with `ActivityMarker` (the summary marker of a project or console), `AgentIcon` (an agent's mark), `ConsoleAvatar` (a console's round avatar, its custom image or the default glyph) and `EmptyPanel` (the shape every empty list takes), the toast stack (`Toasts`: HeroUI's own `Toast.Provider` fed by `Daemon.onToast`, always at the bottom right; toasts dismiss themselves, pausing while hovered or focused, and identical ones merge), the connection banner (`ConnectionBanner`, a strip along the window's bottom edge), the panes' drag handle (`PaneResizeHandle`, per side, running as far up as its pane's edge — the sidebar's from the window top, the report panel's from below the top bar, both ending above the banner — rendered by `App` next to its pane), `FadeOverflow` (content that fades out at a clipped edge instead of an ellipsis, cut at its end or, for a path, at its start, with one shared `ResizeObserver`), the labelled-control wrapper (`TitledControl`), the focus hand-off for a control that can disappear while focused (`useFocusHandoff`) and the dimming layer behind a narrow-mode drawer (`Scrim`) |
| `src/sidebar/` | The sidebar: `Sidebar` (one console at a time, picked from its switcher, with its hub and projects; or, in a project's focus mode, `FocusView`, the sidebar given over to that one project's sessions and recent archive), `menus.tsx` (the project and session action menus and the archive submenu), `rows.tsx` (the row primitives: the hand-built tree row, its hover controls, labels, section headings), `order.ts` (the sidebar's sort rules: pinned first, then by how urgently a status asks for attention, and a console's or project's most pressing activity), `useFlip.ts` (the animation of rows moving when that order changes), `sidebarView.ts` (the current console and focus project, persisted) and `types.ts` (`SidebarHandlers`, the callbacks the sidebar raises, and `ArchiveScope`) |
| `src/archive/` | `ArchiveView`: every archived session of a project, or every archived hub of a console, listed over the terminal area (which stays mounted beneath it), loaded a page at a time as the list is scrolled, with deleting one or all |
| `src/settings/` | The settings modal (`SettingsDialog`: a section list on the left, the selected section's rows on the right, over the window with everything under it left mounted), `useSettingsDialog` (whether it is open, including from the app menu's Settings… item (ignored while another modal or an action menu is open, or before the first snapshot), and returning focus to the terminal on close), and its sections: `GeneralSection` (the `AppearanceSetting` Light/Dark/System row and the `LanguageSetting` row, one of the offered languages), `TrustedFoldersSection`, `NotificationsSection`, on the shared `SettingRow`; a section that can unmount a focused control calls `useSectionRefocus` (`useSectionRefocus.ts`) so focus returns to the dialog |
| `src/terminal/` | `TerminalController` (the xterm.js instance, a session's socket, status and focus), `TerminalPane` (its React boundary, reconnect backoff, reporting connection trouble to the top bar, the Not running overlay with Resume) and `xtermThemes.ts` (the light/dark xterm palettes the controller switches between) |
| `src/report/` | The hub's report panel: `ReportPanel` (paging through a console's pages, the form-submission and Escape-relay listener) and `pageDocument` (the sandboxed frame's `srcdoc`: the page's CSP with a per-frame script nonce, DOMPurify, and the page itself as a string literal) with `pageBridge.js` (runs in the frame: sanitizes and renders the page, relays native form submissions and Escape / F6, disables the controls of a history page). A page is a static document with native forms; its own scripts are removed and never run |
| `src/dialogs/` | The dialogs (console, project, session, rename, confirmation, directory picker, workspace-trust prompt), on a shared `Dialog` frame over HeroUI's `Modal` with its `useDialogAction` and `useRefocusIfLost` hooks, plus the form controls they share (`TextInput`, `OptionSelect`); `dialogRequest.ts` is the request the sidebar's menus and the top bar's New console raise, which `App` holds and `RequestedDialog` renders. Escape and an outside click close one, and closing returns focus to where a menu took it from |
