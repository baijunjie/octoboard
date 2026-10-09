# A row action button with Copy path

> Goal: each change row has an action button at its end, offering Copy path.
> Completion criteria: the button opens a menu with Copy path, which copies the change's path; the row's own action
> (opening the viewer) and keyboard navigation are unaffected; the button is reachable by keyboard and named for
> screen readers; product docs describe it.

## Technical design

- [ ] A per-row icon button opening a menu of actions; the first action is Copy path.

## Implementation plan

- [ ] Add the button and menu to change rows, then the Copy path action with its confirmation.
- [ ] Update the product docs for the Git mode.

## Notes for the developer

**Reusable capabilities**

- The application's action menu component and toasts for confirmation. The UI has no clipboard access yet; reach
  it through the platform adapter rather than calling a browser or Tauri API directly.

**Reference docs**

- `docs/product/project-pane-git-mode.md`, `docs/product/toasts.md`
