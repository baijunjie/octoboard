# Final confirmation

> Goal: confirm what the earlier milestones could not verify at the time they closed.
> Completion criteria: every item below is checked off, and any defect it turns up is fixed here.

## Checks

- [ ] Run `pnpm bench:wkwebview` in `packages/ui` on a 2x (Retina) display, the commit before the groove dividers and
      panel card edges landed against the commit that landed them, and confirm no regression. The earlier run was on a
      1x display, which only measured the single-line fallback, not the gradient `border-image` dividers and the 0.5px
      hairline a 2x display draws.
- [ ] In the real app (WebKit), with a project and a console session: F6 / Shift+F6 reach each open panel and land on
      its landing spot; hovering a panel toggle with no panel open floats the card in with the fade-up and leaving
      dismisses it (also after pressing a control inside it); clicking the toggle docks it at once; dragging the width
      grip resizes the pane (light grip on hover, strong while dragging, "Resize" tooltip beside it); closing a focused
      panel hands focus to the agent terminal.
- [ ] On Linux (a container will do), run the daemon's shell terminal and PTY tests and confirm busy detection (the
      `/proc` children reading and its fallback scan) and hanging up a closed terminal whose leftover background job
      still holds the PTY: the job loses its terminal and the reader, master and buffers are released. macOS cannot
      show the latter, since a session leader's exit already revokes its terminal there.
