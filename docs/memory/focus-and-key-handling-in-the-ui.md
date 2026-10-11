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
— first hands focus to the terminal if the region contains `document.activeElement`. The docked panes already do this
through `releaseFocus` in `packages/ui/src/layout/usePaneToggles.ts`; hide a new pane through that hook rather than
beside it.

Reordering does it too: React reorders keyed children by moving their DOM nodes, and moving the node that holds focus
blurs it to `<body>`, so a keyboard user on a row loses their place whenever a live status update re-sorts the list.
Render any list whose order can change while one of its rows may hold focus through `useFlip`
(`packages/ui/src/sidebar/useFlip.ts`), which puts focus back after the move, rather than beside it.

A wrapper that renders nothing once its last child is gone takes focus down with it, and a library's own rescue does
not reach that case: react-aria's `useTagGroup` focuses its list element when the last tag is removed, but a list
that unmounts in the same commit is gone before the effect runs, and one laid out with `display: contents` (as both
tag groups in `packages/ui` are, to flow their tags into a row of the parent's) cannot be counted on to take focus. So
move focus to a named element yourself, in the removal handler, rather than leave it to the library: `ProjectFilterTags`
in `packages/ui/src/sidebar/ProjectFilter.tsx` hands it to `returnFocusTo` through `handFocusOff`
(`packages/ui/src/components/handFocusOff.ts`) on the removal that empties the group, and `TagsInput` in
`packages/ui/src/dialogs/TagsInput.tsx` hands it to its text input on every removal, a dialog being where focus on
`<body>` costs Escape and Tab as well.

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
mounted and reset the per-item state from `resetKey` instead (`useDialogAction(resetKey)` does that for the error and
busy state).

A control that stays mounted costs the same when it turns disabled: react-aria's `isDisabled` puts the native
`disabled` attribute on the element, which blurs the control the user has just pressed. So a button disabled for
the duration of its own action — the obvious way to keep a slow request from being sent twice — drops focus to
`<body>` and takes the dialog's Escape and Tab with it until the action ends. Mark it `isPending` instead: that blocks
press and hover, keeps the control focusable, and announces the pending state. Keep `isDisabled` for a control
that is unavailable for a reason outside the action itself (a field not filled in yet), where focus is not on it.

## Escape is taken above the React tree: a pane's listener, and react-aria's while a tooltip is open

Two capture-phase `keydown` listeners run before React's own dispatch, so a React `onKeyDown` can neither be relied on
to receive Escape nor hold on to it — `onKeyDownCapture` on a wrapper does not help, as React dispatches from its root
container (`<body>` for anything portalled there), below both.

The first is this project's own: while a drawer or a floating pane is open, `packages/ui/src/layout/usePaneToggles.ts`
holds a capture-phase listener on `window` that dismisses the overlay. So a control inside a region marked
`data-escape-scope` (the sidebar, the aside, the terminal pane, the top bar, the rail, the scrim) that needs Escape
for itself is accounted for in that listener's own test instead (`holdsText`, which leaves the key to a text field
that holds text), never guarded locally. A control in a popover or a dialog portalled to `<body>` sits outside every
scope and is unaffected, which is the only reason the sidebar's project filter and the dialogs' fields never needed
this. Check it where it shows: the listener is installed only while an overlay is up, so the symptom appears only
below the layout breakpoint, with the pane as a drawer, or with a docked pane hidden and floated in on hover. A
gallery scenario at a normal width, both panes docked, never reaches it.

The second is react-aria's, and it is unconditional: while any tooltip is open, `useTooltipTrigger` holds a
capture-phase listener on `document` that calls `stopPropagation()` on every Escape, whatever else was waiting for the
key (react-aria 3.52.1, unchanged in 3.53.1). Every icon-only control here carries a tooltip through `TitledControl`,
and a tooltip opens on keyboard focus as much as on hover, so one Tab inside any dialog or view reaches that state —
when Escape does nothing somewhere, what predicts it is "a tooltip is open", not which control holds focus. A surface
that dismisses itself from a React `onKeyDown` loses the key, and a `window` capture-phase listener is the only way
out — which is what `useEscapeWhileTooltipOpen` (`packages/ui/src/dialogs/`) is: it closes whatever surface it is
handed, dialog or not, and only while a tooltip is open. Call it rather than writing your own, and add nothing of your
own to a surface that already calls it, every dialog included. Such a handler can only choose whether to act, never
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
marking is how a surface of this project's own is kept reachable there, but it counts only on an element that was
already in the DOM when the dialog opened: the `MutationObserver` that `ariaHideOutside` watches later additions with
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
