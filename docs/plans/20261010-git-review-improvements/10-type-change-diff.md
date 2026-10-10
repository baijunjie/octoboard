# Showing a type change as one diff

> Goal: a type change is shown the same way as any other changed file — one diff with the unified/split choice —
> instead of two separate Before and After diffs.
> Completion criteria: a type change (file to symbolic link and the reverse) and a rename into or out of a path below
> itself each show as one diff, with the layout toggle, in both views and both layouts; product docs describe it.

## Technical design

- [ ] A change whose patch has two sections (a removal and an addition) is presented as one diff of the old side
  against the new side rather than as two sections.

## Implementation plan

- [ ] Combine the two sections' sides into one diff for the renderer, keeping the kind of change in the title's tag.
- [ ] Update the product docs' description of a type change.

## Notes for the developer

**Development notes**

- The split today is a presentation choice, not a renderer limit: git writes a type change as a removal plus an
  addition, and the viewer draws each as its own diff. Check how the renderer library handles the combined form.

**Reference docs**

- `docs/product/project-pane-git-mode.md`
