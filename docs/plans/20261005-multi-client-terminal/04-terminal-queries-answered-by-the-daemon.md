# 04 Terminal queries answered by the daemon

> Goal: a program's terminal queries are answered once, by the daemon, whether any client is attached or not.
> Completion criteria:
> - With no client attached, a program asking for the cursor position, its colours or the device attributes gets an
>   answer.
> - With several clients attached, each of those queries is answered exactly once, and the answered queries do not
>   reach the clients.
> - The colours a program reads match the theme of the most recently active client.
> - Clients answer only the queries the daemon passes through.

## Technical design

- [ ] Answers come from the daemon's terminal emulator, written to the program as input.
- [ ] Answered queries are removed from the stream clients receive. A query the emulator cannot answer passes through
      for a client to answer.
- [ ] A display-facts frame from the client: foreground, background and cursor colours, and cell size, sent on attach
      and when they change.
- [ ] The facts of the most recently active client set the emulator's display. A change of facts notifies programs
      that subscribed to colour-scheme changes.
- [ ] Defaults for the display facts while no client has reported any.

## Implementation plan

- [ ] Settle which queries the emulator answers. The daemon's current parser may not answer all of them; decide when
      starting whether to extend it or replace it.
- [ ] Add display facts to the protocol and the web client, then switch answering to the daemon.
- [ ] Reduce the client reply path and its rules to the queries passed through (see the overview's "Open").

## Notes for the developer

- **Reusable capabilities**: the daemon's continuous output parser.
- **Development notes**:
  - Focus reports remain each client's own input.
  - The per-connection activity tracking is shared with milestone 01. Whichever of the two lands first builds it, and
    the other reuses it.
  - **`TermFacts::host_answers_queries` and `TermFacts::host_waits_on_queries`** (`apps/daemon/src/platform/term/mod.rs`)
    say how the host's terminal backend treats queries. Where `host_answers_queries` is true (the system's console host
    on Windows) the host answers the device attributes, status and cursor reports itself before anything else sees
    them, so the daemon must not answer those there, or the program reads two answers. Where `host_waits_on_queries`
    is true (the console host bundled on Windows) the host forwards every query and holds the program for about
    500 ms while one goes unanswered, so whatever this milestone builds must answer promptly whether or not a client
    is attached. Both are false on macOS and Linux.
  - A narrow answer already exists for `host_waits_on_queries`: the daemon's emulator (`terminal/snapshot.rs`) keeps
    only its DA1 (`CSI ? 6 c`) and cursor-position answers; `Fan::push` (`terminal/fan.rs`) takes them under the fan's
    lock, only while no client is attached, records them in `ReplyRoute` as the answer of `ReplyRoute::DAEMON` to the
    output up to that chunk (so a client attaching later finds that output settled and a client's late answer to it
    is dropped), and a writer thread per terminal (`terminal/pty.rs`) writes them under the input lock, off the
    reader. A query that arrives while a client is attached is left to the clients. This milestone replaces that
    with answering every query it settles, attached or not, and keeps the `ReplyRoute::DAEMON` bookkeeping or its
    successor.
- **Reference docs**: `apps/daemon/PROTOCOL.md` ("Answers to the program's queries" under the terminal stream).
