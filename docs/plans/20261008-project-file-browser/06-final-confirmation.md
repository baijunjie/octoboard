# Final confirmation

> Goal: run the checks that could not be run when their milestone closed and that no later milestone depends on.
> Completion criteria: every item below is checked; a defect found is fixed here.

## Checks

- [ ] Run the daemon's browse tests on Linux. The `openat2` (`RESOLVE_NO_SYMLINKS`) open, the descriptor-based
  directory listing and the fallback when `openat2` returns `ENOSYS` or `EPERM` have only been type-checked against
  `libc` for `x86_64-unknown-linux-gnu`, never run.
- [ ] In the desktop app, close the window into the background for several minutes while sessions change status,
  then bring it back. Every control-socket send now has a 30 s write deadline; the window must come back connected,
  without a reconnect or Retry screen.
- In the packaged app, with the file viewer open, check by hand what was only checked in WebKit with scripted real
  input: selecting and copying code with the mouse and keyboard, Tab order through the viewer's controls, a narrow
  window, and the controls under a right-to-left language with code and paths still left-to-right.
