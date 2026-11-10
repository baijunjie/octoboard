# 03 Chart component

> Goal: a report page can show an interactive chart, declared as data and drawn by the ECharts bundled in the app.
> Completion criteria: a page with a chart component shows the chart with ECharts' own interactions (tooltip on hover,
> legend toggling) in both appearances; an option outside the allowed subset is refused by `show_page` with the
> reason; no string from the option is ever inserted as HTML. Verified by daemon tests for validation, a UI test that
> an option string reaches the screen only as text, and a gallery scenario.

## Technical design

- [ ] A chart component carrying an ECharts option as JSON, limited to an allowed subset.
- [ ] Options that make ECharts render a string as HTML (an HTML tooltip, formatter templates) are outside the subset,
      because the chart now renders in the window rather than an isolated frame.
- [ ] The guide tool gains a charts topic describing the allowed subset.

## Implementation plan

- [ ] ECharts is bundled into the app and loaded only when a page carries a chart.

## Notes for the developer

- **Development notes**: the window blocks network loads, so the library cannot be fetched at run time.
- **Reference docs**: `docs/ui-standard.md`.
