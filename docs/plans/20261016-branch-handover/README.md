# Handing branches over with a session

## Problem

A session handing its work to another one usually hands over the branch the work is on. Today the only link between a
session and a branch is the branch's **creator**: the session whose own git reported creating it. The project pane's
Git mode follows the selected session to the newest branch it created, and the branch lists mark each branch's creator.

When a session asks with `request_session` for a new session to take its work over, the new session goes on working on
a branch the requester created. Selecting the new session then finds no branch of its own, so the Git mode falls back
to where a project starts, and the branch's mark keeps naming the requester. Not every handover moves a branch: the
requester may simply have been working on whatever the working copy has checked out, and then there is nothing to hand
over.

`transfer_session` and `bind_to_session` only change a session's owner; the session itself stays the same, and so do
its branches. They are out of scope.

## Plan outline

Add a second relation beside the creator: the branch's **responsible session**, the session working on it now. A
branch gets one when it gets a creator, and a handover moves it. `request_session` takes an optional list of branches
to hand to the new session, the confirmation dialog names them, and approval moves their responsible session to the
new one. The Git mode follows the selected session's responsible branches instead of the ones it created.

## Key design decisions

- **One-to-many, not many-to-many.** A branch has at most one responsible session at a time; a session may be
  responsible for several branches. A handover means "I stop, you carry on": two sessions responsible for one branch
  at once would be editing the same working tree against each other, which is the case to avoid, not one to model.
  Who worked on a branch before is history, carried by the creator record, not by the current relation.
- **The creator stays a fact and never changes.** The responsible session is a separate record. The creator badge goes
  on naming who created the branch.
- **A branch's responsible session starts as its creator**, at the moment the creator is recorded. A branch with no
  creator has no responsible session until one is handed it.
- **Handing branches over is optional and explicit.** `request_session` gets an optional `branches` argument. Left
  out, nothing moves: that is the case of a requester working on the working copy's checked-out branch. Nothing is
  inferred, such as handing over the branch the requester has checked out: being checked out is not the same as being
  handed over, and branch attribution in this project never rests on inference.
- **Only what the requester holds can be handed over.** A listed branch that the requester is not responsible for, or
  that is not in the repository of the project the new session would be in, is refused before the user is asked, as
  any other `request_session` refusal is. A console session asked for takes no branches.
- **The move happens on approval only.** Refused, timed out or withdrawn, nothing moves.
- **The dialog names the branches.** They are not text the agent wrote but branches checked to exist and to be the
  requester's, so naming them keeps the dialog to what Octoboard knows. Branch names are technical literals and show as
  code.
- **The Git mode follows responsibility.** Selecting a session sets the selectors from the branch it most recently
  became responsible for, whether by creating it or by having it handed over.

## Milestones

1. [Responsible session for a branch](01-responsible-session.md)
2. [Handing branches over with `request_session`](02-handing-branches-over.md)

## Open

- Where the new session runs when a handed-over branch is checked out in a linked worktree that Octoboard has as a
  project of its own: the requester names that project with `project`, and a mismatch is refused. Whether to point the
  requester at the right project in the refusal, or to resolve it for them, is not settled.
- Whether the branch lists mark the responsible session as well as the creator, and how the two marks read side by side.
