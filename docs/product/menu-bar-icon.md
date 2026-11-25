# Menu bar icon

The macOS application puts one icon in the system menu bar for the whole application, whichever daemons it shows: an
octopus, which carries an orange dot while a session of any of them waits for the user or is dormant, with a quota
indicator for the selected session when one is available, and whose eyes turn red while Octoboard keeps the computer
awake (see "The icon's look" below). Its menu has a tab for each daemon the application shows: the local daemon's,
then each connected remote daemon's (see "The tabs" below, and `docs/product/remote-daemons.md`). It is there for as
long as the application runs — while the window is shown, while it is closed into the background, and on the screen shown when the daemon failed
to start — and whatever the user chose about keeping Octoboard running when the window is closed (see "Closing the
window" in `docs/product/application-lifecycle.md`), unless the user has hidden it (see "Hiding the icon" below). The
Windows application puts the same icon, with the same menu, in the taskbar's notification area, drawn and behaving as
the system's notification area items do; what differs there is in "The notification-area icon on Windows" below. A
browser has no such icon. Connecting to a remote daemon is not offered here: that is the Remote menu's and the other
places "Where remote daemons are reached from" in `docs/product/remote-daemons.md` lists.

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
  its own. Sessions across every console of every daemon the application shows count, independently of which statuses
  the menu lists. A dormant
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

**Whose quota and eyes the icon shows.** The attention dot is every daemon's; the quota bars and the red eyes are those
of one window: the one the user focused last, whichever daemon it shows — its selected session's quota, and whether its
daemon keeps its own computer awake — else the main window's. They stay that window's while every window is hidden,
and follow another window as soon as it is focused. A closed remote window's go with it.

The account, the windows drawn and the hiding rules are the same as "The selected session's quota indicator" in
`docs/product/account-quotas.md`. On macOS, resting the pointer on the icon shows the application name as its tooltip,
regardless of the quota or keep-awake state. With a quota, its accessible name identifies the
agent and account and, for each window drawn, the window and its whole remaining percent, in the UI's language; without one it
names only Octoboard. While the computer is kept awake, it adds that Octoboard is keeping the computer awake — after the
quota's words, or in place of Octoboard's name alone. While the application shows more than one daemon, the accessible
name begins with the name of the daemon whose state it shows — This Mac (This PC on Windows), or the remote daemon's
whole name — followed by a dash. Selecting another session updates the indicator even if no
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

A click on the icon, left or right, opens the icon's menu, described below. A click does nothing else: a window comes
back through the menu's **Open board**, its update entry or one of its sessions.

On macOS, for about the first 0.6 seconds after the window is closed from native fullscreen, a click on the icon, left
or right, is lost and opens nothing; macOS swallows it while it leaves fullscreen.

## The icon's menu

The menu shows one daemon at a time, the one whose tab is chosen (see "The tabs" below), and below it the lines that are
the application's. From top to bottom:

1. **The row of tabs**, only while the application shows more than one daemon.
2. **The chosen tab's sessions**, only those whose shown status is **waiting for the user**, **paused**, **done** or
   **awaiting instructions**, in that order, one group per status. Each group has a heading that cannot be chosen and names
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
3. **The chosen tab's quota submenus**, one per added agent that is available — **Claude usage**, **Codex usage**, **Grok
   usage**, in the order of the added agents (see "Added agents" in `docs/product/agent-accounts.md`), the agents'
   names never translated — each opening to that agent's accounts and showing what Settings and the rail's panel
   show, including window order (see "Quota windows" and "How an account's quota is shown" in
   `docs/product/account-quotas.md`). An account that is turned off (see "Turning an account off" in
   `docs/product/agent-accounts.md`) is not listed, and the menu follows the switch as it flips. The exception is the
   account the selected session is running on, as that section has it: it stays listed in its place, its header line
   ending in **Disabled**, in secondary text after its reached-limit tag, or after its name and plan when there is
   no tag. An agent that is not added, not available, whose availability is not yet determined, or whose
   accounts are all turned off, bar that one, has none, and neither has an agent whose quota Octoboard does not read
   (see "Agents whose quota Octoboard does not read" in `docs/product/account-quotas.md`). The submenus are set off from the sessions and from
   Check for updates by separators. Inside a submenu, nothing can be chosen:
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
4. **Check for updates**, the application's, whichever tab is chosen, which opens Settings on its Updates section in
   the main window and checks when no newer version is known. Its label identifies a known newer version, or a download
   in progress, as described in "The menu bar update entry" in `docs/product/in-app-updates.md`.
5. **Open board**, which brings the chosen tab's daemon's window back from the background, as described under "Closing
   the window" in `docs/product/application-lifecycle.md`. A window that is shown, or minimized, is brought to the front,
   restored first if it was minimized. For a remote daemon whose connection is held for the browser it is **Open in
   browser** instead, which opens the browser at it again (see "Opening a remote daemon in the browser" in
   `docs/product/remote-daemons.md`).
6. **Disconnect**, for a remote daemon's tab alone, described below.
7. **Quit completely**, after a separator, described below.

A separator sets the sessions off from the quota submenus, and those from Check for updates; a tab that lists no
sessions or no quota submenus has no separator for them, so the menu never starts with a separator, under the row of
tabs or without it, and never shows two in a row. The open entry names the board rather than repeating the application
name; the quit entry makes clear that it ends the whole application, including background operation. The menu offers
no way to connect to a remote daemon.

The menu follows the sessions and update entry as they change, including while every window is closed into the
background. It is never replaced while open, though: while any of Octoboard's menus is open — this one or the
application menu — changes wait, and once no menu is open the icon's menu takes on the latest of them, so an open menu
stays open and unchanged until it is dismissed or an item is chosen. The icon's look waits with them (see "The icon's
look" above). It is worded wholly in the main window's language, whichever window is in front and whichever tab is
chosen (see "What follows the language" in `docs/product/language.md`). A tab's sessions and quota submenus are what
that daemon's window last sent; a daemon held for the browser has no window, so its tab lists neither. Until the main
window's UI has loaded and received the daemon's state — and throughout on the screen shown when the daemon failed to
start — the local daemon's tab holds no sessions and no quota submenus, and the application's lines are in English.

### The tabs

The local daemon's tab comes first, labelled **This Mac** (**This PC** on Windows), then one tab for each connected
remote daemon — shown in a remote window, or held for the browser — in the order they were connected in. A remote
daemon's tab is labelled with its short name: the first label of a dotted host name (`build` for `build.example.com`),
or the whole name for an address or any other name, cut to at most **12** characters, ending in "…" when cut. A tab
whose daemon has a session waiting for the user or dormant carries an orange dot after its name, centred on it and clear
of it (which the Windows flyout's tab also says to assistive technology); a name too long for its tab is cut before the
dot's room, never the dot. A tab comes with its daemon's connection and goes with it.

On macOS the row of tabs is a segmented control at the top of the menu, its background starting where the lines' text
does. **The tab chosen as the menu opens** is that of the daemon of the application's frontmost window, else the local
daemon's. Choosing another tab shows that daemon's lines in place while the menu stays open; the change of tab is the
one change the menu shows while open.

### Choosing a session

Choosing a session in the menu brings its daemon's window back as Open board does, then selects that session as if it
had been selected in the sidebar — the sidebar follows it to its console, and an interrupted session is resumed (see
"Selecting a session" in `docs/product/sidebar.md`) — and puts keyboard focus in its terminal. A session that has gone
away since the menu was opened only brings the window back.

### Disconnecting

**Disconnect**, on a remote daemon's tab, ends that daemon's connection: it closes its window, or lets go of the
connection held for the browser, and its tab goes. The application runs on, and nothing changes on the remote daemon
(see "Disconnecting and closing" in `docs/product/remote-daemons.md`).

### Quitting completely

The quit entry behaves like `Cmd+Q`, whichever tab is chosen: while a session or a busy shell terminal of the local
daemon holds a quit up it asks the same confirmation in the main window, bringing it back from the background to ask
it, and otherwise it quits at once (see "Quitting" in `docs/product/application-lifecycle.md`, which says what holds a
quit up).

## Hiding the icon

The **Show menu bar icon** switch in Settings' General section — **Show notification area icon** on Windows — is on
to begin with, and only the main window offers it; turning it off takes the icon out of the menu bar, or out of the notification area, and turning it back
on puts it back (see "Show menu bar icon" in `docs/product/general-settings.md`). Nothing else changes while it is hidden: the
menu and the icon's look keep following the sessions, the quota and the keep-awake hold, so the icon comes back as it
would have been. On macOS the icon keeps its place among the status items rather than being removed and made again.

Hidden, the icon's menu is not there to bring a window closed into the background back: opening Octoboard again, or one
of its notifications, still does (see "Closing the window" in `docs/product/application-lifecycle.md`). The choice is
kept by the desktop application in its settings file, together with keeping Octoboard running in the background (see
`docs/product/files-octoboard-owns.md`), and read as it starts: a hidden icon is hidden as the application creates it
at launch, before the window's UI has loaded, so it stays hidden on the screen shown when the daemon failed to start
too. Turning the switch takes effect at once and is written straight away. A configuration export that includes the
General section carries it (see "The settings a client holds" in
`docs/product/settings-in-a-configuration.md`).

## The notification-area icon on Windows

In the Windows application a click on the icon, left or right, opens a flyout the application draws itself instead of
the system's plain menu, which can hold neither tabs nor bars. The flyout holds what the sections above describe for the
macOS menu, drawn with the application's own components, in the same order and with the same separators: the tabs of the
daemons, the chosen daemon's session lines with their pictures, its quota rows with their bars, headers, ages and notes,
then Check for updates, Open board (or Open in browser), for a remote daemon Disconnect, and after a divider Quit
completely. The tabs stand on a rule that runs across the flyout, the chosen tab's underline lying on that rule, and a
tab's waiting dot follows its name. It opens beside the notification area, against the edge the taskbar sits on and
inside the screen's work area, with the tab of the window focused last chosen, else This PC's; opening it asks for a
fresh quota reading, and a change waits while it is open, as on macOS. It closes on a choice, on Esc, on a click outside
it or on the icon, and when it loses the focus. When a choice or an Esc cannot be handed to the application at all, the
flyout stays open instead and shows an error at its top, above the tabs, saying that the choice could not be carried
out (or, for Esc, that the menu could not be closed) and to try again, restarting the app if it keeps happening; the
error is announced as it appears, again on each further failure, and stays until the flyout is next shown. From the
keyboard the focus starts on the chosen tab (on the first line
when there is only one daemon); Left and Right move between tabs, Down or Tab from a tab to the first line that can be
chosen, Up from the first line back to the chosen tab, Up and Down between lines, Enter chooses, and Narrator reads each
line by the name the macOS menu gives it. It follows Octoboard's appearance, which by default is Windows' light or dark
app mode, and high contrast, including a change of either while it is open.

What differs from macOS, and why:

- **The icon** is Octoboard's application icon in its own colours, not a monochrome octopus tinted for the bar: the
  notification area does not tint pictures. While a session waits for the user or is dormant it carries the orange dot
  in its corner, by the same rule as above. While the computer is kept awake, larger bright red eyes are drawn over its
  own eyes, which are red already, so the change reads by their size and not only by their colour. No quota bars are
  drawn on it: the notification area's icon has a fixed size, which leaves them no room.
- **The icon's label is its tooltip**: what macOS gives as the icon's accessible name shows when the pointer rests on
  the icon, and is what the notification area reads out as the icon's name.
- **A quota row opens in place**, below its line, one at a time (Right or Enter opens, Left or Esc closes; Esc closes an
  open row before the flyout), instead of as a submenu beside the menu: the flyout is a window of its own, which a
  submenu would have to leave. Within an opened quota row, the accounts are set apart by separators that start where
  an account's content does, and the opened block is closed by a full-width separator when another line follows it.
- **Under high contrast** Windows draws its own high-contrast frame around the flyout.

