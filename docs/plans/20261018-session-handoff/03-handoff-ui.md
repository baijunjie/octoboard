# 03 Hand off entry in the UI

> Goal: let the user hand a session off from the session row.
> Completion criteria: a session row's menu has **Hand off to…**, which opens a dialog to choose the receiving agent,
> account, model and effort; confirming runs the daemon's `hand_off_session` request, the receiving session appears in the
> original's place and is selected, and a refusal or failure shows in an error alert. Verified in the gallery's
> scenarios for the dialog and in the running app.

## Technical design

- [ ] **Hand off to…** in the session row's menu, for project and console sessions that are not archived, disabled
      while the session is working.
- [ ] A dialog choosing the receiving agent and account (only available agents other than the session's own), with
      model and effort following the chosen account's defaults, and a line saying what will move: branches, owned
      sessions, owner.
- [ ] Errors from the operation shown in the ordinary error alert.

## Implementation plan

- [ ] The menu item and the dialog, wired to the daemon operation.
- [ ] Selecting the receiving session once it appears.
- [ ] Gallery scenarios for the dialog, including a console session with owned sessions and a project session with
      branches.
- [ ] UI strings in every supported language.

## Notes for the developer

- **Reusable capabilities**: the session row menu and its **Switch account** submenu; the new-session dialog's agent,
  account, model and effort fields; error alerts.
- **Development notes**: follow the UI copy conventions; the gallery is written only through its own skill.
- **Reference docs**: `docs/product/sessions.md`, `docs/product/settings.md`, `docs/product/error-alerts.md`,
  `docs/ui-accessibility.md`.

## Handoff

- A handoff whose held messages cannot be delivered (a forward that fails, or a queue left with a receiving session
  that has no process) raises the existing `queued_messages_dropped` notice, whose wording speaks of part of a
  half-written message possibly left on the input line — wrong for this case. Give the handoff case wording of its
  own (with a reason carried by the notice, or a notice of its own).
