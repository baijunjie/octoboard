# Labels and tooltips

How the window's controls and names are labelled where their text does not fit or is not shown.

## Tooltips on icon-only controls

A control shown only as an icon has a tooltip naming it, shown both when the mouse rests on it and when it receives
keyboard focus. The tooltip's text is the same name assistive technology announces for the control, except on the consoles'
avatars and the ⋮ buttons below. These are:

- the top bar's buttons — Back, Forward and the sidebar toggle, whose tooltip follows its "Show …" / "Hide …" label;
- the rail's buttons — each console's avatar, whose tooltip is the console's name while the name announced for it also
  says what is going on in it (see "The console switcher" in `docs/product/sidebar.md`), New console, the waiting
  count, whose tooltip says how many sessions are waiting and that pressing it goes to the next one, Reconnect
  terminal, Turn on notifications, the right pane's toggle, whose tooltip follows its "Show …" / "Hide …" label and
  names what the pane shows (see "The two pane toggles" in `docs/product/window-layout.md`), and Settings;
- the ⋮ button that opens an action menu — a row's, the console's in the sidebar's header, and the project's in
  focus mode. Its tooltip is a short "More actions", while the name announced for it also names what it belongs to
  ("Actions for session …" and the like), so that each ⋮ can be told apart;
- the sidebar's other icon buttons — a project's **+** (New session), wherever it appears, the Projects heading's
  Filter projects, the Clear filter beside it, and the Expand all projects or Collapse all projects button (see
  "Expanding and collapsing the listed projects" in `docs/product/sidebar.md`), and focus mode's Leave focus mode;
- the archive view's Close button and each of its rows' Delete button, whose tooltip names the session;
- the three options of Settings' Appearance row — Light, Dark and System (see "Appearance" in
  `docs/product/settings.md`);
- the report panel's previous-page and next-page buttons;
- the project pane's Refresh button, a folder's chevron in its file tree ("Expand" / "Collapse"), the file viewer's
  Previous file and Next file buttons (see `docs/product/project-pane.md`), and the Git mode's Swap branches (see
  "The branch selectors" in `docs/product/project-pane-git-mode.md`);
- the close button of every dialog, of Settings and of each toast ("Close").

While the docked sidebar is hidden at 1148 px and wider, a console's avatar opens its tooltip on keyboard focus only:
resting the mouse on it floats the sidebar in, which names the console at its top (see "Previewing a console from the
rail" in `docs/product/sidebar.md`).

## Names too long for their space

A single-line name or label too long for its space — a sidebar row's name, the top bar's breadcrumb, a session listed
in a confirmation, the report panel's page timestamp, a name in a project pane's file tree or its header, a name or
what follows it in the Git mode's change list, a worktree's name or path in its worktree selector, a branch's name in
its branch selectors — fades out where it is cut off rather than ending in an ellipsis. A name, unlike the timestamp,
then has its full text as its tooltip.

**While the pointer is over it, such a label runs as a marquee**, so all of it can be read:

- For a sidebar row the pointer only has to be over the row; elsewhere it has to be over the label itself.
- After a short pause the text scrolls through at reading speed, about 60 px a second, pauses at its end, scrolls
  quickly back to its start and goes round again for as long as the pointer stays.
- When the pointer leaves, the text scrolls back to its start.
- The fades follow the text: its start fades once text has scrolled past it, and its end stops fading once the last
  of the text is in view.
- Nothing moves where the system asks for reduced motion, and a label that is cut at its start, such as a path
  (see "Right-to-left layout" in `docs/product/window-layout.md`), never runs.
