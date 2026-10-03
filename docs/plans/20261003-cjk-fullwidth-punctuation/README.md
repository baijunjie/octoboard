# Full-width punctuation under a CJK input method

> Goal: typing a full-width punctuation mark with a CJK input method puts it into the terminal on the first key press,
> as it does in a native macOS application.
> Completion criteria: with a Chinese input method active, pressing the key for `？` once inserts `？` into the agent's
> input line; the same holds for the other full-width marks the input method emits without a candidate window, and
> composed CJK *text* (which already works) is unaffected.

Low priority. This is a usability defect in one input path, not a blocker for anything else, and the terminal is usable
in the meantime by pressing the key twice or by switching the input method to English.

## Problem

Found during milestone 00's technical validation, in the prototype's `xterm.js` terminal inside WKWebView.

With a Chinese input method active, a full-width punctuation mark such as `？` — which the input method emits directly,
with no candidate window — **needs the key pressed twice**. The first press is swallowed. Composed CJK text, which goes
through candidate conversion, is not affected and lands correctly on the first try. Native macOS applications take the
punctuation on the first press, so the behaviour is the terminal's, not the input method's.

Everything below the composition layer was ruled out during validation: plain shifted ASCII symbols typed into the
window reach the PTY correctly, the same symbols sent straight to the daemon arrive intact, and the UI installs no
custom key handler. What remains is `xterm.js` 5.5.0's own composition handling in WKWebView.

## Key design decisions

- **Keep the framework's default composition handling; do not hand-roll one.** Writing a bespoke composition handler
  means owning input-method behaviour for every language and every macOS release, which is a far larger liability than
  the defect it would fix. Settled with the user.
- It follows that the fix has to come from one of: a newer `xterm.js` whose composition handling resolves it, a
  configuration option on the terminal, or an upstream fix. Which of the three applies is the open question below.

## Plan

- [ ] Reproduce on the real desktop application once it exists, and confirm the defect is not an artefact of the
      throwaway validation prototype
- [ ] Localise it first: the bisection done during validation ruled out everything *below* the composition layer, but it
      did not separate the terminal library from the web engine. Run the same terminal build under another engine and
      compare, so the next step is not chosen on an assumption
- [ ] Establish whether a newer `xterm.js` than 5.5.0 fixes it, and whether any terminal option changes the behaviour
- [ ] If neither does, report it upstream with the reproduction, and record here what the upstream outcome was
- [ ] Once a fix exists, take it and verify against the completion criteria above — including that composed CJK text
      still works

## Notes for developers

- **Development notes**: verifying this needs a person. Scripted key injection bypasses macOS input methods entirely,
  so an automated check would prove nothing about this path either way.
- **Reference docs**: the terminal requirements this sits under are in `docs/mvp.md` section 3, and the sibling
  keyboard and WebView pitfalls — including this defect's own entry — are in its "Known pitfalls of the Tauri / Rust
  approach" section.
