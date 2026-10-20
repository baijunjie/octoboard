# Contrast and custom visual treatment coverage

> Raw captures and probes are archived outside Git. Evidence names below identify that archive, not repository files.
> Commands using those probes describe the historical collection; regenerate current-source checks for implementation.

Baseline: `3a65c903ee040533c898069e96ba5216f296bc39`. The priority is stock HeroUI appearance, followed by
WCAG AA for application styling. A default library placeholder, soft chip or control fill below a custom threshold
is not a finding. No palette or UI styles were changed during this audit.

## Source-to-surface inventory

| Authored source / baseline lines | Consumers and actual surfaces | Classification and evidence |
| --- | --- | --- |
| `packages/ui/src/style.css:441-500` window/panel/sidebar/hover/selected/control families | TitleBar, Rail, ChromeButton, sidebar/focus/archive/Files/Git/report panels, dialogs, tooltip/popover portals | Custom surfaces; computed token matrix below checks panel/selected/hover/overlay and recorded material samples. Decorative panel hairlines, grooves and shadows are not control boundaries |
| `style.css:506-559` sidebar-fills/control-fills | Row actions, fields, chips, buttons, quota track, tooltip/popover | Custom family remaps must be composited on their actual ancestor surface; HeroUI secondary controls retain their supported variant. Project filter popover deliberately retains library fills |
| `style.css:578-592` app text/accent/danger/focus | Hand-built rows/cards, glyphs, custom focus rings, Alert fill | Custom tokens are distinct from library defaults. Application-muted/focus computed over both panel families, selected/hovered rows, overlay and field; no override is proposed for default library text |
| `style.css:611-634` appearance choice and IconToggle | Settings appearance; Files/Git/viewer two-state toggles | Approved appearance fill-only exception stays. IconToggle uses shape and pressed state; its resets are an explicitly specified behavior, not permission for new token overrides |
| `style.css:647-661` untracked chip | Change rows and viewer status | Custom coral chip: label checked over its 15% fill composited on panel/sidebar/hover/selected/overlay, rather than bare background |
| `style.css:684-697` glyph and line-count tokens | StatusIcon, WorkingDot, WavingHand, BranchOriginBadge, change counts and import result | Custom accent/warning/success-glyph and line-added/removed checked separately. Idle/background `text-success` is an explicitly preserved shortfall; it does not authorize dimming other glyphs |
| `style.css:718-738` avatar palette | ConsoleAvatar over rail/current-console tile | Custom avatar foreground/background pairs for all seven hues. Avatar pictures supplied by users are images; do not invent a uniform contrast promise about arbitrary images |
| `components/SessionMetadata.tsx:27` | Native description-term labels inside custom UsageTooltip | Explicit application-authored text-muted was measured rather than exempted as a library default; actual white/dark-tooltip surfaces give 4.83:1 light / 6.76:1 dark. See gallery-usage-tooltip-contrast.json |
| `components/StatusBadge.tsx:14-17` | Availability badges in fields/overlays/popovers | Custom 80% foreground over 16% muted fill; both alpha layers are composited before measuring text. Hairline is decorative; the visible status words carry meaning |
| `style.css:1164-1172,1212-1215`; `components/InlineChip.tsx` | Diagnostic CodeBlock tone and secondary purple InlineChip extension | Custom mixed inks/fills are checked as such. Normal InlineChip colours, shape and soft preset remain library defaults. Technical code child shares the chip's metrics; no caller typography override is proposed |
| `style.css:343-361`; `quota/QuotaMeter.tsx` | Claude/Codex custom quota fills over actual field track | Custom fills, including dark variants. Grok uses foreground. Recorded numeric percentage and accessible name also carry the state |
| `components/ActionMenu.tsx`, `sidebar/rows.tsx`, `components/RowControls.tsx` | File/Git/project/session rows, focus cards, pane headers | Transparent muted rest; foreground on translucent hover/open fill; actual ancestor composites and settled transition recorded by gallery probes. Selected rows require their own sample; header-only evidence is not substituted |
| `components/ChromeButton.tsx:10-16`, `components/ActivityMarker.tsx:65-90`, `components/Rail.tsx:219` | Top bar and rail over native material, avatar activity disc, waiting count | Chrome uses recorded material colours as well as browser stand-in. ActivityBadge has its own opaque disc: the blue glyph never sits directly on the assumed chrome hover tint. Waiting count has warning fill/dark foreground; waiting hand uses warning-glyph |
| `components/statusGlyphs.tsx:16-18,26` | Working/running-background/waiting status in rows/cards | Ping's fading copy is backed by a solid same-size dot/ring; there is no frame where the sole carrier fades away. Hand animates transform, not opacity. Motion preference findings are separate |
| `viewer/codeTheme.ts`, `MarkdownSurface.tsx:24,91`, `viewer/markdown*.css`, `viewer/diagramConfig.ts` | Code/document/formula/diagram reading surfaces | Custom editor paper/ink, document link, muted text, tint and derived rules inventory. Body/link/muted and diagram ink checked from supplied palette. Foreign syntax tokens retain source renderer palette; decorative rules/tints are not controls |
| `viewer/renderer.tsx:60-63,105-140`; `viewer/rendererDom.ts` | Shadow-root diff additions/deletions, line numbers, separators | Application-authored diff fills/decorations coexist with upstream high-contrast theme. Separator focus/target exceptions remain; document comparison captures show real shadow content, not a claim that every syntax token was numerically enumerated |
| `terminal/xtermThemes.ts:22-66,80-83` | Agent/shell terminal and link hover | Application link colours measured against matching terminal paper. Raw ANSI output retains renderer model and xterm minimum contrast policy; terminal overlays use shared UI tokens |
| `style.css:866`; `components/Toasts.tsx` | Notice toast title/indicator; default description | Accent indicator is an explicit approved visual treatment. Rich name findings are separate from colours; no replacement toast styling proposed |
| `website-preview/*`, report iframe, gallery shell | Dev preview/report content and gallery controls | Fixture/example document contents and gallery control chrome are not production host UI. Application-owned report frame/pager/empty state remain in scope |

The inventory was obtained from application colour declarations and JSX classes as well as the shared stylesheet:
`color`, `background`, `border`, opacity and CSS-variable readers were inspected, including renderer-injected styles.
A scan of hex values alone would miss the custom `color-mix` labels and translucent row fills.

## Measurement record and candidate disposition

evidence/token-contrast.mjs (archived evidence) is a reproducible fixture-only probe. Run from `packages/ui`
with a gallery origin and JSON output path; `OCTOBOARD_CHROMIUM` can select an installed browser. It resolves CSS tokens
in the real gallery document, converts computed OKLCH/OKLab values to sRGB, composites alpha in sRGB, and uses WCAG
relative luminance. The result (archived evidence) contains every foreground/background/ratio rather than
ratios sampled from screenshots. It deliberately includes candidate combinations that need source-route checks.
Gallery probes separately record actual rendered ancestors and pointer/selection states.

The final matrix contains **314 token/surface pairs** plus **eight actual rendered diagnostic blocks** across both
appearances. All tested panel/overlay/field/badge/untracked/line-count/avatar/quota-track-candidate/document pairs
pass their stated thresholds. Diagnostic tone coverage includes primary, secondary, success, warning and danger;
status Alert fill is composited before the diagnostic's 18% fill, then its custom ink is measured. Secondary tone
is standalone because TintedAlert has no secondary status. No omitted tone is silently treated as neutral.

The actual ancestor-chain checks cover `startup-daemon-failed`, `startup-crash` (durable and acknowledgement blocks)
and `git-unreadable` in both themes, including their actual intermediate wrappers/fills. Danger diagnostic text is
6.78:1 on the startup/crash surface in both themes and 6.46:1 light / 6.28:1 dark on the Git error surface. Neutral
crash acknowledgement text is 13.98:1 light / 11.92:1 dark. Every ancestor colour and opacity is retained in JSON.
These are final computed colours, not screenshot ratios. The probe rejects unsupported colour syntax rather than
silently interpreting another format as OKLab; its observed inputs are rgb/oklch/oklab. Out-of-gamut channels use
clamping, so a near-threshold pair would need confirmation against the browser's gamut mapping before filing it.

Sub-threshold candidate pairs occur when accent/warning glyphs or chrome-muted are hypothetically placed on a
chrome-current or pressed native material surface. They are **not findings**: actual activity glyphs use opaque
`ActivityBadge` discs, the current console tile holds an avatar, and breadcrumb muted text does not sit on a current
button fill. No extra foreground override is justified by an unused token/surface cross-product. These hypothetical
rows remain in the evidence so the exclusion is inspectable. The current tile and native-material glyph consumers
must be reconsidered if a later feature adds a new glyph directly to them.

The gallery ledger records sampled ActionMenu ratios and the selected-row follow-up. The actual selected Hub row
has glyph 4.94:1 light / 5.05:1 dark at rest and 12.21:1 light / 10.37:1 dark on settled hover/focus/open; its 24px
controls and 4px gap mirror under Arabic. Custom source/renderer tokens outside these sampled declarations are
covered by the source inventory, not asserted to have an individual numeric record for every possible document. A passing sample does not
certify every state, every foreign renderer token or the full native-material range. The material samples
`#e2e4e5`, `#d0d2d3`, `#484a4b` are the project's recorded source values (`style.css` window-surface comment), not a
new packaged-window capture. No native window or real daemon was started by this audit.

## Findings and limits

No additional contrast-ratio defect is confirmed by this measurement set. Confirmed stock-appearance violations
are S01/S02 in `audit-states.md`: custom Alert typography, colours, gaps, radius, shadow and content structure.
Restore stock parts while retaining the approved status fill; do not "fix" default HeroUI colours to an unrelated
custom threshold. Tab width is G01, and reduced-motion defects are I-04/G02.

The following standard subsections were each applied: Thresholds; Do not change how a HeroUI component looks; Form
controls on a surface; Hand-built rows and cards; A colour mark whose meaning is in words; Row action menu buttons;
HeroUI's own text colours; Where to fix a shortfall; Finding every rule that draws a colour; Surface-filled components
in a dialog or popover; Auditing a change to a colour token or a surface; A region with a surface colour of its own;
A glyph or tint on the window material. The surface inventory above gives their concrete consumers and limits.
There is no token change in this milestone; any later custom palette/surface change must remeasure every affected
scenario in both appearances, including selected/hovered composites and animation minima.
