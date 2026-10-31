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
- **Reference docs**: `apps/daemon/PROTOCOL.md` ("Answers to the program's queries" under the terminal stream).
