# The session terminal

The terminal in the window's content panel, where the selected session's agent is shown and typed into. Sessions
themselves are described in `docs/product/sessions.md`, opening them in `docs/product/opening-sessions.md`, their
statuses in `docs/product/session-statuses.md`, and archiving, interruption and resuming in
`docs/product/archiving-and-resuming-sessions.md`. What an archived session shows in the terminal's place is in
`docs/product/session-history.md`.

## With no session selected

With no session selected, the terminal's area shows a full empty panel (see "The full panel" in
`docs/product/empty-states.md`). It offers the next creation action for the current console and focus scope (see "A
console with nothing in it" below); otherwise a terminal glyph, a heading saying no session is selected and a line
asking for a session to be selected. With no button in that panel, the terminal cannot take keyboard focus: whatever
would hand focus to it (hiding a pane that holds focus, or the session going away while the terminal holds it) puts
focus on the top bar's first enabled control instead, and leaves it on nothing when it is already on nothing. With **no
console at all** there is no terminal in the window and the welcome screen fills the content panel instead (see
`docs/product/welcome-screen.md`).

## Typing and the mouse

The selected session's terminal is live: keystrokes go straight to the agent, exactly as in a system terminal. Mouse
reporting works, so an agent's own mouse-driven TUI is usable: while an agent has mouse reporting on (Claude Code's
fullscreen view, for one), a plain drag goes to the agent and makes no selection of the terminal's own; how to select
anyway is in "Copy, paste and selection" below. An Option-click (Alt-click outside macOS) moves the agent's prompt
cursor to the clicked cell, by sending it arrow keys, only on a screen without mouse reporting and while the terminal is
scrolled to the bottom; under mouse reporting it never sends arrow keys. `Ctrl+C` reaches the agent, and so do `Tab` and
`Shift+Tab`. `F6` and `Shift+F6` never do: they move keyboard focus to another region of the window (see "Moving focus
between regions with F6" in `docs/product/moving-focus-between-regions.md`).

## Copy, paste and selection

With a session selected that is not archived, right-clicking its terminal opens an editing menu in both the desktop app
and a browser. On macOS, Control-click opens it too. With keyboard focus in the terminal, the context-menu key or
`Shift+F10` opens the same menu; neither key is sent to the agent. Opening it preserves the terminal's selection, and
closing it returns keyboard focus to the terminal. There is no separate menu button.

- **Copy** copies the terminal text selected when the menu opened or, with nothing selected in the terminal, the text a
  program last copied, while that is still on offer (see "Text a program copies" below). It is disabled with neither.
  When it is disabled while the running program has mouse reporting on, it says under its name how to make a
  selection anyway (see **Selecting with the mouse** below): "Option-drag to select" on macOS, "Shift-drag to select"
  elsewhere. macOS here is what the terminal itself treats as macOS, a browser platform of `Macintosh`, `MacIntel`,
  `MacPPC` or `Mac68K`, so an iPhone or iPad gets the Shift-drag hint. Without mouse reporting it is disabled with no
  reason given.
  Where the platform has no clipboard, or it refuses, the text is copied through the browser's selection copy instead.
- **Paste** reads text from the clipboard and sends it as terminal input, respecting the running program's bracketed
  paste mode. It is disabled without clipboard reading support, saying under its name that the clipboard is
  unavailable, and while the selected session cannot accept input, with no reason given (the terminal itself shows why,
  by its reconnecting spinner or its disconnected notice).
- **Select all** selects the terminal's contents, including its available scrollback, for copying.

The clipboard is read only when Paste is chosen, so merely opening the menu does not request clipboard access. If
clipboard access completes after the user switches sessions or the original session can no longer accept input, the
paste is discarded. It is never redirected to another session. A failed clipboard read, or a copy that neither way
allows, shows an error alert.

**Selecting with the mouse.** A drag selects terminal text, except while the running program has mouse reporting on:
then a plain drag goes to the program, which may highlight and copy the text itself (Claude Code's fullscreen view
does), but makes no selection of the terminal's own. **Option-drag on macOS, and Shift-drag on other platforms, selects
even under mouse reporting.** On macOS an Option-drag therefore always makes an ordinary selection, never a column
(block) one; on other platforms an Alt-drag on a screen without mouse reporting selects a column.

Moving the pointer away to right-click keeps a selection made under mouse reporting: while the terminal holds a
selection, pointer motion with no button held is not reported to a program that follows all motion, since the report (in
the SGR encodings Claude Code uses) would clear the selection, so that program sees no hover until the selection is
gone. Typing, and a click or the wheel reported to the program, may still clear it.

The selection also survives the program re-asserting its modes, as Claude Code does whenever input reaches it after a
few idle seconds (a focus report as the window comes back to the front, say): output that only sets modes, leaving mouse
reporting as it was, leaves the selection where it was, unless a key, a paste, a press that cannot open the editing
menu, or a drag or the wheel reported to the program reached the terminal while that output was being parsed. A
re-assertion in the middle of a forced drag (Option-drag, or Shift-drag elsewhere) still ends the drag, keeping what it
had selected so far. The program does clear the selection when re-asserted mouse modes come in the same output as
anything else, such as text it redraws, when it switches to another mouse reporting mode, and when it switches between
the main and the alternate screen.

**Text a program copies.** A program running in the terminal can put text on the clipboard with the OSC 52 escape
sequence, as Claude Code does with the text selected in its fullscreen view. Whichever selection the request names, the
text goes to the one system clipboard. Only writes are honoured:

- a request to read the clipboard is never answered, and a request to clear it is ignored;
- a request in output written before the session was attached is ignored: the replay that opens each attach (or, when
  there was nothing to replay, the first live output after it), a dormant session's saved output, and output still
  waiting to be drawn when the screen is reset, as on a switch to another session;
- where the platform has no clipboard nothing is put on it; the text is still offered to Copy in the editing menu, which
  falls back to the browser's selection copy.

A write the clipboard fails shows the same error alert as a failed Copy, with its clipboard remedy (see "Recovery and
unexpected errors" in `docs/product/error-alerts.md`). A browser's refusal of a copy the user did not just make (no
recent gesture, or the page not focused) stays silent, so in a browser that refuses every such copy (Safari) a program's
copy can fail without notice; Copy in the editing menu, a gesture of the user's, then puts it there.

**The menu offers what a program copied.** The text of the latest honoured write stays on offer to the editing menu's
Copy while the terminal has no selection of its own; a selection in the terminal comes first. It is offered until the
user turns back to the program or the screen moves on: a key typed to the program (`Control-C` included), a paste, a
press in the terminal other than a right-click or, on macOS, a `Control`-click (a click, a drag, an Option-click), a
reset of the screen (another session selected, a reconnect) or the terminal closing. Moving the pointer and the
right-click that opens the menu leave it on offer, as does the wheel while the program has mouse reporting on; on an
alternate screen without mouse reporting the wheel sends the program arrow keys, which drop it like any other key. The
program's next copy replaces it.

## Output, scrollback and size

- Attaching to a running session replays the **most recent 2 MiB** of that session's raw output, then follows live
  output. The terminal itself keeps 10,000 lines of scrollback. An interrupted session shows the saved preview described
  in "Interrupted, archived and dormant sessions" below; an archived one shows its history instead.
- **Once a session has written more than those 2 MiB, the replay first restores the terminal modes the agent has on at
  that moment**, since agents announce them only once, at startup: the alternate screen, mouse reporting, focus
  reporting, bracketed paste, application cursor and keypad keys, a hidden cursor and disabled line wrap. A full-screen
  agent therefore stays full-screen and keeps answering the mouse, the wheel included, after a reattach.
  Keyboard-protocol enhancements an agent asked for (the kitty keyboard flags) are not restored.
- A reattach always redraws from the replay rather than appending to what is on screen, so scrollback older than the
  replay window is lost on every reconnect.
- Whatever changes the terminal pane's size resizes the agent's terminal: resizing the window, resizing the sidebar or
  the right pane, hiding or showing the sidebar, and the right pane coming or going as its panels open and close all do;
  a pane floating in or the sidebar's drawer opening over the terminal does not (see `docs/product/window-layout.md` and
  `docs/product/sidebar-drawer.md`).
  The screen follows at once (except while a docked column eases, below), but the agent is told the new size only once
  it has held still for about 120 ms, and only when it differs from the size the agent already has, so a drag sends the
  agent one size change rather than one for every step of the drag. The window has a minimum size, sized so that the
  sidebar, the terminal and the right pane all stay usable side by side — see "The window's minimum size" in
  `docs/product/window-layout.md`.
- While the docked sidebar's column is easing open or closed (see "Hiding the sidebar" in
  `docs/product/window-layout.md`), the terminal's grid keeps the size it had when the ease began, and is fitted to its
  area once, when the column comes to rest — at most about a quarter of a second after the toggle was last pressed. Any
  other change to the terminal pane's size during the ease, a window resize included, is held until then too. Until that
  fit, a closing column leaves blank space along the terminal's right edge and an opening one covers its rightmost
  columns. The agent is then told the new size as above. Where the system asks for reduced motion the column does not
  ease, and the terminal follows at once. The right pane does not ease at all.
- The terminal's colours follow the window's light or dark appearance, and its text is held to a minimum contrast
  against its cell's background whatever colours the agent prints — see "What follows the choice" in
  `docs/product/appearance.md`.

## Answers to a program's queries

A program can ask its terminal about itself through its output — the cursor's position, the background colour, the
features the terminal supports — and reads the answer back as input. Prompt libraries ask this way before drawing a
prompt (`gh`'s does, for one), and a program handed an answer it is no longer waiting for reads it as typed text. Every
client showing the terminal at the same moment (another window, a browser, a phone) runs a terminal of its own that
answers, yet **the program gets one answer per query**:

- **The first client to answer a query is the one whose answer the program gets**; the copies the other clients send
  are dropped. So is an answer to output older than output whose query has already been answered.
- **A query is answered within 5 seconds of its output or not at all.** A later answer is dropped, since a program
  waits no longer than that and would take a late answer as typed input. A client that falls that far behind, such as
  a browser tab the browser throttles in the background, has its answers dropped even when it is the only one attached.
- **Attaching does not answer old queries**: in the output replayed on attaching (selecting a session, reconnecting,
  reopening the Terminal panel), queries already answered, or more than 5 seconds old, are not answered. A query
  no client answered, from a program that gave up waiting in under 5 seconds, can still be answered by a client
  attaching within those 5 seconds.
- **Read-only screens answer nothing**: an interrupted session's saved preview, a session frozen for archiving, and a
  shell terminal whose shell has exited.
- **A report that the terminal gained or lost keyboard focus is not an answer**: each client's report reaches the
  program as that client's own input.

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
- **The Reconnect button is offered only while the connection to the daemon is open.** While it is not, the disconnected
  panel shows its message without the button, and the connection banner and its Retry are the way back (see "Losing the
  daemon connection" in `docs/product/application-lifecycle.md`).
- **Pressing Reconnect starts the automatic attempts over and moves keyboard focus to the terminal.** A mouse press on
  the button does not itself take keyboard focus.
- **Keyboard focus and the Reconnect button**: whatever hands keyboard focus to the terminal — `F6` reaching the
  terminal's region, for one (see "Moving focus between regions with F6" in
  `docs/product/moving-focus-between-regions.md`) — puts it on the Reconnect button instead while the button is offered,
  since the terminal's own `Tab` goes to the agent. When the button appears while keyboard focus is in the terminal's
  area, focus moves to it; focus anywhere else in the window is left where it is. When the button goes away while it
  holds focus, focus goes to the terminal, or to the top bar's first enabled control when the terminal cannot take it.
- **A terminal whose automatic attempts are spent is tried again only** by pressing Reconnect, by selecting another
  session and then this one again, or, by itself and without moving keyboard focus, when the application re-reads the
  daemon's whole state, as it does on each new connection to the daemon (see "Losing the daemon connection" in
  `docs/product/application-lifecycle.md`). Each of these starts the five attempts over. An update to the session's
  status does not try it again.

## Loading

**Until the session's first output is on screen, the terminal is covered by a loading state** — a spinner and a line
saying the session is being resumed, while a resume of it is under way or it has no process yet; that the session is
starting, while it is still *starting* (see `docs/product/session-statuses.md`), during which there is nothing to
connect to and the terminal attaches once the agent is up; and that the terminal is loading otherwise. It covers
whatever comes before that first output, every time the terminal attaches — when a session is selected, resumed or
reconnected: the resume starting the process, the connection being made, and a freshly started agent drawing its first
screen. An agent that prints nothing does not keep it up: once the terminal has been connected for 10 seconds without
output, the bare terminal is shown. While the terminal is not connected — a first connection that hangs, an attempt
started with Reconnect, a resume still starting the process — the loading state stays, however long that takes. While
the terminal is reconnecting or disconnected, the cover described in "Reconnecting" above is shown in its place. While
the session is shown as switching its account, the cover described in "While a session is switching" in
`docs/product/switching-session-accounts.md` is shown in place of all of these. Assistive technology is told the line of
the loading state and of the switching cover as each begins, by the terminal's area alone, as it is told the covers in
"Reconnecting" above.

## Interrupted, archived and dormant sessions

- Selecting an interrupted session resumes it rather than attaching, unless it is shown as switching its account (see
  "While a session is switching" in `docs/product/switching-session-accounts.md`). Two ways of reaching one only show
  it, with its card, and never resume it: moving with Back or Forward (see "Moving back and forward" in
  `docs/product/navigation-history.md`), and Next or Previous console session (`Ctrl+Tab` / `Ctrl+Shift+Tab` by default)
  in a console session's focus mode (see "The switch strip" in `docs/product/focus-mode.md`). While a session has no
  process and no resume of it is under way, the terminal shows a card at its bottom centre, raised over it by a large
  shadow with no outline (see "The window chrome and the content panel" in `docs/product/window-layout.md`): for an
  interrupted session it reads "Not running" and offers "Resume"; for an archived one it says the session is archived
  and offers "Reopen". The card is hidden while the session is being resumed or is shown as switching its account.
  Assistive technology is told the card's sentence once, as the card appears or its sentence changes; its button is not
  part of what is said, and is reached as any other control. When the card appears or its sentence changes while a
  dialog is open over the window — the confirmation that selecting an interrupted session can ask first (see "Starting a
  session on an account whose last change is incomplete" in `docs/product/sharing-the-default-configuration.md`), for
  one — the sentence is told about 1.5 seconds after the dialog closes, after what the return of keyboard focus says. A
  sentence already told before a dialog opened is not told again when it closes.
- **An archived session shows its history, not its terminal**: a read-only list of what was said in it, read from the
  agent's own record (see `docs/product/session-history.md`). Its terminal cannot be reached: nothing typed or pasted
  goes anywhere, and typing does not reopen it. Reopening it — the card's "Reopen", or the other ways in "Archiving,
  interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md` — is the way back to its terminal.
- **An interrupted session shows its own saved terminal preview**, behind that card, read-only. A session interrupted
  while it is shown has its live screen replaced by that preview; reopening the view or loading it on another client
  shows the same saved content. Showing a different interrupted session shows its own preview, or a blank terminal if it
  has none. Clicking, selecting, scrolling or focusing the terminal does not resume it, and nothing typed reaches the
  stopped process.
- **Archiving preserves the screen before the agent is asked to exit.** The same rule applies when switching accounts or
  stopping the daemon normally: cleanup, a clear screen or leaving an agent's full-screen interface cannot replace the
  captured preview. An agent that exits or crashes on its own instead leaves its final screen, so content it clears
  before exiting need not survive. These rules are shared by every supported agent. An archived session's preview is
  kept, but its history is what is shown.
- The preview contains the active terminal screen, retaining text colours and styles. With the normal screen active, it
  includes up to **10,000 rows of scrollback**. An agent's alternate full-screen interface has no scrollback: its
  preview contains only that visible screen, without the hidden normal screen's history. The preview is independent of
  the live replay's 2 MiB limit and wraps to the reader's terminal width. It is **not a full conversation record**:
  content an agent already overwrote inside its full-screen interface is not reconstructed. The agent's own stored
  conversation is separate.
- Each process end replaces the previous preview; starting a new process discards it. While a resume is under way, the
  loading state covers it, so it is what shows when a resume fails before starting a process. A successful resume
  redraws from the new process's replay rather than appending to the preview. Older saved output is displayed as
  recorded, including any clearing it contains; it is not converted into a recovered screen. A session whose process
  ended before Octoboard kept output, or with the daemon killed outright, shows a blank terminal, even if an earlier
  process left some. Deleting the session's record deletes its saved preview too (see "Files Octoboard owns" in
  `docs/product/application-lifecycle.md`).

## Links in the output

Web addresses and file paths in the output are links: what makes each one a link, and where it opens, is in
`docs/product/terminal-links.md`.

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
  added and that one is to be added to open a session, and offers **Add agent…**, which opens Settings on its Agent
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
  terminal when it can take it, otherwise to the top bar's first enabled control.
