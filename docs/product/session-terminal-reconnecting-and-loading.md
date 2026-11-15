# Reconnecting and loading in the session terminal

How the session terminal (see `docs/product/session-terminal.md`) recovers when its connection drops, what it shows
while the session's process cannot be reached, and the loading state that covers it until the session's first output
is on screen.

## Reconnecting

- A terminal's connection can drop while the session is still running — the network failing, the daemon restarting;
  falling behind the output does not drop it (see "Sending output to clients" in
  `docs/product/terminal-output-scrollback-and-size.md`). The application then reconnects by itself, with a backoff,
  up to five times. **While those attempts are under way the terminal is covered by a spinner and a line saying it is
  reconnecting**, over whatever output was on screen. **Once the attempts are
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
  terminals are handed over" in `docs/product/planned-restart-and-planned-stop.md`).

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
  A session that has become interrupted meanwhile shows its interrupted state instead (see "Resuming on selection, and
  the card" in `docs/product/session-terminal-interrupted-and-archived-sessions.md`).

**Keyboard focus.** The cover is a region named by the session's name, not by its heading, which VoiceOver would
otherwise read twice, and described by its heading and its line. Keyboard focus in the terminal as the cover comes, or
on a Reconnect button that goes away with it, moves to the cover, its focus ring showing only when the last input was a
key; whatever hands keyboard focus to the terminal — `F6` reaching the terminal's region, for one (see "Moving focus
between regions with F6" in `docs/product/moving-focus-between-regions.md`) — puts it on the cover; focus anywhere else
in the window is left where it is. When the cover goes, focus moves on only if it is still on the cover: back to the
terminal, its focus ring showing only when the last input was a key, or, when the last interaction was a tap, to the
first control that takes no text, so that no on-screen keyboard comes up — the first such control of a maximized or
floating panel covering the terminal, otherwise the top bar's — with no focus ring. Either way the reattach counts as
turning to the terminal (see "The most recently active client" in `docs/product/terminals-on-several-clients.md`). A
recovery never moves focus that is elsewhere.

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
