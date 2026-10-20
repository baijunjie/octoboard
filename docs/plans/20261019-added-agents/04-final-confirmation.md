# 04 Final confirmation

> Goal: confirm with real agents and assistive technology what the earlier milestones could only test with fakes.
> Completion criteria: every item below is confirmed on an installed build that contains milestones 01–03 — by an
> agent session where it can, by the user where it needs them —
> and any defect found is fixed here.

## To confirm

- [ ] A real **Log in** per agent (Claude Code, Codex, Grok Build) against a fresh account, after which the account's
      quota reads. This exercises what tests only covered with fake CLIs: capturing Claude Code's authorization URL
      through the `BROWSER` helper, Grok Build's real `--device-auth` stderr format, Claude Code's `initialize`
      `tokenSource` for a signed-in account, Codex `account/read` after login, a `BROWSER` helper path under a `HOME`
      containing spaces, and the coordinator's `start_account_login` handler (real login-shell snapshot and its
      post-start re-check), which has no test of its own.
- [x] VoiceOver reads the login dialog's waiting text as it changes, and the account row's hidden "logged in"
      announcement once a login succeeds.
- [x] In the real app window, Move later in the agent tab menu keeps focus on the moved tab.
- [x] The menu bar icon's native menu (not shown in the gallery, covered only by unit tests): its quota submenus list
      only agents that are added and available, in the added order; a live session whose agent was removed keeps its
      quota capsule.
- [x] In the real app window, focus after: an "Add agent…" button opening Settings on the Add agent dialog and that
      dialog closing; an error alert's footer recovery button opening or navigating Settings; a session or handoff
      dialog closing itself because no agent is left and its alert being acknowledged; closing Settings opened over
      another dialog. Dragging an agent tab to reorder it still works now that tabs take their content width.
- [x] VoiceOver reads an agent's status badge with its name ("Codex, Not added") in pickers, the console dialog's
      account rows and its Advanced tabs.

## Notes for the developer

- A development build (`pnpm tauri dev`) shares `~/.octoboard` with an installed Octoboard, so it would migrate the
  user's real database and clash with the running daemon; confirm on the installed build after merging instead.
- Measuring a login must not run against the user's browser without asking first (see `docs/memory/probing-agent-clis.md`).

## Mid-task handoff

### Installed confirmation build

The isolated packaged clone was first rebuilt from local main `9f4e53ca2`, including the merged focus fixes in
`0d95add46`, then rebuilt from the reviewed Settings opener fix described below and finally from a code tree equal to
main `429be4556`. Its signature verified; the live sidecar's executable, open database, lock and snapshot home were
clone-owned. Its hash matched a fresh build signed with the same clone identifier. The first plist-only launch ignored
the isolated environment and was refused by the original instance's occupied lock before starting. Subsequent launches
explicitly passed every clone environment value through `open --env` and connected correctly. Machine-specific source,
PID, launch, ownership and evidence records remain outside the repository with the clone.

The owner could not capture this display through computer-use either. Reopening the clone by its bundle identifier,
then raising and making its AX window main, restored the missing WebKit tree. All input thereafter targeted only
that clone: AX control actions, PID-targeted keys, and bounded pointer events with the verified clone window ID and
separate top-left window-local coordinates. No event was posted globally and the real cursor was not moved.
Only clone-owned main-window and native-popup window IDs were captured. An AX status-item press returned
`cannotComplete` while actually opening its native popup; the popup's AX geometry and capture verified the result.

### Agent checks and evidence

- [x] **Native menu order, availability and removal.** The reopened native popup showed Codex quota before Claude
      quota for that added order. A saved snapshot simultaneously showed Grok Build added but unavailable, and its
      submenu was absent. Removing Codex removed its submenu. An idle credential-free fixture session stayed live;
      the native status item's own accessibility description retained its Codex default-account quota at 85%
      remaining, while the reopened popup listed that session without a Codex quota submenu. The selected-session
      rail capsule also remained. Native popup captures and AX trees, plus the availability snapshot, are retained.
- [x] **Quota-panel Add agent.** With no added agent, the panel's Add agent action opened Settings with the nested
      Add agent dialog holding focus. Closing that dialog returned focus to Settings' Add agent button.
- [x] **Add agent error footer, Settings closed.** A clone-only sidecar proxy held an actual window `open_session`,
      removed its chosen agent while withholding that window's broadcasts, then forwarded the unchanged request.
      The real daemon's `agent_not_added` reply reached the normal window alert; Add agent opened the nested dialog
      over Settings and focused it. Another eligible agent prevented premature session-dialog closure.
- [x] **Add agent error footer, Settings already open.** The proxy held the window's quick-create request while
      Settings was opened on Git. Releasing the real refusal produced the normal footer, which navigated Settings
      to Agent accounts and focused the nested Add agent dialog.
- [x] **Manage accounts footer, Settings closed.** Opening a session with an uninitialized clone-only Grok account
      produced the real launch-failure alert. Its recovery action opened Agent accounts and focused that section's
      tab. This failure arrives after `session_opened`, so the session dialog has already closed; it is not a
      suitable way to retain that dialog beneath Settings.
- [x] **Manage accounts footer, Settings already open on Git.** The proxy held a real quick-create request carrying
      a named account. Restoring the temporarily removed agent and deleting that account made the unchanged request
      receive `unknown_account`. Manage accounts navigated the existing Settings to Agent accounts and focused it.
- [x] **Stale account editor recheck.** After an account was removed through the clone control connection, changing
      and saving its still-open editor produced the real `unknown_account` alert. Manage accounts dismissed the
      editor and focused the Agent accounts section tab, confirming the navigation fix in `0d95add46`.
- [x] **Handoff self-close.** With an idle fixture source and only one eligible receiving agent, removing that
      receiver closed Hand off without a handoff or new session. Its acknowledgement-only alert returned focus to
      the selected terminal after the exit transition.
- [x] **Selected-terminal and underlying-dialog fallbacks.** Removing the last eligible agent from a session dialog
      above a selected live fixture terminal returned focus to Terminal input after acknowledgement. Repeating with
      Settings above a retained session dialog returned focus inside Settings. The latter was staged from the
      pre-recording `agent_not_added` refusal, whose session dialog remains open beneath recovery Settings.
- [x] **Pointer acknowledgement ring recheck.** A real clone-targeted pointer press acknowledged the last-agent
      alert with no selected terminal. AX reported the console actions control focused; its window capture showed
      the blue focus ring, confirming the modality fix in `0d95add46`.
- [x] **Content-width tab dragging and Escape.** A completed targeted pointer drag moved Claude Code after Codex and
      kept the moved tab focused. The same active-drag route with Escape before release left the prior order intact
      and focused the dragged tab. Both the real Settings strip and daemon snapshots confirmed the orders.
- [x] **Settings close button and backdrop.** The close button returned focus to Terminal input or the underlying
      console's Manage accounts control. A pointer-only opening and backdrop dismissal also returned there. An
      additional accessibility-activation followed by pointer-dismissal case exposed the defect below; the reviewed
      fix was installed and the exact failing sequence passed, as did pure-pointer, close-button and Escape paths.

The native-menu, combined nested-focus and Move later checks are complete. Move later was confirmed in the earlier
baseline run. The user confirms UI focus behavior themselves; login-focus checks are removed from this checklist.
No real login or browser authorization was performed. Native VoiceOver results are recorded below.

### Additional defect fixed on main

Activating the actual Console dialog's Manage accounts button through AX, then dismissing Settings with a real
pointer backdrop press, left focus on the WebKit page even after two seconds. Opening the same button with a
pointer first did not reproduce it. This is the accessibility-activation path: the trigger need not receive browser
focus before Settings mounts. Captures and AX results preserve both variants.

The fix, now on main as `39c1ec3f3` (Settings opener restored), changes the actual Console dialog's local Settings
lifecycle: Console accounts supplies the exact opener, and the console repairs otherwise-lost focus after React Aria
settles, with existing focus and foremost-dialog constraints preserved. Restoration cancels on reopen or unmount. The
actual Console dialog regression reproduces the baseline failure. An initial review caught a fix aimed only at the
app-level Settings hook; that superseded code and its synthetic test were removed. The revised actual-path
implementation passed the second independent code-review round with no remaining findings.

The revised source passed workspace type checking, lint and all 300 affected tests in 21 files with two workers. An
unrestricted run hit one model-list test timeout under concurrent build load; the same complete suite passed with
reduced concurrency and no source changes. A packaged build of the fix, built before it reached main, passed the exact
AX-activation plus pointer-backdrop regression, then the pure-pointer, close-button and Escape variants. Its selected
Console gallery sweep passed 34 loads (17 scenarios, English and Simplified Chinese, light appearance). No gallery
states or mocks changed: this restores an existing interaction contract. The earlier focus fixes had separately passed
126 affected tests and the full 784-load gallery sweep; those earlier results are not substituted for this fix's checks.

### Clone state and next action

The rebuilt clone and all evidence remain installed and recorded externally. The build now installed is E4 of the
[VoiceOver verification cases](../../voiceover-cases.md), whose code tree equals main `429be4556` (fixes `39c1ec3f3` and
`429be4556`); the hashes of the earlier local builds are unmerged and not durable references. Temporary credential-free
Codex/Claude CLIs and a sidecar proxy have been used only inside the clone to stage native quotas, live sessions and
real error replies; they validate rendering and recovery, not real login. Test sessions and named accounts were removed.
The proxy is no longer installed in the active bundle, the original clone CLI paths are restored, all three agents are
available and their added order is restored to Claude Code, Codex, Grok Build. The clone is running the E4 build with
its confirmation console, no sessions or named accounts, no dialog open, and no login in progress. Its live sidecar
path, clone database/lock and equally signed fresh-build hash were verified again, and the built source fingerprint was
checked against its source before recording it. Do not launch the retained baseline bundle and current bundle together:
they share the clone identifier. Do not merge, push, close the milestone or delete plan documents; the owner and user
retain those decisions.

### Testing-only handoff boundary

The scope changed to testing and defect reporting after the Settings opener fix had already been committed and installed
in the clone. No further implementation or builds are part of this session's work. The passing packaged comparison is
retained as evidence; it is not approval or a merge. Both fixes are now on main (`39c1ec3f3` and `429be4556`), and the
evidence is for a build whose code tree equals main `429be4556`. The owner and user-directed agents decide any further
implementation.

### Native VoiceOver verification, 2026-10-20

Installed builds: a first-round comparison build and builds integrating main with the two fixes named below, in their
intermediate and final forms (rechecks); only the final one, E4, equals a main commit (`429be4556`). Cases, mechanisms,
ARIA wiring, exact speech and limits are in [VoiceOver verification cases](../../voiceover-cases.md).

- **Sixth checkbox: PASS.** See the retained picker trigger, opened picker option, console account row and Advanced tab
  cases.
- **Second checkbox: PASS on the final build, with a fixture.** On the build whose code tree equals main `429be4556`
  (fixes `39c1ec3f3` and `429be4556`), the progress changes, the named success announcement (menu, direct button and
  return to the window) and the focus after success and cancel pass; the earlier builds' failures are recorded as
  known-not-to-work patterns. See the login cases. Fixture success does not verify real authorization or quota, which
  stay in the first checkbox.
- **Settings backdrop focus: PASS** on the same build (focus only, no speech claim).
- **Restoration: PASS.** Original CLI PATH and console settings restored; Claude Code / Codex / Grok Build order;
  no named accounts, sessions or active logins. VoiceOver off; no preference or volume setting changed.

Login-focus checks are removed; the user confirms UI focus themselves. Only real login and quota remain user-only. The
milestone remains open.

### User-only work

1. Choose fresh Claude Code, Codex and Grok Build accounts entitled to quota. Create fresh named configuration
   directories in the clone, start each login through Settings, complete authorization and verify quota. The
   clone's actual home contains spaces, covering the Claude Code helper-path case. Record only CLI version/path,
   account-directory layout, outcome and quota windows; never credentials or authorization URLs.

The original detailed procedures below remain the pass criteria; procedures 4 and 5 document the completed agent
window work, including the additional fix's packaged recheck.

### Verification procedures and expected results

Use only the separately installed confirmation clone, and choose fresh account configuration directories under its
isolated home. Do not perform these procedures in the user's original Octoboard instance.

1. **Real login and quota (first checkbox).** In Settings → Agent accounts, create a fresh named account with a new
   config directory for each of Claude Code, Codex and Grok Build. Use its actions menu → Log in and complete browser
   authorization against an account the user chooses. Claude Code and Codex should open the authorization page once;
   Grok Build should open it itself once and show a matching device code. On success the dialog closes, the account
   is announced as logged in, and a fresh quota reading appears with the agent's windows. Use an account entitled to
   quota; an account without a subscription cannot confirm a successful quota reading. Record the resolved CLI path
   and version, account-directory layout, result and quota windows without credentials or authorization URLs.
   Repeat the Claude Code login in a test environment whose actual `HOME` contains spaces; a space in the account
   directory alone does not exercise the helper's `HOME` path. Start through Octoboard's Log in action so the real
   coordinator and login-shell capture are exercised, rather than invoking a CLI login separately.
2. **Login announcements (second checkbox).** Enable VoiceOver before starting a login and confirm the initial
   logging-in text, the change to waiting for authorization (the device code for Grok Build) and one announcement
   naming the account as logged in on success. Record results as cases in
   [VoiceOver verification cases](../../voiceover-cases.md), keeping a credential-free fixture run distinct from a real
   authorization.
3. **Reorder focus (third checkbox).** From an agent tab's menu choose Move later: the new order should appear and
   the moved tab should keep visible keyboard focus. This was observed in the earlier baseline run.
4. **Native menu (fourth checkbox).** With at least two added and installed agents, reorder them in Settings, then
   reopen the menu bar icon's menu: quota submenus should follow that order. Remove one and reopen the menu: its
   submenu should be absent. In the agreed test environment also check an added but unavailable agent has no quota
   submenu. Select a live session with a readable quota, remove its agent, and reopen the menu: the session must
   remain running and its selected-session quota capsule in the menu bar icon must remain. The removed agent's
   quota submenu should still be absent. Restore the test agent list afterwards.
5. **Nested focus and dragging (fifth checkbox).** With no agent added, activate Add agent… from the empty terminal
   or quota panel: focus should be in the Add agent dialog over Settings. Close it: focus should be on Settings'
   Add agent button. For Manage accounts…, create a Grok Build account pointing to a fresh empty directory, select
   it for a test session, and attempt to open that session: the uninitialized-home refusal should offer that button.
   For Add agent…, have the assisting agent stage a controlled stale request in the agreed test environment that
   reaches the window's normal error path with `agent_not_added`; keep another agent eligible so the session dialog
   does not close itself first. The assisting agent must arrange this error before handing the control to the user,
   rather than asking the user to reproduce a timing race. Stage each alert with Settings closed and already open
   (on another section). Each footer button acknowledges the alert and focuses the requested dialog or section.
   Open Settings from
   the console dialog's Manage accounts button and close Settings: focus should return to that button in the
   underlying dialog. Have the assisting agent use a separate control connection to the clone's daemon to remove
   the last eligible agent while a session dialog is open; repeat with a handoff dialog by removing its last
   eligible receiving agent.
   Each dialog should close without creating or handing off a session, then show an acknowledgement-only alert.
   After acknowledgement, focus should reach the remaining dialog, else the selected terminal, else the sidebar
   or rail. Finally drag a content-width agent tab across another and release: order should change and the moved
   tab keep focus. Escape during another drag should leave the order unchanged.
6. **Named status badges (sixth checkbox).** Before removal, save a console/project Default agent choice pointing
   to an installed agent, a named account selection for that agent in the console, and a supported non-default
   model or effort in its Advanced settings; then remove that agent and check the retained choice in a picker, the
   console dialog's account rows and its Advanced tabs with VoiceOver. Passed: see
   [VoiceOver verification cases](../../voiceover-cases.md).

Record pass/fail per scenario and only tick a checkbox when its whole scope has been observed. Any failure needs
reproduction details and a fix on a dedicated branch, followed by the required checks and review before a merge is
proposed. The milestone stays open; merge, installation and closure remain with the owner and user.
