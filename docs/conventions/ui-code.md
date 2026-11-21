# UI code conventions

How React and TypeScript code in `packages/ui` is written: the React Compiler, state and the store, reading daemon
state, requests, raising error alerts, focus and key handling in code, and performance in the app's WebKit. How
components are built and used (the wrapper layer, HeroUI and React Aria, dialogs and fields, styling) is
`docs/conventions/ui-components.md`; the visual, interaction and accessibility rules are
`docs/conventions/ui-standard.md`; localized text in code is `docs/conventions/i18n-in-code.md`; what differs per agent
is `docs/conventions/provider-neutral-core.md`. Directory placement, dependency direction and the table of shared entry
points are in `docs/structure/ui.md`.

Each rule is a bullet: the rule, then why. Where nothing names an enforcing check, review is the check.

## Layers and shared entry points

- **Use the shared entry point for a need, never a second implementation.** `docs/structure/ui.md`, "Shared entry
  points", lists which helper does what (polling, browse budgets, the clipboard, external links, window focus, account
  and session predicates, settings writes and the rest); a second implementation drifts from the first in the cases the
  first learned the hard way. A rule that says more than "use helper X" is written in these docs.
- **Layering and the confinement of libraries are enforced by lint** (`packages/ui/eslint.layers.mjs`, spread into
  `eslint.config.mjs`; `pnpm lint`). Do not widen a block to get an import through; move the code to the layer that may
  import it (`docs/structure/ui.md`, "Dependency direction" and "Where new code goes").
- **Read the store only through the `src/store/index.ts` barrel** (lint). Below `src/` it is the only store import
  allowed.
- **Reach native capabilities only through `PlatformAdapter` (`src/platform/`); `@tauri-apps/*` is imported only by the
  platform layer.** The adapter is the one place that knows whether the page is in the desktop shell or a browser. Lint
  confines `platform/` to importing `protocol`; nothing checks the `@tauri-apps` confinement itself.
- **Behaviour or words that depend on the operating system read the host, never `navigator`.** The client's host comes
  from `usePlatform()` / `useHost()` (`PlatformAdapter.host`), the daemon machine's from `useHostPaths()`
  (`snapshot.host`); a pure function takes the host as an argument. Two machines are involved and the user agent
  describes neither reliably. `platform/host.ts` builds the client's host; `i18n/language.ts` reads
  `navigator.languages` for the browser's language, which is not an OS question.
- **A setting the desktop shell must know before the page loads is a `createShellPreference`, not a `localStorage`
  preference.** The shell cannot read the page's storage before the page exists; the shell side is
  `docs/conventions/desktop-shell.md`.
- **Absolute daemon-host paths are read and shown only through `format/hostPath`** (style from `useHostPaths()`);
  project-relative wire paths stay `/`-separated and never go through it. The daemon may be on another OS than the
  window.
- **The page reloads itself only through `reloadPage`**, and code that must not run on an unloading page asks
  `isReloading`; a reload started elsewhere runs effects and requests on a page that is going away.
- **The page never sends its address as a referrer** (`<meta name="referrer" content="no-referrer">` in
  `packages/ui/index.html`), so the daemon key in the URL stays between the page and the daemon.
- **A report page's `pattern` is tested only with `checkPattern`** (a Web Worker with a time limit), never on the main
  thread: a page's pattern is untrusted and can hang a regular-expression engine.
- **Third-party objects with private members are touched from one module.** xterm's private members only from
  `terminal/xtermInternals.ts` and `terminal/links/terminalLinks.ts`; `@pierre/diffs` only inside `viewer/` (and the
  markup it renders only through `viewer/code/rendererDom.ts`). A library upgrade then changes one file.
- **Agent-specific UI differences follow the provider-neutral rule** (`docs/conventions/provider-neutral-core.md`):
  names, install pages, configuration directories and detection markers come from `providerLabel` and the other readers
  of `useProviders()` in `src/agents/providers.ts`, marks and colours from the provider's presentation module, never
  from a branch on the agent in generic code.

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
- **Write no `??=`, no `||` whose left side holds a ternary or logical expression inside a call, and no destructuring
  with a computed key.** Name the left side in a local; copy the object and `delete` the key. The compiler cannot lower
  them. `??=` is refused by lint (`no-restricted-syntax` in `eslint.config.mjs`) in every file of `packages/ui/src`
  but tests and test support; write `x = x ?? y`, an `if`, or a getter (for a module-level value made on first use).
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
- **Count a component's executions to prove it stayed idle**, as `App.renderCounts.test.tsx` does; unchanged DOM does
  not prove it.
- **Lazy components use `retryableLazy`, not `React.lazy`**, and Suspense content observed before a capture goes through
  `observeLazyReadiness` / `trackSuspenseFallback` (`components/hooks/`); a failed chunk then retries instead of staying
  failed.

## Reading daemon state

A daemon output enum is read through its normaliser, never raw: `docs/conventions/protocol-and-contracts.md`, "Versions
and compatibility".

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
- **Do not add a spinner-glyph pattern to the UI for terminal titles.** The daemon strips the leading glyph before it
  stores the title; if one slips through, fix the daemon's list (`apps/daemon/src/terminal/title.rs`).

## Requests and polling

The helpers (`usePoll`, `usePolled`, `useLatestRequest`, `useSerialPush`, `Daemon.onImportProgress` and the rest) are in
`docs/structure/ui.md`, "Shared entry points"; use them rather than a timer or a request loop of your own. The rule that
goes beyond them:

- **Every browse request goes through a window-wide lane or slot of `browse/budget.ts`, and a new kind takes its share
  there; never a per-component concurrency limit.** The daemon refuses a connection's browse requests past 16
  outstanding with `limit_exceeded`, a sent request has no timeout, and unmounting cancels nothing, so per-component
  limits add up past the bound (`budget.test.ts` keeps the shares within it).

## Raising error alerts

The routes (`raiseError`, `alertFailure`), the title and `session` rules, `ActionableError`, recovery functions and the
window-lifetime outcome of a long-running action are `docs/conventions/error-codes.md`, "Raising a failure in the UI";
where an error is shown is `docs/conventions/ui-standard.md`, "Where an error is shown".

## Focus handling in code

What the user sees and hears (rings, names, announcements) is `docs/conventions/ui-standard.md`, "Visible focus" and
"Focus moved by a change the user did not make". These are the mechanics. jsdom drops focus differently from WebKit, so
a test that asserts only `document.activeElement` passes against a broken handoff; the test helpers are in
`docs/conventions/tests.md`.

- **A pressable control in the sidebar, toasts, banners and the terminal's surroundings takes `focusOnPress={false}`**
  unless its handler refocuses the terminal. A control that unmounts on press otherwise drops focus to `<body>` and the
  terminal stops receiving keys; a plain `<button>` shows no such defect, only the running page does. Build every "⋯"
  menu on `ActionMenu`, which also handles the focus return to the trigger.
- **Every code path that hides a region able to hold focus first hands focus to the terminal when the region contains
  `document.activeElement`**: a toggle, scrim, Escape, a breakpoint change, `inert` (`releaseFocus` in
  `layout/usePaneToggles.ts`, `HandoffOnUnmount` in `panels/PanelStack.tsx`). Run the check in the close path or a
  `useLayoutEffect`, never a passive effect: after a native listener's update React runs passive effects after paint and
  the browser has already moved focus.
- **A list whose order can change while a row may hold focus renders through `useListMotion`**; reordering moves DOM
  nodes and blurs the focused one.
- **Move focus to a named element yourself when its holder goes away; do not leave it to the library.** In a removal
  handler, call `handFocusOff` before removing a control (`RowMarkButton`, `AvatarField`); hand focus to the right
  element on every removal (`TagsInput` hands to its text input). Where the element leaves by a state change, wrap it in
  `useFocusHandoff` (`components/focus/`): the caller must stay mounted and render on the change, and selects the value
  itself. Not a hand-rolled "forget the element on blur": WebKit blurs a control as it is disabled, before the effect.
- **A handoff because the holder is going away goes through `HandoffOnUnmount` (it replays `focusin` after commit) or a
  layout effect of a component rendering in that commit (`useHandoffAfterRemoval`).** Never `focus()` from a
  layout-effect cleanup (react-aria never hears of it, so no ring) and never `queueMicrotask` (passive effects of a
  synchronous commit run before it).
- **Never move focus by script inside a mouse press's `pointerdown`; move it a frame later
  (`requestAnimationFrame`).** react-aria selects on `pointerdown` (a `Tabs` tab, among others), so a layout effect keyed
  on the selection runs inside that event, and the browser's `mousedown` default then focuses the pressed element and
  takes focus back. The scripted `focus()` used up react-aria's input flag, so that browser focus counts as having no
  input behind it: the modality turns virtual and the next focus draws a keyboard ring. No other path in the press's
  commit (a handoff, a pending focus request) may move focus either (`terminal/shell/ShellTerminalPanel.tsx`'s tab
  strip).
- **A button disabled for the duration of its own action is `pending` (`isPending`), not `disabled`.** The native
  `disabled` blurs the pressed control, and the dialog's Escape and Tab go with it. Keep `disabled` for a control
  unavailable for an outside reason, and where such a change can land under focus use `useFocusLeavingWhenDisabled`
  (buttons) or `useFocusHandoff` (fields).
- **Where a dialog replaces content that can hold focus, restore it with `useRefocusIfLost` or give `Dialog` a
  `resetKey`.** Never switch a dialog to its next subject by re-keying it (`key={item.id}`): the remounted modal records
  the outgoing element as its restore target. A field that sends its value on blur must be decided for dialog close: no
  blur fires, so the edit is discarded or sent on the way out.
- **Nothing focuses at dialog mount except the control the dialog opens on.** Give the opening focus through
  react-aria's `autoFocus` (`Button`, `TextInput`, a field's own prop) or, for a hand-focused control (a bare `Input`, a
  tab, a switch), a mount `useLayoutEffect`; never the DOM element's `autoFocus`, which a dialog over another loses to
  the outer dialog's containment. A "refocus if lost" hook compares its dependencies with the last ones, never a "skip
  the first run" flag (StrictMode runs it twice). A control in content that replaces other content in an open dialog
  takes focus through `useFocusWhenShown` keyed on the subject, never `autoFocus`.
- **A modal shown from state of its own (an error alert, a pending question) is rendered from the commit after the
  raising one and passes `topLayerAtMount`.** Copy the raised value into state from a `useLayoutEffect` (not a passive
  effect), as `shell/ErrorAlerts.tsx` and `dialogs/PendingQuestionDialog.tsx` do: two react-aria modals mounting in one
  commit each hide the other and focus falls to `<body>`. Never mark a modal a top layer for longer than its mount
  commit, or a later modal cannot cover it.
- **A modal that can close with nowhere to return focus calls `focusAfterModalCloses` / `cancelFocusAfterModalCloses`**
  (`components/focus/modalFocus.ts`) rather than a fallback of its own. Code that focuses into such a modal by hand at
  mount uses `useFocusWhenShown`, never a mount effect, `setTimeout` or a delayed take-back timer; a caller that focuses
  the modal itself passes `focusesItselfOnOpen` to `Dialog` / `DialogFrame`.
- **A dialog that another dialog opens as it mounts names its own close-focus destination** (`handFocusOnceClosed` in
  `settings/accounts/AgentAccountsSection.tsx`): the first candidate passing `canTakeFocus` via `.find`, never a `??`
  chain (a detached element is not null), retried every animation frame up to a cap, and focused once more a turn later
  if focus left it.
- **A rule of the shape "hide, close or hand focus once focus leaves" gates on the window having focus before it answers
  a `focusout` without `relatedTarget`**, and follows window focus only through `components/hooks/windowFocus.ts`
  (`windowHasFocus`, `subscribeWindowFocus`, `onWindowFocusReturn`, `onWindowFocusLoss`), not `visibilitychange` and not
  `focusin`/`focusout`. Otherwise it takes its region away while the user is in another application.
- **Where focus must go to a row that stays when rows leave a react-aria collection, move it in the handler before the
  state change with `focusRowBeforeFold`** (`project-browser/focusRow.ts`, with a `scrollIntoView` working from layout
  offsets). The library's focused-key reset re-aims focus at the nearest surviving row two renders later and undoes a
  handoff made after. A row's menu returns focus from `ActionMenu`'s `onClosed` (`focusRowControl`), not from the close.
  Delaying the move is only the last fallback, for a destination that is not a row.
- **A non-modal surface that stays usable while a dialog is open carries `topLayerProps` on an element already in the
  DOM before the dialog opens**, kept mounted when empty or portalled to `body`, and the backdrop is lowered rather than
  the surface raised. `ariaHideOutside` ignores later additions in hidden subtrees, and a raised surface lands over
  popover rows whose presses it then swallows.
- **A scroller that scrolls rows into view by script stops `FADE_SIZE` short of the edge** (imported, not written out);
  scroll padding governs only what the browser and react-aria scroll.
- **A hand-built control inside a rendering library's shadow root, and a surface that moves focus itself on a key,**
  follow the code-level rules in `docs/conventions/ui-standard.md`, "A control inside a rendering library's shadow root"
  and "A keyboard handler that moves focus itself".
- **Retry a focus move across frames with `tryEachFrame` / `afterFrames`** (`components/focus/`) and send focus into a
  panel opened on purpose with `landInPanel`; put a panel of the aside's owner on screen with `showPanel`
  (`useProjectPanels`). Do not write a timer.

## Key handling in code

- **A key handler goes on the plain DOM element a HeroUI or react-aria component renders, not on the component.**
  react-aria wraps a component's `onKeyDown` / `onKeyUp` and calls `stopPropagation()` after it returns, so an
  Enter-to-save on a `TextField` stops the Escape that closes the dialog around it, with no warning. Use the inner
  `InputGroup.Input`; where the handler must sit on the component, call `continuePropagation()` on every path.
- **Escape is taken above the React tree; never guard it locally.** While the sidebar drawer or a floating pane card is
  open, `layout/usePaneToggles.ts` holds a capture-phase `window` listener: a control inside a `data-escape-scope`
  region that needs Escape for itself is accounted for in that listener's test (`holdsText`). A dispatched Escape is
  never a neutral way to close a tooltip in a scenario or a test.
- **A surface that dismisses itself on Escape calls `useEscapeWhileTooltipOpen`** (`components/dialog/`; dialogs already
  do) and adds nothing of its own. While a tooltip with a trigger is open, react-aria stops every Escape at `document`
  capture, so a React `onKeyDown` never sees it.
- **Something inside a dialog that closes on Escape before the dialog (the file viewer's find bar) passes `Dialog`'s
  `keyboardDismissHeld` and closes from a native `keydown` listener on the dialog element with `stopPropagation()`**
  (`viewer/find/useFind.ts`). Not a `window` capture listener, which hides the key from react-aria's input tracking and
  the tooltip's own dismissal, and not yielding to `defaultPrevented`, which depends on listener order.
- **A filter over a collection wrapped in react-aria `Autocomplete` gives the collection and its items no single-key
  shortcuts and stops Backspace/Delete reaching react-aria's field handler** with `stopPropagation()` in a capture-phase
  `keydown` handler on an element around the field (never `preventDefault()` while editable). `Autocomplete`
  re-dispatches every key to the collection, and WebKit's re-dispatched Backspace navigates back.
  `primitives/MenuFilter.tsx` already does this for filtered menus, which therefore take no single-letter shortcuts
  either.
- **Every window shortcut is a command registered in `shortcuts/commands.ts`** (`REBINDABLE`, or `FIXED` for a key the
  user may not change), shown with `ShortcutKbd` and matched with `matchesCommand` inside `useWindowShortcut`. A key
  outside the registry is missing from Settings, cannot be rebound and is outside `validate.ts`'s conflict check, so a
  user binding can collide with it and swallow it.
- **A window shortcut whose default may also be a browser's or the system's key takes a press only when it changes
  something, in a browser client;** a press that would change nothing stops propagation but keeps its default
  (`KEEP_DEFAULT` from `useWindowShortcut`), so it reaches the browser and never the terminal; in the desktop app it is
  consumed. The repeats of a held key are taken (`sidebar/focusShortcut.tsx`).
- **Renaming or splitting a rebindable command keeps the old id readable:** add it to `RENAMED_IDS` in
  `shortcuts/store.ts`, mapped to the command that takes its binding, so stored preferences and imported configuration
  documents keep the user's key; a retired id with no successor is left to `sanitizeOverrides`, which drops it and
  counts it as skipped.
- **A matcher that accepts keys beyond its command's binding (a layout's typed `+` and `-`) yields when the pressed keys
  are another command's binding on this client**, and a press that types one command's character is never taken as
  another command by its key code (`terminal/terminalFontShortcut.ts`). `useWindowShortcut` stops propagation but not
  immediate propagation, so one press could otherwise run two commands.
- **The only key compared by hand is a `FIXED` key that a view outside `shortcuts/` owns** (Find in `viewer/`, F6 in
  `layout/`, which may not import `shortcuts/`). It is still listed in `FIXED`, and its test builds the key press from
  that entry (`viewer/FileViewerFind.test.tsx`) so the two cannot part.
- **Any Cmd/Ctrl test goes through `isPrimaryModifier` from `input/keys.ts`, and showing the key goes by
  `host.modifier`** (`ShortcutKbd`); never derive the platform again and never accept `metaKey || ctrlKey`. The helper
  also requires the other modifier up, so every shortcut and click agrees on which key it is. The image viewer's wheel
  zoom (`viewer/image/ZoomableImage.tsx`) is the exception: it takes either modifier.
- **Add a window `keydown` listener in an effect (`useWindowShortcut`), never at module evaluation, and keep it in the
  statically imported graph from `App.tsx`** (no lazy Settings load): `shortcuts/recording.ts` installs its capture
  listener at module load so that it runs first while a shortcut is recorded.
- **A command that is also a native menu item has its binding pushed to the shell as the menu accelerator**; the rule
  and the shell side are in `docs/conventions/desktop-shell.md`.
- **Detect a finger gesture from touch events, not pointer events**, and do not use `touch-action` to keep pointer
  events alive: Chromium sends `pointercancel` once a sideways touch becomes a pan. Swipes go through `useSwipe` and the
  same open/close paths as the toggles (`docs/conventions/ui-standard.md`, "A touch gesture is never the only way").

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
- **Every CSS rule written or generated for `packages/ui` keeps a tag, class or id in its rightmost compound** (details
  in `docs/conventions/ui-components.md`, "Styling mechanisms").
- **Animate the real property on each element that needs it (`width`); never ease a registered `@property` custom
  property.** WebKit jumps such a transition to its end whenever any stylesheet is rewritten, and xterm's renderer
  rewrites one on every fit.
- **A layout animation that changes the terminal's size is counted in the `resizing` prop `App` passes to
  `TerminalPane`**, which holds the fit until the ease ends; a fit per frame is a full restyle in WebKit.
- **A style that follows a layout measurement is taken in an effect with no dependency array** (a layout effect when the
  first paint needs it) **plus a `ResizeObserver`, writing state only when the value moved.** An observer alone misses a
  box that keeps its size while its content changes and an element React replaces in place; a `MutationObserver` is not
  the answer. The scroll fade and `FadeOverflow` are built this way.
- **Judge UI latency with `pnpm bench:wkwebview` in `packages/ui`, base against branch, on the same machine, and never
  point it at the user's own daemon** (omit `--port`): its scenarios rename and create sessions. Options and column
  meanings are in `packages/ui/scripts/wkbench/README.md`.

## Reviewing announcements and focus moves against recorded cases

- **Before writing or changing code that announces (`StatusAnnouncer` / `AttendedStatusAnnouncer` writes) or moves focus
  on its own (an automatic move, a handoff, a focus return), find a passing case in `docs/voiceover-cases.md` that the
  code can equal on every item of "What \"matches a case\" means", build to it and name it in the plan.** Code built
  first and compared afterwards usually matches nothing. A status case plus a focus case do not cover the pair's timing;
  if no case can be matched, tell the user before building. How verification is reused is in
  `docs/conventions/ui-standard.md`, "Reusing VoiceOver verification".
