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
- [ ] In the packaged desktop app (WKWebView), on a project with several shell terminals, and across two projects:
      tabs, switching tabs and projects, reload restoring tabs and scrollback, the Files/Terminal separator (pointer,
      keyboard, double-click reset), New terminal focusing the new shell, and the busy-close confirmation.
- [ ] In the packaged app: maximizing the Terminal panel with focus in the agent terminal hands focus to the shell
      before the agent area turns inert (focus never on the page body); F6 while maximized skips the agent area;
      hovering the Files toggle while maximized floats Files in and pressing docks it; toggling the sidebar while
      maximized refits the shell once at the end of the ease, not on every frame; opening an archive view restores.
- [ ] In the packaged app, quitting with a busy shell terminal asks (terminals-only wording, and the sessions plus
      terminals wording with a working session too), and quitting with only idle or exited shells does not.
- [ ] With VoiceOver: "Starting terminal…" is announced when a first terminal starts, and the reconnecting and
      disconnected states of the shown tab are announced.
- [ ] In a right-to-left language: the Terminal tab strip keeps the shown tab in view and its scroll buttons clear of
      the close mark, and the maximized Terminal card sits over the agent terminal's area and follows window and
      sidebar changes.
- [ ] With a real powerlevel10k prompt (its `gitstatusd`), a shell terminal at its prompt reads idle and closes without
      asking; with `npm run dev > log 2>&1 &` it reads busy.
