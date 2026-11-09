# VoiceOver verification cases

Real-device VoiceOver results for the packaged macOS application, kept as reusable cases. A new implementation that
matches a passing case below needs no further real-device testing; one that matches none needs one listening pass and a
case of its own. How to build an announcement lives in `docs/ui-standard.md` ("Announcing a change"); this file
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
  or a caller wait before each text (a timer started at a state change, or the attended wait of
  `AttendedStatusAnnouncer`), and whether `firstWriteDelayMs` overrides it.
- **Exposure beside a modal.** Whether the region is exposed at the moment it writes: in the main window while no modal
  is open, inside the modal's own subtree, or kept exposed beside a modal through a `data-live-announcer` or top-layer
  marker. These three count as the same; a region under an open modal without a marker is hidden, and a write into it is
  a different item (see KNW-2).
- **Window attention.** Whether the write is held until the window is attended (visible and focused) or happens whatever
  the window is doing. The window's focus is the web view's own window `focus` and `blur` with `document.hasFocus()`;
  E12 was heard while it came from the shell's native window focus, and the web view's event, measured in the packaged
  macOS application, follows the native one by 1–2 ms with `document.hasFocus()` already updated, a gap far inside
  `AFTER_FOCUS_MOVE_DELAY_MS`, so the two count as the same item. E5 and E10 were heard while `packages/ui/src/input/focusGuard.ts`
  held back every window focus event, a different predicate.
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
  element is left exposed with its ancestors; the rule is in `docs/ui-standard.md`, "Announcing a change").
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
| E2 | An unmerged local build: an earlier main + the Settings opener fix + an earlier login fix | The login success text is written by a caller timer 1500 ms after success, with no window-attention gate and no focus handoff. |
| E3 | An unmerged local build: an earlier main + the Settings opener fix + an intermediate login fix | Adds a focus handoff after a successful close only (not after a cancel); no window-attention gate. |
| E4 | An unmerged local build whose `packages/` and `apps/` tree equals main `429be4556` (fixes `39c1ec3f3` and `429be4556`) | The final product state of the login mechanisms; the same installed build was used for the later recordings of the other status callers (SA-G to SA-K, PLAIN-2, KNW-6 to KNW-8). |
| E5 | Branch `fix/voiceover-recheck-defects` as merged, last fix `42d775745`: main `29d9d1505` + `42b9984f8` (diff-expansion focus) + `e1191fc7c` (Permissions announcer) + `42d775745` (Advanced per-opening announcer) | Heard on the branch build itself, for the cases SA-L, SA-M and FOCUS-2 and for the status in SA-K. |
| E6 | Local main `cd10ab4e7020142bbabec5b052c447f5bf27c368` | Packaged `Octoboard Verification`, identifier and scratch ad-hoc signing identifier `dev.octoboard.verify.vo20261025h7gwl`; real daemon and throwaway files/Git repository. NAME-5, SA-N, FOCUS-3 and ALERT-1; no real agent or quota. |
| E7 | Local main `2d8e1a705888dbe319da3fc2c0e0951d6f9766d5` | Packaged `Octoboard Verification`, identifier and scratch ad-hoc signing identifier `dev.octoboard.verify.vo6-20261025-fyq175xi`; WARN-1 and WARN-2, real daemon and Git data with fixture Codex CLI/records, no Safari or gallery. |
| E8 | Local main `9d199cb42573568a07d618034494a95c75a0a2c7` | Packaged `Octoboard Verification`, identifier and scratch ad-hoc signing identifier `dev.octoboard.verify.vo8-20261025-2j89ms59`; FOCUS-4, bare Error and Note viewer-header marks with real daemon/throwaway files; no agent, browser or gallery. |
| E9 | Branch `fix/startup-error-details` at `f37fc3d925f35eb78f1048ea4767df369a1f6b1d` | Packaged `Octoboard Verification`, identifier and scratch ad-hoc signing identifier `dev.octoboard.verify.vo9-ddnbws96`; WARN-3, WARN-4 and WARN-5, real daemon with isolated HOME/TMPDIR/agent directories, fixture Codex CLI and dated records, throwaway real Git repositories and worktrees; no Safari or gallery. |
| E10 | Local main `287dec4a61d1cc57f4ad1b3c09c8211210320a11`, source of `docs/voiceover-relisten` | Packaged `Octoboard Verification`, build-time identifier and scratch ad-hoc signing identifier `dev.octoboard.verify.vo10-20261027-relisten`; SA-D, SA-E and FOCUS-1 on the current accounts table and account details dialog; real daemon with a credential-free Grok CLI fixture. |
| E11 | An unmerged local wrapper-verification build based on main `9ee45c135` | Packaged `Octoboard Wrapper Verify Fixed`, identifier and scratch signing identifier `dev.octoboard.verify.wrappers.fixed-20261107`; normal production entry, real isolated daemon and credential-free Claude fixture records/processes. FOCUS-5 uses native caption-panel screenshots, not AppleScript speech reads. |
| E12 | Branch `fix/window-focus-refresh` at `7c717c35d` | Packaged `Octoboard Verification`, build-time identifier `dev.octoboard.verify.fr1108b`; SA-L with a human listener, real daemon and throwaway HOME, no agent CLIs. The branch's later commits `dba8e528d` and `2ff384292` change only tests, comments and polling, not the announcer path. |
| E13 | Local main `6e8b8845bf5fd988c4fc72d923e0c1bff4423c5d`, source of `docs/voiceover-removal-git-retry` | Packaged `Octoboard Verification`, build-time identifier and scratch ad-hoc signing identifier `dev.octoboard.verify.vo13-20261109-retry`; SA-O and SA-P, real daemon with isolated HOME/TMPDIR/shell and agent configuration directories, throwaway Git repositories, no agent CLI or login. |
| E14 | Local main `263bfa3f88e77e1f507eacddf2b26380d5ccc5f3`, daemon holder-runtime repair `30eb3e434` | Packaged `Octoboard Recovery Verify`, identifier and ad-hoc signing identifier `dev.octoboard.verify.recovery.vo14`; real isolated daemon/holders, credential-free Claude CLI and real shell processes. Recovery baseline and pending-cover observations. |
| E15 | Local main `263bfa3f88e77e1f507eacddf2b26380d5ccc5f3` (E14's source and daemon) with the UI repairs of `docs/voiceover-results` applied to its UI sources as a patch, before the follow-up changes measured on E17: the recovered mark's handoff waiting two frames from the mark and from its explanation (`shell/TitleBar.tsx`), the terminal ring from react-aria's text-input rule (`terminal/TerminalSurface.tsx`), the shell mark keyed by its terminal (`terminal/shell/ShellTerminalPanel.tsx`) and the recovered sentence deferred while the agent area is inert (`terminal/TerminalPane.tsx`, `App.tsx`) | The same packaged entry as E14, fixing recovered-mark focus handoff, terminal focus indication, shell mark identity and recovered delivery while the agent area is inert. Native rechecks use the repaired production UI. Follow-up changes are measured separately on E17. |
| E16 | UI tree equal to main `8aba9fc5e`: built from `fix/row-axpress-delete` at `28ee0d0dc` in its own detached worktree (`git diff --stat 28ee0d0dc 8aba9fc5e -- packages/ui` is empty) | The same isolated desktop/daemon fixture as E15, with this UI's assets rather than E15's focus repairs. Native AXPress and actual VO-Space on session/project rows, plus the separate unused-session Delete shortcut. |
| E17 | UI from `docs/voiceover-results` at `727a782e705dbb61e9d99655041ffbe1a0b76356` | Rebuilt packaged `Octoboard Recovery Verify`, identifier and ad-hoc signing identifier `dev.octoboard.verify.recovery.vo14`, with this branch's complete production UI via an absolute `frontendDist`. The isolated E14 daemon source, holder-runtime repair and scratch routing/timing/update fixtures are unchanged. Re-listens the follow-up handoff/ring mechanisms and the new held-sentence paths. |

E14, E15 and E16 used macOS 26.2 (25C56), arm64, English application text, Simplified Chinese VoiceOver, and light
appearance (E16's from its own-window screenshots) on 2026-11-10 JST. Reports use native AppleScript `content of last phrase`, requested every 25 ms, with
actual read times, foreground PID and screen-lock checks, plus separately sampled native accessibility trees and
own-window screenshots. Caption visibility was not used as evidence. Existing AppleScript-control, Accessibility and
Automation grants worked outside the restricted execution sandbox; no new grant or setting change was requested.
The scripts started VoiceOver only when it was initially off and tracked ownership for cleanup.

The packaged production entry used a throwaway HOME/TMPDIR/XDG/agent configuration, a real Git project, a
credential-free Claude CLI and real shell/holder processes. An isolated loopback gateway closed only holder WebSockets,
leaving processes alive, and the real reconnect callbacks produced pending and inexact-screen states. For the
under-cover rise variants an explicit verification-only 1000 ms scheduling gate ran the real resumed callback before
clearing pending. That timing fixture is not the production callback order; ordinary-order E15 recordings omitted it.
The UI was not populated by synthetic state events or synthetic VoiceOver `output`. Paths in RCV-1 to RCV-5 are
relative to `packages/ui/src/`.

E15's update cases installed an ephemeral-key-signed localhost archive for version 9.9.9 containing the same
verification executables and a versioned native bundle. A Mach-O sidecar launcher kept its embedded ad-hoc signature
through archive extraction; strict verification of both an extracted archive and the actually installed bundle passed.
The feed's incompatible holder floor caused the genuine restart confirmation. A proxy held the real `handover_check`
request for 900 ms from receipt, below its 2000 ms timeout, then forwarded it unchanged. No positive restart was
accepted. Earlier updater attempts with a shell-script launcher lost its extended-attribute signature in the archive,
failed strict validation and made fresh AX clients receive `AXErrorAPIDisabled`; those are excluded setup failures.
The final update pass used an AX connection established before installation, retained across it, with a persistent
native tree observer. It does not require another permission grant.

E17 used macOS 26.2 (25C56), arm64, English application text, Simplified Chinese VoiceOver and light appearance on
2026-11-10 JST. Its native last-phrase reader, native AX trees/streams and own-window captures use the same procedure
and existing grants as E14–E16. The 1000 ms under-cover callback scheduling fixture was enabled; this round does not
re-certify ordinary callback ordering. The bundle was rebuilt by `cargo-voiceover-relisten` with
`CARGO_BUILD_JOBS=2`, after waiting for other Rust executables; no scripted test suite ran. Strict signature verification
of the extracted local update passed. `relisten-environment.json` records the actual bundle identity/signature and
executable SHA-256 `6937ee800e77ca31ecdd76f72ea8535dbe243747ae36278e6766a6766b626e9e`;
`relisten-ui-assets.sha256.json` records the frontend assets. The local verification toolkit retains the source,
`verify-relisten.conf.json`, build log and `relisten-cases.json` recipes. Recording paths below are under its
`run/relisten-evidence/`, with full date prefix `20261110T`; each directory contains `steps.jsonl`, `phrases.jsonl`,
`ax-stream.jsonl`, numbered native trees and PNGs. These are measured recordings, not synthetic speech or gallery
state. All completed recordings were unlocked and had no phrase-reader errors.

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

E10 used macOS 26.2 (25C56), arm64, English application text, Simplified Chinese system/VoiceOver interface and
light application appearance following System (confirmed from the isolated window's screenshot). Native speech was read
through AppleScript's `content of last phrase`, requested every 25 ms, with native focus sampled every fourth poll;
IPC adds latency. The caption panel was already enabled and was left unchanged; it was not the reader. No synthetic
VoiceOver `output` command was used. Accessibility presses and focus actions targeted the isolated PID; window attention
was set explicitly for attended runs. Command-F5 was the only global key used, with a frontmost-PID check before the
listening toggles. The final off toggle followed a frontmost check of the user's application after the verification app
had exited. The reader was stopped before that final toggle, and VoiceOver's enabled flag was verified off. Volume,
input source and other system preferences were left unchanged. No screen lock was observed during the pass.

The E10 build used `pnpm build:app --bundles app` with only native product-name and identifier overrides, then an
ad-hoc-signed scratch copy on the internal disk. The running daemon's executable path, scratch database/lock/port and
identifier-specific WebKit storage were checked; its executable text section matched the fresh daemon build (whole-file
hashes differ after packaging/signing). A clean launch environment, independent HOME/TMPDIR/ZDOTDIR/XDG and agent
configuration directories, and a shell rc with only the fixture and system executable paths kept real accounts out.
The fixture used the same 10 s / 10 s / 10 s login progression described above, wrote an empty auth marker and answered
quota probes locally; it opened no browser and made no authorization or model request. The table first held the renamed
default account, `VoiceOver fixture`; later runs added `VoiceOver cancel`, `VoiceOver details` and `VoiceOver away`.
The default's table had two rows including the header, and the later table had five. All temporary accounts, processes,
app/data directories and identifier-specific storage were removed after recording; the documentation worktree remains.

E12 differs from the common recording procedure: a human listened with VoiceOver on, and no AppleScript polling,
native samples or caption verification were used, so the phrases are as the listener reported them, not verbatim
captures. The system was macOS 26.2; the interface languages and the application appearance were not recorded. The
build was packaged from the branch's commit `7c717c35d` with its own build-time identifier and ran against the real
daemon with a throwaway HOME and no agent CLIs. The Screen Recording permission was already granted; the Accessibility
grant was made in System Settings by the listener, and the application was reached again with Command-Tab, with no
minimizing and no click.

E13 used macOS 26.2 (25C56), English application text, mixed English/Simplified Chinese VoiceOver output and
light appearance, confirmed from the verification window. It was built with `pnpm build:app --bundles app` and only
the identifier/product-name overrides. In the scratch bundle, a shell wrapper forwarded the real daemon's stdout
through `tee` to a scratch file for control-socket setup; neither the UI nor the daemon's implementation was patched.
The running scratch executable, database, lock, port and identifier-specific WebKit content-rule storage were checked;
the daemon's executable text section matched the fresh build (whole-file hashes differ after ad-hoc signing).
The daemon's login shell used a scratch PATH without Git; its `binary_not_found` status and removal-inspection refusal
were confirmed. For the other failure, Git was made available only in that scratch PATH and the second throwaway
repository's config was deliberately malformed: the real inspection refused with `internal_error`, detail
`fatal: bad config line 1 in file ./config`.

E13 used no human listener or caption reading. Native speech came from AppleScript `content of last phrase`, requested
at 25 ms intervals; the maximum observed sample gap in the five formal recordings was 345 ms, including AppleEvent
latency. A separate native observer, requested at 50 ms intervals, captured focus and effective live-region attributes
and descendant text; its maximum observed gap was 163 ms. The formal recordings lasted 10 seconds each, except the
non-missing-Git tick/untick recording (12 seconds). These are sampled observations, not an audio recording or an exact
measurement of daemon-response-to-write latency; identical consecutive replays remain indistinguishable as described
under "Evidence and its limits". Native trees before/after actions supplied hierarchy and mark/checkbox wiring;
literal DOM IDs and HTML were not extracted. Initial process-targeted CGEvent input attempts did not establish an
action and supplied no listening result. The successful setup used accessibility focus/press actions and global pointer
input, guarded before each input by the verification application's frontmost PID. No synthetic VoiceOver `output`
command was used. The user authorized VoiceOver on/read/off, then instructed continuation after the global-input
question. VoiceOver was initially off, enabled with Command-F5 and restored off with the same shortcut; no permission
or speech preference was changed. Both removals were cancelled. The temporary malformed config was restored before
cleanup; the verification app, its processes, scratch data and identifier-specific storage were removed afterwards.
Raw scratch captures were reviewed for this transcription and are not retained in the repository.

Every `StatusAnnouncer` in the SA cases renders an unnamed `sr-only` `role="status"` with no explicit `aria-live`,
`aria-atomic` or `aria-relevant` props. For E1 to E5 and E10, the captured effective native mapping is a group with the
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
| [SA-D Written after a dialog closes](#sa-d-written-after-a-dialog-closes) | `StatusAnnouncer` written by a caller timer, gated on window attention | Pass: current table menu, direct button, return to the window and account details (E10) |
| [SA-E Success already present at mount](#sa-e-success-already-present-at-mount) | suppression at remount | Pass: current table remount (E10) |
| [SA-F Hidden status text stays navigable](#sa-f-hidden-status-text-stays-navigable) | plain text in the status after its announcement | Pass |
| [SA-G Confirmation that clears with its tooltip, focus retained](#sa-g-confirmation-that-clears-with-its-tooltip-focus-retained) | `StatusAnnouncer` span, default delay elapsed | Pass, attribution shared with the button's renamed name |
| [SA-H First mount at the default delay, no focus move](#sa-h-first-mount-at-the-default-delay-no-focus-move) | `StatusAnnouncer` whose text is present from the first render, empty until its first write, focus kept | Pass |
| [SA-I Default-delay first mount while a menu takes focus](#sa-i-default-delay-first-mount-while-a-menu-takes-focus) | `StatusAnnouncer` in a submenu | Pass, attribution shared with a visible item |
| [SA-J New child at 1500 ms inside an open dialog, focus retained](#sa-j-new-child-at-1500-ms-inside-an-open-dialog-focus-retained) | `StatusAnnouncer` with `firstWriteDelayMs` in a viewer | Pass |
| [SA-K Loading written, then the pressed control is removed](#sa-k-loading-written-then-the-pressed-control-is-removed) | `StatusAnnouncer` in a diff view | Pass for the status; focus relation recorded per build (E4 fell, E5 restored) |
| [SA-L Section status written after an attended wait, tab focus retained](#sa-l-section-status-written-after-an-attended-wait-tab-focus-retained) | section `StatusAnnouncer` fed by a caller wait | Pass; direct opening untested |
| [SA-M Region mounted with each opening of a disclosure](#sa-m-region-mounted-with-each-opening-of-a-disclosure) | `StatusAnnouncer` mounted per opening | Pass |
| [SA-N Viewer notice after paint, then an empty status](#sa-n-viewer-notice-after-paint-then-an-empty-status) | `FileNotice` through `StatusAnnouncer`, then clear; first-paint large-file notice without an announcement region | Pass for the recorded branches and focus handoff; native live attributes not sampled |
| [SA-O Project-removal inspection failure after tick and retry](#sa-o-project-removal-inspection-failure-after-tick-and-retry) | `DelayedAnnouncer` before `StatusAnnouncer`, per-failure wait inside the removal dialog | Pass: missing Git tick/retry; malformed config tick and untick (E13) |
| [SA-P Project Git retry beside a row button](#sa-p-project-git-retry-beside-a-row-button) | `useGitRetryAnnouncement` through `AttendedStatusAnnouncer`, alternating trailing no-break space | Pass: two still-missing-Git retries in the sidebar (E13) |
| [RCV-1 Recovered display after uncover, attention and exposure](#rcv-1-recovered-display-after-uncover-attention-and-exposure) | attended rise through `StatusAnnouncer`, read-time clear | Pass for listed agent/shell variants; baseline inert loss repaired; responsive floating unconfirmed |
| [RCV-2 Recovered mark removed while it or its explanation holds focus](#rcv-2-recovered-mark-removed-while-it-or-its-explanation-holds-focus) | disappearing-source handoff and visible keyboard landing | Pass for separately listed repaired destinations; baseline failures retained |
| [RCV-3 Pending cover entry, delayed sentence and recovery](#rcv-3-pending-cover-entry-delayed-sentence-and-recovery) | named focused cover and conditional attended status | Pass for listed focus/delayed paths; double-reading sequence and immediate modal silence recorded |
| [RCV-4 Restart preflight while keyboard focus stays on the button](#rcv-4-restart-preflight-while-keyboard-focus-stays-on-the-button) | retained pending button, genuine preflight and cancel | Pass for Settings and notice focus/state; exact feedback and busy-word limit recorded |
| [RCV-5 Pointer Restart while terminal input keeps focus](#rcv-5-pointer-restart-while-terminal-input-keeps-focus) | non-focusing pointer press, preflight and cancel | Pass for focus/return; pending speech silent |
| [RCV-6 Pending and no-process sentences held while the agent area is inert](#rcv-6-pending-and-no-process-sentences-held-while-the-agent-area-is-inert) | held status on exposure, focused-cover suppression | Pass for five separately measured maximized-panel variants |
| [ROW-1 Native row activation does not invoke its nested quick action](#row-1-native-row-activation-does-not-invoke-its-nested-quick-action) | hidden first native press target in composite row | Pass: native AXPress and VO-Space for unused/used session and project; Delete pointer works, Space not certified |
| [FOCUS-1 Focus after the login dialog closes](#focus-1-focus-after-the-login-dialog-closes) | explicit focus handoff | Pass: current table and account details; transient web-area phrase still observed (E10) |
| [FOCUS-2 Focus after an expansion takes the pressed control away](#focus-2-focus-after-an-expansion-takes-the-pressed-control-away) | caller-told focus restore and code-frame handoff | Pass |
| [FOCUS-3 Status mark and quota popover focus and reading paths](#focus-3-status-mark-and-quota-popover-focus-and-reading-paths) | shared hover/press popover, focus and item chooser | Pass for quota Enter/Escape focus paths, the mark's focus return on Escape and item-chooser reachability; the bare-mark Enter naming failed, see KNW-9 and FOCUS-4 |
| [FOCUS-4 Bare mark dialog named by its tone](#focus-4-bare-mark-dialog-named-by-its-tone) | tone-named bare mark dialog; sentence remains the trigger name | Pass for Enter speech without the repeated sentence and Escape focus return, Error and Note |
| [FOCUS-5 Busy dialog hands focus to its pending button and returns after refusal](#focus-5-busy-dialog-hands-focus-to-its-pending-button-and-returns-after-refusal) | busy context, layout-effect handoff, nested error acknowledgement and explicit input return | Pass by native caption verification, SessionDialog Enter submission |
| [PLAIN-1 Text change in a mounted plain status](#plain-1-text-change-in-a-mounted-plain-status) | `role="status"` replaced in place inside a modal | Pass (first build) |
| [PLAIN-2 Persistent visible status with an action](#plain-2-persistent-visible-status-with-an-action) | plain `role="status"` filled in place | Pass |
| [ALERT-1 A conflicted file read fails](#alert-1-a-conflicted-file-read-fails) | delayed `PaneFailure` alert inside the viewer | Pass for file-not-found |
| [WARN-1 Deletion warning order and checked option descriptions](#warn-1-deletion-warning-order-and-checked-option-descriptions) | warning title, session list and consequence text; checkbox name and hint | Pass (history; superseded by WARN-3) |
| [WARN-2 Trust prompt caution prefix and description](#warn-2-trust-prompt-caution-prefix-and-description) | hidden caution prefix, visible title, then agent-specific text | Pass in packaged app with fixture Codex CLI (history; superseded by WARN-5) |
| [WARN-3 Deletion warning modal with option notes outside the checkbox](#warn-3-deletion-warning-modal-with-option-notes-outside-the-checkbox) | warning modal: plain body, session list and consequences; checkbox name and an external note through `aria-describedby` | Pass for focus and sequential reading; opening announcement unconfirmed |
| [WARN-4 Remove project with running sessions and a lookup-failure mark](#warn-4-remove-project-with-running-sessions-and-a-lookup-failure-mark) | warning modal with running sessions; external note; a separate mark after the checkbox | Pass |
| [WARN-5 Trust prompt caution as plain body text](#warn-5-trust-prompt-caution-as-plain-body-text) | the caution as one plain paragraph, no prefix or title | Pass for sequential reading; opening announcement unconfirmed |
| [Known not to work](#known-not-to-work) | twelve failing or negative observations (KNW-1 to KNW-12; each names the case that replaced it) | **Fail** (history) |
| [OPEN-1 Import result dialog rename](#open-1-import-result-dialog-rename) | focus move onto a renamed dialog | Unconfirmed |
| [OPEN-2 Startup failure page alert](#open-2-startup-failure-page-alert) | delayed `EmptyPanel` alert on a page with no focus move | Unconfirmed |

### NAME-1 Retained picker trigger

- **Location and state change.** Console settings, Default agent, "Console sessions". A saved default of Codex, a named
  Codex account and a supported non-default console effort, then Codex removed from the added agents, then the editor
  reopened. The stored selection stays visible and is badged "Not added".
- **Mechanism.** The accessible name and description of the trigger, read when it takes focus. No live region and no
  write delay. `packages/ui/src/components/form/OptionSelect.tsx` (`OptionSelect`: the select's `aria-label`, optional
  `aria-labelledby` and descriptions, and the selected value rendered with `StatusBadge`), fed by
  `packages/ui/src/dialogs/ConsoleDialog.tsx` (label "Console sessions", hidden label, stored default agent) and the
  agent option list in `packages/ui/src/agents/agents.ts` (held agents are retained and badged "Not added").
  `packages/ui/src/components/status/StatusBadge.tsx` adds an `sr-only` comma and space before the visible "Not added"
  text; it has no role, live attribute or explicit label. The agent icon is `aria-hidden`.
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
- **Mechanism.** Option name read on navigation. `packages/ui/src/components/form/OptionSelect.tsx` (the list item with
  its `textValue` set to the option label, followed by ", " and the badge text when it has one, `isDisabled` from the
  agent option, a child name and `StatusBadge`; the select's `disabledBehavior` of selection, so a removed option can be
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
  the agent is marked "Not added"), and `packages/ui/src/components/form/OptionSelect.tsx` (`describedBy` becomes
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
  This is not `packages/ui/src/components/tabs/AgentTabList.tsx`.
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
  `packages/ui/src/sidebar/ConsoleView.tsx` (the no-match text, derived from projects existing and none matching the filter)
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
  `packages/ui/src/shell/ConnectionBanner.tsx` (`ConnectionBanner`: `ANNOUNCEMENT_DELAY_MS`, 100 ms, withholds the text
  from the announcer for that long after the banner becomes visible, re-armed per outage; the announcer sits in an
  element marked `data-react-aria-top-layer`), and `packages/ui/src/components/announce/StatusAnnouncer.tsx`
  (`StatusAnnouncer`, whose own first write delay applies once per mount). The visible strip text and icons are
  `aria-hidden`; no nested status or alert.
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
  `AFTER_FOCUS_MOVE_DELAY_MS` (1500 ms). `packages/ui/src/settings/accounts/AccountLoginDialog.tsx`
  (`AccountLoginDialog`: the visible spinner and the visible progress or code line are `aria-hidden`; the sibling
  announcer receives the plain translated text, the code inside isolation marks, and `undefined` while the login is not
  active), and `packages/ui/src/components/announce/StatusAnnouncer.tsx`. The first text is written once after the
  delay; later changes are written at once. No nested status or alert; focus rests on Cancel login (`autoFocus`)
  throughout.
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
  and success while the window is minimized followed by return. E10 also exercises the Log in button in the account
  details dialog, which remains open under the login dialog.
- **Mechanism.** A row-lifetime `StatusAnnouncer` written by a caller timer.
  `packages/ui/src/settings/accounts/AccountLogin.tsx` (`AccountLoginAnnouncer`: on a success that was not already there
  at mount it waits `AFTER_FOCUS_MOVE_DELAY_MS` and only while the window is attended, restarting the wait on each
  return, then gives the announcer the account-named text; `useClearSucceededLogin` dismisses the success after
  `SUCCESS_SHOWN_MS`, 6000 ms of attended time without a break),
  `packages/ui/src/components/announce/useWindowAttended.ts` (`useWindowAttended`: document visible and window focused),
  `packages/ui/src/settings/accounts/AccountsTable.tsx` (`AccountTableRow` mounts the announcer and
  `useClearSucceededLogin`), and `StatusAnnouncer` with its default 100 ms already elapsed (the timer is the caller's,
  not `firstWriteDelayMs`). The dialog closes on success, hiding its own progress status.
- **ARIA wiring as rendered.** The common mapping; empty before success, populated with the account text in descendants
  after. The text is `⁨VoiceOver fixture⁩: logged in.` with isolation marks around the account name.
- **Earlier result (E4).** Spoken automatically, once, in all three paths. After the Settings and Actions-button focus
  phrase (`Settings web dialog 包含3个项目 Actions for ⁨VoiceOver fixture⁩ (Grok Build) More actions 菜单弹出 按钮 Grok Build tab
  panel`), `⁨VoiceOver fixture⁩: logged in.` followed about 1.5 s later. Direct button path: the same two phrases; a
  transient `Octoboard 网页内容` and a no-action button phrase may precede the final Actions phrase and are not a failure of
  the announcement. Away path: success while the window was minimized produced no speech while away; on the first return
  the announcement came about 1.5 s after the Actions focus phrase; a second minimize and return produced no second
  announcement, and the success cleared 6.0 s of attended time after the second return.
- **Capture.** Last phrase through AppleScript, 25 ms polling, with native focus and window state (screen unlocked,
  application frontmost, window minimized) sampled alongside; the daemon's success event timestamped independently.
- **Environment.** E10 for the current table and details dialog; E4 for the earlier row. The direct-button and menu
  paths were also heard in E3 and the menu path in E2.
- **Date.** 2026-10-20; rechecked 2026-10-27.
- **Reuse scope.** Covers a durable `StatusAnnouncer` in a row that is hidden by a modal while it is open, written by a
  caller timer that starts at the success (once the window is attended) and runs 1500 ms, the modal closing and focus
  returning about two frames after the success, so both come before the write, in WKWebView on macOS. Window states
  tested: attended, and minimized with the application otherwise frontmost. Not tested: a window visible but not
  frontmost, a different attention predicate, another agent, real login, other languages or appearances. A caller
  without the attention gate, or with a write at the close, is a known-not-to-work pattern below.
- **Current result (E10).** Spoken automatically through the current table's direct button and Actions menu: `⁨VoiceOver
  fixture⁩: logged in.` The direct path first said `Octoboard 网页内容`, then `Settings web dialog 包含3个项目 Actions for
  ⁨VoiceOver fixture⁩ (Grok Build) More actions 菜单弹出 按钮 Grok Build accounts table 5列，2行`. The success phrase followed
  about 1.5 s later; in the menu run it followed about 0.9 s after the final focus phrase (about 1.5 s after the
  daemon's success). Minimized with the application still frontmost: no success phrase while away; after restoring, the
  Actions phrase named the five-row table and `⁨VoiceOver away⁩: logged in.` followed about 1.3 s later. A second
  minimize/restore gave no second success phrase, and the native region was empty after seven seconds back. In account
  details, after returning to the sharing switch (FOCUS-1), `⁨VoiceOver details⁩: logged in.` followed about 1.5 s
  later, once; the underlying table row was absent from the exposed native tree, and only the details region held the
  sentence. This does not establish counts of identical consecutive native speech replays beyond the last-phrase polling
  limit.
- **Current wiring and reuse scope.** The table announcer is inside the Account row-header cell; the Log in control is
  in the Status cell. The details announcer is inside the still-open Account settings modal. Both native status
  regions had the common mapping, including polite, atomic true and additions/text relevance, with empty own
  title/description/value and the sentence in a static-text child. Source wiring is `role="status"`, `sr-only`, with
  no explicit live/atomic/relevant props; generated HTML ID references were not extracted. The table's region was
  mounted empty before login, hidden under the login modal and written after its close; the details region was exposed
  after the nested login modal closed. This confirms the timer/attention mechanism on the current layout and covers
  the distinct row-Actions and details-sharing-switch focus destinations. The earlier scope's exclusions still apply;
  the visible-but-background window and a minimized application that is also not frontmost were not tested.

### SA-E Success already present at mount

- **Earlier location and state change (E4).** After a success, Settings General then Agent accounts then the Claude Code
  tab then the Grok Build tab are shown within six seconds, remounting the account row.
- **Mechanism.** `AccountLoginAnnouncer` in `packages/ui/src/settings/accounts/AccountLogin.tsx` initializes from the
  success present when the row mounts and does not write it; it clears that hold when the state next leaves success.
- **ARIA wiring as rendered.** The common mapping, empty after the remount.
- **Earlier result (E4).** Not spoken, as intended: only the tab-selection phrases were captured; no second logged-in
  phrase through 10 s or more after the success. The original announcement count stayed one.
- **Capture.** Last phrase through AppleScript, with native samples of the empty region.
- **Environment.** E10 for the current table; E4 (and E2, E3 with an earlier form of the same suppression) for the
  earlier row.
- **Date.** 2026-10-20; rechecked 2026-10-27.
- **Reuse scope.** Covers suppressing a state that is already present when the announcer's row mounts. It is evidence
  for silence on remount only alongside SA-D's positive result.
- **Current location and result (E10).** About 2.5 s after the menu-path success, while the row still held its success
  text, General then Agent accounts were pressed, remounting the table. The native status region was empty on return
  and after another 11 s. Only the selection phrases, `General 已选择 tab，第1项（共10项） Settings sections tab group` and
  `Agent accounts 已选择 tab，第3项（共10项）`, followed by VoiceOver's tab hint, were heard; no second logged-in sentence
  occurred. SA-D supplies the positive announcement immediately before this suppression check. The current pass did
  not switch agent tabs or reopen account details during success; it confirms row-remount suppression through the
  Settings section switch, with the same empty-region source wiring and native mapping recorded by SA-D.

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
  `packages/ui/src/components/controls/CopyButton.tsx` (`CopyButton`: the confirmation text defaults to "Copied"; the
  clear is driven by the tooltip lifecycle, `COPIED_HOLD_MS` of 2000 ms applying only while no tooltip is open, and 150
  ms after the tooltip closes), reached through `packages/ui/src/project-browser/git/GitView.tsx` (`Notice`). The
  default 100 ms first write delay had long elapsed, so the write is immediate. Main window, no modal; the check icon is
  `aria-hidden`.
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
  while the button had focus; the viewer's copy button sets `focusOnPress={false}` and is not covered by that relation.

### SA-H First mount at the default delay, no focus move

- **Location and state change.** Hiding and re-showing the Files panel, with focus left on the panel toggle, mounts the
  Changes view again: it loads (`Loading changes…`) and the loading clears.
- **Mechanism.** A `StatusAnnouncer` mounted with the Changes view, its text present from the first render and written
  after the default 100 ms first write delay. `packages/ui/src/project-browser/git/GitView.tsx` (`GitView`; the card and
  its content are recreated by the panel stack when the panel is shown). The visible loading placeholder is
  `aria-hidden`. Main window, no modal; focus stays on the toggle, which is renamed from "Show files panel" to "Hide
  files panel".
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
  `packages/ui/src/components/menus/MenuItems.tsx` (`LoadedMenuItems`); the caller is the project menu's async branch
  list in `packages/ui/src/sidebar/menus.tsx`. The visible disabled "Loading…" menu item is not `aria-hidden`. The
  popover contains the region, so it is exposed. The git request was stretched to 2 s by a clone-only wrapper to make
  the load observable; this is not natural latency.
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
- **Mechanism.** Two `StatusAnnouncer`s. `packages/ui/src/viewer/ViewerContentView.tsx` (`ViewerContentView`: mounted
  with the dialog, its 1500 ms first delay long elapsed, so the navigation text is written at once) and
  `packages/ui/src/viewer/ChangeView.tsx` (`UnchangedView`: mounted with the new child, `firstWriteDelayMs` of 1500).
  The visible final notice is `aria-hidden`. The Next file button keeps focus; no dialog opens.
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
  `packages/ui/src/viewer/code/CodeSurface.tsx` (`DiffSurface`: the expansion callback sets the loading text at once; the
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
  as ordinary visible copy, not `aria-hidden` and with no `role="status"`, and the permission names with their states go
  to the announcer as one text), `packages/ui/src/components/announce/AttendedStatusAnnouncer.tsx` (writes each text
  after `AFTER_FOCUS_MOVE_DELAY_MS` of attended time, restarting the wait on each return to the window, and writes
  nothing again while the text has not changed; heard as a local `PermissionsAnnouncer` holding the same code),
  `packages/ui/src/components/announce/useWindowAttended.ts` (it takes the window's focus through
  `packages/ui/src/components/hooks/windowFocus.ts`; heard while that came from the shell's native window focus, which
  the web view's own event follows by 1–2 ms, see "Window attention") and `StatusAnnouncer` with its default 100 ms long elapsed (the wait is the caller's, not
  `firstWriteDelayMs`). Inside Settings' own dialog; the Permissions tab keeps focus. The
  wording heard below predates the current catalog sentences (one message of whole sentences per permission and state,
  such as "Screen Recording is not granted."): the same composition, a name and a state per permission in one text, with
  the timing mechanism unchanged.
- **ARIA wiring as rendered.** The common mapping; empty when the tab was focused, populated 1.5 s later with the
  sentence in a descendant. A single status holds both permissions; the visible row copy stays separately readable, so
  the states are met twice by a reader going through the rows.
- **Result.** Spoken automatically, once: after the tab phrase (`Permissions 已选择 tab，第5项（共7项）`), `Screen Recording Not
  granted. Accessibility Not granted.` followed 1.73 s later; the native status was written 1.50 s after native focus on
  the tab. Returning from a minimized window with the states unchanged produced the window phrase and the tab phrase and
  no repeat of the sentence in 16 s. No permission was granted, refused or changed.
  E12, heard by a listener: with Settings > Permissions open and Screen Recording already granted, Grant on the
  Accessibility row was pressed, the application was enabled in System Settings > Accessibility, and Command-Tab
  returned to the still-visible window. VoiceOver first read the focused control, then after a short pause announced the
  section status, Screen Recording granted and Accessibility granted. Command-Tab away and back with nothing changed read
  only the focused control; the status was not announced again.
- **Capture.** E5: last phrase through AppleScript, 25 ms polling, with native focus, status and window samples about
  every 40 ms; the minimized window was restored with no other application activated. E12: a listener's report, no
  capture tooling.
- **Environment.** E5 and E12.
- **Date.** 2026-10-20 (E5), 2026-11-08 (E12).
- **Reuse scope.** Covers a section-level `StatusAnnouncer` fed by a caller that waits `AFTER_FOCUS_MOVE_DELAY_MS` of
  attended time, in a section shown by a tab focus move inside an already-open dialog, with the durable copy left
  readable. E12 adds a change of the states made while Settings is open (the Accessibility grant in System Settings),
  announced after a Command-Tab return with the window still visible, and no repeat on an unchanged return by the same
  path; the earlier minimize-and-restore return was heard on the DOM-based predicate only. Not heard: a first reading
  that arrives while the section is already showing, not at the tab focus move, a denied or revoked permission, a
  return by a route other than Command-Tab and minimize-restore, and Settings opened directly onto
  Permissions, which no production control does today, so it is untested rather than failing. The rows' own copy was not
  tested as a reading path.


### SA-M Region mounted with each opening of a disclosure

- **Location and state change.** The launch dialog's Advanced section with Grok Build chosen (a credential-free CLI
  fixture whose model listing fails after 3 s): expanding the section starts the model request and its failure is
  announced; collapsing and reopening the section keeps the answer and requests again.
- **Mechanism.** A `StatusAnnouncer` mounted outside the folded body and only while the section is expanded.
  `packages/ui/src/components/form/AdvancedSection.tsx` (`AdvancedSection`) with
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
- **Mechanism.** `packages/ui/src/viewer/BodyView.tsx` (`ImageView`) raises a failed `FileNotice` with `speak` when
  image decoding fails. `packages/ui/src/viewer/notices.tsx` (`FileNotice`, `SpokenSentence`) portals the mark to the
  header and mounts a `StatusAnnouncer` beside the content, inside the open viewer dialog. The region starts empty, has
  `firstWriteDelayMs` of 1500 and `clearAfterRead`; source `READ_TIME_MS` is 5000 after writing.
  `packages/ui/src/viewer/code/CodeSurface.tsx` (`CodeSurface`) supplies no `speak` for the large-file notice, so that branch
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
- **Reuse scope.** Covers the later `FileNotice` SVG fallback with this focus handoff relation, its clear and
  subsequent reading path. The first-paint large-file notice, which has no announcement region (`CodeSurface` passes no
  `speak`), stayed silent; that is context, not a covered write. SA-J remains the retained-focus case; it does not by
  itself establish this clear. Other image failures, patch fallbacks and notice replacements were not heard.
  `SpotNotice` has not been listened to; the shared `SpokenSentence` is a candidate only when all matching conditions
  hold. No general match is claimed for callers whose native live attributes were not captured here.

### SA-O Project-removal inspection failure after tick and retry

- **Location and state change.** Remove project, with no sessions: tick **Also move the project folder to the Trash**
  while the verification daemon cannot find Git. After the sentence is announced, open **Other worktrees unknown**,
  focus **Try again** and press it, still without Git. In a second throwaway project, make Git available but leave its
  config malformed, open Remove project, tick Trash, wait for the sentence, then untick. Cancel both removals.
- **Mechanism.** `packages/ui/src/dialogs/BranchCleanupOptions.tsx`, `DelayedAnnouncer`, mounted by
  `useProjectRemovalCleanup`, feeding `packages/ui/src/components/announce/StatusAnnouncer.tsx`, `StatusAnnouncer`.
  The region mounts empty with the Trash option and remains for the dialog's lifetime. Every failure waits
  `AFTER_FOCUS_MOVE_DELAY_MS` (1500 ms) before writing; the first-write gate of `StatusAnnouncer` has already elapsed.
  There is no attention gate or `clearAfterRead`. A pending retry clears the text, allowing the same sentence to be
  written again; unticking clears it too. The retry closes the mark's popover and restores focus to the mark.
- **ARIA wiring as rendered.** The dialog was an `AXGroup` with `AXApplicationAlertDialog`, title/description
  `Remove project`, live assertive, atomic 0, relevant additions text. The status was a separate unnamed `AXGroup`
  inside that modal's form/project-folder subtree, outside the checkbox and the mark: empty own title, description,
  help and value; role description `application status`, polite, atomic 1, relevant `additions text`. It changed from
  `AXEmptyGroup` to `AXApplicationStatus` with the full sentence in descendant static text. No nested status or alert
  was observed in that region. Source wiring is the unnamed, `sr-only` `div role="status"` with no explicit live,
  atomic or relevant props; native values are WebKit's mapping, not literal HTML. The checkbox's title stayed
  `Also move the project folder to the Trash`, with value 0/1 and empty own description/help. The separate button
  `Other worktrees unknown` followed its label and preceded its hint; the sentence did not join the checkbox name.
  The missing-Git mark used `RetryStatusMark` and offered Download Git/Try again; the malformed-config mark used the
  plain `StatusMark`.
- **Result.** Pass for all three failure writes. The exact automatic phrase was:

  ```text
  Worktrees of the repository outside the project folder may be moved to the Trash with it, and its branches deleted permanently.
  ```

  It appeared once in last-phrase changes after the missing-Git tick, once again after its failed retry, and once
  after the malformed-config tick, each corroborated by one native status-text transition. These were automatic
  announcements without moving the VoiceOver cursor onto the sentence. Focus stayed on the checked checkbox for
  each initial write; on retry it passed briefly through the web area and returned to `Other worktrees unknown`
  before the delayed sentence. Approximate sampled gaps from final focus/check-state observation to native text were
  1.53 s (missing-Git tick), 1.23 s (retry mark restoration) and 1.51 s (malformed-config tick); these are not timings
  from the daemon's reply. Unticking the malformed-config case returned the checkbox to 0 and emptied the status;
  no further risk sentence appeared during the remaining approximately six seconds.
- **Other automatic speech.** Before the delayed sentence, the dialog also announced its changed consequence and
  the mark's name twice. The malformed-config capture read exactly:

  ```text
  Its sessions are deleted with it, and its folder is moved to the Trash. Other worktrees unknown Other worktrees unknown
  ```

  This was a separate earlier utterance, not a second reading of the risk sentence. The missing-Git capture had the
  same words with one leading space. The mark's press-opened explanation was setup for retry, not evidence for the
  automatic status announcement.
- **Capture.** E13 AppleScript speech polling and separate native focus/live-region/text observation, plus settled
  native trees. The real daemon refused the first inspection with `binary_not_found` for `git` and the malformed
  one with `internal_error`; no fabricated daemon reply was used.
- **Environment.** E13.
- **Date.** 2026-11-09.
- **Reuse scope.** Covers the per-failure delayed, empty-first status inside this removal modal, both missing-Git and
  other inspection failures, same-words repetition after a pending retry, and clear on untick, with the recorded
  checkbox/mark focus relation. It does not cover a background window, dialog closure during the delay, rapid retries
  before a write, another language, other cleanup warnings, or executing the removal. It supplements WARN-4's mark
  reading; it does not recheck WARN-4's running-session layout.

### SA-P Project Git retry beside a row button

- **Location and state change.** In the sidebar, open the branch badge's Git-missing mark for **Missing Git
  verification**, focus **Try again** and press it. Wait for the announcement and automatic clear, then open the
  mark and repeat, with Git still absent from the verification daemon's PATH. The real daemon's project status was
  `repository: true`, `activity: idle`, `error_code: binary_not_found`.
- **Mechanism.** `packages/ui/src/sidebar/useGitRetryAnnouncement.tsx`, `useGitRetryAnnouncement`, mounted by
  `packages/ui/src/sidebar/ProjectNode.tsx`, `ProjectNode`, through
  `packages/ui/src/components/announce/AttendedStatusAnnouncer.tsx`, `AttendedStatusAnnouncer`, and
  `packages/ui/src/components/announce/StatusAnnouncer.tsx`, `StatusAnnouncer`. One region remains mounted empty for
  the project placement's lifetime, as a sibling after `TreeRow`, outside its `role="button"`. The request's answer
  feeds a 1500 ms attended wait for every retry, with `clearAfterRead` (5000 ms). The second failing answer appends
  U+00A0, a no-break space, making a new text without changing the words.
- **ARIA wiring as rendered.** Before the action, the unnamed region was exposed as a sibling of the project
  `AXButton`, not its child: `AXGroup`, role description `application status`, `AXEmptyGroup`, empty own title,
  description, help and value, polite, atomic 1, relevant `additions text`. Each write changed its subrole to
  `AXApplicationStatus`, with the phrase in descendant static text; the sampled own attributes stayed the same.
  Source wiring is the `sr-only` `div role="status"` with no explicit live/atomic/relevant props. No enclosing modal
  or nested status/alert affected the write. The project button's title/description was
  `Missing Git verification, Git check failed`. The popover was a separate native web dialog named `Git check failed`;
  its recovery button was `Try again`. Mark icons were decorative in source; no separate icon text was present in
  the sampled native row/region. This records native effective attributes, not a literal HTML extraction.
- **Result.** Pass. Each retry returned focus from Try again to the project row before the delayed automatic status.
  The first speech sequence was exactly:

  ```json
  [
    "Try again button Git check failed web dialog",
    "Missing Git verification, Git check failed 已折叠 button Sessions navigation",
    "Git check failed"
  ]
  ```

  The second sequence had the same first two phrases, then:

  ```json
  "Git check failed\u00a0"
  ```

  The final phrase in each sequence was a separate automatic utterance, not the row's focus name or a manual
  VoiceOver cursor read. Each appeared once in last-phrase changes, with one native write. Native text contained no
  trailing space on the first write and U+00A0 on the second. Sampled row-focus-to-write gaps were approximately
  1.48 s and 1.49 s. Each region emptied approximately 5.0 s after its write while focus remained on the row.
- **Capture.** E13 AppleScript speech polling, native live-region/text/focus sampling and settled native trees. Each
  formal retry recording lasted 10 seconds; the two retries were separated by the first write and clear.
- **Environment.** E13.
- **Date.** 2026-11-09.
- **Reuse scope.** Covers the sidebar's durable sibling-of-row-button placement, a still-missing-Git request answer,
  return of focus to that row, the attended delay and alternating-space repeat, and automatic clear. The window stayed
  visible and frontmost. It does not establish the focus-mode header/group placements, leaving/returning to the window,
  switching projects during a request, timeout/success, or multiple answers arriving within one wait.

### FOCUS-1 Focus after the login dialog closes

- **Location and state change.** A dialog opened from the row's own Log in button closes, by success or by cancelling.
  The current table's Status-cell button is shown only while the account reads as signed out, stays focusable and is
  `aria-disabled` while a login runs; the menu path opens from a menu item instead. Account details offers a second
  direct button, with the details modal remaining open beneath the login dialog.
- **Mechanism.** `useLoginFocusReturn` in `packages/ui/src/settings/accounts/AccountLoginButton.tsx`
  (`AccountLoginButton`), driven by the button's `onPress`; `AccountTableRow` passes the row's actions button as the
  fallback, and `AccountDetailsDialog` passes its first `input[role="switch"]`. After success focus goes to the row's
  actions button or the details dialog's first switch, as applicable. After a cancel it goes to the Log in button once
  the daemon has answered and the button is available again, otherwise to the configured fallback. The move waits one
  animation frame past React Aria's restore (two after the unmount), acts only when focus is on `<body>` or inside the
  original button, sets the keyboard modality, and leaves a restore that landed correctly alone. The current menu path
  supplies an explicit handoff to the row's Actions button once the dialog has gone (`AgentAccountsSection.tsx`,
  `handFocusOnceClosed` with `rowActionsTrigger`).
- **Earlier ARIA wiring as rendered (E4).** The row's Actions control is a pop-up button named `Actions for ⁨VoiceOver
  fixture⁩ (Grok Build)` with has-popup set; the direct button is a button named "Log in". Final screenshots show the
  keyboard focus ring on the correct control.
- **Earlier result (E4).** Success, direct button: focus on the Actions button, then the SA-D announcement. Cancel after
  the login had started: focus on the Log in button (`Settings web dialog 包含3个项目 Log in button Grok Build tab panel`),
  enabled and focused for 9 s or more. Cancel before the daemon had registered the login (the start request delayed 3.5
  s by a temporary proxy): focus on the Log in button at once, and it stayed there after the late start and cancel.
  Observed and accepted: in the cancel-after-start run a generic `Octoboard 网页内容` phrase was spoken about 99 ms before
  the correct button phrase, so a transient web-area utterance is not excluded; the correct target is spoken immediately
  after. Intermediate disabled states shorter than the sample resolution were not observed.
- **Capture.** Last phrase through AppleScript; native focus sampled at 40 ms in E4, every fourth requested 25 ms
  phrase poll in E10 (about 100 ms plus IPC latency).
- **Environment.** E10 for the current table and account details; E4 for the earlier row. E3 recorded the earlier
  cancel failure (KNW-5) and E2 the lost focus after a success (KNW-3).
- **Date.** 2026-10-20; rechecked 2026-10-27.
- **Reuse scope.** Covers the focus destination for a dialog whose opener may be disabled or gone when it closes, as
  `useLoginFocusReturn` does it. Not tested: a window visible but not frontmost, a different opener.
- **Current result and wiring (E10).** The table's success path focused the row's named Actions pop-up button and
  spoke its table context before SA-D's success sentence. Cancelling after the login started returned focus to the
  enabled direct button for at least 9 s: `Octoboard 网页内容` was followed about 63 ms later by
  `Settings web dialog 包含3个项目 Log in to ⁨VoiceOver cancel⁩ Log in button Grok Build accounts table 5列，5行`.
  The button's source accessible name is now `Log in to ⁨VoiceOver cancel⁩`, with a decorative `aria-hidden` icon;
  while active it has `aria-disabled`, with `aria-describedby` pointing to its hidden `Logging in…` span. The native
  start phrase `Log in to ⁨VoiceOver cancel⁩ Logging in… 变暗 button Grok Build accounts table` captured that state.
  In account details, cancel returned to the enabled Log in button:
  `Account settings web dialog 包含19个项目 Log in to ⁨VoiceOver details⁩ Log in button`, again after a transient web-area
  phrase. Success focused the first switch instead:
  `Account settings web dialog 包含19个项目 Share default configuration for ⁨VoiceOver details⁩ 关 开关`.
  Native focus identified that switch as `AXCheckBox` / `AXSwitch`, value 0 and enabled; no switch was toggled. The
  table opener's name and Actions pop-up name were captured natively; exact generated description IDs were not.
- **Current reuse scope.** Confirms the current focusable, aria-disabled opener for a normal started login and its
  cancel, the row fallback after success, and the details modal's first-switch fallback after success. The transient
  web-area phrase remains observed. The E4 artificially delayed start/cancel race was not rerun on this table and
  remains historical evidence only; the current pass does not cover that race, another opener, an unattended window
  or the default account's details fallback (its first switch is Disable account rather than Share configuration).

### FOCUS-2 Focus after an expansion takes the pressed control away

- **Location and state change.** The viewer's diff of a 180-line file with one modified line: "Show all 86 unmodified
  lines" is pressed by an accessibility press (VoiceOver presses with a click), then the remaining "Show whole file".
- **Mechanism.** `packages/ui/src/viewer/code/expansion.ts` (`Expansion.activate`: told whether the pressed control had
  focus, decided in the click handler from the focused element and the event path; a mouse press takes no focus), with
  `packages/ui/src/viewer/code/rendererDom.ts` (`refocusControl`, `focusControl`, `handFocusToCode`: the redraw restores the
  same or the nearest surviving control, and with none left hands focus to the named code frame with the keyboard
  modality) and `packages/ui/src/viewer/code/CodeSurface.tsx` (`DiffSurface`: the loading status, see SA-K).
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
- **Mechanism.** `packages/ui/src/components/status/StatusMark.tsx` (`StatusMark`, `PopoverMark`, `MarkButton`) and
  `packages/ui/src/shell/RailQuotaEntry.tsx` (`RailQuotaEntry`) with `packages/ui/src/quota/QuotaPanel.tsx`
  (`QuotaPanel`) share
  `packages/ui/src/components/hint/hintPopover.tsx` (`PopoverHint`, `useHoverPopover`). E6 source wiring opens hover
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
  and the keyboard and assistive technology open the press form (`docs/ui-standard.md`, "A floating panel that
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
- **Mechanism.** `packages/ui/src/components/status/StatusMark.tsx` (`PopoverMark`) gives a bare mark's `Hint` the
  localized tone label, while `MarkButton` retains the notice sentence as its `aria-label`.
  `packages/ui/src/components/hint/hintPopover.tsx` (`PopoverHint`) names its pressed react-aria `Dialog` with
  `aria-label`, independently of the body text, and moves focus inside. Escape restores the existing trigger.
  `packages/ui/src/viewer/notices.tsx` (`FileNotice`) supplies the whole-file mark after the path.
  `packages/ui/src/viewer/BodyView.tsx` (`ImageView`) and `packages/ui/src/viewer/code/CodeSurface.tsx` (`CodeSurface`)
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

### FOCUS-5 Busy dialog hands focus to its pending button and returns after refusal

- **Location and state change.** In the normal packaged app, put a valid fixture conversation ID in Session ID and
  press Enter. Hold that one real request, then let the real isolated daemon bind the conversation to another
  throwaway project before forwarding it. Read the resulting refusal, Tab to OK and press Enter. The original
  dialog stays open with Session ID focused and Open disabled pending a new lookup. No gallery reply supplies the
  error, and no real model or agent authorization is used.
- **Mechanism.** `packages/ui/src/primitives/Modal.tsx` (`Modal`) provides the busy context;
  `packages/ui/src/primitives/busy.ts` (`useDisabled`) disables the other controls. The pending `Button` in
  `packages/ui/src/primitives/Button.tsx` keeps its label and focusability, becomes unavailable through HeroUI's
  pending behavior and sets `aria-busy`. `packages/ui/src/components/dialog/useFocusLeavingWhenBusy.ts`
  (`useFocusLeavingWhenBusy`), called by `packages/ui/src/components/dialog/Dialog.tsx` (`Dialog`), moves focus from
  the newly disabled input to that pending button in a layout effect. `packages/ui/src/dialogs/SessionDialog.tsx`
  (`SessionDialog`) names the input through `returnFocusRef`; the handoff waits until the nested error is
  acknowledged before returning there.
  This is focus-driven control reading, not a `StatusAnnouncer` or a separate automatic busy sentence.
- **ARIA wiring as rendered.** The source field is a native `AXTextField` named Session ID. While held, it is
  disabled and unfocused; the native Open button is disabled and focused. The error has a focused native alert-dialog
  group named Could not open a session, with the refusal as body text and an enabled OK button. Tab focuses OK;
  acknowledgement leaves Session ID enabled and focused, while Open is disabled and unfocused. These native values
  corroborate the focus path; they do not stand in for the caption evidence or claim literal generated HTML IDs.
- **Result, pending button.** Pass. Without a VoiceOver cursor-read command, the native caption panel displays:

  ```text
  Open，网站“button”正忙 变暗，button
  ```

- **Result, refusal and acknowledgement.** Pass for the observed path. The long alert initially focuses its body,
  rather than OK. Its automatic caption begins with the visible excerpt below. After a deliberate Tab, the caption
  begins `OK，button，` followed by that error text, and native focus is on OK. This deliberate reading is not a second
  automatic-announcement count.

  ```text
  Could not open a session This conversation already
  belongs to a session outside this project. Check the
  conversation ID and selected account. Resume the
  ```

- **Result, input return.** Pass. Enter on OK returns native focus to Session ID. The caption displays the Open
  session dialog context, the field's example ID, the same fixture ID `269912fb-6358-414d-bb67-379792d92913`, and
  text-position feedback. The caption panel clips the tail; neither a complete utterance nor a separate reading of
  the field label is claimed. The source field was also captioned with that UUID before submission.
- **Capture.** Direct screenshots of the existing native VoiceOver caption panel, two samples per state, plus separate
  own-application screenshots and native AX snapshots. The caption window's running system-bundle owner and exact
  window ID were checked; each screenshot checked the screen-lock state before and after. This was not a browser,
  synthetic VoiceOver `output`, AppleScript last-phrase result or human audio listening. Sparse panel samples do not
  count identical replays or establish a complete speech history. Local artifacts are indexed by
  `out/wrapper-native-20261107/evidence-index.html` under the VoiceOver caption confirmation.
- **Environment.** E11, macOS 26.2 (25C56), English application text, mixed English/Simplified Chinese VoiceOver
  output, light appearance. Only the isolated application was driven. The user explicitly authorized temporary
  foreground operation and Command-F5 startup/restoration. The panel was already visible and its display preference
  was left unchanged in this sequence; no volume, mute or VoiceOver settings were changed. The sequence ended with
  VoiceOver disabled, its service absent and the exact original foreground application restored. The production
  entry had no diagnostic key, focus or announcement overrides.
- **Date.** 2026-11-07.
- **Reuse scope.** Covers this newly-disabled text input to pending-button layout-effect handoff, the nested refusal
  read, explicit Tab to acknowledgement and named-input return even when submit becomes disabled. Callers still
  have to satisfy every matching item above. It is not direct listening coverage for an Install link, another
  dialog, pointer submission, a removed return target, another language or unrelated live-region announcements.

### PLAIN-1 Text change in a mounted plain status

- **Location and state change.** The first form of the login dialog: starting to browser-waiting after the URL, then to
  device-code waiting after the code.
- **Mechanism.** Text replaced in place in a `role="status"` div mounted with the dialog (E1), with no
  `StatusAnnouncer`, no delay and no nested alert; focus rested on Cancel login.
  `packages/ui/src/settings/accounts/AccountLoginDialog.tsx` in that build; the code chip rendered through
  `packages/ui/src/components/text/InlineReference.tsx` then (now `packages/ui/src/i18n/InlineReference.tsx`).
  Superseded in the product by SA-C's mechanism; the result remains evidence for this kind.
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
  with no first write delay. `packages/ui/src/terminal/shell/ShellTerminalView.tsx` (`ShellTerminalView`: the visible
  lower status holds the sentence and the Close terminal button). Main window, no modal; terminal input keeps focus. The
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
- **Mechanism.** `packages/ui/src/viewer/ViewerContentView.tsx` (`ViewerContentBody`, conflict branch) renders only the
  read `Failure` when no conflict body is available. `packages/ui/src/viewer/viewerStates.tsx` (`Failure`) passes the
  first-paint delay and announcement flag to `packages/ui/src/components/alerts/PaneFailure.tsx` (`PaneFailure`), then
  `packages/ui/src/components/panel/EmptyPanel.tsx` (`EmptyPanel`) and
  `packages/ui/src/components/announce/useAlertAnnouncement.ts` (`useAlertAnnouncement`). After the viewer delay, the heading and
  reason are in a keyed `role="alert"` wrapper. The conflict kind remains in the viewer heading; no separate conflict
  notice announces it.
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
  `useAlertAnnouncement` caller.

### WARN-1 Deletion warning order and checked option descriptions

- **Location and state change.** Delete an archived console owner with two archived bound project sessions. Before
  opening its Delete confirmation, the real daemon's branch inspection reported one uncommitted worktree and one locked
  worktree, and its agent-record inspection reported `present`. Tick the force-remove, locked-worktree and agent-record
  options, let the warning update settle, read each checkbox, then read through the warning.
- **Mechanism.** `packages/ui/src/dialogs/confirmations/DeleteSessionDialog.tsx`
  (`DeleteSessionDialog`; `ConfirmWarning` no longer
  exists) put the warning title, the bound-session list and then the consequence lines inside
  `TintedAlert` (a component since removed), under `Alert.Content`. The indicator is `aria-hidden`.
  `packages/ui/src/components/dialog/ConfirmDialog.tsx` (`ConfirmDialog`) keeps the options before that warning.
  `packages/ui/src/components/form/ConfirmOption.tsx` (`ConfirmOption`) uses HeroUI `Label` and `Description` for the
  name and hint, plus an optional `aria-describedby` reference supplied by its caller;
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
  `Caution`) supplies the prompt to `ConfirmDialog`. `TintedAlert` (a component since removed) held an `Alert.Title`
  containing the `sr-only` `Caution:` prefix followed by the visible title; `Alert.Description` held the agent-specific
  caution. The warning glyph is `aria-hidden`. This is durable dialog text read on demand; no `StatusAnnouncer` is involved.
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
- **Mechanism.** `packages/ui/src/dialogs/confirmations/DeleteSessionDialog.tsx`
  (`DeleteSessionDialog`, `ConfirmConsequences`),
  `packages/ui/src/components/dialog/ConfirmDialog.tsx` (`ConfirmDialog`) and
  `packages/ui/src/components/dialog/Dialog.tsx` (`Dialog`): a warning modal, the warning icon in its header, a plain
  title, the subject in the body's first sentence, and the session list and consequence paragraphs as plain body text
  with no `TintedAlert`. `packages/ui/src/components/form/ConfirmOption.tsx` (`ConfirmOption`) keeps the `Label` inside
  the `Checkbox` and renders the note as a `Description` outside it, linked by an explicit `aria-describedby` to its
  `useId`; `packages/ui/src/dialogs/BranchCleanupOptions.tsx` (`BranchOption`) is one such caller. Durable reading and
  focus; no `StatusAnnouncer`.
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
  its title; E7 heard the same join). The current source puts the separator between the sentences
  (`common.sentenceSeparator`: a space in en, nothing in zh-Hans) into the message's own text node and ends the hidden
  agent name with a comma ("Codex (Default), "), since a hidden element's trailing space is dropped in rendering. That
  changes the text only, not any item under "What \"matches a case\" means"; the new phrases were not heard.
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
- **Mechanism.** `packages/ui/src/dialogs/confirmations/DeleteProjectDialog.tsx`
  (`DeleteProjectDialog`, `ConfirmConsequences`),
  `packages/ui/src/components/dialog/ConfirmDialog.tsx`, `packages/ui/src/components/dialog/Dialog.tsx`,
  `packages/ui/src/components/form/ConfirmOption.tsx`, `packages/ui/src/dialogs/BranchCleanupOptions.tsx`
  (`useProjectRemovalCleanup`) and `packages/ui/src/components/status/StatusMark.tsx` (`StatusMark`): the warning mark
  is a 24 px button after the checkbox, outside its label; the failure adds no description to the checkbox. The note is
  linked as in WARN-3.
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
- **Since recorded.** The source now also writes the mark's sentence into a status region `AFTER_FOCUS_MOVE_DELAY_MS`
  after the failure arrives (just after the tick); SA-O heard that mechanism in E13, including the missing-Git
  retry and a malformed-config failure, without running sessions.
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
  `Caution`), `packages/ui/src/components/dialog/ConfirmDialog.tsx` and `packages/ui/src/components/dialog/Dialog.tsx`:
  the caution is the agent-specific sentence in one plain `p` after the prompt's other paragraphs, with no hidden
  prefix, no title of its own, no `TintedAlert` and no `StatusAnnouncer`.
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

### RCV-1 Recovered display after uncover, attention and exposure

- **Location and state change.** A running agent terminal or the shown shell loses its holder connection, becomes
  `holder_link: pending`, then returns with `screen_inexact: true`. The process remains alive. Agent recovery passes
  through its loading/first-paint cover; the shell uncovers directly.
- **Mechanism.** `terminal/TerminalPane.tsx` (`recoveredSaid`) and `terminal/shell/ShellTerminalView.tsx`, through
  `terminal/useAnnouncedOnRise.ts` and `terminal/useAttendedDelivery.ts`, feed a `StatusAnnouncer` with
  `clearAfterRead`. The sentence waits `AFTER_FOCUS_MOVE_DELAY_MS` (1500 ms) of attended, uncovered time. Its
  `liveAnnouncerProps` wrapper keeps it exposed beside an unrelated modal, but not inside an inert area. While the agent
  area is inert, E15 defers the sentence: `App`'s `agentAreaInert = covered || floatingShown` is passed to
  `TerminalPane` as `exposed`, and the sentence is written once after exposure. Merely undocking the desktop pane is a
  different condition. E17 also measures the pending and no-process sentences held by `TerminalPane` in RCV-6;
  other cover sentences remain outside those measurements.
- **ARIA wiring as rendered.** Native `AXGroup`, `AXARIALive: polite`, `AXARIAAtomic: true`,
  `AXARIARelevant: additions text`; a descendant `AXStaticText` carries the sentence. Its own value is empty, so the
  captured child text, rather than the group's value alone, establishes the write and clear. The recovered mark is a
  separate `AXButton`; it remains after the automatic sentence clears.
- **Result.** The exact automatic sentence is `Recovered terminal display`. Each row is a separate recording, not a
  claim that another focus or destination matches it. “Once” has the last-phrase limit described above.

| Variant | Result and observed speech | Recording |
|---|---|---|
| Agent, focus on pending cover | Pass: named pending region, `Terminal input 文本输入区`, then recovered sentence 1.59 s later | E14 `013542-g1-agent-cover` |
| Agent, focus elsewhere | Pass: Settings kept focus; one recovered sentence | E14 `013602-g1-agent-elsewhere` |
| Agent, recovery while Finder is frontmost | Pass: no recovered sentence while away; window/input phrase on return, then recovered sentence | E14 `013622-g1-agent-away` |
| Agent, unrelated Settings modal | Pass: recovered sentence beside the still-open dialog | E14 `013641-g1-agent-modal` |
| Agent, second recovery with flag unchanged | Pass: two cover/input transitions, only the first recovered sentence | E14 `013658-g1-agent-duplicate` |
| Agent, switch subject before delivery and return with flag already on | Pass: no stale or newly replayed recovered sentence | E14 `013720-g1-agent-subject-change` |
| Shell, focus on pending cover | Pass: named shell region, input phrase, then recovered sentence 1.60 s later | E14 `013320-g1-shell-cover` |
| Shell, focus elsewhere | Pass: Settings kept focus; one recovered sentence | E14 `013858-g1-shell-elsewhere` |
| Shell, recovery while Finder is frontmost | Pass: no recovered sentence while away; window/input phrase first on return, then recovered sentence | E14 `013918-g1-shell-away` |
| Shell, unrelated Settings modal | Pass: recovered sentence beside the dialog | E14 `013938-g1-shell-modal` |
| Shell, second recovery with flag unchanged | Pass: only the first recovered sentence | E14 `013955-g1-shell-duplicate` |
| Shell, switch before delivery and return with flag already on | Pass: no stale or newly replayed recovered sentence | E14 `014019-g1-shell-subject-change` |
| Agent, maximized Terminal panel holds it inert for 8 s | Fail in E14: sentence lost, including after Restore panel size | E14 `020945-g1-agent-inert-maximized` |
| Same maximized path, exposure gate | Pass in E15: no measured-subject recovered sentence while inert; one after Restore panel size | E15 `021902-g1-agent-inert-maximized` |
| Desktop Float panels | Recovered sentence heard while undocked; this does not cover an inert agent area | E14 `021019-g1-agent-inert-floating` |
| Agent ordinary callback order, without under-cover scheduling gate | Pass: input phrase precedes one recovered sentence | E15 `022419-g1-agent-cover` |
| Shell ordinary callback order, without under-cover scheduling gate | Pass: direct input return precedes one recovered sentence | E15 `022440-g1-shell-cover` |
| Agent cover → input, follow-up ring mechanism | Pass in E17: `Terminal input 文本输入区`, one recovered sentence 1.58 s later; blue input ring in `10-recovered.png` | `042626-g1-agent-cover` |
| Shell cover → input, follow-up ring mechanism | Pass in E17: `Terminal input 文本输入区`, one recovered sentence 1.58 s later; blue input ring in `13-recovered.png` | `042645-g1-shell-cover` |
| Responsive floating panel | Unconfirmed: native window minimum is 1168 px, above the 768 px responsive boundary; requested 740 px was clamped to 1168 px | E15 `021937-g1-agent-inert-responsive` |

- **Clear and on-demand reading.** Agent and shell native streams each captured one write followed by an empty region
  about 5.12 s and 5.02 s later respectively (the 5000 ms read time plus sampling). Later focusing the separate mark
  read `Recovered terminal display 已折叠 按钮 横幅` for the agent and
  `Recovered terminal display (⁨VO shell d176f⁩) 已折叠 按钮 Terminals of ⁨VO project⁩ 区域` for the shell. That manual
  reading is not a second automatic announcement.
- **Capture, environment and date.** E14/E15/E17 native AppleScript last phrase, persistent native AX streams and
  screenshots, 2026-11-10 JST. All recording identifiers in this section have the date prefix `20261110T`.
- **Reuse scope.** Only the listed cover/exposure/attention/focus relations. Recovery with the flag deliberately rising
  under the pending cover used the E14 scheduling fixture; ordinary ordering and actual daemon restart are separate
  observations. An additional real daemon SIGKILL/relaunch did produce `screen_inexact: true` on the surviving
  session (`022607-g1-agent-daemon-restart`), but the frontend remounted with no project session selected. That does
  not certify an automatic announcement for a continuously open view; its attempted mark lookup was excluded.
  The responsive floating condition has no passing native case.

### RCV-2 Recovered mark removed while it or its explanation holds focus

- **Location and state change.** End the isolated agent process, or close the shown shell, while the recovered trigger
  or its pressed explanation holds focus. The explanation really opens and receives native focus before removal.
- **Mechanism.** `shell/TitleBar.tsx` (`Breadcrumb`, `HandoffOnUnmount`) waits two frames for the explanation's
  focus-scope restore when focus was inside the explanation, then focuses the terminal only if focus is still
  unclaimed. With only the trigger focused, E17 hands off immediately. `terminal/TerminalSurface.tsx` uses
  `primitives/focus.ts` (`useArrivalFocusProps`) in E17 to decide the ring as focus arrives; E15 used
  `useFocusVisibleProps` with the text-input rule. E17's ring is shown for keyboard arrival or an automatic handoff,
  suppressed beneath an opaque cover, and unchanged by Escape/Tab typed after a pointer arrival.
  `terminal/shell/ShellTerminalPanel.tsx` keys the recovered-mark wrapper by terminal identity, so replacing a marked
  shell with another marked shell unmounts the old source; `useShellTabFocus` hands focus to the selected tab or
  New terminal. Events while another control holds focus move nothing.
- **ARIA wiring as rendered.** The trigger is `AXButton`, expanded while pressed; the explanation is a named native
  web dialog with its body in a group. Agent landing is `AXTextArea`, named `Terminal input`; shell landing is the
  selected native `AXRadioButton` tab, or the header's `AXButton` named `New terminal` when no tabs remain.
- **Baseline failures.** E14 agent trigger removal reached input without a visible ring (`013739-g2-agent-trigger`).
  An actually opened explanation then removed left only `Octoboard 网页内容` and no input landing
  (`014344-g2-agent-explanation`). Closing a marked shell when its replacement also had a mark reused the focused
  mark for the replacement instead of reaching its tab (`014040-g2-shell-trigger`). These failures remain history;
  earlier Enter attempts that did not open the explanation are not explanation evidence.
- **Results, E15 and separately marked E17.** Every row below is independent. A transient `Octoboard 网页内容` was still sampled during removal,
  followed by the correct destination phrase; it is not a web-area-only final landing. Settled native focus and the
  visible blue ring were checked in the screenshots for each destination.

| Source and destination | Result | Recording |
|---|---|---|
| Agent trigger → terminal | Pass: `Terminal input 文本输入区 主要`, ring visible | `022028-g2-agent-trigger` |
| Agent pressed explanation → terminal | Pass: actual body/dialog phrase before removal, then input phrase and ring | `022048-g2-agent-explanation` |
| Agent trigger → terminal, immediate handoff | Pass in E17: `Terminal input 文本输入区 主要`; blue ring in `12-after-end.png` | `042402-g2-agent-trigger` |
| Agent pressed explanation → terminal, two-frame handoff | Pass in E17: actual body/dialog phrase, transient web-area phrase, then `Terminal input 文本输入区 主要`; blue ring in `14-after-end.png` | `042436-g2-agent-explanation` |
| Agent trigger, Settings already focused | Pass: Settings remains focused | `022109-g2-agent-trigger-elsewhere` |
| Agent explanation closed, then Settings focused | Pass: Settings remains focused when process ends | `022128-g2-agent-explanation-elsewhere` |
| Shell trigger → remaining marked shell's tab | Pass: `VO spare 34647 已选择 标签，第1项（共1项） Terminals 标签组`, ring visible | `022149-g2-shell-trigger` |
| Shell pressed explanation → remaining marked shell's tab | Pass: `VO spare 5f5fc 已选择 标签，第1项（共1项） Terminals 标签组`, ring visible | `022210-g2-shell-explanation` |
| Shell trigger, Settings already focused | Pass: Settings remains focused | `022231-g2-shell-trigger-elsewhere` |
| Shell explanation closed, then Settings focused | Pass: Settings remains focused | `022253-g2-shell-explanation-elsewhere` |
| Last shell's trigger → no-tabs fallback | Pass: `New terminal 按钮 Terminals of ⁨VO project⁩ 区域`, header button ring visible | `022315-g2-shell-no-tabs` |
| Last shell's pressed explanation → no-tabs fallback | Pass: explanation was focused, then New terminal and its visible ring | `023132-g2-shell-explanation-no-tabs` |

- **Capture, environment and date.** E15 and the separately marked E17 native last phrase, native focus/trees and
  screenshots, 2026-11-10 JST. Recording identifiers have the date prefix `20261110T`.
- **Reuse scope.** Only these disappearing sources, real process-end/terminal-close transitions and destinations.
  “Elsewhere after explanation” closes the explanation before focusing Settings; it does not claim that focus can
  be placed behind an open modal. The table does not certify arbitrary popup-removal or terminal-focus patterns.

### RCV-3 Pending cover entry, delayed sentence and recovery

- **Location and state change.** Disconnect only the daemon-to-holder transport while the real agent or shell
  process continues. The daemon reports `holder_link: pending` and retries; releasing the connection restores it.
- **Mechanism.** `terminal/TerminalCover.tsx` renders the pending cover as a focusable named region, described by its
  title and body. `terminal/usePendingFocus.ts` hands focus from input or a vanished Reconnect to that region and
  returns it to input only while the cover still holds it. `terminal/useAnnouncedWhilePending.ts` supplies separate
  immediate and delayed `StatusAnnouncer` texts in `TerminalPane` and `shell/ShellTerminalView`. An already-pending
  subject waits 1500 ms of attended time; at delivery it checks whether focus is on the cover. Retries and renames do
  not start another period. The delayed region is marked as a live announcer beside a modal; the immediate one is not.
- **ARIA wiring as rendered.** The focused cover is native `AXGroup`, role description `区域`, named by the session's
  display title or shell title. The shell also has a same-named `标签页面板`; these are two separate ancestors, not two
  live regions. Title/body are native readable text. Status regions have the polite/atomic/additions-text mapping in
  RCV-1. Generated `aria-describedby` ID strings were not extracted from the DOM.
- **Exact copy.** Agent focus reads the session name, `Reconnecting to the session…`, and
  `The session process is still running. Input is unavailable until the connection returns. Reconnection is automatic.`
  The delayed automatic sentence begins `Reconnecting to the session.` and continues with that same body. The shell
  uses `Reconnecting to the terminal…` on focus and `Reconnecting to the terminal.` automatically, with
  `The shell process is still running. Input is unavailable until the connection returns. Reconnection is automatic.`
  Each focused phrase ends in `区域` (and can include the main-region prefix `主要`).

| Variant | Result | Recording (E14 unless marked) |
|---|---|---|
| Agent input → pending → recovery | Pass: session name/title/body once from focus, no second pending sentence; input on recovery | `015136-g3-agent-input` |
| Agent open subject turns pending, Settings focus rested 700 ms | Pass: one immediate pending sentence; Settings retained through recovery | E15 `022820-g3-agent-elsewhere` |
| Agent immediate write while away, pending held another 4 s after return | Silent throughout away interval and the post-return pending interval; no replay | E15 `022904-g3-agent-immediate-away` |
| Agent focused Reconnect → pending → recovery | Pass: `Reconnect 按钮`, named pending region, then `Terminal input 文本输入区` | `020807-g3-agent-reconnect` |
| Agent shown already pending, Settings focused | Pass: one delayed sentence, Settings retained | `015220-g3-agent-delayed` |
| Agent move onto cover before due | Pass: focused region copy, no delayed sentence | `015241-g3-agent-onto-cover` |
| Agent reach cover, then leave before due | Observed two readings: focused region copy, then the automatic pending sentence | `015302-g3-agent-cover-and-leave` |
| Agent leave window during delayed wait | Pass: no pending sentence in Finder; one after return | `015323-g3-agent-away` |
| Agent immediate write under Settings modal | Silent while modal open; after Escape, the focused cover's copy is read; this is not live-announcement success | `015412-g3-agent-immediate-modal` |
| Agent rename and repeated connection attempts | Pass: no extra pending sentence; recovery still returns input | `020847-g3-agent-rename-retry` |
| Shell input → pending → recovery | Pass: shell name/title/body once from focus, no second pending sentence; input on recovery | `015432-g3-shell-input` |
| Agent input → pending → recovery, follow-up ring | Pass in E17: named cover description only during pending, input phrase on return; ring in `11-input-restored.png` | `042456-g3-agent-input` |
| Agent Reconnect → pending → recovery, follow-up ring | Pass in E17: actual Reconnect, named cover, then input phrase; ring in `13-restored.png` | `042520-g3-agent-reconnect` |
| Shell input → pending → recovery, follow-up ring | Pass in E17: named cover description only, input phrase on return; ring in `14-input-restored.png` | `042600-g3-shell-input` |
| Shell open subject turns pending, Settings focus rested 700 ms | Pass: one pending sentence; Settings retained | E15 `023651-g3-shell-elsewhere` |
| Shell immediate write while away, pending held another 4 s after return | Silent while away and after return; no replay | E15 `023714-g3-shell-immediate-away` |
| Shell shown already pending, Settings focused | Pass: one delayed sentence, Settings retained | `015521-g3-shell-delayed` |
| Shell move onto cover before due | Pass: focused region copy, no delayed sentence | `020004-g3-shell-onto-cover` |
| Shell reach cover, then leave before due | Observed two readings: focused region copy, then the automatic pending sentence | `020026-g3-shell-cover-and-leave` |
| Shell leave window during delayed wait | Pass: no pending sentence in Finder; one after return | `020048-g3-shell-away` |
| Shell immediate write under Settings modal | Silent while modal open; subsequent cover focus reads its copy | `020141-g3-shell-immediate-modal` |
| Shell already pending, keyboard tab switch | Pass: tab retains focus; one delayed pending sentence | `020203-g3-shell-tab-keyboard` |
| Shell already pending, pointer tab switch | Pass: focus moves into the named cover; no second pending sentence | `020225-g3-shell-tab-pointer` |
| Shell rename and repeated connection attempts | Pass: no extra pending sentence | `020915-g3-shell-rename-retry` |

- **Pointer arrival and covered surface, E17.** In `042456-g3-agent-input` and `042600-g3-shell-input`, a real
  pointer click into the recovered input removed its ring. Real Escape then Tab stayed in the same terminal input and
  did not bring it back (`13-mouse-no-ring.png` / `17-mouse-escape-tab-no-ring.png` for agent;
  `16-mouse-no-ring.png` / `20-mouse-escape-tab-no-ring.png` for shell). The focused pending region has its own ring,
  distinct from the hidden input's. RCV-6's exposed off-cover screenshots show no input ring through the opaque
  pending covers. RCV-6 separately measures the new held immediate/delayed sentences while the agent area is inert.
- **Usable input.** E15 `024151-g3-agent-input-usable` recovered from pending, retained the returned input focus and
  sent real character key events plus Enter; the fixture's stdin receipt was exactly `VO-AGENT-INPUT-OK`.
  `024213-g3-shell-input-usable` sent `printf 'VO-SHELL-INPUT-OK\n'` after recovery and captured its terminal output.
  Earlier whole-string Unicode-event probes only delivered the first character to xterm and are excluded as typing
  fixtures, rather than treated as successful full probe commands.
- **Timing/setup limits.** Reconnect was awaited until it actually appeared after the production backoff (about 23 s),
  not inferred from a fixed 8 s wait. The early agent immediate-elsewhere attempt moved Settings focus less than 300 ms
  before the pending write and was silent; it is not an at-rest SA-A match. For the away immediate path, the first
  attempts released pending immediately on return and do not establish a full post-return interval.
- **Capture, environment and date.** E14, E15 and E17 native last phrase, native trees/focus and screenshots,
  2026-11-10 JST, each row on the build its recording column names; recording identifiers have the date prefix `20261110T`.
- **Reuse scope.** Listed focus/attention relations only. The two-copy reach-and-leave sequence is explicitly observed,
  not evidence for an exactly-once total. Immediate writes under a modal are recorded as silence, not covered by the
  delayed path's exposed-live-region result. No real model or login service was involved.

### RCV-4 Restart preflight while keyboard focus stays on the button

- **Location and state change.** From installed-update Settings > About, then separately from the bottom update
  notice, focus Restart now and send Enter. The real preflight is held for 900 ms; send a second Enter about 150 ms
  after the first. The real incompatible-holder confirmation opens, and Cancel is pressed.
- **Mechanism.** `settings/UpdatesSetting.tsx` and `updates/UpdateNotice.tsx` pass the update store's restarting state
  to `primitives/Button.tsx` (`pending`, HeroUI/react-aria `isPending`). `updates/updateStore.ts` guards the restart
  operation; the desktop platform's real handover preflight opens the ordinary exit confirmation. This is pending on
  a retained control, rather than a disabled control replaced with a different one.
- **ARIA wiring as rendered.** Same native `AXButton` and DOM identifier before, during and after preflight.
  Pending retains `AXFocused: true`, changes `AXEnabled` to false and removes `AXPress`; Cancel restores
  `AXEnabled: true` and `AXPress`. Native `AXARIABusy` was not returned, so no literal native busy attribute is
  claimed. Frame captures show the spinner and the button's visible focus ring during the held interval.
- **Result.** Both keyboard placements pass focus retention, blocked second activation, genuine confirmation and
  cancellation return, recorded separately below. Each interval contains exactly one outbound `handover_check`,
  one receipt-based hold, one forward after 900 ms and one confirmation; a second Enter produces no second request.
  The final native button is enabled and focused. VoiceOver's pending feedback is recorded verbatim, without
  inferring a busy utterance from source markup or from FOCUS-5.

| Placement | Speech and return | E15 recordings |
|---|---|---|
| Settings > About | During the first pending run: `Restart now`, then `Restart now Restart now`; confirmation: `Restart Octoboard Restarting interrupts the sessions below. You can resume each one after the restart. 网页警告对话框`. Cancel returns to the same Settings button; the return phrase includes `Restart now 网站“按钮”正忙 按钮 About 标签页面板`, followed by the settled enabled native state. | `024110-g4-settings`; visual/state repeat `024344-g4-settings` |
| Bottom notice, keyboard | During pending: `Restart now 按钮 Application update 区域`, then `Restart now`; same confirmation sentence. Cancel returns to the notice button, with `Restart now 网站“按钮”正忙 按钮 Application update 区域`, followed by settled enabled state. | `024124-g4-notice`; visual/state repeat `024359-g4-notice` |

- **Capture, environment and date.** E15, last-phrase polling, a persistent AX observer established before actual
  update installation, snapshots and timestamped own-window frame captures, 2026-11-10 JST. Recorded speech can
  still describe pending just as focus returns, before the async restart promise clears it; the settled samples
  establish that it did clear. The first held interval did not yield an explicit busy word in either placement.
- **Reuse scope.** These two real installed-update keyboard paths only. It does not establish a universal spoken
  busy phrase, another pending duration, timeout/refusal behavior, or actual restart execution.

### RCV-5 Pointer Restart while terminal input keeps focus

- **Location and state change.** With a real signed update installed and terminal input focused, click the bottom
  notice's Restart now. Hold the real preflight for 900 ms, allow its confirmation, then Cancel.
- **Mechanism.** `updates/UpdateNotice.tsx` uses `Button` with `focusOnPress={false}` and `pending` from the same
  guarded update operation as RCV-4. The native pointer click, rather than AXPress or a keyboard activation, is used.
  The ordinary exit confirmation restores the input that held focus before opening.
- **ARIA wiring as rendered.** The notice button changes native enabled/action state as in RCV-4, but stays
  `AXFocused: false`; the terminal `AXTextArea` retains `AXFocused: true` throughout the held preflight. After Cancel
  the enabled Restart button has its action again and the same terminal input is focused. Held-interval screenshots
  show the spinner without a focus ring on the pointer-pressed button.
- **Result.** Pass for focus and return; silence is the measured busy-feedback result. `Terminal input 文本输入区 主要`
  precedes the held interval, no pending/busy sentence is read during it, the real confirmation sentence from RCV-4
  follows, and Cancel returns to `Terminal input 文本输入区 主要`. A transient `Octoboard 网页内容` occurs on cancellation
  before that input phrase. One `handover_check` was actually held and forwarded. No positive restart was accepted.
- **Capture, environment and date.** E15 `024138-g5-notice-pointer`, repeated with held-interval frames as
  `024413-g5-notice-pointer`; persistent native AX state and last-phrase polling, 2026-11-10 JST.
- **Follow-up re-listen, E17.** `042959-g5-notice-pointer` passes with the new arrival-based terminal ring. The real
  signed update installed and passed strict bundle verification. One `handover_check` was received at
  `2026-11-09T19:30:05.670Z`, held from `.671Z` and forwarded at `19:30:06.573Z` (900 ms configured, 902 ms
  between logged hold/release). Native samples in `held-interval-summary.json` keep `Terminal input` focused while
  Restart now is disabled, unfocused and lacks `AXPress`; `AXARIABusy` is absent. The phrase log contains no busy
  utterance during the hold. The genuine confirmation opens before the 2000 ms timeout, Cancel returns through
  `Octoboard 网页内容` to `Terminal input 文本输入区 主要`, and `12-cancel-return.png` shows the blue input ring.
  Held-state frames and `persistent-update-ax.jsonl` are retained beside the proxy-event slice. No positive restart
  action was sent.
- **Reuse scope.** This non-focusing pointer notice path and its input return only. Silence here cannot claim
  FOCUS-5 or RCV-4's focused-button speech result.

### RCV-6 Pending and no-process sentences held while the agent area is inert

- **Location and state change.** Maximize the project's Terminal panel, making the agent area inert. For immediate
  pending, the selected live agent then loses its holder transport. For delayed pending, select the alternate agent,
  block the transport, then select the already-pending target while still maximized. For the no-process bar, end only
  the selected fixture process while maximized. Wait 6 s in the hidden state, press Restore panel size, then place
  native focus on Settings or the newly exposed pending cover before the held delivery is due. No modal is opened.
- **Mechanism.** `App.tsx` passes `agentAreaInert` as `terminal/TerminalPane.tsx`'s `exposed`. `TerminalPane`'s
  `coverSaid`, `pendingDelayedSaid` and `barSaid` use `terminal/useHeldWhileHidden.ts` (`useHeldWhileHidden`);
  `terminal/useAnnouncedWhilePending.ts` (`useAnnouncedWhilePending`) distinguishes a live rise
  from opening an already-pending subject. `useHeldWhileHidden` waits `AFTER_FOCUS_MOVE_DELAY_MS` (1500 ms) after
  exposure before writing to `components/announce/StatusAnnouncer.tsx` (`StatusAnnouncer`). For pending,
  `stillDue: notOnCover` consumes the held sentence silently if the cover already holds focus.
  An already delivered sentence stays unchanged across hiding and exposure. The no-process bar measured here says
  `Not running`; the archived variant and the other cover sentences were not heard in this round.
- **Effective native mapping.** No target agent sentence or target cover was exposed in the inert native tree. On
  exposure, the off-cover cases have one native `AXGroup` with empty name/value, `AXARIALive: polite`,
  `AXARIAAtomic: true`, `AXARIARelevant: additions text`, and an `AXStaticText` child carrying the exact sentence.
  In the on-cover cases the named `AXGroup` region holds focus and its title/body remain readable; no pending sentence
  is present in either status region, including after focus subsequently moves to Settings. Settings remains a named
  native `AXButton`, not a modal. The no-process bar's visible sentence is hidden separately while Resume remains
  reachable; the spoken text comes from its status region. Source ARIA wiring and native exposure were checked
  separately; generated description IDs were not extracted.
- **Result.** All five E17 variants pass within last-phrase polling's repeat-count limit. The exact automatic pending
  phrase is `Reconnecting to the session. The session process is still running. Input is unavailable until the
  connection returns. Reconnection is automatic.` Cover focus instead reads `主要`, the actual session name,
  `Reconnecting to the session…`, the same body and `区域`. That is a focus description, not a live-region success.

| Variant after Restore panel size | Result | Recording and snapshots |
|---|---|---|
| Immediate pending, focus on Settings | No target sentence while inert; one automatic pending phrase 1.62 s after Restore; no replay after later focus action | `042705-new-immediate-elsewhere`: `8-inert`, `12-exposed`, `15-no-replay` |
| Immediate pending, focus lands on cover | Cover description only; no automatic pending sentence, including after moving to Settings 6 s later | `042740-new-immediate-cover`: `8-inert`, `12-exposed`, `15-no-replay` |
| Delayed pending, focus on Settings | No target sentence while inert; one automatic pending phrase 1.60 s after Restore; no later replay | `042814-new-delayed-elsewhere`: `10-inert`, `14-exposed`, `17-no-replay` |
| Delayed pending, focus lands on cover | Cover description only; no automatic pending sentence, including after moving to Settings 6 s later | `042847-new-delayed-cover`: `10-inert`, `14-exposed`, `17-no-replay` |
| No-process bar, focus on Settings | No `Not running` while inert; one `Not running` 1.61 s after Restore; no second reading after another maximize/restore | `042921-new-no-process`: `7-inert`, `11-exposed`, `17-second-exposure` |

- **Evidence and reuse scope.** E17 native phrase samples include timestamps, foreground PID and focus; the numbered
  snapshots above each have `.ax.json` and `.png` artifacts. The restored off-cover pending screenshots have no
  terminal ring beneath the cover; the on-cover screenshots show the cover's own ring. The shared gateway also
  disconnects the visible shell; its distinct `Reconnecting to the terminal.` phrase is retained in the logs and is
  not counted as the hidden agent's sentence. These cases cover only the named maximized-panel exposure and focus
  relations. They do not cover responsive floating, archived bars, other cover copy, away-window immediate delivery,
  or pending focus reached and then left before its original delayed delivery (RCV-3's two-reading history remains).

### ROW-1 Native row activation does not invoke its nested quick action

- **Location and state change.** Sidebar TreeRows: an unused Claude fixture A (`has_conversation: false`,
  SessionStart only), a used fixture B (`has_conversation: true`, UserPromptSubmit/Stop), and their project.
  Native AXPress is sent to each row itself; actual VO-arrow navigation and VO-Space repeat those three paths.
- **Mechanism.** E16 `packages/ui/src/components/rows/TreeRow.tsx` (`RowPressTarget`) places a plain hidden native
  button first, outside layout, tab order and the accessibility tree. The row remains a focusable `role="button"`
  composite with its own click/keyboard activation. The experiment checks WebKit's native action dispatch on the
  packaged WKWebView; a DOM click or unit-test `.click()` alone would not establish it.
- **ARIA wiring as rendered.** The session and project rows are native `AXButton`s with their full label/status
  descriptions and `AXPress`. No extra hidden press target is exposed as a row control. VO cursor text and its bounds
  were checked against the intended row before every VO-Space. The chord used explicit Control/Option down, Space
  down/up and modifier release, with real VO-arrow movements; it was not substituted with AXPress.
- **Baseline failure.** In the earlier supplied calibration build (UI tree equal to main `505f1c2fc`) with the holder-runtime repair, native
  AXPress on an unused session row sent `delete_session` and the daemon emitted `session_deleted`. No confirmation
  action was sent and the session was actually removed. That observation was AXPress, not an earlier VO-Space result.
- **E16 result.** Pass for all six row actions in `20261110T025652-row-axpress`, repeated in
  `20261110T025855-row-axpress`. Each row below is its own before/action/after interval. Neither session was deleted
  or archived, and no session was created by either project activation.

| Row | Native AXPress | Actual VO-Space |
|---|---|---|
| Unused A | Pass: a real `/ws/term/<A id>` connection selects A; no `delete_session` | Pass: the same selection, A remains unused and present, no delete request/event |
| Used B | Pass: a real `/ws/term/<B id>` connection selects B; no `archive_session` | Pass: the same selection; B remains idle/present, no archive request/event |
| Project | Pass: project collapses; session IDs unchanged, no `open_session`/`create_session` | Pass: collapse is spoken, no new-session request or record |

- **Speech.** Navigation read `⁨VO row unused A⁩ session, ⁨⁨Claude Code⁩ (⁨Default⁩)⁩, Awaiting instructions, Default model 按钮`
  and the used row's full title/status, followed by `Terminal input 文本输入区 主要` when it activated. The project read
  `⁨VO project⁩, Done, Branch ⁨main⁩ 已折叠 按钮`. These are row navigation/activation observations, not automatic
  status announcements.
- **Independent Delete shortcut.** `20261110T030034-delete-shortcut` reached the visible Delete quick button with
  Tab (its own blue ring and Delete tooltip are captured). The first generated Space did not issue a delete request;
  that keyboard activation is not certified. A subsequent real pointer click on that separately pictured garbage-bin
  control sent `delete_session` and produced the real `session_deleted` event for the unused fixture, with no
  confirmation action. The shortcut therefore still deletes on its own by pointer; this does not assert its VoiceOver
  name/discovery route or turn the unsuccessful Space attempt into a pass.
- **Capture, environment and date.** E16, 2026-11-10 JST: native last phrase, guarded native AX actions, VO cursor
  bounds/text, window frames, GUI request log, real terminal connection paths, daemon event observer and before/after
  snapshots. Initial stale-VO-cursor and incomplete-modifier attempts were excluded before VO-Space was delivered.
  VoiceOver was restarted using only the process started by this verification; a real Finder phrase was read before
  the valid VO pass. The row source was not changed during this recheck.
- **Reuse scope.** The three tested row roles/placements and these two native activation routes only. The successful
  behavior is consistent with the hidden first target working in this WebKit build; internal DOM event targeting was
  not instrumented. Other composite roles, other engines and the Delete keyboard/discovery path need separate proof.

### Known not to work

These patterns were heard failing or heard to leave a gap. Do not reuse them, and do not treat an implementation of
their kind as a match for any case. KNW-6 to KNW-8 were heard on E4 and have since been replaced by passing cases heard
on E5; KNW-9 was heard on E6 and is replaced by FOCUS-4, heard on E8. They stay as history of what the earlier
mechanisms did. KNW-8 and KNW-9 are the exceptions in kind: each records one failing aspect of a recording whose case
stays a pass. KNW-8 records the focus destination of the same recording as the passing case SA-K, whose reuse scope
covers the status write and states that focus relation, so it never barred matching SA-K's status write. KNW-9 records
the bare-mark Enter naming of the recording behind FOCUS-3, whose other results stay passing. KNW-10 and KNW-12 were
heard on E14 and are replaced by RCV-1 and RCV-2 heard on E15; KNW-11 was heard on the supplied calibration build (UI
tree equal to main `505f1c2fc`, with the holder-runtime repair `30eb3e434`) and is replaced by ROW-1 heard on E16.

#### KNW-1 A plain status mounted already holding its text

- **What and where.** The same first-form login dialog (E1): the `role="status"` was inserted holding "Logging in…".
- **Result.** Readable manually only. On opening, the automatic output was the dialog name and `Cancel login button`;
  moving the VoiceOver cursor left from Cancel read `Logging in…`. The automatic initial announcement was not observed
  and must not be reported as an automatic pass. That the region arrived populated is a timing lead, not a proven cause.
- **Replaced by.** SA-C, where the same text is spoken automatically.

#### KNW-2 Success written into a durable status while its modal closes

- **What and where.** E1: the account row's `sr-only` wrapper with an inner `role="status"`, empty until success, was
  filled at the moment the state became succeeded, while the login dialog was closing and focus was being returned.
  `AccountLoginAnnouncement` in that build (a component since removed; SA-D's form replaced it), mounted by the
  account row (`AccountRow` then; `AccountTableRow` now).
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

#### KNW-5 Cancel with a handoff that only handles success

- **What and where.** E3: the dialog opened from the direct Log in button was cancelled while starting. The focus
  handoff of that build covered a successful close only.
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
  with the re-exposure. `packages/ui/src/components/form/AdvancedSection.tsx` (`AdvancedSection`) with
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

#### KNW-10 A status written into an inert agent area

- **What and where.** E14: the recovered sentence of RCV-1 for an agent session while a maximized Terminal panel made
  the agent area inert for 8 s. `terminal/TerminalPane.tsx` (`recoveredSaid`) wrote it into its region inside the
  `liveAnnouncerProps` wrapper, which exposes a region beside a modal but not inside an inert ancestor.
- **Result.** Not spoken: the sentence was lost, including after Restore panel size exposed the area again
  (`20261110T020945-g1-agent-inert-maximized`). The inert ancestor is the working explanation; a live-announcer mark
  does not help there.
- **Replaced by.** RCV-1 (E15), where the sentence is held until the area is exposed and then written once.

#### KNW-11 A native press on a role=button row clicks its first nested button

- **What and where.** The supplied calibration build (UI tree equal to main `505f1c2fc`, with `30eb3e434`): native AXPress on an unused
  session's sidebar row, a `role="button"` composite whose first native button inside is its Delete shortcut
  (ROW-1 baseline failure).
- **Result.** The press sent `delete_session` and the daemon emitted `session_deleted`; the session was removed with no
  confirmation. The cause, WebKit's choice of the first native button inside the row as its action element, is
  source-derived and was not reproduced in isolation.
- **Replaced by.** ROW-1 (E16), where a hidden native button leads the row.

#### KNW-12 Removing the explanation's source leaves focus on the web area

- **What and where.** E14: RCV-2's agent recovered mark, with its explanation actually opened and focused, then the
  process ended so the mark was removed.
- **Result.** Only `Octoboard 网页内容` was spoken and focus stayed on the web area, with no landing in the terminal
  input (`20261110T014344-g2-agent-explanation`).
- **Replaced by.** RCV-2 (E15), whose rows land on the terminal input or the surviving tab.

### OPEN-1 Import result dialog rename

- **Location and state change.** The configuration import's result step: the dialog's title becomes "Import finished"
  and the step moves focus to the dialog itself.
- **Mechanism.** Focus moved onto the dialog so it is read out by its new name, with no `StatusAnnouncer`.
  `packages/ui/src/configuration/ImportConfigurationDialog.tsx` (`ImportConfigurationDialog`, result step).
- **ARIA wiring, capture, environment, date.** Not recorded: this case has not been listened to.
- **Result.** Unconfirmed. That VoiceOver reads the renamed dialog on this focus move has not been listened to yet.
- **Reuse scope.** None until confirmed.

### OPEN-2 Startup failure page alert

- **Location and state change.** The window opens on a startup failure page (the daemon failed to start, or the crash
  fallback after the application crashed) with no dialog over it and no focus move. On the crash page focus has fallen
  to the document, as what held it went with the crashed view.
- **Mechanism.** `packages/ui/src/StartupScreen.tsx` (`StartupFailure`) passes `announce` and `announceDelayMs={0}` to
  `packages/ui/src/components/panel/EmptyPanel.tsx` (`EmptyPanel`), through
  `packages/ui/src/components/announce/useAlertAnnouncement.ts` (`useAlertAnnouncement`): the heading and the first line are
  inserted in a keyed `role="alert"` wrapper from a timer after the first paint, so the alert is a change to the page,
  as in ALERT-1's remount, rather than part of its first render.
- **ARIA wiring, capture, environment, date.** Not recorded: this case has not been listened to.
- **Result.** Unconfirmed. That VoiceOver speaks the heading and the first line once when the window opens on this page,
  and whether the alert is spoken at all for a page with no focus move, has not been listened to yet.
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
| `sidebar/ConsoleView.tsx` | filter no-match | SA-A for a change while the section is mounted; no match for the rest | Heard | The section is kept per console, so it can mount with the no-match text already present (switching back to a console, leaving a focus view). SA-H does not cover that: SA-H was heard for a panel toggle with focus left on the toggle, while these mounts come with a console or view switch whose focus relation was not recorded. |
| `shell/ConnectionBanner.tsx` | outage | SA-B | Heard | Matches in full. |
| `settings/accounts/AccountLoginDialog.tsx` | login progress | SA-C for the dialog opening; no match for the rest | Heard | After Log in again the announcer remounts with a footer focus move, not a dialog opening, which was not heard. |
| `settings/accounts/AccountLogin.tsx`, `AccountLoginAnnouncer` | login success | SA-D on the current table and the second mount in `settings/accounts/AccountDetailsDialog.tsx` (E10) | Heard | Current table direct/menu/return paths and the owned-account details path were heard in E10; SA-E heard the table remount. |
| `settings/PermissionsSection.tsx`, through `AttendedStatusAnnouncer` | permission states | SA-L for the section opening, a return with the states unchanged and a grant made in System Settings, returned to by Command-Tab (E12) | Heard | E12 heard the return while the window's focus came from the shell's native window focus; the web view's own event, used now, follows it by 1–2 ms (see "Window attention"). A revoked permission is not heard, and Settings opened directly onto Permissions is unreachable from production controls and untested. |
| `configuration/ImportReview.tsx` | "apply to all" applied | SA-A | Candidate | Mounted empty in the review step, text written after a press while focus rests, inside the modal's own subtree, default delay. Writing the identical sentence again does not rewrite it. |
| `project-browser/git/ChangeList.tsx` | filter no-match | SA-A for typing; no match for a first render holding the text | Candidate | Typing in the filter writes in place at rest. The text can also be present at the first render (a persisted filter, a branch change that closes a popover and returns focus), a mount SA-H does not clearly cover. |
| `viewer/ViewerContentView.tsx`, `ViewerContentView` | loading, disconnected | SA-J for navigation loading; SA-C for the dialog opening as a candidate; no match for disconnected | Heard for SA-J, candidate for SA-C | The navigation text is written at once with focus on Next file (SA-J). The region is mounted with the dialog with a 1500 ms first delay as focus moves, the shape of SA-C, but the opening was heard on the login dialog only. The disconnected text was not exercised. |
| `viewer/ChangeView.tsx`, `UnchangedView` | loading, read outcome | SA-J for the outcome after navigating; no match for its own loading text or other outcomes | Heard | 1500 ms first delay on a new child inside the open dialog with focus kept, heard for the unchanged outcome only. Its own "Loading…" (written when the read takes longer than the first delay), the failure and other outcomes, and a dialog that opens straight onto the view, were not heard; the `Loading…` heard came from the parent. Each text is emptied `READ_TIME_MS` after it was written (`clearAfterRead`), which was not heard either. |
| `components/controls/CopyButton.tsx` | "Copied" | SA-G for the confirmation and the tooltip-driven clear, for a press made while the button has focus; no match for the 2 s automatic clear | Heard | Heard in the main window with focus kept. The 2000 ms branch without a tooltip, a repeat press and a failure were not heard. The copy button in the viewer dialog sets `focusOnPress={false}` (a pointer press does not move focus to it), carries its own "Path copied" text and sits in a dialog, so it is a candidate only for a keyboard press made while it has focus. |
| `project-browser/git/GitView.tsx` | git loading, comparing, listing, spoken failures | SA-H for the first mount with focus kept; no match for the Changes-tab mount, replacement sequences or the failure sentences | Heard | Heard for the Changes loading at mount with focus on the panel toggle. The tab variant, which moves focus into the pane, captured no sentence, and the comparing and listing texts replacing one another in place were not heard. The failure sentences (`useSpokenFailures`) were not exercised. |
| `project-browser/files/FilesPanel.tsx` | files loading | SA-H for the load when the Files panel is shown with focus kept; no match for the Files-tab mount or search loading | Candidate | Same hidden `div`, default delay and main window as the representative, but its own Files load finished too fast to sample, switching to the Files tab is the untested focus-move variant, and search-triggered loading was not recorded. |
| `components/menus/MenuItems.tsx`, `LoadedMenuItems` | menu loading, load failed | SA-I for the loading text; no match for load failed | Heard | Heard with the submenu focused and the git list stretched to 2 s; a visible "Loading…" item says the same word, so attribution is shared. The failure update was masked by an error alert. |
| `viewer/code/CodeSurface.tsx` | expansion loading and note | SA-K and FOCUS-2 for the loading text and the focus after an expansion; no match for the outcome notes | Heard | Loading heard while the pressed button had focus, and focus restored to the surviving control or the code frame on E5. Notes, failure and capped outcomes were not exercised, nor the emptying of each text `READ_TIME_MS` after it was written (`clearAfterRead`). |
| `components/form/AdvancedSection.tsx`, `AdvancedSection` | models loading, unavailable, failed | SA-M for loading and failed; no match for the unavailable text | Heard | Region mounted with each opening outside the folded body, heard for a fresh opening and two reopenings. The unavailable text was not heard separately. |
| `terminal/shell/ShellTerminalPanel.tsx` | shell starting | none | none | Text can be present at the first render at default delay and clears as a terminal appears and takes focus, a write at a focus move; the starting status was not observed populated. |
| `terminal/TerminalPane.tsx`, cover and dormant status | loading, starting, resuming, reconnecting, disconnected and switching covers; the pending cover's sentence when an open session turns pending (`useAnnouncedWhilePending`'s `immediate`, not written while the cover has focus); the dormant bar's "Not running" or "This session is archived" | SA-A for the writes into the mounted regions, the pending sentence only with focus at rest elsewhere, the window attended and no modal open (the pane always mounts with no session, so SA-H applies only to test and gallery mounts); no match for a write made as selecting a session moves focus into the terminal, and none for the dormant bar's button | Candidate | Two `StatusAnnouncer` regions mounted with the pane, default delay (browser-observed: regions inserted empty, text written 100 to 250 ms later). The bar's sentence is `aria-hidden` and its button stays reachable, so only the sentence is spoken, not sentence and button as PLAIN-2 heard. The bar's sentence is held while a modal is open (selecting an interrupted session on an incomplete account asks first) and written `AFTER_FOCUS_MOVE_DELAY_MS` after the close, so it is not written under the modal (KNW-2). A sentence already spoken stays in the region unchanged through a modal, so closing it says nothing. RCV-3 heard the named pending cover and its focused/elsewhere variants; it does not cover the other cover or dormant texts here. RCV-6 separately heard the immediate pending and `Not running` sentences held during maximized-panel inertness, with its stated focus relations; this does not certify other cover text or the archived bar. An archived bar and `SessionHistoryView`'s own region can both hold text while the history loads (different sentences, no duplicate). |
| `terminal/shell/ShellTerminalView.tsx`, trouble and exited status | disconnected, reconnecting and exited, for the shown tab only; the pending cover's sentence when the shown terminal turns pending (`immediate`, as in the pane) | close to SA-H for the first write after a keyboard tab switch (focus stays on the tab; the regions are mounted with each showing, but with the longer `AFTER_FOCUS_MOVE_DELAY_MS`), a candidate only; no case matches a pointer switch or a new terminal, where focus moves into the view (open item); SA-A for trouble changes; PLAIN-2 for the exit filling in place while the terminal input keeps focus, with the sentence only | Candidate | Two `StatusAnnouncer` regions per shown tab, empty when mounted (browser-observed); a hidden tab has none, so switching to an exited or troubled tab writes its text after the delay rather than with the region. PLAIN-2 heard the earlier hand-built region holding the sentence and the Close terminal button, spoken as `The shell has exited Close terminal`; the button is now outside the region, so the match is for the sentence only, by decision, and it has to be listened to again before it is claimed. The write after a tab switch made with the pointer comes with a focus move into the view, which no case covers. |
| `viewer/notices.tsx`, `FileNotice` | the notice's sentence, for a notice that arrives after the viewer has painted | SA-N for SVG fallback, clear and later reading; the first-paint large-file notice has no announcement region (no `speak`) and stayed silent, as context | Heard for these branches | 1500 ms first delay and `clearAfterRead`; SVG sentence present at about 2 s and empty by about 7 s, with focus handed from the now-unavailable Previous file to Next file. E6 did not sample native live attributes. Other image failures, patch fallbacks and replacement sequences remain unheard; SA-J is still the retained-focus representative. |
| `viewer/notices.tsx`, `BlockFallback` | the fallback sentence, for a block (a Mermaid diagram or a formula) that fails after paint, when `speak` is set | none | none | Uses `SpokenSentence` as `FileNotice`; `MermaidBlock` and `FormulaView` pass `speak`. Not heard; focus relation, exposure and native live attributes are unmatched. |
| `viewer/notices.tsx`, `SpotNotice` | the notice's sentence, for a spot in a document that fails or falls back after paint | SA-N is the nearest recorded mechanism; no complete match established | none | Uses `SpokenSentence` as `FileNotice`, rendered as a `span` inside a paragraph, but this caller was not heard. Focus relation, exposure and native live attributes still need matching evidence; SA-N does not establish them for a document spot. |
| `app/useSessionRequests.ts`, account switch outcome | a batch of account switches finished | none | none | Written seconds after the switch dialog has closed, once the batch finishes without failures, and cleared after 10 s. The credential-free fixture does not produce conversation records to switch, so it was not recorded. |
| `settings/UpdatesSetting.tsx` | update check outcome and failure, once the user has engaged the row | none | none | Region mounted with the Updates row in Settings; the buttons go pending rather than disabled so they keep focus. Not heard, and no case records a write after a press in Settings. |
| `settings/ShortcutsSection.tsx` | shortcut rebind, conflict and reset sentences | none | none | Region mounted with the section; an identical consecutive sentence is made distinct by alternating a trailing no-break space so it is written again. Not heard. |
| `settings/accounts/AccountSharingSwitch.tsx` | sharing change failure (`clearAfterRead`); direction and phase of a running change | none | none | Two regions, one beside the switch and one in `SharingProgress`. Not heard. |
| `updates/UpdateNotice.tsx`, through `AttendedStatusAnnouncer` | update available, installed and similar notice | SA-L is the nearest recorded mechanism (the same attended wait); no complete match established | none | The wait is the one `PermissionsSection` uses, but the region sits beside a bottom strip in the main window, not in a section shown by a tab focus move, and the strip can arrive while the user is elsewhere. Not heard. |
| `terminal/history/SessionHistoryView.tsx` | history loading, unavailable, empty | none | none | Region mounted with the view, which comes with a focus move into the history region. Not heard. |
| `about/ThirdPartyLicensesDialog.tsx` | search result count or none, typed in the search field; loading line at the dialog opening | none | none | Two regions in an open dialog: the count region writes in place while focus rests in the field (the shape of SA-A, inside a dialog), the loading region has `firstWriteDelayMs` of `AFTER_FOCUS_MOVE_DELAY_MS` as the dialog opens (the shape of SA-C). Neither was heard. |
| `dialogs/ResumeConversationFields.tsx` | resume lookup outcome | none | none | Always-mounted region in the new-session dialog's Resume tab; text arrives after typing in the Session ID field while focus stays there. Not heard. |
| `dialogs/SessionRequestForms.tsx`, `IncompleteSharingAnnouncer` | an account's unfinished sharing change marked after the dialog opened | none | none | Mounted with the request dialog; text appears after the opening. Not heard. |
| `dialogs/HandoffDialog.tsx` | branch check failed (`firstWriteDelayMs` of `AFTER_FOCUS_MOVE_DELAY_MS`) | none | none | Region mounted with the dialog; a failure arrives after it opened and again on each failed retry. Not heard. |
| `report/ReportPanel.tsx` | list failure mark's words | none | none | Region mounted through every state of the panel. Not heard. |
| `sidebar/useGitRetryAnnouncement.tsx`, through `AttendedStatusAnnouncer` | "Git check failed" after a Try again on the branch badge's Git-missing mark that still finds no Git | SA-P for the sidebar `ProjectNode` placement (E13); other placements have no complete match | Heard | Mounted empty outside the row button; two request-answer writes after the attended wait, alternating trailing no-break space, row focus restored and automatic clear heard. Focus-mode header/group placements and window-attention transitions were not heard. |
| `dialogs/BranchCleanupOptions.tsx`, `DelayedAnnouncer` | the sentence of the project-removal dialog's "Other worktrees unknown" mark after inspection fails, and after a still-failing missing-Git retry | SA-O for the recorded tick/retry/untick paths (E13) | Heard | Region mounted empty with the Trash option; each failure delayed 1500 ms, pending retry empties it for a same-words repeat. Missing Git tick/retry and malformed config tick were heard; untick cleared it. Unattended windows and other focus relations were not heard. |
| `dialogs/ConsoleDialog.tsx`, `dialogs/ProjectDialog.tsx`, `settings/accounts/AccountDialog.tsx`, through `ChangedElsewhereAnnouncer` | the fields the user had edited that were replaced because they changed elsewhere | SA-L is the nearest recorded mechanism (the same attended wait); no complete match established | none | `ChangeAnnouncer` is remounted per change and written `AFTER_FOCUS_MOVE_DELAY_MS` of attended time after it arrives, inside the open dialog with focus kept in the form, and cleared `READ_TIME_MS` after the write. A write while another modal covers the dialog (Settings over the console dialog, an error alert for another failure) is the KNW-2 shape, where the announcement is lost. Not heard. |
| `settings/CloneDirSetting.tsx`, through `ChangedElsewhereAnnouncer` | the clone directory field replaced over what was typed because it changed elsewhere | SA-L is the nearest recorded mechanism (the same attended wait); no complete match established | none | The same region as the dialogs', in the Settings dialog beside the field. A write while another modal covers Settings (an error alert for another failure) is the KNW-2 shape, where the announcement is lost. Not heard. |
| `configuration/ImportConfigurationDialog.tsx`, through `ChangeAnnouncer` | the file was read again because settings changed | SA-L is the nearest recorded mechanism (the same attended wait); no complete match established | none | Written after the same delay, which also outlasts the focus handed on when a failed read's Alert goes away. Not heard. |
| `terminal/TerminalPane.tsx`, through `useAttendedDelivery` | "Recovered terminal display" when `screen_inexact` rises under the open session (`useAnnouncedOnRise`, `clearAfterRead`); the pending cover's sentence when a session is opened already pending (`useAnnouncedWhilePending`'s `delayed`) | RCV-1 for recovered rise; RCV-3 for the separately listed delayed pending relations | Heard for listed variants | Listed for `useAttendedDelivery`: the attended wait after a focus move that delivers these sentences (RCV-1, RCV-3). Two regions mounted with the pane inside a `liveAnnouncerProps` wrapper, so a write beside an unrelated modal stays exposed. Each sentence is queued per session and written `AFTER_FOCUS_MOVE_DELAY_MS` of attended time later, the wait restarted on each return to the window; it is dropped unsaid when the session changes, the flag turns off or the pending period ends first. A rise under the pending cover is queued only once that cover and the loading cover after it are gone, as is a rise still waiting when a new pending cover comes up, so the wait follows the uncover and its focus return. The delayed pending sentence is skipped when the cover has focus as the wait ends. RCV-6 heard it held through maximized-panel inertness and either spoken on exposure or consumed silently when the cover holds focus. The RCV cases give the measured terminal-specific attention, cover and destination relations; responsive inert floating remains unconfirmed. |
| `terminal/shell/ShellTerminalView.tsx`, through `useAttendedDelivery` | "Recovered terminal display" when `screen_inexact` rises while the tab is shown; the pending cover's sentence when a tab is shown already pending | RCV-1 and RCV-3 for their listed shell relations | Heard for listed variants | Listed for `useAttendedDelivery`, as the pane's: the attended wait after a focus move that delivers these sentences (RCV-1, RCV-3). The regions are mounted with each showing of the tab, at the default first delay, which the attended wait outlasts; hiding the tab drops a queued sentence. A rise under the pending cover is queued as the cover lifts (no loading cover follows); RCV-1 and RCV-3 keep the pointer and keyboard destinations separate. |

Totals: of 41 callers (the three dialogs of one row count as three, and the terminal views' attended regions count apart from their other regions), 17 were heard for the writes named (`Sidebar`, `ConnectionBanner`, `AccountLoginDialog`,
`AccountLoginAnnouncer`, `PermissionsSection`, `ViewerContentView`, `UnchangedView`, `CopyButton`, `GitView`,
`ActionMenu`, `CodeSurface`, `AdvancedSection`, `FileNotice`, `useGitRetryAnnouncement`, `DelayedAnnouncer`, and the
attended regions of `TerminalPane` and `ShellTerminalView`), 5 are candidates (`ImportReview`, `ChangeList`,
`FilesPanel`, `TerminalPane` and `ShellTerminalView` for their other regions) and 19 have no complete match (`ShellTerminalPanel`, `SpotNotice`,
`BlockFallback`, the account switch outcome in `app/useSessionRequests.ts`, and the fifteen unmatched callers listed after it, the three dialogs counted as three). Of
the heard callers, `ConnectionBanner` matches for every write it makes, and `AccountLoginAnnouncer` matches the E10
table and owned-account details paths recorded in SA-D; the other fifteen match for some writes only, with SA-N's
native-capture limits stated in that case. Of the candidates, `ImportReview` matches for every write it makes;
`ChangeList`, `FilesPanel`, `TerminalPane` and `ShellTerminalView` for some.

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
- SA-O and SA-P (E13) heard the removal-inspection failure and sidebar Git-retry mechanisms. The earlier immediate
  consequence utterance after ticking Trash repeated the mark name twice; the delayed risk sentence appeared once.
  Focus-mode header/group Git-retry placements, attention loss/return and rapid overlapping retries remain untested.
- Accessibility press on a sidebar row: ROW-1 heard the repaired row (E16) for native AXPress and VO-Space on an unused
  session, a used session and a project, with the baseline failure kept as KNW-11. Still open: the independent Delete
  shortcut's keyboard route (the generated Space did not delete; its VoiceOver name and discovery were not recorded),
  and the `tab`, `menuitem` and `listitem` roles of other composite rows.
- Import result dialog rename: not yet listened to (OPEN-1).
- Startup failure page alert (daemon-failed and crash pages): not yet listened to (OPEN-2). No case matches an alert
  inserted after the first paint on a page the window opens on, with no focus move.
- A choice-consequence warning (`ErrorAlert` with `announce` and `announceDelayMs` of `AFTER_FOCUS_MOVE_DELAY_MS`,
  mounted under a checkbox as it is ticked inside a dialog, focus kept on the checkbox, and added to the checkbox's
  `aria-describedby`): the export dialog's full-paths option and the trust prompt's folder option. No case matches it;
  not yet listened to. WARN-5 heard the trust prompt's folder option before its note was split by state and before this
  warning existed, so it does not cover the ticked state. The trust prompt is an alert dialog, which WebKit exposes as an
  assertive live region (WARN-1, WARN-5), so the Alert mounting inside it may be spoken once by the dialog at once and
  again by its own delayed alert; listen for a double reading there.
- A `StatusAnnouncer` region whose text is present from the first render at the default delay: heard with focus kept in
  the main window (SA-H), with a menu taking focus (SA-I) and, as a region mounted with each opening of a disclosure,
  inside a dialog with the trigger focused (SA-M); not with a focus move into the pane or under a modal.
- Writes not yet heard, with the reason each is still open (the list is partial; the caller table names the rest):
  `CopyButton`'s 2000 ms automatic clear (the presses used kept the tooltip open); `ActionMenu`'s load-failed update (an
  error alert masked it); `CodeSurface`'s outcome notes and failure, `UnchangedView`'s other outcomes and
  `ViewerContentView`'s disconnected text (not exercised); `AdvancedSection`'s unavailable text (not separately
  recorded); `PermissionsSection`'s revoked permission and Settings
  opened directly onto Permissions (no production control opens it there, so it is untested); `GitView`'s replacement
  sequences and failure sentences (not exercised); `FilesPanel`'s search loading (not recorded); the account switch
  outcome in `app/useSessionRequests.ts` (the credential-free fixture does not produce conversation records to switch);
  `ShellTerminalPanel`'s starting status (not observed populated); the terminal statuses of `TerminalPane` and
  `ShellTerminalView` (see the open items below).
- Terminal statuses, written through `StatusAnnouncer`, not yet listened to: the agent cover texts (loading, starting,
  resuming, reconnecting, disconnected, switching account) and the dormant bar's sentences; the shell tab's trouble and
  exited sentences. Open questions: whether the exited sentence alone, with the Close terminal button outside the
  region, still speaks as PLAIN-2 did with the button inside (accepted by decision); writes made as selecting a session
  or pressing a shell tab moves focus into the terminal match no case (the same focus-move gap as SA-C and SA-I). The
  interrupted dormant bar is staged by the gallery scenario `terminal-not-running-bar`; its sentence is held while the
  "Start on <account>?" question is open and written after it closes, a write after a modal closes that no passing case
  covers directly. The close-terminal confirmation of `ShellTerminalPanel` is not counted in the pane's modal flag
  (rare: the session would have to end during that confirmation).
- Claim-check candidates that need no listening once the checks under "Claiming a match" pass: the recovered mark's
  ordinary press and reading of its custom body (WARN-4 candidate), the immediate pending sentence at rest (SA-A
  candidate; RCV-3 heard it at rest for the agent and the shell), the ordinary attended opening and reading of the exit
  confirmation from its other entry points (WARN-4 candidate; RCV-4 heard only its opening through Restart now) and the
  on-demand named "Reconnecting" image in confirmation lists (WARN-4 candidate).
- E17 re-listened the follow-up recovered-mark handoff and terminal ring paths in RCV-1, RCV-2, RCV-3 and RCV-5;
  RCV-6 measures immediate pending, delayed pending and the `Not running` bar held while a maximized panel makes
  the agent area inert, including the two pending paths whose restored focus lands on the cover. Those listed
  variants are covered; other cover sentences held by `TerminalPane` and the archived no-process bar remain unmeasured.
- Recovery/restart measurements now live in RCV-1 to RCV-6. Remaining limits: no passing native case for a
  responsive floating panel making the agent area inert (the desktop's 1168 px minimum prevents reaching the 768 px
  layout); immediate pending sentences while away or behind an unrelated modal were silent, so those writes still
  have no passing match; reaching and then leaving the cover before delayed delivery reads its copy twice. Restart
  keyboard focus/state and pointer return passed, with no explicit busy word during the first keyboard holds and
  silence during the pointer hold. These observations do not certify another timeout/duration or actual restart.
- A write made while the region is under an open modal outside a marker: heard failing in the login success form
  (KNW-2); no passing case.
- A window visible but not frontmost, and a minimized window with the application also not frontmost (KNW-4's condition,
  not rerun on E4): not tested for SA-D or FOCUS-1. E12 heard a Command-Tab return with the window visible for SA-L
  only; the login-success announcer (SA-D) over a Command-Tab return was not heard, and SA-D's E10 recordings predate
  the window's focus events reaching the page (`packages/ui/src/input/focusGuard.ts` held them all back then).
- A transient web-area utterance before the correct button phrase after a cancel (FOCUS-1): still observed on the
  current table and in account details in E10, accepted, not explained. The artificially delayed start/cancel race was
  heard only on the E4 row, not rerun on the current table; the default account's details fallback was not heard.
- Last-phrase polling cannot show how much of a status was spoken before a focus move replaced it (SA-K, FOCUS-2).
- Not covered by any case: real login, other agents' login dialogs, languages and appearances other than those recorded,
  other macOS versions.
