# A document view for Markdown in the viewer

> Goal: a Markdown file in the file viewer can be switched between its source and a rendered document view.
> Completion criteria: opening a Markdown file offers a source/document toggle with the viewer's other view controls;
> the document view renders headings, lists, links, tables and fenced code (highlighted as the source view highlights
> code); both themes, keyboard, screen reader and RTL behaviour match the rest of the viewer; product docs describe it.

## Technical design

- [ ] A source/document toggle shown only for Markdown files, in the viewer header's view-control slot.
- [ ] A Markdown document renderer, reached through the renderer adapter like the code and diff renderers, loaded
  only when the document view is first shown.

## Implementation plan

- [ ] Render the file body the viewer already has as a document; nothing new is requested from the daemon.
- [ ] Keep the rendered document inert: no scripts, no raw HTML executed, no remote resources fetched on open; a link
  opens through the platform adapter rather than navigating the viewer.
- [ ] Update the product docs for the viewer.

## Notes for the developer

**Reusable capabilities**

- The viewer's renderer adapter and its lazily loaded code renderer (Shiki), whose highlighting fenced code reuses.
- The diff layout toggle's header slot and the persisted preference mechanism, if the choice is remembered.

**Development notes**

- No Markdown library is in the UI's dependencies yet; choosing one is part of this milestone.

**Reference docs**

- `docs/product/project-pane.md`, `packages/ui/README.md`
