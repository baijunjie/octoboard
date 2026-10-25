# 05 Settings UI

> Goal: the user sees and changes the worktree settings globally in Settings and per project in the project dialog.
> Completion criteria: Settings has a **Worktrees** section with the four items and their defaults; the project dialog
> offers each item as following the global value or its own; both are translated into every offered language and shown
> in the gallery; UI tests pass.

## Handoff

- The wire records already carry the settings, but the client does not know them yet: add `worktrees` to `Settings`
  and `Project`, and to the `update_settings` / `update_project` requests, in `packages/ui/src/protocol.ts` (and the
  gallery fixtures that build those records).
- The import review's apply-settings note (`packages/ui/src/configuration/ImportReview.tsx`) and the import result
  (`ImportResult.tsx`) get a line for the worktree settings, shown only when the document carries them
  (`InspectedSettings.worktrees` / `current_worktrees`).
- `add_project` takes no `worktrees`, so a new project follows every global value; if the project dialog offers the
  Worktrees group while adding a project, `add_project` (control request and the agents' tool) has to take it too.

## Technical design

- [ ] Settings → **Worktrees**: two switches (use worktrees, rebase before merging), the delivery choice (ask / merge /
      pull request), and the revert gate switch.
- [ ] The row text says a rule change reaches sessions begun after it, while the revert gate switch takes effect at
      once, and that a project's own rules or own installed gate come first.
- [ ] Project dialog: a Worktrees group with each item as "follow global" or a value of its own.

## Notes for the developer

- **Reusable capabilities**: the Settings dialog's sections and switch rows (as **Automatically pull repositories**);
  the project dialog's editable fields.
- **Development notes**: copy follows the `knowledge-i18n-copy` skill; gallery scenarios go through the
  `agent-gallery` skill; the accessibility standard applies to the new controls.
- **Reference docs**: `docs/product/settings.md`, `docs/product/projects.md`, `docs/ui-accessibility.md`.
