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
- `pnpm typecheck` — `tsc --noEmit`, including gallery, website-preview and test sources.
- `pnpm typecheck:app` — typechecks the production application through `tsconfig.app.json`.
- `pnpm test` — `vitest run`: the unit tests beside the code, plus jsdom tests for stateful classes and components.
- `pnpm test:app` — application tests, excluding the gallery, website preview and WKWebView benchmark tests.
- `pnpm build` — generates third-party license data, runs `typecheck:app` and the i18n catalog check
  (`vitest run src/i18n`), then writes `dist/` with relative asset paths.
- `pnpm build:preview` — builds the website demonstration into `apps/web/public/preview/` with
  `vite.preview.config.ts`.
- `pnpm bench:wkwebview` — a latency benchmark of the production bundle in the system WKWebView (macOS only); see
  [`scripts/wkbench/README.md`](scripts/wkbench/README.md).

### Pointing it at a daemon

Start `octoboardd` and read its port and key from the first line it prints on stdout,
`octoboardd listening on 127.0.0.1:<port> with key <key>`. The daemon makes a new key every time it starts and refuses
every connection that does not carry it, so the UI needs both. Either:

- open `http://localhost:5174/?port=<port>&key=<key>`, or set `VITE_DAEMON_PORT` and `VITE_DAEMON_KEY`; or
- run `OCTOBOARD_DAEMON_PORT=<port> pnpm dev` and open `http://localhost:5174/?key=<key>` (or set `VITE_DAEMON_KEY`),
  which reaches the daemon through a same-origin `/ws` proxy (`vite.config.ts`).

The key is only on that line: the daemon does not write it anywhere (`$TMPDIR/octoboardd.port` holds the port alone), so a
`VITE_DAEMON_KEY` kept in the repo-root `.env.local` (gitignored; template: `.env.local.example`) goes stale at every
daemon restart. The page never sends its address as a referrer (`index.html`), so the key in it stays between the page and
the daemon.

### The UI state gallery

A dev-only page, needing no daemon, that shows UI states hard to reach against a live daemon: the real `App` over a
fixture daemon (`src/gallery/fixtureDaemon.ts`). `pnpm gallery` serves it on port 5175; under `pnpm dev` it is at
`http://localhost:5174/gallery.html` (which resets the dev app's `octoboard.*` preferences, hence port 5175 for
`pnpm gallery`). `pnpm build` does not bundle it, and its type check (`typecheck:app`) leaves `src/gallery/` out except for
`src/gallery/fixtures/builders.ts`, which the dialogs' `*TestSupport.tsx` helpers pull in; `pnpm typecheck` covers all of it.

- Scenarios are listed by functional area, Normal previews first and Exceptional second. Theme, language, window
  width and height, and whether the shortcut modifier is held are controls above the window, all kept in the URL. The
  window is an iframe, so its width decides the `docked` and `phone` breakpoints for real.
- A scenario's page sets `data-gallery-ready` on `<html>` once its steps have run, its own toasts have been raised and
  finite animations ended; automation waits on it before capturing. A failure of the gallery's own machinery (a step
  that threw, a fixture that cannot be answered as the real daemon would, an exit transition that never ends) is painted
  over the page as a `[data-gallery-failure]` overlay (`src/gallery/banner.ts`); check a capture for it.
- A frame URL with `chrome=1` gives the page the desktop window's `windowChrome` capability (`src/gallery/frame.tsx`),
  so what only the desktop app offers can be driven in a browser.
  A scenario's `desktopApp` gives it that and the native menu bar as well, for a view that exists only in the app (the
  full Keyboard shortcuts list). Which keys are drawn (⌘ or Ctrl) and whether Back and Forward are listed follow the
  platform the gallery is viewed in; steps press keys with `ui.chord`, which turns a portable binding into that platform's.
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
  shortest `steps`, which press controls by name as a user would (`ui.press(ui.t("rail.settings"))`; `ui.pressTab`
  presses a tab where a checkbox or button before it carries the same name, as the import review's "What to import"
  checkboxes and its tabs do; `ui.confirm()` presses a dialog's confirming button, `ui.contextMenu` opens a row's
  right-click menu, `ui.touchPress` and `ui.tapTopBarStatus` press with a finger, for the popover a touch opens in place
  of a tooltip; `ui.swipe` swipes a finger across the window (the touch events `useSwipe` reads) to open or close a
  phone-width drawer or floating panel; `ui.hoverStatusMark` rests the mouse on a status mark and waits for its
  non-modal popover, where `ui.touchPressStatusMark` opens the press form with focus inside; `ui.openGitFailure` does
  the same for a git badge's failure mark, in a given sidebar row or in focus mode's header; a raw-text disclosure of an
  error alert is opened with `ui.press(ui.t("error.diagnostics"))`).
- Steps run at any width the page's width control picks (the frame reloads at the chosen width), so they name controls
  as they would in a docked window. A session row or rail control that sits only in the sidebar's closed drawer (inert
  below `docked` for the rows, below `phone` for the rail) makes the lookup press the drawer's toggle first, once the
  control has not shown up anywhere else for a moment. A panel of the right pane is not reached so: below `phone` the
  panels float, none is open and a closed one is not in the page, so a step that presses a control in one calls
  `ui.showPanel("files" | "terminal" | "report")` after selecting the session, which presses the panel's Show toggle and
  does nothing where its Hide toggle shows. Where a session pick leaves the drawer up over the terminal area the target
  is in (from 782px up to 1168px), follow the pick with `ui.closeDrawer()`, before the step that acts on the target.
- Other fields of the scenario type (`scenario.ts`) include `preferences` (what the app persists), `toasts` (a toast's `message` may be a function of `t`, worded in the viewing language; kind `"session_notice"` is routed as the daemon's own notice is),
  `sessionOpenError`, `terminal: "refuse"`, `clipboard: "unavailable"` (a plain browser over insecure http),
  `shellTerminal` (how the Terminal panel's shell terminals behave: `"shell"` at its prompt by default, or `"refuse"`; an exited shell
  always draws its last screen and closes, and `terminal` never reaches them; a shell the window starts draws a fresh prompt in the folder it was started in, where the sample shows a used one), `shellTerminalStart: "pending"` (a shell start that is never answered, for the panel's "Starting terminal…" state), `shellTerminalStartError` (a start the daemon refuses with that error), `quitRequested` (the platform gets a stubbed exit capability and
  asks the window to quit once the steps have run, so the quit confirmation is on screen; confirming ends nothing),
  `externalLinkError` (opening an external link fails with that text, as a refused desktop opener does; without it the link opens in a new tab as in the browser),
  `updater` (the desktop shell's updater, so the Settings version rows and the update notice exist: what a check answers, what an install reports and how it ends, and `checked` to have an automatic check already run when the window opens; it also gives the platform the exit capability a restart goes through) with `preferences.updateChannel`,
  `licensesDialog` (the third-party licenses dialog alone, over a list the scenario gives, in its loading or failed state, or as `{ failsOnce: list }`, which fails the first load and answers with the list from Try again: the real dialog reads the list the build generated, so `licensesGallery.tsx` mounts the real dialog with a loader answering that, since Settings > About cannot be given one),
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
- Sessions changing their model: `sessionModelChanges` stages what the daemon answers to a `set_session_model` the
  steps send: `"pending"` (the dialog has closed and the session is shown starting), or an error (an alert over the
  closed dialog). A running session is broadcast as starting while the request is held, with what its records showed
  forgotten, as the real daemon does; a `model_change_did_not_come_up` leaves it interrupted, any other error puts it
  back as it was, and success returns it to idle with the new values. An interrupted session only has the values
  recorded.
- Account sign-in in Settings → Agent accounts: `state.accountLogins` holds the logins the daemon already reports when
  the window opens (waiting with its page or device code, signed in, failed, timed out), so a login in progress is
  shown straight away; build them with the `loginOf` helper in `fixtures/settings.ts`. `loginOutcome`
  (`"succeeded"`, `"failed"`, `"timed_out"`) is how a login the steps start ends, a moment after it reaches waiting;
  left out, it stays waiting until cancelled. `loginStartError` makes `start_account_login` refuse with that error.
  `agentListError` makes `add_agent`, `remove_agent` and `reorder_agents` refuse with that error, for what the window
  could not have seen coming; left out, they follow the added list's own rules. `trustParentDirError` makes a
  `confirm_trust` that ticks the parent folder (`trust_parent_dir`) refuse with that error; a go-ahead for the project
  alone is always acknowledged.
  `claudeCompatibilityError` makes an `update_settings` carrying `claude_compatibility` (an agent's Claude compatibility
  switch) refuse with that error.
  A scenario's `state.settings.claude_compatibility` holds the user's explicit choices, as the daemon stores them; the
  window is told the value in effect (`effectiveClaudeCompatibility`, `builders.ts`), so Codex reads on only while Claude
  Code is among `added_agents` unless chosen, and follows the added list when a step changes it.
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
- Removing an account in Settings → Agent accounts: `accountDeleteError` makes a `delete_account` that asks for the
  account's directory to be moved to the Trash (`trash_directory`) refuse with that error (the account kept); a removal
  that does not ask for it is not refused. `accountTrashInspection` is what the `inspect_account_trash` the confirmation
  sends each time the option is ticked (and again after a refusal the directory's state may decide, other than a login that did not end) answers: an
  `AccountTrashOutcome`, a `FixtureError` that fails the request, or `"pending"` for one never answered, or a list of
  them for the asks in order (the last repeating); left out, `ok`. The sessions the confirmation lists are the live ones
  of the state, matched by `account_id` or `config_dir`, so a state's sessions decide that list.
- Accounts turned off in Settings → Agent accounts: `Account.disabled` on an owned account and
  `Settings.disabled_default_accounts` (agent wire names) for an agent's default account, both written in the state's
  `settings`. The fixture daemon, like the real one, records `set_account_disabled` (so the Enabled switch works), refuses
  opening a session on, handing off to and switching to a disabled account with `account_disabled` before it looks at the
  quota (and, after the quota, one whose last reading says it is not logged in with `account_not_signed_in`), and leaves
  the sessions already on the account alone. The pickers hide a turned-off account, apart from the one a console,
  project or session holds.
- `configuration` sets what the platform and daemon answer to configuration export and import
  (`src/gallery/fixtureConfiguration.ts`): `file` (`"dismissed"`, `"pending"`), `readError`, `saveError`, `downloads`,
  `unavailable`. Exporting needs nothing; an inspection and an import result are written out per scenario, and the
  fixture daemon refuses an import result that does not follow from what the steps asked for. Like the real daemon, it echoes the document's `keyboard_shortcuts` in a result's `settings_applied` (the client applies them
  itself), so a result is written without them.
  `fixtures/configuration.test.ts` holds the written results to the daemon's rules.
- Write no `width` unless the window itself is part of the scenario (the page opens at 1440). A copy of a scenario made
  to open the drawer first, or because its layout differs at a width, earns nothing: choose the width with the control,
  and write the pointer ("choose 560px to see the rows stack") in the description of the scenario that shows the state.
  Set a `width` only where the target can be reached only in a band of widths the default 1440 is outside of, and steps
  cannot get there at whatever width the control picks: a swipe, or the open drawer card itself, where pressing the
  drawer's toggle is fine because the open drawer is the target (`focus-console-session-drawer`,
  `focus-project-drawer`). Pick a width inside that band, and say in the title or description what it buys. What exists
  only in a docked window (a card floated in by hovering a toggle, a pane's width handle, Maximize panel, the hidden
  docked sidebar) states `minWidth` instead, the narrowest window it exists in, which a width chosen with the control or
  by `--width` is raised to; a scenario carries one or the other, and a comment beside `minWidth` says why. Write a
  `height` only where a step needs the window too short for a dialog's body, which then scrolls (`ui.tabToRegion` finds
  none in a tall window). A scenario that merely wants to be seen narrow or short uses the page's width and height
  controls. A chosen width stays on scenarios that carry none, and is dropped on picking one that carries its own.
- `held: true` opens a scenario with the shortcut modifier held, for a view that exists only while it is held: the
  new-session "+" buttons' quick form. The viewer can still choose Released with the page's Modifier control, and like
  a width that choice stays on scenarios that carry none and is dropped on picking one that carries its own. The
  modifier is pressed after the steps, so they find those buttons by their plain names; `ui.pressWithModifier`
  presses a control with the modifier down for that press alone, for what a press does when the page has not seen
  the modifier held.
- Live changes are written with the builders in `fixtures/liveChanges.ts` (`accountAdded`, `accountRemoved`,
  `agentAvailability`, `terminalOpened`, `terminalClosed`, `pagePushed`, `latestPageDeleted`, `allPagesDeleted`), not raw `daemonEvents` steps; add a builder there for another
  kind of collection.
- Steps name a control in the language the window is in, so copy is never written out: controls through `ui.t`, a
  session row through `ui.session`, a change row through `ui.changeRow`. A name that is not copy (a branch, a file, a
  title the fixture sets) is a plain string or a test of the name; a matcher is never a regular expression.
  `fixtures.test.ts` fails a step that names a control by the English text of a message another language's catalog
  translates, bare or filled, and a test of the name that takes a name followed by the English catalog's separator
  and refuses the language's own (a literal comma and space, where `zh-Hans` writes `，`); a custom test of the name
  that stands for copy must be built from the helpers, and may be given a description with `describing(...)` (what a
  failed step reports).
- The fixture daemon refuses a reply it cannot fill faithfully rather than sending a field unfilled: where the real
  daemon counts a change side's `lines`, the fixture carries the text to count it from (`FixtureChange.whole`), or the
  request rejects with `FixtureUnfillableError`. Change fixtures are therefore written through the factories in
  `fixtures/changeFixtures.ts` (`textChange`, `deletion`, `untracked`, ...), never as literals. A rule over a whole
  repository that no single change shows is held by a test over the sample repository (`objectIdCheck` in
  `fixtures/invariants.ts`); add such a rule there.
- A reply that gains a field the UI draws from is taught to the fixture daemon together with the condition the real
  daemon fills the field under, and every scenario's fixture is given what that condition needs, those that answer
  with an error included (they answer the reply too, and an unfilled field there is a state the real daemon never
  sends). Take the condition from the daemon's own code and tests, never from a list of cases in a doc or a plan,
  which can lag the rule; and never import a UI helper that happens to compute the same thing: the fixture stands on
  the daemon's side of the protocol, so repeat the rule inside `src/gallery/` (as `lineCount` in `fixtureGit.ts`
  repeats the daemon's count), since UI code is free to change for the UI's reasons.
- Hold the condition in a signature before holding it at run time: a field the reply gains goes first into the
  parameters of the `changeFixtures.ts` factories whose kind of change the real daemon fills it for, because an
  unfilled field is a legal value of the reply and only a parameter that has to be passed stops a fixture that cannot
  be answered faithfully being written. Leave to the run-time refusal only a condition that depends on the data rather
  than on the kind of change. Extend `refuseUnfillable` (`banner.ts`) to each new field of this kind, and have the
  guard test over the fixture daemon's replies read every scenario's reply, those carrying no such field included, so
  a refusal fails the test run and not only the scenario someone opens. That test (`fixtures/projectGit.test.ts`,
  "answer every read a scenario's repository offers without a refusal") takes the repositories from `SCENARIOS` (the
  sample one every scenario without a `git` is answered from included), so a new fixture is read without being named.
  It lets a daemon error through only where the fixture stages that very error for the change, and checks that each
  read reached its target, so a mis-addressed one cannot pass as an error. A `FixtureUnfillableError` is deliberately
  not a `DaemonRequestError`: a scenario's daemon error is a legitimate UI state to show, so a gap in the gallery's
  own data must not be dressed as one that a test, or someone looking at the page, would let pass.
- Write a fixture daemon handler only for a request some scenario sends, and delete one left unused: no scenario
  renders its reply and no guard test reads it, so it is an unfaithful reply nothing can catch, waiting for whoever
  adds the first scenario that asks.
- A fixture field says what a reply costs (`expand.delay`, `expand.error`), never whether it is given. Whether the
  daemon answers at all follows from the change's own data (its group, its sides, whether making its patch already
  failed), and the fixture daemon reads it from there; gating on `expand`, which only some scenarios carry, refused a
  deletion, a binary change and a change past the patch budget with an `invalid_change` the real daemon never sends
  there. Likewise a factory parameter carries the content a reply needs, not the right to ask for it.
- A scenario that answers a request with a daemon error on purpose uses the code the real daemon sends for that
  request in that situation, taken from its handler and `apps/daemon/PROTOCOL.md`, not one that merely sounds right:
  the UI words each code its own way, so `list_dir` refused with `permission_denied` instead of the
  `directory_unreadable` the daemon sends for an unreadable directory shows copy the app never shows.

#### Sweeping the gallery

`pnpm gallery:sweep [ids or patterns]` loads the selected scenarios (all by default) in a headless browser and fails on
any gallery failure; `pnpm gallery:sweep --help` lists the flags (`--group`, `--lang`, `--theme`, `--width`, `--list`,
`--url`, ...). `--width 420,800` loads each scenario that carries no width of its own at those widths, as the page's
width control would, to prove that its steps hold in a narrow window. It loads the scenarios it is given, every time;
run it on the scenarios just written or changed, at the default width and again with `--width 420,800`, and on the ones
shown for confirmation.

Playwright installs no browser: run `pnpm exec playwright install chromium` from `packages/ui` once, or point
`--chromium <path>` / `OCTOBOARD_CHROMIUM` (or `--playwright <dir>` / `OCTOBOARD_PLAYWRIGHT`) at an existing one.

A sweep loads English unless `--lang` names more. A translation is held where it is written: each catalog is typed
`Translation<...>`, so a missing message or plural category fails type checking, `src/i18n/catalog.test.ts` checks the
placeholders and plural categories of every catalog, and `src/gallery/fixtures/fixtures.test.ts` fails a step that
names a control in English copy or with the English separator, which would find nothing in another language.
`--lang <list>` names other languages and `--lang all` sweeps every language with a catalog (`en`, `zh-Hans`). The one
check that runs only for English is the archived session's Reopen in `history-loaded`, which needs the real pointer;
the name Reopen carries in each language is held by `TerminalPane.test.tsx`.

The page loads wait for a free heavy-step slot, the machine-wide lock `pnpm verify` also takes (`OCTOBOARD_VERIFY_SLOTS`
sets how many), and say who holds it and who is queued ahead while they wait, so sweeps in several worktrees queue
rather than overload the machine. The queue is first come, first served: a sweep run again after another run started
waiting lines up behind it.

## Layout

Imports run one way between the feature directories: `src/panels/` imports `src/browser/`, `src/browser/` imports
`src/viewer/`, and `src/viewer/` imports neither `src/browser/` nor anything that does. Code they share lives in
`src/components/`, which imports none of them. Nothing enforces this.

### Build, entries and scripts

| Path | Role |
|---|---|
| `vite.config.ts`, `typography.mjs` | The Vite and Vitest configuration (React Compiler, Tailwind, the plugins serving the app name, the gallery frame's Content Security Policy and the viewer theme shim, and the failed-test-files reporter `pnpm verify` asks for) and the Tailwind typography plugin wrapper |
| `index.html` | The window's entry page: its Content Security Policy and the script that applies the theme class before first paint |
| `website-preview.html`, `vite.preview.config.ts`, `src/website-preview/` | The fixture-backed build embedded by the public website: mounts the real `App` over `createPreviewDaemon` with sample repositories, three scenes and simulated agent terminals, and relays scrolling, input modality and report injection to the embedding page while keeping keyboard focus on the homepage until the visitor moves into the frame and letting Tab leave it again; the iframe and scene buttons belong to [`apps/web/`](../../apps/web/README.md) |
| `gallery.html`, `gallery-frame.html`, `src/gallery/` | The dev-only UI state gallery (see "The UI state gallery"): the shell and frame pages, the scenario type and step helpers, the fixture daemon, and `fixtures/` (the scenarios and their builders and groups); `preview/` is per-checkout and untracked |
| `scripts/gallery-sweep.mjs` | `pnpm gallery:sweep`, with its tests and `test-fixtures/` (see "Sweeping the gallery"); `gallery-chip-geometry.mjs` and `gallery-report-focus.mjs` are standalone Playwright checks run by hand against a served gallery |
| `scripts/generate-licenses.mjs` | `pnpm licenses:generate` (also the first step of `pnpm build`): writes the gitignored `src/about/licenses.generated.json` from this package's production dependencies and the Cargo workspace's |
| `scripts/wkbench/` | `pnpm bench:wkwebview`, the WKWebView latency benchmark (see [its README](scripts/wkbench/README.md)) |
| `src/test/` | Vitest setup and the reporter that lists failed test files for `pnpm verify` |

### Application shell and daemon connection

| Path | Role |
|---|---|
| `src/main.tsx`, `src/App.tsx` | Startup (platform, daemon location, connection, theme, native window reveal, global error handlers) and the screen: window chrome, the content panel, session selection and resume, toasts, connection banner and the dialog layer |
| `src/StartupScreen.tsx`, `src/ErrorBoundary.tsx`, `src/WelcomeScreen.tsx` | The screens for no daemon connection, a UI crash, and a daemon with no console |
| `src/platform/` | `PlatformAdapter`, the interface to everything native, made of optional capabilities; implemented by `tauri.ts` (the only importer of `@tauri-apps/*`) and `browser.ts`, exposed by `react.tsx` |
| `src/daemon.ts`, `src/daemon-client.ts`, `src/protocol.ts` | Locating the daemon (`resolveDaemonAddress`: its origin and the key it requires, from the page's address; `resolveRecovered`: whether the shell restarted it), its WebSocket client for `GET /ws/control` (request correlation, reconnect, event dispatch), and the hand-written TypeScript mirror of the wire protocol |
| `src/store.ts` | The Zustand store of the connection and the state derived from daemon events: `createDaemon`, `useDaemon`, `useDaemonStore`, `useDaemonStoreEqual`, `useErrorRecords` and the `onToast` stream |
| `src/sessions/`, `src/sameValue.ts`, `src/waiting.ts` | `indexSessions` and the hooks reading session groups, deep equality, and the sessions waiting for the user in sidebar order |
| `src/accountSwitchBatch.tsx`, `src/accountSwitchFailureAlert.tsx` | `runAccountSwitchBatch`, the app-level runner of a batch of session account switches that outlives the dialog starting it, and the error alert for its failures |
| `src/preferenceKeys.ts`, `src/persistedPreference.ts` | The `localStorage` key of every persisted preference and `createPersistedPreference`, the guarded store behind them |
| `src/gitWorkflow.ts` | The git workflow settings' defaults, choices and change detection |
| `src/appConfig.ts` | The app-level facts from the repo-root `config/app.json`; `src/appConfig.test.ts` checks the copies that must live elsewhere against it |

### Failures, language and theme

| Path | Role |
|---|---|
| `src/daemonMessage.ts`, `src/errorPresentation.ts`, `src/errorText.ts`, `src/errorRecovery.ts`, `src/actionableError.ts`, `src/uncataloguedFailure.ts` | Turning a failure into words: localized daemon wording, `ErrorPresentation` (the shared failure value and the predicates for how it is worded), plain-text forms, `ErrorRecovery` / `classifyError` (known user remedies), `ActionableError` (a local failure that carries its remedy) and `isUncataloguedFailure` (whether the application has wording of its own for a failure); the last two are separate files so the error modules do not import each other in a cycle (`src/errorModulesCycle.test.ts` checks it) |
| `src/errorAlerts.ts`, `src/errorAlertParts.tsx`, `src/reportError.tsx`, `src/runtimeError.ts` | The window's error queue, an alert's title, body and details, structured failure submission, and extracting exceptions from browser error events |
| `src/issueReport.ts` | `issueReportUrl`: the prefilled GitHub new-issue address behind an unexpected-failure alert's Report issue link, with `sanitizeReportText` and `recordNames` removing what is the user's from every text that goes into it |
| `src/i18n/` | Internationalization: languages and system-language matching, the typed English catalog and translated ones (`messages/`), lookup, the language choice, the React side (`useT`, `Message`, `LanguageProvider`) and display-width text measuring; every user-facing string goes through it |
| `src/theme.tsx` | `ThemeProvider` / `useOctoboardTheme`: the light, dark or system choice |
| `src/style.css` | The Tailwind theme and the window's design tokens |

### Feature directories

| Path | Role |
|---|---|
| `src/components/` | Shared building blocks: window chrome (`TitleBar`, `Rail`, `ContentPanel`, `Toasts`), panel headers, `PaneToolbar` (the strip under a panel header that holds a field) and resize handles, menus (`ActionMenu`) and `Hint`, the one component behind every tooltip and floating popover, `Tag` (shared removable tags for inputs and read-only tags for summaries), status and error presentations (`StatusMark`, `DotStatusBadge`, `MarkLineBox`, `ErrorAlert`, `ChoiceWarning`, `FailureAlert`, `PaneFailure`, `DaemonErrorMessage`, `DiagnosticBlock`), scroll-fade and list-motion helpers, focus-handoff hooks, `useAccessibilityHidden` (whether a container or its ancestors hide it from assistive technology), `useBottomStrip` (the bottom strips' shared behavior), `AttendedStatusAnnouncer` (announcing a status that can arrive while the user is out of the window) and `ExternalLink`. Alerts go through `ErrorAlert` (and `FailureAlert` and `ChoiceWarning`, built on it); `ErrorAlert` and `AlertFrame` are the only importers of HeroUI's `Alert` parts (enforced by `eslint.config.mjs`) |
| `src/layout/` | Pane layout: the `docked` and `phone` breakpoints, pane widths, the sidebar drawer and the aside's frame (`AsidePane`), whose content the aside shows (`asideOwner.ts`), pane toggles, F6 region cycling, and the touch swipe that opens and closes the drawer and the floating panels below `phone` (`useSwipe`, `paneSwipe`) |
| `src/shortcuts/` | The central registry of keyboard shortcuts: command definitions, the portable `Binding`, validation, the persisted per-client overrides and the key recorder `recordKeys`; the Settings section is `src/settings/ShortcutsSection.tsx` |
| `src/navigation/` | Back and Forward through the locations the content area has shown |
| `src/lifecycle/` | Hooks tying the UI to the platform: the exit flow (`useAppExit`), notifications, the native menu and menu bar icon, native window theme, run in background, the git status schedule and reporting the interface language to the daemon |
| `src/sidebar/` | The sidebar: one console's sessions and projects, the focus modes, rows and action menus, the project filter and new-session controls |
| `src/archive/` | `ArchiveLayer` and `ArchiveView`: archived sessions listed over the terminal area, with deletion |
| `src/terminal/` | The xterm.js terminal: `TerminalController`, `TerminalPane`, an archived session's history view, the project's shell terminals (`ShellTerminalPanel`) and terminal path links |
| `src/report/` | A session's report: `ReportPanel` and the sandboxed page frame, `listPages` (one page listing per session shared between the panel and the window's check) and `useProjectReportSession` (the project session whose Report panel a project's right pane carries) |
| `src/panels/` | The right pane, a stack of inset panel cards: `RightPane`, `PanelStack`, the registry of panel kinds (stacked and toggled in the order Report, Files, Processes, Terminal), `panelMeta.ts` (each kind's name and icon, which the sidebar's menus use without importing the panels) and the persisted per-owner stack state, with a project's Report open or closed per session |
| `src/browser/` | The project browser, the Files panel's content: `ProjectBrowser` with its Files tree and search, and the Git mode's Changes view over uncommitted changes, branch comparison and commits; also the viewer over a file a terminal path link named |
| `src/viewer/` | The read-only file viewer modal: `FileViewer` over the application-owned `ViewerSubject` model, with code, diff and Markdown surfaces; `renderer.tsx` is the only importer of `@pierre/diffs` |
| `src/quota/` | Agent account plan quota: wording and used-up judgement, the meters and usage bars, and `RailQuotaEntry`, the rail's quota entry and panel |
| `src/settings/` | The settings modal: `SettingsDialog` and its sections (general, git, agent accounts and the added agents, where an account opens a read-only details dialog (`AccountDetailsDialog.tsx`, with the account's email) or a settings dialog for sharing and turning it off (`AccountSettingsDialog.tsx`), and removal's confirmation with its option to move the account's directory to the Trash (`RemoveAccountDialog.tsx`), with each agent's Claude compatibility switch in `AgentClaudeCompatibilitySetting.tsx` and `hasClaudeCompatibility` (`claudeCompatibility.ts`) saying which agents have it, agent behavior, trusted folders, macOS permissions, notifications, shortcuts, configuration, About) |
| `src/dialogs/` | The console, project, session, rename, confirmation, directory-picker and daemon-question dialogs on the shared `Dialog` frame, `ErrorAlerts` (the acknowledgement host), `ConfirmConsequences` (what a confirmation says it takes along: sessions and lines), `useAfterErrorAlert` (runs something once a failure's alert has closed), `useFailureWhenBlocked` (turns a dialog unusable as it stands into a failure modal) and the shared form controls; `SessionModelDialog.tsx` changes a session's model and effort, using the `LaunchPickers` and `useLaunchModels` shared with the Advanced sections in `LaunchSettingsFields.tsx`, and `sessionModelLauncher.ts` connects the window to controls opening that dialog from outside it; `RequestedDialog` also confirms an archive or removal with its branch and process choices, on `useSessionBranches` (the archive and removal inspections), `BranchCleanupOptions`, `ProcessStopOptions` (`StoppedRow`, `useProcessStopping`) and `OptionDetails` (the collapsed details of an option) |
| `src/process/` | The Processes panel and what feeds it: `ProcessesPanel` (a session's or project's process trees, each with Stop and Keep), `ProcessSessionSelect`, `useProcessPanel` (the panel's data, given to it through `ProcessPanelProvider`), `useProcessTrees` (one target's listings, polled) and `useProjectProcesses` (the project processes a project removal lists) |
| `src/configuration/` | Configuration export and import: `ExportConfigurationDialog` (chooses the regions to export independently) and `ImportConfigurationDialog`, which shows the review (`ImportReview` as the frame with its "What to import" region checkboxes, `RegionTabs`, and one panel per region: `ImportConsoles`, `ImportAccounts`, `ImportSettings`) and then the result (`ImportResult` with `ResultCards`); `importParts.tsx` holds the pieces they share and `importPlan.ts` the pure plan (regions, account review state, fallbacks) |
| `src/about/`, `src/updates/` | The third-party licenses dialog and its data (the generated `licenses.generated.json`, with the committed `licenses.fallback.json` standing in until the generator has run; `src/cssDependencies.test.ts` checks that every package a stylesheet names is a production dependency, so it reaches that list); the self-update on the UI side: `createUpdateStore`, `UpdatesProvider`, the persisted channel and `UpdateNotice` |

### Shared helpers

| Path | Role |
|---|---|
| `src/agents.ts`, `src/agentLaunchSettings.ts`, `src/agentFields.ts`, `src/accountChoices.ts` | The agents' names, listed agents and install pages, model and effort picker rules, per-agent dialog fields and the pure account-picker rules |
| `src/accountOrder.ts` | `orderedAccounts`: the shared saved display order of an agent's accounts for settings, pickers and quota surfaces |
| `src/processes.ts`, `src/processFacts.ts`, `src/components/ProcessTreeDetails.tsx` | `listProcesses`, `stopProcesses` and `keepProcesses`, the client-side process requests; `processFacts`, `ProcessCommand`, `ProcessTreeDetails` and the `StopGlyph` icon, the process wording shared by the Processes panel and the archive and removal choices |
| `src/sessionLabel.ts`, `src/sessionColours.ts`, `src/gitStatusLabel.ts`, `src/projectFiltering.ts`, `src/sessionCopy.tsx`, `src/gitStatusCopy.tsx`, `src/sessionModel.ts`, `src/repositoryLink.ts` | Plain and rich session and git-badge wording, session colours, the sidebar filter's rules, a session's model and effort for display, and the repository host link |
| `src/useWindowShortcut.ts`, `src/useQuickCreateModifier.ts`, `src/useNow.ts`, `src/focusGuard.ts`, `src/contextMenuGuard.ts`, `src/touchInput.ts` | Window-wide shortcut and modifier hooks, a shared clock, a react-aria focus workaround, native context menu suppression and touch-input queries |
| `src/pathDisplay.ts`, `src/wirePath.ts`, `src/relativeTime.ts`, `src/conversationId.ts`, `src/reducedMotion.ts`, `src/imeKey.ts`, `src/avatarImage.ts`, `src/cssColour.ts` | Small pure helpers for path display, wire paths, relative time, conversation ids, motion preference, input-method keys, avatar images and resolving a CSS colour |
