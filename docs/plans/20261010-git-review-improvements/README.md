# Git review improvements

## Problem

Several gaps showed up while confirming the project file browser in the packaged app.

- **Collapsed lines cannot be expanded.** The file viewer shows a change from the Git mode as a patch. Unchanged lines between hunks are collapsed into a
separator such as "6 unmodified lines", and the separator cannot be expanded: the patch the daemon sends holds only
the hunks and their context, so the client has no text for the collapsed lines.
- **Long lines cannot be wrapped.** Code is always shown unwrapped.
- **The change list is always flat**, cannot be filtered by name, and a row offers no action of its own.

## Plan outline

The renderer already supports expanding collapsed lines once it is given both sides' full file contents. The daemon
gains a bounded read of the two full file bodies a diff was made from; the viewer requests them when the user expands
a separator and hands them to the renderer. The viewer gains a word-wrap choice beside the diff layout choice; the
change list gains a flat/tree choice, a file-name filter and a per-row action button whose first action copies the
path.

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

## Milestones

1. [Reading a diff's full file bodies](01-diff-file-bodies.md)
2. [Expanding collapsed lines in the viewer](02-viewer-expansion.md)
3. [Word wrap in the viewer](03-word-wrap.md)
4. [The change list as a directory tree](04-change-tree.md)
5. [Filtering the change list by file name](05-change-filter.md)
6. [A row action button with Copy path](06-row-actions.md)

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
