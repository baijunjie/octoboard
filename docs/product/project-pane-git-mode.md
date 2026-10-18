# Project pane Git mode

The project pane's **Git mode** reviews one local branch of the project's repository in a single view, **Changes**:
what the branch has not committed yet — what is staged, what is in conflict, what is changed on disk and what Git does
not track, in the worktree that has the branch checked out — and what it has committed since it forked from the branch
the repository's main working copy has checked out, in one Committed section, either all together or under its
commits. Each change opens in the project pane's read-only file viewer (see
`docs/product/project-pane-git-change-viewer.md`). Git is one of the project pane's two modes: opening the pane, its
header, switching modes and the file viewer in general are in `docs/product/project-pane.md` (see "Files and Git" and
"The file viewer" there).

## Reading only

The Git mode reads the repository and never writes to it. It stages, unstages, commits and checks out nothing —
comparing a branch with the main working copy's branch checks neither out — takes no lock, never refreshes the index
(not even for a file whose timestamps moved while its content did not), and fetches nothing from a remote. The one
exception is Git itself before version 2.45: reading an object that a partial clone does not have yet can make such a
Git fetch it.

The project's **branch badge** in the sidebar is separate (see `docs/product/project-git-status.md`): its check fetches
the remote's refs and, with Automatically pull repositories on or on Pull current branch, may fast-forward the branch.
Pushing the current branch is not that check and not this mode (see "Pushing the current branch" in
`docs/product/project-git-actions.md`). The Git mode does none of these and refreshes none of them. A change of the
reported branch or detached state refreshes the visible mode
(see "Files and Git" in `docs/product/project-pane.md`); switching from the project action menu remains separate
from this read-only mode.

## The Changes view

The project pane's single header row holds two peer tabs, **Files** and **Changes** (see "Files and Git" in
`docs/product/project-pane.md`); choosing Changes shows the Git mode. The tabs stay available while the repository is
being read, cannot be read, or is not a repository. Three icon buttons stay before Refresh, at the header's trailing
side, in every state of the Git mode, in this order: the one that shows and hides the file-name filter field, **Group
by folder**, and **Group by commit** (see "Filtering by file name", "Flat or grouped by folder" and "Grouped by commit" below).

At the top of the view is the **branch selector** (see "The branch selector" below); under it, lines saying what the
list is made of and what could not be read; then the **change list**, the branch's uncommitted and committed changes
together (see "The change list" below). The view reads the repository only while it is on screen (see "Keeping the
list up to date" below).

## The repository and its worktrees

The repository is found by looking for a `.git` entry in the project's folder and then in each folder above it,
stopping below the home directory: the home directory is taken for a project's repository only when it is the
project's own folder.

- A project in no repository says it is not in a Git repository.
- A repository Git cannot read — `git` not installed or older than 2.36, a repository belonging to another user, a
  damaged one — says Git review is unavailable, with Git's own message.
- Either way the Files mode still works.

The repository's worktrees are every one there is now — not a bare main repository, and not a worktree Git reports as
prunable. Two of them matter to the view:

- **The worktree holding the project's folder**: the checkout the project's folder is in, which may be a linked
  worktree. The branch checked out there is the view's default branch (see "The branch selector" below).
- **The worktree that has the chosen branch checked out**, which is where its uncommitted changes are read from (see
  "Uncommitted changes" below). Showing them checks nothing out, moves no session and leaves the Files mode on the
  project's own folder.

The worktrees are read when the Git mode is first shown after the project pane is put in the right pane, on Refresh,
after the connection to the daemon comes back, when the worktree the changes are read from is found gone, when the
project's reported branch or detached state changes, and when one of the project's sessions is selected (see
"Following the selected session" below). A worktree added, or a branch checked out elsewhere, in the meantime is taken
into account then.

A project associated with a subdirectory of its repository keeps that scope in every worktree and in what is
committed: only changes touching the project's folder are listed (see "What is listed" below).

## The branch selector

The **Branch** selector chooses whose changes are shown. Only a branch is chosen; what it is compared with follows
from the repository (see "Committed changes" below).

- It offers every local branch of the repository — checked out in some worktree or in none — in byte order of their
  names, each with the first seven characters of the commit at its tip. At most 10,000 branches, or as many as 2 MiB of
  names hold, are offered, the first in that order; nothing says the list was cut.
- A branch that has a responsible session, the session working on it now, carries a small ring between its name and
  its commit, **in the list only and never on the selector's chosen branch**; most branches carry none, which is
  ordinary (see "The responsible-session badge" in `docs/product/branch-responsible-session.md`).
- **The default branch** is the one checked out in the worktree holding the project's folder, and the selector shows
  it as chosen. While no branch has been chosen, the view follows that default as it is now: a checkout of another
  branch there, once the worktrees are read again, shows that branch's changes. **A branch chosen in the selector
  stands** — for as long as the window is open — whatever is checked out where. Selecting one of the project's sessions
  can set it (see "Following the selected session" below).
- **No branch** — the worktree holding the project's folder has a detached `HEAD`, and none has been chosen — leaves
  the selector saying to choose a branch, and the view says no branch is chosen and to choose one to see its
  uncommitted changes and what it has committed since it forked. A repository with no branches at all says it has none
  yet, the selector having nothing to offer.
- **Choosing another branch is another list**: nothing of the previous branch's changes stays on screen, its selected
  change is cleared and the viewer closes.
- A chosen branch's name too long for its space fades out (see "Names too long for their space" in
  `docs/product/labels-and-tooltips.md`), in the selector and in its list.
- **A chosen branch that a complete branch list no longer has** stays chosen, its name marked as no longer existing,
  until another one is chosen (see "A branch that goes away" below). A branch checked out with no commit yet, which no
  branch list has, is not taken for gone.
- A broken branch — its reference names an object the repository does not have, or one that is not a commit — is
  offered with the id its reference names, and comparing it fails (see "What the list says instead" below).

The branches are read while the Git mode is on screen, once the repository is found: when the Git mode is first shown
after the project pane is put in the right pane, and then the way the uncommitted changes are listed again (see
"Keeping the list up to date" below) — every 10 seconds while it is on screen and the window is visible, when it comes
back more than 5 seconds after the last reading, on Refresh, and after the connection to the daemon comes back; and, as
the worktrees are, when one of the project's sessions is selected. A branch created, moved or deleted in the meantime
shows in the selector then.

When the branches cannot be read, the view says so with the reason: in place of the change list, with **Try again**,
while no branch is shown and no list of branches has been read yet; otherwise in a line under the selector, announced
to assistive technology, the branches already offered staying on offer, until the branches are read again.

## The change list

### What is listed

The change list shows the branch's changes in sections, in this order, each headed with its name and how many changes
it holds; a section with none is left out. First what the branch has not committed, from the worktree that has it
checked out (see "Uncommitted changes" below):

- **Conflicted** — paths in conflict. It is first, being what has to be dealt with before a commit can be made.
- **Staged** — the commit at `HEAD` against the index. Before the first commit every staged file is an addition.
- **Unstaged** — the index against the files on disk.
- **Untracked** — files on disk Git neither tracks nor ignores, each listed by itself rather than as its folder.

Then what it has committed since it forked (see "Committed changes" below):

- **Committed** — everything the branch has changed from the fork to its tip; or, with **Group by commit** on, the
  changes of each commit the branch has made since the fork, under a row for that commit (see "Grouped by commit"
  below).

A file staged and changed again since is in both Staged and Unstaged: two changes, each against its own baseline. A
file can likewise be both uncommitted and committed, a row in each section.

Not listed:

- Files the repository ignores, which the Files mode still shows.
- A submodule's own uncommitted work, which belongs to that repository. A submodule is listed only when it is checked
  out at another commit than the index records, or, in what is committed, recorded at different commits on the two
  sides. An untracked folder that is a repository of its own is listed as one entry. **A submodule moved to another
  path** is listed as one renamed change only when both sides record the same commit — Git pairs two submodules by
  exact match and never by similarity — and as a deletion at the old path plus an addition at the new one as soon as
  the recorded commits differ, whether or not the two paths nest.

For a project that is a subdirectory of its repository, a change is listed when its old or its new path is inside the
project's folder (the folder itself is not inside it).

- A rename into the project from elsewhere in the repository, or out of it, names the other side by its path from the
  repository's root, as being outside the project. Among the uncommitted changes only a staged rename crosses the
  boundary that way: a file moved on disk across it is, until staged, a deletion and an untracked file, and only the
  half inside the project is listed.
- In a worktree where the project's folder is not on disk, its changes are still listed: a deletion is read from the
  index and `HEAD`, never from the disk.

Within a section, changes are in the order the file tree would show their paths (see "Order" in
`docs/product/project-pane.md`), a deleted file by the path it had.

### Uncommitted changes

The uncommitted sections are read from the worktree that has the branch checked out, whichever worktree that is.

- **From another worktree than the one holding the project's folder**, a line above the list names that worktree's
  path, `~` standing for the home directory.
- **A worktree in which the project's folder is not a directory** — missing there, or reached only through a symbolic
  link — has a line above the list saying the project's folder is not in the worktree the branch is checked out in.
- **A branch checked out in no worktree** has no uncommitted changes: a line says so, naming the branch, and only what
  it has committed is listed.
- **A branch with no commit yet** — checked out in a worktree whose `HEAD` has none — has a line saying there are no
  commits yet; its uncommitted changes are listed, and nothing is compared (see "Committed changes" below).
- **A worktree found gone** has the worktrees read again; the uncommitted sections then follow the branch to wherever
  it is checked out, if anywhere. Another worktree's changes are never shown in its place.

### Committed changes

What the branch has committed is compared with the branch the repository's **main working copy** has checked out,
its **base**:

- **Since the fork.** The old side is the commit the two branches forked at — their merge base, as
  `git diff base...branch` takes it — and the new side is the commit at the branch's tip, so the list shows only what
  the branch has changed since it forked; what only the base has done since is not in it. Where the two have more than
  one best common ancestor, the one `git merge-base` gives is the fork. A branch at the fork itself — at the base's
  commit, or one the base already contains — has nothing committed to list.
- **At the commits the branches were at when it was made.** The fork is found once, together with the two tips, and the
  committed changes listed, and every one opened from them, are between those commits until the comparison is made
  again (see "Keeping the list up to date" below).
- **Nothing is read from the disk or from any worktree's index**, so a committed change is listed and opened whether or
  not it is in the project's folder on disk or in any worktree.
- **A subdirectory project** is compared within its scope as "What is listed" above says. When comparing the whole
  repository's trees gives more than 8 MiB of output from Git — far-apart branches of a large repository — a
  subdirectory project is compared within its own folder instead, where a rename across the boundary is listed as the
  addition or the deletion of its half inside. A project at the repository's root whose comparison is that large cannot
  be compared, and says so.

A line above the list says what the committed part is:

- **Forked from** the base **at** the fork's commit, **now at** the branch's tip, each commit by its first seven
  characters, once the comparison is in; **Comparing with** the base while it is made with none on screen.
- **The branch is the base itself** — the one the main working copy has checked out: it has no commits of its own to
  compare, so only its uncommitted changes are listed.
- **The main working copy is on no branch** — its `HEAD` detached: the branch's commits cannot be compared, and only
  its uncommitted changes are listed.
- Where the branch has no commit yet (see "Uncommitted changes" above), or the main working copy's branch has none,
  nothing is compared.
- Two branches with no commit in common have no fork to compare from and are not compared, rather than one whole branch
  being shown as a change of the other: the comparison fails (see "What the list says instead" below). In a shallow
  clone this is reported instead as a history that may be incomplete: no shared commit was found, and since the
  repository is a shallow clone, the history that would show one may be missing.

### Grouped by commit

**Group by commit**, the third of the Git toggles in the pane's header, lists the Committed section's changes **under
the commits** the branch has made since the fork. It is off to begin with. It is a single icon button whose icon shows
which form the committed changes are in and whose name says what pressing it will do, alternating between "Group by
commit" and "Show commits together" (see "Two-state icon toggles" in `docs/product/labels-and-tooltips.md`). Each
project keeps its own choice across reloads and restarts (see "What is remembered" below).

- **The Committed section stays one section**, with one heading band whose count is of all the committed changes it
  shows. Inside it is **one commit row per commit**, newest first, in the order `git log` gives them, and each commit's
  changes are one level in, under its row; grouped by folder, the folders are inside each commit (see "Flat or grouped
  by folder" below).
- **A commit row** has an expand arrow, as a folder's row does, then the first seven characters of the commit's id in a
  monospace chip, its subject — which fades out where it is too long, with its whole text as its tooltip, the count
  staying in view — and how many changes it holds. A subject is cut to its first 200 characters. Assistive technology
  hears the row named by the same three, in that order — the id, the subject and the count — and is told whether it is
  unfolded. A commit row has no button of its own actions, and typing the first characters of its id moves to it.
- **Commit rows scroll with the rows**: only the section's heading band stays at the top of the list (see "Folding a
  section" below).
- **A commit row folds and unfolds its changes**: its arrow or a click on it does, and from the keyboard **Left** folds
  it, **Right** unfolds it (the other way round under a right-to-left language) and **Enter** does either; **Space**
  does nothing, as on a folder's row. A folded commit keeps its row and its count. Its fold stays with the commit
  whatever else the branch commits meanwhile, is apart from the Committed section's own fold, and is kept as the other
  folds are (see "What is remembered" below). Folding the Committed section hides its commit rows too, which keep the
  state they were in.
- **Each commit's changes are against its first parent**: a merge commit's are what it took in, and a commit with no
  parent — met only where the branch merged in an unrelated history — is compared with the empty tree. A file several
  commits changed is a row under each of them.
- **A commit that changed nothing in the project's folder** has no row.
- **At most the 100 newest commits** are read. Only when the branch really has more than 100 commits since the fork does
  a line under the list say that only that many newest commits are listed (see "What the list says instead" below).
  The commits' changes together are held to one change list's limit — 10,000 changes or 2 MiB of paths: past it, the
  last commit listed holds only the changes read before the limit, older commits are left out, a commit none of whose
  changes fit is not listed at all, and the line saying not every change is shown appears.
- A subdirectory project compared within its own folder (see "Committed changes" above) lists only the commits that
  touch that folder, and the 100 are counted among those.
- The commits are read only while Group by commit is on, from the same fork and tip as the comparison on screen, and
  again whenever the comparison is made again at other commits; the commit rows on screen stay until the new ones are
  in. Like the comparison, nothing is read from any worktree.
- Opening a change under a commit row shows that commit against its parent (see "Opening a change" in
  `docs/product/project-pane-git-change-viewer.md`).

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

A committed change is only ever A, M, D, R or T.

**The letter says in a tooltip what it stands for**: resting the mouse on it shows the status named in words, in the
same capitalised wording the file viewer's header carries as an opened change's tag (see "Opening a change" in
`docs/product/project-pane-git-change-viewer.md`). The letter is the only part of the row that opens it, and it opens
on the mouse alone, after the usual wait: the letter is no tab stop and adds none to the row, so the tooltip never
shows on keyboard focus (see "Tooltips on indicators" in `docs/product/labels-and-tooltips.md`). The letter itself
stays out of the accessibility tree, which costs nothing: assistive technology hears a row as its name, its status in
words and what follows the name.

A name, or what follows it, too long for its row fades out, with its full text as its tooltip (see "Names too
long for their space" in `docs/product/labels-and-tooltips.md`), each on the part of the row it belongs to.

- **Up** and **Down** move through the rows — commit rows and folder rows included — and the section headings between
  them (see "Folding a section" below), **Home** and **End** through the rows alone, and typing a name's first letters
  moves to a row. The list, flat or grouped by folder, is one tree of rows to assistive technology.
- A click or Enter opens the change in the file viewer (see "Opening a change" in
  `docs/product/project-pane-git-change-viewer.md`). Nothing in the list is selectable otherwise.
- The change the viewer showed last is the list's **selected change**: its row is tinted, assistive technology hears
  it named as selected, and the list keeps it in view behind the viewer while the viewer moves from change to change.

### Folding a section

**Every section heading is a control**: pressing anywhere on its band folds the section away, and pressing it again
unfolds it. That holds for every section — Conflicted, Staged, Unstaged, Untracked and Committed — and in both forms
of the list, flat or grouped by folder. A commit row inside Committed folds as a row, not as a section (see "Grouped
by commit" above).

**The heading stays at the top of the scrolling change list while its rows move underneath.** As the next section's
heading reaches the top, it pushes the current heading upward out of view and takes its place. Each heading is
**30 px** high, with an opaque background, and the first begins at the list's top edge without a padding gap below
the branch selector and the lines under it. The edge fade begins below the heading, and a row scrolled into view for
keyboard focus or as the viewer's selected change stays clear of both (see "The edge that hides content fades" and
"Scrolling a surface from the keyboard" in `docs/product/scrolling-surfaces.md`).

- **A folded section keeps its heading and its count** — of the changes it holds, those the filter leaves (see
  "Filtering by file name" below) — and shows none of its rows. Those changes have no row at all, so the viewer moves
  past them (see "Moving between changes" in `docs/product/project-pane-git-change-viewer.md`), as it moves past the
  changes under a folded folder.
- **The commit rows and folders inside a folded section keep the state they were in** and come back as they were
  when the section is unfolded (see "Grouped by commit" above and "Flat or grouped by folder" below).
- The heading shows **a chevron**, pointing right while the section is folded and down while it is not; here it is
  not a control of its own, the whole band being one. Assistive technology is told whether the section is unfolded.
  The band **carries no tooltip** of its own, saying in text what it holds, and answers a hover with nothing: the
  chevron is what marks it as a control.
- **The band is no tab stop of its own.** The change list is one tab stop, as "Rows and the keyboard" above says: a
  heading is reached by arrowing onto it with Up or Down, and **Enter** or **Space** on it folds and unfolds the
  section. Home and End pass the headings by.
- Pressing a heading with the pointer leaves keyboard focus where it was. **Folding a section that holds the row with
  keyboard focus puts focus on the heading**, which stands for the rows it has taken away.
- **Everything starts unfolded.** A section or a commit row folded in one form of the list is folded in the other. What is folded is
  kept per project for as long as the window is open (see "What is remembered" below).

### Flat or grouped by folder

**Group by folder**, the second of the Git toggles in the pane's header, switches the change list between one row per
change and the changes grouped under the folders they are in. It is a single icon button whose icon shows which form
the list is in and whose name says what pressing it will do, so the name alternates between "Group by folder" and
"Show as flat list" (see "Two-state icon toggles" in `docs/product/labels-and-tooltips.md`). Each project keeps the
Changes tab's layout across reloads and restarts (the Files tab has none: see "Always a folder tree" in
`docs/product/project-pane.md`). Without one saved for the Changes tab, it takes the layout an earlier version of
Octoboard saved for the project's uncommitted changes, or else a grouped preference retained from before per-tab
choices were available, and otherwise starts flat.

Grouped by folder, the list keeps its sections and their counts, and within each section the changes sit under their
folders — with Group by commit on, the committed changes under the folders inside each commit's row:

- Within a folder, folders come first and then changes, each in the order the flat list has them (see "What is
  listed" above).
- A chain of folders that each hold nothing but the next is one row, named by the whole chain — `tools/release`.
- A folder's row has an expand arrow before its folder icon, open or closed with the folder, and its name. The arrow
  points right while collapsed (left in a right-to-left language), and down while expanded. A change's row is as it is
  in the flat list, less the folder its parent row now carries.
- Folders start open, and which ones the user folded away is kept per project for as long as the window is open: a
  reload or a restart opens them all again. A fold survives switching to Files and back, and belongs to the section,
  or the commit, it was made in, so the same folder under another section or another commit stays open.

The rows take the keyboard as the Files mode's tree does (see "Rows and the keyboard" in
`docs/product/project-pane.md`): Up and Down move through them and the section headings between them, Home and End
through the rows alone, typing a name's first letters moves to one, Right and Left open and close a folder (the other
way round under a right-to-left language), and Enter opens a change or opens and closes a folder; Space does nothing
on a folder's row. A folder's row tells
assistive technology whether it is open, and its arrow button is named Expand or Collapse (see "Tooltips on icon-only
controls" in `docs/product/labels-and-tooltips.md`).

A change under a folded folder has no row, and it is the rows that the viewer moves through (see "Moving between
changes" in `docs/product/project-pane-git-change-viewer.md`).

### Filtering by file name

The **filter field** narrows the change list to the changes whose file name contains its text, in every section and
both forms of the list, flat or grouped by folder. It is not on screen to begin with: the **first of the Git toggles in
the pane's header** shows and hides it, its name saying what pressing it will do — "Filter by file name" while the
field is hidden, "Hide the filter" while it is shown (see "Two-state icon toggles" in
`docs/product/labels-and-tooltips.md`). Assistive technology is also told whether the field is on screen.

- **Opening it from the toggle puts the field directly below the pane's header**, above the branch selector, and
  **keyboard focus in the field**. The field is there in every state of the view, over the lines that say the changes
  are being read, that reading them failed, that there is nothing to list, or that a branch has still to be chosen (see
  "What the list says instead" below), so the toggle always has a field to open.
- **An empty field takes itself away once keyboard focus leaves it** — on Tab out of it, or on a press that puts focus
  elsewhere — so the only field on screen is one whose effect can be seen; a field holding nothing but whitespace
  counts as empty. The window itself losing focus is not focus leaving the field: the field stays for the user coming
  back to it. **A field with text typed in it stays**, so no list is left narrowed by a field that has gone.
- **Closing it from the toggle drops the text with the field**, and hands keyboard focus back to the toggle when the
  field was holding it. Opening the field again starts from empty.
- **Only the file's name is matched** — the last part of its path, as its row shows it — ignoring case and any
  whitespace around the text; the folders it is in are not, so `src` does not keep every file under `src/`. A rename
  is matched by its new name. An empty field, or one holding only whitespace, shows every change.
- **It narrows what is shown, not what is listed.** The sections' and the commit rows' counts are of the rows shown,
  a section or a commit with no match is left out, and grouped by folder only the folders leading to a match remain. What the list holds, and when it
  is listed again, is unchanged; a list listed again is narrowed by the same text.
- **When nothing matches**, the list's place says there are no matching changes (announced to assistive technology
  as it appears), and the field stays, so the text can be changed or cleared. Clearing it brings the whole list back.
- The field has a **clear** button, and **Escape** in it clears it; the field itself stays, keyboard focus not having
  left it. In a drawer or a floating pane, Escape in a field with text only clears it and leaves the pane; Escape in
  the then empty field closes the pane, as it does from anywhere else in it.
- The field is apart from the list: typing in it never moves through the list's rows, as typing a name's first
  letters in the list does. While an input method is composing text in it, its keys, Escape included, belong to the
  composition, and do not clear the field.

The Changes tab keeps its own keyword, field-open state, layout and folded sections, commits and folders
independently of Files. Filter lifetime and tab-switch focus behavior are in "Filtering by file name" in
`docs/product/project-pane.md`. Choosing another branch keeps the keyword. The viewer moves through the rows the filter
leaves (see "Moving between changes" in `docs/product/project-pane-git-change-viewer.md`).

### What the list says instead

- **Loading** while the worktrees are read, and while nothing is listed yet and some part of the list is still being
  read, announced to assistive technology. Once something is listed, the part still being read — the comparison, or,
  grouped by commit, the commits — is said in a line above the list and announced instead.
- **No changes**, when the branch has nothing uncommitted and nothing committed to list; the lines above the list still
  say what it is made of.
- **Not every change is shown**, under the list, when a part of it was cut short: at most 10,000 changes, or 2 MiB of
  paths, are listed in each of the uncommitted changes, the committed changes, and the commits' changes together —
  those Git reported first, not a chosen subset. A worktree whose whole `git status` output is more than 8 MiB has its
  uncommitted changes not listed at all, and says so.
- **Only the newest commits are listed**, with how many, in a line under the list, grouped by commit, only when the
  branch has more than 100 commits since the fork (see "Grouped by commit" above).
- **A failure** of a part — the uncommitted changes could not be listed, the branch could not be compared (a branch
  that no longer exists or is broken, or two branches with no commit in common, say), or the commits could not be
  listed — names the reason **in a line above the list**, announced to assistive technology, while the rest of the
  list is shown. When nothing is listed, the first failure, in the list's order, takes the list's place instead, with
  **Try again**, and any other is a line above it. Changes that kept moving while they were listed, after two more
  tries, say so; Refresh tries again. Try again pressed from the keyboard hands keyboard focus to the header's
  Refresh.
- **The repository could not be read** takes the whole view's place, with Try again.

A list on screen stays while it is listed again, and so does a failure, with its Try again and keyboard focus: a
background refresh never replaces either with the loading line. Only Try again, Refresh and choosing another branch do.
When the list goes away while one of its rows holds keyboard focus — nothing is left to list, or the list failed —
focus goes to the branch selector, or, when the selector is unavailable because the repository has no branches, leaves
the pane as it does from any pane with nothing to hold it.

### Keeping the list up to date

Nothing tells Octoboard when a repository changes, so the uncommitted changes are listed again:

- once the Git mode is shown and the repository and the worktree the branch is checked out in are known;
- **every 10 seconds** while the Git mode is on screen — the project pane docked, in an open drawer, or floating in —
  and the window is visible, skipping a turn while a listing is still out;
- when the window comes back to the front or becomes visible again, with the Git mode on screen, or when it comes back
  on screen — a hidden pane shown, the Changes tab chosen — either one more than 5 seconds after the last listing;
- on the header's **Refresh**, which reads the worktrees, the branches, the comparison and, grouped by commit, the
  commits again too;
- after the connection to the daemon comes back, while the Git mode is on screen.

**The comparison follows the branches.** It is made once the branch and its base are known with the Git mode on
screen, on Refresh and on Try again — and, on its own, **once a branch list read after the comparison came in shows
the branch or its base at another commit**: what the branch committed meanwhile then moves from its uncommitted
sections to its committed ones. The comparison on screen stays until the new one is in. A comparison already being
made again is not started over by later branch lists that show the branches at the same commits. A comparison that
failed is made again on its own only when a later branch list shows the branches at other commits than the last ones
it was seen or made at; one that failed before any comparison of the same two branches was on screen waits for
Refresh or Try again. Switching modes, or the connection coming back, does not make it again, except for a comparison
whose reply was lost with the connection, which is asked for again once it is back.

While the pane is hidden, or shows the Files mode, nothing is listed or compared on its own. When the connection is
lost, the list on screen stays until it is back.

### A branch that goes away

The comparison on screen stays on the commits it was made at. Once a complete branch list no longer has the branch or
its base, a line under the others says that branch no longer exists and that the comparison still uses the commits it
was made at, announced to assistive technology as it appears; a chosen branch that is gone is named as no longer
existing in the selector too, and nothing compares it again on its own. A branch list cut short says nothing of a
branch it does not have. Refresh then fails, saying there is no such branch. A change opened from the comparison
still on screen reads from its commits for as long as the repository has them (see "Errors" in
`docs/product/project-pane-git-change-viewer.md`).

## Following the selected session

Selecting one of the project's sessions in the sidebar sets the branch selector to **the session's branch**: of the
branches whose responsible session is that session (see "The responsible session" in
`docs/product/branch-responsible-session.md`), the one it became responsible for last. The changes shown are then that
branch's: its uncommitted changes from the worktree that has it checked out, and what it has committed since it forked
from the main working copy's branch.

- **A session with no branch of its own** sets the selector back to the default (see "The branch selector" above),
  which then follows what is checked out where the project's folder is. A branch list that could not be read counts as
  naming no branch for the session.
- A branch set this way is as one chosen by the user: it stands, and moving it clears the selected change and closes
  the viewer.

When the selector is set:

- **Once for each selection of one of the project's sessions.** Every change of the selected session is a new
  selection, whether or not the project pane was in the right pane when it was made: selecting a session, then any
  other session — of this project or another, or a console session — and then the first again sets it again. The
  project pane put back in the right pane, or the Git mode shown again, while the same session stays selected does not
  set it again: what the selector holds stands, the user's choice included (see "What is remembered" below).
- **Only from what is read after the selection.** The worktrees and branches the pane held from before the selection
  are asked for again — at once while the Git mode is on screen, otherwise when it next comes on screen — and the
  selector is set once both have been read since, never from the earlier lists. Until then it holds what it held.
- **A session's branch that no listed worktree has checked out** is set at once, and the worktrees are asked for once
  more, so a worktree made for it after the list was read is found: its uncommitted changes are then read from there.
- **A choice made in the selector stands while the same session stays selected**: nothing sets the selector again
  until the next selection.
- **A session's branch that turns up later** — in a branch list read after the selection, its responsible session
  having been recorded only after the branch itself appeared, say — is followed then, once, provided the user has
  chosen no branch since the selection. Once a session's branch has been followed, later branch lists leave the
  selector alone, even one that names a newer branch for the session.
- **Selecting any other session** — another project's, or a console session — or none leaves the selector as it is.

## What is remembered

**Per project, for as long as the window is open**: the chosen branch, the selected change, the sections folded away
(see "Folding a section" above), the commit rows folded away (see "Grouped by commit" above) and, grouped by folder,
the folders folded away. Reloading the window, or restarting the application, starts every project on its default
branch, with no change selected and every section, commit and folder unfolded. Selecting one of the project's sessions sets the branch anew, once for that selection; the pane put back in
the right pane while the same session stays selected keeps it as it is (see "Following the selected session" above).
Change lists, branch lists, comparisons, commit lists and their contents are never stored. Whether the filter field is
on screen, and the text in it, last only while the project's pane stays in the right pane, and the text only until the
field is closed (see "Filtering by file name" above).

**Per project, across restarts**: whether the Changes tab's list is grouped by folder (see "Flat or grouped by folder"
above), and whether its committed changes are grouped by commit (see "Grouped by commit" above). They are kept with the
rest of the pane's state, as the mode is (see "What is remembered" in `docs/product/project-pane.md`).
