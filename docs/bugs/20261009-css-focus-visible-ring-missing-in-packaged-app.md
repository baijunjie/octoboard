> Severity: Moderate

## Symptom

Suspected: focusable elements styled with CSS `:focus-visible` rings may show no focus ring in the packaged app when
reached with Tab, as the file viewer's code region did.

## Reproduction steps

1. Build the packaged app (`pnpm build:app` in `apps/desktop`) and launch it with a console holding a project and a
   session.
2. Move focus with Tab (and F6 between regions) onto each of these elements: a sidebar row
   (`packages/ui/src/sidebar/rows.tsx`), the pane resize handle (`packages/ui/src/components/PaneResizeHandle.tsx`),
   and the connection status button (`packages/ui/src/components/ConnectionStatus.tsx`).
3. Look for the focus ring on the element that has focus.

## Expected vs. actual

- Expected: every focusable element shows a visible focus ring when reached from the keyboard
  (`docs/memory/writing-ui-components.md`, "The UI meets WCAG 2.2 AA", "Visible focus").
- Actual: Unknown for these three elements. Verified for the file viewer's code region before its fix: with focus
  confirmed on it through the accessibility API, its `focus-visible:ring-2 focus-visible:ring-focus` ring did not show.

## Environment

- `main` at 4a5b92a; macOS, the packaged app's WKWebView. Playwright WebKit shows the ring, so a check there does not
  reproduce it.

## Scope of impact

Keyboard users of the packaged app, if confirmed: they cannot see where focus is on the affected elements. No
workaround beyond moving focus by other means.

## Leads

- Verified: the three elements use `outline-none focus-visible:ring-2 focus-visible:ring-focus`
  (`rows.tsx:67`, `PaneResizeHandle.tsx:126`, `ConnectionStatus.tsx:38`).
- Verified: the viewer's code region, reached with Tab inside a react-aria focus-trapped dialog, did not match CSS
  `:focus-visible` in the packaged app, while HeroUI controls, styled from react-aria's `data-focus-visible`, showed
  their rings.
- Inferred: the defect may be specific to focus moved inside react-aria's focus scope; elements outside a dialog may
  be unaffected. Root cause not established.

## Acceptance criteria

- [ ] In the packaged app, a sidebar row reached with Tab shows a focus ring.
- [ ] In the packaged app, the pane resize handle reached with Tab shows a focus ring.
- [ ] In the packaged app, the connection status button reached with Tab shows a focus ring.
