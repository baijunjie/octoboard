# Terminal panel

The **Terminal panel** is a kind of panel in the right pane's stack (see `docs/product/right-pane-panels.md`) that
shows a project's shell terminals — the plain login shells the daemon runs for a project (see
`docs/product/shell-terminals.md`) — as **tabs**, one tab per terminal. Only a project's stack has it: a console
session has no Terminal panel and no Terminal toggle, since a shell needs a project's directory. It is stacked last,
under the Files and Processes panels, and is **closed** until the user opens it.

Its header's title is "Terminal", retained as an accessible heading when its visual title is hidden (see "A panel's
header" in `docs/product/right-pane-panels.md`); for assistive technology the panel is a region named for the project's
terminals.

## Starting a terminal

**A shell is started only when the user asks for one**:

- **opening the Terminal panel with its toggle** in the top bar while the project has no terminal, a floating Terminal
  card docked by pressing its toggle included (see "The panel toggles in the top bar" in
  `docs/product/right-pane-panels.md`);
- **Terminal under Open panel** in a project's action menu while the project has no terminal, which opens the panel as
  its toggle does (see "Opening a panel from a project's action menu" in `docs/product/right-pane-panels.md`);
- **New terminal**, the "+" button in the panel's header;
- **New terminal** in the panel's empty state;
- **Terminal under Open in on a folder's row** in the project pane (see "A row's actions" in
  `docs/product/project-pane.md`), which opens the project's Terminal panel too, as Open panel would.

Nothing else starts one. While the project has no terminal and none is being started, the panel shows the empty state
"No terminals", with a line saying a terminal runs the user's own shell for the project and a **New terminal** button.
That is what a Terminal card floating in shows, what an open Terminal panel shows after a reload or after the right
pane passes to a project with no terminal, and what is left when the project's last tab is closed: the panel stays
open, and stays maximized if it was (see "Maximizing the Terminal panel" in `docs/product/right-pane-panels.md`).

**While the first terminal of the project is being started**, the panel shows a spinner with "Starting terminal…" in
place of the empty state, and assistive technology is told so. Opening the panel or pressing New terminal meanwhile
joins that start rather than starting a second shell; a folder row's Terminal never joins one, and always starts a
shell of its own. Once the project has a terminal, every press of New terminal starts another; a project may have any
number.

**A new terminal becomes the shown tab.** Started from the panel's own New terminal buttons, or from a folder's row, it
also takes keyboard focus once the panel shows it; started by the toggle or by Open panel, it takes no focus of its
own, and focus is wherever opening the panel leaves it (see "The panel toggles in the top bar" in
`docs/product/right-pane-panels.md`). A start the daemon refuses raises an error alert (see
`docs/product/error-alerts.md`) and adds no tab.

**Where a new shell starts**: in the project's place in the worktree whose uncommitted changes the project's Files
panel shows in its Git mode (see "The repository and its worktrees" in `docs/product/project-pane-git-mode.md`), while
that panel is in its Git mode; otherwise, and when no worktree has the Git mode's branch checked out, in the project's
own directory. What counts is how this window's Files panel last showed the project: it holds after the Files panel is
closed, until the window is reloaded. A folder row's Terminal starts the shell in that folder instead, in the place the
row's actions use (see "A row's actions" in `docs/product/project-pane.md`). What refuses a start, and the shell's
directory afterwards, is in "Starting a shell terminal" in `docs/product/shell-terminals.md`.

## Tabs

The tabs sit in the panel's header after its title when that title is visible, one per terminal of the project,
**oldest first**.

- **A tab's name is its terminal's title**, which follows the title the shell sets until the user renames the terminal
  (see "Terminal title" in `docs/product/shell-terminals.md`). A tab whose shell has exited reads "*title* (exited)",
  in the muted colour. A name is at most 160 px wide; a longer one fades out at its cut edge, with its full text as
  its tooltip.
- **When the tabs do not fit, the tab strip scrolls**, with scroll buttons at its ends, and the shown tab is kept in
  view in it — as it becomes the shown tab and whenever the strip changes width.
- **Pressing a tab** with the pointer shows its terminal and puts keyboard focus in it. From the keyboard the arrow
  keys move between the tabs, showing each one's terminal, and focus stays in the strip.
- **Each tab keeps its screen and its connection while it is hidden**, so going back to it shows it as it was left.

**Which tab is shown** is kept per project and per client, in that client's own browser storage, for up to 64
projects, the ones set longest ago forgotten first: the one last shown while that terminal is still there, otherwise
the newest.

**Terminals follow the project.** Switching the right pane to another project shows that project's tabs; the other
projects' shells keep running, and coming back finds their tabs as they were. Since the daemon owns the terminals,
reloading the window, or reopening it while the daemon kept running, restores every tab and its scrollback. Terminals
started, renamed, exited or closed from another client show here as they happen. A removed project's tabs go with it.

## A terminal's actions

**Rename terminal** and **Close terminal** are offered:

- for the shown tab, in the "⋯" menu in the panel's header, there only while a tab is shown;
- for any tab, in its context menu: a right-click on the tab, or `Shift+F10` or the context-menu key with keyboard
  focus on it.

**The close mark** on a tab closes that terminal with the pointer: it shows on the tab under the pointer and on the
shown tab, has a 24×24 px target and "Close terminal" as its tooltip, and pressing it closes without showing that tab.
It is for the pointer alone; from the keyboard a terminal is closed through the menus above.

**Renaming** opens a "Rename terminal" dialog with a Name field holding the current title. A blank name is refused
under the field; the name is saved trimmed, and from then on the shell's own titles no longer change it.

**Closing** a terminal whose shell is busy (see "Busy shells" in `docs/product/shell-terminals.md`) asks first: a
confirmation titled "Close terminal" names the terminal in its first sentence, its title set apart as a chip, and says
something is still running in the terminal and closing it ends what runs there, with a destructive Close terminal button
and Cancel. An idle running shell, or one that has exited, closes at once without asking. What closing does to the shell
and its jobs is in "Closing a shell terminal" in `docs/product/shell-terminals.md`.

- Until the daemon has ended it, the tab stays, dimmed, and stays shown if it was; Close terminal is unavailable for
  it meanwhile.
- Once it has gone, the tab after it is shown in its place, or the one before it when it was the last.
- A close the daemon refuses raises an error alert.

**Keyboard focus when a terminal goes**, whether closed here or from another client: focus that was in its terminal
moves to the terminal now shown; focus on its tab, or in that tab's open context menu, moves to the shown tab in the
strip. With no tab left, either moves into the panel (to New terminal). The control focus lands on shows its focus
ring, except after a touch. Focus anywhere else stays where it is.

A busy terminal's close confirmation hands focus on as it goes. After Cancel, focus goes back to where the close was
asked from. After Close terminal, when what focus would go back to has gone, focus moves to the shown tab in the strip
(to New terminal with no tab left) if the close was asked from a menu, or to the terminal now shown if it was asked
with the tab's close mark, showing its focus ring except after a touch. A close the daemon refuses leaves focus where
the confirmation put it back.

## The shell terminal

The shown tab's terminal fills the panel under its header. It is live: keystrokes go to the shell, `Ctrl+C`, `Tab` and
`Shift+Tab` included. `F6` and `Shift+F6` never reach the shell: they move keyboard focus to another region (see
"Moving focus between regions with F6" in `docs/product/moving-focus-between-regions.md`).

**`Escape` goes to the shell**, also while the right pane is only floating in, so programs that need it (vim, less,
fzf) get it there too: from inside a shell terminal it does not send a floating pane away. A floating Terminal card is
then sent away with its Close panel button or by moving the pointer away. In the agent terminal in the centre,
`Escape` behaves as described in "A hidden pane floats in on hover" in `docs/product/window-layout.md`.

- **Copy, paste and select all**: the same editing menu as a session's terminal, opened the same ways (see "Copy,
  paste and selection" in `docs/product/session-terminal.md`). Paste is unavailable while the terminal is not
  connected, a terminal whose shell has exited included. Selecting under mouse reporting, and text a program copies
  with OSC 52, follow the same section; a program in a tab not shown can copy too, and each tab's menu offers what its
  own program copied. An Option-click moves the shell's cursor as described in "Typing and the mouse" in the same doc.
- **Answers to a program's queries**: a program in the shell gets one answer to each query it puts to its terminal,
  however many clients show the terminal, and none to old queries in the output replayed on attaching, by the rules of
  "Answers to a program's queries" in `docs/product/session-terminal.md`.
- **Links**: web addresses and file paths in the output are links by the rules of `docs/product/terminal-links.md`, with
  the terminal's project in the place of the session's. **A file opened in the file viewer from a shell terminal hands
  keyboard focus back to that terminal** when the viewer closes, while its tab is still there and shown; otherwise focus
  goes to the agent terminal.
- **Size**: whatever changes the panel's size resizes the shell's terminal, and the shell is told the new size the way
  an agent is (see "Output, scrollback and size" in `docs/product/session-terminal.md`). A terminal whose tab has not
  been shown yet tells the shell no size until it is first shown. While the panel is maximized and the docked
  sidebar's column eases open or closed, the shown terminal keeps its size, whatever else changes it meanwhile, and is
  fitted once, when the column comes to rest, as the agent terminal is.
- **Drawing**: a shell terminal is drawn without GPU acceleration, so however many tabs are open none of them takes a
  graphics context from the agent terminal. Its colours, and the minimum contrast its text is held to, follow the
  window's appearance as the agent terminal's do (see "What follows the choice" in `docs/product/appearance.md`).

## When the shell exits

A tab whose shell has exited (see "When the shell exits on its own" in `docs/product/shell-terminals.md`) keeps its
final screen and is never reconnected. Its name reads "*title* (exited)", and while it is the shown tab a notice at the
bottom of the terminal says the shell has exited, with a **Close terminal** button, which closes it without asking.
Assistive technology is told the notice's sentence, only while the tab is shown: as the notice appears in the shown
tab, and, for a tab that had already exited, about 1.5 seconds after the tab is shown. The Close terminal button is not
part of what is said; it is reached as any other control. The tab stays until it is closed.

## Reconnecting

A running shell's terminal whose connection drops reconnects by itself, as a session's terminal does (see
"Reconnecting" in `docs/product/session-terminal.md`): with a backoff, up to five times, under a cover with a spinner
saying it is reconnecting; then a cover saying it is disconnected, with a **Reconnect** button offered only while the
connection to the daemon is open. Pressing Reconnect starts the attempts over and puts keyboard focus in the terminal.
A terminal whose attempts are spent is tried again by Reconnect, or by itself when the application re-reads the
daemon's whole state on a new connection to the daemon. Each tab reconnects on its own, shown or hidden; only the shown
tab's state is told to assistive technology, as it begins, and, for a tab already reconnecting or disconnected, about
1.5 seconds after the tab is shown. A reconnect in the background never moves keyboard focus.

Keyboard focus and Reconnect:

- whatever puts focus in the panel's terminal — `F6`, a maximize — puts it on Reconnect while the button is offered,
  since the terminal keeps `Tab`;
- Reconnect appearing while keyboard focus is in that terminal takes focus; focus anywhere else is left where it is;
- Reconnect going away while it holds focus — the connection back, the shell exited — hands focus to the terminal.

## Keyboard focus

The Terminal panel is a stop of its own in the `F6` cycle (see "Panels and keyboard focus" in
`docs/product/right-pane-panels.md`), the first among the panels while it is maximized. `F6` lands in the shown tab's
terminal, so typing reaches the shell, or on its cover's Reconnect while that is offered; with no tab, on New terminal
in the header. From there `Tab` goes to the shell, so `F6` and `Shift+F6` are the way out of a shell terminal.
