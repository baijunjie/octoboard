# The session terminal

The terminal in the window's content panel, where the selected session's agent is shown and typed into. Sessions
themselves are described in `docs/product/sessions.md`, opening them in `docs/product/opening-sessions.md`, their
statuses in `docs/product/session-statuses.md`, and archiving, interruption and resuming in
`docs/product/archiving-and-resuming-sessions.md`. What an archived session shows in the terminal's place is in
`docs/product/session-history.md`, and how the program's queries to its terminal are answered in
`docs/product/terminal-queries.md`.

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
cleared the selection — a console with no session to show chosen from the rail, say, or Back to where none was
selected — the focus ring follows the input that cleared it: it shows after a key, and not after a pointer press. Where
the selected session's record goes away by itself, which nobody chose, the ring shows, except after a tap. The same
holds when focus is in an archived session's history instead of the terminal (see "Keyboard focus" in
`docs/product/session-history.md`).

## Typing and the mouse

The selected session's terminal is live: keystrokes go straight to the agent, exactly as in a system terminal. Mouse
reporting works, so an agent's own mouse-driven TUI is usable: while an agent has mouse reporting on (Claude Code's
fullscreen view, for one), a plain drag goes to the agent and makes no selection of the terminal's own; how to select
anyway is in "Copy, paste and selection" below. An Option-click (Alt-click outside macOS) moves the agent's prompt
cursor to the clicked cell, by sending it arrow keys, only on a screen without mouse reporting and while the terminal is
scrolled to the bottom; under mouse reporting it never sends arrow keys. `Ctrl+C` reaches the agent, and so do `Tab` and
`Shift+Tab`. `F6` and `Shift+F6` never do: they move keyboard focus to another region of the window (see "Moving focus
between regions with F6" in `docs/product/moving-focus-between-regions.md`).

On macOS, the terminal sends conventional editing commands for these keys:

- `Command+Left` / `Command+Right`: move to the start / end of the input line (`Ctrl+A` / `Ctrl+E`).
- `Command+Backspace`: delete towards the start of the input line (`Ctrl+U`).
- `Option+Left` / `Option+Right`: move backward / forward by a word (`Escape`, then `b` / `f`).

These mappings apply with only the modifier shown, outside input-method composition. The running program decides
how it interprets the commands, including where they move in a multiline prompt. Other Option combinations retain
their normal character input.

**Typing waits while the session's messages are held.** While Octoboard holds the messages for a session — while it is
being handed off (see "Messages for the original during a handoff" in `docs/product/handing-off-sessions.md`), while
its owner's agent transfers its branches to another session (see "Transferring branches between sessions of an owner" in
`docs/product/branch-responsible-session.md`), and while a change of its account's sharing has claimed it (see "When a
change is carried out" in `docs/product/sharing-the-default-configuration.md`) — what the user types or pastes into its
terminal is not written into the agent, so it is not echoed and starts no turn. Answers to the program's queries still
go in. When the hold ends with the session still running, what waited goes in, in order. When the terminal's
connection ends first — the session archived, as a successful handoff archives the original, or its process ended for
a restart — what waited is reported as lost, as "Typing a connection had not yet passed on is reported, not retried"
under "Reconnecting" below says; it is never dropped silently.

## Copy, paste and selection

With a session selected that is not archived, right-clicking its terminal opens an editing menu in both the desktop app
and a browser. On macOS, Control-click opens it too. With keyboard focus in the terminal, the context-menu key or
`Shift+F10` opens the same menu; neither key is sent to the agent. Opening it preserves the terminal's selection, and
closing it returns keyboard focus to the terminal. There is no separate menu button. Copy, Paste and Select all form
one group, with no divider between them.

- **Open in file manager** shows a file path link in the file manager on the machine the daemon runs on (see "File
  paths" in `docs/product/terminal-links.md`). It is offered only when the menu was opened with the pointer — a
  right-click, or a Mac's Control-click — on a file path the terminal shows as a link, and then comes first, set apart
  from the items below by a separator. A menu opened from the keyboard, or with the pointer anywhere else, does not
  have it.
- **Copy** copies the terminal text selected when the menu opened or, with nothing selected in the terminal, the text a
  program last copied, while that is still on offer (see "Text a program copies" below). It is disabled with neither.
  When it is disabled while the running program has mouse reporting on, it says under its name how to make a
  selection anyway (see **Selecting with the mouse** below): "Option-drag to select" on macOS, "Shift-drag to select"
  elsewhere. macOS here is what the terminal itself treats as macOS, a browser platform of `Macintosh`, `MacIntel`,
  `MacPPC` or `Mac68K`, so an iPhone or iPad gets the Shift-drag hint. Without mouse reporting it is disabled with no
  reason given.
  Where the platform has no clipboard, or it refuses, the text is copied through the browser's selection copy instead.
- **Paste** reads text from the clipboard and sends it as terminal input, respecting the running program's bracketed
  paste mode, as Cmd+V / Ctrl+V does. A clipboard holding no text (only an image, say) is pasted as an empty paste
  rather than refused, which is how an agent's input knows to fetch the image from the clipboard itself. It is disabled
  without clipboard reading support, saying under its name that the clipboard is unavailable, and while the selected
  session cannot accept input, with no reason given (the terminal itself shows why, by its reconnecting spinner, its
  disconnected notice or the cover of a process that cannot be reached).
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
- a request in output written before the session was attached is ignored: whatever opens each attach (the first paint
  of a running program, or the paint of an exited shell terminal's last screen), the saved output of a session without
  a process, and output still waiting to be drawn when the screen is reset, as on a switch to another session;
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
reset of the screen (another session selected, a reconnect, a fresh paint sent to a client that fell behind the output)
or the terminal closing. Moving the pointer and the
right-click that opens the menu leave it on offer, as does the wheel while the program has mouse reporting on; on an
alternate screen without mouse reporting the wheel sends the program arrow keys, which drop it like any other key. The
program's next copy replaces it.

## Output, scrollback and size

- **Attaching to a running session starts from a first paint of its terminal**, then follows live output, with no byte
  lost or repeated between the two. The daemon keeps a continuously parsed copy of every running terminal, and the first
  paint is that copy written out as escape sequences: it resets and clears the terminal, writes up to **10,000 rows** of
  the normal screen's scrollback, then the screen's rows with their colours, styles and links — when the agent is on its
  alternate screen, the normal screen's rows and cursor first, then the alternate screen's — and then restores the
  scroll region, the saved and current cursor with the attributes each draws with, the terminal modes (mouse reporting,
  focus reporting, bracketed paste, application cursor and keypad keys, insert, newline and origin modes, a hidden
  cursor, disabled line wrap), the cursor style the agent chose and the window title. Attaching covers selecting a
  session, reconnecting and reloading the window. The terminal itself keeps 10,000 lines of scrollback. An interrupted
  session shows the saved preview described in "Interrupted and archived sessions" below; an archived one shows its
  history instead.
- **A newly attached or reconnecting terminal therefore shows the agent's exact current screen however much output it
  has written**, and a full-screen agent stays full-screen and keeps answering the mouse, the wheel included. Its
  scrollback is the daemon's parsed scrollback, up to 10,000 rows. Lines the output wrapped stay joined, so they reflow
  when the terminal is resized.
- Not painted, and so missing on a terminal attached after the agent set them: the normal screen behind the alternate
  screen after the terminal's width changed while the agent was on its alternate screen, until the agent enters it again
  (a terminal that attached then gets no scrollback, and shows a blank normal screen once the agent leaves the alternate
  one), character sets, tab stops, colour palette changes, and keyboard-protocol enhancements an agent asked for (the
  kitty keyboard flags).
- The first paint is drawn at the size the agent's terminal has at that moment. The size the attaching client reports
  is applied after its paint, and an agent told a different size redraws for it over the painted screen. Attaching
  makes the client the most recently active one, which is what decides the size while several clients show the
  terminal (see "The terminal's size" in `docs/product/terminals-on-several-clients.md`).
- A reattach always redraws from a new first paint rather than appending to what is on screen, so scrollback older
  than the daemon's 10,000 rows is lost on every reconnect.
- **A client that falls behind the output stays connected, and holds up nothing else.** The daemon sends each client
  output only up to about **512 KiB** ahead of what that client's terminal has taken in; the rest waits in the daemon
  for that client alone, so neither the program nor any other client showing the terminal is slowed by it. Once more than
  **4 MiB** waits for one client, what waits is discarded and the client is sent a fresh paint of the current screen in
  its place, drawn as the first paint is but without scrollback; live output follows it with no byte lost or repeated.
  The fresh paint resets the terminal, so that client's scrollback is gone from then on, and with it, as on any reset of
  the screen, what a program copied that the editing menu offered (see "The menu offers what a program copied" in "Copy,
  paste and selection" above), a link's tooltip and what was known of the file paths on the replaced screen (see
  `docs/product/terminal-links.md`). A client that stops reading altogether (a suspended window, a stalled network) is
  not disconnected either: once it reads again it is sent that paint rather than the output it missed. A client of the
  daemon that does not report what its terminal has taken in is sent output as fast as its connection takes it, and is
  moved to a fresh paint by the 4 MiB limit alone.
- **When a terminal finishes** — the session's process ends, or a shell terminal's shell exits or the terminal is
  closed — each attached client is still sent the output waiting for it before its connection ends.
- The character grid has a 14px base gutter on each physical side, in either reading direction. The right gutter is
  the terminal's internal scrollbar area, reserved even while the scrollbar is hidden or the agent uses a full-screen
  interface; it has no additional outer padding. Docked panels add no extra gap beside the terminal, and resize handles
  add no terminal padding. Any space left after fitting whole character columns is shared between the two sides.
- Whatever changes the terminal pane's size resizes the agent's terminal: resizing the window, resizing the sidebar or
  the right pane, hiding or showing the sidebar, and the right pane coming or going as its panels open and close all do;
  a pane floating in or the sidebar's drawer opening over the terminal does not (see `docs/product/pane-docking.md` and
  `docs/product/sidebar-drawer.md`).
  The screen follows at once (except while a resize handle is dragged or a docked column eases, below), but the agent
  is told the new size only once it has held still for about 120 ms, and only when it differs from the size the agent
  already has, so a drag sends the agent one size change rather than one for every step of the drag. A size change
  made in the window the user is in makes this client the most recently active one, so it applies at once even while
  another client shows the same terminal; one made in a window the user is not in waits while another client showing
  the same terminal is more recently active, until this one is again (see "The terminal's size" in
  `docs/product/terminals-on-several-clients.md`). The
  window has a minimum size, sized so that the sidebar, the terminal and the right pane all stay usable side by
  side — see "The window's minimum size" in `docs/product/window-layout.md`.
- While any pane-width, panel-height or column-width handle is being dragged, the terminal keeps its grid size and
  continues showing the agent's output. After the drag ends, a shown terminal whose area changed is fitted once to
  its current area; a terminal whose area did not change needs no fit. Resizing a floating sidebar therefore leaves
  the agent terminal's grid unchanged. A hidden terminal fits when it is shown. The agent is told a changed size
  with the same delay as above.
- While the docked sidebar's column is easing open or closed (see "Hiding the sidebar" in
  `docs/product/pane-docking.md`), the terminal's grid keeps the size it had when the ease began, and is fitted to its
  area once, when the column comes to rest — at most about a quarter of a second after the toggle was last pressed. Any
  other change to the terminal pane's size during the ease, a window resize included, is held until then too. Until that
  fit, a closing column adds blank space on both sides of the grid and an opening one covers its rightmost
  columns. The agent is then told the new size as above. Where the system asks for reduced motion the column does not
  ease, and the terminal follows at once. The right pane does not ease at all.
- The terminal's colours follow the window's light or dark appearance, and its text is held to a minimum contrast
  against its cell's background whatever colours the agent prints — see "What follows the choice" in
  `docs/product/appearance.md`.

## Reconnecting

- A terminal's connection can drop while the session is still running — the network failing, the daemon restarting;
  falling behind the output does not drop it (see "Output, scrollback and size" above). The application then
  reconnects by itself, with a backoff, up to five times. **While those attempts are under way the terminal is covered
  by a spinner and a line saying it is reconnecting**, over whatever output was on screen. **Once the attempts are
  spent the cover becomes a disconnected panel**, in the shape every empty state takes (see "The full panel" in
  `docs/product/empty-states.md`): an unplugged-cord glyph, a heading saying the terminal is disconnected, and a
  **Reconnect** button. The top bar shows nothing about the terminal's connection in
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
  `docs/product/application-lifecycle.md`), or when its process can be reached again after a time it could not (see
  "While the session's process cannot be reached" below). Each of these starts the five attempts over. An update to the
  session's status does not try it again.
- **Typing a connection had not yet passed on is reported, not retried.** When a terminal's connection ends, for any
  reason, before everything typed into it over that connection has been written into the program — or a write into the
  program failed part way — every client is sent the error `terminal_input_lost`, with no request id. It carries
  `session` (a session's terminal) or `shell_terminal` (a shell terminal of the Terminal panel), the terminal's id, and `detail`, the failed write's own message, left out when no
  write failed. Answers to the program's queries left unwritten do not raise it. What was typed is not written again,
  so the program's input line may hold only part of it. The window shows an error alert titled "Terminal input not
  accepted", naming the session (or the shell terminal) whose terminal disconnected before it had taken everything
  typed into it, and telling the user to check the terminal's input line before continuing; `detail` is under Error
  details. The alert is a known failure with no remedy beyond its own text, not an unexpected one. Typing refused whole
  because a planned stop has handed the terminal over is told with `daemon_shutting_down` instead (see "While the
  terminals are handed over" in `docs/product/application-lifecycle.md`).

## While the session's process cannot be reached

While Octoboard cannot reach the process of a session that is neither interrupted nor archived — the session's record
carries `holder_link: "pending"`, as "A holder that cannot be connected to" in `docs/product/process-holders.md`
describes — the process keeps running, but nothing reaches it and none of its output arrives. The session shows the
status *Reconnecting* (see "Which status a session shows" in `docs/product/session-statuses.md`), and its terminal:

- **is covered at once**, whatever its connection reports and whatever it last drew: over the terminal's background, a
  spinner, a heading saying it is reconnecting to the session, and a line saying that the session's process is still
  running, that input is unavailable until the connection returns and that reconnection is automatic. The output already
  on screen stays under the cover and is not shown as live. The cover offers no button, no countdown and no error:
  Octoboard retries by itself, for as long as the process runs. It comes before every other cover of the terminal — the
  loading state, the reconnecting and disconnected covers, and the switching cover of "While a session is switching" in
  `docs/product/switching-session-accounts.md`, which comes back once the process is reached again with the switch still
  under way — and a terminal opened in this state shows only it.
- **sends the program nothing.** Typing, pastes, mouse reports, the terminal's answers to the program's queries, size
  changes and turning to the terminal (see "The most recently active client" in
  `docs/product/terminals-on-several-clients.md`) are not sent, and none of it is kept to be sent later: what had been
  typed but not yet sent as the cover came is discarded, and a clipboard read that completes after the cover came is
  discarded as well. The covered terminal can be neither focused nor reached by assistive technology, and the editing
  menu's Paste is unavailable. Typing a connection had already taken is reported as "Typing a connection had not yet
  passed on is reported, not retried" in "Reconnecting" above says.
- **is not reconnected meanwhile.** No automatic reconnect attempt is made or counted, the disconnected panel and its
  Reconnect do not appear, and the application re-reading the daemon's whole state attaches nothing.
- **attaches again by itself once the process can be reached**, once, with the five automatic attempts fresh, even where
  they had been spent before. The loading state covers it until the new first paint is on screen (see "Loading" below).
  A session that has become interrupted meanwhile shows its interrupted state instead (see "Interrupted and archived
  sessions" below).

**Keyboard focus.** The cover is a region named by the session's name, not by its heading, which VoiceOver would
otherwise read twice, and described by its heading and its line. Keyboard focus in the terminal as the cover comes, or
on a Reconnect button that goes away with it, moves to the cover, showing its focus ring; whatever hands keyboard focus
to the terminal — `F6` reaching the terminal's region, for one (see "Moving focus between regions with F6" in
`docs/product/moving-focus-between-regions.md`) — puts it on the cover; focus anywhere else in the window is left where
it is. When the cover goes, focus moves on only if it is still on the cover: back to the terminal, showing its focus
ring, or, when the last interaction was a tap, to the first control that takes no text, so that no on-screen keyboard
comes up — the first such control of a maximized or floating panel covering the terminal, otherwise the top bar's — with
no focus ring. Either way the reattach counts as turning to the terminal (see "The most recently active client" in
`docs/product/terminals-on-several-clients.md`). A recovery never moves focus that is elsewhere.

**Assistive technology** is told the cover's heading and line as one message, once: as the cover comes while the session
is shown, or, for a session shown while its process already cannot be reached, once the window has been attended
(visible and focused) for about 1.5 seconds after it is shown, the wait starting over each time the user comes back to
the window, so nothing is said while the user is away. That later message is dropped unsaid if the session is no
longer shown or the process can be reached again before then, and an open dialog that hides the rest of the window
from assistive technology, such as Settings, does not silence it. It is not told at all when keyboard focus is on the
cover at the moment it would be said, since the cover's name and description already say the same. Reconnect
attempts, renames and other updates of the session do not repeat it. When the recovered-display note (see "The screen
of a terminal taken over" in `docs/product/process-holders.md`) comes while the cover is up, its announcement waits
until the cover, and the loading state that follows it, are gone.

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
`docs/product/switching-session-accounts.md` is shown in place of all of these, and while the session's process cannot
be reached, the cover described in "While the session's process cannot be reached" above is shown in place of every
other. Assistive technology is told the line of the loading state and of the switching cover as each begins, by the
terminal's area alone, as it is told the covers in "Reconnecting" above.

## Interrupted and archived sessions

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
  preview contains only that visible screen, without the hidden normal screen's history. The preview does not depend
  on how much output the agent wrote, and wraps to the reader's terminal width. It is **not a full conversation record**:
  content an agent already overwrote inside its full-screen interface is not reconstructed. The agent's own stored
  conversation is separate.
- Each process end replaces the previous preview; starting a new process discards it. While a resume is under way, the
  loading state covers it, so it is what shows when a resume fails before starting a process. A successful resume
  redraws from the new process's first paint rather than appending to the preview. Older saved output is displayed as
  recorded, including any clearing it contains; it is not converted into a recovered screen. A session whose process
  ended before Octoboard kept output, or that was lost with a daemon killed outright and not taken over from a process
  holder (see "What a daemon start takes over" in `docs/product/process-holders.md`), shows a blank terminal, even if
  an earlier process left some; one whose screen a planned stop saved before its holder was lost shows that screen
  instead (see "The screen of a terminal handed over" in `docs/product/process-holders.md`). Deleting the session's record deletes its saved preview too (see "Files Octoboard owns" in
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
  bar's first enabled control, with its focus ring showing; after a tap, to the top bar's first enabled control with no
  ring.
