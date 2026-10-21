# Moving focus between regions

How `F6` moves keyboard focus between the window's regions: the top bar, the rail, the panes (or the welcome screen
standing in for them, see `docs/product/welcome-screen.md`), the connection banner and the toasts, whose places in the
window are in `docs/product/window-layout.md`.

## Moving focus between regions with F6

`F6` moves keyboard focus to the next region of the window and `Shift+F6` to the previous one, in a cycle, in this
order:

1. the top bar;
2. the rail;
3. the welcome screen, only while there is no console, when it is the only region of the content panel (see
   `docs/product/welcome-screen.md`);
4. the sidebar;
5. the archive view, only while it is open (see "The archive view" in `docs/product/sidebar.md`);
6. the terminal;
7. the right pane's panels, each a region of its own, in stacking order, except that a maximized Terminal panel
   comes first among them (see "Panels and keyboard focus" in `docs/product/right-pane-panels.md`);
8. the connection banner, only while it is shown;
9. the toasts, only while at least one toast is shown.

It works wherever focus is, the terminal included: there `Tab` and `Shift+Tab` still go to the agent, and `F6` and
`Shift+F6` never do. The same holds in a shell terminal of the Terminal panel, for the shell. It works from inside a report page too (see "Escape and F6 inside a page" in
`docs/product/report-panel.md`). With focus in none of the regions, `F6` goes to the first region shown and
`Shift+F6` to the last.

The rail's account quota panel is not a region and does not change the cycle: both keys work as usual while it is
open, and from focus inside it they go as from focus in none of the regions, closing the panel as focus leaves it
(see "Closing the panel" in `docs/product/account-quotas.md`).

**A region not on screen is skipped**: a docked sidebar the user has hidden, the sidebar's drawer while it is closed,
a pane that is only floating in, the right pane's panels when nothing owns the pane or none of its panels is open, a
panel with nothing to take focus (a Files panel while it is still loading, see "Loading the project pane" in
`docs/product/project-pane.md`), the terminal and the archive view while a maximized Terminal panel covers them (see
"Maximizing the Terminal panel" in `docs/product/right-pane-panels.md`), and the terminal when no session is selected —
unless its area is then offering a creation action, which makes it a stop (see "A console with nothing in it" in
`docs/product/session-terminal.md`) — or while the archive view covers it.
With no console the sidebar, the archive view, the terminal and the right pane are all skipped together, none of them
being on screen, and the welcome screen is the only stop between the rail and the connection banner; with a console
the welcome screen is the one skipped. The sidebar counts as shown when it is docked in the row at 1168 px and wider,
or its drawer is open below that. A docked sidebar column that is still easing closed already counts as hidden, and
one that is still easing open already counts as shown (see "The sidebar toggle and the panel toggles" in
`docs/product/window-layout.md`). A right pane panel counts as shown while it is open, docked in the pane's column or
maximized, at every width.

Where focus lands in each region:

| Region | Lands on |
|---|---|
| Top bar | its first enabled control |
| Rail | the current console's avatar, or the rail's first control when there is no console |
| Welcome screen | its New console button |
| Sidebar | the selected session's row, or the sidebar's first control when that row is not on screen |
| Archive view | its first control |
| Terminal | the terminal, so typing reaches the agent; instead the button its area offers in the terminal's place while there is one — its cover's Reconnect, or its empty state's Add project, New session or New console session (see "Reconnecting" and "A console with nothing in it" in `docs/product/session-terminal.md`); for an archived session, its history in the terminal's place (see "Keyboard focus" in `docs/product/session-history.md`) |
| Report panel | the pager's first enabled button in its header, else its Close panel; Tab goes on to the report page |
| Files panel | its header's selected tab, Files or Changes, preserving the current mode; the arrow keys move between Files and Changes, and Tab goes through the tab's controls (the filter toggle, and in Changes Group by folder and Group by commit), then Refresh, the panel's own buttons (see "A panel's header" in `docs/product/right-pane-panels.md`) and the content below; Try again while the panel failed to load (see "Loading the project pane" in `docs/product/project-pane.md`) |
| Terminal panel | the shown tab's shell terminal, so typing reaches the shell; instead its cover's Reconnect while that is offered, and New terminal in its header while the project has no terminal (see "Keyboard focus" in `docs/product/terminal-panel.md`) |
| Connection banner | its Retry button; it has none while the banner only says it is reconnecting, and F6 then skips it |
| Toasts | the newest toast |

Pressing Retry from the keyboard works as with the mouse. If Retry holds focus when it goes away — because a retry
begins or the banner disappears — focus returns to an open modal dialog: to the control it left while that control is
still available in the dialog, otherwise to the dialog itself. With no modal dialog open, focus goes to the terminal
— or, where the terminal cannot take it, where it goes instead (see "With no session selected" in `docs/product/session-terminal.md` and
"Keyboard focus" in `docs/product/welcome-screen.md`). Pressing Retry with the mouse leaves keyboard focus where it
was.

The control focus lands on shows its focus ring, even when the last input before `F6` was the mouse.

**With a modal dialog open**, Settings and the file viewer included, `F6` and `Shift+F6` move focus between the dialog
and the banner's Retry button, when that button is available. Either key reaches Retry from the dialog; either key
from Retry returns to the control focus left while it is still available in the dialog, otherwise to the dialog
itself. The dialog stays open, and once focus is back inside it, Tab stays within it and Escape closes it as usual.
The rest of the window, toasts included, remains outside this cycle. While the banner only says it is reconnecting,
or no banner is shown, both keys leave focus where it is.

**With a menu open**, both keys leave focus where it is, even when Retry is available or the menu is inside a dialog.
The menu has to be dismissed before these keys reach Retry. A mouse press can still reach Retry without dismissing
what is open (see "What an open dialog, menu or popover covers" in `docs/product/window-layout.md`).
