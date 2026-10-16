# One horizontal inset for both panes

> Goal: the sidebar and the right pane share one horizontal inset, so their content lines up the same way from each
> pane's edge.
> Completion criteria: every header, row, filter bar, selector row, section heading and notice in the right pane
> (project browser and report) starts its content 16px from the pane edge, with row fills at 8px, matching the
> sidebar; the sidebar looks unchanged; both read the inset from shared tokens rather than literals. Verified in the
> UI state gallery for the sidebar, the project pane's three modes and the report pane, in both themes and in a
> right-to-left locale.

## Technical design

- [ ] A pane inset token (16px) and a row fill inset token (8px) beside the existing row tokens, used by both panes.
- [ ] The right pane's header, tabs strip, filter bar, worktree selector, From / To row, section headings and notices
      start at the pane inset.
- [ ] Tree and change rows, flat and nested, put their fill at the row fill inset and their first level's content at
      the pane inset; deeper levels indent from there as now.
- [ ] The report pane's bar uses the same inset.

## Implementation plan

- [ ] Add the tokens and move the sidebar's literals onto them with no visual change.
- [ ] Move the right pane's elements onto them, one view at a time, checking each in the gallery.
- [ ] Sweep the gallery scenarios this touches.

## Notes for the developer

- **Reusable capabilities**: the row tokens (`--row-control-size`, `--row-leading-icon-*`, `--row-control-gap`) and
  the shared row controls; the gallery's layout and project-pane groups.
- **Development notes**: use logical utilities (start / end), never left / right; keep the focus ring's room inside
  scroll containers; keep 24px targets.
- **Reference docs**: `docs/ui-accessibility.md`, `docs/memory/styling-the-ui.md`, `docs/product/window-layout.md`,
  `docs/product/project-pane.md`.
