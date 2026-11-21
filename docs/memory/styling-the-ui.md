# Styling the UI

The styling rules are in `docs/conventions/ui-components.md`, "Styling mechanisms", and the visual rules in
`docs/conventions/ui-standard.md`. What they leave to judgement:

## Read HeroUI's own sheet, not its component file, before styling content inside a component

HeroUI's sheet (`@heroui/styles/dist/components/<name>.css`) holds base rules that reach every child (`.empty-state`
sets text size and colour); the component file does not show them. A newly used HeroUI component needs its sheet, and
its dependencies' sheets, imported in `src/style.css` (`style.test.ts` names what is missing), and moving
`@heroui/styles` also moves the `tw-animate-css` pin.

## Styling project content inside a HeroUI component is not changing its look

Project content inside a HeroUI component inherits that component's base rule, and HeroUI's own rules correct only the
parts it styles by slot. Giving such a child its own size or colour corrects what it inherited on project-authored
content; it is not a change to the component's look.

## Find the rule that draws a colour before changing it

A custom token reaches only the rules that read it. Inspect the rendered element's computed styles to find which rule
supplies a colour; HeroUI's compiled stylesheet (`packages/ui/node_modules/@heroui/styles/dist/heroui.min.css`) is the
reference for its defaults, including where an input inherits a placeholder colour from preflight rather than a
component rule (which does not make that placeholder an application-authored style).

## Check a surface with what is drawn on it, and audit a token change across the gallery

- Check an application-authored surface with the text and controls actually rendered on it, at rest, hovered and
  selected, compositing translucent fills over their actual backgrounds; prefer a surface that works with HeroUI's
  default controls over overriding them.
- A change to a colour token or surface covers every gallery scenario it affects, in both appearances, and also
  measures what a pass over text and glyphs misses: every fill (a chip, a tertiary button, a ghost button's hover)
  against the surface it sits on; an animated state at its faintest frame (an `animate-pulse` glyph sinks to about
  2:1, so animate a glyph without lowering its opacity, and let a box clip a moving part rather than fade it); a
  translucent fill (HeroUI's soft chip is 15% of its colour) composited over each surface it lands on, never measured
  against the bare surface or the fill's own colour.
- A ratio read off a capture of the packaged window carries the display's colour profile, so it says only which side
  of a threshold a pair lands on (the selected toggle's fill reads 3.26:1 light where the tokens give 3.67:1): take a
  recorded ratio from the tokens. A figure is recorded, in the comment at its code, only where it shows a threshold
  met, records an accepted shortfall or shows why a value was chosen; a decorative fill or edge is described in words,
  since its figure drifts with every token change.
- A glyph or tint on the macOS window material (top bar, rail) is measured against the material's colours given in
  the comment on the window's surfaces in `packages/ui/src/styles/surfaces.css`, not against `--chrome`, the browser's
  stand-in; the dark material is far lighter than dark `--chrome`.

## Diagnosing a focus move that never lands, or text a pixel off

- When a deferred focus move never lands under virtual modality, look for a transition with no end: log `transitionrun`
  / `transitionend` / `transitioncancel` document-wide in a browser against the gallery. jsdom runs no transitions,
  ignores rules inside `@layer` and does not expand the `transition` shorthand in computed style, so a test asserts the
  computed `transition-property` with the section collapsed and expanded, as
  `packages/ui/src/components/form/AdvancedSection.test.tsx` does.
- When text in the app or Safari sits a pixel off from Chrome, compare the line box's height in the two engines first: a
  half-pixel text position on its own is not the cause, since both engines draw it on the same pixel rows.
