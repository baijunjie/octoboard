# Session rows

A session row lists a console session in the sidebar's console sessions section (see "The console sessions
section and the project list" in `docs/product/sidebar.md`), or a project's session under its project row (see
`docs/product/sidebar-project-rows.md`).

A session row shows the session's status glyph (see "Session statuses" in `docs/product/sessions.md`; the *switching
account* status takes its place while the session is shown as switching its account, see "While a session is
switching" in `docs/product/switching-session-accounts.md`), its agent's icon, its displayed title (see "Displayed
names" in `docs/product/session-titles.md`), and its pin button when it is pinned (see "Pinning" in
`docs/product/order-of-projects-and-sessions.md`). Outside focus mode, every console and project session row, nested
sessions included, shows its context-size chip immediately after its title when the context size is known and above
zero (see "The context-size chip" in `docs/product/session-context.md`). The title clips and fades when space is short,
keeping the context-size chip visible. The row's accessible description carries the context size. A project session
bound to a console session also carries that console session's **binding badge**: a small dot in the console session's
colour, whose tooltip names it (see "The binding badge" below); one bound to a project session carries none, since its
row is listed under that session instead (see "Project rows" in `docs/product/sidebar-project-rows.md`), with the owner
named in the row's own accessible name. A console session's own row carries no *binding* badge — what it shows there
instead is its own colour (see "The binding badge" below).
Selecting a row resumes it when it is interrupted, unless it is shown as switching its account; no menu item does that.
The row shows its agent as an icon alone; wherever the agent is named in visible session metadata, its name appears
without an account suffix. The agent's icon's tooltip has the agent's name on a line of its own, then a labelled list in plain text: **Account**, the account
the session runs under; **Model** and **Effort**, the model and reasoning effort it runs with, as "The model and effort
a session is shown running with" in `docs/product/model-and-reasoning-effort.md` decides them, each reading
**Default** when unknown. The account is the account's name, the default account's name for the agent's default
account (the name the user gave it, else **Default**; see "The default account" in `docs/product/agent-accounts.md`),
or, for a removed account, the directory the session recorded, shown left to right. A removed account that recorded no
directory has no Account row. A long value wraps inside the tooltip rather than running past it. The tooltip shows on
pointer hover only, and the icon adds no tab stop. The same tooltip is on focus mode's session cards and archived rows (see
"What both views show" in `docs/product/focus-mode.md`) and the archive view's rows (see "The archive view" in
`docs/product/archive-view.md`). The row's accessible name names the agent with its account and ends with
the model and effort, read as "<model> · <effort>" (or **Default model** when neither is known); a focus mode archived
row's accessible name ends the same way.

On sessions listed in a confirmation, the agent icon's tooltip is shorter: the agent's name followed by the account as
a small grey badge (the badge described in "Which agents the agent pickers list" in `docs/product/consoles.md`), with
no badge for a removed account that recorded no directory; it shows the same account, and wraps a long one, as above.
Confirmations provide the agent and account as hidden accessible text beside the icon.

A live session's action menu separates these groups, in order: Lock / Unlock, Pin / Unpin and Focus mode;
Rename, Copy session ID and Colour; Switch account, Hand off and Unbind; then Archive or Delete. Items appear only
where available as described below. An archived session's menu offers Reopen and Copy session ID, then a separator
and Delete.

A session row's action menu offers:

- **Lock** / **Unlock** — see "Session lock controls" below.
- **Pin** / **Unpin**.
- **Focus mode** — a console session's row only: enters that console session's focus mode (see
  `docs/product/focus-mode.md`).
  A project session's row has no such entry, and neither has an archived console session's.
- **Rename** — see "Renaming a session" in `docs/product/sessions.md`.
- **Copy session ID** — immediately after Rename, shows the session ID below its label in muted monospace text,
  left to right on one line within the same menu item, fading where it overflows. An overflowing ID scrolls while the
  pointer is over the ID line and returns to its beginning when the pointer leaves; it stays still under reduced motion.
  Selecting the item copies the full ID exactly to the clipboard. Available for project and console sessions wherever
  their action menus are offered, including right-click menus, focus mode and nested sessions. An archived session's
  menu offers it after Reopen. Where the platform has no clipboard, or it
  refuses, the ID is copied through the browser's selection copy instead. Success raises a notice toast; a copy neither
  way allows raises an error alert.
- **Colour** — a console session's row only: a submenu of swatches where the user chooses the console session's colour
  (see "The binding badge" below). A project session's row has no such entry, and neither has an archived console
  session's.
- **Switch account** — a submenu of the accounts of the session's agent, the default account first, with the one the
  session is on checked and doing nothing; picking another asks for confirmation, then moves the session to it (see
  `docs/product/switching-session-accounts.md`). A session whose account has since been removed still shows it,
  checked and named by the directory the session recorded. The entry is present for a console session's row as well
  as a project session's, and only when the session's agent has more than one account to be on; with just the default
  account it is left out rather than shown disabled, and so it is when every account but the session's own is turned
  off. A session in the archive has no such entry, and neither has one still starting. Each
  account shows its compact usage bar before its name, as in the account picker, and assistive technology reads what
  remains of its quota as the item's description (see "In an account picker" in `docs/product/account-quotas.md`). An
  account other than the session's own that is used up (see "A used-up account" in `docs/product/account-quotas.md`),
  as judged when the menu opens, is disabled, with the short reason under its name on two lines, saying it is used up
  and when it resets; it stays reachable with the arrow keys, its accessible name is just the account's name, and the
  full reason is its description. An account other than the session's own that is turned off (see "Turning an
  account off" in `docs/product/agent-accounts.md`), as judged when the menu opens, is not listed; the session's own
  account is listed, checked, even when it is turned off. An account other than the session's own whose sharing of
  the default configuration is being changed, as judged when the menu opens, is disabled like a used-up one, whatever its
  quota, with a short reason saying so; while the session's own account is having its sharing changed, every other account is disabled with a
  reason saying that the session's account is changing, as the session cannot be switched away from it either (see
  "When a change is carried out" in `docs/product/sharing-the-default-configuration.md`). While the session is shown as
  switching its account, every other account is disabled with a reason saying that the session is switching its
  account. A removed account named by
  its directory shows a key icon in the bar's place, lined up with the bars, and is never marked used up.
- **Hand off** — opens the handoff dialog, which moves the session's place to a new session of another agent (see
  "Handing off from the window" in `docs/product/handing-off-sessions.md`). Present for a project session's and a
  console session's row; an archived session has no such entry. Disabled while the session is *working* or
  *starting*, while it is locked, while no agent other than its own can be picked, while the session's own account is
  having its sharing changed, with that reason, and while the session is shown as switching its account, with that
  reason.
- **Unbind** — releases the session from its owner at once, without confirmation, leaving it the user's own, unbound
  session (see "Releasing a session from its owner" in `docs/product/changing-session-owners.md`); its row then shows
  it as unbound (see "Project rows" in `docs/product/sidebar-project-rows.md` and "The binding badge" below). Present
  only for a bound session's row — a project session bound to a console session or to a project session; an archived
  session has no such entry. Disabled while the session is *starting* and while it is shown as switching its account.
  A refusal is shown as an error alert.
- **Archive** — archives the session (see "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`). A
  session at its prompt or interrupted, with no unarchived session under it, is archived at once, unless a branch or
  worktree bound to it needs the user's word (see "The confirmation" in `docs/product/branch-and-worktree-cleanup.md`);
  any other asks for confirmation first, and so does one still being resumed or shown as switching its account. When the archived row takes keyboard focus with it, focus
  goes to the terminal. The console session can
  also archive itself once nothing under it is unarchived (see "A session archiving itself" in
  `docs/product/hub-orchestration.md`); its confirmation lists the
  interrupted sessions under it — bound to it, or to a lead session bound to it (see "Lead sessions" in
  `docs/product/hub-orchestration.md`) — inside a warning titled with their number, as the ones archived with it, and the
  archive is refused, with the sessions named, while any session under it has a process running. A project session that
  has started sessions is archived under the same rule, with the same confirmation while any of them is unarchived.
- **Delete**, in place of Archive for a session that has never been used and is not still starting (what counts as
  unused is in "Deleting an unused session" in `docs/product/deleting-sessions.md`); it stops the session and deletes
  it outright, leaving nothing in the archive. Deletion needs no confirmation and automatically moves its agent records
  to the Trash, as described in that section.

Archive and Delete menu items are disabled while the session is locked.

**The Delete or Archive button.** Just before its action-menu button, an unlocked session row — a console
session's included — carries a **Delete** button while the session is unused and not still starting (see "Deleting an unused session" in
`docs/product/deleting-sessions.md`). Agent records do not affect its availability. Being archived does not qualify
a session for the shortcut. Pressing it takes the same deletion action as the menu, without confirmation and with
automatic cleanup of its agent records; deletion is refused if the session is no longer unused when it takes place.
A row whose menu offers **Archive** instead carries an Archive button while archiving would go through without a
confirmation: the session is at its prompt or interrupted, no unarchived session is bound to it or to a session
bound to it, and it has been used. Pressing Archive does what the menu's **Archive** does — a session still being
resumed or shown as switching its account asks for confirmation first there too, and so does one with a bound branch or worktree that needs the user's
word — and when the row leaves with keyboard focus, focus goes to the terminal.
When the Archive conditions stop holding — the session starts working or raises a hand, or a session becomes bound
to it — the button goes away.

A pinned session's pin button stays before the shortcut. A mouse press on either shortcut neither selects the row
nor moves keyboard focus. Its tooltip is "Delete" or "Archive", while the name assistive technology announces for it
also names the session ("Delete session …" or "Archive session …"). It shows with the row's other controls (see "Rows,
names and keyboard focus" in `docs/product/sidebar.md`). When a shortcut goes away while holding keyboard focus, focus
goes to the row.

## Session keyboard shortcuts

These shortcuts act on the **selected session**, regardless of which row or control has keyboard focus, including
while typing in the terminal. They work both inside and outside focus mode. The table gives their defaults, with
**Cmd** on an Apple keyboard or **Ctrl** elsewhere; each can be changed (see `docs/product/keyboard-shortcuts.md`):

| Action | Default shortcut | Availability |
|---|---|---|
| Lock / Unlock | Cmd/Ctrl+Shift+L | A session that is not archived |
| Pin / Unpin | Cmd/Ctrl+Shift+P | A session that is not archived |
| Archive | Cmd/Ctrl+Shift+A | An unlocked session that is not archived |
| Copy session ID | Cmd/Ctrl+Shift+C | Any selected session, including an archived one |

Pinning also works when the selected session is the console session whose focus mode is open, even though its own
card offers no pin control. The archive shortcut follows the normal archive confirmation and protection rules (see
"Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`); for an unused session
it still archives, even when its menu and button offer Delete. It never deletes the session.

The corresponding session menu items show their current shortcut, and the Unlock, Unpin and Archive button tooltips
show it too. Project pin controls have no session shortcut hint. Menu and button actions still act on their own session;
the keyboard shortcuts always act on the selected one.

The shortcuts match physical keys whatever the keyboard layout. With no selected session, the key combinations
are left alone. With a selection, holding a combination does not repeat its action; unavailable actions do nothing.
No action runs while a dialog or menu is open or during input-method composition. Outside composition, recognized
combinations are withheld from the terminal even when the action is unavailable.

## The binding badge

A project session **bound to a console session** — one carrying the id of the console session it reports to (see
"Console sessions and project sessions" in `docs/product/sessions.md`) — carries a small dot in that console session's
colour on its sidebar row. A **lead session** carries it like any other session
bound to a console session (see "Lead sessions" in `docs/product/hub-orchestration.md`). On sidebar rows an unbound
session carries no badge, and neither does a session bound to a project session, which is listed under its owner
instead (see "Project
rows" in `docs/product/sidebar-project-rows.md`). The badge's tooltip names the owning console session; the badge is no
tab stop, so the tooltip shows on pointer hover only. The row's own accessible name carries the same fact in words for
assistive technology ("Bound to …"), in the same wording for a project session's owner as for a console session's, which
is the only place the nesting is put into words. The badge itself still carries no information its tooltip does not, so
colour alone never distinguishes two owners for a user who cannot tell the colours apart. A console session's own row
shows the same colour, decoratively, since the row's own label already names it, as do its own card in its focus mode,
its chip in a project's focus mode and its chip in the switch strip. Cards in a project's combined focus view carry the
dot for the console session above them, including cards inset under a lead session; the tooltip and accessible name
identify that console session. The other focus views carry no binding badge on their cards. See "A project's focus mode"
in `docs/product/focus-mode.md` for the switch and the card labels.

**Where the colour comes from.** A console session's colour is one of a fixed palette of 16, one colour per family —
red, orange, amber, yellow, lime, green, teal, cyan, sky, blue, indigo, violet, fuchsia, pink, brown and gray — each a
vivid colour that looks like its name. Some of them lie near the colours that mark a status elsewhere; that is
deliberate, since a console session's colour says nothing about its status, which its status glyph does. No colour is
ever named in the interface. Octoboard gives each new console session a colour picked at random among the palette
colours that none of the console's other console sessions that are not archived holds, or at random among all 16 once
every one is held; an archived console session's colour is free for others. Afterwards only the user changes it (see
**Choosing the colour** below); the console session's agent has no tool for it. A chosen colour counts as held for a
later console session's colour as an assigned one does. A console session keeps its colour, assigned or chosen, when
it is archived and when it is resumed, so with more than 16 at once, with one resumed after its colour has gone to
another, or with a colour chosen that another already has, two console sessions of one console can share a colour,
which is the other reason nothing is ever told apart by colour alone. A colour stored by an earlier version of
Octoboard is moved onto the nearest family of this palette when the daemon opens its store — olive to lime, jade to
green, azure to sky and rose to pink, while teal and violet stay as they are — so such a console session keeps much
the same colour. Each colour has its own light and dark value (see "What follows the choice" in
`docs/product/appearance.md`).

**Choosing the colour.** **Colour** in a console session's action menu — wherever that menu is offered, its sidebar row
and its own card in its focus mode among them, after **Rename** and the optional **Copy session ID** — is a submenu
entry, marked with the same chevron as **Switch account**. Its submenu holds the palette as swatches in two rows of
eight, in the order above. The console session's current colour is selected, marked with a check mark and a ring in its
own colour. Pointing at **Colour** opens the submenu without moving keyboard focus; `ArrowRight` (`ArrowLeft` under a
right-to-left language) or `Enter` on the entry opens it, or moves into it when it is already open, with focus on the
current swatch, or the first one when the current colour is not in the palette. Inside it the arrow keys move focus
between swatches; `ArrowLeft` from a swatch in the first column (`ArrowRight` under a right-to-left language), or
`Escape`, closes the submenu and puts focus back on **Colour** in the menu, which stays open. Clicking a swatch, or
pressing `Enter` or `Space` on the focused one, gives the console session that colour at once and closes the whole menu;
choosing the current colour just closes it. A click outside closes the menu without a change. When the menu closes,
keyboard focus goes where closing it always puts it (see "Rows, names and keyboard focus" in `docs/product/sidebar.md`).
For assistive technology the swatches are a list named by the **Colour** entry, and each swatch is announced by the
colour description generated in the interface language; the palette keeps those descriptions distinct in every offered
language and both appearances, except that in Arabic violet and fuchsia share one, and in Korean teal shares one with
cyan in the light appearance and with sky in the dark one. The colour is stored by the daemon with the console session,
so every window and every client sees it and it survives a restart. A change that fails is shown as an error alert, as a
failed **Pin** is (see "Error alerts" in `docs/product/error-alerts.md`). An archived console session's menu has no
**Colour** entry, and a project session has no colour to choose.

## Session lock controls

Lock controls are offered only for sessions that are not archived. Their action menus start with **Lock** or **Unlock**,
including nested sessions and a console session's own card in its focus mode. Protection and persistence follow "Locking
a session" in `docs/product/sessions.md`.

A locked session has a persistent yellow lock button before its other action controls, ahead of its pin
button when there is one. It remains visible without hover on sidebar rows and focus cards, a console session's own
card in its focus mode included. Pointer hover or visible keyboard focus changes the closed lock
to an open lock. Its tooltip offers unlocking, and its accessible name also identifies the session. Pressing it
unlocks that session; a mouse press neither selects the session nor moves keyboard focus. When the disappearing
button holds keyboard focus, focus goes to its row or card with a visible focus ring.

Archived sessions have no Lock or Unlock action or lock button, in either the full archive or
focus mode's compact preview. A stored lock remains effective: Delete is disabled for a locked archived session,
and the full archive's Delete all button is disabled while a listed session is locked. Reopen that session to reach
its live Unlock control. A lock among descendants outside the list still refuses deletion under the rules of
"Locking a session" in `docs/product/sessions.md`.
