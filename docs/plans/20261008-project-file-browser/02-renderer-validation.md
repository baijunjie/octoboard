# Validate the read-only renderers

> Goal: select and isolate the code, image and diff renderers before building the real project browser.
> Completion criteria: a small gallery exercises the same viewer in a browser and the packaged desktop WebView,
> with representative source files, images and patches. The selected library and version, supported image formats,
> resource-loading requirements and tested limits are recorded. Unsupported or oversized input has an explicit
> fallback. This milestone can be merged without a project tree or Git endpoint.

## Technical design

- [ ] The application owns the viewer interface. File identity, source metadata and content classification do not
  depend on a rendering library's component types or patch parser.
- [ ] A read-only modal accepts text, image or unsupported-binary content. A text renderer handles mainstream
  languages and falls back to plain text when a grammar is unavailable.
- [ ] A diff adapter accepts the old/new sources and their change metadata, including an absent side. Plain-file
  viewing and diff viewing share presentation where appropriate without making the daemon contract library-specific.
- [ ] A restricted-side preview is distinct from an absent-side diff. The viewer can show only the permitted side
  with a restriction label, without inventing a complete patch or implying that the other side is empty.
- [ ] The modal remains mounted when its subject changes between files or content types. Closing restores focus;
  changing the subject preserves focus containment and clears obsolete content and errors.
- [ ] Repository content is displayed as data. HTML, scripts and embedded SVG markup never execute in the application
  document. Supported images use an image-rendering surface, not repository markup inserted into the document.
- [ ] Rendering exposes no edit or save interaction. Image zoom/pan and editor capabilities are not prerequisites for
  the first usable viewer.

## Implementation plan

- [ ] Evaluate the candidates listed in the overview with a small representative fixture set before committing to
  one. Include ordinary files and diffs so selection does not optimize only the first screen.
- [ ] Verify bundled, offline loading in the packaged WebView: language/theme chunks, workers, WASM and image URLs
  must work under the actual application CSP without weakening report-page isolation.
- [ ] Exercise mixed code/image switching, unknown text languages, unsupported binary input, one-sided diffs and
  restricted-side previews and renderer failures. Record which image formats are guaranteed and the presentation
  of image changes.
- [ ] Exercise long lines, large text and large patches against explicit renderer budgets. Confirm that exceeding
  a budget produces a fallback rather than freezing the viewer; virtualization alone is not a parsing limit.
- [ ] Verify text selection and copying, keyboard access, closing and focus restoration, light/dark appearance,
  narrow-window sizing and RTL controls. Keep code and paths in their own reading direction.
- [ ] Record the library decision, dependency version, supported formats, fallback behavior and browser/WebView
  verification results with the module documentation so the real browser can adopt the tested adapter.

## Notes for the developer

**Reusable capabilities**

- The UI dialogs module supplies the shared HeroUI modal frame, focus containment, closing and reset-key refocus.
- The UI gallery supplies isolated verification scenarios; the theme and internationalization modules supply
  appearance and localized controls.

**Development notes**

- The shared dialog currently exposes form-oriented small/medium/large sizes. Evaluate a file-viewer size through
  that wrapper instead of assuming the existing sizes fit code or split diffs.
- The current image CSP admits `data:` only. A successful dev-server run does not establish compatibility with the
  packaged build. Inspect the application policy and the report frame's separate policy before changing either.
- Do not select a library based on advertised virtualization alone: content replacement, copying, focus and
  resource-loading behavior need verification in the actual WebView.
- Renderer budgets and daemon read budgets are separate; this milestone does not replace the daemon's browse
  bounds on reading, transport or concurrency.

**Reference docs**

- `packages/ui/README.md`
- `docs/product/window-layout.md`, `docs/product/report-panel.md`
