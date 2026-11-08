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
| `index.html`, `src/main.tsx` | The window's entry: startup (platform, daemon location, connection, native window reveal, global error handlers), mounting `App` under `AppRoot` |
| `package.json` `exports` | The files `apps/web/` imports by package name: `./agent-marks`, `./languages`, `./match-language`, `./preview-messages`; nothing else of the package is importable from outside |
| `eslint.layers.mjs` | The import-layering blocks and the rule that confines the third-party UI libraries to `src/primitives/` (see "Dependency direction"), spread into the root `eslint.config.mjs` |
| `vite.config.ts`, `typography.mjs`, `tsconfig.json`, `tsconfig.app.json` | Vite and Vitest configuration; `tsconfig.app.json` is the production type check, which leaves out tests, `src/gallery/`, `src/fixtures/` and `src/website-preview/` |
| `gallery.html`, `gallery-frame.html` | Entries of the gallery |
| `website-preview.html`, `feature-preview.html`, `vite.preview.config.ts` | Entries and build of the website preview and the feature views (`pnpm build:preview` writes into `dist-preview/`; the site's `build-preview` passes `--outDir`) |
| `packages/ui/scripts/gallery-sweep.mjs`, `gallery-compare.mjs` | `pnpm gallery:sweep` checks scenario rendering; the manual `pnpm gallery:compare` writes screenshot difference reports from two served galleries. Both use `gallery-browser.mjs` for browser resolution; comparison and report helpers live in `gallery-compare-report.mjs`. Usage is in the [gallery README](../../packages/ui/src/gallery/README.md). `gallery-chip-geometry.mjs` and `gallery-report-focus.mjs` are standalone Playwright checks run by hand |
| `packages/ui/scripts/compiler-bailouts.mjs` | `pnpm compiler:bailouts`: lists the components and hooks the React Compiler leaves uncompiled, and exits 1 when there is any (run by hand; not part of `pnpm verify`) |
| `packages/ui/scripts/generate-licenses.mjs` | `pnpm licenses:generate`, the first step of `pnpm build`: writes the gitignored `src/about/licenses.generated.json` |
| [`packages/ui/scripts/wkbench/`](../../packages/ui/scripts/wkbench/README.md) | `pnpm bench:wkwebview`, the latency benchmark in the system WKWebView (a developer tool with its own usage guide) |
| `src/test/` | Vitest setup, the reporter listing failed test files for `pnpm verify`, and the shared test daemon (`testDaemon`, with `withEmptyReplies` to turn a stand-in's `ack` into the empty reply of the requests that have one), its providers (`TestHost`) and `mountHost`, which renders under them; `hosts.ts`, the `HostFacts` of a macOS and a Windows desktop shell (`MAC_HOST`, `WINDOWS_HOST`) for tests |

## Directory map of `src/`

### Root files

`App.tsx` (the screen: window chrome, content panel, toasts, connection banner and dialog layer composed from the hooks
in `app/`), `AppRoot.tsx` (the provider tree around `App` — platform, language, error boundary, theme, daemon, native
sync hooks, updates — shared by `main.tsx`, the gallery frame and the website preview), `StartupScreen.tsx`,
`ErrorBoundary.tsx`, `WelcomeScreen.tsx`; `daemon.ts` (locating the daemon from the
page's address), `daemon-client.ts` (the WebSocket client: request correlation, reconnect, event dispatch, and the `hello` check that ends in the final connection state `incompatible` when the daemon speaks another protocol generation);
`requests.ts` (the request-to-reply contract: `REPLY_TYPES` names the reply each request is answered with, `ReplyOf`
and `RequestSender` type a request's result from it, and `looseRequestSender` adapts a stand-in for the daemon);
`protocol.ts` (the one door to the wire types: it re-exports `protocol.generated.ts`, which the daemon's
`protocol::bindings` test writes from the Rust types, and adds the unions `ClientRequest` and `ClientRequestBody` a
client sends with; no UI rule lives in it); `appConfig.ts` (facts from `config/app.json`);
`style.css` (the shared stylesheet entry: Tailwind configuration, shared utilities and ordered stylesheet imports).

### Stylesheets

`src/styles/` holds cross-component definitions: `layout.css` for shared geometry and spacing tokens,
`surfaces.css` for window surfaces and control fills, and `palette.css` for semantic colours and shared tints.
Wrapper styles live beside their wrappers in `src/primitives/`; styles specific to a composite or feature live
beside their owner elsewhere in `src/`.

`src/style.css` loads those files through ordered CSS imports. The application, gallery and both website-preview
entries all load that entry before rendering, so component styles are available before a lazy component mounts and
their cascade order does not depend on which screen opened first.

### Wrapper layer (bottom)

- [`src/primitives/`](../../packages/ui/src/primitives/README.md) — the wrapper layer, the one importer of HeroUI and React Aria in application source: one project
  wrapper per third-party component, the project hooks over React Aria's and the utilities it re-exports, all from its
  `index.ts`. It imports nothing else of the application, and every other directory may import it.

### Pure domain layers

- `src/values/` — `sameValue`, the structural equality the store keeps unchanged data by; imports nothing.
- `src/platform/` — `PlatformAdapter`, the interface to everything native, made of optional capabilities: `tauri.ts`
  (the only place that imports `@tauri-apps/*`) and `browser.ts`, exposed by `react.tsx`; `host.ts` the `HostFacts`
  every adapter carries as `host` (the system, the shortcut modifier, the window chrome and which native features exist,
  read from the window's `?host=` query under the shell and derived from the browser's platform otherwise);
  `useAppVersion.ts` the hook that asks the shell for the running application's version.
- `src/preferences/` — the `localStorage` key of every persisted preference, the guarded store behind them, the theme.
- `src/input/` — shortcut modifier and key predicates, touch, IME and focus workarounds.
- `src/i18n/` — languages, the typed English catalog and translated ones (`messages/`), lookup, the React side; every
  user-facing string goes through it, and `languages.ts` is also the language list the website derives from. The
  system's own words in a message (`{trash}`, `{systemSettings}`, `{hostSettings}`, `{settingsApp}`, `{menuBarIcon}`,
  `{developerMode}`, and the vocabulary words a translation alone may ask for, such as Korean's `{appWindow}`) and
  `{osName}` are global params filled where messages are formatted, from the host `AppRoot` announces (`announceHost`;
  macOS's until one is), except the words for what happens on the daemon's machine: `{trash}` and `{hostSettings}` (the
  settings app a setting there is changed in), and a daemon error's `{osName}`, come from
  the daemon's host (`snapshot.host`), which `DaemonHostAnnouncement` (`store/`) announces (`announceDaemonHost`; the
  client's own until a snapshot names it) and `useT` and `Message` re-render on: `platformTerms.ts`
  holds the per-language `PLATFORM_TERMS` table selected by `host.terms`, with forms (`{trashTo}`, `{trashFrom}`,
  `{systemSettingsAcc}`) where a language inflects the word and a capitalised name (`{MenuBarIcon}`) where a message starts
  with it. A caller never passes them; `catalog.test.ts` checks every one is filled in every language for every host.
- `src/format/` — small pure formatting helpers (relative time, sizes, `Reason`, wire-path display) and `hostPath`, the one module that reads and shows the daemon host's absolute paths by its `path_style` (`isAbsolute`, `split`, `basename`, `parent`, `display` with the `~` rule that applies on Unix hosts only, `isPathLike` for terminal links, home expansion); every such rule takes the style as an argument, taken from `snapshot.host` by `useHostPaths()` (`components/hooks/`), and defaults to `unix` before the first snapshot. Project-relative wire paths stay `/`-separated and never go through it.
- `src/providers/` — one presentation module per provider (`claude.ts`, `codex.ts`, `grok.ts`: mark, quota colour and the `provider.<id>.*` catalog messages, among them the trust, sharing and directory-hint wording that `types.ts`'s `ProviderMessageName` names; each also owns the copy of every setting its descriptor declares (`provider.<id>.setting.<setting>.*`) and the record and wording of every code the descriptor declares (`codes`, `provider.<id>.<code>`, whose recovery may be the module's own `provider.<id>.<code>.remedy` copy); `planNames` for plan ids whose generic wording reads badly and `params`, the product-name placeholders its messages use, which `i18n/catalog.ts` merges into the catalogs' global params as `PROVIDER_PARAMS`), `registry.ts` listing them (the built-in three plus a test-only `testAgent` module registered under the test runner, one line each) and looking them up by provider id (`genericMark.ts` is the mark of a provider with no module), and `agentMarks.ts`, the `@octoboard/ui/agent-marks` entry. Imports nothing; the catalogs and the website import it.
- `src/agents/` — agent rules (`agents.ts`: the pickable, listed and served agents, always taken from the providers the daemon serves in `snapshot`, in its registry order), provider names and facts read from the module or from the providers the daemon serves in `snapshot` (`providers.ts`, which also reads the served model display rule and capabilities; the store holds them as `State.providers`, read with `useProviders()`; `ProviderMessage.tsx` shows a provider's message with a generic fallback), model and effort picker rules, per-agent launch settings; `providers.ts` also holds `settingMessageKey` (the copy key of a provider setting), `idWords` (an id read as words, for a setting or choice a module has no copy for) and re-exports the providers' code records as `PROVIDER_CODES`.
- `src/git/` — git workflow settings and git-badge wording, repository host links.
- `src/sessions/` — pure session rules and wording (index, order — with the scope of an archive deletion, `archivedDeletionScope` — labels, status derivation — `sessionStatus.ts` holds the live and no-process status sets and `sessionIsWorking`, the predicate for a session that is doing or about to do work — colours, filtering, waiting).
- `src/accounts/` — pure account rules: order, quota of an account, quota formatting, the default account's name
  (`defaultAccount.ts`), how an account is found among the
  records (`accountRecords.ts`), and the account picker split by concern (`choiceEntries.ts`, `accountInheritance.ts`,
  `accountRanking.ts`, `openingChoice.ts`, `switchEntries.ts`).
- `src/errors/` — turning a failure into words: `codedError.ts` (`CodedError`, the failure carrying a daemon or client
  code), `codes.ts` (the one record per error code, core codes and the providers' `PROVIDER_CODES` merged, with `Code` and `CODE`; `messageKey` maps a code to its catalog key), daemon wording (`daemonMessage.ts`, with
  `sharingFailure.ts` and `branchCleanupItems.ts` for the two items-bearing messages, and the folder rules the latter
  applies), `causes.ts` (parsers for the structured causes a daemon failure carries), the shared `ErrorPresentation`,
  recovery classification, issue-report address.
- `src/browse/` — browse requests to the daemon: `budget.ts` (the window's shares of the daemon's bound of outstanding
  browse requests, the shared slot names and the lanes every browse request goes through), the lane primitive
  (`lane.ts`), and how a failed browse request is treated and worded (`failure.ts`).
- `src/store/` — the Zustand store of the connection and the state derived from daemon events (state, reducer, actions,
  session requests, the notification bus in `notifications.ts`, connection, React bindings, session-group hooks),
  re-exported by `index.ts`.

### Leaf layer and window shell

- `src/components/` — shared building blocks by kind: `alerts/` (error alerts and the error queue), `dialog/` (the
  `Dialog` frame and toolkit), `form/`, `controls/`, `menus/`, `hint/` (the one component behind tooltips and floating
  popovers), `text/`, `status/`, `icons/`, `tabs/`, `motion/`, `scroll/` (the faded scroller and the sticky section
  heading), `focus/`, `announce/`, `panel/`, `host/`, `rows/` (the tree row, its icon and mark buttons, the
  hover-actions class), `hooks/` (breakpoints, `createLauncher`, `retryableLazy`, lazy readiness for capture hosts,
  polling, window focus (`windowFocus.ts`), the latest-request hook,
  `useLatest` and `useRenderLatest`, `useSerialPush` over `createSerialQueue` (`serialQueue.ts`), `settle`, a `try`/`finally` helper kept outside components because
  the React Compiler skips a function holding one).
- `src/shell/` — the window chrome: `TitleBar`, `Rail` (with its quota and global-processes entries, `RailQuotaEntry`
  with its panels in `src/quota/`, and `RailProcessesEntry`, both on the shared popover wiring of `RailPanelEntry`),
  `NotificationsBell`, `ContentPanel`, `Toasts`, `ConnectionBanner`, `Scrim`, `ErrorAlerts` (the error acknowledgement
  host), `IncompatibleDaemonScreen` (the whole window while the connection state is `incompatible`).
- `src/app/` — the hooks `App.tsx` is composed of, one concern each (dialog routing, console switching, session
  selection and actions, focus mode, archive view, pane layout and widths, project panels, jumps from the global process
  list to a Processes panel, announcements).

### Feature directories

- `src/layout/` — pane layout: panel kinds and widths, the resize handle, the sidebar drawer, swipes below `phone`.
- `src/shortcuts/` — the central registry of keyboard shortcuts and its persisted overrides.
- `src/navigation/` — Back and Forward through the locations the content area has shown.
- `src/lifecycle/` — hooks tying the UI to the platform: exit flow, notifications, native menu, native
  window theme, the two settings the desktop shell holds in its own file, run in background and menu bar icon visibility (`shellSettings.ts`: the store behind both preferences, started from the window's `settings` query, pushing changes through the platform capabilities, `connectShellSettings` called in `main.tsx`), keep awake (`keepAwake.ts` decides
  whether to hold from the setting and the sessions, `useKeepAwakeSync.ts` hands it to the shell), whether a system-initiated
  quit needs no confirmation (`useSystemQuitSync.ts`), git status schedule, interface language report.
- `src/statusItem/` — the menu-bar menu: its builder, quota submenus, icons and the hook that pushes it to the platform.
- `src/sidebar/` — one console's sessions and projects: `Sidebar` picks the view over `SidebarFrame`, `ConsoleView` and
  `FocusView` list them with `ProjectNode` and `SessionRow` (their pin and lock buttons in `MarkButtons.tsx`, the row's
  derived state in `sessionRowModel.ts`), plus focus modes, menus and new-session controls.
- `src/archive/` — archived sessions over the terminal area, with deletion.
- `src/terminal/` — the xterm.js terminal: controller and the surface it draws on, pane, reconnect, the reading of its
  output as text, path `links/`, project `shell/` terminals (with their tabs), archived-session `history/`;
  xterm's private members are reached from two adapters only: `xtermInternals.ts` (`watchUserInput`, the mark that
  tells the user's input from xterm's own replies) and `links/terminalLinks.ts` (OSC 8 link data).
- `src/report/` — a session's report panel and its sandboxed page frame.
- `src/panels/` — the right pane, a stack of inset panel cards, and the registry of panel kinds.
- `src/project-browser/` — the Files panel's content: `files/` (tree, listings, search, and the linked-file viewer
  `App` opens for a path a terminal names) and `git/` (changes, branch comparison, commits, following a session's
  branch); at its root, the browser's per-project state (`browserState.ts` over saved preferences and window-lifetime
  selections) and the list helpers the two modes share.
- `src/viewer/` — the read-only file viewer modal (the shell at its root), and the app's Markdown and code rendering
  library, which `terminal/history/` uses too: `markdown/` (documents, sanitized HTML, images, Mermaid, formulas, fenced code), `code/` (the
  code and diff renderer and its budgets) and `find/` (the find bar); `@pierre/diffs` is imported only inside it.
- `src/quota/` — plan quota wording, meters, reset credits, and the rail's quota panels (`RailQuotaBars`, `QuotaPanel`,
  `AgentPanel`, `QuotaAccount`).
- `src/settings/` — the settings modal and its sections (`sections.ts` holds the section ids and `SettingsNavigation`);
  `AppBehaviorSection.tsx` the App behavior section (startup, window, power and terminals groups, each shown only where the
  platform has the capability; `LaunchAtLoginSetting.tsx`, `MenuBarIconSetting.tsx`, `KeepAwakeSetting.tsx` and
  `BackgroundRunSetting.tsx` are its native-backed rows), `CleanupSection.tsx` the Cleanup section;
  `accounts/` the Agent accounts section, where `AgentProviderSettings.tsx` renders every setting the daemon serves for an agent,
  `AgentMenus.tsx` holds an agent tab's menu and actions button, which open `AgentSettingsDialog.tsx` (the per-agent
  settings) and `AgentVersionDialog.tsx` (the agent's CLI version, its content `AgentCliVersionSetting.tsx`, with
  `cliVersion.ts` judging whether an update exists), both on `agentDialogSubject.tsx` (the heading and the closing of
  a dialog whose agent has gone), and `AgentCliUpdateAlerts.tsx`, mounted in `App.tsx` for the window's life, raises the
  alert of an update this window started that ended unfinished.
- `src/dialogs/` — the feature dialogs (console, project, session, request, handoff, account switch, model, trust,
  archive and removal choices), on the `components/dialog/` toolkit. `RequestedDialog.tsx` only routes a requested
  dialog to its component; the archive, removal and restore confirmations are in `confirmations/`. The dialogs' daemon
  lookups (detected agent, located conversation, agent models, session records, branch inspections) are built on
  `useLatestRequest`; `branchInspection.ts` holds the pure part of a branch inspection and `useBranchInspections.ts`
  its hooks.
- `src/process/` — the Processes panel and its data, the polled global process count the rail shows
  (`processPolling.ts` holds the polling paces), and the rail's process panel (`ProcessCounts`).
- `src/configuration/` — configuration export and import dialogs (with the cards and parts they share, `SettingsGroups.tsx`,
  `AgentSettingsCard.tsx` and `importParts.tsx`; `clientSettings.ts` carries the settings only this client holds (appearance, Markdown style, run in the background, menu bar icon, keep awake, language) into and out of the document; the review and result show one line per distinct provider-setting heading, from the served schema and the providers' copy; `ImportProgress.tsx` shows a running import item by item, its state kept in `progressState.ts`, with `ResultCards.tsx` shared by progress and result), and the list of plain switch settings (`booleanSettings.ts`, grouped by the Settings section each belongs to).
- `src/about/`, `src/updates/` — the licenses dialog and its data; the UI side of self-update.

### Dev-only and website-facing

- [`src/fixtures/`](../../packages/ui/src/fixtures/README.md) — the fixtures runtime (fixture daemon with its per-domain reply modules, fake terminal,
  builders, step driver, sample data) shared by the gallery, the website preview and the application tests.
- [`src/gallery/`](../../packages/ui/src/gallery/README.md) — the UI state gallery: pages, the `Scenario` type and the
  scenarios under `fixtures/`; its README also holds the conventions for writing scenarios, sweeping the gallery and
  manually comparing screenshots.
- `src/website-preview/` — the fixture-backed build the website embeds: the real `App` over a preview daemon, scenes,
  simulated agent terminals, `features/` focused views, and `messages.ts`, the `postMessage` contract with the embedding
  page that `apps/web/` imports too (through the `./preview-messages` export).

## Dependency direction

`no-restricted-imports` blocks in `eslint.layers.mjs`, spread into the root `eslint.config.mjs`, enforce it (`pnpm lint`;
tests and test-support files are exempt). A layer imports only the directories and root files its block names, so the exact allowed set is in the config;
the shape is:

- `primitives/` imports nothing else below `src/`, and every other layer may import its barrel (`primitives`, never a
  file inside it), on top of what its own rule below allows.
- `protocol` imports only `protocol.generated`, and nothing but `protocol` imports that file (`GENERATED_PROTOCOL` in
  `eslint.layers.mjs`; the rule sees import declarations, not `import("…")` type expressions);
  `values/`, `providers/` and `preferences/` import nothing else below `src/`; `platform/` imports only `protocol`. Each other pure layer imports only layers beneath it: `input/` only the `platform/` barrel; `i18n/` only
  `preferences/`, `providers/` and `appConfig`; `format/` only `i18n/`; `agents/` only `i18n/`, `providers/` and `protocol`; `git/` only `i18n/` and `protocol`; `sessions/`
  adds `agents/`; `accounts/` adds `sessions/` and `format/`; `errors/` adds `accounts/` and `appConfig`; `browse/`
  imports `i18n/`, `errors/`, `format/` and `protocol`; `store/` imports `values/`, `i18n/`, `agents/`, `errors/`, `accounts/`,
  `sessions/`, `preferences/`, `git/`, `protocol`, `requests`, `daemon` and `daemon-client`. `errors/` and `browse/`
  do not import `daemon-client`: its `CodedError` lives in `errors/codedError.ts`, and the codes it raises are records in
  `errors/codes.ts`.
- `components/` is the leaf layer: outside itself it imports only the pure layers, the `store/` barrel and the shared
  root files. `shortcuts/` imports `i18n/`, `platform/`, `preferences/`, `input/` and `components/`.
- `viewer/`, `about/`, `updates/`, `layout/`, `quota/` and `statusItem/` import the leaf layer and the pure layers (`quota/` also
  `accounts/`; `statusItem/` also `accounts/` and `quota/`) and no other feature directory; `project-browser/` additionally imports `viewer/`. The remaining feature
  directories import one another freely.
- From below `src/`, the root files that may be imported are `protocol`, `requests`, `daemon`, `daemon-client`, `appConfig` and the
  `store/`, `platform/` and `primitives/` barrels. The entry, `App` and the screens are imported by nothing below them.
- `shell/` is imported only by root files, `app/`, the gallery and the website preview; `app/` only by `App.tsx`.
- Production code imports neither `fixtures/` nor `gallery/`; `fixtures/` and `website-preview/` never import
  `gallery/`; `fixtures/` does not import `shell/`.
- `@heroui/react`, `@heroui/styles`, `react-aria-components`, `react-aria` and the `@react-aria/*`, `@react-stately/*`
  and `@react-types/*` packages are imported only by `primitives/` in application source; tests, test support,
  `gallery/`, `fixtures/` and `website-preview/` are exempt (`WRAPPED_LIBRARIES` in `eslint.layers.mjs`, a
  typescript-eslint rule of its own so the layer blocks above do not replace it).
- The alert parts (`Alert` and the parts named after it, from `primitives` or HeroUI) are imported only by
  `components/alerts/AlertFrame.tsx` and `ErrorAlert.tsx` (lint), so every alert
  has a title.

Other checks that hold structure: `src/errorModulesCycle.test.ts` (the error modules do not import each other in a
cycle), `src/daemonCodes.test.ts` (the registry in `errors/codes.ts` matches the codes the daemon declares, the providers' included, and the
catalogs), `src/i18n/catalog.test.ts` (placeholders and plural categories of every catalog, the platform words for every host), `src/appConfig.test.ts`
(copies of `config/app.json` elsewhere), `src/cssDependencies.test.ts` (every package a stylesheet names is a production
dependency).

## Where new code goes

- A third-party UI component, hook or utility the application needs and `src/primitives/` does not export yet gets its
  wrapper there first, in the same change, and is used from the barrel; application code never imports the libraries
  (see the layer's README).
- A common correction or approved extension of a wrapped control belongs in that wrapper's adjacent stylesheet in
  `src/primitives/`. A composite or feature's own layout and visual treatment belongs beside that owner. Shared
  theme and geometry definitions belong in `src/styles/`; register owned stylesheets in `src/style.css` rather than
  loading them from component modules.
- A screen-level concern of `App` is a hook in `app/`; the window chrome is in `shell/`; a new feature is a feature
  directory importing the leaf and pure layers (and other features only where the rules above allow it).
- Pure rules and wording about sessions, accounts, agents, git or formatting go in the matching pure layer, not in a
  component; a shared building block goes in `components/<kind>/`.
- What is specific to one provider lives in the provider: its presentation module (`src/providers/<id>.ts`) for its mark,
  colour and names, and the facts the daemon serves about it (names, install page, configuration directory, declared
  capabilities). Components and rules branch on those capabilities and read facts through `src/agents/providers.ts`,
  not on which provider it is. Quota window names (`QuotaNaming`, `src/accounts/quotaFormat.ts`) and the display of
  model ids (`modelDisplayName`, `src/sessions/sessionModel.ts`) follow the rules the daemon serves in each provider's
  descriptor; reset credits and the account sharing switch show only for a provider that declares the capability.
  A provider's settings are drawn from the schema the daemon serves (`ProviderInfo.settings`) by
  `AgentProviderSettings` (`src/settings/accounts/`), with their copy in the provider's presentation module; Settings
  does not show a setting whose provider has no heading copy for it, while the configuration import review still lists
  such a setting, worded from the served label and the setting's id (`configuration.import.settingUnnamed`). Text that
  names a provider sets its name apart through `ProviderName` (`src/components/text/`).
- A new provider: its presentation module `src/providers/<id>.ts` (mark, colour, its messages in every language, the
  record and wording of each code it declares, with the code's recovery copy `provider.<id>.<code>.remedy`, the copy of
  each of its settings, `planNames`, `params`), added to `BUILT_IN` in `src/providers/registry.ts`, whose keys type the
  catalogs. `providers/agentMarks.ts` and the website's marks are outside this bar, updated by hand when a provider is to
  be shown there. A provider the daemon serves with no module here is shown by its served label and a generic mark.
- A user-facing string goes through `i18n/` with its key in every catalog (typed `Translation<...>`, so a missing
  message fails type checking).
- A new daemon error code needs its record in `errors/codes.ts` in the same change; a code only one provider raises
  (`provider.<id>.<code>`) has its record and wording in that provider's presentation module instead.
- A new request names its reply in `REPLY_TYPES` in `requests.ts`; the type check fails until it does.
- A new wire type is added in the daemon's Rust types (`apps/daemon/src/protocol/`) and `protocol.generated.ts` is
  regenerated; it is never written by hand in the UI. What a client has to do with the wire in general is
  `apps/daemon/PROTOCOL.md`'s "What every client implements" section. A rule of the UI about a wire value (a predicate, a list of
  codes, a parser of a cause) goes in the layer that owns it, not in `protocol.ts`.
- A new UI state worth seeing is a scenario under `src/gallery/fixtures/`; data used by both the gallery and the website
  preview or application tests belongs in `src/fixtures/`.
- Tests sit beside the code (`*.test.ts(x)`); application tests take their data from `src/fixtures/`, not from gallery
  scenarios.

## Shared entry points

New code uses these rather than a second implementation. None is enforced by a check unless a check is named.

| Need | Entry point |
|---|---|
| Raise an error alert for a failure | `raiseError`, or `alertFailure` as a request's catch handler, in `src/components/alerts/raiseError.tsx`, the one route, which takes the id of the session a failure is about (`session`); `errorAlertOf` there turns a daemon's error broadcast into the same alert. The alert names the session from the store as it shows (`AlertSessionLines`). Alerts are drawn through `ErrorAlert` and the two built on it (`FailureAlert`, `ChoiceWarning`) |
| What a daemon broadcast or failure shows as a notification (a toast, or an error alert to acknowledge) | `broadcastNotification` in `src/store/notifications.ts`, which the daemon's broadcasts and `Daemon.showNotice` feed to the window through `onNotification` (used by the store and the fixture daemon); `errorNotification` there is the request `raiseError` builds |
| The button a recovery offers besides its words (Settings, Change model, or a link out such as Download Git) | `recoveryAction` and its `RecoveryAction` union in `src/components/alerts/ErrorGuidance.tsx`; a link is drawn by `RecoveryLink` among the actions of the alert footer or popover, and a pane failure takes it as `PaneFailure`'s `linkAction` (`src/components/alerts/PaneFailure.tsx`) |
| Speak the outcome of a retry the user asked for | answer the request with its outcome and announce from that, not from broadcast status changes; write it through `AttendedStatusAnnouncer` (`src/components/announce/`), mounted beside a row's `role="button"`, not inside it, as `useGitRetryAnnouncement` does for the Git-missing mark (`src/sidebar/useGitRetryAnnouncement.tsx`) |
| Wording and handling of an error code | one record per code in `src/errors/codes.ts` (`CODES`; `Code` and `CODE` name a code, `variantOf` on a record picks the message variant) |
| A request to the daemon, and the type of its reply | `Daemon.request` (`src/store/connection.ts`) typed by `REPLY_TYPES` in `src/requests.ts`; a reply of another type rejects with the client code `unexpected_reply` (checked in `DaemonClient.request`, which `Daemon.request` wraps). A stand-in for the daemon (fixture daemon, preview, `testDaemon`) wraps its handler in `looseRequestSender`, which does not check the reply type |
| A failure carrying a daemon or client code | `CodedError` (`src/errors/codedError.ts`) |
| Whether a session is working (the set keep awake follows, and the quit confirmation lists without the sessions waiting for the user) | `sessionIsWorking` (`src/sessions/sessionStatus.ts`) |
| Whether an account switch holds a session, the daemon's or one this window queued | `holdsAccountSwitch` (`src/sessions/sessionStatus.ts`), or `useAccountSwitchHeld` in a component; never `Session.switching_account` alone, as the store keeps the daemon's record as sent |
| Keep unchanged data by identity when a reducer rebuilds it | `sameValue` (`src/values/sameValue.ts`), as the `settings_updated` reducer does |
| A tree row in a list, with its icon and mark buttons | `TreeRow`, `RowIconButton`, `RowMarkButton` and `HOVER_ACTIONS_CLASS` in `src/components/rows/`; a list that animates its reordering uses `useListMotion` (`src/components/motion/`), not a row-level FLIP |
| A section heading that pins while its section scrolls (sidebar row groups, Settings section groups, a side pane's categories with a note) | `StickyHeading` with `variant` `list`, `group` or `category`, and `STICKY_HEADING_HEIGHT` for the scroller that pins it (`src/components/scroll/StickyHeading.tsx`); a plain section heading over a scroller goes through it, as `SettingsGroup` and the Processes panel do. The exception is the virtualized Git change list, whose foldable section headings are positioned by the virtualizer (`ChangeSectionHeading`, `src/project-browser/git/changeSectionHeading.tsx`). A pinned row or strip that is not a heading (the `DirectoryPicker`'s parent-directory row, the agent strip in `src/settings/accounts/AgentAccountsSection.tsx`) calls `useStickyHeadingFocus` (`src/components/scroll/stickyHeadingFocus.ts`) directly |
| A strip of text tabs | the tab wrappers in `src/primitives/Tabs.tsx`, with `fit="content"` on `TabListContainer` and `TabList`, shared by every such strip (`TextTabStrip`, `RegionTabs`, `AgentTabList`, the console launch fields, the project browser); `Tab size="content"` is the tab for every strip, `size="compact"` only for a strip in a pane header beside the pane's title, as the project browser's Files / Changes tabs |
| The frame of the sidebar's console and focus headers and the archive header | `PANE_HEADER_FRAME_CLASS` (`src/components/panel/paneHeaderFrame.ts`) |
| A previous / next pager button | `PagerButton` (`src/components/controls/PagerButton.tsx`) |
| A block of code shown as text | `CodeBlock`, or `SurfaceCodeBlock` for a wrapping block on its own surface (`src/components/text/CodeBlock.tsx`); a flush `CodeBlock` built on a surface of its own, such as inside a scroller as `DiagnosticBlock` does, puts `CODE_SURFACE_CLASS` (the classes of the secondary `Surface` it sits on) on that surface |
| Open an external address with the platform adapter and shared failure handling | `ExternalLink` (`src/components/text/ExternalLink.tsx`) for application links; `useExternalLinkAction` from the same module for document-owned link markup |
| A settings section's lead paragraph and the rows under it | `SettingsLead` and `LeadRows` (`src/settings/SettingsLead.tsx`) |
| An on/off switch in Settings that saves | `SettingSwitch` or `SwitchSettingRow` (`src/settings/SettingRow.tsx`); the `onChange` returns the save so the switch ignores presses until it settles |
| Pin and lock buttons of a session row; its derived state | `PinButton`, `LockButton` and `SessionMarkButtons` (`src/sidebar/MarkButtons.tsx`); `sessionRowModel` (`src/sessions/sessionRowModel.ts`) with the `useSessionRowModel` hook (`src/sidebar/`) |
| Resolve the selected session's focus context for an action or shortcut | `selectedSessionFocusTarget` (`src/sidebar/focus.ts`), shared by `useFocusMode` and session menus; it resolves ownership through `consoleSessionAbove` and checks target availability through `resolveFocus` |
| Let an open menu take an advertised window command | `ActionMenuItem.commandShortcut` (`src/components/menus/menuEntries.ts`) pairs its displayed binding with its matcher; `useWindowShortcut` (`src/shortcuts/`) delegates a matched, consumed command during a modal through `runMenuCommand` (`src/components/menus/menuCommands.ts`). `MenuItems` registers through `registerMenuCommand` and owns the active root menu's eligibility; components receive matchers from their callers and do not import `shortcuts/` |
| An account's record, login or key among the daemon's lists | `accountRecordOf`, `accountLoginOf`, `loginIsActive` and `accountKey` (`src/accounts/accountRecords.ts`) |
| Retry a focus move across frames until its target exists, or run after a few frames | `tryEachFrame` and `afterFrames` (`src/components/focus/`) |
| The Git workflow items a setting offers | `GIT_WORKFLOW_ITEMS` and `GIT_WORKFLOW_ITEM_KEYS` (`src/git/gitWorkflow.ts`) |
| A test that mounts a component under the app's providers | `mountHost` (`src/test/mountHost.tsx`) over `testDaemon` (the real store, requests rejected unless the test gives a handler); the container it returns has `render(ui)` to render again under the same host |
| A plain on/off application setting in Settings | `DaemonSwitchSetting` (`src/settings/DaemonSwitchSetting.tsx`) over the list in `src/configuration/booleanSettings.ts` |
| An xterm terminal view | `useTerminalController` and `useTerminalReconnect`, with `TerminalSurface`, `TerminalCover` and `TerminalNoticeBar` in its `TerminalNoticeSlot` (`src/terminal/`), shared by the session pane and shell terminals |
| The loading line of a panel | `LoadingState` (`src/components/panel/LoadingState.tsx`) |
| A failure's status mark with its Try again | `RetryStatusMark` (`src/components/status/RetryStatusMark.tsx`) |
| Read something again now and then: pace, a pause while the document is hidden with a catch-up on return, reads the caller sends itself | `usePoll` (`src/components/hooks/usePoll.ts`), on `startPolling`; the browser's lists poll at `AUTO_REFRESH_MS` (`src/project-browser/autoRefresh.ts`) |
| Ask the daemon for what a view shows and keep only the latest answer, scoped to what it is for, with the failure sorted out and a resend on reconnect | `useLatestRequest` (`src/components/hooks/useLatestRequest.ts`), as the project browser's lists and readers and the dialogs' lookups (`src/dialogs/`) do |
| Send a browse request | through a lane or a slot in `src/browse/budget.ts` (`listingLane`, `listLane`, `documentImageLane`, `BROWSE_SLOTS`), whose shares `budget.test.ts` keeps within the daemon's bound; a new kind of request takes its share from them |
| Treat a failed browse request | `browseFailure` and `projectBrowseFailure` (`src/browse/failure.ts`) |
| Open a dialog from a control outside its window | `createLauncher` (`src/components/hooks/createLauncher.ts`) |
| Open an App-level dialog (a `RequestedDialog`) from a control inside another dialog, so the dialog's lifetime is the window's and not the opener's | `openDialogFromAnywhere` (`src/dialogs/dialogLauncher.ts`), registered in `App.tsx` through `createLauncher`, as Settings → Configuration → Import does |
| Follow a running configuration import | `Daemon.onImportProgress` (`src/store/connection.ts`), which delivers the `configuration_import_progress` events of one import (named by the `import_id` the request sent) to its listener outside the store and reducer, through `createImportProgressListeners` (`src/store/importProgressListeners.ts`); the fixture daemon implements it too |
| A dialog whose work cannot be left while it runs | `Dialog`'s `busy` prop (`src/components/dialog/Dialog.tsx`), which sets `Modal`'s `busy` (`src/primitives/Modal.tsx`): Escape and an outside click are ignored, and every control in the dialog is disabled but a `pending` `Button`, which takes the focus from a control that busy disables and gives it back to the control it was taken from when the request is over (`useFocusLeavingWhenBusy`, `src/components/dialog/useFocusLeavingWhenBusy.ts`). The wrappers read the state through `useDisabled` (`src/primitives/busy.ts`), and code that draws a control of its own (a link) reads `useBusy`; a dialog does not disable its own controls for it |
| A decorative spinner beside a label that already says what runs | the layer's `Spinner` without a `label` (`src/primitives/Spinner.tsx`), which `Button` (while `pending`, which also sets `aria-busy`), `ExternalLink` and the import progress view use; never one with a `label` there, which is announced as a status |
| Load a component lazily, with a way to load it again after a failed import | `retryableLazy` (`src/components/hooks/retryableLazy.ts`), not `React.lazy`, which keeps the rejection |
| Observe visible Suspense content before a capture | `observeLazyReadiness` and `trackSuspenseFallback` (`src/components/hooks/lazyReadiness.tsx`); the gallery frame enables observation, while production hosts leave it disabled |
| Poll a daemon answer that is shown or counted (process trees, the global process counts), publishing only a changed one | `usePolled` (`src/components/hooks/usePoll.ts`), under `useProcessTrees` and `useAllProcesses` at the paces in `src/process/processPolling.ts`, rather than a timer of its own |
| A badge on the corner of a control that comes and goes | `BadgedControl` (`src/primitives/Badge.tsx`), which `ChromeButton`'s `badge` prop and `ActionMenu`'s `badge` prop (`src/components/menus/ActionMenu.tsx`) share |
| A count in a row, or on a rail control | `CountBadge` (`src/components/status/CountBadge.tsx`), reusing `StatusBadge`, in a row; `RailCountBadge` (`src/shell/RailCountBadge.tsx`) through `ChromeButton`'s `badge` prop on a rail control |
| A rail entry whose button opens a popover panel | `RailPanelEntry` (`src/shell/RailPanelEntry.tsx`), which wires the popover; the status marks and Details popovers the panel opens are tagged with `PopoverOwnerContext` (`src/components/hint/popoverOwner.ts`) and asked about through `nestedPopovers` (`src/shell/nestedPopover.ts`), and `CloseWhenCovered` (`src/shell/`) closes the panel under another modal |
| Put a panel of the aside's owner (a project, or a console session) on screen | `showPanel` from `useProjectPanels` (`src/app/`); a jump from the global process list is `useProcessJump` |
| Send focus into a panel the user opened deliberately | `landInPanel` (`src/panels/panelLanding.ts`), as `useFloatingPanel` and `useProcessJump` do |
| Queue best-effort pushes to the native shell in order | `useSerialPush` (`src/components/hooks/useSerialPush.ts`) in a component, `createSerialQueue` (`src/components/hooks/serialQueue.ts`) outside one |
| A setting the desktop shell must know before the page loads | `createShellPreference` (`src/lifecycle/shellSettings.ts`), not a `localStorage` preference |
| Browse directories in a picker | `useDirectoryBrowse` (`src/components/dialog/DirectoryPicker.tsx`), which routes local desktop browsing through the platform's `directoryPicker` capability and otherwise uses the daemon-backed `DirectoryPicker`; locality comes from `Daemon.isLocal`, derived by `isLocalDaemon` (`src/daemon.ts`) |
| Delete options of a dialog that deletes sessions | `useSessionDeletionOptions` (`src/dialogs/`) |
| A provider's name, install page or configuration directory; the file and folder names that detect it | `providerLabel`, `providerShortName`, `providerInstallUrl`, `providerConfigDir`, `detectionMarkers` (`src/agents/providers.ts`) over `useProviders()` |
| A provider's setting in Settings; its copy | `AgentProviderSettings` (`src/settings/accounts/AgentProviderSettings.tsx`) over the served `ProviderInfo.settings`; copy keys by `settingMessageKey` (`src/agents/providers.ts`), messages in the provider's module |
| A provider's name set apart in a sentence | `ProviderName` (`src/components/text/ProviderName.tsx`) |
| A provider's mark and colour | `providerMark`, `providerColour` (`src/providers/registry.ts`); `AgentIcon` (`src/components/icons/`) draws the mark, `providerFillVars` (`src/quota/providerFill.ts`) paints a quota bar |
| Which accounts a quota list shows | `listedQuotaAccounts` (`src/quota/quotaListing.ts`) |
| The tags offered to a tag field (projects and agents) | `tagVocabulary` (`src/components/form/tagVocabulary.ts`) with `TagsInput` |
| Platform capabilities (native menu, menu bar icon visibility, keep awake, launch at login, updater, clipboard, directory picker, permissions) | the `PlatformAdapter` through `src/platform/` |
| Follow the user leaving the window and coming back (whether the window has focus, a change of it, a return to it, a loss of it) | `windowHasFocus`, `subscribeWindowFocus`, `onWindowFocusReturn` and `onWindowFocusLoss` (`src/components/hooks/windowFocus.ts`), which follow the webview's own window `focus` and `blur` and `document.hasFocus()` in the desktop app as in a browser; they arrive only because `src/input/focusGuard.ts` swallows window-targeted focus events solely between a press's mousedown and the tick after the next frame, then replays a swallowed change. Users of the return: `usePoll`, `useMacosPermissions`, `useNotificationPermission`, `LaunchAtLoginSetting`, `ImportConfigurationDialog`; of the loss: `useQuickCreateModifier`, `tabDrag`, `AgentTabList`, `accountDrag`, `useNavigationHistory`; `useWindowAttended` uses the subscription and `windowHasFocus` |
| A behaviour or a word that differs by operating system | on the client's machine, `PlatformAdapter.host` (`HostFacts`, `src/platform/host.ts`), read with `usePlatform()` or `useHost()`; on the daemon's machine (its paths, its Trash), `snapshot.host` in the store (`useHostPaths()`, the daemon's words in `platformTerms.ts`); never `navigator`, and a pure function takes the host as an argument |
| The daemon's view of each agent's CLI version and update, and which updates this window started | `agentCliStatuses` and `cliUpdatesStartedHere` of the store state (`src/store/state.ts`; `trackAgentCliUpdate` in `src/store/actions.ts`); the Version dialog and `AgentCliUpdateAlerts` read them |
| Hand focus back to an agent tab's actions button | `tabActionsTrigger` (`src/components/tabs/AgentTabList.tsx`) |
| Read the store | the barrel `src/store/index.ts` (`useDaemon`, `useDaemonStore`, `useDaemonStoreEqual`, session hooks); below `src/` the barrel is the only store import allowed |
| The provider tree around `App` in a new host of the app | `AppRoot` (`src/AppRoot.tsx`), as `main.tsx`, the gallery frame and the website preview use it |
| Data for the gallery, the website preview and application tests | `src/fixtures/` (builders, `snapshotState`, `createFixtureDaemon`), never gallery scenarios |
