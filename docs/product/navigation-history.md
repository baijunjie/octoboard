# Navigation history

The window keeps a history of the places it has shown, and the top bar's **Back** and **Forward** buttons move through
it, as in a web browser (see "The top bar" in `docs/product/window-layout.md`).

## What counts as a place

A place is what the window shows, taken together:

- the current console (see "The console switcher" in `docs/product/sidebar.md`); a console the floating sidebar is
  only previewing is not part of the place;
- the selected session, or none;
- the focus mode the sidebar is in, if any (see `docs/product/focus-mode.md`);
- the archive view and what it is for, while it is open (see "The archive view" in `docs/product/sidebar.md`).

What the right pane shows is not part of a place: Browse files records no visit, and a move gives the right pane what
selecting the session moved to would, or leaves it as it is when the selected session stays (see "What the right pane
shows" in `docs/product/window-layout.md`).

**Every change of any of these is a visit**, whatever made it: a sidebar row, a console avatar on the rail, entering or
leaving focus mode, a chip in a focus mode, the archive view opening or closing, the waiting count, the menu bar icon's
menu, a session this window opened being selected, a keyboard shortcut. A change that leaves the window where it already
was is not recorded again.

One press can make two changes and still be one visit: a press inside the floating sidebar that makes the console it
previews the current one and goes on to select a session there (see "Previewing a console from the rail" in
`docs/product/sidebar.md`). Back from it returns to where the window was before the press. A selection made after that
press has ended is a visit of its own.

The history starts once the daemon's state has first arrived. It is kept in memory for the window alone: it is not
stored, so reloading the window or restarting the application starts it empty, and every client has its own. It holds
the **100** most recent places; older ones are dropped.

## Moving back and forward

- **Back** goes to the place visited before the one on screen, **Forward** to the one after it. Neither records a
  visit of its own.
- A **visit after going back** drops every place that was ahead, as a browser does: Forward then has nowhere to go.
- **A place that cannot be shown any more is passed over**: one whose console, session, project or console session has
  since been deleted, whose focus mode no longer exists (a console session archived, a project removed), or whose focus
  mode does not list its session. So is a place identical to the one on screen. Each button is **disabled** while every
  place in its direction is passed over or there is none.
- **Moving shows the place as it was, without acting on it**: going back to an interrupted session shows it without
  resuming it, and going back to a place with the archive view open opens it again. Selecting a session, by contrast,
  resumes an interrupted one (see "Selecting a session" in `docs/product/sidebar.md`).
- **When the place on screen stops existing** — its selected session deleted, its console deleted, its focus mode gone —
  the window falls back by itself, as described where each of those is, and the place it falls back to takes the
  place of the one that has gone rather than counting as a visit.
- When a move leaves keyboard focus on nothing, or on a Back or Forward button that the move has just disabled, focus
  goes to the terminal.

## Keyboard shortcuts

**In the macOS application**, `Cmd+[` goes back and `Cmd+]` goes forward, matched on the physical bracket keys
whatever the keyboard layout, and only with no other modifier held.

- They work wherever keyboard focus is, the terminal included, and the keys never reach the terminal, so the agent
  never receives them. Pressed inside a report page they are passed to the window (see "Window shortcuts inside a
  page" in `docs/product/report-panel.md`).
- They do nothing while a dialog or a menu is open, and are left alone during an input-method composition.
- In a browser there are no such shortcuts: the keys are left to the browser and the terminal. The buttons work
  everywhere.
