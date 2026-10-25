# Worktree settings

The **worktree settings** say how a project session is to use git worktrees and how it delivers finished work. There
are four, each with an application-wide value, and each project can hold its own value for any of them (see "A
project's own worktree settings" below).

## What they do now

Three of the settings — use worktrees for development, rebase before merging and delivery — shape the worktree rule a
project session is given in its role description (see "The worktree rule" below). The fourth, the revert gate, switches
the revert gate on a project session's git (see `docs/product/revert-gate.md`), and is read every time the gate runs, so
a change reaches running sessions at once. They are set in Settings' Worktrees section and, per project, in the project
dialog's Worktrees group, and through the daemon's requests and the console session's settings tools (see "Reading and
changing them" below).

Use worktrees for development is the main switch: rebase before merging, delivery and the revert gate apply only while
a project's effective use worktrees is on. While it is off no rule is given and the revert gate does not run, whatever
the other three hold. Their stored values are kept, so turning use worktrees back on restores them as they were set.

## The four settings

| Setting | Key | Values | Application-wide default | What it names |
|---|---|---|---|---|
| Use worktrees for development | `use_worktrees` | `true` / `false` | on | Whether project sessions are asked to develop in a worktree of their own. |
| Rebase before merging | `rebase_before_merge` | `true` / `false` | on | Whether a branch is rebased onto its target branch before it is merged. |
| Delivery | `delivery` | `ask`, `merge`, `pull_request` | `ask` | How finished work leaves its branch: the user is asked each time, the branch is merged into its target branch locally, or it is pushed and a pull request opened. |
| Revert gate | `revert_gate` | `true` / `false` | off | Whether git run inside a project session is refused a branch update or a push that quietly undoes changes already on a guarded branch (see `docs/product/revert-gate.md`). |

The defaults hold for a fresh installation and for one from before the settings existed. Any other `delivery` value is
refused.

The application-wide values are kept by the daemon and sent to every window with the other application settings, so
every window shows the same.

## A project's own worktree settings

For each of the four settings a project either **follows the application-wide value** or **holds its own**, item by
item: a project can hold its own delivery and follow the application-wide value for the other three. Every item can be
held per project, not only use worktrees, because how work is delivered is what most often differs between
repositories.

A project's **effective** worktree settings are, item by item, its own value where it holds one and the current
application-wide value otherwise. They are always worked out from the values as they are at that moment, so a change to
an application-wide value reaches at once every project that follows it, and a project holding its own value for an
item is not affected by it.

A project starts out following all four application-wide values unless it is given its own when it is added: the user's
form and the console session's `add_project` can both set any of the four items for the new project, and an item left
out follows the application-wide value. When a parent directory adds several projects, each takes the same values. Every
project from before the settings existed follows all four. A project created by an import takes its own values from the
configuration document (see "What each project carries" in `docs/product/configuration-export-and-import.md`).

## The worktree rule

Every launch of a project session — a fresh one, a resume and an account switch alike — reads its project's effective
worktree settings as they are at that moment. With use worktrees for development on, the session's role description
(see "What is injected on every launch" in `docs/product/launching-agents.md`) carries a section headed "Developing in
a worktree", placed just before the closing sentence that says the project's own instructions, conventions and
configuration apply unchanged. With it off, the role description carries no such section. A console session is never
given the rule, whatever the settings: it changes no project itself (see `docs/product/hub-orchestration.md`). Nothing
is written into the project for it: the rule is part of the injected role text and goes with the process, keeping the
guarantee that Octoboard installs nothing into a project (see "What Octoboard never modifies" in
`docs/product/launching-agents.md`).

The rule first says that where the project's own instructions set out how to use worktrees or branches, those are
followed instead of the rule. Whether they do is left to the agent: Octoboard does not look into the project's
instruction files, by keyword or otherwise, and gives the rule whatever they say. The agent reads them anyway and
judges what they set out better than a keyword match would, and this works alike for every agent. Otherwise:

1. **A branch and worktree of its own before changing files.** Before changing files in a git repository, the session
   creates a new branch from the current branch, which becomes the new branch's target branch, with its own worktree at
   `.worktrees/<short-name>` in the project directory it was started in, and adds `.worktrees/` to `.git/info/exclude`
   so no tracked file changes; it works and commits there, running git in the worktree as `git -C <worktree>` rather
   than after a `cd` (Claude Code asks for approval of any command that changes directory before running git, even a
   git command that is allowed). Read-only work, such as investigating or answering questions, needs no worktree.
2. **A handed-over branch.** A branch handed to the session that already has a worktree is worked on there.
3. **Delivery.** When done, the session reports the branch and its changes, and merges, pushes or opens a pull request
   as the user says — or, for a session whose role description names an owner, as the user or its owner says. A
   session given the unbound project session's text, a lead session included, takes it only from the user. Where
   nobody has said, the delivery setting decides: `ask` — it asks the user which; `merge` — it merges the branch into
   its target branch locally; `pull_request` — it pushes the branch and opens a pull request.
4. **Merging.** With rebase before merging on, the branch is rebased onto its target branch and the target branch
   fast-forwarded to it, and a pushed branch that is not the session's alone is never rebased. With it off, the branch
   is merged into its target branch without being rebased first.
5. **Cleaning up.** The session removes a worktree itself as soon as it is no longer needed, rather than leaving it to
   the cleanup run when the session is archived (see `docs/product/branch-and-worktree-cleanup.md`):
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
whether with `git worktree add -b <branch> <path> <remote>/<branch>` or with `git worktree add <path> <branch>`, where
git creates the local branch from the remote one — makes that session the branch's responsible session like any other
branch it creates (see "When a branch gets a responsible session" in `docs/product/branch-responsible-session.md`).

### When a change reaches a session

Claude Code and Grok Build keep the role text a conversation was first given for the conversation's whole life, resumes
included (see "What is injected on every launch" in `docs/product/launching-agents.md`). So, unlike the other settings
read at launch, a change to use worktrees for development, rebase before merging or delivery — application-wide or a
project's own — is sure to reach only project sessions begun after it: a session begun earlier may keep the rule it
started with, or its absence, even when resumed. The settings tools' `worktrees` argument says so for each of those
three items.

## Reading and changing them

**In Settings.** The application-wide values are set in Settings' Worktrees section, which shows the rebase, delivery
and revert gate rows only while use worktrees for development is on (see "Worktrees" in `docs/product/settings.md`).

**In the project dialog.** A project's own values are set in a **Worktrees** group inside the project dialog's collapsed
Advanced section, both when a project is associated and when it is edited (see "The project's account, model and
reasoning effort" in `docs/product/projects.md` for where the section sits). The group lists the four items in the order
Settings does, each as a picker whose first choice, **Same as app setting**, carries the current application-wide value
in a badge and follows it; the other choices are the item's own values — On and Off, or for delivery its three choices.
The group's text says that each item follows the application setting unless a value is chosen for the project, that a
change to a rule reaches only sessions begun after it, and that a change to the revert gate takes effect at once. While
the project's effective use worktrees — the value picked in the dialog, or the application-wide one while it is on Same
as app setting — is off, the other three pickers are not shown; what they hold is kept, and they come back with it when
use worktrees is on again. When a picker that goes had keyboard focus, focus moves to the use worktrees picker. The
group is disabled while the rest of the form waits on the check of its location (see "Associating a project" in
`docs/product/projects.md`).

Saving an edit changes only the items changed in the dialog, an item put back on Same as app setting following the
application-wide value again; an item left alone keeps what the project holds, even where another window changed it
meanwhile. What associating a project gives it is in "A project's own worktree settings" above.

**Through the daemon's requests.** A change to the application-wide values names only the items it changes; an item
left out stays as it is. A change to a project's own values does the same per item, and an explicit `null` for an item
puts the project back to following the application-wide value. The application-wide values are broadcast to every
window only when a change actually changed one, and a changed project reaches every window as any other change to a
project does.

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
