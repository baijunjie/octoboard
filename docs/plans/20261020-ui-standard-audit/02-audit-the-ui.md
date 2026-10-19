# 02 Audit the UI against the standard

> Goal: a complete list of every place in the UI that does not conform to `docs/ui-accessibility.md`.
> Completion criteria: every section of the standard has been checked against the whole UI; each non-conforming
> place is recorded in this topic with where it is, which rule it breaks, and the gallery scenario that shows it (or
> a note that none does); the fix milestones are written from the list and the user has agreed to their split.

## Implementation plan

- [ ] Check the code against each section of the standard, one section at a time, across the whole UI.
- [ ] Check the gallery scenarios visually against the rules that only show on screen (layout, glyphs, contrast,
      duplicate entries, badges, tab width), in both themes and in a language besides English.
- [ ] Record the findings in this topic, grouped so they can be split into fix milestones.
- [ ] Write the fix milestones from the findings and agree their split with the user.

## Notes for the developer

- **Reusable capabilities**: the UI state gallery and its sweep for walking every scenario; the shared Alert,
  InlineChip, status badge and agent tab strip components the rules name.
- **Development notes**: the audit only records; it changes no UI. A finding that is a defect outside the standard
  is filed as a bug ticket instead of being recorded here.
- **Reference docs**: `docs/ui-accessibility.md`; `packages/ui/README.md` "The UI state gallery".
