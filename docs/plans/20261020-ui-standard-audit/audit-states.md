# Alerts, states and navigation audit

Baseline: `3a65c903ee040533c898069e96ba5216f296bc39`. Paths and line numbers below refer to this baseline,
not to other owners' working copies. This is an audit record, not permission to implement fixes.

## Findings

### S01 Shared error content replaces the stock Alert structure

- Rule: **Roles and states / Standalone errors use the shared Alert**; **Confirmed visual choices**;
  **Not by colour alone**.
- Evidence: `packages/ui/src/components/ErrorAlert.tsx:9-29` exposes `headingLevel`, creates `h1`/`h2`/`h3`,
  uses a custom flex body instead of `Alert.Title` and `Alert.Description`, sets `text-sm`, `text-base`/`text-lg`,
  `text-foreground`, `gap-3`, and `shadow-none`. A heading consequently loses HeroUI's status colour.
- Scope: replace the wrapper's visual and content contract with the stock indicator/content/title/description
  parts. Retain only the status background fill as custom visual treatment. Preserve announcement ownership,
  queueing, focus, and error classification. Move diagnostics, block content, page headings and remedy outside.
  Changing only the title colour would leave the rest of the violation in place.
- Scenario evidence: startup failures/crash, file/Git pane failures, viewer failures and import settings failure;
  actual rendering and capture references are recorded in `audit-gallery.md`.

The complete caller migration inventory is below. Layout constraints such as an outer maximum width can remain
outside the Alert; they are not permission to alter its internal spacing, typography, radius or shadow.

| Baseline caller | Content requiring migration | Gallery confirmation target |
| --- | --- | --- |
| `packages/ui/src/StartupScreen.tsx:67` | Startup title and diagnostic `CodeBlock` | `startup-daemon-failed` |
| `packages/ui/src/StartupScreen.tsx:78` | Page heading, paragraphs, list of connection instructions | `startup-no-address` |
| `packages/ui/src/ErrorBoundary.tsx:53` | Crash page heading and independently scrollable diagnostic region | `startup-crash` |
| `packages/ui/src/browser/LazyProjectBrowser.tsx:86` | Short explanation; retry is already outside | Missing lazy-chunk failure scenario; exercise in an isolated fixture preview |
| `packages/ui/src/browser/ProjectBrowser.tsx:367` | Root-read title and structured daemon explanation | `files-root-error` |
| `packages/ui/src/browser/gitStates.tsx:38` | Git source/branch/commit/change-read title and structured explanation | `git-changes-listing-failed`, `git-changes-failed`, `git-commits-failed` |
| `packages/ui/src/browser/GitView.tsx:100` | Git-unavailable title and raw diagnostic `CodeBlock` | `git-unreadable` |
| `packages/ui/src/viewer/viewerStates.tsx:217` | Viewer failure title and potentially structured daemon content | `viewer-files-errors`, `viewer-changes-errors`, `viewer-document-errors` |
| `packages/ui/src/configuration/ImportResult.tsx:184` | Settings-import failure with nested structured reason | `configuration-import-result-settings-failed` |
| `packages/ui/src/dialogs/SessionDialog.tsx:233` | Fixed owner disappeared; currently plain interpolated name | `focus-console-new-session-owner-gone`; see S04 |

`TintedAlert.tsx:16-35` itself applies the approved per-status fills and a nonvisual tone context. The violations
are in `ErrorAlert` and the callers that add other styles or replace the content parts. Its migration must audit
every caller, rather than treating the shared fill as a contrast defect or removing it.

### S02 Warning and viewer notice callers alter the Alert

| Caller and evidence | Rule broken / current behavior | Concrete fix scope and confirmation |
| --- | --- | --- |
| `packages/ui/src/dialogs/RequestedDialog.tsx:54-78` | `ConfirmWarning` suppresses shadow and puts session lists and consequence paragraphs directly in custom Alert content with local typography/spacing | Keep one stock warning explanation; place affected-session list and option consequences in surrounding content without losing option description IDs, list motion or the single-warning meaning. Confirm `dialog-remove-project-trash`, `dialog-archive-console-session` and delete cascades, with each cleanup option selected and cleared |
| `packages/ui/src/dialogs/TrustPromptDialog.tsx:61-70` | `shadow-none`, description `text-foreground` | Keep stock warning description, indicator and shadow; preserve trust checkbox and path placement. Confirm `trust-prompt` and `trust-prompt-dismissed`, including nested folder choice |
| `packages/ui/src/configuration/ExportConfigurationDialog.tsx:108-114` | `shadow-none` despite stock title/description already being available | Remove the shadow override; preserve warning fill and stock title colour. Confirm `configuration-export-console` and `configuration-export-everything-only` |
| `packages/ui/src/viewer/viewerStates.tsx:96-105` | `Notice` sets padding, gaps, radius, shadow, indicator inset and description colour/line height | Both strip and Markdown document notices keep original HeroUI structure/styles and their status fill. Preserve `dir`, visual hiding and delayed announcement rules. Confirm all three notice tones above content and embedded block notices, long text and both themes |
| `packages/ui/src/gallery/codeGallery.tsx:64-86` | Comparison fixtures put custom blocks directly in content; nested examples omit the stock indicator/description parts | Align the developer comparison through gallery-writer when the shared Alert migration lands; do not let this example teach the superseded content model |

These are violations of the same **Standalone errors use the shared Alert** rule, not separate requests for a new
Alert design. Inline Markdown notices remain spans: a block Alert inside a paragraph is invalid and is explicitly
excluded by the standard.

### S03 Directory-picker emptiness has a second presentation

- Rule: **An empty state is a glyph, a heading, and what fills it** (nested lists use the compact row).
- Evidence: `packages/ui/src/dialogs/DirectoryPicker.tsx:153-159` creates a disabled list option containing only
  a fading text label. It uses neither `EmptyPanel` nor a glyph. The code explains why a listbox option must stay;
  that semantic requirement must survive the fix.
- Scenario: `dialog-directory-picker` shows a populated listing; an empty-directory fixture/interaction is required.
  Do not claim the populated scenario proves the empty state.
- Fix scope: use the shared compact empty presentation within the collection's permitted structure, preserving
  a valid disabled option, keyboard skipping, parent-directory navigation, and no action on the empty row.

### S04 A fixed owner disappearing leaves an unusable form open

- Rule: **Standalone errors use the shared Alert**: prevent an impossible state; if it arises while a dialog is
  open, close it and show a plain acknowledgement-only error dialog.
- Evidence: `packages/ui/src/dialogs/SessionDialog.tsx:103` closes only for unavailable launch choices;
  `:230` disables submit for `fixedOwnerGone`, while `:233` leaves the dialog mounted with an inline error.
  The fixed owner cannot be replaced inside this form.
- Scenario: `focus-console-new-session-owner-gone` (`gallery/fixtures/focus.ts:556`) opens this dialog, then archives its fixed
  owner. Its existing target is the disabled form and inline error. During the fix, update that target to assert the
  form closes, acknowledgement is the only action, and focus returns to a surviving control. Actual rendered evidence
  is recorded separately in the gallery ledger.
- Fix scope: route this invalidation through the existing close-when-blocked pattern with plain recovery `none`;
  keep the separate optional picked-owner fallback, whose choice can still be changed.

### S05 Uninstalled-agent absence uses a warning glyph

- Rule: **An empty state is a glyph, a heading, and what fills it**: normal absence uses a neutral or purpose glyph.
- Evidence: `packages/ui/src/terminal/TerminalPane.tsx:492` uses `TriangleAlert` for no installed agent;
  `packages/ui/src/sidebar/Sidebar.tsx:634` does the same for its compact no-agent state. The no-added-agent branches
  already use `Bot`. `RailQuotaEntry.tsx:266` uses the purpose glyph `Gauge` for the equivalent unavailable-agent absence.
- Scenario: `empty-console-no-agent` (compare `empty-console-no-agent-added`); see the visual ledger. This absence is not a failed
  installation request or a failed load. Keep genuine installation/runtime failures in the error presentation.
- Fix scope: use the neutral/purpose glyph consistently in both empty forms; retain the PATH explanation and
  disabled creation affordances. No colour-token change is required. Preserve the explicitly approved not-installed warning glyph on `AgentTabList`; this finding is limited to ordinary empty states.

### S06 Startup and crash content has no main landmark

- Rule: **Roles and states / Landmarks**: the content panel always holds exactly one `main`.
- Evidence: `packages/ui/src/components/ContentPanel.tsx:21-26` renders a `div` for both ordinary and bare panels;
  `packages/ui/src/StartupScreen.tsx:39-55` puts only `div` wrappers and a named `FadedScroller` region inside its
  bare panel. No `main` or `role="main"` is supplied at that boundary. The standard states no pre-App/startup exception.
- Scenarios: `startup-no-address`, `startup-daemon-failed`, `startup-crash`; existing
  `evidence/gallery-dom-observations.json` records zero main elements for these rendered startup/crash views.
  A named scroll region is not a replacement for the main landmark.
- Fix scope: establish exactly one main landmark around startup/crash primary content while migrating its page
  heading and Alert in milestone 03. Preserve independently named overflow regions, modal error ownership and
  retry/quit actions. Do not put a second main around ordinary views that already provide their own.
- Validation: inspect the three scenarios before and after error acknowledgement, both themes and Chinese/Arabic;
  assert exactly one main in the content panel, retained region names and keyboard access to long diagnostics/actions.

## Coverage and conforming patterns

| Standard area | Inspected surfaces and code evidence | Result / limits |
| --- | --- | --- |
| Error dialog dismissal, queue and footer recovery | `errorAlerts.ts:42`, `dialogs/ErrorAlerts.tsx:50-116`, `dialogs/Dialog.tsx`, `reportError.tsx`, `errorRecovery.ts` | Dedicated error alert has danger icon/acknowledgement, recovery button and issue action in footer, no body recovery button. Acknowledgement owns dismissal; durable errors do not reannounce. Browser checks are in the gallery ledger |
| Navigation instructions and duplicate entries | Welcome, terminal absence, quota empty panel, console accounts, account login, error guidance, project/tag empty states | Quota Add agent hides its competing Manage accounts footer (`RailQuotaEntry.tsx:290-300`). Console accounts has one management route. Retry and issue actions are real controls. Explanations about external installation/PATH are not navigation instructions to an in-app destination |
| Impossible creation entries | `sidebar/NewSessionButton.tsx:33-43`, `NewSessionMenu.tsx:39-40`, `useNewSessionMode.ts:19-20`, `dialogs/HandoffDialog.tsx:49`, `SessionDialog.tsx:103` | New-session actions disable without added/installed choices; agent loss closes the form through `useCloseWhenBlocked`. S04 is the fixed-owner gap. Optional empty/no-choice selectors retain their own valid states |
| Tabs | `SettingsDialog.tsx:173-204`; `ProjectBrowser.tsx:266-272`; `SessionDialog.tsx:239-251`; `ConsoleLaunchFields.tsx:86-100`; shared `AgentTabList.tsx:60-76` used by settings and quota; `ShellTerminalPanel.tsx:252-267` | Gallery measurements decide width/start alignment; hover foreground/full opacity is an approved exception. Shared horizontal library defaults include `min-w-full` and tab `w-full`, so source class names alone cannot establish a pass |
| Empty panels and headings | `components/EmptyPanel.tsx`, `WelcomeScreen`, `TerminalPane`, `ShellTerminalPanel`, `ArchiveView`, `ReportPanel`, sidebar/focus nested lists, project Files/Git/filter, viewer stand-ins, trusted folders, agent accounts and quota | Shared full/compact shape is widely reused. Full headings are real heading elements; compact rows add none. `silent` excludes actions in its type contract. S03 and S05 are the alternate presentation/glyph findings |
| External links | `components/ExternalLink.tsx:28-91`, `settings/AddAgentDialog.tsx:58`, `AccountLoginDialog.tsx:134`, `dialogs/ErrorAlerts.tsx:93`, `viewer/MarkdownDocument.tsx:304-328`, `platform/browser.ts:73` | Application anchors keep destinations, external icon, localized description, browser target/rel, pending label and shared failure path. Markdown fragment/file links retain document navigation; native titles there are an approved exception. Renderer/terminal/iframe foreign content is not application prose |
| Custom headings | Full empty panels, settings section headings, import result/review, account groups, startup/crash, viewer failure | Alert headings are S01; a stock `Alert.Title` is explicitly exempt from page heading semantics. Move actual page/section headings outside the Alert during migration |

## Concurrent integration handoff

The coordinating session identified the quota-exhausted Work inline Alert in quota-gating working-copy edits
while this audit was collecting evidence. Those edits have since been integrated into local main as
`7a7a2f34ebc19c5bb2ee29f9ae804354c2a40ad3`, on top of the audited `3a65c903` baseline; that owner's worktree was removed.
This audit did not inspect or modify its dirty working copy, and its original captures do not certify the new code.
The committed-versus-pending distinction changed during the audit; the evidence baseline did not.

The coordinating session's concrete delta includes `useLaunchChoice` used-up state in session, handoff and
switch-account dialogs; quick-create refusals in `dialogs/useQuickCreateSession.ts`; and `quotaUsedUp` /
`quotaUsedUpAgent` recovery routes. The new inline value is still formed as the plain
`quotaUsedUpSentence(t, chosenEntry.name, ...)`. The parent supplied these facts; this is a migration handoff,
not a new completed source/visual audit of the 95-file quota integration.

Milestone 03 must freshly inventory **all** current Alert callers, including those newly committed paths, and
reproduce the exact exhausted Work account in light/dark and translated UI. Preserve late announcements, stock
status-coloured Alert.Title, default description, indicator/spacing/typography/radius/shadow and only the status
background fill. Include these exact newly committed fixture IDs:

- `dialog-new-session-quota-all-used-up`
- `dialog-hand-off-quota-all-used-up`
- `dialog-switch-account-quota-used-up`
- `dialog-new-session-quota-refused`
- `sidebar-quick-create-quota-refused`

Milestones 04/05 must refresh parameter/render-route inventory across `quota/accountUsage.tsx` (including ReasonText),
`quota/quotaFormat.ts`, launch choice, recovery and refusal paths. The original interpolation index and counts remain
a `3a65c903` snapshot, not current-main certification. The separate `feat/resume-autofill-agent` work remains a later
integration boundary. Other owners' archived-history and claude-otel-usage work was untouched. The coordinating
session owns cross-branch communication; this audit does not audit unrelated work or implement any migration.
