# Milestone 4: The confirmation callout's agent icon

> Goal: The agent icon of each session listed in a confirmation dialog's callout shows the same tooltip the
> same icon carries everywhere else: the account the session runs under.
> Completion criteria: Resting the mouse on the icon in each of the three confirmations that list sessions shows
> the tooltip after the default delay, for a session on the default account, on a named account and on an account
> that is no longer stored; the screen-reader-only name beside the icon is unchanged, and so are the dialog's
> accessible names; the two tooltips in one list row hand over to each other at once rather than waiting again;
> type check and the affected tests pass; the tooltip is listed in the product doc and shown in the gallery.

## Technical design

- [ ] The icon takes the account tooltip the agent icon already uses on a session row, a focus mode's card and
  archived row, and the archive view's row — no new wording and no new catalog key.
- [ ] The screen-reader-only name beside it stays as it is: it, not the tooltip, is how the agent and account reach
  assistive technology.

## Implementation plan

- [ ] Give the icon the tooltip in all three confirmations that list sessions; the fourth lists none.
- [ ] Add the placement to the agent icon's entry in the indicator list in the product doc, and to wherever that
  tooltip's meaning is already described.
- [ ] Show it in the gallery, extending the callout scenarios that already pin the status glyph's tooltip, and
  check it there.

## Notes for the developer

- **Reusable capabilities**: the account tooltip the agent icon is given elsewhere; the indicator tooltip shape; the
  gallery's step for hovering a mark in a callout list, which has a sibling for the status glyph.
- **Development notes**: the status glyph beside it already carries a tooltip, so the two sit a few pixels apart in
  one list row; the window opens a second tooltip at once when one closed less than half a second ago, which is what
  makes that pair bearable, so nothing about the delay needs changing. Keep the tooltip identical to the other four
  placements rather than matching the screen-reader-only text: the agent icon's tooltip already has one settled
  meaning in the product, and a user who learned it on a row should meet the same thing here.
- **Reference docs**: `docs/product/labels-and-tooltips.md`, `docs/product/sidebar.md`, `docs/ui-accessibility.md`.
