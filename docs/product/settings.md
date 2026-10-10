# Settings

Settings is a large dialog over the whole window. A list of sections runs down its left side and the selected
section's settings fill the right, one row per setting: its name and a line saying what it does on the left, its
control on the right; under a right-to-left language the sides swap (see "Right-to-left layout" in
`docs/product/window-layout.md`). The sections, in order, are **General**, **Git**, **Agent accounts**, **Trusted
folders** and **Notifications**; the dialog opens on General unless it was asked to open on another section.

## In a narrow window

Below 1148 px — the width under which the window's panes become drawers (see "Below 1148 px: the sidebar and the right
pane become drawers" in `docs/product/window-layout.md`), so only a client with no window minimum of its own, such as a
plain browser, gets here — the dialog fills the window inside a 16 px margin on every side, and the list of sections
becomes a row of tabs along its top, above the selected section. When the tabs do not fit the row's width, the row
scrolls sideways, and the selected section's tab is scrolled into view when the dialog opens. While the connection
banner's strip is on screen the dialog keeps that 16 px clear of the strip's top edge instead of the window's bottom
edge, and is that much shorter (see "What an open dialog, menu or popover covers" in `docs/product/window-layout.md`).

Below 640 px each setting row puts its control under its name and description instead of beside them.

## Moving between sections

The list of sections is a tab list — vertical, or the row along the top in a narrow window — with the selected
section marked. Opening Settings puts keyboard focus on the selected section's tab. Only that tab is reached with Tab:
the arrow keys move to the previous or next section and show it at once, wrapping around at either end, and Tab from
the list moves on into the section's settings. In the row along the top only Left and Right move, mirrored under a
right-to-left language; in the vertical list Up and Down do too. When a control holding keyboard focus disappears
from a section — a trusted folder's or an account's Remove button, which goes with its folder or account, or the
Notifications Enable button once the browser has answered — focus goes back to the selected section's tab rather than
leaving the dialog.

## Opening and closing Settings

Settings opens from:

- the **Settings** button at the bottom of the rail (see "The rail" in `docs/product/window-layout.md`);
- in the macOS application, the application menu's **Settings…** item, shortcut `Cmd+,`. The menu item does nothing
  while the window has nothing to show yet (before the daemon's state has first arrived), while Settings is already
  open, and while another dialog or a row's action menu is open;
- a session row's **Switch account** submenu, whose last entry, **Manage accounts…**, opens it on the Agent accounts
  section instead of the first (see "Session rows" in `docs/product/sidebar.md`).

Settings closes on `Escape`, on its close button, and on a press on the dimmed area around it. Closing it puts
keyboard focus back on the selected session's terminal.

While Settings is open everything under it stays as it was, the terminal included, and its size does not change.

## How paths are shown

A directory path listed in Settings — a trusted folder, an account's directory, the directory an agent's default
account resolves to — is set apart from the text around it: a folder icon ahead of it and the path in a monospace
font. A path too long for its row is cut from its start and fades out there, so the directory's own name stays
visible; the icon stays in place.

A path inside the home directory of the host the daemon runs on — the daemon's, not the browser's, since the two may
be different machines — is shown with that home directory written as `~`: `~/Projects/app`, and `~` for the home
directory itself. A path that only shares its first characters with the home directory (`/Users/devx` against
`/Users/dev`) is shown in full. Whenever a path is shown shortened — written with `~`, cut, or both — its tooltip is
the full path. When the daemon cannot determine its home directory (`HOME` unset, relative or the filesystem root),
every path is shown in full.

## General

Small app-wide settings: an **Appearance** row, a **Language** row, then a **Default clone directory** row.

### Appearance

The Light / Dark / System choice for the window's appearance, as a three-way segmented control showing each option as
an icon — a sun for Light, a moon for Dark, a monitor for System — named by its word. What each option means,
which one is the default and where the choice is kept are in `docs/product/appearance.md`.

### Language

The language of the UI, chosen from a drop-down of the offered languages, each named in its own language. What the
options are, how the first launch picks a language, what follows the choice and where it is kept are in
`docs/product/language.md`.

### Default clone directory

A text field with a folder icon in it and a **Browse** button beside it (the directory browser, see "Browsing
directories" in `docs/product/consoles-and-projects.md`), showing the directory a repository is cloned into when a
project is added from a git URL and no other directory is named. It is `~/Projects` until the user sets one. The field
shows the directory in a monospace font, with the daemon host's home directory written as `~` under the same rules as
in "How paths are shown" above, and the full path as the field's tooltip whenever the two differ. The daemon stores
the directory expanded, as an absolute path without a trailing slash; a leading `~/` in what is sent is expanded
again. The value is sent when the field loses focus or Enter is pressed, and at once when a directory is picked; a
value equal to what the field shows unedited, or to the full path it stands for, sends nothing. Once a send is
answered the field shows the stored directory again, written as above. A value that is neither absolute nor starts
with `~/` is refused with a toast and the field returns to the stored one, and a blank value goes back to
`~/Projects`. The browser opens on the nearest existing ancestor when the directory does not exist yet (see
"Browsing directories" in `docs/product/consoles-and-projects.md`). Where the directory applies is in "Associating a
project" in the same document.

## Git

One row, **Automatically sync repositories**, a switch, off to begin with: whether a project's branch is
fast-forwarded on its own when it is behind its upstream, rather than only reported as behind. What it does, what it
never does and where the value is kept are in "Automatically syncing repositories" in
`docs/product/project-git-status.md`.

## Agent accounts

Every account, grouped by agent in the order the agents are listed elsewhere, each group headed by the agent's name and
icon. The section says what an account is — a named config directory of an agent, where it keeps its login and
conversation history — and that an agent has to be on the user's `PATH` for its accounts to be usable. An agent that is
not available is headed as not installed and its accounts are marked not usable; one whose availability is not yet
determined is headed plainly.

Each group starts with the agent's **default account**, said to be the setup a session runs under when nothing is
pinned, with the directory it currently resolves to. It has no actions: its name is Octoboard's own and it cannot be
edited or removed. The accounts the user owns follow, each with its name, its directory, an **Edit** button and a
**Remove** button. Both kinds of directory are shown as in "How paths are shown" above.

One **Add account** button, above the groups, serves every agent: its form asks for the agent — every agent can be
picked, one that is not installed included, since an account may be set up before its agent is — then a name and a
directory, which can be typed or picked with the directory browser (see "Browsing directories" in
`docs/product/consoles-and-projects.md`). Editing an account offers its name and its directory, never its agent: a
directory belongs to one agent's layout. The directory is checked for being absolute (or starting with `~/`) and for
nothing else: one that does not exist yet is accepted, and the form says that Octoboard does not require it to exist.
For a Grok Build account the form adds that the directory has to be a Grok home that Grok has already been run
against, which is checked when a session launches. Every account is named by the user, and the name is required: it is
stored trimmed, and one left blank is refused. A name has to be unique within its agent, compared trimmed and ignoring
letter case; accounts of different agents may share a name. A name already taken by another account of the same agent,
or by the default account — its name in the current language, and the English word "Default", which is reserved in
every language — is reported under the name field. Removing an account asks first and says that consoles referring to it go
back to the agent's default account and that sessions already open are not affected. What an account is and how
consoles and sessions use one are in "Agent config directories" in `docs/product/consoles-and-projects.md`.

## Trusted folders

The folders under which Octoboard presses every agent's trust confirmation without asking, for every project — the
folder form of the trust permission all agents share — with a way to stop trusting each one. What the list shows and
what removing a folder does are in "Trusted folders" in `docs/product/folder-trust.md`.

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
