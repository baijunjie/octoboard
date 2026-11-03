# 02 Marking children idle after their last report

> Goal: an owner reading its sessions can tell a child that is idle after its last delivered report from one awaiting
> instructions.
> Completion criteria: the session records `list_sessions` and `get_session` give carry a field that is set while a
> session with a parent is idle and its last report — its own or synthesised — has been delivered to the owner since
> its last turn, and is clear otherwise (working, a new instruction delivered, background work running, archived);
> covered by daemon tests and documented in the session-record docs and `PROTOCOL.md`.

## Technical design

- [ ] One new field on session records (name and shape open, see the overview), additive to the records the session
      tools and the control protocol already give.
- [ ] The idle judgement, shared with milestone 03: the session is at its prompt, has a parent, its last turn's report
      was accepted for the owner, and nothing has started a turn since. The exact facts it excludes on (paused,
      background tasks, sessions under it running, queued messages, waiting for the user) are open in the overview.
- [ ] A report that was written but left unsent in the owner's input does not count as delivered.

## Implementation plan

- [ ] Record, per session, that its last turn's report (own or synthesised) was accepted for its owner, and clear it
      when the session's next turn starts.
- [ ] Derive the field from that record and the session's current status when building session records.
- [ ] Tests for each transition: own report accepted, synthesised report accepted, new instruction, background work,
      unsent report.
- [ ] Document the field in "Session records" in `docs/product/what-every-session-reads.md` and in `PROTOCOL.md`.

## Notes for the developer

- **Reusable capabilities**: the turn bookkeeping that already decides whether a report has to be synthesised; the
  delivery outcomes (delivered, queued, kept, left unsent) the reporting path already returns.
- **Development notes**: wire additions follow `PROTOCOL.md`'s "Absent, null and given" and "Naming" conventions and
  regenerate the UI's generated protocol types rather than editing them by hand.
- **Reference docs**: `docs/product/what-every-session-reads.md`, `docs/product/session-statuses.md`,
  `docs/product/reporting-to-owners.md`, `apps/daemon/PROTOCOL.md`.
