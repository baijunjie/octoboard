# Links in terminal output

Web addresses and file paths in a session's terminal are links; how a shell terminal of the Terminal panel follows the
same rules is in "The shell terminal" in `docs/product/terminal-panel.md`. What makes each one a link, and where it
opens, is described under "Web addresses" and "File paths" below; the rest holds for both:

- **A link is underlined while the pointer is on it.**
- **A link is drawn in the link colour** (see "The link colour" in `docs/product/appearance.md`), the one every link in
  Octoboard takes, following the current appearance. It is shown all the time rather than only under the pointer, and
  over whatever colour the program wrote the text in, in ordinary output and an agent's full-screen interface alike. It
  covers every row the link spans and exactly the text that is the link, so text that looks like an address or a path
  but is not a link stays uncoloured.
- **A link opens with a primary-button click**, without requiring a modifier key.
- **Selecting text does not open a link**: a click that leaves a terminal selection, including dragging within a link,
  opens nothing. Other mouse buttons and a Mac's Control-click do not open a link. Right-click and a Mac's Control-click
  open the editing menu described in "Copy, paste and selection" in `docs/product/session-terminal.md`, which has
  nothing of its own for a link. Opening a link does not resume a dormant session.

## Web addresses

- **What is a link**: an `http` or `https` address written out as plain text, and an OSC 8 hyperlink the program emits,
  whose visible text may differ from the address it leads to. An OSC 8 hyperlink to any other scheme is not a link at
  all.
- **The link colour** covers plain-text addresses and the visible labels of OSC 8 hyperlinks, an address the terminal
  wraps onto the next row and an address the program itself breaks onto the next line (see below) alike.
- **Where a plain-text address ends**: at the first non-ASCII character, or at an ASCII delimiter such as whitespace, a
  quote mark, a parenthesis, a brace or an angle bracket; a trailing punctuation mark such as `.`, `,`, `:`, `!` or `?`
  is left out. So an address written straight into CJK prose stops before the full-width bracket or the words that
  follow it. An address that spells its host or path in raw non-ASCII characters is cut short at the first one, and one
  whose host starts with one (`http://例え.jp`) is not a link at all; a percent-encoded address is recognised whole. An
  OSC 8 hyperlink is not affected, its target being the one the program gives.
- **An address opens in the user's own browser** — to a new tab, in the UI opened in a browser — as a web link in a
  Markdown document does (see `docs/product/markdown-documents.md`).
- **An OSC 8 hyperlink shows its real target as a tooltip** once the pointer has rested on it for the window's usual
  tooltip delay (see "How a tooltip behaves" in `docs/product/labels-and-tooltips.md`). A plain-text address has no
  tooltip, its text being its target. The tooltip goes when the pointer leaves the link, when the terminal's view
  scrolls, and when the terminal is cleared to draw a session afresh — selecting another session, a reattach, or an
  interrupted session's preview replacing the screen. It also closes on any key press or pointer press, including the
  click that opens the link. The target reaches no assistive technology, as none of the terminal's output does.
- **An address the program itself breaks onto the next line is joined**: when the program ends a row partway through an
  address and carries on with it on the next row — as Claude Code does with a long line of text, usually after an indent
  — the pieces are one link, which covers the rows it spans and opens the whole address; the next row's indent is no
  part of it. This holds when the piece is the last text of the line above and the next row's first non-blank text is
  made of address characters. The continuation runs to the first character that ends an address. The pieces may span
  several rows; joining stops after a piece followed by other text on its row, and once the rows joined, each counted at
  the terminal's full width, would hold more than 2048 characters.

  - **When the row above reaches the terminal's last column**, the piece takes along the next row's address characters,
    even when they form a separate word: a finished address at that column and a broken one cannot be told apart.
  - **When the row above stops short of the last column**, the piece joins unless the next row starts a separate `http`
    or `https` address, the piece ends with an ASCII letter or digit and the next row's run of address characters
    consists entirely of ASCII letters and digits, or that run is a list item marker — marks alone, with no letter or
    digit (`-`, `+`), or a number, a single letter or a roman numeral written with i, v and x alone, in either case,
    followed by a period (`1.`, `a.`, `iv.`, `IV.`). Thus `…auto-select-` followed by `account&theme=light&lang=zh-Hans`
    joins, while `http://a.dev/x` followed by `Hans` stays separate, and so does `…lang=zh-Hans` followed by the list
    item `- Show the hint`. A break inside an ordinary word may therefore leave an address incomplete, and a piece made
    only of punctuation (a lone `/`) or shaped exactly like such a marker (`12.`, `a.`) is not joined after such a
    break; conversely, separate text that mixes letters or digits with address punctuation may be taken along. Only the
    terminal's own width counts: output the program laid out narrower than the terminal follows this rule too, however
    long its lines are.
  - **A blank next row, or one whose first non-blank character cannot belong to an address**, ends the link. A piece
    followed by other non-address text on its own row, a border among it, does not join onto a later row — except inside
    a table, below.
  - **Inside a table drawn with box-drawing borders** — rows whose text starts and ends with a vertical border (`│`, `┃`
    or `║`), as Claude Code draws a Markdown table, and as the sides of a box are drawn — each cell is taken on its own,
    as if it were a row of its own. On consecutive rows whose vertical borders stand at the same columns, a cell's piece
    joins the cell below it in the same column by the rules above, the blank after the cell's left border standing for
    an indent. A rule (`├──┼──┤`, or the table's top or bottom border), or a row whose vertical borders stand at other
    columns, ends the link, and no row outside the table joins a cell's piece.

    - **In a table of two or more columns that rules its records apart**, as Claude Code does, the rows between two
      rules, both meeting the table's outer borders, are one record. Claude Code fills a cell before it wraps its text,
      so within a record the cell's last column — before the blank that pads it from its right border — stands for the
      terminal's last one: a piece that reaches it takes the next row's address characters along, as at the terminal's
      last column — except a row that starts with `http://` or `https://`, which starts an address of its own, as a cell
      lists its addresses one to a row. A table that rules off only its header makes its whole body one record, so there
      a filled address still takes along a word on the next row, though that word belongs to the next record. A record
      spanning more rows than 2048 characters fill at the terminal's full width is treated as below, whichever of its
      rows is looked at.
    - **In a box (a single column), or a table that does not rule its records apart**, a cell is as wide as its longest
      text, which then reaches the cell's edge without being broken there, and the next row may be another record. There
      the edge is no evidence: a piece joins the next row only by the rules for a row that stops short of the last
      column. So `Docs: http://localhost:3000/api` stays apart from a `Press q to quit` below it, and an address stays
      apart from the next row's `https://…`.

    Either way, an address joined over several rows of a cell is one link that opens the whole address, in every column
    alike. It covers only the address's text in each cell, never a border, a cell's padding or another column's text;
    under the pointer, only the piece the pointer is on is underlined. A table row the terminal wrapped onto several
    rows, as after it narrowed, is still recognised as one table row, with its cells where the program drew them. The
    2048-character limit counts each cell's row at the terminal's full width, once for every row its table row spans.

  An address that runs onto the next row only because the terminal is too narrow for it is recognised whole. A line the
  terminal wrapped is taken as a whole by the rules above, whether the row after it continues the address depending on
  the line's last row — so output replayed or resized into a narrower terminal still joins.

## File paths

A file path in the output is a link only once the machine the daemon runs on has confirmed it names a file or folder the
session may open. Until then — for a moment after it appears — and whenever it does not, it is plain text. A path the
pointer already rests on when it is confirmed becomes a link under the pointer — underlined, and opened by a click —
without the pointer having to move; when the pointer is outside the terminal or a mouse button is held at that moment,
it does so at the pointer's next move over the terminal or the button's release there. In a terminal one row high, the
pointer has to move first.

- **What is taken for a path**:
  - an absolute path, `/…`;
  - a path under the home directory, `~/…` — the home directory of the machine the daemon runs on;
  - a relative path with a directory in it: `./a`, `../a`, `src/a.ts`;
  - a file name with an extension followed by a line, or a line and a column: `a.ts:42`, `a.ts:42:7`.

  Any path may carry such a `:line` or `:line:column`; it belongs to the link, and its line is where a file opened in
  the file viewer is shown (see "Opened from the terminal" in `docs/product/file-viewer.md`). A path mentioned with a
  leading `@` (`@src/a.ts`) is a link without the `@`. A compiler's position in parentheses after a path
  (`src/a.ts(12,5)`, as `tsc` writes it) leaves the path alone as the link, its line used as a `:line` is. A bare file
  name with neither a directory nor a line (`README.md`) is not taken for a path, and neither is text longer than 1024
  characters.
- **Where a path starts and ends**: it starts at the beginning of a line or after a blank, a quote mark or backtick, an
  opening parenthesis, bracket, brace or angle bracket, `=`, `,`, `;`, or CJK punctuation. It ends at a blank, a quote
  mark or backtick, a parenthesis, bracket, brace or angle bracket, `|`, `\`, `^`, `*`, `?`, or full-width or CJK
  punctuation; a trailing `.`, `,`, `;`, `:`, `!` or `?` is left out. Other non-ASCII characters stay in the path
  (`/tmp/图片.png`), unlike in a web address. A path containing a space is never recognised whole, quoted or escaped.
- **What is not a path**: text containing `://` or starting with `//`, and text that runs straight into a `[` or `{`, or
  a directory ending in `/` followed by `(` — so a route pattern such as `app/[id]/page.tsx` or `app/(group)/page.tsx`
  is no link, not even its first part.
- **Rows**: a path the terminal wraps onto the next row is recognised whole and covers both rows. A path the program
  itself breaks onto the next line — as Claude Code does with a long line of text, usually after an indent — may be
  joined too:
  - A row is taken as carrying on the line above when that line reached the width the program laid it out at, its last
    text ends in a character a path may contain, and the row's first non-blank character is one as well. The line above
    reached that width when it is at least as wide as the terminal, counting the rows the terminal wrapped it onto, or
    when it is at least 40 cells wide and no line near it in the same block of text (a blank line ends a block) is
    longer — so output laid out narrower than the terminal joins, while a block of short lines, such as a listing, does
    not. The row's indent is no part of the path. Rows go on joining this way while the rows joined, each counted at the
    terminal's full width, hold no more than 1024 characters.
  - Both the joined path and the separate pieces are checked, by the rules below. When the joined path passes, it is the
    link — one link covering every row it spans, whichever piece the pointer is on — and the pieces it covers are not;
    otherwise each piece that passes is a link on its own.
  - A joined path may also start at the beginning of any row the program carried the line onto, so a word ending the row
    above (`looks right:`) is not taken into a path that starts on the row below.
  - Inside a table drawn with box-drawing borders, each cell is taken on its own, as for a web address (see "Web
    addresses" above). In a record of a table that rules its records apart, a cell's piece is joined with the cell below
    it in the same column when the piece reaches the cell's last column, before its padding, ends in a character a path
    may contain, and the cell below starts with one; the 40-cell rule and the lines around play no part. In a box, or a
    table that does not rule its records apart, no path is joined across a cell's rows. A record spanning more rows than
    20 or the rows 2048 characters fill at the terminal's full width, whichever is more, plus the rows 1024 characters
    fill, is treated as unruled. A joined path covers only its text in each cell, and its pieces are offered as links
    one by one, so only the piece under the pointer is underlined, though each opens the whole path.
- **Where a path may lead**: inside the session's project — its directory and, when the project is in a git repository,
  its place in each worktree of that repository — or inside a temporary directory: `/tmp`, `/private/tmp` and the
  daemon's own temporary directory, which covers agents' scratch space. A temporary directory that is the filesystem
  root or holds the home directory is not used.
  - Symbolic links are followed before this is checked, so a link leading out of these places is no link. An absolute or
    `~/` path must also be written starting inside one of them, not only resolve into one.
  - A relative path is looked for in the project's directory and in each worktree's place — not in whichever directory
    the agent happens to be working in — and is a link only when exactly one of them has it. Found in several, it is no
    link.
  - A console session has no project: in it, only absolute and `~/` paths inside a temporary directory are links. A
    project whose directory cannot be found still has its paths inside a temporary directory linked.
- **Anything that is neither a file nor a folder** is never a link.
- **What the system could launch rather than show is never opened on the host by a click**: a file with an executable
  bit, and a path any part of which, below the project or temporary directory, has one of these extensions, compared
  without regard to case: `app`, `appex`, `bundle`, `framework`, `plugin`, `kext`, `xpc`, `prefpane`, `saver`,
  `workflow`, `action`, `command`, `tool`, `terminal`, `term`, `scpt`, `scptd`, `pkg`, `mpkg`, `dmg`, `jar`, `sh`,
  `bash`, `zsh`, `csh`, `tcsh`, `ksh`, `fish`, `desktop`, `webloc`, `inetloc`, `fileloc`, `afploc`, `ftploc`, `mailloc`,
  `newsloc`, `atloc`, `nslloc`, `url`, `mobileconfig` and `shortcut` — so an application bundle and everything inside it
  as well. Inside a temporary directory such a path is no link at all, and in the project neither is such a folder. Such
  a file in the project is a link, which only ever opens in the file viewer (see "Opened from the terminal" in
  `docs/product/file-viewer.md`).
- **Inside a temporary directory**, which other users can write to, the file or folder and every folder from the
  temporary directory down to it must also belong to the user the daemon runs as and be closed to writes by its group
  and by others.
- **A click** on a file inside the project — its directory or a worktree's place — opens it in Octoboard's file viewer,
  at the path's line, when the viewer can show it; what decides that, and how the viewer then behaves, is in "Opened
  from the terminal" in `docs/product/file-viewer.md`. A click on a folder there opens the folder's README or index file
  in the file viewer instead, when it has one the viewer can show; which file that is, and when and how the folder opens
  in the file manager instead, is in the same section. Anything else opens on the machine the daemon runs on — not on
  the machine a browser client happens to be open on: a file with its default application, a folder in the file manager.
  A line given with the path plays no part there, so such a file opens as it would from the file manager. A path link
  has no tooltip. The path is checked again by the rules above as it is opened on the host, and one that no longer
  passes them is not opened. Every error alert a click raises is titled that the path could not be opened; which of them
  carry a remedy is in "Recovery and unexpected errors" in `docs/product/error-alerts.md`. The system failing to open
  the path raises one. One written inside the project or a temporary directory where nothing is any more — deleted,
  moved or renamed since it was checked, a file now where a folder on the way should be, or a symbolic link leading
  nowhere or round in a loop — raises one saying it does not exist any more. Any other that no longer passes them is
  refused with one whose sentence says why:
  - it leads outside the project and the temporary directories — written outside them, or leading out of them through
    a symbolic link;
  - a relative path is found in more than one of the places it is looked for;
  - it is something the system would run;
  - inside a temporary directory, it can be changed by other users of the machine;
  - it is not a file or folder;
  - Octoboard is not allowed to read it, or a directory on the way to it;
  - it could not be read for another reason, the system's own text then under **Error details**;
  - for any other reason, or one the window does not know, only that it cannot be opened from a link.
- **When paths are checked**: only paths on screen, or under the pointer, are checked; those that appear together are
  checked together, about 120 ms after they appear, at most 64 at a time. An answer is kept for about 30 seconds for a
  path found and about 10 seconds for one that was not, and a path still on screen is checked again once its answer is
  older, the next time the screen changes or the pointer passes over it — so a path named before the agent writes the
  file becomes a link shortly after the file exists, and one removed stops being a link. A check that fails leaves a
  path as it was, and plain text when nothing was known of it. Every answer is dropped when the terminal is cleared to
  draw a session afresh.
