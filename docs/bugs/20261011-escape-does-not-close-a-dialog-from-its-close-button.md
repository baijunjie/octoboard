> Severity: Moderate

# Escape does not close a dialog while its own close button has focus

## Symptom

A dialog whose close button holds keyboard focus — one Tab from where focus starts when it opens — does not close on
Escape; the key reaches neither the dialog nor its backdrop.

## Reproduction steps

1. Run the gallery (`pnpm --filter @octoboard/ui gallery`) and open the scenario "Edit console"
   (`dialog-edit-console`). Focus starts on the dialog's `<section>`.
2. Press Escape: the dialog closes. Reopen the scenario.
3. Press Tab once — focus lands on the dialog's close button ("×" at its corner, HeroUI's
   `Modal.CloseTrigger`) — then press Escape: nothing happens, the dialog stays open.
4. Press Tab once more, so focus moves on to the name field, then press Escape: the dialog closes again.

## Expected vs. actual

- Expected: Escape closes the dialog wherever focus sits inside it. The product docs state Escape as a way to close
  a dialog throughout — `docs/product/settings.md`, "Settings closes on `Escape`, on its close button, and on a
  press on the dimmed area around it"; `docs/product/folder-trust.md`'s "Not now — also what Escape, the dialog's
  close button and a click outside it do"; the refusal row of `docs/product/requesting-a-console-session.md`'s
  table — and name no state in which it stops working. The user confirmed on the spot (2026-10-11) that this is a
  defect worth filing.
- Actual: with focus on the close button Escape does nothing, measured three runs out of three; with focus on the
  `<section>` or on any field it closes as documented.

## Environment

- macOS (Darwin 25.2.0), Apple Silicon; measured in Playwright's `webkit` (WebKit 27.2, viewport 1440×900) against
  the gallery page, on `main`.
- Not checked in the packaged application, in a browser, or on Linux.
- It is not specific to a lost connection or to the connection banner: it reproduces in the untouched
  `dialog-edit-console` scenario, which has neither.

## Scope of impact

- Any keyboard user of any dialog that has a close button, at one particular focus position. The workaround is to
  Tab on (or back) and press Escape, or to activate the close button itself with Enter or Space.
- Whether the Settings dialog, whose Escape is the one the product docs state outright, is affected the same way is
  Unknown — only the Edit console dialog was measured.

## Leads

- Verified: with focus on the close button, instrumenting both the dialog element and its backdrop showed **no**
  `keydown` arriving at either, although `document.activeElement` reported the close button. WebKit's real focus and
  the DOM's `activeElement` appear to have diverged.
- Verified: it is independent of the connection banner and of the banner's top-layer marking — the same three focus
  positions behave identically with the banner's strip up over the dialog and with no strip shown at all.
- Verified: every dialog except Settings is built on the one frame in `packages/ui/src/dialogs/Dialog.tsx`, which
  renders HeroUI's `Modal` (or `AlertDialog` for an alert) and its `Modal.CloseTrigger`, so for those the cause
  cannot be per-dialog. Settings builds its own `Modal` and its own close trigger directly, which is why it is
  listed as Unknown above.
- Inferred: react-aria's `FocusScope` and HeroUI's close trigger are where to look, since the dialog's own Escape
  handling is react-aria's and the one focus position that fails is the trigger's. Not investigated.

## Acceptance criteria

- [ ] Escape closes the dialog with focus on its close button, in both the `Modal` and the `AlertDialog` frame.
- [ ] Escape still closes it from every other focus position inside it, and still does nothing elsewhere.
- [ ] Where focus goes after such an Escape is the same as for an Escape pressed anywhere else in the dialog.
