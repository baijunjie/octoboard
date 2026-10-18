# 04 Handoff tool for owners

> Goal: let an owner hand off a session of its own to another agent through an Octoboard tool, without the user.
> Completion criteria: an owner's `handoff_session` call runs the daemon's `hand_off_session` operation for one of its own
> sessions and returns the receiving session; a call on a session that is not the caller's, or that the operation
> would refuse, is refused with the reason in prose and changes nothing. Verified by daemon integration tests from a
> console session and from an unbound project session.

## Technical design

- [ ] `handoff_session`, offered to every session that can own sessions (console sessions, unbound project sessions,
      lead sessions), with the arguments: the session to hand off (one of the caller's), the receiving agent and
      account, optional model and effort, and an optional note for the receiving session.
- [ ] It returns the receiving session's id, and what moved: branches, owned sessions.
- [ ] The receiving session stays the caller's; the caller is not sent a separate handover message for it, since it
      made the call.

## Implementation plan

- [ ] The tool's declaration in the owner's tool list and role description.
- [ ] Its call routed to the daemon operation, with the "one of the caller's" check that the other tools acting on
      a session of the caller's own already make.

## Notes for the developer

- **Reusable capabilities**: the tool declarations and ownership checks of `transfer_session`, `archive_session` and
  `reopen_session`; the role descriptions injected on every launch.
- **Development notes**: a tool added to an owner reaches a running session only once it is next launched, as any
  tool Octoboard gains does.
- **Reference docs**: `docs/product/hub-orchestration.md`, `docs/product/launching-agents.md`.
