# Project pane Git change list

The **change list** is the body of the project pane's Git mode (see `docs/product/project-pane-git-mode.md`): the
chosen branch's uncommitted changes and what it has committed since it forked, together, in sections. The Git mode as a
whole — the Changes view, the repository and its worktrees, the branch selector, following the selected session and
what is remembered — is in that doc; a change opened from the list is shown as
`docs/product/project-pane-git-change-viewer.md` describes.

## What is listed

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

## Uncommitted changes

The uncommitted sections are read from the worktree that has the branch checked out, whichever worktree that is.

- **From another worktree than the one holding the project's folder**, a line above the list names that worktree by
  its folder's name, not its path, just under the line saying what the committed part is (see "Committed changes"
  below), or under the other lines when there is none.
- **A worktree in which the project's folder is not a directory** — missing there, or reached only through a symbolic
  link — has a line above the list saying the project's folder is not in the worktree the branch is checked out in,
  just under the line naming the worktree, where there is one.
- **A branch checked out in no worktree** has no uncommitted changes: a line says so, naming the branch, and only what
  it has committed is listed.
- **A branch with no commit yet** — checked out in a worktree whose `HEAD` has none — has a line saying there are no
  commits yet; its uncommitted changes are listed, and nothing is compared (see "Committed changes" below).
- **A worktree found gone** while its changes are listed is no failure of the list and opens no error alert: the
  uncommitted changes count as still being read while the worktrees and the branches are read again, and then follow
  the branch to wherever it is checked out, if anywhere — a branch still there but checked out in no worktree as above,
  a branch gone with its worktree as "A branch that goes away" below says. Another worktree's changes are never shown
  in its place. Any other failure to list them is reported as "What the list says instead" below says.

## Committed changes

What the branch has committed is compared with its **base**:

- **Any branch but the main working copy's** — the one the repository's **main working copy** has checked out — has
  that branch as its base.
- **The main working copy's branch itself** has its **remote branch** as its base, so what is listed is what it has
  committed and not pushed. Its remote branch is, in this order:
  1. the remote-tracking branch it is set to track, when what it tracks is a remote branch the repository has;
  2. otherwise the branch of the same name on one of the repository's configured remotes — `origin`'s when that has
     one, else the first such remote's in byte order of the remotes' names;
  3. otherwise none — a branch never pushed — and **its whole history** is listed (see below).

  The remote branch is as the repository last fetched it — the branch badge's check fetches, for one (see "When a
  project is checked" in `docs/product/project-git-status.md`); the Git mode never does. A branch list read later that shows the
  remote branch at another commit, or gives the branch a remote branch where it had none or another one, has it
  compared again (see "Keeping the list up to date" below). Until a branch list that has the branch is in, and when
  the repository could not be asked which remote branch it has — the lookup past the daemon's limits, or Git unable
  to say which branch the main working copy has checked out — the branch has no Committed section, and nothing says so.

The comparison:

- **Since the fork.** The old side is the commit the branch and its base forked at — their merge base, as
  `git diff base...branch` takes it — and the new side is the commit at the branch's tip, so the list shows only what
  the branch has changed since it forked; what only the base has done since is not in it, so a remote branch that has
  moved on is not shown as changes undone. Where the two have more than one best common ancestor, the one
  `git merge-base` gives is the fork. A branch at the fork itself — at the base's commit, or one the base already
  contains — has nothing committed to list.
- **With its whole history**, for the main working copy's branch with no remote branch: it is compared with the empty
  tree, so every file at its tip is listed as added and, grouped by commit, its commits are listed back to its first
  (see "Grouped by commit" below).
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
  characters, once the comparison is in; **Comparing with** the base while it is made with none on screen. A remote
  branch is named with its remote — `origin/main`, say.
- **No remote branch** — the main working copy's branch listed with its whole history: the line says the branch has
  no remote branch, so all of its commits are listed, in place of the two above, while the comparison is made too.
- **The main working copy is on no branch** — its `HEAD` detached: the branch's commits cannot be compared, and only
  its uncommitted changes are listed.
- Where the branch has no commit yet (see "Uncommitted changes" above), or the main working copy's branch has none,
  nothing is compared.
- Two branches with no commit in common have no fork to compare from and are not compared, rather than one whole branch
  being shown as a change of the other: the comparison fails (see "What the list says instead" below). In a shallow
  clone this is reported instead as a history that may be incomplete: no shared commit was found, and since the
  repository is a shallow clone, the history that would show one may be missing.

## Grouped by commit

**Group by commit**, the third of the Git toggles in the pane's header, lists the Committed section's changes **under
the commits** the branch has made since the fork. It is off to begin with. It is a single icon button whose icon shows
which form the committed changes are in and whose name says what pressing it will do, alternating between "Group by
commit" and "Show commits together" (see "Two-state icon toggles" in `docs/product/labels-and-tooltips.md`). Each
project keeps its own choice across reloads and restarts (see "What is remembered" in
`docs/product/project-pane-git-mode.md`).

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
  folds are (see "What is remembered" in `docs/product/project-pane-git-mode.md`). Folding the Committed section
  hides its commit rows too, which keep the state they were in.
- **Each commit's changes are against its first parent**: a merge commit's are what it took in, and a commit with no
  parent — the first commit of a branch listed with its whole history, or one where the branch merged in an unrelated
  history — is compared with the empty tree. A file several
  commits changed is a row under each of them.
- **A commit that changed nothing in the project's folder** has no row.
- **At most the 100 newest commits** are read. Only when the branch really has more than 100 commits since the fork — or,
  listed with its whole history, in all — does a line under the list say that only that many newest commits are listed (see "What the list says instead" below).
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

## Rows and the keyboard

Each row shows a status letter, the file's name, and then the folder it is in — or, for a rename, where it came from,
and for a rename across the project's boundary, where it came from or went, outside the project. Grouped by folder a
row drops the folder, which its parent row carries instead, and keeps the rest (see "Flat or grouped by folder"
below). After them come the lines the change adds and removes, where they are counted (see "Line counts" below), and
at the row's end is the button of its own actions (see "A row's actions" in `docs/product/project-pane.md`). The
letters:

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
words and what follows the name, its line counts included.

A name, or what follows it, too long for its row fades out, with its full text as its tooltip (see "Names too
long for their space" in `docs/product/labels-and-tooltips.md`), each on the part of the row it belongs to.

- **Up** and **Down** move through the rows — commit rows and folder rows included — and the section headings between
  them (see "Folding a section" below), **Home** and **End** through the rows alone, and typing a name's first letters
  moves to a row. The list, flat or grouped by folder, is one tree of rows to assistive technology.
- A click or Enter opens the change in the file viewer (see "Opening a change" in
  `docs/product/project-pane-git-change-viewer.md`). Nothing in the list is selectable otherwise.
- The change the viewer showed last is the list's **selected change**: its row is tinted, assistive technology hears
  it named as selected, and the list keeps it in view behind the viewer while the viewer moves from change to change.

## Line counts

**A change's row shows how many lines the change adds and removes**, as `+38 -2`: the lines added after a `+`, in
green, then the lines removed after a `-`, in red, each in the current language's number format. They come after the
row's detail, at the row's end before the button of its actions, in every section but Conflicted — Staged, Unstaged,
Untracked and Committed, listed together or under a commit row — flat or grouped by folder.

- **A side that counts none is left out**: an added file shows `+12` alone, a deleted one `-7` alone. A change that
  counts none either way — a rename with nothing else changed, a change of the executable bit alone — shows no counts.
- **Only a change's row has them**: a folder's row, a commit row and a section heading carry no totals.
- **They stay put**: the button of the row's actions keeps its room while it is hidden (see "A row's actions" in
  `docs/product/project-pane.md`), so the counts do not move as it appears. They never shrink; the name and the detail
  fade out first. They read `+` then `-`, left to right, under a right-to-left language too.
- **Assistive technology hears them in the row's name**, after the detail, as the lines added and the lines removed,
  both numbers said even where one is zero. The counts drawn on the row stay out of the accessibility tree.

What is counted is the change as the list has it: a staged change from the commit at `HEAD` to the index, an unstaged
one from the index to the file on disk, a committed one listed together from the fork to the branch's tip, and one
under a commit row from the commit's first parent to the commit. A rename counts only what changed on top of the
rename; an added or untracked file counts every line as added, a deleted file every line as removed. An untracked file
is counted from its content on disk, a last line with no newline at its end counting as a line; it is judged binary by
its content alone, whatever the repository's attributes say of it.

**A row has no counts** — nothing says why — for:

- a binary file, as Git judges it, and an untracked file holding a NUL byte in its first 8,000 bytes;
- a symbolic link, a submodule, an untracked repository of its own, and a type change;
- a path in conflict;
- a change with a side outside the project (a rename across the project's boundary);
- an untracked file larger than 4 MiB, or one that cannot be read; and, once a listing has read 32 MiB of untracked
  files to count them, the untracked files that would take it past that;
- a change that Git, counting within the project's folder, pairs differently from the list — as a rename where the
  list has an addition and a deletion, or the reverse;
- every change of a part of the list — the staged changes, the unstaged ones, the committed ones listed together, or
  the commits' changes — whose count fails, takes more than 5 seconds, or gives more than 2 MiB of output from Git. The
  part is listed all the same, without counts.

The counts are read with the list every time it is listed (see "Keeping the list up to date" below), and never on
their own; a committed change's are at the commits the comparison, or the commit, was made at.

## Folding a section

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
- **Everything starts unfolded.** A section or a commit row folded in one form of the list is folded in the other.
  What is folded is kept per project for as long as the window is open (see "What is remembered" in
  `docs/product/project-pane-git-mode.md`).

## Flat or grouped by folder

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

## Filtering by file name

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

## What the list says instead

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
  branch has more than 100 commits since the fork, or in all when listed with its whole history (see "Grouped by
  commit" above).
- **A failure** of a part — the uncommitted changes could not be listed (a worktree found gone is not one: see
  "Uncommitted changes" above), the branch could not be compared (a branch
  that no longer exists or is broken, or two branches with no commit in common, say), or the commits could not be
  listed — names the reason **in a line above the list**, announced to assistive technology, while the rest of the
  list is shown. When nothing is listed, the first failure, in the list's order, takes the list's place instead, with
  **Try again**, and any other is a line above it. Changes that kept moving while they were listed, after two more
  tries, say so; Refresh tries again. Try again pressed from the keyboard hands keyboard focus to the header's
  Refresh.
- **The repository could not be read** takes the whole view's place, with Try again.

A list on screen stays while it is listed again, and so does a failure, with its Try again and keyboard focus: a
background refresh never replaces either with the loading line. Only Try again, Refresh, choosing another branch and,
for the uncommitted changes, their worktree found gone do.
When the list goes away while one of its rows holds keyboard focus — nothing is left to list, or the list failed —
focus goes to the branch selector, or, when the selector is unavailable because the repository has no branches, leaves
the pane as it does from any pane with nothing to hold it.

## Keeping the list up to date

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

## A branch that goes away

**A chosen branch** that a complete branch list no longer has is chosen no more: the view goes back to the default
branch, with no change selected and the viewer closed (see "The branch selector" in
`docs/product/project-pane-git-mode.md`), so nothing of the gone branch stays on screen, and Refresh does not fail for
it.

Any other branch the comparison is of — the default branch, as read from the worktrees, or the base of whichever
branch is shown — is not dropped: the comparison on screen stays on the commits it was made at. Once a complete branch
list no longer has the branch or its base, a line under the others says that branch no longer exists and that the
comparison still uses the commits it was made at, announced to assistive technology as it appears; a default branch
that is gone is named as no longer existing in the selector too, and nothing compares it again on its own. A branch
list cut short says nothing of a branch it does not have. Refresh then fails, saying there is no such branch. A remote
branch that was the base is not reported this way: a branch list that no longer has it gives the main working copy's
branch another base, or its whole history, which is then compared (see "Committed changes" above). A change opened
from the comparison still on screen reads from its commits for as long as the repository has them (see "Errors" in
`docs/product/project-pane-git-change-viewer.md`).
