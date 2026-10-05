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

## Notes for the developer

- **Reusable capabilities**: the daemon's directory listing (the picker needs no native dialog); the existing sandboxing
  rules of the report page.
- **Development notes**: the report page is untrusted content; carry its isolation over exactly.
- **Reference docs**: `docs/product/report-panel.md`, `docs/product/launching-agents.md`, `docs/product/hub-orchestration.md`.
