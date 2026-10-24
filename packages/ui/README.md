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
- A scenario with `liveChanges` lists them as buttons beside the other controls, offered once its page is ready: each makes
  what another window or the machine would while the view is on screen (an account added, a terminal closed there, an agent
  found missing), as the daemon's own events through the steps' `daemonEvents`. They are for the motion and focus of a
  collection that changes under a view, which no first render shows. A pointer press on a button does not take focus from the window, so
  a pinned panel or an open menu stays as it is (a keyboard press on one does move focus out). A change's `id` in a frame URL's `live` parameter (comma-separated) applies
  it after the steps, which is how `pnpm gallery:sweep` runs every one.
- Report scenarios expose the document event `gallery:report-page-created` through
  `src/gallery/reportPageFixture.ts`, applying fixture pages through the store's `applyDaemonEvent`.
  `node scripts/gallery-report-focus.mjs <gallery-origin>` drives report replacement and focus checks against a served gallery.
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
  presses a dialog's confirming button, `ui.contextMenu` opens a row's right-click menu, `ui.touchPress` and `ui.tapTopBarStatus` press with a finger,
  for the popover a touch opens in place of a tooltip).
- Other fields of the scenario type (`scenario.ts`) include `preferences` (what the app persists), `toasts`,
  `sessionOpenError`, `terminal: "refuse"`, `clipboard: "unavailable"` (a plain browser over insecure http),
  `shellTerminal` (how the Terminal panel's shell terminals behave: `"shell"` at its prompt by default, or `"refuse"`; an exited shell
  always draws its last screen and closes, and `terminal` never reaches them), `shellTerminalStart: "pending"` (a shell start that is never answered, for the panel's "Starting terminal…" state), `quitRequested` (the platform gets a stubbed exit capability and
  asks the window to quit once the steps have run, so the quit confirmation is on screen; confirming ends nothing),
  `externalLinks` (the external-link capability stub),
  `sessionHandoffs` (a handoff the daemon fails or never answers, ahead of the refusals it works out from the session),
  `macosPermissions` (stubs the desktop shell's privacy grants; without it the page is a plain browser's),
  `sessionBranches` / `projectRemovalBranches` (the branches and worktrees bound to sessions, and those a project's
  removal with its folder in the Trash takes along, that the fixture daemon's inspections find; built with `branchItemOf` /
  `worktreeItemOf`),
  `pathLinks` / `worktreeFiles` (the paths the fixture daemon's `probe_path_links` finds, each file the viewer shows (or folder with a README or index file) with where it is read from, and the files `read_project_file` finds in a worktree; with `terminal: "paths"`),
  `conversationLocation` (what `locate_conversation_to_resume` answers for any ID: a location, a `FixtureError` that refuses the lookup, or `"pending"` for one never answered; left out, not found),
  `sessionHistory` (what `read_session_history` answers per archived session: messages, `[]` for none, `"unavailable"`, or `"pending"` for a read never answered) and `sessionResumes` (a `resume_session` never answered, which keeps the "Resuming session…" cover),
  `agentModels` (what the fixture daemon lists per agent, default `SAMPLE_CLAUDE_MODELS`), `files` / `filesByProject` /
  `git` (replace the project browser's fixtures; a `null` `git` is no repository) and `viewer` (renders the file
  viewer alone, in the "File viewer" group).
- Sessions switching their account: `switchingAccount(state, ids)` (`fixtures/builders.ts`) lays the daemon's
  `Session.switching_account` over the named sessions of a state, so they open straight in the "Switching account…"
  view. `sessionAccountSwitches` stages what the daemon answers to a switch the steps start: a refusal, or `"pending"`;
  the fixture daemon broadcasts the session as switching while the request is held, as the real one does.
- Account sign-in in Settings → Agent accounts: `state.accountLogins` holds the logins the daemon already reports when
  the window opens (waiting with its page or device code, signed in, failed, timed out), so a login in progress is
  shown straight away; build them with the `loginOf` helper in `fixtures/settings.ts`. `loginOutcome`
  (`"succeeded"`, `"failed"`, `"timed_out"`) is how a login the steps start ends, a moment after it reaches waiting;
  left out, it stays waiting until cancelled. `loginStartError` makes `start_account_login` refuse with that error.
  `agentListError` makes `add_agent`, `remove_agent` and `reorder_agents` refuse with that error, for what the window
  could not have seen coming; left out, they follow the added list's own rules.
- Account sharing in Settings → Agent accounts and in the account pickers: `state.accountSharing` holds the sharing
  states the daemon reports when the window opens, by account; build it with `accountSharingOf` (`builders.ts`, taken
  by `snapshotState` as `accountSharing`). An entry with `in_use` makes the account's switch unusable, and one with
  `progress` is a change running (its switch, its pickers' entries and the session operations on it unavailable). An
  account left out is free. The fixture daemon, like the real one, turns away `set_account_sharing` for an account marked
  in use or changing (recording nothing), and refuses opening, resuming, switching or handing off a session of a changing
  account. `accountSharingError` decides how a change that is not turned away ends: an `account_sharing_failed` error
  records the choice, marks the account's change incomplete and keeps the failure (its `items` and `detail` from the
  error's params) in its state, which the switch shows with Try again; any other code is a refusal with nothing
  recorded. Left out, the change finishes at once. The fixture daemon sends no progress; a running change is drawn by
  putting `progress` in the starting state.
- `configuration` sets what the platform and daemon answer to configuration export and import
  (`src/gallery/fixtureConfiguration.ts`): `file` (`"dismissed"`, `"pending"`), `readError`, `saveError`, `downloads`,
  `unavailable`. Exporting needs nothing; an inspection and an import result are written out per scenario, and the
  fixture daemon refuses an import result that does not follow from what the steps asked for.
  `fixtures/configuration.test.ts` holds the written results to the daemon's rules.
- Write no `width` unless the window itself is part of the scenario (the page opens at 1440): set one where steps open
  a drawer (the sidebar's, which exists only below the breakpoint) or the layout under test is about a single width, and say in the
  title or description what that width buys. Write a `height` only where a step needs the window too short for a
  dialog's body, which then scrolls (`ui.tabToRegion` finds none in a tall window). A scenario that merely wants to be
  seen narrow or short uses the page's width and height controls; a copy differing only by width earns nothing. A
  chosen width stays on scenarios that carry none, and is dropped on picking one that carries its own.
- `held: true` opens a scenario with the shortcut modifier held, for a view that exists only while it is held: the
  new-session "+" buttons' quick form. The viewer can still choose Released with the page's Modifier control, and like
  a width that choice stays on scenarios that carry none and is dropped on picking one that carries its own. The
  modifier is pressed after the steps, so they find those buttons by their plain names; `ui.pressWithModifier`
  presses a control with the modifier down for that press alone, for what a press does when the page has not seen
  the modifier held.
- Live changes are written with the builders in `fixtures/liveChanges.ts` (`accountAdded`, `accountRemoved`,
  `agentAvailability`, `terminalOpened`, `terminalClosed`), not raw `daemonEvents` steps; add a builder there for another
  kind of collection.
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
any gallery failure; `pnpm gallery:sweep --help` lists the flags (`--group`, `--lang`, `--theme`, `--fresh`, `--list`,
`--url`, ...).
Playwright installs no browser: run `pnpm exec playwright install chromium` from `packages/ui` once, or point
`--chromium <path>` / `OCTOBOARD_CHROMIUM` (or `--playwright <dir>` / `OCTOBOARD_PLAYWRIGHT`) at an existing one.

The page loads wait for a free heavy-step slot, the machine-wide lock `pnpm verify` also takes (`OCTOBOARD_VERIFY_SLOTS`
sets how many), and say who holds it while they wait, so sweeps in several worktrees queue rather than overload the
machine. A page that came out clean (scenario, language and theme) is recorded against a key of everything the result
depends on: this package including the git-ignored `src/gallery/preview/`, `config/`, `scripts/env.mjs` and
`scripts/env.d.mts`, the env files the dev server reads (`.env`, `.env.local`), the manifests, the lockfile and the
dependency patches, the Node version, the browser's version and the `OCTOBOARD_DAEMON_PORT` and `VITE_*` environment
variables. The key covers the whole package at once, so any edit under `packages/ui` loads everything again. A later sweep of the same content skips the recorded
pages and reports how many it skipped; `--fresh` loads them anyway. Only clean pages are recorded, and not when the
content changed during the run. The records are kept with `pnpm verify`'s stamps, in `verify-stamps/` of the repository's
common git directory, so every worktree shares them; with `--url`, whose gallery is another checkout's content, none are
read or written. A clean page also counts as clean for a later sweep that loads more pages at once, though a failure
that shows only under load is what that sweep would have looked for.

## Layout

Imports run one way between the feature directories: `src/panels/` imports `src/browser/`, `src/browser/` imports
`src/viewer/`, and `src/viewer/` imports neither `src/browser/` nor anything that does. Code they share lives in
`src/components/`, which imports none of them. Nothing enforces this.

| Path | Role |
|---|---|
| `vite.config.ts` | The Vite and Vitest configuration: the React Compiler, Tailwind, and the plugins serving the app name, the gallery frame's Content Security Policy and the viewer theme shim |
| `typography.mjs` | A build-time Tailwind plugin wrapping `@tailwindcss/typography` for `src/style.css` |
| `index.html` | The window's Content Security Policy (a `<meta>` tag) and the inline script that applies the theme class before first paint |
| `website-preview.html`, `vite.preview.config.ts` | The entry and build config of the fixture-backed React build embedded by the public website |
| `src/website-preview/` | The preview's bootstrap and fixtures: `main.tsx` mounts the real `App` over `createPreviewDaemon` (`daemon.ts`), with sample repositories (`repositories/`), `createPreviewScenario` (`scenarios.ts`) for its three scenes, with the sample agent availability and quota readings, and a scroll bridge to the embedding page; the iframe and scene buttons belong to [`apps/web/`](../../apps/web/README.md) |
| `gallery.html`, `gallery-frame.html`, `src/gallery/` | The dev-only UI state gallery (see "The UI state gallery"): `shell.tsx` and `frame.tsx` (the shell page and the page one scenario renders in), `scenario.ts` (the scenario type), `interact.ts` (step helpers), `fixtureDaemon.ts` with `fixtureGit.ts` and `fixtureConfiguration.ts` (the fixture daemon), `fixtures/` (the scenarios, `builders.ts`, `groups.ts`, sample projects and repositories, change factories), and the untracked, per-checkout `preview/` (extra scenarios appended to `SCENARIOS`) |
| `scripts/gallery-sweep.mjs` | `pnpm gallery:sweep` (takes a heavy-step slot and skips pages recorded as clean; see "Sweeping the gallery") |
| `scripts/gallery-chip-geometry.mjs` | A standalone Playwright check of chip geometry over gallery scenarios, run by hand against a served gallery |
| `scripts/wkbench/` | `pnpm bench:wkwebview`: the WKWebView latency benchmark (see [its README](scripts/wkbench/README.md)) |
| `src/main.tsx` | Startup: picks the platform, locates the daemon, creates the connection, pushes the theme, reveals the native window, registers global runtime-error handlers, and mounts the app or `StartupScreen` with the error-alert host |
| `src/App.tsx` | The screen: window chrome (`TitleBar`, `Rail`), the content panel (`WelcomeScreen`, or sidebar, terminal pane and aside), session selection and resume, the open archive, toasts, connection banner and dialog layer; holds who owns the aside and renders it as `RightPane` (a project's panels are Files and Terminal, a console session's is its Report) |
| `src/StartupScreen.tsx`, `src/ErrorBoundary.tsx` | The screens shown when there is no daemon connection, or the UI itself crashed |
| `src/WelcomeScreen.tsx` | `WelcomeScreen`: what the content area shows while the daemon has no console (New console, and Import configuration where the platform can) |
| `src/platform/` | `PlatformAdapter`, the interface to everything native, made of optional capabilities (an absent one means the feature is absent), `StatusItemMenu`, the native menu bar icon's menu and optional selected-session quota display, and `StatusItemSubmenu`, its submenu contents and optional console-account quota display; implemented by `tauri.ts` and `browser.ts`, exposed by `react.tsx`. Only `tauri.ts` may import `@tauri-apps/*` |
| `src/daemon.ts` | Locating the daemon (`?port=`, `VITE_DAEMON_PORT`, or the page's own origin) and the `?error=` startup message |
| `src/daemon-client.ts` | The WebSocket client for `GET /ws/control`: request/reply correlation, reconnect, event dispatch and batching |
| `src/protocol.ts` | Hand-written TypeScript mirror of `apps/daemon/src/protocol.rs` / `PROTOCOL.md` |
| `src/store.ts` | The Zustand store of the connection and the console/project/session state derived from daemon events; `createDaemon(origin)`, `useDaemon` / `useDaemonStore` / `useDaemonStoreEqual`, `useErrorRecords` (record subscriptions for error surfaces with or without a daemon provider), and the `onToast` stream (`toastError`, `toastNotice`); it also holds the daemon's shell terminal records, with `applyDaemonEvent` and `seedShellTerminal` for a caller acting on a request's reply ahead of its broadcast, and `accountSharing`, each account's sharing state (in use, progress, last failure) from the snapshot and the `account_sharing` event; and the account-switch state: `queuedAccountSwitches` (sessions of a batch waiting their turn, set by `setAccountSwitchesQueued`) and `daemonSwitching` (sessions the daemon reports as `switching_account`), both shown as the session switching |
| `src/accountSwitchBatch.tsx` | `runAccountSwitchBatch`: the app-level runner of a batch of session account switches, started by the switch dialog but outliving it; runs a few at a time, marks the rest queued in the store and raises one error alert for the failures at the end; `canSwitchTo` tells whether a session is still a target |
| `src/sessions/`, `src/sameValue.ts` | `indexSessions` (sessions grouped by project and console), the hooks reading a group, and `sameValue` (deep equality) |
| `src/daemonMessage.ts` | `daemonMessage` provides localized plain daemon wording; `daemonParams` and the semantic maps distinguish producer-supplied record references, literal names and technical values; `richMessageCode` selects rich summaries, with shared diagnostic and recovery classification; `sharingFailureItems` and `sharingStateKey` decode and classify unfinished sharing entries for both plain and rich error renderers (the switch renders its own failure through the same ones) |
| `src/errorPresentation.ts`, `src/errorText.ts` | `ErrorPresentation`, the shared failure value preserving daemon metadata and local exception causes for rendering and classification, and `errorText`, its plain-text form |
| `src/errorRecovery.ts` | `ErrorRecovery` names known user remedies, `ActionableError` attaches one to a local exception, and `classifyError` distinguishes recognized actionable failures (the daemon's `account_quota_exhausted` refusal among them) from unexpected ones |
| `src/errorAlerts.ts`, `src/reportError.tsx`, `src/runtimeError.ts` | `errorAlerts` owns the window's error queue, `reportError` submits structured failures, and `runtimeError` extracts exceptions from browser error events |
| `src/i18n/` | Internationalization: languages and system-language matching, the typed English catalog and translated ones (`messages/`), lookup (`catalog.ts`), the language choice (`language.ts`) the React side (`react.tsx`: `useT` for plain text, `Message` for rich copy including shared product and control references, and `LanguageProvider`), and `displayColumns.ts` (`displayColumns` and `ellipsize`, measuring and cutting text by display width for lines with a column cap). Every user-facing string goes through it |
| `src/theme.tsx` | `ThemeProvider` / `useOctoboardTheme`: the light/dark/system choice and what "system" resolves to |
| `src/appConfig.ts` | The app-level facts from the repo-root `config/app.json` (`name`, `repositoryUrl`, `issueUrl`) |
| `src/preferenceKeys.ts`, `src/persistedPreference.ts` | The `localStorage` key of every persisted preference, and `createPersistedPreference`, the guarded module-wide store behind them |
| `src/style.css` | The Tailwind theme and the window's design tokens (breakpoint, chrome sizes and colours, the `--app-*` palette) |
| `src/layout/` | Pane layout: the `docked` breakpoint (`useIsNarrow`, on `useMediaQuery` in `breakpoint.ts`) and the narrower one below which the aside no longer docks (`useAsideFloats`, `paneWidth.ts`), the sidebar's drawer and floating geometry and motion, pane width, the sidebar's toggle and the hover float-in of both panes, the aside's from its panel kinds' toggles in the top bar (`usePaneToggles.ts`), `AsideOwner` (`asideOwner.ts`: whose content the aside shows), `AsidePane` (the aside's frame: docked at once while a panel is open, over the terminal's area in a window too narrow to dock it, or floating over the terminal while no panel is open; it paints nothing, only pads the cards that carry their own surface and edge) and F6 region cycling (`useRegionCycle.ts`: one region per panel kind of `src/panels/`) |
| `src/navigation/` | Back and Forward through the locations the content area has shown (`history.ts`, `useNavigationHistory`) |
| `src/lifecycle/` | Hooks tying the UI to the platform: the exit flow (`useAppExit`, whose quit confirmation is held up by busy sessions and by shell terminals running something), system notifications, native menu and menu bar icon (`useStatusItemMenu`; `statusItemMenu.ts` builds its sessions and selected-session quota indicator, `statusItemQuota.ts` its per-agent quota submenus (whose 5-hour/weekly windows are `meter` items) and console-account opener indicators as structured display data the shell draws), native window theme, run in background, and the git status schedule |
| `src/waiting.ts` | The sessions waiting for the user in sidebar order, and the one after the current selection |
| `src/useWindowShortcut.ts`, `src/useQuickCreateModifier.ts`, `src/useNow.ts` | `useWindowShortcut` (a window-wide capture-phase shortcut), `useQuickCreateModifier` (whether the shortcut modifier is held), `useNow` (a shared 30 s clock) |
| `src/focusGuard.ts`, `src/contextMenuGuard.ts` | A react-aria focus workaround imported by `main.tsx`, and `suppressNativeContextMenu` (desktop app only) |
| `src/agents.ts`, `src/agentLaunchSettings.ts`, `src/agentFields.ts`, `src/accountChoices.ts` | The agents' names (`AGENT_LABEL`, and `AGENT_SHORT_NAME` for the menu bar icon's menu), the added agents the UI lists (`listedAgents`) and each agent's install page (`AGENT_INSTALL_URL`) and per-agent field tables, the model and effort picker rules, the console/project dialogs' per-agent field diffing, and the pure account-picker rules (including steering a default choice off a used-up account, `avoidExhaustedAccount`, `settledInitialChoice`, and whether an account is unavailable because its sharing change runs, `sharingChangeRuns`, `sharingChangeUnavailable`, `choiceChangingSharing`, and `sessionSwitchingUnavailable`, the reason a switching session's actions are unavailable) |
| `src/sessionLabel.ts`, `src/sessionColours.ts`, `src/gitStatusLabel.ts`, `src/projectFiltering.ts` | Plain session and git-badge wording and accessible names (`sessionChoiceTitle` supplies raw session labels for matching and native options), the console session colours' appearance, and the sidebar project filter's pure rules |
| `src/sessionCopy.tsx`, `src/gitStatusCopy.tsx` | Rich display companions to the plain session and git helpers: `sessionLocationContent` for location titles and `gitBadgeContent` for git-state tooltips |
| `src/sessionModel.ts` | A session's model and reasoning effort for display: `modelDisplayName`, `inheritedLaunch` (what the console or project would give it), `sessionModelEffort` (observed values first, the daemon's `default_effort` standing in for an effort the records leave unset, then the launch override or inherited value) and `sessionModelLabel` (the model line beside the agent mark in `sidebar/FocusView.tsx`) |
| `src/pathDisplay.ts`, `src/wirePath.ts`, `src/relativeTime.ts`, `src/conversationId.ts`, `src/reducedMotion.ts`, `src/imeKey.ts`, `src/avatarImage.ts` | Small shared helpers: home abbreviation and wire-path decoding for display, relative time, input-method key detection, avatar image conversion, and the conversation id and reduced-motion helpers |
| `src/components/` | Shared building blocks used across features: window chrome (`TitleBar`, whose end carries one toggle per panel kind of the aside's owner and no toggle for the pane, `Rail`, `ContentPanel`, `Toasts`), the header every panel of the right pane starts with (`PanelHeader`, with the stack's frame context), the resize handles (`ResizeSeparator`, the axis-generic one drawing a line or a grip, and `PaneResizeHandle`, the pane-width one wrapping it), menus and tooltips (`ActionMenu`, whose items may be a function read at open and may carry a reason for being unavailable, set by `ReasonText`, which also serves the option selects), the agent tab strip (`AgentTabList`, used by the settings accounts section and the rail quota panel; `tabDrag.ts` is the pointer drag that reorders its tabs), status and identity marks (`StatusBadge`, the pill after a name, in a HeroUI semantic colour and a soft or bordered variant; `InlineReference`, a reference inside running copy that always takes the font of the text around it; its sizes differ only in padding, `xs` by default), error and code display (`DaemonErrorMessage`, whose summary and details can render separately), per-failure remedies (`ErrorGuidance`), the presentations of a failure on the screen (below), scroll-fade and list-motion helpers, and focus-handoff hooks, and `useHostPathActions` (reveal or open a project file or folder on the host, shared by the browser's row actions and the viewer's toolbar). Imports neither `src/browser/` nor `src/viewer/` directly; `Rail.tsx` takes the console menu from `src/sidebar/`, the quota entry from `src/quota/` and a type from `src/settings/`, and `ActivityMarker.tsx` a type from `src/sidebar/` |
| `src/components/DiagnosticBlock.tsx` | `DiagnosticBlock` renders raw diagnostic output in a left-to-right code block with keyboard-accessible horizontal scrolling |
| `src/components/ErrorAlert.tsx`, `FailureAlert.tsx`, `PaneFailure.tsx`, `FailureMark.tsx`, `useHasPainted.ts` | The presentations of a failure that is the state of what the screen shows: `ErrorAlert` (the durable alert: title, reason, details, actions, with the announcement held to failures that arrive after a view has painted), `FailureAlert` (the shared on-screen failure with its reason, details and Try again), `PaneFailure` (`FailureAlert` centred in place of a pane's subject), `FailureMark` (the glyph that marks a failure belonging to one row, cell or place in a document, its reason in a popover) and `useHasPainted` (whether a view has painted, so only later failures are announced) |
| `src/components/ExternalLink.tsx` | `ExternalLink` renders shared outbound anchors through the platform link capability, `ExternalLinkIcon` supplies their marker for native action menus, and `reportExternalLinkFailure` reports opener failures with a manual destination |
| `src/sidebar/` | The sidebar: `Sidebar` (one console's sessions and projects), `FocusView.tsx` (the focus modes), rows and action menus, the project filter, new-session controls, `order.ts` (sort rules, binding-graph queries such as `sessionWithDescendants`) and `sidebarView.ts` (current console and focus target) |
| `src/archive/` | `ArchiveLayer` and `ArchiveView`: archived sessions of a project, console or console session, listed over the terminal area, with deletion |
| `src/terminal/` | The xterm.js terminal: `TerminalController` (instance, session socket, selection, paste, dormant snapshot), `TerminalPane` (React boundary, reconnect and cover states), `SessionHistoryView` with `useSessionHistory` (an archived session's history and retained request failure, shown as a `PaneFailure` with retry), `reconnectPolicy.ts` (the reconnect backoff `TerminalPane` and the shell views share), the Terminal panel's content: `ShellTerminalPanel` (a project's shell terminals as tabs, with the start, rename and close actions and the pending and empty states), `ShellTerminalView` (one kept-mounted controller per tab, with its reconnect policy and exited notice) and `shellTerminals.ts` (the per-project list, the active-tab preference and `startShellTerminal`, which joins a start already in flight), `TerminalEditMenu`, link handling (`terminalLinks.ts`, `linkColors.ts`, `TerminalLinkTooltip`), with the rows and logical lines it reads from the buffer and the cells a link fills (`terminalLines.ts`), file paths in the output that the daemon confirms and turns into links (`pathLinks.ts`), and what a click on one does (`openPathTarget.ts`: show the file in the viewer, or open it on the host), `emptyState.ts` and `xtermThemes.ts` |
| `src/report/` | A console session's report, the content of the Report panel: `ReportPanel`, and `pageDocument` / `pageBridge.js` for the sandboxed page frame |
| `src/viewer/` | The read-only file viewer modal, opened by the project browser and by a path link in the terminal: `FileViewer` over the application-owned `ViewerSubject` model (`content.ts`; no rendering-library type crosses it), code, diff and Markdown surfaces, and the header choices (diff layout, word wrap, Markdown view). `renderer.tsx` (loaded lazily by `codeRenderer.tsx`) is the only importer of `@pierre/diffs`; `themeShim.ts` is swapped in for the library's theme modules by a `vite.config.ts` plugin |
| `src/panels/` | The right pane, a vertical stack of inset panel cards: `RightPane` (the pane frame around the stack), `PanelStack` (the cards and the resize separators between them), `panelRegistry.tsx` (the registry of panel kinds, Files, Terminal and Report, each for one owner's stack) and `stackState.ts` (the pure stack state: open, close, maximize, toggle, resize, and its persisted form) with `usePanelStacks.ts` (the per-client preference `octoboard.panelStacks`, one stack per owner, which also carries over the old visibility preference), and `useFloatingPanel.ts` (the one panel per owner that floats over the agent terminal in a window too narrow to dock the pane, kept apart from the stacks) |
| `src/browser/` | The project browser, the Files panel's content: `ProjectBrowser` (Files / Changes tabs, its header a `PanelHeader`; mounted lazily through `LazyProjectBrowser`), the Files tree and search, and the Git mode's single Changes view (`GitView.tsx`; `BranchSelect.tsx` picks the branch, whose uncommitted changes, comparison with the main working copy's branch and commits form one list, with the lines above it saying what it is made of) with its request hooks (`useGitReview`, `useChangeReader`, `useBranchComparison`, `useCommitList`, ...), the pure rules `changes.ts`, `changesScope.ts`, `changesStatus.ts`, `comparisonNews.ts` and `shortCommit.ts`, and `browserState.ts` (per-project mode and selections, and the worktree the Git mode shows, where a new shell terminal starts), and `TerminalFileViewer` (mounted lazily through `LazyTerminalFileViewer`), the viewer over a file a terminal path link named, read on its own so it needs no project browser and leaves the right pane as it is |
| `src/configuration/` | Configuration export and import: `ExportConfigurationDialog`, `ImportConfigurationDialog` (review and result steps) and the pure `importPlan.ts` |
| `src/quota/` | Agent account plan quota: `quotaFormat.ts` (React-free wording, percent, countdown, plan formatting, detail-window ordering, primary-window selection and the used-up judgement and wording, `exhaustedQuotaWindow`, `quotaUsedUpAlert` and `quotaUsedUpSentence`), `quotaCopy.tsx` (`quotaUsedUpCopy` for rich used-up notices and `QuotaWindowName` for rich window references), `accountQuotaOf.ts` (an account's entry in the store's `accountQuotas`, React-free), `sessionQuota.ts` (`sessionQuotaBarOf` (selected-session reading eligibility, yielding the `QuotaBar` to draw) and `accountQuotaLabel` (account/window identification shared by the rail and native menu bar)), `useAccountQuota` (an account's reading, aware of whether its agent is available), `QuotaMeter` (with `QuotaMeterPair`, the two-layer 5-hour/weekly bar), `AccountUsageBar` (the compact decorative bar in account pickers), `AccountQuotaSummary`, `AccountNameLine` (shared with the settings accounts section) and `QuotaPlan`, and `RailQuotaEntry` (the rail button with its tooltip-less two-layer selected-session indicator and the non-modal floating panel listing each available agent's accounts with their quota, with a way into Settings' accounts section) |
| `src/settings/` | The settings modal: `SettingsDialog` with its sections, `useSettingsDialog` and `useMacosPermissions`; the agent accounts section (`AgentAccountsSection`) with the added agents list (`AgentListMenu`: the tab menu, the More menu, the remove confirmation, reordering with focus; `AddAgentDialog`; `agentUsage.ts`, what still uses an agent) and account sign-in (`AccountLogin`: the login line and its actions, and `LoginPageOpener`, mounted in `App.tsx` so a login's page opens wherever the window is), `AccountApprovalSelect` and `StartQuotaWindowsSetting` (the toggle for the daemon starting reset usage windows); `AccountSharingSwitch` owns each account’s sharing control, its reason when disabled, its progress bar, its failure with Try again and agent-specific help; `SettingRow.tsx` exports `SettingRow`, the accessible `SettingSwitch` control and `SwitchSettingRow` for settings sections |
| `src/dialogs/` | The console, project, session, rename, confirmation and directory-picker dialogs and the daemon's two questions (trust prompt, session request) on the shared `Dialog` frame, `ErrorAlerts` (the window's acknowledgement host), the shared form controls (`OptionSelect`, with `Option.labelContent` for visible labels and tooltips while `label` supplies matching and native options; `TextInput`, with `labelContent` for a rich visible label while `label` supplies its plain accessible name, and `describedBy` for an alert that describes the field), `useAgentModels` (model discovery with structured request failures), and `dialogRequest.ts` / `RequestedDialog.tsx` (dialog requests raised from menus), the session dialog's Resume tab in three files: `useLocatedConversation.ts` (asking the daemon where an entered conversation ID is held), `useResumeTarget.ts` (turning the answer into the target the tab resumes or opens, with the ID's problem, the looking flag and the status text) and `ResumeConversationFields.tsx` (the Session ID field and the fields the target decides), `accountOptions.tsx` (account picker options with usage bar and used-up reason), and `BranchCleanupOptions.tsx` with `useSessionBranches.ts` (the branches and worktrees bound to sessions that an archive, deletion or project removal would take along, asked of the daemon and offered in the confirmations as `ConfirmOption` checkboxes) |
| `src/dialogs/projectNameDefault.ts`, `src/dialogs/RenameDialog.tsx`, `src/dialogs/useCloseWhenBlocked.ts` | `projectNameDefault` derives display-only project-name hints from the daemon host's lexical path and repository naming rules; `RenameDialog` renders a caller-supplied required-field example; `useCloseWhenBlocked` closes unusable dialogs and preserves structured error metadata for their acknowledgement |
