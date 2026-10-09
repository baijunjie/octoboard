# Project pane Git mode

The project pane's **Git mode** shows the **uncommitted changes** of one worktree of the project's repository — what is
staged, what is in conflict, what is changed on disk and what Git does not track — and opens each change in the
project pane's read-only file viewer. It is the second of the project pane's two modes: opening the pane, its header,
switching modes and the file viewer in general are in `docs/product/project-pane.md` (see "Files and Git" and "The
file viewer" there).

## Reading only

The Git mode reads the repository and never writes to it. It stages, unstages, commits and checks out nothing, takes
no lock, never refreshes the index (not even for a file whose timestamps moved while its content did not), and fetches
nothing from a remote. The one exception is Git itself before version 2.45: reading an object that a partial clone
does not have yet can make such a Git fetch it.

The project's **branch badge** in the sidebar is separate (see `docs/product/project-git-status.md`): its check fetches
the remote's refs and, with Automatically sync repositories on, may fast-forward the branch. The Git mode does neither,
and neither one refreshes the other.

## The repository and its worktrees

The repository is found by looking for a `.git` entry in the project's folder and then in each folder above it,
stopping below the home directory: the home directory is taken for a project's repository only when it is the
project's own folder.

- A project in no repository says it is not in a Git repository.
- A repository Git cannot read — `git` not installed or older than 2.36, a repository belonging to another user, a
  damaged one — says Git review is unavailable, with Git's own message.
- Either way the Files mode still works.

A project associated with a subdirectory of its repository keeps that scope in every worktree: only changes touching
the project's folder are listed (see "What is listed" below).

### The worktree selector

At the top of the Git mode, the **worktree selector** chooses whose changes are shown. It lists every worktree of the
repository there is now — not a bare main repository, and not a worktree Git reports as prunable. Each one is named by
the branch checked out in it, by "Detached at" and the first seven characters of its commit when its `HEAD` is
detached, or as having no commits yet; under its name is its path, `~` standing for the home directory.

- The worktree holding the project's folder is marked as holding it, and is where the selector starts: the checkout
  the project's folder is in, which may be a linked worktree, never whichever worktree has some branch checked out.
- A worktree in which the project's folder is not a directory — missing there, or reached only through a symbolic
  link — says the project's folder is not in it, in the selector and above its changes.
- **Choosing a worktree changes only where the changes are read from.** It checks nothing out, moves no session and
  leaves the Files mode on the project's own folder. Its changes are read afresh; nothing of the previous worktree's
  list stays on screen, and the selected change is cleared.
- **A chosen worktree that is gone** — removed, its path now holding another checkout, or removed and added again —
  stays chosen: the selector names it as unavailable and the mode says it is no longer available and to choose another
  one. Another worktree's changes are never shown in its place.

The worktrees are read when the Git mode is first shown after the project pane is put in the right pane, on Refresh,
after the connection to the daemon comes back, and when the chosen worktree is found gone. A worktree added in the
meantime appears in the selector then.

## The change list

### What is listed

The changes are listed in sections, in this order, each headed with its name and how many changes it holds; a section
with none is left out:

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
and for a rename across the project's boundary, where it came from or went, outside the project. The letters:

| Letter | Status |
|---|---|
| A | added |
| M | modified |
| D | deleted |
| R | renamed |
| T | type changed — a file that became a symbolic link, say |
| U | untracked |
| C | in conflict |

The letter is decoration: assistive technology hears a row as its name, its status in words and what follows the
name. A name, or what follows it, too long for its row fades out, with its full text as its tooltip (see "Names too
long for their space" in `docs/product/labels-and-tooltips.md`).

- The arrow keys move through the rows, the section headings skipped, and typing a name's first letters moves to it.
- A click or Enter opens the change in the file viewer (see "Opening a change" below). Nothing in the list is
  selectable otherwise.
- The change the viewer showed last is the list's **selected change**: its row is tinted, assistive technology hears
  it named as selected, and the list keeps it in view behind the viewer while the viewer moves from change to change.

### What the list says instead

- **Loading** while the worktrees or the changes are read.
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

- once the Git mode is shown and the repository and the chosen worktree are known;
- **every 10 seconds** while the Git mode is on screen — the project pane docked, in an open drawer, or floating in —
  and the window is visible, skipping a turn while a listing is still out;
- when the window comes back to the front or becomes visible again, with the Git mode on screen, or when it comes back
  on screen — a hidden pane shown, or the Git tab chosen — either one more than 5 seconds after the last listing;
- on the header's **Refresh**, which reads the worktrees again too;
- after the connection to the daemon comes back, while the Git mode is on screen.

While the pane is hidden, or shows the Files mode, nothing is listed on its own. When the connection is lost, the list
on screen stays until it is back.

## Opening a change

A change opens in the project pane's file viewer (see "The file viewer" in `docs/product/project-pane.md`). Its title
is the file's name; under it are its path within the project, its section (Staged, Unstaged, Untracked or Conflicted)
and its status — Added, Deleted, Modified, Renamed, with the path it was renamed from, or Type changed. An untracked
file is Added; a path in conflict has its section alone.

### What it shows

- **A text change** is a diff, drawn from the patch Git makes for it: unified at first, or split into its two sides
  side by side, chosen above it. The choice holds while the viewer moves from change to change, until it closes. Long
  lines wrap. A patch of more than 1,000,000 characters or 10,000 lines is shown as the plain patch, with a line
  saying it is shown that way because the change is large; so is a patch the diff could not be drawn from. A patch
  that is not valid UTF-8 — of a file in another encoding — is shown with replacement characters.
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
- **An untracked file** is shown as new content: the file itself, as the file viewer shows a file, with a line saying
  there is no earlier version to compare it with. An untracked symbolic link, or an untracked repository of its own,
  says what it is, having no content to show.
- **A rename across the project's boundary** shows only the version inside the project, as a file rather than a diff,
  under a warning that the previous or the new version is outside the project, with its path from the repository's
  root. Nothing of the version outside is read.
- **A type change** — a file that became a symbolic link, or the reverse — is shown as two diffs, Before and After,
  one per side, sharing one choice of unified or split, which is left out when both are shown as plain patches. A
  rename into or out of a path below itself (`foo` to `foo/bar`, or back) is shown the same way: its old path's
  removal and its new path's addition, under Renamed.
- **No differences**, when the two sides now hold the same content: the change was staged, committed or undone since
  it was listed.
- **A path in conflict** is not a two-sided change: a warning names the kind of conflict — both modified, both added,
  both deleted, added by us, added by them, deleted by us, deleted by them — above the file as it is on disk, with its
  conflict markers where both sides modified or added it. A path both sides deleted has no file to show and says it
  does not exist.

How a diff is coloured in each appearance is in "What follows the choice" in `docs/product/appearance.md`.

### Moving between changes

**Previous file** and **Next file**, and the **Left** and **Right** arrow keys, move through the change list as it is
on screen, top to bottom, from one section into the next, and stop at either end rather than going round. A file in
both Staged and Unstaged is two stops. A change the list no longer has moves on to the changes that were beside it.
When the keys do nothing, and how they mirror under a right-to-left language, is as in "Moving between files" in
`docs/product/project-pane.md`.

### Closing it

**Escape** or the viewer's **Close** button closes it. Keyboard focus then goes to the row of the change it showed
last.

### When the open change changes

The listings that keep the change list up to date (see above) also decide when the open change is read again:

- a listing with news of it — the change listed at other versions (a file overwritten, staged again or committed), or
  gone from a list that is not cut short — reads it again, and it shows as it is now;
- a change that kept changing while it was read, after two more tries, says so, and is read again at each new listing
  that still has it, until a read succeeds;
- a change whose worktree is found gone, by its list or by the worktrees read again, is read again and then says that
  worktree is no longer available.

When the connection to the daemon is lost, what is shown stays until the connection is back, and then the change is
read again; a change that had nothing on screen yet says the connection was lost and that it is read again once it is
back.

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

## What is remembered

**Per project, for as long as the window is open**: the chosen worktree and the selected change. Reloading the window,
or restarting the application, starts every project on the worktree holding its folder, with no change selected.
Change lists and their contents are never stored. The mode itself is kept with the rest of the pane's state (see "What
is remembered" in `docs/product/project-pane.md`).
