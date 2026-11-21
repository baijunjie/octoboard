# UI code conventions

How React and TypeScript code in `packages/ui` is written: the React Compiler, the store and daemon state, requests, and
performance in the app's WebKit. Components and styling are `docs/conventions/ui-components.md`; focus and key handling
are `docs/conventions/ui-accessibility.md`; localized text is `docs/conventions/i18n-in-code.md`. Directory placement,
dependency direction and the shared entry points are `docs/structure/ui.md`.

## General principles

- **A render reads only props, state, context and hook results**, since the React Compiler caches every value on the
  inputs it can see.
- **The store holds daemon records exactly as the daemon sent them**; what only this window knows is kept beside them.
- **Per-interaction state goes on the narrowest element that needs it**: the app's WebKit restyles everything under an
  element whose attributes or classes change.
- **Window-wide concerns (focus, keys, polling, browse budgets, window focus) go through their window-wide helper**,
  never a per-component timer, listener or limit that adds up past what the window can bear.

## Platform and the page

- **Reach native capabilities only through `PlatformAdapter` (`src/platform/`); `@tauri-apps/*` is imported only by the
  platform layer.** The adapter is the one place that knows whether the page is in the desktop shell or a browser.
- **Behaviour or words that depend on the operating system read the host, never `navigator`.** The client's host comes
  from `usePlatform()` / `useHost()` (`PlatformAdapter.host`), the daemon machine's from `useHostPaths()`
  (`snapshot.host`); a pure function takes the host as an argument. Two machines are involved and the user agent
  describes neither reliably. `platform/host.ts` builds the client's host; `i18n/language.ts` reads
  `navigator.languages` for the browser's language, which is not an OS question. Desktop versus browser is not a
  local-versus-remote test either (a loopback address may be an SSH tunnel): copy that depends on where things run uses
  the adapter's capabilities, never a hostname or user-agent heuristic.
- **A setting the desktop shell must know before the page loads is a `createShellPreference`, not a `localStorage`
  preference.** The shell cannot read the page's storage before the page exists; the shell side is
  `docs/conventions/desktop-shell.md`.
- **Absolute daemon-host paths are read and shown only through `format/hostPath`** (style from `useHostPaths()`);
  project-relative wire paths stay `/`-separated and never go through it. The daemon may be on another OS than the
  window.
- **The page reloads itself only through `reloadPage`**, and code that must not run on an unloading page asks
  `isReloading`; a reload started elsewhere runs effects and requests on a page that is going away.
- **Third-party objects with private members are touched from one module.** xterm's private members only from
  `terminal/xtermInternals.ts` and `terminal/links/terminalLinks.ts`; `@pierre/diffs` only inside `viewer/` (and the
  markup it renders only through `viewer/code/rendererDom.ts`). A library upgrade then changes one file.

## The React Compiler

Every component and hook in `packages/ui` is compiled by the React Compiler, which caches each value on the inputs it
can see. `eslint.config.mjs` applies the compiler's lint rules to `packages/ui`, with `react-hooks/exhaustive-deps` and
`react-hooks/set-state-in-effect` off by design (effects list the values they react to).

- **A render reads only props, state, context and hook results.** Anything else (`Date.now()`, `store.getState()`, a
  module-level `let` or `Map`, an object mutated in place) is frozen at its first value, which shows only once the app
  has sat still: a "2 minutes ago" that never moves. Pass `useNow()` as the `now` of anything that formats a time, read
  the store through a `useDaemonStore` selector, and put state that changes in place behind a store or
  `useSyncExternalStore` whose `getSnapshot` returns a new immutable object per change, rendering everything from that
  snapshot (`viewer/image/zoomGroup.ts` is the shape). A value only a menu's items need is read inside `ActionMenu`'s
  `items` function, which runs when the menu opens. Where module state must stay, mark the function `"use no memo"` with
  a comment saying what it reads.
- **Never write or read a ref during render.** Keep the latest value with `useLatest` or `useStableHandler`
  (`components/hooks/useLatest.ts`), or `useRenderLatest` where a child reads it in the same commit.
- **A `try` in a component or hook has a `catch`, no `finally`, and no conditional, logical, optional-chaining
  expression or `throw` inside it**; the compiler skips such a function silently. For "run this, report a rejection,
  then always do that" use `settle` (`components/hooks/settle.ts`); otherwise build the value before the `try` or move
  the helper to module level.
- **Write no `||` whose left side holds a ternary or logical expression inside a call, and no destructuring with a
  computed key** (`??=` is refused by lint). Name the left side in a local; copy the object and `delete` the key. The
  compiler cannot lower them.
- **Never add `eslint-disable` for `react-hooks/exhaustive-deps` or `react-hooks/rules-of-hooks`.** The compiler skips
  any function carrying one, which is why `exhaustive-deps` is off.
- **A `useMemo` or `useCallback` body reads only what its dependency list names.** Resolve values into locals above it;
  otherwise the whole component is skipped ("Existing memoization could not be preserved") and nothing is reported.
- **A `use*` function calls at least one React hook.** One that calls none is not taken for a hook, its results are new
  on every call, and moving code into it un-memoizes the caller. Code needing no hook is a plain function without the
  prefix; the exception is a builder of handlers that read refs, which is a hook wrapping each handler in
  `useStableHandler` (`terminal/shell/useShellTabFocus.ts`).
- **After changing a component or hook, run `pnpm compiler:bailouts <files>` from `packages/ui` and leave none of your
  functions in its list.** Nothing else runs it (`pnpm verify` does not). A function left uncompiled on purpose carries
  `"use no memo"` and a reason; the directive does not silence a ref accessed or a hook value mutated during render, so
  remove that cause instead.
- **Whatever every row of a list takes keeps its identity across daemon events.** One prop that changes on every render
  re-renders every row of the sidebar. Wrap a handler capturing a store map (or anything that changes per event) in
  `useStableHandler` before passing it to rows; build an object handed to every row from named members, never `...rest`;
  take a per-group session array from the session index (`indexSessions`, read through `store/useSessions.ts`) and
  stabilise any other filtered array with `useShallow`; put hooks before the values derived from their inputs.
- **Lazy components use `retryableLazy`**, and Suspense content observed before a capture goes through
  `observeLazyReadiness` / `trackSuspenseFallback` (`components/hooks/`); a failed chunk then retries instead of staying
  failed.

## Reading daemon state

The reducer (`store/reducer.ts`) builds a new `Map` for `sessions`, `projects`, `consoles`, `pages` and `gitStatuses` on
every event that touches one record, so a map read through `useDaemonStore` has a new identity after every event of its
kind.

- **An effect or memo that reacts to one change depends on resolved values (ids, statuses), never on a store map.** A
  map among the dependencies re-runs it on every unrelated event, and a one-off check ends up undoing state the user set
  by another route.
- **A derivation that must re-run on every event of a kind subscribes to the store itself** (`store.subscribe` with
  `getState()`, as `useStatusItemMenu` does) or selects an identity-stable array (`useShallow`); it does not read the
  map through render.
- **Read a session through the narrowest selector** (`useSession`, `useProjectSessions`, `useConsoleSessions`,
  `useHasBoundSessions`); a selector that builds an object is wrapped in `useShallow`, or `useDaemonStoreEqual` when the
  result holds objects of its own. `App` does not subscribe to the `sessions` map, so nothing may rely on `App` or a
  list's parent re-rendering when a session changes: select the value the code waits for, or watch the DOM
  (`useListMotion`'s `ResizeObserver`).
- **Daemon collections in the store hold each record exactly as the last event sent it.** State only this window has
  (the queued account switches) is a field of its own, never written into the records; a component that sorts or shows
  sessions selects it (`useQueuedAccountSwitches`) or it does not render when the queue changes. Because the store keeps
  the daemon's record as sent, read whether an account switch holds a session with `holdsAccountSwitch` (in a component,
  `useAccountSwitchHeld`), never `Session.switching_account` alone: that misses a switch this window queued.
- **Skip a state set that may change nothing**: a daemon polling answer, a reset in an effect on mount or dependency
  change, a per-commit measurement, a reconcile after an event. React does not reliably drop an equal set. Compare with
  the current state and skip the setter, or clear during render behind the same check. Polling hooks in `App` read the
  latest state through `useLatest`, compare an error's message and diagnostics explicitly (`sameValue` omits an Error's
  message and stack), and let a success clear a previous failure even when its list is unchanged.
- **Keep unchanged data by identity when a reducer rebuilds it, with `sameValue`** (`values/sameValue.ts`), so an event
  that changes nothing renders nothing.
- **Anything standing for "the daemon has none of these" gates on `hosts !== undefined`** (the `ready` flag `App` passes
  its hooks), never on an empty collection alone: the store starts empty on every launch. Code that acts on what the
  daemon holds now, above all code that writes back, gates on a snapshot of the current connection: remember
  `snapshotEpoch` when `connectionState` changes and wait for a larger one (`lifecycle/useInterfaceLanguage.ts`);
  `hosts`, `"open"` or `snapshotEpoch > 0` alone still pass on the previous connection's data. Fixture scenarios are
  built with `hosts` set, so a gallery cannot catch a missing gate; a unit test of the hook can.
- **Code after `await request(...)` is correct whichever of the reply and the change's broadcast arrives first.** Seed
  the store from the reply only when it lacks that id and never overwrite a held record (`seedShellTerminal`, the
  `session_opened` case of the reducer); hold work that must happen exactly once as a ref "debt" cleared when the work
  is done; do not assume the sender or the surface the change replaces is still mounted.
- **A control showing a daemon value shows the store's value, not a local one set on press.** Ignore a second press
  while the first is unanswered with a ref, not a shown value; a Settings switch gets this from `useSettingsWrite`'s
  version guard and holds no ref of its own.
- **A control whose every edit sends the whole value it shows (tags, a text saved as a whole) keeps a local last-asked
  value while a save is outstanding**, sends one save at a time, lets an edit made meanwhile wait and go at the reply on
  the version the store holds then (only while the stored value is explained by this run's saves, otherwise drops it,
  shows the stored value and reports a conflict), and on a conflict while the store is still behind its own save waits
  (`whenState`) and resends. A save sent while another is out conflicts with this window's own write; an edit over a
  stored value the run cannot explain overwrites a change the user never saw. Copy
  `settings/accounts/AgentTagsSetting.tsx` or `settings/CloneDirSetting.tsx`.
- **Order two events with a counter that grows on every read** (`gitMoment` in `project-browser/git/gitRequests.ts`),
  never by comparing `performance.now()` or `Date.now()`. The app's WebKit rounds `performance.now()` to whole
  milliseconds, so two readings are usually equal while jsdom's clock passes either way. Clocks are for durations and
  wall-clock time.

## Requests and polling

The helpers (`usePoll`, `usePolled`, `useLatestRequest`, `useSerialPush`, `Daemon.onImportProgress` and the rest) are in
`docs/structure/ui.md`, "Shared entry points"; use them rather than a timer or a request loop of your own. The rule that
goes beyond them:

- **Every browse request goes through a window-wide lane or slot of `browse/budget.ts`, and a new kind takes its share
  there; never a per-component concurrency limit.** The daemon refuses a connection's browse requests past 16
  outstanding with `limit_exceeded`, a sent request has no timeout, and unmounting cancels nothing, so per-component
  limits add up past the bound (`budget.test.ts` keeps the shares within it).

## Performance in the app's WebKit

The app runs in the system WKWebView, which recalculates style many times slower per element than Chrome. Playwright's
WebKit and Chrome under-report it.

- **Per-interaction state (an overlay opening, hover, drag, an update per daemon event) never changes an attribute,
  class, inline style or custom property on `<html>`, `<body>` or `#root`, nor which structural pseudo-classes a large
  subtree root matches.** Put state on the narrowest element that needs it. Tailwind's `space-y-*` and `divide-y` select
  on `:last-child`, so such a change restyles everything under it. Two cases may be set there: a window-wide cursor lock
  held for a gesture (`html.pane-resizing` and `--pane-resize-cursor` during a pane drag), set once when it starts and
  cleared when it ends, and layout values that do not change every frame (`useToastClearance`, `useBottomStrip`).
- **An HTML entry that renders the application (`index.html`, `website-preview.html`) keeps a permanent hidden sibling
  after `#root`**, so portalled overlays mounting and unmounting at the end of `<body>` do not flip `#root`'s
  `:last-child`. A new entry that renders the application needs the same sibling.
- **Do not interleave DOM changes and synchronous layout reads across components in one interaction.** `offsetHeight`,
  `getBoundingClientRect`, `getComputedStyle` and `getAnimations()` after a DOM change force the restyle, once per read.
- **Before easing an element's size, check the `ResizeObserver`s on it and inside it.** One whose write reaches past its
  own element waits for the running animations' `finished` (asked once per wait, a frame after the report;
  `animationend` is not sent for `element.animate()`).
- **Do not chase the per-overlay `inert` toggle** with `isNonModal` (a press outside then stops closing the overlay), by
  patching `node_modules`, or by removing `HTMLElement.prototype.inert`.
- **Animate the real property on each element that needs it (`width`); never ease a registered `@property` custom
  property.** WebKit jumps such a transition to its end whenever any stylesheet is rewritten, and xterm's renderer
  rewrites one on every fit.
- **A layout animation that changes the terminal's size is counted in the `resizing` prop `App` passes to
  `TerminalPane`**, which holds the fit until the ease ends; a fit per frame is a full restyle in WebKit.
- **A style that follows a layout measurement is taken in an effect with no dependency array** (a layout effect when the
  first paint needs it) **plus a `ResizeObserver`, writing state only when the value moved.** An observer alone misses a
  box that keeps its size while its content changes and an element React replaces in place; a `MutationObserver` is not
  the answer. The scroll fade and `FadeOverflow` are built this way.
