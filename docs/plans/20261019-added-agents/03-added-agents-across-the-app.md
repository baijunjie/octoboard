# 03 Added agents across the app

> Goal: every other place that lists agents shows only added agents, in the order the user set.
> Completion criteria: in the gallery, the console, project and session dialogs' agent pickers and the rail quota
> panel list only added agents; the dialogs' pickers also leave out an added agent that is not installed, except as a
> picker's current value, which stays visible and marked; a never-added agent appears nowhere else. Unit tests and the gallery quick check pass.

## Technical design

- [ ] The console, project and session dialogs' agent pickers list only added agents, in the added order.
- [ ] The console dialog's per-agent sections — the "Agent default accounts" rows and the Advanced options agent tabs
      (model and reasoning effort) — follow the same rule; an agent kept there only for its current value is marked.
- [ ] The rail quota panel lists only added agents, in the added order.
- [ ] Product docs (`docs/product/consoles.md` "Agents that are not installed", `docs/product/account-quotas.md`
      and any other place describing which agents are listed) describe the added-agents rule.
- [ ] Gallery scenarios for these pickers and the rail panel updated to the new model.

## Handoff

- Code that landed on `main` while 02 was under way lists every supported agent: the selected session's quota bar
  and the menu bar's opener bars loop over `AGENT_OPTIONS` and agent availability. Bring them under the added-agents
  rule with the other lists.

## Notes for the developer

- **Reusable capabilities**: the shared agent picker options and the not-installed labelling the pickers already
  use; the added list from milestone 01.
- **Development notes**: see the README's key decision on the dialogs listing only added and installed agents.
- **Reference docs**: `docs/product/consoles.md`, `docs/product/account-quotas.md`,
  `docs/product/model-and-reasoning-effort.md`.
