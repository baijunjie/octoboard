# Answers to a program's terminal queries

How the queries a program puts to its terminal are answered, in a session's terminal (see
`docs/product/session-terminal.md`) and in a shell terminal of the Terminal panel (see
`docs/product/terminal-panel.md`) alike.

A program can ask its terminal about itself through its output — the cursor's position, the background colour, the
features the terminal supports — and reads the answer back as input. Prompt libraries ask this way before drawing a
prompt (`gh`'s does, for one), and a program handed an answer it is no longer waiting for reads it as typed text.
**The program gets one answer per query**, however many clients show the terminal at the same moment (another window,
a browser, a phone), and most queries are answered by Octoboard itself rather than by a client.

## Queries Octoboard answers itself

- **Octoboard answers these at once, from its own copy of the terminal, whether or not any client is attached**, and
  takes them out of the output, so no client sees them:
  - the primary and secondary device attributes;
  - the status report and the cursor position report;
  - the state of these modes: application cursor keys, origin, auto-wrap, cursor visibility, mouse reporting (1000,
    1002, 1003 and 1006), focus reporting, the alternate screen, bracketed paste, colour scheme updates (2031), insert
    and newline;
  - the text area's size in characters and in pixels, and a cell's size in pixels;
  - the foreground, background and cursor colours;
  - whether the colour scheme is dark or light (`CSI ? 996 n`, answered `CSI ? 997 ; 1 n` for dark, `2 n` for light).
- **A program that asks while nobody is watching is answered as promptly as one on screen**, so an agent's start does
  not wait on a window being open.
- **On Windows, a terminal running on the system's own console host** rather than the one Octoboard ships has the
  device attributes, the status report and the cursor position report answered by that console host, so Octoboard
  leaves those to the clients (see "Queries the clients answer" below).
- **The Program Status Protocol's support query** (`OSC 7501 ; ?`) is answered by Octoboard in every session's
  terminal, whichever its agent, on every host; in a shell terminal of the Terminal panel it reaches the clients, none of
  which answers it. See "The support query" in `docs/product/program-status-reports.md`.

## The colours and sizes a program reads

- **A program reads the colours and the cell size of the most recently active client** that has told Octoboard its
  display. Each client tells it its terminal's foreground, background and cursor colours, and a cell's size in whole
  pixels once its screen is laid out, on attaching and whenever they change: a change of the window's appearance (see
  "What follows the choice" in `docs/product/appearance.md`), or a zoom that changes a cell's size. The text area's size
  in pixels is the terminal's size in cells times that cell.
- **Which client is the most recently active one** — attaching, which counts for 30 seconds, input sent from it and
  the person turning to its terminal, but not what its terminal sends by itself — is set out in "The most recently
  active client" in `docs/product/terminals-on-several-clients.md`; the same client decides the terminal's size.
- **Until any client has told its display, and once every client that had has left**, a program reads the dark
  appearance's terminal colours (foreground `#e4e4e6`, background `#1e1f22`, cursor `#808080`) and a cell of 8 by 17
  pixels. A client that does not know its cell size is taken to have that cell.
- **Dark or light** is whether the background is darker than the foreground.
- **A colour the program set itself** is read back as the program set it.
- **A program that turned on colour scheme updates (mode 2031) is told dark or light whenever the colours it reads
  change**: the most recently active client changing its appearance, another client showing other colours becoming the
  most recently active one, or the client whose colours it read leaving. A terminal reset turns the mode off.

## Queries the clients answer

Every other query reaches every attached client, and each client's terminal answers it:

- **The first client to answer a query is the one whose answer the program gets**; the copies the other clients send
  are dropped. So is an answer to output older than output whose query has already been answered.
- **A query is answered within 5 seconds of its output or not at all.** A later answer is dropped, since a program
  waits no longer than that and would take a late answer as typed input. A client that falls that far behind, such as
  a browser tab the browser throttles in the background, has its answers dropped even when it is the only one attached.
- **Attaching does not answer old queries**: a client attaching (selecting a session, reconnecting, reopening the
  Terminal panel) starts from a first paint of the screen, which asks nothing, and answers only the queries in the
  output that follows it. Such a query asked while no client was attached gets no answer. Likewise, a client that fell
  so far behind that it was sent a fresh paint in place of the output it missed (see "Output, scrollback and size" in
  `docs/product/session-terminal.md`) answers none of the queries in that output.
- **A client's answer can arrive after Octoboard's answer to a later query**: when a program asks one of these and
  then one Octoboard answers, it reads Octoboard's answer first.
- **A client showing a read-only screen answers nothing**: an interrupted session's saved preview, a session frozen for
  archiving, and a shell terminal whose shell has exited.
- **A report that the terminal gained or lost keyboard focus is not an answer**: each client's report reaches the
  program as that client's own input.
