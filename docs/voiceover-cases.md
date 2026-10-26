# VoiceOver verification cases

Real-device VoiceOver results for the packaged macOS application, kept as reusable cases. A new implementation that
matches a passing case below needs no further real-device testing; one that matches none needs one listening pass and a
case of its own. How to build an announcement lives in `docs/ui-accessibility.md` ("Announcing a change"); this file
holds what was actually heard.

## Rules

### When a real-device pass is required

Real-device testing is required only when the implementation approach changes **and** no implementation of that kind has
been tested before. "Of that kind" means "matching a passing case" as defined below, so the criterion reduces to one
test: **a pass is required exactly when no passing case matches the new implementation.** A failing or unconfirmed case
counts as untested: it never satisfies the criterion, and an implementation of its kind needs its own pass.

A caller that matches a passing case claims that case's result by passing the checks under "Claiming a match"; it adds
no new listening result. A new caller, location or message alone, with every item under "What \"matches a case\" means"
the same as a passing case, is a match, not a new approach.

### What "matches a case" means

A new caller matches a case only when every item below is the same as the case's, as rendered in the packaged
application, not as written in source:

- **Announcement mechanism.** The same kind of element and the same component: a `StatusAnnouncer`, a `role="status"`
  region on durable copy, a focus move onto a renamed dialog, or the accessible name/description of a control read when
  it takes focus. A different mechanism is a different kind even when the text is the same. A `StatusAnnouncer` rendered
  as a `span` and as a `div` count as the same: its own comment says the announcement is the same either way, and a
  `span` (SA-G) and `div`s (the others) were both heard.
- **ARIA wiring.** The same effective role, name and description sources (own text, `aria-label`, `aria-labelledby`,
  `aria-describedby` to a hidden subtree), the same live attributes in effect (politeness, atomicity, relevance), the
  same nesting (no nested status or alert) and the same decorative content (`aria-hidden` icons).
- **Rendering surface.** The packaged application's WKWebView on macOS. A browser, another OS or another screen reader
  is not covered by any case.
- **Timing relation to focus.** Whether the text is written before or after a focus move, how long after, and whether
  focus stays put or moves while it is written. A write within about 300 ms of a focus move is a different relation from
  one made while focus has rested.
- **Write delay.** None, once per mount (`StatusAnnouncer`'s first write delay), per appearance (the banner's own gate),
  or a caller wait before each text (a timer started at a state change, or the attended wait of `PermissionsAnnouncer`
  and `AttendedStatusAnnouncer`), and whether `firstWriteDelayMs` overrides it.
- **Exposure beside a modal.** Whether the region is exposed at the moment it writes: in the main window while no modal
  is open, inside the modal's own subtree, or kept exposed beside a modal through a `data-live-announcer` or top-layer
  marker. These three count as the same; a region under an open modal without a marker is hidden, and a write into it is
  a different item (see KNW-2).
- **Window attention.** Whether the write is held until the window is attended (visible and focused) or happens whatever
  the window is doing.
- **Region lifetime.** Whether the region is mounted empty and stays mounted, or is mounted already holding its text,
  and what ends its life (a panel switch, a removed row, a closing dialog).
- **Change shape.** Text replaced in place, text first set in an empty region, or text read on demand only.
- **Composition of the text.** For a case about what is spoken from a control, the same way the name, state and
  description are put together (for example a name followed by a badge, or a description reached through
  `aria-describedby`). Different wording in the same composition is still a match.
- **Focus after the action.** For a status written in response to a press, where focus is afterwards: kept on the
  control, moved by the caller, or lost to the web area because the control went away.
- **Focus destination.** For a case about where focus lands after a dialog closes, the same opener situation (opener
  kept, disabled or gone) and the same destination rule.

Appearance is not a matching item: a case's environment line records the appearance it was heard in, and a caller in
another appearance still matches, noted as outside that record (Open items lists the appearances not heard). A language
that changes the composition of the text (word order, a right-to-left layout) needs its own pass. Any difference in an
item is not a match. A case's own "Reuse scope" line says which items it constrains most; where a case is silent on an
item, it does not cover a change in that item.

### Claiming a match

Passing these checks establishes that the new caller is wired like the case. They do not replace the case's listening
result and they are not a listening result themselves; report them as checks.

- Automated tests of the caller's wiring: the region's lifetime and text changes, duplicate suppression, and focus
  behavior around the write.
- Browser inspection of the rendered DOM: the effective role, accessible name and description, the live attributes, and
  that neither the region nor an ancestor is hidden by `aria-hidden` or `inert` while the real overlay is open (a marked
  element is left exposed with its ancestors; the rule is in `docs/ui-accessibility.md`, "Announcing a change").
- A statement of which case is matched and, item by item, why each point under "What \"matches a case\" means" holds.

### When a new listening pass is needed

- No passing case matches: one representative pass, then record a case.
- A new implementation that fixes a failing case is also checked against that case's symptom before a passing case is
  recorded. A pattern listed under "Known not to work" bars a match for the aspect it records: a speech pattern differs
  from every passing case in at least one item, so a caller wired like one matches no passing case and needs the pass; a
  pattern that records only one aspect of a passing case's recording (a focus destination in KNW-8, a dialog's naming in
  KNW-9) leaves that case valid for the rest of what it heard.
- A change to the live-region role, mounting or timing behavior of a mechanism that a passing case covers makes the case
  not match the changed caller.

### Evidence and its limits

- Rechecks keep the earlier results: a later build passing does not erase an earlier build's failure, which stays as a
  known-not-to-work pattern under its own build. A case states the build it was heard on; a later change to the
  mechanism it covers is judged by "What \"matches a case\" means", not by the build number.
- Existing code and passing DOM tests establish the implementation, not a listening history; do not report them as a
  listening result.
- The native VoiceOver caption window displaying an announcement is accepted as real-device confirmation of that
  announcement. Record it as caption verification, with the displayed text and the state that caused it.
- Reading VoiceOver's last phrase through AppleScript (`content of last phrase`) is accepted as actual native speech
  output where no human listener is present. Record it as such.
- Reading a control manually (moving the VoiceOver cursor onto it) shows the text is reachable. It is not proof of an
  automatic announcement; a case records the two separately.
- Native accessibility attributes (as macOS WebKit serializes them) show the effective role, name and live values. They
  are not the literal HTML DOM: generated ID strings in `aria-labelledby` and `aria-describedby`, and the markup a
  component library generates, were not extracted. A case therefore states the source wiring and the native effective
  values separately and does not claim literal attributes beyond what was captured.
- A fixture CLI exercises the real daemon coordinator and the real UI, but not a real login, authorization, credential
  or quota. Cases recorded with a fixture say so and are not evidence for real-login behavior.
- Speech is recorded by polling VoiceOver's last phrase, which shows changes of that property. "Once" means one change
  of the exact phrase, corroborated by one native text transition; an identical consecutive replay cannot be counted
  separately by this method. Delays between native text and speech include IPC and polling latency.
- Chinese fragments in recorded speech are verbatim native output of the VoiceOver interface language.

### How to record a case

Each case states its name, UI location and state change, mechanism with the component by file and symbol (never line
numbers), the ARIA wiring as rendered, the result (spoken automatically, readable manually only, or not spoken) with the
exact phrase, how it was captured, the environment, the date, and the reuse scope. A failed or unconfirmed observation
is recorded too, so nobody rebuilds a pattern already known not to work.

## Environments

Common to every case unless a case says otherwise: macOS 26.2 (25C56); application interface English; system and
VoiceOver interface Simplified Chinese; Settings appearance following System, light (the 2026-10-14 banner record is
dark); the packaged application's WKWebView in an isolated confirmation instance, not the user's own. No human listener:
speech was read from VoiceOver through AppleScript's `content of last phrase`, polled every 25 ms in the rechecks; the
native caption window was enabled but was not the reader, and no synthetic VoiceOver `output` command was used. After
the confirmation build's web view restarts, VoiceOver must be restarted and the window raised before it tracks the web
content again; a poll made before that returns nothing and is not evidence. A run that hit an automatic screen lock was
excluded. Output volume and mute were left as found; no VoiceOver or system setting was changed. Input was limited to
accessibility actions, keyboard and pointer events targeted at the confirmation build's process, and VoiceOver cursor
commands. Native accessibility trees were read separately (VoiceOver off or on) as rendering evidence, not speech.

| Label | Build | Notes |
|---|---|---|
| E1 | An unmerged comparison build, not on main | First round. Login in its first form: a plain `role="status"` region, no `StatusAnnouncer`. |
| E2 | An unmerged local build: an earlier main + the Settings opener fix + an earlier login fix | The login success text is written by a caller timer 1500 ms after success, with no window-attention gate and no focus hand-off. |
| E3 | An unmerged local build: an earlier main + the Settings opener fix + an intermediate login fix | Adds a focus hand-off after a successful close only (not after a cancel); no window-attention gate. |
| E4 | An unmerged local build whose `packages/` and `apps/` tree equals main `429be4556` (fixes `39c1ec3f3` and `429be4556`) | The final product state of the login mechanisms; the same installed build was used for the later recordings of the other status callers (SA-G to SA-K, PLAIN-2, KNW-6 to KNW-8). |
| E5 | Branch `fix/voiceover-recheck-defects` as merged, last fix `42d775745`: main `29d9d1505` + `42b9984f8` (diff-expansion focus) + `e1191fc7c` (Permissions announcer) + `42d775745` (Advanced per-opening announcer) | Heard on the branch build itself, for the cases SA-L, SA-M and FOCUS-2 and for the status in SA-K. |
| E6 | Local main `cd10ab4e7020142bbabec5b052c447f5bf27c368` | Packaged `Octoboard Verification`, identifier and scratch ad-hoc signing identifier `dev.octoboard.verify.vo20261025h7gwl`; real daemon and throwaway files/Git repository. NAME-5, SA-N, FOCUS-3 and ALERT-1; no real agent or quota. |
| E7 | Local main `2d8e1a705888dbe319da3fc2c0e0951d6f9766d5` | Packaged `Octoboard Verification`, identifier and scratch ad-hoc signing identifier `dev.octoboard.verify.vo6-20261025-fyq175xi`; WARN-1 and WARN-2, real daemon and Git data with fixture Codex CLI/records, no Safari or gallery. |
| E8 | Local main `9d199cb42573568a07d618034494a95c75a0a2c7` | Packaged `Octoboard Verification`, identifier and scratch ad-hoc signing identifier `dev.octoboard.verify.vo8-20261025-2j89ms59`; FOCUS-4, bare Error and Note viewer-header marks with real daemon/throwaway files; no agent, browser or gallery. |
| E9 | Branch `fix/startup-error-details` at `f37fc3d925f35eb78f1048ea4767df369a1f6b1d` | Packaged `Octoboard Verification`, identifier and scratch ad-hoc signing identifier `dev.octoboard.verify.vo9-ddnbws96`; WARN-3, WARN-4 and WARN-5, real daemon with isolated HOME/TMPDIR/agent directories, fixture Codex CLI and dated records, throwaway real Git repositories and worktrees; no Safari or gallery. |

E4's tree equals main `429be4556`. E1 to E3 and the build commits behind them were unmerged local-only builds, so they
are not durable references and are not cited by hash. E5's three commits are the fixes as they landed on main; the build
was made from the same changes on the branch before it was rebased onto main. E6, E7 and E8 were built directly from the
committed local main revisions listed in the table.

The login cases used a credential-free fixture standing in for the Grok Build CLI, run through the real daemon's login
coordinator and the real UI. It validates rendering, announcement and recovery; it does not validate real CLI output
formats, authorization, credentials or quota. For exactly `login --device-auth` it sleeps 10 s and emits a reserved URL,
sleeps 10 s and emits a device code, sleeps 10 s, writes an empty auth marker and exits successfully. No browser opens
(the agent opens its own page, which the fixture does not) and nothing is sent over a network. Only the clone-targeted
accessibility, keyboard and pointer routes were used; no real login was performed.

E6 differs from the common recording procedure above. Application text was English and VoiceOver output mixed English
roles with Simplified Chinese. The operating-system version and caption-panel state were not independently recorded in
that pass; the application appearance is not recorded (E7 states its appearance). Whether a frontmost-process check
preceded each global input is not recorded. AppleScript last-phrase changes were polled at a requested 25 ms interval,
with additional deliberate reads and sparse native trees. The machine locked partway through setup; locked-screen empty
phrases were excluded, and VoiceOver was restarted to resume tracking and left off as originally found. Global System
Events keystrokes (Command-F5 among them), `cliclick` and real pointer movement were used, as well as process-targeted
accessibility actions and VoiceOver cursor commands, so input was not confined to the application's process. The cases
preserve the observations transcribed from the tool output, not retained raw artifacts.

E7 differs from the common recording procedure in its global input (Tab/Enter, pointer hover and Command-F5), each
preceded by a frontmost-process check for the isolated application; accessibility actions and VoiceOver cursor commands
were preferred. It used macOS 26.2 (25C56), English application text, mixed English/Simplified Chinese VoiceOver output
and the light application appearance. The caption panel was visible but was not the reader. Last-phrase polling,
deliberate cursor reads and native trees supplied the evidence. The fixture CLI used no credentials or model service.
Three ordinary fixture sessions were created and bound through the real daemon, marked used through its hook route, and
archived together; synthetic records used Codex's dated rollout layout so inspection could report `present`. Real branch
creation, daemon branch-reference validation and inspection established ownership of the uncommitted and locked
worktrees. A separate fixture session emitted the trust question through its PTY for the real recognizer. Throwaway
HOME, agent directories and identifier-specific WebKit storage isolated the run.

E8 used macOS 26.2 (25C56) and English application text, with mixed English/Simplified Chinese VoiceOver output.
Appearance and caption-panel visibility were not independently recorded in this pass. Accessibility actions targeted
only the isolated application; the global input actually used was System Events Enter/Escape and Command-F5, each
preceded by a frontmost-PID check for that app. No global pointer input was used. The build-time identifier, scratch
signing identifier, isolated daemon database/lock/port and identifier-specific WebKit storage were checked before
driving the window. The user explicitly authorized Command-F5 and the global input used.

E9 was built from a commit of an unmerged branch, `git archive`d into a scratch copy (not a worktree); only packaging
metadata was changed in the copy (identifier and native product name), and the UI was built with its original product
name. The bundle was ad-hoc signed with its unique identifier; no Developer ID, notarization or update signing. It used
macOS 26.2 (25C56) on arm64, English application text, mixed English/Simplified Chinese VoiceOver output and the light
application appearance, confirmed from a screenshot of its own window. The caption panel was enabled but was not the
reader. Global input (System Events keys including Command-F5, `cliclick` and real pointer input) and each VoiceOver
cursor move were preceded by a frontmost-PID check for the isolated application. Accessibility actions were preferred;
the archive row's inner buttons were not exposed as separate accessibility children, so two guarded Tabs, one guarded
hover and a guarded Enter opened its Delete confirmation, which is setup, not evidence about the dialog. The first
background recorder expired after its 10-minute bound and a second was started; speech from other applications picked
up during setup was excluded, and every quoted phrase was correlated with a deliberate action in the isolated
application. Sessions, binding, hooks, branch-reference validation, inspections and archiving went through the real
daemon; Codex records were synthetic dated rollouts, with no login or model use.

Every `StatusAnnouncer` in the SA cases renders an unnamed `sr-only` `role="status"` with no explicit `aria-live`,
`aria-atomic` or `aria-relevant` props. For E1 to E5, the captured effective native mapping is a group with the
application-status subrole (empty-group subrole while empty), role description "application status", empty own name,
title, description and value (the own value was not sampled for short-lived statuses by the compact native observer),
the phrase in a descendant static text, live polite, atomic true, relevant additions and text. This is the native
mapping macOS WebKit supplies, not a literal HTML serialization. E6 captured fewer native fields; SA-N states those
limits rather than claiming this complete mapping. PLAIN-1 and PLAIN-2 are hand-built regions and state their own native
mapping.

## Cases

### Summary

| Case | Mechanism | Result |
|---|---|---|
| [NAME-1 Retained picker trigger](#name-1-retained-picker-trigger) | control name read on focus | Pass |
| [NAME-2 Opened picker option](#name-2-opened-picker-option) | option name read on navigation | Pass |
| [NAME-3 Console account row](#name-3-console-account-row) | `aria-describedby` to a hidden name and badge | Pass |
| [NAME-4 Advanced tab](#name-4-advanced-tab) | tab name read on focus | Pass |
| [NAME-5 Conflicted file heading includes its kind](#name-5-conflicted-file-heading-includes-its-kind) | single-string viewer heading and dialog name | Pass |
| [SA-A Later change, no focus move](#sa-a-later-change-no-focus-move) | `StatusAnnouncer`, exposed beside a modal | Pass |
| [SA-B Outage with a per-appearance gate](#sa-b-outage-with-a-per-appearance-gate) | `StatusAnnouncer` with a caller gate | Pass |
| [SA-C Delayed first write after a dialog opens](#sa-c-delayed-first-write-after-a-dialog-opens) | `StatusAnnouncer` with `firstWriteDelayMs` | Pass |
| [SA-D Written after a dialog closes](#sa-d-written-after-a-dialog-closes) | `StatusAnnouncer` written by a caller timer, gated on window attention | Pass: menu, direct button, return to the window; possibly stale, row layout changed |
| [SA-E Success already present at mount](#sa-e-success-already-present-at-mount) | suppression at remount | Pass; possibly stale, row layout changed |
| [SA-F Hidden status text stays navigable](#sa-f-hidden-status-text-stays-navigable) | plain text in the status after its announcement | Pass |
| [SA-G Confirmation that clears with its tooltip, focus retained](#sa-g-confirmation-that-clears-with-its-tooltip-focus-retained) | `StatusAnnouncer` span, default delay elapsed | Pass, attribution shared with the button's renamed name |
| [SA-H First mount at the default delay, no focus move](#sa-h-first-mount-at-the-default-delay-no-focus-move) | `StatusAnnouncer` whose text is present from the first render, empty until its first write, focus kept | Pass |
| [SA-I Default-delay first mount while a menu takes focus](#sa-i-default-delay-first-mount-while-a-menu-takes-focus) | `StatusAnnouncer` in a submenu | Pass, attribution shared with a visible item |
| [SA-J New child at 1500 ms inside an open dialog, focus retained](#sa-j-new-child-at-1500-ms-inside-an-open-dialog-focus-retained) | `StatusAnnouncer` with `firstWriteDelayMs` in a viewer | Pass |
| [SA-K Loading written, then the pressed control is removed](#sa-k-loading-written-then-the-pressed-control-is-removed) | `StatusAnnouncer` in a diff view | Pass for the status; focus relation recorded per build (E4 fell, E5 restored) |
| [SA-L Section status written after an attended wait, tab focus retained](#sa-l-section-status-written-after-an-attended-wait-tab-focus-retained) | section `StatusAnnouncer` fed by a caller wait | Pass; direct opening untested |
| [SA-M Region mounted with each opening of a disclosure](#sa-m-region-mounted-with-each-opening-of-a-disclosure) | `StatusAnnouncer` mounted per opening | Pass |
| [SA-N Viewer notice after paint, then an empty status](#sa-n-viewer-notice-after-paint-then-an-empty-status) | `FileNotice` through `StatusAnnouncer`, then clear; first-paint large-file notice without an announcement region | Pass for the recorded branches and focus hand-off; native live attributes not sampled |
| [FOCUS-1 Focus after the login dialog closes](#focus-1-focus-after-the-login-dialog-closes) | explicit focus hand-off | Pass; one transient phrase observed and accepted; possibly stale, button and row changed |
| [FOCUS-2 Focus after an expansion takes the pressed control away](#focus-2-focus-after-an-expansion-takes-the-pressed-control-away) | caller-told focus restore and code-frame hand-off | Pass |
| [FOCUS-3 Status mark and quota popover focus and reading paths](#focus-3-status-mark-and-quota-popover-focus-and-reading-paths) | shared hover/press popover, focus and item chooser | Pass for quota Enter/Escape focus paths, the mark's focus return on Escape and item-chooser reachability; the bare-mark Enter naming failed, see KNW-9 and FOCUS-4 |
| [FOCUS-4 Bare mark dialog named by its tone](#focus-4-bare-mark-dialog-named-by-its-tone) | tone-named bare mark dialog; sentence remains the trigger name | Pass for Enter speech without the repeated sentence and Escape focus return, Error and Note |
| [PLAIN-1 Text change in a mounted plain status](#plain-1-text-change-in-a-mounted-plain-status) | `role="status"` replaced in place inside a modal | Pass (first build) |
| [PLAIN-2 Persistent visible status with an action](#plain-2-persistent-visible-status-with-an-action) | plain `role="status"` filled in place | Pass |
| [ALERT-1 A conflicted file read fails](#alert-1-a-conflicted-file-read-fails) | delayed `PaneFailure` alert inside the viewer | Pass for file-not-found |
| [WARN-1 Deletion warning order and checked option descriptions](#warn-1-deletion-warning-order-and-checked-option-descriptions) | warning title, session list and consequence text; checkbox name and hint | Pass (history; superseded by WARN-3) |
| [WARN-2 Trust prompt caution prefix and description](#warn-2-trust-prompt-caution-prefix-and-description) | hidden caution prefix, visible title, then agent-specific text | Pass in packaged app with fixture Codex CLI (history; superseded by WARN-5) |
| [WARN-3 Deletion warning modal with option notes outside the checkbox](#warn-3-deletion-warning-modal-with-option-notes-outside-the-checkbox) | warning modal: plain body, session list and consequences; checkbox name and an external note through `aria-describedby` | Pass for focus and sequential reading; opening announcement unconfirmed |
| [WARN-4 Remove project with running sessions and a lookup-failure mark](#warn-4-remove-project-with-running-sessions-and-a-lookup-failure-mark) | warning modal with running sessions; external note; a separate mark after the checkbox | Pass |
| [WARN-5 Trust prompt caution as plain body text](#warn-5-trust-prompt-caution-as-plain-body-text) | the caution as one plain paragraph, no prefix or title | Pass for sequential reading; opening announcement unconfirmed |
| [Known not to work](#known-not-to-work) | nine failing or negative observations (KNW-1 to KNW-9; each names the case that replaced it) | **Fail** (history) |
| [OPEN-1 Import result dialog rename](#open-1-import-result-dialog-rename) | focus move onto a renamed dialog | Unconfirmed |

### NAME-1 Retained picker trigger

- **Location and state change.** Console settings, Default agent, "Console sessions". A saved default of Codex, a named
  Codex account and a supported non-default console effort, then Codex removed from the added agents, then the editor
  reopened. The stored selection stays visible and is badged "Not added".
- **Mechanism.** The accessible name and description of the trigger, read when it takes focus. No live region and no
  write delay. `packages/ui/src/dialogs/OptionSelect.tsx` (`OptionSelect`: the select's `aria-label`, optional
  `aria-labelledby` and descriptions, and the selected value rendered with `StatusBadge`), fed by
  `packages/ui/src/dialogs/ConsoleDialog.tsx` (label "Console sessions", hidden label, stored default agent) and the
  agent option list in `packages/ui/src/agents.ts` (held agents are retained and badged "Not added").
  `packages/ui/src/components/StatusBadge.tsx` adds an `sr-only` comma and space before the visible "Not added" text; it
  has no role, live attribute or explicit label. The agent icon is `aria-hidden`.
- **ARIA wiring as rendered.** Native tree: a pop-up button with has-popup set, enabled; title and description `Codex ,
  Not added Console sessions`; no live, atomic or relevant attributes. The source select label "Console sessions" is
  distinct from the computed trigger content.
- **Result.** Spoken on focus: `Codex , Not added Console sessions 列表框弹出 按钮 Default agent A project with its own default
  agent uses that instead. group`. No unnamed badge was encountered. Repeated explicit focus reads were deliberate, not
  duplicate automatic announcements.
- **Capture.** Last phrase through AppleScript after focusing this exact control with an accessibility action.
- **Environment.** E1.
- **Date.** 2026-10-20.
- **Reuse scope.** Covers a select-style trigger built with `OptionSelect` whose selected value renders a name followed
  by `StatusBadge`, read on focus in WKWebView on macOS. It says nothing about live announcements. A trigger whose name
  or badge composition differs, or that is not built on `OptionSelect`, does not match.

### NAME-2 Opened picker option

- **Location and state change.** The same picker, opened after removal. Down arrow twice moved Claude Code, Grok Build,
  then the retained Codex. The retained choice stayed selected and was not reselected.
- **Mechanism.** Option name read on navigation. `packages/ui/src/dialogs/OptionSelect.tsx` (the list item with its
  `textValue` set to the option label, followed by ", " and the badge text when it has one, `isDisabled` from the agent
  option, a child name and `StatusBadge`; the select's `disabledBehavior` of selection, so a removed option can be
  navigated to but not chosen). No explicit option `aria-label` and no live region. That the child badge contributes to
  speech is a verified runtime outcome, not something `textValue` alone implies.
- **ARIA wiring as rendered.** Native tree: the container is a list described "Console sessions"; the retained item is
  static text titled `Codex , Not added`, selected, enabled, no live attributes. The source `isDisabled` flag does not
  map to a disabled native state here (observed: enabled, navigation permitted); do not claim it does.
- **Result.** Spoken on navigation: `Codex , Not added 已选择 （3/3）`. The preceding option read `Grok Build （2/3）`. No
  unnamed "Not added" phrase.
- **Capture.** Last phrase through AppleScript after keyboard events targeted at the confirmation build.
- **Environment.** E1.
- **Date.** 2026-10-20.
- **Reuse scope.** Covers an `OptionSelect` list item with a name plus `StatusBadge` child and `disabledBehavior` of
  selection. A list item with a different disabled mapping or without the badge child does not match.

### NAME-3 Console account row

- **Location and state change.** Console settings, Agent default accounts, the retained Codex account. A named account
  saved before removal; the editor reopened after removal to read persisted state.
- **Mechanism.** `aria-describedby` from the account picker to a hidden name-plus-badge subtree. No live region and no
  write delay. `packages/ui/src/dialogs/ConsoleAccountsField.tsx` (the visible agent name and badge wrapper is
  `aria-hidden`; its inner span carries a generated ID that the account `OptionSelect` receives as `describedBy` when
  the agent is marked "Not added"), and `packages/ui/src/dialogs/OptionSelect.tsx` (`describedBy` becomes
  `aria-describedby`). The agent name and state are therefore the picker's description, separate from its selected
  value; an explicitly referenced hidden subtree supplies the description.
- **ARIA wiring as rendered.** Native tree: a pop-up button with has-popup set, enabled; title and description `Speech
  badge fixture Codex account`; no live attributes. This native API did not serialize the `aria-describedby` ID itself.
  The speech confirms the referenced subtree is consumed; the whole spoken phrase is not the popup's accessible name.
- **Result.** Spoken on focus: `Speech badge fixture Codex account Codex , Not added 列表框弹出 按钮 Agent default accounts
  Every session of an agent in this console uses its account here, unless a project or the session picks another.
  group`. Named account and agent state, no unnamed status.
- **Capture.** Last phrase through AppleScript after focusing this exact control.
- **Environment.** E1.
- **Date.** 2026-10-20.
- **Reuse scope.** Covers `aria-describedby` pointing at an `aria-hidden` sibling subtree holding a name and a
  `StatusBadge`, on a select trigger, in WKWebView. It does not cover a description supplied any other way. Note: with
  the editor already open, removing the agent changed the field's local presentation; reopening from persisted state
  restored the named selection. No defect was claimed or investigated from that draft behavior.

### NAME-4 Advanced tab

- **Location and state change.** Console settings, Advanced. A supported console reasoning effort of low stored while
  Codex was added; Codex removed; the tab stays because a saved model or effort remains.
- **Mechanism.** Tab name read on focus. `packages/ui/src/dialogs/ConsoleLaunchFields.tsx` (retains any agent with a
  non-empty saved model or effort; the tab list is labelled "Agent"; each tab holds a decorative icon, the agent name,
  `StatusBadge` and an indicator, with no explicit `aria-label`, `aria-labelledby`, `aria-describedby` or live region).
  This is not `packages/ui/src/components/AgentTabList.tsx`.
- **ARIA wiring as rendered.** Native tree: a radio-button role described as tab, titled `Codex , Not added`, selected,
  enabled, no live attributes; the parent is a tab group named "Agent"; the panel's title is also `Codex , Not added`.
- **Result.** Spoken on focus: `Codex , Not added 已选择 tab，第3项（共3项） Agent tab group`. Name and state together.
- **Capture.** Last phrase through AppleScript after focusing this exact tab. A first preparation that opened Advanced
  before the new account directory existed produced a model-list error and a stale suffix; it was corrected, and the
  later capture shows the effort offered normally. The first error is not evidence that the effort is unsupported.
- **Environment.** E1.
- **Date.** 2026-10-20.
- **Reuse scope.** Covers a tab built like those in `ConsoleLaunchFields` (name plus `StatusBadge` as its content, no
  explicit naming props). A tab list with another structure, or `AgentTabList`, does not match.

### NAME-5 Conflicted file heading includes its kind

- **Location and state change.** Open `merge.md` from the Changes pane of a throwaway repository with a real Git merge
  conflict: both branches modified the same line. Read the viewer's heading with VoiceOver.
- **Mechanism.** `packages/ui/src/viewer/FileViewer.tsx` (`FileViewer`) composes `titleName` from the basename, status
  and conflict kind in one `sr-only` string. Its drawn basename and `TitleStatusBadge` are `aria-hidden`; the shared
  `Dialog` uses this title for the heading and the dialog name. No live region announces the conflict kind separately.
- **ARIA wiring as rendered.** Native `AXHeading`, level 2, title `merge.md: Conflicted. Both modified.`; the viewer is
  an `AXApplicationDialog` with that title and description. The literal HTML naming attributes were not extracted.
- **Result.** Opening spoke `merge.md: Conflicted. Both modified. web dialog 包含7个项目 Close button`. Deliberate navigation
  to the heading spoke `标题级别2 merge.md: Conflicted. Both modified.`.
- **Capture.** AppleScript last phrase after opening and after VoiceOver cursor navigation; native accessibility tree
  for the heading and dialog.
- **Environment.** E6, packaged app with real daemon and throwaway Git data.
- **Date.** 2026-10-25.
- **Reuse scope.** Covers this single-string composition of the conflicted viewer heading, read on navigation, and its
  use as the opening dialog name. No other conflict kind, language, status combination or tooltip interaction was heard.

### SA-A Later change, no focus move

- **Location and state change.** The sidebar's project filter popover: with a project named "Status fixture Alpha" and
  the field "Project name contains" holding "Alpha", the project is renamed by the daemon to "Status fixture Beta" while
  the field keeps focus and value. The filter then matches nothing, and renaming back clears the status.
- **Mechanism.** A `StatusAnnouncer` mounted for the life of the projects section, empty until the change.
  `packages/ui/src/sidebar/Sidebar.tsx` (the no-match text, derived from projects existing and none matching the filter)
  renders it inside a `div` marked `data-live-announcer="true"`, which keeps the region exposed beside the modal filter
  popover; the visible empty panel is `silent`, so nothing is said twice. The announcer's default 100 ms first write
  delay had long elapsed, so this later change is written at once. No caller timer.
- **ARIA wiring as rendered.** The common `StatusAnnouncer` mapping. Before the change the region is empty; with the
  change it is populated; after the rename back it is empty again. The input is a search field named "Project name
  contains", focus unchanged in every sample.
- **Result.** Spoken automatically, once: `No matching projects`, about 0.15 s after the native text appeared. Clearing
  produced no speech. The trigger caused no focus speech.
- **Capture.** Last phrase through AppleScript, 25 ms polling, with native samples of focus and the region every 40 ms.
- **Environment.** E2.
- **Date.** 2026-10-20.
- **Reuse scope.** Covers a `StatusAnnouncer` that has been mounted empty for longer than its first write delay, whose
  text changes while focus rests, in a region that is not hidden by a modal at the time (a marker, or the modal's own
  subtree). The write was made while the window was attended; a write while it is not is not covered (see KNW-4). It
  does not measure a region mounted already holding its text, a write at a focus move or a text that clears within
  seconds.

### SA-B Outage with a per-appearance gate

- **Location and state change.** The connection banner: the control connection drops, retries are refused, retries are
  spent.
- **Mechanism.** A `StatusAnnouncer` mounted for the life of the banner, fed through the banner's own gate.
  `packages/ui/src/components/ConnectionBanner.tsx` (`ConnectionBanner`: `ANNOUNCEMENT_DELAY_MS`, 100 ms, withholds the
  text from the announcer for that long after the banner becomes visible, re-armed per outage; the announcer sits in an
  element marked `data-react-aria-top-layer`), and `packages/ui/src/components/StatusAnnouncer.tsx` (`StatusAnnouncer`,
  whose own first write delay applies once per mount). The visible strip text and icons are `aria-hidden`; no nested
  status or alert.
- **ARIA wiring as rendered.** The common mapping; the region is empty before the outage and populated unnamed during
  it. Focus stayed on an unrelated button in every sample.
- **Result.** Spoken automatically, once each: `Disconnected from the daemon. Reconnecting…` about 0.3 s after the
  socket was closed, then `Disconnected from the daemon.` when the retries were spent. The connected window produced no
  connection announcement. Cancellation before the delay and duplicate suppression are covered by component tests and
  browser inspection, not by listening. Recovery moved focus elsewhere, so the no-focus-move claim covers the outage
  announcements, not the whole recovery.
- **Capture.** Two records. 2026-10-14: VoiceOver's native caption window (caption verification), which showed the
  reconnecting message after the connection dropped and the disconnected message when retries were spent; build commit
  not recorded. 2026-10-20: last phrase through AppleScript, with the control socket closed through a temporary proxy in
  front of the isolated daemon, which stayed alive.
- **Environment.** 2026-10-14: macOS 26.2 (25C56), Simplified Chinese, dark appearance. 2026-10-20: E2.
- **Date.** 2026-10-14 and 2026-10-20.
- **Reuse scope.** Covers a mounted `StatusAnnouncer` whose text a caller withholds for a delay per appearance and then
  changes in place, in a top-layer-marked element, with no modal open and no focus move at the transition. The writes
  were made while the window was attended (a write while it is not is not covered, see KNW-4). The re-record had no
  modal open, so the top-layer marker's effect beside a modal was not heard; the marker counts as exposure only by the
  code's own rule, not by a listening result.

### SA-C Delayed first write after a dialog opens

- **Location and state change.** Settings, Agent accounts, a named account, Log in: the nested login dialog opens and
  moves focus to Cancel login; the text goes starting, then browser-waiting, then device-code waiting.
- **Mechanism.** A `StatusAnnouncer` mounted with the dialog, with `firstWriteDelayMs` set to
  `AFTER_FOCUS_MOVE_DELAY_MS` (1500 ms). `packages/ui/src/settings/AccountLoginDialog.tsx` (`AccountLoginDialog`: the
  visible spinner and the visible progress or code line are `aria-hidden`; the sibling announcer receives the plain
  translated text, the code inside isolation marks, and `undefined` while the login is not active), and
  `packages/ui/src/components/StatusAnnouncer.tsx`. The first text is written once after the delay; later changes are
  written at once. No nested status or alert; focus rests on Cancel login (`autoFocus`) throughout.
- **ARIA wiring as rendered.** The common mapping inside the modal web dialog named "Log in to Grok Build account
  VoiceOver fixture". The visible progress text is absent from the native tree because of `aria-hidden`; the hidden
  status text is present, so the text exists once.
- **Result.** Spoken automatically, once each: after the dialog's own opening phrase, `Logging in…` about 1.3 to 1.5 s
  later, then `Waiting for you to authorize in your browser…` and `Confirm this code in your browser: ⁨ABCD-1234⁩` (the
  code inside isolation marks, which are part of the phrase), each written at once when the text changed. The opening
  phrase was `Log in to Grok Build account VoiceOver fixture web dialog 包含4个项目 Cancel login button`. In E4 this held in
  three accepted runs (menu, direct button, minimized); closing the dialog before the first delay elapsed produced no
  progress phrase, as intended.
- **Capture.** Last phrase through AppleScript, 25 ms polling; native samples of the region corroborate each transition.
- **Environment.** E4 (first confirmed in E2 and E3, with the same dialog code).
- **Date.** 2026-10-20.
- **Reuse scope.** Covers a `StatusAnnouncer` mounted together with a dialog that moves focus as it opens, using
  `firstWriteDelayMs` of `AFTER_FOCUS_MOVE_DELAY_MS`, with later text changes at rest, and the visible form
  `aria-hidden`. It does not cover a shorter delay, a dialog that does not move focus, or text that is not present at
  the first render.

### SA-D Written after a dialog closes

- **Location and state change.** The fixture login succeeds; the dialog closes; the account row's hidden status says the
  account is logged in. Three paths: Log in from the account's Actions menu, Log in from the row's own direct button,
  and success while the window is minimized followed by return.
- **Mechanism.** A row-lifetime `StatusAnnouncer` written by a caller timer. `packages/ui/src/settings/AccountLogin.tsx`
  (`AccountLoginAnnouncer`: on a success that was not already there at mount it waits `AFTER_FOCUS_MOVE_DELAY_MS` and
  only while the window is attended, restarting the wait on each return, then gives the announcer the account-named
  text; `useClearSucceededLogin` dismisses the success after `SUCCESS_SHOWN_MS`, 6000 ms of attended time without a
  break), `packages/ui/src/components/useWindowAttended.ts` (`useWindowAttended`: document visible and window focused),
  `packages/ui/src/settings/AccountsTable.tsx` (`AccountTableRow` mounts the announcer and `useClearSucceededLogin`),
  and `StatusAnnouncer` with its default 100 ms already elapsed (the timer is the caller's, not `firstWriteDelayMs`).
  The dialog closes on success, hiding its own progress status.
- **ARIA wiring as rendered.** The common mapping; empty before success, populated with the account text in descendants
  after. The text is `⁨VoiceOver fixture⁩: logged in.` with isolation marks around the account name.
- **Result.** Spoken automatically, once, in all three paths. After the Settings and Actions-button focus phrase
  (`Settings web dialog 包含3个项目 Actions for ⁨VoiceOver fixture⁩ (Grok Build) More actions 菜单弹出 按钮 Grok Build tab panel`),
  `⁨VoiceOver fixture⁩: logged in.` followed about 1.5 s later. Direct button path: the same two phrases; a transient
  `Octoboard 网页内容` and a no-action button phrase may precede the final Actions phrase and are not a failure of the
  announcement. Away path: success while the window was minimized produced no speech while away; on the first return the
  announcement came about 1.5 s after the Actions focus phrase; a second minimize and return produced no second
  announcement, and the success cleared 6.0 s of attended time after the second return.
- **Capture.** Last phrase through AppleScript, 25 ms polling, with native focus and window state (screen unlocked,
  application frontmost, window minimized) sampled alongside; the daemon's success event timestamped independently.
- **Environment.** E4. The direct-button and menu paths were also heard in E3 and the menu path in E2.
- **Date.** 2026-10-20.
- **Reuse scope.** Covers a durable `StatusAnnouncer` in a row that is hidden by a modal while it is open, written by a
  caller timer that starts at the success (once the window is attended) and runs 1500 ms, the modal closing and focus
  returning about two frames after the success, so both come before the write, in WKWebView on macOS. Window states
  tested: attended, and minimized with the application otherwise frontmost. Not tested: a window visible but not
  frontmost, a different attention predicate, another agent, real login, other languages or appearances. A caller
  without the attention gate, or with a write at the close, is a known-not-to-work pattern below.
- **Possibly stale.** The three paths were heard on an account row that has since changed: the accounts are now a table
  (`AccountTableRow`) whose Status cell holds the Log in button (`packages/ui/src/settings/AccountLoginButton.tsx`),
  which stays focusable but `aria-disabled` while a login runs, and the account details dialog
  (`packages/ui/src/settings/AccountDetailsDialog.tsx`) mounts a second `AccountLoginAnnouncer`. The announcer's wait
  and attention gate read the same in source, but the row layout, the opener and the focus phrase before the write were
  not heard again, so the recorded paths may no longer describe the product.

### SA-E Success already present at mount

- **Location and state change.** After a success, Settings General then Agent accounts then the Claude Code tab then the
  Grok Build tab are shown within six seconds, remounting the account row.
- **Mechanism.** `AccountLoginAnnouncer` in `packages/ui/src/settings/AccountLogin.tsx` initializes from the success
  present when the row mounts and does not write it; it clears that hold when the state next leaves success.
- **ARIA wiring as rendered.** The common mapping, empty after the remount.
- **Result.** Not spoken, as intended: only the tab-selection phrases were captured; no second logged-in phrase through
  10 s or more after the success. The original announcement count stayed one.
- **Capture.** Last phrase through AppleScript, with native samples of the empty region.
- **Environment.** E4 (and E2, E3 with an earlier form of the same suppression).
- **Date.** 2026-10-20.
- **Reuse scope.** Covers suppressing a state that is already present when the announcer's row mounts. It is evidence
  for silence on remount only alongside SA-D's positive result.
- **Possibly stale.** Heard on the earlier account row layout (see SA-D); the suppression in
  `packages/ui/src/settings/AccountLogin.tsx` is unchanged in source but was not heard on the table row.

### SA-F Hidden status text stays navigable

- **Location and state change.** The login dialog after the device code was announced; the VoiceOver cursor moves left
  from Cancel login.
- **Mechanism.** The visible code line is `aria-hidden`; the plain code text lives in the dialog's `StatusAnnouncer`
  (see SA-C). The text of the status stays in the accessibility tree after its announcement.
- **Result.** Readable manually: the second move left read `Confirm this code in your browser: ⁨ABCD-1234⁩`, and the
  text under the VoiceOver cursor was exactly that. The first move left read `link Open the page again Opens in your
  browser`, which was not activated. This reread was a deliberate manual read about 3 s after the automatic
  announcement, not a duplicate announcement.
- **Capture.** VoiceOver cursor commands and last phrase through AppleScript.
- **Environment.** E2.
- **Date.** 2026-10-20.
- **Reuse scope.** Covers hiding a visible line with `aria-hidden` and keeping its plain text in the `StatusAnnouncer`
  beside it: the text remains reachable by cursor.

### SA-G Confirmation that clears with its tooltip, focus retained

- **Location and state change.** Changes, a linked worktree's notice with the button "Copy worktree path": pressing it
  copies the path, the button's name and tooltip change to "Copied", and the confirmation clears when the tooltip
  closes.
- **Mechanism.** A `StatusAnnouncer` with `element="span"`, mounted empty with the button.
  `packages/ui/src/components/CopyButton.tsx` (`CopyButton`: the confirmation text defaults to "Copied"; the clear is
  driven by the tooltip lifecycle, `COPIED_HOLD_MS` of 2000 ms applying only while no tooltip is open, and 150 ms after
  the tooltip closes), reached through `packages/ui/src/browser/GitView.tsx` (`Notice`). The default 100 ms first write
  delay had long elapsed, so the write is immediate. Main window, no modal; the check icon is `aria-hidden`.
- **ARIA wiring as rendered.** The common mapping; the same button is natively a button named "Copy worktree path", then
  "Copied" while the status is populated.
- **Result.** Spoken automatically, once: `Copied`. Focus stayed on the button. With the tooltip held open the text did
  not clear within the 9 s capture. Dismissing the tooltip with Escape cleared the status and restored the button's name
  with no other focus destination and no cleared-state phrase. The button's own name change and the status both say
  "Copied", so last-phrase capture does not tell which source produced the speech.
- **Capture.** Last phrase through AppleScript, 25 ms polling, native focus and status samples about every 40 ms. The
  automatic 2000 ms branch (no tooltip open) was not obtained: accessibility and keyboard presses kept the tooltip open.
  No failure path was tested. The run changed the pasteboard; the original contents were overwritten by one press during
  preparation and could not be restored, so clipboard restoration is not claimed.
- **Environment.** E4.
- **Date.** 2026-10-20.
- **Reuse scope.** Covers a durable `StatusAnnouncer` span past its first write delay, written when its control is
  pressed while focus stays on that control, in the main window, and cleared by a tooltip lifecycle without a focus
  move. Not covered: the 2000 ms automatic clear, a repeat press while copied, a failure, or a control whose name does
  not change to the same text (the evidence cannot separate the status from the renamed control). The press was made
  while the button had focus; the viewer's copy button sets `preventFocusOnPress` and is not covered by that relation.

### SA-H First mount at the default delay, no focus move

- **Location and state change.** Hiding and re-showing the Files panel, with focus left on the panel toggle, mounts the
  Changes view again: it loads (`Loading changes…`) and the loading clears.
- **Mechanism.** A `StatusAnnouncer` mounted with the Changes view, its text present from the first render and written
  after the default 100 ms first write delay. `packages/ui/src/browser/GitView.tsx` (`GitView`; the card and its content
  are recreated by the panel stack when the panel is shown). The visible loading placeholder is `aria-hidden`. Main
  window, no modal; focus stays on the toggle, which is renamed from "Show files panel" to "Hide files panel".
- **ARIA wiring as rendered.** The common mapping; an empty region appeared with the toggle focused, then was populated
  about 100 ms later, then cleared about 0.2 s after that.
- **Result.** Spoken automatically, once, after the toggle's own phrase (`Hide files panel 已展开 button`): `Loading
  changes…`. A second variant, opening the Changes tab (a focus move into the pane), showed the native loading text but
  captured no loading sentence over the selected-tab speech; that is not a failure of a written region and is not
  claimed either way. A Files panel initial load completed too quickly to sample a populated status.
- **Capture.** Last phrase through AppleScript and native samples.
- **Environment.** E4.
- **Date.** 2026-10-20.
- **Reuse scope.** Covers a `StatusAnnouncer` mounted with its text at the default delay in the main window while focus
  stays on the control that caused it, with the visible form `aria-hidden`. Not covered: a replacement sequence of
  several loading texts, a mount that coincides with a focus move into the pane, or a mount under a modal.

### SA-I Default-delay first mount while a menu takes focus

- **Location and state change.** The project menu, Git, Switch branch: the submenu opens and takes focus while its
  branch list loads.
- **Mechanism.** A `StatusAnnouncer` mounted with the submenu items, holding "Loading…" at the default 100 ms delay.
  `packages/ui/src/components/ActionMenu.tsx` (`LoadedMenuItems`); the caller is the project menu's async branch list in
  `packages/ui/src/sidebar/menus.tsx`. The visible disabled "Loading…" menu item is not `aria-hidden`. The popover
  contains the region, so it is exposed. The git request was stretched to 2 s by a clone-only wrapper to make the load
  observable; this is not natural latency.
- **ARIA wiring as rendered.** The common mapping; a populated status with the submenu named "Switch branch" focused.
- **Result.** Spoken: `Switch branch 已展开 子菜单 （3/5） menuGit`, then `Loading…` about 0.5 s later, then the status cleared
  at the end of the 2 s hold; focus stayed in the submenu. Because the visible disabled item says the same word,
  last-phrase capture does not uniquely attribute the speech to the hidden status; both sources are in the tree. Without
  the wrapper the load finished too fast to observe a populated status, and a load failure was masked by an error alert,
  so the later "could not load" update is not recorded.
- **Capture.** Last phrase through AppleScript and native samples.
- **Environment.** E4.
- **Date.** 2026-10-20.
- **Reuse scope.** Covers `LoadedMenuItems`' initial loading text at the default delay while the menu takes focus. Not
  covered: the load-failed update, a case where the visible item is absent or `aria-hidden`.

### SA-J New child at 1500 ms inside an open dialog, focus retained

- **Location and state change.** In the open file viewer, pressing Next file moves from one unchanged staged rename to
  the next. The content view says `Loading…` at once; the new unchanged view says its outcome 1500 ms later.
- **Mechanism.** Two `StatusAnnouncer`s. `packages/ui/src/viewer/FileViewer.tsx` (`ViewerContentView`: mounted with the
  dialog, its 1500 ms first delay long elapsed, so the navigation text is written at once) and (`UnchangedView`: mounted
  with the new child, `firstWriteDelayMs` of 1500). The visible final notice is `aria-hidden`. The Next file button
  keeps focus; no dialog opens.
- **ARIA wiring as rendered.** The common mapping for both regions; focus on the Next file button at every sample.
- **Result.** Spoken automatically: `Loading…`, then `Renamed without changes to its content.` about 1.5 s after the new
  child started. The same sentence also arrived just before, for the previous file, which is not a duplicate of the
  second file's outcome. A variant in which Previous became unavailable moved focus to Next file; it is a separate
  navigation-focus variant and not the retained-focus representative.
- **Capture.** Last phrase through AppleScript and native samples.
- **Environment.** E4.
- **Date.** 2026-10-20.
- **Reuse scope.** Covers a `StatusAnnouncer` with a 1500 ms first delay mounted with a new child inside a dialog that
  is already open, with focus unmoved, and the parent's immediate navigation text. Other read outcomes and the failure
  outcome were not exercised. It is a candidate for the same-shaped notice branches only when their lifecycle and
  exposure agree.

### SA-K Loading written, then the pressed control is removed

- **Location and state change.** The viewer's diff for a 180-line file with one modified line: pressing "Show all 86
  unmodified lines" expands the collapsed lines.
- **Mechanism.** A `StatusAnnouncer` that stays mounted in the diff view, default delay long elapsed.
  `packages/ui/src/viewer/CodeSurface.tsx` (`DiffSurface`: the expansion callback sets the loading text at once; the
  visible notice, when there is one, is `aria-hidden`). Inside the viewer dialog.
- **ARIA wiring as rendered.** The common mapping; the button was focused when the status was written.
- **Result.** The status is spoken automatically: `Loading unmodified lines…`, in E4 and again in E5. What follows
  differs by build. In E4 the expansion button was removed and native focus fell to the web area; the next phrase was
  `Octoboard 网页内容` (KNW-8). In E5 focus is handed to the surviving control or the code frame (FOCUS-2), and the
  destination's phrase replaces the loading phrase 365 ms after it, so last-phrase polling cannot show how much of the
  loading phrase was spoken. No outcome-note sentence occurred in these successful expansions; the failure and capped
  outcomes were not exercised.
- **Capture.** Last phrase through AppleScript and native samples.
- **Environment.** E4 and E5.
- **Date.** 2026-10-20.
- **Reuse scope.** Covers the loading write of an existing `StatusAnnouncer` while the pressed control has focus. The
  focus move follows within about 0.25 s, inside the roughly 300 ms window in which a focus move can replace a status,
  which is the relation recorded; a caller matches it only with this focus relation and, for the focus destination, with
  FOCUS-2. It covers no outcome note.

### SA-L Section status written after an attended wait, tab focus retained

- **Location and state change.** Inside open Settings, the Permissions tab is selected in place of General, and its
  section replaces the General one. The section says where the two macOS permissions stand (neither granted, in the
  clone's own permission state, left unchanged).
- **Mechanism.** A section-level `StatusAnnouncer`, mounted with the section and writing every text, the first included,
  after a caller wait. `packages/ui/src/settings/PermissionsSection.tsx` (`PermissionsSection`: the rows show each state
  as ordinary visible copy, not `aria-hidden` and with no `role="status"`; `PermissionsAnnouncer`: gives the announcer
  the permission names with their states, one sentence, after `AFTER_FOCUS_MOVE_DELAY_MS` of attended time, restarting
  the wait on each return to the window, and writes nothing again while the text has not changed),
  `packages/ui/src/components/useWindowAttended.ts` and `StatusAnnouncer` with its default 100 ms long elapsed (the wait
  is the caller's, not `firstWriteDelayMs`). Inside Settings' own dialog; the Permissions tab keeps focus.
- **ARIA wiring as rendered.** The common mapping; empty when the tab was focused, populated 1.5 s later with the
  sentence in a descendant. A single status holds both permissions; the visible row copy stays separately readable, so
  the states are met twice by a reader going through the rows.
- **Result.** Spoken automatically, once: after the tab phrase (`Permissions 已选择 tab，第5项（共7项）`), `Screen Recording Not
  granted. Accessibility Not granted.` followed 1.73 s later; the native status was written 1.50 s after native focus on
  the tab. Returning from a minimized window with the states unchanged produced the window phrase and the tab phrase and
  no repeat of the sentence in 16 s. No permission was granted, refused or changed.
- **Capture.** Last phrase through AppleScript, 25 ms polling, with native focus, status and window samples about every
  40 ms; the minimized window was restored with no other application activated.
- **Environment.** E5.
- **Date.** 2026-10-20.
- **Reuse scope.** Covers a section-level `StatusAnnouncer` fed by a caller that waits `AFTER_FOCUS_MOVE_DELAY_MS` of
  attended time, in a section shown by a tab focus move inside an already-open dialog, with the durable copy left
  readable. Not heard: a change of the states while Settings is open (a grant made in System Settings), a first reading
  that arrives while the section is already showing, not at the tab focus move, and Settings opened directly onto
  Permissions, which no production control does today, so it is untested rather than failing. The rows' own copy was not
  tested as a reading path. `AttendedStatusAnnouncer` (`packages/ui/src/components/AttendedStatusAnnouncer.tsx`) carries
  the same attended wait in a component of its own (`PermissionsAnnouncer` is a separate copy); that is not a listening
  result of its own.


### SA-M Region mounted with each opening of a disclosure

- **Location and state change.** The launch dialog's Advanced section with Grok Build chosen (a credential-free CLI
  fixture whose model listing fails after 3 s): expanding the section starts the model request and its failure is
  announced; collapsing and reopening the section keeps the answer and requests again.
- **Mechanism.** A `StatusAnnouncer` mounted outside the folded body and only while the section is expanded.
  `packages/ui/src/dialogs/LaunchSettingsFields.tsx` (`AdvancedSection`) with
  `packages/ui/src/dialogs/useAgentModels.ts` (keeps the earlier answer and asks again on each expansion). Every opening
  gets a fresh region and a fresh default 100 ms first write delay, even with the earlier failure text kept. The trigger
  keeps focus; inside the launch dialog.
- **ARIA wiring as rendered.** The common mapping; a new empty region appears on each opening and is populated shortly
  after. It sits outside the collapsed content.
- **Result.** Spoken automatically, once per opening. First opening: `Advanced 已展开 button`, then `Loading the models
  this agent offers…`, then the failure sentence `⁨Grok Build⁩ could not list its models: ⁨exit status: 1: Fixture has
  no models⁩` (isolation marks are part of the phrase) when the fixture failed 3 s later. Two collapse-and-reopen runs,
  each with the kept failure: the same sentence 465 ms and 95 ms after the expanded-button phrase, once each, with no
  duplicate when the repeated request returned the same error. The differing intervals come from native sampling and
  speech latency, not from the 100 ms delay.
- **Capture.** Last phrase through AppleScript, 25 ms polling; native samples of focus (the Advanced button throughout)
  and of the new region.
- **Environment.** E5.
- **Date.** 2026-10-20.
- **Reuse scope.** Covers a `StatusAnnouncer` mounted per opening outside a disclosure's folded body, with the trigger
  focused, whether the text is fresh or kept from an earlier opening. Not covered: a region inside the folded body (see
  KNW-6), a fresh write of the unavailable text.

### SA-N Viewer notice after paint, then an empty status

- **Location and state change.** In the real file viewer, navigate from a large text file to `broken.svg`, an SVG that
  cannot decode and falls back to its text. Separately, navigate back to `large.txt`, now above the highlighting budget,
  whose plain-text notice is present with its first content paint.
- **Mechanism.** `packages/ui/src/viewer/FileViewer.tsx` (`ImageView`) raises a failed `FileNotice` with `speak` when
  image decoding fails. `packages/ui/src/viewer/notices.tsx` (`FileNotice`, `SpokenSentence`) portals the mark to the
  header and mounts a `StatusAnnouncer` beside the content, inside the open viewer dialog. The region starts empty, has
  `firstWriteDelayMs` of 1500 and `clearAfterRead`; source `READ_TIME_MS` is 5000 after writing.
  `packages/ui/src/viewer/CodeSurface.tsx` (`CodeSurface`) supplies no `speak` for the large-file notice, so that branch
  renders a mark without an announcement region. These lifecycle facts are source wiring, not a timing measurement of
  every intermediate render.
- **ARIA wiring as rendered.** About 2 s after navigation to the SVG, an unnamed native `AXGroup` with subrole
  `AXApplicationStatus` held a descendant static text with the notice sentence. At the sample about 7 s after
  navigation, the sentence was gone from that region; the header's `AXButton`, named by the same sentence, remained. The
  large-file sample had the named mark without a populated status for that sentence. Live politeness, atomicity and
  relevance were not sampled in E6, so the full common native mapping is not claimed for this run.
- **Result.** The SVG sentence was spoken automatically once: `This image cannot be displayed, so its text is shown
  instead.`. Focus was on Next file when the status was sampled; the navigation began with Previous file, which became
  unavailable and handed focus to Next file. After the clear, deliberate reading met the sentence at the mark, then
  these phrases, with no second hidden copy of the sentence:

  ```text
  Contents of ⁨broken.svg⁩ region
  1
  <svg xmlns="http://www.w3.org/2000/svg"><broken
  的结尾 Contents of ⁨broken.svg⁩ region
  ```

  The first-paint large-file sentence `Shown without syntax highlighting because the file is large.` was not spoken
  automatically during the observation. The navigation phrase was `Next file button`; the named mark was still available
  for deliberate reading, as recorded in FOCUS-3. This is a negative observation for that first-paint branch, not
  evidence that the mark is inaccessible.
- **Capture.** AppleScript last-phrase changes polled at a requested 25 ms interval, native samples at about 2 s and 7
  s, then VoiceOver cursor navigation after the status cleared. These sparse native samples establish presence and later
  absence, not an exact 5 s measured lifetime. No sample before the write was recorded, so the corroboration for "once"
  is presence at a later sample rather than an observed transition, and "once" has the last-phrase limitation under
  "Evidence and its limits".
- **Environment.** E6, packaged app with real daemon and throwaway files.
- **Date.** 2026-10-25.
- **Reuse scope.** Covers the later `FileNotice` SVG fallback with this focus hand-off relation, its clear and
  subsequent reading path. The first-paint large-file notice, which has no announcement region (`CodeSurface` passes no
  `speak`), stayed silent; that is context, not a covered write. SA-J remains the retained-focus case; it does not by
  itself establish this clear. Other image failures, patch fallbacks and notice replacements were not heard.
  `SpotNotice` has not been listened to; the shared `SpokenSentence` is a candidate only when all matching conditions
  hold. No general match is claimed for callers whose native live attributes were not captured here.

### FOCUS-1 Focus after the login dialog closes

- **Location and state change.** A dialog opened from the row's own Log in button closes, by success or by cancelling.
  The button is shown only while the account reads as signed out and is disabled while a login runs; the menu path
  opens from a menu item instead.
- **Mechanism.** `useLoginFocusReturn` in `packages/ui/src/settings/AccountLoginButton.tsx` (`AccountLoginButton`),
  driven by the button's `onPress`; `AccountTableRow` passes the row's actions button as the fallback. After success
  focus goes to the row's actions button. After a cancel it goes to the Log in button once the daemon has answered and
  the button is available again, otherwise to the actions button. The move waits one animation frame past React Aria's
  restore (two after the unmount), acts only when focus is on `<body>` or the original button, sets the keyboard
  modality, and leaves a restore that landed correctly alone. The menu path supplies no hand-off, and React Aria returns
  focus to the menu's trigger.
- **ARIA wiring as rendered.** The row's Actions control is a pop-up button named `Actions for ⁨VoiceOver fixture⁩ (Grok
  Build)` with has-popup set; the direct button is a button named "Log in". Final screenshots show the keyboard focus
  ring on the correct control.
- **Result.** Success, direct button: focus on the Actions button, then the SA-D announcement. Cancel after the login
  had started: focus on the Log in button (`Settings web dialog 包含3个项目 Log in button Grok Build tab panel`), enabled and
  focused for 9 s or more. Cancel before the daemon had registered the login (the start request delayed 3.5 s by a
  temporary proxy): focus on the Log in button at once, and it stayed there after the late start and cancel. Observed
  and accepted: in the cancel-after-start run a generic `Octoboard 网页内容` phrase was spoken about 99 ms before the
  correct button phrase, so a transient web-area utterance is not excluded; the correct target is spoken immediately
  after. Intermediate disabled states shorter than the sample resolution were not observed.
- **Capture.** Last phrase through AppleScript and native focus samples at 40 ms.
- **Environment.** E4. E3 recorded the earlier cancel failure (KNW-5) and E2 the lost focus after a success (KNW-3).
- **Date.** 2026-10-20.
- **Reuse scope.** Covers the focus destination for a dialog whose opener may be disabled or gone when it closes, as
  `useLoginFocusReturn` does it. Not tested: a window visible but not frontmost, a different opener.
- **Possibly stale.** Heard on the earlier account row, where the Log in button was natively disabled while a login ran.
  The button now stays focusable and is `aria-disabled` in that state, sits in the table's Status cell and is also
  offered on the account details dialog, and the hand-off acts when focus is on `<body>` or on a button that is going.
  The recorded phrases and the disabled-opener relation were not heard again on this layout.

### FOCUS-2 Focus after an expansion takes the pressed control away

- **Location and state change.** The viewer's diff of a 180-line file with one modified line: "Show all 86 unmodified
  lines" is pressed by an accessibility press (VoiceOver presses with a click), then the remaining "Show whole file".
- **Mechanism.** `packages/ui/src/viewer/expansion.ts` (`Expansion.activate`: told whether the pressed control had
  focus, decided in the click handler from the focused element and the event path; a mouse press takes no focus), with
  `packages/ui/src/viewer/rendererDom.ts` (`refocusControl`, `focusControl`, `handFocusToCode`: the redraw restores the
  same or the nearest surviving control, and with none left hands focus to the named code frame with the keyboard
  modality) and `packages/ui/src/viewer/CodeSurface.tsx` (`DiffSurface`: the loading status, see SA-K).
- **ARIA wiring as rendered.** The destination separator button is a button named "Show whole file". The code-frame
  fallback is a region named "Contents of expand.txt"; it is not a live region.
- **Result.** Three paths, all pass. First, an accessibility press on the focused "Show all 86 unmodified lines": native
  focus moved to the surviving "Show whole file" button 0.23 s after the loading status appeared, with the focus ring
  visible in the window screenshot. Spoken: first, as focus landed on the control before the press, `Show all 86
  unmodified lines button Contents of ⁨expand.txt⁩ region`; then `Loading unmodified lines…` and, 365 ms later, `Show
  whole file button` (isolation marks are part of the raw phrase). No `Octoboard 网页内容` was captured and native focus
  never fell to the web area. The loading phrase was replaced by the destination's speech that quickly; last-phrase
  polling cannot show how much of it was spoken. Second, an accessibility press on "Show whole file" with the bodies
  cached (no loading phase): focus went to the named code frame (`Contents of ⁨expand.txt⁩ region`), with the ring
  around the frame and every line revealed, and no web-area phrase. Third, a pointer press on a separator while another
  control (Close) had focus: the loading status was spoken (`Loading unmodified lines…`), the expansion was delivered,
  and focus stayed on Close, as intended, with no web-area phrase.
- **Capture.** Last phrase through AppleScript, 25 ms polling; native focus samples; window-only screenshots. The
  pointer press used a helper that posts only to the confirmation build's process and window.
- **Environment.** E5.
- **Date.** 2026-10-20.
- **Reuse scope.** Covers the focus destination after an expansion removes the pressed control, for an accessibility
  press and a pointer press, as `Expansion.activate` and `handFocusToCode` do it. Not covered: error and capped
  outcomes, other layouts, and scroll permutations of an off-screen separator.

### FOCUS-3 Status mark and quota popover focus and reading paths

- **Location and state change.** In E6, the large-file notice mark after the viewer's path, the failed SVG notice mark,
  and the rail's Account quota button with no agent available. Rest the real mouse on the mark or quota button;
  separately open with Enter and close with Escape. The viewer's outer dialog is already open for the mark checks; quota
  is in the main window.
- **Mechanism.** `packages/ui/src/components/StatusMark.tsx` (`StatusMark`, `PopoverMark`, `MarkButton`) and
  `packages/ui/src/quota/RailQuotaEntry.tsx` (`RailQuotaEntry`, `QuotaPanel`) share
  `packages/ui/src/components/hintPopover.tsx` (`PopoverHint`, `useHoverPopover`). E6 source wiring opens hover
  non-modally without a focus move, using a focusable `div` with `role="dialog"`, `aria-modal="false"` and
  `tabIndex={-1}`. A keyboard press mounts react-aria's modal `Dialog` and its focus scope; quota explicitly focuses the
  first control when there is no agent tab. Closing the pressed form restores the surviving trigger. The mark's button
  uses `aria-label`; a bare mark's dialog uses `aria-labelledby` to its one sentence. Quota's dialog uses its own label.
  Glyphs are `aria-hidden`; this is on-demand reading, not a status announcement.
- **ARIA wiring as rendered.** Hover: native focus stayed on Next file for the viewer mark and on Settings for quota
  while the floating content appeared. Both panels had `AXApplicationDialog` subroles. The mark panel's native
  description was its sentence; quota's title and description were `Account quota`. Enter on the SVG mark focused its
  dialog group; Enter on quota focused Manage accounts inside the panel. The native mark button title and description
  were its sentence. These are native names and focus samples, not an extraction of the literal HTML attributes or every
  expanded-state attribute.
- **Result, hover.** No focus move on opening. Sequential VoiceOver navigation did not reach either hover panel in the
  attempted walks. The successful route was VoiceOver's item chooser, followed by entering the selected panel or reading
  its content. The large-file mark panel was listed as `Shown without syntax highlighting because the file is large. web
  dialog`; within it the text read `Shown without syntax highlighting because the file is large.`. Quota's selected
  heading read `标题级别2 No agent available`, then `An agent's quota is shown once its command is found on your PATH .`.
  Thus content was reachable through the item chooser; this record does not call sequential navigation a pass or explain
  why it did not reach the panels. A bare mark's hover panel is now also named by its tone, so the mark listing above
  is not the current wiring, and the chooser route for tone-named panels was not heard. The hover form is the mouse's
  and the keyboard and assistive technology open the press form (`docs/ui-accessibility.md`, "A floating panel that
  does not take the window"), so sequential navigation not reaching a hover-opened panel is the designed behaviour, not
  an open defect.
- **Result, Enter.** **Fail for the E6 bare mark's naming (recorded as KNW-9, which holds the exact phrase):** the SVG
  mark focused its inner dialog and repeated its sentence, once as the dialog's name and once as its body, in one
  recorded phrase rather than two sampled automatic status writes. Naming the dialog by its own text caused the double
  read; the current wiring names the dialog with the tone word instead, and FOCUS-4 records the check of that naming.
  Quota focused its first control and spoke `Manage accounts… button Account quota web dialog`. Deliberate reading
  inside spoke `An agent's quota is shown once its command is found on your PATH .`, `标题级别2 No agent available`, then
  `Account quota web dialog` when moving back out.
- **Result, Escape.** After closing the SVG mark's pressed panel, focus returned to its button and spoke:

  ```text
  broken.svg web dialog 包含8个项目 This image cannot be displayed, so its text is shown instead. 已折叠 button
  ```

  Quota returned to its button and spoke `Account quota 已折叠 button Consoles and window controls navigation`.
- **Capture.** AppleScript last phrase after native focus actions, Enter, Escape, VoiceOver cursor navigation and item
  chooser selection; native trees before and after opening for focus and effective roles. Process-targeted key injection
  did not reliably exercise Enter, so the successful Enter and Escape observations used System Events after activating
  the verification app. Hover used `cliclick` and the real pointer; the input and permission exception is recorded under
  E6.
- **Environment.** E6, packaged app; quota's empty state, not a real account or quota probe.
- **Date.** 2026-10-25.
- **Reuse scope.** Records the E6 focus and reading paths of these two `PopoverHint` callers, including a nested viewer
  mark, with an existing trigger to return to. Passing evidence: quota's Enter and Escape focus paths, the mark's focus
  return on Escape, and the item-chooser reachability of the hover content as wired in E6 (not as currently
  wired). The bare mark's Enter speech is a failure (KNW-9), not a passing naming pattern; FOCUS-4 records its
  replacement. Hover reachability establishes the item-chooser route only. It does not cover a populated quota panel, a
  Files-tree failure mark, a removed trigger, touch, mouse clicks, hybrid devices or Escape delivery to an agent or
  shell terminal.

### FOCUS-4 Bare mark dialog named by its tone

- **Location and state change.** In the packaged file viewer, open `large.txt`, above the syntax-highlighting budget,
  and press Enter on its bare Note mark. Close with Escape. Navigate to a malformed `broken.svg` that falls back to its
  text, let the temporary fallback announcement end, then press Enter on its bare Error mark and close with Escape.
  These are the two representative tones heard; no warning or touch interaction was exercised.
- **Mechanism.** `packages/ui/src/components/StatusMark.tsx` (`PopoverMark`) gives a bare mark's `Hint` the
  localized tone label, while `MarkButton` retains the notice sentence as its `aria-label`.
  `packages/ui/src/components/hintPopover.tsx` (`PopoverHint`) names its pressed react-aria `Dialog` with `aria-label`,
  independently of the body text, and moves focus inside. Escape restores the existing trigger.
  `packages/ui/src/viewer/notices.tsx` (`FileNotice`) supplies the whole-file mark after the path.
  `packages/ui/src/viewer/FileViewer.tsx` (`ImageView`) and `packages/ui/src/viewer/CodeSurface.tsx` (`CodeSurface`)
  supplied the SVG fallback and large-file notice. This replaces the E6 body-text naming that failed in FOCUS-3; it is
  on-demand dialog speech, not a live-region repetition test.
- **ARIA wiring as rendered.** For each Enter, the focused native `AXGroup` had subrole `AXApplicationDialog`, role
  description `web dialog`, and title/description `Note` or `Error`. Its one static-text child held the sentence once.
  Its native live value was empty, atomic value `0` and relevant value `additions text`. After Escape, the original
  `AXButton` was focused and still had the sentence as both title and description. The literal HTML and its generated
  attributes were not extracted; the `aria-label` mechanism above comes from source, while the native names, text and
  focus were sampled.
- **Result, Enter.** Pass for both recorded marks. The sentence appeared once in each returned phrase, followed by the
  tone and role. VoiceOver's actual order here was body text before the tone name:

  ```text
  Shown without syntax highlighting because the file is large. Note web dialog
  This image cannot be displayed, so its text is shown instead. Error web dialog
  ```

  Deliberate focus on the trigger immediately before Enter separately read its sentence and `已折叠 button`; those trigger
  reads were not counted as repeated text within the dialog-opening phrase. No new automatic fallback announcement was
  attributed to Enter: the SVG's earlier temporary status was allowed to end first.
- **Result, Escape.** Pass for both focus returns, corroborated by the focused native trigger. Note spoke `Shown without
  syntax highlighting because the file is large. 已折叠 button`. Error spoke:

  ```text
  broken.svg web dialog 包含8个项目 This image cannot be displayed, so its text is shown instead. 已折叠 button
  ```

- **Capture.** AppleScript `content of last phrase` after each action, a background poll requested at 25 ms, and native
  trees after Enter and Escape. Setup and focus used process-targeted AX actions; Enter/Escape used System Events only
  after checking the isolated app's frontmost PID. The authorization for the shortcut and global input is stated
  under E8. No synthetic VoiceOver `output` was used.
- **Environment.** E8, real daemon and throwaway files in the isolated packaged app; no agent CLI, Safari or gallery.
- **Date.** 2026-10-25.
- **Reuse scope.** Covers Enter and Escape on these bare Error and Note viewer-header marks with their existing
  triggers, with the marks inside the open viewer dialog; marks in the main window (the Files tree, the Git view) are
  not covered. It replaces the E6 duplicated-sentence naming result, without erasing it. It does not establish a pass
  for Warning, the restricted-change rich path line, marks with their own custom bodies, hover navigation, quota,
  removed triggers or another language. Touch VoiceOver on iOS/iPadOS and the touch wrapper's generic `Hint` naming were
  not tested; no desktop simulation is claimed.

### PLAIN-1 Text change in a mounted plain status

- **Location and state change.** The first form of the login dialog: starting to browser-waiting after the URL, then to
  device-code waiting after the code.
- **Mechanism.** Text replaced in place in a `role="status"` div mounted with the dialog (E1), with no
  `StatusAnnouncer`, no delay and no nested alert; focus rested on Cancel login.
  `packages/ui/src/settings/AccountLoginDialog.tsx` in that build; the code chip rendered through
  `packages/ui/src/components/InlineReference.tsx`. Superseded in the product by SA-C's mechanism; the result remains
  evidence for this kind.
- **ARIA wiring as rendered.** The region held a spinner (`aria-hidden`) and a text span. Natively a group with the
  application-status subrole, empty own name, live polite, atomic, relevant additions and text, with the text in
  descendants.
- **Result.** Spoken automatically, once each: `Waiting for you to authorize in your browser…` and `Confirm this code in
  your browser:  ABCD-1234` (two spaces before the code in the native utterance). No duplicate was observed.
- **Capture.** Last phrase through AppleScript over two completed runs, one with a 40 ms poll.
- **Environment.** E1.
- **Date.** 2026-10-20.
- **Reuse scope.** Covers a text change in a `role="status"` that has been mounted since the dialog's mount, in a modal
  whose focus rests on a footer button. It does not cover the first text (see KNW-1).

### PLAIN-2 Persistent visible status with an action

- **Location and state change.** A plain shell terminal's process exits (Ctrl-D): the terminal shows a status with a
  Close terminal button.
- **Mechanism.** A hand-built `role="status"` `div` that persists empty until the terminal is shown and not running,
  with no first write delay. `packages/ui/src/terminal/ShellTerminalView.tsx` (`ShellTerminalView`: the visible lower
  status holds the sentence and the Close terminal button). Main window, no modal; terminal input keeps focus. The
  separate `sr-only` trouble status stayed empty.
- **ARIA wiring as rendered.** Natively a group with the application-status subrole, atomic, with the button inside the
  status as a descendant (the full tree shows it; the compact observer lists only static text).
- **Result.** Spoken automatically, once: `The shell has exited Close terminal`.
- **Capture.** Last phrase through AppleScript and native samples. An earlier run typed the keys but did not exit the
  shell and was excluded.
- **Environment.** E4.
- **Date.** 2026-10-20.
- **Reuse scope.** Covers a persistent, visible `role="status"` that fills in place with text and a button, without a
  focus move, in the main window. The recorded mechanism has since been replaced: `ShellTerminalView` now speaks the
  sentence from a `StatusAnnouncer` and the button sits outside the region, so this result stays as history and
  matches the current code only as a candidate (see the caller table).

### ALERT-1 A conflicted file read fails

- **Location and state change.** After reading the real `merge.md` conflict in NAME-5, close the viewer, remove that
  throwaway file from disk without resolving its index conflict, and reopen its row in Changes.
- **Mechanism.** `packages/ui/src/viewer/FileViewer.tsx` (`ViewerContentBody`, conflict branch) renders only the read
  `Failure` when no conflict body is available. `packages/ui/src/viewer/viewerStates.tsx` (`Failure`) passes the
  first-paint delay and announcement flag to `packages/ui/src/components/PaneFailure.tsx` (`PaneFailure`), then
  `packages/ui/src/components/EmptyPanel.tsx` (`EmptyPanel`) and `packages/ui/src/components/useAnnouncement.ts`
  (`useAnnouncement`). After the viewer delay, the heading and reason are in a keyed `role="alert"` wrapper. The
  conflict kind remains in the viewer heading; no separate conflict notice announces it.
- **ARIA wiring as rendered.** Native `AXGroup` with subrole `AXApplicationAlert`, containing the level-2 heading `Could
  not open this file` and the static reason `This file does not exist any more.`. The viewer dialog still had the name
  `merge.md: Conflicted. Both modified.`. The native tree did not sample live politeness, atomicity or relevance, and
  the literal HTML was not extracted.
- **Result.** Spoken automatically once, after the dialog-opening phrase: `Could not open this file This file does not
  exist any more.`. The earlier heading observation in NAME-5 is a deliberate read; it is not a second automatic
  conflict alert.
- **Capture.** AppleScript last-phrase polling, corroborated by the native `AXApplicationAlert` and its descendant text
  present at a later sample; no sample before the write was recorded, so this is not an observed write transition.
  "Once" cannot exclude an identical consecutive replay, as under "Evidence and its limits".
- **Environment.** E6, packaged app, real daemon refusal for a throwaway file that was removed after a real Git merge
  conflict.
- **Date.** 2026-10-25.
- **Reuse scope.** Covers this file-not-found read failure inside the conflicted viewer, with the viewer-opening focus
  move and delayed alert. It establishes neither other read failures nor a blanket match for every `PaneFailure` or
  `useAnnouncement` caller.

### WARN-1 Deletion warning order and checked option descriptions

- **Location and state change.** Delete an archived console owner with two archived bound project sessions. Before
  opening its Delete confirmation, the real daemon's branch inspection reported one uncommitted worktree and one locked
  worktree, and its agent-record inspection reported `present`. Tick the force-remove, locked-worktree and agent-record
  options, let the warning update settle, read each checkbox, then read through the warning.
- **Mechanism.** `packages/ui/src/dialogs/RequestedDialog.tsx` (`DeleteSessionDialog`; `ConfirmWarning` no longer
  exists) put the warning title, the bound-session list and then the consequence lines inside
  `packages/ui/src/components/TintedAlert.tsx` (`TintedAlert`), under `Alert.Content`. The indicator is `aria-hidden`.
  `packages/ui/src/dialogs/ConfirmDialog.tsx` (`ConfirmDialog`) keeps the options before that warning.
  `packages/ui/src/dialogs/ConfirmOption.tsx` (`ConfirmOption`) uses HeroUI `Label` and `Description` for the name and
  hint, plus an optional `aria-describedby` reference supplied by its caller;
  `packages/ui/src/dialogs/BranchCleanupOptions.tsx` (`BranchOption`) is one such caller. This case reads the resulting
  checkboxes and durable warning; it is not a `StatusAnnouncer` case.
- **ARIA wiring as rendered.** The outer native `AXApplicationAlertDialog` had the deletion question as title and
  description, `AXARIALive=assertive`, `AXARIAAtomic=0` and `AXARIARelevant=additions text`. Each checkbox had its
  option title and `AXValue=1`; the sampled `AXDescription` and `AXHelp` strings were empty even though VoiceOver spoke
  the hint on focus. Those strings alone therefore do not establish the description relation. Native static text exposed
  the warning title, two agent-and-session rows, then all three consequence lines. Literal HTML IDs and
  library-generated naming attributes were not extracted.
- **Result, warning.** Deliberate sequential navigation read these phrases in this order; the first consequence was
  split around the inline `Git` reference:

  ```text
  2 archived sessions under it are deleted with it
  Codex (⁨Default⁩)⁨Child 1 fixture turn⁩｜⁨Verification child 1⁩
  Codex (⁨Default⁩)⁨Child 2 fixture turn⁩｜⁨Verification child 2⁩
  Also permanently deletes the worktree or branch
  Git
   keeps.
  Also permanently deletes the locked worktree, uncommitted changes included.
  Also moves the agent's own records of this session and of the sessions deleted with it to the Trash.
  ```

- **Result, checked options.** After the update settled, focusing each checkbox spoke its label, its hint and its
  selected state:

  ```text
  Also force-remove the worktree or branch Git keeps Its uncommitted changes or unmerged commits cannot be recovered. 已勾选 checkbox
  Also delete the locked worktree It goes despite its lock and cannot be recovered. 已勾选 checkbox
  Also move the agent's own session records to the Trash The sessions can then no longer be resumed in the agent itself. You can recover the records from the Trash. 已勾选 checkbox
  ```

  An immediate read during the last option's update was replaced by its new consequence sentence; that sample was not
  counted as the checkbox read. The quoted first checkbox was deliberately focused again after the update settled. No
  deletion was confirmed or performed.
- **Result, consequence lines on checkbox focus.** Not spoken, by design: a checkbox is described by its own hint, which
  already states the consequence, and is not also described by the warning's line for it, which would say the same thing
  twice. `BranchOption` (`packages/ui/src/dialogs/BranchCleanupOptions.tsx`) and `SessionRecordsTrashOption` supply no
  `describedBy` to those lines, and these consequence entries carry no IDs. The pass was run against a checklist item
  that expected the lines as descriptions; no standard or product doc states that expectation, and the user confirmed it
  is not a requirement.
- **Capture.** AppleScript last phrase after process-targeted AX focus and VoiceOver cursor moves, with a requested 25
  ms background poll; native tree of the settled, checked state. The warning-order result is manual reading, not a claim
  that the whole warning was automatically spoken in that order.
- **Environment.** E7. Real daemon and UI, with fixture Codex processes, synthetic throwaway Codex conversation records,
  real bound session records and real Git worktrees. All session creation, binding, usage hooks, branch association,
  inspection and archiving went through daemon interfaces; no gallery or database seed supplied the dialog. This does
  not validate real Codex transcript production.
- **Date.** 2026-10-25.
- **Reuse scope.** Covers the warning order and existing label/hint reading in this delete-session confirmation, with
  one uncommitted worktree Git keeps, one locked worktree, two bound archived sessions and the records option. In this
  delete-session dialog no checkbox is associated with its consequence line, and that is not a case for doing so. The
  remove-project confirmation's trash option does give its checkbox a description, through its inspection-failed line
  (`describedBy` in `BranchCleanupOptions.tsx`); that description was not heard. It does not cover plural cleanup
  counts, live sessions, another dialog's options, deletion execution or recovering records from the Trash.
- **Superseded by.** WARN-3 (E9). The confirmation became a warning modal whose consequences are plain body text with
  no `TintedAlert`, and `ConfirmOption`'s note moved outside the checkbox, linked by `aria-describedby`; the trash
  option's inspection-failed line became a mark (WARN-4). This case no longer matches the product and stays as history.

### WARN-2 Trust prompt caution prefix and description

- **Location and state change.** Open a session in a throwaway untrusted project under the isolated home. The fixture
  Codex CLI writes its folder-trust question to its real PTY; the daemon's recognizer emits `trust_prompt`, and the
  packaged app opens the confirmation. Read its warning with VoiceOver.
- **Mechanism.** `packages/ui/src/dialogs/TrustPromptDialog.tsx` (`useTrustPromptDialogProps`, `TrustPromptMessage`,
  `Caution`) supplies the prompt to `ConfirmDialog`. `TintedAlert` holds an `Alert.Title` containing the `sr-only`
  `Caution:` prefix followed by the visible title; `Alert.Description` holds the agent-specific caution. The warning
  glyph is `aria-hidden`. This is durable dialog text read on demand; no `StatusAnnouncer` is involved.
- **ARIA wiring as rendered.** Native `AXApplicationAlertDialog`, title and description `Trust this folder?`, with
  `AXARIALive=assertive`, `AXARIAAtomic=0` and `AXARIARelevant=additions text`. The inner warning title mapped to an
  unnamed native group with class `alert__title`; its two static-text children were `Caution:` and `The project's own
  configuration takes effect`. The Codex description followed as static text split by inline references. This native
  capture does not claim literal HTML naming IDs or an independently sampled live role on the inner warning.
- **Result.** Deliberate sequential reading spoke the prefix and title as one phrase, then the description as one phrase
  with an internal newline. JSON strings below preserve the double space after `lets`, the space before that newline and
  the phrase boundaries:

  ```json
  [
    "Caution:The project's own configuration takes effect",
    "Trusting lets  Codex load this project's own configuration, hooks and command rules, which can run code even without a model request. This confirmation does not change \nCodex's sandbox or approval policy, though the project's own configuration can."
  ]
  ```

  A system notification interrupted earlier introductory text in that walk; it did not replace these two warning
  phrases. No trust confirmation was accepted and no real agent authentication was used.
- **Capture.** AppleScript last phrase after VoiceOver cursor moves, with a requested 25 ms background poll, and native
  tree/attribute reads. The daemon's actual `trust_prompt` event corroborated the asking agent, project and offered
  parent directory; this was not a fixture WebSocket reply to the UI.
- **Environment.** E7, packaged app with real daemon trust recognition over a credential-free fixture Codex CLI. Its
  entire trust screen was `Trust this folder?`, `› 1. Trust and continue`, `  2. Quit`, followed by a wait for input. It
  is evidence for Octoboard's prompt and reading, not for real Codex's current output or trust policy.
- **Date.** 2026-10-25.
- **Reuse scope.** Covers this hidden caution prefix plus visible title followed by the Codex-specific description in
  the packaged trust dialog. It does not establish the Claude Code or Grok descriptions, other languages, accepting
  trust, folder-wide persistence or any real agent's sandbox behavior.
- **Superseded by.** WARN-5 (E9). The caution is now one plain paragraph, with no `TintedAlert`, hidden prefix or
  title. This case no longer matches the product and stays as history.

### WARN-3 Deletion warning modal with option notes outside the checkbox

- **Location and state change.** Delete an archived console owner with two archived bound project sessions. Before and
  after archiving, the real daemon's inspections reported one uncommitted worktree and one locked worktree (with their
  checked-out branches), and the agent-record inspection reported `present`. Tick the force-remove, locked-worktree and
  agent-record options, let the update settle, focus each checkbox anew, then read from Close through the body, the
  list and the consequences. No deletion was confirmed.
- **Mechanism.** `packages/ui/src/dialogs/RequestedDialog.tsx` (`DeleteSessionDialog`, `ConfirmConsequences`),
  `packages/ui/src/dialogs/ConfirmDialog.tsx` (`ConfirmDialog`) and `packages/ui/src/dialogs/Dialog.tsx` (`Dialog`): a
  warning modal, the warning icon in its header, a plain title, the subject in the body's first sentence, and the
  session list and consequence paragraphs as plain body text with no `TintedAlert`.
  `packages/ui/src/dialogs/ConfirmOption.tsx` (`ConfirmOption`) keeps the `Label` inside the `Checkbox` and renders the
  note as a `Description` outside it, linked by an explicit `aria-describedby` to its `useId`;
  `packages/ui/src/dialogs/BranchCleanupOptions.tsx` (`BranchOption`) is one such caller. Durable reading and focus; no
  `StatusAnnouncer`.
- **ARIA wiring as rendered.** Native `AXApplicationAlertDialog`, title and description `Delete session`,
  `AXARIALive=assertive`, `AXARIAAtomic=0`, `AXARIARelevant=additions text`. The scrolling body is an
  `AXLandmarkRegion` named `Delete session` holding the plain paragraphs and list; the heading is level 2; no named
  warning glyph is exposed; the body has no inner alert or status region. Each checkbox's title is its label alone,
  `AXValue=1`, with empty sampled `AXDescription` and `AXHelp`; each note is a separate following static text. The
  speech on focus establishes that the note is read; literal HTML ID strings were not extracted.
- **Result, checked options.** Pass. Focusing each checkbox spoke its label, its note and its state:

  ```json
  [
    "Also force-remove the worktree or branch Git keeps Its uncommitted changes or unmerged commits cannot be recovered. 已勾选 checkbox",
    "Also delete the locked worktree It goes despite its lock and cannot be recovered. 已勾选 checkbox",
    "Also move the agent's own session records to the Trash The sessions can then no longer be resumed in the agent itself. You can recover the records from the Trash. 已勾选 checkbox"
  ]
  ```

- **Result, sequential reading.** Pass. From Close, the substantive phrases in order (each separate note is reached
  again by the cursor after its checkbox):

  ```json
  [
    "标题级别2 Delete session",
    "Delete session region",
    "The session Fixture turn | Verification project｜Verification owner will be deleted permanently.The project's files are kept.",
    "Also force-remove the worktree or branch Git keeps Its uncommitted changes or unmerged commits cannot be recovered. 已勾选 checkbox",
    "Also force-remove the worktree or branch  Git  keeps",
    "Its uncommitted changes or unmerged commits cannot be recovered.",
    "Details Also force-remove the worktree or branch Git keeps Its uncommitted changes or unmerged commits cannot be recovered. 已折叠 button",
    "Also delete the locked worktree It goes despite its lock and cannot be recovered. 已勾选 checkbox",
    "Also delete the locked worktree",
    "It goes despite its lock and cannot be recovered.",
    "Details Also delete the locked worktree It goes despite its lock and cannot be recovered. 已折叠 button",
    "Also move the agent's own session records to the Trash The sessions can then no longer be resumed in the agent itself. You can recover the records from the Trash. 已勾选 checkbox",
    "Also move the agent's own session records to the Trash",
    "The sessions can then no longer be resumed in the agent itself. You can recover the records from the Trash.",
    "2 archived sessions under it are deleted with it",
    "Codex (Default)Fixture turn | Verification project｜Verification child 2",
    "Codex (Default)Fixture turn | Verification project｜Verification child 1",
    "Also permanently deletes the worktree or branch  Git  keeps.",
    "Also permanently deletes the locked worktree, uncommitted changes included.",
    "Also moves the agent's own records of this session and of the sessions deleted with it to the Trash.",
    "的结尾 Delete session region",
    "Cancel button",
    "Delete all button",
    "的结尾 Delete session web alert dialog"
  ]
  ```

  Unlike E7, the first consequence was one phrase, with double spaces around `Git`. The sessions were listed child 2
  then child 1, and speech followed that order. Consequence sentences updated while ticking were heard but not counted
  as checkbox reads. This is manual reading, not a claim that the whole dialog was spoken automatically in this order.
- **Result, opening announcement.** Unconfirmed: the automatic speech on opening was not reliably captured. Not a
  failure.
- **Two joins in the heard text.** "permanently.The project's" (the subject and the message, which were separated by a
  text node that was only a space) and "Codex (Default)Fixture turn" (a session row's hidden agent name directly before
  its title; E7 heard the same join). The current source puts the space into the message's own text node and ends the
  hidden agent name with a comma ("Codex (Default), "), since a hidden element's trailing space is dropped in
  rendering. That changes the text only, not any item under "What \"matches a case\" means"; the new phrases were not
  heard.
- **Capture.** AppleScript last phrase, by background polling at a requested 25 ms and by direct reads after guarded
  cursor commands; the native tree with all three options checked.
- **Environment.** E9. Real daemon and UI, fixture Codex processes, synthetic dated Codex records, real bound session
  records and real Git worktrees, uncommitted and locked, kept on archiving. No gallery or database seed supplied the
  dialog.
- **Date.** 2026-10-26.
- **Reuse scope.** Covers this warning modal's plain structure (title, subject sentence, session list, consequence
  paragraphs) and `ConfirmOption`'s external note read through `aria-describedby` on checkbox focus, with one
  uncommitted worktree Git keeps, one locked worktree, two archived bound sessions and the records option. It does not
  separately establish Archive, Delete archived, plural cleanup counts, another language, deletion or recovery, or the
  announcement on opening.

### WARN-4 Remove project with running sessions and a lookup-failure mark

- **Location and state change.** Remove project, for a project with two real fixture sessions still `working`. Read the
  unticked Trash option and the body. Then tick Trash, with a deliberately malformed Git configuration in the throwaway
  repository making the real worktree inspection fail (an earlier attempt with a renamed directory gave an empty
  successful lookup and is not evidence of the failure), read the settled ticked checkbox and the separate mark, and
  press the mark. Cancel the removal; the configuration and path were restored before cleanup.
- **Mechanism.** `packages/ui/src/dialogs/RequestedDialog.tsx` (`DeleteProjectDialog`, `ConfirmConsequences`),
  `packages/ui/src/dialogs/ConfirmDialog.tsx`, `packages/ui/src/dialogs/Dialog.tsx`,
  `packages/ui/src/dialogs/ConfirmOption.tsx`, `packages/ui/src/dialogs/BranchCleanupOptions.tsx`
  (`useProjectRemovalCleanup`) and `packages/ui/src/components/StatusMark.tsx` (`StatusMark`): the warning mark is a 24
  px button after the checkbox, outside its label; the failure adds no description to the checkbox. The note is linked
  as in WARN-3.
- **ARIA wiring as rendered.** Native `AXApplicationAlertDialog`, title and description `Remove project`, live
  assertive, atomic 0, relevant additions text; a plain level-2 heading, the subject and body as static text, the
  sessions as an `AXList` with each working status a named image and each agent name static text. The checkbox's title
  is `Also move the project folder to the Trash`, `AXValue` 0 then 1, with empty sampled `AXDescription` and `AXHelp`
  and the note a separate static text. The failed inspection exposed a distinct `AXButton`, title and description
  `Other worktrees unknown`, right after the checkbox and before the note in native order, with a native frame of
  25 × 24; pressed, it opens an `AXApplicationDialog` of the same name holding the explanation. It is not part of the
  checkbox's name.
- **Result, checkbox focus.** Pass, unticked and then ticked with the lookup failed; no failure text joins either:

  ```json
  [
    "Also move the project folder to the Trash The whole folder goes, including anything not committed toGit. You can recover it from the Trash. 未勾选 checkbox",
    "Also move the project folder to the Trash The whole folder goes, including anything not committed toGit. You can recover it from the Trash. 已勾选 checkbox"
  ]
  ```

  `toGit` is the native output. The note's DOM keeps the space before the `Git` reference (sequential reading below
  splits it there); VoiceOver's reading of the description drops it.
- **Result, sequential reading, Trash unticked.** Pass:

  ```json
  [
    "标题级别2 Remove project",
    "The project  Verification running project  will be removed.Its sessions are deleted with it. Files on disk are kept.",
    "Also move the project folder to the Trash The whole folder goes, including anything not committed toGit. You can recover it from the Trash. 未勾选 checkbox",
    "Also move the project folder to the Trash",
    "The whole folder goes, including anything not committed to",
    "Git",
    ". You can recover it from the Trash.",
    "2 sessions are still running and will be ended",
    "Working Working  可点按  image",
    "Codex (Default)Fixture turn | Verification project｜Running fixture 2",
    "Working Working  可点按  image",
    "Codex (Default)Fixture turn | Verification project｜Running fixture 1",
    "Cancel button",
    "End and remove button"
  ]
  ```

- **Result, the mark.** Pass. Its focus, then pressing it:

  ```json
  [
    "Other worktrees unknown 已折叠 button",
    "Worktrees of the repository outside the project folder may be moved to the Trash with it, and its branches deleted permanently. Other worktrees unknown web dialog"
  ]
  ```

- **Result, opening.** Once, on opening, the first recorder caught this automatic phrase; it is not a guarantee for
  every opening:

  ```text
  Remove project The project Verification running project will be removed. Its sessions are deleted with it. Files on disk are kept. Also move the project folder to the Trash The whole folder goes, including anything not committed toGit. You can recover it from the Trash.2 sessions are still running and will be ended web alert dialog
  ```

  The join "Trash.2 sessions" is between two separate blocks of the body in the dialog's own description.
- **Joins in the heard text.** "removed.Its" and "Codex (Default)Fixture turn" have the same cause and the same
  source change as in WARN-3; the new phrases were not heard.
- **Capture.** As WARN-3. The ticked checkbox and the mark were focused again from Cancel first, so an unchanged focus
  could not leave a stale last phrase.
- **Environment.** E9.
- **Date.** 2026-10-26.
- **Reuse scope.** Covers this Remove project state with two working sessions, the external note, the separate
  failed-inspection mark after the checkbox and its press-opened explanation. Opening the mark by hover or by focus,
  carrying out the removal and force-removing locked worktrees were not tested.

### WARN-5 Trust prompt caution as plain body text

- **Location and state change.** Open a session in an untrusted throwaway Git project. The credential-free fixture
  Codex CLI writes exactly `Trust this folder?`, `› 1. Trust and continue`, `  2. Quit` to its real PTY and waits; the
  real daemon emitted `trust_prompt` for that session and project and offered the parent folder. Read from Close through
  the body, the caution and the folder-wide checkbox; finish with Not now, never Trust and continue.
- **Mechanism.** `packages/ui/src/dialogs/TrustPromptDialog.tsx` (`useTrustPromptDialogProps`, `TrustPromptMessage`,
  `Caution`), `packages/ui/src/dialogs/ConfirmDialog.tsx` and `packages/ui/src/dialogs/Dialog.tsx`: the caution is the
  agent-specific sentence in one plain `p` after the prompt's other paragraphs, with no hidden prefix, no title of its
  own, no `TintedAlert` and no `StatusAnnouncer`.
- **ARIA wiring as rendered.** Native `AXApplicationAlertDialog`, title and description `Trust this folder?`, live
  assertive, atomic 0, relevant additions text; a plain level-2 heading. The caution paragraph's static-text children
  run from `Trusting lets ` through the two inline `Codex` references to its end. Neither `Caution:` nor `The project's
  own configuration takes effect` occurs in the prompt's native subtree, and the caution holds no alert or status.
- **Result, sequential reading.** Pass. The phrases as heard (path breaks and double spaces are native):

  ```json
  [
    "标题级别2 Trust this folder?",
    "Codex  is asking whether to trust the folder  /private/tmp/",
    "octoboard-vo9-ddnbws96",
    "/fixtures/trust-fixture for session Trust fixture .",
    "If you agree,  Octoboard  presses  Codex 's own confirmation, and",
    "Codex",
    " records the trust, so it does not ask again here.  Octoboard",
    "also records a permission every agent shares, so a later trust confirmation for this project is pressed without asking.",
    "Codex records the trust for the root of the git repository this folder is in. If this project is a subfolder or a linked worktree, that covers more than this project.",
    "Trusting lets  Codex load this project's own configuration, hooks and command rules, which can run code even without a model request. This confirmation does not change \nCodex's sandbox or approval policy, though the project's own configuration can.",
    "Trust every project under /private/tmp/octoboard-vo9-ddnbws96/fixtures This records the folder instead of this project, and covers every project under it, at any depth, including ones added there later, for every agent. 未勾选 checkbox",
    "Trust every project under  /private/tmp/octoboard-vo9-ddnbws96/fixtures",
    "This records the folder instead of this project, and covers every project under it, at any depth, including ones added there later, for every agent."
  ]
  ```

  The caution was one phrase, with its internal newline, and nothing before it named it a caution.
- **Result, opening announcement.** Unconfirmed: the automatic speech on opening was not reliably captured. Not a
  failure.
- **Capture.** AppleScript last phrase read directly after guarded cursor commands, the native tree, and the daemon's
  actual `trust_prompt` event; no fixture WebSocket reply to the UI.
- **Environment.** E9. Evidence for Octoboard's prompt and its reading, not for real Codex's current output, login or
  trust policy.
- **Date.** 2026-10-26.
- **Reuse scope.** Covers the Codex-specific plain caution in the packaged trust dialog. It does not establish the
  Claude Code or Grok cautions, another language or appearance, accepting trust or its persistence.

### Known not to work

These patterns were heard failing or heard to leave a gap. Do not reuse them, and do not treat an implementation of
their kind as a match for any case. KNW-6 to KNW-8 were heard on E4 and have since been replaced by passing cases heard
on E5; KNW-9 was heard on E6 and is replaced by FOCUS-4, heard on E8. They stay as history of what the earlier
mechanisms did. KNW-8 and KNW-9 are the exceptions in kind: each records one failing aspect of a recording whose case
stays a pass. KNW-8 records the focus destination of the same recording as the passing case SA-K, whose reuse scope
covers the status write and states that focus relation, so it never barred matching SA-K's status write. KNW-9 records
the bare-mark Enter naming of the recording behind FOCUS-3, whose other results stay passing.

#### KNW-1 A plain status mounted already holding its text

- **What and where.** The same first-form login dialog (E1): the `role="status"` was inserted holding "Logging in…".
- **Result.** Readable manually only. On opening, the automatic output was the dialog name and `Cancel login button`;
  moving the VoiceOver cursor left from Cancel read `Logging in…`. The automatic initial announcement was not observed
  and must not be reported as an automatic pass. That the region arrived populated is a timing lead, not a proven cause.
- **Replaced by.** SA-C, where the same text is spoken automatically.

#### KNW-2 Success written into a durable status while its modal closes

- **What and where.** E1: the account row's `sr-only` wrapper with an inner `role="status"`, empty until success, was
  filled at the moment the state became succeeded, while the login dialog was closing and focus was being returned.
  `AccountLoginAnnouncement` in `packages/ui/src/settings/AccountLogin.tsx` in that build, mounted by the account row
  (`AccountRow` in `packages/ui/src/settings/AgentAccountsSection.tsx` then; `AccountTableRow` now).
- **Result.** Not spoken, in two completed runs, with capture running at least 12 s past the success (longer than the
  6000 ms the text was held). The expected phrase `VoiceOver fixture: logged in.` was never captured; what was spoken
  after success was the Settings dialog and the focused control. The native tree did contain the text, which is
  rendering evidence only. The expected string comes from the resource, not from speech; do not quote it as heard. The
  cause was not proven; the region sitting under the closing modal and the focus move replacing the status are the
  working explanation, supported by the later passing form (SA-D).
- **Replaced by.** SA-D.

#### KNW-3 Success written after a delay, but focus left on the web area

- **What and where.** E2: the delayed caller write of SA-D worked through the Actions menu. Through the direct Log in
  button the dialog closed with focus on the web area (the original button gone), and no phrase followed.
- **Result.** Not spoken on the direct path, through more than 12 s: `Octoboard 网页内容` and a no-action button phrase,
  then no success. The region received its text about 1.5 s after closure. The correlation with the lost focus
  destination is verified; a proven cause is not.
- **Replaced by.** FOCUS-1 together with SA-D (E3 and E4).

#### KNW-4 Success written while the window is not attended

- **What and where.** E3: the same delayed write, with the window minimized (application otherwise not frontmost) when
  the login succeeded.
- **Result.** Not spoken: the text appeared in the region while the window was away, about 3 s before native focus
  returned, and no logged-in phrase was captured in about 11 s after the return (15 s after the success). On return
  VoiceOver spoke the window's focused control. Native focus was unavailable while away, so the order of hidden focus
  and animation frames is source-derived.
- **Replaced by.** SA-D, which waits for attention (E4).

#### KNW-5 Cancel with a hand-off that only handles success

- **What and where.** E3: the dialog opened from the direct Log in button was cancelled while starting. The focus
  hand-off of that build covered a successful close only.
- **Result.** Focus fell to the web area and stayed there for 7.5 s or more in two runs, one with each of two fixture
  variants, although the original Log in button was present and enabled in the sample taken afterwards. Only `Octoboard
  网页内容` was spoken; the button's phrase never was. The suspected cause, a button disabled while the login runs that
  could not take the restored focus, is unproven.
- **Replaced by.** FOCUS-1 (E4).

#### KNW-6 Text written at the moment its hidden region is re-exposed

- **What and where.** E4: the Advanced section of the launch dialog. Its `StatusAnnouncer` is mounted while the section
  is collapsed (the disclosure panel keeps its children mounted, hidden), so its default 100 ms delay runs collapsed and
  later writes are immediate. Collapsing resets the model query to idle, so the region stays empty while collapsed; on
  reopening the kept answer comes back in the same render that removes the panel's hidden state, so the write coincides
  with the re-exposure. `packages/ui/src/dialogs/LaunchSettingsFields.tsx` (`AdvancedSection`) with
  `packages/ui/src/dialogs/useAgentModels.ts`. After a model-list failure, the section was collapsed and reopened with
  focus on the Advanced button throughout.
- **Result.** Not spoken. The failure text (`⁨Grok Build⁩ could not list its models: ⁨exit status: 1: Fixture has no
  models⁩`, the isolation marks being part of the phrase) appeared in the native tree when the section was re-exposed,
  but the speech stream held only the button's collapsed and expanded phrases (`Advanced 已折叠 button`, `Advanced 已展开
  button`) and later `Advanced group`, with no failure sentence in 11 s. The text was not claimed to be unreachable by
  navigation. A fresh first-write case was not recorded: an earlier run with a populated sequence had an empty speech
  stream after a relaunch and is excluded.
- **Replaced by.** SA-M (E5), where the region is mounted with each opening, outside the folded body.

#### KNW-7 A plain status populated when its container mounts

- **What and where.** E4: Settings, General then Permissions. `packages/ui/src/settings/PermissionsSection.tsx`
  (`PermissionsSection`): each row's visible state is a `role="status"` span, with a hidden subject, rendered populated
  once the native permission read completes; no `StatusAnnouncer`, no timer. Focus moves to the Permissions tab. Inside
  Settings' own dialog.
- **Result.** Not spoken. Both statuses (`Screen Recording Not granted.` and `Accessibility Not granted.`) were present
  with the common native mapping, but only the tab phrase (`Permissions 已选择 tab，第5项（共7项）`) was spoken in 10 s. This is
  populated-on-mount evidence only; later updates to such a status were not recorded (no permission was granted or
  revoked).
- **Replaced by.** SA-L (E5), where a section-level status writes the states after an attended wait.

#### KNW-8 The pressed control removed, focus falls to the web area

- **What and where.** E4: the diff expansion of SA-K. The expansion button is removed when the expansion is done, and
  nothing receives focus.
- **Result.** Native focus became the web area and the phrase `Octoboard 网页内容` was spoken. The status itself was spoken
  (SA-K). Recorded as observed.
- **Replaced by.** FOCUS-2 (E5), where focus goes to the surviving control or the code frame.

#### KNW-9 A bare mark's dialog named by its own sentence

- **What and where.** E6: Enter on the failed SVG notice mark in the viewer (FOCUS-3). The pressed dialog was named by
  its one sentence through `aria-labelledby`, and the same sentence was its body.
- **Result.** Read twice, as one phrase with the sentence once as the dialog's name and once as its body: `This image
  cannot be displayed, so its text is shown instead. This image cannot be displayed, so its text is shown instead. web
  dialog`. The rest of FOCUS-3 (quota focus paths, the mark's focus return on Escape, item-chooser reachability, as
  wired in E6) stays a pass.
- **Replaced by.** FOCUS-4 (E8), where the dialog is named by its tone word.

### OPEN-1 Import result dialog rename

- **Location and state change.** The configuration import's result step: the dialog's title becomes "Import finished"
  and the step moves focus to the dialog itself.
- **Mechanism.** Focus moved onto the dialog so it is read out by its new name, with no `StatusAnnouncer`.
  `packages/ui/src/configuration/ImportConfigurationDialog.tsx` (`ImportConfigurationDialog`, result step).
- **ARIA wiring, capture, environment, date.** Not recorded: this case has not been listened to.
- **Result.** Unconfirmed. That VoiceOver reads the renamed dialog on this focus move has not been listened to yet.
- **Reuse scope.** None until confirmed.

## Existing `StatusAnnouncer` callers and the case each matches

Compared strictly against the rules above, from the code and the recordings. Paths are relative to `packages/ui/src`.
"Matches" means every item agrees with the named case for the writes listed. A region inside the modal's own subtree
counts as exposed in the same way as one kept exposed by a marker; a region in the main window counts as exposed only
for a write made while no modal is open. The Basis column uses one rule: **Heard** means the caller itself was listened
to for the writes named; **Candidate** means it is matched to a case recorded on another caller, on agreement of the
items, and was not itself heard. A candidate still has to pass the checks under "Claiming a match".

| Caller (file, symbol) | Announces | Case | Basis | Why |
|---|---|---|---|---|
| `sidebar/Sidebar.tsx` | filter no-match | SA-A for a change while the section is mounted; no match for the rest | Heard | The section is kept per console, so it can mount with the no-match text already present (switching back to a console, leaving a focus view). SA-H does not cover that: SA-H was heard for a panel toggle with focus left on the toggle, while these mounts come with a console or view switch whose focus relation was not recorded. |
| `components/ConnectionBanner.tsx` | outage | SA-B | Heard | Matches in full. |
| `settings/AccountLoginDialog.tsx` | login progress | SA-C for the dialog opening; no match for the rest | Heard | After Log in again the announcer remounts with a footer focus move, not a dialog opening, which was not heard. |
| `settings/AccountLogin.tsx`, `AccountLoginAnnouncer` | login success | SA-D on the earlier row layout (possibly stale, see SA-D); the second mount in `settings/AccountDetailsDialog.tsx` is not covered | Heard | The wait and attention gate match SA-D in source; the table row and the details dialog were not heard. |
| `settings/PermissionsSection.tsx`, `PermissionsAnnouncer` | permission states | SA-L for the section opening and a return with the states unchanged; no match for a change while Settings is open | Heard | A grant made in System Settings was not heard, and Settings opened directly onto Permissions is unreachable from production controls and untested. |
| `configuration/ImportReview.tsx` | "apply to all" applied | SA-A | Candidate | Mounted empty in the review step, text written after a press while focus rests, inside the modal's own subtree, default delay. Writing the identical sentence again does not rewrite it. |
| `browser/ChangeList.tsx` | filter no-match | SA-A for typing; no match for a first render holding the text | Candidate | Typing in the filter writes in place at rest. The text can also be present at the first render (a persisted filter, a branch change that closes a popover and returns focus), a mount SA-H does not clearly cover. |
| `viewer/FileViewer.tsx`, `ViewerContentView` | loading, disconnected | SA-J for navigation loading; SA-C for the dialog opening as a candidate; no match for disconnected | Heard for SA-J, candidate for SA-C | The navigation text is written at once with focus on Next file (SA-J). The region is mounted with the dialog with a 1500 ms first delay as focus moves, the shape of SA-C, but the opening was heard on the login dialog only. The disconnected text was not exercised. |
| `viewer/FileViewer.tsx`, `UnchangedView` | loading, read outcome | SA-J for the outcome after navigating; no match for its own loading text or other outcomes | Heard | 1500 ms first delay on a new child inside the open dialog with focus kept, heard for the unchanged outcome only. Its own "Loading…" (written when the read takes longer than the first delay), the failure and other outcomes, and a dialog that opens straight onto the view, were not heard; the `Loading…` heard came from the parent. Each text is emptied `READ_TIME_MS` after it was written (`clearAfterRead`), which was not heard either. |
| `components/CopyButton.tsx` | "Copied" | SA-G for the confirmation and the tooltip-driven clear, for a press made while the button has focus; no match for the 2 s automatic clear | Heard | Heard in the main window with focus kept. The 2000 ms branch without a tooltip, a repeat press and a failure were not heard. The copy button in the viewer dialog sets `preventFocusOnPress` (a pointer press does not move focus to it), carries its own "Path copied" text and sits in a dialog, so it is a candidate only for a keyboard press made while it has focus. |
| `browser/GitView.tsx` | git loading, comparing, listing, spoken failures | SA-H for the first mount with focus kept; no match for the Changes-tab mount, replacement sequences or the failure sentences | Heard | Heard for the Changes loading at mount with focus on the panel toggle. The tab variant, which moves focus into the pane, captured no sentence, and the comparing and listing texts replacing one another in place were not heard. The failure sentences (`useSpokenFailures`) were not exercised. |
| `browser/ProjectBrowser.tsx` | files loading | SA-H for the load when the Files panel is shown with focus kept; no match for the Files-tab mount or search loading | Candidate | Same hidden `div`, default delay and main window as the representative, but its own Files load finished too fast to sample, switching to the Files tab is the untested focus-move variant, and search-triggered loading was not recorded. |
| `components/ActionMenu.tsx`, `LoadedMenuItems` | menu loading, load failed | SA-I for the loading text; no match for load failed | Heard | Heard with the submenu focused and the git list stretched to 2 s; a visible "Loading…" item says the same word, so attribution is shared. The failure update was masked by an error alert. |
| `viewer/CodeSurface.tsx` | expansion loading and note | SA-K and FOCUS-2 for the loading text and the focus after an expansion; no match for the outcome notes | Heard | Loading heard while the pressed button had focus, and focus restored to the surviving control or the code frame on E5. Notes, failure and capped outcomes were not exercised, nor the emptying of each text `READ_TIME_MS` after it was written (`clearAfterRead`). |
| `dialogs/LaunchSettingsFields.tsx`, `AdvancedSection` | models loading, unavailable, failed | SA-M for loading and failed; no match for the unavailable text | Heard | Region mounted with each opening outside the folded body, heard for a fresh opening and two reopenings. The unavailable text was not heard separately. |
| `terminal/ShellTerminalPanel.tsx` | shell starting | none | none | Text can be present at the first render at default delay and clears as a terminal appears and takes focus, a write at a focus move; the starting status was not observed populated. |
| `terminal/TerminalPane.tsx`, cover and dormant status | loading, starting, resuming, reconnecting, disconnected and switching covers; the dormant bar's "Not running" or "This session is archived" | SA-A for the writes into the mounted regions (the pane always mounts with no session, so SA-H applies only to test and gallery mounts); no match for a write made as selecting a session moves focus into the terminal, and none for the dormant bar's button | Candidate | Two `StatusAnnouncer` regions mounted with the pane, default delay (browser-observed: regions inserted empty, text written 100 to 250 ms later). The bar's sentence is `aria-hidden` and its button stays reachable, so only the sentence is spoken, not sentence and button as PLAIN-2 heard. The bar's sentence is held while a modal is open (selecting an interrupted session on an incomplete account asks first) and written `AFTER_FOCUS_MOVE_DELAY_MS` after the close, so it is not written under the modal (KNW-2). A sentence already spoken stays in the region unchanged through a modal, so closing it says nothing. Nothing was heard on the pane itself. An archived bar and `SessionHistoryView`'s own region can both hold text while the history loads (different sentences, no duplicate). |
| `terminal/ShellTerminalView.tsx`, trouble and exited status | disconnected, reconnecting and exited, for the shown tab only | close to SA-H for the first write after a keyboard tab switch (focus stays on the tab; the regions are mounted with each showing, but with the longer `AFTER_FOCUS_MOVE_DELAY_MS`), a candidate only; no case matches a pointer switch or a new terminal, where focus moves into the view (open item); SA-A for trouble changes; PLAIN-2 for the exit filling in place while the terminal input keeps focus, with the sentence only | Candidate | Two `StatusAnnouncer` regions per shown tab, empty when mounted (browser-observed); a hidden tab has none, so switching to an exited or troubled tab writes its text after the delay rather than with the region. PLAIN-2 heard the earlier hand-built region holding the sentence and the Close terminal button, spoken as `The shell has exited Close terminal`; the button is now outside the region, so the match is for the sentence only, by decision, and it has to be listened to again before it is claimed. The write after a tab switch made with the pointer comes with a focus move into the view, which no case covers. |
| `viewer/notices.tsx`, `FileNotice` | the notice's sentence, for a notice that arrives after the viewer has painted | SA-N for SVG fallback, clear and later reading; the first-paint large-file notice has no announcement region (no `speak`) and stayed silent, as context | Heard for these branches | 1500 ms first delay and `clearAfterRead`; SVG sentence present at about 2 s and empty by about 7 s, with focus handed from the now-unavailable Previous file to Next file. E6 did not sample native live attributes. Other image failures, patch fallbacks and replacement sequences remain unheard; SA-J is still the retained-focus representative. |
| `viewer/notices.tsx`, `BlockFallback` | the fallback sentence, for a block (a Mermaid diagram or a formula) that fails after paint, when `speak` is set | none | none | Uses `SpokenSentence` as `FileNotice`; `MermaidBlock` and `FormulaView` pass `speak`. Not heard; focus relation, exposure and native live attributes are unmatched. |
| `viewer/notices.tsx`, `SpotNotice` | the notice's sentence, for a spot in a document that fails or falls back after paint | SA-N is the nearest recorded mechanism; no complete match established | none | Uses `SpokenSentence` as `FileNotice`, rendered as a `span` inside a paragraph, but this caller was not heard. Focus relation, exposure and native live attributes still need matching evidence; SA-N does not establish them for a document spot. |
| `App.tsx`, account switch outcome | a batch of account switches finished | none | none | Written seconds after the switch dialog has closed, once the batch finishes without failures, and cleared after 10 s. The credential-free fixture does not produce conversation records to switch, so it was not recorded. |
| `settings/UpdatesSetting.tsx` | update check outcome and failure, once the user has engaged the row | none | none | Region mounted with the Updates row in Settings; the buttons go pending rather than disabled so they keep focus. Not heard, and no case records a write after a press in Settings. |
| `settings/ShortcutsSection.tsx` | shortcut rebind, conflict and reset sentences | none | none | Region mounted with the section; an identical consecutive sentence is made distinct by alternating a trailing no-break space so it is written again. Not heard. |
| `settings/AccountSharingSwitch.tsx` | sharing change failure (`clearAfterRead`); direction and phase of a running change | none | none | Two regions, one beside the switch and one in `SharingProgress`. Not heard. |
| `updates/UpdateNotice.tsx`, through `AttendedStatusAnnouncer` | update available, installed and similar notice | SA-L is the nearest recorded mechanism (the same attended wait); no complete match established | none | The wait is the one `PermissionsAnnouncer` has, but the region sits beside a bottom strip in the main window, not in a section shown by a tab focus move, and the strip can arrive while the user is elsewhere. Not heard. |
| `terminal/SessionHistoryView.tsx` | history loading, unavailable, empty | none | none | Region mounted with the view, which comes with a focus move into the history region. Not heard. |
| `about/ThirdPartyLicensesDialog.tsx` | search result count or none, typed in the search field; loading line at the dialog opening | none | none | Two regions in an open dialog: the count region writes in place while focus rests in the field (the shape of SA-A, inside a dialog), the loading region has `firstWriteDelayMs` of `AFTER_FOCUS_MOVE_DELAY_MS` as the dialog opens (the shape of SA-C). Neither was heard. |
| `dialogs/ResumeConversationFields.tsx` | resume lookup outcome | none | none | Always-mounted region in the new-session dialog's Resume tab; text arrives after typing in the Session ID field while focus stays there. Not heard. |
| `dialogs/SessionRequestForms.tsx`, `IncompleteSharingAnnouncer` | an account's unfinished sharing change marked after the dialog opened | none | none | Mounted with the request dialog; text appears after the opening. Not heard. |
| `dialogs/HandoffDialog.tsx` | branch check failed (`firstWriteDelayMs` of `AFTER_FOCUS_MOVE_DELAY_MS`) | none | none | Region mounted with the dialog; a failure arrives after it opened and again on each failed retry. Not heard. |
| `report/ReportPanel.tsx` | list failure mark's words | none | none | Region mounted through every state of the panel. Not heard. |

Totals: of 32 callers, 13 were heard for the writes named (`Sidebar`, `ConnectionBanner`, `AccountLoginDialog`,
`AccountLoginAnnouncer`, `PermissionsAnnouncer`, `ViewerContentView`, `UnchangedView`, `CopyButton`, `GitView`,
`ActionMenu`, `CodeSurface`, `AdvancedSection`, `FileNotice`), 5 are candidates (`ImportReview`, `ChangeList`,
`ProjectBrowser`, `TerminalPane`, `ShellTerminalView`) and 14 have no complete match (`ShellTerminalPanel`,
`SpotNotice`, `BlockFallback`, the account switch outcome in `App`, and the ten callers listed after it). Of the heard
callers, `ConnectionBanner` matches for every write it makes, and `AccountLoginAnnouncer` did on the row layout it was
heard on; the other eleven match for some writes only, with SA-N's native-capture limits stated in that case. Of the
candidates, `ImportReview` matches for every write it makes; `ChangeList`, `ProjectBrowser`, `TerminalPane` and
`ShellTerminalView` for some.


Hand-built `role="status"` elements, which are not `StatusAnnouncer`:

| Element (file, symbol) | Case | Why |
|---|---|---|
| `WelcomeScreen.tsx` | none | A plain status populated on mount, then replaced in place from "Checking…" to Installed or Not detected when availability arrives. The populated-on-mount shape is the one not spoken for the earlier Permissions rows (KNW-7); not mounted or tested separately. |

## Open items

- KNW-9 preserves the E6 bare mark's duplicated sentence as a failure. FOCUS-4 heard the replacement in E8 for Error and
  Note, with one sentence in the opening phrase and focus returned on Escape. Warning, the restricted-change rich path
  line, other custom-body marks and the generic `Hint` touch wrapper were not heard. Touch VoiceOver requires iOS/iPadOS
  and is outside this desktop pass.
- WARN-3 to WARN-5 (E9) establish the warning modals' plain body and the external option notes on focus and in
  sequential reading, the Remove project mark and the Codex trust caution as plain text; they replace WARN-1 and
  WARN-2, which stay as history. The automatic announcement on opening a warning modal or the trust prompt was not
  reliably captured and stays unconfirmed. Earlier Safari/gallery readings of these warnings are not used as passing
  cases. Plural cleanup counts, Archive and Delete archived, the other agents' cautions, deletion execution, real agent
  records and trusting a folder in a real agent are not covered. A checkbox note holding an inline reference is read on
  focus without the space before the reference ("committed toGit"), although the DOM keeps it.
- Hover-opened status-mark and quota content was reached through the VoiceOver item chooser in FOCUS-3. Sequential
  navigation did not reach that content in the attempted walks; it is not certified by the successful chooser route.
  A bare mark's hover panel is now named by its tone, so the chooser listing by sentence ("Shown without syntax
  highlighting ... web dialog") is not the current wiring, and the chooser route for tone-named panels was not heard.
  Sequential navigation not reaching a hover-opened panel is the designed behaviour, not an open defect (see
  FOCUS-3). A populated quota panel, Files-tree failure marks and terminal Escape delivery were not heard in E6.
- File notices: SA-N heard the later SVG fallback and sampled its clear, and noted that the first-paint large-file
  notice, which has no announcement region (no `speak`), stayed silent. It did not hear a later patch fallback, other
  image failure or replacement sequence, nor `SpotNotice`. Its native live politeness, atomicity and relevance were not
  captured; it is not a complete native-attribute record for matching other callers.
- Import result dialog rename: not yet listened to (OPEN-1).
- A `StatusAnnouncer` region whose text is present from the first render at the default delay: heard with focus kept in
  the main window (SA-H), with a menu taking focus (SA-I) and, as a region mounted with each opening of a disclosure,
  inside a dialog with the trigger focused (SA-M); not with a focus move into the pane or under a modal.
- Writes not yet heard, with the reason each is still open (the list is partial; the caller table names the rest):
  `CopyButton`'s 2000 ms automatic clear (the presses used kept the tooltip open); `ActionMenu`'s load-failed update (an
  error alert masked it); `CodeSurface`'s outcome notes and failure, `UnchangedView`'s other outcomes and
  `ViewerContentView`'s disconnected text (not exercised); `AdvancedSection`'s unavailable text (not separately
  recorded); `PermissionsAnnouncer`'s change while Settings is open (no permission was granted or revoked) and Settings
  opened directly onto Permissions (no production control opens it there, so it is untested); `GitView`'s replacement
  sequences and failure sentences (not exercised); `ProjectBrowser`'s search loading (not recorded);
  the account switch outcome in `App` (the credential-free fixture does not produce conversation records to
  switch); `ShellTerminalPanel`'s starting status (not observed populated); the terminal statuses of
  `TerminalPane` and `ShellTerminalView` (see the open items below).
- Terminal statuses, written through `StatusAnnouncer`, not yet listened to: the agent cover texts (loading, starting,
  resuming, reconnecting, disconnected, switching account) and the dormant bar's sentences; the shell tab's trouble and
  exited sentences. Open questions: whether the exited sentence alone, with the Close terminal button outside the
  region, still speaks as PLAIN-2 did with the button inside (accepted by decision); writes made as selecting a session
  or pressing a shell tab moves focus into the terminal match no case (the same focus-move gap as SA-C and SA-I). The
  interrupted dormant bar is staged by the gallery scenario `terminal-not-running-bar`; its sentence is held while the
  "Start on <account>?" question is open and written after it closes, a write after a modal closes that no passing case
  covers directly. The close-terminal confirmation of `ShellTerminalPanel` is not counted in the pane's modal flag
  (rare: the session would have to end during that confirmation).
- A write made while the region is under an open modal outside a marker: heard failing in the login success form
  (KNW-2); no passing case.
- A window visible but not frontmost, and a minimized window with the application also not frontmost (KNW-4's condition,
  not rerun on E4): not tested for SA-D or FOCUS-1.
- A transient web-area utterance before the correct button phrase after a later cancel (FOCUS-1): accepted, not
  explained.
- Last-phrase polling cannot show how much of a status was spoken before a focus move replaced it (SA-K, FOCUS-2).
- Not covered by any case: real login, other agents' login dialogs, languages and appearances other than those recorded,
  other macOS versions.
