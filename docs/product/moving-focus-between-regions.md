# Moving focus between regions

How `F6` moves keyboard focus between the window's regions: the top bar, the rail, the panes, the connection banner
and the toasts, whose places in the window are in `docs/product/window-layout.md`.

## Moving focus between regions with F6

`F6` moves keyboard focus to the next region of the window and `Shift+F6` to the previous one, in a cycle, in this
order:

1. the top bar;
2. the rail;
3. the sidebar;
4. the archive view, only while it is open (see "The archive view" in `docs/product/sidebar.md`);
5. the terminal;
6. the right pane;
7. the connection banner, only while it is shown;
8. the toasts, only while at least one toast is shown.

It works wherever focus is, the terminal included: there `Tab` and `Shift+Tab` still go to the agent, and `F6` and
`Shift+F6` never do. It works from inside a report page too (see "Escape and F6 inside a page" in
`docs/product/report-panel.md`). With focus in none of the regions, `F6` goes to the first region shown and
`Shift+F6` to the last.

**A region not on screen is skipped**: a docked pane the user has hidden, a drawer that is closed, a hidden pane that
is only floating in, the right pane when nothing owns it or it has nothing to take focus (a report panel before its
pages have arrived or with no pages yet, a project pane while it is still loading, see "Loading the project pane" in
`docs/product/project-pane.md`), and the terminal when no session is selected or while the archive view covers it. A
pane counts as shown when it is docked in the row at 1148 px and wider, or its drawer is open below that. A docked
sidebar column that is still easing closed already counts as hidden, and one that is still easing open already counts
as shown (see "The two pane toggles" in `docs/product/window-layout.md`).

Where focus lands in each region:

| Region | Lands on |
|---|---|
| Top bar | its first enabled control |
| Rail | the current console's avatar, or the rail's first control when there is no console |
| Sidebar | the selected session's row, or the sidebar's first control when that row is not on screen |
| Archive view | its first control |
| Terminal | the terminal, so typing reaches the agent; its cover's Reconnect button instead while that is offered (see "The terminal" in `docs/product/sessions.md`) |
| Right pane, showing a report panel | the pager's first enabled button, else the report page itself |
| Right pane, showing a project pane | its header's tab for the mode shown (Files or Git), from which Tab goes on to Refresh and then to the file tree, or in Git to the tab of the view shown (Uncommitted or Compare); Try again while the pane failed to load |
| Connection banner | its Retry button; it has none while the banner only says it is reconnecting, and F6 then skips it |
| Toasts | the newest toast |

Pressing Retry from the keyboard works as with the mouse, and when the banner goes away while Retry holds focus, focus
goes to the terminal.

The control focus lands on shows its focus ring, even when the last input before `F6` was the mouse.

`F6` does nothing while a modal dialog, Settings included, or a menu is open; focus stays where it is.
