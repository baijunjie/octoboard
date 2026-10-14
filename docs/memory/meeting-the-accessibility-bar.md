# Meeting the accessibility bar

## Custom styles meet WCAG 2.2 AA, in both appearances

Before treating a visual measurement as a bug in `packages/ui`, identify whether the style is HeroUI's default or
project-authored. Apply the priority in the "Conformance level and exceptions" section of `docs/ui-accessibility.md`:
custom styles are checked against WCAG 2.2 AA in both light and dark appearances; passing in one is not passing.
HeroUI's default appearance is not an AA exception to repair. A few custom shortfalls are kept on purpose, each
marked by a comment at its code saying so: leave a marked one as it is, and do not add a new exception without the
user's say.

Three of those rules settle how the code is written rather than what a later check finds, and the general practice each
one displaces leaves no cue to go looking:

- Never style focus with CSS `:focus-visible` — not the `focus-visible:` or `group-focus-visible:` variant, not a
  `:has(:focus-visible)` selector. A hand-built focusable element that removes the outline takes its ring from
  `useFocusVisibleProps` instead (the "Never style focus with CSS `:focus-visible`" section of
  `docs/ui-accessibility.md`).
- A state that changes without the user acting is announced through `StatusAnnouncer`, never from a `role="status"`
  element rendered together with its text (the "Announcing a change" section of `docs/ui-accessibility.md`).
- A change to a custom colour token or a custom surface is audited over every gallery scenario it reaches in both
  appearances, including the controls placed on it (the "Auditing a change to a colour token or a surface" section of
  `docs/ui-accessibility.md`). Keep HeroUI's default appearance when addressing a shortfall on that custom surface.
