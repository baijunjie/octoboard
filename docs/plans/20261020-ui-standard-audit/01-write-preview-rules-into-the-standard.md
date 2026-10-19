# 01 Write the preview's rules into the UI standard

> Goal: every rule listed under "Key design decisions" in the README is a written rule in `docs/ui-accessibility.md`.
> Completion criteria: each rule appears in the section of the standard it belongs to, stated as a rule a reviewer
> can check a screen against, with no contradiction with the rest of the standard; the user has read and accepted
> the wording.

## Technical design

- [ ] A button that acts replaces copy that points the user to another place.
- [ ] One entry per action in a view.
- [ ] Tab strips use content width, start-aligned.
- [ ] An error alert's recovery action is a secondary footer button beside acknowledgement.
- [ ] A state that must not happen: the entry is disabled; arising under an open dialog, the dialog closes and a
      plain error alert explains.
- [ ] A normal absence uses a neutral or purpose glyph, never a warning glyph.
- [ ] A status mark on a name is a badge after the name.
- [ ] An inline Alert holds only what failed and why; the recovery follows it in ordinary text style.
- [ ] Alerts use HeroUI's Alert with its original styles and structure; rewrite the sentences describing the custom
      wrapper (danger-soft fill, heading-element title, block content inside the callout).
- [ ] Check the bare-coloured-text rule already in "Standalone errors use the shared Alert" reads consistently with
      the new rules.

## Notes for the developer

- **Development notes**: `docs/ui-accessibility.md` has no writer subagent; the main session maintains it. Place each
  rule in the existing section it belongs to (e.g. "Standalone errors use the shared Alert", "An empty state is a
  glyph, a heading, and what fills it") rather than in a new list.
- **Reference docs**: `docs/ui-accessibility.md`, `docs/README.md`.
