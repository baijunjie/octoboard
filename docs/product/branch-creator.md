# Which session created a branch

Octoboard records which session a local branch of a project's repository was created by, and the project pane's Git
mode marks the branches it can name a session for and follows the selected session's newest branch in its selectors
(see "Following the selected session" in `docs/product/project-pane-git-mode.md`). The record names **the session
that created the branch**, or, when that cannot be told, the one working in the branch's worktree — never merely
whoever touches the branch later. A branch Octoboard cannot attribute to exactly one session carries none, which is no
failure.

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
session could have made it at that moment. **The command an agent ran is never read**, so a branch made by
`git branch`, `git checkout -b`, `git switch -c`, `git worktree add -b`, an alias or a script of the agent's own all
count the same. Each of the signals below names a session only when it leaves exactly one candidate.

A branch is attributed to a session when one of these holds:

- **It was born while exactly one session had a tool call running.** Octoboard looks at the repository's branches as
  sessions work — each hook event a session's agent sends can prompt a look — and a branch that was not there at the
  previous look is matched against the moment its reflog says it was born. A session counts as running a tool call
  from the agent's tool-start event to its tool-end event, for its own calls and its subagents' alike, and from a few
  seconds before the start event arrives, since an agent may run a call before that event lands. A call whose
  tool-end event never comes ends otherwise:
  - a call the agent reports as refused at its permission check ends there;
  - on Claude Code, which reports nothing when the user declines a permission prompt or a question, the calls the
    session has open of its own end when Octoboard finds the decline in the agent's transcript, provided the session
    is still waiting on the user then; those of a subagent it runs in the background go on;
  - any other call counts as running until the session next ends a turn or stops running.

  The branch goes to the one session of the repository that had a call running at its birth, give or take a few
  seconds. **A session that is mid-turn but running no tool** — its model writing, or a question to the user up —
  could not have made the branch and does not count: an agent's own ask-the-user tool (Claude Code's
  `AskUserQuestion`, Grok's `ask_user_question`) is not counted as a tool call. Sessions working in parallel therefore
  only collide when their tool calls overlap the branch's birth; when they do, the branch is left unattributed.
- **Its creation falls inside exactly one session's lifetime.** For a branch with no record at all — one made before
  Octoboard ran or kept these records, or a new one the Git mode lists before its tool-call answer is due (see
  below) — the time the branch was created is read from the first entry of the branch's reflog, and the branch is
  attributed when exactly one session of the repository that has had a turn was running then. A session's lifetime is
  all this has to go on, not its tool calls, so it is the weaker signal and only ever fills in a branch that has no
  record at all. It also needs a first reflog entry that a branch's own creation writes: a reference that was merely
  written — the default branch a `git init` leaves, a clone's, a bare `git update-ref` — is never attributed, however
  clear the session.
- **It is checked out in the worktree exactly one running session works in.** A branch neither of the above could
  attribute — including one whose creation was missed altogether — goes to the one running session of the repository
  whose working directory lies inside the worktree that has the branch checked out. The worktree is the innermost one
  holding that directory, since a linked worktree can sit inside the main one. The working directory is the one the
  session's agent last reported in the session's own hook events, never in a subagent's; before it has reported one,
  the session counts as being in its project's directory. This too needs a first reflog entry that a branch's own
  creation writes, made while that session was running, so a default branch from `git init` or a clone, or a branch
  the session only checked out later, is never attributed this way. The main working copy is held to the same rule,
  so with several sessions sitting there it names none.

**A new branch is decided only a few seconds after its birth**, once every hook event that could bear on it has had
time to arrive; Octoboard looks again by itself when that time is up. Until then the looks sessions prompt give it no
creator, so its creator can show up on a later listing than the branch itself. Meanwhile a listing of the Git mode's
branches can give it one of the weaker answers — its reflog or its worktree — and the tool-call answer, when it comes,
replaces that answer or not by the order below.

Later signals can improve on what an earlier look recorded:

- A tool-call attribution replaces a branch left unattributed and a worktree attribution.
- A worktree attribution replaces only a branch left unattributed.
- The reflog fills only a branch with no record at all.
- A creation recorded by tool call or by reflog is never replaced.

A branch carries no creator whenever that leaves more than one answer or none: several sessions could have made it,
nothing was running for it, or the only candidate was not running when the branch was created. In particular:

- Only **sessions of projects in that repository** are candidates. A console session, which runs in a directory of
  Octoboard's own (see "Console sessions and project sessions" in `docs/product/sessions.md`), is never named.
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
- **Nothing runs on a periodic timer.** The records of a repository are brought up to date whenever a session at work
  prompts a look at its branches, by one follow-up look a few seconds later when a look leaves something to catch up
  on, and whenever the Git mode lists them, in either of its views — which is also what removes the record of a
  branch that is gone.
- **Removing a project** clears the records of its repository, unless another project still points into that
  repository (see "Removing a project" in `docs/product/consoles-and-projects.md`). Deleting a console removes every
  project under it, so it clears theirs the same way.
- At most **1,000 branches of one repository** carry a record. In a repository with more branches than that, the
  ones a look reaches first — in the byte order of their names — are the ones attributed, and a record past the cap
  is dropped by the time its branch was created, oldest first.

## Where a record can be missing or wrong

- **The session named is the one whose tool call the branch was born during, which is not always whoever made it.** A
  branch the user makes in their own terminal, or one an agent makes through a shell escape that fires no tool events,
  while exactly one session of the repository is running a tool call goes to that session; so does one a background
  process makes after the call that started it has ended. A tool call Octoboard never heard start or end is invisible,
  and a branch made in it can go to another session's call.
- **A call waiting at a permission prompt counts as running** from the moment it is announced: nothing reliable says
  when the user grants it, so the time it waits cannot be told from the time it runs. A branch another session's tool
  call makes meanwhile is left unattributed, and one made outside any tool call while no other session runs one goes
  to the waiting session.
- **A subagent's whole run counts as the one tool call that started it**, besides the subagent's own calls, so a
  session with a subagent at work counts as running a tool call throughout.
- An agent working in a worktree **without moving its own working directory there** does not count as being in that
  worktree.
- Tool calls are remembered for **10 minutes**: a branch first looked at longer than that after its birth gets no
  answer from them.
- In a repository whose references are kept in **reftable** format there are no reflog files. No branch is attributed
  from a reflog or by its worktree there; the tool-call match goes by the whole stretch since the previous look rather
  than by the branch's birth; and a record cannot be held against the birth of the branch it was made for, so a branch
  deleted and created again under the same name keeps the first one's creator until the name itself goes.
- In any repository, a branch deleted and created again **at the same commit within one second** keeps the first one's
  creator the same way.
- A record does not outlive its reflog. `git gc` drops reflog entries older than `gc.reflogExpire`, 90 days by default;
  a branch whose first entry has gone that way loses its record, and nothing attributes it again.
