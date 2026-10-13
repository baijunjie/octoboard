# 02 Hand-built modals move onto `Dialog`

> Goal: a modal in this interface is a `Dialog`, so that what a dialog has learned reaches every one of them.
> Completion criteria: the settings dialog is a `Dialog` rather than a modal assembled from the library's own parts,
> and so is anything else the sweep finds; each keeps the layout and behaviour it had, and gains what `Dialog` gives
> without stating any of it itself — the body as the scroller, its fade, the scroll padding that keeps a focused
> control clear of that fade, and the focusable region for a body whose controls are all in the footer.

## Technical design

- [ ] A modal states what it is and what it holds; it does not state how its body scrolls, where it fades, or how a
      keyboard reaches that body.
- [ ] Where a modal's content is a set of panels rather than one body, the shape that carries them keeps the
      scroller inside whatever animates away, per the overview's decision.
- [ ] `Dialog` gains what such a modal needs only where the need is general; a need of one modal alone stays with
      that modal rather than widening the shared component.

## Implementation plan

- [ ] Sweep for modals assembled from the library's parts rather than from `Dialog`; the settings dialog is the one
      already known.
- [ ] Each is moved over on its own, keeping its current layout and keyboard behaviour, and verified against what it
      looked like and did before.
- [ ] Each drops whatever it had hand-built that `Dialog` now provides, so the behaviour has one definition.
- [ ] The convention in development memory says plainly that a modal is a `Dialog`, and names what that settles, so
      the next modal is not assembled by hand.

## Notes for the developer

**Reusable capabilities**

- The dialog component already carries the sizes, the footer, the body's scrolling and fade, the focus handling and
  the request-driven opening the rest of the interface uses.

**Development notes**

- Moving a modal changes a surface already in use, so the change reaches every state that surface can be in, not
  only the one being looked at; settle it by putting each state side by side against the merge base rather than by
  judging the new rendering on its own.
- The settings dialog holds the sections the whole application is configured from; its tab panels are the place the
  exit-transition pitfall in the overview was met.

**Reference docs**

- `docs/memory/verifying-the-desktop-ui.md` — how to settle a change to a surface already in use.
- `docs/memory/building-on-heroui-and-react-aria.md` — that a modal is a `Dialog`.
- `docs/memory/writing-ui-dialogs-and-fields.md` — what a dialog's body does with its overflow.
- `docs/product/settings.md` — what the settings dialog has to keep doing.
