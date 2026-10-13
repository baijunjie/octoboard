# Milestone 2: Activity marker

> Goal: The activity marker on a collapsed project row and inside focus mode's console-switch chip shows a
> tooltip saying what activity it stands for.
> Completion criteria: Resting the mouse on the marker in either place shows the tooltip after the default delay, and
> it tells apart "sessions waiting", "sessions working" and "sessions merely running", the three glyphs that look
> alike; the row's and the chip's accessible names are unchanged; the text exists in `en` and `zh-Hans`; type check,
> lint and the affected tests pass; the new tooltips are listed in the product doc and shown in the gallery.

## Technical design

- [ ] One tooltip text per activity (waiting, working, running), defined once in the message catalog in `en` and
  `zh-Hans`, used by both placements. Where a catalog string already names the activity for assistive
  technology, that is the string the tooltip says, as the overview's key design decisions settle.
- [ ] The marker on a collapsed project row gets the tooltip.
- [ ] The marker inside the console-switch chip gets the same tooltip.

## Implementation plan

- [ ] Decide each placement's shape — the indicator shape where the marker can be wrapped where it sits, the
  anchored shape where a collection item prevents it; the marker on a project row sits inside a pressable row.
- [ ] Add the new tooltips to the indicator list in the product doc.
- [ ] Show each activity's tooltip in the gallery and check it there.

## Notes for the developer

- **Reusable capabilities**: the indicator shape, `AnchoredTooltip`, the message catalog, the gallery.
- **Development notes**: the console avatar's corner disc in the rail shows the same activity but is deliberately not
  part of this milestone (see the overview); do not extend the work to it. The wording chosen here is what that disc
  will later reuse, so keep it in the catalog as one shared set of strings.
- **Reference docs**: `docs/product/labels-and-tooltips.md`, `docs/product/sidebar.md`, `docs/ui-accessibility.md`,
  `docs/memory/writing-ui-components.md`.
