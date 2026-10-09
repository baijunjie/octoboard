# Project pane

The **project pane** shows a project's files and its repository's changes, in two **modes**: **Files**, a tree of the
project's directory, read live from disk with whatever an agent has written and not committed, and **Git**, the
uncommitted changes of a worktree of the project's repository and the changes between two of its local branches (see
`docs/product/project-pane-git-mode.md`). Either one opens what it lists in a read-only **file viewer**. It is shown in
the window's right pane, in the place a console session's report panel takes (see "What the right pane shows" in
`docs/product/window-layout.md`, which also says what puts it there and what takes it away), and it needs no session:
the project is all it reads from. How wide the right pane is, how it is hidden, floats in and becomes a drawer is in
`docs/product/window-layout.md`, and where `F6` lands in it in `docs/product/moving-focus-between-regions.md`.

Nothing in the project pane changes anything: there is no editing, saving, creating, renaming or deleting of files,
and nothing is staged, committed or checked out.

## Opening a project's files

- **Browse files**, the first item of a project's action menu — on its row and by right-clicking it (see "Project
  rows" in `docs/product/sidebar.md`), in its focus mode's header, and on its heading in a console session's focus mode
  (see `docs/product/focus-mode.md`) — shows that project's project pane, selecting, starting and resuming no session.
- **Selecting a project session** shows its project's project pane.

Moving between sessions of one project leaves its pane as it was: its mode, its expanded folders and selected file, and
in Git its view, its chosen worktree, its chosen branches and its selected changes. Another project's session, or
another project's Browse files, shows that project's own pane.

The pane's header names the project, with a folder icon, and carries the **Files** and **Git** tabs (see "Files and
Git" below) and a **Refresh** button. Pressing Refresh with the mouse leaves keyboard focus where it was.

## Files and Git

The tabs in the pane's header switch between its two modes; a project's pane first opens in Files. The mode is
remembered per project (see "What is remembered" below), so Browse files or selecting a session shows a project in the
mode it was last left in.

Only the mode on screen is kept up to date on its own: the Files mode as "Keeping the tree up to date" below says, the
Git mode as "Uncommitted and Compare" in `docs/product/project-pane-git-mode.md` says. Switching to a mode counts as
showing it again, so what it keeps up to date is refreshed when its last refresh is more than 5 seconds old. The
header's **Refresh** refreshes the mode on screen: in Files every folder on screen is listed again, in Git the
worktrees are read again, and so are the changes of the Uncommitted view, or the branches and their comparison in the
Compare view, whichever is on screen.

"The file tree", "The selected file" and "Keeping the tree up to date" below are about the Files mode.

## The file tree

### What is listed

Everything in the project's directory is listed — dot files, and files a `.gitignore` names (an agent's build output
and installed dependencies included) — except Git's own metadata: an entry named `.git` is left out at any depth.
Folders start collapsed and each is read only when it is first shown, so a large folder costs nothing until it is
expanded.

A symbolic link is listed under its own name. One that leads to a folder inside the project opens as a folder; one that
leads to a file inside the project opens as that file; one that leads outside the project is listed, but nothing
behind it is shown (see "Errors" below).

### Order

Within each folder, folders come first — a symbolic link leading to a folder inside the project counts as one — then
everything else. Each group is ordered by name, case-insensitively and in natural order, so "item 2" comes before
"item 10", as the sidebar orders names (see "Order of projects and sessions" in `docs/product/sidebar.md`). Two names
that read the same are always kept in the same order. The file viewer moves through files in this same order (see
"Moving between files" below).

### Rows and the keyboard

- A folder opens and closes with a click, Enter, its chevron, or Right and Left (the other way round under a
  right-to-left language). A file opens in the file viewer with a click or Enter.
- The arrow keys, Home and End move through the rows, and typing a name's first letters moves to it.
- Pressing a row takes keyboard focus into the tree, unlike a sidebar row: the tree is content to read and move
  through from the keyboard.
- A name too long for its row fades out, with the full name as its tooltip (see "Names too long for their space" in
  `docs/product/labels-and-tooltips.md`).

A folder that is not simply listed says so in a row of its own under it:

- **loading** while it is being read;
- **empty** when it has nothing in it;
- **unreadable**, with the reason (see "Errors" below); pressing the row reads it again;
- **cut short** — "not every entry is shown" — after the entries read before a limit stopped the listing: at most
  10,000 entries of one folder, or 2 MiB of names, are listed. These are the entries read first, not a chosen subset.

For the project's own directory the same states take the whole pane: a loading line while it is first read, a line
saying the project folder is empty, a line under the tree when it is cut short, and, when it cannot be read at all, an
error naming the reason (with the folder shown as its path, `~` standing for the home directory) and **Try again** in
place of the tree. Try again pressed from the keyboard hands keyboard focus to the header's Refresh, which stays while
the error and its button go.

## The selected file

The tree's **selected file** is the one the file viewer showed last. Its row is tinted, and assistive technology hears
it named as selected. Nothing else in the tree is selected; a row's one action is to open or close it.

While the viewer moves from file to file, the tree keeps the selected file's row in view behind it.

A selected file that its folder no longer lists stays as a row in its place, struck through, until the selection moves
on: marked **Removed** when the folder's listing is complete, or **Not listed** when it was cut short and the file may
still be there.

## Keeping the tree up to date

The tree is read from the project's directory as it is on disk; nothing tells Octoboard when files change, so it lists
the folders on screen — the project's own directory and every expanded folder whose parents are expanded — again:

- the first time the Files mode is shown after the project pane is put in the right pane, which lists it afresh;
- when a folder is expanded, if it had been listed before;
- on the header's **Refresh**;
- **every 10 seconds** while the Files mode is on screen — the project pane docked, in an open drawer, or floating in
  — and the window is visible, skipping a turn while a listing of the pane is still out;
- when the window comes back to the front or becomes visible again, with the Files mode on screen, or when it comes
  back on screen — a hidden pane docked by its toggle, its drawer opened, or floating in, or the Files tab chosen —
  either one more than 5 seconds after the last refresh, and unless a listing of the pane is still out;
- after the connection to the daemon comes back.

While the pane is hidden, or shows the Git mode, the tree is not refreshed on its own, apart from after a reconnect. A
folder being listed again keeps its rows on screen until the new listing is in. When the project's directory turns out
to have been replaced by another one at the same path, everything listed from the old one is dropped and listed
afresh.

## What is remembered

**Per project, per client**, in that client's own browser storage (as the right pane's width is, see "Resizing the
sidebar and the right pane" in `docs/product/window-layout.md`): the mode, Files or Git, and which folders are
expanded, so the pane comes back as it was after a reload or a restart. At most 200 expanded folders are kept per
project, the earliest expanded dropped first, and the state of at most 50 projects, the one shown least recently
dropped first. A removed project's state is dropped.

The selected file is kept only while the window is open: reloading it, or restarting the application, starts every
project with no file selected. So are the Git mode's view, chosen worktree, chosen branches and selected changes (see
"What is remembered" in `docs/product/project-pane-git-mode.md`). Listings, change lists and file contents are never
stored.

## The file viewer

Opening a file shows it in the **file viewer**, a dialog nearly as large as the window (at most 1440 px wide). Its title
is the file's name; under it are the file's path within the project and its size. The Git mode opens a change in the
same viewer; what it shows of a change is in "Opening a change" in `docs/product/project-pane-git-mode.md`.

### What it shows

- **Text** is shown as code, highlighted for its language as told from the file's name, in colours that follow the
  window's appearance (see "What follows the choice" in `docs/product/appearance.md`). Long lines wrap. A file of more
  than 10,000 lines or 1,000,000 characters is shown as plain text, with a line saying it is shown without highlighting
  because it is large; within highlighted code, a line longer than 1,000 characters is left unhighlighted. Where
  highlighting is not available for a file, or fails to load, the file is shown as plain text.
- **Images** — PNG, JPEG, GIF, WebP, BMP, ICO and SVG; AVIF on macOS 13 and later — are shown scaled down to fit and
  never scaled up, on a checkerboard that shows their transparent areas. An image is only displayed: an SVG's scripts
  never run and nothing it refers to is fetched. An image that cannot be decoded says so; an SVG that cannot be
  decoded is shown as its text instead.
- **Any other binary file** says it cannot be displayed, with its size.
- **While a file is being read** the viewer says it is loading. It never shows another file's content under this file's
  name, not even for a moment while moving between files.
- **A file that cannot be shown** says why (see "Errors" below).

### Selecting and copying

The **code region** — a file's code, highlighted or plain, or a change's diff or plain patch — takes keyboard focus,
with Tab or a click, so it can be scrolled from the keyboard; reached with Tab it shows a focus ring.

With keyboard focus on the code region, **⌘A** (Ctrl+A off macOS) selects that region's code and nothing else: not
its line numbers, not a diff's markers, and not the viewer's title, path and size, so copying takes the code alone.
While the code has not been drawn yet, the key selects nothing. A change shown as two diffs, Before and After, is two
code regions, and the key selects the one with keyboard focus.

While text inside the viewer is selected, the Left and Right arrow keys leave it selected rather than moving to
another file (see "Moving between files" below).

### Moving between files

**Previous file** and **Next file**, at the foot of the viewer, and the **Left** and **Right** arrow keys move to the
file before and after the one shown, without closing the viewer. They move through the file rows the tree shows, top
to bottom: folders are skipped, and so is everything inside a collapsed folder. They stop at either end rather than
going round, Previous file disabled on the first file and Next file on the last. A file shown as removed moves on to
the files that were beside it.

- Under a right-to-left language the keys are mirrored: Right moves to the previous file and Left to the next.
- The keys do nothing while a modifier is held, during an input method's composition, when keyboard focus is on a
  control that uses the arrow keys itself, or while text inside the viewer is selected, so a selection made for copying
  is not lost. A key held down moves on file after file.
- No key pressed in the viewer reaches the terminal.

### Closing it

**Escape** or the viewer's **Close** button closes it. Keyboard focus then goes to the tree row of the file it showed
last, not necessarily the one it was opened from.

### When the open file changes

The listings that keep the tree up to date (see above) also decide when the open file is read again, so it follows
changes on disk:

- a file that is rewritten in any way — a rewrite of the same size, or another file put in its place, included — is
  read again and shows its new content;
- a file that is gone, or whose folder is gone or can no longer be listed, is read again and then shows why it cannot
  be shown, its row staying in the tree as removed (see "The selected file" above);
- a file shown as gone that comes back shows again;
- a file that kept changing while it was being read is read again at each new listing of its folder, until a read
  succeeds.

A read that finds the file unchanged leaves what is shown as it is. When the connection to the daemon is lost, the
content shown stays until the connection is back, and then the file is read again; a file that had nothing on screen
yet says the connection was lost and that it is read again once it is back.

## Errors

A file or folder that cannot be shown says why, in the current language (see "What follows the language" in
`docs/product/language.md`), naming the file or folder it is about by its path within the project. The cases:

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

The project pane's own code loads the first time a project pane is shown in the window. Meanwhile the right pane is in
place, empty. A load that fails shows, in the right pane only, that the project pane could not be loaded, with **Try
again**; the rest of the window is unaffected. Try again pressed from the keyboard hands keyboard focus to the terminal
while the next load runs. A project pane that fails while it is shown, with keyboard focus inside it, shows the same
failure and puts keyboard focus on its Try again.
