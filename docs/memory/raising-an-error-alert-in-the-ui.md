# Raising an error alert in the UI

## Every route that raises an error alert passes a title naming what failed

Whenever code in `packages/ui` raises an error alert for something the user did, it passes a title: a plain name for
what failed, worded as `docs/product/error-alerts.md`, section "The error alert", sets out, with its key in every
message catalog. The routes and where each takes the title:

- `toastError(message, session?, title?, recovery?)` from `useDaemon()`, and anything handed it to call, as
  `openPathTarget`'s `toastError` is: the third argument.
- `reportError(error, session?, recovery?, content?, title?)` (`packages/ui/src/reportError.tsx`): the fifth argument.
- `errorAlerts.report({ ... })` called directly: its `title`.
- `useDialogAction(resetKey, errorTitle)` (`packages/ui/src/dialogs/Dialog.tsx`), and the `errorTitle` prop of
  `ConfirmDialog` and `RenameDialog`: required, so the type check catches a missing one.

The first three take the title as optional, so leaving it out there fails no type check and no test: the alert is then
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
(`reportError`'s third argument, `toastError`'s fourth, `errorAlerts.report`'s `recovery`). Use the recovery whose
remedy fits, or `"none"` where the alert's body already says what to do.

The same explicit `recovery` is how a call site gives a daemon failure a better remedy than its code's group has. It
replaces the group's remedy for whatever the call raises (an operating-system signature can still win over it), so where
it fits only some codes, pass it only when the error's code is one of them, choosing it in one function in
`packages/ui/src/errorRecovery.ts` that every route raising that kind of failure calls, never in a code set of the call
site's own: `hostPathRecovery` is that function for a project file or folder that is gone, used by the host-path
actions, and `pathLinkRecovery`, which builds on it, is the one for either step of a terminal path link click, so reuse
them there. A remedy that depends on why the daemon refused rather than on which action asked is not passed here at
all: it goes by the code's `cause` in `classifyError`, so that every route raising the code gets it.
