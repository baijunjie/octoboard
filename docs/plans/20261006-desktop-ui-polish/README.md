# Desktop UI Polish Development Plan

## Problem and approach

The macOS application now runs the HeroUI UI in `packages/ui`, which reached parity with the old UI before the
switch. Polishing the application was held back until then so that it is done once, on the new UI. The parity passes
in a browser and on a real build found the items below: none of them is a regression from the old UI, but each is
something the application should do better. This is the starting list; it is expected to grow.

## Key design decisions

- Polish is done on `packages/ui` only, and has to keep working both in the macOS shell and in a plain browser.
- An item that changes behaviour the product docs describe updates those docs along with it.

## Polish

> Goal: the application looks and behaves finished on the points below.
> Completion criteria: each item is resolved and checked on a real macOS build (and in a browser where the item applies
> there too).

### Technical design

- [ ] Theme: the old UI was dark throughout; the new one uses HeroUI's default light theme around a dark terminal.
  Settle on one look for the whole window.
- [ ] The checkbox in the new-session dialog ("Include in hub") has a white square on a white background, with too
  little contrast to be seen easily.
- [ ] Toasts sit over the report panel's ◀/▶ bar and the connection banner's area, hiding them while shown.
- [ ] When a session's process ends, its terminal is cleared to blank under "Not running" instead of keeping the last
  output on screen.
- [ ] The failure toast for submitting a report page to a stopped hub reads "this session is not running": it starts
  lowercase and does not say which console's hub it means.
- [ ] The system notification for a waiting session shows the default Tauri icon instead of Octoboard's.
- [ ] In a plain browser narrower than 1100px, a selected hub's terminal floor and report panel overflow the page
  sideways; only the macOS window enforces the 1100×600 minimum. Decide how a narrow browser window should lay out.

## Open

- Whether the window should follow the system's light/dark appearance, or keep one fixed look.

## Notes for the developer

- **Reusable capabilities**: HeroUI's theming; the session-location labels the toasts already use for notices.
- **Development notes**: follow the conventions for `packages/ui` components in development memory (keyboard focus
  staying in the terminal); check every item on a real build, since the WebKit view differs from Chrome.
- **Reference docs**: `docs/product/sessions.md`, `docs/product/report-panel.md`,
  `docs/memory/verifying-the-desktop-ui.md`, `docs/memory/writing-ui-components.md`.
