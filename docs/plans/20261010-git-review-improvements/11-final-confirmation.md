# Final confirmation

> Goal: the checks that only a person can make are made, together with the ones held back for one packaged build
> rather than one per milestone, and anything they turn up is fixed here.
> Completion criteria: every item below is confirmed in the packaged app, or fixed and then confirmed; nothing is
> left beyond this milestone.

## Carried from milestone 02 (expanding collapsed lines in the viewer)

Everything else about the expansion was verified in the packaged app. These two cannot be driven by an agent at
all — they need a person at the machine:

- [ ] A screen reader (VoiceOver) on the separators: their names, the loading and failure announcements, and the
  controls marked unavailable while a read is out.
- [ ] A real input method typing into the viewer while a diff is shown, confirming composition does not reach the
  separator controls' key handling.

## Carried from milestone 05 (filtering the change list by file name)

Implemented and covered by unit tests over synthetic composition events; a real input method cannot be driven by an
agent, as injected keystrokes bypass macOS input methods entirely.

- [ ] A real input method typing into the change list's filter field, confirming composition does not reach the
  list's type-ahead and navigation keys, and that the key ending a composition does not clear the field.
- [ ] A real input method composing in the terminal while a drawer or floating pane is open: the key that ends the
  composition cancels it rather than closing the pane, and an Escape outside a composition still closes the pane.
  The layout's Escape listener now skips an editable target that holds a value, and xterm.js's hidden
  `textarea.xterm-helper-textarea` is excluded from that skip, so the terminal keeps the pane's escape hatch and the
  composition is guarded by the input-method check instead.

## Carried from milestone 06 (a row action button with Copy path)

Implemented and verified in the component gallery under WebKit with real keyboard input, in both list forms, both
themes and under `ar`: the button's reveal and its zero width while hidden, its accessible name, the arrow key
reaching it along the row, and Copy path raising its toast. These are what that could not settle:

- [x] Copy path in the packaged app: pressing the button with a real mouse and choosing Copy path left the change's
  path on the system clipboard, read back after the toast, so the plugin and the `clipboard-manager:allow-write-text`
  capability entry both reach it.
- [x] A pointer press on the action button opens the menu and does not open the file viewer behind it.
- [ ] A screen reader (VoiceOver) on the button: its name, and that the row's own name is not spoken twice.
- [ ] The button's contrast against the row in light mode, hidden and revealed, measured rather than read off a
  screenshot.

## Carried from milestone 07 (a document view for Markdown in the viewer)

Verified in the packaged app: the document renderer's own chunk loads from `tauri://localhost`, a link opens the
system browser with the viewer staying put, Select All then copy yields the file's Markdown source including the code
inside fenced blocks, and an untracked Markdown file opened from the Git mode renders as a document. The rest was
verified in the component gallery under WebKit with real input, in both themes and under `ar`. These are what neither
could settle:

- [ ] The document's loading state is centred in its frame. The change was made but never seen: the renderer's chunk
  loads too fast to catch the Suspense fallback, in the gallery and in the packaged app alike. Throttling, or a file
  large enough to slow the first draw, may make it visible.
- [ ] A screen reader (VoiceOver) on the rendered document: the region's name, the heading and list structure, the
  links, and the footnote heading that is marked for assistive technology only.
- [ ] A Tab walk in the real WKWebView rather than Playwright's WebKit build: the document region, then each link in
  it, then on out of the document.
- [ ] One measurement of a Markdown file near the viewer's text budget. The document is gated on the same budget as
  highlighting, on the stated grounds that a document that size would hold the window as long as highlighting it
  would — but highlighting tokenizes in workers and only pays an insert cost on the main thread, while
  `react-markdown` parses and builds the whole element tree on the main thread. The claim is plausible and unmeasured;
  if it does not hold, the document needs a smaller budget of its own.

## Notes for the developer

**Development notes**

- Every check that an agent can drive was handed to milestone 05 instead, since the only thing in its way was the
  machine's screen locking.
- The filter field's composition guard stops a `keydown` in the capture phase while a composition is in flight, so
  what the real-input check exercises is that guard, in the packaged app's WKWebView.

**Reference docs**

- `docs/memory/building-and-launching-the-app-for-verification.md`, `docs/memory/verifying-the-desktop-ui.md`
