# 14 Final confirmation

> Goal: confirm in Playwright WebKit and the native app what could only be checked in headless Chromium: the tree chevrons on the shared button, and the shared collection motion and reduced-motion behaviour.
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

### Collection motion and reduced motion

Shared list motion now drives the agent tab strips (Settings → Agent accounts, rail quota panel), the console
dialog's Advanced agent tabs, Shell terminal tabs and the rail quota panel's account rows; authored transitions are
guarded and running script motion stops when reduced motion turns on. All of it passed in headless Chromium,
including the Shell header's one-row, title and pane-floor contract and F6 entry in RTL. Gallery scenarios with live
changes: `rail-quota-live`, `shell-panel-tabs`, `shell-panel-tab-menu-focused`, `settings-accounts`,
`dialog-new-console-advanced`, `dialog-edit-console-agents` (press the header's "Live changes" buttons, or add
`live=<id>,<id>` to the frame URL).

- [ ] In Playwright WebKit, en and ar: add, remove and reorder (drag and Move earlier/later) animate in those
  collections, with `reducedMotion: 'reduce'` from the start and flipped mid-animation (no copies left, nothing
  running).
- [ ] In Playwright WebKit: a Shell tab closed from its context menu, or removed by "Another window closes pnpm dev"
  in `shell-panel-tab-menu-focused`, hands focus to the shown tab with a visible ring; with no tab left, to New
  terminal.
- [ ] In the native app, the user toggles macOS Reduce motion by hand while a collection change, a dialog height
  change or a tab drag runs: the effect stops at its end state. This needs the user; it cannot be driven by script.

## Test invocation

Use `pnpm --filter @octoboard/ui exec vitest run <explicit paths>` for focused UI tests. Record the actual command,
selected files and outcome. Use gallery fixtures only, and keep generated artifacts in a task-specific `/tmp` directory.
