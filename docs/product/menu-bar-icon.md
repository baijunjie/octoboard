# Menu bar icon

The macOS application puts an icon in the system menu bar: an octopus, which carries an orange dot while a session waits for
the user, with a quota indicator for the selected session when one is available (see "The icon's look" below).
It is there for as long as the application runs — while the window is shown, while it is closed into the background,
and on the screen shown when the daemon
failed to start — and whatever the user chose about keeping Octoboard running when the window is closed (see
"Closing the window" in `docs/product/application-lifecycle.md`). A browser has no such icon.

## The icon's look

The icon has two attention states. In both, the octopus is monochrome, and macOS tints it to suit a light or dark menu bar:

- **The octopus alone**, while no session is waiting for the user.
- **The octopus with an orange dot** at its upper-right corner, separated from the octopus by a clear margin,
  while at least one session is waiting for the user. The dot is drawn in the system orange, which macOS resolves
  for a light or dark menu bar, rather than in the octopus's tint. The dot is about **6.5 pt** across within the
  18 pt icon. It appears with or without a readable quota.
  It is part of the icon's look and is not presented to assistive technology on
  its own. The sessions that count are exactly those the menu lists under **Waiting for you** (see "The icon's menu"
  below), across every console.

When the selected session has a readable account quota, bars below the octopus show the remaining percentage, with
a gap above them. They use the octopus's monochrome tint and span the icon's width, covering its lower tentacles while
keeping the octopus at its original scale; the icon keeps its **18 pt** footprint. As on the rail, they come in two
looks:

- **Two layers**, when the account has both a 5-hour and an account-wide Weekly window: two **outlined capsules**,
  each with a hollow unfilled portion — about **18 × 2.9 pt** for the 5-hour window and, about 0.8 pt beneath it,
  about **18 × 2.5 pt** for the Weekly window.
- **One bar** otherwise: an **outlined capsule** of about **18 × 4.9 pt**, with a hollow unfilled portion.

The attention dot stays at the upper right, clear of the bars. Without a readable quota the bars disappear; without
attention or a quota the original octopus returns.

The account, the windows drawn and the hiding rules are the same as "The selected session's quota indicator" in
`docs/product/account-quotas.md`. The icon has **no tooltip**. With a quota, its accessible name identifies the agent
and account and, for each window drawn, the window and its whole remaining percent, in the UI's language; without one
it names only Octoboard. Selecting another session updates the indicator even if no session data changed, and a
session's account switch, account edits, availability changes and new readings update it too. The bars fill from the
left in every UI language: the menu bar belongs to the system, not to the window, whose layout is what a right-to-left
language mirrors (see "Right-to-left layout" in `docs/product/window-layout.md`).

The look changes together with the menu: a change that has to wait for an open menu to close (see the end of "The
icon's menu" below) changes the look and its label only when it reaches the menu, so the icon and the menu always
agree on whether a session is waiting; the octopus's dot clearance and the orange dot always appear and go together. Until
the window's UI has sent its first menu — and throughout on the screen shown when the daemon failed to start — the icon
is the octopus alone.

## Clicking the icon

A click on the icon, left or right, opens the icon's menu, described below. A click does nothing else: the window
comes back through the menu's **Open Octoboard** or one of its sessions.

## The icon's menu

From top to bottom:

1. **The running sessions**, one group per status, each group under a heading that cannot be chosen and that names the
   status with the number of sessions in it: **Waiting for you (n)**, then **Working (n)**, then **Running in
   background (n)**, then **Awaiting instructions (n)**. A group with no session is left out. Sessions still
   starting, interrupted and archived sessions are never listed. A session running in background (see "Running in
   background" in `docs/product/sessions.md`) is listed in its own group, not under **Working** or **Awaiting
   instructions**.
   - **Headings.** A heading is system text at the menu font's size, regular weight, in the secondary text
     colour, with its status glyph to the left of the text. Its glyph and text sit where the session lines below place
     their agent marks and titles — the same left edges and the same line height, the glyph centred on the text — as
     macOS 26 lays out a menu item; another version of macOS may place a menu item's picture and title otherwise. It
     is not dimmed the way a menu line that cannot be chosen is, never highlights under the pointer, and is read by
     assistive technology once, as its menu item, the heading's words.
   - **Pictures.** Each heading has a still glyph for its status — the accent dot for working, the waving hand for
     waiting for you, the green hollow ring for running in background, the speech bubble for awaiting instructions —
     and each session line has the mark of its agent (Claude Code, Codex, Grok), as the sidebar draws them. The menu
     cannot animate, so the working dot and the ring do not pulse.
     The marks the sidebar draws in the text colour (Codex, Grok) are template images here, which macOS tints to suit a
     light or dark menu; the other pictures keep their own colours, chosen to read on both. The "N more…" line has
     none. Each quota submenu's opening line (below) carries the mark of its agent in the same way; the title is what
     is read, the mark adds nothing to it.
   - Within a group, sessions come in the order the sidebar lists them: console by console in the order the consoles
     were created, its console sessions before its projects' sessions, both in the sidebar's own order (see "Order of
     projects and sessions" in `docs/product/sidebar.md`). Every console is included, not only the one the sidebar
     shows.
   - Each session reads as its title and where it is — the project it runs in, or the console whose console session it
     is — worded as its system notification names it (see "The raised hand" in `docs/product/sessions.md`). A session
     titled after its project reads as the title alone rather than naming it twice. Like every other place, it reads
     the terminal title without the glyphs an agent animates at its start (see "Leading activity glyphs" in
     `docs/product/session-titles.md`).
   - A group names at most **10** sessions. Beyond that, a last line that cannot be chosen says how many more there are
     ("N more…"); the heading's number still counts them all.
   - With no session in any of these groups — none running, or only ones still starting — this part is a single
     heading, **No sessions running**, styled like the other headings but with no glyph.
2. **The quota submenus**, one per added agent that is available — **Claude quota**, **Codex quota**, **Grok
   quota**, in the order of the added agents (see "Added agents" in `docs/product/agent-accounts.md`), the agents'
   names never translated — each opening to that agent's accounts and showing what Settings and the rail's panel
   show, including window order (see "Quota windows" and "How an account's quota is shown" in
   `docs/product/account-quotas.md`). An agent that is not added, not available, or whose availability is not yet
   determined has none. The submenus are set off from the sessions and from Open Octoboard by separators. Inside a submenu,
   nothing can be chosen:
   - each account is a header line, at the small size of a section header, regular weight, in the secondary text
     colour, with its name — for the agent's default account the name the user gave it, else **Default** (see "The
     default account" in `docs/product/agent-accounts.md`) — and its plan after a dot where the agent names one
     ("Default · Max"), the default account first and then the accounts the user owns, each set off by a separator;
   - under the header, each window is one row: its name on the left ("5-hour"), "N% remaining · resets in
     <countdown>" ("82% remaining · resets in 1h 57m", "resets now" once the time has passed, no countdown for a
     window with no reset time) right-aligned in the secondary text colour, and a bar of what remains directly
     below, on a translucent grey track. The bar is orange for Claude Code and blue for Codex, each one fixed colour
     whether the menu is light or dark (their contrast against the track is not measured), and the text colour for
     Grok Build. A row whose text needs more room than the submenu's usual width widens the submenu rather than
     being cut short. A row is read by assistive technology once, as one phrase ("5-hour 82% remaining · resets in
     1h 57m"), by its menu item alone: neither the name, the detail nor the bar is read on its own;
   - a row, a header and a note are not dimmed the way a menu line that cannot be chosen is: the window's name is at
     full strength, and none ever highlights under the pointer. Their text and the bar's track take the system's
     colours for the menu they are drawn in, light or dark. A header and a note are each read by assistive
     technology once, as their menu item, their words;
   - an account never read shows **Reading quota…** in place of windows, and an account with no windows to show
     shows **Quota unavailable**, a note in the secondary text colour at the menu font's size, and without the
     reason. That includes an entry that has no windows and no failure to explain, which Settings and the rail show
     nothing for. An account that is not logged in (see "Quota states" in `docs/product/account-quotas.md`) shows the
     note **Not logged in** instead. One whose latest reading failed after a good one keeps its last windows under a
     note "Out of date: last read 12 minutes ago", without the reason either.

   Each submenu's opening line can show **remaining-quota bars** at its right, before the submenu arrow.
   It uses that agent's account configured for the **current console**, independently of the selected session that
   supplies the menu bar icon's capsule. A missing, deleted or wrong-agent account reference falls back to the
   agent's default account. With no current console there are no opener bars. When the account has both a 5-hour
   and an account-wide Weekly window, the 5-hour bar has a thinner Weekly bar beneath it. Otherwise there is one
   bar, with window priority 5-hour, then Weekly, then the credit pool; model-specific weekly limits are excluded.
   An unread account, a reading without an eligible window or a failed latest reading has no opener bars,
   including when the submenu retains stale windows. Switching consoles or editing their account choices updates
   the bars.

   These opener bars are solid rounded tracks in a muted tint, with the remaining share filled at full strength,
   aligned at the right across the opening lines. Their tint follows the native row in light and dark menus and
   while highlighted. The row keeps the system's pointer, highlight and keyboard submenu behavior, including
   Right Arrow to open it. Its
   accessible name identifies the agent and account and, for each window drawn, the window and whole remaining
   percent; the bars are not read separately. The row has no tooltip, which would float over the submenu it opens
   and repeat what that shows.

   Countdowns and the age of an out-of-date reading are as of the last time the menu was updated, which a change of
   the selected session, current console, sessions, consoles, projects, accounts, added agents, agents' availability, quotas or the UI's language causes, not the
   clock alone, so they can lag behind the clock. Opening the menu asks the daemon for a fresh reading, as opening
   the rail's panel or Settings' section does (see "When quotas are read" in `docs/product/account-quotas.md`); since
   the menu never changes while open, what that brings shows from the next time it is opened. The menu does not wait for the reply.
3. **Open Octoboard**, which brings a window closed into the background back, as described under "Closing the window"
   in `docs/product/application-lifecycle.md`. A window that is shown, or minimized, is brought to the front, restored
   first if it was minimized.
4. **Quit Octoboard**, after a separator, described below.

The menu follows the sessions as they change, including while the window is closed into the background. It is never
replaced while open, though: while any of Octoboard's menus is open — this one or the application menu — changes
wait, and once no menu is open the icon's menu takes on the latest of them, so an open menu stays open and unchanged
until it is dismissed or an item is chosen. The icon's look waits with them (see "The icon's look" above). It is
worded in the UI's current language (see "What follows the language" in `docs/product/language.md`). Until the
window's UI has loaded and received the daemon's state — and throughout on the screen shown when the daemon failed to
start — the menu holds only Open Octoboard and Quit Octoboard, in English, with no quota submenus.

### Choosing a session

Choosing a session in the menu brings the window back as Open Octoboard does, then selects that session as if it had
been selected in the sidebar — the sidebar follows it to its console (see "Selecting a session" in
`docs/product/sidebar.md`) — and puts keyboard focus in its terminal. A session that has gone away since the menu was
opened only brings the window back.

### Quit Octoboard

Quit Octoboard is a quit like `Cmd+Q`: while a session or a busy shell terminal holds a quit up it asks the same
confirmation, bringing a window closed into the background back to ask it, and otherwise it quits at once (see
"Quitting" in `docs/product/application-lifecycle.md`, which says what holds a quit up).

## What has been tried by hand

A closed window below means one closed into the background, which is what closing it does only while the user has
turned that on (see "Closing the window" in `docs/product/application-lifecycle.md`).

**Observed** in the packaged application: the menu's groups and session lines (seen under Simplified Chinese); the
menu updating within about 1.5 seconds of a session changing while the window was closed; opening and dismissing the
menu without choosing anything leaving a closed window closed; choosing a session bringing the window back with that
session selected; and Open Octoboard. What was seen of Quit Octoboard is under "Quitting" in
`docs/product/application-lifecycle.md`. Also observed, with a quota submenu popped up natively over a light-looking
desktop (not opened from the icon's own menu): a Codex account's two window rows with the name at full strength, the
detail right-aligned in grey and a bar whose fill is the remaining share (72% over a gray track), a Grok Build row at
0% as a bare rounded track, and a note under an account with no quota. After the texts were drawn rather than set in
text fields, a Claude Code account's window rows were seen again, again in a submenu popped up natively, with the
same look and size, and read once each by assistive technology, as a menu item whose title is the whole phrase. How
the current build renders the texts (their inset and vibrancy) has still to be seen live. Under Simplified
Chinese, in a light menu: the three session headings — **Waiting for you**, **Working**, **Awaiting instructions** —
each with its status glyph (the hand, the blue dot, the speech bubble) to the left of its text, lined up with the
session lines below: the glyphs' centres on the same vertical line as the agent marks' centres, the heading text on
the same left edge as the session titles, and each glyph centred vertically on its text to within half a point.

**Not yet observed** in the packaged application: the headings' styling as described above (their colour, weight, not
being dimmed, never highlighting, and how assistive technology reads them), **No sessions running**, and the account
headers in a submenu; the session lines' agent marks and the headings' status glyphs in a dark menu; either look on a
light menu bar, and the octopus alone while the icon is highlighted with its menu open; opening the menu asking for
a refresh of the quotas; a menu changing between light and dark while the app runs; a quota detail row never
highlighting under the pointer; and a row widened by a long window name.

**Observed in native previews and probes:** an AppKit image-view preview showed the original icon without a quota
and capsules at 0%, 65% and 100% in light and dark appearances. A native menu
probe showed the quota opener bars in light and dark menus, both resting and highlighted, verified Right Arrow
opening a submenu, and confirmed that assistive technology reads the whole quota label. These probes did not
launch the packaged application.

**Observed in a packaged application with preview fixtures:** the full-size octopus with its larger capsule and
enlarged orange dot at the upper right in the system menu bar. Clicking that application's own icon opened its native dark
menu, showing Claude at 65% and Codex at 88% as capsules aligned at the right before the submenu arrows. The native
quota submenu showed the account-wide Weekly window at 65% before Fable weekly at 10%. These checks did not
establish packaged-app behavior during an in-place theme change, keyboard submenu opening or quota-label
accessibility; the latter two were checked in the native probe above. The orange dot without a readable quota and
its behavior during waiting-state changes have not yet been observed in the packaged application.

**Observed caveat:** for about the first 0.6 seconds after closing the window from native fullscreen, a click on the
icon is lost — a right click does not even open the menu — apparently swallowed by macOS during its exit from
fullscreen. From about 0.65 seconds on, every click was answered.
