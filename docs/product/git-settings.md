# Git settings

Settings' **Git** section (see "Git" in `docs/product/settings.md`), in three groups, each under a heading (see "Group
headings" in `docs/product/settings.md`): **Repository**, holding **Default clone directory** and then **Automatically
pull repositories**; **Development**, holding **Where sessions develop**, **Squash commits before delivery**, **Rebase
before delivery** and **Delivery** (see "Development" below); and **Protection**, holding **Revert gate** (see
"Protection" below). How the switches and choices save is in "How switches save" in `docs/product/settings.md`.

## Default clone directory

A text field with a folder icon in it and a **Browse** button beside it (the directory picker, see "Browsing
directories" in `docs/product/projects.md`), showing the directory a repository is cloned into when a project is added
from a git URL and no other directory is named. It is `~/Projects` until the user sets one. The field shows the
directory in a monospace font, with the daemon host's home directory written as `~` under the same rules as in "How
paths are shown" in `docs/product/settings.md`, and the full path as the field's tooltip, while the pointer is on the
field, whenever the two differ; the tooltip is in monospace like the field and reads left to right in every language
(see "How a tooltip behaves" in `docs/product/labels-and-tooltips.md`). The daemon stores the directory expanded, as an
absolute path without a trailing slash; a leading `~/` in what is sent is expanded again. The value is sent when the
field loses focus or Enter is pressed, and at once when a directory is picked; a value equal to what the field shows
unedited, or to the full path it stands for, sends nothing. Focus moving from the field into its Browse directory
picker is not leaving it and sends nothing: a pick replaces what was typed and sends the picked directory, and a
cancelled picker hands focus back to the field, whose next loss of focus sends what is typed. A mouse press on Browse
leaves focus in the field until the picker opens. `Escape` while the field holds keyboard focus closes Settings and
discards what was typed. It is the only exit that loses it: the close button, a press on the dimmed area around the
dialog, and moving focus out of the field all save the typed path first. Once a send is answered the field shows the
stored directory again, written as above. A value that is neither absolute nor starts with `~/` is refused with an
error alert and the field returns to the stored one, and a blank value goes back to `~/Projects`. One send is out at a
time: a value saved while one is unanswered waits, the latest replacing an earlier one, and is sent when the reply
comes, unless by then the stored directory is another window's, which it would overwrite — the waiting value is then
dropped, the field shows the stored directory and an error alert, titled that the setting could not be saved, says
that a change was made elsewhere and nothing was saved. A send is checked against the setting's version, so a change
another window made just before it is refused the same way, the field then showing that change. The window's own sends
in quick succession never conflict with one another. A change made elsewhere while nothing is being sent replaces what
the field shows, and replacing what the user had typed is announced as in "A form while its record changes elsewhere"
in `docs/product/concurrent-edits.md`. Browse opens the picker on the path the field holds — the stored directory until
something is typed — by the rules every directory field's Browse follows, which also say where it opens on a relative
path or on a directory that does not exist yet (see "Browsing directories" in `docs/product/projects.md`). Where the
directory applies is in "Associating a project" in the same document. A configuration export that includes the Git
section carries it, and an import asked to apply that section sets it unless the user keeps this machine's own (see
"The default clone directory on import" in `docs/product/settings-in-a-configuration.md`).

## Automatically pull repositories

A switch, off to begin with: whether a project's branch is pulled (fast-forwarded) on its own when it is behind its
upstream, rather than only reported as behind. The row's line of text follows the switch — while it is off, what
turning it on will do; while it is on, what is happening now — and either way says that a branch is pulled when it is
behind its upstream and can be fast-forwarded, that nothing is pushed, and that a branch with commits of its own is
left alone; a "?" at the end of its name says that the remote is checked every few minutes whichever way the switch is
set, so the branch always shows how far ahead or behind it is. What it does, what it never does and where the value is
kept are in "Automatically pulling repositories" in `docs/product/project-git-status.md`.

## Development

The Development and Protection rows are the application-wide git workflow settings. What each one does, its values and
its default are in `docs/product/git-workflow-settings.md`, and a project can hold its own value for any of them (see
"A project's own git workflow settings" there). A change applies that one item alone. The daemon keeps the values, so
every window shows the same. As with the other switches here, while a switch is off its row's description says what
turning it on will do, and while it is on it says what is happening now. A console session may read and change all
five git workflow settings, and a configuration export that includes the Git section carries them (see "Application
settings: `get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md` and "The application
settings an export carries" in `docs/product/settings-in-a-configuration.md`).

- **Where sessions develop**, a choice of **On the current branch**, **On a branch of its own** and **In a worktree of
  its own**, In a worktree of its own to begin with. Its description follows the choice. On the current branch, it says
  that a project session works directly on the current branch and is given no rule for branches or worktrees, that a
  change reaches only sessions begun after it, and that Squashing, Rebase and Delivery are shown once sessions work on a
  branch of their own. On a branch of its own, it says that a project session is asked to create a branch of its own in
  the project directory before it changes files, that the branch shares that directory so this suits one session at a
  time, that a change reaches only sessions begun after it, and that a project's own rules for worktrees and branches
  come first. In a worktree of its own, it says that a project session is asked to create a branch and a worktree of its
  own before it changes files, that a change reaches only sessions begun after it, and that a project's own rules for
  worktrees and branches come first. While it is on the current branch the other three rows of the group are not shown:
  their values are kept, and the rows come back with them when a branch or a worktree of its own is chosen again. When a
  row that goes had keyboard focus — another window or a console session can change the choice — focus moves to this
  row's picker.
- **Squash commits before delivery**, a switch, off to begin with. Its description says that a branch's commits are
  squashed into one commit for its feature before delivering finished work by merging, pushing or opening a pull
  request, that only a bug fix unrelated to that feature keeps a commit of its own, that a change reaches only sessions
  begun after it, and that a project's own rules come first.
- **Rebase before delivery**, a switch, on to begin with. Its description says that a branch is rebased onto its target
  branch before delivering finished work by merging, pushing or opening a pull request, that a change reaches only
  sessions begun after it, and that a project's own rules come first.
- **Delivery**, a choice of **Ask each time**, **Merge locally** and **Open a pull request**, Ask each time to begin
  with. Its description says that it is how a session delivers finished work when nobody has said how, that a change
  reaches only sessions begun after it, and that a project's own rules come first.

## Protection

- **Revert gate**, a switch, off to begin with, always shown whatever Where sessions develop holds. Its description
  says that in a project session a branch update that quietly undoes changes already on its target branch is blocked,
  and that a change takes effect at once, running sessions included. A "?" help tip at the end of its name ("About
  passing the revert gate") says what it blocks — usually what a merge or rebase whose conflicts were resolved wrongly
  leaves — and how a deliberate undo passes: made with `git revert`, or with a `Reverts: <sha>` line in the message of
  the commit that undoes it (see "A deliberate revert" in `docs/product/revert-gate.md`).
