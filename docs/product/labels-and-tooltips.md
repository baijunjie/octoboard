# Labels and tooltips

How the window's controls and names are labelled where their text does not fit or is not shown, and which
explanations are kept out of the visible text.

## How a tooltip behaves

Every tooltip in the window is the application's own, drawn in its own styling and under one set of rules, whatever
carries it — a control, an indicator or a label too long for its space:

- It opens once the pointer has rested on what carries it for 500 ms. Within half a second of
  another tooltip closing it opens at once instead, so moving from one to the next does not wait again at each one,
  whether along a row of controls or between two marks side by side, such as a session's status glyph and its agent
  icon.
  On a control it also opens the instant the control takes keyboard focus, with no wait (see "Tooltips on icon-only
  controls" below).
- **Only one is open at a time**: opening one takes any other away.
- It goes the moment the pointer leaves what carries it, with no lingering, and it goes when what it sits over is
  scrolled. It takes no pointer input of its own, so a press lands on whatever is under it.
- A tooltip on clipped text or a change row's status letter closes on any pointer press or key press in the window,
  even while waiting to open. It stays closed until the pointer leaves and returns, including when the press opens
  a dialog without moving the pointer. The tooltip showing an OSC 8 hyperlink's target in a session's terminal also
  closes on any pointer press or key press (see "Links in the output" in `docs/product/session-terminal.md`).
- Text too long for the tooltip's width **wraps at word boundaries**. A single run with no boundary in it — a path, a
  long branch name — is broken within rather than widening the tooltip. A script written without spaces wraps between
  its characters, which is its word boundary.

Two places show the operating system's own tooltip instead, which follows the system's styling and timing: a link
inside a Markdown document in the file viewer, where — for a web or mail address, or a path in the project — the
address it leads to is also what assistive technology is given as the link's description (see "A Markdown file as a
document" in `docs/product/project-pane.md`), and the separators standing for the lines a diff leaves out (see
"Expanding the collapsed lines" in `docs/product/project-pane-git-change-viewer.md`).

## Tooltips on icon-only controls

A control shown only as an icon has a tooltip naming it, shown both when the mouse rests on it and when it receives
keyboard focus. The tooltip's text is the same name assistive technology announces for the control, except on the ⋮
buttons, the pin buttons and the session Delete or Archive shortcuts below. These are:

- the top bar's controls — Back, Forward, the sidebar toggle at its start and the right pane's toggle at its end;
  each pane toggle's tooltip follows its "Show …" / "Hide …" label, and the right pane's also names what the pane
  shows (see "The two pane toggles" in `docs/product/window-layout.md`);
- the rail's buttons — each console's avatar, whose tooltip is the console's name with what is going on in it folded
  in, the same text as the name announced for it (see "The console switcher" in `docs/product/sidebar.md`), New
  console, the waiting count, whose tooltip says how many sessions are waiting and that pressing it goes to the next
  one, Turn on notifications, Account quota and Settings. Account quota's floating panel opens sooner than its
  tooltip would (see "Opening and pinning the panel" in `docs/product/account-quotas.md`), so a resting mouse meets
  the panel; the tooltip is held back while the panel is open. When the panel closes with the pointer resting on the
  button or keyboard focus on it — Escape after a keyboard pin returns focus there — the tooltip stays shut until
  focus or the pointer leaves the button; closed by a press elsewhere, nothing is held back;
- the ⋮ button that opens an action menu — a sidebar row's, the console's in the sidebar's header, the project's in
  focus mode, and a file or folder's in the Files mode's tree or a file's in the Git mode's change list (see "A
  row's actions" in `docs/product/project-pane.md`). Its tooltip is a short "More actions", while the name announced
  for it also names what it belongs to ("Actions for session …", "Actions for file …" and the like), so that each ⋮
  can be told apart;
- the sidebar's other icon buttons — a project's **+** (New session), wherever it appears, the Projects heading's
  Filter projects / Clear filter button (see "Filtering the project list" in `docs/product/sidebar.md`) and its
  button to expand projects with unarchived sessions or collapse all projects (see "Expanding and collapsing the
  listed projects" in the same doc), and focus mode's Leave focus mode;
- the pin button of a pinned project or session, whose tooltip is a short "Unpin", while the name announced for it
  also names what it unpins (see "Pinning" in `docs/product/sidebar.md`);
- the Delete or Archive shortcut of a session row, a focus mode's session card or its archive preview row, whose
  tooltip is a short "Delete" or "Archive", while the name announced for it also names the session it acts on
  (see "Session rows" in `docs/product/sidebar.md`);
- the archive view's Close button and each of its rows' Delete button, whose tooltip names the session;
- the three options of Settings' Appearance row — Light, Dark and System (see "Appearance" in
  `docs/product/settings.md`);
- the report panel's previous-page and next-page buttons;
- the project pane's Refresh button, a folder's arrow button in its file tree and in the Git mode's change list, and a
  commit row's arrow button there ("Expand" / "Collapse"), the file viewer's Copy path button and its Previous file
  and Next file buttons (see `docs/product/project-pane.md`);
- the file viewer's three view controls and the Git mode's three view toggles — the one that shows and hides the
  change filter field, Group by folder and Group by commit — whose tooltip is the name they announce and alternates with their state (see
  "Two-state icon toggles" below);
- the two controls of the console dialog's Avatar field (see "Avatar" in
  `docs/product/consoles.md`) — the round control itself, named for choosing an image while it is empty
  and for replacing it once one is picked, and the delete button over its corner, named for removing the image;
- the "?" help tip, a small muted icon button that takes an explanation out of a row's visible text: its tooltip is the
  explanation, and the name announced for it says what it explains ("About the default account"), so the explanation
  is the control's description while the tooltip is open. Pressing it (a click, a tap, Enter or Space) opens the same
  explanation in a popover instead, which holds the tooltip back while it is open, stays until an outside press,
  Escape or another press of the "?" closes it, and gives focus back to the "?" — the way a touch screen, which has no
  hover, reaches it. When the popover closes with the pointer resting on the "?" or the keyboard in use, the tooltip
  stays shut until focus or the pointer leaves the "?", so it does not reopen at once; closed by an outside press with
  the pointer elsewhere, nothing is held back, and the next rest on the "?" shows the tooltip;
- the close button of every dismissable dialog, of Settings and of each toast ("Close").

While the docked sidebar is hidden at 1168 px and wider, a console's avatar opens its tooltip on keyboard focus only:
resting the mouse on it floats the sidebar in instead (see "Previewing a console from the rail" in
`docs/product/sidebar.md`). That card names the console at its top but not what is going on in it, so **a mouse user
with the docked sidebar hidden never reads the avatar's tooltip**; what is going on is then read from the sessions the
card lists, each with its own status glyph and that glyph's tooltip (see "Tooltips on indicators" below).

## Explanations in help tips

An explanation that only needs to be read once, and that is not needed on screen while operating what it explains,
goes into a "?" help tip rather than a line of text of its own (for how the help tip behaves see "Tooltips on
icon-only controls" above). What stays in the visible text instead:

- status and warnings — an agent that is not installed, an account that is not usable, an error;
- the consequences and timing the user has to weigh before acting — when a change takes effect, an extra cost, what
  is lost;
- what the user needs to read while operating the control — a format, a limit, the meaning of the current value.

The help tip sits at the very end of the line it explains, after the label and anything else on that line, such as
an account's plan. It is used on the default account's row in Settings (see "Agent accounts" in
`docs/product/settings.md`).

## Two-state icon toggles

A control that switches a view between two states is **one icon button**, not a set of choices side by side. These are
the two pane toggles (see "The two pane toggles" in `docs/product/window-layout.md`), the file viewer's three view
controls (see "The file viewer" in `docs/product/project-pane.md`), the Files mode's file-name filter toggle
(see "Filtering by file name" in that doc), and the Git mode's three — the toggle of its change
filter field (see "Filtering by file name" in `docs/product/project-pane-git-change-list.md`), Group by folder (see "Flat
or grouped by folder" in the same doc) and Group by commit (see "Grouped by commit" in the same doc). For all of them:

- Pressing the button switches the state, and its icon becomes the other state's.
- **The icon shows the state the view is in; the name says what pressing will do**, so the name alternates with the
  state. That name is both the tooltip and what assistive technology announces, as "Tooltips on icon-only controls"
  above says. Assistive technology is also told which state the control is in: whether the button is pressed; a pane
  toggle says instead whether its pane is expanded, and each file-name filter toggle says both, pressed while its
  field is on screen.
- **Nothing about the state is said in colour**: the button in its pressed state is drawn exactly as in its
  unpressed one.

Settings' Appearance choice is not one of these: being a choice among three, it stays a three-way segmented control,
where the chosen option is marked by a fill of its own rather than by its icon (see "Appearance" in
`docs/product/settings.md`).

## Tooltips on indicators

A few marks that are not controls carry a tooltip too. They are not tab stops, so their tooltip shows only while the
mouse rests on them, never on keyboard focus; what it says reaches assistive technology through the name of the row or
badge they sit in, or the text beside them, and where nothing around a mark says it, through the mark's own name.

Carrying a tooltip makes none of them a control. A mouse press on such a mark does nothing of its own and takes no
keyboard focus; where the mark sits in something that answers a press, the press goes on to that and does what a press
there does — pressing a session's status glyph on its sidebar row selects the session, as pressing the row does, and
pressing a change row's status letter opens the change. So such a mark is no dead spot in what carries it.

These are:

- a session's status glyph on its sidebar row, on the top bar's breadcrumb and on each non-archived session a
  confirmation lists among the ones it affects, whose tooltip says the status in the same words as everywhere else
  it is named — the placements where no status text sits beside the glyph (see "Session statuses" in
  `docs/product/sessions.md`);
- the activity marker, the mark standing for what is going on beneath a **collapsed project's row** in the sidebar
  (see "Project rows" in `docs/product/sidebar.md`) and inside focus mode's **console-switch chip** (see "The switch
  strip" in `docs/product/focus-mode.md`), whose tooltip says which activity it stands for, in the same words the name
  of the row or chip around it folds in beside the project's or console session's own name. Beneath a collapsed
  project's row that is one of three — a session waiting for the user, one working, one merely running — while a chip
  only ever shows the first two of them. The same activity shown as a badge on a console's avatar in the rail adds no
  tooltip of its own: the badge is decorative, and the avatar's own tooltip already says the activity, folded in beside
  the console's name (see "The console switcher" in `docs/product/sidebar.md`). So one activity is worded two ways — on
  its own in the marker's tooltip, folded into the name in the avatar's;
- the binding badge, the coloured dot on a project session's row bound to a console session, whose tooltip names that
  console session (see "The binding badge" in `docs/product/sidebar.md`);
- a session's agent icon on its row, on a focus mode's card or archived row, on the archive view's row and on each
  session a confirmation lists among the ones it affects, whose tooltip names the agent and the account the session
  runs under (see "Session rows" in `docs/product/sidebar.md`);
- a project's branch badge, on its sidebar row and in focus mode's header, which carries **one tooltip for the whole
  badge** rather than one per mark: resting the mouse on its state glyph, on the branch name or on either upstream
  count shows the same sentence, the one the badge's whole state is said in — the branch, each count while it is not
  zero and a failed check's message. It shows whether or not the badge's text is cut, and the one state that says in
  visible text that the repository has no commits yet carries no tooltip (see "The branch badge" in
  `docs/product/project-git-status.md`);
- the responsible-session badge, the ring a branch carries in the Git mode's branch lists, whose tooltip names the
  session working on the branch, says so differently when that session no longer exists and when it is the selected
  one, and is on the ring itself rather than on the row around it (see "The responsible-session badge" in
  `docs/product/branch-responsible-session.md`);
- a change's status letter on its row in the Git mode's change list, flat or grouped by folder, whose tooltip names
  the status in the same word the file viewer's header carries as the tag of an opened change, and is on the letter
  itself rather than on the row around it, the name and the detail beside it keeping their own tooltips (see "Rows
  and the keyboard" in `docs/product/project-pane-git-change-list.md`).

## Names too long for their space

A single-line name or label too long for its space — a sidebar row's name, a focus mode session card's title, the top
bar's breadcrumb, a session listed in a confirmation, the report panel's page timestamp, a name in a project pane's file
tree or its header, a name or what follows it in the Git mode's change list, a commit's subject on its commit row there,
a branch's name in its branch selector, a picked tag's label (see "Picked tags" in `docs/product/project-tags.md`) and
the project filter's strip of conditions in the sidebar's Projects heading, which counts as one label (see "Filtering
the project list" in `docs/product/sidebar.md`) — fades out where it is cut off rather than ending in an ellipsis. The
same holds for action-menu items and submenu names, a select's current choice or placeholder and its listed options,
directory-picker rows, the console name above a focus view's title, the archive view's heading and count, the model line
on a focus mode card and the agent name on a full archive row. A fixed session-owner label and a diff's collapsed-line
label follow the same fade and marquee rules.

Except for the report timestamp, clipped text then has its full text as its tooltip — for the project filter's strip,
its conditions drawn as the same tags the strip shows (see "Filtering the project list" in `docs/product/sidebar.md`)
— shown while the pointer is on the label and not on keyboard focus; the label's whole text is in the document whether or not it is cut, so assistive
technology is given it either way. The diff separator's tooltip follows the operating system's rules instead.

**A path is cut at its start instead**, so that its last folder stays in view (see "Right-to-left layout" in
`docs/product/window-layout.md`); every other label is cut at its end. Everything this section says holds for a label
cut at its start, mirrored: it rests pinned to its end, fades at its start, and carries its full text as its tooltip
while it is cut.

A project's branch badge also follows these fade and marquee rules, with its branch and upstream counts sharing one
line. Its tooltip is not one of this section's, though: the whole badge carries one tooltip describing the full git
state, shown whether or not the line is cut (see "The branch badge" in `docs/product/project-git-status.md`).

**While the pointer is over it, such a label runs as a marquee**, so all of it can be read:

- The pointer has to be over the label itself. In sidebar rows, focus headers and session cards, and archive headers
  and rows, each text label runs independently: a project's name and branch-and-count line, a console name and focus
  title, a session title and its model line or agent name, or an archive heading and count do not start one another's
  marquee.
  Pointing at a status or agent icon, a branch glyph, a row or header button, or the surrounding space starts none of
  those labels. The branch and its counts remain one line and run together; session usage and time fields remain
  separate from the model line's and the agent name's marquees.
- An action-menu item or submenu item, a select's option or directory-picker row runs its one label while the pointer
  is anywhere over that item or row. A select's current choice or placeholder runs while the pointer is over its
  trigger, and a picked tag while it is over the tag. The project filter's strip of conditions runs as a whole while
  the pointer is anywhere over it, its tags moving together rather than each on its own. A fixed session-owner label and a diff's collapsed-line label
  run over their text controls.
- After an initial pause of about 600 ms the text scrolls at reading speed, about 60 px a second, toward the edge it
  is cut at — a label cut at its end scrolls forward through its text, one cut at its start scrolls back toward its
  beginning. It pauses there for three seconds, returns to where it rests in about 200 ms, then waits three seconds
  before starting the next reading pass for as long as the pointer stays. The pacing is the same whichever way it
  runs.
- When the pointer leaves, the text immediately resets to where it rests: the beginning of a label cut at its end,
  the end of one cut at its start.
- The fades follow the text: an edge fades while text is hidden behind it and stops fading once that text is in view,
  so the cut edge stops fading as the run reaches it and the opposite edge fades meanwhile.
- Nothing moves where the system asks for reduced motion.
- On a picked tag, where the remove button covers the label's end while the pointer is over the tag, the text scrolls
  until its end is clear of the button (see "Picked tags" in `docs/product/project-tags.md`).
