# 06 Conflict-checked writes

> Goal: two clients changing the same field of a record edited through a form cannot silently overwrite each other.
> Completion criteria:
> - Two clients read a setting and both change it. The second write is refused as a conflict, and that client shows
>   the current value.
> - Two clients change different fields of the same record, and both writes succeed.
> - The same holds for project settings, a console and an account's configuration.
> - A setting changed by another client after an import's preview makes the import conflict.

## Technical design

- [x] A version per field on records edited through a form (settings, project settings, consoles, account
      configuration), carried in the snapshot and in their events. Changes made in one action, such as a rename or
      a pin, are not checked, but still advance their field's version, so a form based on the old value conflicts.
- [x] A write to such a record carries, for each field it changes, the version it was based on. A mismatch on any of
      them refuses the whole write with a conflict error naming the current versions.
- [x] Configuration import is a second writer of settings. Its write carries the versions its preview read, so a
      field changed elsewhere after the preview conflicts and the user previews again.

## Implementation plan

- [x] Settings first, then the other records edited through a form.

## Notes for the developer

- **Development notes**:
  - A new error code follows the project's way of adding daemon error codes.
  - The UI shows a conflict through its existing error alerts.
  - For a record that can be deleted, deleting keeps its field versions, so an old writer cannot recreate it
    unnoticed.
- **Reference docs**: `docs/product/settings.md`, `docs/product/error-alerts.md`, `apps/daemon/PROTOCOL.md`
  (`update_settings`), `docs/product/configuration-export-and-import.md`.

## Mid-task handoff

- **How far it got**: everything in "Technical design" and "Implementation plan" is implemented on branch
  `hold/conflict-checked-writes` (daemon, UI, product docs, project map, UI standard). The boxes are ticked: the code compiles and passes
  clippy on macOS and the Windows cross-check, a first full Rust test run passed the daemon tests that cover them
  (`coordinator::checked_write_tests`, `state::field_versions`, `config_document::import::tests::settings`; the one
  import test that failed there was fixed and passes alone), and the full UI suite passes. The Rust tests remain to be
  rerun after the fixes that followed that run; the gallery sweep and the unverified items below are still open.
- **Condition to continue**: compiling and testing allowed again. Then run, in order: `cargo check`/`clippy` of the
  daemon crate and fix what the uncompiled commits broke; the daemon tests `coordinator::checked_write_tests`,
  `coordinator::settings`, `state::field_versions`, `config_document::import`, `server::control`, then the whole
  daemon suite (`coordinator::checked_write_tests::a_checked_console_write_that_waits_on_a_model_check_*` relies on an
  unchecked rename never yielding on the current-thread runtime; if it is flaky, rework the staging rather than drop
  it); the full UI suite (`pnpm --filter @octoboard/ui test`); the full gallery sweep (the shared fixture
  `snapshotState` changed); `pnpm verify`. Then tick the boxes against the completion criteria (each is covered by a
  daemon test in `checked_write_tests.rs` or `config_document/import/tests/settings.rs`, and the UI side by the
  dialog, settings and import tests) and close the milestone.
- **Settled during the work**:
  - By the user: a `write_conflict` refusal resets each conflicting dialog field to the current value, keeping the
    other edits; a change elsewhere reaching an open form replaces that field at once, edited or not (no submit-time
    alert for a conflict the window can foresee), since multi-client state sync needs the same. The UI standard's
    "keeps its input" rule gained that exception rather than being wrong: it was written for refusals where the input
    stays valid.
  - By the dispatcher, from the plan and the conventions: versions live in memory, seeded from the wall clock per
    daemon run (a restart makes every earlier version stale; nothing is persisted); bumping happens at publication by
    diffing the published values, so single actions advance versions with no per-site work; unchecked writers (agents'
    tools, single actions) take no lock; an account repoint refused after its wait may already have turned sharing off
    (documented in `PROTOCOL.md`); a version-only move re-bases a form without resetting the edit; replacements of a
    value the user edited are announced naming the fields, with no visible mark, and not in the refusal path; the
    import re-reads the file automatically when settings change during the review, keeping choices, with Import
    pending meanwhile, and a failed re-read is an Alert with Try again (or Choose file) while Import is unavailable;
    the import's conflict copy is the window's own, not the daemon code's; the result view's remedy stays text (no
    generic "open Settings" action exists there); the fixture daemon neither advances nor enforces versions.
- **Unverified, to look at**: no gallery scenario shows the new states and none has been seen rendered: the
  `write_conflict` alert's copy (its layout matches `error-unknown-session-known`), a dialog field replaced while open,
  the import review's "Could not read the file again" Alert with the unavailable Import, and the result view's
  conflict Alert. The translations of the new keys in the 16 non-English catalogs are the implementers' drafts and
  want a native read. The announcements have had no VoiceOver listening pass (`docs/voiceover-cases.md` lists the new
  callers as not heard).
