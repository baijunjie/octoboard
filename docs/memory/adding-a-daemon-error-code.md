# Adding a daemon error code

## A new daemon error code gets its record in the UI in the same change, chosen so the window does not call it a bug

Everything the window says about a code — its remedy, how its params are shown, what its alert body holds — comes from
the code's record in `CODES` (`packages/ui/src/errors/codes.ts`), and `classifyError`
(`packages/ui/src/errors/errorRecovery.ts`) shows a code whose record gives no remedy as an unexpected failure: the
alert says the failure may be a bug and asks the user to file an Issue, even when the refusal is deliberate and the user
can fix the cause. `packages/ui/src/daemonCodes.test.ts` reads every code the daemon declares and fails for one with no
words in the catalogs, no daemon record (or a record the daemon no longer declares), or, for an error or failure
notice, a record with none of `recovery`, `causeOnly` or `unexpected`; it also checks the record's facets against the
catalogs. Which remedy, and the wording rules below, are still on you. So when daemon code starts sending a new code,
the same change:

- Declares it in `apps/daemon/src/protocol/codes.rs`, as its own `pub const NAME: Code = Code("code");` in the code
  module for its kind (`error_code`, `notice_code`, `models_unavailable_code`, `quota_unavailable_code`,
  `login_failure_code`, reached elsewhere as `crate::protocol::error_code` and so on); every wire `code` field is a
  `Code`, which nothing outside that file can make. A code of a quota reading, a models list or a login is shown where
  it is read, so its record needs only its message facets, not a remedy.
- Adds its record to `CODES` with `origin: "daemon"`, and `daemon.<code>` in every message catalog under
  `packages/ui/src/i18n/messages/`. The sentence says what happened, and no remedy beyond a code-specific step as set
  out below. Where `messageCode` words the code only by a param (one key per `kind` or `cause`), the key it resolves to
  with no such param is still needed: it is what a newer daemon's variant the window does not know is shown with, so
  word it for any of them, not as a placeholder. A notice that is a failure, shown as an error alert rather than a
  toast, needs `failureNotice: true`; nothing else makes it one.
- Gives the record a `recovery`. Reuse a group whose remedy fits, or add an `ErrorRecovery` with its
  `error.recovery.<name>` text in every catalog. The group's remedy, the general advice for its kind of failure, goes in
  that `error.recovery.<name>` text, never in `daemon.<code>`. The group does not decide the remedy on its own:
  `classifyError` takes the remedy an operating-system signature names (an `errno` param, or `(os error N)` in
  `detail`, `reason` or the message, which a failure built from the system's error carries) before the record's, and
  that remedy (`access`, `diskSpace`) is about writing. Where the group's remedy must hold whatever the system text
  says, as `readAccess` does for `permission_denied`, give it an early return in `classifyError` ahead of the
  signature, and cover the code with a signature in its `detail` in `errorRecovery.test.ts`. Use `causeOnly` for a
  refusal whose sentence says why and no remedy follows. Use `unexpected: "<why>"` only where the unexpected alert is
  right for it — a refusal of a request the window never makes, a failure the daemon did not foresee, or one
  classified by another code or `cause` it carries. A refusal the user can run into and fix, however rare, gets a
  `recovery` (`none` where its own sentence says it all).
- Only where the code calls for a step of its own, one the group's remedy does not say ("Reopen it first."):
  `recoveryLine: true`, with `daemon.<code>.summary` saying what happened, `daemon.<code>.recovery` that step, and
  `daemon.<code>` both sentences, for text-only consumers. An alert whose body is the daemon's own message then shows
  that step in place of the group's remedy.
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
  (`apps/daemon/src/coordinator/open.rs`) writes those over the failure's own params. `reason` is also the daemon's
  English text, which the window scans for an operating-system error signature.
- Where the record has `detailBlock: true` (its `detail` shown as raw text of its own), an error of it that carries
  `detail` is worded by `daemon.<messageCode>.summary` (`richMessageCode`), and with that key missing the alert shows
  the daemon's own English message. The test only checks that some message of the code has a `.summary`, so where
  `messageCode` words the code by a param such as `cause`, give a `.summary` key to every variant that could ever
  arrive with `detail`, the one the normaliser maps an unknown `cause` to included (a newer daemon's cause with
  `detail` lands there), not only the variants today's daemon sends `detail` with; never drop such a key as unused,
  except for a code marked `noSummary`, which has none (next bullet).
- The `.summary` is the alert body's optional opening sentence, and it must add what the alert's title does not say (a
  cause, what the failure left as it was). Where the code is raised only by actions whose alert is titled for what
  failed, and the summary would say that again ("Git could not push this branch" under "Could not push the branch"),
  write none: mark the record `noSummary: true` and the body starts with the remedy. Keep the summary where the code is
  also shown with no title, in a pane's failure, a mark's popover or beside a field, as `models_command_failed` and
  `quota_command_failed` are.
- Where the remedy is done in Settings' Agent accounts section, a case in `recoveryAction`
  (`packages/ui/src/components/alerts/ErrorGuidance.tsx`). The alert then offers the button, and the recovery text says only
  what to do there, without naming the path to it. See `docs/product/error-alerts.md`, section "Recovery and unexpected
  errors".
