# Expanding collapsed lines in the viewer

> Goal: a collapsed run of unchanged lines in a diff can be expanded in the file viewer.
> Completion criteria: in the packaged app, expanding a separator in both the Uncommitted and Compare views shows the
> right lines in unified and split layouts; a file over the limit, a version mismatch and a failed read each show an
> explicit state; keyboard, screen reader, RTL and narrow-window behaviour match the rest of the viewer; product docs
> describe it.

## Technical design

- [ ] The renderer adapter accepts a loader for a diff's full bodies, without letting the renderer library's types
  define the daemon protocol.
- [ ] Separators are expandable only when the bodies can be offered; otherwise they keep today's form.
- [ ] One expansion reveals a small step; repeated expansions of the same gap grow until the whole gap is shown (see
  the overview's design decisions).

## Implementation plan

- [ ] On the first expansion of a diff, request its bodies (milestone 01) and hand them to the renderer; later
  expansions of the same diff reuse them.
- [ ] Show loading and failure states at the separator; a version mismatch leaves the patch shown as it is and says
  that the change has moved on.
- [ ] Update the product docs for the viewer.

## Notes for the developer

**Reusable capabilities**

- The application's renderer adapter for code and diffs; the renderer library supports expansion when given full file
  contents.
- The viewer's existing loading, failure and stale-reply handling for patches.

**Development notes**

- Keep HeroUI, localization, RTL, keyboard access and focus restoration consistent with the existing viewer.

**Reference docs**

- `docs/product/project-pane.md`, `docs/product/project-pane-git-mode.md`, `packages/ui/README.md`
