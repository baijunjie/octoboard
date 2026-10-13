# Milestone 3: Confirmation dialog callout icons

> Goal: The status icon of each affected session in the confirmation dialog's callout lists shows a tooltip
> saying the status.
> Completion criteria: Resting the mouse on a status icon in a callout list shows the tooltip after the default delay,
> with the same text as that status carries on a sidebar row and in the top bar's breadcrumb; the dialog's accessible
> names are unchanged; type check, lint
> and the affected tests pass; the new tooltip is listed in the product doc and shown in the gallery.

## Technical design

- [ ] The status icon in a callout list reuses the `session.status.*` catalog strings the glyph is already named by,
  which is what the glyph's tooltip elsewhere says; no new status strings.
- [ ] The agent icon beside it is not decided here (see "Open" in the overview).

## Implementation plan

- [ ] Place the tooltip with the shape that fits the callout list.
- [ ] Add the tooltip to the indicator list in the product doc.
- [ ] Show it in the gallery and check it there.

## Notes for the developer

- **Reusable capabilities**: `StatusIcon`'s `withTooltip`, which already gives the glyph this tooltip elsewhere; the
  `session.status.*` strings; the gallery.
- **Development notes**: this is a transient dialog and the weakest case of the audit; keep the change small.
- **Reference docs**: `docs/product/labels-and-tooltips.md`, `docs/ui-accessibility.md`.
