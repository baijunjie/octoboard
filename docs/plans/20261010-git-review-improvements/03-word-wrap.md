# Word wrap in the viewer

> Goal: the user can choose whether the file viewer wraps long lines, and the choice is remembered.
> Completion criteria: a wrap toggle sits with the viewer's other view controls; code opens unwrapped by default; the
> last choice applies to the next file and after a restart; the header's layout is unchanged; keyboard, screen reader
> and RTL behaviour match the diff layout toggle; product docs describe it.

## Technical design

- [ ] A persisted word-wrap preference, defaulting to unwrapped for code.
- [ ] A wrap toggle in the viewer header, beside the diff layout toggle.

## Implementation plan

- [ ] Pass the preference to the renderer adapter for files (and diffs, per the overview's open point).
- [ ] Update the product docs for the viewer.

## Notes for the developer

**Reusable capabilities**

- The persisted preference mechanism and the diff layout toggle's header slot.
- The renderer library's own wrap option, reached through the renderer adapter.

**Reference docs**

- `docs/product/project-pane.md`, `packages/ui/README.md`
