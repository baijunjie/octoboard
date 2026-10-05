# Writing UI components

## A HeroUI control takes keyboard focus on a mouse press; keep it off the terminal's way

Clicking around the terminal must not move keyboard focus off it (see the "The console → project → session menu"
section of `docs/product/sessions.md`). In `packages/ui` every pressable HeroUI 3 control (`Button`,
`Dropdown.Trigger` and the rest, all built on react-aria's press handling) focuses itself when pressed with the
mouse. For a control that unmounts right after the press, such as a list item's × or a toast's Dismiss, focus then
falls to `<body>` and the terminal silently stops receiving keystrokes.

So give `preventFocusOnPress` to every pressable control in the sidebar, the toasts, the banners and the terminal's
surroundings, unless its handler puts focus back on the terminal itself. A menu also needs it on its trigger, and on
top of that react-aria moves focus back to the trigger when the menu closes. Build every "⋯"-style menu on
`packages/ui/src/components/ActionMenu.tsx`, which handles both, rather than on a bare `Dropdown`.

Nothing in the markup gives this away — a plain `<button>` never takes focus on click in WKWebView — so the defect
only appears in the running page.

## Inside a dialog, a focused control that unmounts takes Escape and Tab with it

When the focused element is removed, WebKit and Chrome fire no `focusout` and focus falls to `<body>`. A dialog's
Escape handling and Tab containment hang off focus being inside it, so both stop working. Whenever a dialog replaces
content that can hold focus — a listing swapped for the next one, a queue moving on to its next item — put focus back
with `useRefocusIfLost` from `packages/ui/src/dialogs/Dialog.tsx`, or pass the shared `Dialog` a `resetKey` naming the
current item, which does it for you.

Do not switch a dialog to its next subject by re-keying it (`key={item.id}`): the remounted modal records the
outgoing one's soon-detached element as its focus-restore target, so focus is lost again when it closes. Keep it
mounted and reset the per-item state from `resetKey` instead (`useDialogAction(resetKey)` does that for the error and
busy state).
