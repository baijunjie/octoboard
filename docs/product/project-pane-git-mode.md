# Project pane Git mode

The project pane's **Git mode** reviews one local branch of the project's repository in a single view, **Changes**:
what the branch has not committed yet — what is staged, what is in conflict, what is changed on disk and what Git does
not track, in the worktree that has the branch checked out — and what it has committed since it forked from the branch
the repository's main working copy has checked out (for that branch itself, what it has not pushed to its remote
branch), in one Committed section, either all together or under its commits. The **change list** that shows them is
in `docs/product/project-pane-git-change-list.md`. Each change opens in the project pane's read-only file viewer (see
`docs/product/project-pane-git-change-viewer.md`). Git is one of the project pane's two modes: opening the pane, its
header and switching modes are in `docs/product/project-pane.md` (see "Files and Git" there), and the file viewer in
general in `docs/product/file-viewer.md`.

Failures to read the repository, load branches, list changes or compare commits are shown in the pane alone: one
that leaves nothing to show takes the place of what failed as an empty-state failure, and one of a part while the rest
is shown is a mark on that part, whose popover holds its reason and **Try again** — except the uncommitted changes',
which span several sections of the change list and so are a compact alert at its top; none of them opens an error
alert (see "Failures shown on the screen" and "Failures, warnings and notes marked at their point" in
`docs/product/error-alerts.md`). A worktree found gone while its changes are listed is not such a failure and is not
shown as one (see "Uncommitted changes" in `docs/product/project-pane-git-change-list.md`).

## Reading only

The Git mode reads the repository and never writes to it. It stages, unstages, commits and checks out nothing —
comparing a branch with its base checks neither out — takes no lock, never refreshes the index
(not even for a file whose timestamps moved while its content did not), and fetches nothing from a remote: a remote
branch it compares with is as the repository last fetched it (see "Committed changes" in
`docs/product/project-pane-git-change-list.md`). The one
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
by folder**, and **Group by commit** (see "Filtering by file name", "Flat or grouped by folder" and "Grouped by
commit" in `docs/product/project-pane-git-change-list.md`).

At the top of the view, the **branch selector** (see "The branch selector" below), the **Worktree line** when shown,
then the lines saying what the list is made of and what could not be read share one area, with no divider between them.
The Worktree line names the worktree the uncommitted changes are read from, with a button copying its path; it is shown
when that is another worktree than the one holding the project's folder, and for that one too while its uncommitted
changes are cut short, ending then on the warning mark about them (see "Uncommitted changes" and "What the list says
instead" in `docs/product/project-pane-git-change-list.md`). Below is the **change list**, the branch's uncommitted and
committed changes together (see `docs/product/project-pane-git-change-list.md`). The view reads the repository only
while it is on screen (see "Keeping the list up to date" in that doc).

## The repository and its worktrees

The repository is found by looking for a `.git` entry in the project's folder and then in each folder above it,
stopping below the home directory: the home directory is taken for a project's repository only when it is the
project's own folder.

- A project in no repository says it is not in a Git repository.
- A repository Git cannot read — `git` not installed or older than 2.36, a repository belonging to another user, a
  damaged one — shows an empty-state failure headed that Git review is unavailable, with Git's own message behind
  Details and no Try again.
- Either way the Files mode still works.

The repository's worktrees are every one there is now — not a bare main repository, and not a worktree Git reports as
prunable. Two of them matter to the view:

- **The worktree holding the project's folder**: the checkout the project's folder is in, which may be a linked
  worktree. The branch checked out there is the view's default branch (see "The branch selector" below).
- **The worktree that has the chosen branch checked out**, which is where its uncommitted changes are read from (see
  "Uncommitted changes" in `docs/product/project-pane-git-change-list.md`). Showing them checks nothing out, moves no session and leaves the Files mode on the
  project's own folder.

The worktrees are read when the Git mode is first shown after the project pane is put in the right pane, on Refresh,
after the connection to the daemon comes back, when the worktree the changes are read from is found gone, when the
project's reported branch or detached state changes, and when one of the project's sessions is selected (see
"Following the selected session" below). A worktree added, or a branch checked out elsewhere, in the meantime is taken
into account then.

A project associated with a subdirectory of its repository keeps that scope in every worktree and in what is
committed: only changes touching the project's folder are listed (see "What is listed" in
`docs/product/project-pane-git-change-list.md`).

## The branch selector

The **Branch** selector chooses whose changes are shown. Only a branch is chosen; what it is compared with follows
from the repository (see "Committed changes" in `docs/product/project-pane-git-change-list.md`).

- It offers every local branch of the repository — checked out in some worktree or in none — in byte order of their
  names, each with the first seven characters of the commit at its tip. At most 10,000 branches, or as many as 2 MiB of
  names hold, are offered, the first in that order; nothing says the list was cut.
- A branch that has a responsible session, the session working on it now, carries a small ring between its name and
  its commit, **in the list only and never on the selector's chosen branch**; most branches carry none, which is
  ordinary (see "The responsible-session badge" in `docs/product/branch-responsible-session.md`).
- **The default branch** is the one checked out in the worktree holding the project's folder, and the selector shows
  it as chosen. While no branch has been chosen, the view follows that default as it is now: a checkout of another
  branch there, once the worktrees are read again, shows that branch's changes. **A branch chosen in the selector
  stands** — for as long as the window is open, whatever is checked out where — until the repository no longer has it
  (see below). Selecting one of the project's sessions can set it (see "Following the selected session" below).
- **No branch** — the worktree holding the project's folder has a detached `HEAD`, and none has been chosen — leaves
  the selector saying to choose a branch, and the view says no branch is chosen and to choose one to see its
  uncommitted changes and what it has committed since it forked. A repository with no branches at all says it has none
  yet, the selector having nothing to offer.
- **Choosing another branch is another list**: nothing of the previous branch's changes stays on screen, its selected
  change is cleared and the viewer closes.
- A chosen branch's name too long for its space fades out (see "Names too long for their space" in
  `docs/product/labels-and-tooltips.md`), in the selector and in its list.
- **A chosen branch that a complete branch list no longer has** — merged and deleted, say — is chosen no more,
  whether it was chosen in the selector or set by following a session: the view goes back to the default branch,
  following it as when no branch has been chosen, its selected change is cleared and the viewer closes, before
  anything of the gone branch is shown. A branch checked out with no commit yet, which no branch list has, is not
  taken for gone. A branch whose worktree alone is removed is still there and stays chosen, checked out in no worktree
  (see "Uncommitted changes" in `docs/product/project-pane-git-change-list.md`).
- **The default branch, as read from the worktrees, that a complete branch list does not have** — a list newer than
  the worktrees it was read from — keeps its branch name in the selector, followed by a separate status badge saying
  it no longer exists. When the name is cut, its tooltip gives that status with the complete branch name set apart
  as a code chip (see "A branch that goes away" in `docs/product/project-pane-git-change-list.md`).
- A broken branch — its reference names an object the repository does not have, or one that is not a commit — is
  offered with the id its reference names, and comparing it fails (see "What the list says instead" in
  `docs/product/project-pane-git-change-list.md`).

The branches are read while the Git mode is on screen, once the repository is found: when the Git mode is first shown
after the project pane is put in the right pane, and then the way the uncommitted changes are listed again (see
"Keeping the list up to date" in `docs/product/project-pane-git-change-list.md`) — every 10 seconds while it is on screen and the window is visible, when it comes
back more than 5 seconds after the last reading, on Refresh, and after the connection to the daemon comes back; and, as
the worktrees are, when one of the project's sessions is selected and when the worktree the changes are read from is
found gone. A branch created, moved or deleted in the meantime
shows in the selector then.

When the branches cannot be read, the view says so: in place of the change list, as an empty-state failure with
**Try again**, while no branch is shown and no list of branches has been read yet; otherwise as a red mark beside the
selector, named as the branches that could not be listed, whose popover holds the reason and **Try again**, the
branches already offered staying on offer, until the branches are read again. Either is announced to assistive
technology once when it appears after the view was drawn.

## Following the selected session

Selecting one of the project's sessions in the sidebar sets the branch selector to **the session's branch**: of the
branches whose responsible session is that session (see "The responsible session" in
`docs/product/branch-responsible-session.md`), the one it became responsible for last. The changes shown are then that
branch's: its uncommitted changes from the worktree that has it checked out, and what it has committed (see
"Committed changes" in `docs/product/project-pane-git-change-list.md`).

- **A session with no branch of its own** sets the selector back to the default (see "The branch selector" above),
  which then follows what is checked out where the project's folder is. A branch list that could not be read counts as
  naming no branch for the session.
- A branch set this way is as one chosen by the user: it stands until the repository no longer has it (see "The
  branch selector" above), and moving it clears the selected change and closes the viewer.

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

**Per project, for as long as the window is open**: the chosen branch (dropped once the repository no longer has it:
see "The branch selector" above), the selected change, the sections folded away
(see "Folding a section" in `docs/product/project-pane-git-change-list.md`), the commit rows folded away (see "Grouped
by commit" in that doc) and, grouped by folder, the folders folded away. Reloading the window, or restarting the application, starts every project on its default
branch, with no change selected and every section, commit and folder unfolded. Selecting one of the project's sessions sets the branch anew, once for that selection; the pane put back in
the right pane while the same session stays selected keeps it as it is (see "Following the selected session" above).
Change lists, branch lists, comparisons, commit lists and their contents are never stored. Whether the filter field is
on screen, and the text in it, last only while the project's pane stays in the right pane, and the text only until the
field is closed (see "Filtering by file name" in `docs/product/project-pane-git-change-list.md`).

**Per project, across restarts**: whether the Changes tab's list is grouped by folder (see "Flat or grouped by folder"
in `docs/product/project-pane-git-change-list.md`), and whether its committed changes are grouped by commit (see
"Grouped by commit" in that doc). They are kept with the
rest of the pane's state, as the mode is (see "What is remembered" in `docs/product/project-pane.md`).
