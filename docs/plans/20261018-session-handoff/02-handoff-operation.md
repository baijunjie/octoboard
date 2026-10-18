# 02 Handoff operation

> Goal: one daemon operation that hands a session off to a new session of a chosen agent and account, moving its
> place in Octoboard to it.
> Completion criteria: after a handoff, the receiving session runs with the package in its brief; it has the
> original's kind, project or console, role and owner; the original's branches name it as responsible; every session
> the original owned is bound to it; the sessions concerned and the owner have been told; the original is archived.
> Verified by daemon integration tests for an unbound project session with branches and sessions, a lead session, a
> bound session and a console session.

## Technical design

- [ ] A handoff request: the session to hand off, the receiving agent and account, and optional model and effort.
- [ ] Validation before anything changes, with the reason in prose: the session exists, is not archived and is not
      working (in the middle of a turn); the receiving agent is not the session's own agent, the refusal pointing to
      switching the account instead; the agent is available and the account belongs to it; the model and effort are
      ones the agent offers.
- [ ] The receiving session's brief carries the handoff package (the daemon's `handoff` module) — its path and the instruction to read it
      first — together with the original's brief.
- [ ] What moves to the receiving session:
  - the original's owner binding, keeping its role (unbound, lead or bound);
  - the responsible-session record of every branch the original is responsible for;
  - the binding of every session the original owns, archived ones included.
- [ ] Who is told, through the ordinary message delivery: each moved session, that its owner is now the receiving
      session; the owner, that the original was handed off to the receiving session and why; the receiving session,
      what it took over.

## Implementation plan

- [ ] Build the package, then start the receiving session with it; nothing moves if the start fails.
- [ ] Once it has started, move the binding, the branches and the owned sessions together, then send the messages.
- [ ] Archive the original last, keeping the screen as archiving does.

## Notes for the developer

- **Reusable capabilities**: session starting with a brief (as `new_session` and session requests do); the
  handing of branches in an approved session request; `transfer_session`'s owner change and its two messages; archiving
  and its preservation of the screen; message delivery queued while a session cannot take a message.
- **Development notes**: the owner rules of "Who can own a session" still hold — the receiving session takes the
  original's role, so a lead session's sessions stay ordinary bound sessions and no third level appears. Moving the
  owned sessions is new: `transfer_session` deliberately leaves them where they are, so this is a separate step,
  not a change to it. Archiving the original must not archive the sessions it owned; they have moved before it.
- **Reference docs**: `docs/product/hub-orchestration.md`, `docs/product/session-requests.md`,
  `docs/product/sessions.md`, `docs/product/message-delivery.md`, `docs/product/branch-responsible-session.md`.

## Handoff

- Remove the package file when the receiving session is deleted (`handoff::remove` with the package's key); the
  file is written by `handoff::prepare` under a key the caller chooses, so pick a key that exists before the
  receiving session is started, and remove the file if the start fails.
- Pass the original session's brief to `handoff::build` / `handoff::prepare`: a session does not store its brief, so
  find where it can be had (otherwise the package's brief section stays empty; the first user message usually holds
  the same text).
- Once the operation calls the `handoff` module, remove its `#![allow(dead_code)]` and TODO, and the "Nothing calls
  it yet" sentence in the `src/handoff/` row of the daemon's README.
