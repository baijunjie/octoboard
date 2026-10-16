# Terminal panel

> Goal: a Terminal panel in the right pane's stack runs the project's shell terminals as tabs.
> Completion criteria: a "new terminal" action adds a tab and each tab can be closed; switching tabs keeps every tab's
> screen; switching project shows that project's tabs while the others keep running; reloading the window or reopening
> the panel restores the tabs and their scrollback; the Terminal panel stacks under Files and the handle between them
> resizes both by pointer and keyboard; maximize works with both open; the Terminal panel is its own F6 stop and F6 /
> Shift+F6 leave a focused shell terminal; Escape inside a floating or drawer pane reaches
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
- [ ] Change the floating and drawer pane's Escape handling for shell terminals, with the case covered in that
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
