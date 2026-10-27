# Project pane

The **project pane** is the window's right pane while a project owns it. Its panels are the **Files panel**, described
here, the **Terminal panel**, the project's shell terminals (see `docs/product/terminal-panel.md`), and, while the
selected project session has pages, that session's **report panel** (see `docs/product/report-panel.md`); how they are
stacked is in `docs/product/right-pane-panels.md`. The Files panel shows a project's files and its repository's changes,
in two **modes**: **Files**, a tree of the project's directory, read live from disk with whatever an agent has written
and not committed, and **Git**, the changes of one local branch of the project's repository, uncommitted and committed
since it forked (see `docs/product/project-pane-git-mode.md`). Either one opens what it lists in a read-only **file
viewer** (see `docs/product/file-viewer.md`). The pane is shown in the window's right pane, in the place a console
session's report panel takes (see "What the right pane shows" in `docs/product/window-layout.md`, which also says what
puts it there and what takes it away), and it needs no session: the project is all it reads from. How wide the right
pane is and how it floats in is in `docs/product/window-layout.md`, how the Files panel is opened, closed and drawn in
`docs/product/right-pane-panels.md`, and where `F6` lands in it in `docs/product/moving-focus-between-regions.md`.

The Files panel itself is read-only: there is no editing, saving, creating, renaming or deleting of files, and
nothing is staged, committed or checked out. A row can open its live file or folder in an external application
(see "A row's actions" below).

Failures to load the pane, list the project's directory or search its files are shown in the pane alone, each as an
empty-state failure in place of what could not be loaded, with **Try again**, and a folder that could not be listed is
marked on its row; none of them opens an error alert (see "Failures shown on the screen" and "Failures, warnings and
notes marked at their point" in `docs/product/error-alerts.md`).

## Opening a project's files

- **Browse files**, in a project's action menu — on its row and by right-clicking it (see "Project rows" in
  `docs/product/sidebar-project-rows.md`), in its focus mode's header, and on its heading in a console session's focus
  mode (see `docs/product/focus-mode.md`) — shows that project's project pane in Files mode, selecting, starting and
  resuming no session; it opens the Files panel (see "Opening and closing panels" in
  `docs/product/right-pane-panels.md`). It is omitted while that project's Files panel is on screen, docked, floating in
  or floating over the terminal, and offered otherwise: while the right pane belongs to another owner or while the Files
  panel is closed. This applies in Git mode too; choose the Files tab to return to the tree while the panel is already
  on screen.
- **Selecting a project session** gives the right pane to its project, whose Files panel shows while it is open.

Moving between sessions of one project leaves its pane as it was: its mode, its expanded folders and selected file, and
in Git its selected change — except that the Git mode's branch is set from the session moved to, which clears the
selected change when the branch moves (see "Following the selected session" in
`docs/product/project-pane-git-mode.md`). Another project's session, or
another project's Browse files, shows that project's own pane.

When the right pane passes to another project while the Files panel is on screen, its header and content fade in
where they stand
(see "Switching projects under an open panel" in `docs/product/right-pane-panels.md`).

The Files panel has **one header row**, its panel header (see "A panel's header" in
`docs/product/right-pane-panels.md`). At its leading side are the visible project name as the panel's title and, right
after the name, two peer tabs — **Files** and **Changes**; at its trailing side are icon buttons alone: the controls for
the chosen tab, then **Refresh**, then the panel's own **Close panel**. Spare width stays between the tabs and those
buttons. The tabs are compact, with a pill behind the selected tab and muted labels on the others, and no filled track
behind them. The tabs and Close panel have priority over the project title and other icon buttons; their behavior in a
narrow header is in "A panel's header" in `docs/product/right-pane-panels.md`. Files offers the file-name filter toggle
(see "Filtering by file name" below) and then **Collapse all folders** (see "Collapse all folders" below); Changes
offers its filter toggle, Group by folder and Group by commit (see "The Changes view" in
`docs/product/project-pane-git-mode.md`). The buttons keep that place in loading, empty and error states too. Pressing
Refresh or Collapse all folders with the mouse leaves keyboard focus where it was.

## Files and Git

The **Files** and **Changes** tabs in the header switch directly between the project's directory and its Git review (see
"The Changes view" in `docs/product/project-pane-git-mode.md`): Changes is the Git mode. A project's pane first opens in
Files. The mode is remembered per project (see "What is remembered" below), so selecting a session shows a project in
the mode it was last left in. Browse files explicitly chooses Files. Both tabs remain available while content is
loading, has failed, or has no repository.

Only the mode on screen is kept up to date on its own: the Files tree as "Keeping the tree up to date" below says, a
file-name search as "Filtering by file name" below says, and the Git mode as "Keeping the list up to date" in
`docs/product/project-pane-git-change-list.md` says. Switching to a mode counts as showing it again, so the tree or the
Git mode's changes are refreshed when their last refresh is more than 5 seconds old; an active file-name search is
repeated whenever its filter is shown again. The header's **Refresh** refreshes the mode on screen: in Files every
visible folder of the unfiltered tree is listed again, or an active file-name search is repeated; in Git the worktrees,
the branches and the uncommitted changes are read again, the comparison is made again and, grouped by commit, the
commits are listed again. While a refresh the button started is out, its icon turns, slowly, and it keeps turning for at
least a moment, so a refresh that finishes at once still shows it happened. It spins until the reads the press sent have
been answered or have failed, or the connection drops; a folder that does not answer keeps it spinning, because a read
from disk has no deadline. The periodic refreshes do not turn it, and the icon stays still under the system's Reduce
motion setting.

When the project's reported branch or detached state changes, the visible mode is refreshed immediately. A mode
that was hidden catches up when shown, without waiting for the five-second freshness threshold. This includes a
successful project-menu branch switch (see "Switching the local branch" in `docs/product/project-git-actions.md`).

"The file tree", "The selected file" and "Keeping the tree up to date" below are about the Files mode.

## The file tree

### What is listed

Everything in the project's directory is listed — dot files, and files Git ignores (an agent's build output and
installed dependencies included), which are shown dimmed (see "Ignored entries" below) — except the following names,
which are left out at any depth, a folder with everything in it:

- Git's own metadata: `.git`;
- the files an operating system leaves in folders: `.DS_Store`, `.AppleDouble`, `.Spotlight-V100`, `.Trashes`,
  `.fseventsd`, `.TemporaryItems`, `Thumbs.db`, `desktop.ini`, and any name starting with `._` (the AppleDouble files
  macOS writes beside files).

Apart from the `._` prefix, names are matched whole and ignoring the case of the letters A–Z only, so `Desktop.ini`,
`.ds_store` and `.GIT` are left out too, while a name differing in any other character is listed. The list is fixed; no
setting changes it. A folder holding only left-out entries shows as empty.

Folders start collapsed and each is read only when it is first shown, so a large folder costs nothing until it is
expanded.

A symbolic link is listed under its own name. One that leads to a folder inside the project opens as a folder; one that
leads to a file inside the project opens as that file; one that leads outside the project is listed, but nothing
behind it is shown (see "Errors" below).

### Ignored entries

An entry is **ignored** when Git ignores it in the repository it is in: by any `.gitignore` level, the repository's
`.git/info/exclude` or the user's global excludes file, as Git itself applies them. Everything inside an ignored folder
is ignored too, while a file Git tracks never is, even where a pattern names it. Nor is a folder holding a file Git
tracks, whatever pattern names it: it is listed at full strength and the entries in it are judged each on its own. A
folder that is a repository of its own — a submodule, or a clone inside the project — is judged by that repository's own
rules. A project whose own folder Git ignores has every entry ignored except the files Git tracks. A project outside a
Git repository has nothing ignored.

An ignored entry is still listed and does everything any other row does — it opens, expands, shows its actions and is
moved through from the keyboard alike. It is only drawn **dimmed**, the way the sidebar dims its inactive projects (see
"Order of projects and sessions" in `docs/product/order-of-projects-and-sessions.md`): a veil of the pane's own surface
lies over the whole row, name and icon together. Unlike the sidebar's group, each ignored row lifts its own veil, back
to full strength, while it is pointed at, is the selected file, holds keyboard focus (on the row or on a control in it),
or has its actions menu open. Assistive technology hears its name followed by "ignored" ("ignored, selected" for the
selected file). File-name filter results hold ignored files only while the search includes them (see "Filtering by file
name" below); there a folder is dimmed only when every result beneath it is ignored.

Which entries are ignored is looked up each time a folder is listed. A change to a `.gitignore`, to
`.git/info/exclude` or to what Git tracks shows at the next listing; a change to the global excludes file can take up
to **30 seconds** to show. When it cannot be determined — Git cannot be run, fails, or takes longer than 5 seconds for
one listing — the folder's entries count as not ignored, and the listing never fails for it. An ignore pattern holding
non-ASCII text matches only a name written in the same Unicode normalization form as the pattern, here and in the
file-name search alike.

### Order

Within each folder, folders come first — a symbolic link leading to a folder inside the project counts as one — then
everything else. Each group is ordered by name, case-insensitively and in natural order, so "item 2" comes before
"item 10", as the sidebar orders names (see "Order of projects and sessions" in
`docs/product/order-of-projects-and-sessions.md`). Two names that read the same are always kept in the same order. The
file viewer moves through files in this same order (see "Moving between files" in `docs/product/file-viewer.md`).

### Rows and the keyboard

File rows use muted icons suited to their names and extensions: code, scripts, configuration, JSON, documents,
images, audio, video, archives, spreadsheets, databases, fonts, keys and certificates, Git metadata, and licenses.
Unrecognized file names use a generic file icon. These name-based icons apply in the folder tree and
filtered results. In the unfiltered folder tree, symbolic links and unavailable or special entries have their own
icons instead.

- A folder has an expand arrow before its folder icon: pointing right while collapsed (left in a right-to-left
  language), and down while expanded. The arrow, a click on the row, Enter, or Right and Left opens and closes it
  (the arrow keys are reversed under a right-to-left language). A file opens in the file viewer with a click or Enter.
- At a file or folder row's end is the button of its own actions, reached with the same Right and Left; a collapsed
  folder is expanded first (see "A row's actions" below).
- The arrow keys, Home and End move through the rows, and typing a name's first letters moves to it.
- Pressing a row takes keyboard focus into the tree, unlike a sidebar row: the tree is content to read and move
  through from the keyboard.
- A name too long for its row fades out, with the full name as its tooltip (see "Names too long for their space" in
  `docs/product/labels-and-tooltips.md`).

A folder that is not simply listed says so:

- **loading** while it is being read, in a row of its own under it;
- **empty** when it has nothing in it, in a row of its own under it;
- **unreadable**: a red failure mark on the folder's own row, right after its name (not at the row's end), named as the
  folder that could not be listed, and nothing under the folder, which is drawn closed. The mark opens a popover with
  the reason, and **Try again** where asking again can help, with the mouse resting on it or with a tap, Enter or Space
  (a mouse click on it does nothing; see "Errors" below and "Failures, warnings and notes marked at their point" in
  `docs/product/error-alerts.md`). Opening the closed folder again — pressing the row, Enter, or its arrow — lists it
  again, as Try again does, and pressing the mark does not. Right (Left under a right-to-left language) on the row moves
  keyboard focus to the mark instead of opening the folder, Enter or Space on the mark opens its popover, a further
  Right reaches the row's own actions button, and the opposite key goes back. A failure that arrives while the folder is
  open closes it and keyboard focus goes to its row, and so does Try again; the folder stays open in what the user
  chose, so it opens once it lists;
- **cut short**: an amber warning mark on the folder's own row, right after its name, named and saying that not every
  entry of this folder is shown; under it are the entries read before a limit stopped the listing, and no row of its
  own says so. Its popover opens as the failure mark's does. Right (Left under a right-to-left language) on the open
  folder's row moves keyboard focus to the mark, a further Right reaches the row's own actions button, and the
  opposite key goes back, the folder staying open. At most 10,000 entries of one folder, or 2 MiB of names, are listed.
  These are the entries read first, not a chosen subset. A folder whose listing failed carries its failure mark
  instead. When a later listing is complete and the mark goes while it or its popover held keyboard focus, focus goes
  to the folder's row.

For the project's own directory the same states take the whole pane: a loading line while it is first read, announced to
assistive technology, an empty panel saying the project folder is empty (see "The full panel" in
`docs/product/empty-states.md`), when it is cut short an amber warning mark right after the pane's title, named and
saying that not every entry of this folder is shown (not while a file name filter is active, whose results have their
own mark; see "Filtering by file name" below) — when the mark goes while it or its popover held keyboard focus, focus
goes to the header's first control — and, when it cannot be read at all, an empty-state failure in place of the tree,
headed that the project's files could not be listed, with the reason (the folder shown as its path, `~` standing for the
home directory) and **Try again**; it is announced to assistive technology when it appears after the panel was drawn.
Try again pressed from the keyboard hands keyboard focus to the header's Refresh, which stays while the error and its
button go.

### Always a folder tree

**Files is always a folder tree**; it has no flat layout and no control to choose one, and a flat layout an earlier
version of Octoboard saved for the Files tab is ignored. With no filter, the tree lists folders lazily as they are
expanded (see "What is listed" above). A nonempty filter scans the whole project and shows its results as a tree too:
only the folders leading to a match, each match under its folder, starting expanded except for folders the user
folded away (see "Filtering by file name" below).

Each project's **Files and Changes tabs remember their own folded folders independently**, and switching tabs leaves
the other tab's state alone. The unfiltered Files tree retains its ordinary expansion state; its filter results retain
which folders were folded separately while that project's pane stays mounted. Folding a folder in the ordinary tree
also marks it folded for filter results, and expanding it clears that mark. Clearing a filter returns to the ordinary
tree. The viewer navigates only through the displayed file rows, skipping folded folders.

### Collapse all folders

**Collapse all folders**, the header's icon button after the filter toggle in Files, folds every folder at once. In the
unfiltered tree it closes every expanded folder, including those inside folders already closed, so each opens again with
its subfolders closed, and each counts as folded for filter results as folding it by hand would. With a nonempty filter
it folds every folder of the results instead. It is disabled while no folder is drawn open; a folder that could not be
listed is drawn closed and does not count. When keyboard focus was on a row that the folding takes away, it moves to the
top-level folder that row was under; when it was on the button itself, which is then disabled, it moves to Refresh.

## Filtering by file name

In Files, the **filter toggle**, the first of the header's controls, shows and hides a field directly below the
header. It starts hidden. Opening it puts keyboard focus in the field; closing it clears its text and returns focus
to the toggle if the field held it. An empty field closes when focus leaves it, with whitespace alone counting as
empty; focus moving to the incomplete-search mark or the include-ignored toggle beside it (see below) is not leaving
it, and the window itself losing focus leaves it in place. A field with text stays visible. The toggle follows the
convention in "Two-state icon toggles" in `docs/product/labels-and-tooltips.md`.

A nonempty filter **searches the entire project directory**, including folders never expanded in the tree, and
shows the matching files as a tree, each name under its folders (see "Always a folder tree" above). Clearing the text
restores the unfiltered tree.

- Only the **file name**, the last part of its path, is matched, by containment, ignoring case and whitespace around
  the query. Folder names and file contents are not searched. An empty query is no filter: the ordinary tree is shown.
- Dot files are included. **Ignored entries** (see "Ignored entries" above) **are left out by default**: an ignored
  file is not matched and an ignored folder is not searched. The names "What is listed" above leaves out are excluded
  at every depth, with everything beneath them. Symbolic links to files
  inside the project can match under the link's own name; symbolic links to directories are not traversed, and links
  outside the project are excluded.
- **Include ignored files**, an icon toggle at the end of the field's row, after the field,
  includes them: an ignored file then matches as any other, and ignored results are drawn dimmed (see "Ignored
  entries" above). It shows a file
  with a cross while ignored files are left out, named "Include ignored files", and a file with a magnifying glass
  while they are included, named "Exclude ignored files", following "Two-state icon toggles" in
  `docs/product/labels-and-tooltips.md`. It starts off, and goes back to off whenever the field closes. Pressing it
  with a nonempty filter repeats the search at once, without the 250 ms wait.
- Results follow the tree's path order: directory branches before files at each level, with names compared
  case-insensitively and in natural order (see "Order" above). Their rows open the file viewer and offer the
  same actions as tree rows do (see "A row's actions" below). The viewer's Previous/Next controls follow the
  visible result rows in this order, skipping folded folders.
- Typing waits **250 ms** before searching. A changed query or include-ignored choice shows a searching state until its
  own results arrive; a reply to an earlier one never replaces them. A failure shows, in place of the results, an
  empty-state failure headed that the search failed, with its reason and **Try again**. No matches has a distinct empty
  state, leaving the field available to change or clear.
- A search is bounded to **250,000 visited entries**, **2,000 matching files**, **2 MiB of encoded result paths**, **128
  directory levels** and **5 seconds**. An unreadable subdirectory or entry also makes the result incomplete. Incomplete
  results — with matches or with none — are marked by an amber warning triangle inside the filter field, at its trailing
  end before its clear button, named that the search is incomplete; its popover says that the search stopped at its
  limits, or could not read some files or folders, so not every match may be listed, and it opens as a row's mark does
  (see "Failures, warnings and notes marked at their point" in `docs/product/error-alerts.md`). When a later search
  completes and the mark goes while it or its popover held keyboard focus, focus goes to the field's input. An
  unreadable project root instead fails the search.
- Reading which entries are ignored takes at most **2 seconds** of a search's 5. Where it cannot be read in that time or
  within its size limits, or Git cannot be run or fails, the search goes on with nothing left out for that repository:
  it may then show ignored files, and it is not marked incomplete for that. A nested repository the search reaches only
  after those 2 seconds is judged by the rules of the repository around it.
- Searches repeat when the query changes, the include-ignored toggle is pressed, Files becomes active again, the daemon
  reconnects, Refresh is pressed, or the project's reported branch or detached state changes. **A nonempty filter has no
  periodic refresh**, and neither the window coming back to the front nor its becoming visible again repeats it. While a
  filter is active, the tree's folder listings pause except for the folder containing an open file and its ancestors
  through the project root: these continue to be listed to keep the viewer's content current, without repeating the
  search.

The field's clear button and **Escape** clear its text while leaving focus in the field. In a floating pane,
Escape with text clears it; Escape once empty can close the pane. Input-method composition owns its keys,
including Escape, so those keys do not clear the field. Typing in the field does not trigger the list's name navigation.

Each of **Files and Changes has its own keyword and filter-field open state**, and Files its include-ignored choice
with them. Switching tabs restores that tab's choices without taking keyboard focus into a restored field. Clearing or
closing the field affects only the current tab; closing it drops its keyword, and in Files the include-ignored
choice. These choices last while the project's pane stays in the right pane,
including tab switches and hiding the pane; replacing it with another owner, reloading the window or restarting the
application discards them. They are not shared across projects.

## The selected file

The tree's or search results' **selected file** is the one the file viewer showed last. Its row is tinted, and assistive
technology hears it named as selected. Nothing else in the tree is selected; a row's one action is to open or close it.

While the viewer moves from file to file, the tree or search results keep the selected file's row in view behind it.

A selected file that its folder no longer lists stays as a row in its place, struck through, until the selection moves
on: marked **Removed** when the folder's listing is complete, or **Not listed** when it was cut short and the file may
still be there.

## Keeping the tree up to date

The tree is read from the project's directory as it is on disk; nothing tells Octoboard when files change, so it lists
the folders on screen — the project's own directory and every expanded folder whose parents are expanded — again,
while no nonempty file-name filter is active (see "Filtering by file name" above):

- the first time the Files mode is shown after the project pane is put in the right pane, which lists it afresh;
- when a folder is expanded, if it had been listed before;
- on the header's **Refresh**;
- **every 10 seconds** while the Files mode is on screen — the Files panel docked, floating in or floating over the
  terminal — and the window is
  visible, skipping a turn while a listing of the pane is still out;
- when the window comes back to the front or becomes visible again, with the Files mode on screen, or when it comes
  back on screen — the Files panel opened or floating in, or the Files tab chosen —
  either one more than 5 seconds after the last refresh, and unless a listing of the pane is still out;
- after the connection to the daemon comes back.

While the pane is hidden, or shows the Git mode, the tree is not refreshed on its own, apart from after a reconnect. A
folder being listed again keeps its rows on screen until the new listing is in. When the project's directory turns out
to have been replaced by another one at the same path, everything listed from the old one is dropped and listed
afresh.

## A row's actions

Every file and folder row in Files ends with an icon button that opens a **menu of what can be done with that
entry**, whether the folder is collapsed or expanded, filtered or not. Every file's row in the Git mode's change list
has the same button, uncommitted and committed alike, flat or grouped by folder. **Right-clicking a row opens the same
menu at the pointer** (Control-click also works in the desktop app).

- The menu's order is **Copy path**, then the **Open in** submenu, omitting unavailable actions. Open in holds
  **File manager**, then **Default application**.
- **File manager** opens the containing folder and selects that entry, including when the entry is a folder.
  **Default application** opens the live file or folder with the operating system's default application.
  Both act on the machine running the daemon, including when the client is a browser on another machine. Files uses
  the project directory; a change, uncommitted or committed, uses the worktree the chosen branch is checked out in
  and the project's directory within it. When the branch is checked out nowhere, a committed change uses the project's
  registered directory, which can itself be in a linked worktree. They act on what is on disk now, including for a
  staged change or a change under an individual commit; they do not open a historical version, and the current file
  can differ from the change shown in the viewer. Neither says anything when it succeeds. A refusal or a failure raises
  an error alert titled for the action — that it could not open in the file manager, or in the default application —
  with the reason. When the file or folder is no longer there, its remedy says it may have been moved, renamed or
  deleted, and that one moved or renamed can be found in the project's files and the action tried again from there; a
  project directory or worktree place that itself cannot be read gets the remedy of a folder a request is scoped to,
  worded by why it cannot be read (see "Recovery and unexpected errors" in `docs/product/error-alerts.md`). The
  viewer's own header actions follow "The header" in `docs/product/file-viewer.md`.
- **Copy path** puts the file or folder's path on the clipboard, slash separated, as a path within the project. For a
  change it is the path the row names, which for a rename is its new path — the old one only where the new side is
  absent (a deletion) or outside the project (see "What is listed" in `docs/product/project-pane-git-change-list.md`).
  The path is copied **as the window shows it**, for a human to paste: a byte of a name that is not valid UTF-8 is
  copied as the replacement character U+FFFD, the character the name is displayed with. A **toast** then confirms that
  the path was copied; where neither the clipboard nor the browser's selection copy allows it, an error alert says it
  could not be copied (see "Error alerts" in `docs/product/error-alerts.md`). The file viewer's header offers the same
  action, and File manager and Default application too (see "The header" in `docs/product/file-viewer.md`).
- **The Git mode's grouping folders and commit rows have no button**, and neither does a row that says something about a
  folder instead of listing an entry (loading, empty; see "Rows and the keyboard" above).
- **The button shows while its row is hovered, holds keyboard focus, is the selected file or change, or has its menu
  open**, and always on a device whose primary input cannot hover — a phone, or a tablet's touch screen. It is
  reachable from the keyboard either way. In Files it fades and opens out as it appears, as the
  sidebar's row controls do (see "Rows, names and keyboard focus" in `docs/product/sidebar.md`), and while hidden it
  takes up no width, so the entry's name has the room. In the Git mode's change list it keeps its room while hidden and
  only fades in, so the line counts before it do not move (see "Line counts" in
  `docs/product/project-pane-git-change-list.md`).
- **The keyboard reaches it along the row, not with Tab.** Tab moves into the tree or the change list and out of it
  again, never along a row's parts; it is the arrow key that runs along the row — Right, Left under a right-to-left
  language — that moves from the row to its button; the opposite key moves back to the row. On a collapsed folder that
  first key expands it, and a second press reaches the button. On an expanded folder the opposite key still collapses it
  while focus is on the row.
- **The row keeps its own action**: a click or Enter on the row opens the file or the change in the file viewer, or
  opens or closes a folder. Opening the menu or choosing one of its actions does none of those. A menu opened from
  the keyboard hands keyboard focus back to the button when it closes; one opened with the pointer hands it back to
  whatever had it before, as a sidebar row's menu does.
- Its **accessible name names the file or folder** — "Actions for file server.ts" or "Actions for folder src" — so each
  row's button is told apart, while its tooltip is the short one every such button has (see "Tooltips on icon-only
  controls" in `docs/product/labels-and-tooltips.md`).
- **Where the platform has no clipboard, Copy path still works**, through the browser's selection copy. A plain browser
  whose page is not a secure context — served over `http` from anywhere but `localhost` — has no clipboard; the
  packaged desktop app always does.

Default application and File manager raise no success toast; failures raise an error alert saying the file or folder,
or the file manager, could not be opened, with the reason under its **Error details**. A missing
live entry, such as a deleted file that is still absent on disk, is refused with `file_not_found`; an entry or
symbolic-link target outside the project's directory in the worktree acted on is refused with `outside_scope`. A
worktree that no longer exists is refused with `worktree_unavailable`. Revealing a symbolic link selects the
link itself.

On macOS the system opener handles Default application and asks Finder to reveal the entry for File manager. When the
system opener fails because no application claims the file's type (an unregistered extension such as `.env.secret`), a
regular file whose first 8 KiB contain no NUL byte and are valid UTF-8 is opened in the user's default text editor
instead, as Finder does; any other file, or any other failure, is still reported. On Linux Default application uses
`xdg-open`; File manager requires `gdbus` and a desktop file manager providing `org.freedesktop.FileManager1.ShowItems`.
The Linux tools are looked up in the user's login-shell environment. On macOS, and for Linux File manager, the request
waits for the launcher to finish, at most **15 seconds** with at most **16 KiB** each of output and error, rather than
waiting for the application window to close. Linux Default application succeeds once its launcher starts; a later
launcher failure is not reported. Unsupported operating systems, unavailable launchers or desktop services, and launcher
failures are reported as `path_open_failed` for Default application and `directory_unavailable` for File manager,
carrying the reason as `detail`.


## What is remembered

**Per project, per client**, in that client's own browser storage (as the right pane's width is, see "Resizing the
sidebar and the right pane" in `docs/product/window-layout.md`): the mode, Files or Git, whether the Changes tab's list
is flat or grouped by folder and whether it groups its committed changes by commit, and which folders are expanded in
the ordinary Files tree, so the pane comes back as it was after a reload or a restart. At most 200 expanded folders are
kept per project, the earliest expanded dropped first, and the state of at most 50 projects, the one shown least
recently dropped first. A removed project's state is dropped.

The selected file is kept only while the window is open: reloading it, or restarting the application, starts every
project with no file selected. So are the Git mode's chosen branch, its selected change and the sections and folders it
has folded away (see "What is remembered" in `docs/product/project-pane-git-mode.md`). Listings, search results, change
lists and file contents are never stored. Each tab's filter text and visibility, the Files filter's include-ignored
choice and the Files filter results' folded folders, last only while the project's pane stays in the right pane (see
"Filtering by file name" above).

## Errors

A file or folder that cannot be shown says why, in the current language (see "What follows the language" in
`docs/product/language.md`), naming the file or folder it is about by its path within the project — except where the
file viewer's header already shows that path (see "Notices and stand-ins" in `docs/product/file-viewer.md`). The cases:

| Situation | What the user is told |
|---|---|
| The file or folder does not exist any more, or a symbolic link leads nowhere | it does not exist any more |
| The entry is not a regular file — a FIFO, a socket, a device | it is not a regular file |
| A symbolic link leads outside the project | it leads outside the project, so it is not shown |
| Octoboard is not allowed to read it | it is not allowed to read it, with the system's reason |
| The file is larger than 4 MiB | its size, and the most that can be opened |
| A file kept changing while it was being read, after two more tries | it kept changing, and is read again at the next refresh |
| A folder kept changing while it was being listed, after two more tries | it kept changing, and Refresh tries again |
| The project's directory itself cannot be read | the folder cannot be read, with the reason |

## Loading the project pane

The Files panel's own code loads the first time a Files panel is shown in the window. Meanwhile its card is in place,
empty. A load that fails shows, in the Files panel only, under its header — the project's name as its title, and the
panel's own controls — an empty-state failure headed that the project pane could not be loaded, with **Try again**; the
rest of the window is unaffected. Try again pressed from the keyboard hands keyboard focus to the terminal while the
next load runs. A project pane that fails while it is shown, with keyboard focus inside it, shows the same failure and
puts keyboard focus on its Try again.
