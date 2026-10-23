# Settings

Settings is a large dialog over the whole window. A list of sections runs down its left side and the selected section's
settings fill the right under the section's heading, one row per setting: on the left its name and a line of text under
it saying what it does — a trusted folder's row has its path alone — and on the right its control. Under a right-to-left
language the sides swap (see "Right-to-left layout" in `docs/product/window-layout.md`). A section that explains itself
as a whole does so in a paragraph under its heading, and no section heading has a "?" help tip; help tips are on one
row in "Git", on each agent's default account and on the permission-approval headings in Agent accounts (see "Approval
modes" and "The default account" in
`docs/product/agent-accounts.md`, and "Explanations in help tips" in `docs/product/labels-and-tooltips.md`). The
sections, in order, are **General**, **Git**, **Agent accounts**, **Trusted folders**, **Permissions**, **Agent
permissions**, **Notifications** and **Configuration**; the dialog opens on General unless it was asked to open on
another section.

A console session can read the default clone directory, the automatic pulling of repositories, the automatic deletion of
archived sessions whose conversation is gone, automatic account selection, the agent accounts (without their
directories) and the trusted folders, and change the first four (see "Application settings: `get_settings` and
`update_settings`" in `docs/product/agent-settings-tools.md`).

## In a narrow window

Below 1168 px — the width under which the sidebar becomes a drawer (see "Below 1168 px: the sidebar becomes a drawer"
in `docs/product/window-layout.md`), so only a client with no window minimum of its own, such as a
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
- in the macOS application, the application menu's **Settings…** item, shortcut `Cmd+,`. The menu item does nothing
  while the window has nothing to show yet (before the daemon's state has first arrived), while Settings is already
  open, and while another dialog or an action menu — a row's or a header's, its submenus included — is open;
- the **Manage accounts** button beside the title of the console dialog's Agent default accounts section, which opens
  it on the Agent accounts section above the console dialog, left open beneath it (see "Agent config directories" in
  `docs/product/consoles.md`);
- the **Manage accounts…** button in the rail's account quota panel, which closes the panel and opens it on the Agent
  accounts section (see "The account quota panel on the rail" in `docs/product/account-quotas.md`);
- the **Add agent…** button of the "No agent added" states — the terminal's empty panel (see "A console with nothing
  in it" in `docs/product/session-terminal.md`) and the rail's account quota panel, which it closes first — which
  opens it on the Agent accounts section with the Add agent dialog in front (see "Adding an agent" in
  `docs/product/agent-accounts.md`);
- an error alert's **Add agent…** or **Manage accounts…** button, which acknowledges the alert and opens it the same
  way, above whatever dialog is open (see "Recovery and unexpected errors" in `docs/product/error-alerts.md`).

When the Add agent dialog is put in front, that dialog holds keyboard focus rather than the selected section's tab. A
request to open Settings on a section while it is already open moves it to that section and closes the account add,
edit, rename and remove forms and the agent-removal form. It also closes the Add agent dialog unless the request is
for that dialog. When the destination is not behind a login dialog, focus goes to the requested section's tab, or,
when Add agent is requested, to that dialog in front of the Agent accounts section. The login dialog keeps its
lifecycle (see "Logging an account in" in `docs/product/agent-accounts.md`).

Settings closes on `Escape`, on its close button, and on a press on the dimmed area around it. Closing it puts
keyboard focus back on the selected session's terminal, unless a dialog is open beneath it: focus then stays in that
dialog, on the control that opened Settings — the console dialog's Manage accounts button, say — or on the dialog
itself when that control is gone.

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

Small app-wide settings: an **Appearance** row, a **Language** row, a **Markdown style** row, an **Order the sidebar
by status** row, then an **Automatically delete archived sessions whose conversation is gone** row, and last, in the
macOS application only, a **Keep running in the background** row.

### Appearance

The Light / Dark / System choice for the window's appearance, as a three-way segmented control showing each option as
an icon — a sun for Light, a moon for Dark, a monitor for System — named by its word. What each option means,
which one is the default and where the choice is kept are in `docs/product/appearance.md`.

### Language

The language of the UI, chosen from a drop-down of the offered languages, each named in its own language. What the
options are, how the first launch picks a language, what follows the choice and where it is kept are in
`docs/product/language.md`.

### Markdown style

How a rendered Markdown document in the project pane's file viewer looks, chosen from a drop-down of the styles:
**Tailwind**, **GitHub**, **Newsprint** and **LaTeX**. The styles are named for what they are modelled on, so their
names are proper names, the same in every language. What each style looks like, which one is the default and where
the choice is kept are in "A Markdown file as a document" in `docs/product/file-viewer.md`.

### Order the sidebar by status

A switch, **on to begin with** — installations from before the switch existed included: whether the sidebar ranks
sessions and projects by their status, so that the ones waiting for the user come first after the pinned ones. While
the switch is on the row's description says what is happening now: sessions and projects move as their status changes,
and projects with no unarchived session end the list. While it is off the description says how the sidebar is ordered
instead — sessions newest first and projects by name, pinned ones first, so nothing moves up for waiting or working,
and projects with no unarchived session still end the list — and what turning it on will do. The daemon stores it as
`sidebar_order_by_status` and keeps it, so every window shows the same. A console session may read and change it, and a
full configuration export carries it (see "Application settings: `get_settings` and `update_settings`" in
`docs/product/agent-settings-tools.md` and "The application settings a full export carries" in
`docs/product/configuration-export-and-import.md`). The order under either state, and what else follows it, are in
"Order of projects and sessions" in `docs/product/order-of-projects-and-sessions.md`.

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
row. In the macOS application this row sits directly under that switch.

The row's description says what the choice means and never mentions quitting, which the label already carries: while
the switch is off it says what turning it on will do, and while it is on it says what is happening now — closing the
window leaves the sessions, their agents and their notifications running, and the menu bar icon brings the window
back.

What closing the window actually does under either state, and where the choice is kept, are in "Closing the window"
in `docs/product/application-lifecycle.md`.

## Git

Two rows: **Default clone directory** first, then **Automatically pull repositories**.

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

### Automatically pull repositories

A switch, off to begin with: whether a project's branch is pulled
(fast-forwarded) on its own when it is behind its upstream, rather than only reported as behind. The row's line of
text says that it pulls a branch that is behind its upstream and can be fast-forwarded, never pushes, and leaves a
branch with commits of its own alone; a "?" at the end of its name says that the remote is checked every few minutes
whichever way the switch is set, so the branch always shows how far ahead or behind it is. What it does, what it
never does and where the value is kept are in "Automatically pulling repositories" in
`docs/product/project-git-status.md`.

## Agent accounts

The agents the user has added and every account of each, grouped by agent, with the means to add, reorder and remove
agents, to add, edit, rename and remove accounts, to sign an account in, and each account's approval mode and plan
quota, and the two switches **Automatically select an account** and **Start usage windows automatically**. Where those
switches sit, and what the rest of the section shows and offers, are in `docs/product/agent-accounts.md`.

### Automatically select an account

A switch, **off to begin with** — installations from before the switch existed included: whether a new session whose
account was not named uses the account of its agent whose weekly quota resets soonest. While the switch is off the
row's description says what turning it on will do, and while it is on it says what is happening now: a new session uses
the account whose weekly quota resets soonest, so quota that is about to reset is used before it is lost. The daemon
stores it as `auto_select_account` and keeps it, so every window shows the same. Unlike the switches in Agent
permissions, a console session may read and change it, and a full configuration export carries it (see "Application
settings: `get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md` and "The application settings
a full export carries" in `docs/product/configuration-export-and-import.md`). Which account is chosen, and who applies
the choice, are in "Which account a session uses" in `docs/product/sessions.md`. Turning it on or off changes no
session already open.

### Start usage windows automatically

A switch, **off to begin with** — installations from before the switch existed included: whether Octoboard starts a
usage window that has reset but not begun counting by sending one small request on its account. While the switch is
off the row's description says what turning it on will do, and while it is on it says what is happening now: such a
window is started with one small request on that account with the agent's cheapest model, so its countdown shows and
its next reset is not pushed later, and the request uses a little of the account's quota. The daemon stores it as
`auto_start_quota_windows` and keeps it, so every window shows the same. A console session can neither read nor change
it, since it spends the user's quota (see "Who reads and changes what" in `docs/product/agent-settings-tools.md`); a
full configuration export carries it (see "The application settings a full export carries" in
`docs/product/configuration-export-and-import.md`). When a request is sent, what it is and what it leaves behind are in
"Starting a window that has not begun counting" in `docs/product/account-quotas.md`.

## Trusted folders

The folders under which Octoboard presses every agent's trust confirmation without asking, for every project — the
folder form of the trust permission all agents share — with a way to stop trusting each one. What the section's
opening paragraph says, what the list shows and what removing a folder does are in "Trusted folders" in
`docs/product/folder-trust.md`.

## Permissions

The two macOS privacy permissions the agents running in Octoboard's terminals can need: **Screen Recording**, which
agents use to take screenshots of what they built, and **Accessibility**, which lets them drive another application's
interface. macOS attributes such a request to the application responsible for the process that made it, which is
Octoboard rather than the agent, so the system's own prompt names Octoboard, appears in the middle of a task and says
nothing about why — and Octoboard cannot intercept a prompt one of its agents raised. This section is therefore the
place to grant them deliberately beforehand, and a paragraph above the rows says as much — shown only together with the
rows, so not outside the macOS application, not when the read of the grants failed and not before the first read is
in. A grant the user gave another application does not count for Octoboard; macOS ties a grant
to one application. These two are the only macOS permissions Octoboard ever asks for.

One row per permission, naming it, what agents use it for and its current state. What the row offers follows that
state:

| State | What the row shows |
|---|---|
| Granted | A large **Granted** badge in the success colour, with a circled check before its label, at the row's end in place of a control; the row's description says nothing more about the state. |
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
effect in this section on the user's return. For assistive technology the section has one status region of its own,
not shown on screen, which says every row's state in one announcement, row by row: the permission's name, its state,
and the restart line (below) where that row shows it. It speaks about 1.5 seconds after the rows appear, when the
user switches to this section. After a change it speaks
about 1.5 seconds after the user is back in the window: the wait counts only time the window is visible and has focus,
and starts over each time the user comes back. A reading that changes nothing says nothing again, and a state replaced
while it waits is never said. The rows' own states stay readable where they stand but are not status regions
themselves.

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
of the grants fails, the section is instead a red alert saying that the granted permissions could not be read, which
opens no error alert (see "Failures shown on the screen" in `docs/product/error-alerts.md`). Until the first read comes
back the section shows neither line and nothing else either.

## Agent permissions

What a session may do without asking the user first. A paragraph under the heading says as much, above two rows:
**Automatically approve session requests**, then **Automatically approve resume requests**. The section is the same in
every client. Each switch is kept by the daemon, so every window shows the same, and besides its row only an import
asked to apply a full configuration document's application settings changes it (see "The application settings a full
export carries" in `docs/product/configuration-export-and-import.md`); turning one on or off leaves the other as it
is. What an automatic approval does, what turning a switch on changes, and why agents cannot change either switch are
in "Approving requests automatically" in `docs/product/session-requests.md`.

### Automatically approve session requests

A switch, **off to begin with** — installations from before the switch existed included: whether a session's
`request_session` call, its request for a session that is nobody's, is approved without asking the user. While the
switch is off the row's description says what turning it on will do, and while it is on it says what is happening now:
a session can start a session that reports to the user without asking first, and nobody but the user owns the new
session.

### Automatically approve resume requests

A switch, **off to begin with** — installations from before the switch existed included: whether a session's
`request_resume_session` call, its request to resume a conversation, is approved without asking the user (see
`docs/product/requests-to-resume-a-conversation.md`). While the switch is off the row's description says what turning
it on will do, and while it is on it says what is happening now: a session can reopen or resume a session, or resume a
conversation in a new session, without asking first; a reopened or resumed session keeps its owner and has its
archived owners reopened with it; and a new session started this way is owned by nobody but the user.

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
