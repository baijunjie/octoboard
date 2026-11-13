# Terminal font size

The size of the text in every terminal the window shows — a session's terminal and every shell terminal alike — as one
size for all of them, in whole pixels.

## The size and how it is changed

The size is **15 px to begin with**, and runs from **10 px to 24 px**. It is changed in two ways:

- the **Terminal font size** row of Settings' Appearance section, which sets any size in the range and has a button
  back to the default (see "Terminal font size" in `docs/product/settings.md`);
- the three font size shortcuts, which act only while a terminal has keyboard focus (which keys they are, and when a key
  press counts as one, are in "What is listed" in `docs/product/keyboard-shortcuts.md`): **Increase terminal font
  size** and **Decrease terminal font size** each move the size by one pixel, held within the range, so at either end
  the one pointing past it changes nothing; **Reset terminal font size** sets it back to 15 px. The key press never
  reaches the terminal's program.

## What follows the size

A change applies at once to every terminal open in the window, without reconnecting: each is redrawn at the new size
and fitted to its area anew, as many columns and rows as now fit. The client then asks for that size and tells the new
cell size, as it does whenever its terminal area changes, so the program running in the terminal is told the new size
under the rules of "The terminal's size" in `docs/product/terminals-on-several-clients.md`, and the cell size a
program reads follows (see "The colours and sizes a program reads" in `docs/product/terminal-queries.md`). A link's
tooltip shown at that moment is dropped. A terminal opened later starts at the size in force.

Since each client has its own size (see "Where the size is kept" below), two clients showing the same terminal can fit a
different number of columns and rows into the same area; the terminal takes the size of the most recently active one,
as for any other difference in their sizes.

## Where the size is kept

The size is kept **per client**, in that client's own browser storage, as the appearance choice is (see "Where the
choice is stored" in `docs/product/appearance.md`): the daemon neither stores it nor carries it, so a desktop window and
a browser tab, or two machines reaching the same daemon, each keep their own. Another page of the same client open at
the same time, such as a second browser tab, picks up a change only once reloaded. A client whose storage has been
cleared, or holds no size or one that is not a number, is back to 15 px; a stored size outside the range is held to it.
Where storage is unavailable, a change lasts for the current page load only.

A configuration export that includes the Appearance section carries the exporting client's size, and an import asked to
apply that section sets it on the client that runs the import alone (see "The application settings an export carries"
and "The settings a client holds" in `docs/product/settings-in-a-configuration.md`).
