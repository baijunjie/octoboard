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
7. the right pane;
8. the connection banner, only while it is shown;
9. the toasts, only while at least one toast is shown.

It works wherever focus is, the terminal included: there `Tab` and `Shift+Tab` still go to the agent, and `F6` and
`Shift+F6` never do. It works from inside a report page too (see "Escape and F6 inside a page" in
`docs/product/report-panel.md`). With focus in none of the regions, `F6` goes to the first region shown and
`Shift+F6` to the last.

**A region not on screen is skipped**: a docked pane the user has hidden, a drawer that is closed, a hidden pane that
is only floating in, the right pane when nothing owns it or it has nothing to take focus (a report panel before its
pages have arrived or with no pages yet, a project pane while it is still loading, see "Loading the project pane" in
`docs/product/project-pane.md`), and the terminal when no session is selected — unless its area is then offering Add
project, which makes it a stop (see "A console with nothing in it" in `docs/product/sessions.md`) — or while the
archive view covers it.
With no console the sidebar, the archive view, the terminal and the right pane are all skipped together, none of them
being on screen, and the welcome screen is the only stop between the rail and the connection banner; with a console
the welcome screen is the one skipped. A pane counts as shown when it is docked in the row at 1148 px and wider, or
its drawer is open below that. A docked sidebar column that is still easing closed already counts as hidden, and one
that is still easing open already counts as shown (see "The two pane toggles" in `docs/product/window-layout.md`).

Where focus lands in each region:

| Region | Lands on |
|---|---|
| Top bar | its first enabled control |
| Rail | the current console's avatar, or the rail's first control when there is no console |
| Welcome screen | its New console button |
| Sidebar | the selected session's row, or the sidebar's first control when that row is not on screen |
| Archive view | its first control |
| Terminal | the terminal, so typing reaches the agent; instead the button its area offers in the terminal's place while there is one — its cover's Reconnect, or its empty state's Add project while the console has nothing in it (see "The terminal" and "A console with nothing in it" in `docs/product/sessions.md`) |
| Right pane, showing a report panel | the pager's first enabled button, else the report page itself |
| Right pane, showing a project pane | its header's tab for the mode shown (Files or Git), from which Tab goes on to Refresh and then to the file tree, or in Git to the tab of the view shown (Uncommitted or Compare); Try again while the pane failed to load |
| Connection banner | its Retry button; it has none while the banner only says it is reconnecting, and F6 then skips it |
| Toasts | the newest toast |

Pressing Retry from the keyboard works as with the mouse. If Retry holds focus when it goes away — because a retry
begins or the banner disappears — focus returns to an open modal dialog: to the control it left while that control is
still available in the dialog, otherwise to the dialog itself. With no modal dialog open, focus goes to the terminal
— or, where the terminal cannot take it, where it goes instead (see "The terminal" in `docs/product/sessions.md` and
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
