# Session titles

A session has a **created name**, chosen when it opens or supplied by its default (see "Opening a session" in
`docs/product/sessions.md`), and an optional **terminal title** supplied by the running agent. Octoboard does not
generate the terminal title or infer it from a prompt: it follows the title the agent sends to its terminal.

## Displayed names

With a terminal title, session navigation shows **terminal title｜created name**, separated by a fullwidth vertical
bar with no spaces. With no title, or one equal to the created name after surrounding whitespace is removed, it
shows the created name alone. The terminal title's surrounding whitespace is removed before display. Each part
keeps its own reading direction when the names contain different writing systems.

This displayed name is used in sidebar rows, archived-session menus, focus-mode headers, cards, archive rows and
switch chips, the archive view and the selected session's breadcrumb. Session-owner labels, session action labels
and the archive, account-switch and delete confirmations identify the session by the same name. Names in these
surfaces' accessible labels and full-name tooltips follow the visible name.

**Rename** edits the created name alone; it does not replace the agent's terminal title. Its availability is described
in "Renaming a session" in `docs/product/sessions.md`.

## Following an agent's title

Title changes are followed for every running session, including a session whose terminal is not selected or open
in any client. Connected clients receive the changes. The last title is saved with the session and survives
archiving, interruption, reconnecting and restarting Octoboard. Resuming or reopening keeps it until the agent
sends another title; an empty title, or one containing only whitespace without control characters, clears it and restores the
created name alone.

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
not a change, and one that is nothing but such glyphs clears the title, leaving the created name alone.
