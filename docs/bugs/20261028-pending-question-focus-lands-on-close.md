> Severity: Major

# A trust prompt or session request raised over Settings opened by an assistive press focuses its Close button

## Symptom

When a trust prompt or an agent's session request arrives while Settings is open and was opened by a press without a
pointer, the question dialog ends with focus on its Close (X) button rather than on itself, so Enter refuses the request
or dismisses the prompt.

## Reproduction steps

1. In `packages/ui`, run `BROWSER=none pnpm exec vite --port <free port> --strictPort`.
2. Open `http://localhost:<port>/gallery-frame.html?scenario=session-request-project&theme=light&lang=en` in Chromium
   at a 1440×900 window and wait until the page has the `data-gallery-ready` attribute.
3. In the page's console, reach the fixture daemon's store by walking React's fiber tree from `#root` to a fiber whose
   `memoizedProps.value` has `store.setState` and `request`. Save `store.getState().sessionRequests`, then call
   `store.setState({ sessionRequests: [] })`.
4. Run `[...document.querySelectorAll("button")].find((b) => b.getAttribute("aria-label") === "Settings").click()` and
   do not move focus afterwards.
5. About 800 ms later, restore the saved `sessionRequests` with `store.setState`.
6. About 1 s later, read `document.activeElement`, then press Enter.

It happens on every run (3 of 3 in a headless Chromium driven by Playwright). The arrival delay in step 5 does not
matter while focus has not moved since Settings opened: the same result was seen at 0, 100, 400, 1000 and 1500 ms.

## Expected vs. actual

- Expected: focus is on the question's own `role="alertdialog"`, and Enter answers nothing.
  - `PendingQuestionDialog` renders `ConfirmDialog` with `unprompted`, which exists so that an Enter meant for
    something else answers nothing.
  - `docs/ui-accessibility.md`, "The error alert dialog", says the alert takes focus.
- Actual: focus is on the question's header Close button, and Enter sends
  `{"type":"answer_session_request","request_id":"r-1","approve":false}`, refusing the agent's request. For a trust
  prompt in the same position, Enter dismisses the prompt.

## Environment

- Commit c538fc0db on main.
- The UI state gallery under Vite's dev server (React StrictMode on), headless Chromium via Playwright, window 1440×900.
- The press in step 4 is a script `click()`, which leaves react-aria's interaction modality virtual, as a screen
  reader's press does. A mouse or keyboard press does not reproduce it.

## Scope of impact

- Users of a screen reader or other assistive technology who open Settings and leave focus where it lands.
  - A question pushed by the daemon while Settings is open is answered with "Refuse", or dismissed with "Not now", by
    the next Enter.
  - Any unprompted question can arrive within about a frame of such a press opening any dialog; this was seen with
    "New console session" too.
- Workaround: move focus inside Settings (e.g. to another tab) before a question arrives, or check where focus is
  before pressing Enter.

## Leads

- **Verified, both by reproduction:**
  - Only `PendingQuestionDialog` (`packages/ui/src/dialogs/PendingQuestionDialog.tsx`) raises `ConfirmDialog` with
    `unprompted` over another dialog without a press inside that dialog; it is driven by daemon pushes (`trust_prompt`,
    `session_request`).
  - Once focus moves inside Settings before the question arrives, the question ends on itself.
- **Verified (react-aria 3.52.1 source):**
  - `ConfirmDialog` focuses itself from a mount `useLayoutEffect` when `unprompted`.
  - When the underlying dialog's FocusScope has no recorded focused node, its `useFocusContainment` `onFocus` calls
    `focusFirstInScope`, and `focusSafely` defers that move via `runAfterTransition` under virtual modality.
- **Inferred:**
  - Settings' scope has no recorded focused node because Settings focuses its selected tab from a ref callback during
    the mount commit (`attachSelected` in `packages/ui/src/settings/SettingsDialog.tsx`), before its containment is
    on. That would explain why the window stays open for as long as focus does not move.
- **Related:**
  - `docs/memory/focus-and-key-handling-in-the-ui.md`, section "A modal that focuses itself as it opens over another
    dialog does it one render after mounting, not at mount or from a timer", describes the same race for a modal that
    focuses itself at mount. `ErrorAlertHost` in `packages/ui/src/dialogs/ErrorAlerts.tsx` follows that section and is
    not affected.
  - `PendingQuestionDialog` passes a `resetKey`, while `DialogFrame`'s `focusesItselfOnOpen` (in
    `packages/ui/src/dialogs/Dialog.tsx`) is documented for callers that pass none.
  - `ConfirmDialog`'s `opensOnConfirm` focus runs from a passive effect and showed the same result in jsdom. No flow was
    found that reaches it this way.
  - A destructive `ConfirmDialog` raised in the same window ended, in jsdom only, with focus on the underlying
    dialog's `role="dialog"`. That is the shape the exit confirmation would take if Cmd+Q landed in the window; it was
    not tried in a browser.

## Acceptance criteria

- [ ] Following the reproduction steps, `document.activeElement` is the question's `role="alertdialog"` and Enter sends
      no `answer_session_request` and dismisses nothing, on repeated runs.
- [ ] The same holds for a trust prompt in place of the session request.
- [ ] A question raised within a frame of a virtual press that opens another dialog (e.g. "New console session") also
      ends on itself.
- [ ] An automated jsdom test reproduces the case (virtual modality, `requestAnimationFrame` and `cancelAnimationFrame`
      stubbed as 0 ms timers, with and without StrictMode) and fails without the fix.
- [ ] A question raised with a mouse or keyboard press beforehand still ends on itself.
