# Filtering the change list by file name

> Goal: the user can narrow the change list by typing part of a file name.
> Completion criteria: a filter field above the list narrows rows in both the flat and tree forms and in both views;
> an empty result says so; clearing restores the list; viewer navigation follows the filtered rows; input methods
> compose without triggering list keys; product docs describe it.

## Technical design

- [ ] A filter field whose text narrows the rows shown; section counts follow what is shown.

## Implementation plan

- [ ] Apply the filter to the listed changes before they are laid out, flat or as a tree.
- [ ] Update the product docs for the Git mode.

## Notes for the developer

**Development notes**

- The list's type-ahead and the filter field must not compete for the same keys.

**Reference docs**

- `docs/product/project-pane-git-mode.md`
