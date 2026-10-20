# Session history

What the terminal's area shows for an archived session: a read-only **Session history** of what was said in it, read
from the agent's own conversation record, so the user can recall what the session was about. It stands in for the
terminal; the way back to the session's terminal is reopening it. The terminal itself, and the floating bar that
offers Reopen, are in "Interrupted, archived and dormant sessions" in `docs/product/session-terminal.md`; reopening is
in "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`.

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
- **Octoboard** (the app's name) — a message Octoboard itself wrote into the session: the brief a session was started
  with, a report from a session under it, a report panel form submission, information shared with the session (or the
  copy of what it shared), a notice that begins "Message from Octoboard", and the opening prompt of a session started
  at another session's request, of one handed branches by the session that started it, or of one taking over a
  handed-off session (see `docs/product/handing-off-sessions.md`). These are shown, set apart from the user's
  messages, not hidden.
- **Agent** — the agent's reply to a turn, shown only while "Show agent replies" is on (below).

Each message the user or Octoboard sent opens a turn. Left out altogether: tool calls and their output, the model's
reasoning, the replies of a turn before its last one, slash-command expansions, the note an agent leaves when a turn
is interrupted, and what the agent injects into its conversation on its own.

**Which messages are Octoboard's is told from how they begin.** So text an owner session sends with `send_message`,
which has no fixed opening, reads as the user's; and a message the user typed that happens to begin the way one of
Octoboard's does reads as Octoboard's.

### Show agent replies

A **Show agent replies** switch in the history's header, off each time a history is shown, adds each turn's final reply
from the agent under the message that opened the turn. A turn the agent did not answer has none. A reply is rendered
as Markdown, its headings ranking below the history's own title; a web address in it opens in the user's own browser.

### Per agent

- **Claude Code**: the conversation as its transcript records it.
- **Codex**: only a turn's final answer counts as its reply; the progress notes Codex writes along the way are left
  out. A thread Codex itself archived, which it moved into the account's `archived_sessions/`, is read from there,
  without being brought back. A thread that had a turn reverted is read as it stands, the reverted turns left out.
- **Grok Build**: Grok keeps only a summary of what came before it compacted its history, so a compacted session's
  history lists only the turns since the compaction.

The record is looked for as the handoff package's conversation is (see "The handoff package" in
`docs/product/handing-off-sessions.md`): in every account of the agent that could hold it.

## Loading, empty, unavailable and failed

- **Loading**: a spinner and a line saying the history is loading.
- **Empty**: a full empty panel (see "The full panel" in `docs/product/empty-states.md`) saying there are no messages
  in the session — nothing was said in it, including a session that never had a conversation.
- **Unavailable**: a full empty panel with a warning glyph saying the history is unavailable: the agent's record of the
  session could not be found or read. Its line suggests reopening the session to see its terminal.
- **Failed**: the request for the history itself failed — the connection to the daemon dropped, or the daemon
  refused it. The failure enters the error-alert queue for acknowledgement, preserving the actual reason and any
  diagnostic details (see "Error alerts" and "Names and diagnostic details" in `docs/product/error-alerts.md`).
  Acknowledgement leaves the failed pane visible with that reason and **Try again**; it does not retry the read.
  A failed history is also read again by itself when the connection to the daemon comes back. Reopen remains
  available on the floating bar.

Assistive technology is told the loading, empty and unavailable states as they begin. The failed request is announced
by its acknowledgement dialog; the retained pane does not announce the same failure again.

## Keyboard focus

The messages are a scrolling region of their own, which takes keyboard focus so they can be scrolled from the
keyboard. Whatever hands keyboard focus to the terminal — `F6` reaching the terminal's region, for one (see "Moving
focus between regions with F6" in `docs/product/moving-focus-between-regions.md`) — puts it on that region instead for
an archived session; while there are no messages to list, on the history's first control, the Show agent replies
switch; and failing that, on the floating bar's Reopen. Focus in the terminal when the session on screen is archived
moves to the history the same way, and so does focus left on nothing by a refused reopen. When Try again holds focus
and goes away as the history reloads, focus goes to the messages, or to the switch while there are none.
