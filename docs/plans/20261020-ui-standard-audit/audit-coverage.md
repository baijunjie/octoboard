# Audit coverage and delivery record

Baseline: main `3a65c903ee040533c898069e96ba5216f296bc39`. The audit worktree is isolated on
`docs/ui-standard-audit-02`. Findings describe that committed source. The later quota integration is not included in that snapshot;
see the concurrent quota/resume integration handoff in [the state audit](audit-states.md).

The audit is **closed after independent review and documentation checks**. The user accepted the exact 03–13 fix split and sequential implementation after the audit passes. No production UI, formal fixture, standard or
protocol behavior is changed. The audit may confirm source-rule mismatches without a fixture; those entries name
the missing scenario and require it during the fix. They do not claim a reproduction that was not performed.

After collection, quota-gating was integrated into local main at `7a7a2f34ebc19c5bb2ee29f9ae804354c2a40ad3`.
This documentation may be rebased onto that commit, but the source/standard/capture/index evidence stays pinned to
`3a65c903`. Current-main quota Alert/name/recovery routes and five new fixture IDs are explicit re-inventory handoffs
to 03/04/05. No global re-audit or production build of the concurrent integration is claimed.

## Evidence map

| Record | Coverage and evidence |
| --- | --- |
| [Alerts, states and navigation](audit-states.md) | Complete Alert caller inventory, S01–S06, empty-state/creation/navigation/heading/link/tab surfaces and concurrent-integration handoff |
| [Copy and badges](audit-copy.md) | COPY-01–09, each rich/plain render route, noncatalog and daemon routing, exact scenarios or fixture gaps |
| [Interpolation index](audit-interpolations.md) / [machine inventory](audit-interpolations.json) | 425 parameterized messages, 565 parameter pairs (including plural-selector count), 382 literal references, 123 dynamic expressions, English and Chinese catalog locations and route classifications |
| [Interactions](audit-interactions.md) | Production surface inventory, every editable input, I findings, keyboard/focus/names/roles/direction/motion code and unresolved probes |
| [Gallery](audit-gallery.md) | Actual capture/inspection ledger, light/dark and translated Chinese, Arabic RTL, measured tabs, pointer/keyboard/motion probes, exact smoke outcomes and baseline failures |
| [Contrast](audit-contrast.md) | Custom selector/token/consumer inventory, token conversion/composition, actual-state gallery sampling, default-style exclusions and native-material limits |

Counts are inventory coverage, not counts of defects. Parameter classifications never exempt a fixed agent/product
name merely because it comes from a finite table. A name referenced in rich visible prose gets the name rule;
whole structured entity rows, technical paths, counts and native/accessibility strings retain their own contract.

## Every standard section

Each row records the concrete source surfaces checked. The linked evidence records contain baseline line numbers,
scenario IDs, measured observations and explicit limits. “Source” does not claim browser, speech or native execution;
actual interaction and capture coverage is identified in the gallery ledger and probe artifacts.

| Standard heading | Checked consumers and disposition | Evidence record |
| --- | --- | --- |
| Conformance level and exceptions | HeroUI defaults separated from authored styles; explicit exceptions below preserved | Contrast; States |
| Confirmed visual choices | Alert-only fill, full-opacity tab hover, StatusBadge, notice toast indicator, tooltip portal fills | S01/S02, COPY-04, Contrast |
| Keyboard | Dialog choice Enter/Space, confirm/unprompted/typed focus, held-key handling; terminal escape | Interactions surface table; Gallery focused probes |
| Reaching a control with Tab | Collections' row/arrow model, console column order, viewer float actions, change headings, empty panel tab stop | Interactions; Gallery tab/collection probes |
| Buttons | Shared HeroUI wrapper, justified custom shadow/separator/scrim controls, collection chevron primitive reviewed separately | Interactions I-08; justified section-heading exception |
| A control inside a rendering library's shadow root | rendererDom separator names, duplicate markup, native title exception, manual Tab traversal | Interactions viewer inventory; viewer gallery/error snapshots |
| Leaving the terminal with F6 | App region visibility, terminal, panes/panel stack, floated quota, archive, modal suppression | Interactions; Gallery F6 trace |
| A floating panel that does not take the window | Quota nonmodal pointer/keyboard opening, controls ID, topmost Escape, tooltip suppression | Interactions RailQuotaEntry inventory; Gallery quota probe |
| Input placeholders | Every editable text input, required/optional semantics and context changes | Interactions editable inventory; I-01/I-02 |
| Visible focus | Library rings, custom data-focus-visible, tab clearance/scroll reveal | Interactions; Gallery nonterminal F6 markers |
| A hand-built focusable element | Rows, separators, scroll regions, ExternalLink and renderer controls use shared focus mechanism | Interactions I-08; actual row/section focus probe |
| Never style focus with CSS :focus-visible | Production selector/variant scan; no prohibited focus-visible selector found | Interactions source coverage |
| Focus moved by a change the user did not make | HandOffOnUnmount/useFocusHandoff/handFocusOff, pending questions, login/dialog return, report replacement probe | Interactions I-07; actual iframe-replacement probe |
| Focus scrolled to the edge of a scroller that fades there | Shared scrollFade padding and sticky heading focus preservation across dialogs/panes/popovers | Interactions; gallery short/narrow surfaces |
| A scroller nothing else on screen scrolls takes focus itself | Startup/crash, dialog footer-only body, quota overflow, permanent reading/formula/diagram regions | Interactions; Gallery scroll/reading limits |
| A keyboard handler that moves focus itself | F6/capture modality and manual shadow focus markers | Interactions code; Gallery actual F6 |
| Accessible names | Button/menu names, visible label inclusion, field-table headers, viewer title, named regions/images | Interactions surface inventory; copy plain/rich split |
| An icon-only control: tooltip and aria-label | Chrome/row/header/menu/copy/frame controls; shell tab close equivalent | Interactions; Gallery targets/hover/focus probes |
| A tooltip is the application's own, never the browser's title | Shared tooltip shapes; document links and shadow separators explicitly allowed | Interactions; COPY-03 rich tooltip routes |
| A help tip: an explanation behind a ? | HelpTip trigger/popover, focus return and suppression, label placement | Interactions; actual Settings help fixtures in gallery ledger |
| Hover marquees belong to one text label | FadeOverflow scopes in rows/cards/tags/options; actual pointer enter/leave/neighbor/reduce | Interactions; Gallery marquee JSON |
| A tooltip on a mark that is not a control | Status/agent/account/binding/branch indicators; names and approved menu/nested-trigger gaps | Interactions; copy metadata routes; Gallery indicator surfaces |
| Roles and states | Application selection/disclosure/native menu/error/empty/announcement/landmark contracts below | Interactions and States |
| Expanded and selected | Custom aria-current/expanded/pressed, library aria-selected; horizontal strip content width | Interactions; G01 |
| The menu bar icon and its native menu | Native tray_lines/tray NSMenuItem title and custom non-element views/text/bar/image; no new AppKit execution | Interactions native source and recorded-case limit |
| Standalone errors use the shared Alert | All ErrorAlert/TintedAlert/direct Alert callers, error queue/dismissal/recovery, name/literal chips, impossible form state | S01/S02/S04; COPY-01–09 |
| External links | Shared real-anchor/icon/description/pending/failure behavior; Markdown/terminal/native renderer boundaries | States external-link inventory; Interactions |
| A visual heading is a heading element | Full EmptyPanel and page/section headings versus stock Alert.Title exception | S01/S02; States heading coverage |
| An empty state is a glyph, a heading, and what fills it | Full/compact/silent forms throughout welcome/terminal/sidebar/Files/Git/viewer/archive/report/settings/quota/picker | S03/S05; States full caller coverage |
| Landmarks | Normal App views provide one content main; startup/crash incorrectly have none, with no approved exemption; named panel and scroll regions remain distinct | S06; Interactions; Gallery main/region DOM observations |
| A context menu has a visible trigger too | ActionMenu row/frame/terminal menus, agent tab current More trigger and move earlier/later | Interactions; Gallery actual menus |
| Announcing a change | Dedicated errors, StatusAnnouncer first write/delay, attended login/permissions, import result, Advanced disclosure, terminal status gap | I-05; Interactions; Gallery timing/exposure limits |
| Reusing VoiceOver verification | Case matching requires same role, lifetime, focus/modality, timing, exposure and tests; existing open cases stay open | Interactions; docs/voiceover-cases.md; milestone 11 |
| Contrast | Entire custom token/selector inventory, derived/translucent/composite pairs, library distinction | Contrast; token and actual-state JSON |
| Thresholds | Custom text 4.5, glyph/state/boundary 3; decorative/default exclusions | Contrast measurements; Gallery menu glyphs |
| Do not change how a HeroUI component looks | Stock defaults preserved; only approved visual exceptions admitted | S01/S02; Contrast |
| Form controls on a surface | Dialog/popover/card secondary controls and custom control-fill surfaces | Interactions input inventory; Contrast source routes |
| Hand-built rows and cards | Opaque panel cards, decorative edges, selected borders, lock state and title marquee | Contrast; Interactions; Gallery focus/selected row evidence |
| A colour mark whose meaning is in words | Named session dots/swatches, inset edge, accepted colour-name collisions | Contrast; Interactions exceptions |
| Row action menu buttons | Shared 24px target/16px glyph, rest/hover/open/selected surfaces and translucent fills | Gallery selected-row JSON and captures; Contrast |
| HeroUI's own text colours | Default placeholders/text not recast as custom failures | Contrast classification |
| Where to fix a shortfall | Findings target authored part/shared token, not arbitrary library overrides | Contrast finding/limit disposition |
| Finding every rule that draws a colour | JSX utilities, custom CSS, renderer injected styles, theme/palette readers | Contrast source-to-surface inventory |
| Surface-filled components in a dialog or popover | Supported component variants/status; only Alert fill is approved custom visual | S01/S02; Contrast |
| Auditing a change to a colour token or a surface | No token change here; actual composites and solid glyph animation minima inspected; subsequent full affected-scenario requirement retained | Contrast; Gallery |
| A region with a surface colour of its own | Panel/sidebar/overlay/tooltip/field/code/document/terminal/quota track | Contrast numeric/raw token artifacts |
| A glyph or tint on the window material | Recorded material samples, ChromeButton states, ActivityBadge opaque disc and avatar routing | Contrast; no new native-window claim |
| Pointer target size | Shared xs/RowControls geometry, resize handles, avatar delete/trust target extension, tab close equivalent, paired diff/sidebar exceptions | Interactions; Gallery 24px/4px RTL columns |
| Not by colour alone | IconToggle state glyph, statuses, notices and indicators, panel chip shape; approved appearance exception | Interactions; S01/S02/S05; Gallery |
| Text direction | Drawn technical values, string isolation, name auto-direction, mirrored glyphs/physical offsets, actual Arabic same-width renders | COPY-05/06/08; I-06; Gallery RTL |
| Motion | Static/custom transition guards and in-flight preference changes | I-04/G02; Gallery reduced-motion computed styles |
| Dynamic collections | Every mutable tab/account/tag/tree/row list, stable identity and virtualization/fold/scroll exclusions | I-03; Interactions full motion caller inventory |

## Preserved choices and exclusions

Keep the explicitly approved appearance fill-only choice; idle/background success glyph shortfalls; sidebar resize
handle width; paired diff separator proportions; menu indicator/agent omissions; sighted keyboard indicator-tooltip
gap; indicator trigger nested in console-switch button; branch badge text-selection cost; swatch naming collisions;
terminal screen-reader/OSC8 target limitation; and library tab-fade/default styling limitations. Keep the approved
AgentTabList not-installed warning glyph: S05 applies to ordinary empty terminal/sidebar absence only. None of these
becomes a new defect because an automated scan or a generic AA rule could flag it.

Report/source/Markdown/terminal foreign content retains its renderer contract. Host controls, application-injected
notices/links/controls, and application-owned reading surfaces remain in scope. A failed scenario initialization is
an evidence gap or baseline tooling/application problem, not automatically a standards finding. Genuine defects
outside the standard require a separate bug ticket once reproduced; no production repair is authorized here.

## Accepted fix ownership

The user accepted the numbered split without a scope change. The milestone files carry complete scope and validation. Every confirmed finding is assigned:

| Accepted milestone | Findings | Review boundary |
| --- | --- | --- |
| [05 Rich names/tooltips](05-rich-names-and-tooltips.md) | COPY-02/03, caller-agent COPY-09 | Actual rich visible prose and metadata tooltips |
| [06 Choice status/direction](06-choice-status-and-direction.md) | COPY-04/05/08, numeric COPY-06 | Badges, raw matching labels, isolation and fixed-order values |
| [07 Form defaults/validity](07-form-defaults-and-validity.md) | I-01/02/06, S04 | Placeholder/default/typed word and impossible fixed-owner form |
| [08 Content-width tabs](08-content-width-tabs.md) | G01 | Tab allocation and entire strip paint, ring and scroll clearance |
| [09 Empty states](09-consistent-empty-states.md) | S03/S05/G03 | Shared compact listing and neutral absence glyphs |
| [10 Collection/reduced motion](10-collection-and-reduced-motion.md) | I-03/04/G02 | Shared movement contract, guards and in-flight cancellation |
| [11 Terminal status](11-terminal-status-announcements.md) | I-05 | Deferred first writes and recorded-case/native listening requirements |
| 12 Report focus handoff (closed) | I-07 | Focused iframe replacement destination and visible ring |
| [13 Collection button primitives](13-collection-button-primitives.md) | I-08 | Shared HeroUI chevron slot while preserving row semantics |

Source and actual-gallery IDs that describe one issue are aliases, not additional defects (S05/G03 and I-04/G02).
The daemon/caller halves above are explicit ownership divisions, not permission to drop the other render routes.
Shared option, Alert and AgentTabList contracts need one owner at a time; parent coordinates integrated quota and resume-by-ID callers.
R-01/R-03 probes resolved into I-07/I-08; final gallery failures and exact verification handoffs are retained in the delivery record.

## Delivery checks

The [check record](audit-checks.md) retains passing documentation gates, independent-review corrections,
failed optional production checks, service cleanup and native/font evidence limits. Independent final review
passed the historical baseline boundary, quota refresh scopes, accepted split and exact failure labels.
All 52 standard headings and inventory totals reconcile; actual gallery and interaction evidence is retained.
The completed audit changes documentation and evidence only. The accepted implementation milestones must
satisfy their own runtime requirements before closing; this audit does not certify their future fixes.

S01/S02/S06 now use the shared stock Alert structure and corrected Alert-only fills. COPY-01, daemon COPY-06/09
and COPY-07 use the common semantic error router with separate rich/plain and diagnostic roles. This source does
not certify future rich-chip contrast or relabel the original optional audit failures. Required current-source
provenance and check limits remain in the existing check record.
