# 01 Every remaining scroller fades

> Goal: no surface in the interface scrolls content out of sight without fading the edge it hides it behind.
> Completion criteria: the file viewer's code and Markdown surfaces, the branch list of a branch comparison, the
> project filter's tag list, the rail's console column, the startup screen and the error boundary's stack block each
> fade at an edge with more to scroll towards, and stop fading at an edge with nothing; each takes its fade from the
> shared way rather than styles of its own; a surface that fits shows no fade.

## Technical design

- [ ] A surface takes its fade from the shared capability already used elsewhere, which exists in two forms — the
      element that is itself the scroller, and the hook for a scroller that cannot be wrapped because it has to stay
      the scroll container of its own rows. Which form a surface takes follows from whether it owns its scrolling.
- [ ] A reading surface fades like any other unless the Open question about its size is settled otherwise.
- [ ] A surface inside a popover fades the same way; a popover is not a reason to skip it.

## Implementation plan

- [ ] Each of the listed surfaces takes a fade, one surface at a time, each verifiable on its own.
- [ ] A surface whose element animates away keeps the scroller inside that element rather than on it, as the key
      design decision in the overview requires.
- [ ] A surface that was given a hand-built mask or a bespoke gradient drops it in favour of the shared way; none is
      known, but the sweep is what would find one.
- [ ] Every surface this milestone touches can be seen in the UI state gallery, both overflowing and fitting, so the
      difference is visible rather than asserted.

## Notes for the developer

**Reusable capabilities**

- The UI package already has both forms of the fade — a component that is the scroller, and a hook for a scroller
  that must stay its own container — and the second already carries the size every fade in the application shares.
- The dialog component already owns body scrolling, its fade, the scroll padding that keeps a focused control clear
  of that fade, and the focusable region for a body whose controls are all in the footer. A surface inside a dialog
  body needs none of that again.

**Development notes**

- The UI state gallery is written only through its own skill, which dispatches the subagent that owns it.
- Every user-visible string goes through the message catalogs; a focusable region needs an accessible name, and that
  name is copy.
- The accessibility standard holds a section on a focus scrolled to the edge of a scroller that fades there; a new
  scroller that takes focus has to follow it.

**Reference docs**

- `docs/ui-accessibility.md` — the UI accessibility standard.
- `docs/memory/writing-ui-components.md` — the convention this milestone carries out.
- `packages/ui/README.md` — where each surface and the shared capabilities live.
