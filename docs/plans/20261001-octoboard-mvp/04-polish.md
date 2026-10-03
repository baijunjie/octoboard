# 04 Polish

> Goal: finish off the status details and the exit flow, and complete packaging, signing, and notarization so the
> application is distributable.
> Done when: the signed and notarized application bundle installs and works on a clean macOS machine (with the bundled
> daemon sidecar); the exit, crash, and interruption-recovery paths each pass a manual test.

## Handover

Two items from the shell, both marked `TODO(milestone 04)` in the code.

- **The Dock's Quit bypasses the exit confirmation.** Cmd+Q and the application menu go through the application's own
  menu item and do reach it, but the Dock sends `terminate:` straight to the process, and nothing in the Tauri stack
  turns that into an event the window can answer. Catching it needs an `applicationShouldTerminate:` override on the
  application delegate (`TODO` in `app/src-tauri/src/lib.rs`, where the menu is built).
- **Codex's hooks run under `--dangerously-bypass-hook-trust`**, which costs two warning lines in the session's own
  output on every launch. The warning-free alternative is seeding `hooks.state` with the trust hashes, which have to be
  captured once per Codex version and shipped with the adapter (`TODO` in `adapter/codex.rs`).

## Implementation

- [ ] Status details: the edge cases of each state's icon, bubbling, notifications, and Dock count
- [ ] Exit flow: the confirmation dialog, terminating sessions and the daemon, no orphan processes after a crash
- [ ] Packaging: the daemon binary bundled with the application and signed and notarized along with it, so Gatekeeper does
  not block it
- [ ] PATH and environment variables are complete for agents when the application is launched from Finder

## Notes for developers

- **Key points**: background operation and remote hosts are out of scope this round, but the architectural allowances for
  them — such as the daemon's events being replayable by sequence number — must not be broken.
- **Reference**: `docs/mvp.md` sections 4.4 and 8.
