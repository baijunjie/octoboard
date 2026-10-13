# Milestone 3: Confirmation dialog callout icons

> Goal: The status icon of each affected session in the confirmation dialog's callout lists shows a tooltip
> saying the status.
> Completion criteria: Resting the mouse on a status icon in a callout list shows the tooltip after the default delay,
> with the same text as the same status in milestone 1; the dialog's accessible names are unchanged; type check, lint
> and the affected tests pass; the new tooltip is listed in the product doc and shown in the gallery.

## Technical design

- [ ] The status icon in a callout list reuses the status texts from milestone 1; no new status strings.
- [ ] The agent icon beside it is not decided here (see "Open" in the overview).

## Implementation plan

- [ ] Place the tooltip with the shape that fits the callout list.
- [ ] Add the tooltip to the indicator list in the product doc.
- [ ] Show it in the gallery and check it there.

## Notes for the developer

- **Reusable capabilities**: the status strings from milestone 1; the indicator shape; the gallery.
- **Development notes**: this is a transient dialog and the weakest case of the audit; keep the change small.
  Depends on milestone 1 only for the shared wording.
- **Reference docs**: `docs/product/labels-and-tooltips.md`, `docs/ui-accessibility.md`.
