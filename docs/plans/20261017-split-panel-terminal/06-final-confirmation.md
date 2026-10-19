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
- [x] On Linux (a container will do), run the daemon's shell terminal and PTY tests and confirm busy detection (the
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
- [x] In a right-to-left language: the Terminal tab strip keeps the shown tab in view and its scroll buttons clear of
      the close mark, and the maximized Terminal card sits over the agent terminal's area and follows window and
      sidebar changes.
- [ ] With a real powerlevel10k prompt (its `gitstatusd`), a shell terminal at its prompt reads idle and closes without
      asking; with `npm run dev > log 2>&1 &` it reads busy.

## Mid-task handoff

The milestone remains open. Verification on 2026-10-20 started from `b41e61792`, which includes the Terminal panel commit
`1adc71045`. No product behavior or key design decision has been changed. Browser and unit-test evidence below
supports the outstanding desktop checks; it does not count as packaged WKWebView or VoiceOver confirmation.

### Automated checks

- `pnpm install --frozen-lockfile`, `pnpm typecheck` and `pnpm format:check` passed in the dedicated worktree.
- The targeted Vitest run passed 95 tests in 12 files: `ShellTerminalPanel.test.tsx`, `ShellTerminalView.test.tsx`,
  the tests under `src/panels/`, `useRegionCycle.test.tsx`, `usePaneToggles.test.ts`, `AsidePane.test.tsx` and
  `paneOverlay.test.tsx`. A separate `src/lifecycle/useAppExit.test.ts` run passed all 10 tests.
  These exercise tab ordering, starting in the chosen worktree, new-shell focus, busy-close confirmation, startup
  status-region lifetime, reconnect focus, panel stacking and maximize focus handoff, F6 landing, and quit gating
  for busy versus idle/exited shells.
- Targeted gallery smoke passed all 38 loads in English and Simplified Chinese, light appearance, one page at a
  time: `pnpm gallery:sweep --jobs 1 'shell-panel-*'` (15 scenarios, 30 loads), then
  `pnpm gallery:sweep --jobs 1 shell-rename shell-close-busy layout-quit-terminals
  layout-quit-sessions-and-terminals` (four scenarios, eight loads). The cached Chromium headless shell ran these;
  this confirms scenario rendering and steps, not native desktop behavior. No full gallery sweep was run.
- **RTL layout passed** in Playwright WebKit build 2370, at device scale 2, with the gallery's Arabic language
  (`dir="rtl"`, English fallback copy). In `shell-panel-many-tabs`, measured the initially selected newest tab
  and the pointer-selected first tab inside the strip's scroller, repeated with keyboard Home/End, and verified
  visible scroll buttons did not overlap their close marks. In `shell-panel-maximized`, `shell-panel-maximized-alone` and
  `shell-panel-maximized-files-floating`, measured the maximized card against the agent area: 8 px top/bottom/start
  insets, 0 px on the column side with Files docked (the column supplies the 16 px gap), and 8 px on both sides
  without Files. All three stayed aligned after resizing from 1440 x 900 to 1280 x 800 and hiding/showing the
  sidebar. Hovering Files then pressing its toggle floated and docked it while Terminal stayed maximized.
  Screenshots were inspected. This settles the RTL checkbox in a browser's WebKit; packaged-app checks remain open.
- Additional WebKit gallery interaction checks passed in `shell-panel-stacked`: F6 reached the shell, Files,
  top bar and rail and Shift+F6 reversed; the separator moved 16 px with ArrowDown, reset to 417/417 px on
  double-click and moved 32 px on drag. Pointer maximize from the agent input moved focus to the shell with the
  agent area inert; subsequent F6 stops skipped that area, and closing the focused maximized panel returned focus
  to the agent input. Initial Vite loading produced transient module-load and ResizeObserver diagnostics; the
  settled checks passed without a gallery-failure overlay. These results supplement, rather than close, the
  explicit real-app checks. The verification gallery was stopped and its service record removed.
- **Linux shell/PTY verification passed** in `rust:1.86` containers on Linux aarch64, started with `--init` to reap
  orphaned jobs. The worktree was mounted read-only and the source copied into the containers' writable layers.
  `cargo test --locked -p octoboardd shell_terminal:: -- --test-threads=2` passed all 16 tests and
  `cargo test --locked -p octoboardd pty:: -- --test-threads=2` passed all eight tests against unmodified source.
  The busy tests cover foreground jobs, background jobs holding the terminal and helpers whose streams are all
  redirected away. `closing_a_terminal_lets_it_go_whatever_still_holds_it_open` confirms that a job outlives its
  shell, closing hangs up the master and refuses further writes, and the final strong reference to the terminal
  (including its reader and buffers) is released. `hanging_up_stops_the_reader_of_a_process_still_running` also passed.
  To exercise the fallback on this kernel, a second disposable copy replaced only the Linux `children_source`
  capability probe with `let listed = false;`, asserting exactly one replacement. All 16 shell-terminal tests
  passed again through the `/proc` parent scan. This verifies fallback behavior by temporary source injection,
  rather than by running a kernel built without `CONFIG_PROC_CHILDREN`; no repository code was changed.
  The disposable copy was then restored byte-for-byte (`cmp` against the read-only source), and
  `cargo clippy --locked --all-targets -p octoboardd -- -D warnings` passed without warnings. Both containers were
  removed; no Linux check needs the user to start Docker now.

### Checks requiring the user

For the desktop checks, first use a packaged build containing `1adc71045` or later. Read-only accessibility
inspection of the running installed app showed Files but no Terminal toggle, so that window cannot confirm the
new Terminal panel. Build with `pnpm build:app --bundles app` in `apps/desktop` and arrange the app/profile switch
when existing work can be stopped, or ask for an isolated verification run with the profile handling below.

1. **Retina benchmark:** make a 2x display the main display, then request the comparison again (or run it yourself).
   Compare `dd137f49320bd2243dc34c4fc4b0b6acf84eec7c`, the parent of the edge/divider change, against
   `ef8ff5bbb`, the change itself. In separate detached verification worktrees, install dependencies and run
   `pnpm bench:wkwebview --reps 3` from `packages/ui`, without `--port`, keeping both runs on that display with
   other work quiet. Both restyle tables must report `devicePixelRatio 2`; compare their switch, scroll and restyle
   tables, allowing for the harness's documented noise. The current macOS screen inventory reports only a
   1920 x 1080 display at scale 1.0, so no new 1x run was substituted.
2. **Real-app panel stack:** in the packaged app, open a project with Files and Terminal, then a console session
   with Report. Use F6 and Shift+F6 to visit each open panel: Files lands on its first header tab; Terminal on the
   shown shell's input, its Reconnect button when offered, or header New terminal with no tab; Report on its first
   enabled pager button, otherwise Close panel. Close all
   panels, hover each kind's toggle, move into the floating card, press a control inside it, then leave; confirm the
   fade-up and dismissal. Press the toggle to dock immediately. Hover and drag the width grip, checking its light
   and strong states and adjacent Resize tooltip. Close a panel while focus is inside it and confirm focus returns
   to the agent terminal.
3. **Packaged terminal lifecycle:** use several terminals in each of two projects. Switch tabs and projects, print
   distinct scrollback markers, reload the window, and confirm the tabs and markers return. Exercise the
   Files/Terminal separator by dragging, keyboard arrows (including Shift), Home/End and double-click reset.
   Press New terminal and type to confirm focus reaches the new shell. Run a foreground job, request tab close,
   and confirm the busy warning; cancel before ending any work you want to keep.
4. **Packaged maximize:** start with focus in the agent terminal, maximize Terminal, and confirm focus reaches the
   shell before the covered area becomes inert, never the page body. Cycle F6 and confirm the agent is skipped.
   With Files closed, hover its toggle, then press to dock it. Toggle the sidebar and check the shell refits once
   after the ease. Open an archive view and confirm Terminal restores to the stack.
5. **Packaged quit:** with no working agent session and a busy shell, request Quit and inspect the terminals-only
   warning, then cancel. Repeat with a working session too and inspect the combined wording. Finally, with only
   idle or exited shells and no working session, confirm Quit proceeds without a warning. Use disposable work.
6. **VoiceOver:** enable VoiceOver in the packaged app and listen while the first terminal starts, then while the
   shown tab reconnects and exhausts its retries. Confirm each starting/reconnecting/disconnected announcement,
   without duplicate reading of the visible copy. The native VoiceOver caption window is also acceptable evidence.
   If a first start is too fast or connection loss cannot be staged reliably, request an isolated instrumented
   verification build rather than counting an unobserved announcement as passed.
7. **Real powerlevel10k:** install/configure powerlevel10k in the login shell if needed, open a terminal and wait for
   its real prompt with `gitstatusd` running. Confirm closing this idle tab asks nothing. In another terminal, in a
   project with a working `dev` script, run `npm run dev > log 2>&1 &` (leave stdin attached), wait for busy polling,
   and confirm closing asks. Cancel, stop the test job, and clean up its log. No powerlevel10k installation was
   found in the usual user/theme/Homebrew locations, and the user's shell configuration was left alone.

No interactive packaged checks were attempted against the user's running app. A separate build with scratch
`HOME`/`TMPDIR` still shares the installed app's WebKit profile and can prune its saved project preferences. An automated packaged
run needs the user's go-ahead to preserve and restore that profile, with every app of the same identifier stopped;
otherwise the user can perform the checks above in the installed app. No app was installed or replaced.
