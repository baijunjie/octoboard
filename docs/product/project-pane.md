# Project pane

The **project pane** shows a project's files and its repository's changes, in two **modes**: **Files**, a tree of the
project's directory, read live from disk with whatever an agent has written and not committed, and **Git**, the
uncommitted changes of a worktree of the project's repository and the changes between two of its local branches (see
`docs/product/project-pane-git-mode.md`). Either one opens what it lists in a read-only **file viewer**. It is shown in
the window's right pane, in the place a console session's report panel takes (see "What the right pane shows" in
`docs/product/window-layout.md`, which also says what puts it there and what takes it away), and it needs no session:
the project is all it reads from. How wide the right pane is, how it is hidden, floats in and becomes a drawer is in
`docs/product/window-layout.md`, and where `F6` lands in it in `docs/product/moving-focus-between-regions.md`.

Nothing in the project pane changes anything: there is no editing, saving, creating, renaming or deleting of files,
and nothing is staged, committed or checked out.

## Opening a project's files

- **Browse files**, the first item of a project's action menu — on its row and by right-clicking it (see "Project
  rows" in `docs/product/sidebar.md`), in its focus mode's header, and on its heading in a console session's focus mode
  (see `docs/product/focus-mode.md`) — shows that project's project pane, selecting, starting and resuming no session.
- **Selecting a project session** shows its project's project pane.

Moving between sessions of one project leaves its pane as it was: its mode, its expanded folders and selected file, and
in Git its view, its chosen worktree, its chosen branches and its selected changes. Another project's session, or
another project's Browse files, shows that project's own pane.

The pane's header names the project, with a folder icon, and carries the **Files** and **Git** tabs (see "Files and
Git" below) and a **Refresh** button. Pressing Refresh with the mouse leaves keyboard focus where it was.

## Files and Git

The tabs in the pane's header switch between its two modes; a project's pane first opens in Files. The mode is
remembered per project (see "What is remembered" below), so Browse files or selecting a session shows a project in the
mode it was last left in.

Only the mode on screen is kept up to date on its own: the Files mode as "Keeping the tree up to date" below says, the
Git mode as "Uncommitted and Compare" in `docs/product/project-pane-git-mode.md` says. Switching to a mode counts as
showing it again, so what it keeps up to date is refreshed when its last refresh is more than 5 seconds old. The
header's **Refresh** refreshes the mode on screen: in Files every folder on screen is listed again, in Git the worktrees
are read again, and so are the changes of the Uncommitted view, or the branches and their comparison in the Compare
view, whichever is on screen. While a refresh the button started is out, its icon turns, slowly, and it keeps turning
for at least a moment, so a refresh that finishes at once still shows it happened. It spins until the reads the press
sent have been answered or have failed, or the connection drops; a folder that does not answer keeps it spinning,
because a read from disk has no deadline. The periodic refreshes do not turn it, and the icon stays still under the
system's Reduce motion setting.

"The file tree", "The selected file" and "Keeping the tree up to date" below are about the Files mode.

## The file tree

### What is listed

Everything in the project's directory is listed — dot files, and files a `.gitignore` names (an agent's build output
and installed dependencies included) — except Git's own metadata: an entry named `.git` is left out at any depth.
Folders start collapsed and each is read only when it is first shown, so a large folder costs nothing until it is
expanded.

A symbolic link is listed under its own name. One that leads to a folder inside the project opens as a folder; one that
leads to a file inside the project opens as that file; one that leads outside the project is listed, but nothing
behind it is shown (see "Errors" below).

### Order

Within each folder, folders come first — a symbolic link leading to a folder inside the project counts as one — then
everything else. Each group is ordered by name, case-insensitively and in natural order, so "item 2" comes before
"item 10", as the sidebar orders names (see "Order of projects and sessions" in `docs/product/sidebar.md`). Two names
that read the same are always kept in the same order. The file viewer moves through files in this same order (see
"Moving between files" below).

### Rows and the keyboard

- A folder opens and closes with a click, Enter, its chevron, or Right and Left (the other way round under a
  right-to-left language). A file opens in the file viewer with a click or Enter.
- At a file or folder row's end is the button of its own actions, reached with the same Right and Left; a collapsed
  folder is expanded first (see "A row's actions" below).
- The arrow keys, Home and End move through the rows, and typing a name's first letters moves to it.
- Pressing a row takes keyboard focus into the tree, unlike a sidebar row: the tree is content to read and move
  through from the keyboard.
- A name too long for its row fades out, with the full name as its tooltip (see "Names too long for their space" in
  `docs/product/labels-and-tooltips.md`).

A folder that is not simply listed says so in a row of its own under it:

- **loading** while it is being read;
- **empty** when it has nothing in it;
- **unreadable**, with the reason (see "Errors" below); pressing the row reads it again;
- **cut short** — "not every entry is shown" — after the entries read before a limit stopped the listing: at most
  10,000 entries of one folder, or 2 MiB of names, are listed. These are the entries read first, not a chosen subset.

For the project's own directory the same states take the whole pane: a loading line while it is first read, announced to
assistive technology, an empty panel saying the project folder is empty (see "The full panel" in
`docs/product/empty-states.md`), a line under the tree when it is cut short, and, when it cannot be read at all, an
error naming the reason (with the folder shown as its path, `~` standing for the home directory) and **Try again** in
place of the tree. Try again pressed from the keyboard hands keyboard focus to the header's Refresh, which stays while
the error and its button go.

## The selected file

The tree's **selected file** is the one the file viewer showed last. Its row is tinted, and assistive technology hears
it named as selected. Nothing else in the tree is selected; a row's one action is to open or close it.

While the viewer moves from file to file, the tree keeps the selected file's row in view behind it.

A selected file that its folder no longer lists stays as a row in its place, struck through, until the selection moves
on: marked **Removed** when the folder's listing is complete, or **Not listed** when it was cut short and the file may
still be there.

## Keeping the tree up to date

The tree is read from the project's directory as it is on disk; nothing tells Octoboard when files change, so it lists
the folders on screen — the project's own directory and every expanded folder whose parents are expanded — again:

- the first time the Files mode is shown after the project pane is put in the right pane, which lists it afresh;
- when a folder is expanded, if it had been listed before;
- on the header's **Refresh**;
- **every 10 seconds** while the Files mode is on screen — the project pane docked, in an open drawer, or floating in
  — and the window is visible, skipping a turn while a listing of the pane is still out;
- when the window comes back to the front or becomes visible again, with the Files mode on screen, or when it comes
  back on screen — a hidden pane docked by its toggle, its drawer opened, or floating in, or the Files tab chosen —
  either one more than 5 seconds after the last refresh, and unless a listing of the pane is still out;
- after the connection to the daemon comes back.

While the pane is hidden, or shows the Git mode, the tree is not refreshed on its own, apart from after a reconnect. A
folder being listed again keeps its rows on screen until the new listing is in. When the project's directory turns out
to have been replaced by another one at the same path, everything listed from the old one is dropped and listed
afresh.

## A row's actions

Every file and folder row in the Files mode's tree ends with an icon button that opens a **menu of what can be done with
that entry**, whether the folder is collapsed or expanded. Every file's row in the Git mode's change list has the same
button in both its views and both forms of the list, flat or grouped by folder (see "Rows and the keyboard" in
`docs/product/project-pane-git-mode.md`). **Copy path** is its only action so far.

- **Copy path** puts the file or folder's path on the clipboard, slash separated, as a path within the project. For a
  change it is the path the row names, which for a rename is its new path — the old one only where the new side is
  absent (a deletion) or outside the project (see "What is listed" in `docs/product/project-pane-git-mode.md`). The path
  is copied **as the window shows it**, for a human to paste: a byte of a name that is not valid UTF-8 is copied as the
  replacement character U+FFFD, the character the name is displayed with. A **toast** then confirms that the path was
  copied; where the clipboard refuses it, an error toast says it could not be copied (see `docs/product/toasts.md`). The
  file viewer's header offers the same action (see "The file viewer" below).
- **The Git mode's grouping folders have no button**, and neither does a row that says something about a folder instead
  of listing an entry (loading, empty, unreadable, cut short; see "Rows and the keyboard" above).
- **The button shows while its row is hovered, holds keyboard focus, is the selected file or change, or has its menu
  open**, fading and opening out as it appears, as the sidebar's row controls do (see "Rows, names and keyboard focus"
  in `docs/product/sidebar.md`). While hidden it takes up no width, so the entry's name has the room; it is reachable
  from the keyboard either way.
- **The keyboard reaches it along the row, not with Tab.** Tab moves into the tree or the change list and out of it
  again, never along a row's parts; it is the arrow key that runs along the row — Right, Left under a right-to-left
  language — that moves from the row to its button; the opposite key moves back to the row. On a collapsed folder that
  first key expands it, and a second press reaches the button. On an expanded folder the opposite key still collapses it
  while focus is on the row.
- **The row keeps its own action**: a click or Enter on the row opens the file or the change in the file viewer, or
  opens or closes a folder. Pressing the button or copying a path does none of those. The button is the only way to the
  menu — a right-click on a row opens none, unlike a sidebar row (see "Right-click menus" in
  `docs/product/window-layout.md`). A menu opened from the keyboard hands keyboard focus back to the button when it
  closes; one opened with the pointer hands it back to whatever had it before, as a sidebar row's menu does.
- Its **accessible name names the file or folder** — "Actions for file server.ts" or "Actions for folder src" — so each
  row's button is told apart, while its tooltip is the short one every such button has (see "Tooltips on icon-only
  controls" in `docs/product/labels-and-tooltips.md`).
- **Where the platform has no clipboard there is no button on the row at all**, rather than a menu whose only action
  cannot act: Copy path being the only action, nothing is left to offer. That is the case in a plain browser whose page
  is not a secure context — served over `http` from anywhere but `localhost`. The packaged desktop app always has a
  clipboard.


## What is remembered

**Per project, per client**, in that client's own browser storage (as the right pane's width is, see "Resizing the
sidebar and the right pane" in `docs/product/window-layout.md`): the mode, Files or Git, and which folders are
expanded, so the pane comes back as it was after a reload or a restart. At most 200 expanded folders are kept per
project, the earliest expanded dropped first, and the state of at most 50 projects, the one shown least recently
dropped first. A removed project's state is dropped.

The selected file is kept only while the window is open: reloading it, or restarting the application, starts every
project with no file selected. So are the Git mode's view, chosen worktree, chosen branches, selected changes and the
sections and folders it has folded away (see "What is remembered" in `docs/product/project-pane-git-mode.md`).
Listings, change lists and file contents are never stored.

## The file viewer

Opening a file shows it in the **file viewer**, a dialog nearly as large as the window (at most 1440 px wide). The Git
mode opens a change in the same viewer; what it shows of a change is in "Opening a change" in
`docs/product/project-pane-git-mode.md`.

The header has three parts, one under the other:

- **The title** — the file's name, and after it the tags saying what the subject is, which only a change carries
  (see "Opening a change" in `docs/product/project-pane-git-mode.md`). The name comes first and the tags follow it,
  on screen and in what assistive technology is given alike — "server.ts: Modified, Unstaged". The tags never
  shrink, so the name is what the fade cuts.
- **A subtitle**, a line of its own directly under the title, which carries one thing only: the comparison a change
  from the Compare view is read from. It belongs to the title as a tag does, but is far too long to sit among them.
  A subject with no comparison has no such row at all, and the row costs the content below it that one line and no
  more. It is a single line that fades out where it is cut off and carries its full wording as its tooltip.
- **The details** — the file's path within the project, its size and the path a rename came from — laid out beside
  the **controls**, which hold the end of the row in a fixed order: **Copy path**, then the document-or-source
  choice for a Markdown file (see "A Markdown file as a document" below), then the diff's unified-or-split choice,
  then the wrap choice (see "Wrapping long lines" below). The size shows as a tag of its own, drawn as the title's
  neutral stage tag is; a rename's origin reads as a sentence naming the path.

At most two of the three view choices are ever offered at once, since the document-or-source choice belongs to a whole
Markdown file's body and the unified-or-split choice to a two-sided diff, and a Markdown diff keeps its source view
alone. So the header never holds more than Copy path and two view choices, and Copy path being first, the order on
screen is also the order the keyboard moves through them.

**Copy path** is the same action a file's row offers, on the same path and in the same form, with the same toast
saying the path was copied or that it could not be (see "A row's actions" above). It is a single icon button, named
"Copy path" in its tooltip and in what assistive technology announces. **Where the platform has no clipboard the
button is not there at all**, as no row offers the action there either.

**Each of the three view choices is a single icon button**: its icon shows the view on screen, pressing it switches to
the other view, and its name — the tooltip and what assistive technology announces — says what pressing will do, so it
alternates between "Show document" and "Show source", between "Show split diff" and "Show unified diff", and between
"Wrap lines" and "Don't wrap lines". Nothing about the state is said in colour; the whole convention is in "Two-state
icon toggles" in `docs/product/labels-and-tooltips.md`.

The details share that row with the controls to use the width beside them, not to hold the header to one row. Each
detail is an item of its own, and one too long for the space left beside the controls moves down to a row of its own,
where it has the header's whole width; several can move down, so the header is one row of details, or two, or three.
Each item is one line that is never wrapped: where it does not fit even on a row of its own it fades out rather than
ending in an ellipsis and carries its full text as its tooltip — one fade and one tooltip per detail, not one for the
row. The title and the subtitle fade the same way. A path — the file's own and a rename's origin — fades at its start
instead, so the file's own name stays visible, and shows with a file icon in front of it, as paths read elsewhere in
the application. A cut detail, whichever edge it is cut at, runs as a marquee while the pointer is over it (see "Names
too long for their space" in `docs/product/labels-and-tooltips.md`).

**Moving from file to file can move the code below the header**, when the details of one file need a row that the next
one's do not, or when one subject has the subtitle row and the next has none.

### What it shows

- **Text** is shown as code, highlighted for its language as told from the file's name, in colours that follow the
  window's appearance (see "What follows the choice" in `docs/product/appearance.md`). A file of more than 10,000 lines
  or 1,000,000 characters is shown as plain text, under a notice saying it is shown without highlighting because it is
  large; within highlighted code, a line longer than 1,000 characters is left unhighlighted. Where the grammar a file
  needs cannot be loaded, or the code cannot be drawn at all, the file is shown as plain text under a failure notice
  saying which of the two it was (see "Notices and stand-ins" below). A **Markdown** file opens as a rendered document instead, with its source a choice away (see "A
  Markdown file as a document" below).
- **Images** — PNG, JPEG, GIF, WebP, BMP, ICO and SVG; AVIF on macOS 13 and later — are shown scaled down to fit and
  never scaled up, on a checkerboard that shows their transparent areas. An image is only displayed: an SVG's scripts
  never run and nothing it refers to is fetched. An image that cannot be decoded says so, with its size; an SVG that
  cannot be decoded is shown as its text instead, under a failure notice saying so.
- **Any other binary file** says it cannot be displayed, with its size.
- **While a file is being read**, and until its code is drawn, the viewer says it is loading. It never shows another
  file's content under this file's name, not even for a moment while moving between files. Assistive technology is
  told it is loading while the file is read — in a viewer just opened, after the viewer itself has been announced —
  but not while its code is drawn.
- **A file that cannot be shown** says why (see "Errors" below).

### Notices and stand-ins

What the viewer says about the subject on screen — a file or a change — takes one of three shapes, chosen by how
much of the subject it still has to show.

**A notice** sits above content the viewer is still showing. Above the reading surface it is a filled block across
the body's width; where a subject carries more than one, they stack as a single strip above the content. Inside a
rendered Markdown document, a diagram's or block formula's notice is a compact filled alert whose outer edge aligns
with the document's content edge. Its glyph aligns with the first line of text and has the same horizontal inset as
the source below it. Each notice is in one of three tones, and each tone draws its own fill and its own glyph,
so a tone never rests on colour alone:

- **neutral**, with a circled `i`, states how the content below is drawn and why, for what this file or change is — a
  size budget, a change read at an older version, lines that stay collapsed because the whole file is not to be had, a
  change with no differences whose content is shown whole in place of a diff;
- **a warning**, amber, with a warning triangle, says something about the content that has to be known before reading
  it — a file still in conflict, a side of a change that lies outside the project;
- **a failure**, red, with a circled `!`, reports that something went wrong, so the view on screen is not the one that
  was asked for — a read that failed, a grammar that could not be loaded, code, a diff or a document that could not be
  drawn, an image that would not decode.

A notice that can appear only once the content has been drawn, which a reader would otherwise never meet, is announced
to assistive technology, each one once: the fall back to plain text where the grammar or the drawing fails, to a
Markdown file's source, to a plain patch where the diff could not be drawn, and to an image's own text; a diagram, a
formula or a project image in a rendered document that cannot be shown after the document has been drawn (see "A
Markdown file as a document" below); and what a diff says about its collapsed lines (see "Expanding the collapsed
lines" in `docs/product/project-pane-git-mode.md`). A notice that arrives with the content it describes is not
announced on its own; it is read with the rest of the dialog. Inside a rendered document, a notice for an image or for
a formula inside a paragraph sits in the paragraph rather than as the block above; it uses the same tones.

**A stand-in** fills the body when there is nothing to draw at all: the empty state's own panel, centred in the body — a
glyph saying what kind of nothing this is, a heading naming it, and, where there is more to say than the heading
carries, a line under it, which is where a size goes (see "The full panel" in `docs/product/empty-states.md`). It is
what the viewer shows for

- nothing it can show at all: a body it cannot display, with its size — or, for a change between two such bodies, the
  size before and after — and a change with no diff at all, which take the same glyph;
- a change with no differences to show — its two sides holding the same content, or a rename with no change to its
  content — but only where that content could not be read; where it was read, the change shows the file whole under a
  neutral notice instead (see "What it shows" in `docs/product/project-pane-git-mode.md`);
- a side that is not a file: a symbolic link, or a repository of its own;
- a subject nothing has been read of yet while the connection to the daemon is lost, which is deliberately this same
  neutral stand-in rather than an error, the read resuming on its own (see "When the open file changes" below). This one
  stand-in is hidden from assistive technology, which hears the state as one sentence from the viewer itself instead
  (see "A state another region already announces" in `docs/product/empty-states.md`).

**An error** fills the body when the file or change cannot be opened at all: a red block, in the same red and with the
same glyph as a failure notice so that neither is louder than the other, headed that it could not be opened, with the
reason under it (see "Errors" below). Assistive technology is told of it however long after the
viewer opened it arrives.

### A Markdown file as a document

A Markdown file — one whose name ends in `.md` or `.markdown`, in any case — opens as a **rendered document**, and the
first of the header's view controls switches between the document and the source. The source is the code view every
other text file gets. The choice is remembered, like the unified-or-split and wrap choices: it holds for the next
Markdown file, and after the application restarts.

**What the document shows.** Headings, paragraphs, lists — GFM task lists among them, whose boxes show the state the
text records and cannot be ticked — quotes, horizontal rules, tables, inline code, links, images, diagrams, formulas
and fenced code. A fenced block is highlighted the way the source view highlights code, by the grammar its info word
names (a file extension such as `ts` as readily as a grammar's own name), and it wraps rather than scrolling sideways;
a block whose grammar is not one the viewer knows is shown as plain text, and so are the blocks past the document's
budget for highlighting them, in a document that holds a great many of them or very long ones. A fence whose info word
is exactly `mermaid` is a diagram instead of highlighted code, and it does not count against that budget. A table's
columns keep the alignment the author wrote — GFM's left, centre and right, which stay physical as GFM defines them
whichever direction the text reads in — and only an unaligned column follows the direction of its table. GFM footnotes
are set off at the end, under a heading only assistive technology is given; a reference moves to its definition and
the definition's back link moves to the reference, both scrolling within the document rather than taking the viewer
anywhere.

**Which direction the document reads.** Every block — each heading, paragraph, list, quote and table — is laid out in
the direction of **its own text**, whichever direction the window reads in (see "Right-to-left layout" in
`docs/product/window-layout.md`): the direction of the first strong directional character anywhere in that block's
text, nested content included. So a list, a quote or a table whose text begins in Arabic or Hebrew reads from the
right — its markers, its quote bar and its columns with it — inside an otherwise English document, and an English one
reads from the left inside an Arabic document.

- A list, a quote and a table are each **one block with one direction**, taken from the first strong character of the
  whole of their text: a list that begins in English and goes on in Arabic reads from the left throughout, and a cell
  reads with its table.
- A paragraph of its own inside a list item — which is what a list with blank lines between its items is made of — is
  a block in its own right and may read the other way than the list around it does.
- Digits, Western and Arabic-Indic alike, punctuation and symbols are passed over, as is the text of inline code, of a
  fenced block, of a diagram and of a formula, and of anything between Unicode isolate controls (U+2066 to U+2069); an
  image counts as its alt text. Chinese, Japanese and Korean text reads left to right. The scripts read right to left
  are Hebrew, Arabic, Syriac, Thaana, N'Ko, Samaritan, Mandaic, Adlam and Hanifi Rohingya; a letter of any other script
  reads left to right.
- A left-to-right, right-to-left or Arabic letter mark (U+200E, U+200F, U+061C) counts as a strong character, so one
  put before a block's text sets that block's direction whatever letters follow it.
- A block with no strong directional character at all — a list of version numbers, a heading of numerals — is given no
  direction of its own and reads the way the document around it does.
- **Inline code, a fenced block, a diagram and a formula always read left to right.** An inline formula is isolated
  inside its paragraph, so it does not reorder the words around it. Text the file holds as raw HTML, which comes out
  with no block of its own, reads the way the document does.
- **The document as a whole** takes the direction of the first strong character of its whole text, and only where it
  holds none does it follow the window's direction.

**Nothing in the file runs or reaches outside the project.** Raw HTML is shown as text and never run. The text does not
use the network and does not read a path outside the project. Beyond the file itself, what is loaded for the document
is the diagram library and the formula library, each the first time one is drawn, and a project file read the same way
the viewer reads a file opened for itself.

**Addresses.** A relative image or link is resolved against the document's own path: `.` and `..` are applied, and a
query is dropped. A path that would leave the project is not read and is not followed. So is an address that does not
resolve to a path inside the project, which stays its text: a `javascript:` address, any scheme other than `http`,
`https` and `mailto`, a protocol-relative address (`//host`), a path that starts with `/`, and a path that contains a
backslash or that cannot be decoded.

**Links.** An absolute `http:`, `https:` or `mailto:` address is a link only when the window can hand it to the user's
own browser or mail client — to a new tab, in the UI opened in a browser. Following it does not move the viewer. It
carries that address as its tooltip and as its accessible description, and one with no text of its own reads as the
address.

A relative link that resolves to a project file opens that file in the viewer, replacing the file that was open. There
is no way back to the previous document from the link itself. The link carries the path within the project as its
tooltip and as its accessible description, with the heading id after `#` when it names one, and a link with no text of
its own reads as that path. `other.md#heading` opens that file and scrolls to the heading with that id when the
document is shown. A bare `#heading` scrolls inside the current document and does not open another file. A path the
loaded directory listing shows as a folder — a symbolic link to a folder inside the project counts as one (see "What
is listed" above) — is not opened and stays its text. A path whose parent folder has not been listed yet may still be
opened as a file.

From a document shown in Git mode, following such a file link closes the change viewer without moving focus onto a Git
row, switches to Files, and opens the live file.

**Headings.** Headings get ids compatible with GitHub. A repeated heading stays distinct. An id a footnote already
uses is not taken. A heading that already has an id keeps it. A heading whose text yields no id is `section`, and a
later one stays distinct on that basis.

**Images.** An address that is not a project file, an absolute image address included, is not a link and is never
fetched: the reader sees its alt text, with no notice and no read. A relative image that stays inside the project is
shown, in the formats the viewer already shows as images (see "What it shows" above), no wider than the document. An
SVG is shown as an image, so its scripts never run and nothing it refers to is fetched, as for an SVG opened for
itself. Its alt text is its accessible name. Until the read has answered, the alt text is what is shown.

**Diagrams.** A fence whose info word is exactly `mermaid` is a diagram, in both light and dark, using the same ink
and paper as highlighted code (see "What follows the choice" in `docs/product/appearance.md`). Its accessible name is
the title the diagram declares (`accTitle`), or a generic diagram name. Its text alternative is the description the
diagram declares (`accDescr`), or the diagram's source when it declares none. A link drawn inside a diagram is not
followed. The diagram is configured so the text cannot make it emit HTML or run a script: `securityLevel` is strict,
and `htmlLabels` cannot be turned back on.

**Formulas.** `$...$` and `$$...$$` outside code are formulas. `$$` on its own line is a block formula. `$$` inside a
paragraph is display math that stays in the paragraph. A single-dollar run that GitHub leaves as text stays text: the
inside starts or ends with whitespace, or a digit follows the closing dollar. Those two exceptions do not apply to
`$$`. `$20 and $30` stays text. A backslash-escaped dollar stays text. Text in code stays code. A `math` fence and a
dollar wrapped in backticks are not formulas. A formula is read as math, not as an image. The renderer does not trust
the text: a command that would fetch or create a link is not honored, so the formula contains no link and no image.
Those commands stay visible as part of the formula rather than switching the formula to its failure fallback. A link
inside a formula is not followed. Display math that stays in the paragraph counts as an inline formula for direction
(see **Which direction the document reads** above).

Once drawn, a diagram and a block formula are each a named region the keyboard can reach, and one wider than the
document scrolls inside that region, fading the side it hides. An inline formula takes a tab stop only while it is
wider than the line, so that side can be scrolled from the keyboard. While it fits, it is not a tab stop.

**When one cannot be drawn.** Until a diagram or a formula has been drawn, its source is what is shown, with no
notice. One that cannot be drawn shows its source under a failure notice, and the rest of the document stays. A
failure that appears after the document has been drawn is announced once (see "Notices and stand-ins" above). A
diagram or a formula past either of its limits below, and a project image past the count, are shown as source — or, for
the image, as its alt text — under a neutral notice. That notice is part of the first paint and is not announced on
its own.

The budget is per document:

- At most 12 diagrams, each at most 8,000 characters. A diagram over that length does not consume one of the 12. Past
  either limit the source is shown under the neutral notice that the document is large.
- At most 80 formulas, each at most 2,000 characters. A formula over that length does not consume one of the 80. Past
  either limit, the same notice and the source.
- At most 30 project images, each at most 4 MiB, the most a file read delivers (see "Errors" below). An address that
  is not a project file does not consume one of the 30. Over the count: the alt text and a neutral notice that the
  document is large, part of the first paint and not announced on its own. A reply larger than 4 MiB is not shown: the
  alt text and a neutral notice that the image is large, announced once, because it appears after the read. A read
  that fails — which is also what a file larger than 4 MiB does, since the read does not deliver it — or a file that
  is not an image: the alt text and a failure notice, announced once.

**The document always wraps** and offers no wrap control (see "Wrapping long lines" below).

**Where the document is offered.** A Markdown file opened for itself, and a whole Markdown file body the Git mode
shows — an untracked file, one added with no patch, or a change with no lines to draw whose content was read (see
"What it shows" in `docs/product/project-pane-git-mode.md`). A diff and the file behind a path in conflict keep the
source view alone and show no document-or-source control; the conflict's markers are what it is looked at for. So does
a Markdown file past the budget for highlighting (see "What it shows" above): it is shown as plain source, with
no control either. Where the document cannot be drawn at all, the source is shown under a failure notice saying so.

### Wrapping long lines

The **wrap control**, the last of the header's view controls, chooses whether lines too long for the viewer's width
are broken onto the next line; its name alternates between "Wrap lines" and "Don't wrap lines". **Nothing wraps until
it is turned on** in what the viewer shows as source text — prose as much as code. It is one choice for all of it,
files and changes alike: code, a file shown as plain text, a Markdown file's source, a diff, a plain patch, and the
file behind a path in conflict. The choice is remembered, like the unified-or-split choice: it holds for the next file
and the next change, and after the application restarts.

- The control is there only while the viewer shows source text whose lines could wrap. An image change whose sides fell
  back to their text has the one choice between them rather than one for each side. An image shown as an image, a
  binary file, anything the viewer fills with a stand-in rather than content (see "Notices and stand-ins" above), an
  error and a rendered Markdown document — which wraps whatever the choice says (see "A Markdown file as a document"
  above) — offer none.
- **While lines do not wrap, a long one is read by scrolling sideways with the pointer.** Those scrollers take no
  keyboard focus, and Left and Right in the viewer move between subjects (see "Moving between files" below), so from
  the keyboard the way to the end of a long line is to turn wrapping on.
- In the split layout, wrapped lines keep the two sides' rows level with each other.

### Selecting and copying

The **code region** — a file's code, highlighted or plain, or a change's diff or plain patch — takes keyboard focus,
with Tab or a click, so it can be scrolled from the keyboard; reached with Tab it shows a focus ring. Tab again leaves
the region, unless it holds a diff whose collapsed lines can be expanded, whose controls it enters first (see
"Expanding the collapsed lines" in `docs/product/project-pane-git-mode.md`).

With keyboard focus on the code region, **⌘A** (Ctrl+A off macOS) selects that region's code and nothing else: not
its line numbers, not a diff's markers, and not the viewer's title, path and size, so copying takes the code alone.
While the code has not been drawn yet, the key selects nothing. An image change whose sides both fell back to their
text is two code regions, and the key selects the one with keyboard focus.

A **rendered Markdown document** (see "A Markdown file as a document" above) is a region of the same kind, named for
the file as the code region is: Tab or a click puts keyboard focus on it so it can be scrolled from the keyboard, and
Tab from there goes on, in the document's order, through each link, each diagram and each block formula, before
leaving it. An inline formula is a stop only while it is wider than the line (see "A Markdown file as a document" above). ⌘A with keyboard
focus on the document selects the document and not the viewer's title, path and size. Copying a selection that is the
whole document puts the file's **Markdown source** on the clipboard, the source inside its fenced blocks included,
which a selection of the drawn document cannot reach; a selection of part of the document copies as what is selected.

While text inside the viewer is selected, the Left and Right arrow keys leave it selected rather than moving to
another file (see "Moving between files" below).

### Moving between files

**Previous file** and **Next file**, at the foot of the viewer, and the **Left** and **Right** arrow keys move to the
file before and after the one shown, without closing the viewer. They move through the file rows the tree shows, top
to bottom: folders are skipped, and so is everything inside a collapsed folder. They stop at either end rather than
going round, Previous file disabled on the first file and Next file on the last. A file shown as removed moves on to
the files that were beside it.

- Under a right-to-left language the keys are mirrored: Right moves to the previous file and Left to the next.
- The keys do nothing while a modifier is held, during an input method's composition, when keyboard focus is on a
  control that uses the arrow keys itself, or while text inside the viewer is selected, so a selection made for copying
  is not lost. A key held down moves on file after file.
- No key pressed in the viewer reaches the terminal.

### Closing it

**Escape** or the viewer's **Close** button closes it. Keyboard focus then goes to the tree row of the file it showed
last, not necessarily the one it was opened from.

### When the open file changes

The listings that keep the tree up to date (see above) also decide when the open file is read again, so it follows
changes on disk:

- a file that is rewritten in any way — a rewrite of the same size, or another file put in its place, included — is
  read again and shows its new content;
- a file that is gone, or whose folder is gone or can no longer be listed, is read again and then shows why it cannot
  be shown, its row staying in the tree as removed (see "The selected file" above);
- a file shown as gone that comes back shows again;
- a file that kept changing while it was being read is read again at each new listing of its folder, until a read
  succeeds.

A read that finds the file unchanged leaves what is shown as it is. When the connection to the daemon is lost, the
content shown stays until the connection is back, and then the file is read again; a file that had nothing on screen
yet says, in the neutral stand-in rather than as an error (see "Notices and stand-ins" above), that the connection was
lost and that it is read again once the connection is back.

## Errors

A file or folder that cannot be shown says why, in the current language (see "What follows the language" in
`docs/product/language.md`), naming the file or folder it is about by its path within the project. The cases:

| Situation | What the user is told |
|---|---|
| The file or folder does not exist any more, or a symbolic link leads nowhere | it does not exist any more |
| The entry is not a regular file — a FIFO, a socket, a device | it is not a regular file |
| A symbolic link leads outside the project | it leads outside the project, so it is not shown |
| Octoboard is not allowed to read it | it is not allowed to read it, with the system's reason |
| The file is larger than 4 MiB | its size, and the most that can be opened |
| A file kept changing while it was being read, after two more tries | it kept changing, and is read again at the next refresh |
| A folder kept changing while it was being listed, after two more tries | it kept changing, and Refresh tries again |
| The project's directory itself cannot be read | the folder cannot be read, with the reason |

## Loading the project pane

The project pane's own code loads the first time a project pane is shown in the window. Meanwhile the right pane is in
place, empty. A load that fails shows, in the right pane only, that the project pane could not be loaded, with **Try
again**; the rest of the window is unaffected. Try again pressed from the keyboard hands keyboard focus to the terminal
while the next load runs. A project pane that fails while it is shown, with keyboard focus inside it, shows the same
failure and puts keyboard focus on its Try again.
