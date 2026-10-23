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
  or a caller wait before each text (a timer started at a state change, or `PermissionsAnnouncer`'s attended wait), and
  whether `firstWriteDelayMs` overrides it.
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
  that neither the region nor an ancestor is hidden by `aria-hidden` or `inert` while the real overlay is open (React
  Aria's modal hiding leaves exposed any element, with its ancestors, marked `data-live-announcer="true"` or
  `data-react-aria-top-layer`; a region outside such a marker is hidden by an open modal).
- A statement of which case is matched and, item by item, why each point under "What \"matches a case\" means" holds.

### When a new listening pass is needed

- No passing case matches: one representative pass, then record a case.
- A new implementation that fixes a failing case is also checked against that case's symptom before a passing case is
  recorded. A pattern listed under "Known not to work" bars a match for the aspect it records: a speech pattern differs
  from every passing case in at least one item, so a caller wired like one matches no passing case and needs the pass; a
  pattern that records only a focus destination (KNW-8) leaves the passing case it came from valid for the status write
  alone.
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

Only E4's tree is a durable reference: it equals main `429be4556`, which carries the Settings opener fix `39c1ec3f3` and
the login success announcement fix `429be4556`. E1 to E3 and the build commits behind them were unmerged local-only
builds, so they are not durable references and are not cited by hash. E5's three commits are the fixes as they landed on
main; the build was made from the same changes on the branch before it was rebased onto main.

The login cases used a credential-free fixture standing in for the Grok Build CLI, run through the real daemon's login
coordinator and the real UI. It validates rendering, announcement and recovery; it does not validate real CLI output
formats, authorization, credentials or quota. For exactly `login --device-auth` it sleeps 10 s and emits a reserved URL,
sleeps 10 s and emits a device code, sleeps 10 s, writes an empty auth marker and exits successfully. No browser opens
(the agent opens its own page, which the fixture does not) and nothing is sent over a network. Only the clone-targeted
accessibility, keyboard and pointer routes were used; no real login was performed.

Every `StatusAnnouncer` in the SA cases renders an unnamed `sr-only` `role="status"` with no explicit `aria-live`,
`aria-atomic` or `aria-relevant` props. Its effective native mapping is a group with the application-status subrole
(empty-group subrole while empty), role description "application status", empty own name, title, description and value
(the own value was not sampled for short-lived statuses by the compact native observer), the phrase in a descendant
static text, live polite, atomic true, relevant additions and text. This is the native mapping macOS WebKit supplies,
not a literal HTML serialization. PLAIN-1 and PLAIN-2 are hand-built regions and state their own native mapping.

## Cases

### Summary

| Case | Mechanism | Result |
|---|---|---|
| [NAME-1 Retained picker trigger](#name-1-retained-picker-trigger) | control name read on focus | Pass |
| [NAME-2 Opened picker option](#name-2-opened-picker-option) | option name read on navigation | Pass |
| [NAME-3 Console account row](#name-3-console-account-row) | `aria-describedby` to a hidden name and badge | Pass |
| [NAME-4 Advanced tab](#name-4-advanced-tab) | tab name read on focus | Pass |
| [SA-A Later change, no focus move](#sa-a-later-change-no-focus-move) | `StatusAnnouncer`, exposed beside a modal | Pass |
| [SA-B Outage with a per-appearance gate](#sa-b-outage-with-a-per-appearance-gate) | `StatusAnnouncer` with a caller gate | Pass |
| [SA-C Delayed first write after a dialog opens](#sa-c-delayed-first-write-after-a-dialog-opens) | `StatusAnnouncer` with `firstWriteDelayMs` | Pass |
| [SA-D Written after a dialog closes](#sa-d-written-after-a-dialog-closes) | `StatusAnnouncer` written by a caller timer, gated on window attention | Pass: menu, direct button, return to the window |
| [SA-E Success already present at mount](#sa-e-success-already-present-at-mount) | suppression at remount | Pass |
| [SA-F Hidden status text stays navigable](#sa-f-hidden-status-text-stays-navigable) | plain text in the status after its announcement | Pass |
| [SA-G Confirmation that clears with its tooltip, focus retained](#sa-g-confirmation-that-clears-with-its-tooltip-focus-retained) | `StatusAnnouncer` span, default delay elapsed | Pass, attribution shared with the button's renamed name |
| [SA-H First mount at the default delay, no focus move](#sa-h-first-mount-at-the-default-delay-no-focus-move) | `StatusAnnouncer` whose text is present from the first render, empty until its first write, focus kept | Pass |
| [SA-I Default-delay first mount while a menu takes focus](#sa-i-default-delay-first-mount-while-a-menu-takes-focus) | `StatusAnnouncer` in a submenu | Pass, attribution shared with a visible item |
| [SA-J New child at 1500 ms inside an open dialog, focus retained](#sa-j-new-child-at-1500-ms-inside-an-open-dialog-focus-retained) | `StatusAnnouncer` with `firstWriteDelayMs` in a viewer | Pass |
| [SA-K Loading written, then the pressed control is removed](#sa-k-loading-written-then-the-pressed-control-is-removed) | `StatusAnnouncer` in a diff view | Pass for the status; focus relation recorded per build (E4 fell, E5 restored) |
| [SA-L Section status written after an attended wait, tab focus retained](#sa-l-section-status-written-after-an-attended-wait-tab-focus-retained) | section `StatusAnnouncer` fed by a caller wait | Pass; direct opening untested |
| [SA-M Region mounted with each opening of a disclosure](#sa-m-region-mounted-with-each-opening-of-a-disclosure) | `StatusAnnouncer` mounted per opening | Pass |
| [FOCUS-1 Focus after the login dialog closes](#focus-1-focus-after-the-login-dialog-closes) | explicit focus hand-off | Pass; one transient phrase observed and accepted |
| [FOCUS-2 Focus after an expansion takes the pressed control away](#focus-2-focus-after-an-expansion-takes-the-pressed-control-away) | caller-told focus restore and code-frame hand-off | Pass |
| [PLAIN-1 Text change in a mounted plain status](#plain-1-text-change-in-a-mounted-plain-status) | `role="status"` replaced in place inside a modal | Pass (first build) |
| [PLAIN-2 Persistent visible status with an action](#plain-2-persistent-visible-status-with-an-action) | plain `role="status"` filled in place | Pass |
| [Known not to work](#known-not-to-work) | eight failing or negative observations (KNW-1 to KNW-8; each names the case that replaced it) | **Fail** (history) |
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
  `textValue` set to the option label, followed by ", " and the badge text when it has one, `isDisabled` from the agent option, a child name and `StatusBadge`; the
  select's `disabledBehavior` of selection, so a removed option can be navigated to but not chosen). No explicit option
  `aria-label` and no live region. That the child badge contributes to speech is a verified runtime outcome, not
  something `textValue` alone implies.
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
  `packages/ui/src/settings/AgentAccountsSection.tsx` (`AccountRow` mounts the announcer and `useClearSucceededLogin`),
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
  tested as a reading path.

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

### FOCUS-1 Focus after the login dialog closes

- **Location and state change.** A dialog opened from the row's own Log in button closes, by success or by cancelling.
  The button is shown only while the account reads as signed out and is disabled while a login runs; the menu path opens
  from a menu item instead.
- **Mechanism.** `useLoginFocusReturn` in `packages/ui/src/settings/AgentAccountsSection.tsx`, driven by `AccountRow`
  and the button's `onPress`. After success focus goes to the row's actions button. After a cancel it goes to the Log in
  button once the daemon has answered and the button is enabled again, otherwise to the actions button. The move waits
  one animation frame past React Aria's restore (two after the unmount), acts only when focus is on `<body>` or the
  original button, sets the keyboard modality, and leaves a restore that landed correctly alone. The menu path supplies
  no hand-off, and React Aria returns focus to the menu's trigger.
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

### Known not to work

These patterns were heard failing or heard to leave a gap. Do not reuse them, and do not treat an implementation of
their kind as a match for any case. KNW-6 to KNW-8 were heard on E4 and have since been replaced by passing cases heard
on E5; they stay as history of what the earlier mechanisms did. KNW-8 is the exception in kind: it records the focus
destination of the same recording as the passing case SA-K, whose reuse scope covers the status write and states that
focus relation, so it never barred matching SA-K's status write.

#### KNW-1 A plain status mounted already holding its text

- **What and where.** The same first-form login dialog (E1): the `role="status"` was inserted holding "Logging in…".
- **Result.** Readable manually only. On opening, the automatic output was the dialog name and `Cancel login button`;
  moving the VoiceOver cursor left from Cancel read `Logging in…`. The automatic initial announcement was not observed
  and must not be reported as an automatic pass. That the region arrived populated is a timing lead, not a proven cause.
- **Replaced by.** SA-C, where the same text is spoken automatically.

#### KNW-2 Success written into a durable status while its modal closes

- **What and where.** E1: the account row's `sr-only` wrapper with an inner `role="status"`, empty until success, was
  filled at the moment the state became succeeded, while the login dialog was closing and focus was being returned.
  `AccountLoginAnnouncement` in `packages/ui/src/settings/AccountLogin.tsx` in that build, and `AccountRow` in
  `packages/ui/src/settings/AgentAccountsSection.tsx`.
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
| `settings/AccountLogin.tsx`, `AccountLoginAnnouncer` | login success | SA-D | Heard | Matches in full. |
| `settings/PermissionsSection.tsx`, `PermissionsAnnouncer` | permission states | SA-L for the section opening and a return with the states unchanged; no match for a change while Settings is open | Heard | A grant made in System Settings was not heard, and Settings opened directly onto Permissions is unreachable from production controls and untested. |
| `configuration/ImportReview.tsx` | "apply to all" applied | SA-A | Candidate | Mounted empty in the review step, text written after a press while focus rests, inside the modal's own subtree, default delay. Writing the identical sentence again does not rewrite it. |
| `browser/ChangeList.tsx` | filter no-match | SA-A for typing; no match for a first render holding the text | Candidate | Typing in the filter writes in place at rest. The text can also be present at the first render (a persisted filter, a branch change that closes a popover and returns focus), a mount SA-H does not clearly cover. |
| `viewer/FileViewer.tsx`, `ViewerContentView` | loading, disconnected | SA-J for navigation loading; SA-C for the dialog opening as a candidate; no match for disconnected | Heard for SA-J, candidate for SA-C | The navigation text is written at once with focus on Next file (SA-J). The region is mounted with the dialog with a 1500 ms first delay as focus moves, the shape of SA-C, but the opening was heard on the login dialog only. The disconnected text was not exercised. |
| `viewer/FileViewer.tsx`, `UnchangedView` | loading, read outcome | SA-J for the outcome after navigating; no match for its own loading text or other outcomes | Heard | 1500 ms first delay on a new child inside the open dialog with focus kept, heard for the unchanged outcome only. Its own "Loading…" (written when the read takes longer than the first delay), the failure and other outcomes, and a dialog that opens straight onto the view, were not heard; the `Loading…` heard came from the parent. |
| `components/CopyButton.tsx` | "Copied" | SA-G for the confirmation and the tooltip-driven clear, for a press made while the button has focus; no match for the 2 s automatic clear | Heard | Heard in the main window with focus kept. The 2000 ms branch without a tooltip, a repeat press and a failure were not heard. The copy button in the viewer dialog sets `preventFocusOnPress` (a pointer press does not move focus to it), carries its own "Path copied" text and sits in a dialog, so it is a candidate only for a keyboard press made while it has focus. |
| `browser/GitView.tsx` | git loading, comparing, listing | SA-H for the first mount with focus kept; no match for the Changes-tab mount or replacement sequences | Heard | Heard for the Changes loading at mount with focus on the panel toggle. The tab variant, which moves focus into the pane, captured no sentence, and the comparing and listing texts replacing one another in place were not heard. |
| `browser/ProjectBrowser.tsx` | files loading | SA-H for the load when the Files panel is shown with focus kept; no match for the Files-tab mount or search loading | Candidate | Same hidden `div`, default delay and main window as the representative, but its own Files load finished too fast to sample, switching to the Files tab is the untested focus-move variant, and search-triggered loading was not recorded. |
| `components/ActionMenu.tsx`, `LoadedMenuItems` | menu loading, load failed | SA-I for the loading text; no match for load failed | Heard | Heard with the submenu focused and the git list stretched to 2 s; a visible "Loading…" item says the same word, so attribution is shared. The failure update was masked by an error alert. |
| `viewer/CodeSurface.tsx` | expansion loading and note | SA-K and FOCUS-2 for the loading text and the focus after an expansion; no match for the outcome notes | Heard | Loading heard while the pressed button had focus, and focus restored to the surviving control or the code frame on E5. Notes, failure and capped outcomes were not exercised. |
| `dialogs/LaunchSettingsFields.tsx`, `AdvancedSection` | models loading, unavailable, failed | SA-M for loading and failed; no match for the unavailable text | Heard | Region mounted with each opening outside the folded body, heard for a fresh opening and two reopenings. The unavailable text was not heard separately. |
| `terminal/ShellTerminalPanel.tsx` | shell starting | none | none | Text can be present at the first render at default delay and clears as a terminal appears and takes focus, a write at a focus move; the starting status was not observed populated. |
| `terminal/TerminalPane.tsx`, cover and dormant status | loading, starting, resuming, reconnecting, disconnected and switching covers; the dormant bar's "Not running" or "This session is archived" | SA-A for the writes into the mounted regions (the pane always mounts with no session, so SA-H applies only to test and gallery mounts); no match for a write made as selecting a session moves focus into the terminal, and none for the dormant bar's button | Candidate | Two `StatusAnnouncer` regions mounted with the pane, default delay (browser-observed: regions inserted empty, text written 100 to 250 ms later). The bar's sentence is `aria-hidden` and its button stays reachable, so only the sentence is spoken, not sentence and button as PLAIN-2 heard. The bar's sentence is held while a modal is open (selecting an interrupted session on an incomplete account asks first) and written `AFTER_FOCUS_MOVE_DELAY_MS` after the close, so it is not written under the modal (KNW-2). A sentence already spoken stays in the region unchanged through a modal, so closing it says nothing. Nothing was heard on the pane itself. An archived bar and `SessionHistoryView`'s own region can both hold text while the history loads (different sentences, no duplicate). |
| `terminal/ShellTerminalView.tsx`, trouble and exited status | disconnected, reconnecting and exited, for the shown tab only | close to SA-H for the first write after a keyboard tab switch (focus stays on the tab; the regions are mounted with each showing, but with the longer `AFTER_FOCUS_MOVE_DELAY_MS`), a candidate only; no case matches a pointer switch or a new terminal, where focus moves into the view (open item); SA-A for trouble changes; PLAIN-2 for the exit filling in place while the terminal input keeps focus, with the sentence only | Candidate | Two `StatusAnnouncer` regions per shown tab, empty when mounted (browser-observed); a hidden tab has none, so switching to an exited or troubled tab writes its text after the delay rather than with the region. PLAIN-2 heard the earlier hand-built region holding the sentence and the Close terminal button, spoken as `The shell has exited Close terminal`; the button is now outside the region, so the match is for the sentence only, by decision, and it has to be listened to again before it is claimed. The write after a tab switch made with the pointer comes with a focus move into the view, which no case covers. |
| `viewer/viewerStates.tsx`, `Notice` | the notice's sentence | none | none | The announcing branch is dormant: every call that speaks uses the failed tone, which suppresses its announcer, so no reachable non-failed speaking instance exists. SA-J is the nearest case by delay and exposure if one appears. |
| `viewer/inlineNotice.tsx`, `InlineNotice` | the notice's sentence | none | none | As `Notice`; the only non-failed speaking branch cannot be reached because the daemon refuses files over its limit first. |
| `App.tsx`, account switch outcome | a batch of account switches finished | none | none | Written seconds after the switch dialog has closed, once the batch finishes without failures, and cleared after 10 s. The credential-free fixture does not produce conversation records to switch, so it was not recorded. |

Totals: of 21 callers, 12 were heard for the writes named (`Sidebar`, `ConnectionBanner`, `AccountLoginDialog`,
`AccountLoginAnnouncer`, `PermissionsAnnouncer`, `ViewerContentView`, `UnchangedView`, `CopyButton`, `GitView`,
`ActionMenu`, `CodeSurface`, `AdvancedSection`), 5 are candidates (`ImportReview`, `ChangeList`, `ProjectBrowser`, `TerminalPane`, `ShellTerminalView`) and 4
match no case (`ShellTerminalPanel`, `Notice`, `InlineNotice`, the account switch outcome in `App`). Of the heard callers,
`ConnectionBanner` and `AccountLoginAnnouncer` match for every write they make; the other ten match for some writes
only. Of the candidates, `ImportReview` matches for every write it makes; `ChangeList`, `ProjectBrowser`, `TerminalPane` and `ShellTerminalView` for some.

Hand-built `role="status"` elements, which are not `StatusAnnouncer`:

| Element (file, symbol) | Case | Why |
|---|---|---|
| `WelcomeScreen.tsx` | none | A plain status populated on mount, then replaced in place from "Checking…" to Installed or Not detected when availability arrives. The populated-on-mount shape is the one not spoken for the earlier Permissions rows (KNW-7); not mounted or tested separately. |
| `browser/GitView.tsx`, notices region | none | The "branch deleted" notice was not reached: the review drops a gone selected branch and returns to its own branch. |

## Open items

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
  sequences and notices region (no fixture reached them); `ProjectBrowser`'s search loading (not recorded);
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
  covers directly. The close-terminal confirmation of `ShellTerminalPanel` is not counted in the pane's modal flag (rare: the session would have to end during that confirmation).
- A write made while the region is under an open modal outside a marker: heard failing in the login success form
  (KNW-2); no passing case.
- A window visible but not frontmost, and a minimized window with the application also not frontmost (KNW-4's condition,
  not rerun on E4): not tested for SA-D or FOCUS-1.
- A transient web-area utterance before the correct button phrase after a later cancel (FOCUS-1): accepted, not
  explained.
- Last-phrase polling cannot show how much of a status was spoken before a focus move replaced it (SA-K, FOCUS-2).
- Not covered by any case: real login, other agents' login dialogs, languages and appearances other than those recorded,
  other macOS versions.
