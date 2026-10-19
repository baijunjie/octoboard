# Terminal panel

> Goal: a Terminal panel in the right pane's stack runs the project's shell terminals as tabs.
> Completion criteria: a "new terminal" action adds a tab and each tab can be closed; switching tabs keeps every tab's
> screen; switching project shows that project's tabs while the others keep running; reloading the window or reopening
> the panel restores the tabs and their scrollback; the Terminal panel stacks under Files and the handle between them
> resizes both by pointer and keyboard; maximize works with both open; the Terminal panel is its own F6 stop and F6 /
> Shift+F6 leave a focused shell terminal; Escape inside a floating pane reaches
> the shell; the agent terminal in the centre is unaffected. Verified in the real app with several tabs and projects,
> and by gallery scenarios using the fake terminal socket.

## Technical design

- [ ] The Terminal panel kind in the panel registry, with a header action for a new terminal and a tab strip.
- [ ] One terminal controller per tab, kept mounted while its tab is hidden, so a hidden tab keeps its screen and its
      connection.
- [ ] Per-project tab state read from the daemon's shell terminal records; the active tab per project kept as a
      per-client preference.
- [ ] The panel's own reconnect policy for a dropped terminal socket, driven by the shell terminal record.
- [ ] The Escape rule: focus inside a shell terminal in the right pane keeps Escape for the terminal; the agent terminal
      keeps leaving Escape to the pane.
- [ ] Terminal styling and lookups scoped to their own region, so several terminals on the page do not match each
      other's selectors.

## Implementation plan

- [ ] Add the Terminal panel and its tab strip, outside the tab component's panel so hidden tabs stay mounted.
- [ ] Wire tabs to the daemon's shell terminal records and requests, and attach each controller to its terminal socket.
- [ ] Change the floating pane's Escape handling for shell terminals, with the case covered in that
      handler's own test.
- [ ] Scope the document-wide terminal lookups in the styles, the gallery, the gallery sweep script and the WebKit
      benchmark harness, and give the new terminals the terminal style rule now scoped to the centre region.
- [ ] Check the renderer with many tabs open, including falling back when graphics contexts run out.
- [ ] Add gallery scenarios for one tab, several tabs, a shell that has exited (as the overview's Open settles), the
      panel stacked under Files, and a maximized panel.

## Notes for the developer

- **Reusable capabilities**: the terminal controller (Ctrl+C handling, CJK input fix, WebGL renderer with DOM
  fallback); the store's terminal socket URL; the scrolling tab strip labels; the gallery's fake terminal socket.
- **Development notes**:
  - Reconnecting after a dropped socket is the agent terminal pane's job today, not the controller's; the controller
    also carries session-only behaviour (waking a dormant session on input, saved output) that a shell terminal must
    keep off.
  - A HeroUI `Tabs` panel is mounted only while its tab is selected; terminal content must not live inside it.
  - Exceptions to the pane's Escape handling go in that capture-phase listener and its own test, never as a local
    guard; its test currently asserts that xterm's textarea leaves Escape to the pane.
  - A terminal keeps Tab, so F6 is its only way out; the panel joins the region cycle set up in milestone 3.
  - All strings go through the i18n catalog.
  - New panel states go to the gallery through its skill.
- **Reference docs**: `docs/ui-accessibility.md` ("Leaving the terminal with F6"),
  `docs/memory/focus-and-key-handling-in-the-ui.md`, `docs/memory/building-on-heroui-and-react-aria.md`,
  `docs/memory/ui-text-and-reading-direction.md`, `docs/product/moving-focus-between-regions.md`,
  `docs/product/project-pane.md`.

## Handoff

- Closing the last panel when it is not Files (only Terminal open) empties the stack, and an empty stack falls back to
  showing Files, so the Files card would mount and load inside the pane while the pane eases closed. Make the fallback
  apply only when the pane is next shown, or keep the closed card until the pane has finished hiding, and check it.
- Stacking two real panels is first possible here: verify the panel separator between Files and Terminal (pointer,
  keyboard, double-click reset), the top-bar toggle closing a focused panel while another stays open, maximize and
  restore with both open, and the per-pair separator names.
- Once an owner has changed under the panel stack, a kind closed and re-opened while the pane stays shown is told it
  is switching, so re-opening Files beside a Terminal would play the project-switch slide. Limit the slide to an owner
  change for the panel itself, and check it.
- A maximize hides the other cards after handing focus to the kept card; verify in the real app (WebKit) that focus
  never drops to the page body when maximizing with focus in another card.
- The daemon's shell terminals are ready: the `ShellTerminal` record in the snapshot, `shell_terminal_upserted` /
  `shell_terminal_deleted`, and `create_shell_terminal` (taking the Git mode's worktree id), `rename_shell_terminal`
  and `close_shell_terminal`; the terminal socket accepts a shell terminal's id. See `apps/daemon/PROTOCOL.md`
  "Shell terminals". The UI store does not keep these records yet.
- The UI side of the busy rule is still to build: closing a busy tab asks first, and the quit prompt (`holdsQuit` in
  `useAppExit.ts`) counts busy shells, never idle or exited ones. The record carries `busy`.
- `daemonMessage.ts` resolves a `console`, `project` or `session` parameter to its name but not a `shell_terminal`
  one, which `PROTOCOL.md` says is shown by the terminal's name.
- Check the busy rule in the real app with a real shell setup: a prompt that keeps a long-lived child of the
  interactive shell (powerlevel10k's `gitstatusd`, async prompt workers) would read busy at its prompt forever, and
  every close would ask.
