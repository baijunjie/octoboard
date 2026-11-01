# Raising an error alert in the UI

## Every route that raises an error alert passes a title naming what failed

Whenever code in `packages/ui` raises an error alert for something the user did, it passes a title: a plain name for
what failed, worded as `docs/product/error-alerts.md`, section "The error alert", sets out, with its key in every
message catalog. The routes and where each takes the title:

- `raiseError(error, { title, recovery?, content? })`, and `alertFailure({ title, ... })` as a request's catch handler
  (`packages/ui/src/components/alerts/raiseError.tsx`): for a failure about no session. They queue the alert directly.
- `raiseError(error, { title, session?, recovery?, content? })` from `useDaemon()` (`Daemon.raiseError`), and anything
  handed it to call, as `openPathTarget` is: the route for a failure about a session, which only it can name from the
  store's records, and for code whose tests inject the raiser. Its alert goes through `onToast` and is shown once
  `Toasts` has mounted.
- `errorAlerts.report({ ... })` called directly: its `title`.
- `useDialogAction(resetKey, errorTitle)` (`packages/ui/src/components/dialog/Dialog.tsx`), and the `errorTitle` prop of
  `ConfirmDialog` and `RenameDialog`.

`title` is a required key of the first two routes' options, but `title: undefined` passes the type check, and
`errorAlerts.report` takes it as optional: the alert is then titled with the generic "Error" (or "Unexpected error"),
which says nothing about what failed. Leave it undefined only in the cases `AlertOptions`
(`packages/ui/src/store/toasts.ts`) lists. Pass a title even where the failure usually carries a code with a
`daemon.<code>.alertTitle` of its own: that title wins only where the failure carries it, and the call site's covers
every other failure the action can return.

## The body's opening sentence adds to the title, or there is none

The title and the opening sentence of the body (a daemon code's `.summary`, an `ActionableError`'s message) are both the
application's own copy. The opening sentence is optional: it says only what the title leaves unexplained, never the
title again in other words. When the title says it all, the body has no opening sentence and starts with the remedy. So when a
new action's title names what failed, do not write a `.summary` that repeats it for the code it raises: mark the code's
record `noSummary: true` (`packages/ui/src/errors/codes.ts`) if it is raised only under such titles.

## A failure the UI itself detects is raised as an `ActionableError`, or the alert calls it a bug

Applies wherever code in `packages/ui` raises an error alert for a failure it detects or words itself — a lazy chunk
that did not load, a refused precondition, a clipboard that refused. The alert classifies
what it is given (`classifyError` in `packages/ui/src/errors/errorRecovery.ts`), and a string, a plain `Error`, or a
`DaemonRequestError` whose code's record gives no remedy classifies as unexpected: the alert says the failure may be a
bug and asks the user to file an Issue, however ordinary the cause. So raise such a failure as
`new ActionableError(message, recovery, { cause, detail })`, from `packages/ui/src/errors/actionableError.ts`, or pass the
`recovery` option of the route that raises it. Use the recovery whose remedy fits, or `"none"` where the alert's body
already says what to do.

The same explicit `recovery` is how a call site gives a daemon failure a better remedy than its code's group has. It
replaces the group's remedy for whatever the call raises (an operating-system signature can still win over it), so where
it fits only some codes, pass it only when the error's code is one of them, choosing it in one function in
`packages/ui/src/errors/errorRecovery.ts` that every route raising that kind of failure calls, never in a code set of
the call site's own: `hostPathRecovery` is that function for a project file or folder that is gone, used by the
host-path actions, and `pathLinkRecovery`, which builds on it, is the one for either step of a terminal path link click,
so reuse them there. A remedy that depends on why the daemon refused rather than on which action asked is not passed
here at all: it goes by the code's `cause` in `classifyError`, so that every route raising the code gets it.
