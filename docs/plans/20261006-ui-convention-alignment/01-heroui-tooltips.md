# Tooltips through HeroUI's Tooltip

> Goal: every icon-only control shows a HeroUI tooltip whose text matches its accessible name.
> Completion criteria: in the running app, light and dark, hovering and keyboard-focusing each icon-only control shows
> its tooltip; a mouse press on a control outside a dialog still leaves keyboard focus in the terminal; tooltips inside
> dialogs render above the dialog; `pnpm --filter @octoboard/ui build` passes.

### Technical design

- [ ] The labelled-control wrapper renders HeroUI's `Tooltip` (trigger and content) instead of a native `title`, with
  the tooltip text taken from the same string as the control's accessible name.
- [ ] Tooltips on the icon-only controls that lack one: the report panel's previous-page and next-page buttons.
- [ ] Tooltip on the dismiss × of the sidebar's notification-permission prompt, which also becomes HeroUI's
  `CloseButton`.
- [ ] Tooltip on the "⋯" trigger of every action menu, given once in the shared action-menu component.
- [ ] Tooltip on the close button (X) of the shared dialog frame and of the settings dialog.
- [ ] Tooltip on a toast's close button, if HeroUI's toast close button does not already carry one.

## Notes for the developer

- **Reusable capabilities**: the labelled-control wrapper in `packages/ui`'s components (the top bar's button wrapper
  already goes through it); the shared action-menu component, so the menu trigger is fixed in one place.
- **Development notes**: a tooltip must not make its control take focus on a mouse press (keyboard focus stays in the
  terminal outside dialogs). Check in both the macOS app and a browser, since WKWebView differs from Chrome.
- **Reference docs**: `docs/memory/writing-ui-components.md`, `packages/ui/README.md`.
