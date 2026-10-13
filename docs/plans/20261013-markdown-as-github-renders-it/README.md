# The Markdown document renders as GitHub renders it

## Problem

The file viewer shows a `.md` file as a rendered document, but only a subset of what GitHub draws. Three things a
README routinely relies on come out wrong:

- A `mermaid` fence is shown as code, not as the diagram it describes.
- A formula written as `$...$` or `$$...$$` is shown as its raw text.
- An image is shown as its alt text alone, and a link to another file in the project or to a heading of the same
  document is drawn as plain text, so a document that links its own parts cannot be followed.

The last two are not an oversight: the document is deliberately inert today, and that is documented behaviour. So
this work changes a stated safety premise as well as adding renderers, and the new premise has to be written down.

## Plan outline

Add three capabilities to the document view, and only there: Mermaid diagrams, math, and project-local images with
relative links and heading anchors. The diagram and math renderers are loaded the first time a document needs them,
the way the code highlighter is. Local images are read through the daemon's existing file reading. Following a
relative link opens the target in the viewer; following an anchor scrolls within the document. One milestone, merged
in one go.

Out of scope, offered and not chosen: GitHub alerts (`> [!NOTE]` and the like) and emoji shortcodes.

## Key design decisions

- **Rendering happens inside the document, with the library lazily loaded.** Chosen over a sandboxed frame and over
  rendering to static SVG in the daemon because it is the simplest and gives the best interaction. The accepted cost
  is that the document is no longer wholly inert, so the diagram renderer is configured not to emit or run HTML or
  scripts from the document's text, and the formula renderer is configured not to trust the text either (see "The new
  premise" below).
- **No hard bundle-size budget.** Lazy loading is enough: the first screen is unaffected, and only a document that
  actually holds a diagram or a formula pays. (The diagram library is roughly 1 MB raw, a KaTeX-class formula
  renderer with its fonts roughly 300 KB.) A formula renderer other than KaTeX is acceptable if it reads the same
  syntax.
- **Capabilities live only where the document view is offered.** A Markdown diff and the file behind a path in
  conflict keep the source view alone; a document past the highlighting budget is shown as plain source. None of
  these gains a diagram, a formula or an image. The source view is unchanged: a `mermaid` fence is code there, and
  `$` is a character.
- **A block that fails to render falls back to its plain source under a failure notice,** the project's habit for
  code, diffs and documents, and never takes the document down. One failing diagram or formula leaves every other
  block drawn.

### Which syntax maps to which capability

- A fenced block whose info word is `mermaid` is a diagram. Every other fence is code, as today.
- `$...$` is an inline formula and `$$...$$` a block formula. A dollar sign that does not form a pair by GitHub's
  rule (a price in prose, a lone `$`) stays text. Neither is recognised inside inline code or a fenced block.
- An image whose address is a relative path to a file in the project is displayed. A link whose address is a
  relative path to a file in the project, or a `#fragment` of the document itself, is followed inside the viewer.

### The new premise

The document used to be wholly inert. Its new premise is: **nothing in the file's text can reach outside the
project, run, or reach the network; the only things loaded are the lazy renderers and files of the project itself.**

- **Loaded:** the diagram and formula renderers, on first need; an image file of the project, read through the
  daemon's file reading and shown inline, subject to the viewer's usual size limits.
- **Where from:** a relative image or link address is resolved against the document's own path. A path that resolves
  outside the project, or to nothing, is not followed and not read.
- **Still refused:** raw HTML in the file, shown as text and never run; an absolute image address (`http:`,
  `https:` or any other scheme), which stays its alt text and is never fetched; a `javascript:` address, drawn as
  its text; an SVG's scripts, and anything an SVG refers to, are never run or fetched (as in the viewer's image
  view); anything a diagram or a formula asks to run or fetch, which is turned off in the renderers rather than
  filtered afterwards.
- **Links in the document:** an absolute `http:`, `https:` or `mailto:` address still goes to the user's own browser
  without moving the viewer. A relative link now moves the viewer (below). Footnote links keep scrolling within the
  document.
- **A link inside a diagram or a formula** is not followed: it neither opens a page nor moves the viewer.

### Following a relative link or an anchor

The viewer is a dialog over one subject. A relative link to a file of the project **opens that file as the viewer's
subject**, exactly as if it had been opened from the file tree: a Markdown file as a document, an image as an image,
other text as code. An anchor (`#heading`) **scrolls the document to that heading** and moves nothing else. Headings
carry no ids today; they get ids that match the ones GitHub gives them, so an author's existing anchors work, and
two headings with the same text stay distinct the way GitHub keeps them distinct. These ids must not collide with the
footnote ids.

### Accessibility and appearance

The requirement, not the implementation; the standard is `docs/ui-accessibility.md`.

- **A diagram works in both appearances.** It takes the window's appearance and follows it when it changes, with
  text and lines that stay legible in each; the colour convention is the project's two-appearance one. Nothing in a
  diagram is said by colour alone beyond what its author drew.
- **A diagram is reachable and readable by assistive technology.** It is exposed as an image with an accessible name
  and a text alternative that conveys what it shows, not only that it exists. One wider or taller than the viewer
  scrolls inside its own region, which a keyboard can reach and which has an accessible name.
- **A formula has an accessible reading,** a spoken form of the mathematics and not the visual glyphs alone. A block
  formula wider than the viewer scrolls the same way a diagram does.
- **A failure notice and a refusal to draw are announced once,** as the other notices of the viewer that can appear
  only after the content is drawn.
- **An image carries its alt text** as its accessible name. A broken image path is shown as its alt text with a
  notice that the file could not be shown, not as an empty gap.

### Right-to-left

Direction is already decided per block from its text. A diagram and a formula **both read left to right**,
whatever direction their block or the window has, as inline code and a fenced block already do, and their text is
passed over when a block's direction is taken. An image counts as its alt text, as today. A formula inside a
paragraph is an isolate in that paragraph's flow, not a break in it.

### Budgets

The document already caps how many fenced blocks are highlighted. Diagrams and formulas need the same kind of cap
so one document cannot stall the window: a cap on how many diagrams, how many formulas, and how large one source may
be. A block past a cap is shown as plain source under a neutral notice saying it is shown without rendering because
the document is large, as the highlighting budget does. Likewise a cap on how many images are read and how large
each may be, with an image past it shown as its alt text. **The numbers are chosen at implementation time;** the
user set no hard limit.

### States to be seen in the gallery

The UI state gallery is the project's way of showing every UI state, so the new states must be visible there: a
rendered diagram (in both appearances), a diagram that failed, a formula (inline and block), an image, and a broken
image path. States the budgets add (a block past a cap) are shown with them.

## Open

Not settled; none of these changes what the milestone is for.

- Whether the viewer can go back to the document after following a relative link to another file, and how.
- What an anchor on a relative link to another file (`other.md#heading`) does once the file has opened.
- What a relative link to a directory does.
- Whether the lone absolute image address stays as plain alt text or ever becomes a link offering the address to the
  browser.
- Whether GitHub's other math notations (a `math` fence, the `$`...`$` form) belong with this work.

## Milestones

One milestone, below.

# 01 Mermaid, math, and project images and links in the document

> Goal: a Markdown file's document view draws what GitHub draws for a `mermaid` fence, `$...$` and `$$...$$`
> formulas, and project-local images, and follows relative links and heading anchors inside the viewer.
> Completion criteria: a `mermaid` fence is drawn as its diagram in both appearances and a formula is drawn inline
> and as a block, each with an accessible reading; a relative image of the project is displayed and one that
> resolves outside the project, to nothing, or to an absolute address is not fetched and stays its alt text; a
> relative link to a project file opens that file in the viewer, an anchor scrolls to its heading, and a `javascript:`
> address and raw HTML remain text; a diagram or formula that fails shows its plain source under a failure notice
> without taking the rest of the document down; a document past the budgets draws the excess as plain source; the
> source view, a Markdown diff and a file in conflict are unchanged; the new states are visible in the UI state
> gallery; the project's static checks and tests pass.

## Technical design

- [ ] Syntax mapping as in "Which syntax maps to which capability": a `mermaid` fence is a diagram, `$...$` and
      `$$...$$` are formulas outside code, nothing else changes meaning.
- [ ] The diagram and formula renderers are each loaded on first need and never for a document without one, like the
      code highlighter; a load that fails is a render failure of the blocks waiting on it.
- [ ] The diagram renderer runs in its strictest mode: no HTML in labels, no click handlers or scripts from the
      text, no link it draws is followed. The formula renderer does not trust the text: no command that fetches or
      links is honoured.
- [ ] An image address is resolved against the document's own path and refused when it leaves the project; the bytes
      are read through the daemon's file reading and shown inline within the viewer's usual size limits, in the
      formats the viewer already displays as images, SVG without scripts or fetching.
- [ ] A relative link is resolved the same way; a target inside the project opens as the viewer's subject, one outside
      or missing is drawn as its text alone.
- [ ] Headings get ids that match GitHub's, with duplicates kept distinct and no clash with footnote ids; an anchor
      link scrolls within the document and moves nothing else.
- [ ] A diagram and a formula carry the accessible name, text alternative and reachable scroll region of
      "Accessibility and appearance"; a diagram follows the window's appearance, including when it changes.
- [ ] A diagram and a formula read left to right and are passed over by the per-block direction.
- [ ] A failed diagram, formula or image falls back as in the key design decisions, announced once.
- [ ] Caps on diagrams, formulas, source size, and images, chosen at implementation time, with the over-cap
      presentation described under "Budgets".

## Implementation plan

- [ ] Extend the document's component map and plugins so a `mermaid` fence is routed to a diagram renderer before the
      code renderer sees it, and formulas are recognised by the Markdown pipeline outside code.
- [ ] Add the lazily loaded diagram and formula renderers behind one loading path each, with their failure fallbacks.
- [ ] Add the image resolver and the link resolver as one shared path-resolution step, so both refuse the same
      paths; the image path reads through the daemon, the link path hands the resolved subject to the viewer.
- [ ] Give headings ids, and make an anchor link scroll within the document.
- [ ] Apply the budgets and their notices alongside the existing highlighting budget.
- [ ] Update the product docs that state the document as inert (see the notes), and the accessibility standard where
      it now has something to say.
- [ ] Every new state listed under "States to be seen in the gallery" is visible in the UI state gallery.

## Notes for the developer

**Reusable capabilities**

- The code highlighter's lazy loading, its failure fallback to plain source under a failure notice, and its
  highlighting budget with the neutral notice are the model for the diagram and formula renderers and their budgets.
- The viewer's notices (neutral, warning, failure), including the rule for which are announced once, and its image
  view with the size limits and the script-free SVG handling.
- The daemon's existing file reading, which the UI already uses for the viewer, serves a relative image; there is no
  URL to fetch.
- The viewer already opens a file as its subject (from the file tree); a followed relative link goes through the same
  way. The document's two local plugins (the budget marker and the per-block direction) are where those concerns
  already live.
- The shared tooltip and message catalogs for any new string; the existing per-block direction treatment of inline
  code and fenced blocks for the left-to-right reading.

**Development notes**

- Every user-visible string goes through the message catalogs, including accessible names and notices.
- The UI state gallery is written only through its own skill, which dispatches the subagent that owns it.
- The inert-document premise is documented as product behaviour; the product docs must change with this work, not
  after it.
- Colours follow the two-appearance convention; do not hard-code one.

**Reference docs**

- `docs/product/project-pane.md`, "A Markdown file as a document" and "The file viewer" (and "Notices and stand-ins"
  within it)
- `docs/ui-accessibility.md`
- `docs/memory/styling-the-ui.md`
- `docs/memory/ui-text-and-reading-direction.md`
- `docs/memory/fading-the-edge-of-a-scroller.md`
- `docs/memory/line-wrapping.md`
