# Final confirmation

> Goal: the checks that only a person can make are made, and anything they turn up is fixed here.
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

## Notes for the developer

**Development notes**

- Every check that an agent can drive was handed to milestone 05 instead, since the only thing in its way was the
  machine's screen locking.
- The filter field's composition guard stops a `keydown` in the capture phase while a composition is in flight, so
  what the real-input check exercises is that guard, in the packaged app's WKWebView.

**Reference docs**

- `docs/memory/building-and-launching-the-app-for-verification.md`, `docs/memory/verifying-the-desktop-ui.md`
