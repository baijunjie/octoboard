# The binding, and the end of the one-live rule

> Goal: a project session records which console session it is bound to, reports travel along that binding, and a
> console may hold any number of live console sessions.
> Completion criteria: two console sessions run in one console at the same time, each reachable and selectable; a
> project session bound to one of them has its report delivered to that one and not to the other; an unbound
> session's report is still refused for having nobody to report to. Static checks and the test suite pass.

## Technical design

- [ ] A session carries the id of the console session it is bound to, or nothing. A console session itself is never
  bound. The field is set when the session is created and does not change afterwards.
- [ ] The membership boolean is removed. Everywhere it decided "is this session part of the orchestration", the
  presence of a binding decides instead.
- [ ] A console session carries a **colour**, assigned on creation from a fixed palette: an entry not already in use
  among that console's console sessions, wrapping round only when every entry is in use. Each entry has a light and
  a dark variant, both meeting the contrast the project's appearance rules require.
- [ ] A console session carries an **ordinal** within its console, which gives it its default title. The ordinal of
  a new console session is one past the highest ever used in that console, so a title is not reused after a console
  session is archived or deleted.
- [ ] A console session's title can be renamed, as a project session's can.
- [ ] Report routing reads the reporting session's binding and delivers to that console session. The lookup that
  finds a console's one console session is deleted.
- [ ] Reporting fails, leaving the session as it was, when the reporting session is unbound, when its owner is no
  longer on record, or when it has already been wrapped up — the same three cases as today, with the first two
  restated in terms of the binding.
- [ ] The one-live-console-session rule is deleted: the claim taken before a console session is opened or reopened,
  the refusal that rule produced, and the error code it raised.
- [ ] The database schema is rewritten to this shape with no migration path. Existing local data is discarded.

## Implementation plan

- [ ] Change the stored session record and the protocol's session record together: add the binding, the colour and
  the ordinal, drop the membership boolean.
- [ ] Rewrite the schema and drop whatever migration support existed for the columns being replaced.
- [ ] Route reports by the binding, and restate the three refusal cases accordingly.
- [ ] Remove the claim machinery and the refusal path, and remove the application's pre-emptive suppression of
  requests it believed would be refused — selecting, resuming or reopening a console session while another is live
  must simply work.
- [ ] Set the binding at session creation: always to the creating console session for a session a console session
  starts; to whatever the creation request names, for one the user starts.
- [ ] Assign colour and ordinal when a console session is created, and keep the ordinal's high-water mark per
  console.

## Notes for the developer

**Reusable capabilities**

- The project already has a per-appearance colour system with contrast rules; the palette's light and dark variants
  belong in it rather than as literal values at the point of use.
- Session creation, titling and renaming already exist for project sessions; a console session's renaming reuses
  them rather than adding a parallel path.

**Development notes**

- Removing the claim removes a concurrency guard. Check that nothing else relied on it — the reason it existed was
  that reading the store and then inserting is two steps, and that reasoning may apply elsewhere.
- The stored record documents which of its columns never change; the binding joins that set and the documentation of
  it must be updated with the column.
- Reports and instructions written into a session go through the project's existing sanitizing path; this milestone
  changes only which session a report is addressed to.

**Reference docs**

- `docs/product/sessions.md`, `docs/product/hub-orchestration.md`
- `docs/architecture.md`, `apps/daemon/PROTOCOL.md`
- `docs/product/appearance.md` for the light and dark variants
