# Help tips for one-time explanations

## Problem

Across the UI, many controls and sections carry a permanent line of explanatory text under their label: what a
setting means, what an account is, what a field is for. A user needs to read such a line once; after that it only takes
space and pushes the controls apart. The default account row in Settings → Agent accounts was the first case moved
into a help tip; the rest of the UI has not been reviewed.

## Plan outline

Apply one rule everywhere: an explanation that only needs to be read once, and that later use of the control does not
need on screen, moves into a help tip; everything else stays visible.

## Key design decisions

- **The rule (set by the user).** Explanatory text that is needed once, with no need to stay on screen for later
  operation, goes into a help tip. Stays visible:
  - status and warnings (not installed, not usable, errors);
  - consequences and timing a user must weigh when acting (when a change takes effect, extra cost, what is lost);
  - anything a user needs to read while operating the control (formats, limits, the current value's meaning).
- **The help tip.** The reusable "?" component introduced with the account quotas work: hovering shows the explanation
  as a tooltip; clicking, tapping or Enter / Space opens it in a popover that stays until Escape or a click outside.
  It sits at the very end of the line it explains — after the label and any chips beside it.
- **The rule is a standing convention**, written into the product docs on labels and tooltips, so later UI follows it
  without this plan.

> Goal: every one-time explanation line in the UI is moved into a help tip per the rule; lines the rule keeps visible
> stay as they are.
> Completion criteria: an inventory of the UI's explanation lines, each marked moved or kept with its reason under the
> rule; every moved line rendered through the help tip at the end of its line, in every i18n catalog; component tests
> and gallery scenarios updated through the gallery skill; the full gallery quick check passes; the user confirms the
> result in a UI preview.

## Implementation plan

- [ ] Inventory explanation lines across all UI surfaces (Settings sections, console settings, project settings,
      dialogs, panels, popovers) and classify each under the rule; report borderline cases to the user before moving
      them.
- [ ] Move the lines classified as one-time into help tips; keep the visible ones unchanged.
- [ ] Update tests, i18n catalogs and gallery scenarios for the moved lines.

## Notes for the developer

- **Prerequisite**: the help tip component and the written rule arrive with the account quotas work (Settings
  accounts milestone); start only once that is on the main branch.
- **Reusable capabilities**: the help tip component; the existing tooltip and popover wrappers it builds on.
- **Development notes**: a help tip's accessible name says what it explains ("About …"), distinct from the explanation
  itself; the explanation must stay reachable to screen readers.
- **Reference docs**: `docs/product/labels-and-tooltips.md`, `docs/ui-accessibility.md`, `docs/product/settings.md`.
