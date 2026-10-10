# 01 Lead sessions in the binding model

> Goal: the daemon can hold a project session that is bound to a console session and owns sessions of its own project
> (a lead session), and every rule that follows a binding works across the two levels.
> Completion criteria: daemon tests cover binding an unbound project session to a console session while it keeps its
> own bound sessions, the lead session's tool permissions, its reports and their refusal while its sessions are not
> archived, the synthesised report being withheld while its sessions run, a relaunched lead session's role description
> and tools, archive, reopen and delete (single and bulk) across both levels, and removing a project that holds a lead
> session; the existing orchestration tests still pass. No user-facing entry point yet.

## Technical design

- [ ] A binding operation that binds an existing unbound project session to a running console session of the same
  console. It refuses a session that is already bound, a console session, an owner that is not a console session, and
  an owner in another console. Sessions already bound to the caller keep their binding.
- [ ] A lead session is identified from the data already stored: a project session that is bound to a console session
  and is the owner of other sessions. Its `owner_kind` reads `console`; its own sessions' `owner_kind` stays `project`.
- [ ] A lead session keeps the project-scoped orchestration tools of an unbound project session (`start_session`,
  `send_message`, `get_session`, `archive_session`, `reopen_session`) and its `report` now goes to the console
  session. Sessions bound to a lead session still get no `start_session`.
- [ ] Every launch of a lead session — a resume, a reopen, a switch of account — passes the unbound project
  session's role description and tool list, as at its first launch, not those of a session bound to a console
  session.
- [ ] A session bound to an owner that is itself bound can never be an owner: the depth limit of two is enforced where
  a binding is resolved, not only by which tools are offered.
- [ ] A lead session's `done` report with no open items is refused while any session bound to it is not archived, the
  reason naming those sessions (see "A lead session follows its team's rules" in the overview).
- [ ] No synthesised report is sent for a lead session's turn while any session bound to it is running.

## Implementation plan

- [ ] Archiving a console session follows its bound sessions down to the lead session's own sessions; the refusal
  when a bound session is still running is checked at both levels, naming the sessions in the way.
- [ ] Reopening an archived session reopens its archived owners level by level upward, the topmost first: a session
  bound to an archived lead session brings back the lead session's archived console session, then the lead session,
  then itself. The walk goes up through the owners that are archived and stops at the first that is not; an owner
  that is only interrupted is not relaunched, as today. Its siblings — the other sessions bound to the same owners —
  are not reopened. If an owner cannot be reopened, the whole reopen fails with its reason; owners already brought
  back stay reopened and the rest stay archived. This extends the existing rules that reopening a bound session brings
  its owner back and that reopening an owner reopens nothing bound to it.
- [ ] Deleting an archived console session and deleting an archived lead session each take their archived bound
  sessions at both levels with them, and the counts a client is given before confirming include both levels. The bulk
  deletes (every archived console session of a console, every archived session bound to a console session) do the
  same.
- [ ] Removing a project that holds a lead session handles the lead session's binding to the console session as for
  any session bound to a console session.
- [ ] Update the product docs on orchestration and sessions (a binding being fixed from launch, one-level-deep
  orchestration, reporting, archive and reopen cascades), and the daemon's protocol docs where they describe binding.

## Notes for the developer

- **Reusable capabilities**: the coordinator's existing binding resolution, archive cascade and archived-owner reopen;
  the reporting module's owner lookup by id and its synthesised report on a turn end; the MCP catalogue's tool
  selection by role and binding, which the daemon re-checks on every call.
- **Development notes**: today an unbound project session is already offered `report` and refused; the lead session
  keeps the same tool list, so no tool-list change is needed mid-run. A relaunch, though, works its role description
  and tool list out from the binding stored at that moment, which is why every launch of a lead session needs
  handling. Keep "an owner drives only the sessions bound to it" unchanged; the console session must still be refused
  when it acts on a lead session's sessions.
- **Reference docs**: `docs/product/hub-orchestration.md`, `docs/product/sessions.md`, `docs/architecture.md`,
  `apps/daemon/PROTOCOL.md`.
