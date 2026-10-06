# Window layout

The window holds up to three panes, left to right: the **sidebar** with the console → project → session
tree (see "The console → project → session menu" in `docs/product/sessions.md`), the selected session's
**terminal**, and — only while the selected session is a hub session — that console's **report panel**
(see `docs/product/report-panel.md`).

How the three are arranged depends on one width, **1100 px**. At that width and above they sit side by
side in a row, the layout the macOS window is always in. Below it the sidebar and the report panel become
overlay drawers over the terminal; only a client with no window minimum of its own, such as the UI opened
in a plain browser, can get there.

For the window's light/dark appearance see `docs/product/appearance.md`.

## The window's minimum size

The macOS window has a **minimum size of 1100×600**. The width is the three panes' own floors added up, so
none of them can be squeezed past usability by a narrower window; the height leaves the terminal about 30
rows.

Nothing enforces a minimum when the UI is opened in a browser, so the narrow layout below applies there.

## Pane widths at 1100 px and wider

- The sidebar is 280 px wide and does not change with the window.
- The report panel is 420 px wide and does not grow: width a wider window frees goes to the terminal.
- As the window narrows it is the panel that gives up width, down to a floor of 300 px.
- The terminal never goes below 55 columns.

Resizing the window resizes the agent's terminal (see "The terminal" in `docs/product/sessions.md`).

## Below 1100 px: the sidebar and the report panel become drawers

Below 1100 px wide, the terminal is the only pane in the row and fills the width. The sidebar and the
report panel each become an **overlay drawer** over it, **closed by default**:

- The sidebar drawer slides in from the left and is 280 px wide; the report panel's drawer slides in from
  the right and is 420 px wide, or 92% of the viewport where that is narrower.
- Each is toggled from its own button in the terminal pane's header bar — sessions on the left, the report
  panel on the right. The report panel's button is there only while the selected session is a hub session,
  since no other session has a panel. Neither button is shown at 1100 px and above, where both panes are
  already in the row.
- A drawer closes on its own toggle, on a press anywhere in the dimmed area beside it, or on `Escape`.
  While a drawer is open `Escape` closes it and **does not reach the running agent**.
- Opening one drawer closes the other; at most one is ever open.
- Widening the window to 1100 px or more closes both, and selecting a session that has no report panel
  closes the report drawer.

Because the drawers overlay the terminal rather than pushing it aside, **opening or closing one never
resizes the terminal**, so the running agent is never sent a terminal-size change by a drawer.

The terminal's floor below the breakpoint is 382 px, about 40 columns. A viewport narrower than that
**scrolls horizontally** rather than squeezing the terminal further.
