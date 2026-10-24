# Adding a daemon error code

## A new daemon error code is classified in the UI in the same change, or the window calls it a bug

The window classifies a daemon error by its code alone (`classifyError` in `packages/ui/src/errorRecovery.ts`). If the
code is not in that file's `GROUPS`, the window shows it as an unexpected failure. That alert says the failure may be a
bug and asks the user to file an Issue, even when the refusal is deliberate and the user can fix the cause. No type
check or test links the codes the daemon returns to that table, so nothing fails when a new code is missing from it.
So when daemon code starts returning a new error code, the same change adds, in `packages/ui`:

- `daemon.<code>` in every message catalog under `packages/ui/src/i18n/messages/`. It says only what happened. Its params are
  listed in `packages/ui/src/daemonMessage.ts`. A param shown as code goes in `CODE_PARAMS`, and a param shown as a
  name goes in `NAME_PARAMS`. An account param that can hold the default account goes in `DEFAULT_ACCOUNT_NAME_PARAMS`
  too.
- The code in a `GROUPS` entry of `packages/ui/src/errorRecovery.ts`. Reuse a group whose remedy fits, or add an
  `ErrorRecovery` with its `error.recovery.<name>` text in every catalog, and add the code to the classification cases
  in `packages/ui/src/errorRecovery.test.ts`. The remedy goes in the recovery text, never in `daemon.<code>`.
- Where the remedy is done in Settings' Agent accounts section, a case in `recoveryAction`
  (`packages/ui/src/components/ErrorGuidance.tsx`). The alert then offers the button, and the recovery text says only
  what to do there, without naming the path to it. See `docs/product/error-alerts.md`, section "Recovery and unexpected
  errors".
