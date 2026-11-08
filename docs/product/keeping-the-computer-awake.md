# Keeping the computer awake

The desktop application, on macOS and on Windows, can keep the computer and its display from sleeping while the user is
idle, through the **Prevent sleep** switch and its **When to prevent sleep** choice in Settings' App behavior section
(the rows themselves are described in "Prevent sleep" in `docs/product/settings.md`). The switch is off to begin with.
A browser has neither row and holds nothing.

## When the computer is kept awake

While Prevent sleep is on, Octoboard keeps the computer awake according to When to prevent sleep:

- **Always**: for as long as Octoboard runs — with its window shown or closed into the background, and on the screen
  shown when the daemon failed to start.
- **While sessions work**, the choice to begin with: while at least one session, in any console, is *starting*,
  *working*, *dormant* or *working in background*, a *paused* one that still has work running in the background
  included (see `docs/product/session-statuses.md`). A session *waiting for the user* does not count, nor does one
  otherwise at its prompt — so this is the set that holds a quit up (see "Quitting" in
  `docs/product/application-lifecycle.md`) without the sessions waiting for the user. Once no session counts, the
  computer is left to sleep by the system's own settings. On the screen shown when the daemon failed to start there are
  no sessions, so nothing is held.

Turning the switch off, or a change of the choice or of the sessions that ends the need, releases the hold at once.
Each launch holds nothing until the window's UI has loaded and decided, and quitting Octoboard — or its ending in any
other way, a crash included — releases the hold with it.

## What is held

Only the sleep that idleness brings is held off: the system does not go to sleep, and the display does not turn off,
so the screen does not lock, while the user is idle. Closing a laptop's lid, or putting the computer to sleep by hand,
still sleeps it.

- **macOS**: two power assertions, one against idle display sleep and one against idle system sleep
  (`PreventUserIdleDisplaySleep` and `PreventUserIdleSystemSleep`), each named "Octoboard is keeping the computer
  awake", as `pmset -g assertions` lists them.
- **Windows**: one power request for the display and the system, with the reason "Octoboard is keeping the computer
  awake", as `powercfg /requests` lists it.

While the hold is up, the menu bar icon shows it: its eyes turn red, and its accessible name — its tooltip on Windows —
says that Octoboard is keeping the computer awake (see "The icon's look" in `docs/product/menu-bar-icon.md`).

## Where the choice is stored

Prevent sleep and When to prevent sleep are kept **per client**, in that client's own browser storage, under the same
rules as the appearance (see "Where the choice is stored" in `docs/product/appearance.md`): the daemon neither stores
nor reads them. A configuration export that includes the application settings carries the exporting application's
choices, and an import asked to apply them sets them on the application that runs the import (see "The settings a
client holds" in `docs/product/settings-in-a-configuration.md`).
