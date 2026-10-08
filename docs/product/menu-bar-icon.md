# Menu bar icon

The macOS application puts an icon in the system menu bar: a monochrome octopus, which macOS tints to suit a light or
dark menu bar, with the tooltip "Octoboard". It is there for as long as the application runs — while the window is
shown, while it is closed into the background (see "Closing the window" in `docs/product/application-lifecycle.md`),
and on the screen shown when the daemon failed to start. A browser has no such icon.

## Clicking the icon

- **A left click** brings a window closed into the background back, as described under "Closing the window" in
  `docs/product/application-lifecycle.md`. A window that is shown, or minimized, is brought to the front, restored
  first if it was minimized.
- **A right click** opens the icon's menu, described below.

## The icon's menu

From top to bottom:

1. **The running sessions**, one group per status, each group under a heading that cannot be chosen and that names the
   status with the number of sessions in it: **Waiting for you (n)**, then **Working (n)**, then **Awaiting
   instructions (n)**. A group with no session is left out. Interrupted and archived sessions are never listed.
   - Within a group, sessions come in the order the sidebar lists them: console by console in the order the consoles
     were created, its console sessions before its projects' sessions, both in the sidebar's own order (see "Order of
     projects and sessions" in `docs/product/sidebar.md`). Every console is included, not only the one the sidebar
     shows.
   - Each session reads as its title and where it is — the project it runs in, or the console whose console session it
     is — the same way its system notification names it (see "The raised hand" in `docs/product/sessions.md`). A
     session titled after its project reads as the title alone rather than naming it twice.
   - A group names at most **10** sessions. Beyond that, a last line that cannot be chosen says how many more there are
     ("N more…"); the heading's number still counts them all.
   - With no session running at all, this part is a single line that cannot be chosen: **No sessions running**.
2. **Open Octoboard**, which brings the window back as a left click on the icon does.
3. **Quit Octoboard**, after a separator, described below.

The menu follows the sessions as they change, including while the window is closed into the background. It is worded
in the UI's current language (see "What follows the language" in `docs/product/language.md`). Until the window's UI
has loaded and received the daemon's state — and throughout on the screen shown when the daemon failed to start — the
menu holds only Open Octoboard and Quit Octoboard, in English.

### Choosing a session

Choosing a session in the menu brings the window back as Open Octoboard does, then selects that session as if it had
been selected in the sidebar — the sidebar follows it to its console (see "Selecting a session" in
`docs/product/sidebar.md`) — and puts keyboard focus in its terminal. A session that has gone away since the menu was
opened only brings the window back.

### Quit Octoboard

Quit Octoboard is a quit like `Cmd+Q`: with a session running it asks the same confirmation, bringing a window closed
into the background back to ask it, and with none it quits at once (see "Quitting" in
`docs/product/application-lifecycle.md`).

## What has been tried by hand

**Observed** in the packaged application: a left click bringing a closed window back; the menu's groups, headings and
session lines (seen under Simplified Chinese); the menu updating within about 1.5 seconds of a session changing while
the window was closed; opening and dismissing the menu without choosing anything leaving a closed window closed;
choosing a session bringing the window back with that session selected; and Open Octoboard. What was seen of Quit
Octoboard is under "Quitting" in `docs/product/application-lifecycle.md`.

**Observed caveat:** for about the first 0.6 seconds after closing the window from native fullscreen, a click on the
icon is lost — a right click does not even open the menu — apparently swallowed by macOS during its exit from
fullscreen. From about 0.65 seconds on, every click was answered.
