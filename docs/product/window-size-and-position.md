# Window size and position

Where the macOS application's window opens: its size, position and maximized state across launches. Its minimum size
is in "The window's minimum size" in `docs/product/window-layout.md`.

## The window's size and position across launches

The macOS window reopens with the **size, position and maximized state it had when the application last quit**. A
browser tab has no window of its own to place, so none of this applies there.

- **First launch**, or a saved state that cannot be read: the window opens at **1200×760**, placed by the system,
  which centres it.
- The state is saved whenever the application quits normally — every way of quitting in "Quitting" in
  `docs/product/application-lifecycle.md`. A crash or a killed process saves nothing, and the next launch uses what the
  last normal quit saved.
- What is kept is the window's normal frame plus whether it was maximized: a window quit maximized reopens maximized,
  on the display its frame is on, and un-maximizes back to that frame. **Native fullscreen and minimized are never
  restored**: a window quit in either state reopens as it was before entering it.
- The saved frame is checked against the displays connected at launch, measured by their usable area (without the
  menu bar and the Dock). It is restored when at least 200×20 points of its top 40 points — the top bar, which is
  where the window is grabbed to move it — lie on one connected display. The restored frame is then held inside the
  rectangle spanning all the connected displays, its size first (never below the 1148×600 minimum) and then its
  position, so a window spanning displays that are all still there comes back unchanged, and one that reached onto a
  display since unplugged is pulled back onto the ones left.
- Otherwise — its display is gone, its top bar is off every display, or pulling it back would leave its top bar off
  every display — the window opens at 1200×760, centred on the main display. A saved maximized state is kept in this
  case too.
- The frame is decided before the window is shown, so the window is never seen moving into place.

The state is kept in `~/Library/Application Support/dev.octoboard.app/window-state.json` (see "Files Octoboard owns"
in `docs/product/application-lifecycle.md`).
