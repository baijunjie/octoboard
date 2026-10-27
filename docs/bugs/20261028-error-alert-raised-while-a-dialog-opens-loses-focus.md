> Severity: Moderate

# An error alert raised in the same task that opens a dialog ends with focus on the page

## Symptom

When an error alert is raised in the same task as a press without a pointer that opens a dialog, before that dialog has
rendered, focus ends on `<body>` instead of on the alert.

## Reproduction steps

1. In `packages/ui`, run `BROWSER=none pnpm exec vite --port <free port> --strictPort`.
2. Open `http://localhost:<port>/gallery-frame.html?scenario=sidebar-quick-create-quota-used-up&theme=light&lang=en`
   in Chromium at a 1440×900 window and wait until the page has the `data-gallery-ready` attribute.
3. In the page's console, run, all in one task:
   ```js
   const { errorAlerts } = await import("/src/errorAlerts.ts");
   [...document.querySelectorAll("button")].find((b) => b.textContent?.trim() === "New session").click();
   errorAlerts.report({ message: "First failure" });
   setTimeout(() => console.log(document.activeElement), 800);
   ```
4. Read the logged element.

It happens on every run (3 of 3 in a headless Chromium driven by Playwright).

## Expected vs. actual

- Expected: focus is on the error alert's `role="alertdialog"` (`docs/ui-accessibility.md`, "The error alert dialog":
  the alert takes focus).
- Actual: `document.activeElement` is `<body>` 800 ms after the report; the alert and the New session dialog are both
  on screen.

## Environment

- Commit 38595e6d2 on main.
- The UI state gallery under Vite's dev server (React StrictMode on), headless Chromium via Playwright, window 1440×900.
- The press is a script `click()` (virtual interaction modality), so it stands for a screen reader or other assistive
  technology pressing the button.

## Scope of impact

- A keyboard or screen-reader user whose press opens a dialog while a failure is reported in the same task loses
  focus to the page and has to move back into the alert by hand.
- The ordering needs the failure to be reported synchronously in the task of the press; whether any real flow does
  that is unknown.
- Workaround: move focus into the alert with Tab or the screen reader's navigation.

## Leads

- Verified (from a focus trace during the review of 38595e6d2): the New session dialog focused itself, the alert mounted
  and never received `focusin`, and the dialog was then blurred to `<body>`, where focus stayed through 800 ms.
- Inferred: the alert's own focus call (`ErrorAlertHost` in `packages/ui/src/dialogs/ErrorAlerts.tsx`) had no effect,
  possibly because the newly mounted modal had already hidden the rest of the page.
- Unknown: whether this predates 38595e6d2.
- `docs/memory/focus-and-key-handling-in-the-ui.md`, section "A modal that focuses itself as it opens over another
  dialog does it one render after mounting, not at mount or from a timer", and the "over a dialog opened without a
  pointer" cases in `packages/ui/src/dialogs/ErrorAlerts.test.tsx` cover the neighbouring orderings, where the dialog
  has rendered before the alert is raised.

## Acceptance criteria

- [ ] Following the reproduction steps, `document.activeElement` is the error alert's `role="alertdialog"`, on
      repeated runs.
- [ ] An automated test reproduces this ordering (alert reported in the same task as the virtual press that opens the
      dialog) and fails without the fix.
- [ ] The existing "over a dialog opened without a pointer" cases in `ErrorAlerts.test.tsx` still pass, with and without
      StrictMode.
