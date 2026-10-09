# Final confirmation

> Goal: run the checks that could not be run when their milestone closed and that no later milestone depends on.
> Completion criteria: every item below is checked; a defect found is fixed here.

## Checks

- [ ] Run the daemon's tests on Linux (`cargo test -p octoboardd`). The `openat2` (`RESOLVE_NO_SYMLINKS`) open, the
  descriptor-based directory listing and the fallback when `openat2` returns `ENOSYS` or `EPERM` have only been
  type-checked against `libc` for `x86_64-unknown-linux-gnu`; the browse and change tests have only run on macOS.
- [ ] In the desktop app, close the window into the background for several minutes while sessions change status,
  then bring it back. Every control-socket send now has a 30 s write deadline; the window must come back connected,
  without a reconnect or Retry screen.
- [ ] In the packaged app, with the file viewer open, check by hand what was only checked in WebKit with scripted real
  input: selecting and copying code with the mouse and keyboard, Tab order through the viewer's controls, a narrow
  window, and the controls under a right-to-left language with code and paths still left-to-right.
- [ ] With an input method (e.g. Chinese or Japanese), compose text while the file viewer is open: the arrow keys must
  move within the composition, not to another file.
- [ ] In the packaged app (these ran only in WebKit against a real daemon): navigate quickly through files of a project
  on a slow disk or network share; delete and overwrite the open file while it is shown; quit and relaunch the daemon
  connection (reconnect) with the project pane open; delete the project whose pane is shown. Each must show the right
  file's content or an explicit state, never another file's body.
- [ ] In the packaged app, the Git mode by hand: the worktree selector's popover, the Files/Git tabs and the change list
  with VoiceOver and under a right-to-left language, and a type change's two sections in a narrow window.
- [ ] F6 from the terminal lands on the project pane's selected mode tab (`docs/product/moving-focus-between-regions.md`
  says so from reading the code; not run).
- [ ] With Reduce motion on, the Files/Git tabs' indicator does not slide.
- [ ] The Compare view with VoiceOver: the Uncommitted/Compare tabs, the two branch selectors, Swap branches, the
  commits line and the moved/deleted notice; also whether the Git mode's loading placeholders (a `role="status"`
  inserted already holding its text, `gitStates.tsx` and `ProjectBrowser.tsx`) are announced.
- [ ] Type a branch's first letters in a branch selector's popover with an input method composing (type-ahead), and
  check the Uncommitted/Compare tabs' indicator under Reduce motion.
- [ ] The Compare view in the packaged app under a right-to-left language, and its branch selectors in a narrow
  window.
