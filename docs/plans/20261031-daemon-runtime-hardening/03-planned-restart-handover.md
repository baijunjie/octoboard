# 03 Planned restart handover

> Goal: an explicit restart of the daemon hands every terminal over to its successor with its exact screen.
> Completion criteria:
> - Request a planned restart of a daemon started by hand, not following an application, while sessions write output:
>   - every session keeps running, and its screen is exactly as it was, not marked inexact;
>   - the output written during the restart appears once;
>   - the daemon answers the queries in that output.
> - A plain stop still ends every session, as today.
> - After a planned-stop request, a daemon started by hand next restores every screen exactly.
> - A daemon that follows an application refuses the planned-restart request.
> - Under a systemd user service, the planned restart's successor is started by systemd and the sessions survive.

## Technical design

- [ ] A planned-restart request on the daemon's control interface, which makes a planned stop and starts the
      successor on the same address. A daemon that follows an application refuses it.
- [ ] Under systemd the daemon does not start the successor: after its planned stop it exits with an agreed status,
      and the unit's restart rule restarts it on that status, so the successor runs in the unit. A daemon with no
      service manager starts its successor itself.
- [ ] A planned-stop request, which makes the planned stop and starts no successor. The desktop update restart of
      milestone 04 sends it.
- [ ] A `restart` command beside start, stop and status, which makes the planned-restart request.
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
  - The holders from milestone 02.
- **Development notes**: builds on daemon-side answering of terminal queries, which is planned separately and has to
  exist first. The `restart` command joins the daemon's command-line start, stop and status commands, which
  have to exist first; both requests and the planned stop do not wait on them. The systemd case needs the daemon
  running as a systemd user service, which does not exist yet. A desktop update is milestone 04.
- **Reference docs**: `docs/product/application-lifecycle.md` ("The daemon on its own"),
  `docs/product/in-app-updates.md`.
