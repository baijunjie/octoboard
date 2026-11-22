# General settings

Settings' **General** section (see "General" in `docs/product/settings.md`): the **Language** row first, on its own
because it governs the whole interface, then app-wide settings in six groups, each under a heading (see "Group
headings" in `docs/product/settings.md`). The groups are ordered by how often each is used, most frequent first:

- **Sorting**: **Sessions**, **Active projects** and **Inactive projects**, in every client;
- **Terminals**: **Close idle terminals when closing the panel**, in every client;
- **Power**, in every client, where the daemon's host can be held awake: **Prevent sleep**, and under it, while it is
  on, **When to prevent sleep**;
- **Window**, in the desktop application's main window only: **Keep running in the background**, then **Show menu bar
  icon**;
- **Startup**: **Launch at login**, in the desktop application's main window where the system offers it (see "Launch
  at login" below), then **Resume sessions on start**, in every client;
- **Command line**, in the macOS application's main window only: **Command-line tool** (see "Command-line tool"
  below).

A group with nothing to show in the client is left out, so a browser, and a remote window of the desktop application
(see "The remote window" in `docs/product/remote-daemons.md`), show Language, then Sorting, Terminals, Power where the
daemon's host can be held awake, and Startup with Resume sessions on start alone. In a remote window
every setting of the section that the daemon keeps is the remote daemon's.

How each of these switches saves is in "How switches save" in `docs/product/settings.md`.

## Language

The language of every window of the daemon, and the one agents talk to the user in, chosen from a drop-down of the
offered languages, each named in its own language; the row's description says both. In a remote window it is the
remote daemon's. What the options are, how a language is picked before one is known, what follows the choice, where it
is kept, how every window follows it and how it becomes the language sessions are told are in
`docs/product/language.md`.

## Launch at login

A switch: whether the system opens Octoboard when the user logs in. **Only the desktop application's main window has
this row**, on Windows and on macOS 13 and later; on macOS 11 and 12 and in a browser it is absent. The switch shows the system's own
state rather than a choice Octoboard keeps — off until the system has an entry for Octoboard — and the row appears once
that state has been read. While it is off the row's description says that, once it is
on, Octoboard opens when the user logs in; while it is on, that it does; either way it adds that, while Octoboard keeps
running in the background, it then starts without showing its window. A second sentence names the list of apps the
system, named as it names itself, opens at login: while the switch is off, that turning it on adds Octoboard to that
list and turning it off removes it; while it is on, that Octoboard is in that list until the switch is turned off.
While the system holds the entry for the user's
approval (macOS), the switch is on, the description says that Octoboard is waiting for the user's approval and to allow
it in System Settings, and an **Open System Settings** button stands beside the switch; at 768 px and below the two go
under the text (see "In a narrow window" in `docs/product/settings.md`). What the system's entry is, how the state is
read, the approval, failures and what a launch at login does are in `docs/product/launch-at-login.md`. A configuration
export never carries it.

## Command-line tool

**Command line**, **Command-line tool**, in the macOS application's main window only. macOS puts nothing inside an
application bundle on the PATH, so a terminal finds no `octoboardd` by name; this row makes the link that does. The link
is a symbolic link named `octoboardd` to the `octoboardd` inside the application (`Contents/MacOS/octoboardd`), not a
copy, so it runs the build an update puts there. **Nothing makes it at launch**: the user installs it from the row.

- **Install** goes to `/usr/local/bin`: made directly where the user may write there, and otherwise through the system's
  own administrator prompt, which creates the directory too. Where that is refused or fails, the link goes to
  `~/.local/bin` instead, and the row then says to add that directory to the PATH if it is not on it yet; but where
  `/usr/local/bin` already holds a link of the application's, the refusal or failure is the answer, said in the row,
  with no fallback, since a link in `~/.local/bin` would be hidden behind the old one. A file of that name that is not a
  link is never replaced: the row says so. Nor is a link, at either location, that leads to anything but an
  application's `octoboardd`: it blocks the install, and the row says what is there. A link to another copy of the
  application is replaced. A link left in `~/.local/bin` by an earlier install is removed once the one in
  `/usr/local/bin` is made.
- **The row shows its state each time Settings opens**, and again when the window gets focus back, saying so in the row
  and never in a dialog: **Not installed**, followed by a line saying that installing adds a link named `octoboardd` to
  `/usr/local/bin`, or to `~/.local/bin` if the system does not allow that; **Installed at** the link's path; installed but running **another copy** of
  the application; or installed but the copy it points to **is no longer there**. A refusal or failure of an action is
  said in the row too.
- **The actions follow the state**: Install where there is no link; Reinstall (which points the link at this
  application) and Remove where there is one, whichever of the three states.
- **Remove deletes every link of the application's** in both `/usr/local/bin` and `~/.local/bin`, through the
  administrator prompt where the directory is not the user's to write; a file that is not a link, or a link to anything
  but an application's `octoboardd`, is left. A directory installing created stays, empty. The clean-up before
  uninstalling lists the link wherever there is one and its **Remove all** removes it as Remove does (see
  `docs/product/clean-up-before-uninstalling.md`). Deleting the application without either leaves the link, leading
  nowhere (see `docs/product/what-remains-after-uninstalling.md`).

Run from the link, `octoboardd` is the application's own copy, whose `start` and `stop` start and quit the application
(see "The application's copy of `octoboardd`" in `docs/product/daemon-command-line.md`).

## Resume sessions on start

A switch, **off to begin with**: whether Octoboard resumes the interrupted sessions each time it starts, those the
previous run left and any interrupted before it last stopped. While the switch is off the row's description says what
turning it on will do, and while it is on it says what is happening now, the same in every client. The daemon stores it as
`resume_sessions_on_start` and keeps it, so every window shows the same. A console session may read and change it, and a
configuration export that includes the General section carries it (see "Application settings: `get_settings` and
`update_settings`" in `docs/product/agent-settings-tools.md` and "The application settings an export carries" in
`docs/product/settings-in-a-configuration.md`). Turning it on or off resumes nothing at once: it is read when the daemon
next starts. Which sessions are resumed, and how, is in "Resuming the interrupted sessions at start" in
`docs/product/archiving-and-resuming-sessions.md`.

## Keep running in the background

A switch, labelled for what turning it on does: keep Octoboard running in the background when the window is closed. It
is **off to begin with**, so closing the window quits.

**Only the desktop application's main window has this row**, on macOS and on Windows. A browser has no native window to
close into the background, so the row is absent there, and a remote window leaves it to the main window, the choice
being the application's.

The row's description says what the choice means and never mentions quitting, which the label already carries: while
the switch is off it says what turning it on will do, and while it is on it says what is happening now — closing the
window leaves the sessions, their agents and their notifications running, and the menu bar icon (the
notification-area icon on Windows) brings the window back. While the menu bar icon is hidden (see "Show menu bar icon"
below), it names, in the icon's place, opening Octoboard again or clicking one of its notifications as the way back to
the window.

What closing the window actually does under either state, and where the choice is kept, are in "Closing the window"
in `docs/product/application-lifecycle.md`.

## Show menu bar icon

A switch, **on to begin with**: whether the menu bar icon is shown. On Windows its label names the notification-area
icon instead. **Only the desktop application's main window has this row**: the icon is the application's (see
`docs/product/menu-bar-icon.md`). While the switch is on the row's description says that the
icon's menu lists the sessions waiting for the user, paused, done or awaiting instructions, and opens the window; while
it is off, that once it is on the icon's menu does so — or, while Octoboard keeps running in the background, that once
it is on the icon's menu opens the window and that meanwhile opening Octoboard again or clicking one of its
notifications brings the window back. The choice is kept per client, as Keep running in the background is, and a
configuration export that includes the General section carries it (see "The settings a client holds" in
`docs/product/settings-in-a-configuration.md`). What hiding the icon does is in "Hiding the icon" in
`docs/product/menu-bar-icon.md`.

## Prevent sleep

A switch, **off to begin with**: whether Octoboard keeps the computer and its display awake. The computer is the
daemon's host, and the row is there in every client, the desktop application and a browser alike, **only where that
host can be held awake** — macOS and Windows always, Linux where logind answers on the system bus (see "Which computers
can be held awake" in `docs/product/keeping-the-computer-awake.md`); elsewhere it is absent. While the switch is off
the row's description says that, once it is on, Octoboard keeps the computer and its display awake, so the screen does
not turn off or lock while the user is idle; while it is on, that it does so as chosen below.

While the switch is on, a **When to prevent sleep** row follows it, a drop-down of **Always** and **While sessions
work**, While sessions work to begin with. Its description follows the choice: for Always, that Octoboard keeps the
computer awake for as long as it is running; for While sessions work, that it does so while a session is starting or
working, in the background included, and that once none is the computer can sleep. While the switch is off the row is
not shown and its value is kept; when the switch is turned off by something else while the row has keyboard focus — an
import, say — focus moves to the switch. The drop-down saves a choice as it is picked and shows the value the daemon has
stored; a failed save is shown in an error alert saying the setting could not be saved.

The daemon stores both, as `keep_awake` and `keep_awake_when`, so every client shows the same. A console session may
read and change them, and a configuration export that includes the General section carries them (see
"Application settings: `get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md` and "The
application settings an export carries" in `docs/product/settings-in-a-configuration.md`). What is held, and exactly
when, is in `docs/product/keeping-the-computer-awake.md`.

## Close idle terminals when closing the panel

A switch, **off to begin with**: whether explicitly closing a project's Terminal panel also closes its idle terminals.
The description says what turning it on will do while it is off, and what happens while it is on; either way it says
terminals with running programs stay open. Which closes count and which terminals are kept are in "Closing the panel"
in `docs/product/terminal-panel.md`. Turning it on closes nothing at once; it applies to later panel closes.

The daemon stores it as `auto_close_idle_shell_terminals` and keeps it, so every window shows the same. A console
session may read and change it, and a configuration export that includes the General section carries it (see
"Application settings: `get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md` and "The
application settings an export carries" in `docs/product/settings-in-a-configuration.md`).

## Sorting

How sessions and projects are ordered, as three selects, each taking effect as it is chosen: **Sessions** (Status or
Recently started), **Active projects**, the projects with at least one unarchived session, which always come before the
inactive ones (Status, Name or Recently used), and **Inactive projects**, the projects with none, which always come
after the active ones (Name or Recently used). What each choice does, the defaults, everything that follows the sort and
where it is kept are in "Choosing the sort" in `docs/product/order-of-projects-and-sessions.md`.
