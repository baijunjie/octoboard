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
  well as a dark one. On top of that, in either appearance the terminal enforces a **minimum contrast ratio of
  4.5:1** between each character and the background of its own cell — the terminal's background, a background colour
  the agent set for that cell, or the selection colour where the text is selected. A foreground colour that falls
  under it, whether the default foreground, one of the sixteen ANSI colours, the 256-colour range or a true colour, is
  shown lightened or darkened until it reaches the ratio, or as near to it as the colour can go where it cannot; colours
  that already clear it are shown unchanged. Faint text is held to half the ratio, 2.25:1, so it stays distinct from
  ordinary text. Box-drawing and block-element glyphs and most Powerline separator glyphs are exempt and keep the colour
  they were given; the Powerline branch, line-number and lock symbols are adjusted like any other character. So
  output from an agent whose own colour scheme assumes the other appearance (pale greys and blues meant for a dark
  background, for instance) stays readable rather than all but vanishing.
- **A console session's colour**, shown in its own sidebar row and as the binding badge on every project
  session bound to it (see "The binding badge" in `docs/product/sidebar.md`).
  Each colour in the fixed palette of console session colours has its own light and dark value, so in either
  appearance every one still looks like its name; the swatches of the action menu's **Colour** submenu show the
  values of the appearance in force. The colour is decorative — the console session's name is always beside it, or in
  the binding badge's tooltip and its row's accessible name — so it is not held to a contrast ratio; a faint ring of
  the foreground colour round each dot and each swatch keeps the palest colours (yellow in the light appearance, gray
  in the dark one) visible.
- **The window chrome** — the top bar, which in the macOS application stands in for the window's titlebar, and the
  rail. In the macOS application it is the system's translucent sidebar material, drawn in the chosen appearance; in a
  browser it is an opaque colour of the appearance's own (see "The window chrome and the content panel" in
  `docs/product/window-layout.md`).
- **Code in a project pane's file viewer**, highlighted in a high-contrast colour scheme of its own for each
  appearance, every colour of which meets 4.5:1 against the code's background. That background is the colour
  scheme's own: white in the light appearance, near-black in the dark one. What sets the reading area apart from the
  surrounding dialog — white in light, lighter than the reading area in dark — is a narrow band of its own round the
  area, not the area's fill. The background fills the whole code area, including space beyond the text,
  and a file shown as plain text sits on it too;
  a file shown as an image sits on a checkerboard drawn in the appearance's colours (see "The file viewer" in
  `docs/product/project-pane.md`). A diff there (see "Opening a change" in `docs/product/project-pane-git-change-viewer.md`) is
  highlighted in the same scheme, with added and removed lines tinted and marked `+` and `-`, and a changed word within
  a line underlined rather than tinted, so that every colour still meets 4.5:1 on the tinted lines. A rendered
  Markdown document (see "A Markdown file as a document" in `docs/product/project-pane.md`) sits on that background
  too, whichever Markdown style is chosen, its links underlined as well as coloured, in a colour that meets 4.5:1 on
  it in either appearance. A diagram
  in that document is drawn in the same ink and paper as the highlighted code.
- **A console's default avatar**, whose fills have their own light and dark values (see "Avatar" in
  `docs/product/consoles.md`).
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
own unstyled form controls stay light as well, regardless of the operating system's preference. The panel's own
chrome around the page — the header bar with the paging controls — follows the chosen appearance like everything
else.

This is a decision, not an oversight. A page is arbitrary HTML the console session's model wrote (see "What a page may
contain, and what it cannot do" in `docs/product/report-panel.md`), and one that hard-codes dark text would
become unreadable if the surface under it went dark. Holding the surface light keeps every page legible, at
the cost of a page not matching a dark window.
