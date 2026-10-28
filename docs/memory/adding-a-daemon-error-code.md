# Adding a daemon error code

## A new daemon error code is classified in the UI in the same change, or the window calls it a bug

The window classifies a daemon error by its code (`classifyError` in `packages/ui/src/errorRecovery.ts`), and for a few
codes by their `cause` param as well. If the code is not in that file's `GROUPS`, the window shows it as an unexpected
failure. That alert says the failure may be a bug and asks the user to file an Issue, even when the refusal is
deliberate and the user can fix the cause. No type check or test links the codes the daemon returns to that table, so
nothing fails when a new code is missing from it. So when daemon code starts returning a new error code, the same change
adds, in `packages/ui`:

- `daemon.<code>` in every message catalog under `packages/ui/src/i18n/messages/`. It says what happened, and no
  remedy beyond a code-specific step as set out below. Its params are listed in `packages/ui/src/daemonMessage.ts`. A
  param shown as code goes in `CODE_PARAMS`, and a param shown as a name goes in `NAME_PARAMS`. An account param that
  can hold the default account goes in `DEFAULT_ACCOUNT_NAME_PARAMS` too.
- The code in a `GROUPS` entry of `packages/ui/src/errorRecovery.ts`. Reuse a group whose remedy fits, or add an
  `ErrorRecovery` with its `error.recovery.<name>` text in every catalog, and add the code to the classification cases
  in `packages/ui/src/errorRecovery.test.ts`. The group's remedy, the general advice for its kind of failure, goes in
  that `error.recovery.<name>` text, never in `daemon.<code>`. The group does not decide the remedy on its own:
  `classifyError` takes the remedy an operating-system signature names (an `errno` param, or `(os error N)` in `detail`,
  `reason` or the message, which a failure built from the system's error carries) before it looks at `GROUPS`, and that
  remedy (`access`, `diskSpace`) is about writing. Where the group's remedy must hold whatever the system text says, as
  `readAccess` does for `permission_denied`, give that group an early return in `classifyError` ahead of the signature,
  and cover the code with a signature in its `detail` in the test.
- Only where the code calls for a step of its own, one the group's remedy does not say ("Reopen it first."): the code
  in `RECOVERY_CODES` (`packages/ui/src/daemonMessage.ts`), with `daemon.<code>.summary` saying what happened,
  `daemon.<code>.recovery` that step, and `daemon.<code>` both sentences, for text-only consumers. An alert whose body
  is the daemon's own message then shows that step in place of the group's remedy.
- Where one code covers several situations the user fixes in different ways (a folder gone, a link in the way, a file
  where a folder should be), the daemon sends a fixed-vocabulary `cause` param, and `classifyError` picks the remedy
  from a table keyed by it (`SOURCE_CAUSE_RECOVERY`), each remedy an `ErrorRecovery` of its own. Map it there, never
  with an explicit `recovery` at the call sites: every route that raises the code then gets it, the easily missed ones
  too (opening a terminal path link reads through the same browse requests). Put the vocabulary in
  `packages/ui/src/protocol.ts` as a closed type with a normaliser that decides what a `cause` the window does not know
  and an error with no `cause` each become (as `SourceUnavailableCause` and `sourceUnavailableCause` do), key the table
  by that type, and read the param through the normaliser there and in every other place that reads it (a view, the
  gallery fixtures): written by hand in each place, those two defaults drift apart, and a new cause is not caught by the
  type check where a place lacks it. Cover both defaults in `errorRecovery.test.ts`; nothing else makes you decide them,
  and either can otherwise land on a remedy that does not fit or on the unexpected alert. Name the param `cause`, never
  `reason`, `reason_code` or `title`: when the failure ends a launch, `report_failed_launch`
  (`apps/daemon/src/coordinator.rs`) writes those over the failure's own params. `reason` is also the daemon's English
  text, which the window scans for an operating-system error signature.
- Where the code is in `DETAIL_BLOCK_CODES` (`packages/ui/src/daemonMessage.ts`), its `detail` shown as raw text of its
  own: an error of it that carries `detail` is worded by `daemon.<messageCode>.summary` (`richMessageCode`), and with
  that key missing the alert shows the daemon's own English message, which no type check or test catches. So where
  `messageCode` words the code by a param such as `cause`, give a `.summary` key to every variant that could ever arrive
  with `detail`, the one the normaliser maps an unknown `cause` to included (a newer daemon's cause with `detail` lands
  there), not only the variants today's daemon sends `detail` with; never drop such a key as unused, except for a code in
  `NO_SUMMARY_CODES`, which has none (next bullet).
- The `.summary` is the alert body's optional opening sentence, and it must add what the alert's title does not say (a
  cause, what the failure left as it was). Where the code is raised only by actions whose alert is titled for what
  failed, and the summary would say that again ("Git could not push this branch" under "Could not push the branch"),
  write none: add the code to `NO_SUMMARY_CODES` (`packages/ui/src/daemonMessage.ts`) and the body starts with the
  remedy. Keep the summary where the code is also shown with no title, in a pane's failure, a mark's popover or
  beside a field, as `models_command_failed` and `quota_command_failed` are.
- Where the remedy is done in Settings' Agent accounts section, a case in `recoveryAction`
  (`packages/ui/src/components/ErrorGuidance.tsx`). The alert then offers the button, and the recovery text says only
  what to do there, without naming the path to it. See `docs/product/error-alerts.md`, section "Recovery and unexpected
  errors".
