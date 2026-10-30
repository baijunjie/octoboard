# Right pane panels

The window's right pane (see "What the right pane shows" in `docs/product/window-layout.md`) is a vertical **stack of
panels**. Each **panel** is a card of its own, of one **kind**, and which kinds there are depends on the pane's owner:

| Owner | Kinds |
|---|---|
| A project | **Report**, the **report panel**: the pages the selected project session pushed, only while it has some (see "A project session's Report panel" below and `docs/product/report-panel.md`); **Files**, the **Files panel**: the project's files and its repository's changes (see `docs/product/project-pane.md`); **Processes**, the **Processes panel**: the background processes of the project's sessions and the project's own (see `docs/product/processes-panel.md`); **Terminal**, the **Terminal panel**: the project's shell terminals as tabs (see `docs/product/terminal-panel.md`) |
| A console session | **Report**, the **report panel**: the pages the console session pushed (see `docs/product/report-panel.md`); **Processes**, the **Processes panel**: the console session's background processes (see `docs/product/processes-panel.md`) |

A project's Files, Processes and Terminal panels are the project's own, the same whichever of its sessions is selected;
its Report panel is the selected project session's own.

**The right pane is on screen while at least one of its owner's panels is open**, docked beside the terminal in a
window **782 px** wide or wider; with none open there is no right pane in the row, and its width goes to the terminal.
In a narrower window the pane does not dock: one panel at a time floats over the terminal instead (see "Below 782 px:
one panel floats over the terminal" below). There is no toggle for the pane as a whole: it is shown and hidden by
opening and closing its panels, from the panel toggles in the top bar (see "The panel toggles in the top bar" below)
and from each panel's own header. With no panel open, a panel can still float in over the terminal for a moment (see
"Floating a panel in" below). A maximized Terminal panel leaves the pane's column for the terminal's area (see
"Maximizing the Terminal panel" below).

How wide the right pane is and how it is resized is in "Pane widths at 1168 px and wider" and "Resizing the sidebar
and the right pane" in `docs/product/window-layout.md`.

## The panel cards

The right pane has **no frame or surface of its own**: it is its cards. Each card looks exactly like the sidebar's
floating card (see "A hidden pane floats in on hover" in `docs/product/window-layout.md`): rounded corners, the hairline
edge, the light card shadow and the sidebar's background, docked and floating alike. At 782 px and wider the cards
keep **8 px** from the top bar, from the window's end edge and from its bottom (or the connection banner), **8 px**
from the terminal and **10 px** from each other; the gaps on the terminal's side and between cards hold the resize
grips (see "Resizing the sidebar and the right pane" in `docs/product/window-layout.md` and "Resizing panels" below).
Below 782 px the gaps are in "Below 782 px: one panel floats over the terminal" below. What shows around and between
the cards is the terminal's area, in the terminal's own background, so the cards read as lying on the content. A
single open panel takes the stack's full height.

A panel is never shorter than **120 px**.

## A panel's header

Every panel starts with a **header**, laid out as the sidebar's floating card's header is, with the same inset, title
size and weight, and icon buttons. Header heights and alignment are in "The window chrome and the content panel" in
`docs/product/window-layout.md`. It holds, from its leading side:

- the panel's **title** — the project's name for the Files panel, "Processes" for the Processes panel, "Terminal" for
  the Terminal panel, "Report" for the report panel;
- whatever the kind puts beside its title — the Files panel's Files and Changes tabs (see "Opening a project's files"
  in `docs/product/project-pane.md`), the Terminal panel's tabs (see "Tabs" in `docs/product/terminal-panel.md`), the
  report panel's pager (see "Paging through the history" in `docs/product/report-panel.md`);
- the kind's own icon buttons, at the trailing side;
- in the Terminal panel's header only, **Maximize panel**, or **Restore panel size** while it is maximized, there
  whenever the panel is open in the pane's column, alone or not, and not while it only floats in or floats over the
  terminal below 782 px (see "Maximizing the Terminal panel" below); the other panels have neither;
- **Close panel** (see "Opening and closing panels" below).

The title is at most 160 px wide; a longer one fades out, with its full text as its tooltip while it is cut (see
"Names too long for their space" in `docs/product/labels-and-tooltips.md`). In the Files panel, **Files and Changes
and Close panel stay fully visible**. As space tightens, the project title shrinks, fades and disappears before the
other icon buttons lose room. Those buttons scroll horizontally within their own area, fading at edges with more
beyond them; Close panel stays outside that area. Scrolling and moving keyboard focus through the buttons reveal
them, and the area's fades disappear while a button has keyboard focus so its focus ring stays visible.

In the Terminal panel, the header stays on one row. Its visual title appears only when the window is at least 1168 px
wide and the header has at least 352 px of content width, excluding its horizontal padding. Otherwise the same title
remains available as a level-two heading to assistive technology and gives its space to the tabs. The tabs scroll when
needed, while the buttons at the end keep their width. In the report panel, the pager gives way while the title and
trailing buttons keep their width. Pressing a header button with the mouse leaves keyboard focus where it was.

The pane's 16 px horizontal inset (see "The window chrome and the content panel" in `docs/product/window-layout.md`)
is measured from the card's edge.

## Opening and closing panels

What follows is how panels open and close in a window 782 px wide or wider; below that see "Below 782 px: one panel
floats over the terminal" below.

- **Each kind is in its owner's stack at most once.**
- **Panels are stacked in a fixed order of kinds**, whatever order they were opened in, top to bottom: Report, Files,
  Processes, Terminal.
- **Close panel** takes a panel out of the stack. Its height goes to the panels that remain, in proportion to theirs.
- **Closing the last panel hides the right pane**, at once; the terminal takes its width back.
- **Open panel** in a project's action menu opens the chosen panel in the project's stack, and so puts the pane on
  screen; a maximized Terminal panel stays maximized (see "Opening a panel from a project's action menu" below).
- A panel that opens takes an even share of the stack's height — a third of it when it makes three — taken from the
  others in proportion to their heights.

Opening the first panel docks the pane at once, with no animation, and takes its width from the terminal, which is
resized (see "Output, scrollback and size" in `docs/product/session-terminal.md`).

## The panel toggles in the top bar

At the top bar's end there is **one toggle per kind of the current owner's panels**, in their stacking order: the
Report toggle (only while the selected project session's Report panel is offered, see "A project session's Report
panel" below), the Files, Processes and Terminal toggles while a project owns the right pane, the Report and Processes
toggles while a console session does, and none while nothing owns it (see "The top bar" in
`docs/product/window-layout.md`).

| Toggle | Icon | Label while its panel is not on screen | Label while it is |
|---|---|---|---|
| Report | a document | Show report panel | Hide report panel |
| Files | a closed folder | Show files panel | Hide files panel |
| Processes | a square with a pulse line across it | Show processes panel | Hide processes panel |
| Terminal | a terminal window with a prompt | Show terminal panel | Hide terminal panel |

- **The label says what pressing it will do**, and is also its tooltip.
- **The icon stays the same whatever the state.** A toggle whose panel is on screen is drawn with the pressed fill, the
  same fill the current console's tile has on the rail, which it keeps under the pointer and while pressed.
- It tells assistive technology whether its panel is expanded: expanded while the panel is on screen.

A panel counts as **on screen** while it is open, docked in the pane's column or maximized, or, below 782 px, while it
floats over the terminal. A panel only floating in counts as not on screen.

Pressing a toggle:

- **while its panel is not open** opens it, which docks the pane if no panel was open, a floating card included (see
  "Floating a panel in" below); opening the Terminal panel this way also starts a shell when the project has none (see
  "Starting a terminal" in `docs/product/terminal-panel.md`);
- **while its panel is on screen** closes it, a maximized Terminal panel included, and closing the last panel hides
  the pane.

Below 782 px pressing a toggle floats its panel over the terminal, or closes it while it is the one floating (see
"Below 782 px: one panel floats over the terminal" below).

Pressing a panel toggle leaves keyboard focus where it was, as every control in the top bar does (see "Pressing the
chrome's controls" in `docs/product/window-layout.md`), except that below 782 px a panel it opens takes focus.

## Opening a panel from a project's action menu

**Open panel** is a submenu of a project's action menu — on its row and by right-clicking it (see "Project rows" in
`docs/product/sidebar-project-rows.md`), in its focus mode's header, and on its heading in a console session's focus
mode (see `docs/product/focus-mode.md`). It holds **Files**, then **Processes**, then **Terminal**, each drawn with its
toggle's icon; the Report panel is not offered there. The submenu's own icon, a window with a panel at its side, is
mirrored under a right-to-left language (see "What is mirrored" in `docs/product/right-to-left-layout.md`).

- **An entry is left out while its panel of that project is on screen** (see "The panel toggles in the top bar" above)
  **or floating in** on hover (see "Floating a panel in" below), and offered otherwise — in particular whenever the right
  pane belongs to another owner, or that panel is closed. With every entry left out, the submenu is left out.
- **Choosing an entry** gives the right pane to the project (see "What the right pane shows" in
  `docs/product/window-layout.md`) and opens that panel in the project's stack, as pressing its toggle while the panel
  is not open does: docking the pane at once if no panel was open, with a maximized Terminal panel staying maximized;
  below 782 px it floats the panel over the terminal instead (see "Below 782 px: one panel floats over the terminal"
  below). It selects, starts and resumes no session.
- **Files** shows the Files panel in its Files mode (see "Opening a project's files" in
  `docs/product/project-pane.md`).
- **Processes** lists the processes of all the project's sessions and its project processes, with the session filter
  empty even while a session of the project is selected (see "The session filter" in `docs/product/processes-panel.md`).
- **Terminal** starts a shell only when the project has none, as the toggle does (see "Starting a terminal" in
  `docs/product/terminal-panel.md`).

## Floating a panel in

While the right pane has an owner and none of its panels is open, **resting the mouse on a panel toggle floats that
panel's card in** over the terminal; moving onto another toggle shows that toggle's panel instead. Nothing floats in on
hover in a window narrower than 782 px, where panels open only when pressed (see "Below 782 px: one panel floats over
the terminal" below), nor at any width on a device whose primary input cannot hover, such as a phone or a tablet's touch
screen. The same holds while the only open panel is a maximized Terminal panel, for the toggle of a panel that is not
open: the card floats in over the maximized one (see "Maximizing the Terminal panel" below). Resting on a toggle
otherwise does nothing. How the floating card enters and leaves, what keeps it up and how `Escape` sends it away are in
"A hidden pane floats in on hover" in `docs/product/window-layout.md`.

- The card is the panel's own card, at the right pane's chosen width (at most 92% of the window's), with the same
  gaps around it as docked; the terminal shows in those gaps and is not resized.
- **Pressing the toggle** opens the panel, docking the pane at once, with no animation; keyboard focus stays where it
  was.
- **Close panel** in a floating card sends it away, opening nothing. A floating card has no Maximize panel.
- Floating a panel in closes the sidebar's drawer (see "Below 1168 px: the sidebar becomes a drawer" in
  `docs/product/sidebar-drawer.md`).

## Below 782 px: one panel floats over the terminal

A window narrower than **782 px** — the rail's 48 px, the terminal's 398 px floor and the right pane's 336 px minimum
with the Terminal panel side by side — cannot dock the right pane (see "Pane widths below 1168 px" in
`docs/product/window-layout.md`). There the right pane's open panel **floats over the terminal**, the agent
terminal taking the content panel's whole width under it. The threshold is the same whichever panels are open.

- **One panel at a time.** Opening a panel — its toggle, Open panel in a project's action menu (see "Opening a panel
  from a project's action menu" above), a folder row's Terminal (see "Starting a terminal" in
  `docs/product/terminal-panel.md`) — closes the one floating before it. The toggle of the floating panel is drawn
  pressed and reads "Hide …"; every other toggle reads "Show …". Pressing the floating panel's toggle closes it.
  Opening the Terminal panel on a project with no shell starts one, as it does docked (see "Starting a terminal" in
  `docs/product/terminal-panel.md`).
- **How it is drawn**: the panel's own card at the right pane's chosen width, capped to what the content panel leaves
  beside a **dimmed strip** of the terminal, with a **4 px** gap all round the card, from the top bar, the window's
  end edge, its bottom (or the connection banner) and the strip. It has no width handle and no Maximize panel. The
  strip is on the side away from the window's end, reaching the window's start edge, the rail not being in the row
  there (see "Below 782 px: the rail comes in with the drawer" in `docs/product/sidebar-drawer.md`); it is **48 px**
  wide, or **24 px** when the window — for the website's embedded preview, the preview frame — is 360 px wide or less.
- **How it comes and goes**: the card fades in while sliding in from the window's end edge by **40 px**, and fades
  out while sliding back towards it, over 240 ms with the easing of a pane floating in on hover (see "A hidden pane
  floats in on hover" in `docs/product/window-layout.md`); under a right-to-left language the end edge is the window's
  left. Opening another panel in its place plays the entrance again for the new panel. The dimmed strip fades in and
  out with the card over the same 240 ms. While they fade out, neither the card nor the strip takes presses, and the
  card takes no keyboard focus. Where the system asks for reduced motion
  both appear and go at once.
- **Opening or closing it never resizes the terminal**, and the terminal has no minimum width here, so the row never
  scrolls sideways.
- **It closes** on Close panel in its header, on its toggle, on a press on the dimmed strip or in the card's 4 px
  gap, on a touch swipe (see `docs/product/touch-swipes.md`), and on `Escape`, which does not reach the running
  agent. In a field holding text `Escape` only clears the field first, and inside a shell terminal of the Terminal panel it goes to the shell and leaves the panel open.
- **While it is shown the agent terminal under it cannot be used**, with the pointer or the keyboard, and is not an
  `F6` stop (see "Moving focus between regions with F6" in `docs/product/moving-focus-between-regions.md`). Opening a
  panel moves keyboard focus into it, where `F6` would land in it — opened by a tap, where no on-screen keyboard is
  raised instead (see "Focus after a tap" in `docs/product/moving-focus-between-regions.md`). Closing it while it
  holds focus, or the window crossing 782 px with focus left on nothing, hands focus to the agent terminal, or to the
  top bar's first enabled control where the terminal takes none: no session selected and no creation action in its
  place, or after a tap.
- **The sidebar's drawer and a panel floating over the terminal are never on screen together.** Opening the drawer
  closes the floating panel, for every kind of owner, and no panel floats while the drawer is open. Selecting a session
  in the drawer closes the drawer, showing the session's terminal with no panel over it. Opening a panel closes the
  drawer. Opening an archive view closes the floating panel too, so the view it would cover shows (see "The archive
  view" in `docs/product/archive-view.md`).
- **The stacks are left as they are.** Nothing done below 782 px opens, closes, resizes or restores a panel in the
  remembered stacks (see "What is remembered" below). Widening the window to 782 px or more shows the stack exactly
  as it was, a maximized Terminal panel included. Narrowing the window below 782 px, a load included, floats the
  panel last on screen while it is still open in the stack, otherwise the first open panel in stacking order, and
  nothing when none is open; with the drawer open it floats nothing. Each kind of owner keeps its own floating panel,
  as it keeps its own stack.
- **The panel last on screen** is kept per kind of owner, in this window only: the panel last opened, docked or
  floating, or the panel last floating over the terminal when it was closed or opening the drawer closed it. Besides
  narrowing the window, a touch swipe brings it back (see `docs/product/touch-swipes.md`).

## Resizing panels

Between two neighbouring panels in the pane's column, in the gap that separates them, is a **resize handle**, drawn as a
grip like the right pane's width handle (see "Resizing the sidebar and the right pane" in
`docs/product/window-layout.md`), with the same "Resize" tooltip. It moves only the boundary between that pair: the pair
keeps its combined height, neither panel goes under 120 px, and the other panels and the stack's own height stay as they
are, so **resizing panels never resizes the terminal**. Its name says which pair it resizes, naming both panels' kinds,
upper one first.

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
it is open in the pane's column, alone or beside other panels.

**Maximized, the Terminal card leaves the column and covers the terminal's area** — the agent terminal's, or what
stands in its place, the archive view included — and nothing else:

- The card is inset **8 px** from every side of that area, the side beside the pane's column excepted, where the
  column's own 8 px gap parts them; the gaps show the terminal's background, and nothing of what it covers shows
  through. The covered area cannot be used, with the pointer or the keyboard, while it is covered.
- **The sidebar keeps its own state**, docked, hidden, floating in or as a drawer, and a floating sidebar or its drawer
  comes up above the maximized card as usual.
- **The other open panels stay docked in the column** — Report, Files, Processes — sharing the column's full height; the
  right pane's width handle stays with the column.
- **With no other panel open, the column folds away**: the terminal's area, and the maximized card over it, reach the
  window's end edge, and the pane's width handle goes with the column. Opening or closing another panel meanwhile — its
  toggle, Open panel in a project's action menu — brings the column back or folds it again, and leaves the Terminal
  panel maximized.
- **Floating another panel in**: with the column folded away, resting the mouse on another panel's toggle floats its
  card in over the maximized Terminal panel, as in "Floating a panel in" above; pressing the toggle docks it in the
  column beside the maximized card.

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

## A project session's Report panel

While a project owns the right pane and one of its project sessions is selected, the project's stack can carry that
session's **Report panel**, showing the pages it pushed (see `docs/product/report-panel.md`); only a project coordinator
pushes pages. With no project session of that project selected, the stack has no Report panel.

- **It is offered only while the session has pages**, or while listing them failed, so that its failure card can say
  why and offer Try again (see "Paging through the history" in `docs/product/report-panel.md`). Its pages are listed
  when the session is selected; until that listing answers, a session not known to have pages has neither the panel nor
  the Report toggle. When the window reloads its state from the daemon, the last answer for the session stands until
  the new listing answers, so the panel and its toggle do not go and come back.
- While it is not offered, it has no toggle in the top bar and is not in the stack: the Files, Processes and Terminal
  toggles are the only ones, and the pane is on screen only while one of those panels is open.
- **A page pushed** to a session with none brings the panel and its toggle in, the panel open unless the user closed it
  for that session (see "What is remembered" below).
- **When the session's last page is deleted** (see "Withdrawing pages" in `docs/product/report-panel.md`), the panel
  and its toggle go at once, docked or floating, and the pane goes too when that was its only open panel. A card the
  Report toggle had floated in on hover goes with them. Keyboard focus held in the panel or on the toggle is handed to
  the agent terminal, as when a panel holding focus closes (see "Panels and keyboard focus" below).
- **Below 782 px**, a Report panel floating over the terminal that went because its pages were deleted does not float
  back by itself when a page is pushed later: only its toggle comes back, and pressing it floats the panel again.

## Switching projects under an open panel

When the right pane passes from one project to another while a Files or Terminal panel is on screen, docked or
floating in, that panel's header and content fade in where they stand, with no sideways movement in either reading
direction; the card keeps its width and position throughout. Only a card already on screen as the
project changes plays it: a panel that docks, floats in or is opened — the pane passing to a project from a console
session or from no owner, as the window starts, say, included — plays none, and nor does a panel opened beside
another after a switch. Moving between sessions of the same project, changing views or refreshing play none either.
The report and Processes panels play none. The system's Reduce motion setting disables it.

## What is remembered

**Per kind of owner, per client**, in that client's own browser storage, as the right pane's width is (see "Resizing
the sidebar and the right pane" in `docs/product/window-layout.md`): which panels are open and each one's share of the
stack's height, so the stack comes back as it was after a reload or a restart. Without that storage they last until
the page is reloaded.

There are two stacks, one for projects and one for console sessions: every project shares the first and every console
session the second, so passing the right pane to another project keeps the open panels, their heights and whether the
Terminal panel is maximized, and passing it to another console session keeps its panels and heights. Until the user
changes it, the Files panel is open and the Processes and Terminal panels closed in the projects' stack, and the report
panel is open and the Processes panel closed in the console sessions' stack, so the right pane is shown. A stored stack
that cannot be read counts as that default. Which panel floats over the terminal below 782 px is not part of the stacks
and is not remembered. A right pane the user had hidden with the earlier pane toggle of Octoboard starts with no panel
open in either stack.

**Whether a project session's Report panel is open is kept per session**, not in the projects' stack: it is open by
default, and closing it is remembered for that session alone, so another session of the project still has its own
open. The window keeps the closes of the 200 sessions closed most recently; a session dropped from that list has its
Report panel open again. They are kept in the same browser storage as the stacks.

## Panels and keyboard focus

Each panel is a **named region** — the Files panel named for the project's files, the Processes panel "Processes", the
Terminal panel for the project's terminals, the report panel "Report" — and each is a stop of its own in the `F6` cycle,
in stacking order (Report, Files, Processes, Terminal), except that a maximized Terminal panel comes first (see "Moving
focus between regions with F6" in `docs/product/moving-focus-between-regions.md`). A panel only floating in is not a
stop; a panel floating over the terminal below 782 px is.

**Closing a panel that holds keyboard focus hands focus to the terminal** — or, where the terminal cannot take it,
where it goes instead (see "With no session selected" in `docs/product/session-terminal.md`). So does the right pane
going while its width handle holds focus, a project session's Report panel going with its last page while it holds
focus, and a panel toggle going from the top bar while it holds focus (see "A project session's Report panel" above).
