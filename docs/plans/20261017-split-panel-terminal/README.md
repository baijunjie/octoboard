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
2. Replace decorative outline borders with a short two-layer shadow, edge panel cards with a 0.5px hairline and that shadow,
   and draw dividers as a 0.5px double line that reads as a groove.
3. Turn the right pane into a vertical stack of panels, in the manner of Claude's desktop app: each panel kind (Files,
   Terminal and a console session's Report to begin with) is its own inset card with a header holding its title,
   actions, maximize and close, a resize handle sits in the gap between neighbouring cards and another on the stack's
   inner edge, and the top bar's one toggle per panel kind replaces the pane's own toggle.
4. Let the daemon run plain shell terminals that belong to a project, beside the agent sessions it already runs.
5. Add the Terminal panel: several terminal tabs per project, following the project the way the file browser does.

## Key design decisions

- **The inset is 16px in both panes**, the sidebar's current value. Rows keep the sidebar's structure: an 8px outer
  inset for the hover and selected fill, content at 16px. Only the right pane changes visibly.
- **No decorative outline borders, except a panel card's hairline.** Every decorative outline border on a card gives
  way to the short two-layer shadow. The one exception, after Claude's desktop app, is a panel card — each panel in the split
  right pane, the floating pane and the drawer while they hold a report, and the sidebar's floating card and drawer: it
  keeps a 0.5px hairline outline, a 1px line on a 1x display. Borders that identify a control (form fields and the
  like) and borders that show a selected state stay, at the 3:1 contrast they need.
- **A floating window keeps its elevation shadow.** The short two-layer shadow is for cards lying in the layout. Whatever
  floats over the content — the floating cards and the drawers of both panes, the notice floating over the terminal —
  keeps the larger elevation shadow it has today, and loses only its outline border (a panel card keeping its
  hairline).
- **The right pane is its panel cards, inset.** The right pane has no frame of its own, in any form: it is the stack
  of panel cards, as in Claude's desktop app. The cards are inset: the same gap separates them from each other, from
  the top bar, from the window's right and bottom edges and from the agent terminal. Behind them is the agent
  terminal's background, so the cards read as floating on the content; a floating pane is only its cards, with the
  agent terminal showing in the gaps. A single open panel takes the stack's full height. The
  pane's 16px inset is measured from the card's edge.
- **Dividers are grooves.** A divider is a 0.5px double line, dark beside light, that reads as recessed. A 1x display
  cannot draw it — two half-pixel lines blend into one pixel of a single colour, scaled or not — so there the divider
  falls back to a single 1px dark line, the same thickness as on a high-density display, without the recess. A
  vertical groove does not mirror under right-to-left: its dark line stays on the left, so the light it implies keeps
  one physical direction.
- **Rendered documents keep their own rules.** The lines and frames a rendered document draws (a Markdown theme's
  table rules, quote bar and heading rules, a fenced code block's frame, a footnote rule) are the document's
  typography, not the app's chrome, and stay as they are.
- **Each owner shows its own kinds.** A project's stack holds Files and Terminal; a console session's holds its Report,
  a panel kind like the others (an inset card with the same header and toggle). A terminal needs a project's directory,
  so a console session has no Terminal toggle.
- **The top bar has only the per-kind toggles.** The right pane has no toggle of its own: it shows while at least one
  panel is open, and closing the last panel hides it. While no panel is open, hovering a kind's toggle floats that
  panel's card over the agent terminal, and clicking the toggle pins it: the card docks, taking room from the agent
  terminal. Neither slides: a hovered card floats in with the sidebar's floating card animation (fading in as it
  rises), and a pinned card docks at once, with no drawer animation. A toggle keeps its icon (Files: a closed folder)
  and shows its panel open with the pressed fill only. There is no drawer form:
  a narrow window docks the pane too, its width clamped. The pane's width is set by a resize handle on the stack's
  inner edge, drawn as a grip, as in Claude's desktop app; the handles between cards are drawn the same way.
- **Panels are a registry of kinds**, so a later kind (such as a browser) adds an entry rather than a new layout.
- **The daemon owns the terminals.** A terminal lives on after its panel or tab view is closed, and after a window is
  closed while the daemon keeps running (background running, or a browser client); a client reopening the project
  finds its tabs and their scrollback. A terminal ends when the user closes its tab, its shell exits, its project is
  removed, or the daemon exits.
- **Terminals follow the project.** Each project has its own set of terminal tabs; switching project shows that
  project's set, and the others keep running in the background.
- **A terminal keeps Escape.** Inside a floating right pane, Escape with focus in a shell terminal goes to the shell
  (vim, less, fzf and the like need it) instead of dismissing the pane. The pane is dismissed with its close control,
  its toggle or by moving the pointer away. The agent terminal in the centre keeps today's behaviour.
- **A built-in shell is a remote shell.** It adds no new class of exposure on the loopback-only daemon, which already
  lets anything local type into agent PTYs, but it makes arbitrary command execution one click away in any client.
  The server form must not ship with it before remote access has credentials.
- **Stack rules.** Each panel kind appears in the stack at most once; several terminals are tabs inside the one
  Terminal panel. A per-kind toggle pressed while the whole pane is hidden shows the pane with that panel open.
  Closing the last panel in the stack closes the pane. Maximizing a panel hides the other panels until it is restored.
- **Shell terminal rules.** A new terminal starts in the worktree the project pane's Git mode has selected, the project
  root when none is. Opening the Terminal panel on a project with no terminal starts one. A tab whose shell has exited
  stays with its final screen until the user closes it, so the daemon's record keeps an exited state and its final
  screen until then. A tab's title follows the shell's title (OSC sequences) until the user renames it. A project may
  have any number of terminals.
- **A shell counts as busy while it runs something.** The daemon judges a shell busy when its PTY's foreground process
  group is not the shell itself or the shell has any child process (which covers a server sent to the background).
  Closing a busy shell's tab asks first; an idle one closes without asking. Quitting asks while any shell is busy, as
  it does while an agent session runs; idle shells do not count.
- **A shell launch uses the agent's environment snapshot policy**, but a failed capture does not refuse it: the login
  shell starts with the inherited environment and loads its own profile.

## Carrying out this plan

The user's standing instruction for whoever picks this plan up:

- The main session does not write the code: it dispatches subagents (`implement`, or `investigate` for harder work)
  for all development, and keeps to briefing, reviewing their results, running the delivery checks, reporting to the
  user and merging.
- It carries the remaining milestones one after another itself, through the final confirmation and the deletion of
  this directory, without handing the plan to another session. Each milestone gets its own worktree and branch, and
  is closed and merged only after the user confirms its preview.
- It asks the user about an Open item, with options to pick from, as soon as a milestone depends on it.

## Milestones

1. 01 One horizontal inset for both panes (closed)
2. 02 Shadow edges and groove dividers (closed)
3. 03 Split right pane (closed)
4. 04 Shell terminals in the daemon (closed)
5. [Terminal panel](05-terminal-panel.md)
6. [Final confirmation](06-final-confirmation.md)
