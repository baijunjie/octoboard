# 03 Lead sessions and their teams in the sidebar

> Goal: the sidebar shows which sessions belong to a project session, a lead session included, and the lead
> session's binding to its console session, so the user can find a team and the sessions that hold up an archive.
> Completion criteria: in the app, a project's sessions bound to a project session are listed under it as a third
> level, the lead session carries the binding badge of a session bound to a console session, and a console session's
> focus mode, keyboard focus on rows and the archive view handle the nesting; UI tests cover the ordering. A project's
> focus mode is outside these criteria until its open point (see the overview) is settled.
> Depends on 01 and 02.

## Technical design

- [ ] In a project's session list, sessions bound to a project session are nested under that session, one level below
  it. Sessions bound to a console session, and unbound ones, stay where they are today.
- [ ] A lead session carries the binding badge of a session bound to a console session.

## Implementation plan

- [ ] Ordering, pinning and folding of the nested rows, consistent with the existing project and session order.
- [ ] Keyboard focus and screen-reader structure of the third level.
- [ ] Focus mode of a console session: the lead session appears among the console session's bound sessions; its own
  sessions are not shown as the console session's.
- [ ] Archive view: a lead session's archived sessions grouped under it.
- [ ] Update the product docs on the sidebar and focus mode.

## Notes for the developer

- **Reusable capabilities**: the sidebar's existing owners map, binding badge and row ordering; focus mode's bound
  session chips; the archive view's grouping of bound archived sessions.
- **Development notes**: today the binding badge, the owners map and focus mode's chips cover console-session owners
  only, so a session bound to a project session shows as a plain session.
- **Reference docs**: `docs/product/sidebar.md`, `docs/product/focus-mode.md`, `docs/product/labels-and-tooltips.md`.
