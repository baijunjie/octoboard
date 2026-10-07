# Archiving, reopening and deleting along the binding

> Goal: a console session and the sessions bound to it are archived, reopened and deleted as one group, with the line
> drawn at whether a process is running.
> Completion criteria: archiving a console session while one of its bound sessions has a process running is refused
> and the refusal names those sessions; with none running, archiving it archives its dormant bound sessions too;
> reopening one of those bound sessions brings its console session back with it, and the session's next report
> reaches it; deleting an archived console session deletes its archived bound sessions. Static checks and the test
> suite pass.

## Technical design

- [ ] **Archiving a console session is refused while any session bound to it has a process running** — whether it is
  working, awaiting instructions, or waiting for the user. The refusal says how many and names them.
- [ ] With no bound session's process running, archiving a console session also archives every bound session that is
  interrupted. Nothing bound to it is left outside the archive.
- [ ] **Reopening a bound session reopens its console session first**, then the session itself. This is only valid
  once the one-live rule is gone; before it, such a reopen was refused.
- [ ] Reopening a console session on its own does not reopen the sessions bound to it. The group is pulled back one
  session at a time, from the session the user asked for.
- [ ] A console session still cannot archive itself. The user archives it.
- [ ] **Deleting an archived console session deletes the archived sessions bound to it**, and says how many before
  doing it.
- [ ] Deleting an archived bound session on its own leaves its console session alone.
- [ ] Automatic archiving on a `done` report with no open items is unchanged: it archives the reporting session only,
  never its console session.
- [ ] The dialogs that ask for confirmation name what the cascade will take: how many sessions will be archived with
  the console session, or deleted with it.

## Implementation plan

- [ ] Add the check for a running bound process to the archive path for a console session, and have it produce the
  refusal with the offending sessions named.
- [ ] Make archiving a console session a group operation over its dormant bound sessions.
- [ ] Make reopening a session consult its binding and reopen the owner first, failing the whole reopen if the owner
  cannot be reopened.
- [ ] Make deleting an archived console session a group operation over its archived bound sessions.
- [ ] Extend the archive and delete confirmation dialogs with the counts, following the project's copy conventions
  for a destructive confirmation.

## Notes for the developer

**Reusable capabilities**

- Archiving, reopening and deleting a session all exist; this milestone groups them along the binding rather than
  adding new operations.
- The project already has a confirmation-dialog pattern for destructive actions, including one that names a count.

**Development notes**

- Whether a session's process is running is distinct from its status; the check must be on the process, since an
  interrupted session has no process but is not archived.
- Milestone 8 ends a console session's process the way archiving does, to relaunch it under another account. That
  path must not pick up the cascade this milestone adds: a switch archives nothing.
- A report for a console session with no process running is still refused, as any message for a session without a
  process is. The archive rule removes the common case but not an interrupted console session; this plan does not
  change that behaviour.

**Reference docs**

- `docs/product/sessions.md` for the statuses and their transitions, archiving and deletion
- `docs/product/hub-orchestration.md` for automatic archiving
