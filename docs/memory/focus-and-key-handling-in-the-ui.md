# Focus and key handling in the UI

## Focus dropped to `<body>`: a HeroUI control's press, a hidden pane, a reordered list

Clicking around the terminal must not move keyboard focus off it (see the "Rows, names and keyboard focus" section of
`docs/product/sidebar.md`). In `packages/ui` every pressable HeroUI 3 control (`Button`,
`Dropdown.Trigger` and the rest, all built on react-aria's press handling) focuses itself when pressed with the
mouse. For a control that unmounts right after the press, such as a list item's × or a toast's Dismiss, focus then
falls to `<body>` and the terminal silently stops receiving keystrokes.

So give `preventFocusOnPress` to every pressable control in the sidebar, the toasts, the banners and the terminal's
surroundings, unless its handler puts focus back on the terminal itself. A menu also needs it on its trigger, and on
top of that react-aria moves focus back to the trigger when the menu closes. Build every "⋯"-style menu on
`packages/ui/src/components/ActionMenu.tsx`, which handles both, rather than on a bare `Dropdown`.

Nothing in the markup gives this away — a plain `<button>` never takes focus on click in WKWebView — so the defect
only appears in the running page.

Hiding a region with CSS (`hidden`, a `docked:hidden` variant) while focus is inside it drops focus to `<body>` the
same way, with no event, and `preventFocusOnPress` does not help there: focus reaches a pane by Tab or by a click into
an iframe. So every code path that hides a region able to hold focus — a toggle, a scrim, Escape, a breakpoint change
— first hands focus to the terminal if the region contains `document.activeElement`. The sidebar does this through
`releaseFocus` in `packages/ui/src/layout/usePaneToggles.ts`, and a right-pane card through the `HandOffOnUnmount`
that `packages/ui/src/panels/PanelStack.tsx` renders with it; hide a new pane or card the same way rather than beside
them. Making a region `inert` drops focus the same way. Run the "is focus inside it" check in the close path itself
or in a `useLayoutEffect`, never in a passive `useEffect`: when the state change comes from a native listener of a
continuous event (the phone-width swipe's `touchmove`), React runs passive effects after paint, so the browser has
already moved focus to `<body>`, and the check finds nothing inside the region to hand on.

Reordering does it too: React reorders keyed children by moving their DOM nodes, and moving the node that holds focus
blurs it to `<body>`, so a keyboard user on a row loses their place whenever a live status update re-sorts the list.
Render any list whose order can change while one of its rows may hold focus through `useFlip`
(`packages/ui/src/sidebar/useFlip.ts`), which puts focus back after the move, rather than beside it.

A wrapper that renders nothing once its last child is gone takes focus down with it, and a library's own rescue does
not reach that case: react-aria's `useTagGroup` focuses its list element when the last tag is removed, but a list
that unmounts in the same commit is gone before the effect runs, and one laid out with `display: contents` (as
`TagsInput`'s is, in `packages/ui/src/dialogs/TagsInput.tsx`, to flow its tags into the field's row) cannot be counted
on to take focus. So move focus to a named element yourself, in the removal handler, rather than leave it to the
library — `TagsInput` hands it to its text input on every removal, a dialog being where focus on `<body>` costs Escape
and Tab as well — and for any control a handler of its own takes away, call `handFocusOff`
(`packages/ui/src/components/handFocusOff.ts`) before the removal, as `PinButton` in
`packages/ui/src/sidebar/rows.tsx` and `AvatarField` in `packages/ui/src/dialogs/AvatarField.tsx` do. Where the
element goes away from a state change rather than from a handler of its own — a region closed by a toggle pressed
with `preventFocusOnPress`, a control removed or disabled by the answer it was waiting for — there is no removal
handler to put focus back from, so wrap it in `useFocusHandoff` (`packages/ui/src/components/useFocusHandoff.ts`),
which after each render notices that the element focus was on has left the document or turned disabled. Use it rather
than a check of your own: the browser blurs a control as it is disabled, before the render's effect runs, so a
hand-rolled "forget the element on blur" has already forgotten it by then. Its caller has to stay mounted and render
nothing for that check to run in, rather than unmount with the element.

## A modal raised over another dialog mounts a commit after it as a top layer for that commit, and focuses itself one render after mounting

Applies to `packages/ui`, to a modal shown from state of its own (a store, a daemon push) rather than by the press that
opens it — an error alert, a pending question — wherever that state can change in the same update that opens another
dialog, as when a press opens a dialog and reports a failure in the same task. Two react-aria modals mounting in one
commit each hide the other: each hides the rest of the page with `ariaHideOutside` (`inert` in a browser) from a
passive effect, those run once both are in the document, and a hider only hides what lies outside its own modal and
never lifts what another one hid, even an ancestor of its modal. Both stay inert while the dialog is open, the modal's
focus call does nothing, and focus falls to `<body>`. Such a modal needs both of the following; either alone leaves a
case where it mounts with another modal:

- Show it from the commit after the one that raised it: copy the raised value into state from a `useLayoutEffect` and
  render from that state, as `ErrorAlertHost` in `packages/ui/src/dialogs/ErrorAlerts.tsx` and
  `packages/ui/src/dialogs/PendingQuestionDialog.tsx` do. Not from a passive effect: that render lands in a later task,
  by which time the dialog's hider has noticed the insertion and hides the new modal; the layout effect's synchronous
  render stays in the same task, after the dialog's hider has run and before its observer reports.
- Pass `topLayerAtMount` to its `Dialog` / `ConfirmDialog` (`packages/ui/src/dialogs/useTopLayerAtMount.ts`). The
  delay moves it out of the press's commit but into the next one, where a dialog that the press's dialog opens in turn
  from a mount effect also mounts (Settings opening the account settings dialog; an alert's Add agent…), and where an
  alert and a question raised in the same task meet. The flag keeps the modal out of every hider of its mount commit,
  and of several flagged in one commit leaves the last rendered on top.

Never mark such a modal a top layer for longer than its mount commit (the lasting marking is for a surface that is not
a modal): a modal opened later must cover it. Marked for good, a covered question takes focus back from the dialog on top
(react-aria's containment never counts focus inside a top layer as escaping), a tooltip under it no longer closes, as
tooltips close on turning inert, and a covered alert stays exposed to screen readers.

A modal of this kind also hands focus on when its last one closes with nowhere to return focus to — a control gone
with the dialog that closed before it, or no focus ever given: call `focusAfterModalCloses` from
`packages/ui/src/components/regionFocus.ts` as the last one closes and `cancelFocusAfterModalCloses` as one comes up,
as `packages/ui/src/errorAlerts.ts` and `PendingQuestionDialog` do, rather than a fallback of its own.

Code that moves focus into such a modal by hand as it mounts (`dialogAround(ref)?.focus()` and the like), where the
modal can open over another dialog — an alert or a question raised while one is open — focuses it from a layout
effect of the render after the mount: a mount `useLayoutEffect` sets a state value, and a second
`useLayoutEffect` keyed on that state does the focusing, as `ErrorAlertHost` in `packages/ui/src/dialogs/ErrorAlerts.tsx`
does. The trouble is a dialog underneath with no focused element of its own yet — one opened by a press without a
pointer (assistive technology, a script), whose opening focus react-aria defers to a frame. Both obvious timings lose to
it:

- a layout or passive effect at mount focuses the modal before react-aria has turned its containment on (that happens
  in the render a mount effect of its own forces), so the dialog underneath reads the focus as escaping and moves it to
  the modal's first control, where Enter then presses it;
- a `setTimeout(..., 0)` races that deferred frame: the browser may render first, the dialog underneath takes focus
  and becomes react-aria's active scope, and its containment pulls focus back off the modal once the timer focuses it.

The state-driven second layout effect runs after the containment render, in the same task, before any frame. Do not
answer either failure with a timer that takes focus back after a delay: it pulls back a user who has already tabbed on.

The second-render focus is still too late for the modal's own react-aria dialog: `useDialog` checks from a passive
mount effect, which React flushes before that render, whether focus is inside it, and when it is not, it takes focus
off the dialog half a second later and gives it back only once no transition is running anywhere on the page — never,
while one that never reports its end is running (in Chrome, one still running inside a disclosure as it folds away).
So whenever the caller focuses the modal itself, pass `focusesItselfOnOpen` to `Dialog` / `DialogFrame`
(`packages/ui/src/dialogs/Dialog.tsx`), with or without a `resetKey`, as `ErrorAlertHost` and an `unprompted`
`ConfirmDialog` do; a caller with a `resetKey` then refocuses itself for each new subject. The flag keeps the frame's
own `useRefocusIfLost` from putting focus found on `<body>` back on the dialog when `resetKey` changes, and holds focus
on an empty element inside the modal from the mount until the caller's focus lands.

A mouse never shows it, and a jsdom test reaches it only with `framesAsTimers()` from
`packages/ui/src/dialogs/dialogTestSupport.ts` (virtual modality, and each animation frame a 0 ms timer that runs ahead
of any timer set after it), focus on an opener outside (a test starting from `<body>` passes either way), a `Dialog`
mounted first (`openable` in the same file), the modal raised in an `act` of its own, and `settleFrames()` rather than a
fixed wait. The half-second take-back needs on top of that a `transitionrun` that never ends, dispatched on an element
of the dialog underneath, and a real wait past 500 ms. Cover the replacing cases too — a queued modal shown once the
first is acknowledged, and one put in front of it — and the meeting ones: a dialog that opens another from a mount
effect (`OpensAnotherDialog` in `packages/ui/src/dialogs/nestedDialogTestSupport.tsx`), an alert and a question raised
in one task, and a dialog opened later, which must cover the modal (assert the modal hidden then); run them under
`StrictMode` as well, as the app does. Copy the "over a dialog opened without a pointer" cases in
`packages/ui/src/dialogs/ErrorAlerts.test.tsx`. A test without
`framesAsTimers()` passes against the timer version. jsdom has no `inert`, so react-aria hides with `aria-hidden` there,
which does not stop focus: a test that asserts only where focus is passes against two modals mounted in one commit. Also
raise the modal in the same `act` as opening the dialog underneath (`openable(dialog, true)`), and assert that it is not
inside `[aria-hidden="true"], [inert]` as well as holding focus, as that file's `alertHoldsFocus` does.

## A dialog's opening focus comes from react-aria's `autoFocus` or a mount layout effect, and nothing else focuses at mount

Applies to `packages/ui`, to every dialog (and modal popover) and to any code that runs as one mounts. After a press
without a pointer (assistive technology, a script's `click()`) on a control that takes no focus when pressed, a dialog
mounts with focus on `<body>`; react-aria puts its opening focus — a react-aria `autoFocus`, or the dialog itself — off
to a frame, and drops it if focus has moved by then. So:

- Focus nothing at mount but the control the dialog opens on. A "put focus back if it fell to `<body>`" check that runs
  at mount, even one that only focuses the dialog element, silently cancels the first field's opening focus. That is why
  `useRefocusIfLost` (`packages/ui/src/dialogs/Dialog.tsx`) acts only when its dependencies change; a hook of your own
  of that kind compares its dependencies with the last ones it saw, never a "skip the first run" flag, which
  StrictMode's second mount run (the app runs in `StrictMode`) gets past.
- Give the opening focus through react-aria's `autoFocus` (`Button`, `TextInput`, a react-aria field's own prop). A
  control that has to be focused by hand — a bare HeroUI `Input`, a tab, a switch — is focused from a mount
  `useLayoutEffect`, as `DirectoryPicker` does, never through the DOM element's own `autoFocus`: React applies that as it
  inserts the node, before the new dialog's focus scope exists, so a dialog opened over another one loses it to the
  outer dialog's containment, which pulls focus back to its trigger, and then focuses its own dialog element. Such a
  control is also marked as `docs/ui-accessibility.md`, "The error alert dialog", sets out.

The two failures show under opposite presses, so test both: the mount refocus only after a press without a pointer
(`framesAsTimers()` from `packages/ui/src/dialogs/dialogTestSupport.ts`, under `StrictMode` as well), the DOM
`autoFocus` only after a keyboard or mouse press, since under a press without a pointer the outer dialog's pull-back is
put off until it is already inert (the "opens the directory browser on its path field" cases in
`packages/ui/src/dialogs/ProjectDialog.test.tsx`).

## "Focus left" is three things: check `document.hasFocus()` before answering a `focusout` with no `relatedTarget`

Applies to `packages/ui`, to any rule of the shape "hide this, close this, hand focus on once focus leaves it". A
`focusout` (React's `onBlur`) carrying no `relatedTarget` has three causes, and only one of them is the user moving
on: focus fell to `<body>` in this document; focus went into another document — the report panel's iframe, which this
document then shows as its own active element; or the window itself lost focus, with the element still active and the
user about to come back to it. A rule that answers all three takes its region off the screen while the user is in
another application, and moves keyboard focus behind their back with it.

So gate such a rule on `document.hasFocus()`, which is false in the third case alone, and where the iframe has to be
told apart from `<body>` as well, read `document.activeElement` a task later, once focus has settled — the iframe
element is what it shows then.

jsdom (30) has no window focus to report: its `hasFocus()` answers whether anything in the document has been focused,
so it is `false` in a test that never focuses anything and `true` from the first focus on, whatever the test means the
window to be doing. A test of a rule gated on it therefore sets the answer itself through `windowAttention()` in
`packages/ui/src/components/windowTestSupport.ts`, which starts the window attended and returns a setter that moves
`hasFocus()` and dispatches the window's `focus` / `blur` together — not a hand-rolled spy — or a gated rule looks dead
in a test that focuses nothing. Only such a test of the window-left case catches the ungated rule; no gallery scenario
leaves the window for another application.

## Inside a dialog, a focused control that unmounts or turns disabled takes Escape and Tab with it

When the focused element is removed, WebKit and Chrome fire no `focusout` and focus falls to `<body>`. A dialog's
Escape handling and Tab containment hang off focus being inside it, so both stop working. Whenever a dialog replaces
content that can hold focus — a listing swapped for the next one, a queue moving on to its next item — put focus back
with `useRefocusIfLost` from `packages/ui/src/dialogs/Dialog.tsx`, or pass the shared `Dialog` a `resetKey` naming the
current item, which does it for you.

No `blur` fires on that removal either, and closing a dialog unmounts the control holding focus, so a field that sends
what was typed on blur sends nothing when its dialog closes from under it. Decide for such a field whether the edit is
meant to be discarded there, or has to be sent on the way out.

Do not switch a dialog to its next subject by re-keying it (`key={item.id}`): the remounted modal records the
outgoing one's soon-detached element as its focus-restore target, so focus is lost again when it closes. Keep it
mounted and reset the per-item state from `resetKey` instead (`useDialogAction(resetKey)` does that for the busy
state).

A control that stays mounted costs the same when it turns disabled: react-aria's `isDisabled` puts the native
`disabled` attribute on the element, which blurs the control the user has just pressed. So a button disabled for
the duration of its own action — the obvious way to keep a slow request from being sent twice — drops focus to
`<body>` and takes the dialog's Escape and Tab with it until the action ends. Mark it `isPending` instead: that blocks
press and hover, keeps the control focusable, and announces the pending state. Keep `isDisabled` for a control
that is unavailable for a reason outside the action itself (a field not filled in yet), where focus is not on it.
When such an outside change can still land while focus is on a button (a quota reading arriving under an open
dialog), hand focus on with `useFocusLeavingWhenDisabled` from `packages/ui/src/dialogs/Dialog.tsx`, as the shared
dialog footer and `ConfirmDialog` do, rather than testing `document.activeElement` against the control in an effect:
WebKit has already blurred it to `<body>` by then. jsdom never blurs a disabled element, so the suite passes that
test; a test reaches the WebKit case only by dispatching the `focusout` and reporting `<body>` as the active element
by hand.
For a field such a change locks (the answer to a lookup), hand focus on with `useFocusHandoff`, as for a control that
goes away; there too a test blurs the control itself as it disables it, or it passes against the ordering that breaks
in the browser.

## Escape is taken above the React tree: a pane's listener, and react-aria's while a tooltip is open

Two capture-phase `keydown` listeners run before React's own dispatch, so a React `onKeyDown` can neither be relied on
to receive Escape nor hold on to it — `onKeyDownCapture` on a wrapper does not help, as React dispatches from its root
container (`<body>` for anything portalled there), below both.

The first is this project's own: while the sidebar's drawer or either pane's floating card is open,
`packages/ui/src/layout/usePaneToggles.ts` holds a capture-phase listener on `window` that dismisses the overlay. So
a control inside a region marked `data-escape-scope` (the sidebar, the aside, the terminal pane, the top bar, the
rail, the scrim) that needs Escape for itself is accounted for in that listener's own test instead (`holdsText`,
which leaves the key to a text field that holds text), never guarded locally. A control in a popover or a dialog
portalled to `<body>` sits outside every scope and is unaffected, which is the only reason the sidebar's project
filter and the dialogs' fields never needed this. Check it where it shows: the listener is installed only while an
overlay is up, so the symptom appears only with the sidebar's drawer open below the layout breakpoint, or with a
pane floated in on hover — the hidden docked sidebar, or the right pane while it has no panel open. A gallery
scenario at a normal width, with the sidebar docked and nothing floating, never reaches it. For the same reason, a
dispatched Escape is never a neutral way to close a tooltip, whether in a scenario step or a test: while an overlay
is up, this listener takes the key first and dismisses the overlay too.

The second is react-aria's: while a tooltip **that has a trigger element** is open, `useTooltipTrigger` holds a
capture-phase listener on `document` that calls `stopPropagation()` on every Escape, whatever else was waiting for the
key (react-aria 3.52.1, unchanged in 3.53.1). The listener is installed for every open tooltip, but its whole body is
guarded on the trigger's ref, which only the trigger element's provider ever sets — so a tooltip hung on a control or
on an indicator swallows the key and one anchored to an element that is not its trigger does not. Every icon-only
control here carries a tooltip through `TitledControl`, and such a tooltip opens on keyboard focus as much as on
hover, so one Tab inside any dialog or view reaches that state — when Escape does nothing somewhere, what predicts it
is "a tooltip with a trigger is open", not which control holds focus. A surface that dismisses itself from a React
`onKeyDown` loses the key, and a `window` capture-phase listener is the only way out — which is what
`useEscapeWhileTooltipOpen` (`packages/ui/src/dialogs/`) is: it closes whatever surface it is handed, dialog or not,
and only while such a tooltip is open. Call it rather than writing your own, and add nothing of your own to a
surface that already calls it, every dialog included. Such a handler can only choose whether to act, never
route the key onwards: once react-aria has swallowed it nothing can hand it back to the control that wanted it, which
is why a combo box's suggestion list inside a dialog does not withdraw on Escape while a tooltip is open.

## A key handler goes on a plain DOM element, not on a HeroUI or react-aria component

react-aria routes a component's `onKeyDown` / `onKeyUp` through `useKeyboard`, which wraps the handler in
`createEventHandler` and calls `stopPropagation()` for you once it returns, unless the handler calls
`continuePropagation()` — stopping is deliberate there, not a bug. So a handler written for one key takes every
keydown away from everything above it in the React tree: an Enter-to-save handed to a HeroUI `TextField` is enough to
stop the Escape that closes the dialog around the field, and nothing warns — not the types, not a console message, not
the suite. Put the handler on the plain DOM element the component renders instead: a HeroUI field's inner
`InputGroup.Input` is a react-aria-components `Input`, which passes `onKeyDown` straight to the DOM, and `useKeyboard`
installs no handler at all when it is given none, so moving it off the component costs nothing. When a handler
genuinely has to sit on the component, call `continuePropagation()` on every path through it, the one that handled the
key included.

## A surface that stays usable while a dialog is open is marked a top layer before the dialog opens

While a modal is open React Aria sets `inert` on everything outside it (`ariaHideOutside`) and dismisses it on a press
outside it (`useInteractOutside`); both make an exception for an element carrying `data-react-aria-top-layer="true"` —
and for the ancestors containing it — which is how HeroUI's own toast region stays usable over a dialog. So that
marking is how a surface of this project's own that is not itself a modal is kept reachable there, but it counts only
on an element that was already in the DOM when the dialog opened: the `MutationObserver` that `ariaHideOutside` watches
later additions with
skips anything added inside a subtree it has already hidden, and never looks at the marking on it. Keep the marked
element mounted whatever the state therefore — empty, and a flex item of no height, when there is nothing to show — or
portal it to `document.body` as the toast region does, and never mark it only while its content is on screen.

The marking does not lift the surface over the modal's backdrop, which covers the whole viewport, so the two have to
be ordered as well or the press lands on the backdrop. Lower the backdrop rather than raise the surface: React Aria
puts its popover-level overlays (a menu, a select's list, a tooltip) at the same level as the backdrop and anchors
them to the viewport's own bottom edge, knowing nothing about the window's chrome, so a surface raised above that
level ends up over the lowest rows of a list where every press is swallowed — the marking makes React Aria ignore a
press inside the surface, so such a press does not even dismiss what it covers. Having the marking without the
ordering reads as a contradiction that sends a diagnosis the wrong way: the surface paints undimmed on top of the
backdrop and still takes no pointer event at all.

## Rows taken out of a react-aria collection, or a row's menu closing, and the library re-aims focus two renders later

Applies to `packages/ui`, to every list built on a react-aria collection (`GridList`, `Tree`, `ListBox`) whose rows can
go away while keyboard focus is inside the list: a section folded away, a filter narrowed, a daemon event dropping a
change. Focus does not simply fall to `<body>` there. `useListState`'s focused-key reset (react-stately 3.50.0)
notices in an effect that the focused key has gone and re-points it at the nearest surviving row — it walks items
alone, so a section header is never a candidate — and `useSelectableItem`'s own effect then pulls DOM focus onto that
row. A hand-off written into the handler that takes the rows away is therefore undone a moment later: focus ends up on
a row of some other section, and the list scrolls to it.

So a control that has to keep focus across such a change takes it back only once both of those renders have gone by —
from a `requestAnimationFrame` nested in a `requestAnimationFrame`, guarded on focus still being inside the list so
that a Tab or a press in between stands, the way `packages/ui/src/browser/focusRow.ts` does it. The library's pull
runs only while the collection itself holds focus, so a control outside the list, such as a filter field above it,
needs none of this.

The same pull follows a popup opened from a control inside a row — a row's `⋯` menu — closing, and it comes later
than the close looks: a HeroUI popover keeps its content mounted, with focus still on the menu item, through its exit
animation after `onOpenChange(false)`; only when the content unmounts does focus fall to `<body>`, and the collection
then puts it on the row. So a focus return timed from the close (a `setTimeout(0)` after `onOpenChange(false)`) runs
while focus is still in the popover and is overridden. Time it from the popover's content unmounting —
`ActionMenu`'s `onClosed` — and wait out the collection from there, with the same guards, as `focusRowControl` in
`focusRow.ts` does. jsdom runs no exit animation, so a component test passes against the close-timed version too;
settle such a focus return in a browser on the gallery page, driven from the keyboard.

## A window shortcut is a command in the shortcut registry, never a key compared by hand

Applies to `packages/ui`, to every key combination the window answers or shows as a shortcut. Register it in
`packages/ui/src/shortcuts/commands.ts` — `REBINDABLE`, or `FIXED` for a key the user may not change — match it with
`matchesCommand` from `packages/ui/src/shortcuts/store.ts` inside `useWindowShortcut`, and show its key with
`ShortcutKbd` (`packages/ui/src/shortcuts/ShortcutKbd.tsx`). A key compared by hand is missing from Settings' list,
cannot be rebound, and is outside the set that `packages/ui/src/shortcuts/validate.ts` checks a recorded or imported
binding against, so the user can bind another command onto it. Matching is exact — no modifier more or less — and the
Cmd-on-Apple / Ctrl-elsewhere split of a binding is made in `packages/ui/src/shortcuts/binding.ts` alone.

Any other Cmd / Ctrl test — a modifier+click, the held-modifier quick-create affordance, a widget's own key — goes
through `isPrimaryModifier` (and, for showing the key, `IS_MAC`) from `packages/ui/src/useWindowShortcut.ts`. Do not
derive the platform again (from `navigator.userAgent` or anything else) and do not accept `metaKey || ctrlKey`: the
helper also requires the *other* modifier to be up, so every shortcut and click in the window agrees on which key it is.

Registering does not reach two places the window's listeners never see:

- a report page is a document of its own, and relays to the window only the keys passed in `composePageDocument`'s
  options (`packages/ui/src/report/pageDocument.ts`, from `ReportPanel.tsx`); add a command there if it should work
  while a page holds focus;
- a native menu item's key is the menu's accelerator, so a command that is one has its binding pushed to the shell as
  the Settings item's is (`useNativeMenuLabels`, `set_menu_labels` in `apps/desktop/src-tauri/src/menu.rs`), and is
  refused ⇧ with a number or symbol key in `validate.ts` as `openSettings` is: AppKit matches a menu key against the
  character with Shift applied, so such a combination never fires.

Add a window `keydown` listener in an effect (`useWindowShortcut`), never as a module is evaluated:
`packages/ui/src/shortcuts/recording.ts` installs its capture listener at module load so that it runs first and keeps
every key from the rest of the window while a shortcut is being recorded, and a listener installed before it would
see those keys. For the same reason keep it in the statically imported graph from `App.tsx`: loading the Settings dialog
or its Keyboard shortcuts section lazily would install it after the window's own listeners.
