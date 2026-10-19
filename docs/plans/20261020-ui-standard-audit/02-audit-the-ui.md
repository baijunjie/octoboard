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
- **Names in copy** (user request): sweep the whole project for names, titles and control labels interpolated into
  UI copy as plain text instead of the shared inline chip, per the standard's chip rules after "Standalone errors use
  the shared Alert" (plain-text surfaces such as accessible names and system notifications keep text). Inventory
  every i18n message with an interpolated parameter together with every place it is rendered; also check copy built
  in code and daemon error messages shown in the UI — `daemon.unknown_console`, `daemon.unknown_project`,
  `daemon.unknown_session` and `daemon.unknown_shell_terminal` render their name parameter as plain text today.
- **Development notes**: the audit only records; it changes no UI. A finding that is a defect outside the standard
  is filed as a bug ticket instead of being recorded here.
- **Reference docs**: `docs/ui-accessibility.md`; `packages/ui/README.md` "The UI state gallery".
