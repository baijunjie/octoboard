# 04 Menu bar quota submenus

> Goal: the menu bar icon's menu has, after the sessions and before Open Octoboard, one submenu per available agent
> ("Claude quota ▸", "Codex quota ▸", "Grok quota ▸"); each lists that agent's accounts with their plan and, per
> window, a line "<window> remaining N% · resets in <countdown>" followed by a bar picture.
> Completion criteria: the menu shows the submenus with real data in the packaged app, in a light and a dark menu;
> the menu still never changes while open; unit tests cover building the menu model from quota data.

## Technical design

- [ ] The menu model the UI sends to the desktop shell gains the quota submenus, worded in the UI's current language.
- [ ] Bar pictures generated as images for menu items, readable in light and dark menus.

## Implementation plan

- [ ] Account headers and window lines cannot be chosen; an account whose quota is unavailable shows one line saying
      so.
- [ ] Before the UI has loaded the daemon state, the submenus are absent, as the sessions are.
- [ ] Countdowns are as of the last menu update; the existing rule of deferring updates while a menu is open applies.

## Notes for the developer

- **Reusable capabilities**: the existing tray menu builder, its deferred-update handling and its template-image
  handling for agent marks.
- **Reference docs**: `docs/product/account-quotas.md`, `docs/product/menu-bar-icon.md`, `docs/product/language.md`.
