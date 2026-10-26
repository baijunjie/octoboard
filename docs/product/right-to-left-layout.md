# Right-to-left layout

How the window is laid out under a right-to-left language, and what is never mirrored. Which languages are right-to-left
is in "What follows the language" in `docs/product/language.md`; the window's layout itself is in
`docs/product/window-layout.md`.

## What is mirrored

Under a right-to-left language — Arabic, see "What follows the language" in `docs/product/language.md` — the whole
window is mirrored. Every left and right named in `docs/product/window-layout.md` swaps, and so do the sides the other
product docs name for parts of the window:

- The rail is down the window's right edge, the sidebar on the right inside the content panel, beside the rail, and the
  right pane on the left; the content panel's rounded corner is its top right. The top bar's Back, Forward and sidebar
  toggle are at its right end, in mirrored order, the panel toggles at its left end, and the breadcrumb runs leftward
  from them, starting over the terminal's area while the sidebar is shown and ending where the right pane does, on the
  window's left, while the right pane is docked, following each pane's inner edge while that pane is resized or eases,
  as in "The top bar" in `docs/product/window-layout.md`.
- The sidebar's drawer and the floating cards keep their vertical fade and drift in either reading direction.
- Each resize handle stays on its pane's inner edge, the sidebar's left edge and the right pane's right one. Dragging
  toward the terminal still widens the pane, and so does the arrow key pointing away from the pane.
- Settings and the other dialogs are mirrored the same way, and toasts sit at the bottom left. So is the file viewer's
  layout: an image change's Before and After images, side by side, have Before on the right.
- A name or breadcrumb too long for its space fades out where it ends in its own direction: a name the user typed is
  laid out in the direction of its own text, so a Latin name in an Arabic window still fades at its right edge.
- Paths always read left to right, in the fields that take one and in the settings list. Settings' Default clone
  directory field reads left to right as one control: its folder mark stays beside the path at the field's left edge,
  rather than sitting at the field's leading edge with the path pushed away from it (see "Default clone directory" in
  `docs/product/settings.md`). The Browse button beside the field keeps to the reading direction's end, so it is at the
  field's left.
- **A value named inside a sentence keeps its own order**, wherever that sentence appears — a message, a tooltip, a
  `title`, a toast or an accessible name. A path, a branch name, a commit id, a `git` or operating-system message quoted
  inside localized wording (see "What follows the language" in `docs/product/language.md`) and a name the user typed all
  read in the direction of their own text inside right-to-left prose, and the quotation marks, parentheses and full stop
  the sentence puts around them stay where the sentence puts them.
- The rail's waiting count carries its badge at the hand glyph's top left rather than its top right, so the badge points
  into the content panel and the overflow pill stays clear of the window's edge, the rail being down the window's right
  edge (see "The rail" in `docs/product/window-layout.md`).
- The rail's Account usage indicator fills its capsules from the right (see "The selected session's quota indicator" in
  `docs/product/account-quotas.md`).
- Icons that point a direction or show a side are mirrored: Back and Forward, the sidebar toggle, whose pane is drawn on
  its own side, the breadcrumb's separators, the report panel's previous-page and next-page buttons, the file viewer's
  Previous file and Next file buttons and its wrap control, focus mode's back button, the directory browser's
  parent-directory entry, an action menu's submenu chevron, which points towards the side the submenu opens on — left
  here — and the chevron of a collapsed project row, which points left. An expanded row's chevron points down, as in
  left-to-right.
- Glyphs that hang their rows or lines off the leading side are mirrored too: the Projects heading's Expand all projects
  / Collapse all projects button, whose own chevrons stay vertical, the Git mode's Group by folder toggle in either of
  its two states, and the "View all" glyph of an archive submenu and of a focus mode's archived list.

## What is not mirrored

- **The macOS window buttons** — close, minimise and zoom stay at the window's top left, and the top bar keeps its clear
  space for them there (see "The top bar" in `docs/product/window-layout.md`).
- **The notifications bell's dot**, a mark on the glyph itself rather than a decoration of the layout: it stays at the
  glyph's top right, unlike the waiting count's badge above it on the rail.
- **Notation read in a fixed order**: a keyboard shortcut's key caps, wherever they are shown (see "How a combination is
  written and matched" in `docs/product/keyboard-shortcuts.md`), stay in the order the shortcut is written in; the
  report panel's position reads "current / total", so "1 / 12" is page 1 of 12 (see "Paging through the history" in
  `docs/product/report-panel.md`); and the waiting count's overflow label reads "99+".
- **A literal to type, and program text**: the query parameter and the environment-variable names the startup screen
  gives when it has no daemon address to connect to (`?port=`, `VITE_DAEMON_PORT`, `OCTOBOARD_DAEMON_PORT`) and the
  command it names, and the stack trace in the heading mark's popover on the screen for a failed UI, which also keeps
  its left alignment and so its indentation.
- **The terminal**, which is never mirrored (see "What follows the language" in `docs/product/language.md`).
- **A report page**, which keeps its own direction; only the panel around it is mirrored (see "What follows the
  language" in `docs/product/language.md`).
- **A file's content in the file viewer**: code, a diff — whose split layout keeps the old side on the left — the file's
  name and path, and the paths a change names, read left to right, and a binary change's sizes run from before to after
  left to right; only the viewer around them is mirrored. A file or folder name in the file tree, a name and what
  follows it in the Git mode's change list, a branch's name in the Git mode — in the viewer's subtitle too, where a
  committed change names the branches it is read from (see "Opening a change" in
  `docs/product/project-pane-git-change-viewer.md`) — and the text of the separator a diff shows where it collapsed
  unchanged lines (see "Expanding the collapsed lines" in `docs/product/project-pane-git-change-viewer.md`), are laid
  out in the direction of their own text, as a name the user typed is, and so is a commit's subject on its commit row; a
  commit's id reads left to right.
- **A rendered Markdown document in the file viewer** (see `docs/product/markdown-documents.md`), which is laid out in
  the direction of its own text, block by block: each paragraph, heading, list, quote and table takes the direction of
  its own first strong character, so a right-to-left passage in a left-to-right document reads from its own side, with
  its markers, quote bar and columns on that side, and an English block in an Arabic document reads from the left.
  Inline code, a fenced block, a diagram and a formula always read left to right, and an inline formula is isolated
  inside its paragraph so it does not reorder the words around it. The document as a whole, frame included, follows the
  window only where it holds no strong directional character at all.
