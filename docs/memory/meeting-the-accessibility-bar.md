# Meeting the accessibility bar

## Every control meets WCAG 2.2 AA, in both appearances

Every control and view in `packages/ui` is held to WCAG 2.2 level AA and checked in both the light and the dark
appearance; passing in one of them is not passing. The rules to check one against, and the measured facts behind them,
are `docs/ui-accessibility.md`. A few shortfalls are kept on purpose, each marked by a comment at its code saying so:
leave a marked one as it is, and do not add a new exception without the user's say.

Three of those rules settle how the code is written rather than what a later check finds, and the general practice each
one displaces leaves no cue to go looking:

- Never style focus with CSS `:focus-visible` — not the `focus-visible:` or `group-focus-visible:` variant, not a
  `:has(:focus-visible)` selector. A hand-built focusable element that removes the outline takes its ring from
  `useFocusVisibleProps` instead (the "Never style focus with CSS `:focus-visible`" section of
  `docs/ui-accessibility.md`).
- A state that changes without the user acting is announced through `StatusAnnouncer`, never from a `role="status"`
  element rendered together with its text (the "Announcing a change" section of `docs/ui-accessibility.md`).
- A change to a colour token or a surface is a change to every screen: it is audited over every gallery scenario in
  both appearances, and no colour a HeroUI component draws by default is assumed to pass (the "Auditing a change to a
  colour token or a surface" and "HeroUI's own text colours" sections of `docs/ui-accessibility.md`).
