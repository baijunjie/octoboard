# What a console session may see and touch

> Goal: a console session reads its whole console but acts only on the sessions bound to it, and can always tell
> which console session owns what it is looking at.
> Completion criteria: with two console sessions in one console, each lists the other's sessions and sees their
> owner; an attempt by one to message, archive or reopen a session bound to the other is refused with a reason in
> prose, and the session is untouched. Static checks and the test suite pass.

## Technical design

- [ ] Reading is console-wide. The tool that lists projects and the tool that fetches one session's record both
  return every session of the console, each carrying which console session owns it, or that it is unbound.
- [ ] Writing is restricted to the caller's own bound sessions. Sending an instruction, archiving and reopening are
  refused for a session that is unbound or bound to another console session.
- [ ] Starting a session binds it to the calling console session. A console session cannot start a session on another
  console session's behalf.
- [ ] A refusal remains a tool error carrying the reason in prose, naming who owns the session, so the model can act
  on it rather than retrying.
- [ ] The existing refusals are kept and restated: a session or project from another console, and a write aimed at
  any console session, are still refused.
- [ ] The tool descriptions tell the console session plainly that a session it does not own is somebody else's and is
  to be left alone — the wording already used for an unbound session, widened to cover another console session's.
- [ ] Reading the archive of a project lists that project's archived sessions, with their owner on each, rather than
  being filtered to the caller's own. The caller still cannot reopen one it does not own.

## Implementation plan

- [ ] Add the owner to every session record the tools hand out.
- [ ] Put the ownership check in one place that every write-side tool goes through, rather than repeating it per
  tool, and have it produce the prose refusal.
- [ ] Rewrite the tool descriptions for the widened rule.

## Notes for the developer

**Reusable capabilities**

- The tool layer already refuses cross-console access and already carries the "this session is the user's, leave it
  alone" convention for unbound sessions; both are the pattern to follow.
- Refusals already have an error-code and prose-reason mechanism; no new mechanism is needed.

**Development notes**

- Projects are named either by id or by an unambiguous name; that resolution is unchanged and stays console-wide.

**Reference docs**

- `docs/product/hub-orchestration.md`
