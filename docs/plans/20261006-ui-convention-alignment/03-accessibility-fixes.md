# Remaining Accessibility Fixes

> Goal: the remaining known WCAG 2.2 AA gaps in `packages/ui` are closed.
> Completion criteria: with a screen reader or the accessibility tree, every expandable control announces whether it
> is expanded; with reduced motion turned on in the OS, no spinner turns; the sidebar resize handle shows a focus
> indicator that meets WCAG 2.2's focus criteria; checked in the macOS app and a browser; `pnpm --filter @octoboard/ui
> build` passes.

### Technical design

- [ ] `aria-expanded` on the collapsible rows of the sidebar tree (console rows, project rows, archive group rows),
  which today show their state only through the chevron.
- [ ] `aria-expanded` on the top bar's sidebar toggle and report-panel toggle, which today only swap their label and
  icon.
- [ ] The working-status spinner stops under `prefers-reduced-motion`.
- [ ] The sidebar resize handle's focus indicator is measured against WCAG 2.2's focus-visible and focus-appearance
  criteria and, if it falls short, given a proper one (today it removes the outline and only turns a thin line the
  accent colour).

## Open

- How keyboard checks are run in the macOS app: WebKit by default moves Tab only between text fields unless the
  system's keyboard-navigation setting is on. Confirm how the app's WKWebView behaves before relying on a Tab walk
  there.

## Notes for the developer

- **Reusable capabilities**: Tailwind's `motion-reduce:` / `motion-safe:` variants.
- **Development notes**: the floating-pane logic decides "a popup is open inside this pane" from popup triggers
  (`aria-haspopup` with `aria-expanded`); adding `aria-expanded` to tree rows must not make it match them.
- **Reference docs**: `docs/memory/writing-ui-components.md`, `docs/product/window-layout.md`,
  `docs/product/sessions.md`.
