> Severity: Moderate

## Symptom

In the macOS application the webview dispatches no `blur` on `window` when the application is deactivated, so the
UI's `window` `blur` listeners never run when the user switches to another application.

## Reproduction steps

1. Build and launch the packaged macOS application (an independently identified verification build, as described in
   "Isolate a packaged verification with a build-time app identifier and a throwaway environment" in
   `docs/memory/building-and-launching-the-app-for-verification.md`).
2. Open a console with at least one project so the sidebar shows its **+** buttons.
3. With the application's window focused, press and hold ⌘: the **+** buttons switch to their with-defaults form.
4. Still holding ⌘, press Tab to switch to another application, then release ⌘ there.
5. Switch back to the application with ⌘+Tab (or the Dock) and do not move the pointer over the window.
6. Look at the sidebar's **+** buttons.

## Expected vs. actual

- Expected: the **+** buttons are in their ordinary form, since the primary modifier is no longer held (the
  with-defaults form applies "with the primary modifier held", `docs/product/focus-mode.md`, the header **+** and the
  creation tooltips).
- Actual: Unknown for the **+** buttons — not observed. Observed for the underlying event: an instrumented packaged
  build logging `window` `focus`/`blur` recorded zero of either across about ten deactivations and reactivations, while
  the native window focus events fired on every one.

## Environment

macOS 26.2, packaged Tauri application built from `main` as of 2026-11-08 (commit `2b81861b6` and the branch
`fix/window-focus-refresh` on top of it). Not checked on Windows or in a browser.

## Scope of impact

Users of the macOS application. Every `window` `blur` listener in `packages/ui/src` is affected; moving the pointer over
the window corrects the **+** buttons, which is the only workaround known.

## Leads

- Verified: in the macOS application, `window` `focus` and `blur` are not dispatched on application deactivation or
  reactivation; Tauri's native window focus events (`tauri://focus` / `tauri://blur`) are. `document.hasFocus()`
  follows the change about 100 ms late, and `visibilitychange` fires only when the window becomes covered.
- Verified: `window` `blur` listeners exist at `packages/ui/src/sidebar/useQuickCreateModifier.ts` (releases the held
  modifier), `packages/ui/src/components/tabs/tabDrag.ts`, `packages/ui/src/settings/accounts/accountDrag.ts`,
  `packages/ui/src/components/tabs/AgentTabList.tsx` and `packages/ui/src/navigation/useNavigationHistory.ts`.
- Verified: three `focusout` gates read `document.hasFocus()` directly rather than the shell's window focus:
  `packages/ui/src/project-browser/FileFilterField.tsx`, `packages/ui/src/shell/Toasts.tsx` and
  `packages/ui/src/process/ProcessSessionSelect.tsx`. Inferred: whether `focusout` fires at all on application
  deactivation in the macOS webview was not measured, so their visible effect is unknown.
- Inferred: the drag cases may already end through `pointercancel` / `lostpointercapture` when the application is
  deactivated mid-drag, so their visible effect is unconfirmed.
- Verified: `packages/ui/src/components/hooks/windowFocus.ts` (`subscribeWindowFocus`) and the platform's `windowFocus`
  capability report the native window focus, with the window's own events as the browser fallback.

## Acceptance criteria

- [ ] After steps 1–6 the sidebar's **+** buttons are in their ordinary form without the pointer moving.
- [ ] Each listed `blur` consumer releases or cancels its state when the macOS application is deactivated, or is
      confirmed by a packaged-build check to be ended by another event already.
- [ ] The browser build keeps releasing on the window's own `blur`.
