# 03 Rail quota entry

> Goal: the left rail has a quota button directly above the Settings button; hovering it shows a floating panel with
> one tab per available agent listing that agent's accounts and their quotas; clicking pins it open.
> Completion criteria: hover shows and leaving hides the panel; click or keyboard activation pins it until Escape or a
> click outside, returning focus to the button; tabs switch agents; the panel offers a way to open Settings' accounts
> section; component tests and gallery scenarios cover these; verified in the UI preview.

## Technical design

- [ ] A rail button with an icon and tooltip, placed above the Settings button.
- [ ] A floating panel anchored to the button, holding agent tabs; each tab lists accounts with the account quota
      summary from milestone 2 and the plan name.

## Implementation plan

- [ ] Hover opens after a short delay and closes when the pointer leaves both the button and the panel; click or
      keyboard activation switches to pinned mode.
- [ ] Opening the panel sends the refresh request.
- [ ] Strings in every i18n catalog; gallery scenarios through the gallery skill.

## Notes for the developer

- **Reusable capabilities**: HeroUI `Popover` and `Tabs` as used by existing filters and dialogs; pointer focus return
  handling; the rail's existing button component.
- **Reference docs**: `docs/product/window-layout.md`, `docs/product/labels-and-tooltips.md`,
  `docs/ui-accessibility.md`, `docs/product/moving-focus-between-regions.md`.
