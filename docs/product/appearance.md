# Appearance

## Light, dark and follow the system

The window has three appearances, chosen in the Appearance row of Settings' General section (see
`docs/product/settings.md`): **Light**, **Dark** and **System**. One of the three is always current.

**The default is System**, which follows the operating system's own appearance and switches with it while
the window is open — the user does not have to come back to the control after changing the system setting.

## What follows the choice

Everything the window shows follows the chosen appearance, including:

- **The terminal.** Each appearance has its own full palette — background, foreground, cursor, selection
  and all sixteen ANSI colours — so ordinary coloured agent output stays readable on a light background as
  well as a dark one.
- **The top bar**, which in the macOS application stands in for the window's titlebar (see "The top bar" in
  `docs/product/window-layout.md`).
- **The native window's own appearance** in the macOS application — the window's close, minimise and zoom buttons
  and anything else the system draws for the window. Choosing System hands the window's appearance back to the
  operating system rather than pinning it to whatever System resolved to at that moment, so it keeps following a
  live system change too. A client with no native window, such as the UI opened in a browser, has nothing native to
  match and ignores this.

The one deliberate exception is a report page — see below.

How a launch avoids showing the window before the appearance has been applied is described under
"Starting up" in `docs/product/application-lifecycle.md`.

## Where the choice is stored

The choice is remembered **per client**, in that client's own browser storage. The daemon neither stores
it nor carries it: nothing in the daemon's protocol mentions an appearance, and it is not among the files
under `~/.octoboard` (see "Files Octoboard owns" in `docs/product/application-lifecycle.md`).

So the choice does not travel. A desktop window and a browser tab, or two machines reaching the same
daemon, each keep their own; a client whose storage has been cleared is back to System.

## A report page stays on a light surface in both appearances

A page in the report panel is rendered on a **light surface whatever the window's appearance is**, and its
own unstyled form controls and canvas rendering stay light as well, regardless of the operating system's
preference. The panel's own chrome around the page — the header bar with the paging controls — follows the
chosen appearance like everything else.

This is a decision, not an oversight. A page is arbitrary HTML the hub's model wrote (see "What a page may
contain, and what it cannot do" in `docs/product/report-panel.md`), and one that hard-codes dark text would
become unreadable if the surface under it went dark. Holding the surface light keeps every page legible, at
the cost of a page not matching a dark window.
