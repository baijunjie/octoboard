# Settings

Settings is a large dialog over the whole window. A search field and, under it, a list of sections run down its left
side, and the selected section's settings fill the right under the section's heading, one row per setting: on the left
its name and a line of text under it saying what it does — a trusted folder's row has its path alone — and on the right
its control. Under a right-to-left language the sides swap (see `docs/product/right-to-left-layout.md`). A section that
explains itself as a whole does so in a paragraph under its heading, and no section heading has a "?" help tip; help
tips are on the **Automatically pull repositories** and **Revert gate** rows in Git (see "Automatically pull
repositories" and "Protection" in `docs/product/git-settings.md`), and in Agent accounts on the **Start usage windows
automatically** row (see "Start usage windows automatically" in `docs/product/starting-usage-windows.md`), on each
agent's Permission approval row, on the accounts table's Disable heading, after the default account's directory in its
account details dialog, and on the sharing row of an account's settings dialog (see "Approval modes" in
`docs/product/per-agent-settings.md`, "The accounts table", "The account details dialog" and "Sharing the default
configuration" in `docs/product/agent-accounts.md`, and "Explanations in help tips" in
`docs/product/labels-and-tooltips.md`). The sections, in order, are **General**, **App behavior**, **Agent
behavior**, **Agent accounts**, **Git**, **Trusted folders**, **Permissions**, **Notifications**, **Keyboard shortcuts**,
**Cleanup**, **Configuration** and **About**; the dialog opens on General unless it was asked to open on another
section. **Permissions** is there only in the macOS application; the Windows application and a browser leave it out. The
search field narrows the sections and their rows to the settings that match what is typed (see
`docs/product/searching-settings.md`).

A console session can read and change the default clone directory, the automatic pulling of repositories, the
automatic deletion of archived sessions whose conversation is gone, resuming sessions on start, closing idle shell
terminals with their panel, the application-wide git workflow settings and the language the daemon holds for the user
(see "The language the daemon holds" in `docs/product/language.md`). It can read, but not change, automatic account
selection, Claude compatibility, Codex compatibility, the agent tags, the agent accounts (without their directories) and the trusted
folders. The switches of Agent behavior, the approval modes, and the switches that switch accounts at the usage limit
and start usage windows, it can neither read nor change (see "Who reads and changes what" and "Application settings:
`get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md`).

## In a narrow window

At **768 px and below**, including a portrait iPad at exactly 768 px, the search field becomes the dialog's top row and
the list of sections a row of tabs under it, above the selected section. The dialog fills the available window area
without outer margins, rounded corners or a shadow. Connection and update notices at the bottom remain outside it (see
"What an open dialog, menu or popover covers" in `docs/product/window-layout.md`). When the tabs do not fit the row's
width, the row scrolls sideways, fading at the end with more beyond it (see "The surfaces that fade" in
`docs/product/scrolling-surfaces.md`), and the selected section's tab is scrolled into view when the dialog opens.

Above **768 px**, the sections stay in a vertical list in a column beside the selected section, the search field at the
column's top and a divider between the column and the section. When the dialog is too short to show every section,
the list of sections scrolls up and down on its own while the search field stays in place, fading at the edge with more
beyond it (see "The surfaces that fade" in `docs/product/scrolling-surfaces.md`), and the selected section's tab is
scrolled into view when the dialog opens. Below 1024 px the dialog keeps a 16
px outer margin; at **1024 px and wider** it keeps at least 48 px and is at most 1000 px wide. Every wider desktop
window uses this same presentation. The section's horizontal padding is 16 px below 1024 px and 32 px at 1024 px and
wider. Below **390 px**, the section padding becomes 12 px and the tab row's spacing tightens. This compact spacing
keeps the same single-panel interaction as the rest of the narrow range.

A setting row whose control is one small control of a bounded width — a switch, a single button, a status badge or a
short link, with a status mark beside it at most — keeps it beside its name and its line of text at every width. At 768
px and below any other control (a select, a text field, a group of controls, a URL) goes under the name and the text
instead. There it sits at the row's trailing edge (the left edge under a right-to-left language), its controls keeping
their order, unless it is wider than the row, when it starts from the leading edge instead. A field — a select, a text
field, the default clone directory's path field with its **Browse** button — fills the row's width instead, any button
beside it keeping its size. The same trailing edge holds the Keyboard shortcuts section's **Restore defaults** button,
which at 768 px and below goes under the section's introduction.

## Moving between sections

The list of sections is a tab list — vertical, or the row along the top in a narrow window — with the selected
section marked. Opening Settings puts keyboard focus on the selected section's tab (unless the Add agent dialog is
put in front; see "Opening and closing Settings" below). Only that tab is reached with Tab:
the arrow keys move to the previous or next section and show it at once, wrapping around at either end, and Tab from the
list moves on into the section's settings. In the row along the top only Left and Right move, mirrored under a
right-to-left language; in the vertical list Up and Down do too. When a control holding keyboard focus disappears from a
section — a trusted folder's Remove button or an account's actions menu, which goes with its folder or account once
the removal is confirmed, an agent's tab once the agent's removal is confirmed, the Notifications Enable button once
the browser has answered, or a Permissions row's button, which goes once
the permission is granted or its request has come back without the grant — focus goes back to the selected section's tab rather than
leaving the dialog. When anything else hides or removes the control holding keyboard focus — a search that no longer
matches its row once the language changes, or once a setting is changed from another window (see
`docs/product/searching-settings.md`) — focus goes to the search field, showing its keyboard focus ring. After a tap it
goes to the selected section's tab instead, so the on-screen keyboard does not come up, or to the dialog's close button
while the search matches nothing and the tabs are hidden. Focus that has already moved elsewhere in the dialog stays
there.

## Opening and closing Settings

Settings opens from:

- the **Settings** button at the bottom of the rail (see "The rail" in `docs/product/window-layout.md`);
- in the macOS application, the application menu's **Settings…** item, whose shortcut is Open settings' — `Cmd+,`
  unless the user has changed it (see `docs/product/keyboard-shortcuts.md`). The menu item does nothing
  while the window has nothing to show yet (before the daemon's state has first arrived), while Settings is already
  open, and while another dialog or an action menu — a row's or a header's, its submenus included — is open;
- where there is no native menu — the Windows application and a browser — the Open settings shortcut itself (`Ctrl+,`
  unless the user has changed it), which the page takes wherever keyboard focus is, the terminal included, so the key
  never reaches the terminal. It does nothing in the situations the macOS menu item does nothing in;
- in the macOS application, the application menu's **About Octoboard** item, which opens it on the About section
  (there is no separate About panel) and, while Settings is already open, moves it there; it does nothing in the
  situations the Settings… item does nothing in, except that an open Settings does not stop it;
- the **Manage accounts** button beside the title of the console dialog's Agent default accounts section, which opens
  it on the Agent accounts section above the console dialog, left open beneath it (see "Agent config directories" in
  `docs/product/consoles.md`);
- the **Manage accounts** button in the rail's account quota panel, which closes the panel and opens it on the Agent
  accounts section (see `docs/product/account-quota-panel.md`);
- the **Add agent** button of the "No agent added" states — the terminal's empty panel (see "A console with nothing
  in it" in `docs/product/session-terminal.md`) and the rail's account quota panel, which it closes first — which
  opens it on the Agent accounts section with the Add agent dialog in front (see "Adding an agent" in
  `docs/product/agent-accounts.md`);
- an error alert's **Add agent** or **Manage accounts** button, which acknowledges the alert and opens it the same
  way, above whatever dialog is open (see "Recovery and unexpected errors" in `docs/product/error-alerts.md`).

When the Add agent dialog is put in front, that dialog holds keyboard focus rather than the selected section's tab. A
request to open Settings on a section while it is already open moves it to that section and closes the account add,
edit, rename and remove forms, an account's details or settings dialog and the agent-removal form. It also closes the
Add agent dialog unless the request is for that dialog, and it clears the search (see "While a query is entered" in
`docs/product/searching-settings.md`). When the destination is not behind a login dialog, focus goes to the requested
section's tab, or, when Add agent is requested, to that dialog in front of the Agent accounts section. The login dialog
keeps its lifecycle (see `docs/product/logging-accounts-in.md`).

Settings closes on `Escape` — except in the search field while it holds text, where `Escape` clears it (see "The search
field" in `docs/product/searching-settings.md`) — on its close button, and on a press on the dimmed area around it.
Closing it puts keyboard focus back on the selected session's terminal, unless a dialog is open beneath it: focus then
stays in that dialog. It goes to the control in that dialog that opened Settings — the console dialog's Manage accounts
button, say — while that control is still there, even when the press gave it no focus; else to the control that dialog
opens on (see "Where keyboard focus starts in a form dialog" in `docs/product/dialogs.md`); and only else to the dialog
itself. Opened from the rail while the floating sidebar is open, it gives focus back to the rail instead (see "When the
sidebar cannot dock" in `docs/product/sidebar-drawer.md`).

With no dialog beneath it and no session's terminal to take focus — no session is selected, or Settings was opened
from the open sidebar overlay — focus is still handed to something. It goes, in order:

1. to the control whose press opened Settings, while that control is still on screen, even when the press gave it no
   focus — a screen reader's press on the terminal empty panel's **Add agent**, say, or on the rail's **Settings**
   button. Settings opened from the macOS application menu has no such control;
2. else to what held keyboard focus when Settings opened, while that is still there;
3. else to the sidebar, or, when the sidebar is not shown or has no control to take it, to the rail, each landing where
   `F6` would — where focus goes when the last error alert is acknowledged with nowhere to return to (see "The error
   alert" in `docs/product/error-alerts.md`).

While Settings is open everything under it stays as it was, the terminal included, and its size does not change.

## Group headings

A section whose rows fall into groups — General, App behavior, Git and Keyboard shortcuts — heads each group with a
heading of its own. Once the section is scrolled far enough for a group's heading to reach the top of the section's
area, the heading stays pinned there, opaque, and the group's rows scroll beneath it, until the next group's heading
arrives and pushes it out; the top fade then begins below the pinned heading (see "The edge that hides content fades" in
`docs/product/scrolling-surfaces.md`).

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
the full path. When the daemon cannot determine its home directory, every path is shown in full: the home it
settles on (`HOME` when absolute, else the user's entry in the system user database) is the filesystem root.

A path is shown in the spelling of the host the daemon runs on, as the daemon gives it. On a Windows daemon host that
is Windows' own (`C:\Users\dev\Projects\app`), and **no path is written with `~`**: every path is shown in full, cut
only where it does not fit. A `~` the user types for a directory is still expanded to the daemon host's home directory
there, followed by `\` or `/`.

## How switches save

A row's switch whose value the daemon holds saves when it is turned, on its own: in App behavior, **Resume sessions on
start**, **Prevent sleep** and **Close idle terminals when closing the panel**; in Git, **Automatically pull
repositories**, **Squash commits before delivery**, **Rebase before delivery** and **Revert gate**; in Agent accounts,
**Automatically select an account**, **Automatically switch accounts at the usage limit** and **Start usage windows
automatically**; the five of Agent behavior; and in Cleanup, **Automatically delete archived sessions whose conversation
is gone**. The switch always shows the value the daemon has stored, which every window shares: it moves when that value
changes, not on the press itself. Once a press has sent a save, further presses are ignored until the stored value shows
that save, or the save fails. Nothing marks that wait — the switch does not look unavailable — and it keeps showing the
stored value meanwhile. A failed save is shown in an error alert saying the setting could not be saved, the switch
keeping its position (see "The error alert" in `docs/product/error-alerts.md`). App behavior's **When to prevent
sleep** and the Git section's **Where sessions develop** and **Delivery** choices save the same way. Each save is
checked against the setting's version, so a change another window made just before it is refused rather than
overwritten, the error alert saying so and the control showing that change (see `docs/product/concurrent-edits.md`).

Other switches in Settings save by rules of their own: an account's Disable switch (see "Turning an account off" in
`docs/product/agent-accounts.md`), each agent's Claude compatibility and Codex compatibility switches, each unavailable
while its save is unanswered (see "Provider settings" in `docs/product/per-agent-settings.md`), the sharing switch of an account's
settings dialog (see "Sharing the default configuration" in `docs/product/agent-accounts.md`), **Keep running in
the background** and **Show menu bar icon**, choices the client keeps itself rather than the daemon, which take effect
as they are turned (see "Keep running in the background" and "Show menu bar icon" in
`docs/product/app-behavior-settings.md`), and **Launch at login**, which shows and changes the system's own state (see
"Launch at login" in the same document).

## General

Small app-wide settings: three rows, **Appearance**, **Language** and **Markdown style**, then the **Sorting** group
(see "Group headings" above).

### Appearance

The Light / Dark / System choice for the window's appearance, as a three-way segmented control showing each option as
an icon — a sun for Light, a moon for Dark, a monitor for System — named by its word. What each option means,
which one is the default and where the choice is kept are in `docs/product/appearance.md`.

### Language

The language of every window, and the one agents talk to the user in, chosen from a drop-down of the offered
languages, each named in its own language; the row's description says both. What the options are, how a language is
picked before one is known, what follows the choice, where it is kept, how every window follows it and how it becomes
the language sessions are told are in `docs/product/language.md`.

### Markdown style

How a rendered Markdown document in the project pane's file viewer looks, chosen from a drop-down of the styles:
**Tailwind**, **GitHub**, **Newsprint** and **LaTeX**. The styles are named for what they are modelled on, so their
names are proper names, the same in every language. What each style looks like, which one is the default and where the
choice is kept are in `docs/product/markdown-documents.md`.

### Sorting

How sessions and projects are ordered, as three selects, each taking effect as it is chosen: **Sessions** (Status or
Recently started), **Active projects**, the projects with at least one unarchived session, which always come before the
inactive ones (Status, Name or Recently used), and **Inactive projects**, the projects with none, which always come
after the active ones (Name or Recently used). What each choice does, the defaults, everything that follows the sort and
where it is kept are in "Choosing the sort" in `docs/product/order-of-projects-and-sessions.md`.

## App behavior

How the application behaves around its window, in four groups — **Startup**, **Window**, **Power** and **Terminals** —
each under a heading (see "Group headings" above), some of them only in the desktop application. Which rows each group
holds, where each is shown and what each does are in `docs/product/app-behavior-settings.md`.

## Agent behavior

What a session may do without asking the user first, and how it asks the user: four switches that approve a session's
requests automatically, then **Ask questions as choices**. The section and its rows are in
`docs/product/agent-behavior-settings.md`.

## Agent accounts

The agents the user has added and every account of each, grouped by agent, with the means to add, reorder and remove
agents, to add, edit, rename and remove accounts, to turn an account off and on, to sign an account in, each agent's
tags, permission approval, Claude compatibility and Codex compatibility, the version of each agent's CLI and its update, each account's
plan quota and each account's details, and the three
switches **Automatically select an account**, **Automatically switch accounts at the usage limit** and **Start usage
windows automatically**. Where those switches sit, and what the rest of the section shows and offers, are in
`docs/product/agent-accounts.md`. Each switch's row, default and storage are in "Automatically select an account" in
`docs/product/opening-sessions.md`, "Automatically switch accounts at the usage limit" in
`docs/product/automatic-account-switch.md` and "Start usage windows automatically" in
`docs/product/starting-usage-windows.md`. Every setting of the section is the user's alone: no agent changes one through the
settings tools, though a console session reads some of them (see "Who reads and changes what" in
`docs/product/agent-settings-tools.md`).

## Git

Three groups, each under a heading (see "Group headings" above): **Repository**, with the default clone directory and
automatic pulling of repositories; **Development**, with the application-wide git workflow settings for where sessions
develop and how they deliver; and **Protection**, with the revert gate. The rows are in `docs/product/git-settings.md`.

## Trusted folders

The folders under which Octoboard presses every agent's trust confirmation without asking, for every project — the
folder form of the trust permission all agents share — with a way to stop trusting each one. What the section's
opening paragraph says, what the list shows and what removing a folder does are in "Trusted folders" in
`docs/product/folder-trust.md`.

## Permissions

In the macOS application only: one row for each of the two macOS privacy permissions the agents can need, Screen
Recording and Accessibility, to grant it ahead of time. What the section shows and what each row offers are in
`docs/product/macos-privacy-permissions.md`.

## Notifications

Whether Octoboard may show a system notification when a session starts waiting for the user. What the section shows
and offers in each client is in `docs/product/notification-settings.md`.

## Keyboard shortcuts

Every keyboard shortcut of the client, grouped, with a way to change the app's own commands' keys, reset one, or
restore every default. The changes are this client's own, and a client lists only the commands it has. What the section
lists and how a shortcut is changed are in `docs/product/keyboard-shortcuts.md`.

## Cleanup

The cleanup Octoboard does on its own, one row: **Automatically delete archived sessions whose conversation is gone**.

### Automatically delete archived sessions whose conversation is gone

A switch, **on to begin with**: whether Octoboard deletes on
its own the archived sessions whose conversation their agent no longer keeps. While the switch is off the row's
description says what turning it on will do, and while it is on it says what is happening now: such an archived
session is deleted automatically, Octoboard checks each time it starts and then about once a day, and pinned and locked
sessions are kept. The value is kept by the daemon, so every window shows the same. What the deletion does, when it
runs and what turning the switch off or on changes are in "Deleting archived sessions whose conversation is gone" in
`docs/product/deleting-sessions.md`.

## Configuration

Two rows, each opening its own dialog: **Export configuration**, which saves consoles and projects, settings or
accounts as a file, and **Import configuration**, which creates consoles, projects and accounts, and applies settings,
from such a file (see `docs/product/configuration-export-and-import.md` and
`docs/product/configuration-export-and-import-dialogs.md`). A single console is exported from the one the sidebar shows,
which is the first while none is chosen. Both rows need the window to be able to save and open files, and where it
cannot the section holds a line saying that exporting and importing configuration files is not available here.

## About

The application itself, its updates, the project's links, its license, the third-party licenses and the copyright.
What the section shows, and the third-party licenses dialog it opens, are in `docs/product/about-octoboard.md`.
