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
- [x] In the packaged desktop app (WKWebView), on a project with several shell terminals, and across two projects:
      tabs, switching tabs and projects, reload restoring tabs and scrollback, the Files/Terminal separator (pointer,
      keyboard, double-click reset), New terminal focusing the new shell, and the busy-close confirmation.
- [ ] In the packaged app: maximizing the Terminal panel with focus in the agent terminal hands focus to the shell
      before the agent area turns inert (focus never on the page body); F6 while maximized skips the agent area;
      hovering the Files toggle while maximized floats Files in and pressing docks it; toggling the sidebar while
      maximized refits the shell once at the end of the ease, not on every frame; opening an archive view restores.
- [x] In the packaged app, quitting with a busy shell terminal asks (terminals-only wording, and the sessions plus
      terminals wording with a working session too), and quitting with only idle or exited shells does not.
- [ ] With VoiceOver: "Starting terminal…" is announced when a first terminal starts, and the reconnecting and
      disconnected states of the shown tab are announced.
- [x] In a right-to-left language: the Terminal tab strip keeps the shown tab in view and its scroll buttons clear of
      the close mark, and the maximized Terminal card sits over the agent terminal's area and follows window and
      sidebar changes.
- [ ] With a real powerlevel10k prompt (its `gitstatusd`), a shell terminal at its prompt reads idle and closes without
      asking; with `npm run dev > log 2>&1 &` it reads busy.

## Mid-task handoff

The milestone remains open. The first verification round on 2026-10-20 started from `b41e61792`, which includes
`1adc71045`. The later packaged round used `1f41e42c4`. No product behavior or key design decision was changed.
The automated checks and packaged observations are distinguished below; no VoiceOver listening is claimed.

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
  visible scroll buttons did not overlap their close marks. In `shell-panel-maximized`, `shell-panel-maximized-alone`
  and `shell-panel-maximized-files-floating`, measured the maximized card against the agent area: 8 px top/bottom/start
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

### Packaged verification round

A separate packaged build was made from local main at `1f41e42c4` for this round. The Tauri build configuration
used a unique bundle identifier and product name from the start. The scratch copy's `CFBundleIdentifier` and ad hoc
signing identifier matched, and `codesign --verify --deep --strict` passed. The build used `pnpm build:app --bundles
app --config ...`, so the daemon and production UI were built together without Developer ID signing or notarization.

Before behavioral checks, process and open-file inspection confirmed that the test app ran its scratch executable,
its child daemon held only the scratch home's database and lock on a separate loopback port, and its WebKit processes
opened LocalStorage, ResourceLoadStatistics and content rule lists under the unique identifier. The installed app
continued running separately. The test launch used fresh `HOME`, `TMPDIR`, `ZDOTDIR`, XDG and agent configuration
directories, with a controlled PATH exposing only a stand-in agent. Both projects and every shell job were disposable.

The test build included a temporary native command-file observer for DOM state, focus events, synchronous inert
changes, animations, terminal dimension style writes and resize messages. It did not change the product frontend.
The native source and permissions were restored byte-for-byte after building. Behavioral pointer and keyboard actions
used a driver that checked the exact test bundle identifier, executable path, PID, foreground app and window bounds.
DOM inspection supplied coordinates and observations; fixture setup used only the test daemon's protocol.

- **Panel focus:** native F6 and Shift+F6 reached Files on its first header tab, Terminal on the shown shell input,
  and the agent input. In a console session with no report pages, F6 landed on Report's Close panel fallback.
  Closing the Terminal panel while its shell held focus returned focus to the agent input.
- **Separator:** a native drag changed the Files/Terminal boundary by 36 px, and double-click reset equal shares
  of 347 px. For keyboard operation, DOM focus setup placed focus on the actual separator, then native keys changed
  its value from 347 to 363 with Down, back to 347 with Up, to 411 with Shift+Down, to 120 with Home and to 574 with
  End. The earlier attempt to focus it by clicking preserved the previous focus, as designed, and was not counted
  as keyboard evidence. Reaching that separator by Tab has not been established by this run.
- **Maximize focus and fit:** from the agent textarea, a native maximize press moved focus to the shell before the
  synchronous inert setter ran. Both the setter's before/after observations held shell focus and `body=false`.
  Native F6 then skipped the inert agent area. While maximized, Files could be docked with its toggle. Hiding and
  showing the sidebar each produced one shell dimension-style update, respectively 9 ms and 22 ms after the width
  transition ended, followed by one resize message; no per-frame shell dimension updates occurred during the ease.
- **Tabs and reload:** native New terminal presses created three Alpha shells and two Beta shells, focusing each
  new shell. Native tab and project selection showed the corresponding sets. Distinct `M06_ALPHA_1/2/3` and
  `M06_BETA_1/2` output markers were sent through the test daemon's binary PTY input frames and observed in xterm's
  read-only text view. Each project was checked after a real `location.reload()` of the packaged page and native
  session reselection: the same terminal IDs and all markers returned. Reload resets session selection, so the
  check does not claim that selection itself was restored. An earlier text-frame setup attempt was ignored by the
  terminal socket and was discarded; native Unicode text injection was also unsuitable for seeding command strings.
- **Busy close:** with a real foreground `sleep 600` in the disposable shell, the native close mark opened the
  warning that something was still running and closing would end it, with Cancel and Close terminal. The warning
  was cancelled and the disposable job interrupted.

- **Quit:** the native observer called the existing app-specific quit request entry point. With all stand-in
  sessions idle and a real busy shell, the confirmation said: "Some terminals are still running something. Quitting
  ends what they run." With a working stand-in session too, it said: "Some sessions and terminals are still running.
  Quitting interrupts the sessions, which can each be resumed next time, and ends what the terminals run."
  Both were cancelled with native Escape. After the job was interrupted and sessions made idle, Quit ended the app
  and daemon with only idle shells and no warning. A separately relaunched instance created a shell, let it exit,
  confirmed its record was `exited` and not busy, and Quit again ended both processes without warning. This tests the
  packaged quit handler and confirmation; no global Command+Q was sent to any application.

`pnpm typecheck` and `pnpm format:check` passed. The packaged build also passed its TypeScript and 27 i18n tests.
No new gallery scenario or product code was changed, so no additional gallery sweep was needed in this round.

Cleanup was verified after the checks: all recorded test app and daemon PIDs had exited; the scratch bundle, home,
temporary directory, native driver, build outputs and identifier-specific WebKit/cache data were removed. The test
bundle's Launch Services registration was removed too. The installed app remained running with the same PID, and
SHA-256 hashes of its application and sidecar executables matched the pre-test values. No installed app was stopped,
overwritten or uninstalled, and its profile was not moved, copied or restored.

### Checks still requiring the user

The lifecycle and quit checkboxes are complete. The real-app panel-stack and maximize checkboxes remain open because
their hover-dependent parts were not established, even though their focus and resizing subchecks above passed.

1. **Retina benchmark:** make a 2x display the main display, then request the comparison again or run it yourself.
   Compare `dd137f49320bd2243dc34c4fc4b0b6acf84eec7c` against `ef8ff5bbb` in separate verification worktrees.
   Install dependencies, then run `pnpm bench:wkwebview --reps 3` in `packages/ui`, without `--port`, on the same
   quiet machine. Both restyle tables must report `devicePixelRatio 2`; compare switch, scroll and restyle tables
   with the harness's documented noise in mind. This run's packaged viewport reported DPR 1, so it cannot settle it.
2. **Panel stack and width grip:** in a packaged build containing `1adc71045` or later, close every panel, then hover
   Files, Terminal and a console session's Report toggles. Confirm fade-up, leaving dismisses the card even after
   pressing a control inside it, and pressing the toggle docks immediately. Hover and drag the width grip: confirm
   the light hover state, strong dragging state, adjacent Resize tooltip and changed pane width. Native clicks and
   separator drags worked in this test, but automated mouse movement did not establish the React hover/reveal path;
   CSS hover alone was not counted as proof. Also reach the Files/Terminal separator with Tab; its native keyboard
   operation was verified only after programmatic focus setup.
3. **Remaining maximize interactions:** with Terminal maximized and Files closed, hover Files and confirm it floats
   over Terminal, then press to dock it. Open an actual archive view from a project with archived sessions and confirm
   Terminal restores to the stack. Clicking Files to dock, maximize focus-before-inert, F6 skipping the covered agent
   area and one shell refit after each sidebar ease already passed. The initial archive popover had no archived
   sessions, so opening that popover was not counted as opening an archive view. The later isolated relaunch had no
   native window accessible to the driver despite its page and daemon responding, preventing the final native pass.
4. **VoiceOver:** in the packaged app, listen or inspect native VoiceOver captions while the first terminal starts,
   then while the shown tab reconnects and exhausts retries. Confirm starting, reconnecting and disconnected are
   announced once without duplicate reading of the visible copy. Request a separate instrumented run if the states
   are too brief or cannot be staged reliably; an unobserved announcement is not a pass.
5. **Real powerlevel10k:** configure the login shell with powerlevel10k if needed, open a shell and wait for its real
   prompt with `gitstatusd` running. Confirm closing the idle tab asks nothing. In a project with a working dev
   script, run `npm run dev > log 2>&1 &`, leaving stdin attached; wait for busy polling and confirm closing asks.
   Cancel, stop the test job and remove its log. No prompt package was installed or user shell configuration changed.
