# Milestone 1: Session status glyph

> Goal: Wherever a session's status glyph is shown without a visible status label beside it, hovering it shows a
> tooltip saying the status.
> Completion criteria: On the sidebar's session rows and in the top bar's breadcrumb, resting the mouse on the glyph
> shows the tooltip after the default delay, for each status (working, waiting for you, idle, interrupted, archived);
> focus mode's card, which already has a visible status label, is unchanged; the row's and the breadcrumb's accessible
> names are unchanged; the text exists in `en` and `zh-Hans`; type check, lint and the affected tests pass; the new
> tooltips are listed in the product doc and shown in the gallery.

## Technical design

- [ ] The status glyph used on the sidebar's session rows gets a tooltip with one text per status.
- [ ] The breadcrumb's status icon (the selected session's status, a `role="img"` span with no handler) gets the same
  tooltip text as the sidebar's glyph for the same status.
- [ ] The text for each status is defined once in the message catalog and used by both places, in `en` and `zh-Hans`.
- [ ] Decide whether the text is the existing `aria-label` string or a new, shorter one (see "Open" in the overview),
  and keep the two placements identical.

## Implementation plan

- [ ] Decide each placement's shape: the indicator shape where the glyph can be wrapped where it sits, the anchored
  shape if the row is a collection item that prevents it.
- [ ] Leave the glyph's use on focus mode's card as it is.
- [ ] Add the new tooltips to the indicator list in the product doc.
- [ ] Show each status's tooltip in the gallery and check it there.

## Notes for the developer

- **Reusable capabilities**: the indicator tooltip shape; `AnchoredTooltip` for a mark inside a collection
  item; the message catalog; the gallery and its `gallery-writer` subagent.
- **Development notes**: a tooltip on a non-tab-stop mark shows on mouse hover only. The press on a sidebar row
  belongs to the row; the tooltip must not take the press or change the row's accessible name.
- **Reference docs**: `docs/product/labels-and-tooltips.md`, `docs/ui-accessibility.md`,
  `docs/memory/writing-ui-components.md`.
