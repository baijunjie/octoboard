> Severity: Moderate

# UI tests expect bidi isolates that the rendered strings lack

**Symptom**: ten `packages/ui` tests fail because the labels and tooltips they read have no Unicode isolate marks
(U+2068 … U+2069) around the interpolated values, while the tests expect them.

**Reproduction steps**:

1. On `main` at `810e63844`, run `pnpm install`, then in `packages/ui` run `npx vitest run src/App.test.tsx
   src/gallery/interact.hover.test.tsx`.
2. These tests fail:
   - `src/App.test.tsx` — "opens console session settings from the sidebar before launching" and "opens console
     session settings from the terminal before launching".
   - `src/gallery/interact.hover.test.tsx` — eight `hoverGitBadge` tests: "puts the pointer on the glyph or line for
     checking / idle / pulling / pushing, as asked", "puts the pointer on the line of a row's badge and puts up the
     badge's sentence", "takes the badge of the checking / pulling / pushing glyph the status draws".

**Expected vs. actual**:

- Expected (as the tests assert): `Open console session in ⁨Main⁩`, and git badge tooltips such as `Pulling branch
  ⁨main⁩, Git error: ⁨fetch failed⁩`, with each value wrapped in U+2068 … U+2069.
- Actual: `Open console session in Main` and `Pulling branch main, Git error: fetch failed`, with no isolate marks.

**Environment**: macOS (Darwin 25.2.0), `main` at `810e63844`; reproduced in a fresh worktree after `pnpm install
--frozen-lockfile`. `cargo test --workspace`, typecheck, lint and build pass on the same tree.

**Scope of impact**: the `packages/ui` test suite fails on `main`, so every delivery's full check reports these ten
failures. Whether users see wrong text is unknown: whichever side is wrong, the tests or the rendering, has not been
established.

**Leads**:

- Verified: the failures reproduce with the two test files run on their own (8 failures in
  `interact.hover.test.tsx` alone), so they do not depend on other test files.
- Verified: the change that was being checked when they were found touched only `apps/daemon/` and a doc, no UI code.
- Inferred: `docs/product/message-delivery.md` says the UI wraps values in isolates (U+2068, U+2069), so the rendering
  path for these strings may have lost them; recent `fix(ui)` copy changes (e.g. `9df400d90`) are candidates, not
  checked.

**Acceptance criteria**:

- [ ] The ten tests listed above pass.
- [ ] The full `packages/ui` suite (`pnpm -r --if-present test`) passes on `main`.
- [ ] Rendered labels and tooltips interpolate values in the form the product docs specify, whichever form that is.
