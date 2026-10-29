# Session history

What the terminal's area shows for an archived session: a read-only **Session history** of what was said in it, read
from the agent's own conversation record, so the user can recall what the session was about. It stands in for the
terminal; the way back to the session's terminal is reopening it. The terminal itself, and the floating bar that
offers Reopen, are in "Interrupted, archived and dormant sessions" in `docs/product/session-terminal.md`; reopening is
in "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`. Sessions read the same
record, of any session, with `read_session_history` (see "Reading a session's history" in
`docs/product/what-every-session-reads.md`).

## When the history is shown

- **Selecting an archived session shows its history in place of its terminal**, wherever it is selected from, and so
  does showing one by Back or Forward or a console switch. The session stays archived. A session shown while it is
  archived — by the user, its owner or another client — has its terminal replaced by the history at once.
- **The terminal underneath is out of reach**: nothing typed or pasted goes anywhere, there is no editing menu, and
  typing does not reopen the session. Its saved terminal screen is not shown.
- **While a reopen is under way** the loading state covers the history (see "Loading" in
  `docs/product/session-terminal.md`), and the history cannot be reached. A reopen that succeeds shows the session's
  live terminal; one that is refused leaves the session archived, with its history shown again.
- **The history is read afresh each time it is shown**, from the agent's record as it is then; nothing is kept between
  showings, and nothing is written to the agent's record.

## What the history lists

The history lists the conversation in order, each message under the name of who it came from and, where the agent's
record dated it, the date and time, written for the app's language:

- **You** — a message the user typed or pasted. A slash command the user invoked reads as typed, `/name args`, not as
  what it expanded to. A paste reads as its text, without the wrapper the agent records around it.
- **Agent** — the agent's final reply to a turn, always shown. A turn the agent did not answer has none. A reply
  is rendered as Markdown, its headings ranking below the history's own title; a web address in it opens in the
  user's own browser.

User messages and agent replies share rounded message surfaces with a soft shadow, distinct from the page. User messages
have a soft accent-coloured background; agent replies have a neutral gray background. Both follow the app's light or dark
appearance and use the app's UI font at 14 px with a 24 px line height. Replies keep compact Markdown spacing; their
headings use the body text's size and line height with semibold weight, and tables keep the body text's size. Inline
code and fenced code retain a monospace face; fenced code keeps its code theme. The Markdown style preference for
documents in the file viewer does not change the history's typography (see "Markdown style" in
`docs/product/settings.md`).

**Messages Octoboard wrote into the session are hidden**: the brief a session was started with, a report from a
session under it, a report panel form submission, information shared with the session (or the copy of what it
shared), a notice that begins "Message from Octoboard", and the opening prompt of a session started at another
session's request, of one handed branches by the session that started it, or of one taking over a handed-off session
(see `docs/product/handing-off-sessions.md`). This hides only those messages from the history view; the original
conversation record is unchanged. The agent's final reply to a hidden message remains visible.

**Which messages are Octoboard's is told from how they begin.** So text an owner session sends with `instruct_session`,
which has no fixed opening, reads as the user's; and a message the user typed that happens to begin the way one of
Octoboard's does is hidden too.

Each message the user or Octoboard sent opens a turn. Also left out: tool calls and their output, the model's
reasoning, the replies of a turn before its last one, slash-command expansions, the note an agent leaves when a turn
is interrupted, and what the agent injects into its conversation on its own.

### Per agent

- **Claude Code**: the conversation as its transcript records it.
- **Codex**: only a turn's final answer counts as its reply; the progress notes Codex writes along the way are left
  out. A thread Codex itself archived, which it moved into the account's `archived_sessions/`, is read from there,
  without being brought back. A thread that had a turn reverted is read as it stands, the reverted turns left out.
- **Grok Build**: Grok keeps only a summary of what came before it compacted its history, so a compacted session's
  history lists only the turns since the compaction.

The record is looked for as the handoff package's conversation is (see "The handoff package" in
`docs/product/handing-off-sessions.md`): in every account of the agent that could hold it.

## The context size beside the heading

The history's header shows the session's last known context size right after the history's "Session history" heading,
whatever state the history is in, as the session's context-size chip (see "The context-size chip" in
`docs/product/session-context.md`): the same compact number badge as the sidebar's session rows, led by a ring of the
share in use when a window is known, with the same Context window card on hover and the same accessible name. With
the context size unknown or zero, no context-size chip follows the heading.

As the header narrows, the heading clips and fades while the chip keeps its size; the header wraps its contents
when needed to keep the dates readable. Pressing the chip does not move keyboard focus.

## The owner the session reports to

When the archived session is bound to an owner (see `docs/product/hub-orchestration.md`), the history's header shows a
line below the heading and its context-size chip saying the session reports to that owner, the owner named by its
displayed name (see "Displayed names" in `docs/product/session-titles.md`) as a link. The line is there whatever state
the history is in. There is none when the session is bound to no owner, or when its owner is not among the sessions
the window knows, such as an owner that was deleted; it goes as soon as the owner does.

- **Pressing the link shows the owner as selecting it does** (see "Selecting a session" in
  `docs/product/sidebar.md`): an archived owner is shown with its own history, an interrupted one is resumed, and the
  sidebar follows the owner to its console.
- **A long owner name fades where it is cut off** rather than ending in an ellipsis (see "Names too long for their
  space" in `docs/product/labels-and-tooltips.md`); the dates keep their place at the header's trailing side.
- Keyboard focus on the link is described in "Keyboard focus" below.

## Archive and creation times

The history's header shows the archive time first, when known, then the creation time on the line below it, at the
trailing end after the heading, its optional context-size chip and the optional owner line. Each is labeled by its
meaning and aligned to the trailing edge of the date block. Both are absolute dates with year, month, day, hour and
minute, formatted in the current language and the client's local time zone. An unknown archive time is omitted; the
creation time never stands in for it. The dates remain visible while messages scroll and in every loading, empty,
unavailable or failed state.

## Loading, empty, unavailable and failed

- **Loading**: a spinner and a line saying the history is loading.
- **Empty**: a full empty panel (see "The full panel" in `docs/product/empty-states.md`) saying there are no messages
  to display — including a session that never had a conversation, or one with only Octoboard messages and no
  agent replies.
- **Unavailable**: a neutral empty panel in the history's place (no danger glyph, since the record's state is not a
  failed load and reading it again would not change it), headed that the history is unavailable, saying the agent's
  record of the session could not be found or read and suggesting reopening the session to see its terminal. It has
  no Try again.
- **Failed**: the request for the history itself failed — the connection to the daemon dropped, or the daemon
  refused it. An empty-state failure in the history's place, headed that the history could not be loaded, gives the
  actual reason, with any diagnostic details under it (see "Names and diagnostic details" in
  `docs/product/error-alerts.md`), and **Try again**, which reads it again. A failed history is also read again by
  itself when the connection to the daemon comes back. Reopen remains available on the floating bar.

None of these states opens an error alert (see "Failures shown on the screen" in `docs/product/error-alerts.md`).
Assistive technology is told the loading, empty and unavailable states as they begin; a failed request is announced by
its failure when it appears after the history was drawn.

## Keyboard focus

The messages are a scrolling region of their own, which takes keyboard focus so they can be scrolled from the
keyboard. Whatever hands keyboard focus to the terminal — `F6` reaching the terminal's region, for one (see "Moving
focus between regions with F6" in `docs/product/moving-focus-between-regions.md`) — puts it on that region instead for
an archived session; while there are no messages to list, on the history's first available control below its header,
such as Try again in a failed state, never on the header's owner link; otherwise on the history view itself, and
failing that, on the floating bar's Reopen. Focus in the terminal when the session on screen is archived moves to the
history the same way, and so does focus left on nothing by a refused reopen. When Try again holds focus and goes away
as the history reloads, focus goes to the messages, or to the history view itself while there are none. Both the
scrolling region and the history view show a visible ring when focused from the keyboard.

The owner link is a `Tab` stop in the history, with a visible ring when focused from the keyboard. Pressing it while it
holds keyboard focus hands focus to the terminal once the owner is shown: into a panel covering the terminal when
showing the owner brings one up, such as a console session owner's report panel floating over the terminal below
782 px (see "Below 782 px: one panel floats over the terminal" in `docs/product/right-pane-panels.md`), and
otherwise into the owner's terminal, or, for an archived owner, its history as described above. When the link goes
away while holding focus, because its owner did, focus is handed to the terminal the same way, so it lands in this
session's history.
