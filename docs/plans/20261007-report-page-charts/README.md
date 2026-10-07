# Charts in report pages, rendered by Octoboard

> Goal: a report page can show an interactive chart. The page declares the chart as data, and Octoboard's own code
> renders it, so the page itself still runs no script of its own.
> Completion criteria: a page pushed with `show_page` that carries a chart declaration shows the chart in the report
> panel, with ECharts' own interactions (tooltip on hover, legend toggling) working. The report panel's isolation
> guarantees still hold with charts on the page: no network egress from the page, and no script of the page's own runs.

Builds on report pages being script-free documents: the page's HTML is sanitized inside the frame, and Octoboard's
own scripts run under a per-render nonce.

## Problem

Report pages no longer run their own scripts. A page is model-authored HTML, and under prompt injection a scripted
page could carry what the user typed out to an address of its choosing through channels no content security policy
governs in WebKit (WebRTC, `<link rel=preconnect>`, `<link rel=dns-prefetch>`). Removing page scripts closed those
channels, but it also took away charts drawn by a library such as ECharts. Before that change such charts were
impractical anyway: the CSP refused external scripts, so the hub would have had to inline the whole library (about
1 MB) into every page.

The workaround available today is for the hub to render a chart to SVG in its own environment (for example ECharts'
server-side `renderToSVGString` under Node) and put the SVG into the page. That needs the agent to install
dependencies and run a script each time, may raise permission prompts, gives uneven results, and loses all
interaction.

## Outline

- The page declares a chart with an element carrying the chart's ECharts option as JSON, of the form
  `<div data-chart='{…ECharts option…}'>`.
- Octoboard bundles ECharts and renders each declared chart into that element with its own trusted code. No separate
  rendering server is involved: the app's own UI already runs JavaScript.
- The page itself keeps running no script of its own. The chart is rendered from data the page supplied, never from
  code it supplied.
- The hub learns the declaration from the `show_page` tool description, alongside the rest of the page contract.

## Key design decisions

- **Declarative charts rendered by Octoboard, not scripts in the page.** This is the only form that brings charts back
  without reopening the page's script channel, which was closed to stop exfiltration under prompt injection.
- **ECharts as the chart library.** It is the library the user asked about, and its option object is plain data that
  JSON can carry.
- **Rendered in Octoboard's code, not by the agent.** This is the least work for the agent and gives stable results.
  Server-side rendering by the agent stays possible but is not what this plan builds on.
- **No local rendering server.** A chart needs only a renderer in code Octoboard already runs, so no extra process is
  started.

## Milestone

- [ ] Define the chart declaration: the element and attribute that mark a chart, and that the attribute's value is an
      ECharts option object as JSON.
- [ ] Render each declared chart in the report page with the bundled ECharts, from Octoboard's own trusted code, after
      the page's HTML has been sanitized.
- [ ] Describe the chart declaration in the `show_page` tool description and the hub's instructions, so the hub writes
      charts this way instead of embedding scripts or rendering SVG itself.
- [ ] Re-check the report panel's isolation with a chart page: no UDP, TCP or DNS leaves the app, and a script or
      `<link>` hidden in the declaration is not run or inserted.

## Notes for the developer

- **Reusable capabilities**: the page's in-frame sanitizer and the nonce-authorised bridge script are where Octoboard's
  trusted code already runs inside a page. The window's content rule list blocks every `http(s)` load from the
  webview, so the chart library has to be bundled; it cannot be fetched.
- **Development notes**: everything that comes into the frame from a page is untrusted data. The page contract and its
  isolation are described in the report panel's product doc and must stay true. Keep the verification to the chart
  path; the isolation probes were done once already and only the chart path is new.
- **Reference docs**: `docs/product/report-panel.md`, `docs/memory/building-and-launching-the-app-for-verification.md`,
  `docs/memory/verifying-the-desktop-ui.md`.

## Open

- **Where the library runs, and its cost.** It could load into every page's frame as a nonced script, or only into
  pages that declare a chart, or render in the window and hand the frame the result. Interactive charts need the
  library live in the frame. ECharts is about 1 MB minified, so including it in every page has a cost.
- **HTML that ECharts itself inserts.** An option can carry strings that ECharts renders as HTML, for example an
  HTML-mode tooltip or its formatter templates. Either that HTML is sanitized the same way as the page, or the
  options that produce HTML are restricted (for example, tooltips rendered as rich text only). Otherwise a
  declaration could insert a `<link>` past the sanitizer.
- **Charts on history pages**: whether they stay interactive or are shown as a static snapshot.
- **A malformed declaration** (not valid JSON, or rejected by ECharts): what the element shows instead.
- **Chart types and options allowed**: all of ECharts, or a subset.
