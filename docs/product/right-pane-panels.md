# Right pane panels

The window's right pane (see "What the right pane shows" in `docs/product/window-layout.md`) is a vertical **stack of
panels**. Each **panel** is a card of its own, of one **kind**, and which kinds there are depends on the pane's owner:

| Owner | Kinds |
|---|---|
| A project | **Files**, the **Files panel**: the project's files and its repository's changes (see `docs/product/project-pane.md`); **Terminal**, the **Terminal panel**: the project's shell terminals as tabs (see `docs/product/terminal-panel.md`) |
| A console session | **Report**, the **report panel**: the pages the console session pushed (see `docs/product/report-panel.md`) |

**The right pane is on screen while at least one of its owner's panels is open**, docked beside the terminal at every
window width; with none open there is no right pane in the row, and its width goes to the terminal. There is no
toggle for the pane as a whole: it is shown and hidden by opening and closing its panels, from the panel toggles in
the top bar (see "The panel toggles in the top bar" below) and from each panel's own header. With no panel open, a
panel can still float in over the terminal for a moment (see "Floating a panel in" below). A maximized Terminal panel
leaves the pane's column for the terminal's area (see "Maximizing the Terminal panel" below).

How wide the right pane is and how it is resized is in "Pane widths at 1168 px and wider" and "Resizing the sidebar
and the right pane" in `docs/product/window-layout.md`.

## The panel cards

The right pane has **no frame or surface of its own**: it is its cards. Each card looks exactly like the sidebar's
floating card (see "A hidden pane floats in on hover" in `docs/product/window-layout.md`): rounded corners, the
hairline edge, the light card shadow and the sidebar's background, docked and floating alike. The cards keep
**8 px** from the top bar, from the window's end edge and from its bottom (or the connection banner), **8 px** from
the terminal and **10 px** from each other; the gaps on the terminal's side and between cards hold the resize grips
(see "Resizing the sidebar and the right pane" in `docs/product/window-layout.md` and "Resizing panels" below). What shows around and between the cards is the
terminal's area, in the terminal's own background, so the cards read as lying on the content. A single open panel
takes the stack's full height.

A panel is never shorter than **120 px**.

## A panel's header

Every panel starts with a **header**, laid out as the sidebar's floating card's header is: **40 px** high, with the
same inset, title size and weight, and icon buttons, so the line under it stays level with the line under the docked
panes' headers beside it (see "The window chrome and the content panel" in `docs/product/window-layout.md`). It holds,
from its leading side:

- the panel's **title** — the project's name for the Files panel, "Terminal" for the Terminal panel, "Report" for the
  report panel;
- whatever the kind puts beside its title — the Files panel's Files and Changes tabs (see "Opening a project's files"
  in `docs/product/project-pane.md`), the Terminal panel's tabs (see "Tabs" in `docs/product/terminal-panel.md`), the
  report panel's pager (see "Paging through the history" in `docs/product/report-panel.md`);
- the kind's own icon buttons, at the trailing side;
- in the Terminal panel's header only, **Maximize panel**, or **Restore panel size** while it is maximized, there
  whenever the panel is open, alone or not, and not while it only floats in (see "Maximizing the Terminal panel"
  below); the Files and report panels have neither;
- **Close panel** (see "Opening and closing panels" below).

The title is at most 160 px wide; a longer one fades out, with its full text as its tooltip while it is cut (see
"Names too long for their space" in `docs/product/labels-and-tooltips.md`). In the Files panel, **Files and Changes
and Close panel stay fully visible**. As space tightens, the project title shrinks, fades and disappears before the
other icon buttons lose room. Those buttons scroll horizontally within their own area, fading at edges with more
beyond them; Close panel stays outside that area. Scrolling and moving keyboard focus through the buttons reveal
them, and the area's fades disappear while a button has keyboard focus so its focus ring stays visible.

In the Terminal panel, the header stays on one row. Its visual title appears only when the window is at least
1168 px wide and the header has at least 352 px of content width, excluding its horizontal padding. Otherwise the
same title remains available as a level-two heading to assistive technology and gives its space to the tabs. The tabs
scroll when needed, while the buttons at the end keep their width. In the report panel, the pager gives way while the title
and trailing buttons keep their width. Pressing a header button with the mouse leaves keyboard focus where it was.

The pane's 16 px horizontal inset (see "The window chrome and the content panel" in `docs/product/window-layout.md`)
is measured from the card's edge.

## Opening and closing panels

- **Each kind is in its owner's stack at most once.**
- **Panels are stacked in a fixed order of kinds**, whatever order they were opened in: in a project's stack the Files
  panel is above the Terminal panel.
- **Close panel** takes a panel out of the stack. Its height goes to the panels that remain, in proportion to theirs.
- **Closing the last panel hides the right pane**, at once; the terminal takes its width back.
- **Browse files** opens the Files panel in the project's stack, and so puts the pane on screen (see "Opening a
  project's files" in `docs/product/project-pane.md`); a maximized Terminal panel stays maximized.
- A panel that opens takes an even share of the stack's height — a third of it when it makes three — taken from the
  others in proportion to their heights.

Opening the first panel docks the pane at once, with no animation, and takes its width from the terminal, which is
resized (see "Output, scrollback and size" in `docs/product/session-terminal.md`).

## The panel toggles in the top bar

At the top bar's end there is **one toggle per kind of the current owner's panels**: the Terminal toggle and then
the Files toggle while a project owns the right pane (the reverse of their stacking order), the Report toggle while a
console session does, and none while nothing owns it (see "The top bar" in `docs/product/window-layout.md`).

| Toggle | Icon | Label while its panel is not on screen | Label while it is |
|---|---|---|---|
| Files | a closed folder | Show files panel | Hide files panel |
| Terminal | a terminal window with a prompt | Show terminal panel | Hide terminal panel |
| Report | a document | Show report panel | Hide report panel |

- **The label says what pressing it will do**, and is also its tooltip.
- **The icon stays the same whatever the state.** A toggle whose panel is on screen is drawn with the pressed fill, the
  same fill the current console's tile has on the rail, which it keeps under the pointer and while pressed.
- It tells assistive technology whether its panel is expanded: expanded while the panel is on screen.

A panel counts as **on screen** while it is open, docked in the pane's column or maximized. A panel only floating in
counts as not on screen.

Pressing a toggle:

- **while its panel is not open** opens it, which docks the pane if no panel was open, a floating card included (see
  "Floating a panel in" below); opening the Terminal panel this way also starts a shell when the project has none (see
  "Starting a terminal" in `docs/product/terminal-panel.md`);
- **while its panel is on screen** closes it, a maximized Terminal panel included, and closing the last panel hides
  the pane.

Pressing a panel toggle leaves keyboard focus where it was, as every control in the top bar does (see "Pressing the
chrome's controls" in `docs/product/window-layout.md`).

## Floating a panel in

While the right pane has an owner and none of its panels is open, **resting the mouse on a panel toggle floats that
panel's card in** over the terminal, at any window width; moving onto another toggle shows that toggle's panel
instead. The same holds while the only open panel is a maximized Terminal panel, for the toggle of a panel that is not
open: the card floats in over the maximized one (see "Maximizing the Terminal panel" below). Resting on a toggle
otherwise does nothing. How the floating card enters and leaves, what keeps
it up and how `Escape` sends it away are in "A hidden pane floats in on hover" in `docs/product/window-layout.md`.

- The card is the panel's own card, at the right pane's chosen width (at most 92% of the window's), with the same
  gaps around it as docked; the terminal shows in those gaps and is not resized.
- **Pressing the toggle** opens the panel, docking the pane at once, with no animation; keyboard focus stays where it
  was.
- **Close panel** in a floating card sends it away, opening nothing. A floating card has no Maximize panel.
- Floating a panel in closes the sidebar's drawer (see "Below 1168 px: the sidebar becomes a drawer" in
  `docs/product/window-layout.md`).

## Resizing panels

Between two neighbouring panels in the pane's column, in the gap that separates them, is a **resize handle**, drawn
as a grip like the right pane's width handle (see "Resizing the sidebar and the right pane" in `docs/product/window-layout.md`), with the
same "Resize" tooltip. It moves only the boundary between that pair: the pair keeps its combined height, neither panel
goes under 120 px, and the other panels and the stack's own height stay as they are, so **resizing panels never
resizes the terminal**. Its name says which pair it resizes, naming both panels' kinds, upper one first.

- **Dragging** it moves the boundary.
- **Double-clicking** it gives the pair an even split of their combined height.
- It is reachable with Tab and is then operated from the keyboard: Down makes the upper panel taller and Up shorter,
  by 16 px, by 64 px with Shift held; Home makes the upper panel as short as it can be and End as tall. The handle is
  not mirrored under a right-to-left language.
- Its pointer target is **24 px** tall, centred on the gap, so it reaches 7 px into the cards on either side.
- Pressing it with the mouse does not take keyboard focus off the terminal, and while it is dragged the pointer shows
  the resize cursor wherever it is.

## Maximizing the Terminal panel

Only the Terminal panel can be maximized, from **Maximize panel** in its header (see "A panel's header" above), while
it is open in the pane's column, alone or beside the Files panel.

**Maximized, the Terminal card leaves the column and covers the terminal's area** — the agent terminal's, or what
stands in its place, the archive view included — and nothing else:

- The card is inset **8 px** from every side of that area, the side beside the pane's column excepted, where the
  column's own 8 px gap parts them; the gaps show the terminal's background, and nothing of what it covers shows
  through. The covered area cannot be used, with the pointer or the keyboard, while it is covered.
- **The sidebar keeps its own state**, docked, hidden, floating in or as a drawer, and a floating sidebar or its drawer
  comes up above the maximized card as usual.
- **An open Files panel stays docked in the column**, taking the column's full height, with no resize handle beside
  it; the right pane's width handle stays with the column.
- **With the Files panel closed, the column folds away**: the terminal's area, and the maximized card over it, reach
  the window's end edge, and the pane's width handle goes with the column. Opening or closing the Files panel
  meanwhile — its toggle, Browse files — brings the column back or folds it again, and leaves the Terminal panel
  maximized.
- **Floating Files in**: with the Files panel closed, resting the mouse on its toggle floats the Files card in over the
  maximized Terminal panel, as in "Floating a panel in" above; pressing the toggle docks it in the column beside the
  maximized card.

If keyboard focus was in the terminal's area when Maximize panel is pressed, it moves into the Terminal panel first,
where `F6` would land in it (see "Keyboard focus" in `docs/product/terminal-panel.md`). While the card covers the
terminal's area, keyboard focus that would be handed to the agent terminal — a panel holding focus closing, a floating
pane or the sidebar's drawer going — goes there in the Terminal panel instead.

**Restore panel size** in its header puts the card back in the column, the layout exactly as it was before
maximizing. `Escape` never restores it. The Terminal panel is also restored:

- when it is closed, from its header or its toggle, after which keyboard focus that was in it goes to the agent
  terminal;
- when the right pane passes to a console session or to no owner;
- when an archive view is opened — View all in a project's "View archive" submenu, say (see "The archive view" in
  `docs/product/archive-view.md`) — or arrived at by Back or Forward, so that the view it would cover shows. Going Back
  or Forward anywhere else does not by itself restore it.

It stays maximized when the right pane passes to another project, showing that project's tabs, and when its last tab
is closed, showing its empty state (see "Starting a terminal" in `docs/product/terminal-panel.md`).

Whether the Terminal panel is maximized is not remembered: a reload shows it in the column.

## Switching projects under an open panel

When the right pane passes from one project to another while a Files or Terminal panel is on screen, docked or
floating in, that panel's header and content fade in with a small slide from the right, or from the left in a
right-to-left language; the card keeps its width and position throughout. Only a card already on screen as the
project changes plays it: a panel that docks, floats in or is opened — the pane passing to a project from a console
session or from no owner, as the window starts, say, included — plays none, and nor does a panel opened beside
another after a switch. Moving between sessions of the same project, changing views or refreshing play none either.
The report panel plays none. The system's Reduce motion setting disables it.

## What is remembered

**Per kind of owner, per client**, in that client's own browser storage, as the right pane's width is (see "Resizing
the sidebar and the right pane" in `docs/product/window-layout.md`): which panels are open and each one's share of the
stack's height, so the stack comes back as it was after a reload or a restart. Without that storage they last until
the page is reloaded.

There are two stacks, one for projects and one for console sessions: every project shares the first and every console
session the second, so passing the right pane to another project keeps the open panels, their heights and whether the
Terminal panel is maximized, and passing it to another console session keeps its panels and heights. Until the user changes it, the Files panel is open
and the Terminal panel closed in the projects' stack, and the report panel is open in the console sessions' stack, so
the right pane is shown. A stored stack that cannot be read counts as that default. A right pane the user
had hidden with the earlier pane toggle of Octoboard starts with no panel open in either stack.

## Panels and keyboard focus

Each panel is a **named region** — the Files panel named for the project's files, the Terminal panel for the project's terminals, the report panel
"Report" — and each
is a stop of its own in the `F6` cycle, in stacking order, except that a maximized Terminal panel comes first (see
"Moving focus between regions with F6" in `docs/product/moving-focus-between-regions.md`). A panel only floating in is
not a stop.

**Closing a panel that holds keyboard focus hands focus to the terminal** — or, where the terminal cannot take it,
where it goes instead (see "With no session selected" in `docs/product/session-terminal.md`). So does the right pane
going while its width handle holds focus.
