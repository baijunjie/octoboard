# Gallery evidence

> Raw captures and probes are archived outside Git. Evidence names below identify that archive, not repository files.
> Commands using those probes describe the historical collection; regenerate current-source checks for implementation.

Baseline: `3a65c903ee040533c898069e96ba5216f296bc39`. This audit changes no production UI or formal gallery fixture.
The catalog contains 396 scenarios in 20 groups. Evidence uses the real application over gallery fixture data,
Chromium headless shell, desktop chrome enabled, 1440 × 900 unless the scenario supplies its own dimensions.
Arabic is a direction probe: it renders the English fallback catalog under RTL; Simplified Chinese is the actual
non-English catalog. Browser evidence does not claim packaged WKWebView, VoiceOver listening, native menu-bar
accessibility, native window material rendering, or real daemon behavior.

The subsequent 95-file quota integration at `7a7a2f34e` is not certified by this record. Reproduce its 396-scenario
snapshot from an isolated `3a65c903` checkout; current-main validation must include the new quota scenarios handed
to 03/04/05, rather than assuming these counts or captures describe the rebased tree.

## Reproduction and evidence boundaries

Start an owned gallery from the checkout with `BROWSER=none pnpm gallery --port 5186 --host 127.0.0.1` in
`packages/ui`. Direct entries use `/gallery-frame.html?scenario=<id>&theme=<light|dark>&lang=<en|zh-Hans|ar>&chrome=1`.
Run `pnpm gallery:sweep --url <origin> --theme both --lang en,zh-Hans,ar` for the full six-condition smoke pass.
The collector in gallery-probe.mjs (archived evidence) runs from the repository root. Set
`AUDIT_GALLERY_ORIGIN`, `AUDIT_OUTPUT`, and `OCTOBOARD_CHROMIUM` to the owned service, output directory, and an installed
browser. Optional positional scenario IDs select captures; `AUDIT_LANGUAGES` and `AUDIT_THEMES` choose viewing
conditions. It writes computed geometry and screenshots, not an accessibility verdict.

The initial capture pass records all scenarios in English/light and representatives from every group in
Chinese/dark and Arabic/dark. The six-condition sweep is separate. A ready marker and a clean smoke result prove
neither visual correctness nor complete lazy content: screenshots displaying a loading surface are recorded as
that state, and do not certify the later reading surface. Collection transitions, hover targets and keyboard paths
require the focused probes below. A screenshot contact sheet establishes a visual overview; it cannot establish
precise contrast, target geometry or spoken announcements.

## Confirmed visual findings

### G01: horizontal tab strips retain unused background or stretch labels

Rule: “Roles and states / Expanded and selected” requires every tab strip to use its content width and stay
start-aligned. Source sites: `dialogs/SessionDialog.tsx:239-251`, `dialogs/ConsoleLaunchFields.tsx:86-100` and
`components/AgentTabList.tsx:60-76`, under `packages/ui/src`. Computed measurements distinguish the tab widths from the painted list's width:

| Surface and scenario | Actual measured strip and labels | Scope |
| --- | --- | --- |
| Project session, `dialog-new-session-advanced`, Chinese/dark | Strip 392px; New and Resume each 192px despite their short labels | Remove equal allocation and constrain the list to content; preserve keyboard selection and focus-ring clearance. |
| Console Advanced, `dialog-new-console-advanced`, English/light | Strip 678px; agent tabs total 363.14px | Constrain the list itself, retaining the existing content-sized tabs. |
| Settings agents, `settings-accounts-many-narrow`, Chinese/dark | Strip 424px; tabs total 363.14px plus 12px padding | Repair the shared agent-strip list sizing and check all callers. |
| Quota floating panel, `rail-quota-hover`, Chinese/dark | Strip 384px; two tabs total 238.72px plus 12px padding | Include quota panel in the shared agent-strip fix. |

The Files / Changes strip is content-sized: Chinese tabs 40px each in a 92px strip including 12px padding.
Settings' narrow top-level navigation is also content-sized: labels total 687.03px plus 12px padding in a 699.03px
strip. The wide vertical Settings navigation uses a 192px column and 184px tabs; this is recorded separately and
is not called a horizontal-strip defect. Evidence: tab measurements (archived evidence),
project session (archived evidence),
Console Advanced (archived evidence),
Settings agent strip (archived evidence),
quota panel (archived evidence).

### G02: reduced motion leaves custom transitions active

Rule: “Motion” requires nonessential animations and transitions to stop under reduced motion. Source sites:
`archive/ArchiveView.tsx:278`, `sidebar/rows.tsx:91`, `sidebar/Sidebar.tsx:578` and
`components/ResizeSeparator.tsx:169`, under `packages/ui/src`; the complete handoff is I-04 in
[the interaction audit](audit-interactions.md).
The browser context used `reducedMotion: "reduce"`. In `archive-console-sessions`, custom archive row-action wrappers
still computed `transition-property: opacity; transition-duration: 0.15s`. Sidebar custom rows still computed their
color/background/border/fill/stroke transition family at 0.15s. The custom resize grip also retained a color
transition at 0.15s. Entries whose property computed `none` are not failures even where a duration remained nonzero.
Scope: guard these custom transitions, preserve immediate action availability, and verify reduced-motion collection
cancellation without replacing shared list motion. Evidence: keyboard and motion probe (archived evidence).
The code-audit record owns exact source locations and the complete caller inventory.

### G03: no installed agent uses warning imagery for normal absence

`empty-console-no-agent` visibly uses a warning triangle in the main empty state and sidebar row; the added-but-not
installed distinction is separate from failure to load. `empty-console-no-agent-added` uses the neutral agent glyph.
The standard says normal absence uses a neutral or purpose glyph, never a warning glyph. Source sites:
`packages/ui/src/terminal/TerminalPane.tsx:492` and `packages/ui/src/sidebar/Sidebar.tsx:634`, corresponding to
S05 in [the state audit](audit-states.md). `Sidebar.tsx` describes
its current warning choice in a comment, but that description alone is not user approval of a new exception.
Scope: use a purpose glyph for ordinary absence in the shared terminal/sidebar states; retain environmental
explanations and omit creation actions when no agent is installed. Evidence:
Chinese dark absence (archived evidence),
neutral absence comparison (archived evidence).

### Alert evidence and branch boundary

`focus-console-new-session-owner-gone` reproduces main's pink blocking Alert with the short foreground-colored
message. Startup and trust screenshots show the shared nonstock Alert content/typography treatment. These are
instances of the shared Alert finding in the main audit; the fix must retain only the approved status fill and
restore HeroUI indicator/content/title/description, status title color, and all other original treatments.
Evidence: main owner-gone blocker (archived evidence),
startup RTL (archived evidence),
trust prompt Chinese dark (archived evidence).

The user's quota-exhausted Work screenshot came from quota-gating edits subsequently integrated at `7a7a2f34e`, outside this evidence baseline.
It is not fabricated as a main scenario. The shared-component migration handoff must explicitly include those newly committed
callers when milestone 03 migrates the current inventory; no dirty worktree was inspected here.

## Focused interaction results

The F6 probe walks eight times from each fixture's settled focus. `files-tree` cycles Files → top bar → rail →
sidebar → agent terminal and repeats. `archive-console-sessions` includes Archive instead of the covered terminal.
`empty-console-no-agent` skips the inert terminal and includes top bar, rail and sidebar; the no-agent-added state
also includes the terminal region's Add agent action. Nonterminal handoffs carry `data-focus-visible`. An open Settings
modal holds F6 rather than cycling behind it. These are observed paths, not a claim that every focus handoff passes.
The terminal input's own focus drawing is renderer-managed and is not judged by a missing `data-focus-visible` alone.

Chinese/dark Tab traversal was exercised on project-session fields, narrow Settings, the directory picker and the
floating quota panel. ArrowDown was sent on `git-changes`; the raw focus sequence records whether the starting focus
was actually in the collection, so it does not stand in for a successful section-heading walk. Queued-question focus,
held-key suppression, shadow-root separator traversal, late-announcement delivery and real VoiceOver speech remain
owned by the separate interaction/code evidence and its explicit validation requirements.

## Contrast evidence

The representative shared ActionMenu header trigger was measured at rest and hover on focus-project, Files and
Archive fixtures in both themes. The glyph's final computed token color and each ancestor background were converted
to sRGB through a CSS-color canvas; translucent fills were composited from the root inward. The recorded ratios
come from these colors, not screenshot pixels. Rest is 5.45:1 light / 5.75:1 dark; hover is 13.43:1 light / 11.86:1
dark. These exceed the custom glyph threshold for the sampled surfaces. Evidence includes every ancestor color:
action contrast (archived evidence).

This sample does not certify selected row actions, every custom label or status, translucent chip labels, animation
minima, or native material. Those remain distinct checks in the global contrast inventory. HeroUI's own defaults,
decorative fills and approved exceptions are not recast as custom-style failures. In particular, weak Alert fill
separation is not itself a defect; the required severity glyph and stock parts must be retained.

## Full-gallery record

The per-scenario matrix and final smoke outcomes accompany this record. Visual inspections identify the actual
captures viewed and their conditions. Any failed scenario remains failed; no fixture, step, check or assertion was
removed or relaxed. Baseline failures are recorded separately from standards findings and require an unchanged,
serial rerun before attributing them to an application defect.

### Selected-row actions, RTL columns and real marquee input

A selected Hub card in `focus-console-session-selected` was measured in light/dark × English/Arabic with actions
resting, hovered, keyboard-focused and menu-open. The selected-surface glyph ratios are 4.94:1 light / 5.05:1 dark
at rest and 12.21:1 / 10.37:1 with the active trigger treatment. Unlock and Actions have 24px layout targets and
4px gaps: English x positions 260 / 288 mirror to Arabic 1156 / 1128, at y132. The stock press transform briefly
reduces a bounding rectangle during opening; that transient rectangle is not classified as a custom target defect.
The fixture includes a persistent lock and selected actions; hidden/unselected cases are covered by the visual
catalog, not inferred from these selected-row measurements. See
selected-row colors and geometry (archived evidence).

Real pointer input in `focus-project-long`, Arabic/dark, independently hovered the clipped console and project
labels. Only the pointed label moved (observed `scrollLeft` 54 and 60 respectively); the neighboring labels stayed
at zero. Leaving reset each to zero, and hovering the neighboring Actions button left all labels still. Under
reduced motion all positions stayed at zero while full-name tooltips remained available. See
marquee probe (archived evidence). These observations support the shared label behavior for this
representative long-text surface; timing through an entire repeated reading cycle was not measured.

### Additional surfaces and safe exclusions

The usage tooltip's application-authored `dt.text-muted` labels were tested separately from stock HeroUI text.
They pass on the actual tooltip: 4.83:1 on white in light and 6.75:1 on `rgb(24,24,27)` in dark. See
computed tooltip colors (archived evidence),
light tooltip (archived evidence) and
dark tooltip (archived evidence). This is measured evidence against adding a contrast finding.

An ignored scratch scenario derived from `dialog-directory-picker`, replacing `directories` with
`{ "/Users/dev": [] }`, shows the previously missing empty-directory target without touching production code or
formal fixtures. It displays a disabled plain “No subdirectories” row without the shared compact EmptyPanel glyph.
This confirms the empty-state finding's missing-scenario branch:
Chinese dark empty picker (archived evidence),
English light empty picker (archived evidence).
The scratch scenario is temporary and is removed during cleanup. Its insertion changes the dev-only fixture module;
concurrent module-fetch failures during that insertion are not attributed to the baseline without a stable rerun.

The viewer requires subject coverage beneath the scenario catalog: `viewer-files` has 31 subjects,
`viewer-changes` has 28, `viewer-document` has two, and each of the three additional Markdown styles has one.
The subject walk captures all of them in English/light and Chinese/dark, plus the scratch picker (130 observations).
Viewer subject evidence (archived evidence) records named reading regions, MathML presence and
shadow-root controls; source text is stored as a bounded excerpt plus length and SHA-256 rather than duplicating large
fixture files. Exceptional file, change and document subjects are also visited after the required acknowledgement,
retaining their durable error content in exceptional subject evidence (archived evidence).
Normal source, Markdown, foreign repository markup, image/binary/large-file fallback, empty content and changed-file
shapes were inspected in contact sheets. An initial source or diagram capture that still says Loading is not
classified as a settled reading surface; separate settled captures address those cases.

## Completed capture inventory and inspection scope

Capture matrix (archived evidence) contains 831 unique viewing-condition records:
396 English/light, 396 Chinese/dark and 39 Arabic/dark representatives. All 827 existing captures were viewed in
contact-sheet overviews; four navigation/ready failures produced no capture. Six captured views carry explicit
failure overlays. The original failure records and screenshot hashes are retained rather than replaced with
successful retries. DOM observations (archived evidence) supplies the corresponding tab, input,
direction and landmark records. Each overview checks the view actually drawn for its empty/error/glyph/label/tab
shape and direction; it is not a claim to have activated every control or measured every color on that view.

The inspection sequence covered the initial 30 captures, the remaining initial pass in successive 20-capture sheets,
the 357 additional dark captures in 18 sheets, all 130 normal-subject/picker captures in seven sheets, and the 20
exceptional viewer-subject captures in one sheet. The matrix identifies every exact scenario and condition, including
failed and loading observations. Focused full-resolution inspections are preserved as the linked evidence images.
This inventory covers all baseline scenario IDs; the temporary empty-directory scenario is recorded separately.

The intended first source and first diff subjects were recaptured after their Loading presentation disappeared;
both have an actual `diffs-container` shadow root. The document was recaptured after its named Request flow region
existed, showing both rendered diagrams, inline/block formulas, the mixed-direction paragraph and internal links:
settled source (archived evidence),
settled diff (archived evidence),
settled document (archived evidence),
reader metadata (archived evidence).

To reproduce the targeted measurements, run `node` on
the selected-row probe (archived evidence) or
the marquee probe (archived evidence) from the repository root with the same environment variables
as the collector. The selected-row probe records computed colors and composites them from the root inward; opacity
of a hidden ancestor must be checked separately before applying its result to a visible state.

### Shadow-root controls

The existing `git-change-viewer` fixture supplies the collapsed contexts absent from the small standalone subjects.
In English/light, Chinese/dark and Arabic/dark, Tab from the named code region enters the first named separator
control and sets its manual `data-focus-visible` attribute. Enter expands the leading gap and hands focus to the
next available named separator with that ring still present. Before expansion the renderer exposes 20 role-button
nodes including hidden/non-tabbable helper copies, with 11 tab stops. The visible text control's name includes its
count (“Show all 6 unmodified lines”, localized in Chinese). The one-control bar is 28px tall; the paired controls
are 20px each inside a 40px bar, preserving the expressly approved target-size exception. This is supporting evidence,
not a new shortfall. See keyboard/geometry observations (archived evidence),
English light (archived evidence),
Chinese dark (archived evidence), and
Arabic dark (archived evidence).

The picker placeholder probe includes inputs with an implicit text type, not only `[type=text]`: its named Directory
path control omits a `type` attribute, has `/Users/dev` as its value and `dir="ltr"`, and has no placeholder.
Actual picker input (archived evidence) supports I-01's source finding; the initialized value does
not satisfy the placeholder requirement.

## Official smoke result

The full, unchanged command ran 396 scenarios × English/Chinese/Arabic × light/dark = **2,376 loads** at concurrency
six. It completed in 2,276.1 seconds with **2,329 clean and 47 failed** initial loads. Its built-in serial diagnosis
re-ran all 47 failures, and **all 47 passed alone**. The command nevertheless exited **1**, as its contract requires:
this audit does not convert load-only failures into a pass. Every case is retained in
the six-condition result matrix (archived evidence) and
the command log (archived evidence).

The primary run overlapped capture/probe workloads and the parent's verification, and one ignored scratch fixture
was added while it ran. That fixture module invalidation can account for a module-fetch error; the evidence does not
attribute every initial failure to unchanged baseline behavior. The automatic serial diagnosis ran with the fixture
module stable. It demonstrates no persistent application defect. No scenario, mock, step, assertion or check was
changed, skipped, removed or weakened; no production UI was changed. Native speech/material and other stated browser
limits remain explicit rather than being represented as passed checks.

## Final retry and cleanup

After the official run ended, the audit-owned scratch fixture was removed. Only the ten unresolved initial capture
cases were then re-opened serially against the 396-scenario baseline: **10/10 reached ready without a failure overlay**.
All ten resulting captures were visually inspected. Their independent records are in
the capture retries (archived evidence); the capture matrix retains both original results and
retry hashes. No persistent failed ID remained from the official serial diagnosis, so no further broad rerun was
performed. The original official command remains failed.

The owned gallery service on port 5186 was stopped, and its ignored scratch scenario and service record were removed.
No formal fixture, mock, gallery checker, production UI or other owner's service was modified. The audit's three
reproducible gallery scripts passed ESLint and Node syntax checks. Repository-wide validation is reported by the parent.

The checkout used shared dependency symlinks. On service shutdown, Vite's diagnostics included KaTeX font requests
outside its serving allow list. Formula evidence therefore establishes rendered content, MathML, names and direction;
it does not certify the packaged KaTeX font metrics. No contrast or typography defect is inferred from that environment
limitation. Native speech, packaged WebKit behavior and live window-material rendering retain the earlier stated limits.
