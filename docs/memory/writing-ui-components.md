# Writing UI components

## Focus dropped to `<body>` cuts the terminal off: a HeroUI control's press, a hidden pane

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

Hiding a region with CSS (`hidden`, a `docked:hidden` variant) while focus is inside it drops focus to `<body>` the
same way, with no event, and `preventFocusOnPress` does not help there: focus reaches a pane by Tab or by a click into
an iframe. So every code path that hides a region able to hold focus — a toggle, a scrim, Escape, a breakpoint change
— first hands focus to the terminal if the region contains `document.activeElement`. The docked panes already do this
through `releaseFocus` in `packages/ui/src/layout/usePaneToggles.ts`; hide a new pane through that hook rather than
beside it.

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

## A Tailwind class name has to stand in the source as literal text

Tailwind 4 reads the source as plain text and emits a utility only for a class name it can find spelled out there, so
a name assembled at runtime — `` `${side}-0` ``, a suffix appended to a prefix, anything concatenated — compiles to no
CSS at all and the element silently loses that property. Write each variant out in full and branch between them.

Nothing on the way past catches it: `packages/ui` has neither a linter nor a test suite, so `tsc --noEmit` is its only
automated gate, and a class whose utility was never emitted is valid TypeScript, builds clean and reads fine in a
diff. After adding or changing a utility class, grep the built `packages/ui/dist/assets/*.css` for it.

## An outline meant to be seen uses `--muted`, not HeroUI's own border tokens

In HeroUI 3's default theme `--border`, `--border-secondary` and `--border-tertiary` all measure under 3:1 against the
surface they sit on in both light and dark (1.2:1 to 2.7:1) — they are dividers between content on one surface, not an
edge a user is meant to find. `--muted`, the token HeroUI uses for secondary text, clears 3:1 in both (4.8:1 light,
6.7:1 dark). So anything whose line has to read as a boundary — a field, a checkbox, a pressable surface — takes
`--muted`, and the border tokens stay for separators. `style.css` already routes HeroUI's own field and checkbox
borders through `--muted`, so a HeroUI form control needs nothing on top.
