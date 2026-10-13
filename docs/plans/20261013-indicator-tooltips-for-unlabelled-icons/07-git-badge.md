# Milestone 7: The branch badge

> Goal: The branch badge says its whole state in one tooltip, covering the badge's state glyph and its ahead and
> behind counts together.
> Completion criteria: Resting the mouse anywhere on the badge — the glyph, the branch name or either count — shows,
> after the default delay, the same sentence the badge's accessible name is, on a sidebar project row and in focus
> mode's header, in the checking, fast-forwarding, detached, on-a-branch, ahead, behind and error states and in the
> squeezed layouts where the branch name gives way; the state that shows visible text instead of a branch carries no
> tooltip; the badge's two earlier tooltips — the error's and the clipped branch name's — are gone and nothing they
> said is lost; the branch name still runs as a marquee while the pointer is over the row; the accessible names of
> the row and of focus mode's header are unchanged; type check and the affected tests pass; the product doc records
> the badge's one tooltip in place of the warning triangle's, and the gallery shows it, including a squeezed layout.

## Technical design

- [ ] One tooltip for the whole badge, whose text is the badge's existing accessible-name sentence — the branch
  phrase, the ahead and behind counts and the error, joined as they already are. No new catalog key.
- [ ] The badge's accessible name keeps reaching assistive technology as it does now: named on the tooltip's trigger
  where the badge names itself, and left unnamed where the row around it folds the same sentence into its own name.
- [ ] The error-only tooltip on the state glyph and the clipped-text tooltip on the branch name are removed, their
  content being part of that sentence.
- [ ] The state that says "no commits yet" in visible text gets no tooltip.

## Implementation plan

- [ ] Make the badge itself the tooltip's trigger, keeping the layout it has: the trigger has to be the part that
  gives way when the row is squeezed, or the badge will stop collapsing as it does now.
- [ ] Check the squeezed layouts again afterwards: the whole point of putting the tooltip on the badge rather than
  on each element is that the branch name can still be read once it has been squeezed out.
- [ ] Replace the warning triangle's entry in the indicator list in the product doc with the badge's, and say in the
  branch badge's own doc what the tooltip carries.
- [ ] Show the tooltip in the gallery, in a plain state and in a squeezed one, and check it there.

## Notes for the developer

- **Reusable capabilities**: the badge's existing accessible-name sentence and the messages it is built from; the
  indicator tooltip shape, which the badge's error tooltip already uses; the gallery's squeeze scenarios.
- **Development notes**: this is the one place in the plan where the tooltip deliberately covers several marks at
  once, because the badge's accessible name is already one sentence about all of them — unlike the change row's
  letter, which is one mark with one meaning. Two consequences to accept rather than work around: hovering a fully
  visible branch name shows a sentence that repeats it, and the badge's state glyph has five states, not four, the
  fifth being the error whose tooltip this replaces. The catalog comment that says those branch phrases are only
  ever an accessible name stops being true and needs correcting.
- **Reference docs**: `docs/product/labels-and-tooltips.md`, `docs/product/project-git-status.md`,
  `docs/ui-accessibility.md`, `docs/memory/building-on-heroui-and-react-aria.md`,
  `docs/memory/ui-text-and-reading-direction.md`.
