# Settings

Settings is a large dialog over the whole window. A list of sections runs down its left side and the selected section's
settings fill the right under the section's heading, one row per setting: on the left its name and a line of text under
it saying what it does — a trusted folder's row has its path alone — and on the right its control. Under a right-to-left
language the sides swap (see `docs/product/right-to-left-layout.md`). A section that explains itself as a whole does so
in a paragraph under its heading, and no section heading has a "?" help tip; help tips are on the **Automatically pull
repositories** and **Revert gate** rows in Git (see "Git" below), and in Agent accounts on the **Start usage windows
automatically** row (see "Start usage windows automatically" below), on each agent's Permission approval row, on the
accounts table's Disable heading, after the default account's directory in its account details dialog, and on the
sharing row of an account's settings dialog (see "Approval modes" in `docs/product/per-agent-settings.md`, "The
accounts table", "The account details dialog" and "Sharing the default configuration" in
`docs/product/agent-accounts.md`, and "Explanations in help tips" in `docs/product/labels-and-tooltips.md`). The
sections, in order, are **General**, **Git**, **Agent accounts**, **Agent behavior**, **Trusted folders**,
**Permissions**, **Notifications**, **Keyboard shortcuts**, **Configuration** and **About**; the dialog opens on
General unless it was asked to open on another section.

A console session can read the default clone directory, the automatic pulling of repositories, the automatic deletion of
archived sessions whose conversation is gone, automatic account selection, the ordering of the sidebar by status, the
application-wide git workflow settings, the agent accounts (without their directories) and the trusted folders, and
change all but the last two (see "Application settings: `get_settings` and `update_settings`" in
`docs/product/agent-settings-tools.md`).

## In a narrow window

Below 1168 px — the width under which the sidebar becomes a drawer (see "Below 1168 px: the sidebar becomes a drawer"
in `docs/product/sidebar-drawer.md`), so only a client with no window minimum of its own, such as a
plain browser, gets here — the dialog fills the window inside a 16 px margin on every side, and the list of sections
becomes a row of tabs along its top, above the selected section. When the tabs do not fit the row's width, the row
scrolls sideways, fading at the end with more beyond it (see "The surfaces that fade" in
`docs/product/scrolling-surfaces.md`), and the selected section's tab is scrolled into view when the dialog opens.
While the connection banner's strip is on screen the dialog keeps that 16 px clear of the strip's top edge instead of
the window's bottom edge, and is that much shorter (see "What an open dialog, menu or popover covers" in
`docs/product/window-layout.md`).

Below 640 px each setting row puts its control under its name and its line of text instead of beside them.

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
the permission is granted or its request has been answered — focus goes back to the selected section's tab rather than
leaving the dialog.

## Opening and closing Settings

Settings opens from:

- the **Settings** button at the bottom of the rail (see "The rail" in `docs/product/window-layout.md`);
- in the macOS application, the application menu's **Settings…** item, whose shortcut is Open settings' — `Cmd+,`
  unless the user has changed it (see `docs/product/keyboard-shortcuts.md`). The menu item does nothing
  while the window has nothing to show yet (before the daemon's state has first arrived), while Settings is already
  open, and while another dialog or an action menu — a row's or a header's, its submenus included — is open;
- in the macOS application, the application menu's **About Octoboard** item, which opens it on the About section
  (there is no separate About panel) and, while Settings is already open, moves it there; it does nothing in the
  situations the Settings… item does nothing in, except that an open Settings does not stop it;
- the **Manage accounts** button beside the title of the console dialog's Agent default accounts section, which opens
  it on the Agent accounts section above the console dialog, left open beneath it (see "Agent config directories" in
  `docs/product/consoles.md`);
- the **Manage accounts…** button in the rail's account quota panel, which closes the panel and opens it on the Agent
  accounts section (see `docs/product/account-quota-panel.md`);
- the **Add agent…** button of the "No agent added" states — the terminal's empty panel (see "A console with nothing
  in it" in `docs/product/session-terminal.md`) and the rail's account quota panel, which it closes first — which
  opens it on the Agent accounts section with the Add agent dialog in front (see "Adding an agent" in
  `docs/product/agent-accounts.md`);
- an error alert's **Add agent…** or **Manage accounts…** button, which acknowledges the alert and opens it the same
  way, above whatever dialog is open (see "Recovery and unexpected errors" in `docs/product/error-alerts.md`).

When the Add agent dialog is put in front, that dialog holds keyboard focus rather than the selected section's tab. A
request to open Settings on a section while it is already open moves it to that section and closes the account add,
edit, rename and remove forms, an account's details or settings dialog and the agent-removal form. It also closes the
Add agent dialog unless the request is for that dialog. When the destination is not behind a login dialog, focus goes
to the requested section's tab, or, when Add agent is requested, to that dialog in front of the Agent accounts section.
The login dialog keeps its lifecycle (see `docs/product/logging-accounts-in.md`).

Settings closes on `Escape`, on its close button, and on a press on the dimmed area around it. Closing it puts
keyboard focus back on the selected session's terminal, unless a dialog is open beneath it: focus then stays in that
dialog. It goes to the control in that dialog that opened Settings — the console dialog's Manage accounts button,
say — while that control is still there, even when the press gave it no focus; else to the control that dialog opens
on (see "Where keyboard focus starts in a form dialog" in `docs/product/dialogs.md`); and only else to the dialog
itself. Opened from the rail while the sidebar's drawer is open, it gives focus back to the
rail instead (see "Below 1168 px: the sidebar becomes a drawer" in `docs/product/sidebar-drawer.md`).

With no dialog beneath it and no session's terminal to take focus — no session is selected, or Settings was opened
from the open drawer — focus is still handed to something. It goes, in order:

1. to the control whose press opened Settings, while that control is still on screen, even when the press gave it no
   focus — a screen reader's press on the terminal empty panel's **Add agent…**, say, or on the rail's **Settings**
   button. Settings opened from the macOS application menu has no such control;
2. else to what held keyboard focus when Settings opened, while that is still there;
3. else to the sidebar, or, when the sidebar is not shown or has no control to take it, to the rail, each landing where
   `F6` would — where focus goes when the last error alert is acknowledged with nowhere to return to (see "The error
   alert" in `docs/product/error-alerts.md`).

While Settings is open everything under it stays as it was, the terminal included, and its size does not change.

## Group headings

A section whose rows fall into groups — Git and Keyboard shortcuts — heads each group with a heading of its own. Once
the section is scrolled far enough for a group's heading to reach the top of the section's area, the heading stays
pinned there, opaque, and the group's rows scroll beneath it, until the next group's heading arrives and pushes it out;
the top fade then begins below the pinned heading (see "The edge that hides content fades" in
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

## General

Small app-wide settings: an **Appearance** row, a **Language** row, a **Markdown style** row, an **Order the sidebar
by status** row, then an **Automatically delete archived sessions whose conversation is gone** row, and last, in the
macOS application only, a **Keep running in the background** row.

### Appearance

The Light / Dark / System choice for the window's appearance, as a three-way segmented control showing each option as
an icon — a sun for Light, a moon for Dark, a monitor for System — named by its word. What each option means,
which one is the default and where the choice is kept are in `docs/product/appearance.md`.

### Language

The language of the UI, chosen from a drop-down of the offered languages, each named in its own language. What the
options are, how the first launch picks a language, what follows the choice, where it is kept and how a pick also
becomes the language sessions are told are in `docs/product/language.md`.

### Markdown style

How a rendered Markdown document in the project pane's file viewer looks, chosen from a drop-down of the styles:
**Tailwind**, **GitHub**, **Newsprint** and **LaTeX**. The styles are named for what they are modelled on, so their
names are proper names, the same in every language. What each style looks like, which one is the default and where the
choice is kept are in `docs/product/markdown-documents.md`.

### Order the sidebar by status

A switch, **on to begin with**: whether the sidebar ranks
sessions and projects by their status, so that the ones waiting for the user come first after the pinned ones. While
the switch is on the row's description says what is happening now: sessions and projects move as their status changes,
and projects with no unarchived session end the list. While it is off the description says what turning it on will
do, and that projects with no unarchived session end the list. The daemon stores it as `sidebar_order_by_status` and
keeps it, so every window shows the same. A console session may read and change it, and a configuration export that
includes the application settings carries it (see "Application settings: `get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md` and "The application settings an export carries" in
`docs/product/configuration-export-and-import.md`). The order under either state, and what else follows it, are in
"Order of projects and sessions" in `docs/product/order-of-projects-and-sessions.md`.

### Automatically delete archived sessions whose conversation is gone

A switch, **on to begin with**: whether Octoboard deletes on
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
row. In the macOS application this row sits directly under that switch.

The row's description says what the choice means and never mentions quitting, which the label already carries: while
the switch is off it says what turning it on will do, and while it is on it says what is happening now — closing the
window leaves the sessions, their agents and their notifications running, and the menu bar icon brings the window
back.

What closing the window actually does under either state, and where the choice is kept, are in "Closing the window"
in `docs/product/application-lifecycle.md`.

## Git

Three groups, each under a heading (see "Group headings" above): **Repository**, holding **Default clone directory** and
then **Automatically pull repositories**; **Development**, holding **Where sessions develop**, **Squash commits before
delivery**, **Rebase before delivery** and **Delivery** (see "Development" below); and **Protection**, holding **Revert
gate** (see "Protection" below).

### Default clone directory

A text field with a folder icon in it and a **Browse** button beside it (the directory browser, see "Browsing
directories" in `docs/product/projects.md`), showing the directory a repository is cloned into when a project is added
from a git URL and no other directory is named. It is `~/Projects` until the user sets one. The field shows the
directory in a monospace font, with the daemon host's home directory written as `~` under the same rules as in "How
paths are shown" above, and the full path as the field's tooltip, while the pointer is on the field, whenever the two
differ; the tooltip is in monospace like the field and reads left to right in every language (see "How a tooltip
behaves" in
`docs/product/labels-and-tooltips.md`). The daemon stores the
directory expanded, as an absolute path without a trailing slash; a leading `~/` in what is sent is expanded again. The
value is sent when the field loses focus or Enter is pressed, and at once when a directory is picked; a value equal to
what the field shows unedited, or to the full path it stands for, sends nothing. `Escape` while the field holds keyboard
focus closes Settings and discards what was typed. It is the only exit that loses it: the close button, a press on the
dimmed area around the dialog, and moving focus out of the field all save the typed path first. Once a send is answered
the field shows the stored directory again, written as above. A value that is neither absolute nor starts with `~/` is
refused with an error alert and the field returns to the stored one, and a blank value goes back to `~/Projects`. The
browser opens on the nearest existing ancestor when the directory does not exist yet (see "Browsing directories" in
`docs/product/projects.md`). Where the directory applies is in "Associating a project" in the same document.

### Automatically pull repositories

A switch, off to begin with: whether a project's branch is pulled (fast-forwarded) on its own when it is behind its
upstream, rather than only reported as behind. The row's line of text follows the switch — while it is off, what
turning it on will do; while it is on, what is happening now — and either way says that a branch is pulled when it is
behind its upstream and can be fast-forwarded, that nothing is pushed, and that a branch with commits of its own is
left alone; a "?" at the end of its name says that the remote is checked every few minutes whichever way the switch is
set, so the branch always shows how far ahead or behind it is. What it does, what it never does and where the value is
kept are in "Automatically pulling repositories" in `docs/product/project-git-status.md`.

### Development

The Development and Protection rows are the application-wide git workflow settings. What each one does, its values and
its default are in `docs/product/git-workflow-settings.md`, and a project can hold its own value for any of them (see
"A project's own git workflow settings" there). A change applies that one item alone. The daemon keeps the values, so
every window shows the same. As with the other switches here, while a switch is off its row's description says what
turning it on will do, and while it is on it says what is happening now.

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
- **Squash commits before delivery**, a switch, off to begin with. Its description says that commits are consolidated
  into independently meaningful, revertible commits before delivering finished work by merging, pushing or opening a
  pull request, before any rebase, that a change reaches only sessions begun after it, and that a project's own rules
  come first.
- **Rebase before delivery**, a switch, on to begin with. Its description says that a branch is rebased onto its target
  branch before delivering finished work by merging, pushing or opening a pull request, that a change reaches only
  sessions begun after it, and that a project's own rules come first.
- **Delivery**, a choice of **Ask each time**, **Merge locally** and **Open a pull request**, Ask each time to begin
  with. Its description says that it is how a session delivers finished work when nobody has said how, that a change
  reaches only sessions begun after it, and that a project's own rules come first.

### Protection

- **Revert gate**, a switch, off to begin with, always shown whatever Where sessions develop holds. Its description
  says that in a project session a branch update that quietly undoes changes already on its target branch is blocked,
  and that a change takes effect at once, running sessions included. A "?" help tip at the end of its name ("About
  passing the revert gate") says what it blocks — usually what a merge or rebase whose conflicts were resolved wrongly
  leaves — and how a deliberate undo passes: made with `git revert`, or with a `Reverts: <sha>` line in the message of
  the commit that undoes it (see "A deliberate revert" in `docs/product/revert-gate.md`).

A console session may read and change all five git workflow settings, and a configuration export that includes the
application settings carries them (see "Application settings: `get_settings` and `update_settings`" in
`docs/product/agent-settings-tools.md` and "The application settings an export carries" in
`docs/product/configuration-export-and-import.md`).

## Agent accounts

The agents the user has added and every account of each, grouped by agent, with the means to add, reorder and remove
agents, to add, edit, rename and remove accounts, to turn an account off and on, to sign an account in, each agent's
permission approval and Claude compatibility, each account's plan quota and each account's details, and the three switches **Automatically
select an account**, **Automatically switch accounts at the usage limit** and **Start usage windows automatically**.
Where those switches sit, and what the rest of the section shows and offers, are in `docs/product/agent-accounts.md`.

### Automatically select an account

A switch, **off to begin with**: whether a new session whose account was not named automatically chooses a usable
account, preferring the account of its initial agent whose weekly quota resets soonest and trying another added agent
only when that agent has no usable account. While the switch is off the row's description says what turning it on will
do, and while it is on it says what is happening now: a new session uses the account whose weekly limit resets soonest,
so usage left before a reset is not wasted. The daemon stores it as `auto_select_account` and keeps it, so every window
shows the same. Unlike the switches in Agent behavior, a console session may read and change it, and a configuration
export that includes the application settings carries it (see "Application settings: `get_settings` and
`update_settings`" in `docs/product/agent-settings-tools.md` and "The application settings an export carries" in
`docs/product/configuration-export-and-import.md`). Which account is chosen, and who applies the choice, are in "Which
account a session uses" in `docs/product/opening-sessions.md`. Turning it on or off changes no session already open. It
governs the initial agent and account of new sessions; explicit choices still win, and it does not move a session that
is already running (see "Automatically switch accounts at the usage limit" below).

### Automatically switch accounts at the usage limit

A switch, **off to begin with**: whether Octoboard switches a session to another account of its agent by itself when the
session's turn stops on its account's usage limit, so that it carries on there. While the switch is off the row's
description says that, once it is on, a session that stops at its account's usage limit is switched to another account
with usage left and carries on; while it is on, it says that this is what happens. The daemon stores it as
`auto_switch_on_usage_limit` and keeps it, so every window shows the same. A console session can neither read nor change
it, since it ends sessions' processes and spends another account's quota without asking (see "Who reads and changes
what" in `docs/product/agent-settings-tools.md`); a configuration export that includes the application settings carries
it (see "The application settings an export carries" in `docs/product/configuration-export-and-import.md`). Turning it
on does not act on sessions
already stopped. Turning it off cancels waits for quota recovery; turning it back on does not restore them. While it is
on, a turn that stops on the usage limit with no eligible alternative account can continue after a fresh successful
quota reading, on another account of its agent or on its original account (see "When no account qualifies" in
`docs/product/automatic-account-switch.md`). It has no bearing on which account a new session takes, nor on whether an
owner's request to switch an account asks the user (see "Automatically approve account switches" below). Which sessions
are switched, to which account, and who is told, are in `docs/product/automatic-account-switch.md`.

### Start usage windows automatically

A switch, **off to begin with**: whether Octoboard starts a
usage window that has reset but not begun counting by sending one small request on its account. While the switch is off
the row's description says that once it is on, a usage window starts counting once it resets, and while it is on that a
usage window starts counting once it resets; either way it adds that this counts a little toward the account's usage
limits. A "?" help tip at the end of the row's name ("About starting usage windows automatically") holds the why and
how: a usage window that has reset does not start counting until the account's next request, so until then it shows no
countdown and the longer it waits the later its next reset comes; with the switch on, one small request is sent on that
account with the agent's cheapest model to start it. The daemon stores it as `auto_start_quota_windows` and keeps it, so
every window shows the same. A console session can neither read nor change it, since it spends the user's quota (see
"Who reads and changes what" in `docs/product/agent-settings-tools.md`); a configuration export that includes the
application settings carries it (see "The application settings an export carries" in
`docs/product/configuration-export-and-import.md`). When a request
is sent, what it is and what it leaves behind are in `docs/product/starting-usage-windows.md`.

## Agent behavior

What a session may do without asking the user first, and how it asks the user. A paragraph under the heading says as
much, above five rows: **Automatically approve session requests**, **Automatically approve resume requests**,
**Automatically approve account switches**, **Automatically approve handoffs**, then **Ask questions as choices**. The
section is the same in every client. Each switch is kept by the daemon, so every window shows the same, and besides its
row only an import asked to apply a configuration document's application settings changes it (see "The application
settings an export carries" in `docs/product/configuration-export-and-import.md`); turning one on or
off leaves the others as they are. What an automatic approval does, what turning one of the four approval switches on
changes, and why agents cannot change any of them are in "Approving requests automatically" in
`docs/product/session-requests.md`.

### Automatically approve session requests

A switch, **off to begin with**: whether a session's
`request_session` call, its request for a session that is nobody's, is approved without asking the user. While the
switch is off the row's description says what turning it on will do, and while it is on it says what is happening now:
a session can start a session that reports to the user without asking first, and nobody but the user owns the new
session.

### Automatically approve resume requests

A switch, **off to begin with**: whether a session's
`request_resume_session` call, its request to resume a conversation, is approved without asking the user (see
`docs/product/requests-to-resume-a-conversation.md`). While the switch is off the row's description says what turning
it on will do, and while it is on it says what is happening now: a session can reopen or resume a session, or resume a
conversation in a new session, without asking first; a reopened or resumed session keeps its owner and has its
archived owners reopened with it; and a new session started this way is owned by nobody but the user.

### Automatically approve account switches

A switch, **off to begin with**: whether an owner's
`switch_session_account` call, which switches the account of one of its own sessions, is made without asking the user
(see "The user's approval of an owner's switch" in `docs/product/switching-session-accounts.md`). While the switch is
off the row's description says that, once it is on, a session can do so without asking first; while it is on, it says
that a session can do so without asking first. The Switch account the user makes from the window is not affected, and
neither is Octoboard's own switch at the usage limit (see "Automatically switch accounts at the usage limit" above).

### Automatically approve handoffs

A switch, **off to begin with**: whether an owner's
`handoff_session` call, which hands one of its own sessions off to a session of another agent, is made without asking
the user (see "The user's approval of an owner's handoff" in `docs/product/handing-off-sessions.md`). While the switch
is off the row's description says that, once it is on, a session can do so without asking first; while it is on, it
says that a session can do so without asking first. The Hand off the user makes from the window is not affected.

### Ask questions as choices

A switch, **on to begin with**: whether the instructions
Octoboard gives a session ask it to put a question that needs the user's decision as options the user picks from,
through the agent's own tool for asking, rather than listing the options in its reply (what a session is told is in
"The guidance on asking the user" in `docs/product/launching-agents.md`). While the switch is on the row's description
says so, and while it is off, that once it is on sessions are asked to do so; either way it says that a change reaches
sessions started afterwards. The daemon stores it as `ask_questions_as_choices`. A session already running keeps what
it was told when it launched; when a change reaches a console session is in "The console session's instruction file" in
`docs/product/launching-agents.md`. An agent can neither read nor change the switch through the settings tools (see
"Who reads and changes what" in `docs/product/agent-settings-tools.md`).

## Trusted folders

The folders under which Octoboard presses every agent's trust confirmation without asking, for every project — the
folder form of the trust permission all agents share — with a way to stop trusting each one. What the section's
opening paragraph says, what the list shows and what removing a folder does are in "Trusted folders" in
`docs/product/folder-trust.md`.

## Permissions

One row for each of the two macOS privacy permissions the agents can need, Screen Recording and Accessibility, to grant
it ahead of time. What the section shows and what each row offers are in `docs/product/macos-privacy-permissions.md`.

## Notifications

Whether Octoboard may show a system notification when a session starts waiting for the user (see "The raised hand" in
`docs/product/session-statuses.md`). The row says what the current state is:

| Client | What the row shows |
|---|---|
| The macOS application | That Octoboard has no notification switch of its own: macOS manages its notifications, so they are turned off, or the way they appear changed, in System Settings. Beside it is an **Open System Settings** button (below). No permission prompt is ever shown. |
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

In the macOS application, **Open System Settings** opens Octoboard's own entry in the system's Notifications settings: in System Settings on macOS 13 and later, in System
Preferences' Notifications pane on macOS 11 and 12. When the system does not open it, an error alert titled "Could not
open System Settings" says that System Settings did not open and to open it by hand and choose Notifications; it has
only its acknowledgement button, with no remedy button and no request to report an issue (see "The error alert" in
`docs/product/error-alerts.md`).

## Keyboard shortcuts

Every keyboard shortcut of the client, grouped, with a way to change the app's own commands' keys, reset one, or
restore every default. The changes are this client's own, and a client lists only the commands it has. What the section
lists and how a shortcut is changed are in `docs/product/keyboard-shortcuts.md`.

## Configuration

Two rows, each opening its own dialog: **Export configuration**, which saves consoles and projects, the application
settings or the accounts as a file, and **Import configuration**, which creates consoles, projects and accounts, and
applies application settings, from such a file (see `docs/product/configuration-export-and-import.md` and
`docs/product/configuration-export-and-import-dialogs.md`). A single console is exported from the one the sidebar shows,
which is the first while none is chosen. Both rows need the window to be able to save and open files, and where it
cannot the section holds a line saying that exporting and importing configuration files is not available here.

## About

The application itself, under its own heading in the section: its icon, its name and, in the macOS application, the
version running ("Version …"). Then, one row each:

- **Updates** and **Update channel**, in the macOS application only: where the application's updates stand, with the
  button that moves them on, and the choice between the Beta and Stable channels (see "The Updates and Update channel
  rows" in `docs/product/in-app-updates.md`).
- **Website**, **GitHub repository** and **Issue tracker**: external links, each shown as its address without the
  `https://` — `octoboard.dev`, `github.com/baijunjie/octoboard` and
  `github.com/baijunjie/octoboard/issues/new`.
- **License**: a line saying Octoboard is open-source software under the MIT license, with **View license**, a link to
  the license text in the repository.
- **Copyright**: the year and the author's name, `© 2026 BaiJunjie`.
- **Third-party licenses**, with **View licenses**, which opens the dialog described in "The third-party licenses
  dialog" below.

In a browser the Updates and Update channel rows and the version are absent.

### The third-party licenses dialog

A dialog listing the open-source packages the application ships, under an introduction saying each is under the
license shown beside it. The list is generated when the application is built, so it is always the one of the build
running. It holds:

- the npm packages the window's code depends on at run time, and what each of those depends on in turn — not the
  packages used only to develop or test it, nor packages that hold only type declarations (`@types/*`);
- the Rust crates the desktop shell and the daemon are built from, for the two Mac architectures — not the crates that
  run only inside the compiler (procedural macros), nor what only those need.

Octoboard's own packages are not listed. Packages are in order of name; each row is a disclosure showing its name and
version, its license — the license expression the package declares, or "See license file" where it declares none — and
whether it is an npm package or a Rust crate. Opening a row shows its license text, or each of its texts one under the
other, keeping their line breaks; a package that ships no license text says so instead.

A search field above the list narrows it to the packages whose name or license contains what is typed, whatever its
case, and a count under the field says how many packages are listed. While the search holds something, the count is
announced as it changes, or that no package matches; with nothing matching, the list gives way to an empty state saying
no package matches.

The list is loaded when the dialog opens, which says it is loading meanwhile. Where it cannot be read, the dialog shows
an empty-state failure (see "Failures shown on the screen" in `docs/product/error-alerts.md`) saying the licenses could
not be loaded, with the loader's own text under it and **Try again**. A build made without the list shows,
with no search field and no count, an empty state saying that this build has no license list because it was made
without it.
