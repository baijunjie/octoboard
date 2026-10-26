# The archive view

A project's, a console session's or a console's full archive opens in the archive view, over the terminal.

**View all** — in a project's or console's "View archive" submenu, or in a focus mode's archived list — opens the
archive view: **every** archived session of that project, every archived session under that console session (bound to
it, or bound to an archived lead session that is bound to it), or every archived console session of that console. A lead
session's own archived sessions are in that console session's archive only while the lead session is archived too; while
the lead session is still live they are in its project's archive alone. A project's heading in a console session's focus
mode is the exception to "that project": its View all opens that console session's archive (see "A console session's
focus mode" in `docs/product/focus-mode.md`). The submenus offer View all whenever their archive is nonempty, so the
archive's reopen and deletion controls remain reachable even when every session fits in the submenu. Focus mode's
preview offers View all whenever it omits any archived session (see "What both views show" in
`docs/product/focus-mode.md`).

**Archive previews** — the submenus above and focus mode's archived list — list the sessions the archive view lists
as rows of their own, most recently archived first, at most ten. A session inset there under its archived owner (see
below) is left out of the preview and reached through View all; every other archived session is a preview row,
including one bound to a project session that is not archived, such as a session of a lead session that is still
live, and one whose owner is no longer known. A submenu item shows the session's agent's icon, its title and, at the
item's end edge, how long ago it was archived, in muted text; these previews show no context-size chip. Relative
times are worded in the current language, read as now for anything under a minute, and keep up with the clock while
shown, never more than 30 seconds behind it. Resting the mouse on the time shows a tooltip of two lines: the archive
date above the creation date, each identified by its meaning ("Archived …", "Created …"). These are absolute dates
with year, month, day, hour and minute, formatted in the current language and the client's local time zone, as on
the archive view's rows below. The time is not a tab stop, so the tooltip does not open on keyboard focus, and on a
touch screen a tap on it selects the session rather than opening the dates in a popover (see "Tooltips on
indicators" in `docs/product/labels-and-tooltips.md`). Assistive technology receives both dates, the archive date
first, in place of the relative wording; in a submenu they are part of the item's accessible name. A session with
no known archive time shows no time, with no creation-time fallback. The agent icon carries no tooltip in a
submenu. Focus mode's preview rows are described in "What both views show" in `docs/product/focus-mode.md`.

- It **covers the terminal's area only**: the sidebar and the right pane stay as they are.
  The terminal goes on running beneath it and is not resized.
- Its header says how many archived sessions there are, over the title "Archived sessions" (for console sessions,
  "Archived console sessions"; for a console session's bound sessions, "Archived bound sessions"), with **Delete all**
  while there is at least one, and a **Close** button. Which project, console session or console it is for is shown in
  the top bar's breadcrumb (see "The top bar" in `docs/product/window-layout.md`).
- The list is in the order the sessions were archived, most recent first. It shows 30 rows and adds 30 more each time
  its end is scrolled into view. Each row shows the session's agent's icon, its title and, under the title, its **model
  line**, followed by its context-size chip when the context size is known and above zero (see "The context-size chip"
  in `docs/product/session-context.md`). The model line reads as a focus mode session card's does — the account the
  session runs under, then its model and reasoning effort — and the agent's name is not shown on the row (see "What both
  views show" in `docs/product/focus-mode.md`); it fades where it is cut off, with its full text as its tooltip while
  cut (see "Names too long for their space" in `docs/product/labels-and-tooltips.md`). The agent icon's tooltip lists
  the account, model and effort under the agent's name, as on a session row (see "Session rows" in
  `docs/product/sidebar-session-rows.md`). The row's accessible name ends with "<model> · <effort>" alone, as a focus
  mode session card's does. At the trailing end, a metadata block shows only dates: the archive date first, when known,
  then the creation date, each identified by its meaning. These are absolute dates with year, month, day, hour and
  minute, formatted in the current language and the client's local time zone. An unknown archive date is omitted; the
  creation date never stands in for it. The date block sits flush at the row's end in the reading direction, on the
  right in a left-to-right language. The row's **Reopen** and **Delete** buttons show only while the row is hovered or
  holds keyboard focus, and are still reached with Tab. Hidden, they take no room, so a resting row's title runs up to
  the date block; shown, they appear just before the date block (to its left in a left-to-right language), which stays
  at the row's end, and the title gives up room for them, cut off if it must be. On a pane too narrow for one line, the
  date block moves onto a line of its own, still at the row's end; while the row is hovered or focused the buttons join
  it there, so such a row grows by a line when its buttons appear. On a device whose primary input cannot hover — a
  phone, or a tablet's touch screen — they always show. Reopen and Delete have filled backgrounds distinct from the
  hovered row; Delete uses a soft danger fill.
- A session whose owner is **a project session listed here too** — an archived session of an archived lead session, or
  of any archived project session that had started sessions — is **inset under that owner**, with rounded connectors
  as in "Project rows" in `docs/product/sidebar-project-rows.md`, and follows it instead of being ranked against the
  rows beside it; one whose owner is not in the list is listed on its own. The row's accessible name also names the
  owner, in the wording a session row uses (see "The binding badge" in `docs/product/sidebar-session-rows.md`). The 30
  rows are counted as rows, so an inset row is never shown before the row it is inset under.
- **Selecting a row** — clicking it, or `Enter` or `Space` on it — selects its session, which closes the view (see
  "Selecting a session" in `docs/product/sidebar.md`): the session is shown still archived, its history in place of its
  terminal and the card that offers Reopen over it, just as when it is selected from a preview. Selecting it does not
  reopen it. Pressing the row's Reopen or Delete does not select the row. The row of the selected session is marked as
  selected.
- **Reopen** resumes the session and selects it, which closes the view (see "Selecting a session" in
  `docs/product/sidebar.md`).
- **Delete** deletes that session, with the confirmation rules in "Deleting archived sessions" in
  `docs/product/deleting-sessions.md` — for an archived owner, listing the archived sessions
  under it that are deleted with it; **Delete all** asks for confirmation, naming how many, then deletes every archived
  session in the view's scope — for a console session's archive, the archived sessions under it and not the console
  session itself; for a console's archived console sessions it also says how many archived sessions under them are
  deleted with them. What deleting does and does not remove is in "Deleting archived sessions" in
  `docs/product/deleting-sessions.md`.
- With nothing archived — including after everything has been deleted — the view stays open and says there are no
  archived sessions.

Archive rows, including inset sessions, follow "Items moving within a collection" in
`docs/product/collection-motion.md`.

**It closes** on its Close button, on `Escape` while keyboard focus is in it, when a session is selected (from it or
from anywhere else, as in "Selecting a session" in `docs/product/sidebar.md`), when this window opens a new session,
when another console is made the current one (see "What switching console shows" in `docs/product/console-switcher.md`),
and by itself when its project, its console session or its console no longer exists. Closing it hands keyboard focus to
the terminal.

The view slides up from below on opening and slides down on closing, each over 240 ms. Closing takes effect at
once: selection and navigation do not wait for the animation, and the departing view no longer accepts input or
keyboard focus. With reduced motion requested by the system, the view opens and closes without animation.

It takes keyboard focus as it opens, so keystrokes stop reaching the terminal it covers, and it keeps focus after a
deletion. It is a region of its own in the `F6` cycle (see "Moving focus between regions with F6" in
`docs/product/moving-focus-between-regions.md`). Below 1168 px, where the sidebar is a drawer, opening the archive
view closes the sidebar's drawer so that the view is not hidden behind it.
