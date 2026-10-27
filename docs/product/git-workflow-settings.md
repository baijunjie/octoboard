# Git workflow settings

The **git workflow settings** say where a project session develops — on the current branch, on a branch of its own or
in a worktree of its own — how it delivers finished work, and whether the revert gate guards its git. There are four,
each with an application-wide value, and each project can hold its own value for any of them (see "A project's own git
workflow settings" below).

## What they do now

Three of the settings — where sessions develop, rebase before merging and delivery — shape the development rule a
project session is given in its role description (see "The development rule" below). The fourth, the revert gate,
switches the revert gate on a project session's git (see `docs/product/revert-gate.md`), and is read every time the gate
runs, so a change reaches running sessions at once. They are set in Settings' Git section and, per project, in the
project dialog's Git workflow tab, and through the daemon's requests and the console session's settings tools (see
"Reading and changing them" below).

Where sessions develop decides whether there is a rule at all: while a project's effective value is on the current
branch, no rule is given, and rebase before merging and delivery have no effect whatever they hold. Their stored values
are kept, so setting development back to a branch or a worktree of its own restores them as they were set. The revert
gate does not depend on where sessions develop: it runs whenever the project's effective revert gate is on.

## The four settings

| Setting | Key | Values | Application-wide default | What it names |
|---|---|---|---|---|
| Where sessions develop | `development` | `in_place`, `branch`, `worktree` | `worktree` | Where a project session works: directly on the current branch, given no rule (**On the current branch**); on a new branch of its own checked out in the project directory itself (**On a branch of its own**), which suits one session at a time; or on a new branch of its own in a worktree of its own (**In a worktree of its own**). |
| Rebase before merging | `rebase_before_merge` | `true` / `false` | on | Whether a branch is rebased onto its target branch before it is merged. |
| Delivery | `delivery` | `ask`, `merge`, `pull_request` | `ask` | How finished work leaves its branch: the user is asked each time, the branch is merged into its target branch locally, or it is pushed and a pull request opened. |
| Revert gate | `revert_gate` | `true` / `false` | off | Whether git run inside a project session is refused a branch update or a push that quietly undoes changes already on a guarded branch (see `docs/product/revert-gate.md`). |

The defaults hold for a fresh installation and for one from before the settings existed. Any other `development` or
`delivery` value is refused.

The application-wide values are kept by the daemon and sent to every window with the other application settings, so
every window shows the same.

## A project's own git workflow settings

For each of the four settings a project either **follows the application-wide value** or **holds its own**, item by
item: a project can hold its own delivery and follow the application-wide value for the other three. Every item can be
held per project, not only where sessions develop, because how work is delivered is what most often differs between
repositories.

A project's **effective** git workflow settings are, item by item, its own value where it holds one and the current
application-wide value otherwise. They are always worked out from the values as they are at that moment, so a change to
an application-wide value reaches at once every project that follows it, and a project holding its own value for an
item is not affected by it.

A project starts out following all four application-wide values unless it is given its own when it is added: the user's
form and the console session's `add_project` can both set any of the four items for the new project, and an item left
out follows the application-wide value. When a parent directory adds several projects, each takes the same values. Every
project from before the settings existed follows all four. A project created by an import takes its own values from the
configuration document (see "What each project carries" in `docs/product/configuration-export-and-import.md`).

## The development rule

Every launch of a project session — a fresh one, a resume and an account switch alike — reads its project's effective
git workflow settings as they are at that moment. With development on a branch of its own, the session's role
description (see "What is injected on every launch" in `docs/product/launching-agents.md`, which also gives where the
section sits among the others) carries a section headed "Developing on a branch"; with development in a worktree of its
own, one headed "Developing in a worktree". With development on the current branch, the role description carries no
such section. A console session is never given the rule, whatever the settings: it changes no project itself (see
`docs/product/hub-orchestration.md`). Nothing is written into the project for it: the rule is part of the injected role
text and goes with the process, keeping the guarantee that Octoboard installs nothing into a project (see "What
Octoboard never modifies" in `docs/product/launching-agents.md`).

The rule first says that where the project's own instructions set out how to use worktrees or branches, those are
followed instead of the rule. Whether they do is left to the agent: Octoboard does not look into the project's
instruction files, by keyword or otherwise, and gives the rule whatever they say. The agent reads them anyway and
judges what they set out better than a keyword match would, and this works alike for every agent. Otherwise:

1. **A branch of its own before changing files.** Before changing files in a git repository, the session creates a new
   branch from the current branch, which becomes the new branch's target branch, and works and commits there.
   Read-only work, such as investigating or answering questions, needs none.
   - On a branch of its own, it creates the branch with `git switch -c <name>` in the project directory it was started
     in.
   - In a worktree of its own, it gives the branch its own worktree at `.worktrees/<short-name>` in the project
     directory it was started in, and adds `.worktrees/` to `.git/info/exclude` so no tracked file changes.
2. **A handed-over branch.**
   - On a branch of its own, a branch handed to the session is switched to in the project directory, or, when it is
     already checked out in a worktree, worked on in that worktree.
   - In a worktree of its own, a branch handed to the session that already has a worktree is worked on there.

   Git in a worktree is run as `git -C <worktree>` rather than after a `cd`: Claude Code asks for approval of any
   command that changes directory before running git, even a git command that is allowed.
3. **Delivery.** When done, the session reports the branch and its changes, and merges, pushes or opens a pull request
   as the user says — or, for a session whose role description names an owner, as the user or its owner says. A
   session given the unbound project session's text, a lead session included, takes it only from the user. Where
   nobody has said, the delivery setting decides: `ask` — it asks the user which; `merge` — it merges the branch into
   its target branch locally; `pull_request` — it pushes the branch and opens a pull request.
4. **Merging.** With rebase before merging on, the branch is rebased onto its target branch and the target branch
   fast-forwarded to it, and a pushed branch that is not the session's alone is never rebased. With it off, the branch
   is merged into its target branch without being rebased first.
5. **Cleaning up.** The session cleans up itself as soon as a branch or worktree is no longer needed.
   - On a branch of its own:
     - after a local merge, or after opening a pull request, it switches back to the target branch and deletes the
       local branch; after a pull request it leaves the remote branch, and to change the pull request later it switches
       to the remote branch again, pushes to the same branch, and cleans up again;
     - it asks before switching away from a branch with uncommitted changes, and never discards anything on the user's
       behalf.
   - In a worktree of its own, rather than leaving it to the cleanup run when the session is archived (see
     `docs/product/branch-and-worktree-cleanup.md`):
     - after a local merge, it removes the worktree and the local branch from the main working copy at once, as part of
       the merge and without asking again;
     - after opening a pull request, it removes the worktree and the local branch at once, leaving the remote branch; to
       change the pull request later it recreates the worktree from the remote branch, pushes to the same branch, and
       cleans up again;
     - a worktree opened only to verify something, never committed to, is opened detached and removed as soon as the
       verification is done;
     - it asks before removing a worktree that has uncommitted changes, or ignored files beyond regenerable dependencies
       and build output (such as `.env` or local configuration), and never discards or force-removes anything on the
       user's behalf.

A local branch the session's git creates, including one recreated from its remote branch to change a pull request —
whether git creates the local branch from the remote one on a switch to it, or on `git worktree add <path> <branch>`,
or it is created explicitly, as with `git worktree add -b <branch> <path> <remote>/<branch>` — makes that session the
branch's responsible session like any other branch it creates (see "When a branch gets a responsible session" in
`docs/product/branch-responsible-session.md`).

### When a change reaches a session

Claude Code and Grok Build keep the role text a conversation was first given for the conversation's whole life, resumes
included (see "What is injected on every launch" in `docs/product/launching-agents.md`). So, unlike the other settings
read at launch, a change to where sessions develop, rebase before merging or delivery — application-wide or a
project's own — is sure to reach only project sessions begun after it: a session begun earlier may keep the rule it
started with, or its absence, even when resumed. The settings tools' `git_workflow` argument says so for each of those
three items.

## Reading and changing them

**In Settings.** The application-wide values are set in Settings' Git section: where sessions develop, rebase before
merging and delivery in its Development group, which shows the rebase and delivery rows only while development is not
on the current branch, and the revert gate in its Protection group, always shown (see "Git" in
`docs/product/settings.md`).

**In the project dialog.** A project's own values are set under the **Git workflow** tab of the project dialog's
collapsed Advanced section, both when a project is associated and when it is edited (see "The Advanced section" in
`docs/product/model-and-reasoning-effort.md` for the section's tabs, and "The project's account, model and reasoning
effort" in `docs/product/projects.md` for where the section sits). The tab names the group; the group carries no heading
of its own. It lists the four items in the order Settings does — **Where sessions develop**, **Rebase before merging**,
**Delivery**, **Revert gate** — each as a picker whose first choice, **Same as global setting**, carries the current
application-wide value in a badge and follows it; the other choices are the item's own values — the three development
choices, On and Off, or the three delivery choices. The group's text says that each item follows the global setting
unless a value is chosen for the project, that a change to development, rebase or delivery reaches only sessions begun
after it, and that a change to the revert gate takes effect at once. While the project's effective development — the
value picked in the dialog, or the application-wide one while it is on Same as global setting — is on the current
branch, the rebase and delivery pickers are not shown; what they hold is kept, and they come back with it when
development is set to a branch or a worktree of its own. The revert gate picker is always shown. When a picker that goes
had keyboard focus, focus moves to the Where sessions develop picker. The group is disabled while the rest of the form
waits on the check of its location (see "Associating a project" in `docs/product/projects.md`).

Saving an edit changes only the items changed in the dialog, an item put back on Same as global setting following the
application-wide value again; an item left alone keeps what the project holds, even where another window changed it
meanwhile. What associating a project gives it is in "A project's own git workflow settings" above.

**Through the daemon's requests.** The settings travel as `git_workflow`, on the application settings and on every
project. A change to the application-wide values names only the items it changes; an item left out stays as it is. A
change to a project's own values does the same per item, and an explicit `null` for an item puts the project back to
following the application-wide value. The application-wide values are broadcast to every window only when a change
actually changed one, and a changed project reaches every window as any other change to a project does.

**Through the settings tools.** A console session reads and changes the application-wide values, and the own values of
its console's projects, like the other settings it may change: all four, the revert gate included, are open to it, and
none of them is among the settings that must be done by the user personally. Every tool that reports a project gives
both the project's own values and its effective ones. A project session reads its own project's values with
`get_project` and changes none. The arguments and results are in "A project's settings", "Changing a project:
`update_project`" and "Application settings: `get_settings` and `update_settings`" in
`docs/product/agent-settings-tools.md`.

**Through configuration export and import.** A full export carries the application-wide values, and every export, full
or of one console, carries each project's own values (see "The application settings a full export carries" and "What
each project carries" in `docs/product/configuration-export-and-import.md`).
