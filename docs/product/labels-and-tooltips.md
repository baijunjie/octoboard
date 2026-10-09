# Labels and tooltips

How the window's controls and names are labelled where their text does not fit or is not shown.

## Tooltips on icon-only controls

A control shown only as an icon has a tooltip naming it, shown both when the mouse rests on it and when it receives
keyboard focus. The tooltip's text is the same name assistive technology announces for the control, except on the
consoles' avatars, the ⋮ buttons and the pin buttons below. These are:

- the top bar's controls — Back, Forward, the sidebar toggle, whose tooltip follows its "Show …" / "Hide …" label, and
  the terminal connection indicator at the breadcrumb's end, in both its states (see "The terminal connection
  indicator" in `docs/product/window-layout.md`);
- the rail's buttons — each console's avatar, whose tooltip is the console's name while the name announced for it also
  says what is going on in it (see "The console switcher" in `docs/product/sidebar.md`), New console, the waiting
  count, whose tooltip says how many sessions are waiting and that pressing it goes to the next one, Turn on
  notifications, the right pane's toggle, whose tooltip follows its "Show …" / "Hide …" label and
  names what the pane shows (see "The two pane toggles" in `docs/product/window-layout.md`), and Settings;
- the ⋮ button that opens an action menu — a row's, the console's in the sidebar's header, and the project's in
  focus mode. Its tooltip is a short "More actions", while the name announced for it also names what it belongs to
  ("Actions for session …" and the like), so that each ⋮ can be told apart;
- the sidebar's other icon buttons — a project's **+** (New session), wherever it appears, the Projects heading's
  Filter projects and its Expand all projects or Collapse all projects button (see "Expanding and collapsing the
  listed projects" in `docs/product/sidebar.md`), the Clear filter under that heading (see "Filtering the project
  list" in the same doc), and focus mode's Leave focus mode;
- the pin button of a pinned project or session, whose tooltip is a short "Unpin", while the name announced for it
  also names what it unpins (see "Pinning" in `docs/product/sidebar.md`);
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

## Tooltips on indicators

A few marks that are not controls carry a tooltip too. They are not tab stops, so their tooltip shows only while the
mouse rests on them, never on keyboard focus; what it says reaches assistive technology through the name of the row or
badge they sit in, or the text beside them. These are:

- the binding badge, the coloured dot on a project session's row bound to a console session, whose tooltip names that
  console session (see "The binding badge" in `docs/product/sidebar.md`);
- a session's agent icon on its row, on a focus mode's card or archived row and on the archive view's row, whose
  tooltip names the account the session runs under (see "Session rows" in `docs/product/sidebar.md`);
- the branch badge's warning triangle, whose tooltip carries the failed check's message (see "The branch badge" in
  `docs/product/project-git-status.md`).

## Names too long for their space

A single-line name or label too long for its space — a sidebar row's name, the top bar's breadcrumb, a session listed
in a confirmation, the report panel's page timestamp, a name in a project pane's file tree or its header, a name or
what follows it in the Git mode's change list, a worktree's name or path in its worktree selector, a branch's name in
its branch selectors, a picked tag's label and the sidebar filter's keyword (see "Picked tags" in
`docs/product/consoles-and-projects.md`) — fades out where it is cut off rather than ending in an ellipsis. A name,
unlike the timestamp, then has its full text as its tooltip.

**While the pointer is over it, such a label runs as a marquee**, so all of it can be read:

- For a sidebar row the pointer only has to be over the row, and for a picked tag over the tag; elsewhere it has to be
  over the label itself.
- After a short pause the text scrolls through at reading speed, about 60 px a second, pauses at its end, scrolls
  quickly back to its start and goes round again for as long as the pointer stays.
- When the pointer leaves, the text scrolls back to its start.
- The fades follow the text: its start fades once text has scrolled past it, and its end stops fading once the last
  of the text is in view.
- Nothing moves where the system asks for reduced motion, and a label that is cut at its start, such as a path
  (see "Right-to-left layout" in `docs/product/window-layout.md`), never runs.
- On a picked tag, where the remove button covers the label's end while the pointer is over the tag, the text scrolls
  until its end is clear of the button (see "Picked tags" in `docs/product/consoles-and-projects.md`).
