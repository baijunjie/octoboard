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


## Milestone 05 rich references and tooltip closure

Milestone 05 closes COPY-02/03 and the application-caller half of COPY-09 on the audit aggregate, based on
`041aeb1a75c407888514c9a72b694c0d038a14cd`. The fresh English catalog inventory is 505 parameterized messages /
706 message-parameter pairs, including plural count parameters, against 494/684 at that base. These are inventory
counts, not render-route proof; the historical 425/565 inventory remains historical. Manual current-source closure
covered fixed agent/product/control references, owner and restore titles, session/model/Git metadata and tooltips,
login/trust/request/import fallback copy, ReasonText/accountUsage, quota refusal/recovery, and resume/history/error
callers. Whole entity/path rows and foreign source/terminal/report layouts retain their presentation.

`Message` renders referenced names through inherit-size nonmonospace directed wrapping chips; technical values
retain their code contract. Plain translation, native/accessibility/notification/announcement strings and raw
collection matching remain separate. Rich session/Git/quota copy helpers preserve their plain counterparts,
including `quotaUsedUpAlert` and `quotaUsedUpSentence`. Shared field details, complete/summary/details routing,
producer IDs versus literal names, startup-safe error records, resume-aware gating, acknowledgement, retry,
context and viewer/report focus behavior retain the 04/12 contracts. At milestone 05 closure, milestone 06's choice/status/direction work
remained open; the field and option APIs established here are reused in its closure below.

The final global capture freeze covered all 727 UI/control files, including new runtime files, with SHA-256
`f5bb9f903e78af9383ae355cfbbf02234a199af37e4e47979c681e8ca4ca4ff1`. The file set, every hash and base HEAD matched
before and after captures. Independent dependencies and real fonts were used; no source or fixture mutations
occurred during that capture window. Subsequent changes are documentation only.

- `pnpm --filter @octoboard/ui exec vitest run` with 52 explicit paths passed 926/926 tests. Selection included the
  new rich/plain i18n, quota, session metadata and option contracts; retained stock Alert/error-router contracts;
  affected dialogs, settings, import, matching and metadata callers; resume/history/report/viewer behavior; and
  catalog/gallery tests. The exact argument list is retained in task outputs outside Git. This is a focused result.
- `pnpm --filter @octoboard/ui build` passed typecheck, five i18n files / 30 tests and the UI build, retaining the
  existing chunk-size/plugin-timing advisories. `pnpm format:check` passed ESLint with zero warnings and Cargo format.
- The unchanged gallery runner, with `--jobs 1`, passed 472 scenarios × English/Chinese light: 944/944 loads,
  zero failures, 1503.2 seconds. The 91 affected scenarios passed English/Chinese dark: 182/182, 297.2 seconds;
  Arabic fallback in both themes passed 182/182, 297.4 seconds. Canonical total: 1,308 loads, zero failed.
- The affected set includes all 13 current resume fixtures, both history failure states and all five named quota
  scenarios: new-session/handoff all-used-up use Codex Default/Team; switch-account uses Work; new-session refusal
  names Work; quick-create refuses the default agent. They are not five Work fixtures.
- Actual-font/parent measurements passed 276/276 conditions across both themes and English/Chinese/Arabic fallback:
  2,256 chip placements, 96 Alerts and 84 CodeBlocks. Minimum chip text contrast was 4.511:1 light / 5.124:1 dark;
  CodeBlock text was at least 5.823:1 / 8.545:1. All statuses and dark alpha compositing were measured. Only the
  previously accepted four light stock Alert.Description exceptions apply; no chip/title/indicator/control/dark
  exception or foreground override was added. The exact v2 fills remain unchanged.
- `gallery-chip-geometry.mjs` passed both language groups / ten loads: 12/14/16px baseline geometry, 390px wrapping
  and RTL, import path equality and trust sentence containment. Actual interaction checks passed 54/54 conditions:
  rich hover/focus tooltips, accessible names and repeated-description suppression, raw native/owner typeahead,
  per-label marquee and reduced motion, quota controls, and measured viewer clipping/wrapping. English and Arabic
  subtitles exercised rich clipped tooltips and key suppression; Chinese fit/wrapped within bounds.

Earlier attempts remain failed or interrupted. Focused test attempts had 200/207, 170/180, 186/189, 300/370,
835/836 and 168/169 passing before expectation/import/selector corrections; none is relabeled a baseline pass.
The initial static check had two quote errors, corrected before the passing check. An early invocation also named
a nonexistent LanguageSetting test path; the final 52-path command contains only existing tests.
Three earlier full sweeps were interrupted before source repairs. A later 944/944 full pass certified freeze
`bb8191d1dea1eb734b4fbe6b9970fc0a909eb9e0429a4f94040f1292788ddf66`, before gallery-parent corrections; the final
1,308-load record above certifies the final freeze instead.

The initial actual measurement was 243 passing / 33 failing conditions: 24 imposed inline display on normal direct
flex/grid blockification; nine sampled light technical chips on gallery-body parents, including inactive viewer
launchers behind a modal. CodeGallery standalone accent/success/warning readings on that body were
4.215/4.022/4.283:1. Gallery-writer put comparison specimens on representative existing product surfaces; native chip
colors and Alert fills were retained. A following measurement was interrupted after reaching another transparent
nested specimen parent; that parent was corrected and independently reviewed before the final freeze. These body
readings are not exceptions for product chips. Final measurements inspect actual active product parents.

An initial interaction probe was interrupted; a following 54-condition probe passed 40 and failed 14. Fixture/control
scope, settled-state/visibility waits, measured locale-dependent clipping and exit-animation observations were
corrected without runtime changes, then 54/54 passed. The earlier Git-tooltip observation's cause remains
unestablished; no cause is inferred for the separate 03a removed-account failure or the separate 13 menu issue.
The Vite preview logged ResizeObserver-loop notifications; their cause was not diagnosed. Gallery totals refer to
the unchanged runner's existing assertions, rather than a warning-free-console claim.

Independent change checks passed the shared rich/plain and gallery-parent changes, application callers and
metadata/tooltip suppression, explicitly including fixed agent/product references and preservation of 04/12.
Sequential map/product/memory documentation work retained one authority for each rule. The preview and owned
browsers were stopped, the preview port and service record cleared, and no scratch scenarios or staging snapshot
remain. Raw task logs, commands and captures stay outside Git. No real login, live daemon, website build, packaged
WKWebView or VoiceOver listening was performed; the earlier native/listening limits remain unchanged.


## Milestone 06 choice status and direction closure

Milestone 06 closes COPY-04/05, the numeric-ratio half of COPY-06 and COPY-08 on the audit aggregate,
based on `5c2dfcf3c4e6187adcd5d5c41d018b7b2ca038a1`. Fresh English catalog inventory is 506 parameterized
messages / 707 message-parameter pairs, against 505/706 at that base. Historical inventories do not certify this
source. Manual closure covered Add agent's four statuses; missing stored model/effort trigger and popup choices;
gone branch and pending comparison; inherited account/model/effort; import choice/editable-directory labels,
not-JSON and fallback details; tag removal; and the drawn character ratio. Current resume, held-owner,
`useLocatedConversation`, `useResumeTarget` and `useLaunchChoice` routes were refreshed: fixed disabled single-option
agent/account fields, raw holder matching, stale-response suppression, plain lookup status and external details remain
intact. Shared rich option/field APIs, raw labels/keys/textValue/defaults/submitted values, AgentTabList's approved
warning glyph, and the 04/05/12 routing, tooltip and focus contracts are preserved.

Add agent and gone-branch status use the shared badge after the structured name. Name references use inherit-size,
nonmonospace directed wrapping chips; technical sentence values use inherit-size LTR code chips. Comparing-base rich
copy has a separately isolated plain announcement. Only the count/limit ratio is LTR; translated surrounding prose
retains its direction. A local tag-suggestions tooltip anchor prevents ComboBox InputGroup's injected siblings from
sharing its tooltip focus context; raw tag matching/removal and keyboard behavior remain intact. Joined agent/account selectors share width so both status badges remain inside their triggers.

The global capture freeze covered all 731 UI/control files, including new tests and runtime/configuration candidates,
with SHA-256 `9b9362f3a293a5369ec0d667de3c6a2159cf2eedfd59ddd5a6589767bd18bb5c`. File set, every hash and base HEAD
matched across captures. Dependencies were independently installed and actual fonts loaded. All gallery writes were
made by gallery-writer and used fixture-daemon state. No source or fixture mutations occurred during final captures.

- The explicit 64-path Vitest command passed 1,124/1,124 tests, including rich/plain/native/matching regressions,
  current launch/resume/import/catalog/select/branch behavior, retained stock Alert contracts and report/viewer focus.
  This is a focused result, not a whole-suite pass. Exact commands and outputs remain outside Git.
- UI-only build passed typecheck, five i18n files / 30 tests and Vite, retaining existing chunk-size/plugin-timing
  advisories. Repository format checks passed ESLint with zero warnings and Cargo format.

- The unchanged gallery runner with `--jobs 1` passed 474 scenarios × English/Chinese light: 948/948 loads,
  zero failures, 1511.9 seconds. The 34 affected scenarios passed English/Chinese dark: 68/68, 128.2 seconds;
  Arabic fallback in both appearances passed 68/68, 128.2 seconds. Canonical total: 1,084 loads, zero failed.
  The affected set includes all 13 current resume variants and both new gallery fixtures: retained effort choices
  and the durable gone-worktree branch trigger. The existing comparison-failure fixture is not gone-trigger proof.
- Actual-font/parent measurements passed 204/204 conditions (34 scenarios × three languages × both appearances):
  528 chip placements, 120 badges, six Alerts and 18 ratios. Minimum chip text contrast was 5.464:1 light / 5.507:1
  dark; badge minima were 7.026:1 / 6.578:1. The six Alerts include the stock light danger description at 4.117:1,
  covered by the previously accepted exception. Only the four accepted light stock Alert.Description exceptions
  apply; no chip/title/indicator/control/dark exception or foreground override was added. Exact v2 fills are unchanged.
- Joined-control badge geometry passed 18/18 conditions at 1440/600/390px × English/Chinese/Arabic fallback × both
  appearances. The unchanged chip geometry script passed both language groups / ten loads: 12/14/16px baseline,
  390px wrapping/RTL, import path equality and trust sentence containment.
- Final actual interactions passed 90/90 conditions: 54 in the settled-tooltip run plus 36 supplements. They cover
  raw typeahead/arrow selection, clearing, dialog Enter submission, all Add agent statuses, gone/surviving branches,
  comparison rich/plain separation, inherited choices, raw tag add/remove, import-label focus and fixed resume
  fields. Eighteen final tooltip captures have opacity 1 and fit the viewport; measured rich tooltip chip contrast
  was at least 13.975:1 light / 10.602:1 dark. Earlier 54/54 behavioral results remain separate from this corrected
  settled visual measurement.

Earlier attempts remain failed or interrupted. An initial focused run passed 1,120/1,124 tests; four ConsoleDialog
assertions still expected the pre-05 flattened sentence rather than its rich summary, isolated plain announcement
and external details. Those contracts were asserted separately before the final passing run. Build attempts caught
an unsupported public Trigger ref and intermediate focus/union typing errors; the final implementation uses public
event callbacks and typechecks. Initial local preview loads failed with stale optimized-dependency 504s; only the
owned preview was restarted cold, without changing dependency versions.

The first actual measurement passed 186/204 conditions and failed 18: 12 used a Description-only selector although
invalid counts are in FieldError, and six compared chip decoration to a prose edge rather than distinguishing glyphs
from padding and actual clipping. The next run passed 198/204 and failed six. Four exposed a real joined-account badge
clipping problem when the agent half took up to 70% of the row; sharing width corrected it, with fresh checks at
1440/600/390px in all three languages and both appearances. Two Arabic resume conditions needed separate glyph and
decoration measurements. Measured glyphs fit the prose and actual clipping surfaces. Retained observations include
import decoration 2.3125px beyond the paragraph but inside its scroll surface, Git metadata decoration clipped by
4.59375px at its existing FadeOverflow boundary, and Arabic resume-path decoration clipped by 2.265625px while glyphs
remain inside the scroll surface. These are measured decoration observations, not baseline-proven defects or a
permission for glyph overflow. No shared chip styling or palette was changed.

Early interaction probes passed 48/54 and 52/54, with a separate interrupted attempt. Removal/exit settling and a
stationary pointer over a different removal target after reflow were corrected in the probe; raw removal, focus,
matching and tooltip assertions remain. Independent evidence review also caught a tooltip sampled during entrance
at opacity 0.112623: those contrast numbers did not describe settled paint. The bounded final tooltip check waits
for animation completion, asserts every measured ancestor has opacity 1, and checks viewport bounds before sampling
and capturing. The 204 static conditions had no sampled ancestor opacity below 1. No cause is inferred for the
separate historical 03a removed-account failure or the separate 13 menu-focus discrepancy.

Independent source and evidence change checks passed, explicitly including names referenced in visible copy through
shared InlineChip, fixed agent/product references, rich display versus plain accessibility/native/matching boundaries,
and preservation of 04/05/12. Sequential map/product/memory documentation work retained existing authorities; no
map changes were needed and all seven code-tree READMEs remain reachable. The preview and owned browsers stopped,
port and service record cleared, and no scratch scenario remains. The source freeze and bulk slot were released after
final captures. Raw logs, commands and captures remain in task outputs outside Git. No real login, live daemon,
website build, packaged WKWebView or native VoiceOver listening was performed; prior native/listening limits remain.
