> Severity: Minor

# Raw error text is written into the visible sentence of error messages

## Symptom

Several error messages put the system's, the daemon's or Git's raw text inside the application's own sentence instead
of keeping the sentence in the application's words and showing the raw text on its own surface behind Details.

## Reproduction steps

1. In `packages/ui`, run `pnpm gallery --port <free port>`.
2. Open `http://localhost:<port>/gallery.html?scenario=configuration-export-save-failed&theme=light&lang=zh-Hans`
   (or `lang=en`).
3. Read the error alert: its message is 「无法保存文件：Permission denied (os error 13)」 ("Could not save the file:
   Permission denied (os error 13)"), with the raw system text inside the sentence and no Details.

## Expected vs. actual

- Expected: the alert's visible copy is only the application's own words, and the raw text ("Permission denied (os
  error 13)") is shown through `DiagnosticBlock` behind the alert's Details. This rests on `docs/ui-accessibility.md`,
  "Errors shown among other content" (raw text is never shown outside Details; a catalog message's visible summary and
  remedy carry no raw parameter such as `{detail}`), and on the rule the user confirmed on 2026-10-25 that raw error
  text always goes through `DiagnosticBlock`, with no exceptions (`docs/memory/writing-ui-dialogs-and-fields.md`, "Raw
  error text is `DiagnosticBlock`, never a container written for the occasion").
- Actual: the raw text is interpolated into the message through a `{detail}` parameter and shown as part of the
  sentence.

## Environment

`main` after the in-app updater branch (`feat/app-updater`) merges; the gallery in any browser; also the desktop app
when exporting the configuration to a folder that cannot be written.

## Scope of impact

Every user who hits one of these failures sees untranslated system text mid-sentence in an otherwise localized
message. Nothing is blocked; the alert is still readable and dismissible.

## Leads

- Verified: `packages/ui/src/configuration/failureText.ts` `fileFailureText` passes the error's message as
  `{detail}` to `configuration.export.failed` / `configuration.import.readFailed`
  (en: "Could not save the file: {detail}").
- Verified: `packages/ui/src/gitStatusLabel.ts` passes the Git status error as `{error}` to `sidebar.git.error`
  ("{git} error: {error}") for a plain-text label.
- Inferred, not checked one by one: these catalog messages also take a raw parameter in their visible copy, and some
  may already be split into sentence and details by `DaemonErrorMessage` / `failureParam` (for example
  `browser.tree.error` goes through `failureParam`): `git.branch.branchesError {message}`, `browser.tree.error
  {message}`, `viewer.expand.failed {reason}`, `viewer.change.contentFailed {reason}`, `branchCleanup.failed
  {detail}`, `configuration.import.result.settings.failed {reason}`, and many `daemon.*` messages
  (`unreadable_request`, `internal_error`, `project_directory_trash_failed`, `git_clone_failed`, `login_failed`,
  `launch_failed`, the trust messages with `{reason}`).
- Verified: the quit, restart and daemon-exited alerts, the crash alert and the daemon-failed startup alert already
  follow the rule (`packages/ui/src/lifecycle/exitFailure.tsx`, `packages/ui/src/StartupScreen.tsx`).

## Acceptance criteria

- [ ] In `configuration-export-save-failed`, the alert's visible copy contains no raw system text, and "Permission
      denied (os error 13)" is shown in a `DiagnosticBlock` behind Details.
- [ ] Every catalog message listed under Leads has been checked; each one whose visible copy shows raw text when
      rendered now keeps only the application's words visible, with the raw text in a `DiagnosticBlock` behind
      Details (or in the place's own Details where it is not an alert).
- [ ] No `en` or `zh-Hans` message that the UI renders as visible copy takes a raw system, daemon or Git text
      parameter, except references the copy names on purpose (paths, branch names, URLs).
- [ ] The gallery scenarios showing these failures describe the new layout and pass the quick check.
