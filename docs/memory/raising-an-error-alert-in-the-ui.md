# Raising an error alert in the UI

## Every route that raises an error alert passes a title naming what failed

Whenever code in `packages/ui` raises an error alert for something the user did, it passes a title: a plain name for
what failed, worded as `docs/product/error-alerts.md`, section "The error alert", sets out, with its key in every
message catalog. The routes and where each takes the title:

- `toastError(message, session?, title?)` from `useDaemon()`, and anything handed it to call, as `openPathTarget`'s
  `toastError` is: the third argument.
- `reportError(error, session?, recovery?, content?, title?)` (`packages/ui/src/reportError.tsx`): the fifth argument.
- `useDialogAction(resetKey, errorTitle)` (`packages/ui/src/dialogs/Dialog.tsx`): the second argument.
- `ConfirmDialog`: its `errorTitle` prop. `RenameDialog` requires one.
- `errorAlerts.report({ ... })` called directly: its `title`.

Every title is optional except `RenameDialog`'s, so leaving one out fails no type check and no test: the alert is then
titled with the generic "Error" (or "Unexpected error"), which says nothing about what failed. Pass it even where the
failure usually carries a code with a `daemon.<code>.alertTitle` of its own: that title wins only where it applies, and
the call site's covers every other failure the action can return.

## A failure the UI itself detects is raised as an `ActionableError`, or the alert calls it a bug

Applies wherever code in `packages/ui` raises an error alert for a failure it detects or words itself — a lazy chunk
that did not load, a refused precondition, a clipboard that refused, a daemon that did not start. The alert classifies
what it is given (`classifyError` in `packages/ui/src/errorRecovery.ts`), and a string, a plain `Error`, or a
`DaemonRequestError` whose code is in no `GROUPS` entry classifies as unexpected: the alert says the failure may be a
bug and asks the user to file an Issue, however ordinary the cause. So raise such a failure as
`new ActionableError(message, recovery, { cause, detail })`, from the same file, or pass the `recovery` explicitly
(`reportError`'s third argument, `errorAlerts.report`'s `recovery`). Use the recovery whose remedy fits, or `"none"`
where the alert's body already says what to do.
