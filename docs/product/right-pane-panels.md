# Right pane panels

The window's right pane (see "What the right pane shows" in `docs/product/window-layout.md`) is a vertical **stack of
panels**. Each **panel** is a card of its own, of one **kind**, and which kinds there are depends on the pane's owner:

| Owner | Kinds |
|---|---|
| A project | **Files**, the **Files panel**: the project's files and its repository's changes (see `docs/product/project-pane.md`) |
| A console session | **Report**, the **report panel**: the pages the console session pushed (see `docs/product/report-panel.md`) |

Each owner has a single kind, so a stack never holds more than one panel yet, and the resize handle between panels and
Maximize described below do not appear.

**The right pane is on screen while at least one of its owner's panels is open**, docked beside the terminal at every
window width; with none open there is no right pane in the row, and its width goes to the terminal. There is no
toggle for the pane as a whole: it is shown and hidden by opening and closing its panels, from the panel toggles in
the top bar (see "The panel toggles in the top bar" below) and from each panel's own header. With no panel open, a
panel can still float in over the terminal for a moment (see "Floating a panel in" below).

How wide the right pane is and how it is resized is in "Pane widths at 1168 px and wider" and "Resizing the sidebar
and the right pane" in `docs/product/window-layout.md`.

## The panel cards

The right pane has **no frame or surface of its own**: it is its cards. Each card looks exactly like the sidebar's
floating card (see "A hidden pane floats in on hover" in `docs/product/window-layout.md`): rounded corners, the
hairline edge, the large elevation shadow and the sidebar's background, docked and floating alike. The cards keep
**8 px** from the top bar, from the window's end edge and from its bottom (or the connection banner), and **16 px**
from the terminal and from each other; those 16 px gaps hold the resize grips (see "Resizing the sidebar and the right
pane" in `docs/product/window-layout.md` and "Resizing panels" below). What shows around and between the cards is the
terminal's area, in the terminal's own background, so the cards read as lying on the content. A single open panel
takes the stack's full height.

A panel is never shorter than **120 px**.

## A panel's header

Every panel starts with a **header**, laid out as the sidebar's floating card's header is: **40 px** high, with the
same inset, title size and weight, and icon buttons, so the line under it stays level with the line under the docked
panes' headers beside it (see "The window chrome and the content panel" in `docs/product/window-layout.md`). It holds,
from its leading side:

- the panel's **title** — the project's name for the Files panel, "Report" for the report panel;
- whatever the kind puts beside its title — the Files panel's Files and Changes tabs (see "Opening a project's files"
  in `docs/product/project-pane.md`), the report panel's pager (see "Paging through the history" in
  `docs/product/report-panel.md`);
- the kind's own icon buttons, at the trailing side;
- **Maximize panel**, or **Restore panel size** while the panel is maximized, there only while more than one panel is
  open (see "Maximizing a panel" below);
- **Close panel** (see "Opening and closing panels" below).

When space is tight the title gives way, fading out with its full text as its tooltip while it is cut (see "Names too
long for their space" in `docs/product/labels-and-tooltips.md`); the buttons at the end never give way. Pressing a
header button with the mouse leaves keyboard focus where it was.

The pane's 16 px horizontal inset (see "The window chrome and the content panel" in `docs/product/window-layout.md`)
is measured from the card's edge.

## Opening and closing panels

- **Each kind is in its owner's stack at most once.**
- **Panels are stacked in a fixed order of kinds**, whatever order they were opened in.
- **Close panel** takes a panel out of the stack. Its height goes to the panels that remain, in proportion to theirs.
- **Closing the last panel hides the right pane**, at once; the terminal takes its width back.
- **Browse files** opens the Files panel in the project's stack, restoring the stack if another panel's maximizing hid
  it, and so puts the pane on screen (see "Opening a project's files" in `docs/product/project-pane.md`).
- A panel that opens takes an even share of the stack's height — a third of it when it makes three — taken from the
  others in proportion to their heights.

Opening the first panel docks the pane at once, with no animation, and takes its width from the terminal, which is
resized (see "Output, scrollback and size" in `docs/product/session-terminal.md`).

## The panel toggles in the top bar

At the top bar's end there is **one toggle per kind of the current owner's panels**: the Files toggle while a project
owns the right pane, the Report toggle while a console session does, and none while nothing owns it (see "The top bar"
in `docs/product/window-layout.md`).

| Toggle | Icon | Label while its panel is not on screen | Label while it is |
|---|---|---|---|
| Files | a closed folder | Show files panel | Hide files panel |
| Report | a document | Show report panel | Hide report panel |

- **The label says what pressing it will do**, and is also its tooltip.
- **The icon stays the same whatever the state.** A toggle whose panel is on screen is drawn with the pressed fill, the
  same fill the current console's tile has on the rail, which it keeps under the pointer and while pressed.
- It tells assistive technology whether its panel is expanded: expanded while the panel is on screen.

A panel counts as **on screen** while it is open in the docked pane and not hidden by another panel's maximizing. A
panel only floating in counts as not on screen.

Pressing a toggle:

- **while its panel is not open** opens it, which docks the pane if no panel was open, a floating card included (see
  "Floating a panel in" below);
- **while its panel is hidden by another panel's maximizing** restores the stack;
- **while its panel is on screen** closes it, and closing the last panel hides the pane.

Pressing a panel toggle leaves keyboard focus where it was, as every control in the top bar does (see "Pressing the
chrome's controls" in `docs/product/window-layout.md`).

## Floating a panel in

While the right pane has an owner and none of its panels is open, **resting the mouse on a panel toggle floats that
panel's card in** over the terminal, at any window width; moving onto another toggle shows that toggle's panel
instead. Resting on a toggle while a panel is open does nothing. How the floating card enters and leaves, what keeps
it up and how `Escape` sends it away are in "A hidden pane floats in on hover" in `docs/product/window-layout.md`.

- The card is the panel's own card, at the right pane's chosen width (at most 92% of the window's), with the same
  gaps around it as docked; the terminal shows in those gaps and is not resized.
- **Pressing the toggle** opens the panel, docking the pane at once, with no animation; keyboard focus stays where it
  was.
- **Close panel** in a floating card sends it away, opening nothing.
- Floating a panel in closes the sidebar's drawer (see "Below 1168 px: the sidebar becomes a drawer" in
  `docs/product/window-layout.md`).

## Resizing panels

Between two neighbouring panels, in the gap that separates them, is a **resize handle**, drawn as a grip like the
right pane's width handle (see "Resizing the sidebar and the right pane" in `docs/product/window-layout.md`), with the
same "Resize" tooltip. It moves only the boundary between that pair: the pair keeps its combined height, neither panel
goes under 120 px, and the other panels and the stack's own height stay as they are, so **resizing panels never
resizes the terminal**. Its name says which pair it resizes, naming both panels' kinds, upper one first.

- **Dragging** it moves the boundary.
- **Double-clicking** it gives the pair an even split of their combined height.
- It is reachable with Tab and is then operated from the keyboard: Down makes the upper panel taller and Up shorter,
  by 16 px, by 64 px with Shift held; Home makes the upper panel as short as it can be and End as tall. The handle is
  not mirrored under a right-to-left language.
- Pressing it with the mouse does not take keyboard focus off the terminal, and while it is dragged the pointer shows
  the resize cursor wherever it is.

## Maximizing a panel

**Maximize panel** in a panel's header hides the other panels and gives it the whole stack's height. If keyboard focus
was in one of the panels being hidden, it moves first to the maximized panel, where `F6` would land in it. **Restore
panel size** in its header brings the others back at their heights. The stack is also restored when a hidden panel is
asked for — its toggle in the top bar pressed, or Browse files for the Files panel — when another panel is opened,
and when the maximized panel is closed.

A panel hidden by another's maximizing keeps what it shows as it was, so it comes back as it was left.

Which panel is maximized is not remembered: a reload shows every open panel.

## What is remembered

**Per kind of owner, per client**, in that client's own browser storage, as the right pane's width is (see "Resizing
the sidebar and the right pane" in `docs/product/window-layout.md`): which panels are open and each one's share of the
stack's height, so the stack comes back as it was after a reload or a restart. Without that storage they last until
the page is reloaded.

There are two stacks, one for projects and one for console sessions: every project shares the first and every console
session the second, so passing the right pane to another project keeps the open panels, their heights and which one
is maximized, and so does passing it to another console session. Until the user changes it, every kind of each owner
is open, so the right pane is shown. A stored stack that cannot be read counts as that default. A right pane the user
had hidden with the earlier pane toggle of Octoboard starts with no panel open in either stack.

## Panels and keyboard focus

Each panel is a **named region** — the Files panel named for the project's files, the report panel "Report" — and each
is a stop of its own in the `F6` cycle, in stacking order (see "Moving focus between regions with F6" in
`docs/product/moving-focus-between-regions.md`). A panel only floating in is not a stop.

**Closing a panel that holds keyboard focus hands focus to the terminal** — or, where the terminal cannot take it,
where it goes instead (see "With no session selected" in `docs/product/session-terminal.md`). So does the right pane
going while its width handle holds focus.
