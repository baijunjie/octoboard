# 03 Lead sessions and their teams in the sidebar

> Goal: the sidebar shows which sessions belong to a project session, a lead session included, and the lead
> session's binding to its console session, so the user can find a team and the sessions that hold up an archive.
> Completion criteria: in the app, a project's sessions bound to a project session are listed under it as a third
> level, the lead session carries the binding badge of a session bound to a console session, focus mode follows "A
> team goes where its owner goes" in the overview, and keyboard focus on rows and the archive view handle the nesting;
> UI tests cover the ordering.
> Depends on 01 and 02.

## Handoff

- A console session's archive (the archive view's console-session scope and its focus mode's archived list) already
  includes a lead session's archived sessions, through `boundArchivedSessions` in `packages/ui/src/sidebar/order.ts`,
  but lists them flat among the console session's own; grouping them under the lead session is this milestone's
  "Archive view" item. Remove the `TODO` on `ArchiveView` once they are grouped.
- The archive and delete confirmations in `packages/ui/src/dialogs/RequestedDialog.tsx` already count and list both
  levels (`boundArchivedSessions`, `boundSessionsArchivedWith`); keep them consistent with the nesting.
- After an approved console session request, the approving window follows into the new console session's focus mode
  only when it was in the focus mode of the requesting session's own project (`followsStartedConsoleSession` in
  `packages/ui/src/sidebar/focus.ts`, described in `docs/product/focus-mode.md`). Where focus mode lists the lead
  session and its team must stay consistent with that.

## Technical design

- [ ] In a project's session list, sessions bound to a project session are nested under that session, one level below
  it. Sessions bound to a console session, and unbound ones, stay where they are today.
- [ ] A lead session carries the binding badge of a session bound to a console session.

## Implementation plan

- [ ] Ordering, pinning and folding of the nested rows, consistent with the existing project and session order.
- [ ] Keyboard focus and screen-reader structure of the third level.
- [ ] Focus mode of a project: its unbound sessions, each with the sessions bound to it nested under it; a lead
  session and its team are not listed there; the count and chip of sessions bound to its console session include the
  team. Its list of archived sessions nests the sessions bound to a project session under that session.
- [ ] Focus mode of a console session: the lead session appears among the console session's bound sessions, with its
  own sessions nested under it rather than listed as the console session's.
- [ ] Archive view, and a console session's focus mode's list of archived sessions: a lead session's archived
  sessions grouped under it.
- [ ] Update the product docs on the sidebar and focus mode.

## Notes for the developer

- **Reusable capabilities**: the sidebar's existing owners map, binding badge and row ordering; focus mode's bound
  session chips; the archive view's grouping of bound archived sessions.
- **Development notes**: today the binding badge, the owners map and focus mode's chips cover console-session owners
  only, so a session bound to a project session shows as a plain session.
- **Reference docs**: `docs/product/sidebar.md`, `docs/product/focus-mode.md`, `docs/product/labels-and-tooltips.md`.
