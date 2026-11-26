# The session terminal

The terminal in the window's content panel, where the selected session's agent is shown and typed into. Sessions
themselves are described in `docs/product/sessions.md`, opening them in `docs/product/opening-sessions.md`, their
statuses in `docs/product/session-statuses.md`, and archiving, interruption and resuming in
`docs/product/archiving-and-resuming-sessions.md`. What an archived session shows in the terminal's place is in
`docs/product/session-history.md`, and how the program's queries to its terminal are answered in
`docs/product/terminal-queries.md`.

The rest of the session terminal is described in its own docs:

- what the terminal's area shows with no session selected, and what it offers in a console with nothing in it:
  `docs/product/session-terminal-with-no-session-selected.md`;
- the editing menu, copying, pasting and selecting: `docs/product/terminal-copy-paste-and-selection.md`;
- dropping files onto the terminal, which pastes their paths: `docs/product/dropping-files-onto-a-terminal.md`;
- attaching, scrollback, a client that falls behind the output, the terminal's size and colours:
  `docs/product/terminal-output-scrollback-and-size.md`;
- reconnecting, the cover while the session's process cannot be reached, and the loading state:
  `docs/product/session-terminal-reconnecting-and-loading.md`;
- the card, the history and the saved terminal preview of interrupted and archived sessions:
  `docs/product/session-terminal-interrupted-and-archived-sessions.md`;
- the banner over the top of the terminal that says what a running session lacks of Octoboard's additions:
  `docs/product/standing-session-notices.md`;
- the optional message composer below the terminal, for writing a whole message and sending or queuing it:
  `docs/product/message-composer.md`, and the session's queue of unsent messages, with the line over the terminal that
  shows it while the composer is off: `docs/product/composer-queue.md`.

## Typing and the mouse

The selected session's terminal is live: keystrokes go straight to the agent, exactly as in a system terminal. Mouse
reporting works, so an agent's own mouse-driven TUI is usable: while an agent has mouse reporting on (Claude Code's
fullscreen view, for one), a plain drag goes to the agent and makes no selection of the terminal's own; how to select
anyway is in "Selecting with the mouse" in `docs/product/terminal-copy-paste-and-selection.md`. A click that opens a
link is not reported to the agent (see `docs/product/terminal-links.md`). An Option-click
(Alt-click outside macOS) moves the agent's prompt cursor to the clicked cell, by sending it arrow keys, only on a
screen without mouse reporting and while the terminal is scrolled to the bottom; under mouse reporting it never sends
arrow keys. `Ctrl+C` reaches the agent, and so do `Tab` and `Shift+Tab`. `F6` and `Shift+F6` never do: they move
keyboard focus to another region of the window (see "Moving focus between regions with F6" in
`docs/product/moving-focus-between-regions.md`).

`Shift+Enter` sends `Escape` followed by a carriage return (Meta+Enter) on every operating system, not the carriage
return `Enter` sends, so a program can tell the two apart: Claude Code reads it as a newline in its prompt rather than
a submit. The shell terminals in the Terminal panel do not map it (see "The shell terminal" in
`docs/product/terminal-panel.md`).

On macOS, the terminal sends conventional editing commands for these keys:

- `Command+Left` / `Command+Right`: move to the start / end of the input line (`Ctrl+A` / `Ctrl+E`).
- `Command+Backspace`: delete towards the start of the input line (`Ctrl+U`).
- `Option+Left` / `Option+Right`: move backward / forward by a word (`Escape`, then `b` / `f`).

These mappings, and the `Shift+Enter` one, apply with only the modifier shown, and never to a key press that belongs
to an input method: one made while it is composing, or one it reports as handled for it (key code 229). The running
program decides how it interprets what is sent, including where the commands move in a multiline prompt. Other Option
combinations retain their normal character input.

**Typing waits while the session's messages are held.** While Octoboard holds the messages for a session — while it is
being handed off (see "Messages for the original during a handoff" in `docs/product/handing-off-sessions.md`), while
its owner's agent transfers its branches to another session (see "Transferring branches between sessions of an owner" in
`docs/product/branch-responsible-session.md`), and while a change of its account's sharing has claimed it (see "When a
change is carried out" in `docs/product/sharing-the-default-configuration.md`) — what the user types or pastes into its
terminal is not written into the agent, so it is not echoed and starts no turn. Answers to the program's queries still
go in. When the hold ends with the session still running, what waited goes in, in order. When the terminal's
connection ends first — the session archived, as a successful handoff archives the original, or its process ended for
a restart — what waited is reported as lost, as "Typing a connection had not yet passed on is reported, not retried"
under "Reconnecting" in `docs/product/session-terminal-reconnecting-and-loading.md` says; it is never dropped silently.

## Links in the output

Web addresses and file paths in the output are links: what makes each one a link, and where it opens, is in
`docs/product/terminal-links.md`.
