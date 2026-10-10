# Filtering the change list by file name

> Goal: the user can narrow the change list by typing part of a file name.
> Completion criteria: a filter field above the list narrows rows in both the flat and tree forms and in both views;
> an empty result says so; clearing restores the list; viewer navigation follows the filtered rows; input methods
> compose without triggering list keys; product docs describe it.

## Handoff

Milestones 02, 03 and 04 are implemented and reviewed, but none of them was ever seen in the packaged app: every
attempt was stopped by the machine's screen locking, and one by the access request being declined. Keeping the
machine from idle-sleeping does not prevent the lock — that comes from the user's own security setting, so the
run needs them to lengthen or turn off the password-after-screensaver wait, stay at the machine, and approve
controlling the verification copy once.

Make this run before, or alongside, this milestone's own work;
`docs/memory/building-and-launching-the-app-for-verification.md` has the route (build, copy, change the bundle id,
re-sign ad hoc, run under a throwaway `HOME` and `TMPDIR`, never touching the user's own `dev.octoboard.app`), and
`docs/memory/verifying-the-desktop-ui.md` the rules for driving it. Anything it turns up is fixed here, in the milestone
the defect belongs to.

From milestone 02 (expanding a diff's collapsed lines):

- [ ] Right-to-left with text that is actually right-to-left. The run so far used mirrored English, as no Arabic
  catalog exists; confirm against a language the app has, or against the mirrored layout with right-to-left
  content in the file itself.
- [ ] A window below 800 px, where "Show whole file" is positioned absolutely and may cover the end of a long
  separator label. The macOS window's minimum is 1148 px, so this one is reached in the gallery, not the app.

From milestone 03 (word wrap):

- [ ] The wrap toggle in the viewer header, with the header's rows unchanged by it.
- [ ] Wrapping on for a file and for a diff in both layouts, the split layout's two sides staying level.
- [ ] The choice surviving a real restart of the app, not only a written preference.
- [ ] A diff with wrapping off, scrolled sideways: its separator controls still usable and not clipped.

From milestone 04 (the change list as a directory tree):

- [ ] The Group by folder toggle switching flat to tree and back, with the tab row's layout intact at the pane's
  normal width, at the aside's 300 px minimum and under a right-to-left language. The toggle is laid over the tab
  row absolutely, and at 300 px the tab list has about 244 px, where `Tabs.ListContainer`'s own scroll chevrons
  would sit under it.
- [ ] The tree under the existing sections: counts unchanged, directories before changes, a single-child chain as
  one row.
- [ ] Folding and unfolding by pointer and by Left/Right, and a fold surviving a switch between the two views.
- [ ] Previous/Next following the rows as shown, skipping changes under a folded directory, and a change open when
  its directory is folded keeping its place.
- [ ] The grouping choice surviving a real restart, and the folded directories not surviving it.

## Technical design

- [ ] A filter field whose text narrows the rows shown; section counts follow what is shown.

## Implementation plan

- [ ] Apply the filter to the listed changes before they are laid out, flat or as a tree.
- [ ] Update the product docs for the Git mode.

## Notes for the developer

**Development notes**

- The list's type-ahead and the filter field must not compete for the same keys.
- An earlier attempt at this milestone was discarded unfinished; nothing of it survives, and the approach is open
  again.

**Reference docs**

- `docs/product/project-pane-git-mode.md`
