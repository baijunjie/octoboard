# Worktree settings

The **worktree settings** say how a project session is to use git worktrees and how it delivers finished work. There
are four, each with an application-wide value, and each project can hold its own value for any of them (see "A
project's own worktree settings" below).

## What they do now

No session reads the worktree settings yet: they are stored, carried and reported, but change nothing in how any
session works. Settings has no section for them and the project dialog no control: they are read and changed only
through the daemon's requests and through the console session's settings tools (see "Reading and changing them"
below).

## The four settings

| Setting | Key | Values | Application-wide default | What it names |
|---|---|---|---|---|
| Use worktrees for development | `use_worktrees` | `true` / `false` | on | Whether project sessions are asked to develop in a worktree of their own. |
| Rebase before merging | `rebase_before_merge` | `true` / `false` | on | Whether a branch is rebased onto its target branch before it is merged. |
| Delivery | `delivery` | `ask`, `merge`, `pull_request` | `ask` | How finished work leaves its branch: the user is asked each time, the branch is merged into its target branch locally, or it is pushed and a pull request opened. |
| Revert gate | `revert_gate` | `true` / `false` | on | Whether a branch update that quietly undoes changes already on the target branch is blocked. |

The defaults hold for a fresh installation and for one from before the settings existed. Any other `delivery` value is
refused.

The application-wide values are kept by the daemon and sent to every window with the other application settings, so
every window shows the same.

## A project's own worktree settings

For each of the four settings a project either **follows the application-wide value** or **holds its own**, item by
item: a project can hold its own delivery and follow the application-wide value for the other three.

A project's **effective** worktree settings are, item by item, its own value where it holds one and the current
application-wide value otherwise. They are always worked out from the values as they are at that moment, so a change to
an application-wide value reaches at once every project that follows it, and a project holding its own value for an
item is not affected by it.

Every project starts out following all four application-wide values, however it is associated — by the user, by the
console session's `add_project`, which takes no worktree settings, or as one of a parent directory's repositories — and
so does every project from before the settings existed. The one exception is a project created by an import, which
takes its own values from the configuration document (see "What each project carries" in
`docs/product/configuration-export-and-import.md`).

## Reading and changing them

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
