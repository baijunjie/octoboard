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
- [ ] VoiceOver reads the login dialog's waiting text as it changes, and the account row's hidden "logged in"
      announcement once a login succeeds.
- [ ] In the real app window, focus lands where it should when the login dialog's state changes and when it closes, and
      after Move later in the agent tab menu.
- [ ] The menu bar icon's native menu (not shown in the gallery, covered only by unit tests): its quota submenus list
      only agents that are added and available, in the added order; a live session whose agent was removed keeps its
      quota capsule.
- [ ] In the real app window, focus after: an "Add agent…" button opening Settings on the Add agent dialog and that
      dialog closing; an error alert's footer recovery button opening or navigating Settings; a session or handoff
      dialog closing itself because no agent is left and its alert being acknowledged; closing Settings opened over
      another dialog. Dragging an agent tab to reorder it still works now that tabs take their content width.
- [ ] VoiceOver reads an agent's status badge with its name ("Codex, Not added") in pickers, the console dialog's
      account rows and its Advanced tabs.

## Notes for the developer

- A development build (`pnpm tauri dev`) shares `~/.octoboard` with an installed Octoboard, so it would migrate the
  user's real database and clash with the running daemon; confirm on the installed build after merging instead.
- Measuring a login must not run against the user's browser without asking first (see `docs/memory/probing-agent-clis.md`).

## Mid-task handoff

A separate packaged confirmation clone was built from `21f19d33e` using existing configuration only: a build-time
Tauri identifier override and macOS `LSEnvironment` for its own `HOME`, `TMPDIR`, shell and agent configuration
locations. The bundle was ad-hoc signed under its own identifier. No product changes were needed to build the clone.
The real window painted and connected to its sidecar. The daemon's snapshot, open lock/database files and executable
path confirmed it used the clone's data; its sidecar hash matched the newly built binary. Its WebKit profile had the
clone identifier, and all three default account directories resolved under the clone's home. All three reported not
logged in. No real login or browser authorization was started.

The installed clone still contains that original baseline, **not the subsequent focus fixes**. Installation locations,
process IDs, build overrides and cleanup details are recorded in the ignored preview scratch directory of the
`added-agents-final-confirmation` worktree on `docs/added-agents-final-confirmation`;
those machine-specific details are intentionally not committed here. Leave the user's own app, profile and data alone.

No original checkbox is ticked yet: each contains remaining real-device scenarios. The observations below are partial
confirmation, not a substitute for the complete checkbox. Desktop automation later returned `cgWindowNotFound`;
after the account switch no computer-use tool was available in the current session, preventing further window checks
and runtime fix rechecks. Continuing requires a session with that tool enabled, or a clone-scoped macOS accessibility
and input helper with working system permissions. The user authorized the latter route; a helper restricted to
the clone's bundle identifier now reads its real window, focused controls and native status menu, and has
successfully dismissed that menu through Accessibility. Continue window verification through this route after
the reviewed fix is available in the clone.

### Observed in the packaged clone

- **Move later:** opened the agent-tab menu with Shift+F10 and activated Move later with the keyboard. The list
  reordered, the moved Claude Code tab retained focus, and its focus ring was visible.
- **Dragging:** dragged the content-width Claude Code tab past Grok Build. The order changed and the moved tab
  retained focus. Escape during dragging was not exercised.
- **Add agent from an empty terminal:** opened Settings with Add agent in front, whose dialog held focus. Escape
  closed it and focus landed on Settings' Add agent button. The quota-panel entry was not exercised.
- **Settings over a console dialog:** opened it from the console's Manage accounts control. Closing Settings with
  Escape returned focus to that same control in the underlying console dialog.
- **Session dialog loses its last agent:** removed the last eligible agent through a separate control connection.
  The dialog closed without creating a session and showed an acknowledgement-only alert. After the exit transition,
  keyboard acknowledgement focused the sidebar's console actions control with its ring; pointer acknowledgement
  reached the same control but had no ring. The latter is a reproduced defect requiring the focus fix and recheck.
- **Account error recovery with Settings already open:** opened a test account editor, removed that account through
  the clone's control connection, then renamed and saved the stale form. The normal `unknown_account` alert offered
  Manage accounts. Activating it acknowledged the alert but left the account editor above Settings, focused on Save.
  This is a reproduced defect requiring the navigation fix and recheck.

### Focus fixes prepared for review

The separate `fix/final-confirmation-focus` branch addresses both defects. The involuntary alert fallback establishes
keyboard interaction modality before moving focus, so the destination exposes its focus ring after pointer
acknowledgement. Explicit Settings navigation dismisses account add/edit/rename/removal and agent-removal forms
before showing the requested section or Add agent dialog. Active-login lifecycle behavior is unchanged.
The installed baseline clone has not been updated with this branch; passing automated regressions do not replace
the remaining packaged-window rechecks. Merge awaits the owner's re-review; pushing remains unauthorized.

The revised fix passed workspace type checking and lint, and 126 tests in the five affected UI test files with
`--testTimeout=60000`: region focus, Agent accounts, Settings, error alerts and the Settings launcher. Removing each
fix made its new regression fail, confirming that the tests detect the missing focus ring and the stale editor.
The recovery regression uses the real Settings, account editor and error-alert components. A review found items in
documentation, navigation coverage and code clarity that were addressed on the same branches; re-review is pending.
The navigation regressions cover an active login for both a section request and an Add agent request: its dialog
stays mounted and no cancellation request is sent. Pending Add agent requests retain their state and focus-return
flag independently of the dismissal effect's execution order.
No daemon code changed.
The full gallery sweep passed all 784 loads (392 scenarios, English and Simplified Chinese, light appearance),
using an isolated headless browser profile. This confirms rendering and scenario initialization, not native-window
focus or VoiceOver. The initial sweep was interrupted after regression mutation overlapped it; only the complete
rerun on the restored, stable source is counted as passing.

### Remaining window work

After the focus fixes are available in the clone, repeat the last two observations above and confirm both the focus
ring and Settings section destination. Also complete:

- Login-dialog focus across starting, waiting, failure/refusal, retry and closing; coordinate this with the user's
  real-login pass rather than starting another authorization independently.
- Native quota submenu filtering for added/available agents, added order, and retention of a selected live removed
  agent's quota capsule. These native-menu scenarios have not been observed in this round. A readable quota is
  required for the capsule; a fixture CLI can assist native rendering verification, but cannot confirm real login.
- Add agent from the quota panel; both error-footer recovery actions with Settings closed and already open;
  the Add agent recovery case needs a controlled stale request reaching the window's normal request/error path
  (a refusal on a separate control connection is not a window-alert check).
- A handoff dialog closing when its last eligible receiving agent is removed, followed by acknowledgement and focus
  restoration. Session-dialog fallback with an existing selected terminal or underlying dialog also remains.
- Escape cancelling a drag without changing order. Closing Settings through its close button and backdrop remains
  to be checked separately from the confirmed Escape path.

The user chooses and authorizes the accounts for real login, and performs VoiceOver listening or native-caption
observation. Their detailed procedures and pass criteria follow. The clone's actual home already contains spaces;
use it for the Claude Code helper-path check as well. Only tick a checkbox once its whole scope has been observed.

### Remaining checks and expected results

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
2. **Login announcements (second checkbox).** Enable VoiceOver before starting a login. Listen to, or record the
   native captions for, the initial logging-in text and the change to waiting for authorization (the device code for
   Grok Build). Complete authorization and confirm one announcement naming the account as logged in. Record the
   actual text, app build, macOS version and interface language. A visible badge or a DOM live region is not proof
   that VoiceOver announced it.
3. **Login and reorder focus (third checkbox).** Use the keyboard throughout. During login focus should land on
   Cancel login; after a failed/refused login or timeout, on Log in again. Retry and verify that focus returns to
   Cancel login. Check closing after success and cancellation returns focus to the account's available actions
   control within Settings. Exercise Escape and the close button as cancellation paths too. From an agent tab's
   menu choose Move later: the new order should appear and the moved tab should keep visible keyboard focus.
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
   model or effort in its Advanced settings. These stored values keep the corresponding rows and tab visible.
   Remove that agent from the added list. With VoiceOver, inspect the retained current choice in an agent picker,
   the console dialog's account rows and its Advanced tabs. The name and state should be spoken together, such as
   “Codex, Not added”, rather than an unnamed status. Record actual speech or native captions for each location.

Record pass/fail per scenario and only tick a checkbox when its whole scope has been observed. Any failure needs
reproduction details and a fix on a dedicated branch, followed by the required checks and review before a merge is
proposed. The milestone stays open; merge, installation and closure remain with the owner and user.
