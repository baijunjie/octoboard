# Error code conventions

Everything the daemon tells a person (a failed request, a notice about a session) travels as a stable code with params,
and the UI words and handles it from a record per code. This doc covers declaring codes in the daemon, giving each its
record, wording and remedy in the UI, and raising the UI's own failures. Where an error is shown visually (alerts,
inline errors, titles) is in `docs/conventions/ui-standard.md`; the wire shape of `error` and `session_notice` is in
`apps/daemon/PROTOCOL.md`, "Coded messages"; how a provider's codes relate to the provider is in
`docs/conventions/provider-neutral-core.md`.

## Daemon codes

What a client receives (a stable `code` and `params` on every `error` and `session_notice`, the English `message`, the
param conventions, what a code may never change, `internal_error`) is specified in `apps/daemon/PROTOCOL.md`, "Coded
messages". What code must do to meet it:

- **A core code is a `pub const NAME: Code = Code("code")` in `apps/daemon/src/protocol/codes.rs`**, in the module of
  its kind (`error_code`, `notice_code`, `models_unavailable_code`, `quota_unavailable_code`, `login_failure_code`).
  Every wire `code` field is a `Code`, made only there. A new reading of a situation is a new code, never a changed one.
- **A failure becomes an `error` reply only through `src/protocol/errors.rs`**, and is raised as a `CodedError`.
- **Translatable words are never baked into params.**
- **The param that carries the situation of a code covering several is named `cause`**, never `reason`, `reason_code` or
  `title`: `report_failed_launch` overwrites those on a launch failure. Its values are a fixed vocabulary. A code that
  wraps another failure is the exception: it carries that failure as `reason` and `reason_code` (`CodedError::nesting`),
  which the UI reads as the record's `nested` facet.
- **A condition the daemon finds outside a request's reply goes out with a code**, never a plain `anyhow` error (which
  goes out as `internal_error`, shown as a possible bug). About a session:
  `state.broadcast(Notice::new(code, message, params).about(&session_id))` with a `notice_code`; about no session:
  `Event::from_error(None, &CodedError::raised(code, message, params))` with an `error_code`.
- **Store code returns the store's `Result` (`src/store/failure.rs`), and a rusqlite error reaches `?` as itself**,
  never wrapped into `anyhow` first (`.context()`, `anyhow::Error::from`), which skips the classification. A caller
  outside the store unwraps with `Ok(store.x()?)`. Why: the daemon also opens agents' own SQLite files, whose failures
  must not be reported as Octoboard's data folder. Opening and migrating the store at startup, which has no reply to
  classify for, may wrap errors with context.
- **Keep a missing `git`'s `binary_not_found` through to the reply or the `GitStatus`** (`project_git::no_git_or`,
  `StepFailure::no_git`), never flatten the error to a string first.
- **Look up a known record through `Store::require_*`**, refuse a locked or archived session through
  `state::refusals::ensure_unlocked` and `refuse_if_archived`, and check a directory with `hostfs::require_directory`,
  so each failure carries its own code (the list is in `docs/structure/daemon.md`, "Shared entry points").

## Provider codes

- **A code only one provider raises is a `ProviderCode` named `provider.<id>.<code>`**, a `pub const` of
  `src/providers/<id>.rs`, listed in that descriptor's `codes`, and made only through `Code::of_provider`. Core codes
  are not namespaced. Not: a core code named after the provider.
- **Its record and wording are in the provider's presentation module** (`packages/ui/src/providers/<id>.ts`): the record
  in `codes`, the words in every language of `messages`, keyed by the code itself (`provider.<id>.<code>`, no `daemon.`
  before it).
- **A remedy of the code's own, where it has one, is `provider.<id>.<code>.remedy`, named as the record's `recovery`.**
  Not every provider code has one: a record may instead use a recovery group, `"none"`, or `"account"`.

## The UI's record of a daemon code

- **A daemon code a window can show gets its record and its words in the same change that adds the code.** The record
  is one entry per code in `CODES` (`packages/ui/src/errors/codes.ts`), with `origin: "daemon"`; a core code's words are
  `daemon.<code>` in every message catalog. `packages/ui/src/daemonCodes.test.ts` fails for a code with no record or no
  words; the rules below are what it cannot check.
- **Give the record a `recovery`, or say why none.** Reuse an `ErrorRecovery` group or add one with
  `error.recovery.<name>` in every catalog; the group's general remedy goes in that text, never in `daemon.<code>`.
  `causeOnly` is for a refusal whose sentence says why; `unexpected: "<why>"` only where the unexpected alert is right
  (a refusal of a request the window never makes, a failure the daemon did not foresee, or one classified by another
  code or `cause` it carries). A refusal the user can fix, however rare, gets a `recovery` (`none` where its own
  sentence says it all). Why: `classifyError` shows a code whose record gives no remedy as an unexpected failure, which
  tells the user to file an Issue.
- **An `unexpected` code's message states only what happened** (what was refused or failed), never a remedy or a
  guess at the cause: the unexpected alert already says what to do, and a remedy in the sentence contradicts it.
- **The daemon's English free text (an `anyhow` message, `message`, a param carrying a system's wording) never goes into
  a catalog sentence**; it is shown only as the alert's detail. Why: a sentence in the user's language with English
  inside reads as broken, and that text is not worded for users.
- **A remedy that must hold whatever the operating system's text says gets an early return in `classifyError`** ahead
  of the signature check, and the code is covered in `errorRecovery.test.ts` with a signature in its `detail`.
- **A code worded by a param uses `variantOf`**, with `daemon.<code>.<variant>` per variant; the code's own key is
  worded for any variant (what an unknown variant shows), never as a placeholder.
- **A notice shown as an error alert sets `failureNotice: true`**; nothing else makes it one.
- **A code with a step of its own sets `recoveryLine: true`** and has `.summary`, `.recovery` and the combined
  `daemon.<code>` keys.
- **A word the sentence names that the daemon does not send is a fixed `uiParams` entry**; if it varies with where the
  code is shown (the agent), it is put into the params at every place that words the code (`daemonMessage` and the
  raised `CodedError`). Why: with it missing, `hasCodeParams` takes the error as unworded and no test fails.
- **A code covering situations the user fixes differently sends a `cause`, and `classifyError` picks the remedy from
  `SOURCE_CAUSE_RECOVERY`**, never an explicit `recovery` at the call sites, so every route that raises the code gets
  it. The vocabulary is a closed type with a normaliser in `packages/ui/src/errors/causes.ts`; the table is keyed by
  that type, and both defaults (unknown cause, no cause) are covered in `errorRecovery.test.ts`.
- **Every reader of a coded failure's `params.cause` goes through that code's normaliser in
  `packages/ui/src/errors/causes.ts`**, whatever the cause picks (a remedy, a record's `variantOf`, a line of wording,
  a branch in a component), and so does a reader of a typed protocol field that carries the same vocabulary (the
  `account_sharing` event's `failure.cause`); never a comparison of the raw value, or a list of its values, at the
  reader. A normaliser whose unknown cause is worded by the code's own key returns `undefined`; one with a catch-all
  cause (`other`) returns that. Why: a daemon newer than this client may send a cause it does not know, which a
  generated type does not stop at run time, and only the normaliser gives every reader the same fallback.
- **A record with `detailBlock: true` has a `.summary` key for every variant that could arrive with `detail`** (the
  normaliser's unknown-cause default included), and such a key is never dropped as unused except for `noSummary` codes.
- **A `.summary` adds what the alert's title does not say**; where it would repeat the title, the record is
  `noSummary: true` and the body starts with the remedy. Keep the summary where the code is also shown with no title (a
  pane's failure, a mark's popover, beside a field), as `models_command_failed` and `quota_command_failed` do.
- **A code of a quota reading, a models list or a login needs only its message facets, no remedy**
  (`models_unavailable_code`, `quota_unavailable_code`, `login_failure_code`). Why: it is shown where it is read, so the
  remedy a `recovery` gives does not apply.
- **A remedy done in Settings' Agent accounts section gets a case in `recoveryAction`**
  (`packages/ui/src/components/alerts/ErrorGuidance.tsx`), and the recovery text says only what to do there.
- **Settle what a daemon failure arrives as against a real daemon before designing its remedy or its fixture.** The
  daemon's failure cannot be read off the system failure underneath (SQLite words denied access and a full disk in its
  own codes, with no `(os error N)`), nor does a code's name say which route it comes on. A remedy keyed to expected
  text, and fixtures modelled on it, pass every test and never fire in the app.
- **`errors/` and `browse/` do not import `daemon-client`**: `CodedError` lives in `errors/codedError.ts`, and the codes
  the client raises are records in `errors/codes.ts` (enforced by `eslint.layers.mjs`). Error modules do not import each
  other in a cycle.

## Raising a failure in the UI

- **Raise an error alert only through `raiseError` or `alertFailure`** (`src/components/alerts/raiseError.tsx`), passing
  `session` for a failure about a session, even where a dialog or row named it when the action started: the alert names
  the session from the store as it shows, and an action raised from a shortcut or after its dialog closed has nothing
  else on screen that says which session failed; alerts are drawn through `ErrorAlert`, `FailureAlert` and
  `ChoiceWarning`. Where an action goes through a dialog, `useDialogAction` or the `errorTitle` of `ConfirmDialog` and
  `RenameDialog` take the title. Daemon broadcasts and failures shown as notifications go through
  `broadcastNotification` (`src/store/notifications.ts`). Leave `session` out where the message already names the
  session it is about, or another session that may share its name.
- **Every route that raises an alert passes a title naming what failed**, with its key in every catalog. The routes are
  `raiseError`, `alertFailure`, `errorAlerts.report` called directly, `useDialogAction`'s `errorTitle`, and the
  `errorTitle` of `ConfirmDialog` and `RenameDialog`. Leave it undefined only in the cases `AlertOptions` lists: the
  alert is otherwise titled with a generic "Error", and `title: undefined` type-checks, so the compiler does not catch
  it. Pass a title even where the failure usually has a code with its own `alertTitle`; the call site's covers every
  other failure. Not: `alertFailure({ title: undefined })`.
- **A failure the UI itself detects or words is raised as `new ActionableError(message, recovery, { cause, detail })`**
  (`src/errors/actionableError.ts`), or with the route's `recovery` option (`"none"` where the body already says what to
  do). Not: a string, a plain `Error`, or a `CodedError` whose record gives no remedy: the alert calls it a bug. A plain
  `Error` is allowed where the failure is deliberately unexpected and a comment at the raise says so.
- **A call-site `recovery` that fits only some codes is chosen in one function in `errors/errorRecovery.ts` that every
  route calls** (`hostPathRecovery`, `pathLinkRecovery`), never in a code set at the call site. A remedy that depends
  on why the daemon refused goes by the code's `cause` in `classifyError`, not here.
- **A request that writes only the application's own data derives its remedy with `appDataRecovery`**:
  `alertAppDataFailure(title)` in a catch handler, `errorRecovery={appDataRecovery}` on `ConfirmDialog` or the third
  argument of `useDialogAction` on every confirm path. Keep the ordinary route where the action also writes to a folder
  the user chose.
- **A recovery's extra button is a `recoveryAction`** (the `RecoveryAction` union); a link is drawn by `RecoveryLink`;
  a pane failure takes it as `PaneFailure`'s `linkAction`. A failed browse request is treated with `browseFailure` or
  `projectBrowseFailure`.
- **The outcome of a long-running action the user started is raised from window-lifetime code**, not from the surface
  that sent the request (which may be closed by then): record in client store state that this window started it, keyed
  by what tells the run from an earlier one (the status's `started_at`), clear it when the request is refused, raise the
  outcome from a component `App` mounts for the window's life (`AgentCliUpdateAlerts`, `LoginPageOpener`), and on a
  fresh `snapshot` keep a record whose run is running or already ended.
