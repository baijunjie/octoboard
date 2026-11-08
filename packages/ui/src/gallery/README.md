# UI state gallery

A dev-only page that shows UI states hard to reach against a live daemon: the real `App` over the fixture daemon of
`packages/ui/src/fixtures/`, one scenario at a time. It needs no daemon, is not bundled by `pnpm build`, and its sources
are left out of the production type check (`typecheck:app`) apart from `fixtures/builders.ts`, which the dialogs'
`*TestSupport.tsx` helpers and the agent accounts section's test state import; `pnpm typecheck` covers all of it.
`pnpm gallery` serves it on port 5175 and opens it. Under `pnpm dev` it is at `http://localhost:5174/gallery.html`,
which resets the dev app's `octoboard.*` preferences (hence port 5175 for `pnpm gallery`).

## How it works

- `gallery.html` loads `shell.tsx`, the scenario list and the controls: theme, language, window width and height,
  whether the shortcut modifier is held and, for a file-viewer scenario (`viewer`), the Markdown style. Everything
  chosen lives in the URL. On a scenario whose frame shows a modal (every viewer scenario) the modal pulls focus back
  into the frame, so the shell's scenario list and controls can only be used with the mouse there (a known limitation).
  Scenarios are listed by functional area, normal previews first and exceptional second. The chosen scenario renders in
  an iframe (`gallery-frame.html`, `frame.tsx`), so the width control decides pane docking from the available space and the inclusive `tablet` breakpoint for
  real. The frame alone, sized by the browser window, is `gallery-frame.html?scenario=<id>&theme=…&lang=…` with `held=1`
  and `chrome=1` as needed, and, for a file-viewer scenario, `markdown=tailwind|github|newsprint|latex` (it stands over
  the scenario's own `preferences.markdownStyle`).
- `frame.tsx` renders the real `App` under `AppRoot` (`src/AppRoot.tsx`, the provider tree `main.tsx` and the website
  preview share) over a fixture daemon in place of the WebSocket one, and leaves out what only a native shell does. `prepare.ts` sets the app's stored
  preferences from the URL and the scenario before the app's modules load, so nothing the frame imports earlier may read
  a stored preference at load (the same holds for `fixtures/index.ts` and `updaterFixture.ts`). The two choices the
  desktop shell holds (a scenario's `shellSettings`) are not stored: the frame sets them on their preference objects
  once the app's modules are loaded.
- A `Scenario` (`scenario.ts`, extending `Fixture` in `packages/ui/src/fixtures/fixture.ts`) is a starting `state`,
  optional staging of the fixture daemon and platform (errors, never-answered requests, an updater, permissions, a
  clipboard, a terminal behaviour, and so on, each documented where it is declared), `steps` that reach component-local
  state the way a user does, and `liveChanges`.
- The frame page (`gallery-frame.html`, run by `frame.tsx`) sets `data-gallery-ready` on its `<html>` once its steps have
  run, its own notifications have been raised, visible lazy content has committed and finite animations have settled.
  The frame enables the observer in `packages/ui/src/components/hooks/lazyReadiness.tsx`; Suspense boundaries expose
  their pending lifetime through `trackSuspenseFallback`, and `retryableLazy` reports import failures and commits.
  Nested lazy content revealed by a resolved parent is included; intentional pending fixture requests are not waited
  out. A failed visible import or the 15-second lazy readiness limit raises a gallery failure. The shell
  (`gallery.html`, `shell.tsx`) only reads readiness from its iframe and never sets it, so automation opens the frame
  URL directly and waits for readiness before capturing. A failure of the gallery's machinery (a step that threw, a
  fixture that cannot be answered as the real daemon would, lazy content that failed or never settled, an exit
  transition that never ends) is painted over the page as a `[data-gallery-failure]` overlay
  (`packages/ui/src/fixtures/banner.ts`); check a capture for it even when the page signals ready.
- `chrome=1` in a frame URL gives the page the desktop window's `windowChrome` capability (the top bar's drag region, and
  on a macOS host the window's own Back and Forward keys), while keeping the browser platform kind. It does not make the
  page the desktop app: what the app alone has by its platform kind (the console-session switch, which exists in the app on
  any system; the Back and Forward rows of Settings, on macOS; a report page's relay of those keys) needs the scenario's
  `desktopApp`, which gives the page that kind, the window chrome and the native menu bar, for a view that exists only in
  the app (the full Keyboard shortcuts list). A scenario's `host` gives the page the facts a desktop shell reports about its machine (`HostFacts`: the
  system's words such as Recycle Bin, which Settings sections exist, no window controls over a native chrome), with the
  snapshot's `state.host` and `state.homeDir` saying the same of the daemon's machine (`WINDOWS_MACHINE`); without one the
  browser's own facts stand, and a host with no native menu (`appMenu: false`, the Windows app's) gives a `desktopApp`
  scenario none. Which keys are drawn (⌘ and symbols, or Ctrl and text caps) and whether Back and Forward are listed follow
  the scenario's `host`, or the platform the gallery is viewed in where it has none; steps press keys with `ui.chord`, which
  turns a portable binding into that host's.
- A scenario with `liveChanges` lists them as buttons beside the other controls, offered once its page is ready. Each
  makes what another window or the machine would while the view is on screen (an account added, a terminal closed there,
  an agent found missing), as the daemon's own events through the steps' `daemonEvents`, for the motion and focus of a
  collection that changing under a view shows. A pointer press on a button does not take focus from the window (a
  keyboard press does move it out). A change's `id` in a frame URL's `live` parameter (comma-separated) applies it after
  the steps, which is how the sweep runs every one.
- Report scenarios expose the document event `gallery:report-page-created` through `reportPageFixture.ts`, applying
  fixture pages through the store's `applyDaemonEvent`. `node scripts/gallery-report-focus.mjs <gallery-origin>`, run in
  `packages/ui`, drives report replacement and focus checks against a served gallery;
  `scripts/gallery-chip-geometry.mjs` is a standalone check of the same kind. Both are run by hand.
- `src/gallery/preview/` is git-ignored, so it exists per checkout: every `.ts` or `.tsx` file directly in it (not in a
  subdirectory) must default-export a `Scenario[]`, which `fixtures/index.ts` appends to `SCENARIOS`; anything else goes
  in a subdirectory. Those scenarios are listed, swept and held by the gallery's tests in that checkout only. Such files
  import fixtures from their own modules, never from `fixtures/index.ts` (an import cycle).

## Preview widths

The Width presets use CSS viewport pixels: Desktop (1440), Tablet landscape (1024), Tablet portrait (768), and Phone
(390). The tablet widths match the iPad Mini in landscape and portrait; the phone width matches the iPhone 13. Desktop
is a representative desktop window width. These presets change only the width; Height remains independent.

Use the numeric width field for other devices or layout boundaries, such as 983/984 (one right column with the sidebar), 1253/1254 (two columns with the sidebar),
993/994 (two columns with the sidebar hidden), and 768/769 (single-panel versus multi-panel interaction). The fixed
viewport boundaries are 390, 768 and 1024px: 768px and narrower uses the drawer and one panel at a time, below 390px
uses compact spacing, and desktop presentation starts at 1024px. Pane docking is calculated independently from the
space required by the open columns. A scenario with `minWidth` raises a narrower selection to that minimum,
so a view that only exists in a docked window remains docked; the numeric field shows the actual preview width.

Nothing in `packages/ui/src/fixtures/` or `packages/ui/src/website-preview/` imports this directory (enforced by the
`no-restricted-imports` blocks in `eslint.config.mjs`). How the fixture daemon answers, and the rules for teaching it a
new reply, are in [`../fixtures/README.md`](../fixtures/README.md).

## Conventions for writing scenarios

- **Where.** Scenarios live in `fixtures/` (the file for their area, or a new file listed in `fixtures/index.ts`).
  `group` is one of the titles in `GROUPS` (`fixtures/groups.ts`); a new group gets the icon its heading carries. The
  required `category` is `"normal"` (expected UI, including empty, loading and platform-limited views) or
  `"exceptional"` (deliberate failures, validation errors, refusals, conflicts), chosen by the view being confirmed
  rather than by the component or file name. Keep failed subjects out of normal navigation.
- **State.** `state` is built with `snapshotState` from `fixtures/builders.ts`. Build consoles and projects with
  `consoleWith` and `projectWith`, or give the `Map` its type (`new Map<string, Console>(...)`): an object built by
  spreading into an un-annotated `new Map([...])`, `.map(([id, x]) => [id, { ...x, … }])` included, is not checked for
  leftover keys, so a renamed or removed protocol field survives in the fixture and the scenario silently shows the
  wrong state. Helpers for staged state (`switchingAccount`, `accountSharingOf`, `branchItemOf`, `worktreeItemOf`,
  `effectiveProviderSettings`) are in `builders.ts`; reset-credit readings are built with `resetCreditOf` and
  `resetCreditsOf` (`fixtures/quotas.ts`). A scenario's `state.settings.provider_settings` holds the user's explicit
  choices of provider settings, as the daemon stores them; the window is told the value in effect.
- **Steps.** State that lives in a component rather than the store (a dialog, the archive view, the selected session) is
  reached with the shortest `steps`, which press controls by name as a user would (`ui.press(ui.t("rail.settings"))`).
  The step driver's helpers (`ui.pressTab`, `ui.confirm`, `ui.contextMenu`, `ui.touchPress`, `ui.tapTopBarStatus`,
  `ui.swipe`, `ui.hoverStatusMark`, `ui.touchPressStatusMark`, `ui.openGitFailure`, `ui.showPanel`, `ui.closeDrawer`,
  `ui.pressWithModifier`, and so on) are documented on the `Ui` interface in `fixtures/interact.ts`. A raw-text
  disclosure of an error alert is opened with `ui.press(ui.t("error.diagnostics"))`.
- **Widths.** Steps run at any width the page's width control picks (the frame reloads at the chosen width), so they
  name controls as they would in a docked window. A session row or rail control that sits only in the sidebar's closed
  drawer (inert while the sidebar floats for the rows, at 768px and narrower for the rail) makes the lookup press the drawer's toggle
  first. A panel of the right pane is not reached so: at 768px and narrower the panels float, none is open and a closed one is
  not in the page, so a step that presses a control in one calls `ui.showPanel("files" | "terminal" | "report")` after
  selecting the session. Where a session pick leaves the drawer up over the terminal area the target is in (the floating
  sidebar, above 768 px while it cannot dock or is hidden), follow the pick with `ui.closeDrawer()`.
- **Width and height fields.** Write no `width` unless the window itself is part of the scenario (the page opens at
  1440). A copy of a scenario made to open the drawer first, or because its layout differs at a width, earns nothing:
  choose the width with the control, and write the pointer ("choose 560px to see the rows stack") in the description of
  the scenario that shows the state. Set a `width` only where the target can be reached only in a band of widths the
  default is outside of and steps cannot get there at whatever width the control picks (a swipe, or the open drawer card
  itself, where pressing the drawer's toggle is fine because the open drawer is the target:
  `focus-console-session-drawer`, `focus-project-drawer`); pick a width inside that band and say in the title or
  description what it buys. What exists only from a breakpoint up (a card floated in by hovering a toggle, above 768 px;
  a docked pane's width grip and Maximize panel, from the width its visible panes need) states `minWidth` instead, the
  narrowest window it exists in, which a width chosen with the control or by `--width` is raised to; a scenario carries
  one or the other, and a comment beside `minWidth` says why. Write a `height` only where a step needs the window too
  short for a dialog's body, which then scrolls (`ui.tabToRegion` finds none in a tall window). A chosen width stays on
  scenarios that carry none, and is dropped on picking one that carries its own.
- **Held modifier.** `held: true` opens a scenario with the shortcut modifier held, for a view that exists only while it
  is held: the new-session "+" buttons' quick form. The viewer can still choose Released with the page's Modifier
  control; like a width, that choice stays on scenarios that carry none and is dropped on picking one that carries its
  own. The modifier is pressed after the steps, so they find those buttons by their plain names.
- **Markdown style.** The style chosen with the Markdown style control carries over to viewer scenarios that set no
  `preferences.markdownStyle` and is dropped on picking a viewer scenario that sets its own.
- **Live changes.** They are written with the builders in `fixtures/liveChanges.ts` (`accountAdded`, `accountRemoved`,
  `agentAvailability`, `terminalOpened`, `terminalClosed`, `pagePushed`, `latestPageDeleted`, `allPagesDeleted`), not
  raw `daemonEvents` steps; add a builder there for another kind of collection.
- **Copy.** Steps name a control in the language the window is in, so copy is never written out: controls through
  `ui.t`, a session row through `ui.session`, a change row through `ui.changeRow`. A name that is not copy (a branch, a
  file, a title the fixture sets) is a plain string or a test of the name; a matcher is never a regular expression.
  `fixtures/fixtures.test.ts` fails a step that names a control by the English text of a message another language's
  catalog translates, bare or filled, and a test of the name that takes a name followed by the English catalog's
  separator and refuses the language's own (a literal comma and space, where `zh-Hans` writes `，`). A custom test of the
  name that stands for copy must be built from the helpers, and may be given a description with `describing(...)` (what
  a failed step reports).
- **Daemon errors.** A scenario that answers a request with a daemon error on purpose uses the code the real daemon
  sends for that request in that situation, taken from its handler and `apps/daemon/PROTOCOL.md`, not one that merely
  sounds right: the UI words each code its own way, so `list_dir` refused with `permission_denied` instead of the
  `directory_unreadable` the daemon sends for an unreadable directory shows copy the app never shows.
- **Held to rules by tests.** `fixtures/fixtures.test.ts` (copy and catalog rules above, groups),
  `fixtures/projectGit.test.ts` (every fixture repository read is answered), `fixtures/configuration.test.ts` (written
  import results follow the daemon's rules; the fixture daemon refuses an import result that does not follow from what
  the steps asked for). The fixture daemon's own answers follow the rules in
  [`../fixtures/README.md`](../fixtures/README.md).

## Sweeping the gallery

`pnpm gallery:sweep [ids or patterns]` (`packages/ui/scripts/gallery-sweep.mjs`) loads the selected scenarios (all by
default) in a headless browser and fails on any gallery failure; `pnpm gallery:sweep --help` lists the flags (`--group`,
`--lang`, `--theme`, `--width`, `--jobs`, `--list`, `--url`, `--playwright`, `--chromium`). It loads the scenarios it is
given, every time; when to run it is set by `CLAUDE.md`'s "This project", and on which scenarios, widths, languages and
themes by the "Project conventions" section of `.claude/skills/agent-gallery/SKILL.md`. `--width 420,800` loads each
scenario that carries no width of its own at those widths, as the page's width control would, to prove that its steps
hold in a narrow window; a scenario with a `minWidth` is loaded no narrower than it. `--url <origin>` sweeps a gallery
already being served instead of starting a dev server of this checkout. A scenario's live changes are applied after its
steps.

Playwright is a devDependency of `packages/ui`, so `pnpm install` is everything the script needs, but it installs no
browser: run `pnpm exec playwright install chromium` from `packages/ui` once, or point `--chromium <path>` /
`OCTOBOARD_CHROMIUM` (or `--playwright <dir>` / `OCTOBOARD_PLAYWRIGHT`) at an existing one.

A sweep loads English unless `--lang` names more; `--lang all` sweeps every language registered in the served app's catalog
registry. A translation is held where it is written: each catalog is typed `Translation<...>`, so a missing message or
plural category fails type checking, `src/i18n/catalog.test.ts` checks the placeholders and plural categories of every catalog,
and `fixtures/fixtures.test.ts` fails a step that names a control in English copy or with the English separator, which
would find nothing in another language. The one check that runs only for English is the archived session's Reopen in
`history-loaded`, which needs the real pointer; the name Reopen carries in each language is held by
`TerminalPane.test.tsx`.

The page loads wait for a free heavy-step slot, the machine-wide lock `pnpm verify` also takes (`OCTOBOARD_VERIFY_SLOTS`
sets how many), and say who holds it and who is queued ahead while they wait, so sweeps in several worktrees queue
rather than overload the machine. The queue is first come, first served: a sweep run again after another run started
waiting lines up behind it.

## Comparing gallery screenshots

`pnpm gallery:compare` (`packages/ui/scripts/gallery-compare.mjs`) compares two already served galleries and writes a
static HTML difference report, JSON results and PNG images. It runs only when invoked explicitly; automatic checks
do not invoke it. Start each checkout's gallery yourself on a different free port, with dependencies installed and
Chromium available as described in "Sweeping the gallery". The command starts no servers, builds or worktrees.

From `packages/ui`, compare selected scenarios using the baseline and current checkout's actual gallery origins:

```sh
pnpm gallery:compare --base-url http://localhost:5192 --url http://localhost:5191 --output /tmp/gallery-comparison-1 'viewer-*'
```

From the repository root, the same package command is:

```sh
pnpm --filter @octoboard/ui gallery:compare --base-url http://localhost:5192 --url http://localhost:5191 --output /tmp/gallery-comparison-1 'viewer-*'
```

Choose a new output directory for each run; an existing directory, even an empty one, is refused. Keep reports outside
the repository or under its ignored `out/` directory. Open `index.html` directly in a browser: each case shows its
status, viewport and viewing conditions, baseline and current captures, and a difference image with changed pixels
marked red. Pixel counts, percentages and difference bounds describe the comparison. `results.json` records the
origins, selection, threshold, timing, completion state and per-case results; the PNG files sit beside both reports.

With no scenario arguments the command compares the union of both catalogs, reporting scenarios present only in the
current catalog as added and those present only in the baseline as removed. Prefer selecting the cases relevant to
the change: repeat ids or quoted `*` patterns, or use `--group <text>` to select a group substring on either side.
An explicit pattern matching no selected scenario fails rather than silently omitting it.

The defaults are English, the light theme and one capture at a time. The whole comparison waits for the machine-wide
heavy-step lock used by `pnpm verify`; it prints the current holders while queued. `--theme dark|both` and
`--lang en,zh-Hans` widen the viewing conditions; each named language must exist in both galleries. `--width <pixels>`
chooses one width. Both sides use the current scenario's own width when set, otherwise that flag or 1440, widened to
either side's `minWidth`; height is the current scenario's height or 900, with a minimum of 400. A removed scenario
uses its baseline metadata. This keeps the viewport equal on both sides.

Each capture uses a fresh browser context and waits for the scenario's initial steps, currently visible lazy content
and fonts before taking a screenshot, with screenshot animations disabled and the caret hidden. It does not replay
the Live changes buttons. Render errors and gallery failure overlays, including a failed visible lazy import or lazy
readiness timeout, are recorded as errors. A completed comparison records unchanged, changed, added, removed or error;
a case interrupted before completion remains pending. Reports are saved incrementally, and an interrupted or aborted
run shows Incomplete. Browser or comparison infrastructure failures stop the remaining cases and close Chromium; a
scenario render failure can be recorded while the remaining cases continue.

`--threshold <0..255>` sets the largest RGBA channel difference ignored, default 8; a pixel beyond it counts as changed.
`--timeout <ms>` bounds browser operations, default 60000. `--playwright <dir>` / `OCTOBOARD_PLAYWRIGHT` and
`--chromium <path>` / `OCTOBOARD_CHROMIUM` select existing browser installations. `pnpm gallery:compare --help` lists
all options. Exit status is 0 for no differences, 1 for changed/added/removed cases, 2 for rendering, setup or cleanup
errors, and 128 plus the signal number for interruption.

Compare both checkouts on the same machine and inspect the images: dynamic timestamps, terminal output, fixture
changes and rendering differences can change pixels too. A pixel difference identifies something to review; it does
not establish a regression or application correctness.

## External interfaces

- `SCENARIOS`, `GROUPS`, `DEFAULT_SCENARIO` (`fixtures/index.ts`) — the scenario registry the shell lists.
- `Scenario` and its staging types (`scenario.ts`) — what a scenario file writes.
- `fixtures/liveChanges.ts` — the builders for `liveChanges` buttons.
- `reportPageFixture.ts` — the `gallery:report-page-created` document event for report scenarios.
