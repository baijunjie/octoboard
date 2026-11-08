# 06 Conflict-checked writes

> Goal: two clients changing the same field of a record edited through a form cannot silently overwrite each other.
> Completion criteria:
> - Two clients read a setting and both change it. The second write is refused as a conflict, and that client shows
>   the current value.
> - Two clients change different fields of the same record, and both writes succeed.
> - The same holds for project settings, a console and an account's configuration.
> - A setting changed by another client after an import's preview makes the import conflict.

## Technical design

- [ ] A version per field on records edited through a form (settings, project settings, consoles, account
      configuration), carried in the snapshot and in their events. Changes made in one action, such as a rename or
      a pin, are not checked, but still advance their field's version, so a form based on the old value conflicts.
- [ ] A write to such a record carries, for each field it changes, the version it was based on. A mismatch on any of
      them refuses the whole write with a conflict error naming the current versions.
- [ ] Configuration import is a second writer of settings. Its write carries the versions its preview read, so a
      field changed elsewhere after the preview conflicts and the user previews again.

## Implementation plan

- [ ] Settings first, then the other records edited through a form.

## Notes for the developer

- **Development notes**:
  - A new error code follows the project's way of adding daemon error codes.
  - The UI shows a conflict through its existing error alerts.
  - For a record that can be deleted, deleting keeps its field versions, so an old writer cannot recreate it
    unnoticed.
- **Reference docs**: `docs/product/settings.md`, `docs/product/error-alerts.md`, `apps/daemon/PROTOCOL.md`
  (`update_settings`), `docs/product/configuration-export-and-import.md`.
