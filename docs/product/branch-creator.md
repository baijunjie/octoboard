# Which session created a branch

Octoboard records which session a local branch of a project's repository was created by, and the project pane's Git
mode marks the branches it can name a session for. The record answers **who created the branch**, never who is working
on it now, and a branch Octoboard cannot attribute to exactly one session carries none — which is the ordinary case,
not a failure.

## The creator badge

In the **Compare** view of the project pane's Git mode, each branch's row in the From and To lists carries a small
ring when the branch has a recorded creator — the **creator badge** (see "The branch selectors" in
`docs/product/project-pane-git-mode.md`).

- The name assistive technology announces for the badge, and its tooltip, are the same words: they say the branch was
  created by that session and name it by its title, **in quotation marks**, so that where a long title ends can be
  seen. The badge is no tab stop, so the tooltip shows on pointer hover only, and only while the pointer is on the
  ring itself rather than anywhere on the row; it opens after the same wait as every other tooltip in the window (see
  "Tooltips on indicators" and "How a tooltip behaves" in `docs/product/labels-and-tooltips.md`).
- **A session that still exists is named by the title it carries now** — the one the sidebar shows — so that the
  session named can be found there. An archived session still exists and is named the same way.
- **A session that no longer exists** is named instead by the title recorded when the attribution was made, cut to its
  first 80 characters, and the wording says that it no longer exists. Deleting an archived session, or removing a
  project that shares its repository with another project, takes the session's record while the branch's attribution
  stands. Until the window has its first answer from the daemon no session is called gone.
- **A branch created by the selected session** is marked apart from the rest: its ring is filled and in the accent
  colour where every other badge is a hollow ring in a neutral one, and its wording says the creator is the selected
  session as well as naming it. The fill, not the colour, is what tells the two rings apart, so the state does not
  rest on colour. A selected session that no longer exists is named as gone instead, like any other.
- Beyond that the badge carries nothing its name does not: its presence says a session created the branch, the name
  says which.
- **A chosen branch carries no badge**: the mark is on the rows of the two lists only, never on a selector once a
  branch is chosen.
- A branch with no recorded creator carries no mark of any kind, and nothing says why.

## When a branch has a creator

Attribution goes by the result, never by what ran: Octoboard notices that a branch has appeared and asks which
session was acting at the time. **The command an agent ran is never read**, so a branch made by `git branch`,
`git checkout -b`, `git switch -c`, `git worktree add -b`, an alias or a script of the agent's own all count the same.

A branch is attributed to a session when either of these holds:

- **It appeared while exactly one session was acting.** Octoboard looks at the repository's branches as a session
  works — the agent's tool calls prompt a look — and a branch that was not there at the previous look was made by
  whoever acted in between. It is claimed only when one session could have acted: another session of the repository
  acting in the same stretch, or one still mid-turn, leaves the branch unattributed.
- **It predates any look, and its creation falls inside exactly one session's lifetime.** For a branch Octoboard
  never saw appear — one made before it ran, or before it kept these records — the time the branch was created is
  read from the first entry of the branch's reflog, and the branch is attributed when exactly one session of the
  repository was running then. A session's lifetime is all this has to go on, not its turns, so it is the weaker
  signal and only ever fills in what the first one did not see. It also needs a first reflog entry that a branch's own
  creation writes: a reference that was merely written — the default branch a `git init` leaves, a clone's, a bare
  `git update-ref` — is never attributed, however clear the session.

A branch carries no creator whenever that leaves more than one answer or none: several sessions could have made it,
nothing was acting for it, or the session that was acting was not running when the branch was created. In particular:

- Only **sessions of projects in that repository** are candidates, and only ones that have had a turn. A console
  session, which runs in a directory of Octoboard's own (see "Console sessions and project sessions" in
  `docs/product/sessions.md`), is never named.
- A branch the user created in their own terminal usually carries no creator — **but not always**: created while
  exactly one session of the repository was mid-turn, it is attributed to that session. The record means the branch
  appeared alongside that session, whatever made it.
- The records belong to the **repository**, not to one project. With two projects in the same repository, a branch
  created by a session of either is marked in both projects' Git mode, and the session named can be one of the other
  project's.

## What the record is

- **Nothing is written to the user's repository.** No reference, note or configuration entry carries the record; it
  lives in Octoboard's own storage only, and the Git mode stays read-only (see "Reading only" in
  `docs/product/project-pane-git-mode.md`).
- **A record lives and dies with its branch.** Deleting the branch — after merging it, say — deletes its record, so
  a merged-and-deleted branch can no longer be traced. A branch deleted and created again under the same name is a new
  branch and does not inherit the old attribution.
- **Nothing runs on a timer.** The records of a repository are brought up to date whenever a session at work prompts a
  look at its branches, and whenever the Compare view lists them — which is also what removes the record of a branch
  that is gone.
- **Removing a project** clears the records of its repository, unless another project still points into that
  repository (see "Removing a project" in `docs/product/consoles-and-projects.md`). Deleting a console removes every
  project under it, so it clears theirs the same way.
- At most **1,000 branches of one repository** carry a record. In a repository with more branches than that, the
  ones a look reaches first — in the byte order of their names — are the ones attributed, and a record past the cap
  is dropped by the time its branch was created, oldest first.

## Where a record can be missing or wrong

- In a repository whose references are kept in **reftable** format there are no reflog files: no branch is attributed
  from a reflog there, and a record cannot be held against the birth of the branch it was made for, so a branch deleted
  and created again under the same name keeps the first one's creator until the name itself goes.
- The same holds anywhere for a branch deleted and created again **at the same commit within one second**.
- A record does not outlive its reflog. `git gc` drops reflog entries older than `gc.reflogExpire`, 90 days by default;
  a branch whose first entry has gone that way loses its record, and its reflog cannot attribute it again.
