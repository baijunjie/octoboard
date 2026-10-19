# Octoboard UI

`@octoboard/ui`: a HeroUI 3 + React 19 + Tailwind 4 + Vite application, the UI of the desktop application. The macOS
shell in [`../../apps/desktop/`](../../apps/desktop/README.md) loads this package's `dist/` (bundled into the
application, not served by the daemon), and the same build also runs in a plain browser. It has the window chrome (top
bar and left rail), the content panel (sidebar, terminal pane and aside) or, while the daemon has no console, a welcome
screen, the archive view, the dialogs and settings, notices, error acknowledgement and the failure screens, all in the user's language
(`src/i18n/`).

It talks to `octoboardd` (see [`../../apps/daemon/`](../../apps/daemon/README.md)) only over the WebSocket protocol in
[`../../apps/daemon/PROTOCOL.md`](../../apps/daemon/PROTOCOL.md), and reaches anything native only through the platform
adapter (`src/platform/`), chosen once at startup. A separate website-preview entry renders the same `App` over local
fixtures for the public website, without a live daemon.

## Development

This is a package of the repository's pnpm workspace: install from the workspace root with `pnpm install`, then run
the scripts from this directory.

- `pnpm dev` — the Vite dev server on port 5174, which `pnpm tauri dev` in `apps/desktop/` also starts.
- `pnpm gallery` — the same dev server on port 5175, opening the UI state gallery (below).
- `pnpm gallery:sweep` — loads gallery scenarios in a headless browser and fails on any gallery failure (see
  "Sweeping the gallery").
- `pnpm typecheck` — `tsc --noEmit`.
- `pnpm test` — `vitest run`: the unit tests beside the code, plus jsdom tests for stateful classes and components.
- `pnpm build` — typechecks, runs the i18n catalog check (`vitest run src/i18n`), then writes `dist/` with relative
  asset paths.
- `pnpm build:preview` — builds the website demonstration into `apps/web/public/preview/` with
  `vite.preview.config.ts`.
- `pnpm bench:wkwebview` — a latency benchmark of the production bundle in the system WKWebView (macOS only); see
  [`scripts/wkbench/README.md`](scripts/wkbench/README.md).

### Pointing it at a daemon

Start `octoboardd` and read its port from the first line it prints on stdout (or `$TMPDIR/octoboardd.port`). Then either:

- open `http://localhost:5174/?port=<port>`, or set `VITE_DAEMON_PORT`; or
- run `OCTOBOARD_DAEMON_PORT=<port> pnpm dev` and open the plain URL, which reaches the daemon through a same-origin
  `/ws` proxy (`vite.config.ts`).

Both variables can also be set in the repo-root `.env` (commented out there) or a gitignored `.env.local`.

### The UI state gallery

A dev-only page, needing no daemon, that shows UI states hard to reach against a live daemon: the real `App` over a
fixture daemon (`src/gallery/fixtureDaemon.ts`). `pnpm gallery` serves it on port 5175; under `pnpm dev` it is at
`http://localhost:5174/gallery.html` (which resets the dev app's `octoboard.*` preferences, hence port 5175 for
`pnpm gallery`). `pnpm build` does not bundle it but does type-check `src/gallery/`.

- Scenarios are listed by functional area, Normal previews first and Exceptional second. Theme, language, window
  width and height, and whether the shortcut modifier is held are controls above the window, all kept in the URL. The
  window is an iframe, so its width decides the `docked` breakpoint for real.
- A scenario's page sets `data-gallery-ready` on `<html>` once its steps have run and finite animations ended;
  automation waits on it before capturing. A failure of the gallery's own machinery (a step that threw, a fixture
  that cannot be answered as the real daemon would, an exit transition that never ends) is painted over the page as a
  `[data-gallery-failure]` overlay (`src/gallery/banner.ts`); check a capture for it.
- A frame URL with `chrome=1` gives the page the desktop window's `windowChrome` capability (`src/gallery/frame.tsx`),
  so what only the desktop app offers can be driven in a browser.
- `src/gallery/preview/` is git-ignored, so it exists per checkout: every `.ts` / `.tsx` file directly in it (not in
  subdirectories) must default-export a `Scenario[]`, which `fixtures/index.ts` appends to `SCENARIOS`; anything else
  goes in a subdirectory. Those scenarios are therefore listed, swept and held by the gallery's tests in that checkout
  only. Such files import fixtures from their own modules, never from `fixtures/index.ts` (an import cycle).

**Conventions for writing scenarios**

- Scenarios live in `src/gallery/fixtures/` (the file for their area, or a new file listed in `index.ts`). `group` is
  one of the titles in `GROUPS` (`groups.ts`); a new group gets the icon its heading carries. The required `category`
  is `"normal"` (expected UI, including empty, loading and platform-limited views) or `"exceptional"` (deliberate
  failures, validation errors, refusals, conflicts), chosen by the view being confirmed rather than the component or
  file name. Keep failed subjects out of normal navigation.
- `state` is built with `snapshotState` from `builders.ts`, so a protocol change breaks it at typecheck. State that
  lives in a component rather than the store (a dialog, the archive view, the selected session) is reached with the
  shortest `steps`, which press controls by name as a user would (`ui.press(ui.t("rail.settings"))`; `ui.confirm()`
  presses a dialog's confirming button, `ui.contextMenu` opens a row's right-click menu).
- Other fields of the scenario type (`scenario.ts`) include `preferences` (what the app persists), `toasts`,
  `sessionOpenError`, `terminal: "refuse"`, `clipboard: "unavailable"` (a plain browser over insecure http),
  `externalLinks` (the external-link capability stub),
  `sessionHandoffs` (a handoff the daemon fails or never answers, ahead of the refusals it works out from the session),
  `macosPermissions` (stubs the desktop shell's privacy grants; without it the page is a plain browser's),
  `sessionBranches` / `projectRemovalBranches` (the branches and worktrees bound to sessions, and those a project's
  removal with its folder in the Trash takes along, that the fixture daemon's inspections find; built with `branchItemOf` /
  `worktreeItemOf`),
  `agentModels` (what the fixture daemon lists per agent, default `SAMPLE_CLAUDE_MODELS`), `files` / `filesByProject` /
  `git` (replace the project browser's fixtures; a `null` `git` is no repository) and `viewer` (renders the file
  viewer alone, in the "File viewer" group).
- `configuration` sets what the platform and daemon answer to configuration export and import
  (`src/gallery/fixtureConfiguration.ts`): `file` (`"dismissed"`, `"pending"`), `readError`, `saveError`, `downloads`,
  `unavailable`. Exporting needs nothing; an inspection and an import result are written out per scenario, and the
  fixture daemon refuses an import result that does not follow from what the steps asked for.
  `fixtures/configuration.test.ts` holds the written results to the daemon's rules.
- Write no `width` unless the window itself is part of the scenario (the page opens at 1440): set one where steps open
  a drawer (which exists only below the breakpoint) or the layout under test is about a single width, and say in the
  title or description what that width buys. Write a `height` only where a step needs the window too short for a
  dialog's body, which then scrolls (`ui.tabToRegion` finds none in a tall window). A scenario that merely wants to be
  seen narrow or short uses the page's width and height controls; a copy differing only by width earns nothing. A
  chosen width stays on scenarios that carry none, and is dropped on picking one that carries its own.
- Steps name a control in the language the window is in, so copy is never written out: controls through `ui.t`, a
  session row through `ui.session`, a change row through `ui.changeRow`. A name that is not copy (a branch, a file, a
  title the fixture sets) is a plain string or a test of the name; a matcher is never a regular expression.
  `fixtures.test.ts` fails a step that names a control by the English text of a message another language's catalog
  translates, bare or filled; a custom test of the name that stands for copy must be built from the helpers, and may be
  given a description with `describing(...)` (what a failed step reports).
- The fixture daemon refuses a reply it cannot fill faithfully rather than sending a field unfilled: where the real
  daemon counts a change side's `lines`, the fixture carries the text to count it from (`FixtureChange.whole`), or the
  request rejects with `FixtureUnfillableError`. Change fixtures are therefore written through the factories in
  `fixtures/changeFixtures.ts` (`textChange`, `deletion`, `untracked`, ...), never as literals. A rule over a whole
  repository that no single change shows is held by a test over the sample repository (`objectIdCheck` in
  `fixtures/invariants.ts`); add such a rule there.

#### Sweeping the gallery

`pnpm gallery:sweep [ids or patterns]` loads the selected scenarios (all by default) in a headless browser and fails on
any gallery failure; `pnpm gallery:sweep --help` lists the flags (`--group`, `--lang`, `--theme`, `--list`, `--url`, ...).
Playwright installs no browser: run `pnpm exec playwright install chromium` from `packages/ui` once, or point
`--chromium <path>` / `OCTOBOARD_CHROMIUM` (or `--playwright <dir>` / `OCTOBOARD_PLAYWRIGHT`) at an existing one.

## Layout

Imports run one way between the feature directories: `src/browser/` imports `src/viewer/`, and `src/viewer/` imports
neither `src/browser/` nor anything that does. Code both need lives in `src/components/`, which imports neither of
them. Nothing enforces this.

| Path | Role |
|---|---|
| `vite.config.ts` | The Vite and Vitest configuration: the React Compiler, Tailwind, and the plugins serving the app name, the gallery frame's Content Security Policy and the viewer theme shim |
| `typography.mjs` | A build-time Tailwind plugin wrapping `@tailwindcss/typography` for `src/style.css` |
| `index.html` | The window's Content Security Policy (a `<meta>` tag) and the inline script that applies the theme class before first paint |
| `website-preview.html`, `vite.preview.config.ts` | The entry and build config of the fixture-backed React build embedded by the public website |
| `src/website-preview/` | The preview's bootstrap and fixtures: `main.tsx` mounts the real `App` over `createPreviewDaemon` (`daemon.ts`), with sample repositories (`repositories/`), `createPreviewScenario` (`scenarios.ts`) for its three scenes, and a scroll bridge to the embedding page; the iframe and scene buttons belong to [`apps/web/`](../../apps/web/README.md) |
| `gallery.html`, `gallery-frame.html`, `src/gallery/` | The dev-only UI state gallery (see "The UI state gallery"): `shell.tsx` and `frame.tsx` (the shell page and the page one scenario renders in), `scenario.ts` (the scenario type), `interact.ts` (step helpers), `fixtureDaemon.ts` with `fixtureGit.ts` and `fixtureConfiguration.ts` (the fixture daemon), `fixtures/` (the scenarios, `builders.ts`, `groups.ts`, sample projects and repositories, change factories), and the untracked, per-checkout `preview/` (extra scenarios appended to `SCENARIOS`) |
| `scripts/gallery-sweep.mjs` | `pnpm gallery:sweep` |
| `scripts/gallery-chip-geometry.mjs` | A standalone Playwright check of chip geometry over gallery scenarios, run by hand against a served gallery |
| `scripts/wkbench/` | `pnpm bench:wkwebview`: the WKWebView latency benchmark (see [its README](scripts/wkbench/README.md)) |
| `src/main.tsx` | Startup: picks the platform, locates the daemon, creates the connection, pushes the theme, reveals the native window, registers global runtime-error handlers, and mounts the app or `StartupScreen` with the error-alert host |
| `src/App.tsx` | The screen: window chrome (`TitleBar`, `Rail`), the content panel (`WelcomeScreen`, or sidebar, terminal pane and aside), session selection and resume, the open archive, toasts, connection banner and dialog layer; holds who owns the aside |
| `src/StartupScreen.tsx`, `src/ErrorBoundary.tsx` | The screens shown when there is no daemon connection, or the UI itself crashed |
| `src/WelcomeScreen.tsx` | `WelcomeScreen`: what the content area shows while the daemon has no console (New console, and Import configuration where the platform can) |
| `src/platform/` | `PlatformAdapter`, the interface to everything native, made of optional capabilities (an absent one means the feature is absent), `StatusItemMenu`, the native menu bar icon's menu and optional selected-session quota display, and `StatusItemSubmenu`, its submenu contents and optional console-account quota display; implemented by `tauri.ts` and `browser.ts`, exposed by `react.tsx`. Only `tauri.ts` may import `@tauri-apps/*` |
| `src/daemon.ts` | Locating the daemon (`?port=`, `VITE_DAEMON_PORT`, or the page's own origin) and the `?error=` startup message |
| `src/daemon-client.ts` | The WebSocket client for `GET /ws/control`: request/reply correlation, reconnect, event dispatch and batching |
| `src/protocol.ts` | Hand-written TypeScript mirror of `apps/daemon/src/protocol.rs` / `PROTOCOL.md` |
| `src/store.ts` | The Zustand store of the connection and the console/project/session state derived from daemon events; `createDaemon(origin)`, `useDaemon` / `useDaemonStore` / `useDaemonStoreEqual`, and the `onToast` stream (`toastError`, `toastNotice`) |
| `src/sessions/`, `src/sameValue.ts` | `indexSessions` (sessions grouped by project and console), the hooks reading a group, and `sameValue` (deep equality) |
| `src/daemonMessage.ts` | Words a daemon `error` or `session_notice` from its `code` and `params` in a given language (`CODE_PARAMS`, `UI_PARAMS`, `messageCode`, `withUiParams`, `daemonParams`) |
| `src/errorPresentation.ts`, `src/errorText.ts` | `ErrorPresentation`, the shared failure value preserving daemon metadata and local exception causes for rendering and classification, and `errorText`, its plain-text form |
| `src/errorRecovery.ts` | `ErrorRecovery` names known user remedies, `ActionableError` attaches one to a local exception, and `classifyError` distinguishes recognized actionable failures from unexpected ones |
| `src/errorAlerts.ts`, `src/reportError.tsx`, `src/runtimeError.ts` | `errorAlerts` owns the window's error queue, `reportError` submits structured failures, and `runtimeError` extracts exceptions from browser error events |
| `src/i18n/` | Internationalization: languages and system-language matching, the typed English catalog and translated ones (`messages/`), lookup (`catalog.ts`), the language choice (`language.ts`) and the React side (`react.tsx`: `useT`, `Message`, `LanguageProvider`). Every user-facing string goes through it |
| `src/theme.tsx` | `ThemeProvider` / `useOctoboardTheme`: the light/dark/system choice and what "system" resolves to |
| `src/appConfig.ts` | The app-level facts from the repo-root `config/app.json` (`name`, `repositoryUrl`, `issueUrl`) |
| `src/preferenceKeys.ts`, `src/persistedPreference.ts` | The `localStorage` key of every persisted preference, and `createPersistedPreference`, the guarded module-wide store behind them |
| `src/style.css` | The Tailwind theme and the window's design tokens (breakpoint, chrome sizes and colours, the `--app-*` palette) |
| `src/layout/` | Pane layout: the `docked` breakpoint (`useIsNarrow`), drawer and floating pane geometry and motion, pane width and visibility, the sidebar/aside toggles (`usePaneToggles.ts`), `AsideOwner` (`asideOwner.ts`: whose content the aside shows) and F6 region cycling (`useRegionCycle.ts`) |
| `src/navigation/` | Back and Forward through the locations the content area has shown (`history.ts`, `useNavigationHistory`) |
| `src/lifecycle/` | Hooks tying the UI to the platform: the exit flow (`useAppExit`), system notifications, native menu and menu bar icon (`useStatusItemMenu`; `statusItemMenu.ts` builds its sessions and selected-session quota indicator, `statusItemQuota.ts` its per-agent quota submenus and console-account opener indicators as structured display data the shell draws), native window theme, run in background, and the git status schedule |
| `src/waiting.ts` | The sessions waiting for the user in sidebar order, and the one after the current selection |
| `src/useWindowShortcut.ts`, `src/useQuickCreateModifier.ts`, `src/useNow.ts` | `useWindowShortcut` (a window-wide capture-phase shortcut), `useQuickCreateModifier` (whether the shortcut modifier is held), `useNow` (a shared 30 s clock) |
| `src/focusGuard.ts`, `src/contextMenuGuard.ts` | A react-aria focus workaround imported by `main.tsx`, and `suppressNativeContextMenu` (desktop app only) |
| `src/agents.ts`, `src/agentLaunchSettings.ts`, `src/agentFields.ts`, `src/accountChoices.ts` | The agents' names (`AGENT_LABEL`, and `AGENT_SHORT_NAME` for the menu bar icon's menu) and per-agent field tables, the model and effort picker rules, the console/project dialogs' per-agent field diffing, and the pure account-picker rules |
| `src/sessionLabel.ts`, `src/sessionColours.ts`, `src/gitStatusLabel.ts`, `src/projectFiltering.ts` | Session and git-badge wording and accessible names, the console session colours' appearance, and the sidebar project filter's pure rules |
| `src/sessionModel.ts` | A session's model and reasoning effort for display: `modelDisplayName`, `inheritedLaunch` (what the console or project would give it), `sessionModelEffort` (observed values first, else the launch override or inherited value) and `sessionModelLabel` (the model line beside the agent mark in `sidebar/FocusView.tsx`) |
| `src/pathDisplay.ts`, `src/wirePath.ts`, `src/relativeTime.ts`, `src/conversationId.ts`, `src/reducedMotion.ts`, `src/imeKey.ts`, `src/avatarImage.ts` | Small shared helpers: home abbreviation and wire-path decoding for display, relative time, input-method key detection, avatar image conversion, and the conversation id and reduced-motion helpers |
| `src/components/` | Shared building blocks used across features: window chrome (`TitleBar`, `Rail`, `ContentPanel`, `Toasts`), menus and tooltips (`ActionMenu`), the agent tab strip (`AgentTabList`, used by the settings accounts section and the rail quota panel), status and identity marks, error and code display (`DaemonErrorMessage`), failed-state acknowledgement and per-failure remedies (`useErrorAcknowledgement`, `ErrorGuidance`), scroll-fade and list-motion helpers, and focus-handoff hooks. Imports neither `src/browser/` nor `src/viewer/`; `Rail.tsx` takes the console menu from `src/sidebar/`, the quota entry from `src/quota/` and a type from `src/settings/`, and `ActivityMarker.tsx` a type from `src/sidebar/` |
| `src/components/ExternalLink.tsx` | `ExternalLink` renders shared outbound anchors through the platform link capability, `ExternalLinkIcon` supplies their marker for native action menus, and `reportExternalLinkFailure` reports opener failures with a manual destination |
| `src/sidebar/` | The sidebar: `Sidebar` (one console's sessions and projects), `FocusView.tsx` (the focus modes), rows and action menus, the project filter, new-session controls, `order.ts` (sort rules, binding-graph queries) and `sidebarView.ts` (current console and focus target) |
| `src/archive/` | `ArchiveLayer` and `ArchiveView`: archived sessions of a project, console or console session, listed over the terminal area, with deletion |
| `src/terminal/` | The xterm.js terminal: `TerminalController` (instance, session socket, selection, paste, dormant snapshot), `TerminalPane` (React boundary, reconnect and cover states), `TerminalEditMenu`, link handling (`terminalLinks.ts`, `linkColors.ts`, `TerminalLinkTooltip`), `emptyState.ts` and `xtermThemes.ts` |
| `src/report/` | A console session's report in the aside: `ReportPanel`, and `pageDocument` / `pageBridge.js` for the sandboxed page frame |
| `src/viewer/` | The read-only file viewer modal opened by the project browser: `FileViewer` over the application-owned `ViewerSubject` model (`content.ts`; no rendering-library type crosses it), code, diff and Markdown surfaces, and the header choices (diff layout, word wrap, Markdown view). `renderer.tsx` (loaded lazily by `codeRenderer.tsx`) is the only importer of `@pierre/diffs`; `themeShim.ts` is swapped in for the library's theme modules by a `vite.config.ts` plugin |
| `src/browser/` | The project browser in the aside: `ProjectBrowser` (Files / Changes tabs, mounted lazily through `LazyProjectBrowser`), the Files tree and search, and the Git mode's single Changes view (`GitView.tsx`; `BranchSelect.tsx` picks the branch, whose uncommitted changes, comparison with the main working copy's branch and commits form one list, with the lines above it saying what it is made of) with its request hooks (`useGitReview`, `useChangeReader`, `useBranchComparison`, `useCommitList`, ...), the pure rules `changes.ts`, `changesScope.ts`, `changesStatus.ts`, `comparisonNews.ts` and `shortCommit.ts`, and `browserState.ts` (per-project mode and selections) |
| `src/configuration/` | Configuration export and import: `ExportConfigurationDialog`, `ImportConfigurationDialog` (review and result steps) and the pure `importPlan.ts` |
| `src/quota/` | Agent account plan quota: `quotaFormat.ts` (React-free wording, percent, countdown, plan formatting, detail-window ordering and primary-window selection), `accountQuotaOf.ts` (an account's entry in the store's `accountQuotas`, React-free), `sessionQuota.ts` (`sessionQuotaWindowOf` (selected-session reading eligibility) and `accountQuotaLabel` (account/window identification shared by the rail and native menu bar)), `useAccountQuota` (an account's reading, aware of whether its agent is available), `QuotaMeter`, `AccountQuotaSummary`, `AccountNameLine` (shared with the settings accounts section) and `QuotaPlan`, and `RailQuotaEntry` (the rail button with its selected-session indicator and the non-modal floating panel listing each available agent's accounts with their quota, with a way into Settings' accounts section) |
| `src/settings/` | The settings modal: `SettingsDialog` with its sections, `useSettingsDialog` and `useMacosPermissions` |
| `src/dialogs/` | The console, project, session, rename, confirmation and directory-picker dialogs and the daemon's two questions (trust prompt, session request) on the shared `Dialog` frame, `ErrorAlerts` (the window's acknowledgement host), the shared form controls and `dialogRequest.ts` / `RequestedDialog.tsx` (dialog requests raised from menus), and `BranchCleanupOptions.tsx` with `useSessionBranches.ts` (the branches and worktrees bound to sessions that an archive, deletion or project removal would take along, asked of the daemon and offered in the confirmations as `ConfirmOption` checkboxes) |
