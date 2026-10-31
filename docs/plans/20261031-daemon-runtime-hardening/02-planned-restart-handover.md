# 02 Planned restart handover

> Goal: an explicit restart of the daemon hands every terminal over to its successor with its exact screen.
> Completion criteria: run the `restart` command while sessions write output.
> - Every session keeps running, and its screen is exactly as it was, not marked inexact.
> - The output written during the restart appears once.
> - The daemon answers the queries in that output.
> - A plain stop still ends every session, as today.

## Technical design

- [ ] A `restart` command beside start, stop and status, which makes a planned stop and starts the successor.
- [ ] A planned stop:
  - The daemon tells each holder it is handing over.
  - It saves every terminal's screen, including a half-received escape sequence.
  - The holder then starts its tails afresh.
- [ ] The successor restores each saved screen and replays only the tail written since.
- [ ] Query answers produced while replaying a tail are written to the program only after a handover. After a crash
      they would arrive as stray input.

## Implementation plan

- [ ] Add the handover message and screen saving, then restore on start.
- [ ] Add the `restart` command, and route any future server-form update path through it.

## Notes for the developer

- **Reusable capabilities**:
  - The frozen-preview snapshot taken before a controlled stop.
  - The holders from milestone 01.
- **Development notes**: builds on daemon-side answering of terminal queries, which is planned separately and has to
  exist first. The `restart` command joins the daemon's command-line start, stop and status commands, which
  have to exist first. A desktop update is a quit followed by a fresh launch, and is outside this milestone while
  the desktop quit is open (see the overview).
- **Reference docs**: `docs/product/application-lifecycle.md` ("The daemon on its own"),
  `docs/product/in-app-updates.md`.
