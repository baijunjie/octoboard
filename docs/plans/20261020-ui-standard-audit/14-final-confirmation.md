# 14 Final confirmation

> Goal: confirm in Playwright WebKit what could only be checked in headless Chromium when the tree chevrons moved to the shared button.
> Completion criteria: every item below is checked and passes, or a defect it turns up is fixed on the spot. Nothing is left beyond this milestone.

Status: **not started**. Nothing later depends on these checks.

## Scope

The Files and grouped Changes trees (gallery scenarios `files-tree` and `git-changes-tree`) were traversed in
headless Chromium with the shared Button chevron and passed. The same traversal has not been run in Playwright
WebKit: the first attempt's script could not drive keyboard focus into the page, which was a harness problem, not an
observed application failure.

## Checks

- [ ] In Playwright WebKit, over the Files tree and the grouped Changes tree in en and ar, light and dark, plus zh-Hans:
  one Tab stop for the tree, visible focus ring, Home/End, arrow traversal, Left/Right expand and fold (mirrored in RTL),
  Enter toggling, chevron pointer click, and the 24px row action.
- [ ] A pointer fold while focus is on a child row.
- [ ] Section-heading Enter and Space activation.
- [ ] Focus returns to the exact Actions button after a keyboard-opened row menu closes (Escape, and a non-destructive
  item such as Copy path). Menus opened with the Menu key or Shift+F10 return to the row by design.

## Test invocation

Use `pnpm --filter @octoboard/ui exec vitest run <explicit paths>` for focused UI tests. Record the actual command,
selected files and outcome. Use gallery fixtures only, and keep generated artifacts in a task-specific `/tmp` directory.
