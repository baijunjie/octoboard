# The session terminal

The terminal in the window's content panel, where the selected session's agent is shown and typed into. Sessions
themselves — opening, statuses, archiving, interruption and resuming — are described in `docs/product/sessions.md`.

## With no session selected

With no session selected, the terminal's area shows a full empty panel (see "The full panel" in
`docs/product/empty-states.md`). It offers the next creation action for the current console and focus scope
(see "A console with nothing in it" below); otherwise a terminal glyph, a heading saying no session is selected and a
line asking for a session to be selected. With no button in that panel, the terminal cannot take keyboard focus:
whatever would hand focus to it (hiding a pane that holds focus, or the session going away while the terminal holds
it) puts focus on the top bar's first enabled control instead, and leaves it on nothing when it is already on
nothing. With **no console at all** there is no terminal in the window and the welcome screen fills the content panel
instead (see `docs/product/welcome-screen.md`).

## Typing and the mouse

The selected session's terminal is live: keystrokes go straight to the agent, exactly as in a system terminal. Mouse
reporting works, so an agent's own mouse-driven TUI is usable. `Ctrl+C` reaches the agent, and so do `Tab` and
`Shift+Tab`. `F6` and `Shift+F6` never do: they move keyboard focus to another region of the window (see "Moving
focus between regions with F6" in `docs/product/moving-focus-between-regions.md`).

## Output, scrollback and size

- Attaching to a running session replays the **most recent 2 MiB** of that session's raw output, then follows live
  output. The terminal itself keeps 10,000 lines of scrollback. Dormant sessions show the saved preview described in
  "Interrupted, archived and dormant sessions" below.
- A reattach always redraws from the replay rather than appending to what is on screen, so scrollback older than the
  replay window is lost on every reconnect.
- Whatever changes the terminal pane's size resizes the agent's terminal: resizing the window, resizing
  the sidebar or the right pane, and hiding or showing either of them all do; a pane floating in or a drawer
  opening over the terminal does not (see `docs/product/window-layout.md`). The
  screen follows at once, but the agent is told the new size only once it has held still for about
  120 ms, and only when it differs from the size the agent already has, so a drag sends the agent one
  size change rather than one for every step of the drag. The window has a minimum size, sized so that
  the sidebar, the terminal and the right pane all stay usable side by side — see "The window's
  minimum size" in `docs/product/window-layout.md`.
- The terminal's colours follow the window's light or dark appearance, and its text is held to a minimum contrast
  against its cell's background whatever colours the agent prints — see "What follows the choice" in
  `docs/product/appearance.md`.

## Reconnecting

- A client that stops draining output for more than a few seconds is dropped by the daemon rather than letting output
  buffer without bound. The application then reconnects by itself, with a backoff, up to five times. **While those
  attempts are under way the terminal is covered by a spinner and a line saying it is reconnecting**, over whatever
  output was on screen. **Once the attempts are spent the cover becomes a disconnected panel**, in the shape every empty
  state takes (see "The full panel" in `docs/product/empty-states.md`): an unplugged-cord glyph, a heading saying the
  terminal is disconnected, and a **Reconnect** button. The top bar shows nothing about the terminal's connection in
  either state. Assistive technology is told each of these states as it begins, by the terminal's area alone; the
  disconnected panel's message and button can also be reached when browsing, while the spinner covers cannot. A
  reconnect in the background never steals keyboard focus.
- **The Reconnect button is offered only while the connection to the daemon is open.** While it is not, the
  disconnected panel shows its message without the button, and the connection banner and its Retry are the way back
  (see "Losing the daemon connection" in `docs/product/application-lifecycle.md`).
- **Pressing Reconnect starts the automatic attempts over and moves keyboard focus to the terminal.** A mouse press
  on the button does not itself take keyboard focus.
- **Keyboard focus and the Reconnect button**: whatever hands keyboard focus to the terminal — `F6` reaching the
  terminal's region, for one (see "Moving focus between regions with F6" in
  `docs/product/moving-focus-between-regions.md`) — puts it on the Reconnect button instead while the button is
  offered, since the terminal's own `Tab` goes to the agent. When the button appears while keyboard focus is in the
  terminal's area, focus moves to it; focus anywhere else in the window is left where it is. When the button goes away
  while it holds focus, focus goes to the terminal, or to the top bar's first enabled control when the terminal
  cannot take it.
- **A terminal whose automatic attempts are spent is tried again only** by pressing Reconnect, by selecting another
  session and then this one again, or, by itself and without moving keyboard focus, when the application re-reads the
  daemon's whole state, as it does on each new connection to the daemon (see "Losing the daemon connection" in
  `docs/product/application-lifecycle.md`). Each of these starts the five attempts over. An update to the session's status does not try it again.

## Loading

**Until the session's first output is on screen, the terminal is covered by a loading state** — a spinner and a
line saying the session is being resumed, while a resume of it is under way or it has no process yet, and that the
terminal is loading otherwise. It covers whatever comes before that first output, every time the terminal attaches
— when a session is selected, resumed or reconnected: the resume starting the process, the connection being made,
and a freshly started agent drawing its first screen. An agent that prints nothing does not keep it up: once the
terminal has been connected for 10 seconds without output, the bare terminal is shown. While the terminal is not
connected — a first connection that hangs, an attempt started with Reconnect, a resume still starting the process —
the loading state stays, however long that takes. While the terminal is reconnecting
or disconnected, the cover described in "Reconnecting" above is shown in its place.

## Interrupted, archived and dormant sessions

- Selecting an interrupted session resumes it rather than attaching. Two ways of reaching one only show it, with its
  card, and never resume it: moving with Back or Forward (see "Moving back and forward" in
  `docs/product/navigation-history.md`), and `Ctrl+Tab` / `Ctrl+Shift+Tab` in a console session's focus mode (see
  "The switch strip" in `docs/product/focus-mode.md`). While a session has no process and no resume of
  it is under way, the terminal shows a card at its bottom centre: for an interrupted session it reads "Not running"
  and offers "Resume"; for an archived one it says the session is archived and that typing reopens it, and offers
  "Reopen". The card is hidden while the session is being resumed.
- **Typing into an archived session's terminal reopens it.** The first input starts the relaunch, and what is typed
  until the session is connected is held and handed to the agent once it is, so it reaches the agent's prompt.
- **A dormant session shows its own saved terminal preview**, behind that card, read-only. Entering dormancy replaces
  the live screen with that preview, even while the same session remains selected; reopening the view or loading it
  on another client shows the same saved content. Selecting a different dormant session shows its own preview, or a
  blank terminal if it has none. Clicking, selecting, scrolling or focusing the terminal does not resume it. Nothing
  typed reaches the stopped process; typing into an archived session reopens it as above.
- **Archiving preserves the screen before the agent is asked to exit.** The same rule applies when switching accounts
  or stopping the daemon normally: cleanup, a clear screen or leaving an agent's full-screen interface cannot replace
  the captured preview. An agent that exits or crashes on its own instead leaves its final screen, so content it clears
  before exiting need not survive. These rules are shared by every supported agent.
- The preview contains the active terminal screen, retaining text colours and styles. With the normal screen active,
  it includes up to **10,000 rows of scrollback**. An agent's alternate full-screen interface has no scrollback:
  its preview contains only that visible screen, without the hidden normal screen's history. The preview is
  independent of the live replay's 2 MiB limit and wraps to the reader's terminal width. It is **not a full
  conversation record**: content an agent already overwrote inside its full-screen interface is not reconstructed.
  The agent's own stored conversation is separate.
- Each process end replaces the previous preview; starting a new process discards it. While a resume is under way,
  the loading state covers it, so it is what shows when a resume fails before starting a process. A successful resume
  redraws from the new process's replay rather than appending to the preview. Older saved output is displayed as
  recorded, including any clearing it contains; it is not converted into a recovered screen. A session whose process
  ended before Octoboard kept output, or with the daemon killed outright, shows a blank terminal, even if an earlier
  process left some. Deleting the session's record deletes its saved preview too (see "Files Octoboard owns" in
  `docs/product/application-lifecycle.md`).

## Links in the output

Web addresses in a session's terminal are links:

- **What is a link**: an `http` or `https` address written out as plain text, and an OSC 8 hyperlink the program
  emits, whose visible text may differ from the address it leads to. An OSC 8 hyperlink to any other scheme is not a
  link at all. A link is underlined while the pointer is on it.
- **Where a plain-text address ends**: at the first non-ASCII character, or at an ASCII delimiter such as whitespace,
  a quote mark, a parenthesis, a brace or an angle bracket; a trailing punctuation mark such as `.`, `,`, `:`, `!` or
  `?` is left out. So an address written straight into CJK
  prose stops before the full-width bracket or the words that follow it. An address that spells its host or path in
  raw non-ASCII characters is cut short at the first one, and one whose host starts with one (`http://例え.jp`) is
  not a link at all; a percent-encoded address is recognised whole. An OSC 8 hyperlink is not affected, its target
  being the one the program gives.
- **A link opens only with the shortcut modifier held**: `Cmd`+click on a Mac, `Ctrl`+click elsewhere. A click with
  both `Cmd` and `Ctrl` held opens nothing. The address opens in the user's own browser — to a new tab, in the UI
  opened in a browser — as a web link in a Markdown document does (see "A Markdown file as a document" in
  `docs/product/project-pane.md`).
- **A plain click on a link is an ordinary click in the terminal**: it selects text as anywhere else and opens
  nothing. Neither a plain click nor opening a link resumes a dormant session.
- **An OSC 8 hyperlink shows its real target as a tooltip** once the pointer has rested on it for the window's usual
  tooltip delay (see "How a tooltip behaves" in `docs/product/labels-and-tooltips.md`). A plain-text address has no
  tooltip, its text being its target. The tooltip goes when the pointer leaves the link, when the terminal's view
  scrolls, and when the terminal is cleared to draw a session afresh — selecting another session, a reattach, or a
  dormant session's preview replacing the screen. It also closes on any key press or pointer press, so pressing
  `Cmd` or `Ctrl` to open the link closes it. The target reaches no assistive technology, as none of the terminal's
  output does.
- **An address the program itself breaks onto the next line is cut**: when the program ends the line partway through
  an address — as Claude Code does inside a table's cells — only the part on the first line is recognised, and that
  is what opens. An address that runs onto the next row only because the terminal is too narrow for it is recognised
  whole.

## A console with nothing in it

With nothing selected, the terminal's full empty panel offers the next action for the **current** console and focus
scope:

- **In a project's focus mode**, when its live session list is empty, it offers **New session** for that project. The
  dialog has no "Report to" choice and creates an unbound session, just as the focus view's header and compact empty
  row do. Archived sessions and live sessions under console sessions do not fill this list, so neither prevents the
  offer (see "A project's focus mode" in `docs/product/focus-mode.md`).
- **Outside focus mode**, the offer depends on whether the current console has a console session that is not
  archived, and on its number of projects:
  - With **no console session that is not archived**, it offers **New console session**, regardless of the number
    of projects or whether project sessions already exist. This opens the console-session dialog described in
    "Opening a session" in `docs/product/sessions.md`.
  - With **a console session that is not archived and exactly one project**, it offers **New session** for that
    project, even when the project already has sessions. This opens the ordinary new-session dialog with its usual
    "Report to" choice.
  - With **a console session that is not archived and more than one project**, it asks for a session to be selected,
    with no creation button.
  - With **a console session that is not archived and no projects**, it offers **Add project**, opening the ordinary
    new-project dialog.
- **When all supported agents are known to be unavailable**, an otherwise offered **New session** or **New console
  session** is replaced by an explanation that no agent was found and how to make one available, with no creation
  button. Availability that has not yet been determined does not prevent either offer. The **Add project** offer and
  the panel asking for a session to be selected do not depend on agent availability.
- In a project's focus mode with a session that is not archived in its applicable list, or in a console session's
  focus mode, the panel asks for a session to be selected instead. A console session in focus is itself selectable
  even with no bound sessions.

The sidebar's empty lists stay compact alongside this panel (see "Empty rows" in `docs/product/sidebar.md`).

- An **archived console session** does not satisfy the console-session condition outside focus mode; an interrupted
  console session does. Archived project sessions do not fill a project's focus-mode live list.
- A selected session suppresses the panel. The selection survives a console switch, so immediately after switching
  to an empty console, or creating one, the terminal may still show the previous console's selected session; the
  sidebar's compact row carries the offer then.
- The console read is the **current** one, not the one the sidebar is previewing while the pointer rests on another
  console's avatar on the rail (see "Previewing a console from the rail" in `docs/product/sidebar.md`).
- **The archive view covering the terminal suppresses the creation offer**: while it is open, the underlying area
  shows the no-session panel instead (see "The archive view" in `docs/product/sidebar.md`).
- **Keyboard**: while the panel offers a creation button, the terminal's area is a stop in the `F6` cycle, and whatever
  hands keyboard focus to the terminal puts it on that button, just as with the cover's Reconnect button (see "Moving
  focus between regions with F6" in `docs/product/moving-focus-between-regions.md`). A panel with no button is skipped.
  Focus that is on nothing is left on nothing. When the creation button goes away while holding focus, because a
  session is selected or the console's sessions or projects change from this window or another client, focus goes to
  the terminal when it can take it, otherwise to the top bar's first enabled control.
