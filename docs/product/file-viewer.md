# File viewer

The **file viewer** is a read-only dialog nearly as large as the window (at most 1440 px wide) that shows one file, or
one change, at a time. It is opened from three places:

- **a file's row in the project pane's Files mode**, by a click or Enter on the row (see "A row's actions" in
  `docs/product/project-pane.md`);
- **a change in the project pane's Git mode**; what the viewer shows of a change, how it moves through the change list,
  closes and follows a change is in `docs/product/project-pane-git-change-viewer.md`;
- **a file or folder path link in a session's terminal or a shell terminal** (see "File paths" in
  `docs/product/session-terminal.md`); how that differs is in "Opened from the terminal" below.

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
- **The details** — the file's path within the project, its size and the path a rename came from — laid out beside
  the **controls**, which hold the end of the row in a fixed order: **Copy path**, then the document-or-source
  choice for a Markdown file (see "A Markdown file as a document" below), then the diff's unified-or-split choice,
  then the wrap choice (see "Wrapping long lines" below), then **File manager** and **Default application**. The size
  shows as a tag of its own, drawn as the title's neutral stage tag is; a rename's origin reads as a sentence naming
  the path. Right after the file's path stand the marks of the **notices about the whole file or change** on screen,
  one mark per notice (see "Notices and stand-ins" below); they belong to the path's own item, and the path's fade
  cuts only the path, never them. They come after the controls in the keyboard's order.

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

**File manager and Default application** are the two actions of a row's Open in submenu, on the same live file and
with the same effect (see "A row's actions" in `docs/product/project-pane.md`): File manager opens the folder holding
the file and selects it, Default application opens the file with the operating system's default application, both on
the machine running the daemon. They are offered only on a live file — a file opened from Files or from the terminal,
and an uncommitted change that is not a deletion, in the worktree it is read from; a committed change and a change
that deletes the file have neither. Each is an icon button whose tooltip is the short name the row's menu uses while the name announced
says what pressing does (see "Tooltips on icon-only controls" in `docs/product/labels-and-tooltips.md`). Neither says
anything when it succeeds; a refusal or a failure raises an error alert with the reason, as from a row.

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
  instead, with its source a choice away (see "A Markdown file as a document" below).
- **Images** — PNG, JPEG, GIF, WebP, BMP, ICO and SVG; AVIF on macOS 13 and later — are shown scaled down to fit and
  never scaled up, on a checkerboard that shows their transparent areas. An image is only displayed: an SVG's scripts
  never run and nothing it refers to is fetched. An image that cannot be decoded is an error in its place (see "Notices and
  stand-ins" below); an SVG that cannot be decoded is shown as its text instead, with a failure notice saying so.
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

- **a note**, a neutral circled `i`, states how the content is drawn and why, for what this file or change is — a
  size budget, a change read at an older version, lines that stay collapsed because the whole file is not to be had, a
  change with no differences whose content is shown whole in place of a diff;
- **a warning**, an amber warning triangle, says something about the content that has to be known before reading it —
  a side of a change that lies outside the project;
- **a failure**, a red circled `!`, reports that something went wrong, so the view on screen is not the one that was
  asked for — a read that failed, a grammar that could not be loaded, code, a diff or a document that could not be
  drawn, an image that would not decode.

A mark is named by the notice's sentence, and that sentence is what it opens. Every mark, whatever its tone, is a
button and a tab stop that opens the sentence in a popover, with no tooltip, under the rules of a failure mark (see
"Failures marked on a row" in `docs/product/error-alerts.md`): a mouse resting on it opens it without moving keyboard
focus, and it stays open while the pointer moves into it, so its text can be selected and copied with the mouse; a
mouse click opens nothing; and a tap, Enter or Space, or assistive technology opens it with keyboard focus inside.

Where a mark stands follows from what its notice is about:

- **The whole file or change on screen**: right after the file's path in the header (see "The header" above), the
  marks ordered failures first, then warnings, then notes. These are syntax highlighting turned off because the file is
  large, or because its grammar could not be loaded or its code could not be drawn; a Markdown document that could not
  be drawn; an image shown as its text; a side of a change outside the project; a change with no differences, or a rename with no change
  to its content, and why its content could not be read in full; a patch shown as plain text because it is large or
  could not be drawn; a missing newline at the end; and what an expansion of a diff's collapsed lines says or why it
  failed (see `docs/product/project-pane-git-change-viewer.md`).
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
and any details behind **Details** (see "Errors" in `docs/product/project-pane.md`). An image that was read but cannot
be decoded, and has no text to fall back to, is the same kind of error, headed that the image cannot be displayed, with
what to check about the file. Neither has Try again. A file still in conflict whose content cannot be read shows the
first of these errors and nothing more — no mark after the path — its conflict being said by the title's Conflicted
tag (see "Opening a change" in `docs/product/project-pane-git-change-viewer.md`). Each opens no error alert, and is
announced to assistive technology, once and as an alert, when it appears after the viewer was drawn (see "Failures
shown on the screen" in `docs/product/error-alerts.md`).

A reason the viewer gives for a read that failed — in an error or in a failure notice — does not repeat a path the
header already shows: when the reason is about the file on screen, it speaks of "this file" instead. A reason about any
other path, such as a parent folder or the path a rename came from, still names that path, and a reason the viewer
does not recognise is shown as it arrived.

## A Markdown file as a document

A Markdown file — one whose name ends in `.md` or `.markdown`, in any case — opens as a **rendered document**, and the
first of the header's view controls switches between the document and the source. The source is the code view every
other text file gets. The choice is remembered, like the unified-or-split and wrap choices: it holds for the next
Markdown file, and after the application restarts. A file opened at a line opens in its source instead (see "Opened
from the terminal" below).

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

**How the document looks.** The document is drawn in one of four styles, chosen in the Markdown style row of Settings'
General section (see "Markdown style" in `docs/product/settings.md`):

- **Tailwind** — **the default**: Tailwind Typography's prose styling.
- **GitHub** — modelled on GitHub's own rendering of Markdown: a sans-serif face, the first two heading levels ruled
  underneath, tables with a border round every cell and every other row tinted, inline code on a tint, and a quote set
  off by a bar on its starting side, in muted text.
- **Newsprint** — modelled on Typora's Newsprint theme: a serif reading face, the page at most 44 em wide in its own
  text size and centred in the viewer, the first two heading levels centred, and a quote in italics.
- **LaTeX** — modelled on latex.css: a Computer Modern-like serif face, paragraphs and list items justified and
  hyphenated, the page at most 40 em wide and centred, and tables ruled above, below and under their header only, in
  the manner of booktabs. Headings are not numbered.

A face is used only when the system has it, falling back to a common serif or sans-serif face; nothing is fetched for
it. Every style keeps the same reading area as the others: the background described in "What follows the choice" in
`docs/product/appearance.md`, text — muted text included — and links that meet 4.5:1 against it in either appearance,
links underlined, task boxes in the list's marker gutter, and the directions described below. Fenced code, diagrams
and formulas keep their own rendering under every style, and are never justified, hyphenated or styled as the
document's text. Changing the choice restyles a document that is already open at once.

The choice is remembered **per client**, in that client's own browser storage, under the same rules as the appearance
choice (see "Where the choice is stored" in `docs/product/appearance.md`): the daemon neither stores it nor carries it,
so it does not travel between clients. Where nothing is stored, or what is stored names no style, the style is
Tailwind.

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
carries a trailing external-link icon and that address as its tooltip; its accessible description says it opens
externally. One with no text of its own reads as the address.

A relative link that resolves to a project file opens that file in the viewer, replacing the file that was open. There
is no way back to the previous document from the link itself. The link carries the path within the project as its
tooltip and as its accessible description, with the heading id after `#` when it names one, and a link with no text of
its own reads as that path. `other.md#heading` opens that file and scrolls to the heading with that id when the
document is shown. A bare `#heading` scrolls inside the current document and does not open another file. A path the
loaded directory listing shows as a folder — a symbolic link to a folder inside the project counts as one (see "What
is listed" in `docs/product/project-pane.md`) — is not opened and stays its text. A path whose parent folder has not
been listed yet may still be opened as a file.

From a document shown in Git mode, following such a file link closes the change viewer without moving focus onto a Git
row, switches to Files, and opens the live file. How links behave in a document opened from the terminal is in "Opened
from the terminal" below.

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
notice. One that cannot be drawn shows its source with a failure notice at that spot (see "Notices and stand-ins"
above), and the rest of the document stays. A diagram or a formula past either of its limits below, and a project image
past the count, are shown as source — or, for the image, as its alt text — with a note at that spot. That note is part
of the first paint and is not announced on its own.

The budget is per document:

- At most 12 diagrams, each at most 8,000 characters. A diagram over that length does not consume one of the 12. Past
  either limit the source is shown with the note that the document is large.
- At most 80 formulas, each at most 2,000 characters. A formula over that length does not consume one of the 80. Past
  either limit, the same notice and the source.
- At most 30 project images, each at most 4 MiB, the most a file read delivers (see "Errors" in
  `docs/product/project-pane.md`). An address that is not a project file does not consume one of the 30. Over the
  count: the alt text and a note that the document is large, part of the first paint and not announced on
  its own. A reply larger than 4 MiB is not shown: the alt text and a note that the image is large, announced
  once, because it appears after the read. A read that fails — which is also what a file larger than 4 MiB does, since
  the read does not deliver it — or a file that is not an image: the alt text and a failure mark beside it, whose
  popover says the image could not be shown, announced once.

**The document always wraps** and offers no wrap control (see "Wrapping long lines" below).

**Where the document is offered.** A Markdown file opened for itself, and a whole Markdown file body the Git mode
shows — an untracked file, one added with no patch, or a change with no lines to draw whose content was read (see
"What it shows" in `docs/product/project-pane-git-change-viewer.md`). A diff and the file behind a path in conflict keep
the source view alone and show no document-or-source control; the conflict's markers are what it is looked at for. So
does a Markdown file past the budget for highlighting (see "What it shows" above): it is shown as plain source, with
no control either. Where the document cannot be drawn at all, the source is shown with a failure notice saying so.

## Wrapping long lines

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

## Selecting and copying

The **code region** — a file's code, highlighted or plain, or a change's diff or plain patch — takes keyboard focus,
with Tab or a click, so it can be scrolled from the keyboard; reached with Tab it shows a focus ring. Tab again leaves
the region, unless it holds a diff whose collapsed lines can be expanded, whose controls it enters first (see
"Expanding the collapsed lines" in `docs/product/project-pane-git-change-viewer.md`).

With keyboard focus on the code region, **⌘A** (Ctrl+A off macOS) selects that region's code and nothing else: not
its line numbers, not a diff's markers, and not the viewer's title, path and size, so copying takes the code alone.
While the code has not been drawn yet, the key selects nothing. An image change whose sides both fell back to their
text is two code regions, and the key selects the one with keyboard focus.

A **rendered Markdown document** (see "A Markdown file as a document" above) is a region of the same kind, named for
the file as the code region is: Tab or a click puts keyboard focus on it so it can be scrolled from the keyboard, and
Tab from there goes on, in the document's order, through each link, each diagram, each block formula and each notice's
mark in it, before leaving it. An inline formula is a stop only while it is wider than the line (see "A Markdown file as a document"
above). ⌘A with keyboard focus on the document selects the document and not the viewer's title, path and size.
Copying a selection that is the whole document puts the file's **Markdown source** on the clipboard, the source inside
its fenced blocks included, which a selection of the drawn document cannot reach; a selection of part of the document
copies as what is selected.

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

**Escape**, the viewer's **Close** button, or a press on the dimmed area around it closes it (see "What an open
dialog, menu or popover covers" in `docs/product/window-layout.md`). Keyboard focus then goes, for a viewer opened
from Files, to the tree row of the file it showed last, not necessarily the one it was opened from; for one opened
from the terminal, back to the terminal — from a shell terminal of the Terminal panel, back to that shell terminal
while its tab is still there and shown, and to the agent terminal otherwise. A change viewer's closing is in `docs/product/project-pane-git-change-viewer.md`.

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
terminal" in `docs/product/terminal-panel.md`), that names a file inside that terminal's project — its directory or
its place in a worktree of the same repository (see "File paths" in `docs/product/session-terminal.md`) — first reads
the file, from the directory it was found in:

- **text, or an image the viewer draws** (see "What it shows" above), opens in the file viewer, showing what was read
  at once rather than saying it is loading;
- **any other binary file, or a file larger than the 4 MiB a read delivers**, opens on the machine the daemon runs on
  with its default application instead, as a path outside the project does;
- **a read that fails for any other reason** raises an error alert with the reason, and nothing opens.

**A file the system could launch** (which files those are is in "File paths" in `docs/product/session-terminal.md`)
is not read first: it always opens in the viewer, which reads and shows it as any file, saying it cannot be displayed
where it is not text or an image, and it is never opened on the host by the click.

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
  user chooses a view for that file; that choice is then remembered as usual (see "A Markdown file as a document"
  above). The same file opened at another line opens in its source again.
- The line is brought into view once for each file and line: redrawing the code — a change of appearance or of the
  wrap choice — does not scroll back to it. A line the file does not have leaves the file at its top. An image ignores
  the line.

**Links in a Markdown document** opened from the terminal open in the same viewer, as in any document (see "A Markdown
file as a document" above), with two differences: a link to a folder is not told from one to a file, so it opens as a
file and the viewer says why it cannot be shown; and in a document read from a worktree, a relative link stays its
text and a relative image shows its alt text, as an address that is not a project file does.

**The first time a viewer is opened from the terminal** its code is loaded, and nothing is shown until it has loaded.
A load that fails raises an error alert saying the file viewer could not be shown, and the next click tries again.
