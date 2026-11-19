# A terminal shown on several clients

How one terminal behaves while several clients show it at once — desktop windows, browser tabs, another machine
reaching the same daemon. This holds for a session's terminal (see `docs/product/session-terminal.md`) and a shell
terminal of the Terminal panel (see `docs/product/shell-terminals.md`) alike.

Every client attached to a terminal shows the same program, which runs at one size and reads one set of colours.
Octoboard decides both from which client was most recently active, not from whichever client spoke last. A client that
does not attach the terminal plays no part in either, and neither does an attached client that only watches (see
"Watching a terminal" below).

Each client is sent the output at its own pace: one that falls behind holds up neither the program nor the other
clients (see "Sending output to clients" in `docs/product/terminal-output-scrollback-and-size.md`).

## The most recently active client

- **A client becomes the most recently active one when it attaches the terminal, and whenever it is used**: input is
  sent from it, or the person turns to its terminal. Input is typing, a paste, a mouse button press. Attaching covers
  selecting a session, reconnecting and reloading the window, as in "Attaching and scrollback" in
  `docs/product/terminal-output-scrollback-and-size.md`.
- **The person turns to a client's terminal**, in the desktop app and in a browser, when:
  - the terminal is attached at the user's request (selecting its session, choosing Reconnect) while the user is in its
    window, that is, the window is visible and has focus;
  - a mouse button is pressed on the terminal, the right-click that opens the editing menu included;
  - the window gets focus back while it shows the terminal;
  - the terminal's size changes in the window the user is in, once the change has settled (see "The terminal's size"
    below);
  - the terminal attaches again by itself once its process can be reached again, when keyboard focus was on the cover
    shown meanwhile as that happened, wherever focus then goes (see "While the session's process cannot be reached" in
    `docs/product/session-terminal-reconnecting-and-loading.md`).

  This makes the client the most recently active one with nothing typed. A reconnect made in the background (after a
  dropped connection) and a size change in a window the user is not in do not count as turning to the terminal, and
  while a terminal's process cannot be reached nothing a client does in it is sent, turning to it included. A client
  tells Octoboard that the person turned to its terminal with an `{"type":"activate"}` control message on the terminal's
  stream.
- **What its terminal sends by itself does not count**: focus reports, pointer movement, wheel turns and button
  releases the program receives as mouse reports. Neither do pointing at its window, its display (colours and cell
  size) changing, nor watching the output: a client in a background tab receives output just as a focused one does. On
  an alternate screen without mouse reporting, the wheel sends the program arrow keys, and those count as input.
- **Attaching counts for 30 seconds only**, a chosen default rather than a measured idle time. A client that has not
  been used since it attached stops counting once 30 seconds have passed, and ranks below every client that has been
  used. The most recently active client is then the one used last, unless another client has attached since and its
  own 30 seconds have not passed; while no attached client has been used, it is the one that attached last.
- **Being used does not lapse**: the client used last stays the most recently active one until another client attaches
  or is used.
- **A client that leaves** (its view closed, its connection lost) stops counting at once.

## The terminal's size

- **The terminal has the size asked for by the most recently active client that has asked for one.** Octoboard's own
  windows, in the desktop app and in a browser, ask for the size their terminal area fits, on attaching and whenever
  that area changes (see "The character grid and its size" in `docs/product/terminal-output-scrollback-and-size.md`),
  so they always take part in the size. A client asks for a size with a `{"type":"resize","cols":N,"rows":N}` control
  message on the terminal's stream.
- **Octoboard keeps the latest size each attached client asked for.** A size asked for by a client that is not the
  most recently active one waits; it is applied when that client becomes the most recently active one, by being used,
  without the client asking again. A size change made in the window the user is in turns the person to that terminal
  (see "The most recently active client" above), so the new size applies at once; one made in a window the user is not
  in waits.
- **A newly attached client's size is applied after its first paint**, when it asks for one on attaching, since
  attaching makes it the most recently active client. Once its 30 seconds pass without it being used, the terminal goes back to the size of the client
  that is now the most recently active one.
- **When the client whose size the terminal has leaves**, the terminal takes the size of the most recently active
  client still attached that has asked for one.
- **With no client attached, or none that has asked for a size**, the terminal keeps the size it has.
- **The size and the colours change together**: whatever changes the most recently active client brings the terminal's
  size and the colours and cell size programs read up to date at the same moment, each taken from the most recently
  active client among those that have told Octoboard one (a size, a display).
- The program is told a size only when it differs from the size the terminal already has.

## Clients are told the size

- **Every attached client is told the terminal's size** with a `{"type":"size","cols":N,"rows":N,"held":B}` message on
  the terminal's stream: right after the replay mark of its first paint and of each fresh paint (see "Attaching and
  scrollback" and "Sending output to clients" in `docs/product/terminal-output-scrollback-and-size.md`), before the
  paint itself, so the paint can be drawn at that size. It is sent again whenever the size changes, at that point in
  the output, so the output after it is laid out for the new size; a repaint Octoboard has the program make by resizing
  it one row and back shows as two such messages.
- **`held`** is whether any attached client has asked for a size and not withdrawn it (see "Watching a terminal"
  below). A message is sent also when only `held` changes, the size staying the same: the first client asking for a
  size, or the last one that had asked going back to watching or leaving. Nothing is sent while neither the size nor
  `held` changes.
- These messages do not count against the output a client is sent ahead of what its terminal has taken in.
- Octoboard's own windows ignore these messages: they keep fitting their grid to their terminal area as set out in
  "The character grid and its size" in `docs/product/terminal-output-scrollback-and-size.md`.

## Watching a terminal

- **A client that has not asked for a size only watches.** Its attaching, its input and the person turning to its
  terminal (an `activate`) never change the terminal's size, the 30 seconds an attach counts for included; its input
  still reaches the program. It shows the terminal at the size the size messages tell it (see "Clients are told the
  size" above). By the protocol's rule a watcher tells Octoboard no display either, so it neither changes the colours
  and cell size programs read nor makes the program redraw for every client.
- **While no client holds the size** (`held` is false), the terminal keeps the size it has, and a client that asks for
  one takes it over.
- **Taking the terminal over**: a watcher asks for a size, tells its display and sends `activate`. A size asked for
  alone applies only while the client is the most recently active one, and the precedence its attach gave it may have
  lapsed, hence the `activate`. From then on its size and display count as any client's do, under the rules above.
- **Going back to watching**: a client sends `{"type":"watch"}` on the terminal's stream. The size it asked for and
  the display it told no longer count; those of the most recently active client that has one apply in their place,
  and with no client asking for a size the terminal keeps the size it has. It takes part again by asking for a size
  (and telling its display).
- A daemon of the same protocol generation that predates watching ignores `watch` and sends no size messages; a
  client that receives no size message before a paint is talking to such a daemon.

## The colours and cell size

A program that asks its terminal for colours or for a cell's size is answered with the display of the most recently
active client, in the sense above; see "The colours and sizes a program reads" in
`docs/product/terminal-queries.md`.
