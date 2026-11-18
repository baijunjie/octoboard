# Styling the UI

The styling rules (HeroUI's default look and its sheets, literal Tailwind class names, no `tailwind-merge`, nested
`max-` bands, local token overrides, veils, transitions in a collapsed disclosure, line-height ratios, inset tokens,
grooves, card shadows and the borders that stay) are in `docs/conventions/ui-components.md`, "Styling mechanisms"; a
style that follows a measurement is in `docs/conventions/ui-code.md`, "Performance in the app's WebKit". What they leave
to judgement:

## Styling project content inside a HeroUI component is not changing its look

Project content inside a HeroUI component inherits that component's base rule (`.empty-state` sets text size and colour
for every child), and HeroUI's own rules correct only the parts it styles by slot. Giving such a child its own size or
colour corrects what it inherited on project-authored content; it is not a change to the component's look.

## Diagnosing a focus move that never lands, or text a pixel off

- When a deferred focus move never lands under virtual modality, look for a transition with no end: log `transitionrun`
  / `transitionend` / `transitioncancel` document-wide in a browser against the gallery. jsdom runs no transitions,
  ignores rules inside `@layer` and does not expand the `transition` shorthand in computed style, so a test asserts the
  computed `transition-property` with the section collapsed and expanded, as
  `packages/ui/src/components/form/AdvancedSection.test.tsx` does.
- When text in the app or Safari sits a pixel off from Chrome, compare the line box's height in the two engines first: a
  half-pixel text position on its own is not the cause, since both engines draw it on the same pixel rows.
