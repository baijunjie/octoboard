# Pane docking and resizing

The sidebar and right pane can take columns beside the agent terminal or float over it. The window's chrome and
other regions are in `docs/product/window-layout.md`.

## Docking capacity

Above **768 px**, docking depends on the panes actually shown. The terminal keeps **400 px**, the rail **48 px**, and
the sidebar and each right-pane card keep **260 px** of usable width. The right group also reserves **16 px** of outer
gaps and, with two columns, **10 px** between them. One or two open cards need at least one column; three or four need
two columns (see "Opening and closing panels" in `docs/product/right-pane-panels.md`).

- Above **768 px**, the sidebar can dock: the rail, sidebar and terminal fit their minimum widths. Its saved docking
  choice takes priority over the right group; insufficient space for both makes the right group float while the sidebar
  stays docked.
- With the sidebar hidden or floating, a right group of one or two cards can also dock above **768 px**, and one of
  three or four cards from **994 px**.
- With the sidebar shown, both sides can dock from **984 px** with one or two right cards, or **1254 px** with three or
  four. These capacities follow the panes' actual minimum widths, independently of the desktop presentation.

Capacity uses minimum widths, so widening a pane cannot force the other side to float: the displayed widths first
shrink within their limits. Closing cards, hiding the sidebar or widening the window can make room to dock the right
group again. An absent or closed right group reserves no width. Automatically floating either side does not change
its remembered docking choice; it is restored when space allows. The two sides can dock or float independently.

At **768 px and below**, the rail comes in with the floating sidebar and only one right panel floats at a time (see "At
768 px and below: the rail comes in with the sidebar" in `docs/product/sidebar-drawer.md` and "At 768 px and below: one
panel floats over the terminal" in `docs/product/right-pane-panels.md`).

## Pane widths

| Pane | Default | Range the user can choose |
|---|---|---|
| Sidebar | 280 px | 260–480 px |
| Docked right pane | 420 px | 276–720 px; at least 546 px with three or four cards |
| Terminal | everything left over | never below 400 px (about 40 columns) |

The right pane's width includes the gaps around its cards (see "The panel cards" in
`docs/product/right-pane-panels.md`). The sidebar and the right pane are drawn at the width the user chose (see
"Resizing the sidebar and the right pane" below), subject to the effective minimums below and automatic adjustments when
opening or closing panels changes the number of columns (see "Opening and closing panels" in
`docs/product/right-pane-panels.md`). Extra width a wider window frees goes to the terminal. The right pane has one
width, whoever owns it.

**The terminal always keeps its 400 px.** Where the window cannot afford every pane its chosen width, the right pane
gives up width first, down to its effective minimum, and only then the sidebar, down to 260 px. Three or four cards
require a 546 px right pane: two 260 px columns, their 10 px gap and 8 px at each outer edge. These bounds do not
rewrite the width preference. When the window widens again, each pane gets its chosen width back, including any
current automatic column-width adjustment in the right pane.

A sidebar the user has hidden takes no width at all once it is hidden, and neither does one only floating in: its
width goes to the terminal. While the sidebar's docked column is easing closed, it still takes the width it has
reached (see "Hiding the sidebar" below). A right pane with none of its panels open, or with no owner, is not in the
row, takes no width and stops holding back the sidebar's width; a floating right group or a mobile panel takes none
either.

Anything that changes the terminal's size — resizing the window, resizing a pane, hiding or showing the sidebar, the
right pane coming or going as its first panel opens or its last closes or with its owner — resizes the agent's
terminal (see "Output, scrollback and size" in `docs/product/session-terminal.md`). A panel floating over the
terminal, alone or as a whole floating group, coming or going does not.

### Resizing the sidebar and the right pane

Above 768 px, each pane has a resize handle on its inner edge while it is shown, running the pane's full height. A
docked handle runs from below the top bar to above the connection banner; a floating handle follows the card's inset
edge:

- the **docked sidebar's** immediately to the physical right of its inner seam in either reading direction, clear of
  both scrollbars, drawn on hover and keyboard focus as a line along the whole edge;
- the **floating sidebar's** in the 8 px gap outside its inner edge, drawn as the same grip as the right pane's;
- the **right pane's** just inside the cards' inner edge while docked, and in the 8 px gap beside the cards while
  floating, drawn as a **grip**: a short rounded bar in the middle of the handle's height, light grey while the pointer
  or keyboard focus is on the handle, and strong only while it is pressed or dragged. While a mouse pointer rests on
  it, a tooltip saying "Resize" opens beside the grip, toward the pane. It is named "Resize panel area". The floating
  sidebar's grip has the same tooltip, facing into
  the sidebar; it mirrors with the layout under a right-to-left language. Both vertical grips are 4×40 px.

The handles work the same way:

- **Dragging** the handle sets the pane's width within its current limits; the right pane's minimum follows the
  number of cards (see "Floating pane widths" below).
- **Double-clicking** it returns the pane to its default width.
- The handle is also reachable with Tab and is then operated from the keyboard: the arrow pointing away from the pane
  widens it and the other narrows it (Right widens the sidebar and Left the right pane; the other way round under a
  right-to-left language, where the panes have swapped sides), by 16 px, by 64 px with Shift held; Home sets the pane's
  minimum, and End the widest the window currently allows. While it has keyboard focus the docked sidebar's handle
  is drawn as a solid accent-coloured bar with a focus ring around it, and a grip shows with a focus ring around the
  handle. Pressing the handle with the mouse does not take keyboard focus off the terminal, and while it is dragged
  the pointer shows the resize cursor wherever it is.

The sidebar's handle is available while docked and while floating, including a hover preview or an automatically
floating sidebar. Its pointer target is 6 px wide immediately to the physical right of its inner seam while docked and
24 px wide while floating, centred on the 8 px gap outside the floating card's inner edge. It is absent while the
docked column is easing and at 768 px and below. The floating sidebar uses the same remembered width as the docked sidebar, within
**260–480 px**, with a default of **280 px**.

The right pane's handle is available while docked and while its whole group floats, including when it floats
automatically. It remains reachable at the floating group's inner edge and changes the group's total width. It is
absent for a mobile panel and a column folded away under a maximized panel (see "Maximizing a panel" in
`docs/product/right-pane-panels.md`). Its pointer target is 6 px wide just
inside the cards' inner edge while docked, clear of the terminal's scrollbar, and 24 px wide while floating, centred
on the 8 px gap. Resizing a floating sidebar or right group keeps it revealed throughout the drag and does
not resize the agent terminal.

**A floating right pane can widen to the sidebar's content-side edge**, keeping the cards' usual insets. Its maximum
total width is the window's width less the **48 px rail** and the width of the docked sidebar while shown. With the
sidebar hidden or itself floating, only the rail is reserved. Above 768 px, floating widths have no fixed 720 px cap.

**Each pane's chosen width is remembered per client**, in that client's own browser storage, as the appearance choice
is (see "Where the choice is stored" in `docs/product/appearance.md`); without that storage a choice lasts until the
page is reloaded. A stored sidebar width below 260 px is used as 260 px. A right-pane width above 720 px is remembered
too: docking the pane or narrowing the window limits its rendered width without replacing that choice. Floating it
again or widening the available space restores the chosen width as far as the current bounds allow.

## Hiding the sidebar

When the sidebar can dock, the sidebar toggle in the top bar hides and shows the docked sidebar. The sidebar is shown by
default, and **whether it is hidden is remembered per client**, the same way as the panes' widths. Hiding it hands
its width to the terminal. The right pane has a separate remembered docking choice (see "Floating a panel in" in
`docs/product/right-pane-panels.md`).

Hiding the sidebar while it holds keyboard focus — a sidebar row reached with Tab, say — moves focus to the terminal,
or to the top bar when no session is selected.

When the sidebar can dock, hiding or showing the docked sidebar eases its column over **200 ms**. Where the system asks
for reduced motion, the column opens or closes at once. Pressing the toggle again while the column is still moving
reverses the ease. When it cannot dock, the sidebar floats and fades into place instead (see "When the
sidebar cannot dock" in `docs/product/sidebar-drawer.md`).

During the ease the sidebar stays a full-height column, from below the top bar to above the connection banner when
that banner is shown, and does not become the rounded card in "A hidden pane floats in on hover" below. Its contents
stay at the width the open column is drawn at, including where the window is holding that width back (see "Pane
widths" above), anchored against the rail. The inner edge covers them as the column closes and uncovers them as it
opens, so the text does not reflow. The shadow the terminal's area casts over the sidebar follows the column's inner
edge through the ease and is gone once the column has closed. A hover card comes up only once the column has closed.

As the sidebar's column closes, the width it gives up goes to the terminal. When the right pane is docked and had been
held below the width the user chose, it can grow into that room as the room appears. Showing the sidebar limits the
right pane again as soon as the sidebar is shown, when the window cannot afford both panes their chosen widths, and
the opening column takes its own width from the terminal as it eases open. The terminal is not refitted while the
column moves: it keeps its size through the ease and is fitted once the column comes to rest (see "Output, scrollback
and size" in `docs/product/session-terminal.md`).

### A hidden pane floats in on hover

Above 768 px, the sidebar can float in while its docked column is hidden or it cannot dock. The right pane can reveal
its open cards as a floating group (see "Floating a panel in" in `docs/product/right-pane-panels.md`). This is a
**floating pane**. The sidebar floats in as a card inset from the window chrome and the window's edges, with rounded
corners, a hairline edge and the light card
shadow (see "The window chrome and the content panel" in `docs/product/window-layout.md`); the right pane floats in as
its panel's own card, which looks the same (see "The panel cards" in `docs/product/right-pane-panels.md`). The hairline
is 0.5 px on a display of 2× pixel density or more and 1 px below that, and it takes no space, so the card is exactly
its box. An automatically floating sidebar shares this card appearance (see "When the sidebar cannot dock" in
`docs/product/sidebar-drawer.md`). The docked sidebar opens and closes as a full-height column (see "Hiding the sidebar"
above).

- **What brings up the sidebar**: reaching its top bar toggle reveals the current console immediately. Resting on blank
  space anywhere along the **48 px rail for 200 ms** also reveals the current console. Crossing that space briefly
  cancels the pending reveal. Interactive controls — including New console, Account usage, Settings, links and menu
  items — are excluded, so aiming at them does not open the sidebar. Console avatars have their own **130 ms** preview:
  an avatar reveals that console, and moving between avatars while the sidebar is out previews them immediately. Moving
  to the sidebar toggle or resting on blank rail space restores the current console. Hover never selects a console (see
  "Previewing a console from the rail" in `docs/product/console-switcher.md`).
- **What brings up the right pane**: while the group floats and has open cards, reaching its group toggle or any panel
  toggle reveals the whole group immediately. Resting for **200 ms** in the **48 px strip at the window's end edge**,
  below the top bar, reveals it too. Briefly crossing the strip cancels the reveal. The strip follows the logical end
  edge under a right-to-left language. With no cards open, or while the group is docked, hover reveals nothing; see
  "Floating a panel in" in `docs/product/right-pane-panels.md`.
- Touch and pen bring up neither, and on a device whose primary input cannot hover — a phone, or a tablet's touch screen
  — neither floats in at all, whatever pointer is used: the sidebar is shown with its toggle and a panel opened by
  pressing its toggle.
- **How it shows**: both cards fade from transparent to opaque while rising into place by **4% of the card's own
  height**, over **240 ms**, with the same `cubic-bezier(0.2, 0, 0, 1)` easing as switching consoles. The whole card
  fades and rises together — its edge, fill, shadow and content. Neither slides horizontally. Where the system asks for
  reduced motion, they appear at once. Each uses the pane's chosen width within its current limits (see "Resizing the
  sidebar and the right pane" above), with a shadow and no dimming behind it. It overlays the terminal, so **the
  terminal is not resized** and the agent is sent no size change. - The **sidebar's card** keeps an 8 px gap from the
  rail, from the top bar and from the window's bottom (or the connection banner), never drawn over the rail. - The
  **right pane's card** keeps the gaps it keeps docked: 8 px from the top bar, from the window's end edge and from the
  window's bottom (or the connection banner), and 8 px on the terminal's side. - A card's own headers are 8 px shorter
  than the docked sidebar's — the 8 px gap, the card's edge taking no space — so the line under a header stays level
  across the panel (see "The window chrome and the content panel" in `docs/product/window-layout.md`).
- **How it goes away**: 200 ms after the pointer has left it, the card fades out while sinking back by 4% of its height
  over 240 ms, with the same easing as its entrance, or disappears at once where the system asks for reduced motion.
  Having left means the pointer is on none of the card, the gap around it, or what brought it up in the top bar — the
  sidebar's toggle, the group toggle or any panel toggle for the right pane — and, for the sidebar, none of the console
  avatars on the rail either, or the blank rail area that can reveal it. The right pane's end-edge strip also keeps it
  up. **The gap counts as part of the card**: the 8 px around the sidebar's card, and the gaps the right pane keeps
  around its card. The narrow strip directly below a pane toggle, from the button's bottom to the card's surrounding
  gap, also keeps it up where the button and card overlap horizontally. The pointer can cross or rest in that strip in
  either direction without starting dismissal; the rest of the top bar remains outside. A tooltip neither takes pointer
  events nor determines whether the pane stays revealed. A pointer that moves from the card into its gap, or rests in
  it, keeps the card up for as long as it stays. Moving past these areas, or out of the window altogether, sends the
  card away. `Escape` starts its exit without that delay and **does not reach the running agent**, except that in a
  field holding text (the project pane's change filter) it only clears the field, and the next one sends the pane away;
  so does `Escape` pressed inside a report page (see "Escape and F6 inside a page" in `docs/product/report-panel.md`).
  Inside a shell terminal of the Terminal panel `Escape` goes to the shell and never sends the pane away (see "The shell
  terminal" in `docs/product/terminal-panel.md`). A floating pane that holds keyboard focus as it goes away hands focus
  to the terminal, except a floating right pane that goes because the pane's column takes its place — a panel docked, a
  maximized panel restored (see "Maximizing a panel" in `docs/product/right-pane-panels.md`): focus is then not moved. A
  floating sidebar previewing a console that is deleted goes away as if the pointer had left that console's avatar.
- **What keeps it up**, whatever the pointer does: keyboard focus inside it, an open menu, a dialog or a resize drag. A
  file viewer opened from the Files panel counts as a dialog (see `docs/product/file-viewer.md`). So closing a viewer
  opened from a floating Files panel, by the pointer or the keyboard, finds the pane still there to hand focus back to.
  For the right pane, focus keeps it up only when the keyboard put it there: focus a press put inside it, on a tab or a
  button, does not, and the card goes once the pointer leaves, as it would have before the press.
- **Only one at a time**: bringing one up sends the other away. Floating the right pane in also closes the sidebar's
  overlay, and opening the floating sidebar sends a floating right pane away.
- **Pressing the sidebar toggle or the right group toggle** when that side cannot dock reveals the sidebar or any open
  right-panel cards without hiding them or changing a docking preference. Individual right-panel toggles open or close
  their cards, keeping the other cards revealed and hiding the group when the last card closes. Activating a stored open card's toggle while the group is
  hidden only reveals it. See "The panel toggles in the top bar" in `docs/product/right-pane-panels.md`. When that side
  can dock, the right group toggle docks the group, the sidebar toggle docks its hover card, and a docked right pane's
  panel toggle opens or closes its panel normally.
- Hover reveal is unavailable at 768 px and below: only explicit controls and swipes open the mobile overlays.

## Floating pane widths

Above **768 px**, floating panes take no width from the terminal. Only one floating side is revealed at a time, without
changing the terminal's size; the other side can remain docked. The right group uses the remembered total width and the
automatic adjustments in "Opening and closing panels" in `docs/product/right-pane-panels.md`, within the floating bounds
in "Resizing the sidebar and the right pane" above. Its minimum is **546 px with three or four cards**, and **276 px
with one or two cards**: a 260 px column and two 8 px outer gaps. These bounds do not rewrite the remembered width. The
group retains its column-width and panel-height handles as well as its total-width handle.

**At 768 px and below**, only one right-pane panel floats at a time, or the sidebar with its rail. The terminal has no
minimum width and keeps the content panel's whole width, so the row never scrolls sideways. See "At 768 px and below:
one panel floats over the terminal" in `docs/product/right-pane-panels.md` and "At 768 px and below: the rail comes in
with the sidebar" in `docs/product/sidebar-drawer.md`.
