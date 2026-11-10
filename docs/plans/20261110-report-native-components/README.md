# Report pages as native components

## Problem

A report page is a whole HTML document the session writes. HTML was chosen for flexibility, but most of that
flexibility is gone: to keep a page from carrying what the user typed out to the network under prompt injection, none of
its own scripts run and nothing external loads. What is left is freedom of layout and styling, and that freedom is
itself the source of the problems:

- A page cannot react to the user, so a form cannot lock its own submit button. Nothing in Octoboard's code locks it
  either: the frame posts every submission, the panel sends a `submit_page` for each, and nothing in the daemon refuses
  a page for having been submitted already. A user who presses the button twice sends the session two messages.
- Every page styles itself, so pages follow neither the UI standard, nor the dark appearance (a page is kept on a light
  surface), nor the accessibility bar.
- Keeping the page safe takes a sanitizer, a content security policy, a sandboxed frame and repeated isolation
  verification, and charts need a further plan of their own for bundling ECharts into the frame and stopping the HTML
  ECharts inserts.

Report pages serve two jobs: **asking the user questions with a form**, and **showing a friendly visual report**.
Neither needs arbitrary HTML.

## Outline

- A page is a **declarative component tree** in JSON, drawn from a fixed catalogue of components, and Octoboard renders
  it with its own native UI components.
- The session still pushes a page with one tool, `show_page`, which now takes the component tree instead of `html`.
- Free-form text goes in a Markdown component; there is no HTML anywhere in a page.
- Forms have a built-in submission lifecycle, so a page can no longer be submitted twice.
- Charts are one more component.

## Key design decisions

- **HTML pages are dropped entirely.** No rendering path for HTML is kept, not even read-only for pages already stored:
  those are discarded. Keeping one would keep the sanitizer, frame and isolation work alive for pages nobody can submit
  from anyway.
- **One `show_page` taking a component tree, not a separate tool per kind of display.** A page mixes components (a
  question beside a table, a chart above a summary), and separate tools could not compose them into one page.
- **The agent reads the catalogue only when it needs it.** `show_page`'s description stays short — what the tool is
  for, the rules an agent must know before pushing at all, and to read the guide before the first use — and a separate
  guide tool on the same MCP server returns the detailed usage and schema by topic (forms, report components, charts).
  An ordinary task carries only the two short tool descriptions. It is a tool rather than an agent skill because every
  agent a session can run shares Octoboard's MCP server, whereas skills differ per agent and would need installing into
  each user's configuration; and rather than an MCP resource because agents' support for reading resources is uneven.
- **The tree is validated against a schema when the tool is called**, and a tree that fails is refused with the error
  returned to the agent together with the schema of the part that failed, so it can correct the call. Nothing is
  silently dropped, unlike the HTML sanitizer.
- **Rendering is Octoboard's own code**, so a page follows the UI standard, the dark appearance, the accessibility bar
  and the UI's language for every string Octoboard supplies itself; the content's own text is the agent's.
- **Markdown is the escape hatch for free-form content**, not HTML: it covers most free layout and carries no script.
- **A page's content makes no network request.** Rendering in the window loses the frame's content security policy, so
  the rule is kept by the renderer itself: an image is shown only from a `data:` URL, as today, so that an injected
  page cannot carry data out in an image address. A link to an external address opens in the system browser when the
  user clicks it, which is the user's own act.
- **Interaction is safe because the code is ours.** What a scripted page used to be refused (locking a button, showing a
  state) is now done by Octoboard's components, with no page code involved.
- **One submission per page, never sent twice.** Submitting locks the page's form while the submission is in flight. The
  daemon marks the page submitted before delivering, so of two submissions racing from different windows only one gets
  through. A submission that certainly did not arrive clears the mark and unlocks the form for a retry; one that may
  have arrived (a message Codex may have taken without confirming) stays submitted, with no retry, and the user is told
  so. A submitted page is read-only, and its controls show the values that were submitted, in every window and after
  any reload, because those values are stored with the page.
- **Forms and data display cover what a native form and a data view can do**: the full set of form controls and their
  validation, parts of a page shown or hidden depending on what the user picks in its form, and tables the user can sort
  and filter. A choice button submits the form with one press, as a named submit button does today.
- **No language or writing direction per page.** A page follows the UI's language and direction.
- **Charts take a safe subset of an ECharts option**, rendered by the ECharts bundled in the app and loaded only when a
  page carries a chart. This supersedes the plan for charts declared in HTML report pages.

## Milestones

- [01 Component pages with forms](01-component-pages-with-forms.md) — replaces HTML pages; questions and Markdown
  reports work at the end of it
- [02 Visual report components](02-visual-report-components.md)
- [03 Chart component](03-chart-component.md)

## Open

- Which ECharts options the allowed subset contains, beyond excluding those that render a string as HTML.
- Whether a chart's colours come from the app's appearance, so it reads in light and dark, or from the option.
