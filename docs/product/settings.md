# Settings

Settings is a large dialog over the whole window. A list of sections runs down its left side and the selected
section's settings fill the right, one row per setting: its name and a line saying what it does on the left, its
control on the right; under a right-to-left language the sides swap (see "Right-to-left layout" in
`docs/product/window-layout.md`). The sections, in order, are **General**, **Git**, **Agent accounts**, **Trusted
folders**, **Permissions**, **Notifications** and **Configuration**; the dialog opens on General unless it was asked to
open on another section.

A console session can read the default clone directory, the automatic pulling of repositories, the automatic deletion of
archived sessions whose conversation is gone, the agent accounts (without their directories) and the trusted folders,
and change the first three (see "Application settings: `get_settings` and `update_settings`" in
`docs/product/agent-settings-tools.md`).

## In a narrow window

Below 1168 px — the width under which the window's panes become drawers (see "Below 1168 px: the sidebar and the right
pane become drawers" in `docs/product/window-layout.md`), so only a client with no window minimum of its own, such as a
plain browser, gets here — the dialog fills the window inside a 16 px margin on every side, and the list of sections
becomes a row of tabs along its top, above the selected section. When the tabs do not fit the row's width, the row
scrolls sideways, fading at the end with more beyond it (see "The surfaces that fade" in
`docs/product/scrolling-surfaces.md`), and the selected section's tab is scrolled into view when the dialog opens.
While the connection banner's strip is on screen the dialog keeps that 16 px clear of the strip's top edge instead of
the window's bottom edge, and is that much shorter (see "What an open dialog, menu or popover covers" in
`docs/product/window-layout.md`).

Below 640 px each setting row puts its control under its name and description instead of beside them.

## Moving between sections

The list of sections is a tab list — vertical, or the row along the top in a narrow window — with the selected
section marked. Opening Settings puts keyboard focus on the selected section's tab. Only that tab is reached with Tab:
the arrow keys move to the previous or next section and show it at once, wrapping around at either end, and Tab from
the list moves on into the section's settings. In the row along the top only Left and Right move, mirrored under a
right-to-left language; in the vertical list Up and Down do too. When a control holding keyboard focus disappears
from a section — a trusted folder's or an account's Remove button, which goes with its folder or account, the
Notifications Enable button once the browser has answered, or a Permissions row's button, which goes once the
permission is granted or its request has been answered — focus goes back to the selected section's tab rather than
leaving the dialog.

## Opening and closing Settings

Settings opens from:

- the **Settings** button at the bottom of the rail (see "The rail" in `docs/product/window-layout.md`);
- in the macOS application, the application menu's **Settings…** item, shortcut `Cmd+,`. The menu item does nothing
  while the window has nothing to show yet (before the daemon's state has first arrived), while Settings is already
  open, and while another dialog or an action menu — a row's or a header's, its submenus included — is open;
- a session row's **Switch account** submenu, whose last entry, **Manage accounts…**, opens it on the Agent accounts
  section instead of the first (see "Session rows" in `docs/product/sidebar.md`);
- the **Manage accounts…** button in the console dialog's Agent accounts table, which opens it on the Agent accounts
  section above the console dialog, left open beneath it (see "Agent config directories" in
  `docs/product/consoles.md`);
- the **Manage accounts…** button in the rail's account quota panel, which closes the panel and opens it on the Agent
  accounts section (see "The account quota panel on the rail" in `docs/product/account-quotas.md`).

Settings closes on `Escape`, on its close button, and on a press on the dimmed area around it. Closing it puts
keyboard focus back on the selected session's terminal, or, when it was opened from the console dialog, on the row of
the console dialog's Agent accounts table it was opened from.

While Settings is open everything under it stays as it was, the terminal included, and its size does not change.

## How paths are shown

A directory path listed in Settings — a trusted folder, an account's directory, the directory an agent's default
account resolves to — is set apart from the text around it: a folder icon ahead of it and the path in a monospace
font. A path too long for its row is cut from its start and fades out there rather than ending in an ellipsis, so the
directory's own name stays visible, and it runs as a marquee while the pointer is over it (see "Names too long for
their space" in `docs/product/labels-and-tooltips.md`); the icon stays in place.

A path inside the home directory of the host the daemon runs on — the daemon's, not the browser's, since the two may
be different machines — is shown with that home directory written as `~`: `~/Projects/app`, and `~` for the home
directory itself. A path that only shares its first characters with the home directory (`/Users/devx` against
`/Users/dev`) is shown in full. Whenever a path is shown shortened — written with `~`, cut, or both — its tooltip is
the full path. When the daemon cannot determine its home directory (`HOME` unset, relative or the filesystem root),
every path is shown in full.

## General

Small app-wide settings: an **Appearance** row, a **Markdown style** row, a **Language** row, a **Default clone
directory** row, then an **Automatically delete archived sessions whose conversation is gone** row, and last, in the
macOS application only, a **Keep running in the background** row.

### Appearance

The Light / Dark / System choice for the window's appearance, as a three-way segmented control showing each option as
an icon — a sun for Light, a moon for Dark, a monitor for System — named by its word. What each option means,
which one is the default and where the choice is kept are in `docs/product/appearance.md`.

### Markdown style

How a rendered Markdown document in the project pane's file viewer looks, chosen from a drop-down of the styles:
**Tailwind**, **GitHub**, **Newsprint** and **LaTeX**. The styles are named for what they are modelled on, so their
names are proper names, the same in every language. What each style looks like, which one is the default and where
the choice is kept are in "A Markdown file as a document" in `docs/product/project-pane.md`.

### Language

The language of the UI, chosen from a drop-down of the offered languages, each named in its own language. What the
options are, how the first launch picks a language, what follows the choice and where it is kept are in
`docs/product/language.md`.

### Default clone directory

A text field with a folder icon in it and a **Browse** button beside it (the directory browser, see "Browsing
directories" in `docs/product/projects.md`), showing the directory a repository is cloned into when a project is added
from a git URL and no other directory is named. It is `~/Projects` until the user sets one. The field shows the
directory in a monospace font, with the daemon host's home directory written as `~` under the same rules as in "How
paths are shown" above, and the full path as the field's tooltip whenever the two differ. The daemon stores the
directory expanded, as an absolute path without a trailing slash; a leading `~/` in what is sent is expanded again. The
value is sent when the field loses focus or Enter is pressed, and at once when a directory is picked; a value equal to
what the field shows unedited, or to the full path it stands for, sends nothing. `Escape` while the field holds keyboard
focus closes Settings and discards what was typed. It is the only exit that loses it: the close button, a press on the
dimmed area around the dialog, and moving focus out of the field all save the typed path first. Once a send is answered
the field shows the stored directory again, written as above. A value that is neither absolute nor starts with `~/` is
refused with an error alert and the field returns to the stored one, and a blank value goes back to `~/Projects`. The
browser opens on the nearest existing ancestor when the directory does not exist yet (see "Browsing directories" in
`docs/product/projects.md`). Where the directory applies is in "Associating a project" in the same document.

### Automatically delete archived sessions whose conversation is gone

A switch, **on to begin with** — installations from before the switch existed included: whether Octoboard deletes on
its own the archived sessions whose conversation their agent no longer keeps. While the switch is off the row's
description says what turning it on will do, and while it is on it says what is happening now: such an archived
session is deleted automatically, Octoboard checks each time it starts and then about once a day, and pinned and locked
sessions are kept. The value is kept by the daemon, so every window shows the same. What the deletion does, when it
runs and what turning the switch off or on changes are in "Deleting archived sessions whose conversation is gone" in
`docs/product/deleting-sessions.md`.

### Keep running in the background

A switch, the section's last row, labelled for what turning it on does: keep Octoboard running in the background when
the window is closed. It is **off to begin with**, so closing the window quits.

**Only the macOS application has this row.** A browser has no native window to close into the background, so the row
is absent there — the section then ends with the Automatically delete archived sessions whose conversation is gone
row.

The row's description says what the choice means and never mentions quitting, which the label already carries: while
the switch is off it says what turning it on will do, and while it is on it says what is happening now — closing the
window leaves the sessions, their agents and their notifications running, and the menu bar icon brings the window
back.

What closing the window actually does under either state, and where the choice is kept, are in "Closing the window"
in `docs/product/application-lifecycle.md`.

## Git

One row, **Automatically pull repositories**, a switch, off to begin with: whether a project's branch is pulled
(fast-forwarded) on its own when it is behind its upstream, rather than only reported as behind. What it does, what it
never does and where the value is kept are in "Automatically pulling repositories" in
`docs/product/project-git-status.md`.

## Agent accounts

Every account, grouped by agent. The section says what an account is — a named config directory of an agent, where it
keeps its login and conversation history — and that an agent has to be on the user's `PATH` for its accounts to be
usable.

Beneath that, each agent has a tab of its own, in the order the agents are listed elsewhere, each tab showing the
agent's icon and name; only the selected agent's accounts are shown, in its tab's panel. The agent tabs are an
underlined strip, set apart from the list of Settings' sections, and scroll sideways when they do not fit. Once the
section is scrolled far enough for the strip to reach the top of the section's area, at any window width, the strip
stays pinned there, opaque, and the accounts scroll beneath it; the top fade then begins below the strip (see "The
edge that hides content fades" in `docs/product/scrolling-surfaces.md`). Every agent keeps its tab whether or not it
is installed. The section always opens on the first agent's tab, Claude Code, however
it was opened. An agent that is not available has a muted line at the top of its panel saying it is not installed, and
its accounts are marked not usable; one whose availability is not yet determined has no such line.

Each agent's panel starts with the agent's **default account**, with the directory it currently resolves to. A "?"
ends its name line, after the plan, and says in its tooltip, or in a popover once pressed, that the default account is
the setup a session runs under when nothing is pinned. Its name is Octoboard's own; its name and directory cannot be
edited, and it cannot be removed. The accounts the user owns follow, each with its name, its directory, an **Edit**
button and a **Remove** button. Both kinds of directory are shown as in "How paths are shown" above.

Every account of an available agent, the default account included, also shows its plan quota: the plan beside its
name, and beneath the account's row, across its whole width, one line and bar per quota window — or, in its place,
that the quota is still being read, is unavailable, or is out of date. Opening the section asks for a fresh reading
of every account whose last one is due, those of every agent and not only of the tab in view; the readings already
held are shown at once. What is read, when, and how each state looks are in `docs/product/account-quotas.md`.

Every Codex account row, including the default account, has an inline approval-mode selector:

- **Follow account configuration** leaves the reviewer to that account's Codex configuration. This is the initial
  choice for both new and existing accounts; automatic review is never enabled just by adding an account.
- **Automatic review** asks Codex to review approval requests itself and may use additional tokens.
- **Ask me** asks Codex to send approval requests to the user.

The choice is saved when selected, separately from editing the account's name or directory. Choices are unavailable
while saving; a failure is shown in an error alert. It applies when a session next starts or resumes, including an account switch;
a running process keeps its current reviewer. A named account's choice is independent of the default account's:
following configuration does not inherit the default account's selector. If a session's named account was deleted,
or it holds a pinned directory without a stored account reference, it follows that original directory's Codex
configuration instead of adopting the default account's choice. Grok Build has no such selector.

These choices are stored by Octoboard. An explicit reviewer overrides Codex's reviewer for that launch without
writing its configuration files or changing its sandbox or approval policy. They remain local; see
"What is not configuration and never travels" in `docs/product/configuration-export-and-import.md`.

Every Claude Code account row, including the default account, has the same kind of selector with Claude Code's four
choices:

- **Follow account configuration** passes nothing, so the account's own `settings.json` decides. This is the initial
  choice for new and existing accounts.
- **Automatic** launches Claude Code in its auto mode, where a classifier approves what it judges safe and blocks the
  rest. Whether auto mode is available to the account, which depends on its plan among other things, is Claude Code's
  own decision; Octoboard only asks for it.
- **Ask me** launches Claude Code so that every tool call not already allowed asks the user.
- **Approve everything** launches Claude Code so that every tool runs without asking.

The choice is saved and applies exactly as Codex's does, a resume and an account switch included, and it stays local
too. **Approve everything** is saved only once the user accepts a destructive confirmation that names the account (or
the default account) and warns that Claude Code will run every tool in that account's sessions without asking,
commands that change or delete files included; its confirm button carries the choice's own name. Cancelling, Escape
included, leaves the choice as it was. If the save fails, the error is shown in the confirmation, which stays open, and
the choice stays unchanged. Because the user has agreed there, Claude Code's own blocking warning for that mode never
appears (see "Per-agent specifics a user will notice" in `docs/product/launching-agents.md`).

One **Add account** button, above the agent tabs, serves every agent: its form asks for the agent — every agent is
listed and can be picked, one that is not installed included and labelled as not installed, since an account may be set
up before its agent is, unlike the agent pickers elsewhere (see "Agents that are not installed" in
`docs/product/consoles.md`) — then a name and a directory, which can be typed or picked with the directory browser (see
"Browsing directories" in `docs/product/projects.md`). Editing an account offers its name and its directory, never its
agent: a directory belongs to one agent's layout. The directory is checked for being absolute (or starting with `~/`)
and for not being already used by another account of the same agent, the default account included (the full rules are in
"Agent config directories" in `docs/product/consoles.md`); one that does not exist yet is accepted, and the form says
that Octoboard does not require it to exist. A directory already in use is reported, once saving is attempted, under the
directory field, naming the account that uses it — the default account by its name in the current language; the report
goes away as soon as the directory is changed, typed or picked, or another agent is picked. For a Grok Build account the
form adds that the directory has to be a Grok home that Grok has already been run against, which is checked when a
session launches. Every account is named by the user, and the name is required: it is stored trimmed, and one left blank
is refused. A name has to be unique within its agent, compared trimmed and ignoring letter case; accounts of different
agents may share a name. A name already taken by another account of the same agent, or by the default account — its name
in the current language, and the English word "Default", which is reserved in every language — is reported under the
name field. Removing an account asks first and says that consoles referring to it go back to the agent's default account
and that sessions already open are not affected. A project referring to it is put back to inheriting its console's
reference at the same time (see "The project's account, model and reasoning effort" in `docs/product/projects.md`). What
an account is and how consoles, projects and sessions use one are in "Agent config directories" in
`docs/product/consoles.md`.

## Trusted folders

The folders under which Octoboard presses every agent's trust confirmation without asking, for every project — the
folder form of the trust permission all agents share — with a way to stop trusting each one. What the list shows and
what removing a folder does are in "Trusted folders" in `docs/product/folder-trust.md`.

## Permissions

The two macOS privacy permissions the agents running in Octoboard's terminals can need: **Screen Recording**, which
agents use to take screenshots of what they built, and **Accessibility**, which lets them drive another application's
interface. macOS attributes such a request to the application responsible for the process that made it, which is
Octoboard rather than the agent, so the system's own prompt names Octoboard, appears in the middle of a task and says
nothing about why — and Octoboard cannot intercept a prompt one of its agents raised. This section is therefore the
place to grant them deliberately beforehand, and it opens with a paragraph saying as much. A grant the user gave
another application does not count for Octoboard; macOS ties a grant to one application. These two are the only macOS
permissions Octoboard ever asks for.

One row per permission, naming it, what agents use it for and its current state. What the row offers follows that
state:

| State | What the row shows |
|---|---|
| Granted | That it is granted, and no control. |
| Not granted | That it is not granted, with a **Grant…** button. Pressing it raises the system's permission prompt; this is the only thing in Octoboard that ever raises one. |
| A request made in this window came back without the grant | That it is not granted and where to switch it on, with an **Open System Settings** button in place of Grant…, which opens that permission's own pane in System Settings. |

The wording of that last state differs per permission: for Screen Recording it says macOS will not ask a second time,
which is what macOS does — a prompt the user refused is never raised again, and a later request is answered with a
silent no; for Accessibility it says to switch Octoboard on in System Settings, since that permission is never granted
in place at all.

**Accessibility is granted in System Settings, not from the prompt.** Its Grant… button raises the system's dialog
asking to let Octoboard control the computer, whose effect is that Octoboard appears in System Settings' own list, so
the user only flips a switch there instead of adding the application by hand. The request reports back at once, while
that dialog is still coming up, and normally reports the permission not granted, so the row moves to the System Settings
wording straight after the press. Screen Recording's Grant… button instead waits for the user to answer the prompt, and
is marked pending until they have.

**A change made outside Octoboard is picked up when the window comes back.** The grants are read again every time the
window regains focus, so one granted or revoked in System Settings — the only way Accessibility is ever granted — takes
effect in this section on the user's return. Each row's state is announced together with the permission's name, so a
change that happened elsewhere is not silent for assistive technology.

**Screen Recording reaches a process only at launch.** A grant made while Octoboard is already running does not reach
it, so a row that is granted but was not when this Octoboard started adds a line asking the user to restart Octoboard
for it to take effect. There is no restart button: the user quits Octoboard and opens it again, which goes through the
ordinary quit (see "Quitting" in `docs/product/application-lifecycle.md`).

**Nothing about these permissions is stored.** Every row's state comes from macOS alone. macOS cannot tell a permission
that was never asked for from one the user refused, so a permission that is not granted starts at not granted with
the Grant… button, and the System Settings wording appears only after a request made in this window has come back
without the grant. Switching to another Settings section and back puts the Grant… button back; pressing it brings the
guidance back with it —
answered by macOS with a silent no for Screen Recording, and with that dialog again for Accessibility, which macOS
keeps no refusal for.

**Outside the macOS application** — in a browser, or a desktop shell not running on macOS — the section is a single
line saying that permissions can only be managed in the macOS application. Where a desktop shell is there but the read
of the grants fails, it says instead that Octoboard could not read which permissions it has been granted. Until the
first read comes back the section shows neither line and nothing else either.

## Notifications

Whether Octoboard may show a system notification when a session starts waiting for the user (see "The raised hand" in
`docs/product/sessions.md`). The row says what the current state is:

| Client | What the row shows |
|---|---|
| The macOS application | That Octoboard posts notifications directly, and that they are turned off in the system's own notification settings. No permission prompt is ever shown, and there is nothing to press. |
| A browser, not yet decided | That notifications are not enabled yet, with an **Enable** button that asks the browser. |
| A browser, allowed | That they are allowed. |
| A browser, blocked | That they are blocked, and that they are allowed again from the browser's own settings for the site. |
| A client with no notifications at all | That notifications are not available there. |

A browser is asked only from a press of the user's, never on its own: a browser ignores or refuses an ask made
otherwise. Besides the Enable button here, a browser that has not decided yet gets the same ask as the **Turn on
notifications** button on the rail: a bell icon with a small dot on it, shown only while the browser's answer is
undecided (see "The rail" in `docs/product/window-layout.md`). It cannot be dismissed; pressing it asks the
browser. Once the browser has answered, whichever the answer, the bell is gone, and answering from either place
updates the other. The macOS application never shows the bell, since it asks nothing. A browser's state is read
again whenever the window regains focus, since it can be changed in the browser's own settings meanwhile.

## Configuration

Two rows, each opening its own dialog: **Export configuration**, which saves a console, or every console with the
application settings, as a file, and **Import configuration**, which creates consoles and projects from such a file
(see `docs/product/configuration-export-and-import.md`). A console is exported from the one the sidebar shows, which is
the first while none is chosen. Both rows need the window to be able to save and open files, and where it cannot the
section holds a line saying that exporting and importing configuration files is not available here.
