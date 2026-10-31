# Multi-client Terminal Development Plan

## Problem and approach

Several clients — the macOS application, a browser, later a phone — can operate at once with synchronised state. State
changes already broadcast to every client and terminal input already lands in the one PTY. What is missing:

- **A rule for the shared PTY's size.** An agent lays its screen out for one width, so when clients of different
  widths are attached at the same time one of them has to win. An agent redraws itself when the PTY is resized, which
  is what makes a size change work at all; the rule is only about who is allowed to ask. A client that does not attach
  a terminal — a phone showing a conversation view, say — never affects the size.
- **An exact screen on attaching.** Attaching replays a 2 MiB raw ring buffer. Once the ring has dropped its oldest
  output, the screen a client rebuilds is only approximately the program's, patched up with mode-restoring escape
  sequences.
- **Slow clients without disconnecting.** A slow terminal client is dropped after a grace window and replays
  everything on reconnecting.
- **One answer per terminal query.** Clients answer the program's terminal queries. Keeping several clients from
  answering twice, or from answering stale queries, needs the offset, settled-output and five-second rules.

## Key design decisions

- **The most recently active terminal client decides the size**, decided in the daemon, not by each client.
- **A client that attaches and then goes quiet does not keep claiming the size**; becoming active again re-applies its size.
- **Attaching does not leave a stale layout**: the client's size is applied right after what it receives on attaching
  — the replay until milestone 02 replaces it, the first paint after.
- **The daemon's parsed screen is what a client attaches to.** The first paint is the current screen serialised as
  escape sequences, optionally with history, followed by the raw live output. The paint is produced and queued at the
  same point that appends live output, so no byte is lost or repeated between them.
- **Credit-based flow control replaces dropping slow clients.** Each terminal stream starts with a credit window, and
  the client acknowledges the bytes it has consumed. A backlog past a cap is discarded and replaced with a fresh paint.
- **The daemon answers terminal queries itself** from its emulator, and removes the answered ones from what clients
  receive. Clients report their display facts (foreground, background and cursor colours, and cell size) instead of
  answering. The facts used are those of the most recently active client, in the same sense as the size rule. A query
  the emulator cannot answer passes through for a client to answer.

## Milestones

- [01 Size rule](01-size-rule.md)
- [02 Terminal first paint](02-terminal-first-paint.md)
- [03 Terminal stream flow control](03-terminal-stream-flow-control.md) — needs 02
- [04 Terminal queries answered by the daemon](04-terminal-queries-answered-by-the-daemon.md)

## Open

- What counts as "active" (input only, or output viewing as well), and the idle cutoff.
- Whether an exited shell terminal's attach gets a paint of its last screen, or keeps its ring replay.
- How a query the daemon cannot answer is answered when several clients are attached, once the client-side rules for
  answered queries are gone.
