# Final confirmation

> Goal: run the checks that could not be run when their milestone closed and that no later milestone depends on.
> Completion criteria: every item below is checked; a defect found is fixed here.

## Checks

- [x] Run the daemon's tests on Linux (`cargo test -p octoboardd`), with `openat2` working and with it refused as
  `ENOSYS` and as `EPERM`; where it is refused, the link-swap test is skipped, as only the last component is held.
- [x] In the desktop app, close the window into the background for several minutes while sessions change status,
  then bring it back: it comes back connected, without a reconnect or Retry screen.
- [x] In the packaged app, with the file viewer open: selecting and copying code with the mouse and keyboard, Tab order
  through the viewer's controls, a narrow window, and the controls under a right-to-left language with code and paths
  still left-to-right.
- [x] With a Chinese input method active, the arrow keys in the file viewer still move to the previous and next file and
  no composition starts: the viewer has no editable field, so the input method passes the keys through.
- [x] In the packaged app: navigate quickly through files of a project on a slow disk or network share; delete and
  overwrite the open file while it is shown; quit and relaunch the daemon connection (reconnect) with the project pane
  open; delete the project whose pane is shown. Each shows the right file's content or an explicit state.
- [x] In the packaged app, the Git mode by hand: the worktree selector's popover, the Files/Git tabs and the change list
  under a right-to-left language, and a type change's two sections in a narrow window.
- [x] The Git mode with VoiceOver: the worktree selector's popover, the Files/Git tabs and the change list.
- [x] F6 from the terminal lands on the project pane's selected mode tab.
- [x] With Reduce motion on, the Files/Git tabs' indicator does not slide.
- [ ] The Compare view with VoiceOver: the Uncommitted/Compare tabs, the two branch selectors, Swap branches, the
  commits line and the moved/deleted notice; also whether the Git mode's loading placeholders (a `role="status"`
  inserted already holding its text, `gitStates.tsx` and `ProjectBrowser.tsx`) are announced.
- [x] With a Chinese input method active, typing a branch's first letter in a branch selector's popover still moves to
  that branch (type-ahead) and no composition starts.
- [x] The Uncommitted/Compare tabs' indicator under Reduce motion.
- [x] The Compare view in the packaged app under a right-to-left language, and its branch selectors in a narrow
  window.
