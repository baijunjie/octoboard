# Milestone 5: The activity badge on a rail avatar

> Goal: A console avatar in the rail says what its activity badge means, through the avatar's own tooltip rather
> than through a tooltip on the badge.
> Completion criteria: The avatar's tooltip says what is going on in the console as well as naming it, in each
> activity (a session waiting for the user, one working, one merely running) and when nothing is going on; it still
> opens on hover while the docked sidebar is shown and on keyboard focus either way, and still never on hover while
> the hidden sidebar can float in; the avatar's accessible name is unchanged and is not announced twice; type check
> and the affected tests pass; the product doc no longer lists the avatar as a control whose tooltip differs from
> its name, and the gallery shows the tooltip for each activity.

## Technical design

- [ ] The avatar's tooltip text becomes the same string as its accessible name — the console's name with what is
  going on beneath it folded in, or the bare name when nothing is.
- [ ] The badge itself stays decorative and gains no tooltip of its own.

## Implementation plan

- [ ] Change the avatar's tooltip text; add no trigger and no second tooltip.
- [ ] In the product doc, drop the avatar from the list of controls whose tooltip is not the name they announce,
  and correct what the rail's and the sidebar's docs say the avatar's tooltip is.
- [ ] Show a tooltip per activity in the gallery and check it there.

## Notes for the developer

- **Reusable capabilities**: the avatar's existing tooltip, through the labelled-control wrapper that already limits
  it to keyboard focus while the sidebar can float in; the name-folded activity messages in the catalog.
- **Development notes**: the tooltip reuses the **name-folded** activity messages, the ones that read "{name}, a
  session is waiting for you", not the standalone ones the activity marker's own tooltip says. That is deliberate:
  here the tooltip belongs to a control that already names the console, so folding is right, and a reader who finds
  two sets of wording for one activity should not try to unify them. State the one thing this does not solve: a
  mouse user with the docked sidebar hidden still never reads the words, because hovering floats the sidebar in
  instead — and that sidebar shows the sessions themselves, each status glyph with its own tooltip. Watch that the
  tooltip equal to the accessible name is not also given to assistive technology as a description.
- **Reference docs**: `docs/product/labels-and-tooltips.md`, `docs/product/sidebar.md`, `docs/ui-accessibility.md`,
  `docs/memory/building-on-heroui-and-react-aria.md`, `docs/memory/ui-text-and-reading-direction.md`.
