# Project pane Git mode

The project pane's **Git mode** reviews the project's repository in two views: **Uncommitted**, the uncommitted
changes of one worktree — what is staged, what is in conflict, what is changed on disk and what Git does not track —
and **Compare**, the changes between the tips of two local branches. Either view opens each change in the project
pane's read-only file viewer. The Git mode is the second of the project pane's two modes: opening the pane, its header,
switching modes and the file viewer in general are in `docs/product/project-pane.md` (see "Files and Git" and "The
file viewer" there).

## Reading only

The Git mode reads the repository and never writes to it. It stages, unstages, commits and checks out nothing —
comparing two branches checks neither out — takes no lock, never refreshes the index (not even for a file whose
timestamps moved while its content did not), and fetches nothing from a remote. The one exception is Git itself before
version 2.45: reading an object that a partial clone does not have yet can make such a Git fetch it.

The project's **branch badge** in the sidebar is separate (see `docs/product/project-git-status.md`): its check fetches
the remote's refs and, with Automatically sync repositories on or on Sync repository, may fast-forward the branch. The
Git mode does neither, and neither one refreshes the other.

## Uncommitted and Compare

Two tabs at the top of the Git mode switch between its views, **Uncommitted** and **Compare**; a project starts on
Uncommitted. The tabs are there once the project's repository is found (see "The repository and its worktrees" below);
while it is being read, when it cannot be read, or when there is none, what the Git mode says takes their place. At the
end of the tabs' row is the one control both views share, **Group by folder** (see "Flat or grouped by folder" below).

Each view reads the repository only while it is on screen: the Uncommitted view as "Keeping the list up to date"
below says, the Compare view as "The branch selectors" and "When a comparison is made" below say. Switching views
leaves the other view as it was. "The worktree selector" and "The change list" below are about the Uncommitted view,
"Comparing two branches" below about the Compare view; "Flat or grouped by folder", "Filtering by file name" and
"Opening a change" are about both.

## The repository and its worktrees

The repository is found by looking for a `.git` entry in the project's folder and then in each folder above it,
stopping below the home directory: the home directory is taken for a project's repository only when it is the
project's own folder.

- A project in no repository says it is not in a Git repository.
- A repository Git cannot read — `git` not installed or older than 2.36, a repository belonging to another user, a
  damaged one — says Git review is unavailable, with Git's own message.
- Either way the Files mode still works.

A project associated with a subdirectory of its repository keeps that scope in every worktree and in every
comparison: only changes touching the project's folder are listed (see "What is listed" below).

### The worktree selector

At the top of the Uncommitted view, the **worktree selector** chooses whose changes are shown. It lists every worktree
of the repository there is now — not a bare main repository, and not a worktree Git reports as prunable. Each one is
named by the branch checked out in it, by "Detached at" and the first seven characters of its commit when its `HEAD` is
detached, or as having no commits yet; under its name is its path, `~` standing for the home directory. A name too
long for its space fades out (see "Names too long for their space" in `docs/product/labels-and-tooltips.md`).

- The worktree holding the project's folder is marked as holding it, and is where the selector starts: the checkout
  the project's folder is in, which may be a linked worktree, never whichever worktree has some branch checked out.
- A worktree in which the project's folder is not a directory — missing there, or reached only through a symbolic
  link — says the project's folder is not in it, in the selector and above its changes.
- **Choosing a worktree changes only where the uncommitted changes are read from.** It checks nothing out, moves no
  session, leaves the Files mode on the project's own folder and plays no part in a branch comparison. Its changes
  are read afresh; nothing of the previous worktree's list stays on screen, and the selected change is cleared.
- **A chosen worktree that is gone** — removed, its path now holding another checkout, or removed and added again —
  stays chosen: the selector names it as unavailable and the mode says it is no longer available and to choose another
  one. Another worktree's changes are never shown in its place.

The worktrees are read when the Git mode is first shown after the project pane is put in the right pane, on Refresh,
after the connection to the daemon comes back, and when the chosen worktree is found gone. A worktree added in the
meantime appears in the selector then.

## The change list

### What is listed

The Uncommitted view's changes are listed in sections, in this order, each headed with its name and how many changes
it holds; a section with none is left out:

- **Staged** — the commit at `HEAD` against the index. Before the first commit every staged file is an addition.
- **Conflicted** — paths in conflict.
- **Unstaged** — the index against the files on disk.
- **Untracked** — files on disk Git neither tracks nor ignores, each listed by itself rather than as its folder.

A file staged and changed again since is in both Staged and Unstaged: two changes, each against its own baseline.

Not listed:

- Files the repository ignores, which the Files mode still shows.
- A submodule's own uncommitted work, which belongs to that repository. A submodule is listed only when it is checked
  out at another commit than the index records. An untracked folder that is a repository of its own is listed as one
  entry.

For a project that is a subdirectory of its repository, a change is listed when its old or its new path is inside the
project's folder (the folder itself is not inside it).

- A rename into the project from elsewhere in the repository, or out of it, names the other side by its path from the
  repository's root, as being outside the project. Only a staged rename crosses the boundary that way: a file moved on
  disk across it is, until staged, a deletion and an untracked file, and only the half inside the project is listed.
- In a worktree where the project's folder is not on disk, its changes are still listed: a deletion is read from the
  index and `HEAD`, never from the disk.

Within a section, changes are in the order the file tree would show their paths (see "Order" in
`docs/product/project-pane.md`), a deleted file by the path it had.

### Rows and the keyboard

Each row shows a status letter, the file's name, and then the folder it is in — or, for a rename, where it came from,
and for a rename across the project's boundary, where it came from or went, outside the project. Grouped by folder a
row drops the folder, which its parent row carries instead, and keeps the rest (see "Flat or grouped by folder"
below). At the row's end is the button of its own actions (see "A row's actions" below). The letters:

| Letter | Status |
|---|---|
| A | added |
| M | modified |
| D | deleted |
| R | renamed |
| T | type changed — a file that became a symbolic link, say |
| U | untracked — in a muted coral of its own, not Added's green |
| C | in conflict |

The letter is decoration: assistive technology hears a row as its name, its status in words and what follows the
name. A name, or what follows it, too long for its row fades out, with its full text as its tooltip (see "Names too
long for their space" in `docs/product/labels-and-tooltips.md`).

- The arrow keys move through the rows, the section headings skipped, and typing a name's first letters moves to it.
- A click or Enter opens the change in the file viewer (see "Opening a change" below). Nothing in the list is
  selectable otherwise.
- The change the viewer showed last is the list's **selected change**: its row is tinted, assistive technology hears
  it named as selected, and the list keeps it in view behind the viewer while the viewer moves from change to change.

### A row's actions

Every change's row ends with an icon button that opens a **menu of what can be done with that change**, in both the
Uncommitted and the Compare view and in both forms of the list, flat or grouped by folder. **Copy path** is its only
action so far. A folder's row in the grouped form has no such button, and neither has the Files mode's tree.

- **Copy path** puts the change's path on the clipboard as the row names it: the path within the project, slash
  separated, which for a rename is its new path — the old one only where the new side is absent (a deletion) or
  outside the project. A **toast** then confirms that the path was copied; where the clipboard refuses it, an error
  toast says it could not be copied (see `docs/product/toasts.md`).
- **The button shows while its row is hovered, holds keyboard focus, is the selected change, or has its menu open**,
  fading and opening out as it appears, as the sidebar's row controls do (see "Rows, names and keyboard focus" in
  `docs/product/sidebar.md`). While hidden it takes up no width, so the file name has the room; it is reachable from
  the keyboard either way.
- **The keyboard reaches it along the row, not with Tab.** Tab moves into the change list and out of it again, never
  along a row's parts; it is the arrow key that runs along the row — Right, Left under a right-to-left language —
  that moves from the row to its button and back. Grouped by folder, a folder's row keeps those keys for opening and
  closing it (see "Flat or grouped by folder" below).
- **The row keeps its own action**: a click or Enter on the row opens the change in the file viewer, and neither on
  the button does. The button is the only way to the menu — a right-click on a change row opens none, unlike a
  sidebar row (see "Right-click menus" in `docs/product/window-layout.md`). A menu opened from the keyboard hands
  keyboard focus back to the button when it closes; one opened with the pointer hands it back to whatever had it
  before, as a sidebar row's menu does.
- Its **accessible name names the file** — "Actions for file server.ts" — so each row's button is told apart, while
  its tooltip is the short one every such button has (see "Tooltips on icon-only controls" in
  `docs/product/labels-and-tooltips.md`).
- **Where the platform has no clipboard there is no button on the row at all**, rather than a menu whose only action
  cannot act: Copy path being the only action, nothing is left to offer. That is the case in a plain browser whose
  page is not a secure context — served over `http` from anywhere but `localhost`. The packaged desktop app always
  has a clipboard.

### Flat or grouped by folder

**Group by folder**, the toggle at the end of the row of view tabs, switches the change list between one row per
change and the changes grouped under the folders they are in. It is the one choice for both views, kept across
projects and after the application restarts; until it is turned on, both lists are flat.

Grouped by folder, a list keeps its sections and their counts — the Uncommitted view's Staged, Conflicted, Unstaged
and Untracked, the comparison's one section — and within each section the changes sit under their folders:

- Within a folder, folders come first and then changes, each in the order the flat list has them (see "What is
  listed" above).
- A chain of folders that each hold nothing but the next is one row, named by the whole chain — `tools/release`.
- A folder's row is a chevron, a folder icon and its name. A change's row is as it is in the flat list, less the
  folder its parent row now carries.
- Folders start open, and which ones the user folded away is kept per project for as long as the window is open: a
  reload or a restart opens them all again. A fold survives switching to the other view and back, and belongs to the
  section it was made in, so the same folder under another section stays open.

The rows take the keyboard as the Files mode's tree does (see "Rows and the keyboard" in
`docs/product/project-pane.md`): the arrow keys, Home and End move through them with the section headings skipped,
typing a name's first letters moves to one, Right and Left open and close a folder (the other way round under a
right-to-left language), and Enter opens a change or opens and closes a folder. A folder's row tells assistive
technology whether it is open, and its chevron is named Expand or Collapse (see "Tooltips on icon-only controls" in
`docs/product/labels-and-tooltips.md`).

A change under a folded folder has no row, and it is the rows that the viewer moves through (see "Moving between
changes" below).

### Filtering by file name

Above the change list, in both views and both forms of the list, flat or grouped by folder, is a **filter field**
that narrows the rows to the changes whose file name contains its text. It is there whenever the list is: not in
place of the lines that say there is nothing to list (see "What the list says instead" and "What the view says"
below).

- **Only the file's name is matched** — the last part of its path, as its row shows it — ignoring case and any
  whitespace around the text; the folders it is in are not, so `src` does not keep every file under `src/`. A rename
  is matched by its new name. An empty field, or one holding only whitespace, shows every change.
- **It narrows what is shown, not what is listed.** The sections' counts are of the rows shown, a section with no
  match is left out, and grouped by folder only the folders leading to a match remain. What the list holds, and when it
  is listed again, is unchanged; a list listed again is narrowed by the same text.
- **When nothing matches**, the list's place says there are no matching changes (announced to assistive technology
  as it appears), and the field stays, so the text can be changed or cleared. Clearing it brings the whole list back.
- The field has a **clear** button, and **Escape** in it clears it. In a drawer or a floating pane, Escape
  in a field with text only clears it and leaves the pane; Escape in the then empty field closes the pane, as it does
  from anywhere else in it.
- The field is apart from the list: typing in it never moves through the list's rows, as typing a name's first
  letters in the list does. While an input method is composing text in it, its keys, Escape included, belong to the
  composition, and do not clear the field.

The text is **one for both views** of a project, and is kept while the project's pane stays in the right pane —
through switching views and modes, choosing worktrees or branches, and the pane being hidden. Once the right pane is
given to something else (see "What the right pane shows" in `docs/product/window-layout.md`), or the window is
reloaded or the application restarted, the project's pane starts with the field empty. It is not a choice kept like
the grouping by folder.

The viewer moves through the rows the filter leaves (see "Moving between changes" below). The Files mode's tree has no
such field.

### What the list says instead

- **Loading** while the worktrees or the changes are read, announced to assistive technology — once, when reading
  the worktrees gives way to reading the changes.
- **No uncommitted changes** in this worktree.
- **Not every change is shown**, under the list, when it was cut short: at most 10,000 changes, or 2 MiB of paths, are
  listed — those Git reported first, not a chosen subset. A worktree whose whole `git status` output is more than
  8 MiB is not listed at all, and says so.
- **The project's folder is not in this worktree**, above the list or above the line saying there are no changes.
- **A failure** — the repository could not be read, or the changes could not be listed — names the reason, with **Try
  again**. Changes that kept moving while they were listed, after two more tries, say so; Refresh tries again. Try
  again pressed from the keyboard hands keyboard focus to the header's Refresh.
- **An unavailable worktree** (see "The worktree selector" above).

A list on screen stays while it is listed again, and so does a failure, with its Try again and keyboard focus: a
background refresh never replaces either with the loading line. Only Try again, Refresh and choosing a worktree do.
When the list goes away while one of its rows holds keyboard focus — the worktree has no changes left, the list failed,
or the worktree is gone — focus goes to the worktree selector.

### Keeping the list up to date

Nothing tells Octoboard when a repository changes, so the chosen worktree's changes are listed again:

- once the Uncommitted view is shown and the repository and the chosen worktree are known;
- **every 10 seconds** while the Uncommitted view is on screen — the project pane docked, in an open drawer, or
  floating in — and the window is visible, skipping a turn while a listing is still out;
- when the window comes back to the front or becomes visible again, with the Uncommitted view on screen, or when it
  comes back on screen — a hidden pane shown, the Git tab or the Uncommitted tab chosen — either one more than 5
  seconds after the last listing;
- on the header's **Refresh**, which reads the worktrees again too;
- after the connection to the daemon comes back, while the Uncommitted view is on screen.

While the pane is hidden, or shows the Files mode or the Compare view, nothing is listed on its own. When the
connection is lost, the list on screen stays until it is back.

## Comparing two branches

The **Compare** view shows what changed in the project's folder between the commits at the tips of two local branches
of the repository. Neither branch has to be checked out, in any worktree, and nothing is checked out, fetched or
written to compare them. The worktree chosen in the Uncommitted view plays no part: a comparison means the same
whichever worktree is chosen.

### The branch selectors

At the top of the Compare view are two branch selectors, each with its label before it: **From**, the old side, and
**To**, the new side.

- Each one offers every local branch of the repository — checked out in some worktree or in none — in byte order of
  their names, each with the first seven characters of the commit at its tip. At most 10,000 branches, or as many as
  2 MiB of names hold, are offered, the first in that order; nothing says the list was cut.
- A selector with no branch chosen says to choose one. A chosen branch's name too long for its space fades out (see
  "Names too long for their space" in `docs/product/labels-and-tooltips.md`), in the selector and in its list.
- A chosen branch that a complete branch list no longer has stays chosen, its name marked as no longer existing, until
  another one is chosen.
- **Swap branches**, beside the selectors, exchanges From and To, which reverses every change. It is unavailable while
  neither side has a branch chosen.
- **Choosing other branches, or swapping them, is another comparison**: nothing of the previous one stays on screen,
  and its selected change is cleared. Choosing the branches already chosen changes nothing.
- A broken branch — its reference names an object the repository does not have, or one that is not a commit — is
  offered with the id its reference names, and comparing it fails (see "What the view says" below).

The branches are read when the Compare view is first shown after the project pane is put in the right pane, and then
the way the Uncommitted view's changes are listed again (see "Keeping the list up to date" above), with the Compare
view in place of the Uncommitted view: every 10 seconds while it is on screen and the window is visible, when it comes
back more than 5 seconds after the last reading, on Refresh, and after the connection to the daemon comes back. A
branch created, moved or deleted in the meantime shows in the selectors then.

When the branches cannot be read, the view says so with the reason: in place of the line asking to choose a branch,
with **Try again**, while a branch is missing on either side and no list of branches has been read yet; otherwise in a
line under the selectors, announced to assistive technology, the branches already offered staying on offer, until the
branches are read again.

### What is compared

- **Tip to tip.** The comparison is between the two branches' commits themselves, never their merge base: whatever
  From has that To does not reads as removed, and swapping the two reverses every change. Two branches at the same
  commit have no differences.
- **At the commits the branches were at when it was made.** The changes listed, and every change opened from them,
  are between those two commits until the comparison is made again: a branch that moves or is deleted in the meantime
  never changes what is shown (see "A branch that moves or goes away" below).
- **Nothing is read from the disk or from any worktree's index**, so a file is compared and opened whether or not it
  is in the project's folder on disk or in the chosen worktree.
- **A subdirectory project** is compared within its scope as the Uncommitted view lists it (see "What is listed"
  above): a change is listed when its old or its new path is inside the project's folder, and a rename across the
  folder's boundary names its other side as outside the project. When comparing the whole repository's trees gives
  more than 8 MiB of output from Git — far-apart branches of a large repository — a subdirectory project is compared
  within its own folder instead, where a rename across the boundary is listed as the addition or the deletion of its
  half inside. A project at the repository's root whose comparison is that large cannot be compared, and says so.
- A submodule is listed when the two commits record it at different commits. A type change is one change, as in the
  Uncommitted view.

### When a comparison is made

- once both branches are chosen, with the Compare view on screen;
- the first time the Compare view is on screen after the project pane is put in the right pane, with both branches
  chosen;
- on the header's **Refresh**, which reads the worktrees and the branches again too, and on **Try again**.

**A comparison is never made again on its own**: not on a timer, not on switching views or modes, and not after the
connection to the daemon comes back, which keeps the comparison on screen as it is; only a comparison whose reply was
lost with the connection is asked for again once it is back. A comparison on screen stays while it is made again,
until the new one is in.

### What the view says

- **To choose a branch on each side**, while a branch is missing on either side, or, when the repository has no
  branches, that it has none yet.
- **Comparing** while a comparison is made with none on screen, announced to assistive technology.
- **The commits compared**, From's and then To's, each by its first seven characters, in a line above what the
  comparison found:
  - the changed files, in one section, **Changed files**, headed with how many changes it holds. Its rows, their
    order, their status letters (A, M, D, R or T here), the keyboard, their action button and the selected change are
    as in the Uncommitted view (see "Rows and the keyboard" and "A row's actions" above), and it is grouped by folder
    with it (see "Flat or grouped by folder" above) and narrowed by the same filter (see "Filtering by file name"
    above);
  - **Not every change is shown**, under a list cut short: at most 10,000 changes, or 2 MiB of paths, are listed —
    those Git reported first;
  - **both branches are at the same commit**, so there are no differences;
  - **no differences in the project's folder** between the two commits.
- **A failure** — the comparison could not be made — names the reason (a branch that no longer exists or is broken,
  say), with **Try again**. Try again pressed from the keyboard hands keyboard focus to the header's Refresh.

When the list goes away while one of its rows holds keyboard focus, focus goes to the From selector.

### A branch that moves or goes away

The comparison on screen stays on the commits it was made at. Once a branch list read after the comparison came in
shows a compared branch at another commit, a notice under the commits line says that branch has moved since the
comparison and that Refresh compares its current commit. Once a complete branch list no longer has a compared branch,
the notice says it no longer exists and that its commit is still shown, and its selector names it as no longer
existing. A branch list cut short says nothing of a branch it does not have. A notice that appears is announced to
assistive technology.

Refresh then compares the branches as they are now, or, for a branch that no longer exists, fails saying there is no
such branch. A change opened from the comparison still on screen reads from its commits for as long as the repository
has them (see "Errors" below).

## Opening a change

A change opens in the project pane's file viewer (see "The file viewer" in `docs/product/project-pane.md`). Its title is
the file's name, with the kind of change before it as a tag — Added, Deleted, Modified, Renamed or Type changed —
coloured as the change's letter in the list is (green, red, amber, blue; the same colours for the same kind). A staged
or unstaged change has its stage — Staged or Unstaged — as a second, neutral tag between the kind and the name: the two
things a change says, what kind it is and whether it is staged, are kept apart, the first coloured and the second not.
Both tags are in front of the name, and read with it as "Modified, Unstaged: server.ts". An untracked file is always
new, so its only tag is Untracked, in the muted coral the list's U has, which is no kind of change and shares no colour
with Added or Deleted; a path in conflict has only a Conflicted tag, coloured as its C is. Neither has a stage tag, its
status tag already saying where it is from. Under the title, in one row, are the path within the project and, for a
rename, the path it was renamed from. A change from the Compare view has no stage tag either; it names, after the path,
the two branches and the commit each was at — "main at 4f2a9c1 to feature at 9e8d7c6", say. The tags never shrink; the
name fades out when there is no room for it.

A change from the Compare view is read from the two commits of the comparison on screen, its old side from From's
commit and its new side from To's, and is shown as a staged change is below; it is never untracked or in conflict.

### What it shows

- **A text change** is a diff, drawn from the patch Git makes for it: unified at first, or split into its two sides
  side by side, chosen among the view controls at the end of the row under the title. The choice is offered only for a
  diff with lines on both sides: an added or a deleted file reads the same either way, so it has none. It is
  remembered: later diffs open in the layout last chosen, across changes and after the app restarts. Only the patch's
  hunks and the lines around them are shown, each run of unchanged lines between them collapsed into a separator that
  can be expanded (see "Expanding the collapsed lines" below). A change whose patch has no lines — an empty file added
  or removed, say — says it has no diff to show. Whether its long lines wrap is the viewer's own choice, which a diff
  shares with every file (see "Wrapping long lines" in `docs/product/project-pane.md`). A patch of more than 1,000,000
  characters or 10,000 lines is shown as the plain patch, with a line saying it is shown that way because the change is
  large; so is a patch the diff could not be drawn from. A patch that is not valid UTF-8 — of a file in another
  encoding — is shown with replacement characters.
- **A missing newline at the end** of either version is said in a line above the diff — the old version, the new one,
  or neither has one — rather than as a line of it. A symbolic link's change never says so, a link's target having
  no newline at its end anyway. A plain patch keeps Git's own marker lines.
- **An image change** — of a binary image format the file viewer displays (see "What it shows" in
  `docs/product/project-pane.md`); an SVG, being text, is a text change — shows the two images, Before and After, side
  by side where the viewer is wide enough (Before on the left, mirrored under a right-to-left language, see
  "Right-to-left layout" in `docs/product/window-layout.md`) and one above the other otherwise. A side with no file
  says so.
- **Any other binary change**, or an image changed to or from text, says it cannot be shown as a diff, with the size
  before and after.
- **An untracked file** is shown as new content: the file itself, as the file viewer shows a file; its Untracked tag
  says there is no earlier version to compare it with. An untracked symbolic link, or an untracked repository of its
  own, says what it is, having no content to show.
- **A rename across the project's boundary** shows only the version inside the project, as a file rather than a diff,
  under a warning that the previous or the new version is outside the project, with its path from the repository's
  root. Nothing of the version outside is read.
- **A type change** — a file that became a symbolic link, or the reverse — is shown as two diffs, Before and After,
  one per side. Each has lines on one side only, so no choice of unified or split is offered for them. A
  rename into or out of a path below itself (`foo` to `foo/bar`, or back) is shown the same way: its old path's
  removal and its new path's addition, under Renamed.
- **No differences**, when the two sides now hold the same content: the change was staged, committed or undone since
  it was listed.
- **A path in conflict** is not a two-sided change: a warning names the kind of conflict — both modified, both added,
  both deleted, added by us, added by them, deleted by us, deleted by them — above the file as it is on disk, with its
  conflict markers where both sides modified or added it. A path both sides deleted has no file to show and says it
  does not exist.

How a diff is coloured in each appearance is in "What follows the choice" in `docs/product/appearance.md`. A diff's
code is focused, selected and copied as a file's is (see "Selecting and copying" in `docs/product/project-pane.md`).

### Expanding the collapsed lines

Each run of unchanged lines a diff leaves out stands in it as a **separator** saying how many lines it holds — "6
unmodified lines" — in the current language (see "What follows the language" in `docs/product/language.md`). Where the
lines can be shown, the separator reveals them, in both the Uncommitted and the Compare view and in both diff layouts:

- **One expansion reveals 20 lines.** A run between two hunks longer than that offers two controls: one reveals the 20
  lines at the run's top, which appear above the separator, the other the 20 at its bottom, which appear below it. A
  shorter run, and the run before the first hunk or after the last, offers one. The **third expansion of the same run**
  reveals whatever is left of it, whichever of its controls is used.
- **Show whole file**, at the end of a separator, reveals the file's every line at once, leaving no separator behind.
  Each run offers it once — in the split layout on one side only. With wrapping off it stays at the visible end of the
  bar as the diff is scrolled sideways, as the separator's text and controls do.
- **A separator's text is a control too**: clicking it does what the first of that separator's controls does.
- An expandable diff also ends in a separator for the lines beyond its last hunk, whose number the patch does not say;
  it says instead that more unmodified lines may follow.
- **The lines are read only when the user expands.** Nothing of them comes with the patch, with a refresh, or when the
  connection to the daemon comes back. The first expansion of a diff reads both of its sides whole, and every later
  expansion of the same diff uses what that read returned. The read writes nothing to the repository and takes no
  lock, as nothing in the Git mode does (see "Reading only" above).
- **An expansion lasts as long as the diff is on screen.** A change opened again, or read again at other versions, is
  drawn with its lines collapsed again.

While the lines are being read the separator says so, and no other expansion starts until that read is answered: every
control of the diff reads as unavailable meanwhile. That, and each note below, is announced to assistive technology.
Then:

- **A failed read** leaves the diff as it is: the separator says the lines could not be loaded and to try again, a note
  above the diff gives the reason — that the connection to the daemon was lost, among them — and the next expansion
  reads again. A read of the change itself that follows — on a listing with news of it, or once the connection to the
  daemon is back — clears the failure without another expansion.
- **A change that has moved on** since its patch was read, its file or its index having changed underneath, keeps every
  line the patch shows, with a note above the diff saying its unmodified lines cannot be shown, and gives the expansion
  up as below; it is offered again only once the change is read again (see "When the open change changes" below). A
  change from the Compare view, read from two fixed commits, cannot move on this way.
- **Moving on to another change** while the lines are being read gives that read up, saying nothing.

**A diff that offers no expansion** keeps its separators with their counts and nothing else: no controls, and no
trailing separator for the lines beyond the last hunk, which only an expandable diff has. That is so from the
start for a diff whose two sides are not both files whose content can be read — an added or a deleted file, which has
one side only, a symbolic link or a submodule, hence a type change, and a version outside the project. The rest is
found only on the first expansion, and the diff is then drawn again in that form, so its trailing separator goes on
that press: a side larger than the 4 MiB a read opens, a side past the viewer's own budget of 10,000 lines or
1,000,000 characters, and the change having moved on. Of the three only the last says anything; the two limits pass
without a word.

From the keyboard, **Tab** from the code region moves into the separators' controls and then along them, run by run
from the top of the diff, and after the last one leaves the diff; **Shift+Tab** from the first goes back to the code
region. **Enter** or **Space** uses a control, and once the diff is drawn again keyboard focus is back on it, or on the
nearest control left where that one is gone, or on the code region where none is left at all — after Show whole file,
and after a diff turns out to offer no expansion. A control reached from the keyboard shows a focus ring, in either
appearance; pressing one with the mouse leaves keyboard focus where it was.

### Moving between changes

**Previous file** and **Next file**, and the **Left** and **Right** arrow keys, move through the change list the change
was opened from — the Uncommitted view's or the comparison's — as it is on screen, top to bottom, from one section
into the next, and stop at either end rather than going round. They follow the rows as shown: a change the filter
hides is skipped (see "Filtering by file name" above), and so, grouped by folder, are the changes under a folded
folder. A file in both Staged and Unstaged is two stops. A change with no row of its own — one the list no longer has —
keeps its place in the order, and the keys move on to the changes that surround it. When the keys do nothing, and how they mirror under a right-to-left language, is as in "Moving between files" in
`docs/product/project-pane.md`.

### Closing it

**Escape** or the viewer's **Close** button closes it. Keyboard focus then goes to the row of the change it showed
last.

### When the open change changes

For a change from the Uncommitted view, the listings that keep the change list up to date (see above) also decide
when the open change is read again:

- a listing with news of it — the change listed at other versions (a file overwritten, staged again or committed), or
  gone from a list that is not cut short — reads it again, and it shows as it is now;
- a change that kept changing while it was read, after two more tries, says so, and is read again at each new listing
  that still has it, until a read succeeds;
- a change whose worktree is found gone, by its list or by the worktrees read again, is read again and then says that
  worktree is no longer available.

A change from the Compare view, its two commits being fixed, is not read again when the branches are read. When the
comparison is made again at other commits while the change is open, the change is read again from the new commits if
the new comparison lists it; if it does not, there is no such change between those commits: the viewer closes, no
change is selected, and keyboard focus goes to the first row of the list, or to the From selector when the list has no
rows. The viewer never shows a change from other commits than the list's.

When the connection to the daemon is lost, what is shown stays until the connection is back, and then the change is
read again; a change that had nothing on screen yet says, in plain text rather than as an error, that the connection
was lost and that it is read again once the connection is back.

### Errors

A change that cannot be read keeps what the list said of it — its section, its status and its paths — with the reason
in place of the diff, in the current language. The cases beyond those of a file (see "Errors" in
`docs/product/project-pane.md`, which also apply to a path in conflict):

| Situation | What the user is told |
|---|---|
| Its patch is larger than 4 MiB | it is too large to show as a diff, its patch being more than can be opened |
| A file it has to show whole (an untracked file, the inside version of a rename across the boundary, a binary side) is larger than 4 MiB | its size, and the most that can be opened |
| Its worktree is gone | that worktree is no longer available |
| It kept changing while it was being read, after two more tries | it kept changing, and is read again at the next refresh |
| A compared change's commit is one the repository no longer has (a deleted branch's, once Git has pruned it) | the repository has no such commit |
| A compared change's content is missing from the repository | Git could not read it, with Git's own message |

## What is remembered

**Per project, for as long as the window is open**: the view shown, Uncommitted or Compare; the chosen worktree and
the Uncommitted view's selected change; the branches chosen for From and To and the Compare view's selected change;
and, grouped by folder, the folders folded away. Reloading the window, or restarting the application, starts every
project on the Uncommitted view and the worktree holding its folder, with no branches chosen, no change selected and
every folder open. Change lists, branch lists, comparisons and their contents are never stored. The filter's text is
kept only while the project's pane stays in the right pane (see "Filtering by file name" above).

**Across projects and restarts**: whether the change lists are grouped by folder (see "Flat or grouped by folder"
above), kept as the file viewer's diff layout and wrap choices are. The mode itself is kept with the rest of the
pane's state (see "What is remembered" in `docs/product/project-pane.md`).
