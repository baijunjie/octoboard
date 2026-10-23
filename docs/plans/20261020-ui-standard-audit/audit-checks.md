# Completed check summary and remaining limits

Raw browser captures, command logs and source maps stay outside Git. This file retains only the check outcomes and
qualifications needed while the remaining milestones are open; it is not a substitute for rerunning affected checks
against current source.

## Completed milestones

- **03a:** exact HeroUI v2 Alert fills were checked in a 1,884-load matrix. The first concurrent run had one
  English/light load failure (1,883 clean); an unchanged-source serial retry passed that 471-load condition, and
  the other 1,413 conditions passed initially. The cause of the first failure is unknown. The user accepted the
  exact fills and the four light stock `Alert.Description` contrast exceptions (default/accent/success/danger).
- **05:** final focused tests 926/926, gallery 1,308/1,308 and actual-surface checks passed. Earlier focused runs,
  interrupted captures and measurement-probe failures remain historical failures. A removed-account first-hover
  tooltip observation has no established cause. These checks are source-bound and are not native speech evidence.
- **06:** final focused tests 1,124/1,124, gallery 1,084/1,084, actual measurements and interactions passed.
  Earlier stale expectations, type/build attempts, optimized-dependency 504 loads and probe/overflow failures were
  corrected or remain qualified as recorded in the external task archive. Decorative clipping is not treated as
  glyph visibility.
- **07:** final focused tests 1,335/1,335, gallery 1,080/1,080, Chromium/WebKit interactions and format/build
  checks passed. Earlier fixture, geometry, interaction and tooling attempts remain qualified; stock placeholder
  measurements are not new contrast passes. No real daemon, login or native speech check was performed.
- **08:** commit `6cbed0e7f638c87037b23a11c684c39ef324ada5` closes the content-width tab and Shell layout work.
  Final focused tests 29/29, UI build/typecheck/i18n/format passed. The 208 actual contexts were independently
  reviewed with complete provenance and the stated qualified scope. The canonical gallery passed once: 1,516/
  1,516, zero failures or retries (952 English/Chinese light; 282 affected English/Chinese dark; 282 affected
  Arabic light/dark). Seven omitted Settings fixtures add 28 unique contexts to the original 1,488.

### 08 qualifications retained

- The first 600px Shell case failed its harness panel locator; the second failed readiness because a resize handle
  shared the broad region selector. These were probe-domain defects, not product layout results. The corrected
  strict 11-case canary had one genuine RTL integer-endpoint assertion failure; native reachable scroll range was
  one pixel below `scrollWidth - clientWidth`. Physical ring, clipping, hit and panel guards passed. The probe was
  corrected to use physical directional edges without changing its sub-pixel visibility tolerance.
- The completed 208 contexts consist of 102 prior results, 24 qualified Advanced dialog rows, 60 WebKit core-tail
  rows and 22 final rows. Twenty-six oversized Shell cases are selection-only; they are not full-ring passes.
  Six Shell boundaries measured actual 351/352/353px content widths and confirmed title visibility behavior.
- The 24 Advanced rows passed all physical and interaction guards but their original strict stage command failed
  on Playwright page errors. A later isolated 24-row run matched 24 notices to 70 raw `ErrorEvent`s; all 70 had
  `error === null`, with no exception stack. This matches the existing runtime's native ResizeObserver notice
  classification, but the original strict failure remains a failure. The original 23 notices have unknown
  identities, and the original 84-row stage remains failed; its trailing 60 rows have independently reviewed
  complete guards and an error-free execution-log interval. No runner filter, assertion or timeout was changed.
- Earlier 08 attempts remain distinct: F5 had 180 pass / 1 fail / 143 unrun; initial readiness and RTL probe
  failures, the strict two-case diagnostic failure, and prior partial runs are not relabeled as passes or blamed
  on resource contention. The source-bound evidence index and canonical summary are retained under
  `/tmp/octoboard-08/`.

## Open milestone limits

- The original audit's broad workspace production build failed during Nuxt prerender with an `ENOENT` for the
  generated 404 page; its cause was not established. An accidentally broad UI test invocation ran 214 files /
  2,631 tests and failed 56 tests; it did not isolate a production regression. A 2,376-load optional gallery run
  exited 1 after 47 initial failures, all of which passed its built-in serial diagnosis; that result is not a
  clean initial run.
- Chromium and Playwright WebKit do not establish AppKit behavior, packaged WKWebView focus rings, VoiceOver
  speech or native-material contrast. Milestone 11 closes on candidate VoiceOver matches plus automated and browser
  checks, and records the unmatched writes as open items in `docs/voiceover-cases.md`.
- The old 1,636-load implementation matrix had three initial failures (two Chinese/light archive-session controls
  and Chinese/dark terminal-path-links); the cause remains unknown despite passing retries/comparison. Two other
  sweeps exited 130. The first authored Alert fill failed Chinese/light contrast at 3.838:1; the corrected fill
  is covered by 03a's qualified results above.

Open scopes and their accepted findings are in the 10, 11 and 14 briefs. Recreate current-source evidence for the
affected implementation; the historical source inventories and full raw logs were removed after their live scope
was transferred to those briefs. The remaining milestones must preserve these qualifications in their closure.
