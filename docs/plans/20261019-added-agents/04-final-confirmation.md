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
