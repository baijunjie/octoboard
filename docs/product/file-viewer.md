# File viewer

The **file viewer** is a read-only dialog nearly as large as the window (at most 1440 px wide) that shows one file, or
one change, at a time. It is opened from three places:

- **a file's row in the project pane's Files mode**, by a click or Enter on the row (see "A row's actions" in
  `docs/product/project-pane.md`);
- **a change in the project pane's Git mode**; what the viewer shows of a change, how it moves through the change list,
  closes and follows a change is in `docs/product/project-pane-git-change-viewer.md`;
- **a file or folder path link in a session's terminal or a shell terminal** (see "File paths" in
  `docs/product/terminal-links.md`); how that differs is in "Opened from the terminal" below.

Nothing can be edited or saved in it.

A full-width groove below the file-path and details row separates the header from the content. The content fills
the remaining lower area to the dialog's edges, with reading padding inside the code, document or image view.

## The header

The header has three parts, one under the other:

- **The title** — the file's name, and after it the tags saying what the subject is, which only a change carries
  (see "Opening a change" in `docs/product/project-pane-git-change-viewer.md`). The name comes first and the tags follow
  it, on screen and in what assistive technology is given alike — "server.ts: Unstaged, Modified". The tags never
  shrink, so the name is what the fade cuts.
- **A subtitle**, a line of its own directly under the title, which carries one thing only: the commits a committed
  change is read from. It belongs to the title as a tag does, but is far too long to sit among them.
  A subject with no such commits has no such row at all, and the row costs the content below it that one line and no
  more. Its start aligns with the file-path icon in the details below. It is a single line that fades out where it
  is cut off and carries its full wording as its tooltip.
- **The details** — the file's path within the project, its size and the path a rename came from — laid out beside the
  **controls**, which hold the end of the row in a fixed order: **Copy path**, then the document-or-source choice for a
  Markdown file (see `docs/product/markdown-documents.md`), then the diff's unified-or-split choice, then the wrap
  choice (see "Wrapping long lines" below), then **File manager** and **Default application**. The size shows as a tag
  of its own, drawn as the title's neutral stage tag is; a rename's origin reads as a sentence naming the path. Right
  after the file's path stand the marks of the **notices about the whole file or change** on screen, one mark per notice
  (see "Notices and stand-ins" below); they belong to the path's own item, and the path's fade cuts only the path, never
  them. They come after the controls in the keyboard's order.

At most two of the three view choices are ever offered at once, since the document-or-source choice belongs to a whole
Markdown file's body and the unified-or-split choice to a two-sided diff, and a Markdown diff keeps its source view
alone. So the header never holds more than Copy path, two view choices, File manager and Default application, and the
order on screen is also the order the keyboard moves through them.

**Copy path** is the same action a file's row offers, on the same path and in the same form (see "A row's actions" in
`docs/product/project-pane.md`), but as a copy button, which says the path was copied on itself rather than in a toast:
its tooltip and name turn from "Copy path" to "Path copied" (see "Copy buttons" in
`docs/product/labels-and-tooltips.md`). A copy that fails raises an error alert saying the path could not be copied, as
on a row.

**Each of the three view choices is a single icon button**: its icon shows the view on screen, pressing it switches to
the other view, and its name — the tooltip and what assistive technology announces — says what pressing will do, so it
alternates between "Show document" and "Show source", between "Show split diff" and "Show unified diff", and between
"Wrap lines" and "Don't wrap lines". Nothing about the state is said in colour; the whole convention is in "Two-state
icon toggles" in `docs/product/labels-and-tooltips.md`.

**File manager and Default application** are the two actions of a row's Open in submenu, on the same live file and with
the same effect (see "A row's actions" in `docs/product/project-pane.md`): File manager opens the folder holding the
file and selects it, Default application opens the file with the operating system's default application, both on the
machine running the daemon. They are offered only on a live file — a file opened from Files or from the terminal, and an
uncommitted change that is not a deletion, in the worktree it is read from; a committed change and a change that deletes
the file have neither. Each is an icon button whose tooltip is the short name the row's menu uses while the name
announced says what pressing does (see "Tooltips on icon-only controls" in `docs/product/labels-and-tooltips.md`).
Neither says anything when it succeeds; a refusal or a failure raises the same error alert as from a row, with the same
remedy for a file that is no longer there.

The details share that row with the controls to use the width beside them, not to hold the header to one row. Each
detail is an item of its own, and one too long for the space left beside the controls moves down to a row of its own,
where it has the header's whole width; several can move down, so the header is one row of details, or two, or three.
Each item is one line that is never wrapped: where it does not fit even on a row of its own it fades out rather than
ending in an ellipsis and carries its full text as its tooltip — one fade and one tooltip per detail, not one for the
row. The title and the subtitle fade the same way. A path — the file's own and a rename's origin — fades at its start
instead, so the file's own name stays visible. Both paths show the same name- and extension-based icons as file rows
(see "Rows and the keyboard" in `docs/product/project-pane.md`), with a generic file icon for unrecognized names. A cut
detail, whichever edge it is cut at, runs as a marquee while the pointer is over it (see "Names too long for their
space" in `docs/product/labels-and-tooltips.md`).

**Moving from file to file can move the code below the header**, when the details of one file need a row that the next
one's do not, or when one subject has the subtitle row and the next has none.

## What it shows

- **Text** is shown as code, highlighted for its language as told from the file's name, in colours that follow the
  window's appearance (see "What follows the choice" in `docs/product/appearance.md`). A file of more than 10,000 lines
  or 1,000,000 characters is shown as plain text, with a notice saying it is shown without highlighting because it is
  large; within highlighted code, a line longer than 1,000 characters is left unhighlighted. Where the grammar a file
  needs cannot be loaded, or the code cannot be drawn at all, the file is shown as plain text with a failure notice
  saying which of the two it was (see "Notices and stand-ins" below). A **Markdown** file opens as a rendered document
  instead, with its source a choice away (see `docs/product/markdown-documents.md`).
- **Images** — PNG, JPEG, GIF, WebP, BMP, ICO and SVG; AVIF on macOS 13 and later — are shown scaled down to fit and
  never scaled up, on a checkerboard that shows their transparent areas. An image is only displayed: an SVG's scripts
  never run and nothing it refers to is fetched. An image that cannot be decoded is an error in its place (see "Notices
  and stand-ins" below); an SVG that cannot be decoded is shown as its text instead, with a failure notice saying so.
- **Any other binary file** says it cannot be displayed, with its size.
- **While a file is being read**, and until its code is drawn, the viewer says it is loading. It never shows another
  file's content under this file's name, not even for a moment while moving between files. Assistive technology is
  told it is loading while the file is read — in a viewer just opened, after the viewer itself has been announced —
  but not while its code is drawn.
- **A file that cannot be shown** says why (see "Errors" in `docs/product/project-pane.md`).

## Notices and stand-ins

What the viewer says about the subject on screen — a file or a change — takes one of three shapes, chosen by how
much of the subject it still has to show.

**A notice** says something about content the viewer is still showing. It is a **status mark** — a small glyph, one
per notice, never a block of text over the content. Each notice is in one of three tones, and each tone has its own
glyph shape as well as its own colour, so a tone never rests on colour alone:

- **a note**, an accent-coloured circled `i`, states how the content is drawn and why, for what this file or change is —
  a size budget, a change read at an older version, lines that stay collapsed because the whole file is not to be had, a
  change with no differences whose content is shown whole in place of a diff;
- **a warning**, an amber warning triangle, says something about the content that has to be known before reading it —
  a side of a change that lies outside the project, or a change with no differences whose content could not be shown,
  its stand-in still true;
- **a failure**, a red circled `!`, reports that something went wrong, so the view on screen is not the one that was
  asked for — a read that failed, a grammar that could not be loaded, code, a diff or a document that could not be
  drawn, an image that would not decode.

A mark is named by the notice's sentence, and that sentence is what it opens; where the notice carries what the system
or Git said, that text follows the sentence in the popover as raw text, and is never in the sentence or the name. Every
mark, whatever its tone, is a button and a tab stop that opens the sentence in a popover, with no tooltip, under the
rules of a failure mark (see "Failures, warnings and notes marked at their point" in `docs/product/error-alerts.md`): a
mouse resting on it opens it without moving keyboard focus, and it stays open while the pointer moves into it, so its
text can be selected and copied with the mouse; a mouse click opens nothing; and a tap, Enter or Space, or assistive
technology opens it with keyboard focus inside.

Where a mark stands follows from what its notice is about:

- **The whole file or change on screen**: right after the file's path in the header (see "The header" above), the marks
  ordered failures first, then warnings, then notes. These are syntax highlighting turned off because the file is large,
  or because its grammar could not be loaded or its code could not be drawn; a Markdown document that could not be
  drawn; an image shown as its text; a side of a change outside the project; a change with no differences, or a rename
  with no change to its content, and why its content could not be read in full; a patch shown as plain text because it
  is large or could not be drawn; a missing newline at the end; and what an expansion of a diff's collapsed lines says
  or why it failed (see `docs/product/project-pane-git-change-viewer.md`).
- **One side of an image change shown side by side** (Before and After): what is said about that side's own content —
  the image shown as its text, and that text's own highlighting notices — stands beside the side's heading instead, in
  the same order. A side with marks starts its content level with a side without. An image shown alone keeps these
  marks after the path.
- **One spot in a rendered Markdown document**, where the mark stays at that spot. For a diagram or a block formula
  shown as its source, the mark stands at the start of that source, level with its first line. For an image, or a
  formula inside a paragraph, the mark sits in the paragraph at the text's size; inside a link it is drawn right after
  the link rather than in it, so pressing it does not follow the link. A mark there is a tab stop in the document, as
  a link is (see "Selecting and copying" below).

A failure notice opens no error alert: the content the viewer still shows is what the user came for, and the notice
is the failure's only presentation. A notice already there when the content is first drawn is not announced. One that
appears after the content is drawn, a failure notice included, is announced once as a status, including what a diff
says about collapsed lines (see "Expanding the collapsed lines" in `docs/product/project-pane-git-change-viewer.md`).
That announcement is withdrawn about 5 seconds after it is made, so assistive technology reading the viewer line by
line meets the sentence once, on its mark.

**A stand-in** fills the body when there is nothing to draw at all: the empty state's own panel, centred in the body — a
glyph saying what kind of nothing this is, a heading naming it, and, where there is more to say than the heading
carries, a line under it, which is where a size goes (see "The full panel" in `docs/product/empty-states.md`). It is
what the viewer shows for

- nothing it can show at all: a body it cannot display, with its size — or, for a change between two such bodies, the
  size before and after — and a change with no diff at all, which take the same glyph;
- a change with no differences to show — its two sides holding the same content, or a rename with no change to its
  content — but only where that content could not be read; where it was read, the change shows the file whole with a
  note instead (see "What it shows" in `docs/product/project-pane-git-change-viewer.md`);
- a side that is not a file: a symbolic link, or a repository of its own;
- a subject nothing has been read of yet while the connection to the daemon is lost, which is deliberately this same
  neutral stand-in rather than an error, the read resuming on its own (see "When the open file changes" below). This
  one stand-in is hidden from assistive technology, which hears the state as one sentence from the viewer itself
  instead (see "A state another region already announces" in `docs/product/empty-states.md`).

**An error** fills the body when the file or change cannot be opened at all: an empty-state failure, centred in the
body, with the same red circled `!` as a failure notice, headed that it could not be opened, with the reason under it
and any details under that (see "Errors" in `docs/product/project-pane.md`). An image that was read but cannot
be decoded, and has no text to fall back to, is the same kind of error, headed that the image cannot be displayed, with
what to check about the file. Neither has Try again. A file still in conflict whose content cannot be read shows the
first of these errors and nothing more — no mark after the path — its conflict being said by the title's Conflicted
tag (see "Opening a change" in `docs/product/project-pane-git-change-viewer.md`). Each opens no error alert, and is
announced to assistive technology, once and as an alert, when it appears after the viewer was drawn (see "Failures
shown on the screen" in `docs/product/error-alerts.md`).

A reason the viewer gives for a read that failed — in an error or in a failure notice — does not repeat a path the
header already shows: when the reason is about the file on screen, it speaks of "this file" instead. A reason about any
other path, such as a parent folder or the path a rename came from, still names that path, and a reason the viewer
has no words for is the sentence of a failure Octoboard has no message of its own for (see "Failures shown on the
screen" in `docs/product/error-alerts.md`), its own text kept apart as raw text.

## A Markdown file as a document

A Markdown file opens as a rendered document — what it shows, how it looks and reads, its links, images, diagrams and
formulas — as described in `docs/product/markdown-documents.md`.

## Wrapping long lines

The **wrap control**, the last of the header's view controls, chooses whether lines too long for the viewer's width
are broken onto the next line; its name alternates between "Wrap lines" and "Don't wrap lines". **Nothing wraps until
it is turned on** in what the viewer shows as source text — prose as much as code. It is one choice for all of it,
files and changes alike: code, a file shown as plain text, a Markdown file's source, a diff, a plain patch, and the
file behind a path in conflict. The choice is remembered, like the unified-or-split choice: it holds for the next file
and the next change, and after the application restarts.

- The control is there only while the viewer shows source text whose lines could wrap. An image change whose sides fell
  back to their text has the one choice between them rather than one for each side. An image shown as an image, a binary
  file, anything the viewer fills with a stand-in rather than content (see "Notices and stand-ins" above), an error and
  a rendered Markdown document — which wraps whatever the choice says (see `docs/product/markdown-documents.md`) — offer
  none.
- **While lines do not wrap, a long one is read by scrolling sideways with the pointer.** Those scrollers take no
  keyboard focus, and Left and Right in the viewer move between subjects (see "Moving between files" below), so from
  the keyboard the way to the end of a long line is to turn wrapping on.
- In the split layout, wrapped lines keep the two sides' rows level with each other.

## Selecting and copying

The **code region** — a file's code, highlighted or plain, or a change's diff or plain patch — takes keyboard focus,
with Tab or a click, so it can be scrolled from the keyboard; reached with Tab it shows a focus ring. Tab again leaves
the region, unless it holds a diff whose collapsed lines can be expanded, whose controls it enters first (see
"Expanding the collapsed lines" in `docs/product/project-pane-git-change-viewer.md`).

With keyboard focus on the code region, **⌘A** (Ctrl+A off macOS) selects that region's code and nothing else: not
its line numbers, not a diff's markers, and not the viewer's title, path and size, so copying takes the code alone.
While the code has not been drawn yet, the key selects nothing. An image change whose sides both fell back to their
text is two code regions, and the key selects the one with keyboard focus.

A **rendered Markdown document** (see `docs/product/markdown-documents.md`) is a region of the same kind, named for the
file as the code region is: Tab or a click puts keyboard focus on it so it can be scrolled from the keyboard, and Tab
from there goes on, in the document's order, through each link, each diagram, each block formula and each notice's mark
in it, before leaving it. An inline formula is a stop only while it is wider than the line (see
`docs/product/markdown-documents.md`). ⌘A with keyboard focus on the document selects the document and not the viewer's
title, path and size. Copying a selection that is the whole document puts the file's **Markdown source** on the
clipboard, the source inside its fenced blocks included, which a selection of the drawn document cannot reach; a
selection of part of the document copies as what is selected.

While text inside the viewer is selected, the Left and Right arrow keys leave it selected rather than moving to
another file (see "Moving between files" below).

## Moving between files

**Previous file** and **Next file**, beside the title at its leading side at the top of the viewer, and the **Left**
and **Right** arrow keys move to the file before and after the one shown, without closing the viewer. In a viewer
opened from Files, they move through the file rows the tree shows, top to bottom (in the order described in "Order"
in `docs/product/project-pane.md`): folders are skipped, and so is everything inside a collapsed folder. They stop at
either end rather than going round, Previous file disabled on the first file and Next file on the last. With a
file-name filter active, they instead follow the filter results' visible file rows in their displayed order (see
"Filtering by file name" in `docs/product/project-pane.md`). A file shown as removed moves on to the files that were
beside it. How they move through the Git mode's change list is in `docs/product/project-pane-git-change-viewer.md`. A
viewer opened from the terminal has neither button, and the keys do not move it (see "Opened from the terminal"
below).

- Under a right-to-left language the keys are mirrored: Right moves to the previous file and Left to the next.
- The keys do nothing while a modifier is held, during an input method's composition, when keyboard focus is on a
  control that uses the arrow keys itself, or while text inside the viewer is selected, so a selection made for copying
  is not lost. A key held down moves on file after file.
- No key pressed in the viewer reaches the terminal.

## Closing it

**Escape**, the viewer's **Close** button, or a press on the dimmed area around it closes it (see "What an open dialog,
menu or popover covers" in `docs/product/window-layout.md`). Keyboard focus then goes, for a viewer opened from Files,
to the tree row of the file it showed last, not necessarily the one it was opened from; for one opened from the
terminal, back to the terminal — from a shell terminal of the Terminal panel, back to that shell terminal while its tab
is still there and shown, and to the agent terminal otherwise. A change viewer's closing is in
`docs/product/project-pane-git-change-viewer.md`.

## When the open file changes

In a viewer opened from Files, the listings that keep the tree up to date (see "Keeping the tree up to date" in
`docs/product/project-pane.md`) also decide when the open file is read again, so it follows changes on disk:

- a file that is rewritten in any way — a rewrite of the same size, or another file put in its place, included — is
  read again and shows its new content;
- a file that is gone, or whose folder is gone or can no longer be listed, is read again and then shows why it cannot
  be shown, its row staying in the tree as removed (see "The selected file" in `docs/product/project-pane.md`);
- a file shown as gone that comes back shows again;
- a file that kept changing while it was being read is read again at each new listing of its folder, until a read
  succeeds.

A viewer opened from the terminal is not read again as the file changes on disk; clicking the link again reads it
afresh.

In either, a read that finds the file unchanged leaves what is shown as it is. When the connection to the daemon is
lost, the content shown stays until the connection is back, and then the file is read again; a file that had nothing on
screen yet says, in the neutral stand-in rather than as an error (see "Notices and stand-ins" above), that the
connection was lost and that it is read again once the connection is back.

## Opened from the terminal

A click on a file path link in a session's terminal, or in a shell terminal of the Terminal panel (see "The shell
terminal" in `docs/product/terminal-panel.md`), that names a file inside that terminal's project — its directory or its
place in a worktree of the same repository (see "File paths" in `docs/product/terminal-links.md`) — first reads the
file, from the directory it was found in:

- **text, or an image the viewer draws** (see "What it shows" above), opens in the file viewer, showing what was read
  at once rather than saying it is loading;
- **any other binary file, or a file larger than the 4 MiB a read delivers**, opens on the machine the daemon runs on
  with its default application instead, as a path outside the project does;
- **a read that fails for any other reason** raises an error alert titled that the path could not be opened, with the
  reason, and nothing opens. A file no longer there — removed, moved or renamed since the link was drawn — gets the
  remedy of a file that is gone, not that of a path to correct (see "Recovery and unexpected errors" in
  `docs/product/error-alerts.md`).

**A file the system could launch** (which files those are is in "File paths" in `docs/product/terminal-links.md`) is not
read first: it always opens in the viewer, which reads and shows it as any file, saying it cannot be displayed where it
is not text or an image, and it is never opened on the host by the click.

**A folder** inside the same project or worktree place opens its **entry file** in the viewer, when it has one: a
regular file directly in the folder named `README` or `README.<anything>`, or, when there is none, one named
`index.<extension>`, the names compared without regard to case. Among several candidates of the same kind, a name
ending in `.md` comes first, then one ending in `.markdown`, then any other, each group in alphabetical order without
regard to case. A symbolic link is not taken for an entry file, and nothing below the folder's own files is looked at.
The entry file is read first like a file link, with these differences:

- it is shown without a line — a `:line` written after the folder's path plays no part;
- when the folder has no entry file, or its entry file is binary and not an image the viewer draws, is larger than the
  4 MiB a read delivers, or cannot be read for any other reason, the folder itself opens in the file manager on the
  machine the daemon runs on, as a folder outside the project does; an entry file that cannot be read raises no error
  alert;
- **Cmd+click** (Ctrl+click on other systems) opens the folder in the file manager straight away, without looking at
  its entry file. On a file link the modifier changes nothing.

A folder inside a temporary directory always opens in the file manager.

**The viewer is shown over the window and leaves the right pane as it is**: no Files panel is opened and no tree row is
selected. It shows only the file it was opened on: it has no Previous file or Next file, and the arrow keys do not
move it. A file found in a worktree is read from that worktree; its path in the header, and what Copy path copies, is
its path within the project's place in that worktree, and File manager and Default application act on the file there.

**Opening at a line.** The line a path link carries (`:line`, `:line:column`, or a compiler's `(line,column)`) is where
the file is shown; a column plays no part.

- In highlighted code the line is marked as selected and scrolled to the middle of the view once the code is drawn.
- In a file shown as plain text — past the highlighting budget, or because its code could not be drawn — the line is
  scrolled to but not marked.
- A Markdown file opens in its source, at the line, whatever the remembered document-or-source choice says, until the
  user chooses a view for that file; that choice is then remembered as usual (see `docs/product/markdown-documents.md`).
  The same file opened at another line opens in its source again.
- The line is brought into view once for each file and line: redrawing the code — a change of appearance or of the
  wrap choice — does not scroll back to it. A line the file does not have leaves the file at its top. An image ignores
  the line.

**Links in a Markdown document** opened from the terminal open in the same viewer, as in any document (see
`docs/product/markdown-documents.md`), with two differences: a link to a folder is not told from one to a file, so it
opens as a file and the viewer says why it cannot be shown; and in a document read from a worktree, a relative link
stays its text and a relative image shows its alt text, as an address that is not a project file does.

**The first time a viewer is opened from the terminal** its code is loaded, and nothing is shown until it has loaded.
A load that fails raises an error alert, titled that the file viewer could not be shown, saying that it did not load
and that opening the file again tries once more; it is a known failure and asks for no report. The next click tries
again.
