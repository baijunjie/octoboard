# The session terminal

The terminal in the window's content panel, where the selected session's agent is shown and typed into. Sessions
themselves — opening and statuses — are described in `docs/product/sessions.md`, and archiving, interruption and
resuming in `docs/product/archiving-and-resuming-sessions.md`. What an archived session shows in the terminal's
place is in `docs/product/session-history.md`.

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

## Copy, paste and selection

With a session selected that is not archived, right-clicking its terminal opens an editing menu in both the desktop
app and a browser. On macOS, Control-click opens it too. With keyboard focus in the terminal, the context-menu key or `Shift+F10`
opens the same menu; neither key is sent to the agent. Opening it preserves the terminal's selection, and closing it
returns keyboard focus to the terminal. There is no separate menu button.

- **Copy** copies the terminal text selected when the menu opened. It is disabled without a selection. Where the
  platform has no clipboard, or it refuses, the text is copied through the browser's selection copy instead.
- **Paste** reads text from the clipboard and sends it as terminal input, respecting the running program's bracketed
  paste mode. It is disabled without clipboard reading support or while the selected session cannot accept input.
- **Select all** selects the terminal's contents, including its available scrollback, for copying.

The clipboard is read only when Paste is chosen, so merely opening the menu does not request clipboard access. If
clipboard access completes after the user switches sessions or the original session can no longer accept input, the
paste is discarded. It is never redirected to another session. A failed clipboard read, or a copy that neither way
allows, shows an error alert.

## Output, scrollback and size

- Attaching to a running session replays the **most recent 2 MiB** of that session's raw output, then follows live
  output. The terminal itself keeps 10,000 lines of scrollback. An interrupted session shows the saved preview
  described in "Interrupted, archived and dormant sessions" below; an archived one shows its history instead.
- **Once a session has written more than those 2 MiB, the replay first restores the terminal modes the agent has
  on at that moment**, since agents announce them only once, at startup: the alternate screen, mouse reporting,
  focus reporting, bracketed paste, application cursor and keypad keys, a hidden cursor and disabled line wrap. A
  full-screen agent therefore stays full-screen and keeps answering the mouse, the wheel included, after a reattach.
  Keyboard-protocol enhancements an agent asked for (the kitty keyboard flags) are not restored.
- A reattach always redraws from the replay rather than appending to what is on screen, so scrollback older than the
  replay window is lost on every reconnect.
- Whatever changes the terminal pane's size resizes the agent's terminal: resizing the window, resizing
  the sidebar or the right pane, hiding or showing the sidebar, and the right pane coming or going as its panels open
  and close all do; a pane floating in or the sidebar's drawer opening over the terminal does not (see
  `docs/product/window-layout.md`). The screen follows at once (except
  while a docked column eases, below), but the agent is told the new size only once it has held still for about
  120 ms, and only when it differs from the size the agent already has, so a drag sends the agent one
  size change rather than one for every step of the drag. The window has a minimum size, sized so that
  the sidebar, the terminal and the right pane all stay usable side by side — see "The window's
  minimum size" in `docs/product/window-layout.md`.
- While the docked sidebar's column is easing open or closed (see "Hiding the sidebar" in
  `docs/product/window-layout.md`), the terminal's grid keeps the size it had when the ease began, and is fitted
  to its area once, when the column comes to rest — at most about a quarter of a second after the toggle was last
  pressed. Any other change to the terminal pane's size during the ease, a window resize included, is held until
  then too. Until that fit, a closing column leaves blank space along the terminal's right edge and an opening
  one covers its rightmost columns. The agent is then told the new size as above. Where the system asks for
  reduced motion the column does not ease, and the terminal follows at once. The right pane does not ease at all.
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
line saying the session is being resumed, while a resume of it is under way or it has no process yet; that the session
is starting, while it is still *starting* (see "Session statuses" in `docs/product/sessions.md`), during which there is
nothing to connect to and the terminal attaches once the agent is up; and that the terminal is loading otherwise. It covers whatever comes before that first output, every time the terminal attaches
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
  it is under way, the terminal shows a card at its bottom centre, raised over it by a large shadow with no outline
  (see "The window chrome and the content panel" in `docs/product/window-layout.md`): for an interrupted session it
  reads "Not running" and offers "Resume"; for an archived one it says the session is archived and offers "Reopen".
  The card is hidden while the session is being resumed.
- **An archived session shows its history, not its terminal**: a read-only list of what was said in it, read from the
  agent's own record (see `docs/product/session-history.md`). Its terminal cannot be reached: nothing typed or pasted
  goes anywhere, and typing does not reopen it. Reopening it — the card's "Reopen", or the other ways in "Archiving,
  interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md` — is the way back to its terminal.
- **An interrupted session shows its own saved terminal preview**, behind that card, read-only. A session interrupted
  while it is shown has its live screen replaced by that preview; reopening the view or loading it on another client
  shows the same saved content. Showing a different interrupted session shows its own preview, or a blank terminal if
  it has none. Clicking, selecting, scrolling or focusing the terminal does not resume it, and nothing typed reaches
  the stopped process.
- **Archiving preserves the screen before the agent is asked to exit.** The same rule applies when switching accounts
  or stopping the daemon normally: cleanup, a clear screen or leaving an agent's full-screen interface cannot replace
  the captured preview. An agent that exits or crashes on its own instead leaves its final screen, so content it clears
  before exiting need not survive. These rules are shared by every supported agent. An archived session's preview is
  kept, but its history is what is shown.
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

Web addresses and file paths in a session's terminal are links. What makes each one a link, and where it opens, is
described under "Web addresses" and "File paths" below; the rest holds for both:

- **A link is underlined while the pointer is on it.**
- **A link is drawn in the link colour**: a blue chosen for the light and for the dark theme, shown all the time
  rather than only under the pointer, and over whatever colour the program wrote the text in, in ordinary output and
  an agent's full-screen interface alike. It covers every row the link spans and exactly the text that is the link,
  so text that looks like an address or a path but is not a link stays uncoloured.
- **A link opens with a primary-button click**, without requiring a modifier key.
- **Selecting text does not open a link**: a click that leaves a terminal selection, including dragging within a
  link, opens nothing. Other mouse buttons and a Mac's Control-click do not open a link. Right-click and a Mac's
  Control-click open the editing menu described in "Copy, paste and selection" above, which has nothing of its own
  for a link. Opening a link does not resume a dormant session.

### Web addresses

- **What is a link**: an `http` or `https` address written out as plain text, and an OSC 8 hyperlink the program
  emits, whose visible text may differ from the address it leads to. An OSC 8 hyperlink to any other scheme is not a
  link at all.
- **The link colour** covers plain-text addresses and the visible labels of OSC 8 hyperlinks, an address the terminal
  wraps onto the next row and an address the program itself breaks onto the next line (see below) alike.
- **Where a plain-text address ends**: at the first non-ASCII character, or at an ASCII delimiter such as whitespace,
  a quote mark, a parenthesis, a brace or an angle bracket; a trailing punctuation mark such as `.`, `,`, `:`, `!` or
  `?` is left out. So an address written straight into CJK
  prose stops before the full-width bracket or the words that follow it. An address that spells its host or path in
  raw non-ASCII characters is cut short at the first one, and one whose host starts with one (`http://例え.jp`) is
  not a link at all; a percent-encoded address is recognised whole. An OSC 8 hyperlink is not affected, its target
  being the one the program gives.
- **An address opens in the user's own browser** — to a new tab, in the UI opened in a browser — as a web link in a
  Markdown document does (see "A Markdown file as a document" in `docs/product/file-viewer.md`).
- **An OSC 8 hyperlink shows its real target as a tooltip** once the pointer has rested on it for the window's usual
  tooltip delay (see "How a tooltip behaves" in `docs/product/labels-and-tooltips.md`). A plain-text address has no
  tooltip, its text being its target. The tooltip goes when the pointer leaves the link, when the terminal's view
  scrolls, and when the terminal is cleared to draw a session afresh — selecting another session, a reattach, or an
  interrupted session's preview replacing the screen. It also closes on any key press or pointer press, including the
  click that opens the link. The target reaches no assistive technology, as none of the terminal's
  output does.
- **An address the program itself breaks onto the next line is joined**: when the program ends a row partway through
  an address and carries on with it on the next row — as Claude Code does with a long line of text, usually after an
  indent — the pieces are one link, which covers the rows it spans and opens the whole address; the next row's indent
  is no part of it. This holds when the piece is the last text of the line above and the next row's first non-blank
  text is made of address characters. The continuation runs to the first character that ends an address. The pieces
  may span several rows; joining stops after a piece followed by other text on its row, and once the rows joined,
  each counted at the terminal's full width, would hold more than 2048 characters.

  - **When the row above reaches the terminal's last column**, the piece takes along the next row's address
    characters, even when they form a separate word: a finished address at that column and a broken one cannot be
    told apart.
  - **When the row above stops short of the last column**, the piece joins unless the next row starts a separate
    `http` or `https` address, or the piece ends with an ASCII letter or digit and the next row's run of address
    characters consists entirely of ASCII letters and digits. Thus `…auto-select-` followed by
    `account&theme=light&lang=zh-Hans` joins, while `http://a.dev/x` followed by `Hans` stays separate. A break
    inside an ordinary word may therefore leave an address incomplete; conversely, separate text containing
    address punctuation may be taken along. Only the terminal's own width counts: output the program laid out
    narrower than the terminal follows this rule too, however long its lines are.
  - **A blank next row, or one whose first non-blank character cannot belong to an address**, ends the link. A
    piece followed by a table border or other non-address text on its own row does not join onto a later row.

  An address that runs onto the next row only because the terminal is too narrow for it is recognised whole. A line
  the terminal wrapped is taken as a whole by the rules above, whether the row after it continues the address
  depending on the line's last row — so output replayed or resized into a narrower terminal still joins.

### File paths

A file path in the output is a link only once the machine the daemon runs on has confirmed it names a file or folder
the session may open. Until then — for a moment after it appears — and whenever it does not, it is plain text.

- **What is taken for a path**:
  - an absolute path, `/…`;
  - a path under the home directory, `~/…` — the home directory of the machine the daemon runs on;
  - a relative path with a directory in it: `./a`, `../a`, `src/a.ts`;
  - a file name with an extension followed by a line, or a line and a column: `a.ts:42`, `a.ts:42:7`.

  Any path may carry such a `:line` or `:line:column`; it belongs to the link, and its line is where a file opened in
  the file viewer is shown (see "Opened from the terminal" in `docs/product/file-viewer.md`). A path mentioned with a leading `@` (`@src/a.ts`) is a link without
  the `@`. A compiler's position in parentheses after a path (`src/a.ts(12,5)`, as `tsc` writes it) leaves the path
  alone as the link, its line used as a `:line` is. A bare file name with neither a
  directory nor a line (`README.md`) is not taken for a path, and neither is text longer than 1024 characters.
- **Where a path starts and ends**: it starts at the beginning of a line or after a blank, a quote mark or backtick,
  an opening parenthesis, bracket, brace or angle bracket, `=`, `,`, `;`, or CJK punctuation. It ends at a blank, a
  quote mark or backtick, a parenthesis, bracket, brace or angle bracket, `|`, `\`, `^`, `*`, `?`, or full-width or
  CJK punctuation; a trailing `.`, `,`, `;`, `:`, `!` or `?` is left out. Other non-ASCII characters stay in the path
  (`/tmp/图片.png`), unlike in a web address. A path containing a space is never recognised whole, quoted or escaped.
- **What is not a path**: text containing `://` or starting with `//`, and text that runs straight into a `[` or `{`,
  or a directory ending in `/` followed by `(` — so a route pattern such as `app/[id]/page.tsx` or
  `app/(group)/page.tsx` is no link, not even its first part.
- **Rows**: a path the terminal wraps onto the next row is recognised whole and covers both rows. A path the program
  itself breaks onto the next line — as Claude Code does with a long line of text, usually after an indent — may be
  joined too:
  - A row is taken as carrying on the line above when that line reached the width the program laid it out at, its
    last text ends in a character a path may contain, and the row's first non-blank character is one as well. The
    line above reached that width when it is at least as wide as the terminal, counting the rows the terminal wrapped
    it onto, or when it is at least 40 cells wide and no line near it in the same block of text (a blank line ends a
    block) is longer — so output laid out narrower than the terminal joins, while a block of short lines, such as a
    listing, does not. The row's indent is no part of the path. Rows go on joining this way while the rows joined,
    each counted at the terminal's full width, hold no more than 1024 characters.
  - Both the joined path and the separate pieces are checked, by the rules below. When the joined path passes, it is
    the link — one link covering every row it spans, whichever piece the pointer is on — and the pieces it covers are
    not; otherwise each piece that passes is a link on its own.
  - A joined path may also start at the beginning of any row the program carried the line onto, so a word ending the
    row above (`looks right:`) is not taken into a path that starts on the row below.
- **Where a path may lead**: inside the session's project — its directory and, when the project is in a git
  repository, its place in each worktree of that repository — or inside a temporary directory: `/tmp`,
  `/private/tmp` and the daemon's own temporary directory, which covers agents' scratch space. A temporary directory
  that is the filesystem root or holds the home directory is not used.
  - Symbolic links are followed before this is checked, so a link leading out of these places is no link. An absolute
    or `~/` path must also be written starting inside one of them, not only resolve into one.
  - A relative path is looked for in the project's directory and in each worktree's place — not in whichever
    directory the agent happens to be working in — and is a link only when exactly one of them has it. Found in
    several, it is no link.
  - A console session has no project: in it, only absolute and `~/` paths inside a temporary directory are links. A
    project whose directory cannot be found still has its paths inside a temporary directory linked.
- **Anything that is neither a file nor a folder** is never a link.
- **What the system could launch rather than show is never opened on the host by a click**: a file with an
  executable bit, and a path any part of which, below the project or temporary directory, has one of these
  extensions, compared without regard to case: `app`, `appex`, `bundle`, `framework`, `plugin`, `kext`, `xpc`,
  `prefpane`, `saver`, `workflow`, `action`, `command`, `tool`, `terminal`, `term`, `scpt`, `scptd`, `pkg`, `mpkg`,
  `dmg`, `jar`, `sh`, `bash`, `zsh`, `csh`, `tcsh`, `ksh`, `fish`, `desktop`, `webloc`, `inetloc`, `fileloc`,
  `afploc`, `ftploc`, `mailloc`, `newsloc`, `atloc`, `nslloc`, `url`, `mobileconfig` and `shortcut` — so an
  application bundle and everything inside it as well. Inside a temporary directory such a path is no link at all,
  and in the project neither is such a folder. Such a file in the project is a link, which only ever opens in the file
  viewer (see "Opened from the terminal" in `docs/product/file-viewer.md`).
- **Inside a temporary directory**, which other users can write to, the file or folder and every folder from the
  temporary directory down to it must also belong to the user the daemon runs as and be closed to writes by its group
  and by others.
- **A click** on a file inside the project — its directory or a worktree's place — opens it in Octoboard's file
  viewer, at the path's line, when the viewer can show it; what decides that, and how the viewer then behaves, is in
  "Opened from the terminal" in `docs/product/file-viewer.md`. A click on a folder there opens the folder's README or
  index file in the file viewer instead, when it has one the viewer can show; which file that is, and when and how the
  folder opens in the file manager instead, is in the same section. Anything else opens on the machine the daemon
  runs on — not on the machine a browser client happens to be open on: a file with its default application, a folder
  in the file manager. A line given with the path plays no part there, so such a file opens as it would from the file
  manager. The path is checked again by the rules above as it is opened on the host; one that no longer passes them —
  removed since, say — is not opened, and an error alert says so, as it does when the system fails to open it. A path
  link has no tooltip.
- **When paths are checked**: only paths on screen, or under the pointer, are checked; those that appear together are
  checked together, about 120 ms after they appear, at most 64 at a time. An answer is kept for about 30 seconds for a
  path found and about 10 seconds for one that was not, and a path still on screen is checked again once its answer
  is older, the next time the screen changes or the pointer passes over it — so a path named before the agent writes
  the file becomes a link shortly after the file exists, and one removed stops being a link. A check that fails leaves
  a path as it was, and plain text when nothing was known of it. Every answer is dropped when the terminal is cleared
  to draw a session afresh.

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
    new-project dialog. The explanation also tells the user they can ask a console session to create a project for them.
- **When no agent is both added and installed** (see "Opening a session" in `docs/product/sessions.md`), an otherwise
  offered **New session** or **New console session** is replaced by what is missing. When an installed agent waits
  to be added, the panel carries the bot icon, says that no agent is added and that one is to be added to open a
  session, and offers **Add agent…**, which opens Settings on its Agent accounts section with the Add agent dialog in
  front (see "Opening and closing Settings" in `docs/product/settings.md`). Otherwise it carries a warning icon, says
  that no agent was found and how to make one available, and offers no button. An added agent whose availability has not yet
  been determined does not prevent either offer. The **Add project** offer and the panel asking for a session to be
  selected do not depend on the agents.
- In a project's focus mode with a session that is not archived in its applicable list, or in a console session's
  focus mode, the panel asks for a session to be selected instead. A console session in focus is itself selectable
  even with no bound sessions.

The sidebar's empty lists stay compact alongside this panel (see "Empty rows" in `docs/product/sidebar.md`).

- An **archived console session** does not satisfy the console-session condition outside focus mode; an interrupted
  console session does. Archived project sessions do not fill a project's focus-mode live list.
- A selected session suppresses the panel. Switching console shows the session last selected in the console switched
  to, or none (see "What switching console shows" in `docs/product/console-switcher.md`), so a console with no
  remembered session, a console just created among them, shows this panel for itself.
- The console read is the **current** one, not the one the sidebar is previewing while the pointer rests on another
  console's avatar on the rail (see "Previewing a console from the rail" in `docs/product/console-switcher.md`).
- **The archive view covering the terminal suppresses the creation offer**: while it is open, the underlying area
  shows the no-session panel instead (see "The archive view" in `docs/product/archive-view.md`).
- **Keyboard**: while the panel offers a creation button, the terminal's area is a stop in the `F6` cycle, and whatever
  hands keyboard focus to the terminal puts it on that button, just as with the cover's Reconnect button (see "Moving
  focus between regions with F6" in `docs/product/moving-focus-between-regions.md`). A panel with no button is skipped.
  Focus that is on nothing is left on nothing. When the creation button goes away while holding focus, because a
  session is selected or the console's sessions or projects change from this window or another client, focus goes to
  the terminal when it can take it, otherwise to the top bar's first enabled control.
