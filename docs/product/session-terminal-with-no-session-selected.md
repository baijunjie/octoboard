# The session terminal with no session selected

What the session terminal's area (see `docs/product/session-terminal.md`) shows while no session is selected, and the
creation action it offers in a console with nothing in it.

## With no session selected

With no session selected, the terminal's area shows a full empty panel (see "The full panel" in
`docs/product/empty-states.md`). It offers the next creation action for the current console and focus scope (see "A
console with nothing in it" below); otherwise a terminal glyph, a heading saying no session is selected and a line
asking for a session to be selected. With no button in that panel, the terminal cannot take keyboard focus: whatever
would hand focus to it (hiding a pane that holds focus, for one) puts focus on the top bar's first enabled control
instead, and leaves it on nothing when it is already on nothing. While the archive view is open, focus meant for the
terminal goes to the archive view instead (see "The archive view" in `docs/product/archive-view.md`). With **no
console at all** there is no terminal in the window and the welcome screen fills the content panel instead (see
`docs/product/welcome-screen.md`).

**When the session on screen goes away while keyboard focus is in its terminal**, focus is handed on to the creation
action the empty panel now offers, or, with none, to the top bar's first enabled control; after a tap, to the top bar's
first enabled control (see "Focus after a tap" in `docs/product/moving-focus-between-regions.md`). Where the user
cleared the selection — a console with no session to show chosen from the rail, say, or Back to where none was selected
— the focus ring follows the input that cleared it: it shows after a key, and not after a pointer press. Where the
selected session's record goes away by itself, which nobody chose, the ring shows only when the last input was a key
(see "The focus ring after a focus move the window makes on its own" in `docs/product/moving-focus-between-regions.md`).
The same holds when focus is in an archived session's history instead of the terminal (see "Keyboard focus" in
`docs/product/session-history.md`).

## A console with nothing in it

With nothing selected, the terminal's full empty panel offers the next action for the **current** console and focus
scope:

- **In a project's focus mode**, when its live session list is empty, it offers **New session** for that project. The
  dialog has no "Report to" choice and creates an independent project coordinator, just as the focus view's header and
  compact empty row do. Archived sessions and live sessions under console sessions do not fill this list, so neither
  prevents the offer (see "A project's focus mode" in `docs/product/focus-mode.md`).
- **Outside focus mode**, the offer depends on whether the current console has a console session that is not archived,
  and on its number of projects:
  - With **no console session that is not archived**, it offers **New console session**, regardless of the number of
    projects or whether project sessions already exist. This opens the console-session dialog described in "The session
    dialog" in `docs/product/opening-sessions.md`.
  - With **a console session that is not archived and exactly one project**, it offers **New session** for that project,
    even when the project already has sessions. This opens the ordinary new-session dialog with its usual "Report to"
    choice.
  - With **a console session that is not archived and more than one project**, it asks for a session to be selected,
    with no creation button.
  - With **a console session that is not archived and no projects**, it offers **Add project**, opening the ordinary
    new-project dialog. The explanation also tells the user they can ask a console session to create a project for them.
- **When no agent is both added and installed** (see "When no agent can open a session" in
  `docs/product/opening-sessions.md`), an otherwise offered **New session** or **New console session** is replaced by
  what is missing. When an installed agent waits to be added, the panel carries the bot icon, says that no agent is
  added and that one is to be added to open a session, and offers **Add agent**, which opens Settings on its Agent
  accounts section with the Add agent dialog in front (see "Opening and closing Settings" in
  `docs/product/settings.md`). Otherwise it carries a warning icon, says that no agent was found and how to make one
  available, and offers no button. An added agent whose availability has not yet been determined does not prevent either
  offer. The **Add project** offer and the panel asking for a session to be selected do not depend on the agents.
- In a project's focus mode with a session that is not archived in its applicable list, or in a console session's focus
  mode, the panel asks for a session to be selected instead. A console session in focus is itself selectable even with
  no child sessions.

The sidebar's empty lists stay compact alongside this panel (see "Empty rows" in `docs/product/sidebar.md`).

- An **archived console session** does not satisfy the console-session condition outside focus mode; an interrupted
  console session does. Archived project sessions do not fill a project's focus-mode live list.
- A selected session suppresses the panel. Switching console shows the session last selected in the console switched to,
  or none (see "What switching console shows" in `docs/product/console-switcher.md`), so a console with no remembered
  session, a console just created among them, shows this panel for itself.
- The console read is the **current** one, not the one the sidebar is previewing while the pointer rests on another
  console's avatar on the rail (see "Previewing a console from the rail" in `docs/product/console-switcher.md`).
- **The archive view covering the terminal suppresses the creation offer**: while it is open, the underlying area shows
  the no-session panel instead (see "The archive view" in `docs/product/archive-view.md`).
- **Keyboard**: while the panel offers a creation button, the terminal's area is a stop in the `F6` cycle, and whatever
  hands keyboard focus to the terminal puts it on that button, just as with the cover's Reconnect button (see "Moving
  focus between regions with F6" in `docs/product/moving-focus-between-regions.md`). A panel with no button is skipped.
  Focus that is on nothing is left on nothing. When the creation button goes away while holding focus, because a session
  is selected or the console's sessions or projects change from this window or another client, focus goes to the
  terminal when it can take it, otherwise to the creation action the panel offers next, if any, and else to the top
  bar's first enabled control, its focus ring showing only when the last input was a key; after a tap, to the top bar's
  first enabled control.
