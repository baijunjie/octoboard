# 03 File viewer notices become status marks

> Goal: the file viewer shows no Alert; each notice is a status mark from 02.
> Completion criteria: whole-file notices are marks after the file path in the viewer header; notices about one spot in
> a Markdown document or a diff are marks at that spot; the viewer's Alert-based `Notice` is gone; a notice appearing
> after first paint is still spoken once, and one present from first paint is not; viewer tests and the viewer gallery
> scenarios pass, and the user has seen them in the preview.

## Technical design

- [ ] Whole-file notices, one mark each after the path: syntax highlighting off for size, not renderable or failed;
  Markdown rendering failed; image shown as text; the file still in conflict; a side outside the project; the change
  identical or renamed unchanged; change content failed, changed or unavailable; patch too large or failed; no newline
  at end of file; expansion notes and expansion failure. The conflict view's file-read failure (an Alert with Try again
  today) also becomes an error mark whose popover holds Try again.
- [ ] Spot notices stay where they are, as marks: a diagram or formula too large or failed to render, an image too
  large or failed. A block-level one (a diagram, a display formula) sits at the block's place beside its fallback.
- [ ] Tone keeps its meaning (explained → note, warned → warning, failed → error), so the existing tone rules in the
  viewer still pick the glyph.
- [ ] Each mark has an accessible name; its sentence is its content. Announcement keeps today's split: `speak` for a
  notice that can arrive after paint, `announced` where the surface already speaks it, neither for first paint.

## Implementation plan

- [ ] Give the viewer header a place for marks after the path, keeping the header's tab order and floating controls
  working.
- [ ] Route whole-file notices there and spot notices to their place; remove `Notice` and its Alert use.
- [ ] Update `docs/product/` viewer behaviour and `docs/ui-accessibility.md` where it describes the viewer's notices,
  through the docs workflow.
- [ ] Have the gallery's viewer scenarios updated (gallery-writer) to show marks with each tone, several marks on one
  file, and a mark appearing after paint.

## Notes for the developer

- **Reusable capabilities**: `StatusAnnouncer` and the viewer's first-announcement delay; the existing tone type;
  `InlineNotice` for marks in running text.
- **Development notes**: the header's tab order and floating controls are constrained (see the comment on the header
  in the viewer); keep marks out of the path's marquee scope.
- **Reference docs**: `docs/ui-accessibility.md` ("A failure that belongs to one row", "Announcing a change", "Not by
  colour alone"); `docs/voiceover-cases.md` (coordinate with the UI standard audit's milestone 11 if it is touched).
