# Multi-client Terminal Development Plan

## Problem and approach

Several clients — the macOS application, a browser, later a phone — can operate at once with synchronised state. State
changes already broadcast to every client and terminal input already lands in the one PTY. What is missing:

- **A rule for the shared PTY's size.** An agent lays its screen out for one width, so when clients of different
  widths are attached at the same time one of them has to win. An agent redraws itself when the PTY is resized, which
  is what makes a size change work at all; the rule is only about who is allowed to ask. A client that does not attach
  a terminal — a phone showing a conversation view, say — never affects the size.
- **Slow clients without disconnecting.** A slow terminal client is dropped after a grace window and is painted
  afresh on reconnecting.
- **One answer per terminal query.** Clients answer the program's terminal queries. Keeping several clients from
  answering twice, or from answering stale queries, needs the offset, settled-output and five-second rules.

## Key design decisions

- **The most recently active terminal client decides the size**, decided in the daemon, not by each client.
- **A client that attaches and then goes quiet does not keep claiming the size**; becoming active again re-applies its size.
- **What counts as active**: attaching, sending input (the terminal's own reports, such as focus, pointer motion and
  wheel, excepted), and an explicit `activate` frame a client sends when the person turns to its terminal: selects
  its session, clicks it, comes back to its window while it is shown, or changes the layout of the window they work in.
  A terminal's own focus reporting, a reconnect in the background, a resize in a window in the background, a display
  report and output do not count, and viewing output cannot: the daemon cannot tell whether anyone looks at a client.
  Attaching is passive and takes precedence for 30 seconds only, a chosen default rather than a measured idle time:
  past that, a client that has not been used since loses its precedence to every client used, and while none has
  been used the latest attach stays the fallback. Input and activation never lapse. The lapse is settled at one
  deadline taken at the attach, outside the connection's output loop. Each client's latest requested size is kept,
  so becoming the most recently active client re-applies it without the client sending it again; a size asked for by
  any other client waits. The size and the display facts are brought up to date together, each ranked among the
  clients that have reported one.
- **Attaching does not leave a stale layout**: the client's size is applied right after the first paint it receives
  on attaching.
- **The daemon's parsed screen is what a client attaches to.** The first paint is the current screen serialised as
  escape sequences, optionally with history, followed by the raw live output. The paint is produced and queued at the
  same point that appends live output, so no byte is lost or repeated between them.
- **An exited shell terminal's attach is a paint of its last screen**, not a replay of its raw output: the parsed
  screen outlives the shell, and a paint shows it at any width with the scrollback asked for, where raw output laid
  out for the terminal's own width garbles at another.
- **Credit-based flow control replaces dropping slow clients.** Each terminal stream starts with a credit window, and
  the client acknowledges the bytes it has consumed. A backlog past a cap is discarded and replaced with a fresh paint.
- **The daemon answers terminal queries itself** from its emulator, and removes the answered ones from what clients
  receive. Clients report their display facts (foreground, background and cursor colours, and cell size) instead of
  answering. The facts used are those of the most recently active client, in the same sense as the size rule. A query
  the emulator cannot answer passes through for a client to answer, under the existing reply rules (the first
  client's answer to a point, the offset and settled-output rules, the five-second window).

## Milestones

- [01 Size rule](01-size-rule.md)
- 02 Terminal first paint (closed)
- [03 Terminal stream flow control](03-terminal-stream-flow-control.md)
- [04 Terminal queries answered by the daemon](04-terminal-queries-answered-by-the-daemon.md)
