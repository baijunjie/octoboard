# 04 Conflict-checked writes

> Goal: two clients changing the same setting cannot silently overwrite each other.
> Completion criteria: two clients read a setting and both change it. The second write is refused as a conflict, and
> that client shows the current value.

## Technical design

- [ ] A version on conflict-checked settings, carried in the snapshot and in settings events. The granularity is open
      (see the overview).
- [ ] A settings write carries the version it was based on. A mismatch is refused with a conflict error naming the
      current version.

## Implementation plan

- [ ] Settings first, then other records as decided.

## Notes for the developer

- **Development notes**:
  - A new error code follows the project's way of adding daemon error codes.
  - The UI shows a conflict through its existing error alerts.
  - For a record that can be deleted, deleting keeps the version, so an old writer cannot recreate it unnoticed.
- **Reference docs**: `docs/product/settings.md`, `docs/product/error-alerts.md`, `apps/daemon/PROTOCOL.md`
  (`update_settings`).
