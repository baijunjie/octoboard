# Labels and tooltips

How the window's controls and names are labelled where their text does not fit or is not shown, and which
explanations are kept out of the visible text.

## How a tooltip behaves

Every tooltip in the window is the application's own, drawn in its own styling and under one set of rules, whatever
carries it — a control, an indicator or a label too long for its space. The floating surfaces that hold more than a
label — the popover of a status mark (a failure, a warning or a note, see "Failures, warnings and notes marked at their
point" in `docs/product/error-alerts.md`) or of the "?" help tip (see "The help tip" below), and the rail's account
quota panel (see `docs/product/account-quota-panel.md`) — are drawn the same way, on the same surface, with the same
shadow and corners (a mark's and a help tip's popover have the tooltip's padding and text size too; the quota panel
keeps its own width, padding and text sizes, being a panel):

- It opens once the pointer has rested on what carries it for 500 ms. Within half a second of
  another tooltip closing it opens at once instead, so moving from one to the next does not wait again at each one,
  whether along a row of controls or between two marks side by side, such as a session's status glyph and its agent
  icon.
  On a control it also opens the instant the control takes keyboard focus, with no wait (see "Tooltips on icon-only
  controls" below).
- **Only one is open at a time**: opening one takes any other away.
- It goes the moment the pointer leaves what carries it, with no lingering, and it goes when what it sits over is
  scrolled. It also goes once what carries it is put out of use — as when a dialog, menu or popover opens over it (see
  "What an open dialog, menu or popover covers" in `docs/product/window-layout.md`) — even while the pointer rests where
  it was. It takes no pointer input of its own, so a press lands on whatever is under it.
- Where the system asks for reduced motion, it appears and goes at once, without fading or scaling.
- A tooltip on clipped text or a change row's status letter closes on any pointer press or key press in the window, even
  while waiting to open. It stays closed until the pointer leaves and returns, including when the press opens a dialog
  without moving the pointer. The tooltip showing an OSC 8 hyperlink's target in a session's terminal also closes on any
  pointer press or key press (see `docs/product/terminal-links.md`). So does the tooltip on the tab of an added agent
  that is not installed, in Settings, Escape included, and it stays closed until the pointer is off the tab and the tab
  has lost keyboard focus, or until keyboard focus arrives on the tab again (see "Tooltips on indicators" below).
- **A touch has no hover, so on a touch screen the same text opens in a popover.** Which of the two shows follows the
  input of the interaction, not the device: a mouse hover and keyboard focus open the tooltip, and a finger's or a pen's
  press opens the popover, on a phone, on a tablet with a trackpad and on a touch-screen laptop alike. A mouse press
  opens nothing extra, and neither does a keyboard press, since keyboard focus has the tooltip open already. The popover
  is drawn like the tooltip (the same surface, padding, text size and width) and takes focus while open; a tap outside
  it or Escape closes it — that tap does nothing else — and focus goes back to what held it. It opens on a mark that
  only carries a tooltip (which marks, see "Tooltips on indicators" below). It does not open on a control that has an
  action of its own, where a tap runs the action (an icon-only button, a menu trigger, a switch), nor on a mark inside a
  row, a button or an option, where the tap belongs to what holds the mark, nor on clipped text or a change row's status
  letter. Where a tap opens none, a touch screen has no route to the text.
- Text too long for the tooltip's width **wraps at word boundaries**. A single run with no boundary in it — a path, a
  long branch name — is broken within rather than widening the tooltip. A script written without spaces wraps between
  its characters, which is its word boundary.

Two places show the operating system's own tooltip instead, which follows the system's styling and timing: a link inside
a Markdown document in the file viewer, where — for a web or mail address, or a path in the project — the address it
leads to is also what assistive technology is given as the link's description (see
`docs/product/markdown-documents.md`), and the separators standing for the lines a diff leaves out (see "Expanding the
collapsed lines" in `docs/product/project-pane-git-change-viewer.md`).

## Tooltips on icon-only controls

A control shown only as an icon has a tooltip naming it, shown both when the mouse rests on it and when it receives
keyboard focus. The tooltip's text is the same name assistive technology announces for the control, except on the ⋮
buttons, the pin buttons, the session Delete or Archive shortcuts and the file viewer's File manager and Default
application buttons below. These are:

- the top bar's controls — Back, Forward, the sidebar toggle at its start and the panel toggles at its end; each
  toggle's tooltip follows its "Show …" / "Hide …" label (see "The sidebar toggle and the panel toggles" in
  `docs/product/window-layout.md`), and each panel toggle's also names its panel (see "The panel toggles in the top
  bar" in `docs/product/right-pane-panels.md`);
- the rail's buttons — each console's avatar, whose tooltip is the console's name with what is going on in it folded
  in, the same text as the name announced for it (see "The console switcher" in `docs/product/console-switcher.md`), New
  console, the waiting count, whose tooltip says how many sessions are waiting and that pressing it goes to the next
  one, Turn on notifications and Settings (the rail's Account usage button has none, as said below this list);
- the ⋮ button that opens an action menu — a sidebar row's, the console's in the sidebar's header, the project's in
  focus mode, a file or folder's in the Files mode's tree or a file's in the Git mode's change list (see "A
  row's actions" in `docs/product/project-pane.md`), and, in Settings' Agent accounts section, an account's and each
  agent tab's (see "The agent strip" in `docs/product/agent-accounts.md`). Its tooltip is a short
  "More actions", while the name announced for it also names what it belongs to ("Actions for session …", "Actions
  for file …" and the like), so that each ⋮ can be told apart;
- the sidebar's other icon buttons — a project's **+** (New session), wherever it appears, the Projects heading's
  Filter projects / Clear filter button (see "Filtering the project list" in `docs/product/sidebar.md`) and its
  button to expand projects with unarchived sessions or collapse all projects (see "Expanding and collapsing the
  listed projects" in the same doc), and focus mode's Leave focus mode;
- the pin button of a pinned project or session, whose tooltip is a short "Unpin", while the name announced for it
  also names what it unpins (see "Pinning" in `docs/product/order-of-projects-and-sessions.md`);
- the Delete or Archive shortcut of a session row, a focus mode's session card or its archive preview row, whose
  tooltip is a short "Delete" or "Archive", while the name announced for it also names the session it acts on
  (see "Session rows" in `docs/product/sidebar-session-rows.md`);
- the archive view's Close button and each of its rows' Delete button, whose tooltip names the session;
- the three options of Settings' Appearance row — Light, Dark and System (see "Appearance" in
  `docs/product/settings.md`);
- the report panel's previous-page and next-page buttons;
- a right pane panel's Close panel button and the Terminal panel's Maximize panel / Restore panel size (see "A
  panel's header" in `docs/product/right-pane-panels.md`);
- the project pane's Refresh and Collapse all folders buttons (see `docs/product/project-pane.md`), and the file
  viewer's Previous file and Next file buttons (see `docs/product/file-viewer.md`);
- the copy buttons — the file viewer's Copy path and the Git mode's Copy worktree path, on the change list's line
  naming the worktree — whose tooltip, and the name announced with it, turns to the copied wording after a press (see
  "Copy buttons" below);
- the file viewer's File manager and Default application buttons, whose tooltip is the short "File manager" or
  "Default application" that names the action in a project pane row's menu, while the name announced for each says
  what pressing it does, "Open in file manager" or "Open in default application" (see "The header" in
  `docs/product/file-viewer.md`);
- the file viewer's three view controls, the Files mode's two — its file-name filter toggle and the include-ignored
  toggle in its filter field — and the Git mode's three view toggles — the one that shows and hides the change filter
  field, Group by folder and Group by commit — whose tooltip is the name they announce and alternates with their state
  (see "Two-state icon toggles" below);
- the two controls of the console dialog's Avatar field (see "Avatar" in
  `docs/product/consoles.md`) — the round control itself, named for choosing an image while it is empty
  and for replacing it once one is picked, and the delete button over its corner, named for removing the image;
- the console dialog's Manage accounts button, beside the title of its Agent default accounts section (see "Agent
  config directories" in `docs/product/consoles.md`);
- the close button of every dismissable dialog, of Settings and of each toast ("Close").

The rail's **Account usage** button is an exception: it has **no tooltip**, on hover or on keyboard focus. Resting the
mouse on it floats its panel in instead (see "Opening the panel" in `docs/product/account-quota-panel.md`), and its
accessible name, which states the selected session's quota when the button shows one, is described in "The selected
session's quota indicator" in `docs/product/account-quotas.md`.

The **expand arrow** before a folder's row in the Files mode's tree and in the Git mode's change list, and before a
commit row there, is an exception too: it has **no tooltip** and no hover fill of its own, being a bare arrow glyph.
The row it sits on tints under the pointer, and a press anywhere on that row expands or folds it as the arrow does (see
"Rows and the keyboard" in `docs/product/project-pane.md`). Assistive technology names the arrow "Expand" or
"Collapse", followed by the row's name.

The right pane's resize grips — its width handle and the handles between its panels — have a tooltip too, a short
"Resize" shown beside the grip only while a mouse pointer rests on the handle, not on keyboard focus; the name
announced for each says what it resizes (see "Resizing the sidebar and the right pane" in
`docs/product/window-layout.md` and "Resizing panels" in `docs/product/right-pane-panels.md`).

While the docked sidebar is hidden at 1168 px and wider, a console's avatar opens its tooltip on keyboard focus only:
resting the mouse on it floats the sidebar in instead (see "Previewing a console from the rail" in
`docs/product/console-switcher.md`). That card names the console at its top but not what is going on in it, so **a mouse
user with the docked sidebar hidden never reads the avatar's tooltip**; what is going on is then read from the sessions
the card lists, each with its own status glyph and that glyph's tooltip (see "Tooltips on indicators" below).

### The help tip

The **"?" help tip** is a small muted icon button that takes an explanation out of the visible text (see "Explanations
in help tips" below). It is a tab stop, but it has **no tooltip**: the explanation — plain text, or text with literals
such as file names set apart as chips — opens in a popover, by the same rules as a status mark's (see "Failures,
warnings and notes marked at their point" in `docs/product/error-alerts.md`). A mouse resting on it opens the popover
without moving keyboard focus, and it stays open while the pointer moves into it, so its text can be selected and copied
with the mouse; a mouse click opens nothing. Keyboard focus alone shows nothing: Enter or Space, a finger's or a pen's
tap and a screen reader's activation open it with keyboard focus inside, and closing it gives focus back to the "?". The
name announced for the "?" says what it explains ("About automatic pulling"), and names its popover too.

## Explanations in help tips

Explanatory text stays visible by default. Only two kinds move into a "?" help tip (for how the help tip behaves see
"The help tip" above):

1. A principle-level explanation: how something works underneath, technical, and not something the user needs to care
   about while operating it.
2. An explanation only one item in a set carries: in a group of like items, the line that only one of them has. It sets
   that item apart from its siblings, and once understood it need not be read again.

Every other helping description stays visible, even one that only needs reading once. That includes the note under a
field or a setting row, the note at the foot of a form, a section's lead paragraph, status and warnings, and the
consequences and timing of an action.

The permission-approval explanation in Settings' Agent accounts section is an exception: its handling, timing and
Codex's possible extra token cost sit in a help tip beside the name of an agent's **Permission approval** row, whose
visible line says only that it applies to every account of the agent (see "Approval modes" in
`docs/product/per-agent-settings.md`). So, in part, is the **Share default configuration** row: what is shared and never
shared, what turning sharing off does to conversations and the idle sessions a change would end sit in the help tip
beside the row's name, while the row's line says what turning the switch on does, or what is the case while it is on,
and why the switch cannot be turned now stays under that line (see "Sharing the default configuration" in
`docs/product/agent-accounts.md`). So is what turning an account off means, in the help tip on the accounts table's
**Disable** heading; the **Disable account** row of an account's settings dialog says it in its own line instead (see
"Turning an account off" in `docs/product/agent-accounts.md`).

One status is a deliberate exception: an added agent that is not installed, in Settings' Agent accounts section. The
status itself stays visible, as a warning glyph after the agent's name on its tab, and each account the user owns of
that agent still says visibly, in its account details dialog, that it is not usable; only the glyph's explanation —
that the agent is not installed and its accounts are not usable until it is on `PATH` — sits in a tooltip on the tab,
which opens on hover and on keyboard focus (see "Tooltips on indicators" below, and "The agent strip" in
`docs/product/agent-accounts.md`). An agent that is not added has no tab and so no such status. That tooltip is not a
"?" help tip.

An explanation can be split: the part of either kind above goes into the help tip, and the rest stays visible. The help
tip sits at the end of the line it explains, beside the label rather than inside it, so the name announced for that
label or field does not include it. A section heading carries no help tip.

Help tips are used in these places:

- Settings' **Automatically pull repositories** row, at the end of the row's name: that the remote is checked every few
  minutes whichever way the switch is set (see "Git" in `docs/product/settings.md`);
- Settings' **Revert gate** row in the Git section, at the end of the row's name: what the gate blocks and how a
  deliberate undo passes it (see "Protection" in `docs/product/settings.md`);
- Settings' **Start usage windows automatically** row in the Agent accounts section, at the end of the row's name: why a
  usage window that has reset waits for the account's next request before it starts counting, and how the switch starts
  it (see "Start usage windows automatically" in `docs/product/settings.md`);
- the project dialog's **Default agent** field while associating a parent directory, beside the field's label: what
  "Detect from files" looks for in each repository and what happens to one it cannot decide on (see "The project's
  default agent" in `docs/product/projects.md`);
- each agent's **default account** in Settings' Agent accounts section, right after its directory in its account
  details dialog: that it is the agent's own default configuration directory, which new sessions use when no account is
  chosen and whose configuration other accounts can share (see "The default account" in
  `docs/product/agent-accounts.md`);
- the name of Claude Code's and Codex's **Permission approval** row in Settings' Agent accounts section (see "Approval
  modes" in `docs/product/per-agent-settings.md`);
- the **Share default configuration** row of an account's settings dialog: what is linked to the default account's
  directory, what is never shared and what happens when sharing is turned off (see "Sharing the default configuration"
  in `docs/product/agent-accounts.md`);
- the **Disable** heading of Settings' accounts table (see "Turning an account off" in
  `docs/product/agent-accounts.md`).

## Two-state icon toggles

A control that switches a view between two states is **one icon button**, not a set of choices side by side. These are
the sidebar toggle (see "The sidebar toggle and the panel toggles" in `docs/product/window-layout.md`), the file
viewer's three view controls (see "The header" in `docs/product/file-viewer.md`), the Files mode's two — its file-name
filter toggle and the toggle in its filter field that includes ignored files (see "Filtering by file name" in
`docs/product/project-pane.md`) — and the Git mode's three — the toggle of its change filter field (see "Filtering by
file name" in `docs/product/project-pane-git-change-list.md`), Group by folder (see "Flat or grouped by folder" in the
same doc) and Group by commit (see "Grouped by commit" in the same doc). For all of them:

- Pressing the button switches the state, and its icon becomes the other state's.
- **The icon shows the state the view is in; the name says what pressing will do**, so the name alternates with the
  state. That name is both the tooltip and what assistive technology announces, as "Tooltips on icon-only controls"
  above says. Assistive technology is also told which state the control is in: whether the button is pressed; the
  sidebar toggle says instead whether the sidebar is expanded, and each file-name filter toggle says both, pressed
  while its field is on screen.
- **Nothing about the state is said in colour**: the button in its pressed state is drawn exactly as in its
  unpressed one.

Settings' Appearance choice is not one of these: being a choice among three, it stays a three-way segmented control,
where the chosen option is marked by a fill of its own rather than by its icon (see "Appearance" in
`docs/product/settings.md`). Nor are the panel toggles in the top bar: each keeps one icon whatever the state, and an
open panel is marked by the toggle's pressed fill, besides its name and its expanded state (see "The panel toggles in
the top bar" in `docs/product/right-pane-panels.md`).

## Copy buttons

A copy button is an icon button that puts a text on the clipboard and says it did so on the button itself rather than
in a toast. These are the file viewer's Copy path (see "The header" in `docs/product/file-viewer.md`) and the
button after the worktree's name on the Git mode's change list (see "Uncommitted changes" in
`docs/product/project-pane-git-change-list.md`). For both:

- Before a press, its tooltip and name say what it copies — "Copy path", "Copy worktree path" — and its icon is the
  copy glyph.
- Once the text is on the clipboard, **the tooltip turns to the copied wording and stays open through the press**:
  "Path copied" for Copy path, "Copied" for the worktree's. The name assistive technology is given changes with it, and
  the copied wording is also announced as a status message, since a control that keeps focus is not reliably read
  again. The icon changes to a check that fades in.
- **It goes back once the tooltip has closed** — the pointer or keyboard focus leaving it — its copy glyph fading in
  again. After a press that showed no tooltip, such as a tap, it goes back after about two seconds.
- Where the system asks for reduced motion, the icons change without fading.
- Where the platform has no clipboard, or it refuses, the button copies through the browser's selection copy instead: a
  hidden text is selected and copied by the document, without moving focus off the button and with the page's own
  selection put back. A copy that neither way allows changes nothing on the button and raises an error alert saying the
  text could not be copied (see "Recovery and unexpected errors" in `docs/product/error-alerts.md`).

Copy actions in action menus (a row's Copy path, Copy project name, Copy session ID, the terminal's Copy) are not copy
buttons, and fall back the same way; a row's Copy path, for one, confirms in a toast (see "A row's actions" in
`docs/product/project-pane.md`).

## Tooltips on indicators

A few marks that are not controls carry a tooltip too. They are not tab stops, so their tooltip shows only while the
mouse rests on them, never on keyboard focus; what it says reaches assistive technology through the name of the row or
badge they sit in, or the text beside them, and where nothing around a mark says it, through the mark's own name.

Carrying a tooltip makes none of them a control. A mouse press on such a mark does nothing of its own and takes no
keyboard focus; where the mark sits in something that answers a press, the press goes on to that and does what a press
there does — pressing a session's status glyph on its sidebar row selects the session, as pressing the row does, and
pressing a change row's status letter opens the change. So such a mark is no dead spot in what carries it.

On a touch screen a tap on a mark opens its tooltip's content in a popover (see "How a tooltip behaves" above) where the
tap has nothing else to do: the status glyph on the top bar's breadcrumb, the status glyph and agent icon of each
session a confirmation lists, the branch badge in a project's focus mode header and the context-size chip beside the
Session history's heading (see "The context-size chip" in `docs/product/session-context.md`) and the Conflicted tag in
the file viewer's title. On a device with a touch screen such a mark answers a press, and a hover, across at least 24 by
24 pixels, even where it is drawn smaller. A mark inside a row, a button or an option opens none, as the tap there
selects, folds, switches or chooses: the status glyph, agent icon, context-size chip and binding badge on a sidebar row,
a focus mode card or archived row and an archive view row, the relative time on a focus mode card or archived row and on
an item of a View archive submenu, the branch badge on a project's sidebar row, the activity marker, and the
responsible-session badge in the Git mode's branch lists. A touch has no route to what such a mark's tooltip says;
assistive technology has it in the accessible name of what holds the mark.

These are:

- a session's status glyph on its sidebar row, on the top bar's breadcrumb and on each non-archived session a
  confirmation lists among the ones it affects, whose tooltip says the status in the same words as everywhere else it is
  named — the placements where no status text sits beside the glyph (see `docs/product/session-statuses.md`);
- the activity marker, the mark standing for what is going on beneath a **collapsed project's row** in the sidebar
  (see "Project rows" in `docs/product/sidebar-project-rows.md`) and inside focus mode's **console-switch chip** (see
  "The switch strip" in `docs/product/focus-mode.md`), whose tooltip says which activity it stands for, in the same
  words the name of the row or chip around it folds in beside the project's or console session's own name. Beneath a
  collapsed project's row that is one of three — a session waiting for the user, one working, one merely running — while
  a chip only ever shows the first two of them. Background work counts as working in both summaries. The rail's
  console avatar has its own status selection and badge rules (see "The console switcher" in
  `docs/product/console-switcher.md`); that decorative badge has no tooltip of its own;
- the binding badge, the coloured dot on a project session's row bound to a console session, whose tooltip names that
  console session (see "The binding badge" in `docs/product/sidebar-session-rows.md`);
- a session's agent icon on its row, on a focus mode's card or archived row, on the archive view's row and on each
  session a confirmation lists among the ones it affects, whose tooltip names the agent and the account the session runs
  under, and everywhere but in a confirmation also the model and effort it runs with, as a labelled list (see
  "Session rows" in `docs/product/sidebar-session-rows.md`);
- the relative time at the end of a focus mode card, of a focus mode archived row and of an item in a View archive
  submenu, whose tooltip writes out the session's archive date, while it has one, above its creation date, each as a
  full local date and time; the time's own name carries the same dates, and the card's or row's description or the
  item's name takes it in (see "What both views show" in `docs/product/focus-mode.md` and "The archive view" in
  `docs/product/archive-view.md`);
- a project's branch badge, on its sidebar row and in focus mode's header, which carries **one tooltip for the whole
  badge** rather than one per mark: resting the mouse on its state glyph, on the branch name or on either upstream
  count shows the same sentence, the one the badge's whole state is said in — the branch and each count while it is
  not zero. It shows whether or not the badge's text is cut, and the one state that says in visible text that the
  repository has no commits yet carries no tooltip. A failed check is not in it: it is a failure mark of its own at the
  badge's start, a button with a popover and no tooltip (see "The branch badge" in
  `docs/product/project-git-status.md`);
- the responsible-session badge, the ring a branch carries in the Git mode's branch lists, whose tooltip names the
  session working on the branch, says so differently when that session no longer exists and when it is the selected
  one, and is on the ring itself rather than on the row around it (see "The responsible-session badge" in
  `docs/product/branch-responsible-session.md`);
- the Conflicted tag in the file viewer's title, whose tooltip names the kind of conflict; assistive technology
  hears it as part of the title (see "Opening a change" in `docs/product/project-pane-git-change-viewer.md`);
- a change's status letter on its row in the Git mode's change list, flat or grouped by folder, whose tooltip names
  the status in the same word the file viewer's header carries as the tag of an opened change, and is on the letter
  itself rather than on the row around it, the name and the detail beside it keeping their own tooltips (see "Rows
  and the keyboard" in `docs/product/project-pane-git-change-list.md`).

The warning glyph after the name of an added agent that is not installed, on its tab in Settings' Agent accounts
section, is a mark too, but not one of these: its tooltip belongs to the tab around it, which is a control, and is
placed against the tab rather than the glyph. It opens while the pointer is anywhere on the tab and while the tab has
keyboard focus, but not when a mouse press is what focuses the tab. On keyboard focus it shows once the strip and the
section have stopped scrolling the tab into view, since a scroll under an open tooltip closes it; a press or a key
before then cancels it. For how a press or a key closes it see "How a tooltip behaves" above. Nothing links the tooltip
to the tab for assistive technology: the tab's accessible name says the agent is not installed, and a line in the
details dialog of each of the agent's own accounts says it is not usable (see "The agent strip" in
`docs/product/agent-accounts.md`).

## Names too long for their space

A single-line name or label too long for its space — a sidebar row's name, a focus mode session card's title, the top
bar's breadcrumb, a session listed in a confirmation, the report panel's page timestamp, a name in a project pane's file
tree or its header, a name and what follows it in the Git mode's change list (one label together), a commit's subject on
its commit row there, a branch's name in its branch selector, a picked tag's label (see "Picked tags" in
`docs/product/project-tags.md`) and the project filter's strip of conditions in the sidebar's Projects heading, which
counts as one label (see "Filtering the project list" in `docs/product/sidebar.md`) — fades out where it is cut off
rather than ending in an ellipsis. The same holds for action-menu items and submenu names, a select's current choice or
placeholder and its listed options, directory-picker rows, each line of a focus mode's header — the console's name and
the project's name in a project's focus mode, the console's name alone in a console session's — the archive view's
heading and count, the model line on a focus mode card and on a full archive row, and the owner link in a session
history's header (see "The owner the session reports to" in `docs/product/session-history.md`). A fixed session-owner
label and a diff's collapsed-line label follow the same fade and marquee rules.

Except for the report timestamp, clipped text then has its full text as its tooltip — for the project filter's strip,
its conditions as the strip shows them, wrapped (see "Filtering the project list" in `docs/product/sidebar.md`),
and for the archive view's heading, the longer title that also names the project, console session or console the
archive is for — shown while the pointer is on the label and not on keyboard focus; the label's whole text is in the
document whether or not it is cut, so assistive technology is given it either way. This holds for an action-menu
item that cannot be chosen too — one that is disabled, with or without a reason under its name, such as a used-up
account in a session's **Switch account** submenu: its clipped name shows its tooltip while the pointer is on the name,
and pressing the name still runs nothing. The diff separator's tooltip follows the operating system's rules instead.

**A path is cut at its start instead**, so that its last folder stays in view (see
`docs/product/right-to-left-layout.md`). **So is a branch name**, so that its end — the part that tells sibling
branches apart — stays in view: in a project's branch badge, on its sidebar row and in its focus mode's header; in the
Git mode's branch selector, both its current choice and its listed options; and in the project menu's **Switch branch**
submenu. The short commit id a detached `HEAD` shows in the badge, which is known by its beginning, the badge's text
saying the repository has no commits yet and the branch selector's placeholder asking for a branch are cut at their end,
like every other label. Everything this section says holds for a label cut at its start, mirrored: it rests pinned to
its end, fades at its start, and carries its full text as its tooltip while it is cut.

**A label that reads left to right whatever the language** — a path, a branch name, the file viewer's file name (see
"What is mirrored" and "What is not mirrored" in `docs/product/right-to-left-layout.md`) — gives its full text in its tooltip left to right too, under a right-to-left
language as well, so the tooltip shows the same value the label does; where that tooltip wraps, its lines still sit
on the window's reading side. A tooltip that is a sentence rather than the value itself reads in the window's
direction as usual.

A project's branch name in its branch badge also follows these fade and marquee rules. The badge's tooltip is not
one of this section's, though: the whole badge carries one tooltip describing the branch and its counts, shown whether
or not the branch name is cut (see "The branch badge" in `docs/product/project-git-status.md`).

**While the pointer is over it, such a label runs as a marquee**, so all of it can be read:

- The pointer has to be over the label itself. In sidebar rows, focus headers and session cards, and archive headers
  and rows, each text label runs independently: a project's name and branch name, a console name and focus
  title, a session title and its model line, or an archive heading and count do not start one another's
  marquee.
  Pointing at a status or agent icon, a branch glyph or upstream count, a row or header button, or the surrounding
  space starts none of those labels. Context-size chips and time fields remain separate from the model line's marquee.
- An action-menu item or submenu item, a select's option or directory-picker row runs its one label while the pointer is
  anywhere over that item or row. An action-menu item that cannot be chosen is the exception: its name runs only while
  the pointer is on the name itself, and pointing at the rest of its row runs nothing; the row neither lights up nor
  shrinks under the pointer, and the pointer keeps the plain arrow over it, name included. So is the **Copy session ID**
  item, whose session ID line runs only while the pointer is on that line (see "Session rows" in
  `docs/product/sidebar-session-rows.md`). A select's current choice or placeholder runs while the pointer is over its
  trigger, and a picked tag while it is over the tag. The project filter's strip of conditions runs as a whole while
  the pointer is anywhere over it, its tags moving together rather than each on its own. A fixed session-owner label and
  a diff's collapsed-line label run over their text controls.
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
