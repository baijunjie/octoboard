# Menu bar icon

The macOS application puts an icon in the system menu bar: a monochrome octopus, which macOS tints to suit a light or
dark menu bar, with the tooltip "Octoboard". It is there for as long as the application runs — while the window is
shown, while it is closed into the background, and on the screen shown when the daemon failed to start — and whatever
the user chose about keeping Octoboard running when the window is closed (see "Closing the window" in
`docs/product/application-lifecycle.md`). A browser has no such icon.

## Clicking the icon

A click on the icon, left or right, opens the icon's menu, described below. A click does nothing else: the window
comes back through the menu's **Open Octoboard** or one of its sessions.

## The icon's menu

From top to bottom:

1. **The running sessions**, one group per status, each group under a heading that cannot be chosen and that names the
   status with the number of sessions in it: **Waiting for you (n)**, then **Working (n)**, then **Awaiting
   instructions (n)**. A group with no session is left out. Interrupted and archived sessions are never listed.
   - **Pictures.** Each heading has a still glyph for its status — the accent dot for working, the waving hand for
     waiting for you, the speech bubble for awaiting instructions — and each session line has the mark of its agent
     (Claude Code, Codex, Grok), as the sidebar draws them. The menu cannot animate, so the working dot does not pulse.
     The marks the sidebar draws in the text colour (Codex, Grok) are template images here, which macOS tints to suit a
     light or dark menu; the other pictures keep their own colours, chosen to read on both. The "N more…" line has none.
   - Within a group, sessions come in the order the sidebar lists them: console by console in the order the consoles
     were created, its console sessions before its projects' sessions, both in the sidebar's own order (see "Order of
     projects and sessions" in `docs/product/sidebar.md`). Every console is included, not only the one the sidebar
     shows.
   - Each session reads as its title and where it is — the project it runs in, or the console whose console session it
     is — worded as its system notification names it (see "The raised hand" in `docs/product/sessions.md`), except
     that its terminal title is read without the activity glyphs described below. A session titled after its project
     reads as the title alone rather than naming it twice.
   - **Activity glyphs.** A run of the following characters at the very start of a session's terminal title is left
     out of the menu, together with the whitespace after it: braille patterns (U+2800–U+28FF, the usual terminal
     spinners ⠋⠙⠹…), the half-filled circles ◐◓◑◒ (U+25D0–U+25D3) and U+25F0–U+25F7, the stars and asterisks
     U+2722–U+2727 and U+2729–U+273D (among them ✳ ✶ ✻ ✽ ✦), and · • ● ○ ◉ ◎ ◆ ◇. The run is left out only when the
     title goes on after whitespace or ends there: a title that starts with such a character followed directly by
     text keeps it, and such characters further into the title are kept. What remains is shown as described under
     "Displayed names" in `docs/product/session-titles.md`, so a terminal title that is nothing but such glyphs reads
     as the created name alone. Only this menu leaves them out; the sidebar, notifications and every other place show
     the terminal title as the agent sent it.
   - A group names at most **10** sessions. Beyond that, a last line that cannot be chosen says how many more there are
     ("N more…"); the heading's number still counts them all.
   - With no session running at all, this part is a single line that cannot be chosen: **No sessions running**.
2. **Open Octoboard**, which brings a window closed into the background back, as described under "Closing the window"
   in `docs/product/application-lifecycle.md`. A window that is shown, or minimized, is brought to the front, restored
   first if it was minimized.
3. **Quit Octoboard**, after a separator, described below.

The menu follows the sessions as they change, including while the window is closed into the background. It is never
replaced while open, though: while any of Octoboard's menus is open — this one or the application menu — changes
wait, and once no menu is open the icon's menu takes on the latest of them, so an open menu stays open and unchanged
until it is dismissed or an item is chosen. It is worded in the UI's current language (see
"What follows the language" in `docs/product/language.md`). Until the window's UI has loaded and received the
daemon's state — and throughout on the screen shown when the daemon failed to start — the menu holds only Open
Octoboard and Quit Octoboard, in English.

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

A closed window below means one closed into the background, which is what closing it does only while the user has
turned that on (see "Closing the window" in `docs/product/application-lifecycle.md`).

**Observed** in the packaged application: the menu's groups, headings and session lines (seen under Simplified
Chinese); the menu updating within about 1.5 seconds of a session changing while the window was closed; opening and
dismissing the menu without choosing anything leaving a closed window closed; choosing a session bringing the window
back with that session selected; and Open Octoboard. What was seen of Quit Octoboard is under "Quitting" in
`docs/product/application-lifecycle.md`.

**Not yet observed** in the packaged application: the pictures in the menu, in a light and in a dark menu, and a left
click opening the menu.

**Observed caveat:** for about the first 0.6 seconds after closing the window from native fullscreen, a click on the
icon is lost — a right click does not even open the menu — apparently swallowed by macOS during its exit from
fullscreen. From about 0.65 seconds on, every click was answered.
