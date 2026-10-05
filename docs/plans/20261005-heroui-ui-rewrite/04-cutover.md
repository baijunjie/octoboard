# 04 Cutover

> Goal: the macOS application runs the new UI and the old UI code is gone, so that polishing continues on the new UI.
> Completion criteria: the desktop application builds and launches with the new UI and does everything the product docs
> describe, checked on a real build; no UI code of the old one, nor a dependency only it used, remains in the desktop
> application.

## Technical design

- [ ] The desktop shell loads the new UI package.
- [ ] The old UI and the dependencies only it used are removed.

## Implementation plan

- [ ] Switch the shell over, run the whole product-doc parity pass on a real build, then delete the old UI code.
- [ ] Record, as the starting list for polish, whatever the parity pass found that was not a regression.

## Handoff

- The adapter's Tauri implementation (`packages/ui/src/platform/tauri.ts`) and the exit flow on top of it
  (`useAppExit`) have only been type-checked and built, never run in the shell. On the real build, verify:
  - the whole quit sequence: confirmation while sessions are live, `shutdown` to the daemon, then `confirm_quit`, for
    the close button, Cmd+Q, the app menu and the Dock's Quit;
  - the daemon-exited toast;
  - the raised-hand system notification and the Dock badge;
  - the `?port=` and `?error=` hand-over from the shell.

## Notes for the developer

- **Development notes**: this is the milestone that needs a real build; do not delete the old UI before the parity pass
  is done. Parity is the only gate: do not hold the cutover back for polish, which resumes on the new UI afterwards.
- **Reference docs**: `docs/memory/verifying-the-desktop-ui.md`, `docs/product/application-lifecycle.md`.
