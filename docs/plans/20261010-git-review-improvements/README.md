# Git review improvements

## Problem

Several gaps showed up while confirming the project file browser in the packaged app.

- **Collapsed lines cannot be expanded.** The file viewer shows a change from the Git mode as a patch. Unchanged lines between hunks are collapsed into a
separator such as "6 unmodified lines", and the separator cannot be expanded: the patch the daemon sends holds only
the hunks and their context, so the client has no text for the collapsed lines.
- **Long lines cannot be wrapped.** Code is always shown unwrapped.
- **The change list is always flat**, cannot be filtered by name, and a row offers no action of its own.
- **Markdown shows only as source.** The viewer has no rendered document view.
- **The viewer header is crowded.** The path, size, comparison and rename origin share one row; the path has no icon
  unlike paths elsewhere in the app; the neutral Staged tag does not read as a tag in dark mode, and the spacing
  between tags and the name is uneven.
- **Light mode loses boundaries.** The viewer's code area is not distinguishable from the dialog, and the Git mode
  panel as a whole has too little contrast; both are hand-set containers rather than HeroUI surfaces.
- **A type change looks unlike every other change.** It shows as two separate Before and After diffs without the
  unified/split choice, while a modified file shows as one diff with it.

## Plan outline

The renderer already supports expanding collapsed lines once it is given both sides' full file contents. The daemon
gains a bounded read of the two full file bodies a diff was made from; the viewer requests them when the user expands
a separator and hands them to the renderer. The viewer gains a word-wrap choice beside the diff layout choice; the
change list gains a flat/tree choice, a file-name filter and a per-row action button whose first action copies the
path. The viewer also gains a source/document toggle for Markdown, a reworked header, HeroUI surfaces for its code
area and the Git mode panel, and shows a type change as one diff.

## Key design decisions

- **Both bodies come from the diff's own sides.** The Uncommitted view's sides are the index and the worktree (or
  `HEAD` and the index, per section); the Compare view's are the two compared commits. An accepted diff must not
  combine a patch from one revision with file bodies from another, so the bodies are tied to the same source identity
  and version information as the patch they expand, and a mismatch is rejected rather than shown.
- **Bounded like every other read.** Reading, transport, parsing and rendering keep limits; UI virtualization does not
  substitute for bounded daemon output. A file beyond the limit offers no expansion, rather than a partial one.
- **On demand.** Bodies are requested only when the user expands, not with the patch and not in reconnect snapshots.
- **Read-only.** Expanding changes nothing in the repository.
- **Expansion grows.** As in common editors (WebStorm, for instance), one expansion reveals a few lines; expanding
  the same gap again a few times in a row reveals the rest of it.
- **Wrapping is a remembered choice.** Like the unified/split choice, word wrap is a toggle whose last choice is kept
  across files and restarts. Code is not wrapped by default.
- **Flat or tree is a remembered choice.** The change list can show changes flat (today's form) or grouped by
  directory; the choice is kept like the other view choices.
- **Filtering is by file name**, narrowing the rows shown without changing what the list holds.
- **A row's actions sit behind one icon button** at the row's end, so more actions can be added later; the first is
  Copy path.
- **The header uses its width, and a long detail gets its own row.** The details sit beside the view controls so the
  width next to them is not wasted; that, not keeping one row, is why they share a row. The path, size, comparison and
  rename origin are kept apart as distinct items, a detail too long for the space left moves to a row of its own, and
  the path shows with an icon prefix like the app's other paths.
- **Standard components over hand-set styles.** Tags stay HeroUI Chips; the neutral tag's fill is measured against the
  dialog, whose `--overlay` is lighter than the page background HeroUI's defaults are tuned for. The
  viewer's code area and the Git mode panel move onto HeroUI surfaces, so both themes are covered by the
  component rather than by hand.
- **Every diff looks the same.** A type change is one diff with the layout choice, like any other change; the
  Before/After split was a presentation choice, not a renderer limit.

## Milestones

1. [Reading a diff's full file bodies](01-diff-file-bodies.md)
2. [Expanding collapsed lines in the viewer](02-viewer-expansion.md)
3. [Word wrap in the viewer](03-word-wrap.md)
4. [The change list as a directory tree](04-change-tree.md)
5. [Filtering the change list by file name](05-change-filter.md)
6. [A row action button with Copy path](06-row-actions.md)
7. [A document view for Markdown in the viewer](07-markdown-document-view.md)
8. [The viewer header's layout and tags](08-viewer-header-layout.md)
9. [Surfaces and light-mode contrast in the viewer and Git mode](09-surfaces-and-contrast.md)
10. [Showing a type change as one diff](10-type-change-diff.md)

## Open

- The exact step: how many lines one expansion reveals, and after how many expansions the whole gap opens; whether
  there is also an "expand all" for the file.
- Whether word wrap applies to diffs as well as files, and whether non-code text (Markdown, plain text) wraps by
  default.
- Whether the file-name filter and the row action button also come to the Files mode's tree, and whether the filter
  matches the name only or the path too.
- Whether a tree compacts single-child directories into one row (`utils/kickback`), and the default of the flat/tree
  choice.
- What the separator says for a file over the limit, and for a side that does not exist (an added or deleted file
  has only one side).
- Whether the Markdown document view is the default for Markdown, whether the choice is remembered, whether a
  Markdown diff also gets a document view, and which Markdown library to use.
- Which fill the neutral tag takes: another Chip colour or variant, or the `--default` family overridden for the
  dialog.
