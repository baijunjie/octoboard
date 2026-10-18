# Final confirmation

> Goal: confirm what the earlier milestones could not verify at the time they closed.
> Completion criteria: every item below is checked off, and any defect it turns up is fixed here.

## Checks

- [ ] Run `pnpm bench:wkwebview` in `packages/ui` on a 2x (Retina) display, the commit before the groove dividers and
      panel card edges landed against the commit that landed them, and confirm no regression. The earlier run was on a
      1x display, which only measured the single-line fallback, not the gradient `border-image` dividers and the 0.5px
      hairline a 2x display draws.
