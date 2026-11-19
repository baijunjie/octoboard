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
| `index.html`, `src/main.tsx` | The window's entry: startup (platform, daemon location, connection, native window reveal, global error handlers), mounting `App` under `AppRoot`, or, for the flyout window kind, which has no daemon behind it (`windowKind`), `TrayFlyout`; the application's own window that failed to start gets an inert daemon (`createInertDaemon`) and shows the failure inside `App`'s frame |
| `package.json` `exports` | The files `apps/web/` imports by package name: `./agent-marks`, `./languages`, `./match-language`, `./preview-messages`; nothing else of the package is importable from outside |
| `eslint.layers.mjs` | The import-layering blocks and the rule that confines the third-party UI libraries to `src/primitives/` (see "Dependency direction"), spread into the root `eslint.config.mjs` |
| `vite.config.ts`, `typography.mjs`, `tsconfig.json`, `tsconfig.app.json` | Vite and Vitest configuration (it also defines `__OCTOBOARD_VERSION__`, the build's release version, read from `apps/desktop/package.json`); `tsconfig.app.json` is the production type check, which leaves out tests, `src/gallery/`, `src/fixtures/` and `src/website-preview/` |
| `gallery.html`, `gallery-frame.html` | Entries of the gallery |
| `website-preview.html`, `feature-preview.html`, `vite.preview.config.ts` | Entries and build of the website preview and the feature views (`pnpm build:preview` writes into `dist-preview/`; the site's `build-preview` passes `--outDir`) |
| `packages/ui/scripts/gallery-sweep.mjs`, `gallery-compare.mjs` | `pnpm gallery:sweep` checks scenario rendering; the manual `pnpm gallery:compare` writes screenshot difference reports from two served galleries. Both use `gallery-browser.mjs` for browser resolution; comparison and report helpers live in `gallery-compare-report.mjs`. Usage is in the [gallery README](../../packages/ui/src/gallery/README.md). `gallery-chip-geometry.mjs` and `gallery-report-focus.mjs` are standalone Playwright checks run by hand |
| `packages/ui/scripts/compiler-bailouts.mjs` | `pnpm compiler:bailouts`: lists the components and hooks the React Compiler leaves uncompiled, and exits 1 when there is any (run by hand; not part of `pnpm verify`) |
| `packages/ui/scripts/generate-licenses.mjs` | `pnpm licenses:generate`, the first step of `pnpm build`: writes the gitignored `src/about/licenses.generated.json` |
| [`packages/ui/scripts/wkbench/`](../../packages/ui/scripts/wkbench/README.md) | `pnpm bench:wkwebview`, the latency benchmark in the system WKWebView (a developer tool with its own usage guide) |
| `src/test/` | Vitest setup, the reporter listing failed test files for `pnpm verify`, and the shared test daemon (`testDaemon`, with `withEmptyReplies` to turn a stand-in's `ack` into the empty reply of the requests that have one), its providers (`TestHost`) and `mountHost`, which renders under them; `hosts.ts`, the `HostFacts` of a macOS and a Windows desktop shell (`MAC_HOST`, `WINDOWS_HOST`) for tests; `clock.ts`, the one fake-clock module; `dom.ts`, DOM helpers for tests (`type`, `typeInto`, which types the way a keyboard does so react-aria's autocomplete sees it, `inputLabelled`); `lazyImport.ts`, the settle of a lazily imported chunk; `daemonError.ts`, a faked request's failure with a daemon code; `alertQueue.ts`, the drain of the error-alert queue that `setup.ts` runs after every jsdom test |

## Directory map of `src/`

### Root files

`App.tsx` (the screen: window chrome, content panel, toasts, connection banner and dialog layer composed from the hooks
in `app/`), `AppRoot.tsx` (the provider tree around `App` — platform, language, error boundary, theme, daemon, native
sync hooks, updates — shared by `main.tsx`, the gallery frame and the website preview), `StartupScreen.tsx` (the window with no daemon connection: its message, drawn alone where `StartupFramed` says `App`'s own frame is there already),
`ErrorBoundary.tsx`, `WelcomeScreen.tsx`; `preferenceIdentity.ts` (imported first by `main.tsx`: hands the window's
daemon identity to `preferences/`, which imports nothing of the platform layer); `daemon.ts` (locating the daemon from
the page's address, whether an address is a loopback one, `isLocalDaemon`, marking a page the daemon itself served, `servedByDaemon`, recognising the production browser build opened with no key,
`isKeylessDaemonPage`, and the `/key-check` probe, `daemonRefusedKey`), `daemon-client.ts` (the WebSocket client: request correlation, reconnect, event dispatch, and the final connection states: `incompatible`, from the `hello` check, when the daemon speaks another protocol generation, and `keyRefused`, when the daemon that served the page answers its key check with a refusal);
`requests.ts` (the request-to-reply contract: `REPLY_TYPES` names the reply each request is answered with, `ReplyOf`
and `RequestSender` type a request's result from it, and `looseRequestSender` adapts a stand-in for the daemon);
`protocol.ts` (the one door to the wire types: it re-exports `protocol.generated.ts`, which the daemon's
`protocol::bindings` test writes from the Rust types, and adds the unions `ClientRequest` and `ClientRequestBody` a
client sends with; no UI rule lives in it); `appConfig.ts` (facts from `config/app.json`); `launchable.ts` (`isLaunchableName`, whether a name is one the system
launches rather than shows, from `config/launchable-extensions.json`);
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

- `src/values/` — `sameValue`, the structural equality the store keeps unchanged data by, and `knownValues`, the type
  predicate a reader of a daemon enum is built from (`knownValue.ts`); imports nothing.
- `src/platform/` — `PlatformAdapter`, the interface to everything native, made of optional capabilities: `tauri.ts`
  (the only place that imports `@tauri-apps/*`) and `browser.ts`, exposed by `react.tsx`; `host.ts` the `HostFacts`
  every adapter carries as `host` (the system, input facts, window chrome and native capabilities). The desktop shell
  supplies its facts in the window's `?host=` query. A browser derives the input facts it needs, including the xterm
  selection modifier and Android's virtual pointer convention;
  `useAppVersion.ts` the hook that asks the shell for the running application's version; `reloadPage.ts` the one way the
  page reloads itself (`reloadPage`, and `isReloading` for what must not run on a page already unloading);
  `windowKind.ts` which window of the desktop application the page is, as the shell puts it on the URL (`windowKind`:
  the local daemon's, a remote daemon's or the Windows tray flyout; every page
  outside the shell is a local one), with a remote window's daemon name and identity, read by everything that
  differs by window instead of the URL; `shellFailure.ts`, `linkForwardError.ts` and `remoteFileError.ts` the typed
  failures of the shell's commands for forwarding a remote port and copying a remote file (`LinkForwardError`,
  `RemoteFileError`), which callers word in their own language.
- `src/preferences/` — the `localStorage` key of every persisted preference, the guarded store behind them, the theme.
  A daemon-scoped preference is kept under the identity the entry hands in (`setPreferenceDaemonIdentity`).
- `src/input/` — shortcut modifier and key predicates, touch, IME and focus workarounds.
- `src/i18n/` — languages, the typed English catalog and translated ones (`messages/`), lookup, the React side
  (`InlineReference.tsx` for a reference in copy, `InlinePath.tsx` for a navigation path with `pathText`, its plain
  form); every user-facing string goes through it, and `languages.ts` is also the language list the website derives
  from. `language.ts` holds the window's language, a cache of the one language the daemon holds (kept in step by
  `lifecycle/useInterfaceLanguage.ts`); a window with no daemon (the tray flyout) follows the local daemon's windows
  through `followLocalLanguage`, and the tray menu reads it with `useLocalLanguage`. The system's own words in a message
  (`{trash}`, `{systemSettings}`, `{hostSettings}`, `{settingsApp}`, `{menuBarIcon}`, `{developerMode}`, and the
  vocabulary words a translation alone may ask for, such as Korean's `{appWindow}`) and `{osName}` are global params
  filled where messages are formatted, from the host `AppRoot` announces (`announceHost`; macOS's until one is), except
  the words for what happens on the daemon's machine: `{trash}` and `{hostSettings}` (the settings app a setting there
  is changed in), and a daemon error's `{osName}`, come from the daemon's host (`snapshot.host`), which
  `DaemonHostAnnouncement` (`store/`) announces (`announceDaemonHost`; the client's own until a snapshot names it) and
  `useT` and `Message` re-render on: `platformTerms.ts` holds the per-language `PLATFORM_TERMS` table selected by
  `host.terms`, with forms (`{trashTo}`, `{trashFrom}`, `{systemSettingsAcc}`) where a language inflects the word and a
  capitalised name (`{MenuBarIcon}`) where a message starts with it. A caller never passes them; `catalog.test.ts`
  checks every one is filled in every language for every host.
- `src/format/` — small pure formatting helpers (relative time, sizes, `Reason`, wire-path display) and `hostPath`, the one module that reads and shows the daemon host's absolute paths by its `path_style` (`isAbsolute`, `split`, `basename`, `parent`, `display` with the `~` rule that applies on Unix hosts only, `isPathLike` for terminal links, home expansion); every such rule takes the style as an argument, taken from `snapshot.host` by `useHostPaths()` (`components/hooks/`), and defaults to `unix` before the first snapshot. Project-relative wire paths stay `/`-separated and never go through it.
- `src/providers/` — one presentation module per provider (`claude.ts`, `codex.ts`, `grok.ts`: mark, quota colour and
  the `provider.<id>.*` catalog messages, among them the trust, sharing and directory-hint wording that `types.ts`'s
  `ProviderMessageName` names; each also owns the copy of every setting its descriptor declares
  (`provider.<id>.setting.<setting>.*`) and the record and wording of every code the descriptor declares (`codes`,
  `provider.<id>.<code>`, whose recovery may be the module's own `provider.<id>.<code>.remedy` copy, with literal
  placeholders supplied by `recoveryReferences` and marked by `ErrorGuidance`); `planNames` for plan ids whose generic
  wording reads badly and `params`, the product-name placeholders its messages use, which `i18n/catalog.ts` merges into
  the catalogs' global params as `PROVIDER_PARAMS`), `registry.ts` listing them (the built-in three plus a test-only
  `testAgent` module registered under the test runner, one line each) and looking them up by provider id
  (`genericMark.ts` is the mark of a provider with no module), and `agentMarks.ts`, the `@octoboard/ui/agent-marks`
  entry. Imports nothing; the catalogs and the website import it.
- `src/agents/` — agent rules (`agents.ts`: the pickable, listed and served agents, always taken from the providers the daemon serves in `snapshot`, in its registry order), provider names and facts read from the module or from the providers the daemon serves in `snapshot` (`providers.ts`, which also reads the served model display rule and capabilities; the store holds them as `State.providers`, read with `useProviders()`; `ProviderMessage.tsx` shows a provider's message with a generic fallback), model and effort picker rules, per-agent launch settings (`launchCompatibility.ts`: the compatibility switches an agent's served settings declare, and the helpers that set, diff and apply their held values); `providers.ts` also holds `settingMessageKey` (the copy key of a provider setting), `switchValue` (a switch's value in effect, with its declared default before settings arrive), `idWords` (an id read as words, for a setting or choice a module has no copy for) and re-exports the providers' code records as `PROVIDER_CODES`.
- `src/git/` — git workflow settings and git-badge wording, the wording of a manual pull's notice (`pullNotice.ts`), repository host links.
- `src/sessions/` — pure session rules and wording (index, order — with the scope of an archive deletion, `archivedDeletionScope` — labels, the session-order wording `orderWording.ts` shared by Settings and the configuration import, status derivation — `sessionStatus.ts` holds the live and no-process status sets and `sessionIsWorking`, the predicate for a session that is doing or about to do work, `sessionIsBusy` and `childCanReport`, the rest a handoff waits for in a session and in the sessions it owns — `nativeApp.ts`, why a session cannot be opened in its agent's desktop application and what to say when the application may not show it — colours, filtering, waiting).
- `src/accounts/` — pure account rules: order, quota of an account, quota formatting, the default account's name
  (`defaultAccount.ts`), how an account is found among the
  records (`accountRecords.ts`), and the account picker split by concern (`choiceEntries.ts`, `accountInheritance.ts`,
  `accountRanking.ts`, `openingChoice.ts`, `switchEntries.ts`).
- `src/errors/` — turning a failure into words: `codedError.ts` (`CodedError`, the failure carrying a daemon or client
  code), `codes.ts` (the one record per error code, core codes and the providers' `PROVIDER_CODES` merged, with `Code` and `CODE`; `messageKey` maps a code to its catalog key), daemon wording (`daemonMessage.ts`, with
  `sharingFailure.ts` and `branchCleanupItems.ts` for the two items-bearing messages, and the folder rules the latter
  applies), `causes.ts` (parsers for the structured causes a daemon failure carries), `writeConflict.ts` (the `base_versions` a
  checked write carries, and what a `write_conflict` names), the shared `ErrorPresentation`, recovery classification,
  issue-report address.
- `src/browse/` — browse requests to the daemon: `budget.ts` (the window's shares of the daemon's bound of outstanding
  browse requests, the shared slot names and the lanes every browse request goes through), the lane primitive
  (`lane.ts`), and how a failed browse request is treated and worded (`failure.ts`).
- `src/store/` — the Zustand store of the connection and the state derived from daemon events (state, reducer, actions,
  session requests, the notification bus in `notifications.ts`, `contentSearchListeners.ts`: the events of a running Find in files search, which `Daemon.onContentSearch` hands to its listener outside the store and reducer, the installed desktop applications of the agents (`nativeApps.ts`: `State.nativeApps`, `nativeAppsUnseen`, `useNativeAppsRefresh`), connection, React bindings, session-group hooks),
  re-exported by `index.ts`; `settingsWrite.ts` is the checked settings writer (`createSettingsWriter`) that
  `useSettingsWrite` and the daemon's update store share.

### Leaf layer and window shell

- `src/components/` — shared building blocks by kind: `alerts/` (error alerts and the error queue, with the failure of opening a file in the default application, `defaultApplicationFailure.tsx`, and `acknowledgementViewOf` in `acknowledgementView.ts`, the failure-modal view of an error for a dialog a refusal has ended), `card/` (`FlatCard`, the flat card of a dialog: header, band, and rows or blocks set apart by inset hairlines), `dialog/` (the
  `Dialog` frame and toolkit), `form/`, `controls/` (including `CopyBox`, a block's box with its copy button, `CopyCommand`, a command line with its copy button, and `FeedbackButton`, the done-state icon button that `CopyButton` and `PasteButton` are built on), `menus/`, `hint/` (including `PopoverHint`, the floating popover an
  interactive hint opens, in `hintPopover.tsx`, `HelpTip`, the "?" that carries an explanation in a `PopoverHint`, `HeadedTooltip`, the heading, divider and body content shared by the rail and the archive view, and `ValueTooltipText`, the text of a tooltip that is a value in full; the
  tooltip itself is `Tooltip` in `src/primitives/`), `text/`, `status/`, `icons/`, `tabs/`, `qr/` (`QrCode`, a QR code drawn from the modules the daemon encodes), `motion/`, `scroll/` (the faded scroller, `ScrollBox`, a box that scrolls sideways, and the sticky section
  heading), `focus/`, `announce/`, `panel/` (including `ContentMain`, the content area's one `<main>`), `host/` (the host actions on a path: copying it, opening it in the default application, `openPathInDefaultApplication`, and revealing it in the file manager, `revealPathTarget`, and opening a session in its agent's desktop application, `useOpenSessionInApp`), `rows/` (the tree row, its icon and mark buttons, the
  hover-actions class), `hooks/` (breakpoints, `createLauncher`, `retryableLazy`, lazy readiness for capture hosts,
  polling, window focus (`windowFocus.ts`), the latest-request hook,
  `useLatest` and `useRenderLatest`, `useSerialPush` over `createSerialQueue` (`serialQueue.ts`), `useCheckedEdit` (the
  checked-write side of a form dialog), `settle`, a `try`/`finally` helper kept outside components because the React
  Compiler skips a function holding one).
- `src/shell/` — the window chrome: `TitleBar`, `Rail` (with its quota and global-processes entries, `RailQuotaEntry`
  with its panels in `src/quota/`, and `RailProcessesEntry`, both on the shared popover wiring of `RailPanelEntry`),
  `RailUpdateEntry` (the rail's upgrade, download progress and restart control over `src/updates/`),
  `NotificationsBell`, `ContentPanel`, `Toasts`, `ConnectionBanner`, `Scrim`, `ErrorAlerts` (the error acknowledgement
  host), `IncompatibleDaemonScreen` (the whole window while the connection state is `incompatible`), `KeyRefusedScreen` (the whole
  window while it is `keyRefused`, and, as its keyless variant, `main.tsx`'s screen for a browser page opened with no key),
  `RailRemoteDevices` (the rail's Remote devices button, which opens the Remote devices page, under the
  consoles, in the desktop application's own window only; `Rail` offers it alone, with no consoles, while the local daemon cannot be used), `RailForwardsEntry` (a remote window's rail button for the
  ports forwarded to its daemon's machine, built on `RailPanelEntry`), `RailRemoteGap` and `RailSeparator` (the free space and
  the separator between the remote button and the rail's bottom group, and the separator and spacer items they are made of), and the page of the window that has no daemon: `TrayFlyout` with
  `TrayFlyoutLines.tsx` and `TrayFlyout.css` (the Windows notification-area flyout, drawing the model the shell sends).
- `src/app/` — the hooks `App.tsx` is composed of, one concern each (dialog routing, in `useDialogRouting.ts`, which
  also holds a dialog that arrives from the daemon until no modal or alert is open (`openDialogWhenFree`), console switching, session
  selection and actions, focus mode, Find in files' window shortcut (`useFindInFilesShortcut.ts`), archive view, pane layout and widths, project panels, jumps from the global process
  list to a Processes panel, announcements, and `useRemoteDevicesPage.ts`: whether the Remote devices page shows, which
  device it shows, and what leaves it).

### Feature directories

- `src/layout/` — pane layout: panel kinds and widths, the resize handle, the sidebar drawer, swipes below `phone`.
- `src/shortcuts/` — the central registry of keyboard shortcuts and its persisted overrides.
- `src/navigation/` — Back and Forward through the locations the content area has shown, the Remote devices page (`devices`) and the offline window's status (`offline`) among them.
- `src/lifecycle/` — hooks and parts tying the UI to the platform: exit flow (`useAppExit.ts`, which also
  restarts a daemon that updated itself where there is no shell to relaunch, `exitFailure.tsx` the alerts of a failed
  exit or restart, the refusal of a daemon restart coming from `src/updates/restartFailure.tsx`, its confirmation `ExitConfirmDialog.tsx`), notifications, native menu, native window theme, the two settings the desktop
  shell holds in its own file, run in background and menu bar icon visibility (`shellSettings.ts`: the store behind both
  preferences, started from the window's `settings` query, pushing changes through the platform capabilities,
  `connectShellSettings` called in `main.tsx`), keep awake (the daemon holds the computer awake itself;
  `useKeepAwakeSync.ts` hands the shell the daemon's `keep_awake_held` for its menu bar icon), git status schedule,
  keeping the window in the daemon's interface language (`useInterfaceLanguage.ts`), the probe of which agents' desktop applications the host has (`useNativeAppsSync.ts`).
- `src/remoteDaemons/` — a client's remote daemons, reached over SSH by the desktop shell, and the Remote devices page of
  the application's own window that manages them: `RemoteDevices.tsx` (`RemoteDevicesProvider` and `useRemoteDevices`,
  mounted around the whole `App` so the rail button, the sidebar and the page share it: the connect hook, the shell's
  hand-over queue and page request, the add state and the Remove confirmation), `RemoteDevicesSidebar.tsx` (the
  device list in the docked sidebar's place), `RemoteDevicePage.tsx` (the selected device's header and form, or the Add
  form, or the empty state, in `ContentMain`), `RemoteDaemonForm.tsx` and `FormCard.tsx` (the editable entry form and its
  raised cards), `remoteDevicePreference.ts` (the device last viewed, per daemon) and `remoteDevicesTestSupport.tsx`
  (test host for what reads `useRemoteDevices`); and `useRemoteDaemons.ts` (the shell's
  saved entries and which are connected, read again as the shell says they changed), `useRemoteDaemonConnect.tsx` (one
  connection in progress and every question or failure of it: password, host key, version mismatch, in the window that asked),
  `RemoteDaemonPrompts.tsx` (the prompts), `useRemoteForwards.ts` and
  `ForwardedPorts.tsx` (a remote window's forwarded ports, read from the shell, and the list drawn with Stop),
  `connectionDetails.ts` (reading the
  `octoboard+ssh://` line that fills the form), `connectionState.ts`, `port.ts`, and `remoteDaemonsOffered.ts` (the rail has
  the group only in the application's own window). No secret is held here: the shell never returns one to a page.
- `src/statusItem/` — the menu-bar menu: its builder, quota submenus, icons and the hook that pushes it to the platform.
- `src/sidebar/` — one console's sessions and projects: `Sidebar` picks the view over `SidebarFrame`, `ConsoleView` and
  `FocusView` list them with `ProjectNode` and `SessionRow` (their pin and lock buttons in `MarkButtons.tsx`, the row's
  derived state in `sessionRowModel.ts`), plus focus modes, menus (`menus.tsx`: the project menu derives its Git items from the project's git status) and new-session controls.
- `src/archive/` — archived sessions over the terminal area, with deletion.
- `src/terminal/` — the xterm.js terminal: controller and the surface it draws on, pane, reconnect, the reading of its
  output as text, the font size (`terminalFontSize.ts`: the client-held, persisted preference the controller applies to every
  terminal; `terminalFontShortcut.ts`: `useTerminalFontShortcuts`, mounted in `App.tsx`, which runs the three Terminal
  shortcut commands), `pastedImage.ts` (a clipboard image pasted into an agent session's terminal: sent to the daemon to be saved, and its
  path pasted in its place; `TerminalController.pasteImage` is the entry, `ClipboardCapability.readImage` supplies the image to a paste that carries none), path `links/`, project `shell/` terminals (with their tabs), archived-session `history/`;
  xterm's private members are reached from two adapters only: `xtermInternals.ts` (`watchUserInput`, the mark that
  tells the user's input from xterm's own replies) and `links/terminalLinks.ts` (OSC 8 link data).
- `src/report/` — a session's report panel: `ReportPanel.tsx` (pager, header badges, the drafts and the pending or
  accepted submission of each page), `pageDrafts.ts` (the drafts held outside the panel, per daemon store and
  session, so they outlive the panel closing: `ReportPanel.tsx` reads them with `heldDrafts` and writes them with
  `holdDrafts`), `listPages.ts`, `useProjectReportSession.ts`, `pageTime.ts`; `page/` renders a
  page, which is a component tree the daemon has validated, with native components rather than a frame: `PageView.tsx`
  (the page and its form), `registry.tsx` (`COMPONENT_RENDERERS`: the renderer of every kind of component), `formModel.ts`
  (the pure form: values, visibility, issues and the submission), `fields.tsx`, `submitters.tsx`, `layout.tsx`,
  `content.tsx`, `copyable.tsx`, `PageTable.tsx`, `PageMarkdown.tsx` (untrusted Markdown), `ChartView.tsx` with `chart/`
  (ECharts, loaded only when a page shows a chart), and `patternCheck.ts` with `patternWorker.ts`, which test a
  field's `pattern` in a Web Worker under a time limit, never on the window's own thread.
- `src/panels/` — the right pane, a stack of inset panel cards, and the registry of panel kinds.
- `src/project-browser/` — the Files panel's content: `files/` (tree, listings, search, `contentSearch/`: Find in files, the modal that searches the project's file contents through the daemon (the dialog, its streamed-result hook `useContentSearch.ts` and its state `useContentSearchState.ts` that `ProjectBrowser.tsx` keeps, the result list, the preview under it, the resizable split between them, the text a result row shows, and `findInFilesLauncher.ts`, the launcher the window's shortcut calls), and the linked-file viewer
  `App` opens for a path a terminal names, which `usePathLinkReader.ts` reads from the daemon's host) and `git/` (changes, branch comparison, commits, following a session's
  branch); at its root, the browser's per-project state (`browserState.ts` over saved preferences and window-lifetime
  selections) and the list helpers the two modes share; `IncludeIgnoredToggle.tsx`, the include-ignored toggle that the file filter and Find in files share; `useServedByDaemon.ts` tells whether this page is the browser build the daemon serves itself, which its lazy-load failures use to offer a reload.
- `src/viewer/` — the read-only file viewer modal (the shell at its root), and the app's Markdown and code rendering
  library, which `terminal/history/` uses too: `markdown/` (documents, sanitized HTML, images, Mermaid, formulas, fenced code), `code/` (the
  code and diff renderer and its budgets, and `lineMarks.ts`, the marks a file opened at a line shows on that line's text), `find/` (the find bar) and `image/` (the zoomable image of an image file or an image change:
  `ZoomableImage`, `ZoomBar`, the zoom arithmetic in `zoom.ts`, and `ZoomGroup` in `zoomGroup.ts`, which makes the two sides of an image change zoom and scroll as one); `download/` (Download and open, in a remote window of the desktop shell: a live file the viewer
  cannot show is read from its daemon in parts and copied to this computer, `useFileDownload.ts`, `downloadFile.ts`, and its
  failures, `downloadFailure.tsx`) with `standInAction.ts` (what the stand-in of such a file offers: the default
  application, or the download); `@pierre/diffs` is imported only inside it.
- `src/quota/` — plan quota wording, meters, reset credits, and the rail's quota panels (`RailQuotaBars`, `QuotaPanel`,
  `AgentPanel`, `QuotaAccount`).
- `src/settings/` — the settings modal and its sections (`sections.ts` holds the section ids and `SettingsNavigation`).
  The modal's search field filters sections and rows: `settingsSearch.ts` holds the static search index
  (`SETTINGS_SEARCH_INDEX`, one entry per row or block of each section, its `SettingsSearchId` type) and the matching,
  `useSettingsQuery.ts` the dialog's query state, per-section matches and which section is on screen (`initialId`, `select`), `settingsSearchFilter.tsx` hands a section's
  matches to the rows and groups inside it (`SettingsSearchProvider`, `useSettingsSearchFilter`; `SettingsSearchBlock`
  wraps a section's own block that has an index entry), and `useFocusRescue.ts` hands focus on when a re-match hides
  the element that held it;
  the reads of the system that rows wait on are made once per dialog, not in the rows: `SettingsReadsProvider.tsx` (wrapped
  around its content by `SettingsDialog.tsx`) holds `useLaunchAtLogin.tsx` (the launch-at-login state machine) and
  `useMacosPermissionsRead` (`useMacosPermissions.ts`, with the read's `shown` and `unreadable` answers and the `follow()`
  count that decides when a focus return re-reads), `settingsReads.ts` is their context (`useSettingsReads`) and `searchReads`,
  what the search is told of them, so the search counts those rows only once their read has answered, while `useMacosPermissions`
  is the Permissions section's actions on top of that read;
  `GeneralSection.tsx` the General section (the language, then startup, window, power, terminals and Sorting groups, each
  shown only where the platform has the capability, the power one where the daemon's host can be held awake (`host.keep_awake`));
  `LaunchAtLoginSetting.tsx`, `MenuBarIconSetting.tsx` and `BackgroundRunSetting.tsx` are its native-backed rows,
  `KeepAwakeSetting.tsx` its daemon-backed Prevent sleep row, with its choices of when, and what each says, in `configuration/keepAwakeWhen.ts`;
  `RemoteAccessSection.tsx` the Remote access section (this computer's daemon reached from another device over SSH: first whether the
  SSH server is on (`useRemoteLogin.ts`, `sshServerWords.ts`), then the switch and, on Windows while nothing is served, the browser
  port; while served, two underline tabs, the application's connection details with copy and QR, and the browser's numbered steps,
  `ForwardCommand.tsx` (the SSH forward with its address as a token that opens a menu of this computer's names and addresses) and the
  address, the port and Rotate key, each tab's rows in a `SettingsTabPanel.tsx`, which keeps a tab switch from taking part in the
  section's list motion), offered only in the application's own window for a daemon that offers it (`remoteAccessOffered.ts`);
  `AppearanceSection.tsx` the Appearance section (`ThemeSetting.tsx`, `MarkdownStyleSetting.tsx` and `TerminalFontSizeSetting.tsx`,
  the terminals' font size, a `NumberField` from `src/primitives/`), `CleanupSection.tsx` the Cleanup section;
  `accounts/` the Agent accounts section, where `AgentProviderSettings.tsx` renders every setting the daemon serves for an agent
  and `providerSettingCopy.tsx` is the copy and help of any provider setting, shared with the dialogs that hold a setting's value,
  `AgentMenus.tsx` holds an agent tab's menu and actions button, which open `AgentSettingsDialog.tsx` (the per-agent
  settings) and `AgentVersionDialog.tsx` (the agent's CLI version, its content `AgentCliVersionSetting.tsx`, with
  `cliVersion.ts` judging whether an update exists), both on `agentDialogSubject.tsx` (the heading and the closing of
  a dialog whose agent has gone), and `AgentCliUpdateAlerts.tsx`, mounted in `App.tsx` for the window's life, raises the
  alert of an update this window started that ended unfinished.
- `src/dialogs/` — the feature dialogs (console, project, session, request, handoff, account switch, model, trust,
  archive and removal choices), on the `components/dialog/` toolkit. `RequestedDialog.tsx` only routes a requested
  dialog to its component; the archive, removal, restore and pull-merge confirmations are in `confirmations/`. The dialogs' daemon
  lookups (detected agent, located conversation, agent models, session records, branch inspections) are built on
  `useLatestRequest`; `branchInspection.ts` holds the pure part of a branch inspection and `useBranchInspections.ts`
  its hooks. A launch level's compatibility switches (console, project, session, request, handoff and model dialogs) are
  generic rows built from the served setting descriptors by `AgentCompatibilityField.tsx`, whose `CompatibilityPicker`
  the console launch matrix reuses.
- `src/process/` — the Processes panel and its data, the polled global process count the rail shows
  (`processPolling.ts` holds the polling paces), and the rail's process panel (`ProcessCounts`).
- `src/configuration/` — configuration export and import dialogs, by Settings section and by agent. Shared cards and parts: `SettingsGroups.tsx` (the sections' cards), `AgentSettingsCard.tsx` (the per-agent cards: `AgentCardSections` lays out an agent's card, `useAgentSettingLines` words one line per setting of that agent, from the served schema and the providers' copy, naming an unpresented setting by `idWords` of its id) and `importParts.tsx` (review rows, status lines, `ImportChoice` and `choiceHeader`, `LEFT_OUT_STATUS`); `settingsSections.ts` holds the Settings sections a document carries, in Settings order, with their labels; `useImportChoices.ts` holds the review's choice state (which parts, sections and agents are taken) and `useClientSettingsReview.ts` the values this client holds now, for the review to compare against; `clientSettings.ts` carries the settings only this client holds (appearance, Markdown style, terminal font size, run in the background, menu bar icon, language, session order) into and out of the document, with `clientSettingsPlatform` saying which of them this platform has; `ImportProgress.tsx` shows a running import item by item, its state kept in `progressState.ts`, with `ResultCards.tsx` shared by progress and result; `keepAwakeWhen.ts` holds the labels of Prevent sleep's choices of when, and the descriptions the Settings search indexes, shared by its Settings row, the search index and the import's cards; and the list of plain switch settings (`booleanSettings.ts`, grouped by the Settings section each belongs to).
- `src/about/`, `src/updates/` — the licenses dialog and its data; the UI side of self-update: one `UpdateStore` interface, whose shared part (`createUpdateStoreBase`) lives in `updateStore.ts` beside the desktop shell's updater, over that updater or a daemon that updates itself (`daemonUpdateStore.ts`, from `snapshot.daemon_update`; its writes of the settings go through `store/settingsWrite.ts`), chosen by `UpdatesProvider`, which sits inside the daemon's provider. `UpdateNotice.tsx` is the notice, `useReloadAfterRestart.ts` reloads the page that asked for a daemon restart once the daemon is back on the new version (through `src/platform/reloadPage.ts`) and `useUpdatedNotice.tsx` tells a browser once per version that the daemon it uses was updated, `restartBlockedReason.ts` gives Restart now the reason it is unavailable, and `restartFailure.tsx` raises the alert of a refused daemon restart (also used by the exit flow); the command for an update the daemon cannot put in place itself comes from the daemon (`manual.command` of the status), not from a file here. Settings shows it in its `updates` section (`src/settings/UpdatesSetting.tsx`).

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
  `values/`, `providers/` and `preferences/` import nothing else below `src/`; `platform/` imports only `protocol`.
  Each other pure layer imports only what its own clause names: `input/` only the `platform/` barrel; `i18n/` only
  `preferences/`, `providers/` and `appConfig`; `format/` only `values/` and `i18n/`; `agents/` only `values/`, `i18n/`,
  `providers/` and `protocol`; `git/` only `values/`, `i18n/` and `protocol`; `sessions/` only `values/`, `i18n/`,
  `agents/` and `protocol`; `accounts/` only `values/`, `i18n/`, `agents/`, `sessions/`, `format/` and `protocol`;
  `errors/` only `values/`, `i18n/`, `agents/`, `sessions/`, `accounts/`, `format/`, `protocol` and `appConfig`;
  `browse/` only `values/`, `i18n/`, `errors/`, `format/` and `protocol`; `store/` only `values/`, `i18n/`, `agents/`,
  `errors/`, `accounts/`, `sessions/`, `preferences/`, `git/`, `protocol`, `requests`, `daemon` and `daemon-client`.
  `errors/` and `browse/` do not import `daemon-client`: its `CodedError` lives in `errors/codedError.ts`, and the codes
  it raises are records in `errors/codes.ts`.
- `components/` is the leaf layer: outside itself it imports only the pure layers, the `store/` barrel and the shared
  root files. `shortcuts/` imports `i18n/`, `platform/`, `preferences/`, `input/` and `components/`.
- `viewer/`, `about/`, `updates/`, `layout/`, `quota/` and `statusItem/` import the leaf layer and the pure layers (`quota/` also
  `accounts/`; `statusItem/` also `accounts/` and `quota/`; `updates/` also `protocol`, `requests`, `appConfig` and the `store/` barrel) and no other feature directory; `project-browser/` additionally imports `viewer/`. The remaining feature
  directories import one another freely.
- From below `src/`, the root files that may be imported are `protocol`, `requests`, `daemon`, `daemon-client`, `appConfig`, `launchable` and the
  `store/`, `platform/` and `primitives/` barrels. The entry, `App` and the screens are imported by nothing below them.
- `shell/` is imported only by root files, `app/`, the gallery and the website preview; `app/` only by `App.tsx`.
- Production code imports neither `fixtures/` nor `gallery/`; `fixtures/` and `website-preview/` never import
  `gallery/`; `fixtures/` does not import `shell/`.
- `@heroui/react`, `@heroui/styles`, `react-aria-components`, `react-aria` and the `@react-aria/*`, `@react-stately/*`
  and `@react-types/*` packages are confined to `primitives/` (`WRAPPED_LIBRARIES` in `eslint.layers.mjs`, a
  typescript-eslint rule of its own so the layer blocks above do not replace it); the rule is in
  `docs/conventions/ui-components.md`, "The wrapper layer".
- The alert parts (`Alert` and the parts named after it, from `primitives` or HeroUI) are imported only by
  `components/alerts/AlertFrame.tsx` and `ErrorAlert.tsx` (lint), so every alert
  has a title.

Other checks that hold structure: `src/errorModulesCycle.test.ts` (the error modules do not import each other in a
cycle), `src/daemonCodes.test.ts` (the registry in `errors/codes.ts` matches the codes the daemon declares, the providers' included, and the
catalogs), `src/i18n/catalog.test.ts` (placeholders and plural categories of every catalog, the platform words for every host), `src/appConfig.test.ts`
(copies of `config/app.json` elsewhere), `src/cssDependencies.test.ts` (every package a stylesheet names is a production
dependency).

## Where new code goes

Where a file goes. The rules the code in it follows are in the convention docs the bullets point to; the index of them
is `docs/README.md`, "Coding conventions".

- A third-party UI component, hook or utility the application needs and `src/primitives/` does not export yet gets its
  wrapper in `src/primitives/` (the rule: `docs/conventions/ui-components.md`, "The wrapper layer"; how a wrapper is
  written is in the layer's README).
- A common correction or approved extension of a wrapped control belongs in that wrapper's adjacent stylesheet in
  `src/primitives/`. A composite or feature's own layout and visual treatment belongs beside that owner. Shared
  theme and geometry definitions belong in `src/styles/`; register owned stylesheets in `src/style.css` rather than
  loading them from component modules.
- A screen-level concern of `App` is a hook in `app/`; the window chrome is in `shell/`; a new feature is a feature
  directory importing the leaf and pure layers (and other features only where the rules above allow it).
- Pure rules and wording about sessions, accounts, agents, git or formatting go in the matching pure layer, not in a
  component; a shared building block goes in `components/<kind>/`.
- What is specific to one provider lives in the provider: its presentation module (`src/providers/<id>.ts`) for its
  mark, colour and names, and the facts the daemon serves about it (names, install page, configuration directory,
  declared capabilities), read through `src/agents/providers.ts`. Quota window names (`QuotaNaming`,
  `src/accounts/quotaFormat.ts`) and the display of model ids (`modelDisplayName`, `src/sessions/sessionModel.ts`)
  follow the rules the daemon serves in each provider's descriptor. A provider's settings are drawn from the schema the
  daemon serves (`ProviderInfo.settings`) by `AgentProviderSettings` (`src/settings/accounts/`), with their copy in the
  provider's presentation module; Settings does not show a setting whose provider has no heading copy for it, while the
  configuration import review still lists such a setting in its agent's card, named by `idWords` of the setting's id.
  Text that names a provider sets its name apart through `ProviderName` (`src/components/text/`). What generic code may
  and may not do about it is in `docs/conventions/provider-neutral-core.md`, "UI".
- A new provider: its presentation module `src/providers/<id>.ts` (mark, colour, its messages in every language, the
  record and wording of each code it declares, with the code's recovery copy `provider.<id>.<code>.remedy`, the copy of
  each of its settings, `planNames`, `params`), added to `BUILT_IN` in `src/providers/registry.ts`, whose keys type the
  catalogs. `providers/agentMarks.ts` and the website's marks are outside this bar (see
  `docs/conventions/provider-neutral-core.md`, "UI").
- A settings control that saves a field the daemon versions, and a dialog that edits a console, a project or an account,
  write through `useSettingsWrite` and `useCheckedEdit` (`docs/conventions/protocol-and-contracts.md`, "The envelope and
  field conventions"); a field a form edits has its path in the daemon's versioned paths (see the daemon structure doc).
- A user-facing string goes through `i18n/` (the rules: `docs/conventions/i18n-in-code.md`).
- A new daemon error code needs its record in `errors/codes.ts` in the same change; a code only one provider raises has
  its record and wording in that provider's presentation module instead (`docs/conventions/error-codes.md`, "The UI's
  record of a daemon code" and "Provider codes").
- A new wire type is added in the daemon's Rust types (`apps/daemon/src/protocol/`) and `protocol.generated.ts` is
  regenerated; a new request names its reply in `REPLY_TYPES` in `requests.ts`. The rules, including where a UI rule
  about a wire value goes, are in `docs/conventions/protocol-and-contracts.md`, "Wire types and generated files"; what a
  client has to do with the wire in general is `apps/daemon/PROTOCOL.md`'s "What every client implements" section.
- A new kind of report-page component is added to the daemon's `page_content` first (see the daemon structure doc);
  `src/report/page/registry.tsx` then fails type checking until the kind has its renderer there.
- A new UI state worth seeing is a scenario under `src/gallery/fixtures/`; data used by both the gallery and the website
  preview or application tests belongs in `src/fixtures/`.
- Tests sit beside the code (`*.test.ts(x)`); how they are written and where their data comes from is in
  `docs/conventions/tests.md`, "Where tests sit" and "UI component tests (jsdom)".

## Shared entry points

New code uses these rather than a second implementation. None is enforced by a check unless a check is named. The rows
are the helper list; the rules that go beyond "use helper X" are in `docs/conventions/ui-code.md`,
`docs/conventions/ui-components.md` and the other convention docs they point to.

| Need | Entry point |
|---|---|
| Raise an error alert for a failure | `raiseError`, or `alertFailure` as a request's catch handler, in `src/components/alerts/raiseError.tsx`, which takes the id of the session a failure is about (`session`); `errorAlertOf` there turns a daemon's error broadcast into the same alert. The alert names the session from the store as it shows (`AlertSessionLines`). Alerts are drawn through `ErrorAlert` and the two built on it (`FailureAlert`, `ChoiceWarning`) |
| What a daemon broadcast or failure shows as a notification (a toast, or an error alert to acknowledge) | `broadcastNotification` in `src/store/notifications.ts`, which the daemon's broadcasts and `Daemon.showNotice` feed to the window through `onNotification` (used by the store and the fixture daemon); `errorNotification` there is the request `raiseError` builds |
| The button a recovery offers besides its words (Settings, Change model, or a link out such as Download Git) | `recoveryAction` and its `RecoveryAction` union in `src/components/alerts/ErrorGuidance.tsx`; a link is drawn by `RecoveryLink` among the actions of the alert footer or popover, and a pane failure takes it as `PaneFailure`'s `linkAction` (`src/components/alerts/PaneFailure.tsx`) |
| Speak the outcome of a retry the user asked for | `AttendedStatusAnnouncer` (`src/components/announce/`), as `useGitRetryAnnouncement` does for the Git-missing mark (`src/sidebar/useGitRetryAnnouncement.tsx`); how it is used is in `docs/conventions/ui-components.md`, "Shared components to reuse" |
| Wording and handling of an error code | one record per code in `src/errors/codes.ts` (`CODES`; `Code` and `CODE` name a code, `variantOf` on a record picks the message variant; `detailBlock` shows the failure's `detail` as raw diagnostics behind "Error details", `detailLabel` as text the user wrote, under its own label after the explanation: the alert's `attached` part, drawn by `DiagnosticBlock` with `wrap` and `dir="auto"`) |
| A request to the daemon, and the type of its reply | `Daemon.request` (`src/store/connection.ts`) typed by `REPLY_TYPES` in `src/requests.ts`; a reply of another type rejects with the client code `unexpected_reply` (checked in `DaemonClient.request`, which `Daemon.request` wraps). `looseRequestSender` adapts a stand-in for the daemon (fixture daemon, preview, `testDaemon`) and does not check the reply type |
| A failure carrying a daemon or client code | `CodedError` (`src/errors/codedError.ts`) |
| Whether a session is working (the set keep awake follows, and the quit confirmation lists without the sessions waiting for the user) | `sessionIsWorking` (`src/sessions/sessionStatus.ts`) |
| Whether a session is at its turn or may go on without anyone acting (what a handoff waits out), and whether a session it owns can still report to it | `sessionIsBusy`, `childCanReport` and `hasChildThatCanReport` (`src/sessions/sessionStatus.ts`), over the daemon's `Session.at_usage_limit`; the daemon's judgement is `refuse_unless_it_can_be_handed_off` |
| Open a session's conversation in its agent's desktop application | `useOpenSessionInApp` (`src/components/host/useOpenSessionInApp.ts`) sends `open_session_in_app`; whether and how to offer it is `openInAppBlock` and `openInAppNote` (`src/sessions/nativeApp.ts`), the agent's label `nativeAppLabel` (`src/agents/providers.ts`, from the provider's `openInApp` message), and whether the application is installed or sees the session `useNativeAppInstalled` and `useNativeAppUnseen` (`src/store/nativeApps.ts`, refreshed by `useNativeAppsRefresh` and `useNativeAppsSync`) |
| A tooltip, on a control, a mark that is no control, or something that cannot be a trigger | `Tooltip` (`src/primitives/Tooltip.tsx`), the one tooltip component, with `trigger` `control` (the default), `mark` or `anchored`; a `Button`, a `Switch` and a `Checkbox` are never wrapped in one, as they carry their own through `tooltip` (an internal `box` tooltip on a switch's track or a checkbox's box). Its internal parts are in `src/primitives/tooltipParts.tsx` |
| A control that is unavailable for a reason the user is told (it stays focusable, fades, takes no press and says the reason as its tooltip and description) | the `unavailable` prop of `Button`, `Switch` and `Checkbox`, whose shared handling is `src/primitives/unavailable.tsx`; `restartBlockedReason` (`src/updates/restartBlockedReason.ts`) is such a reason |
| A floating panel with something to press or select in it, opened by hover | `PopoverHint` (`src/components/hint/hintPopover.tsx`); a tooltip never holds content to interact with |
| The sentence saying why a process tree cannot be kept | `unkeepableMessage` over `UNKEEPABLE_MESSAGES` (`src/process/processes.ts`), the sentence of the daemon's refusal for that `unkeepable_reason` |
| A tooltip with a heading, a divider and a body | `HeadedTooltip` (`src/components/hint/HeadedTooltip.tsx`), which builds the content only; it is given to a `Tooltip` |
| Whether an account switch holds a session, the daemon's or one this window queued | `holdsAccountSwitch` (`src/sessions/sessionStatus.ts`), or `useAccountSwitchHeld` in a component |
| Tell whether a value the daemon sent is one this client knows (a closed enum's type predicate, built from a `Record<Value, true>`) | `knownValues` (`src/values/knownValue.ts`), as `isKnownSessionStatus` does |
| Keep unchanged data by identity when a reducer rebuilds it | `sameValue` (`src/values/sameValue.ts`), as the `settings_updated` reducer does |
| Show a file at a line with runs of its text marked (where a content search's result opens) | `ViewedFile`'s `line` and `marks` (`src/viewer/viewedFile.ts`), drawn by `CodeSurface` through `lineMarks.ts` (`src/viewer/code/`): a surface paints its ranges with `paintLineMarks` under an owner from `useLineMarkOwner`, so one surface clearing its marks leaves another's painted |
| Follow a running content search | `Daemon.onContentSearch` (`src/store/connection.ts`), which delivers the `project_content_matches` and `project_content_search_ended` events of one search (named by the `search` the request carried) through `createContentSearchListeners` (`src/store/contentSearchListeners.ts`); `useContentSearch` (`src/project-browser/files/contentSearch/`) is its one consumer, and the fixture daemon implements it too |
| A field with an icon button, a text button or another field beside it in one row | `FieldRow` (`src/components/form/FieldRow.tsx`) for the row and its gap, and `besideField` on `Button`, `ToggleButton` or `IconToggle` for any button there (the field's height and corners, `FIELD_CONTROL_CLASS`); `TextInput`'s `trailing` is drawn in one |
| A tree row in a list, with its icon and mark buttons | `TreeRow`, `RowIconButton`, `RowMarkButton` and `HOVER_ACTIONS_CLASS` in `src/components/rows/`; a list that animates its reordering, and rows it hides with `hidden` while they stay mounted (a Settings search), uses `useListMotion` (`src/components/motion/`); a nested list keeps its own rows |
| A section heading that pins while its section scrolls (sidebar row groups, Settings section groups, a side pane's categories with a note and the session groups beneath them) | `StickyHeading` with `variant` `list`, `group`, `category` or `subcategory` (a nested heading pinned below its category), and `STICKY_HEADING_HEIGHT` for fixed-height headings; the pane measures its category and subcategory headings for the scroller (`src/components/scroll/StickyHeading.tsx`); `SettingsGroup` and the Processes panel use it. The virtualized Git change list's foldable section headings are positioned by the virtualizer (`ChangeSectionHeading`, `src/project-browser/git/changeSectionHeading.tsx`) and marked by `TreeHeader` in `src/primitives/Tree.tsx`. A pinned row or strip that is not a heading (the `DirectoryPicker`'s parent-directory row, the agent strip in `src/settings/accounts/AgentAccountsSection.tsx`) calls `useStickyHeadingFocus` (`src/components/scroll/stickyHeadingFocus.ts`) directly. Its `data-sticky-heading` marker also lets `FadedScroller` (with `stickyHeadingHeight`) and `useStickyScrollFade` apply the shared exit fade automatically: `src/components/scroll/scrollFade.tsx` owns the watch, and its co-located `scrollFade.css`, loaded by `src/style.css`, paints the surface-coloured overlay |
| Test a report page's `pattern` against a value | `checkPattern` (`src/report/page/patternCheck.ts`), which runs in a Web Worker with a time limit |
| A strip of text tabs | the tab wrappers in `src/primitives/Tabs.tsx`, with `fit="content"` on `TabListContainer` and `TabList`, shared by every such strip (`TextTabStrip`, `RegionTabs`, `AgentTabList`, the console launch fields, the project browser); `Tab size="content"` is the tab of these strips, `size="compact"` that of the project browser's Files / Changes tabs in a pane header |
| The frame of the sidebar's console and focus headers and the archive header | `PANE_HEADER_FRAME_CLASS` (`src/components/panel/paneHeaderFrame.ts`) |
| The content area's `<main>` (the sessions' and every page that takes the area beside the sidebar) | `ContentMain` (`src/components/panel/ContentMain.tsx`), which owns its positioning and the docked sidebar's edge shadow; a page adds layout through `className` |
| A previous / next pager button | `PagerButton` (`src/components/controls/PagerButton.tsx`) |
| A block with a copy button over its top end corner (a report page's code, diff and copyable blocks, a command in Settings) | `CopyBox`, and `CopyCommand` for a command line (`src/components/controls/CopyBox.tsx`), over `ScrollBox` (`src/components/scroll/ScrollBox.tsx`), a box that scrolls sideways instead of wrapping |
| A button that confirms its action by turning to a done state (a tooltip text and glyph change, announced) | `FeedbackButton` with `useDoneFeedback` (`src/components/controls/FeedbackButton.tsx`), as `CopyButton` and `PasteButton` use it |
| Read the clipboard's text on a user's press | `PasteButton` (`src/components/controls/PasteButton.tsx`) over the platform's `clipboard.readText` |
| A block of code shown as text | `CodeBlock`, or `SurfaceCodeBlock` for a wrapping block on its own surface (`src/components/text/CodeBlock.tsx`); a flush `CodeBlock` built on a surface of its own, such as inside a scroller as `DiagnosticBlock` does, puts `CODE_SURFACE_CLASS` (the classes of the secondary `Surface` it sits on) on that surface |
| Open an external address with the platform adapter and shared failure handling | `ExternalLink` (`src/components/text/ExternalLink.tsx`) for application links; `useExternalLinkAction` from the same module for document-owned link markup |
| A settings section's lead paragraph and the rows under it | `SettingsLead` and `LeadRows` (`src/settings/SettingsLead.tsx`) |
| A new row, or a block of a section's own, in Settings | `SettingRow` / `SwitchSettingRow` with a `searchId`, or `SettingsSearchBlock` around the block, and its entry in `SETTINGS_SEARCH_INDEX` (`src/settings/settingsSearch.ts`); a group's `SettingsGroup` `title` is a message key; `LeadRows` takes `hidden`. `src/settings/settingsSearch.test.tsx` fails when a row and the index drift apart; what the entry must carry is in `docs/conventions/ui-components.md`, "Shared components to reuse" |
| An on/off switch in Settings that saves | `SettingSwitch` or `SwitchSettingRow` (`src/settings/SettingRow.tsx`); the `onChange` may return the save, so the switch ignores presses until it settles; `DaemonSwitchSetting` and `GitSection` return nothing and rely on `useSettingsWrite`'s version guard instead |
| Pin and lock buttons of a session row; its derived state | `PinButton`, `LockButton` and `SessionMarkButtons` (`src/sidebar/MarkButtons.tsx`); `sessionRowModel` (`src/sessions/sessionRowModel.ts`) with the `useSessionRowModel` hook (`src/sidebar/`) |
| Resolve the selected session's focus context for an action or shortcut | `selectedSessionFocusTarget` (`src/sidebar/focus.ts`), shared by `useFocusMode` and session menus; it resolves ownership through `consoleSessionAbove` and checks target availability through `resolveFocus` |
| Let an open menu take an advertised window command | `ActionMenuItem.commandShortcut` (`src/components/menus/menuEntries.ts`) pairs its displayed binding with its matcher; `useWindowShortcut` (`src/shortcuts/`) delegates a matched, consumed command during a modal through `runMenuCommand` (`src/components/menus/menuCommands.ts`). `MenuItems` registers through `registerMenuCommand` and owns the active root menu's eligibility |
| An account's record, login or key among the daemon's lists | `accountRecordOf`, `accountLoginOf`, `loginIsActive` and `accountKey` (`src/accounts/accountRecords.ts`) |
| Retry a focus move across frames until its target exists (with an optional `onGiveUp`), or run after a few frames | `tryEachFrame` and `afterFrames` (`src/components/focus/`) |
| A menu with a filter field that narrows its items by typing | `ActionMenu`'s `filter` prop (`src/components/menus/ActionMenu.tsx`), which draws the filtered presentation through `FilteredMenuItems` (`src/components/menus/FilteredMenuItems.tsx`: the field, the items and the no-match line) over the layer's `MenuFilter` (`src/primitives/MenuFilter.tsx`); `onOpened` tells a caller the menu opened. The console-session new-session menu (`src/sidebar/NewSessionMenu.tsx`) is the first user |
| The placeholder of a search field | the shared catalog key `common.search` (`FileFilterField` has no `placeholder` prop) |
| The Git workflow items a setting offers | `GIT_WORKFLOW_ITEMS` and `GIT_WORKFLOW_ITEM_KEYS` (`src/git/gitWorkflow.ts`) |
| A test that mounts a component under the app's providers | `mountHost` (`src/test/mountHost.tsx`) over `testDaemon` (the real store, requests rejected unless the test gives a handler); the container it returns has `render(ui)` to render again under the same host |
| A faked daemon request that fails with a daemon code | `daemonError` (`src/test/daemonError.ts`): a `CodedError` whose message is the catalog's sentence for its code and params, as the store words it |
| A test that depends on time (a fake clock, a timer, a frame, a wait on the real clock, a lazily imported chunk) | `src/test/clock.ts` (`fakeTimers`, whose one teardown drains pending timers and restores the real clock; `fakeTimersKeepingFrames`, `runOutTimers`, `runPendingTimers`, `afterQueuedWork`, `flushPromises`, `waitOnRealClock`) and `settleLazyImport` (`src/test/lazyImport.ts`). Time helpers tied to one area sit beside it: `settleFrames`, `framesAsTimers`, `framesHeld` and `withModality` (`src/components/dialog/dialogTestSupport.ts`), `press`, `tooltipsCooledDown` and `mountForTooltip` (`src/components/hint/tooltipTestSupport.ts`), `firstWriteDelayPassed` (`src/components/announce/statusAnnouncerTestSupport.ts`), `SUBMENU_HOVER_DELAY_MS`, and `openBy`, `filterField`, `typeInFilter`, `pressInFilter` and `holdExitAnimations` for a menu's opening and its filter field (`src/components/menus/menuTestSupport.ts`), `PAST_ANY_RETRY_MS` (`src/terminal/reconnectTestSupport.ts`), and `settle` and `settleInAct` for gallery steps (`src/fixtures/clockTestSupport.ts`) |
| State a test leaves behind for the next one (the error-alert queue, the window preferences held in module memory) | `src/test/setup.ts` empties the alert queue after every jsdom test and shims `getClientRects` (no rects for a disconnected element, one that is or sits in `hidden` or `display: none`, a rect otherwise, as `useListMotion` reads none as a list not rendered); `stubWindow`'s teardown in `src/appTestSupport.tsx` calls `resetAppPreferences` (panel stacks, closed Report panels, the selected console, sidebar and aside visibility), which `localStorage.clear()` alone does not reset |
| A plain on/off application setting in Settings | `DaemonSwitchSetting` (`src/settings/DaemonSwitchSetting.tsx`) over the list in `src/configuration/booleanSettings.ts` |
| A checked write of application settings from a control (the switches, the choice of when to prevent sleep, the Git section) | `useSettingsWrite` (`src/settings/useSettingsWrite.ts`): sends `update_settings` based on the versions the store holds of the paths it changes, and drops a press while the store still holds a path at the version this window last wrote it from; code that is not a component writes through `createSettingsWriter` (`src/store/settingsWrite.ts`), which it wraps |
| The checked-write side of a form dialog for a console, a project or an account | `useCheckedEdit` (`src/components/hooks/useCheckedEdit.ts`) over `baseVersions` and `writeConflict` (`src/errors/writeConflict.ts`): the `base_versions` of the fields the user changed, and putting a field the store moved past back to its current value, announced through `ChangedElsewhereAnnouncer` when the user had edited it |
| Wait for the store to reach a state from code that outlives its component (a checked write that goes on after its field has gone) | `whenState` (`src/store/whenState.ts`) |
| Announce a change made under the user that nothing marks on screen (a form field replaced because it changed elsewhere, a review read again) | `ChangeAnnouncer` (`src/components/announce/ChangeAnnouncer.tsx`) over `AttendedStatusAnnouncer`; `ChangedElsewhereAnnouncer` for replaced fields, naming them |
| An xterm terminal view | `useTerminalController` and `useTerminalReconnect`, with `TerminalSurface`, `TerminalCover` and `TerminalNoticeBar` in its `TerminalNoticeSlot` (`src/terminal/`), shared by the session pane and shell terminals |
| The loading line of a panel | `LoadingState` (`src/components/panel/LoadingState.tsx`) |
| A failure's status mark with its Try again | `RetryStatusMark` (`src/components/status/RetryStatusMark.tsx`) |
| Read something again now and then: pace, a pause while the document is hidden with a catch-up on return, reads the caller sends itself | `usePoll` (`src/components/hooks/usePoll.ts`), on `startPolling`; the browser's lists poll at `AUTO_REFRESH_MS` (`src/project-browser/autoRefresh.ts`) |
| Run window-wide checks on a fixed schedule even while the document is hidden | `startPeriodicSweep` (`src/components/hooks/periodicSweep.ts`), for the Git status and desktop update sweeps |
| Ask the daemon for what a view shows and keep only the latest answer, scoped to what it is for, with the failure sorted out and a resend on reconnect | `useLatestRequest` (`src/components/hooks/useLatestRequest.ts`), as the project browser's lists and readers and the dialogs' lookups (`src/dialogs/`) do |
| Send a browse request | through a lane or a slot in `src/browse/budget.ts` (`listingLane`, `listLane`, `documentImageLane`, `BROWSE_SLOTS`), whose shares `budget.test.ts` keeps within the daemon's bound |
| Treat a failed browse request | `browseFailure` and `projectBrowseFailure` (`src/browse/failure.ts`) |
| Open a dialog from a control outside its window | `createLauncher` (`src/components/hooks/createLauncher.ts`) |
| Open an App-level dialog (a `RequestedDialog`) from a control inside another dialog, so the dialog's lifetime is the window's and not the opener's | `openDialogFromAnywhere` (`src/dialogs/dialogLauncher.ts`), registered in `App.tsx` through `createLauncher`, as Settings → Configuration → Import does |
| Follow a running configuration import | `Daemon.onImportProgress` (`src/store/connection.ts`), which delivers the `configuration_import_progress` events of one import (named by the `import_id` the request sent) to its listener outside the store and reducer, through `createImportProgressListeners` (`src/store/importProgressListeners.ts`); the fixture daemon implements it too |
| A dialog whose work cannot be left while it runs | `Dialog`'s `busy` prop (`src/components/dialog/Dialog.tsx`), which sets `Modal`'s `busy` (`src/primitives/Modal.tsx`): Escape and an outside click are ignored, and every control in the dialog is disabled but a `pending` `Button`, which takes the focus from a control that busy disables and gives it back to the control it was taken from when the request is over (`useFocusLeavingWhenBusy`, `src/components/dialog/useFocusLeavingWhenBusy.ts`). The wrappers read the state through `useDisabled` (`src/primitives/busy.ts`), and code that draws a control of its own (a link) reads `useBusy` |
| A decorative spinner beside a label that already says what runs | the layer's `Spinner` without a `label` (`src/primitives/Spinner.tsx`), which `Button` (while `pending`, which also sets `aria-busy`), `ExternalLink` and the import progress view use |
| Load a component lazily, with a way to load it again after a failed import | `retryableLazy` (`src/components/hooks/retryableLazy.ts`) |
| Observe visible Suspense content before a capture | `observeLazyReadiness` and `trackSuspenseFallback` (`src/components/hooks/lazyReadiness.tsx`); the gallery frame enables observation, while production hosts leave it disabled |
| Poll a daemon answer that is shown or counted (process trees, the global process counts), publishing only a changed one | `usePolled` (`src/components/hooks/usePoll.ts`), under `useProcessTrees` and `useAllProcesses` at the paces in `src/process/processPolling.ts` |
| A badge on the corner of a control that comes and goes | `BadgedControl` (`src/primitives/Badge.tsx`), which `ChromeButton`'s `badge` prop and `ActionMenu`'s `badge` prop (`src/components/menus/ActionMenu.tsx`) share |
| A count in a row, or on a rail control | `CountBadge` (`src/components/status/CountBadge.tsx`), reusing `StatusBadge`, in a row; `RailCountBadge` (`src/shell/RailCountBadge.tsx`) through `ChromeButton`'s `badge` prop on a rail control |
| A rail entry whose button opens a popover panel | `RailPanelEntry` (`src/shell/RailPanelEntry.tsx`), which wires the popover; the status marks and Details popovers the panel opens are tagged with `PopoverOwnerContext` (`src/components/hint/popoverOwner.ts`) and asked about through `nestedPopovers` (`src/shell/nestedPopover.ts`), and `CloseWhenCovered` (`src/shell/`) closes the panel under another modal |
| Put a panel of the aside's owner (a project, or a console session) on screen | `showPanel` from `useProjectPanels` (`src/app/`); a jump from the global process list is `useProcessJump` |
| Send focus into a panel the user opened deliberately | `landInPanel` (`src/panels/panelLanding.ts`), as `useFloatingPanel` and `useProcessJump` do |
| Queue best-effort pushes to the native shell in order | `useSerialPush` (`src/components/hooks/useSerialPush.ts`) in a component, `createSerialQueue` (`src/components/hooks/serialQueue.ts`) outside one |
| A setting the desktop shell must know before the page loads | `createShellPreference` (`src/lifecycle/shellSettings.ts`) |
| Browse directories in a picker | `useDirectoryBrowse` (`src/components/dialog/DirectoryPicker.tsx`), which routes local desktop browsing through the platform's `directoryPicker` capability and otherwise uses the daemon-backed `DirectoryPicker`; locality comes from `Daemon.isLocal`, derived by `isLocalDaemon` (`src/daemon.ts`) and false in a remote window |
| Delete options of a dialog that deletes sessions | `useSessionDeletionOptions` (`src/dialogs/`) |
| A provider's name, install page or configuration directory; the file and folder names that detect it | `providerLabel`, `providerShortName`, `providerInstallUrl`, `providerConfigDir`, `detectionMarkers` (`src/agents/providers.ts`) over `useProviders()` |
| A provider's setting in Settings; its copy | `AgentProviderSettings` (`src/settings/accounts/AgentProviderSettings.tsx`) over the served `ProviderInfo.settings`; copy keys by `settingMessageKey` (`src/agents/providers.ts`), messages in the provider's module |
| A provider setting's copy and help text, wherever the setting is shown | `useSettingCopy`, `settingText` and `SettingHelp` (`src/settings/accounts/providerSettingCopy.tsx`) |
| A launch level's compatibility switches (rows, or one cell of a matrix) | `AgentCompatibilityField` and `CompatibilityPicker` (`src/dialogs/AgentCompatibilityField.tsx`) over `compatibilitySettings` and the diff helpers of `src/agents/launchCompatibility.ts`; no per-switch UI code |
| A select field, including a branch picker | `OptionSelect` (`src/components/form/OptionSelect.tsx`) owns the field and its option list, including virtualization; feature adapters supply the values and their presentation |
| A choice between following the parent, On and Off | `FollowSwitchSelect` (`src/components/form/FollowSwitchSelect.tsx`), as the Git workflow field and the compatibility pickers use it |
| A provider's name set apart in a sentence | `ProviderName` (`src/components/text/ProviderName.tsx`) |
| A provider's mark and colour | `providerMark`, `providerColour` (`src/providers/registry.ts`); `AgentIcon` (`src/components/icons/`) draws the mark, `providerFillVars` (`src/quota/providerFill.ts`) paints a quota bar |
| Which accounts a quota list shows | `listedQuotaAccounts` (`src/quota/quotaListing.ts`) |
| The tags offered to a tag field (projects and agents) | `tagVocabulary` (`src/components/form/tagVocabulary.ts`) with `TagsInput` |
| Platform capabilities (native menu, menu bar icon visibility, showing the daemon's keep-awake hold on the menu bar icon, launch at login, updater, clipboard, directory picker, permissions) | the `PlatformAdapter` through `src/platform/` |
| Follow the user leaving the window and coming back (whether the window has focus, a change of it, a return to it, a loss of it) | `windowHasFocus`, `subscribeWindowFocus`, `onWindowFocusReturn` and `onWindowFocusLoss` (`src/components/hooks/windowFocus.ts`), which follow the webview's own window `focus` and `blur` and `document.hasFocus()` in the desktop app as in a browser; they arrive only because `src/input/focusGuard.ts` swallows window-targeted focus events solely between a press's mousedown and the tick after the next frame, then replays a swallowed change. Users of the return: `usePoll`, `useMacosPermissionsRead`, `useNotificationPermission`, `useLaunchAtLogin`, `ImportConfigurationDialog`, `terminal/TerminalController.ts`; of the loss: `useQuickCreateModifier`, `tabDrag`, `AgentTabList`, `accountDrag`, `useNavigationHistory`; `useWindowAttended` uses the subscription and `windowHasFocus`, which `terminal/TerminalController.ts` also asks before it activates its terminal |
| A behaviour or a word that differs by operating system | on the client's machine, `PlatformAdapter.host` (`HostFacts`, `src/platform/host.ts`), read with `usePlatform()` or `useHost()` (`readHost()` for startup code with no adapter); on the daemon's machine (its paths, its Trash), `snapshot.host` in the store (`useHostPaths()`, the daemon's words in `platformTerms.ts`) |
| The daemon's view of each agent's CLI version and update, and which updates this window started | `agentCliStatuses` and `cliUpdatesStartedHere` of the store state (`src/store/state.ts`; `trackAgentCliUpdate` in `src/store/actions.ts`); the Version dialog and `AgentCliUpdateAlerts` read them |
| Hand focus back to an agent tab's actions button | `tabActionsTrigger` (`src/components/tabs/AgentTabList.tsx`) |
| Read the store | the barrel `src/store/index.ts` (`useDaemon`, `useDaemonStore`, `useDaemonStoreEqual`, session hooks) |
| The provider tree around `App` in a new host of the app | `AppRoot` (`src/AppRoot.tsx`), as `main.tsx`, the gallery frame and the website preview use it |
| Data for the gallery, the website preview and application tests | `src/fixtures/` (builders, `snapshotState`, `createFixtureDaemon`) |
