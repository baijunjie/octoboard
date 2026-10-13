# Empty states

Wherever a view, a list or a panel has nothing to show — no rows to list, no session selected, nothing to compare,
nothing left to draw — it puts an **empty state** in the content's place rather than leaving the area blank. Every one
of them takes one of two forms, a full panel or a compact row, so the same kind of nothing reads the same wherever it
turns up. A region that is still loading, or whose content failed, says so in a shape of its own instead; each of those
is described where it appears.

This is only the shape the empty states share. Which state a view shows, and in what words, is described where that
view is.

## The full panel

The form an empty state takes wherever it fills an area of its own, centred in it:

- **a glyph** on a round muted field, saying what kind of nothing this is.
- **a heading** naming what is missing, worded as a short title carrying no punctuation at all, a comma inside it
  included: a title that needs one is a sentence, and what follows the comma belongs in the line under it.
- **a line under the heading**, where there is more to say than the title. It is either a whole sentence saying what
  the heading leaves out — where what is missing comes from, or what to do about it — or a bare fact about the subject
  itself, such as the size of a file that cannot be drawn, or the size before and after for a change between two of
  them, which is not a sentence at all. A state whose title says all there is to say carries no such line.
- **one button**, where there is a single thing to do about the state, named by that action and sometimes carrying a
  glyph of its own.

The heading is a heading for assistive technology as well, so the panel shows in the outline of the view it fills, at
the level of the heading naming whatever the panel stands in for: the two then read as one thing rather than the panel
opening a section of its own. A panel filling a pane or a dialog is at the level of that pane's or dialog's own title;
a panel inside one of the sidebar's sections, whose heading is a level below those, is a level below them too. Text
too long for the panel's width wraps; nothing is cut.

Pressing the button with the mouse never takes keyboard focus of itself: whatever the action opens or starts is what
places focus. Where a region decides keyboard focus or its place in the `F6` cycle by whether the panel it holds has a
button to put focus on, that is said where the region is described (see "The terminal" in `docs/product/sessions.md`).

## The compact row

A list nested inside another — the sidebar's sections, the sessions under an expanded project, the trusted folders in
Settings — puts a single **empty row** in its rows' place instead: a glyph and the message on one line, in the padding
of the rows it stands among, with no explaining line under it. A message too long for the row wraps rather than being
cut. The message is not a heading, so a row adds nothing to the outline of the view it sits in, unlike a full panel.

Where there is one thing to do about the list being empty, **the whole row is the button** that does it, and a press
anywhere in the row does that one thing. Such a row:

- is **named by its visible message followed by its action** for assistive technology — "No projects yet", then "Add
  project" — so that saying the words on screen reaches it by voice;
- carries the action's name alone as its **tooltip**, on mouse hover and on keyboard focus.

A row with nothing to do about it is not a button and carries no tooltip.

## A state another region already announces

Where the region around an empty state already tells assistive technology what the state is, the panel itself is
hidden from assistive technology, so the state is not read twice; it is then on screen for the eye alone. Such a state
never carries a button, which would be left reachable by Tab with nothing to name it.

## The same state said in one sentence

Where a state does not stand in for content — the content is shown all the same, under a notice saying what the state
is, or the panel is hidden and a region around it speaks for it — the state is said as **one whole sentence** instead
of a title with a line under it. That sentence is worded on its own rather than being the title and the line run
together. The states this holds for are named where they appear (see "What it shows" and "When the open change
changes" in `docs/product/project-pane-git-mode.md`).
