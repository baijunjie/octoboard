> Severity: Moderate

## Symptom

The task-list rules in `packages/ui/src/viewer/markdown.css` select on `:dir(ltr)` / `:dir(rtl)`, but the built
stylesheet carries `:not(:lang(ae), :lang(ar), …)` / `:is(:lang(ae), :lang(ar), …)` in their place, so a task
item's checkbox direction follows the document's `lang` rather than the item's own reading direction.

## Reproduction steps

1. In `packages/ui`, run `pnpm exec vite build`.
2. `grep -c ':dir(' dist/assets/index-*.css` — prints `0`.
3. `grep -o 'task-list-item[^{]*{' dist/assets/index-*.css` — the two rules read
   `task-list-item:not(:lang(ae),:lang(ar),…)` and `task-list-item:is(:lang(ae),:lang(ar),…)`.
4. For the rendering: open the gallery (`pnpm gallery`) on the viewer scenario built from the Arabic document in
   `packages/ui/src/gallery/fixtures/viewer.ts` (the fixture that holds task items reading from the right inside a
   page whose `<html lang>` is `en`) and look at where each task item's box sits relative to the list's markers.

## Expected vs. actual

- Expected: a task item's box sits with the list's markers whichever way the item reads — the stylesheet's own
  statement above those rules ("its box stays with the list's markers", "including in right-to-left and loose
  lists"); the product docs (`docs/product/project-pane.md`, "What the document shows") name GFM task lists but do
  not state the right-to-left case, so the fixer confirms this expectation.
- Actual: the built rules decide by the page's language, so every item in an `en` page gets the left-to-right
  rule; step 4's visible outcome is not yet verified (see Leads).

## Environment

`packages/ui` at `main` (also before this ticket's branch — the rewrite is in the old build as well); Vite 8.3 with
lightningcss 1.32/1.33 as the CSS transformer; any platform. Preconditions: a Markdown document containing task
items whose reading direction differs from the page's `lang` (the Arabic fixture is one).

## Scope of impact

Readers of Markdown documents with right-to-left task items, in the project pane's file viewer. No workaround.

## Leads

- Verified: the source rules use `:dir()`; the built CSS has no `:dir(` and carries `:lang()` lists in those two
  rules' positions. lightningcss documents `:dir()` as a selector it lowers to `:lang()` lists for browsers without
  `:dir()` support; the build's `targets` / `browserslist` configuration would decide whether it does so.
- Inferred: the box's position comes from `inset-inline-start` on the input (`markdown.css`, the
  `.task-list-item input[type="checkbox"]` rule), which resolves against the input's own `direction`; with the
  wrong direction the box lands on the side opposite the markers.
- Inferred: WebKit supports `:dir()` natively, so emitting it unchanged (or raising the build's targets) would
  restore the intended behaviour.

## Acceptance criteria

- [ ] The built `dist/assets/*.css` contains the task-list rules with `:dir(ltr)` / `:dir(rtl)`, or an equivalent
      that follows the item's direction rather than the page's language.
- [ ] In the Arabic viewer fixture, every task item's box sits with the list's markers, in both appearances.
- [ ] The left-to-right fixtures' task lists render as before (gallery comparison).
