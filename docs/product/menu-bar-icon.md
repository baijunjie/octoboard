# Menu bar icon

The macOS application puts an icon in the system menu bar: an octopus, which carries an orange dot while a session waits
for the user or is dormant, with a quota indicator for the selected session when one is available, and whose eyes turn
red while Octoboard keeps the computer awake (see "The icon's look" below). It is there for as long as the application
runs — while the window is shown, while it is closed into the background, and on the screen shown when the daemon failed
to start — and whatever the user chose about keeping Octoboard running when the window is closed (see "Closing the
window" in `docs/product/application-lifecycle.md`), unless the user has hidden it (see "Hiding the icon" below). The
Windows application puts the same icon, with the same menu, in the taskbar's notification area, drawn and behaving as
the system's notification area items do; what differs there is in "The notification-area icon on Windows" below. A
browser has no such icon.

## The icon's look

The icon has two attention states. In both, the octopus is monochrome, and macOS tints it to suit a light or dark menu
bar. Its eyes are two slanted eye sockets inside its head, drawn lighter than the head — partly see-through, in one flat
tint — and set in solid head all round:

- **The octopus alone**, while no session is waiting for the user or dormant.
- **The octopus with an orange dot** at its upper-right corner, separated from the octopus by a clear margin of about
  **0.5 pt**, while at least one session is waiting for the user or dormant (see "Dormant" in
  `docs/product/session-statuses.md`). The dot is drawn in the system orange, which macOS resolves
  for a light or dark menu bar, rather than in the octopus's tint. The dot is about **5.5 pt** across within the
  18 pt icon. It appears with or without a readable quota.
  It is part of the icon's look and is not presented to assistive technology on
  its own. Sessions across every console count, independently of which statuses the menu lists. A dormant
  session can light the dot even when the menu has no sessions to show. What counts is the shown status (see "Which
  status a session shows" in `docs/product/session-statuses.md`), so a session shown as switching account or
  reconnecting does not light it.

When the selected session has a readable account quota, bars below the octopus show the remaining percentage, with
a gap above them. They use the octopus's monochrome tint and span the icon's width, covering its lower tentacles while
keeping the octopus at its original scale; the icon keeps its **18 pt** footprint. As on the rail, they come in two
looks:

- **Two layers**, when the account has both a five-hour and an account-wide weekly window: two **outlined capsules**,
  each with a hollow unfilled portion — **18 × 3.5 pt** for the five-hour window and, **1 pt** beneath it,
  **18 × 3 pt** for the weekly window. The five-hour capsule, the gap and the weekly capsule stand 7 : 2 : 6, close to
  the rail's two layers' 4 : 1 : 3.
- **One bar** otherwise: an **outlined capsule** of **18 × 4 pt**, with a hollow unfilled portion; its outline is
  **1 pt**, a quarter of its height.

**While Octoboard keeps the computer awake** — while the daemon reports that it holds its host awake (see "Whether the
computer is held now" in `docs/product/keeping-the-computer-awake.md`) — the whole of both eye sockets turns solid red,
in the system red, which macOS resolves for a light or dark menu bar; their shape stays the same. It shows only while
the computer is actually being kept awake, not merely while Prevent sleep is on, and it goes with either attention state
and with or without a quota. The window's UI passes what the daemon reports on to the icon, so the eyes turn red only
while that UI is connected to the daemon: not before it has loaded and connected, not while it has lost its connection
to the daemon, and never on the screen shown when the daemon failed to start, where there is no daemon to hold
anything.

The attention dot stays at the upper right, clear of the bars, and neither the dot, the space cleared around it, nor
the bars ever cover the eyes. Without a readable quota the bars disappear; without attention, a quota or the computer
kept awake the original octopus returns.

The account, the windows drawn and the hiding rules are the same as "The selected session's quota indicator" in
`docs/product/account-quotas.md`. On macOS the icon has **no tooltip**. With a quota, its accessible name identifies the
agent and account and, for each window drawn, the window and its whole remaining percent, in the UI's language; without one it
names only Octoboard. While the computer is kept awake, it adds that Octoboard is keeping the computer awake — after the
quota's words, or in place of Octoboard's name alone. Selecting another session updates the indicator even if no
session data changed, and a session's account switch, account edits, availability changes and new readings update it too, as does a window's reset time
passing, after which that window is drawn full (see "How an account's quota is shown" in
`docs/product/account-quotas.md`). The bars fill from the left in every UI language: the menu bar belongs to the system,
not to the window, whose layout is what a right-to-left language mirrors (see `docs/product/right-to-left-layout.md`).

The look changes together with the menu: a change that has to wait for an open menu to close (see the end of "The icon's
menu" below) changes the look and its label only when it reaches the menu, so both reflect the same session snapshot.
The octopus's dot clearance and the orange dot always appear and go together. The red eyes are the exception to
waiting: they come and go as soon as the window's UI learns that the daemon started or stopped keeping the computer
awake, even while a menu is open, and the accessible name follows them. Until the window's UI has sent its first menu
the icon is the octopus alone, apart from the red eyes while the computer is kept awake; on the screen shown when the
daemon failed to start it is the octopus alone throughout.

## Clicking the icon

A click on the icon, left or right, opens the icon's menu, described below. A click does nothing else: the window
comes back through the menu's **Open board**, its update entry or one of its sessions.

On macOS, for about the first 0.6 seconds after the window is closed from native fullscreen, a click on the icon, left
or right, is lost and opens nothing; macOS swallows it while it leaves fullscreen.

## The icon's menu

From top to bottom:

1. **The sessions**, only those whose shown status is **waiting for the user**, **paused**, **done** or **awaiting
   instructions**, in that order, one group per status. Each group has a heading that cannot be chosen and names
   the status with the number of sessions in it, whatever sort is chosen. A group with no
   sessions is left out. The shown status is the one given by "Which status a session shows" in
   `docs/product/session-statuses.md`: all other shown statuses are excluded, including starting and switching
   account even when the session would become done or awaiting instructions afterwards, and reconnecting whatever the
   session's stored status.
   - **Headings.** A heading is system text at the menu font's size, regular weight, in the secondary text
     colour, with its status glyph to the left of the text. Its glyph and text sit where the session lines below place
     their agent marks and titles — the same left edges and the same line height, the glyph centred on the text — as
     macOS 26 lays out a menu item; another version of macOS may place a menu item's picture and title otherwise. It
     is not dimmed the way a menu line that cannot be chosen is, never highlights under the pointer, and is read by
     assistive technology once, as its menu item, the heading's words.
   - **Pictures.** Each heading has a still glyph for its status, mirroring the sidebar's (see "Status glyphs" in
     `docs/product/session-statuses.md`) — the hand for waiting for the user, the circle with a pause sign for paused, the circle with a check mark for
     done and the speech bubble for awaiting instructions — and each session line has the mark of its agent
     (Claude Code, Codex, Grok), as the sidebar draws them. The menu cannot animate, so the hand does not wave;
     the hand and the speech bubble sit centred as the sidebar's do.
     The marks the sidebar draws in the text colour (Codex, Grok) are template images here, which macOS tints to suit a
     light or dark menu. A status glyph is in the colour the sidebar gives that status in the window's current
     appearance, drawn again in the new colours for the first menu that shows it after the appearance changes; the
     other agent marks keep their own colours, chosen to read on both. The "N more…" line has none. Each quota
     submenu's opening line (below) carries the mark of its agent in the same way; the title is what is read, the
     mark adds nothing to it.
   - Within a group, sessions come in the order the sidebar lists them: console by console in the order the consoles
     were created, its console sessions before its projects' sessions, both in the sidebar's order, as the sort chosen
     in Settings orders them (see "Choosing the sort" in `docs/product/order-of-projects-and-sessions.md`). Every
     console is included, not only the one the sidebar shows.
   - Each session reads as its title and where it is — the project it runs in, or the console whose console session it
     is — worded as its system notification names it (see "The raised hand" in `docs/product/session-statuses.md`). A
     session titled after its project reads as the title alone rather than naming it twice. Like every other place, it
     reads the terminal title without the glyphs an agent animates at its start (see "Leading activity glyphs" in
     `docs/product/session-titles.md`).
   - A session's line is kept to at most **60** display columns, East Asian wide characters and emoji counting as two
     and every other character as one. The place it names is cut first to at most **24** columns, then the title to
     whatever room is left; a cut text ends in "…" and is never cut inside a character made of several code points
     (an emoji sequence, a letter with its accents).
   - A group names at most **10** sessions. Beyond that, a last line that cannot be chosen says how many more there are
     ("N more…"); the heading's number still counts them all.
   - With no session in one of the four listed statuses, this part is a single heading saying there are no
     sessions to show, styled like the other headings but with no glyph. Sessions in other statuses may still be
     running.
2. **The quota submenus**, one per added agent that is available — **Claude usage**, **Codex usage**, **Grok
   usage**, in the order of the added agents (see "Added agents" in `docs/product/agent-accounts.md`), the agents'
   names never translated — each opening to that agent's accounts and showing what Settings and the rail's panel
   show, including window order (see "Quota windows" and "How an account's quota is shown" in
   `docs/product/account-quotas.md`). An account that is turned off (see "Turning an account off" in
   `docs/product/agent-accounts.md`) is not listed, and the menu follows the switch as it flips. The exception is the
   account the selected session is running on, as that section has it: it stays listed in its place, its header line
   ending in **Disabled**, in secondary text after its reached-limit tag, or after its name and plan when there is
   no tag. An agent that is not added, not available, whose availability is not yet determined, or whose
   accounts are all turned off, bar that one, has none. The submenus are set off from the sessions and from the
   entry for opening the board by separators. Inside a submenu, nothing can be chosen:
   - each account is a header line, at the small size of a section header and regular weight, with its name and plan
     in the secondary text colour. The agent's default account uses the name the user gave it, else **Default** (see
     "The default account" in `docs/product/agent-accounts.md`), and a plan the agent names follows after a dot
     ("Default · Max"). Accounts follow the saved display order (see "Reordering accounts" in
     `docs/product/agent-accounts.md`), each set off by a separator.
     For an account with windows to show, how long ago they were read is right-aligned on the header line in the same
     small secondary text ("Updated 3 minutes ago", "Updated just now" under a minute), also when they are kept after a
     newer reading failed, and assistive technology reads the line as one phrase ("Default · Max · updated
     3 minutes ago"); it is left out for an account still being read or with no windows to show;
   - only an account whose quota is used up has a status pill after its name and plan: a rounded, soft-filled
     reached-limit tag using the same warning text and background palette as the quota panel, in light and dark
     appearances. Its localized words are included in the header's accessible name, read once with the rest of the
     line; the pill is not announced separately. Used-up quota follows "A used-up account" in
     `docs/product/account-quotas.md`, including kept windows after a failed reading. Other accounts have no quota
     status label, visually or in the header's accessible name. Accounts still being read or with no windows keep
     their reading, unavailable or login note below the header; an out-of-date reading keeps its note there too;
   - under the header, each window is one row, except as the next point says: its name on the left, as in "Quota
     windows" in `docs/product/account-quotas.md` ("5h", "Session"), "N% remaining · resets in <countdown>" ("82%
     remaining · resets in 1h 57m", "resets now" once the time has passed, no countdown for a window with no reset time)
     right-aligned in the secondary text colour, and a bar of what remains directly below, on a translucent grey track.
     The bar, in this row and in the meter row below, is in its agent's colour, the one the rail's quota panel draws
     that agent's bars in (see "How an account's quota is shown" in `docs/product/account-quotas.md`), for the menu's
     current appearance, light or dark (the menu's text colour when that colour cannot be read). The bars of an
     account whose quota is used up are instead a neutral grey, the one the rail's quota panel draws such bars in, for
     the menu's current appearance (the menu's secondary text colour when that grey cannot be read). A row whose text needs more room than
     the submenu's usual width widens the submenu rather than being cut short. A row is read by assistive technology
     once, as one phrase ("5h 82% remaining · resets in 1h 57m"), by its menu item alone: neither the name, the detail
     nor the bar is read on its own;
   - an account with both a five-hour and an account-wide weekly window shows the two as **one meter row**, in the
     five-hour window's place: its name and detail with its bar, **2.5 pt** beneath that bar a thinner one, **4.5 pt**
     tall against the five-hour bar's **6 pt** (three quarters, as on every two-layer bar), for the weekly window, and
     beneath it the weekly window's name and detail, both in the secondary text colour. Assistive technology reads the
     row once, as the two windows' phrases joined ("5h 82% remaining · resets in 1h 57m, Weekly 40% remaining · resets
     in 2d 3h"). Every other window keeps a row of its own, and the order of windows is unchanged;
   - a row, a header and a note are not dimmed the way a menu line that cannot be chosen is: the window's name is at
     full strength, and none ever highlights under the pointer. Their text and the bar's track take the system's
     colours for the menu they are drawn in, light or dark. A header and a note are each read by assistive
     technology once, as their menu item, their words;
   - an account never read shows **Reading usage…** in place of windows, and an account with no windows to show
     shows **Usage unavailable**, a note in the secondary text colour at the menu font's size, and without the
     reason. That includes an entry that has no windows and no failure to explain, which Settings and the rail show
     nothing for. An account that is not logged in (see "Quota states" in `docs/product/account-quotas.md`) shows the
     note **Not logged in** instead. One whose latest reading failed after a good one keeps its last windows under a
     note, led by a warning sign, saying they are out of date because the last read failed — without the reason
     either, and without an age, which its header carries.

   Each submenu's opening line can show **remaining-quota bars** at its right, before the submenu arrow.
   It uses that agent's account configured for the **current console**, independently of the selected session that
   supplies the menu bar icon's capsule. A missing, deleted or wrong-agent account reference falls back to the
   agent's default account. With no current console there are no opener bars, and neither are there while that
   account is turned off (see "Turning an account off" in `docs/product/agent-accounts.md`), unless the submenu lists
   it as the account the selected session is running on: the opener then shows its bars and names it as for any other
   account. When the account has both
   a five-hour and an account-wide weekly window, the five-hour bar has a thinner weekly bar beneath it: **4 pt** over
   **3 pt**, **2 pt** apart, within a picture **56 × 9 pt**. Otherwise there is one **4 pt** bar, vertically centred in
   that picture, with window priority five-hour, then account-wide weekly, then the credit pool; model-specific weekly
   limits are excluded.
   An unread account, a reading without an eligible window or a failed latest reading has no opener bars,
   including when the submenu retains stale windows. Switching consoles or editing their account choices updates
   the bars. The bars are placed for a menu at most **640 pt** wide: a menu whose lines ask for more is laid out as
   if it were 640 pt wide.

   These opener bars are solid rounded tracks in a muted tint, with the remaining share filled at full strength,
   aligned at the right across the opening lines. Their tint follows the native row in light and dark menus and
   while highlighted. The row keeps the system's pointer, highlight and keyboard submenu behavior, including
   Right Arrow to open it. Its
   accessible name identifies the agent and account and, for each window drawn, the window and whole remaining
   percent; the bars are not read separately. The row has no tooltip, which would float over the submenu it opens
   and repeat what that shows.

   Countdowns, the ages of readings and whether a window's reset time has passed (so that it is drawn full; see "How
   an account's quota is shown" in `docs/product/account-quotas.md`) are as of the last time the menu was updated. A
   change of the selected session, current console, sessions, consoles, projects, accounts, added agents, agents'
   availability, quotas or the UI's language updates it, and so does the clock: at the earliest reset time still to
   come among the readings held, and each time a reading's age reaches another whole minute, so that while the menu
   is closed the countdowns and ages keep up with the clock to the minute and a window goes full at its reset.
   Opening the menu asks the daemon for a fresh reading, as opening the rail's panel or Settings' section does (see
   "When quotas are read" in `docs/product/account-quota-readings.md`); since the menu never changes while open, what that
   brings shows from the next time it is opened. The menu does not wait for the reply.
3. **Check for updates**, which opens Settings on its About section with the update controls and checks when no
   newer version is known. Its label identifies a known newer version, or a download in progress, as described in
   "The menu bar update entry" in `docs/product/in-app-updates.md`.
4. **Open board**, which brings a window closed into the background back, as described under "Closing the window"
   in `docs/product/application-lifecycle.md`. A window that is shown, or minimized, is brought to the front, restored
   first if it was minimized.
5. **Quit completely**, after a separator, described below.

The open entry names the board rather than repeating the application name; the quit entry makes clear that it ends
the whole application, including background operation.

The menu follows the sessions and update entry as they change, including while the window is closed into the
background. It is never replaced while open, though: while any of Octoboard's menus is open — this one or the application menu — changes
wait, and once no menu is open the icon's menu takes on the latest of them, so an open menu stays open and unchanged
until it is dismissed or an item is chosen. The icon's look waits with them (see "The icon's look" above). It is
worded in the UI's current language (see "What follows the language" in `docs/product/language.md`). Until the
window's UI has loaded and received the daemon's state — and throughout on the screen shown when the daemon failed to
start — the menu holds only Check for updates, Open board and Quit completely, in English, with no quota submenus.

### Choosing a session

Choosing a session in the menu brings the window back as Open board does, then selects that session as if it had
been selected in the sidebar — the sidebar follows it to its console, and an interrupted session is resumed (see
"Selecting a session" in `docs/product/sidebar.md`) — and puts keyboard focus in its terminal. A session that has gone away since the menu was
opened only brings the window back.

### Quitting completely

The quit entry behaves like `Cmd+Q`: while a session or a busy shell terminal holds a quit up it asks the same
confirmation, bringing a window closed into the background back to ask it, and otherwise it quits at once (see
"Quitting" in `docs/product/application-lifecycle.md`, which says what holds a quit up).

## Hiding the icon

The **Show menu bar icon** switch in Settings' App behavior section — **Show notification area icon** on Windows — is on
to begin with; turning it off takes the icon out of the menu bar, or out of the notification area, and turning it back
on puts it back (see "Show menu bar icon" in `docs/product/app-behavior-settings.md`). Nothing else changes while it is hidden: the
menu and the icon's look keep following the sessions, the quota and the keep-awake hold, so the icon comes back as it
would have been. On macOS the icon keeps its place among the status items rather than being removed and made again.

Hidden, the icon's menu is not there to bring a window closed into the background back: opening Octoboard again, or one
of its notifications, still does (see "Closing the window" in `docs/product/application-lifecycle.md`). The choice is
kept by the desktop application in its settings file, together with keeping Octoboard running in the background (see
`docs/product/files-octoboard-owns.md`), and read as it starts: a hidden icon is hidden as the application creates it
at launch, before the window's UI has loaded, so it stays hidden on the screen shown when the daemon failed to start
too. Turning the switch takes effect at once and is written straight away. A configuration export that includes the
App behavior section carries it (see "The settings a client holds" in
`docs/product/settings-in-a-configuration.md`).

## The notification-area icon on Windows

In the Windows application the icon and its menu hold what the sections above describe, with these differences, which
follow from the system's own notification-area icons and plain menus:

- **The icon** is Octoboard's application icon in its own colours, not a monochrome octopus tinted for the bar. While
  a session waits for the user or is dormant it carries the orange dot in its corner, by the same rule as above. While
  the computer is kept awake, larger bright red eyes are drawn over its own eyes, which are red already, so the change
  reads by their size and not only by their colour. No quota bars are drawn on it.
- **The icon's label is its tooltip**: what macOS gives as the icon's accessible name — the selected session's agent,
  account and remaining percentages, or Octoboard's name alone, with the keep-awake words added while the computer is
  kept awake — shows when the pointer rests on the icon, and is what the notification area reads out as the icon's
  name.
- **A click**, left or right, opens the menu when the button is released. That opening asks the daemon for a fresh
  quota reading, as opening the menu does on macOS.
- **The menu's lines are the system's plain menu items.** They carry the same text: the group headings, the session
  lines, the quota submenus and Check for updates, Open board and Quit completely. The headings and every line of a
  quota submenu cannot be chosen, and are drawn as the system draws an item that cannot be chosen. Each line of a
  quota submenu is the words assistive technology reads for that line on macOS — an account's header with its plan,
  reached-limit words and age ("Default · Max · updated 3 minutes ago"), a window's phrase ("5h 82% remaining · resets
  in 1h 57m"), the joined phrase of a five-hour and weekly meter, a note's text — with no bars and no pill, and a
  submenu's opening line carries no remaining-quota bars.
- The marks the UI sets around names and values to keep their reading order in right-to-left text are taken out of
  the menu's lines and the tooltip, since the system's menu would draw them as boxes; the order of a left-to-right
  value inside right-to-left text is then the system's to decide.
- **The marks the sidebar draws in the text colour (Codex, Grok)**, which macOS tints as template images, are drawn
  here in one colour for the theme the menu is drawn in: white on a dark menu and black on a light one, following the
  user's default app mode in Windows' colour settings rather than the Windows mode or Octoboard's own appearance, and
  in the menu's text colour while high contrast is on. When the system's light or dark mode changes, the menu is built
  again in the new colours. The other pictures are as on macOS.
- **A change waits while the menu is open**, as above: the latest menu the window sends meanwhile is applied once the
  menu closes. A menu opened again within 10 ms of closing counts as still open, so a menu sent
  meanwhile stays held until that menu closes.
