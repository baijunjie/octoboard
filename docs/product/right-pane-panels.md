# Right pane panels

The window's right pane (see "What the right pane shows" in `docs/product/window-layout.md`) is a **stack of panels**
arranged in columns of at most two cards. Each **panel** is a card of its own, of one **kind**, and which kinds there
are depends on the pane's owner:

| Owner | Kinds |
|---|---|
| A project | **Report**, the **report panel**: the pages the selected project session pushed, only while it has some (see "A project session's Report panel" below and `docs/product/report-panel.md`); **Terminal**, the **Terminal panel**: the project's shell terminals as tabs (see `docs/product/terminal-panel.md`); **Files**, the **Files panel**: the project's files and its repository's changes (see `docs/product/project-pane.md`); **Processes**, the **Processes panel**: the background processes of the project's sessions and the project's own (see `docs/product/processes-panel.md`) |
| A console session | **Report**, the **report panel**: the pages the console session pushed (see `docs/product/report-panel.md`); **Processes**, the **Processes panel**: the console session's background processes (see `docs/product/processes-panel.md`) |

A project's Terminal, Files and Processes panels are the project's own, the same whichever of its sessions is selected;
its Report panel is the selected project session's own.

**Above 768 px the right pane docks beside the terminal by default when there is room.** Its separate top bar toggle
switches all open cards between docked and floating as a group when docking is available. With insufficient room the
group floats automatically without changing the stored choice or taking width from the terminal (see "Docking capacity"
in `docs/product/pane-docking.md`). At **768 px and below**, one panel floats at a time (see "At 768 px and below: one
panel floats over the terminal" below).

The panel toggles and each card's Close panel control decide which cards are open. With none open the pane takes no
width, and hover can preview one card without opening it. A docked Report or Terminal panel can be maximized over
the terminal's area (see "Maximizing a panel" below).

How wide the right pane is and how it is resized is in "Pane widths" and "Resizing the sidebar and the right pane"
in `docs/product/pane-docking.md`.

## The panel cards

The right pane has **no frame or surface of its own**: it is its cards. Each card looks exactly like the sidebar's
floating card (see "A hidden pane floats in on hover" in `docs/product/pane-docking.md`): rounded corners, the hairline
edge, the light card shadow and the sidebar's background, docked and floating alike. Above 768 px the cards keep **8
px** from the top bar, from the window's end edge and from its bottom (or the connection banner), **8 px** from the
terminal and **10 px** from each other; the gaps on the terminal's side and between cards hold the resize grips (see
"Resizing the sidebar and the right pane" in `docs/product/pane-docking.md` and "Resizing panels" below). At 768 px and
below the gaps are in "At 768 px and below: one panel floats over the terminal" below. What shows around and between the
cards is the terminal's area, in the terminal's own background, so the cards read as lying on the content. A single
panel in a column takes that column's full height. Columns divide the pane's chosen total width, with a 10 px gap
between them; their widths start equal and can be adjusted in that gap (see "Resizing panels" below). Changing their
proportion does not change the pane's total width.

A panel is never shorter than **120 px**.

## A panel's header

Every panel starts with a **header**, laid out as the sidebar's floating card's header is, with the same inset, title
size and weight, and icon buttons. Header heights and alignment are in "The window chrome and the content panel" in
`docs/product/window-layout.md`. It holds, from its leading side:

- the panel's **title** — the project's name for the Files panel, "Processes" for the Processes panel, "Terminal" for
  the Terminal panel, "Report" for the report panel;
- whatever the kind puts beside its title — the Files panel's Files and Changes tabs (see "Opening a project's files" in
  `docs/product/project-pane.md`), the report panel's pager (see "Paging through the history" in
  `docs/product/report-panel.md`);
- the kind's own icon buttons, at the trailing side;
- in the Report and Terminal panels' headers, **Maximize panel**, or **Restore panel size** while it is maximized, there
  whenever the panel is docked, alone or not, and not while the whole group floats, while it is a hover preview, or
  while it floats alone at 768 px and below (see "Maximizing a panel" below); Files and Processes have neither;
- **Close panel** (see "Opening and closing panels" below).

The title is at most 160 px wide; a longer one fades out, with its full text as its tooltip while it is cut (see
"Names too long for their space" in `docs/product/labels-and-tooltips.md`). In the Files panel, **Files and Changes
and Close panel stay fully visible**. As space tightens, the project title shrinks, fades and disappears before the
other icon buttons lose room. Those buttons scroll horizontally within their own area, fading at edges with more
beyond them; Close panel stays outside that area. Scrolling and moving keyboard focus through the buttons reveal
them, and the area's fades disappear while a button has keyboard focus so its focus ring stays visible.

In the Terminal panel, the header holds its title, with only Maximize panel or Restore panel size when offered and
Close panel on the same row. Its tabs, New terminal and the shown terminal's actions menu occupy a separate row below
the header (see "Tabs" in `docs/product/terminal-panel.md`). In the report panel, with one column at least 320 px wide
or while maximized, the pager gives way while the title and trailing buttons keep their width.

With two columns, or a single-column card area narrower than **320 px**, the cards' titles can shrink and fade so
their headers fit the narrower cards. The report pager and the kind's own icon buttons scroll horizontally in
separate areas when needed. Maximize panel / Restore panel size and Close panel keep their width outside those
scrolling areas. Scrolling or moving keyboard focus reveals controls beyond the edges; the fades disappear while a
control in that area has keyboard focus. The Files panel keeps the Files and Changes tabs fully visible as
described above. The Terminal panel keeps its tabs on their separate row.

Pressing a header button with the mouse leaves keyboard focus where it was.

The pane's 16 px horizontal inset (see "The window chrome and the content panel" in `docs/product/window-layout.md`)
is measured from the card's edge.

## Opening and closing panels

What follows is how panels open and close in a window wider than 768 px; at 768 px and below see "At 768 px and below:
one panel floats over the terminal" below.

- **Each kind is in its owner's stack at most once.**
- **Panels are arranged in a fixed order of kinds**, whatever order they were opened in: Report, Terminal, Files,
  Processes. Open panels take places in the docked or floating group columns; closing or maximizing a panel repacks
  the remaining ones in this order using the layout rules below.
- **Close panel** takes a panel out of the stack. Each remaining pair shares its column's height in proportion to its
  height weights; a panel alone fills its column.
- **Closing the last panel hides the right pane**, at once; when it was docked, the terminal takes its width back.
- **Open panel** in a project's action menu opens the chosen panel in the project's stack, and so puts the pane on
  screen; a maximized panel stays maximized (see "Opening a panel from a project's action menu" below).
- An opening panel receives an equal height weight among the open panels; the others keep their relative weights. Each
  column divides its own height using the weights of the panels in that column.

The group layout depends on how many panels remain and the width available for their cards:

- **One panel** fills the pane's height.
- **Two panels** sit side by side, each full height, when the pane's card area has room for two **260 px** columns and
  their **10 px** gap: **530 px** in total. Below that width they stack top to bottom in one column. This is the card
  area's width after the pane's outer gaps, not the window's width.
- **Three panels** use two columns: the first panel fills the leading column's height, and the other two stack top to
  bottom in the trailing column.
- **Four panels** use two columns of two: Report above Terminal in the leading column, Files above Processes in the
  trailing column.

With three or four cards the pane is at least **546 px** wide, including two 260 px columns, the 10 px column gap
and two 8 px outer gaps. When opening or closing panels adds or removes a column, the pane adjusts its total width
to keep the surviving cards at their displayed column width, within the pane's current minimum and maximum (see
"Resizing the sidebar and the right pane" in `docs/product/pane-docking.md`). In particular, opening a third panel
beside two stacked cards adds width for the new column; closing it removes that width. If two cards were already
side by side, opening and closing the third keeps their two-column layout and the pane's total width. If cards from
unequal-width columns combine into one column, the first surviving kind in the fixed order determines the width to
keep.

Closing one of two side-by-side cards keeps the surviving card's width; reopening the same card restores the pair's
previous total width and column widths, within the current limits. Two stacked cards return stacked instead. The
pair's width is no longer restored after manually resizing or resetting the pane, a change to the remembered width
preference, closing all cards or changing the owner.

These automatic adjustments do not replace the remembered total-width preference. Resizing the window can temporarily
limit the adjusted width; widening it restores that width as space permits. Manually resizing or resetting the pane's
total width replaces the automatic adjustment, and reloading uses the remembered preference. Maximizing or restoring a
panel, or opening and closing panels while one is maximized, does not make these automatic column-width adjustments.

The leading column is on the left in a left-to-right language and on the right in a right-to-left language. Panels
alone in a column take its full height; a pair divides its column using its height weights. Resizing the pane can
switch two panels between side by side and stacked without restarting their content or discarding their weights.

In docked mode, opening the first panel shows the pane at once and takes its width from the terminal, which is
resized (see "Output, scrollback and size" in `docs/product/session-terminal.md`).

## The panel toggles in the top bar

At the top bar's end there is **one toggle per kind of the current owner's panels**, in their stacking order: the
Report toggle (only while the selected project session's Report panel is offered, see "A project session's Report
panel" below), the Terminal, Files and Processes toggles while a project owns the right pane, the Report and Processes
toggles while a console session does, and none while nothing owns it (see "The top bar" in
`docs/product/window-layout.md`).

| Toggle | Icon | Label while its panel is not on screen | Label while it is |
|---|---|---|---|
| Report | a document | Show report panel | Hide report panel |
| Terminal | a terminal window with a prompt | Show terminal panel | Hide terminal panel |
| Files | a closed folder | Show files panel | Hide files panel |
| Processes | a square with a pulse line across it | Show processes panel | Hide processes panel |

- **The label says what pressing it will do**, and is also its tooltip.
- **The icon stays the same whatever the state.** A toggle whose panel is on screen is drawn with the pressed fill, the
  same fill the current console's tile has on the rail, which it keeps under the pointer and while pressed.
- It tells assistive technology whether its panel is expanded: expanded while the panel is on screen.

A panel counts as **on screen** while docked, maximized, in a revealed floating group, or floating alone at 768 px and
below. Cards in a hidden floating group are not on screen. A single-card hover preview is not an open panel.

While docked, pressing a toggle:

- **while its panel is not open** opens it, which docks the pane if no panel was open, a floating card included (see
  "Floating a panel in" below); opening the Terminal panel this way also starts a shell when the project has none (see
  "Starting a terminal" in `docs/product/terminal-panel.md`);
- **while its panel is on screen** closes it, a maximized panel included, and closing the last panel hides the pane.

While the whole group floats, pressing a panel toggle opens a closed card and reveals the group. **If the group is
already revealed and that card is open, pressing its toggle closes that card**; the other cards stay revealed, or
closing the last card hides the group. If the group is hidden but the card is still open in its remembered stack,
activating its toggle only reveals the group; a further activation while revealed closes the card. This also applies
to keyboard activation. The toggle's label and tooltip offer to hide a card while it is visible and to show it
otherwise; its pressed and expanded states follow that same visibility. Close panel in the card also closes it.
Leaving or Escape hides the floating group without closing its cards.

At 768 px and below pressing a toggle floats its panel over the terminal, or closes it while it is the one floating (see
"At 768 px and below: one panel floats over the terminal" below).

Pressing a panel toggle leaves keyboard focus where it was, as every control in the top bar does (see "Pressing the
chrome's controls" in `docs/product/window-layout.md`), except that at 768 px and below a panel it opens takes focus.

## Opening a panel from a project's action menu

**Open panel** is a submenu of a project's action menu — on its row and by right-clicking it (see "Project rows" in
`docs/product/sidebar-project-rows.md`), in its focus mode's header, and on its heading in a console session's focus
mode (see `docs/product/focus-mode.md`). It holds **Terminal**, then **Files**, then **Processes**, each drawn with its
toggle's icon; the Report panel is not offered there. The submenu's own icon, a window with a panel at its side, is
mirrored under a right-to-left language (see "What is mirrored" in `docs/product/right-to-left-layout.md`).

- **An entry is left out while its panel of that project is on screen** (see "The panel toggles in the top bar" above)
  **or floating in** on hover (see "Floating a panel in" below), and offered otherwise — in particular whenever the
  right pane belongs to another owner, or that panel is closed. With every entry left out, the submenu is left out.
- **Choosing an entry** gives the right pane to the project (see "What the right pane shows" in
  `docs/product/window-layout.md`) and opens that panel in the project's stack, as pressing its toggle while the panel
  is not open does: showing the docked pane or revealing its floating group, with a docked maximized panel staying
  maximized; at 768 px and below it floats the panel over the terminal instead (see "At 768 px and below: one panel
  floats over the terminal" below). It selects, starts and resumes no session.
- **Files** shows the Files panel in its Files mode (see "Opening a project's files" in `docs/product/project-pane.md`).
- **Processes** lists the processes of all the project's sessions and its project processes, with the session filter
  empty even while a session of the project is selected (see "The session filter" in `docs/product/processes-panel.md`).
- **Terminal** starts a shell only when the project has none, as the toggle does (see "Starting a terminal" in
  `docs/product/terminal-panel.md`).

## Floating a panel in

Above **768 px**, the right pane has a group toggle before its individual panel toggles in the top bar.

- **When the right group can dock**, the toggle switches between docking the whole group and floating it over the
  terminal. The choice is remembered per client and independently of which cards are open. Floating immediately reveals
  the group; docking keeps the open cards and returns their width to the row. Its icon distinguishes docked and
  floating, and its label describes the action it will take.
- **When the right group cannot dock**, it floats automatically. The group toggle only reveals it, including when
  pressed repeatedly, and never changes the docking preference. Its label offers to show the panels.
- **Hover entry and dismissal** follow "A hidden pane floats in on hover" in `docs/product/pane-docking.md`: the group
  icon and individual panel icons reveal immediately; the 48 px end-edge strip reveals after 200 ms. No hover entry
  operates at 768 px and below or on a device whose primary input cannot hover.
- **All open cards float together**, using their usual columns, gaps and height proportions, with no dimmed layer and no
  change to the agent terminal's size. Their contents stay alive while the group is hidden. The total-width handle,
  column-width handle and the height handles between stacked cards remain usable. A resize drag keeps the group visible,
  and the total-width handle follows the same viewport cap as the group. Floating cards offer no Maximize panel control;
  a remembered maximization is shown restored while the group floats and returns on docking.
- **With no open cards**, hover previews the hovered panel, or an available panel from the group icon or edge. Moving
  between panel icons changes that preview. With the group docked but its column empty or folded under a maximized
  panel, an individual panel icon can still preview that panel over the terminal. Pressing its toggle opens it normally.
  A preview has no Maximize panel control, and Close panel dismisses it. An empty docked column previews without a
  total-width handle; a floating group keeps its total-width handle even while previewing one card.
- **Closing the last open card** dismisses the floating group. It does not leave a preview behind or immediately reopen
  one. Revealing one side hides the other floating side.

## At 768 px and below: one panel floats over the terminal

At **768 px and below**, including a portrait iPad at exactly 768 px, the right pane's open panel **floats over the
terminal**, the agent terminal taking the content panel's whole width under it. The boundary is the same whichever
panels are open. Above it the panes dock or float as a group according to their space needs (see "Docking capacity" in
`docs/product/pane-docking.md`).

- **One panel at a time.** Opening a panel — its toggle, Open panel in a project's action menu (see "Opening a panel
  from a project's action menu" above), a folder row's Terminal (see "Starting a terminal" in
  `docs/product/terminal-panel.md`) — closes the one floating before it. The toggle of the floating panel is drawn
  pressed and reads "Hide …"; every other toggle reads "Show …". Pressing the floating panel's toggle closes it. Opening
  the Terminal panel on a project with no shell starts one, as it does docked (see "Starting a terminal" in
  `docs/product/terminal-panel.md`).
- **How it is drawn**: the panel's own card at the right pane's chosen width, capped to what the content panel leaves
  beside a **dimmed strip** of the terminal, with a **4 px** gap all round the card, from the top bar, the window's end
  edge, its bottom (or the connection banner) and the strip. It has no width handle and no Maximize panel. The strip is
  on the side away from the window's end, reaching the window's start edge, the rail not being in the row there (see "At
  768 px and below: the rail comes in with the sidebar" in `docs/product/sidebar-drawer.md`); it is **48 px** wide, or
  **24 px** when the window — for the website's embedded preview, the preview frame — is narrower than 390 px.
- **How it comes and goes**: the card fades in while sliding in from the window's end edge by **40 px**, and fades out
  while sliding back towards it, over 240 ms with the easing of a pane floating in on hover (see "A hidden pane floats
  in on hover" in `docs/product/pane-docking.md`); under a right-to-left language the end edge is the window's left.
  Opening another panel in its place plays the entrance again for the new panel. The dimmed strip fades in and out with
  the card over the same 240 ms. While they fade out, neither the card nor the strip takes presses, and the card takes
  no keyboard focus. Where the system asks for reduced motion both appear and go at once.
- **Opening or closing it never resizes the terminal**, and the terminal has no minimum width here, so the row never
  scrolls sideways.
- **It closes** on Close panel in its header, on its toggle, on a press on the dimmed strip or in the card's 4 px gap,
  on a touch swipe (see `docs/product/touch-swipes.md`), and on `Escape`, which does not reach the running agent. In a
  field holding text `Escape` only clears the field first, and inside a shell terminal of the Terminal panel it goes to
  the shell and leaves the panel open.
- **While it is shown the agent terminal under it cannot be used**, with the pointer or the keyboard, and is not an `F6`
  stop (see "Moving focus between regions with F6" in `docs/product/moving-focus-between-regions.md`). Opening a panel
  moves keyboard focus into it, where `F6` would land in it — opened by a tap, where no on-screen keyboard is raised
  instead (see "Focus after a tap" in `docs/product/moving-focus-between-regions.md`). Closing it while it holds focus,
  or the window crossing 768 px with focus left on nothing, hands focus to the agent terminal, or to the top bar's first
  enabled control where the terminal takes none: no session selected and no creation action in its place, or after a
  tap.
- **The floating sidebar and a panel floating over the terminal are never on screen together.** Opening the sidebar
  overlay closes the floating panel, for every kind of owner, and no panel floats while the sidebar overlay is open.
  Selecting a session in the sidebar overlay closes the sidebar overlay, showing the session's terminal with no panel
  over it. Opening a panel closes the sidebar overlay. Opening an archive view closes the floating panel too, so the
  view it would cover shows (see "The archive view" in `docs/product/archive-view.md`).
- **The stacks are left as they are.** Nothing done at 768 px and below opens, closes, resizes or restores a panel in
  the remembered stacks (see "What is remembered" below). Widening the window to more than 768 px shows the stack
  exactly as it was; a remembered maximization is used again when the pane docks. Narrowing the window to 768 px or
  less, a load included, floats the panel last on screen while it is still open in the stack, otherwise the first open
  panel in stacking order, and nothing when none is open; with the sidebar overlay open it floats nothing. Each kind of
  owner keeps its own floating panel, as it keeps its own stack.
- **The panel last on screen** is kept per kind of owner, in this window only: the panel last opened, docked or
  floating, or the panel last floating over the terminal when it was closed or opening the sidebar overlay closed it.
  Besides narrowing the window, a touch swipe brings it back (see `docs/product/touch-swipes.md`).

## Resizing panels

**Column widths.** Whenever a docked or floating group has two columns, a full-height resize handle sits in their gap,
including while another panel is maximized. It changes the columns' relative widths within the pane, leaving the pane's
total width and the agent terminal's size unchanged. Its accessible name identifies it as resizing the panel columns,
and its hover tooltip says "Resize".

- **Dragging** moves the boundary between the columns.
- **Double-clicking** restores equal widths.
- It is reachable with Tab. Left and Right move the boundary in their physical directions, by **16 px**, or **64 px**
  with Shift held. Home makes the leading column as narrow as possible; End makes it as wide as possible. The leading
  column is on the left in a left-to-right language and on the right in a right-to-left language.
- Each column keeps at least **260 px**, measured after subtracting the gap from the available card width. Shrinking
  the group clamps the displayed widths without changing the chosen proportion; widening it restores that proportion
  when there is room.
- Its pointer target is **24 px** wide, centred on the gap. Pressing it with the mouse leaves keyboard focus where it
  was, and the resize cursor stays visible throughout the drag.

The chosen proportion starts at **50:50**. It is kept when the layout temporarily has one column, when all panels
close, and when a panel is maximized or restored. How the choice is remembered across sessions is in "What is
remembered" below.

**Panel heights.** Between the two panels sharing a column, in the gap that separates them, is a **resize handle**,
drawn as a grip like the right pane's width handle (see "Resizing the sidebar and the right pane" in
`docs/product/pane-docking.md`), with the same "Resize" tooltip. It moves only the boundary between that pair: the pair
keeps its combined height, neither panel goes under 120 px, and the other panels and the stack's own height stay as they
are, so **resizing panels never resizes the terminal**. Its name says which pair it resizes, naming both panels' kinds,
upper one first. Each column's pair is resized independently, whether the group is docked or floating. No panel-height
resize handle is shown while a panel is maximized; restoring it brings those handles back.

- **Dragging** it moves the boundary.
- **Double-clicking** it gives the pair an even split of their combined height.
- It is reachable with Tab and is then operated from the keyboard: Down makes the upper panel taller and Up shorter,
  by 16 px, by 64 px with Shift held; Home makes the upper panel as short as it can be and End as tall. The handle is
  not mirrored under a right-to-left language.
- Its pointer target is **24 px** tall, centred on the gap, so it reaches 7 px into the cards on either side.
- Pressing it with the mouse does not take keyboard focus off the terminal, and while it is dragged the pointer shows
  the resize cursor wherever it is.

## Maximizing a panel

The Report and Terminal panels can be maximized, from **Maximize panel** in their headers (see "A panel's header"
above), while docked, alone or beside other panels. Only one panel is maximized at a time; maximizing the other puts its
predecessor back in the docked layout.

**Maximized, the card leaves the docked layout and covers the terminal's area** — the agent terminal's, or what stands
in its place, the archive view included — and nothing else:

- The card is inset **8 px** from every side of that area, the side beside the docked panels excepted, where the pane's
  own 8 px gap parts them; the gaps show the terminal's background, and nothing of what it covers shows through. The
  covered area cannot be used, with the pointer or the keyboard, while it is covered.
- **The sidebar keeps its own state**, docked, hidden or floating. Its floating card comes up above the maximized
  card as usual.
- **The other open panels stay docked**, repacked using the remaining docked count and available width (see "Opening and
  closing panels" above), with their height weights and chosen column proportion preserved. Their panel-height resize
  handles are hidden until the maximized panel is restored; the column-width handle remains while two columns are
  docked. The right pane's width handle stays with the docked panels.
- **With no other panel open, the docked pane folds away**: the terminal's area, and the maximized card over it, reach
  the window's end edge, and the pane's width handle goes with it. Opening or closing another panel meanwhile — its
  toggle, Open panel in a project's action menu — brings the docked pane back or folds it again, and leaves the
  maximized panel maximized.
- **Floating another panel in**: with the docked pane folded away, resting the mouse on another panel's toggle floats
  its card in over the maximized panel, as in "Floating a panel in" above; pressing the toggle docks it beside the
  maximized card.

If keyboard focus was in the terminal's area when Maximize panel is pressed, it moves into the maximized panel first,
where `F6` would land in it (see "Moving focus between regions with F6" in
`docs/product/moving-focus-between-regions.md`). While the card covers the terminal's area, keyboard focus that would be
handed to the agent terminal — a panel holding focus closing, a floating pane or the floating sidebar going — goes there
in the maximized panel instead.

**Restore panel size** in its header puts the card back in the docked layout, with the height weights preserved.
Maximizing or restoring does not restart its content: a report page keeps its form input and a terminal keeps its
connections. `Escape` never restores it. The panel is also restored:

- when it is closed, from its header or its toggle, after which keyboard focus that was in it goes to the agent
  terminal;
- when the right pane passes to a different kind of owner (project to console session or the reverse), or to no owner;
- when an archive view is opened — View all in a project's "View archive" submenu, say (see "The archive view" in
  `docs/product/archive-view.md`) — or arrived at by Back or Forward, so that the view it would cover shows. Going Back
  or Forward anywhere else does not by itself restore it.

Maximization stays in effect between owners of the same kind: a maximized Terminal panel shows the new project's tabs,
and a maximized Report panel shows the newly selected session's pages while its Report panel is available and open. A
Terminal panel also stays maximized when its last tab is closed, showing its empty state (see "Starting a terminal" in
`docs/product/terminal-panel.md`).

Which panel is maximized is not remembered: a reload shows the panels docked.

## A project session's Report panel

While a project owns the right pane and one of its project sessions is selected, the project's stack can carry that
session's **Report panel**, showing the pages it pushed (see `docs/product/report-panel.md`); only a project coordinator
pushes pages. With no project session of that project selected, the stack has no Report panel.

- **It is offered only while the session has pages**, or while listing them failed, so that its failure card can say why
  and offer Try again (see "Paging through the history" in `docs/product/report-panel.md`). Its pages are listed when
  the session is selected; until that listing answers, a session not known to have pages has neither the panel nor the
  Report toggle. When the window reloads its state from the daemon, the last answer for the session stands until the new
  listing answers, so the panel and its toggle do not go and come back.
- While it is not offered, it has no toggle in the top bar and is not in the stack: the Terminal, Files and Processes
  toggles are the only ones, and the pane is on screen only while one of those panels is open.
- **A page pushed** to a session with none brings the panel and its toggle in, the panel open unless the user closed it
  for that session (see "What is remembered" below).
- **When the session's last page is deleted** (see "Withdrawing pages" in `docs/product/report-panel.md`), the panel and
  its toggle go at once, docked or floating, and the pane goes too when that was its only open panel. A card the Report
  toggle had floated in on hover goes with them. Keyboard focus held in the panel or on the toggle is handed to the
  agent terminal, as when a panel holding focus closes (see "Panels and keyboard focus" below).
- **At 768 px and below**, a Report panel floating over the terminal that went because its pages were deleted does not
  float back by itself when a page is pushed later: only its toggle comes back, and pressing it floats the panel again.

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
the sidebar and the right pane" in `docs/product/pane-docking.md`): which panels are open, each one's height weight
within its column, and the chosen column-width proportion, so the stack comes back as it was after a reload or a
restart. Without that storage they last until the page is reloaded.

There are two stacks, one for projects and one for console sessions: every project shares the first and every console
session the second, so passing the right pane to another project keeps the open panels, their heights and which panel is
maximized, and passing it to another console session keeps its panels, heights and maximization. Until the user changes
it, the Files panel is open and the Processes and Terminal panels closed in the projects' stack, and the report panel is
open and the Processes panel closed in the console sessions' stack, so the right pane is shown. A stored stack that
cannot be read counts as that default. Which panel floats over the terminal at 768 px and below is not part of the
stacks and is not remembered. A right pane the user had hidden with the earlier pane toggle of Octoboard starts with no
panel open in either stack.

**Whether a project session's Report panel is open is kept per session**, not in the projects' stack: it is open by
default, and closing it is remembered for that session alone, so another session of the project still has its own
open. The window keeps the closes of the 200 sessions closed most recently; a session dropped from that list has its
Report panel open again. They are kept in the same browser storage as the stacks.

## Panels and keyboard focus

Each panel is a **named region** — the Files panel named for the project's files, the Processes panel "Processes", the
Terminal panel for the project's terminals, the report panel "Report" — and each is a stop of its own in the `F6` cycle,
in stacking order (Report, Terminal, Files, Processes), except that a maximized panel comes first (see "Moving focus
between regions with F6" in `docs/product/moving-focus-between-regions.md`). Cards in a revealed floating group and a
panel floating alone at 768 px and below are stops; a hidden group and a single-card hover preview are skipped.

**Closing a panel that holds keyboard focus hands focus to the terminal** — or, where the terminal cannot take it,
where it goes instead (see "With no session selected" in `docs/product/session-terminal.md`). So does the right pane
going while its width handle holds focus, a project session's Report panel going with its last page while it holds
focus, and a panel toggle going from the top bar while it holds focus (see "A project session's Report panel" above).
