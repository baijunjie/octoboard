# Checks and residual verification

> Raw captures and probes are archived outside Git. Evidence names below identify that archive, not repository files.
> Commands using those probes describe the historical collection; regenerate current-source checks for implementation.

The delivery scope is documentation and audit evidence only. No executable production or formal gallery source
was changed by the audit. A subsequent quota commit is a separate integration boundary. Formatting, whitespace/diff, links, inventory reconciliation, actual audit evidence and independent
review are the agreed delivery gates. Existing or unverified production-check failures are retained as failures;
they are not converted into green audit gates.

## Passed documentation and evidence checks

- `pnpm format:check` passed: repository ESLint with zero warnings and `cargo fmt --all -- --check`.
- `pnpm typecheck` also passed. This broader check is recorded without making it a new recurring audit gate.
- All evidence collector scripts passed scoped ESLint and `node --check`; no production instrumentation was added.
- Markdown links resolved, JSON parsed, and whitespace checks passed. Closure edits passed a fresh link/diff
  check after the completed milestone document was removed.
- Independent review reconciled all 52 standard headings, 425 parameterized messages, 565 key/parameter pairs,
  382 literal references, 123 dynamic expressions, confirmed findings and accepted 03–13 ownership. It reviewed
  full state/interaction/copy records, actual visual evidence and probe boundaries, not just a smoke result.
- Gallery attempted all 396 scenarios in six language/theme conditions. Its final exact outcomes and serial retry
  classifications are retained in the gallery record. A failed or loading target is not a visual pass.

## Additional checks that failed

### Workspace production build — failed, cause unverified

Command: `pnpm build` in the audit worktree. Exit 1. The website's preview bundle built, then Nuxt prerender returned
500 for routes and failed with `ENOENT` opening `apps/web/.output/public/404/index.html`.
`ERR_PNPM_RECURSIVE_RUN_FIRST_FAIL` identified `@octoboard/web` and its `node scripts/build-preview.mjs && nuxt generate`.
The parallel UI build did not yield a separately verified final success and is not counted as passing.

At the time of those checks, the audit and clean comparison checkout had no differences from baseline `3a65c903` in production `apps`, `packages`,
package manifests or lockfile. The audit used dependency symlinks; the comparison checkout installed the frozen
lockfile into its own dependency layout. This establishes unchanged source/dependency intent, not a reproduced
website failure or a diagnosed cause. No website rebuild, repair, assertion change or source fix was undertaken.
Label: **unverified environment/build failure on unchanged production source**, not "proven pre-existing defect".
If this build is needed by a later delivery, its owner must reproduce the failed Nuxt generation step in a clean
checkout with independent dependencies and retain the first underlying 500 error before deciding a fix.

### Unexpectedly broad UI test invocation — failed

Command: `pnpm --filter @octoboard/ui test -- src/gallery/fixtures`. The path did not constrain the actual run:
Vitest executed 214 files / 2,631 tests. Result: 8 files failed, 206 passed; 56 tests failed, 2,575 passed; exit 1.
Representative failures included SessionDialog wrong title/agent/default values, dialog/browser timeouts and suite
collection errors. The retained tail (archived evidence) is a partial log with exact final counts,
not a complete failure-root-cause inventory. No test, timeout, scenario, assertion or production source was changed.

Minimum baseline isolation used a detached, clean `3a65c903` checkout and independent frozen-lockfile dependencies.
Offline installation first lacked a locked notification-package artifact; the regular frozen-lockfile install
completed. No install/repair loop followed. One originally failed test was then run once:

`pnpm --filter @octoboard/ui exec vitest run src/dialogs/SessionDialog.test.tsx -t 'keeps the owner select, back on none' --maxWorkers=1`

It passed (one selected test; 44 filtered tests were not run). See baseline result (archived evidence).
That result proves only this isolated case passes under those conditions. It does not diagnose or pardon the full
suite's other failures. The broader failure remains **unverified dependency-layout/load/test-environment failure**;
a whole-suite regression or pre-existing production defect has not been established. No further suite debugging
belongs to this documentation task.

Future focused test briefs use `pnpm --filter @octoboard/ui exec vitest run <explicit paths>`, record selected files
and actual outcomes, and never infer a whole-suite pass from a focused comparison.

The optional six-condition gallery sweep is retained as **failed**, not a delivery gate for a docs-only change:
2,376 loads, 2,329 initially clean, 47 initial failures, all 47 passing its built-in serial diagnosis; exit remains 1.
Ten unresolved capture targets were separately retried once against the cleaned 396-scenario snapshot and passed.
Mixed capture/test load and scratch HMR mean the original failures are not attributed wholesale to pristine baseline.
No further broad sweep or production test run is required by this audit's agreed gates.

After collection, local main integrated quota work at `7a7a2f34e`. Original broad-check results and the focused
baseline comparison describe `3a65c903`, not verification of that 95-file integration. Rebase validation is bounded
to the owned documentation diff/links/inventory/review; 03/04/05 own the explicit fresh quota-source handoff.

## Native and runtime handoffs

No AppKit menu-bar accessibility execution, packaged WKWebView focus-ring observation, VoiceOver listening or fresh
native-material capture was performed. Source coverage and previously recorded verification cases are identified
in the interaction and contrast records; Chromium is not reported as native evidence. Existing open cases remain
open. These are evidence limits, not invented native failures.

The first dependent implementation must satisfy the corresponding runtime acceptance before it closes:

- 05: preserve error/diagnostic focus and rich/plain speech boundaries; inspect queued failures and real
  fixture-rendered titles, including import result and the separately integrated quota Alert routes.
- 07/08/10/12/13: verify actual focus handoffs, collection-managed controls, tab rings, reduced-motion cancellation
  and focused replacement, as required by their recorded findings.
- 11: match an existing passing VoiceOver case item by item or obtain fixture-backed packaged listening when none
  matches. Source or a Chromium accessibility snapshot cannot close that requirement. The milestone cannot be
  delivered with its required initial/later status verification omitted.

No implementation is being declared complete here, so a future fix's required verification has not been silently
moved out of its scope. Missing fixture targets are named at the relevant finding and in its accepted fix milestone.
Any gallery initialization failures retain exact scenario/condition and retry/verification handoff in the gallery
record. Independent final review checked this distinction before closure.

## Current source and retained check boundaries

The stock Alert implementation completed real-font verification using independent dependencies. Its final full
matrix passed 1,704/1,704 loads, exit 0, at `32ee8e015`, UI tree
`cf0433fb7ed840ddd619e9d13b40efb77c2af6e7`. The later default-effort integration produced UI tree
`1642b8c39b9c6919f9a130dff44b9ebb294e8654`; only the affected eight-load/27-test supplement plus UI build and
format checks certify that delta. Neither source key certifies later name-chip changes. The current rich-copy
handoff requires fresh current-source, actual-font and contrast/geometry checks.

Earlier implementation attempts remain failures or interruptions: a 1,636-load matrix exited 1 with three initial
failures (two Chinese/light archive-session controls and Chinese/dark terminal-path-links), despite serial retries
and a bounded clean comparison passing; its cause was not established. Two other sweeps exited 130. A first authored
Alert fill failed Chinese/light description contrast at 3.838:1; the corrected fill was independently measured at
least 4.609:1 light / 5.493:1 dark for default descriptions. These corrected values do not certify future rich chips.
Completed-only reports, raw logs and screenshots were pruned after transferring open dependencies; their deletion
changes none of the original audit failures or native/listening limits above.

The structured-error source was frozen at UI tree `d70662800ecdc77af200081d7493e2613892fb5e`, based on
`74707d7219ac126d90c9e0646b5ec23889ec2e50`. Scoped regression tests passed 23 files / 458 tests, followed by
focused assertion/isolation deltas; typecheck, UI-only build and repository format checks passed. Fifteen temporary
diagnostic-route mutations failed their targeted tests and were restored. An affected 260-load gallery run had
four `viewer-changes-errors` renderer-import failures and exited 1; all four combinations passed a cold frozen
replay. The initial import failure's cause remains unestablished. Earlier assertion failures were corrected and
rechecked; an ignored scratch probe caused an initial format failure and was removed before the passing run.
Chinese/Arabic light/dark browser inspection covered 108 loads plus model/login/durable-Git supplements, with
keyboard diagnostic scrolling and history acknowledgement checked. Chip geometry used actual loaded KaTeX fonts.
These browser checks are not native VoiceOver listening, real login or a website build.
The source freeze does not imply exclusive machine/browser load: another worktree's already-started scoped
Project-pane gallery batch overlapped part of the full sweep. Concurrency alone neither diagnoses a failure nor
requires repeating a passing check.
The final frozen full light sweep passed: `pnpm gallery:sweep --url http://127.0.0.1:5194 --jobs 2`,
453 scenarios × English/Chinese = 906 loads, 906 clean / zero failed, exit 0, 951.1 seconds. This certifies the
`d70662800ecdc77af200081d7493e2613892fb5e` UI tree. The same 65 affected scenarios in English/Chinese dark
passed 130/130 loads, zero failures, exit 0, 106.0 seconds, with `--theme dark --jobs 2`. Source identity was
confirmed before and after both sweeps. Raw completed-run logs and screenshots are not retained by default.

Local main then added only unrelated plan documentation at `0ae28459f`; rebasing preserved the exact verified
UI tree. On that rebased source, the explicit 23-file regression command passed all 458 tests again; workspace
typecheck, repository format checks and UI-only build passed again. Build output retains large-chunk and plugin
timing advisories. No whole-suite, website-build or native-listening pass is inferred.

The v2 Alert background check retained a failed concurrent run: 1,884 loads, 1,883 clean, one English/light
`focus-agent-tooltip-removed-account` failure, exit 1, despite passing its isolated retry. A serial English/light
run on unchanged runtime source subsequently passed 471/471; the remaining 1,413 language/theme cases were clean
in the original run. All 1,884 cases therefore have a clean latest condition result, but the initial concurrent
failure remains failed. Independent baseline comparisons did not reproduce it; its cause is unresolved, not a
proven baseline defect.
