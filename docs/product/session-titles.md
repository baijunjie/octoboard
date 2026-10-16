# Session titles

A session has a **created name**, chosen when it opens or supplied by its default (see "Opening a session" in
`docs/product/sessions.md`), and an optional **task title** that says what the agent is working on. Octoboard does not
generate the task title: it takes it from what the agent itself records or sends to its terminal, and falls back to the
user's first prompt only for an agent that names nothing.

## Displayed names

With a task title, session navigation shows **task title｜created name**, separated by a fullwidth vertical
bar with no spaces. With no title, or one equal to the created name after surrounding whitespace is removed, it
shows the created name alone. The task title's surrounding whitespace is removed before display. Each part
keeps its own reading direction when the names contain different writing systems.

This displayed name is used in sidebar rows, archived-session menus, focus-mode headers, cards, archive rows and
switch chips, the archive view and the selected session's breadcrumb. Session-owner labels, session action labels
and the archive, account-switch and delete confirmations identify the session by the same name. Names in these
surfaces' accessible labels and full-name tooltips follow the visible name.

**Rename** edits the created name alone; it does not replace the task title. Its availability is described
in "Renaming a session" in `docs/product/sessions.md`.

## Where the task title comes from

The task title is the first of these that says something:

1. **The title the agent's own records name the conversation with**: the name the user gave it with the agent's
   rename, else the one the agent generated (Claude Code, Codex, Grok Build each keep one).
2. **The task in the agent's terminal title**, with the agent's decorations taken off: status glyphs and spinners,
   activity and approval markers, the agent's own name, and the project's name where the agent adds it. A terminal
   title that is only decoration, such as a startup placeholder, says nothing.
3. **The user's first prompt**, for Codex only: its first line that is not a markdown heading, on one line and cut
   to about 80 characters with an ellipsis.

Codex sessions launched by Octoboard usually fall to the third source: Codex does not name a conversation while an
MCP server is supplied to it the way Octoboard has to supply one. An agent Octoboard does not know shows its terminal
title as sent.

## Following an agent's title

Title changes are followed for every running session, including a session whose terminal is not selected or open
in any client, and each reads the same wherever it is shown. Connected clients receive the changes, but only when
the task title changes: an agent that turns a spinner or switches a status glyph in its terminal title causes no
change. The last terminal title and the titles from the agent's records are saved with the session and survive
archiving, interruption, reconnecting and restarting Octoboard. A record title is read from the agent's records
while the session runs and once more after it ends, so a session that has ended, or whose agent clears its terminal
title on exit, keeps the title the records gave it. Resuming or reopening keeps the title until the agent sends
another; an empty terminal title, or one containing only whitespace without control characters, no longer counts as
one, and the session shows what the records give, or the created name alone when they give nothing.

The terminal title comes from OSC 0 or OSC 2 with a BEL or ST terminator. Octoboard accepts valid UTF-8 titles of at
most 4096 bytes without control characters. Other terminal strings and malformed or oversized titles leave the
saved title unchanged. Title observation does not remove or change the output shown in the terminal.

## Leading activity glyphs

Agents animate glyphs at the start of their terminal title to show what they are doing. A run of the following
characters at the very start of a title, together with the whitespace after it, is not part of the title Octoboard
follows: braille patterns (U+2800–U+28FF, the usual terminal spinners ⠋⠙⠹…), the half-filled circles ◐◓◑◒
(U+25D0–U+25D3) and U+25F0–U+25F7, the stars and asterisks U+2722–U+2727 and U+2729–U+273D (among them ✳ ✶ ✻ ✽ ✦),
and · • ● ○ ◉ ◎ ◆ ◇. The run is left out only when the title goes on after whitespace or ends there: a title that
starts with such a character followed directly by text keeps it, and such characters further into the title are
kept.

So the saved title, and every place that shows it, never carries these glyphs; a title that only changes its glyph is
not a change, and one that is nothing but such glyphs clears the terminal title as an empty one does.
