# Milestone 6: The change row's status chip

> Goal: The single-letter status chip on a change row says what the letter stands for.
> Completion criteria: Resting the mouse on the letter shows a tooltip naming the status after the default delay,
> in the change list and in the change tree, for every status a change can have; nothing opens while the pointer is
> on the file name or on what follows it, which carry their own tooltips; the row's accessible name is unchanged and
> the chip stays out of the accessibility tree; a focused row's arrow keys reach the same stops as before, with no
> new one; the tooltip goes when the list is scrolled; type check and the affected tests pass; the tooltip is listed
> in the product doc and shown in the gallery, in both layouts.

## Technical design

- [ ] The tooltip is anchored to the chip itself, and the chip is the only part of the row that opens it.
- [ ] The text is the existing catalog string that names each change status on its own — the capitalised word the
  file viewer's own header chip shows for an opened change. No new catalog key.

## Implementation plan

- [ ] Give the chip the anchored shape: it reports the hover and is itself what the tooltip is placed against.
- [ ] Leave the file name's and the detail's clipped-text tooltips to their own regions.
- [ ] Add the tooltip to the indicator list in the product doc, and correct what the Git mode's doc says about the
  letter being decoration only.
- [ ] Show it in the gallery for both the flat list and the tree, and check it there.

## Notes for the developer

- **Reusable capabilities**: the anchored tooltip shape; the change statuses' standalone catalog strings, already
  used by the file viewer's header; the gallery and its hover steps.
- **Development notes**: the indicator shape cannot be used here. A change row is a react-aria collection item
  whose left and right arrow keys walk every focusable descendant without filtering for tab stops, so that shape's
  trigger would become a new arrow-key stop ahead of the row's ⋮ button and would open on focus. The anchored shape
  exists for exactly this case. A tooltip on a mark of this size inside a pressable row is settled as acceptable:
  the binding badge on a session row and the creator badge in a branch list are smaller still and carry one, and a
  tooltip is not a control, so a control's minimum pointer target does not apply. No gallery scenario drives an
  anchored tooltip yet, so this is the first — expect to find whatever the gallery's hover step is missing for it.
- **Reference docs**: `docs/product/labels-and-tooltips.md`, `docs/product/project-pane-git-mode.md`,
  `docs/ui-accessibility.md`, `docs/memory/writing-ui-components.md`.
