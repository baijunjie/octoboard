# Markdown documents in the file viewer

A Markdown file — one whose name ends in `.md` or `.markdown`, in any case — opens as a **rendered document**, and the
first of the header's view controls switches between the document and the source. The source is the code view every
other text file gets. The choice is remembered, like the unified-or-split and wrap choices: it holds for the next
Markdown file, and after the application restarts. A file opened at a line opens in its source instead (see "Opened from
the terminal" in `docs/product/file-viewer.md`).

**What the document shows.** Headings, paragraphs, lists — GFM task lists among them, whose boxes show the state the
text records and cannot be ticked — quotes, horizontal rules, tables, inline code, links, images, diagrams, formulas and
fenced code. A fenced block is highlighted the way the source view highlights code, by the grammar its info word names
(a file extension such as `ts` as readily as a grammar's own name), and it wraps rather than scrolling sideways; a block
whose grammar is not one the viewer knows is shown as plain text, and so are the blocks past the document's budget for
highlighting them, in a document that holds a great many of them or very long ones. A fence whose info word is exactly
`mermaid` is a diagram instead of highlighted code, and it does not count against that budget. A table's columns keep
the alignment the author wrote — GFM's left, centre and right, which stay physical as GFM defines them whichever
direction the text reads in — and only an unaligned column follows the direction of its table. GFM footnotes are set off
at the end, under a heading only assistive technology is given; a reference moves to its definition and the definition's
back link moves to the reference, both scrolling within the document rather than taking the viewer anywhere.

**How the document looks.** The document is drawn in one of four styles, chosen in the Markdown style row of Settings'
General section (see "Markdown style" in `docs/product/settings.md`):

- **Tailwind** — **the default**: Tailwind Typography's prose styling.
- **GitHub** — modelled on GitHub's own rendering of Markdown: a sans-serif face, the first two heading levels ruled
  underneath, tables with a border round every cell and every other row tinted, inline code on a tint, and a quote set
  off by a bar on its starting side, in muted text.
- **Newsprint** — modelled on Typora's Newsprint theme: a serif reading face, the page at most 44 em wide in its own
  text size and centred in the viewer, the first two heading levels centred, and a quote in italics.
- **LaTeX** — modelled on latex.css: a Computer Modern-like serif face, paragraphs and list items justified and
  hyphenated, the page at most 40 em wide and centred, and tables ruled above, below and under their header only, in the
  manner of booktabs. Headings are not numbered.

A face is used only when the system has it, falling back to a common serif or sans-serif face; nothing is fetched for
it. Every style keeps the same reading area as the others: the background described in "What follows the choice" in
`docs/product/appearance.md`, text — muted text included — and links that meet 4.5:1 against it in either appearance,
links underlined, task boxes in the list's marker gutter, and the directions described below. Fenced code, diagrams and
formulas keep their own rendering under every style, and are never justified, hyphenated or styled as the document's
text. Changing the choice restyles a document that is already open at once.

The choice is remembered **per client**, in that client's own browser storage, under the same rules as the appearance
choice (see "Where the choice is stored" in `docs/product/appearance.md`): the daemon neither stores it nor carries it,
so it does not travel between clients. Where nothing is stored, or what is stored names no style, the style is Tailwind.

**Which direction the document reads.** Every block — each heading, paragraph, list, quote and table — is laid out in
the direction of **its own text**, whichever direction the window reads in (see `docs/product/right-to-left-layout.md`):
the direction of the first strong directional character anywhere in that block's text, nested content included. So a
list, a quote or a table whose text begins in Arabic or Hebrew reads from the right — its markers, its quote bar and its
columns with it — inside an otherwise English document, and an English one reads from the left inside an Arabic
document.

- A list, a quote and a table are each **one block with one direction**, taken from the first strong character of the
  whole of their text: a list that begins in English and goes on in Arabic reads from the left throughout, and a cell
  reads with its table.
- A paragraph of its own inside a list item — which is what a list with blank lines between its items is made of — is a
  block in its own right and may read the other way than the list around it does.
- Digits, Western and Arabic-Indic alike, punctuation and symbols are passed over, as is the text of inline code, of a
  fenced block, of a diagram and of a formula, and of anything between Unicode isolate controls (U+2066 to U+2069); an
  image counts as its alt text. Chinese, Japanese and Korean text reads left to right. The scripts read right to left
  are Hebrew, Arabic, Syriac, Thaana, N'Ko, Samaritan, Mandaic, Adlam and Hanifi Rohingya; a letter of any other script
  reads left to right.
- A left-to-right, right-to-left or Arabic letter mark (U+200E, U+200F, U+061C) counts as a strong character, so one put
  before a block's text sets that block's direction whatever letters follow it.
- A block with no strong directional character at all — a list of version numbers, a heading of numerals — is given no
  direction of its own and reads the way the document around it does.
- **Inline code, a fenced block, a diagram and a formula always read left to right.** An inline formula is isolated
  inside its paragraph, so it does not reorder the words around it. Text the file holds as raw HTML, which comes out
  with no block of its own, reads the way the document does.
- **The document as a whole** takes the direction of the first strong character of its whole text, and only where it
  holds none does it follow the window's direction.

**Nothing in the file runs or reaches outside the project.** Raw HTML is shown as text and never run. The text does not
use the network and does not read a path outside the project. Beyond the file itself, what is loaded for the document is
the diagram library and the formula library, each the first time one is drawn, and a project file read the same way the
viewer reads a file opened for itself.

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
its own reads as that path. `other.md#heading` opens that file and scrolls to the heading with that id when the document
is shown. A bare `#heading` scrolls inside the current document and does not open another file. A path the loaded
directory listing shows as a folder — a symbolic link to a folder inside the project counts as one (see "What is listed"
in `docs/product/project-pane.md`) — is not opened and stays its text. A path whose parent folder has not been listed
yet may still be opened as a file.

From a document shown in Git mode, following such a file link closes the change viewer without moving focus onto a Git
row, switches to Files, and opens the live file. How links behave in a document opened from the terminal is in "Opened
from the terminal" in `docs/product/file-viewer.md`.

**Headings.** Headings get ids compatible with GitHub. A repeated heading stays distinct. An id a footnote already uses
is not taken. A heading that already has an id keeps it. A heading whose text yields no id is `section`, and a later one
stays distinct on that basis.

**Images.** An address that is not a project file, an absolute image address included, is not a link and is never
fetched: the reader sees its alt text, with no notice and no read. A relative image that stays inside the project is
shown, in the formats the viewer already shows as images (see "What it shows" in `docs/product/file-viewer.md`), no
wider than the document. An SVG is shown as an image, so its scripts never run and nothing it refers to is fetched, as
for an SVG opened for itself. Its alt text is its accessible name. Until the read has answered, the alt text is what is
shown.

**Diagrams.** A fence whose info word is exactly `mermaid` is a diagram, in both light and dark, using the same ink and
paper as highlighted code (see "What follows the choice" in `docs/product/appearance.md`). Its accessible name is the
title the diagram declares (`accTitle`), or a generic diagram name. Its text alternative is the description the diagram
declares (`accDescr`), or the diagram's source when it declares none. A link drawn inside a diagram is not followed. The
diagram is configured so the text cannot make it emit HTML or run a script: `securityLevel` is strict, and `htmlLabels`
cannot be turned back on.

**Formulas.** `$...$` and `$$...$$` outside code are formulas. `$$` on its own line is a block formula. `$$` inside a
paragraph is display math that stays in the paragraph. A single-dollar run that GitHub leaves as text stays text: the
inside starts or ends with whitespace, or a digit follows the closing dollar. Those two exceptions do not apply to `$$`.
`$20 and $30` stays text. A backslash-escaped dollar stays text. Text in code stays code. A `math` fence and a dollar
wrapped in backticks are not formulas. A formula is read as math, not as an image. The renderer does not trust the text:
a command that would fetch or create a link is not honored, so the formula contains no link and no image. Those commands
stay visible as part of the formula rather than switching the formula to its failure fallback. A link inside a formula
is not followed. Display math that stays in the paragraph counts as an inline formula for direction (see **Which
direction the document reads** above).

Once drawn, a diagram and a block formula are each a named region the keyboard can reach, and one wider than the
document scrolls inside that region, fading the side it hides. An inline formula takes a tab stop only while it is wider
than the line, so that side can be scrolled from the keyboard. While it fits, it is not a tab stop.

**When one cannot be drawn.** Until a diagram or a formula has been drawn, its source is what is shown, with no notice.
One that cannot be drawn shows its source with a failure notice at that spot (see "Notices and stand-ins" in
`docs/product/file-viewer.md`), and the rest of the document stays. A diagram or a formula past either of its limits
below, and a project image past the count, are shown as source — or, for the image, as its alt text — with a note at
that spot. That note is part of the first paint and is not announced on its own.

The budget is per document:

- At most 12 diagrams, each at most 8,000 characters. A diagram over that length does not consume one of the 12. Past
  either limit the source is shown with the note that the document is large.
- At most 80 formulas, each at most 2,000 characters. A formula over that length does not consume one of the 80. Past
  either limit, the same notice and the source.
- At most 30 project images, each at most 4 MiB, the most a file read delivers (see "Errors" in
  `docs/product/project-pane.md`). An address that is not a project file does not consume one of the 30. Over the count:
  the alt text and a note that the document is large, part of the first paint and not announced on its own. A reply
  larger than 4 MiB is not shown: the alt text and a note that the image is large, announced once, because it appears
  after the read. A read that fails — which is also what a file larger than 4 MiB does, since the read does not deliver
  it — or a file that is not an image: the alt text and a failure mark beside it, whose popover says the image could not
  be shown, announced once.

**The document always wraps** and offers no wrap control (see "Wrapping long lines" in `docs/product/file-viewer.md`).

**Where the document is offered.** A Markdown file opened for itself, and a whole Markdown file body the Git mode shows
— an untracked file, one added with no patch, or a change with no lines to draw whose content was read (see "What it
shows" in `docs/product/project-pane-git-change-viewer.md`). A diff and the file behind a path in conflict keep the
source view alone and show no document-or-source control; the conflict's markers are what it is looked at for. So does a
Markdown file past the budget for highlighting (see "What it shows" in `docs/product/file-viewer.md`): it is shown as
plain source, with no control either. Where the document cannot be drawn at all, the source is shown with a failure
notice saying so.
