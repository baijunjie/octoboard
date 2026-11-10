# 02 Visual report components

> Goal: give friendly visual reports the components beyond Markdown that they need.
> Completion criteria: each component below is in the schema, validated by `show_page`, rendered natively in both
> appearances, and described in the guide tool; each has a gallery scenario, and daemon tests cover its validation.

## Technical design

- [ ] Layout: columns, and tabs.
- [ ] Content: a callout (with a tone such as info, success, warning, danger), a key–value list, a stat (a figure with
      its label and an optional change), and code and a diff.
- [ ] A table of columns and rows of plain values, which the user can sort by a column and filter.
- [ ] Each component's properties added to the page schema from 01, keeping the schema additive.
- [ ] The guide tool gains a report-components topic describing them.

## Notes for the developer

- **Reusable capabilities**: the app's existing code and diff views and its table, callout and badge styling, where the
  UI standard already defines them.
- **Development notes**: columns collapse to one at narrow panel widths, as the panel can be narrow or floated.
- **Reference docs**: `docs/ui-standard.md`.
