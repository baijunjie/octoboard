# Fading the edge of a scroller

## What the fade rules leave to you

Which module, which API and where padding, frames and rings go are in `docs/conventions/ui-components.md`, "Scrollers
and the fade". Beyond them:

- `EXEMPTIONS` in `packages/ui/src/components/scroll/scrollFadeCoverage.test.ts` is the whole account of what the rule
  leaves alone, so a scroller the check cannot see (its overflow comes from a library's stylesheet or another package's
  DOM) is declared there too, rather than passing unmentioned.
- jsdom has no `ResizeObserver`, which the fade needs to mount, so `packages/ui/src/test/setup.ts` installs a stub for
  every test file; a test that has to measure replaces it with `vi.stubGlobal`.
