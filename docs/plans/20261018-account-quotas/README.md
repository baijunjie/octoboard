# Account quotas and Claude approval mode

## Problem

Octoboard manages several accounts per agent (Claude Code, Codex, Grok Build), but nowhere shows how much of each
account's plan quota is left. To find out, the user has to open each agent's own usage page or a separate menu bar
app. Account management in Settings lists only an account's name and config directory.

Codex accounts also have a per-account approval mode ("Follow account configuration / Automatic review / Ask me");
Claude Code accounts have no such choice, although Claude Code takes a permission mode per launch the same way.

## Plan outline

1. The daemon learns each account's quota by asking the agent's own CLI, caches it, polls it in the background and
   pushes it to clients.
2. Account management in Settings is reworked so each account shows its quota.
3. The left rail gets a quota entry above the Settings button that opens a floating panel.
4. The macOS menu bar icon's menu gets one quota submenu per agent.
5. Claude Code accounts get an approval mode choice like Codex's.

## Key design decisions

- **Quota comes from the agent's own CLI, run with the account's config directory**, never from Octoboard reading
  tokens and calling HTTP endpoints itself. The CLI handles its own login, Keychain access and token refresh, so
  Octoboard never raises a Keychain prompt or risks rotating a token out from under the CLI. The sources, measured
  against the default accounts (Claude Code 2.1.302, Codex 0.162.1, Grok Build 1.0.60):
  - Claude Code: `claude -p` in stream-JSON mode, an `initialize` control request then a `get_usage` control request;
    the reply carries the 5-hour window, the weekly window and per-model weekly windows (e.g. "Fable"), each with
    utilization and reset time, plus the subscription type. About 2 s.
  - Codex: `codex app-server`, `initialize` / `initialized` then `account/rateLimits/read`; the reply carries a primary
    (5-hour) and secondary (weekly) window with used percent and reset time, plus the plan type. Stdin must stay open
    until the reply.
  - Grok Build: `grok agent --no-leader stdio`, ACP `initialize` then `_x.ai/billing` (leading underscore required);
    the reply carries one credit pool with used percent and its current period (weekly on the measured account), plus
    the subscription tier. About 3 s. Stdin must stay open until the reply.
- **These calls are marked experimental by the agents.** A probe that fails or returns an unrecognised shape makes
  that account's quota "unavailable"; it never breaks anything else.
- **Refresh policy: background polling every 5 minutes, plus a refresh on open.** Opening the rail panel, the menu
  bar submenu's source data or the Settings accounts section refreshes accounts whose cached value is older than the
  polling interval. Failures back off.
- **What is shown is the essential part only:** per window, "<window> remaining N%", "resets in <countdown>" and a
  bar. Windows: Claude 5-hour, weekly and per-model weekly; Codex session (5-hour) and weekly; Grok its credit pool.
  Plus the plan name (Max, Plus, SuperGrok) beside the account. Extra usage, spend, credits and pace estimates are not
  shown.
- **The rail entry opens on hover and pins on click.** Hovering shows the floating panel and leaving it hides it;
  clicking or activating it from the keyboard keeps it open until Escape or a click outside. The panel has one tab
  per available agent, listing that agent's accounts.
- **The menu bar icon's menu shows one submenu per agent** ("Claude quota ▸", "Codex quota ▸", "Grok quota ▸"),
  placed after the sessions and before Open Octoboard. Inside, accounts are grouped, each with its window lines. A
  native menu cannot host custom views, so each bar is a menu item whose image is a generated bar picture.
- **Claude Code's approval mode offers four choices:** Follow account configuration (no flag; the account's
  `settings.json` decides), Automatic (`--permission-mode auto`), Ask me (`--permission-mode manual`), and Approve
  everything (`--permission-mode bypassPermissions`). It is per account plus a default for the default account, like
  Codex's, passed on fresh launches and resumes alike. Grok Build is not included.
- **Quotas are runtime state, not configuration:** never persisted, never exported with the configuration.

## Milestones

1. 01 Daemon account quotas (closed)
2. 02 Account management with quotas (closed)
3. [Rail quota entry](03-rail-quota-entry.md)
4. [Menu bar quota submenus](04-menu-bar-quota-submenus.md)
5. [Claude Code approval mode](05-claude-approval-mode.md)
6. [Final confirmation](06-final-confirmation.md)

Milestones 2–4 depend on 1; milestone 5 is independent of the others.

## Open

- Whether Approve everything (`bypassPermissions`) shows a blocking confirmation on an account's first use, and
  whether the user's `skipDangerousModePermissionPrompt` setting is what suppresses it — to be measured in milestone 5;
  if it blocks, how to handle it needs the user's call.
- Whether Claude's auto mode raises a normal prompt plus a `PermissionRequest` hook when its classifier escalates, and
  whether a fresh account's first use of auto shows a one-time consent screen — to be measured in milestone 5.
- Colour thresholds for low remaining quota (e.g. warning below some percent) are not settled; bars use one colour
  per agent until decided.
