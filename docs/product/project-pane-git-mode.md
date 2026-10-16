# Project pane Git mode

The project pane's **Git mode** reviews the project's repository in two views: **Uncommitted**, the uncommitted
changes of one worktree — what is staged, what is in conflict, what is changed on disk and what Git does not track —
and **Compare**, what one local branch has changed since it forked from another. Either view opens each change in the
project pane's read-only file viewer. Git is one of the project pane's two modes: opening the pane, its
header, switching modes and the file viewer in general are in `docs/product/project-pane.md` (see "Files and Git" and
"The file viewer" there).

## Reading only

The Git mode reads the repository and never writes to it. It stages, unstages, commits and checks out nothing —
comparing two branches checks neither out — takes no lock, never refreshes the index (not even for a file whose
timestamps moved while its content did not), and fetches nothing from a remote. The one exception is Git itself before
version 2.45: reading an object that a partial clone does not have yet can make such a Git fetch it.

The project's **branch badge** in the sidebar is separate (see `docs/product/project-git-status.md`): its check fetches
the remote's refs and, with Automatically pull repositories on or on Pull current branch, may fast-forward the branch.
Pushing the current branch is not that check and not this mode (see "Pushing the current branch" in
`docs/product/project-git-actions.md`). The Git mode does none of these and refreshes none of them. A change of the
reported branch or detached state refreshes the visible mode
(see "Files and Git" in `docs/product/project-pane.md`); switching from the project action menu remains separate
from this read-only mode.

## Uncommitted and Compare

The project pane's single header row holds three peer tabs, **Files**, **Uncommitted** and **Compare** (see "Files and Git" in
`docs/product/project-pane.md`). Choosing either Git view's tab shows that view directly; the initial Git view is
Uncommitted. The tabs stay available while the repository is being read, cannot be read, or is not a repository.
Two controls stay between the tabs and Refresh in every state: the one that shows and hides the
file-name filter field, then **Group by folder** (see "Filtering by file name" and "Flat or grouped by folder" below).

Each view reads the repository only while it is on screen: the Uncommitted view as "Keeping the list up to date"
below says, the Compare view as "When a comparison is made" below says. The repository's branches are read while
either view is on screen (see "The branch selectors" below). Switching views leaves the other view as it was. "The
worktree selector" and "The change list" below are about the Uncommitted view, "Comparing two branches" below about the
Compare view; "Folding a section", "Flat or grouped by folder", "Filtering by file name", "Following the selected
session" and "Opening a change" are about both.

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
detached, or as having no commits yet; under its name is its path, `~` standing for the home directory. A name or path
too long for its space fades out (see "Names too long for their space" in `docs/product/labels-and-tooltips.md`).

- The worktree holding the project's folder is marked as holding it, and is where the selector starts: the checkout
  the project's folder is in, which may be a linked worktree, never whichever worktree has some branch checked out.
  Selecting one of the project's sessions can choose another (see "Following the selected session" below).
- A worktree in which the project's folder is not a directory — missing there, or reached only through a symbolic
  link — says the project's folder is not in it, in the selector and above its changes.
- **Choosing a worktree changes only where the uncommitted changes are read from.** It checks nothing out, moves no
  session, leaves the Files mode on the project's own folder and plays no part in a branch comparison. Its changes
  are read afresh; nothing of the previous worktree's list stays on screen, and the selected change is cleared.
- **A chosen worktree that is gone** — removed, its path now holding another checkout, or removed and added again —
  stays chosen: the selector names it as unavailable and the mode says it is no longer available and to choose another
  one. Another worktree's changes are never shown in its place.

The worktrees are read when the Git mode is first shown after the project pane is put in the right pane, on Refresh,
after the connection to the daemon comes back, when the chosen worktree is found gone, and when one of the project's
sessions is selected (see "Following the selected session" below). A worktree added in the
meantime appears in the selector then.

## The change list

### What is listed

The Uncommitted view's changes are listed in sections, in this order, each headed with its name and how many changes
it holds; a section with none is left out:

- **Conflicted** — paths in conflict. It is first, being what has to be dealt with before a commit can be made.
- **Staged** — the commit at `HEAD` against the index. Before the first commit every staged file is an addition.
- **Unstaged** — the index against the files on disk.
- **Untracked** — files on disk Git neither tracks nor ignores, each listed by itself rather than as its folder.

A file staged and changed again since is in both Staged and Unstaged: two changes, each against its own baseline.

Not listed:

- Files the repository ignores, which the Files mode still shows.
- A submodule's own uncommitted work, which belongs to that repository. A submodule is listed only when it is checked
  out at another commit than the index records. An untracked folder that is a repository of its own is listed as one
  entry. **A submodule moved to another path** is listed as one renamed change only when both sides record the same
  commit — Git pairs two submodules by exact match and never by similarity — and as a deletion at the old path plus
  an addition at the new one as soon as the recorded commits differ, whether or not the two paths nest.

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
below). At the row's end is the button of its own actions (see "A row's actions" in
`docs/product/project-pane.md`). The letters:

| Letter | Status |
|---|---|
| A | added |
| M | modified |
| D | deleted |
| R | renamed |
| T | type changed — a file that became a symbolic link, say |
| U | untracked — in a muted coral of its own, not Added's green |
| C | in conflict |

**The letter says in a tooltip what it stands for**: resting the mouse on it shows the status named in words, in the
same capitalised wording the file viewer's header carries as an opened change's tag (see "Opening a change" below).
The letter is the only part of the row that opens it, and it opens on the mouse alone, after the usual wait: the
letter is no tab stop and adds none to the row, so the tooltip never shows on keyboard focus (see "Tooltips on
indicators" in `docs/product/labels-and-tooltips.md`). The letter itself stays out of the accessibility tree, which
costs nothing: assistive technology hears a row as its name, its status in words and what follows the name.

A name, or what follows it, too long for its row fades out, with its full text as its tooltip (see "Names too
long for their space" in `docs/product/labels-and-tooltips.md`), each on the part of the row it belongs to.

- **Up** and **Down** move through the rows and the section headings between them (see "Folding a section" below),
  **Home** and **End** through the rows alone, and typing a name's first letters moves to a row.
- A click or Enter opens the change in the file viewer (see "Opening a change" below). Nothing in the list is
  selectable otherwise.
- The change the viewer showed last is the list's **selected change**: its row is tinted, assistive technology hears
  it named as selected, and the list keeps it in view behind the viewer while the viewer moves from change to change.

### Folding a section

**Every section heading is a control**: pressing anywhere on its band folds the section away, and pressing it again
unfolds it. That holds for every section of either view — Conflicted, Staged, Unstaged, Untracked and the
comparison's Changed files (see "What the view says" below) — and in both forms of the list, flat or grouped by
folder.

**The heading stays at the top of the scrolling change list while its rows move underneath.** As the next section's
heading reaches the top, it pushes the current heading upward out of view and takes its place. Each heading is
**30 px** high, with an opaque background, and the first begins at the list's top edge without a padding gap below
the worktree or branch selectors. The edge fade begins below the heading, and a row scrolled into view for keyboard
focus or as the viewer's selected change stays clear of both (see "The edge that hides content fades" and
"Scrolling a surface from the keyboard" in `docs/product/scrolling-surfaces.md`).

- **A folded section keeps its heading and its count** — of the changes it holds, those the filter leaves (see
  "Filtering by file name" below) — and shows none of its rows. Those changes have no row at all, so the viewer moves
  past them (see "Moving between changes" below), as it moves past the changes under a folded folder.
- Grouped by folder, **the folders inside a folded section keep the state they were in** and come back as they were
  when the section is unfolded (see "Flat or grouped by folder" below).
- The heading shows **a chevron**, pointing right while the section is folded and down while it is not; here it is
  not a control of its own, the whole band being one. Assistive technology is told whether the section is unfolded. The band **carries no tooltip**, saying in text what it holds, and answers a hover with
  nothing: the chevron is what marks it as a control.
- **The band is no tab stop of its own.** The change list is one tab stop, as "Rows and the keyboard" above says: a
  heading is reached by arrowing onto it with Up or Down, and **Enter** or **Space** on it folds and unfolds the
  section. Home and End pass the headings by.
- Pressing a heading with the pointer leaves keyboard focus where it was. **Folding a section that holds the row with
  keyboard focus puts focus on the heading**, which stands for the rows it has taken away.
- **Everything starts unfolded.** A section folded in one form of the list is folded in the other, and a fold
  survives switching to the other view and back — the comparison's Changed files being a section of its own, the two
  views never share one. What is folded is kept per project for as long as the window is open (see "What is
  remembered" below).

### Flat or grouped by folder

**Group by folder**, the second of the two Git toggles in the pane's header, switches the change list between
one row per change and the changes grouped under the folders they are in. It is a single icon button whose icon shows
which form the lists are in and whose name says what pressing it will do, so the name alternates between "Group by
folder" and "Show as flat list" (see "Two-state icon toggles" in `docs/product/labels-and-tooltips.md`). Each project
and each Git view keeps its own layout across reloads and restarts. A view without a saved layout starts flat, or
uses a grouped preference retained from before per-view choices were available.

Grouped by folder, a list keeps its sections and their counts — the Uncommitted view's Conflicted, Staged, Unstaged
and Untracked, the comparison's one section — and within each section the changes sit under their folders:

- Within a folder, folders come first and then changes, each in the order the flat list has them (see "What is
  listed" above).
- A chain of folders that each hold nothing but the next is one row, named by the whole chain — `tools/release`.
- A folder's row is a folder icon, open or closed with the folder, and its name; the icon is the folder's expand
  control, so the row has no chevron. A change's row is as it is in the flat list, less the folder its parent row now
  carries.
- Folders start open, and which ones the user folded away is kept per project for as long as the window is open: a
  reload or a restart opens them all again. A fold survives switching to the other view and back, and belongs to the
  section it was made in, so the same folder under another section stays open.

The rows take the keyboard as the Files mode's tree does (see "Rows and the keyboard" in
`docs/product/project-pane.md`): Up and Down move through them and the section headings between them, Home and End
through the rows alone, typing a name's first letters moves to one, Right and Left open and close a folder (the other
way round under a right-to-left language), and Enter opens a change or opens and closes a folder. A folder's row tells
assistive technology whether it is open, and its folder icon is named Expand or Collapse (see "Tooltips on icon-only
controls" in `docs/product/labels-and-tooltips.md`).

A change under a folded folder has no row, and it is the rows that the viewer moves through (see "Moving between
changes" below).

### Filtering by file name

The **filter field** narrows a change list to the changes whose file name contains its text, in both views and both
forms of the list, flat or grouped by folder. It is not on screen to begin with: the **first of the two Git toggles in the
pane's header** shows and hides it, its name saying what pressing it will do — "Filter by file name"
while the field is hidden, "Hide the filter" while it is shown (see "Two-state icon toggles" in
`docs/product/labels-and-tooltips.md`). Assistive technology is also told whether the field is on screen.

- **Opening it from the toggle puts the field directly below the pane's header**, above the worktree selector in
  Uncommitted or the From and To selectors in Compare, and **keyboard focus in the field**. The field is there in
  every state of a view, over the lines that say the changes are being read, that reading them failed, that there is
  nothing to list, or that a branch has still to be chosen (see "What the list says instead" and "What the view says"
  below), so the toggle always has a field to open.
- **An empty field takes itself away once keyboard focus leaves it** — on Tab out of it, or on a press that puts focus
  elsewhere — so the only field on screen is one whose effect can be seen; a field holding nothing but whitespace
  counts as empty. The window itself losing focus is not focus leaving the field: the field stays for the user coming
  back to it. **A field with text typed in it stays**, so no list is left narrowed by a field that has gone.
- **Closing it from the toggle drops the text with the field**, and hands keyboard focus back to the toggle when the
  field was holding it. Opening the field again starts from empty.
- **Only the file's name is matched** — the last part of its path, as its row shows it — ignoring case and any
  whitespace around the text; the folders it is in are not, so `src` does not keep every file under `src/`. A rename
  is matched by its new name. An empty field, or one holding only whitespace, shows every change.
- **It narrows what is shown, not what is listed.** The sections' counts are of the rows shown, a section with no
  match is left out, and grouped by folder only the folders leading to a match remain. What the list holds, and when it
  is listed again, is unchanged; a list listed again is narrowed by the same text.
- **When nothing matches**, the list's place says there are no matching changes (announced to assistive technology
  as it appears), and the field stays, so the text can be changed or cleared. Clearing it brings the whole list back.
- The field has a **clear** button, and **Escape** in it clears it; the field itself stays, keyboard focus not having
  left it. In a drawer or a floating pane, Escape in a field with text only clears it and leaves the pane; Escape in
  the then empty field closes the pane, as it does from anywhere else in it.
- The field is apart from the list: typing in it never moves through the list's rows, as typing a name's first
  letters in the list does. While an input method is composing text in it, its keys, Escape included, belong to the
  composition, and do not clear the field.

Each Git view keeps its own keyword, field-open state, layout and folded sections and folders independently of the
other view and Files. Filter lifetime and tab-switch focus behavior are in "Filtering by file name" in
`docs/product/project-pane.md`. A worktree or branch selection keeps that view's keyword. The viewer moves through
the rows the filter leaves (see "Moving between changes" below).

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
  comes back on screen — a hidden pane shown, the Uncommitted tab chosen — either one more than 5
  seconds after the last listing;
- on the header's **Refresh**, which reads the worktrees and the branches again too;
- after the connection to the daemon comes back, while the Uncommitted view is on screen.

While the pane is hidden, or shows the Files mode or the Compare view, nothing is listed on its own. When the
connection is lost, the list on screen stays until it is back.

## Comparing two branches

The **Compare** view shows what one local branch of the repository, To, has changed in the project's folder since it
forked from another, From. Neither branch has to be checked out, in any worktree, and nothing is checked out, fetched or
written to compare them. The worktree chosen in the Uncommitted view plays no part: a comparison means the same
whichever worktree is chosen.

### The branch selectors

At the top of the Compare view are two branch selectors, each with its label before it: **From**, the branch compared
from, and **To**, the branch whose changes are shown (see "What is compared" below).

- Each one offers every local branch of the repository — checked out in some worktree or in none — in byte order of
  their names, each with the first seven characters of the commit at its tip. At most 10,000 branches, or as many as
  2 MiB of names hold, are offered, the first in that order; nothing says the list was cut.
- A branch Octoboard can name the session that created it for carries a small ring between its name and its commit,
  **in the two lists only and never on a selector's chosen branch**; most branches carry none, which is ordinary (see
  "The creator badge" in `docs/product/branch-creator.md`).
- A selector with no branch chosen says to choose one. A chosen branch's name too long for its space fades out (see
  "Names too long for their space" in `docs/product/labels-and-tooltips.md`), in the selector and in its list.
- A chosen branch that a complete branch list no longer has stays chosen, its name marked as no longer existing, until
  another one is chosen.
- **Swap branches**, beside the selectors, exchanges From and To, which shows what the other branch has changed since
  the same fork — not the same changes turned around. It is unavailable while neither side has a branch chosen.
- **Choosing other branches, or swapping them, is another comparison**: nothing of the previous one stays on screen,
  and its selected change is cleared. Choosing the branches already chosen changes nothing.
- A broken branch — its reference names an object the repository does not have, or one that is not a commit — is
  offered with the id its reference names, and comparing it fails (see "What the view says" below).

The branches are read while the Git mode is on screen in either view, once the repository is found — the Uncommitted
view needs them too, to follow the selected session (see "Following the selected session" below). They are read when
the Git mode is first shown after the project pane is put in the right pane, and then the way the Uncommitted view's
changes are listed again (see "Keeping the list up to date" above), with the Git mode in place of the Uncommitted view:
every 10 seconds while it is on screen and the window is visible, when it comes back more than 5 seconds after the
last reading, on Refresh from either view, and after the connection to the daemon comes back; and, as the worktrees
are, when one of the project's sessions is selected (see "Following the selected session" below). A branch created,
moved or deleted in the meantime shows in the selectors then.

When the branches cannot be read, the Uncommitted view says nothing, and the Compare view says so with the reason: in
place of the empty state asking for a branch on each side, with **Try again**, while a branch is missing on either side
and no list of branches has been read yet; otherwise in a line under the selectors, announced to assistive technology,
the branches already offered staying on offer, until the branches are read again.

### What is compared

- **Since the fork.** The old side is the commit the two branches forked at — their merge base, as
  `git diff From...To` takes it — and the new side is the commit at To's tip, so the comparison shows only what To has
  changed since it forked from From; what only From has done since is not in it. Where the two branches have more than
  one best common ancestor, the one `git merge-base` gives is the fork. Two branches at the same commit have no
  differences, and neither has a To that From already contains (To an ancestor of From), which has changed nothing
  since the fork. Two branches with no commit in common have no fork to compare from and are not compared (see "What
  the view says" below), rather than one whole branch being shown as a change of the other.
- **At the commits the branches were at when it was made.** The fork is found once, together with the two tips, and
  the changes listed, and every change opened from them, are between the fork and To's tip as they were then until the
  comparison is made again: a branch that moves or is deleted in the meantime never changes what is shown (see "A
  branch that moves or goes away" below).
- **Nothing is read from the disk or from any worktree's index**, so a file is compared and opened whether or not it
  is in the project's folder on disk or in the chosen worktree.
- **A subdirectory project** is compared within its scope as the Uncommitted view lists it (see "What is listed"
  above): a change is listed when its old or its new path is inside the project's folder, and a rename across the
  folder's boundary names its other side as outside the project. When comparing the whole repository's trees gives
  more than 8 MiB of output from Git — far-apart branches of a large repository — a subdirectory project is compared
  within its own folder instead, where a rename across the boundary is listed as the addition or the deletion of its
  half inside. A project at the repository's root whose comparison is that large cannot be compared, and says so.
- A submodule is listed when the fork and To's tip record it at different commits. A type change is one change, and a
  submodule moved to another path is paired by the same exact-match rule, both as in the Uncommitted view.

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

- **To choose a From and a To branch**, saying that the comparison shows what To has changed since it forked from
  From, while a branch is missing on either side, or, when the repository has no branches, that it has none yet.
- **Comparing** while a comparison is made with none on screen, announced to assistive technology.
- **The commits compared**, the fork's and then To's, each by its first seven characters, in a line above what the
  comparison found:
  - the changed files, in one section, **Changed files**, headed with how many changes it holds. Its rows, their
    order, their status letters (A, M, D, R or T here), the keyboard, their action button and the selected change are
    as in the Uncommitted view (see "Rows and the keyboard" above and "A row's actions" in
    `docs/product/project-pane.md`), its heading folds the section away as the Uncommitted view's do (see "Folding a
    section" above), and it is grouped by folder
    with it (see "Flat or grouped by folder" above) and narrowed by the same filter (see "Filtering by file name"
    above);
  - **Not every change is shown**, under a list cut short: at most 10,000 changes, or 2 MiB of paths, are listed —
    those Git reported first;
  - **both branches are at the same commit**, so there are no differences;
  - **no differences**, To having changed nothing in the project's folder since the branches forked — a To that From
    already contains among them.
- **A failure** — the comparison could not be made — names the reason (a branch that no longer exists or is broken, or
  two branches with no commit in common, so nothing to compare from, say), with **Try again**. In a shallow clone, two
  branches with no commit found in common are reported instead as a history that may be incomplete: no shared commit was
  found, and since the repository is a shallow clone, the history that would show one may be missing. Try again pressed
  from the keyboard hands keyboard focus to the header's Refresh.

When the list goes away while one of its rows holds keyboard focus, focus goes to the From selector.

### A branch that moves or goes away

The comparison on screen stays on the commits it was made at. Once a branch list read after the comparison came in
shows a compared branch at another commit, a notice under the commits line says that branch has moved since the
comparison and that Refresh compares its current commit. Once a complete branch list no longer has a compared branch,
the notice says it no longer exists and that the comparison still uses the commits it was made at, and its selector
names it as no longer existing. A branch list cut short says nothing of a branch it does not have. A notice that appears
is announced to assistive technology.

Refresh then compares the branches as they are now, finding their fork again, or, for a branch that no longer exists,
fails saying there is no such branch. A change opened from the comparison still on screen reads from its commits for as
long as the repository has them (see "Errors" below).

## Following the selected session

Selecting one of the project's sessions in the sidebar sets the Git mode's selectors from **the session's branch**: of
the branches whose recorded creator is that session (see "When a branch has a creator" in
`docs/product/branch-creator.md`), the one created last. The selectors of both views are set together, whichever view
is on screen; the view shown does not change.

- **The worktree selector** chooses the worktree that has the session's branch checked out, and the worktree holding
  the project's folder when none has.
- **From** takes the branch checked out in the repository's main working copy, and **To** the session's branch, so the
  comparison shows what the session's branch has changed since it forked from it. With the main working copy's `HEAD`
  detached, or on the session's branch itself, From is left with no branch chosen, so no comparison is made until one
  is.
- **A session with no branch of its own** sets the selectors back to where a project starts: the worktree holding the
  project's folder, and neither From nor To chosen. A branch list that could not be read counts as naming no branch
  for the session.
- A selector set this way is as one chosen by the user (see "The worktree selector" and "The branch selectors" above):
  moving it clears the selected change of its view, and setting it to what it already holds changes nothing.

When the selectors are set:

- **Once for each selection of one of the project's sessions.** Every change of the selected session is a new
  selection, whether or not the project pane was in the right pane when it was made: selecting a session, then any
  other session — of this project or another, or a console session — and then the first again sets them again. The
  project pane put back in the right pane, or the Git mode shown again, while the same session stays selected does not
  set them again: what the selectors hold stands, the user's choices included (see "What is remembered" below).
- **Only from what is read after the selection.** The worktrees and branches the pane held from before the selection
  are asked for again — at once while the Git mode is on screen, otherwise when it next comes on screen — and the
  selectors are set once both have been read since, never from the earlier lists. Until then they hold what they held.
- **A session's branch that no listed worktree has checked out** sets From and To at once, and the worktree selector
  to the worktree holding the project's folder, and the worktrees are asked for once more: the reply that follows sets
  the worktree selector to the worktree that has the branch checked out by then, provided the user has changed no
  selector in the meantime. When still no worktree has it, the worktree holding the project's folder stays chosen.
- **A choice made in a selector stands while the same session stays selected**: nothing sets the selectors again until
  the next selection.
- **A session's branch that turns up later** — in a branch list read after the selection, its creator having been
  recorded only after the branch itself appeared, say — is followed then, once, provided the user has changed no
  selector since the selection. Once a session's branch has been followed, later branch lists leave the selectors alone,
  even one that names a newer branch for the session.
- **Selecting any other session** — another project's, or a console session — or none leaves the selectors as they
  are.

## Opening a change

A change opens in the project pane's file viewer (see "The file viewer" in `docs/product/project-pane.md`). Its title is
the file's name, with the kind of change after it as a tag — Added, Deleted, Modified, Renamed or Type changed —
coloured as the change's letter in the list is (green, red, amber, blue; the same colours for the same kind). A staged
or unstaged change has its stage — Staged or Unstaged — as a second, neutral tag after the kind: the two things a
change says, what kind it is and whether it is staged, are kept apart, the first coloured and the second not. Both tags
come after the name, and read with it as "server.ts: Modified, Unstaged". An untracked file is always new, so its only
tag is Untracked, in the muted coral the list's U has, which is no kind of change and shares no colour with Added or
Deleted; a path in conflict has only a Conflicted tag, coloured as its C is. Neither has a stage tag, its status tag
already saying where it is from; a change from the Compare view has none either. The tags read as one group,
spaced closer to each other than the group is to the name, and each of them, the neutral stage tag included, is visibly
a tag against the viewer's background in either appearance. The tags never shrink; the name fades out when there is no
room for it.

A change from the Compare view carries the comparison it is read from in the header's **subtitle**, the line directly
under the title: To and the commit it was at, then From and the commit the two forked at, each commit by its first
seven characters — "feature at 9e8d7c6, since it forked from main at 4f2a9c1", say. The full wording is that line's
tooltip where it is cut off (see "The file viewer" in `docs/product/project-pane.md`), and each branch name in it is
laid out in the direction of its own text (see "Right-to-left layout" in `docs/product/window-layout.md`). A change from
the Uncommitted view has no such line.

Under that are the change's details, laid out as the viewer lays out any subject's (see "The file viewer" in
`docs/product/project-pane.md`): the path within the project and, for a rename, the path it was renamed from.

A change from the Compare view is read from the two commits of the comparison on screen, its old side from the commit
the branches forked at and its new side from To's, and is shown as a staged change is below; it is never untracked or in
conflict.

### What it shows

- **A text change** is a diff, drawn from the patch Git makes for it: unified at first, or split into its two sides
  side by side, chosen among the view controls in the viewer's header. The choice is offered only for a diff with
  lines on both sides: an added or a deleted file reads the same either way, so it has none. It is
  remembered: later diffs open in the layout last chosen, across changes and after the app restarts. Only the patch's
  hunks and the lines around them are shown, each run of unchanged lines between them collapsed into a separator that
  can be expanded (see "Expanding the collapsed lines" below). A change whose patch has no lines — an empty file added
  or removed, say — says it has no diff to show; one whose two sides hold the same content, under one name or two,
  shows the file whole instead, as the bullets below say. Whether its long lines wrap is the viewer's own choice, which
  a diff shares with every file (see "Wrapping long lines" in `docs/product/project-pane.md`). A patch of more than
  1,000,000 characters or 10,000 lines is shown as the plain patch, under a notice saying it is shown that way because
  the change is large; a patch the diff could not be drawn from is shown the same way, under a failure notice saying
  so (see "Notices and stand-ins" in `docs/product/project-pane.md`). A patch that is not valid UTF-8 — of a file in
  another encoding — is shown with replacement characters.
- **A missing newline at the end** of either version is said in a notice above the diff — the old version, the new
  one, or neither has one — rather than as a line of it. A symbolic link's change never says so, a link's target
  having no newline at its end anyway. A plain patch keeps Git's own marker lines.
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
- **A type change** — a file that became a symbolic link, or the reverse — is one diff, with the choice of unified or
  split and the layout remembered, as a modified file's is. Git writes such a change as the old side's removal and the
  new side's addition, and the diff holds the two together: the whole old side as removed lines, then the whole new
  side as added ones. **The two sides are never compared line by line** — a file's lines and a link's target have
  nothing to pair up — so each side is shown whole, with no run of unchanged lines: the diff collapses nothing and has
  no separator to expand. It offers the layout choice by the rule above, so an empty file that became a link, having
  added lines alone, offers none. The kind of change stays in the title's tag. A type change into or out of a path
  below itself (`lib` to `lib/index.ts`) is shown this way as well, not as the rename below it is.
- **A rename into or out of a path below itself** (`lib` to `lib/index.ts`, or back) is compared line by line like any
  other change: its diff is the one between exactly its two versions. Identical content therefore says the change was
  renamed and nothing more, as the bullet below it says, and edited content gives an ordinary diff — hunks with
  context, the runs between them collapsed into separators that can be expanded, the layout choice, Show whole file
  and the run beyond the last hunk, all as above. It shows under Renamed, with the path it came from among the
  details, and nothing below the outer path is read, named in its diff or counted against the limits the read is held
  to. This holds in the Staged section and in the Compare view, in both directions of the nesting, and it takes both
  sides being a file or a symbolic link: with a side that is a repository of its own the change is shown instead as
  the old path's removal followed by the new path's addition in one diff, collapsing nothing and with no separator to
  expand, as a type change is; each side shows only the commit it records, nothing from inside the nested repository.
  Two submodules are listed as one renamed change only when both record the same commit (see "What is listed" above),
  so that is the only way a nesting pair of them is ever opened.
- **A rename with nothing else changed** says it was renamed and its content is unchanged, and shows that content as
  below. It is the wording of every rename whose patch shows nothing but the path changed — an empty patch, or one
  holding the rename's header alone — wherever it was listed and whether or not its two paths nest. A rename whose
  patch shows that something else did change, its executable bit or a side Git treats as binary, is not worded that
  way: it says it has no diff to show, as any change with no lines does.
- **No differences**, when the two sides of a change that is not a rename now hold the same content: the change was
  staged, committed or undone since it was listed. Its content is shown as below as well.
- **A path in conflict** is not a two-sided change: a warning names the kind of conflict — both modified, both added,
  both deleted, added by us, added by them, deleted by us, deleted by them — above the file as it is on disk, with its
  conflict markers where both sides modified or added it. A path both sides deleted has no file to show and says it
  does not exist.

**A change with no lines to draw shows the file itself.** For the two cases above — the same content under one name or
two — there is nothing to compare, so the whole content is read instead, the read a diff's collapsed lines use (see
"Expanding the collapsed lines" below), once as the change opens; while it is in flight the viewer shows its usual
loading, announced as a file's read is (see "What it shows" in `docs/product/project-pane.md`). It is then shown whole
as a file rather than as a diff — so a Markdown file is offered as a document with its source a choice away, exactly as
an untracked file is — under a neutral notice saying which of the two cases the change is, in one sentence. Where the
content cannot be had after all, the change falls back to the centred stand-in and shows none of the file, saying there
the same state the notice would have: as a heading, with the line under it where it has one, rather than as that one
sentence (see "The same state said in one sentence" in `docs/product/empty-states.md`). With a reason to give — the read
itself failed, a lost connection to the daemon among the reasons — a failure notice above it says the file's content
could not be read and gives that reason; where the content simply cannot be had — a side the daemon will not deliver
whole, by the same rule as a diff's collapsed lines, or the change having moved on since it was read, or the two sides
turning out to differ from what the change says they are — a neutral notice above it says which of those it was. Only a
change with nothing to read the content from leaves the stand-in alone: one whose two sides are not both files whose
content can be read, a rename between two symbolic links among them. Every one of these notices and the stand-in are as
"Notices and stand-ins" in `docs/product/project-pane.md` describes them.

A whole file body shown here instead of a diff — an untracked file, one added with no patch, the version inside the
project of a rename across the boundary, or a change with no lines to draw whose content was read — is a Markdown
document when the file is Markdown, as it is for a file read for itself (see "A Markdown file as a document" in
`docs/product/project-pane.md`). Following a link in that document to another project file leaves Git mode, as that
section describes. A diff and the file behind a path in conflict are shown as source whatever the file is.

How a diff is coloured in each appearance is in "What follows the choice" in `docs/product/appearance.md`. A diff's
code is focused, selected and copied as a file's is (see "Selecting and copying" in `docs/product/project-pane.md`).

### Expanding the collapsed lines

Each run of unchanged lines a diff leaves out stands in it as a **separator** saying how many lines it holds — "6
unmodified lines" — in the current language (see "What follows the language" in `docs/product/language.md`); once part
of a run has been revealed, its separator counts the lines of it that are still hidden. Where the lines can be shown,
the separator reveals them, in both the Uncommitted and the Compare view and in both diff layouts:

- **One expansion reveals 20 lines.** A run between two hunks longer than that offers two controls: one reveals the 20
  lines at the run's top, which appear above the separator, the other the 20 at its bottom, which appear below it. A
  shorter run, and the run before the first hunk or after the last, offers one. The **third expansion of the same run**
  reveals whatever is left of it, whichever of its controls is used.
- **Show whole file**, at the end of a separator, reveals the file's every line at once, leaving no separator behind.
  Each run offers it once — in the split layout on one side only. With wrapping off it stays at the visible end of the
  bar as the diff is scrolled sideways, as the separator's text and controls do.
- **A separator's text is a control of its own**, which **reveals the whole of its run at once**, however long the run
  is and however many of its lines the other controls have revealed already. It is named for that, with the count it
  shows — "Show all 53 unmodified lines" — so what it offers, and not only how many lines are hidden, is announced;
  on a separator whose count came from the file's length rather than from the patch the name carries no count. It is
  a tab stop and is used with Enter or Space like the other controls. The text of a separator that offers no
  expansion is inert.
- **The run beyond the last hunk** is a separator of its own, with the one control such a run has. A patch never says
  how long a file is, so the run's length comes from the length of the file the change was read at: where the last hunk
  reaches the file's end there is nothing beyond it and **no separator is drawn at all**; where lines do follow, the
  separator counts them as every other one does. Where the length is not known, it says instead that more unmodified
  lines may follow.
- **The lines are read only when the user expands.** Nothing of them comes with the patch, with a refresh, or when the
  connection to the daemon comes back. The first expansion of a diff reads both of its sides whole, and every later
  expansion of the same diff uses what that read returned. The read writes nothing to the repository and takes no
  lock, as nothing in the Git mode does (see "Reading only" above).
- **An expansion lasts as long as the diff is on screen.** A change opened again, or read again at other versions, is
  drawn with its lines collapsed again.

While the lines are being read the separator says so, and no other expansion starts until that read is answered: every
control of the diff reads as unavailable meanwhile. That, and each notice below, is announced to assistive technology.
Then:

- **A failed read** leaves the diff as it is: the separator says the lines could not be loaded and to try again, a
  failure notice above the diff gives the reason — that the connection to the daemon was lost, among them — and the
  next expansion reads again. A read of the change itself that follows — on a listing with news of it, or once the
  connection to the daemon is back — clears the failure without another expansion.
- **A change that has moved on** since its patch was read, its file or its index having changed underneath, keeps every
  line the patch shows, with a notice above the diff saying its unmodified lines cannot be shown, and gives the
  expansion up as below; it is offered again only once the change is read again (see "When the open change changes"
  below). A change from the Compare view, read from two fixed commits, cannot move on this way.
- **Moving on to another change** while the lines are being read gives that read up, saying nothing.

**A diff that offers no expansion** keeps its separators with their counts and nothing else: no controls, and no
separator for the run beyond the last hunk, which only an expandable diff has. That is so from the start for a diff
whose two sides are not both files whose content can be read — an added or a deleted file, which has one side only, a
symbolic link or a submodule, hence a type change, and a version outside the project. A type change has no separator
at all, its diff holding both of its sides whole so that nothing in it is collapsed (see "What it shows" above). The
rest is found only on the first expansion, and the diff is then drawn again in that form, so the separator for the run
beyond its last hunk goes on that press: a side the daemon will not deliver whole — one larger than the 4 MiB a read
opens, a file type it does not read, a side read from the worktree at a path with a clean filter in effect (see
below), a change it cannot make sense of — a side past the viewer's own budget of 10,000 lines or 1,000,000
characters, and the change having moved on. Each of the three puts a notice above the diff, and each is announced:
that the whole file is not delivered for this change, that the file is too large to be shown in full here, or, as the
bullet above says, that the change has moved on since it was read.

**A path with a clean filter** is one Git filters on its way into the repository — a `filter` attribute naming a
driver configured with a `clean` or a `process` command, as Git LFS sets up for every path it tracks. The patch's
hunks are numbered from what that filter produces rather than from the bytes the file on disk holds, so the daemon
delivers no side read from the worktree at such a path. Only an unstaged change has such a side: a staged change and
both sides of a Compare view comparison are read from the repository, which holds the filtered content already, and
expand as any other change does, and an untracked file is shown as itself with no diff to expand (see "What it shows"
above). A `filter` attribute whose driver has neither command configured filters nothing, so that path expands
normally too. Where such a change has also moved on since it was read, the notice is the one for having moved on.

From the keyboard, **Tab** from the code region moves into the separators' controls and then along them, run by run
from the top of the diff and, within a run, in the order they are read — the control or pair that reveals part of the
run, the separator's own text, then Show whole file — and after the last one leaves the diff; **Shift+Tab** from the
first goes back to the code region. **Enter** or **Space** uses a control, and once the diff is drawn again keyboard
focus is back on it, or on the nearest control left where that one is gone, or on the code region where none is left
at all — after Show whole file, and after a diff turns out to offer no expansion. A control reached from the keyboard
shows a focus ring, in either appearance; pressing one with the mouse leaves keyboard focus where it was.

### Moving between changes

**Previous file** and **Next file**, and the **Left** and **Right** arrow keys, move through the change list the change
was opened from — the Uncommitted view's or the comparison's — as it is on screen, top to bottom, from one section
into the next, and stop at either end rather than going round. They follow the rows as shown: a change the filter
hides is skipped (see "Filtering by file name" above), and so are the changes in a folded section (see "Folding a
section" above) and, grouped by folder, those under a folded folder. A file in both Staged and Unstaged is two stops. A change with no row of its own — one the list no longer has —
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
read again; a change that had nothing on screen yet says, in the neutral stand-in rather than as an error (see
"Notices and stand-ins" in `docs/product/project-pane.md`), that the connection was lost and that it is read again
once the connection is back.

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
the sections folded away (see "Folding a section" above); and, grouped by folder, the folders folded away. Reloading
the window, or restarting the application, starts every project on the Uncommitted view and the worktree holding its
folder, with no branches chosen, no change selected and every section and folder unfolded. Selecting one of the
project's sessions sets the worktree and the branches anew, once for that selection; the pane put back in the right
pane while the same session stays selected keeps them as they are (see "Following the selected session" above). Change
lists, branch lists, comparisons and their contents are never stored. Whether the filter field is on screen, and the
text in it, last only while the project's pane stays in the right pane, and the text only until the field is closed (see
"Filtering by file name" above).

**Per project and per view, across restarts**: whether the change list is grouped by folder (see "Flat or grouped by
folder" above). The mode and layouts are kept with the rest of the pane's state (see "What is remembered" in `docs/product/project-pane.md`).
