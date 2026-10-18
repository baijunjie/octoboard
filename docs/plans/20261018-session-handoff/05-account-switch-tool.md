# 05 Account switch tool for owners

> Goal: let an owner move a session of its own to another account of the same agent through an Octoboard tool,
> keeping the session's conversation, as Switch account does for the user.
> Completion criteria: an owner's call switches one of its own sessions to the named account and the session resumes
> its conversation there; a call on a session that is not the caller's, an account of another agent, or anything the
> daemon's account switch refuses is refused with the reason in prose and changes nothing. Verified by daemon
> integration tests from a console session and from an unbound project session.

## Technical design

- [ ] An Octoboard tool, offered to every session that can own sessions, taking the session (one of the caller's) and
      the account to move it to.
- [ ] It runs the daemon's existing account switch for that session and returns the session with its new account.
- [ ] The session itself is not told by a separate message: its conversation goes on as it was.

## Implementation plan

- [ ] The tool's declaration in the owner's tool list and role description.
- [ ] Its call routed to the daemon's account switch, with the "one of the caller's" check that the other tools acting
      on a session of the caller's own already make.

## Notes for the developer

- **Reusable capabilities**: the daemon's `switch_session_account` request and its refusals; the tool declarations
  and ownership checks of `transfer_session`, `archive_session` and `reopen_session`.
- **Development notes**: this milestone does not depend on milestones 02–04 and can be built first. A tool added to
  an owner reaches a running session only once it is next launched.
- **Reference docs**: `apps/daemon/PROTOCOL.md`, `docs/product/sessions.md`, `docs/product/settings.md`,
  `docs/product/hub-orchestration.md`.
