# UI structure (`packages/ui/`)

`@octoboard/ui`: the UI of the desktop application, a React 19 single-page app built with Vite 8, TypeScript, HeroUI 3
(on react-aria), Tailwind 4, Zustand for the store and xterm.js for terminals; compiled with the React Compiler and
tested with Vitest (jsdom for components). The desktop shell loads its `dist/`, and the same build runs in a plain
browser. It talks to the daemon only over the WebSocket protocol in [`PROTOCOL.md`](../../apps/daemon/PROTOCOL.md) and
reaches anything native only through the platform adapter (`src/platform/`), chosen once at startup. Four more builds
share its code: the dev-only gallery of UI states, the fixture-backed website preview and focused feature views
(embedded by `apps/web/`), and the WKWebView latency benchmark.

Paths in this doc that do not start with `packages/` or `apps/` are relative to `packages/ui/`.

## Build entries and scripts

| Path | Role |
|---|---|
| `index.html`, `src/main.tsx` | The window's entry: startup (platform, daemon location, connection, theme, native window reveal, global error handlers) |
| `vite.config.ts`, `typography.mjs`, `tsconfig.json`, `tsconfig.app.json` | Vite and Vitest configuration; `tsconfig.app.json` is the production type check, which leaves out tests, `src/gallery/`, `src/fixtures/` and `src/website-preview/` |
| `gallery.html`, `gallery-frame.html` | Entries of the gallery |
| `website-preview.html`, `feature-preview.html`, `vite.preview.config.ts` | Entries and build of the website preview and the feature views (`pnpm build:preview` writes into `apps/web/public/preview/`) |
| `packages/ui/scripts/gallery-sweep.mjs` | `pnpm gallery:sweep`: loads gallery scenarios in a headless browser and fails on any gallery failure; `gallery-chip-geometry.mjs` and `gallery-report-focus.mjs` are standalone Playwright checks run by hand |
| `packages/ui/scripts/generate-licenses.mjs` | `pnpm licenses:generate`, the first step of `pnpm build`: writes the gitignored `src/about/licenses.generated.json` |
| [`packages/ui/scripts/wkbench/`](../../packages/ui/scripts/wkbench/README.md) | `pnpm bench:wkwebview`, the latency benchmark in the system WKWebView (a developer tool with its own usage guide) |
| `src/test/` | Vitest setup and the reporter listing failed test files for `pnpm verify` |

## Directory map of `src/`

### Root files

`App.tsx` (the screen: window chrome, content panel, toasts, connection banner and dialog layer composed from the hooks
in `app/`), `StartupScreen.tsx`, `ErrorBoundary.tsx`, `WelcomeScreen.tsx`; `daemon.ts` (locating the daemon from the
page's address), `daemon-client.ts` (the WebSocket client: request correlation, reconnect, event dispatch);
`protocol.ts` (hand-written TypeScript mirror of the wire protocol) checked against `protocol.generated.ts` (written by
the daemon's `protocol::bindings` test) by `protocol.contract.ts`; `appConfig.ts` (facts from `config/app.json`);
`style.css` (Tailwind theme and the design tokens).

### Pure domain layers (bottom)

- `src/platform/` — `PlatformAdapter`, the interface to everything native, made of optional capabilities: `tauri.ts`
  (the only place that imports `@tauri-apps/*`) and `browser.ts`, exposed by `react.tsx`.
- `src/preferences/` — the `localStorage` key of every persisted preference, the guarded store behind them, the theme.
- `src/input/` — shortcut modifier and key predicates, touch, IME and focus workarounds.
- `src/i18n/` — languages, the typed English catalog and translated ones (`messages/`), lookup, the React side; every
  user-facing string goes through it, and `languages.ts` is also the language list the website derives from.
- `src/format/` — small pure formatting helpers (paths, relative time, sizes, `Reason`).
- `src/agents/` — agent names, install pages, model and effort picker rules, per-agent launch settings.
- `src/git/` — git workflow settings and git-badge wording, repository host links.
- `src/sessions/` — pure session rules and wording (index, order, labels, colours, filtering, waiting).
- `src/accounts/` — pure account rules (order, quota of an account, picker rules, quota formatting).
- `src/errors/` — turning a failure into words: `codes.ts` (the one record per error code), daemon wording, the shared
  `ErrorPresentation`, recovery classification, issue-report address.
- `src/store/` — the Zustand store of the connection and the state derived from daemon events (state, reducer, actions,
  session requests, toasts, connection, React bindings, session-group hooks), re-exported by `index.ts`.

### Leaf layer and window shell

- `src/components/` — shared building blocks by kind: `alerts/` (error alerts and the error queue), `dialog/` (the
  `Dialog` frame and toolkit), `form/`, `controls/`, `menus/`, `hint/` (the one component behind tooltips and floating
  popovers), `text/`, `status/`, `icons/`, `tabs/`, `motion/`, `scroll/`, `focus/`, `announce/`, `panel/`, `host/`,
  `hooks/` (breakpoints, `createLauncher`).
- `src/shell/` — the window chrome: `TitleBar`, `Rail`, `NotificationsBell`, `ContentPanel`, `Toasts`,
  `ConnectionBanner`, `Scrim`, `ErrorAlerts` (the error acknowledgement host).
- `src/app/` — the hooks `App.tsx` is composed of, one concern each (dialog routing, console switching, session
  selection and actions, focus mode, archive view, pane layout and widths, project panels, announcements).

### Feature directories

- `src/layout/` — pane layout: panel kinds and widths, the resize handle, the sidebar drawer, swipes below `phone`.
- `src/shortcuts/` — the central registry of keyboard shortcuts and its persisted overrides.
- `src/navigation/` — Back and Forward through the locations the content area has shown.
- `src/lifecycle/` — hooks tying the UI to the platform: exit flow, notifications, native menu and menu bar icon, native
  window theme, run in background, git status schedule, interface language report, `useSerialPush`.
- `src/sidebar/` — one console's sessions and projects, focus modes, rows and menus, new-session controls.
- `src/archive/` — archived sessions over the terminal area, with deletion.
- `src/terminal/` — the xterm.js terminal: controller, pane, reconnect, path `links/`, project `shell/` terminals,
  archived-session `history/`.
- `src/report/` — a session's report panel and its sandboxed page frame.
- `src/panels/` — the right pane, a stack of inset panel cards, and the registry of panel kinds.
- `src/project-browser/` — the Files panel's content: `files/` (tree, listings, search) and `git/` (changes, branch
  comparison, commits).
- `src/viewer/` — the read-only file viewer modal; `@pierre/diffs` is imported only inside it.
- `src/quota/` — plan quota wording, meters, reset credits.
- `src/settings/` — the settings modal and its sections; `accounts/` the Agent accounts section.
- `src/dialogs/` — the feature dialogs (console, project, session, request, handoff, account switch, model, trust,
  archive and removal choices), on the `components/dialog/` toolkit.
- `src/process/` — the Processes panel and its data.
- `src/configuration/` — configuration export and import dialogs, and the list of plain switch settings.
- `src/about/`, `src/updates/` — the licenses dialog and its data; the UI side of self-update.

### Dev-only and website-facing

- [`src/fixtures/`](../../packages/ui/src/fixtures/README.md) — the fixtures runtime (fixture daemon, fake terminal,
  builders, step driver, sample data) shared by the gallery, the website preview and the application tests.
- [`src/gallery/`](../../packages/ui/src/gallery/README.md) — the UI state gallery: pages, the `Scenario` type and the
  scenarios under `fixtures/`; its README also holds the conventions for writing scenarios and how to sweep the gallery.
- `src/website-preview/` — the fixture-backed build the website embeds: the real `App` over a preview daemon, scenes,
  simulated agent terminals, `features/` focused views, and `messages.ts`, the `postMessage` contract with the embedding
  page that `apps/web/` imports too.

## Dependency direction

`no-restricted-imports` blocks in the root `eslint.config.mjs` enforce it (`pnpm lint`; tests and test-support files are
exempt). A layer imports only the directories and root files its block names, so the exact allowed set is in the config;
the shape is:

- Each pure layer imports only layers beneath it: `platform/` (and `protocol`) and `preferences/` import nothing else
  below `src/`; `input/` only the `platform/` barrel; `i18n/` only `preferences/` and `appConfig`; `format/` only
  `i18n/`; `agents/` and `git/` only `i18n/` and `protocol`; `sessions/` adds `agents/`; `accounts/` adds `sessions/`
  and `format/`; `errors/` adds `accounts/`, `appConfig` and `daemon-client`; `store/` imports `i18n/`, `errors/`,
  `accounts/`, `sessions/`, `preferences/`, `git/`, `protocol`, `daemon` and `daemon-client`.
- `components/` is the leaf layer: outside itself it imports only the pure layers, the `store/` barrel and the shared
  root files. `shortcuts/` imports `i18n/`, `preferences/`, `input/` and `components/`.
- `viewer/`, `about/`, `updates/`, `layout/` and `quota/` import the leaf layer and the pure layers (`quota/` also
  `accounts/`) and no other feature directory; `project-browser/` additionally imports `viewer/`. The remaining feature
  directories import one another freely.
- From below `src/`, the root files that may be imported are `protocol`, `daemon`, `daemon-client`, `appConfig` and the
  `store/` and `platform/` barrels. The entry, `App` and the screens are imported by nothing below them.
- `shell/` is imported only by root files, `app/`, the gallery and the website preview; `app/` only by `App.tsx`.
- Production code imports neither `fixtures/` nor `gallery/`; `fixtures/` and `website-preview/` never import
  `gallery/`; `fixtures/` does not import `shell/`.
- HeroUI's `Alert` is imported only by `components/alerts/AlertFrame.tsx` and `ErrorAlert.tsx` (lint), so every alert
  has a title.

Other checks that hold structure: `src/errorModulesCycle.test.ts` (the error modules do not import each other in a
cycle), `src/protocol.contract.ts` (type check) with `src/protocol.contract.test.ts` (every generated protocol type is
asserted), `src/daemonCodes.test.ts` (the registry in `errors/codes.ts` matches the codes the daemon declares and the
catalogs), `src/i18n/catalog.test.ts` (placeholders and plural categories of every catalog), `src/appConfig.test.ts`
(copies of `config/app.json` elsewhere), `src/cssDependencies.test.ts` (every package a stylesheet names is a production
dependency).

## Where new code goes

- A screen-level concern of `App` is a hook in `app/`; the window chrome is in `shell/`; a new feature is a feature
  directory importing the leaf and pure layers (and other features only where the rules above allow it).
- Pure rules and wording about sessions, accounts, agents, git or formatting go in the matching pure layer, not in a
  component; a shared building block goes in `components/<kind>/`.
- A user-facing string goes through `i18n/` with its key in every catalog (typed `Translation<...>`, so a missing
  message fails type checking).
- A new daemon error code needs its record in `errors/codes.ts` in the same change.
- A new protocol type is added to `protocol.ts` by hand; the contract check fails until it matches the generated types.
- A new UI state worth seeing is a scenario under `src/gallery/fixtures/`; data used by both the gallery and the website
  preview or application tests belongs in `src/fixtures/`.
- Tests sit beside the code (`*.test.ts(x)`); application tests take their data from `src/fixtures/`, not from gallery
  scenarios.

## Shared entry points

New code uses these rather than a second implementation. None is enforced by a check unless a check is named.

| Need | Entry point |
|---|---|
| Raise an error alert for a failure | `raiseError`, `alertFailure` (as a request's catch handler) and `errorAlertOf` in `src/components/alerts/raiseError.tsx`; for a failure about a session, `Daemon.raiseError` (`src/store/connection.ts`). Alerts are drawn through `ErrorAlert` and the two built on it (`FailureAlert`, `ChoiceWarning`) |
| What a daemon broadcast or failure shows as a toast | `broadcastToast` and `errorToast` in `src/store/toasts.ts` (used by the store and the fixture daemon) |
| Wording and handling of an error code | one record per code in `src/errors/codes.ts` (`CODES`) |
| A plain on/off application setting in Settings | `DaemonSwitchSetting` (`src/settings/DaemonSwitchSetting.tsx`) over the list in `src/configuration/booleanSettings.ts` |
| An xterm terminal view | `useTerminalController` and `useTerminalReconnect`, with `TerminalCover` and `TerminalNoticeBar` (`src/terminal/`), shared by the session pane and shell terminals |
| Poll the lists on screen | `useAutoRefresh` (`src/project-browser/autoRefresh.ts`); treat a failed browse request through `projectBrowseFailure` (`browseError.ts`) |
| Open a dialog from a control outside its window | `createLauncher` (`src/components/hooks/createLauncher.ts`) |
| Queue best-effort pushes to the native shell in order | `useSerialPush` (`src/lifecycle/useSerialPush.ts`) |
| Browse directories in a picker | `useDirectoryBrowse` with `DirectoryPicker` (`src/components/dialog/`) |
| Delete options of a dialog that deletes sessions | `useSessionDeletionOptions` (`src/dialogs/`) |
| Which accounts a quota list shows | `listedQuotaAccounts` (`src/quota/quotaListing.ts`) |
| Platform capabilities (native menu, updater, clipboard, permissions) | the `PlatformAdapter` through `src/platform/` |
| Read the store | the barrel `src/store/index.ts` (`useDaemon`, `useDaemonStore`, `useDaemonStoreEqual`, session hooks); below `src/` the barrel is the only store import allowed |
| Data for the gallery, the website preview and application tests | `src/fixtures/` (builders, `snapshotState`, `createFixtureDaemon`), never gallery scenarios |
