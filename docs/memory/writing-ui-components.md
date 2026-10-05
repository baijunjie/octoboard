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

Porting a control from `apps/desktop/` does not carry this rule over. That UI's plain `<button>`s never take focus on
click in WKWebView, so its markup shows no sign of it, and the defect only appears in the running page.
