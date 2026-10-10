# The viewer header's layout and tags

> Goal: the viewer header's details are separated rather than run together, using the width beside the view controls,
> its path reads as paths do elsewhere in the app, and its tags read as tags in both themes.
> Completion criteria: in the packaged app, in both themes, the path shows with an icon prefix as other paths do; the
> details and the view controls share the header's width, and a detail too long for the space left (the comparison's
> branches, a rename's origin) moves to a row of its own instead of crowding the others; every tag, including the neutral stage tag, is visibly a tag against the dialog; the spacing between tags and
> between the tags and the name is consistent; product docs describe the new layout.

## Technical design

- [ ] The header's details (path, size, comparison, rename origin) are distinct items laid out beside the view
  controls, using the width left of them; an item that does not fit takes a row of its own rather than being squeezed
  into one shared row.
- [ ] Paths in the header (the file's path, a rename's origin) use the app's shared path display with an icon prefix.
- [ ] Neutral tags in the dialog get a fill that stands off the dialog's background in both themes, still HeroUI's
  Chip and still distinct from the coloured kind-of-change tags.
- [ ] Tags are spaced from each other more tightly than the tag group is spaced from the name.

## Implementation plan

- [ ] Rework the header's details and their fade/tooltip handling to the new layout.
- [ ] Choose the fill by measuring it against the dialog's background in both themes, as the existing chip colours
  were measured; if it is done by overriding HeroUI's `--default` family for the dialog, override the whole family,
  as the sidebar does.
- [ ] Update the product docs' description of the header, including why the details share the view controls' row
  (to use the width beside them, not to keep one row at any cost).

## Notes for the developer

**Reusable capabilities**

- The shared path display with a folder icon used by the Settings sections, and the start-clipped path text; the
  viewer's status chip component and its status-mark colours.

**Development notes**

- The stage tag is HeroUI's Chip with the default colour in the soft variant and no custom colour. HeroUI's own
  showcase draws it on the page background (L 0.12 in dark), where it stands out; the viewer draws it on the dialog's
  `--overlay` (L 0.21), and the soft fill (half of `--default`, L 0.274) lands almost on that, so it does not read as
  a tag. The same `--default` family was already overridden for the sidebar for this reason.
- The chip contrast audit in the UI-components memory is the reference for measuring a chip's fill.

**Reference docs**

- `docs/product/project-pane.md`, `docs/product/project-pane-git-mode.md`, `docs/memory/writing-ui-components.md`
