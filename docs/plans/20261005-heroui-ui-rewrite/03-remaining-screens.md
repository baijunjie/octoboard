# 03 Remaining screens

> Goal: everything else the current UI does is in the new UI.
> Completion criteria: every flow in the product docs works in the new UI in a browser, except what needs the native
> shell, which works through the adapter when run in it.

## Technical design

- [ ] Dialogs: console, project, session, rename, console settings, confirmation, directory picker.
- [ ] The report panel with its sandboxed page.
- [ ] Toasts, error handling, the startup and failure screens.
- [ ] Waiting-for-user notifications through the adapter.

## Implementation plan

- [ ] Build in the order of how often each is used; finish with a parity pass over the product docs.

## Handoff

- Waiting-for-user notifications already exist in `packages/ui`: `useWaitingNotifications` sits on the adapter's
  `notifications` and `badge` members and is called from the placeholder `App`. The transition logic is carried over
  unchanged. What is left for this milestone's item:
  - Wire it into the real screens.
  - Verify it in a browser, which has never been done. The browser adapter maps it to the Web Notifications API.
  - The browser adapter calls `Notification.requestPermission()` from an effect, with no user gesture. Firefox and
    Safari ignore or deny such a request, and Chrome asks again for each newly waiting session while permission is
    still undecided. Ask from a user gesture instead.
  - The browser adapter also notifies when the tab is in the foreground. Decide against `docs/product/sessions.md`
    whether it should.

## Notes for the developer

- **Reusable capabilities**: the daemon's directory listing (the picker needs no native dialog); the existing sandboxing
  rules of the report page.
- **Development notes**: the report page is untrusted content; carry its isolation over exactly.
- **Reference docs**: `docs/product/report-panel.md`, `docs/product/launching-agents.md`, `docs/product/hub-orchestration.md`.
