# The change list as a directory tree

> Goal: the change list can be shown flat or grouped by directory, and the choice is remembered.
> Completion criteria: both the Uncommitted and Compare lists switch between flat and tree; the viewer's previous/next
> order follows the rows as shown; collapsed directories are skipped; keyboard, screen reader, RTL and narrow-pane
> behaviour match the Files tree; product docs describe it.

## Technical design

- [ ] A persisted flat/tree preference.
- [ ] A tree built from the listed changes' paths, under the existing sections.

## Implementation plan

- [ ] Add the toggle to the Git mode, and render the tree form of each section.
- [ ] Keep viewer navigation consuming the visible rows in order, as the Files tree does.
- [ ] Update the product docs for the Git mode.

## Notes for the developer

**Reusable capabilities**

- The Files mode's virtualized tree and its keyboard model; the persisted preference mechanism.

**Reference docs**

- `docs/product/project-pane-git-mode.md`, `packages/ui/README.md`
