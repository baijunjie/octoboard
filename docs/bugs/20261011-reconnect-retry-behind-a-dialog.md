> Severity: Major

# The connection banner's Retry cannot be reached while a dialog is open

## Symptom

When the control connection drops while the file viewer is open, the banner's **Retry** — the only way back once
the automatic attempts are spent — sits under the dialog's overlay, so pressing it closes the viewer instead.

## Reproduction steps

1. Launch the packaged application and open a project's pane, then open any file or change in the file viewer.
2. With the viewer still open, drop the control connection for longer than about two seconds: killing the daemon
   by PID is enough, and so is interrupting the socket (an earlier run used a proxy in front of the daemon with a
   drop control, which is also the way to bring the connection back afterwards).
3. Wait for the banner at the bottom edge to stop saying it is retrying — the two automatic attempts are spent
   after roughly 1.5 seconds — and to offer **Retry**.
4. Press **Retry** where it is drawn.

The viewer closes and the connection is not retried. Pressing Retry again, with the viewer now closed, works.

## Expected vs. actual

- **Expected**: Retry is reachable while a dialog is open, and pressing it retries the connection.
  `docs/product/application-lifecycle.md`'s "Losing the daemon connection" presents Retry as the way back after the
  automatic attempts, and names no state in which it is unavailable.
- **Actual**: the press lands on the dialog's overlay, which dismisses the dialog; the connection stays down.

## Environment

- macOS, the packaged application (seen in an ad-hoc copy of a `pnpm build:app` build on `main`). Not checked in a
  browser or on Linux.
- Any dialog drawn over the content panel should be enough; it was seen with the file viewer.
- No particular project or repository state is needed.

## Scope of impact

Anyone whose daemon connection drops for more than about two seconds while a dialog is open. There is a
workaround once it is known: dismiss the dialog first (pressing Retry does that, so a second press reaches it),
or press F6 to the banner region and activate Retry from the keyboard, which the overlay does not intercept.
The keyboard route was not tried in this state and is listed under Leads rather than as a confirmed workaround.

## Leads

- **Verified in the code**: `packages/ui/src/daemon-client.ts` has `MAX_RECONNECT_ATTEMPTS = 2` and
  `RECONNECT_BASE_DELAY_MS = 500`, with the delay doubling per attempt — so the budget is 500 ms plus 1000 ms, and
  a drop lasting longer than that leaves the connection `closed` with nothing retrying it.
- **Verified in the code**: `packages/ui/src/App.tsx` renders `ConnectionBanner` before the dialogs, so a modal
  dialog's overlay is painted over the banner's strip.
- **Verified by observation**: in the packaged application, the press closed the viewer and did not retry.
- **Inferred**: the banner publishes its height as `--connection-banner-height`, which `style.css` exposes as
  `--bottom-chrome-height` for things overlaying the content area; a dialog's overlay appears not to use it. Not
  checked.
- **Inferred**: the banner is a stop of the F6 region cycle (`data-region="banner"`), so the keyboard may already
  reach Retry past the overlay. Not tried while a dialog was open.

## Acceptance criteria

- [ ] With a dialog open and the connection `closed`, pressing Retry where it is drawn retries the connection.
- [ ] That press does not close the dialog.
- [ ] Reaching Retry by keyboard (F6 to the banner) works in the same state, and focus goes somewhere sensible
  when the banner leaves.
- [ ] The behaviour holds for every dialog drawn over the content panel, not only the file viewer.
- [ ] `docs/product/application-lifecycle.md`'s "Losing the daemon connection" still describes what the
  application does, amended if the fix changes it.
