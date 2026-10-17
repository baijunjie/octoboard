# Split right pane and built-in terminals

## Problem

The right pane holds one thing at a time: either the project browser (Files / Uncommitted / Compare) or a console
session's report. There is no way to see the project's files and run a command beside them; running a command means
the "Open terminal" action, which leaves the app for Terminal.app.

The panes' inner horizontal padding is also inconsistent. Content in the sidebar starts 16px from the pane edge
throughout; in the right pane the header starts at 8px, tree and change rows at 12px (their hover fill at 4px), and
the filter bar, Compare's From / To row, section headings and notices at 12px. No shared token carries the inset;
every value is a literal at its call site.

## Plan outline

1. Give both panes one horizontal inset token and bring the right pane onto it.
2. Replace decorative outline borders with a 4px thin shadow, edge panel cards with a 0.5px hairline and that shadow,
   and draw dividers as a 0.5px double line that reads as a groove.
3. Turn the project's right pane into a vertical stack of panels, in the manner of Claude's desktop app: each panel
   kind (Files, Terminal to begin with) is its own card with a header holding its title, actions, maximize and close,
   a resize handle sits in the gap between neighbouring cards, and the top bar gets one toggle per panel kind beside
   the pane's own toggle.
4. Let the daemon run plain shell terminals that belong to a project, beside the agent sessions it already runs.
5. Add the Terminal panel: several terminal tabs per project, following the project the way the file browser does.

## Key design decisions

- **The inset is 16px in both panes**, the sidebar's current value. Rows keep the sidebar's structure: an 8px outer
  inset for the hover and selected fill, content at 16px. Only the right pane changes visibly.
- **No decorative outline borders, except a panel card's hairline.** Every decorative outline border gives way to the
  4px thin shadow. The one exception, after Claude's desktop app, is a panel card — each panel in the split right pane,
  and the floating pane and the drawer while they hold a report: it keeps a 0.5px hairline outline together with the
  shadow, a 1px line on a 1x display. Borders that identify a control (form fields and the like) and borders that show a
  selected state stay, at the 3:1 contrast they need.
- **The right pane is its panel cards.** In the project view the right pane has no frame of its own, in any form: it is
  the stack of panel cards, as in Claude's desktop app. The cards fill the pane edge to edge with no margin around them,
  and only a gap between neighbouring cards. The gap shows the sidebar's background in every form, so a floating pane or
  a drawer never shows the agent terminal through it. The pane's 16px inset is measured from the card's edge.
- **Dividers are grooves.** A divider is a 0.5px double line, dark beside light, that reads as recessed. A 1x display
  cannot draw it — two half-pixel lines blend into one pixel of a single colour, scaled or not — so there the divider
  falls back to a single 1px dark line, the same thickness as on a high-density display, without the recess.
- **Splitting applies to the project view only.** A console session's report stays a single-owner pane with no panel
  stack and no terminal toggle. This keeps the first version to one owner kind; a terminal needs a project's
  directory anyway.
- **The pane's own toggle stays.** It still shows and hides the right pane as a whole, floats it in on hover and opens
  it as a drawer in a narrow window. The per-kind toggles beside it choose which panels the stack shows.
- **Panels are a registry of kinds**, so a later kind (such as a browser) adds an entry rather than a new layout.
- **The daemon owns the terminals.** A terminal lives on after its panel or tab view is closed, and after a window is
  closed while the daemon keeps running (background running, or a browser client); a client reopening the project
  finds its tabs and their scrollback. A terminal ends when the user closes its tab, its shell exits, its project is
  removed, or the daemon exits.
- **Terminals follow the project.** Each project has its own set of terminal tabs; switching project shows that
  project's set, and the others keep running in the background.
- **A terminal keeps Escape.** Inside a floating or drawer right pane, Escape with focus in a shell terminal goes to
  the shell (vim, less, fzf and the like need it) instead of dismissing the pane. The pane is dismissed with its close
  control, its toggle or the scrim. The agent terminal in the centre keeps today's behaviour.
- **A built-in shell is a remote shell.** It adds no new class of exposure on the loopback-only daemon, which already
  lets anything local type into agent PTYs, but it makes arbitrary command execution one click away in any client.
  The server form must not ship with it before remote access has credentials.

## Milestones

1. 01 One horizontal inset for both panes (closed)
2. [Shadow edges and groove dividers](02-shadow-edges-and-groove-dividers.md)
3. [Split right pane](03-split-right-pane.md)
4. [Shell terminals in the daemon](04-daemon-shell-terminals.md)
5. [Terminal panel](05-terminal-panel.md)

## Open

- Whether a vertical groove mirrors under right-to-left, which also flips the light direction it implies.
- What a per-kind toggle does while the whole pane is hidden: show the pane with that panel, or only change the stack
  for the next time the pane shows.
- What closing the last panel in the stack does: close the pane, or leave it empty.
- Whether the stack may hold two panels of the same kind, or each kind appears at most once.
- What maximize does to the other panels: collapse them to their headers, or hide them until restored.
- Where a new terminal starts: the project root, or the worktree the project pane's Git mode has selected.
- Whether opening the Terminal panel on a project with no terminal starts one, or shows an empty state.
- What a tab whose shell has exited shows: it stays with its final screen until closed, or it disappears. This
  decides whether milestone 4's record keeps an exited state and its final screen until the tab is closed, and which
  broadcast a shell exit sends; settle it before milestone 4 starts.
- Whether a terminal tab's title follows the shell's title (OSC sequences) or stays as numbered by the app, and
  whether the user can rename it.
- How many terminals a project may have, and whether closing a tab whose shell is still running asks first.
- Whether running shells count toward the confirmation on quitting, which today asks only while an agent session runs.
- Which of the shell environment snapshot's cache policies a shell launch uses, and whether a failed capture refuses
  the launch as it does for an agent.
