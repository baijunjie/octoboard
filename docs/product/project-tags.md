# Project tags

A tag is a short free-form label the user puts on a project to group projects, and the sidebar's filter narrows the
project list by them (see "Filtering the project list" in `docs/product/sidebar.md`). There is no list of tags to create
or manage: the first time a tag is used, the user types it in a project's settings (see "Editing a project" in
`docs/product/projects.md`), and once that project is saved it is offered for other projects of the same console. The
tags offered are simply the distinct ones the console's projects carry, so a tag no project carries any more is gone
from the choices. A project can carry as many tags as the user likes. A tag is trimmed, an empty one is dropped, and one
that matches another of the project's tags ignoring case is dropped, the first spelling staying.

The optional Tags field's empty text input distinguishes a project with no tags from one with no additional tag being
entered. Leaving that input empty adds nothing and keeps any tags already picked.

## Picked tags

A tag the user has picked and can remove, in a project's Tags field, looks and behaves as follows. The sidebar's
Projects heading shows the tags of a filter in force as plain text, where none of this applies (see "Filtering the
project list" in `docs/product/sidebar.md`).

- It is filled with a light accent tint and has accent-coloured text.
- Its label is always one line. A label too long for its space fades out at its cut end and, while the mouse pointer is
  over the tag, runs as a marquee through its whole text (see "Names too long for their space" in
  `docs/product/labels-and-tooltips.md`); its full text is then also the tag's tooltip.
- Its remove button is hidden at rest. It shows while the mouse pointer is over the tag or the tag has keyboard focus,
  laid over the tag's end, the label fading out ahead of it. A label whose end the button then covers counts as cut,
  and its marquee scrolls until that end is clear of the button; a label that ends well short of the button does not
  move.
- The remove button's tooltip names the tag, set apart as a chip. Its accessible name names the removal and tag
  once; the tooltip adds no repeated description.
- At rest, the tag fits its content with no minimum width. While the pointer is over it or it has keyboard focus,
  it is at least 3rem wide so the remove button and its fade leave part of a short label visible.
- The tag takes the direction of its own text, decided by its first letter, whatever the UI language: a tag whose
  first letter is Hebrew, Arabic, Syriac, Thaana or N'Ko runs right to left, any other left to right, and a tag with no
  letter follows the UI language's direction. The remove button and the fade sit at the end the text runs to, so a
  Latin tag under the Arabic UI has its button on the right, and an Arabic tag under the English UI on the left.
- With keyboard focus on it, Backspace or Delete removes it.
