# Interaction, direction and motion audit

Audit baseline: main `3a65c903ee040533c898069e96ba5216f296bc39`. This is an audit evidence record; no production behavior is changed. Findings below are source-confirmed unless expressly marked an unresolved probe. A gallery scenario reference identifies a reproduction surface, **not a claim that its interactions were exercised**. Actual browser evidence belongs to the gallery audit record and the targeted `evidence/interaction-probes.json` record below. Native AppKit and packaged WKWebView/VoiceOver observations cannot be inferred from Chromium or source tests.

## Surface coverage

The following inventory covers production UI, excluding dev-only gallery controls and agent-authored report document internals. The application-owned report frame, keyboard bridge, pager and replacement behavior are included. Foreign report/source/terminal content retains its renderer's model under the standard.

| Surface | Interaction evidence inspected | Direction and motion evidence inspected | Result / remaining rendered check |
| --- | --- | --- | --- |
| Startup, crash, welcome | `StartupScreen.tsx`, `ErrorBoundary.tsx`, `WelcomeScreen.tsx`; shared fade region and error host | Stack/code `dir="ltr"`; welcome localized content; scroll regions rather than inert clipped diagnostics | Root reviews error/heading/empty-state rules; browser must check short-window scroll reachability. |
| Window top bar and rail | `components/TitleBar.tsx`, `ChromeButton.tsx`, `Rail.tsx`; tooltip + named Button; `App.tsx:268` region visibility | Logical offsets and mirrored icons; `Rail.tsx:183` shared list motion | No new source-confirmed keyboard defect; rendered F6 and window-material contrast are separate checks. |
| Sidebar ordinary mode and project/session rows | `sidebar/rows.tsx:23`, `:82`; custom button Enter/Space and focus-visible props; row action menus, pins, locks, `Sidebar.tsx:239` handoff | `Sidebar.tsx:469`, `:611`, `:742` useFlip lists with stable IDs; names auto direction | I-04 transition preference gap. No Tab defect inferred from preventFocusOnPress. |
| Sidebar focus mode | `FocusView.tsx:130` focus handoff; header actions; real Button cards and named metadata | `FocusView.tsx:309`, `:440` shared motion for console chips/cards; row icon columns | Existing indicator tooltip/content-model exceptions preserved. Long-label hover and RTL alignment require browser evidence. |
| Project filter, tags and suggestions | `ProjectFilter.tsx:138` named SearchField; selected-tag list and clear actions; `TagsInput.tsx:99`, `:112` focus returns to input | `ProjectFilter.tsx:187`, `:249`, `:266`; `TagsInput.tsx:54`; `PickedTag.tsx:75` stable tagged identities, per-label direction | I-02 optional-tag placeholder; no independent keyword count or search placeholder defect inferred. |
| Archive | `ArchiveView.tsx:158` focus wrapper; named reopen/delete; row menus; body FadedScroller | `ArchiveView.tsx:92` useFlip and `:197` stable data-flip IDs | I-04 unguarded action fade; append-on-scroll must remain free of layout animation. |
| Main agent terminal | `TerminalPane.tsx:449` xterm; reconnect/empty/dormant overlays; App F6 visibility | LTR terminal; guarded status glyph motion | I-05 initial live-region write gap; output screen-reader mode is an approved exception. |
| Shell Terminal panel | `ShellTerminalPanel.tsx:210` tab/panel wiring; `:279` unmount handoff; tab context menu; header Close terminal equivalent | `ShellTerminalView.tsx:249` LTR xterm; terminal tabs stable IDs | I-03 missing committed collection motion; I-05 initial exited announcement gap. Pointer-only close mark is intentional. |
| Pane layout, panel stack, resize controls | `useRegionCycle.ts:54`, `usePaneToggles.ts:293`; `PanelStack.tsx:222` focus handoff; `ResizeSeparator.tsx:137` named separator/value control | Physical resize directions account for RTL (`ResizeSeparator.tsx:21`); reduced-motion pane transitions | I-04 line-color transition. No new target violation asserted; sidebar handle's narrower target is approved. |
| Files tree/search | `FileTree.tsx:160` directory activation; row actions, custom Tree chevron; `treeRow.tsx:22`; named search/clear | `useBrowserListMotion.ts:8`; `FileTree.tsx:128` virtual motion/fold identities | I-08 undocumented chevron primitive; targeted row/action traversal kept rings. Fold-removal focus and additional target geometry remain outside that probe. |
| Git changes flat/tree/section headings and branch choice | `changeSectionHeading.tsx:160`, `:204`; treegrid header navigation; `BranchSelect.tsx:92` choices | `ChangeTree.tsx:107` virtual motion/fold IDs; branch `MotionListBox`; mirrored toggles | Preserve row/section-header collection focus pattern and plain textValue. No blanket interaction pass claimed. |
| File viewer and shadow diff controls | `FileViewer.tsx:123`; `rendererDom.ts:160`, `:189`; manual tab order, deduped names and focus move | LTR source/diffs; `rendererDom` data-focus-visible; mirrored view icons | Approved paired separator target shortfall remains. Browser must verify shadow control states, visible names and ring; root owns alert/copy review. |
| Markdown, code, diagrams and formulae | `MarkdownDocument.tsx:121` link focus; `MermaidBlock.tsx:83`, `FormulaView.tsx:147` named focusable regions | `dir="ltr"`, explicit formula unicode-bidi isolate; source fallback | No new source-confirmed naming/direction defect. Diagram/formula role and overflow checks still require rendered evidence. |
| Report panel | `ReportPanel.tsx:143` named pager; `:300` iframe title; F6/history/switch bridge validates frame source | Pager ratio `dir="ltr"`, directional icons mirrored; replacement snapshots exempt from list motion | I-07 live page replacement loses focus in fixture probe; report internals are foreign content. |
| General, Git, notification and permission settings | `SettingsDialog.tsx:129`; `SettingRow.tsx`; `PermissionsSection.tsx:166` attended StatusAnnouncer | Clone-directory InputGroup LTR; locale/theme fields; static choices excluded from collection motion | Preserve appearance-choice and permission-HelpTip exceptions. |
| Settings agents, accounts, login and trusted folders | `AgentListMenu.tsx:52` focus return and earlier/later menu; `AccountLoginDialog.tsx:159`; `AccountLogin.tsx:108`; `TrustedFoldersSection.tsx:33` refocus | `AgentAccountsSection.tsx:248` account list motion; `TrustedFoldersSection.tsx:25` folder motion; AgentTabList lacks collection motion | I-03 agent tabs; initial login/permissions announcement mechanisms already use dedicated gates. Existing tests are not a new listening pass. |
| Rail quota floating panel and account lists | `RailQuotaEntry.tsx:203` nonmodal, `:205` Escape marker, `:256` no-tab keyboard landing, `:274` tab landing | Logical positioning, shared AgentTabList; account rows `:332` lack shared motion | I-03 live account/agent collections. Hover must leave focus; keyboard open/close and open-modal behavior require browser evidence. |
| Console/project/session/rename/account dialogs | `Dialog.tsx:145` choice Enter submit; `:345` repeat filtering; form field autofocus; dependent-field handoff hooks | Technical fields LTR, ordinary names localized; shared dialog height animation | I-01/I-02 placeholders; I-04 active-height animation preference change. Root owns titles/copy/tabs. |
| Confirm, trust, session requests, requested operations, error alerts | `ConfirmDialog.tsx:78` reset unprompted focus; `:92` confirm focus gate; `ErrorAlerts.tsx:57` dialog focus; pending-question reuse | stable requested-warning session identities and shared motion; typed word lacks explicit direction | I-01/I-06 typed confirmation. Destructive and unsolicited prompts intentionally open on the dialog. |
| Directory browser | `DirectoryPicker.tsx:119` form; `:125` ListBox; prior listing retained on failure | Input LTR; directory replacement is an exempt replacement listing | I-01 path placeholder; no collection-motion finding for directory navigation. |
| Configuration export/import review/result | Shared Dialog; export checkboxes; `ImportReview.tsx:158` custom directory field; stage focus in ImportConfigurationDialog | LTR path fields; fixed result snapshots intentionally lack list motion | I-02 required directory example prefix. Renamed result dialog VoiceOver case remains open per existing verification document. |
| Toast, connection banner, copy and help overlays | `Toasts.tsx`, `ConnectionBanner.tsx`, `CopyButton.tsx`, `HelpTip.tsx`, shared `StatusAnnouncer.tsx` | Copy icon guarded by motion-safe; tooltip/popover logical placement; toast stack owns its animation | Preserve shared libraries and passing-case reuse requirements; root owns severity/recovery/copy findings. |
| Native menu bar icon and menu | `apps/desktop/src-tauri/src/tray.rs:417`, `:473`, `:655`; `tray_lines.rs:112`, `:196`, `:219`, `:276` | Native label colors and NSMenu keyboard behavior retained; decoration views/cells non-elements | Source coverage only; no actual AppKit accessibility-tree or VoiceOver listening verification this round. |

Paths without a prefix in this document are relative to `packages/ui/src/`.

## Coverage by standard subsection

This table supplies the interaction scope of each relevant section. The full audit's root-owned findings and catalog inventory supply the complementary visual/copy coverage; an entry here never replaces actual rendered evidence.

| Standard section | Coverage conclusion |
| --- | --- |
| Conformance level and exceptions / Confirmed visual choices | No HeroUI-default contrast, stock placeholder color, low tooltip glyph target, appearance-choice shape, or approved diff target is filed as a new defect. Alert-only status background and approved opaque tab hover labels remain requirements for root review. |
| Keyboard | Shared Dialog choice/submit/repeat routing, ConfirmDialog destructive/unprompted/typed focus, all native/custom row activations, menu and library controls inspected. No new confirmed global keyboard blocker. |
| Reaching a control with Tab | Tree/GridList rows intentionally own traversal; change headings explicitly extend the arrow walk; console launch table uses column groups; viewer float group contains its controls. Quota tabpanel overflow handoff is explicitly implemented at `RailQuotaEntry.tsx:307`. I-08 records the chevron primitive mismatch; tested row/action keyboard paths keep their rings. |
| Buttons | Production custom native button is `Scrim.tsx:28`, justified by its source comment. Shared HeroUI Button provides compact sizing. The whole-band section button has an explicit custom-primitive justification; I-08 records the unjustified chevron; do not rewrite collection navigation as independent tab stops. |
| A control inside a rendering library's shadow root | `rendererDom.ts:189` names one control copy, marks others hidden; manual focus walk and custom data-focus-visible. Native title exception preserved. |
| Leaving the terminal with F6 | `useRegionCycle.ts:15`, `:54`, App `shown` at `App.tsx:268`, and dynamic panel registration at `PanelStack.tsx:252` cover terminal/no-content/retry, welcome, archive, banner, toast and panel ownership. No new region omitted in source inventory. |
| A floating panel that does not take the window | Quota panel explicitly nonmodal, topmost Escape marked and focus-on-keyboard-open only. Tests/actual gallery evidence must check resulting DOM, not just props. |
| Input placeholders | Every editable input/render site inventoried below. I-01/I-02. Search filters have nonempty semantic filter hints; initial values do not excuse omitted placeholders. |
| Visible focus | HeroUI retained; custom rings use data-focus-visible. Tab strip inner padding and scroll margin are present in shared AgentTabList and shell list. The tested row/action/section-heading paths show rings; chevron primitive is I-08. |
| A hand-built focusable element | `TreeRow`, ResizeSeparator, ExternalLink, scroll regions and shadow controls have application focus wiring; Scrim is justified. No new general outline suppression finding was observed in the targeted browser paths. |
| Never style focus with CSS :focus-visible | Production search found no prohibited selector/variant; comments referencing its failure are intentional. |
| Focus moved by a change the user did not make | `HandOffOnUnmount`, `useFocusHandoff`, `handFocusOff`, pending-dialog reset and renderer-specific handoff inspected; I-07 confirms live report replacement is not covered by those helpers. |
| Focus scrolled to the edge of a scroller that fades there | `scrollFade.tsx:22` shared 24px padding; sticky heading padding/focus helper used; dialog body and browser lists use shared fade implementation. Native Tabs fade limitation is documented, not newly reported. |
| A scroller nothing else on screen scrolls takes focus itself | `useFadeRegion` provides named overflow-only stops; permanent viewer/diagram/formula reading regions retain intentional always-stop behavior. Startup/crash/dialog/quota use shared region. |
| A keyboard handler that moves focus itself | F6 sets keyboard modality before focus; custom shadow handler sets focus marker; capture shortcut callers inspected. No new confirmed capture-modality omission. |
| Accessible names | Named Buttons/searches/menus/separators/regions/frame; visible count included in diff controls; console field-table header naming implemented; viewer title avoids flex-box punctuation reads. I-08 is a primitive-contract finding, without an invented accessible-name failure. |
| An icon-only control: tooltip and aria-label | Shared ChromeButton, ActionMenu, CopyButton, IconToggle, row controls and frame close expose name+tooltip. Shell tab close has a deliberately reachable menu equivalent. |
| A tooltip is the application's own, never browser title | Shared TitledControl/AnchoredTooltip and FadeOverflow used. Raw document-link title, iframe document title and shadow-control native title are not new violations. |
| A help tip | `HelpTip.tsx` named trigger and focus-contained popover; tooltip suppression; uses separate label-adjacent placement. Approved permission explanation exception retained. |
| Hover marquees belong to one text label | Shared `FadeOverflow` implements per-label motion; data-marquee-scope use limited to tags/options/single labels in inspected call sites. Actual pointer isolation/reduced-motion checks remain browser evidence. |
| A tooltip on a mark that is not a control | Agent/account metadata and status marks use shared indicator/anchored patterns; preserve menu-glyph, keyboard-only sighted tooltip and console-switch nested trigger exceptions. |
| Expanded and selected | TreeRow aria-current/expanded; IconToggle state; panel kind toggle controls; shell maximize action omits duplicated state. Root covers tab sizing and visual treatment. |
| The menu bar icon and its native menu | Native informational lines retain disabled NSMenuItem and full title; custom row/text/bar/image elements explicitly non-elements; AppKit native interaction is retained. Actual listening is unperformed. |
| Standalone errors use shared Alert | Root owns structural/visual/content findings; interaction scope verifies ErrorAlerts focus, nondismissable shared frame and acknowledgement queue; field validation remains attached. |
| External links | Shared ExternalLink uses application ring and real anchor destination; root owns caller inventory. Document links keep renderer exception. |
| A visual heading is a heading element / empty states / Landmarks | Root owns visual/content inventory; role=rowheader in change treegrid is intentional. App exactly-one-main and named scroll-region mechanisms inspected; no extra generic-named-region finding. |
| A context menu has a visible trigger too | Shared ActionMenu covers row/terminal menus, Settings AgentListMenu adds current-tab More trigger and reading-order move commands; shell tab context menu has matching header menu. |
| Announcing a change | Shared status lifetime/delay, login attended gates, permissions, dialog Advanced and import result focus inspected. I-05 raw terminal first-render text; R-02 native listening limitations explicit. |
| Reusing VoiceOver verification | `docs/voiceover-cases.md` requires mechanism/ARIA/timing/lifetime/focus match plus automated and actual browser checks. No new passing listening claim made here. |
| Contrast (all subsections) | Root/gallery owns computed custom-color measurements. This audit does not apply application contrast thresholds to stock HeroUI appearance. No ratio inferred from screenshots. |
| Pointer target size | Shared Button xs/RowControls 24px and 4px gaps; resize grip targets; avatar delete target extension; trust checkbox expansion; shell tab pseudo-hit area inspected. Tree chevron 16px has same expansion action on its larger row (`FileTree.tsx:163`, `ChangeTree.tsx:120`), so no unsupported target defect is filed. Preserved narrow sidebar and paired diff separator exceptions. |
| Not by colour alone | IconToggle shapes/state; status glyphs and text; panel chip shape and appearance-choice exception retained. Root owns Alert indicator regressions and unavailable-agent badges. |
| Text direction | Technical input/value/source and report ratio explicit LTR; isolated sentence values/plain textValue split; mirrored directional icons, logical offsets and resize physical direction inspected. I-06 confirms typed-word omission. Catalog agent separately inventories sentence interpolation. Actual ar/en same-width checks remain gallery evidence. |
| Motion / Dynamic collections | Shared hook and virtual fold semantics inspected, plus every production motion/list marker caller. I-03 missing mutable lists; I-04 missing reduced-motion guards. Static model choices, replacement directory/page listings and fixed import snapshots are exempt. |

## Editable input inventory

Fresh source inventory for the form correction on aggregate base `d2ebaeffb`, including current resume-by-ID fields.
Every production `TextInput`, `TextAreaInput`, `Input`, `InputGroup.Input`, `SearchField.Input`, editable ComboBox and
native input/textarea site was searched, and shared rename callers were checked separately. The findings below
retain the original audit observations; this table describes the corrected current contracts.

| Render site / caller | Requiredness and empty semantics | Placeholder after changes |
| --- | --- | --- |
| ConsoleDialog Name, new and edit | Required; submission validation unchanged | localized Ex. Work, including prefilled edit |
| RenameDialog via RequestedDialog rename-project | Required; old name is not an empty default | localized Ex. Website |
| RenameDialog via RequestedDialog rename-session | Required | localized Ex. Review changes |
| RenameDialog via ShellTerminalPanel rename | Required; shell trims as before | localized Ex. Build |
| DirectoryPicker editable navigation path | Required navigation target; Go lists typed path, never implicitly selects an unresolved one | localized Ex. ~/code/website, even while resolved populated path is present |
| ConfirmDialog typed word | Required for heavy deletion; same localized case-insensitive comparison and raw value/composition | localized Ex. {UPPERCASE localized word}; chip + Input explicit LTR, approved ordinary italic chip retained |
| ProjectDialog Repository URL | Required git source, pending/refusal locks dependent fields | localized Ex. https://github.com/owner/website |
| ProjectDialog Directory | Required local/parent source | localized Ex. ~/code |
| ProjectDialog Clone into | Optional git parent; blank uses settings.default_clone_dir | current defaultCloneDir retained; initial clone value and independent local/clone states unchanged |
| ProjectDialog Name, new | Optional; daemon derives normalized directory basename or repository clone basename | projectNameDefault mirrors hostfs.expand/lexically_normalise + coordinator basename/root fallback and hostfs.repo_name URL/scp extraction; updates from current path/source/URL, never changes input value or submitted name |
| ProjectDialog Name, edit | Required; blank must not silently retain old name | localized Ex. Website, populated and cleared |
| ProjectDialog Description | Optional; blank clears/no description | existing No description |
| ProjectDialog Tags → TagsInput | Optional list; blank draft adds no tag, existing chips remain | No tags when list empty; No additional tags when list nonempty |
| SessionDialog Name, New and recordName Resume branch | Optional new-session name, context derived | existing session.defaultName(project/console name) |
| ResumeConversationFields Session ID | Required, only initially editable Resume field | existing localized Ex. canonical conversation ID; LTR and stable InputGroup spinner retained |
| ResumeConversationFields archived Name | Optional; empty retains stored title | existing target.session.title |
| ResumeConversationFields interrupted/live Name | Disabled, not editable | stored title placeholder retained; focus handoff to Session ID if disabled/disappears remains unchanged |
| ConsoleDialog advanced instructions | Optional, blank no additional instructions | existing No additional instructions |
| AccountDialog Name new/named edit | Required | existing localized Ex. Work |
| AccountDialog default-account Name | Optional, blank clears custom name to standard default | existing localized Default account |
| AccountDialog configuration directory | Optional; agent-specific actual default | existing AGENT_DEFAULT_CONFIG_DIR[agent], dynamically follows agent; no change |
| CloneDirSetting | Optional setting; blank restores built-in ~/Projects | existing ~/Projects; LTR input group; no change |
| ImportReview explicit directory | Required when choice=directory; per-project destination | localized Ex. ~/code/website; rich project-name label and plain accessible label retained |
| ProjectFilter text search | Optional search condition, no content to create | existing Filter projects search hint; excluded from example/default metadata rule |
| ProjectBrowser → FileFilterField Files mode | Optional search condition | existing Which file are you looking for? |
| ProjectBrowser → FileFilterField Changes mode | Optional search condition | existing Filter changes |


Static selected options are selection machinery. Hidden avatar file input is activated through a named button;
xterm input, disabled Markdown task inputs, agent-authored report fields and gallery/website controls are renderer
or fixture content. Search fields retain their semantic filter hints. None substitutes for a required/default
metadata hint. The name hint mirrors the daemon's lexical path/home/root and clone-basename rules without filesystem
resolution; when a valid source is not yet known it names the directory/repository default rather than inventing one.

## Confirmed findings and fix handoffs

### I-01 — Required inputs omit placeholders

Rule: Input placeholders, including the explicit requirement to supply one even when current value hides it. Sites: console name `ConsoleDialog.tsx:169`; shared rename `RenameDialog.tsx:47` and all three caller classes listed above; directory path `DirectoryPicker.tsx:121`; confirmation word `ConfirmDialog.tsx:119`; edited project name `ProjectDialog.tsx:279` explicitly undefined.

Scenarios: `dialog-new-console-accounts`, `dialog-edit-console-agents`, `dialog-rename-project-text-too-long`, `dialog-directory-picker`, `dialog-edit-project-description`, `dialog-delete-console`; shell rename is reachable through `shell-panel-header-menu` → Rename. No dedicated session-rename/word-placeholder assertion exists. Empty each populated field to expose the omission; no daemon mutation is needed.

Fix scope: add localized valid-example placeholders for required fields, pass appropriate context through shared RenameDialog callers, and add a confirmation-word placeholder without populating its value. Keep validation, autofocus and submit behavior. Validate populated and cleared fields in light/dark en/ar plus one CJK language, including new/edit paths and all rename callers. Do not change HeroUI placeholder colors.

### I-02 — Example/default semantics do not match requiredness or live context

Rule: Input placeholders. `ProjectDialog.tsx:248`, `:262` and `ImportReview.tsx:163` use bare required path/URL examples (`i18n/messages/en.ts:947`, `:949`, `:1303`), without localized Ex. prefix. New optional project name at `ProjectDialog.tsx:279` uses fixed Ex. Website rather than the directory/repository name that an empty submission actually uses. Optional tags at `ProjectDialog.tsx:304` use Ex. frontend rather than naming their empty/default state.

Scenarios: `dialog-add-project`, `dialog-add-project-detected`, `dialog-add-project-git-unreachable`, `dialog-edit-project-tag-suggestions`, `configuration-import-review-directory`. Existing scenes provide the fields but not a context-change/placeholder assertion for every branch. Fix scope: localize example prefixes in every offered catalog; derive new name placeholder from the same current source as the submission default; change tags' empty hint without creating a default tag. Preserve per-agent directory and optional clone defaults. Validate required URL/local/import paths and optional defaults before/after repository/path/source changes, including untouched empty submission semantics.

### I-03 — Mutable tab and quota collections bypass shared list motion

Rule: Motion / Dynamic collections. Shared `AgentTabList.tsx:71` directly maps mutable added/installed agents with no list-motion ref or markers; Settings add/remove and Move earlier/later commit this order, and rail availability changes update it. `tabDrag.ts:52` only sets a 150ms sibling transform during dragging, not shared 300ms committed collection movement/entry/exit. `dialogs/ConsoleLaunchFields.tsx:87-99` separately maps mutable `listed` agent tabs without shared motion; clearing the last stored launch setting can remove a missing/unadded agent tab while Advanced stays open (the caller documents this lifecycle). `ShellTerminalPanel.tsx:252` similarly maps mutable terminal tabs without shared motion. Quota account list `RailQuotaEntry.tsx:330` maps live owned accounts without motion while the panel can remain mounted as settings events change.

Scenarios: `settings-accounts-agents-reordered` exercises a committed move, `dialog-edit-console-agents-not-added` and `dialog-edit-console-launch-settings` expose the Advanced tabs (a specific last-stored-setting-cleared assertion is missing), `shell-panel-tabs`/`shell-panel-many-tabs` allow create/close, `rail-quota-pinned-codex` shows the live account surface. No current scenario sends account/availability updates while the quota panel stays open; that scenario is missing, not waived.

Fix scope: integrate shared list motion at each actual collection element, with stable agent/terminal/account IDs and lazy mounting respected. Preserve React Aria tab focus/selection, pointer drag, settings move-menu focus return, shell close handoff and quota height stabilization. Coordinate drag transforms with committed motion so they do not double travel. Validate add/remove/reorder both axes in en/ar; actual removed controls must disappear immediately, ghost copies must be inert/hidden; no mount/scroll/resize motion; reduced-motion update cancels running effects. Library tab indicator movement alone is not a substitute for surviving-item movement/entry/exit.

### I-04 — Application transitions do not consistently honor reduced motion

Rule: Motion, all nonessential animation/transition stops under reduce. Sites: `sidebar/rows.tsx:91` transition-colors; `sidebar/Sidebar.tsx:578` inactive overlay color transition; `archive/ArchiveView.tsx:278` action opacity; `components/ResizeSeparator.tsx:169` line color. These authored classes have no motion-safe/reduce guard. Shared reduced-motion rules in `style.css` are scoped and do not globally remove them.

`dialogs/useHeightMotion.ts:39` checks preference only before starting a height animation; changing preference while one runs never cancels it. `components/tabDrag.ts:35` snapshots preference once at drag start, so a running drag's sibling transitions continue after a preference change. The shared useListMotion already listens for changes; do not duplicate its implementation.

Scenarios: ordinary project/session sidebar rows, `dialog-edit-project-description`/Advanced content changes, archive overflow, `shell-panel-stacked` resize, and `settings-accounts-agents-reordered` drag surface. Existing scenarios do not assert a preference change during a running height/drag animation. Fix scope: guard authored transitions and cancel active authored effects on preference changes. Validate computed transition/active WAAPI state under reduce, plus switching to reduce during motion, without overriding stock HeroUI styles or disabling essential pointer tracking.

### I-05 — Terminal notices can mount with their first live-region text already present

Rule: Announcing a change requires a mounted empty region and deferred first write, not a role alone. `TerminalPane.tsx:480` writes cover text directly into raw role=status, including an already-present starting/loading state; `:517` writes already-present dormant notice. `ShellTerminalView.tsx:267` and `:270` do the same for trouble/exited status. Later text changes in persistent regions are distinct and are not claimed broken here.

Scenarios: `terminal-starting`, `terminal-reconnecting`, `terminal-disconnected`, `shell-panel-reconnecting`, `shell-panel-disconnected`, `shell-panel-exited`; inspect which state exists on first mount versus arrives later. `shell-panel-exited` mounts an exited notice from initial fixture state. Source establishes missing deferral; it does not establish a new observed VoiceOver listening failure.

Fix scope: use the established status mechanism for initial status writes while retaining visible durable copy/actions and avoiding duplicate announcements. Keep one announcer for the shown shell tab only. Match an existing passing VoiceOver case item by item (lifetime, modal exposure, timing relative to F6/session focus, initial and later writes), run automated/browser exposure checks; if none matches, perform the required fixture-backed packaged VoiceOver pass. Never claim a browser AX snapshot is a listening result.

### I-06 — Typed confirmation literal lacks explicit left-to-right direction

Rule: Text direction requires literal-to-type values on their drawing element to keep LTR. `ConfirmDialog.tsx:115` renders nonmonospace InlineChip without dir (its component default only supplies LTR for monospace), and `:119` input also lacks dir. Under ar both inherit RTL. Keep the approved ordinary italic UI font for confirmation word.

Scenario: `dialog-delete-console` supplies localized DELETE confirmation word. Add mixed-script/RTL confirmation checks; current fixture supplies a static translated word, not arbitrary script combinations. Fix scope: explicitly direction the chip and editable value/control as appropriate, without changing localized word validation or rewriting input mid-composition. Check Arabic actual rendered chip/input alignment and English/CJK regression; retain independent chip sizing and focus.

### I-07 — Live report replacement drops keyboard focus to the page

Rule: Visible focus / Focus moved by a change the user did not make. `ReportPanel.tsx:191` keys PageFrame by page ID, removing the previous focused iframe when a new report arrives while following newest. There is no focus handoff at that replacement boundary.

Actual interaction evidence: `evidence/interaction-probes.json` → `reportReplacement`, collected against the full App in `report-two-console-sessions`, light/en. Playwright focused the Version text input inside the opaque report iframe. A guarded in-memory gallery DaemonProvider store update appended the next page, equivalent to the application's page-created state update; the previous iframe disconnected, the new iframe mounted, and host document.activeElement became BODY. F6 subsequently recovered to top-bar Back, but no automatic handoff occurred. This used only the existing fixture daemon, with no production hook, file change or real daemon.

Fix scope: preserve a meaningful keyboard focus destination when a newest-following report replaces a focused frame, using keyboard modality for the unsolicited handoff and respecting focus elsewhere. Preserve ordinary pager focus and current history-following rules. Validate new-page arrival with focus inside report controls, on the pager and elsewhere; appearance/RTL and stale/history cases; prove focused replacement never leaves body. Add a maintainable gallery regression path rather than exposing a production daemon/store hook for tests.

### I-08 — Tree chevron bypasses HeroUI Button without the required justification

Rule: Keyboard / Buttons. `browser/treeRow.tsx:3,22` imports and renders bare React Aria Button for `slot="chevron"`, with no comment explaining why HeroUI cannot serve. The installed HeroUI implementation at `packages/ui/node_modules/@heroui/react/dist/components/button/button.js` directly wraps the same React Aria primitive, explicitly forwards `slot` and remaining props, and its declarations extend the primitive's props. The collection slot does not itself prevent using the shared Button.

The section heading is **not** the same finding: `browser/changeSectionHeading.tsx:108-110` explicitly explains why the whole heading band needs a bare primitive instead of HeroUI's fill/size/padding; preserve that documented custom-control treatment. Actual `git-changes-tree` ArrowUp after a pointer press landed on its heading with data-focus-visible and a 2px inset ring.

Actual `files-tree` browser checks in `evidence/interaction-probes.json` ran in light/dark en/ar. A row's inline-direction arrow reached its folder action, then wrapped to the row; Enter kept focus on the row and ArrowDown reached the next row, all with visible-focus attributes and rings. The chevron is 16 by 16 and tabindex -1, with equivalent expansion available through its larger whole row. No separate target, naming or focus defect is inferred from that measurement.

Fix scope: use the shared HeroUI-backed Button with supported variant/sizing, retaining slot integration and collection-managed traversal, tooltip/name, directory/commit expansion, and row activation. Do not add an independent Tab stop. Verify Files and grouped Changes in en/ar and both themes, including Enter/Space, row menus and focus rings. If a real incompatible requirement is discovered during implementation, record it for user agreement rather than silently inventing a new exception.

## Targeted browser evidence and unresolved native checks

`evidence/interaction-probes.mjs` is the reproducible fixture-only collector. `interaction-probes.json` records four Files-tree keyboard traversals (en/ar × light/dark), one Git section-heading arrow landing, actual token syntax in both themes, and the live report focus-loss reproduction. Its node syntax check and repository diff whitespace check passed; the parent records overall lint/check results. These checks do not establish all UI interactions or native behavior.

- **R-02 native/announcement verification:** AppKit informational menu items, packaged programmatic focus rings, imported-result renamed dialog announcement and terminal first writes still require the relevant recorded-case matching or actual listening where no case matches. Existing open VoiceOver cases remain open. No real daemon or account login was used.

No out-of-standard defect is asserted by this interaction pass. Any unrelated data/daemon defect revealed later belongs in a separate bug ticket under the plan.

## Implementation scope handoff

1. **Input placeholder and typed-literal semantics:** I-01, I-02 and I-06; shared dialog fields plus callers/catalogs. Self-contained review of examples/defaults/LTR, existing validation and keyboard paths.
2. **Mutable collection motion:** I-03; shared agent tabs, console Advanced agent tabs, shell tabs and quota account rows, coordinated with drag behavior. Requires normal and reduced motion, RTL, virtualization/scroll exclusion and focus/removal evidence.
3. **Reduced-motion cleanup:** I-04; authored CSS transitions and running height/drag preference changes. Can be reviewed independently; coordinate the shared tabDrag file with collection milestone rather than editing it concurrently.
4. **Terminal first-status delivery:** I-05; source/browser status lifetime and existing VoiceOver case match or necessary fixture-backed native listening. Preserve terminal region escape/reconnect behavior.
5. **Report focus and tree primitive:** I-07 and I-08 are confirmed focused follow-ups; they can be separated if independent review is clearer. Preserve the explicitly justified section-heading primitive and collection keyboard model.

The accepted numbered milestones in the overview combine these scopes with Alert/copy/navigation/visual findings. This evidence record makes no production change.
