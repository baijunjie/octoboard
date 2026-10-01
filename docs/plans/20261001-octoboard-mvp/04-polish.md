# 04 Polish

> Goal: finish off the status details and the exit flow, and complete packaging, signing, and notarization so the
> application is distributable.
> Done when: the signed and notarized application bundle installs and works on a clean macOS machine (with the bundled
> daemon sidecar); the exit, crash, and interruption-recovery paths each pass a manual test.

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
