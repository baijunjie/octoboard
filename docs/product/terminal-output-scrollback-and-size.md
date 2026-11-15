# Terminal output, scrollback and size

How the session terminal (see `docs/product/session-terminal.md`) is attached to and receives its output, what
scrollback it shows, and how its size and colours are set.

## Attaching and scrollback

- **Attaching to a running session starts from a first paint of its terminal**, then follows live output, with no byte
  lost or repeated between the two. The daemon keeps a continuously parsed copy of every running terminal, and the first
  paint is that copy written out as escape sequences: it resets and clears the terminal, writes up to **10,000 rows** of
  the normal screen's scrollback, then the screen's rows with their colours, styles and links — when the agent is on its
  alternate screen, the normal screen's rows and cursor first, then the alternate screen's — and then restores the
  scroll region, the saved and current cursor with the attributes each draws with, the terminal modes (mouse reporting,
  focus reporting, bracketed paste, application cursor and keypad keys, insert, newline and origin modes, a hidden
  cursor, disabled line wrap), the cursor style the agent chose and the window title. Attaching covers selecting a
  session, reconnecting and reloading the window. The terminal itself keeps 10,000 lines of scrollback. An interrupted
  session shows the saved preview described in "The saved terminal preview" in
  `docs/product/session-terminal-interrupted-and-archived-sessions.md`; an archived one shows its history instead.
- **A newly attached or reconnecting terminal therefore shows the agent's exact current screen however much output it
  has written**, and a full-screen agent stays full-screen and keeps answering the mouse, the wheel included. Its
  scrollback is the daemon's parsed scrollback, up to 10,000 rows. Lines the output wrapped stay joined, so they reflow
  when the terminal is resized.
- Not painted, and so missing on a terminal attached after the agent set them: the normal screen behind the alternate
  screen after the terminal's width changed while the agent was on its alternate screen, until the agent enters it again
  (a terminal that attached then gets no scrollback, and shows a blank normal screen once the agent leaves the alternate
  one), character sets, tab stops, colour palette changes, and keyboard-protocol enhancements an agent asked for (the
  kitty keyboard flags).
- The first paint is drawn at the size the agent's terminal has at that moment. The size the attaching client reports
  is applied after its paint, and an agent told a different size redraws for it over the painted screen. Attaching
  makes the client the most recently active one, which is what decides the size while several clients show the
  terminal (see "The terminal's size" in `docs/product/terminals-on-several-clients.md`).
- A reattach always redraws from a new first paint rather than appending to what is on screen, so scrollback older
  than the daemon's 10,000 rows is lost on every reconnect.

## Sending output to clients

- **A client that falls behind the output stays connected, and holds up nothing else.** The daemon sends each client
  output only up to about **512 KiB** ahead of what that client's terminal has taken in; the rest waits in the daemon
  for that client alone, so neither the program nor any other client showing the terminal is slowed by it. Once more than
  **4 MiB** waits for one client, what waits is discarded and the client is sent a fresh paint of the current screen in
  its place, drawn as the first paint is but without scrollback; live output follows it with no byte lost or repeated.
  The fresh paint resets the terminal, so that client's scrollback is gone from then on, and with it, as on any reset of
  the screen, what a program copied that the editing menu offered (see "The menu offers what a program copied" in
  `docs/product/terminal-copy-paste-and-selection.md`), a link's tooltip and what was known of the file paths on the
  replaced screen (see `docs/product/terminal-links.md`). A client that stops reading altogether (a suspended window, a
  stalled network) is not disconnected either: once it reads again it is sent that paint rather than the output it
  missed. A client of the daemon that does not report what its terminal has taken in is sent output as fast as its
  connection takes it, and is moved to a fresh paint by the 4 MiB limit alone.
- **When a terminal finishes** — the session's process ends, or a shell terminal's shell exits or the terminal is
  closed — each attached client is still sent the output waiting for it before its connection ends.

## The character grid and its size

- The character grid has a 14px base gutter on each physical side, in either reading direction. The right gutter is
  the terminal's internal scrollbar area, reserved even while the scrollbar is hidden or the agent uses a full-screen
  interface; it has no additional outer padding. Docked panels add no extra gap beside the terminal, and resize handles
  add no terminal padding. Any space left after fitting whole character columns is shared between the two sides.
- Whatever changes the terminal pane's size resizes the agent's terminal: resizing the window, resizing the sidebar or
  the right pane, hiding or showing the sidebar, and the right pane coming or going as its panels open and close all do;
  a pane floating in or the sidebar's drawer opening over the terminal does not (see `docs/product/pane-docking.md` and
  `docs/product/sidebar-drawer.md`).
  The screen follows at once (except while a resize handle is dragged or a docked column eases, below), but the agent
  is told the new size only once it has held still for about 120 ms, and only when it differs from the size the agent
  already has, so a drag sends the agent one size change rather than one for every step of the drag. A size change
  made in the window the user is in makes this client the most recently active one, so it applies at once even while
  another client shows the same terminal; one made in a window the user is not in waits while another client showing
  the same terminal is more recently active, until this one is again (see "The terminal's size" in
  `docs/product/terminals-on-several-clients.md`). The
  window has a minimum size, sized so that the sidebar, the terminal and the right pane all stay usable side by
  side — see "The window's minimum size" in `docs/product/window-layout.md`.
- While any pane-width, panel-height or column-width handle is being dragged, the terminal keeps its grid size and
  continues showing the agent's output. After the drag ends, a shown terminal whose area changed is fitted once to
  its current area; a terminal whose area did not change needs no fit. Resizing a floating sidebar therefore leaves
  the agent terminal's grid unchanged. A hidden terminal fits when it is shown. The agent is told a changed size
  with the same delay as above.
- While the docked sidebar's column is easing open or closed (see "Hiding the sidebar" in
  `docs/product/pane-docking.md`), the terminal's grid keeps the size it had when the ease began, and is fitted to its
  area once, when the column comes to rest — at most about a quarter of a second after the toggle was last pressed. Any
  other change to the terminal pane's size during the ease, a window resize included, is held until then too. Until that
  fit, a closing column adds blank space on both sides of the grid and an opening one covers its rightmost
  columns. The agent is then told the new size as above. Where the system asks for reduced motion the column does not
  ease, and the terminal follows at once. The right pane does not ease at all.

## Colours

- The terminal's colours follow the window's light or dark appearance, and its text is held to a minimum contrast
  against its cell's background whatever colours the agent prints — see "What follows the choice" in
  `docs/product/appearance.md`.
