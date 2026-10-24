# Branch and worktree cleanup

When sessions are archived or deleted, Octoboard removes the git branches those sessions were working on, and the
linked worktrees that have them checked out, wherever git can remove them safely. What git refuses stays unless the
user's confirmation takes it along. Removing a project with its folder moved to the Trash takes the project's outside
worktrees, and where the repository outlives the folder its sessions' branches, along with the folder (see "With a
project's folder moved to the Trash" below). Archiving itself is in "Archiving, interruption and resuming" in
`docs/product/archiving-and-resuming-sessions.md`, and deleting in `docs/product/deleting-sessions.md`.

## What is bound to a session

Sessions store no branch or worktree of their own.

- **A local branch is bound to a session** when its record names that session as the branch's responsible session
  (see "The responsible session" in `docs/product/branch-responsible-session.md`) and it is still the branch that
  record was made for. A branch handed over is bound to the session it was handed to. A branch no session's git
  reported creating has no responsible session, so it is never bound and never touched by this cleanup.
- **A linked worktree is bound** when it has such a branch checked out.
- Only the repository of the session's project is looked in. A console session has no project, so nothing is bound to
  it.
- What an archive or a deletion takes into account is everything bound to every session it archives or deletes: the
  sessions an owner's archive takes along, and the archived sessions deleted with an archived owner, included.

## Which archives and deletions remove them

- **The user's** archive of a session (with every unarchived session under it), deletion of one session (with the
  archived sessions deleted along), deletion of every archived session at once, and deletion of an unused session.
- **An agent's**: `archive_session`, `archive_self`, and the archive a `done` report with no open items brings (see
  "Archives made by an agent" below).
- **Octoboard's own** deletion of archived sessions whose conversation is gone (see "Deleting archived sessions whose
  conversation is gone" in `docs/product/deleting-sessions.md`). It asks nothing, so it forces nothing.

Ending a session's process otherwise — an interruption, an account switch — removes nothing, and neither does the
archive that ends a handoff to another agent (see `docs/product/handing-off-sessions.md`). Removing a project
without moving its folder to the Trash ends its sessions and removes no branch or worktree, and neither does deleting
a console.

## Where each item stands

| Where it stands | What happens without the user's word |
|---|---|
| A worktree with no modified or untracked files, or whose directory is already gone | Removed (`git worktree remove`, unforced) |
| A branch merged into its upstream — into the main worktree's `HEAD` when it has none — and checked out in no worktree that stays | Deleted (`git branch -d`) |
| A worktree with uncommitted changes (modified or untracked files) | Kept; the confirmation can force it |
| A worktree with initialized submodules | Kept; the confirmation can force it |
| A branch not merged as above | Kept; the confirmation can force it |
| A worktree locked with `git worktree lock` (git's lock, not "Locking a session" in `docs/product/sessions.md`) | Kept; the confirmation can take it, under a choice of its own |
| A branch checked out in a worktree that stays — the main worktree, a worktree not bound, or a bound one that is kept | Kept, whatever is asked |
| A worktree that is a project's folder, or holds one — any project's | Never touched |

- **A branch pushed to its upstream counts as merged into it**, so it is deleted locally even when it is merged
  nowhere else; its remote branch stays.
- The main worktree is never removed.
- Worktrees go first, then branches, so a branch whose only worktree has just gone is deleted after it.
- A worktree whose status cannot be read is not removed, and is reported (see "What is left is reported" below); for
  the forcing rules it counts as having uncommitted changes.

## The confirmation

The confirmations for archiving a session, deleting one archived session and deleting every archived session at once
each look up what is bound to the whole scope when they open. Their Confirm button shows as pending, and cannot be
pressed, until the answer arrives. An inspection that fails counts as finding nothing to force.

Each choice the confirmation offers is a plain block of its own, never inside a warning: a checkbox, unchecked by
default, whose label names what it removes and how many; a short hint; and under them a **Details** disclosure,
collapsed by default and left as the user sets it while it is shown, over the items it covers grouped by what keeps
them, each group led by one line saying so (that these worktrees have uncommitted changes, say) over the names.
Expanded, those groups sit together on a light grey panel, set apart from the option's label and hint. A worktree is
named by its path, with the home directory written as `~`, and the branch it has checked out (or that its `HEAD` is
detached); a branch by its name. Paths and branch names are plain monospace text. A worktree's branch — its branch
glyph and name, or the detached `HEAD` note — is kept together: where it does not fit on the line after the path, it
moves to the next line whole rather than breaking at a `/`; a branch name wraps within itself only when it is wider
than the list on its own.

- **What git cannot remove safely** — worktrees with uncommitted changes, worktrees with initialized submodules, and
  unmerged branches, one group each — comes under **Also force-remove the worktrees and branches Git keeps (n)**. Its
  hint says that their uncommitted changes and unmerged commits cannot be recovered. Checked, those worktrees are
  removed with `git worktree remove --force` and those branches deleted with `git branch -D`.
- **Locked worktrees** come under a block of their own, **Also delete the locked worktrees (n)**, each one marked, by
  an amber badge after its name, when it has uncommitted changes. Its hint says they go despite their locks and cannot
  be recovered. Checked, they are removed past the lock (`git worktree remove` forced twice); the branch one had
  checked out then follows the branch rules above.
- Neither block is shown while it has nothing to cover.
- **What a ticked option does is said in the confirmation's one warning alert**, which sits below its options and
  above its buttons and holds everything it warns of. Its title summarizes the sessions archived or deleted along with
  the session, or how many are deleted along with all the archived sessions; the list of those sessions, where there
  is one, follows inside the alert under the title. Each option ticked adds a line inside the alert, after the list:
  that it also permanently deletes that many worktrees and branches Git keeps, or that many locked worktrees,
  uncommitted changes included. The list and the lines are in the same text style as an alert's description. A
  confirmation that had no warning gets one at the first tick, titled that the action has additional consequences,
  with the lines under that title; it goes again when the last option is unticked.
- **How a ticked option changes the confirm button** — its label, and for archiving its danger colour — is set in
  "What an open dialog, menu or popover covers" in `docs/product/window-layout.md`.
- **A choice holds for the list it was made over.** A list that changes while the confirmation is open — a session
  joining the scope, say — unchecks its checkbox again, so nothing the user has not seen listed is forced.
- **A force goes only as far as the user was shown.** An item forced is removed only while it still stands as it was
  listed: a worktree in another state by then, or with uncommitted changes the confirmation did not show, is left and
  reported. A branch merged in the meantime is deleted without force.
- Unchecked, the listed items are kept, and are not reported afterwards while they stand as they were listed.

**An archive that asks nothing of the sessions** — a session at its prompt or interrupted, with no unarchived session
under it (see "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`) — first asks the daemon what is
bound to it, from the menu's Archive and from the row's Archive button alike. It opens the archive confirmation only
when that finds something git refuses or a locked worktree; otherwise the session is archived at once. Pressing
Archive again while the answer is on its way does nothing. A session archived or gone by the time the answer arrives
is left as it is, one that has come to need the confirmation for other reasons gets it, and when another dialog has
opened in the meantime nothing is done.

Deleting an unused session asks for no confirmation (see "Deleting an unused session" in
`docs/product/deleting-sessions.md`), so it forces nothing.

## When the removal runs

- **After the archive or deletion has been made**: its request is answered, and its confirmation closes, without
  waiting for the removal.
- **Once the processes of the sessions archived or deleted have ended**, since an agent still running may be working in
  its worktree. The wait gives up after 30 seconds and the removal goes ahead; an unforced removal still refuses a
  worktree with changes.
- **A session resumed or reopened in the meantime is skipped**: what is bound to it stays.
- The removals of one repository run one at a time.
- **A failure never fails the archive or deletion**, which stands either way. The item that could not be removed stays
  where it is.

## What is left is reported

After a user's archive or deletion, and after a project's removal with its folder moved to the Trash, what was to go
and is still there is reported to every window as one error alert, `session_branches_not_removed` (see
`docs/product/error-alerts.md`). It groups the items by why each is still there, in this order, each group led by
one line saying so over the names:

- removing it failed — with git's message, or why the move to the Trash failed, under the item's name as a block of
  code;
- the worktree has uncommitted changes; the worktree has initialized submodules;
- the worktree is locked — each one marked when it also has uncommitted changes;
- the branch is not merged;
- the branch is checked out in a worktree that stays;
- the worktree is a project's folder.

Items are named as in the confirmation (see "The confirmation" above). With more than one group, a line saying these
were to be removed and are still there comes first; with a single group its lead line stands alone.

The alert's recovery guidance is to open a Terminal in the repository — from the project's action menu, when the
project is still there — deal with what is reported for each item, then remove it with Git or move it to the Trash, or
leave it in place.

What is reported: every item that could not be removed; every forced item still there; and every item git refuses
that the confirmation did not list as kept in the state it stands in now — one that came to stand so after the
confirmation listed it, or that was never listed. What is not reported: items the user chose to keep, while they stand
as they were listed; a branch checked out in a worktree that stays; a worktree that is a project's folder. Nothing is
shown when nothing is left.

What Octoboard's own deletion of archived sessions whose conversation is gone leaves is not reported to anyone: it is
only written to the daemon's log.

## Archives made by an agent

`archive_session`, `archive_self`, and the archive a `done` report with no open items brings (see "The console
session's tools" and "A session archiving itself" in `docs/product/hub-orchestration.md`, and "Automatic archiving" in
`docs/product/reporting-to-owners.md`)
remove what is bound to the sessions archived the same way, and **never force anything**: an agent's archive removes
no worktree with changes or submodules, no unmerged branch and no locked worktree. No error alert is shown for what
an agent's archive leaves; the agent is told instead:

- **`archive_session`** waits for the removal and answers with `branches_left`, one entry per item left — its `kind`,
  `branch`, `path` where it is a worktree, and the `reason` in plain text — and a `note` saying how to clean them up
  (deal with what keeps each, then `git worktree remove` and `git branch -d`). Both are absent when nothing is left.
- **`archive_self`** answers before the caller's process ends, so its `branches_left` is what the removal will leave as
  judged before the archive, with a note saying so.
- **A `done` report** that archives the session names in its answer the bound branches and worktrees that will not be
  removed, judged the same way.

Example of an `archive_session` answer with something left:

```json
{
  "session": "…",
  "status": "archived",
  "branches_left": [
    { "kind": "worktree", "branch": "feat/x", "path": "/work/repo-x", "reason": "it has uncommitted changes" },
    { "kind": "branch", "branch": "feat/x", "reason": "it is not merged" }
  ],
  "note": "…"
}
```

## With a project's folder moved to the Trash

Removing a project without moving its folder to the Trash touches no branch or worktree. Moving the folder to the
Trash (see "Moving the project folder to the Trash" in `docs/product/removing-projects.md`) takes along what would
otherwise be left without its folder:

- **When the folder holds the repository** — the repository's common git directory is inside the folder — every
  linked worktree of that repository outside the folder is moved to the Trash, bound to a session or not, one with a
  detached `HEAD` included, since none of them works once the repository is gone. The branches go with the repository.
- **When the repository outlives the folder** — the folder is part of a bigger repository, or is itself a linked
  worktree — the linked worktrees outside the folder that are bound to the project's sessions are moved to the Trash,
  the repository's worktree records are then pruned, and the branches bound to the project's sessions are deleted with
  `git branch -D`, permanently. A branch still checked out in a worktree that stays is not deleted, and is reported,
  unless that worktree is a locked one the user chose to keep.
- **No safety check is made**: an outside worktree is moved to the Trash whatever it holds, uncommitted changes
  included, and a branch is deleted merged or not. What is moved can be recovered from the Trash; a deleted branch
  cannot.
- **A locked worktree** goes only when the user chooses so (below), and only while it has no uncommitted changes the
  confirmation did not show. It is moved to the Trash first and only then unlocked, where the repository outlives the
  folder, so one whose move fails keeps its lock. A branch a locked worktree has checked out goes only with it.
- **Never touched**: the main worktree, and a worktree that is another project's folder or holds one.

**The confirmation** looks up what goes along each time its option to move the folder to the Trash is ticked; its
Confirm button is pending until the answer arrives. Under the option, a **What goes with it (n)** disclosure, collapsed
by default, groups the items, named and set out as in "The confirmation" above, as moved to the Trash with the folder
(the outside worktrees) and deleted permanently (the branches). Locked worktrees come after it as a plain block of their own, as in
"The confirmation" above: the checkbox **Also move the locked worktrees to the Trash (n)**, its hint, saying they go
despite their locks and can be recovered from the Trash, and a Details disclosure over them. A branch a locked worktree
has checked out is counted among what goes only while that box is checked.

In the confirmation's one warning alert, after the options (see `docs/product/removing-projects.md` for the running
sessions listed there, and "The confirmation" above for how the alert holds its list and lines), a line appears while
the Trash option is ticked for each kind of item that goes — that it also
moves that many worktrees outside the folder to the Trash, and that it also permanently deletes that many branches of the project's
sessions — and, while the locked box is checked, that it also moves that many locked worktrees to the Trash, uncommitted
changes included. When the lookup fails, the line instead says that worktrees of the repository outside the project
folder may be moved to the Trash, and its branches deleted permanently. A removal that would be refused (see "Moving the
project folder to the Trash" in `docs/product/removing-projects.md`) lists nothing and adds no line; confirming it shows
the refusal.

**What goes is worked out again when the removal is made**, while the folder is still there, so what changed after the
confirmation listed it is taken into account. It runs after the project has been removed and its sessions' processes
have ended, waiting as in "When the removal runs" above; the project's removal stands whatever happens to it, and what
is still there is reported as in "What is left is reported" above. When what goes along cannot be worked out at all,
the folder is still moved and the project removed, but no worktree outside the folder is touched, and where the
repository outlives the folder no branch either (where the folder held the repository, its branches went with it).
Every window is then shown an error alert, `project_branches_not_determined`, naming the removed project and saying
that the worktrees outside its folder, and its branches if the repository stays, were left, with the reason under the
sentence as a block of code. Its recovery guidance is to open a Terminal in the repository and run `git worktree list`
and `git branch` to see what was left, then remove what is no longer needed with Git, or leave it in place. When the
folder's own move fails, the project is not removed and nothing else goes.
