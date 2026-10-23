# 02 Error and warning status marks

> Goal: one status mark for an error, a warning or a note, whose content opens from the glyph in one floating surface
> only — a popover for errors and warnings (also opened by mouse hover), the 01 wrapper for notes.
> Completion criteria: `FailureMark`'s callers (file tree folder failure, Git pane section and branch-list failures,
> account quota read failure and out-of-date reading, import result row, inline Markdown failures) show one surface;
> no mark shows a tooltip and a popover; hovering an error or warning mark with a mouse opens its popover without moving
> focus and keeps it open while the pointer moves into it; a press opens it with focus inside; the standard is updated;
> tests and the affected gallery scenarios pass.

## Technical design

- [ ] Tones: danger and warning open a popover; a note (informational) uses the 01 wrapper. Glyphs per tone: circled
  `!`, triangle, circled `i`.
- [ ] Popover opening: a press (mouse, touch, Enter, Space) opens it and moves focus in; a mouse hover opens it without
  moving focus, after a short delay, and closes it after a short grace once the pointer has left both the mark and the
  popover. Escape closes it and returns focus where it was.
- [ ] The popover is non-modal while hover-opened (`aria-modal="false"`) and takes part in the topmost-Escape rule, as
  the rail quota panel does.
- [ ] The name-repeating hover tooltip is removed; the mark's accessible name stays (`label`).
- [ ] What `FailureMark` supports today carries over: the visible word after the glyph (`text`), the in-header placement
  and its focus return, running-text size (`inline`), Try again closing the popover.

## Implementation plan

- [ ] Rebuild `FailureMark` (renamed if a tone-neutral name fits better) on the hover-openable popover; the content-less
  form becomes the note/text form on the 01 wrapper.
- [ ] Check every caller listed in the completion criteria, including surfaces that watch presses outside themselves
  (the rail quota panel tells the mark's popover apart by its marker attribute).
- [ ] Update `docs/ui-accessibility.md` "A failure that belongs to one row": always a popover for an error or warning,
  opened by hover as well as press; drop "or the reason alone in a tooltip"; describe hover opening and focus.

## Notes for the developer

- **Reusable capabilities**: the rail quota panel's hover-opened, non-modal popover (focus, Escape, outside press);
  `FailureBody` for the popover's content order; `AlertButton` for Try again.
- **Development notes**: a hover-opened popover must never steal focus from the terminal or an editor; the in-header
  mark's focus return to the section heading must keep working.
- **Reference docs**: `docs/ui-accessibility.md` ("A failure that belongs to one row", "A floating panel that does not
  take the window", "Announcing a change"); `docs/voiceover-cases.md`.
